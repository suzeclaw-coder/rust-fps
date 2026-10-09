//! Active Ragdoll physics and physical animation system.
//!
//! Inspired by `active-ragdoll-rs` and `Unity.Humanoid.ActiveRagdoll`, this module
//! provides an 11-node articulated physical skeleton for humanoid characters:
//! - Physical Verlet / XPBD constraint solving maintaining rigid bone lengths and anatomical hinge limits.
//! - Active PD (Proportional-Derivative) muscle motors that pull limbs toward target poses
//!   with controllable muscle stiffness ($K_p$) and damping ($K_d$).
//! - Dynamic collision with level obstacles (`&Boxes` from `src/physics.rs`) and ground (`ground_height`),
//!   allowing bodies to drape across crates, tumble down stairs, and slide across floors.
//! - Directional ballistic impulse transfer from gunshots (headshots snap the head and neck back;
//!   chest shots blast the torso; leg shots sweep the legs) and radial blast impulses from explosions.
//! - Alive physical animation hit reactions (`SlaveController`) where upper-body joints jolt
//!   physically upon bullet impacts or poise breaks while AI navigation steering remains intact.

use bevy::prelude::*;

use crate::physics::{collect_boxes, Boxes};
use crate::rig::{Bone, Pose};
use crate::{AppState, Collider, Phase};

pub struct RagdollPlugin;

impl Plugin for RagdollPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (update_active_ragdolls, update_slave_controllers)
                .in_set(Phase::Present)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

// ---------------------------------------------------------------------------
// Constants & Bone Hierarchy
// ---------------------------------------------------------------------------

pub const GRAVITY: f32 = 18.0;
pub const RESTITUTION: f32 = 0.20;
pub const GROUND_FRICTION: f32 = 0.65;
pub const AIR_DRAG: f32 = 0.985;
pub const LIE: f32 = 3.0;
pub const SINK: f32 = 1.0;

/// Standard 11-node humanoid skeleton mapping 1:1 with `active-ragdoll-rs`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(usize)]
pub enum RagdollBone {
    Hips = 0,
    LeftUpperLeg = 1,
    LeftLowerLeg = 2,
    RightUpperLeg = 3,
    RightLowerLeg = 4,
    Spine = 5,
    LeftUpperArm = 6,
    LeftLowerArm = 7,
    Head = 8,
    RightUpperArm = 9,
    RightLowerArm = 10,
}

#[allow(dead_code)]
impl RagdollBone {
    pub const ALL: [RagdollBone; 11] = [
        RagdollBone::Hips,
        RagdollBone::LeftUpperLeg,
        RagdollBone::LeftLowerLeg,
        RagdollBone::RightUpperLeg,
        RagdollBone::RightLowerLeg,
        RagdollBone::Spine,
        RagdollBone::LeftUpperArm,
        RagdollBone::LeftLowerArm,
        RagdollBone::Head,
        RagdollBone::RightUpperArm,
        RagdollBone::RightLowerArm,
    ];

    #[inline]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Map from `rust-fps`'s `Bone` enum to `RagdollBone`.
    pub fn from_rig_bone(bone: Bone) -> Option<Self> {
        match bone {
            Bone::Pelvis => Some(RagdollBone::Hips),
            Bone::Spine => Some(RagdollBone::Spine),
            Bone::Neck => Some(RagdollBone::Head),
            Bone::UpperArm(0) => Some(RagdollBone::LeftUpperArm),
            Bone::Forearm(0) => Some(RagdollBone::LeftLowerArm),
            Bone::UpperArm(1) => Some(RagdollBone::RightUpperArm),
            Bone::Forearm(1) => Some(RagdollBone::RightLowerArm),
            Bone::Thigh(0) => Some(RagdollBone::LeftUpperLeg),
            Bone::Shin(0) => Some(RagdollBone::LeftLowerLeg),
            Bone::Thigh(1) => Some(RagdollBone::RightUpperLeg),
            Bone::Shin(1) => Some(RagdollBone::RightLowerLeg),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Physical Node & Constraints
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub struct RagdollNode {
    pub pos: Vec3,
    pub prev_pos: Vec3,
    pub vel: Vec3,
    pub inv_mass: f32,
    pub radius: f32,
}

impl RagdollNode {
    pub fn new(pos: Vec3, mass: f32, radius: f32) -> Self {
        Self {
            pos,
            prev_pos: pos,
            vel: Vec3::ZERO,
            inv_mass: if mass > 0.0 { 1.0 / mass } else { 0.0 },
            radius,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DistanceConstraint {
    pub a: usize,
    pub b: usize,
    pub rest_len: f32,
    pub stiffness: f32,
}

impl DistanceConstraint {
    pub fn new(a: usize, b: usize, rest_len: f32) -> Self {
        Self {
            a,
            b,
            rest_len,
            stiffness: 1.0,
        }
    }

    #[inline]
    pub fn solve(&self, nodes: &mut [RagdollNode]) {
        let p_a = nodes[self.a].pos;
        let p_b = nodes[self.b].pos;
        let diff = p_b - p_a;
        let dist = diff.length();
        if dist < 1e-5 {
            return;
        }

        let w_a = nodes[self.a].inv_mass;
        let w_b = nodes[self.b].inv_mass;
        let w_sum = w_a + w_b;
        if w_sum <= 0.0 {
            return;
        }

        let delta = diff / dist * ((dist - self.rest_len) * self.stiffness);
        nodes[self.a].pos += delta * (w_a / w_sum);
        nodes[self.b].pos -= delta * (w_b / w_sum);
    }
}

// ---------------------------------------------------------------------------
// PD (Proportional-Derivative) 3D Controller
// ---------------------------------------------------------------------------

#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub struct PdController3d {
    pub kp: f32,
    pub kd: f32,
    pub max_force: f32,
    pub prev_error: Vec3,
    pub has_prev: bool,
}

#[allow(dead_code)]
impl PdController3d {
    pub fn new(kp: f32, kd: f32, max_force: f32) -> Self {
        Self {
            kp,
            kd,
            max_force,
            prev_error: Vec3::ZERO,
            has_prev: false,
        }
    }

    pub fn calculate(&mut self, error: Vec3, dt: f32) -> Vec3 {
        let derivative = if self.has_prev && dt > 1e-5 {
            (error - self.prev_error) / dt
        } else {
            Vec3::ZERO
        };
        self.prev_error = error;
        self.has_prev = true;

        let output = error * self.kp + derivative * self.kd;
        let len = output.length();
        if len > self.max_force && len > 1e-5 {
            output / len * self.max_force
        } else {
            output
        }
    }
}

// ---------------------------------------------------------------------------
// Ballistic Impulses & Damage Types
// ---------------------------------------------------------------------------

#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HitZone {
    Head,
    Torso,
    Pelvis,
    LeftLeg,
    RightLeg,
    FullBodyExplosion,
}

#[allow(dead_code)]
#[derive(Component, Clone, Copy, Debug)]
pub struct DeathImpulse {
    pub hit_zone: HitZone,
    pub point_of_impact: Vec3,
    pub impulse: Vec3,
    pub headshot: bool,
}

#[allow(dead_code)]
#[derive(Component, Clone, Copy, Debug)]
pub struct HitImpulse {
    pub hit_zone: HitZone,
    pub impulse: Vec3,
    pub stun: f32,
}

// ---------------------------------------------------------------------------
// Active Ragdoll State & Component
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RagdollState {
    Active,
    Settled,
    Sinking,
}

/// The active physical ragdoll attached to an entity.
#[allow(dead_code)]
#[derive(Component)]
pub struct ActiveRagdoll {
    pub state: RagdollState,
    pub nodes: [RagdollNode; 11],
    pub constraints: Vec<DistanceConstraint>,
    pub pd_controllers: [PdController3d; 11],
    pub target_local_positions: [Vec3; 11],
    pub scale: f32,
    pub elapsed: f32,
    pub muscle_stiffness: f32,
    pub muscle_damping: f32,
    pub kinetic_energy: f32,
    pub sleep_timer: f32,
}

impl ActiveRagdoll {
    /// Builds an active ragdoll from a character's current world-space transform and gait.
    pub fn new(root_pos: Vec3, root_rot: Quat, scale: f32, crawl: bool) -> Self {
        // Skeleton anatomical dimensions (metres)
        let hip_y = 0.98 * scale;
        let spine_up = 0.08 * scale;
        let neck_up = 0.56 * scale;
        let shoulder_x = 0.20 * scale;
        let shoulder_y = 0.47 * scale;
        let hip_x = 0.11 * scale;
        let upper_arm = 0.31 * scale;
        let forearm = 0.27 * scale;
        let thigh = 0.45 * scale;
        let shin = 0.45 * scale;

        // Local rest offsets
        let mut local = [Vec3::ZERO; 11];
        if crawl {
            // Lying flat on ground
            local[RagdollBone::Hips.index()] = Vec3::new(0.0, 0.20 * scale, 0.40 * scale);
            local[RagdollBone::Spine.index()] = Vec3::new(0.0, 0.25 * scale, 0.0);
            local[RagdollBone::Head.index()] = Vec3::new(0.0, 0.32 * scale, -0.40 * scale);
            local[RagdollBone::LeftUpperArm.index()] = Vec3::new(-shoulder_x, 0.20 * scale, -0.20 * scale);
            local[RagdollBone::LeftLowerArm.index()] = Vec3::new(-shoulder_x, 0.08 * scale, -0.45 * scale);
            local[RagdollBone::RightUpperArm.index()] = Vec3::new(shoulder_x, 0.20 * scale, -0.20 * scale);
            local[RagdollBone::RightLowerArm.index()] = Vec3::new(shoulder_x, 0.08 * scale, -0.45 * scale);
            local[RagdollBone::LeftUpperLeg.index()] = Vec3::new(-hip_x, 0.15 * scale, 0.70 * scale);
            local[RagdollBone::LeftLowerLeg.index()] = Vec3::new(-hip_x, 0.08 * scale, 1.10 * scale);
            local[RagdollBone::RightUpperLeg.index()] = Vec3::new(hip_x, 0.15 * scale, 0.70 * scale);
            local[RagdollBone::RightLowerLeg.index()] = Vec3::new(hip_x, 0.08 * scale, 1.10 * scale);
        } else {
            // Standard standing skeleton
            local[RagdollBone::Hips.index()] = Vec3::new(0.0, hip_y, 0.0);
            local[RagdollBone::Spine.index()] = Vec3::new(0.0, hip_y + spine_up + 0.30 * scale, 0.0);
            local[RagdollBone::Head.index()] = Vec3::new(0.0, hip_y + spine_up + neck_up, 0.0);

            // Left arm
            local[RagdollBone::LeftUpperArm.index()] = Vec3::new(-shoulder_x, hip_y + spine_up + shoulder_y - upper_arm, 0.0);
            local[RagdollBone::LeftLowerArm.index()] = Vec3::new(-shoulder_x, hip_y + spine_up + shoulder_y - upper_arm - forearm, 0.0);

            // Right arm
            local[RagdollBone::RightUpperArm.index()] = Vec3::new(shoulder_x, hip_y + spine_up + shoulder_y - upper_arm, 0.0);
            local[RagdollBone::RightLowerArm.index()] = Vec3::new(shoulder_x, hip_y + spine_up + shoulder_y - upper_arm - forearm, 0.0);

            // Left leg
            local[RagdollBone::LeftUpperLeg.index()] = Vec3::new(-hip_x, hip_y - thigh, 0.0);
            local[RagdollBone::LeftLowerLeg.index()] = Vec3::new(-hip_x, hip_y - thigh - shin + 0.05 * scale, 0.0);

            // Right leg
            local[RagdollBone::RightUpperLeg.index()] = Vec3::new(hip_x, hip_y - thigh, 0.0);
            local[RagdollBone::RightLowerLeg.index()] = Vec3::new(hip_x, hip_y - thigh - shin + 0.05 * scale, 0.0);
        }

        // Masses & radii for nodes
        let masses = [
            18.0 * scale, // Hips
            8.0 * scale,  // LeftUpperLeg
            5.0 * scale,  // LeftLowerLeg
            8.0 * scale,  // RightUpperLeg
            5.0 * scale,  // RightLowerLeg
            22.0 * scale, // Spine
            4.0 * scale,  // LeftUpperArm
            2.5 * scale,  // LeftLowerArm
            5.5 * scale,  // Head
            4.0 * scale,  // RightUpperArm
            2.5 * scale,  // RightLowerArm
        ];

        let radii = [
            0.22 * scale, // Hips
            0.14 * scale, // LeftUpperLeg
            0.12 * scale, // LeftLowerLeg
            0.14 * scale, // RightUpperLeg
            0.12 * scale, // RightLowerLeg
            0.20 * scale, // Spine
            0.12 * scale, // LeftUpperArm
            0.10 * scale, // LeftLowerArm
            0.18 * scale, // Head
            0.12 * scale, // RightUpperArm
            0.10 * scale, // RightLowerArm
        ];

        let mut nodes = [RagdollNode::new(Vec3::ZERO, 1.0, 0.1); 11];
        for i in 0..11 {
            let world_pos = root_pos + root_rot * local[i];
            nodes[i] = RagdollNode::new(world_pos, masses[i], radii[i]);
        }

        // Distance constraints preserving bone lengths
        let mut constraints = Vec::new();
        let add_c = |c: &mut Vec<DistanceConstraint>, a: RagdollBone, b: RagdollBone, l: &[Vec3; 11]| {
            let len = (l[a.index()] - l[b.index()]).length();
            c.push(DistanceConstraint::new(a.index(), b.index(), len));
        };

        // Torso / Head
        add_c(&mut constraints, RagdollBone::Hips, RagdollBone::Spine, &local);
        add_c(&mut constraints, RagdollBone::Spine, RagdollBone::Head, &local);

        // Arms
        add_c(&mut constraints, RagdollBone::Spine, RagdollBone::LeftUpperArm, &local);
        add_c(&mut constraints, RagdollBone::LeftUpperArm, RagdollBone::LeftLowerArm, &local);
        add_c(&mut constraints, RagdollBone::Spine, RagdollBone::RightUpperArm, &local);
        add_c(&mut constraints, RagdollBone::RightUpperArm, RagdollBone::RightLowerArm, &local);

        // Legs
        add_c(&mut constraints, RagdollBone::Hips, RagdollBone::LeftUpperLeg, &local);
        add_c(&mut constraints, RagdollBone::LeftUpperLeg, RagdollBone::LeftLowerLeg, &local);
        add_c(&mut constraints, RagdollBone::Hips, RagdollBone::RightUpperLeg, &local);
        add_c(&mut constraints, RagdollBone::RightUpperLeg, RagdollBone::RightLowerLeg, &local);

        // Cross-body structural constraints to prevent collapsing into singularities
        add_c(&mut constraints, RagdollBone::LeftUpperArm, RagdollBone::RightUpperArm, &local);
        add_c(&mut constraints, RagdollBone::LeftUpperLeg, RagdollBone::RightUpperLeg, &local);
        add_c(&mut constraints, RagdollBone::Hips, RagdollBone::Head, &local);

        // PD Controllers for active muscle dynamics
        let pd_controllers = [PdController3d::new(60.0, 12.0, 150.0); 11];

        Self {
            state: RagdollState::Active,
            nodes,
            constraints,
            pd_controllers,
            target_local_positions: local,
            scale,
            elapsed: 0.0,
            muscle_stiffness: 60.0,
            muscle_damping: 12.0,
            kinetic_energy: 100.0,
            sleep_timer: 0.0,
        }
    }

    /// Applies a directional ballistic impulse to targeted body nodes.
    pub fn apply_impulse(&mut self, impulse: &DeathImpulse) {
        let dir = impulse.impulse;
        match impulse.hit_zone {
            HitZone::Head => {
                // Headshot snaps head back violently with upward lift and spine recoil
                self.nodes[RagdollBone::Head.index()].vel += dir * (1.3 / self.scale);
                self.nodes[RagdollBone::Head.index()].vel.y += 2.5;
                self.nodes[RagdollBone::Spine.index()].vel += dir * 0.45;
            }
            HitZone::Torso => {
                // Torso / Shotgun shoves center of mass backward
                self.nodes[RagdollBone::Spine.index()].vel += dir * 0.85;
                self.nodes[RagdollBone::Hips.index()].vel += dir * 0.70;
            }
            HitZone::Pelvis => {
                self.nodes[RagdollBone::Hips.index()].vel += dir * 0.90;
            }
            HitZone::LeftLeg => {
                self.nodes[RagdollBone::LeftUpperLeg.index()].vel += dir * 1.1;
                self.nodes[RagdollBone::LeftLowerLeg.index()].vel += dir * 1.4;
                self.nodes[RagdollBone::Hips.index()].vel.y -= 1.5;
            }
            HitZone::RightLeg => {
                self.nodes[RagdollBone::RightUpperLeg.index()].vel += dir * 1.1;
                self.nodes[RagdollBone::RightLowerLeg.index()].vel += dir * 1.4;
                self.nodes[RagdollBone::Hips.index()].vel.y -= 1.5;
            }
            HitZone::FullBodyExplosion => {
                for node in &mut self.nodes {
                    node.vel += dir * (1.0 / node.inv_mass.max(0.01).sqrt());
                }
            }
        }
    }

    /// Applies radial blast impulse from an explosion at `origin`.
    #[allow(dead_code)]
    pub fn apply_blast(&mut self, origin: Vec3, radius: f32, force: f32) {
        for node in &mut self.nodes {
            let diff = node.pos - origin;
            let dist = diff.length();
            if dist < radius && dist > 1e-4 {
                let falloff = 1.0 - (dist / radius).clamp(0.0, 1.0);
                let dir = (diff / dist + Vec3::Y * 0.45).normalize();
                node.vel += dir * (force * falloff * (node.inv_mass * 18.0));
            }
        }
    }

    /// Performs one physical simulation sub-step.
    pub fn step(&mut self, dt: f32, root_rot: Quat, colliders: &Boxes) {
        if self.state != RagdollState::Active {
            return;
        }

        self.elapsed += dt;

        // Dynamic muscle relaxation over time:
        // - At t = 0 to 0.5s: active struggle / flailing ($K_p$ high)
        // - At t = 0.5 to 2.0s: agonal twitches and progressive collapse ($K_p$ relaxes)
        // - At t > 2.0s: limp corpse settling
        let muscle_fade = if self.elapsed < 0.5 {
            1.0
        } else if self.elapsed < 2.0 {
            (1.0 - (self.elapsed - 0.5) / 1.5).clamp(0.0, 1.0)
        } else {
            0.0
        };

        let current_kp = self.muscle_stiffness * muscle_fade;
        let current_kd = self.muscle_damping * (0.3 + 0.7 * muscle_fade);

        // Agonal dying nerve spasms (twitches)
        let twitch = if self.elapsed > 0.4 && self.elapsed < 2.2 {
            (self.elapsed * 42.0).sin() * 0.06 * muscle_fade
        } else {
            0.0
        };

        // 1. Semi-implicit Euler integration & Active PD Muscle Motors
        let hip_pos = self.nodes[RagdollBone::Hips.index()].pos;

        for (i, node) in self.nodes.iter_mut().enumerate() {
            // Apply gravity
            node.vel.y -= GRAVITY * dt;
            node.vel *= AIR_DRAG;

            // Active PD muscle force pulling toward target pose
            if current_kp > 0.1 && i != RagdollBone::Hips.index() {
                let target_offset = root_rot * self.target_local_positions[i]
                    + Vec3::new(twitch * 0.5, twitch, 0.0);
                let target_world = hip_pos + target_offset - (root_rot * self.target_local_positions[RagdollBone::Hips.index()]);
                let error = target_world - node.pos;
                let pd_force = error * current_kp - node.vel * current_kd;
                node.vel += pd_force * (dt * node.inv_mass);
            }

            // Position integration
            node.prev_pos = node.pos;
            node.pos += node.vel * dt;
        }

        // 2. Constraint Projection (PBD / Verlet relaxation passes)
        for _ in 0..4 {
            for constraint in &self.constraints {
                constraint.solve(&mut self.nodes);
            }

            // Knee anatomical limit: knees bend backward (+Z relative to hip-knee line),
            // prevent hyperextending forward.
            for &(hip_idx, knee_idx, foot_idx) in &[
                (RagdollBone::Hips.index(), RagdollBone::LeftUpperLeg.index(), RagdollBone::LeftLowerLeg.index()),
                (RagdollBone::Hips.index(), RagdollBone::RightUpperLeg.index(), RagdollBone::RightLowerLeg.index()),
            ] {
                let hip = self.nodes[hip_idx].pos;
                let knee = self.nodes[knee_idx].pos;
                let foot = self.nodes[foot_idx].pos;
                let thigh_dir = (knee - hip).normalize_or_zero();
                let shin_dir = (foot - knee).normalize_or_zero();

                // Prevent knee from bending backwards into shin
                if thigh_dir.dot(shin_dir) < -0.2 {
                    let corrected = (thigh_dir + Vec3::new(0.0, -1.0, 0.2)).normalize();
                    let len = (foot - knee).length();
                    self.nodes[foot_idx].pos = knee + corrected * len;
                }
            }
        }

        // 3. Collision Resolution (Ground and Level Boxes)
        let mut total_ke = 0.0;

        for node in &mut self.nodes {
            // A. Floor & elevated box tops
            let mut floor_y = 0.0_f32;
            for &(center, half) in colliders {
                let top = center.y + half.y;
                let inside_x = (node.pos.x - center.x).abs() <= half.x + node.radius * 0.7;
                let inside_z = (node.pos.z - center.z).abs() <= half.z + node.radius * 0.7;
                if inside_x && inside_z && node.pos.y >= top - 0.40 {
                    floor_y = floor_y.max(top);
                }
            }

            if node.pos.y - node.radius < floor_y {
                node.pos.y = floor_y + node.radius;
                if node.vel.y < 0.0 {
                    node.vel.y = -node.vel.y * RESTITUTION;
                    if node.vel.y.abs() < 0.25 {
                        node.vel.y = 0.0;
                    }
                    node.vel.x *= 1.0 - GROUND_FRICTION;
                    node.vel.z *= 1.0 - GROUND_FRICTION;
                }
            }

            // B. Box obstacle side collisions (walls, barriers)
            for &(center, half) in colliders {
                let top = center.y + half.y;
                if (node.pos.y - (top + node.radius)).abs() < 0.02 {
                    continue;
                }

                let min = center - half;
                let max = center + half;

                let closest = Vec3::new(
                    node.pos.x.clamp(min.x, max.x),
                    node.pos.y.clamp(min.y, max.y),
                    node.pos.z.clamp(min.z, max.z),
                );

                let diff = node.pos - closest;
                let dist_sq = diff.length_squared();

                if dist_sq < node.radius * node.radius && dist_sq > 1e-8 {
                    let dist = dist_sq.sqrt();
                    let normal = diff / dist;
                    let penetration = node.radius - dist;
                    node.pos += normal * penetration;

                    let vn = node.vel.dot(normal);
                    if vn < 0.0 {
                        let normal_vel = normal * vn;
                        let tangent_vel = node.vel - normal_vel;
                        node.vel = -normal_vel * RESTITUTION + tangent_vel * (1.0 - GROUND_FRICTION);
                    }
                }
            }

            total_ke += 0.5 * (1.0 / node.inv_mass) * node.vel.length_squared();
        }

        self.kinetic_energy = total_ke;

        // 4. Settling & Sleep Detection
        if total_ke < 0.15 {
            self.sleep_timer += dt;
            if self.sleep_timer > 0.35 {
                self.state = RagdollState::Settled;
            }
        } else {
            self.sleep_timer = 0.0;
        }
    }

    /// Translates the simulated nodes into a `rust-fps` visual `Pose`.
    pub fn extract_pose(&self, root_rot: Quat) -> Pose {
        let mut p = Pose::rest();
        let inv_root = root_rot.inverse();

        let hips = self.nodes[RagdollBone::Hips.index()].pos;
        let spine = self.nodes[RagdollBone::Spine.index()].pos;
        let head = self.nodes[RagdollBone::Head.index()].pos;

        // Torso orientation
        let up = (spine - hips).normalize_or(Vec3::Y);
        let left_knee = self.nodes[RagdollBone::LeftUpperLeg.index()].pos;
        let right_knee = self.nodes[RagdollBone::RightUpperLeg.index()].pos;
        let hip_lat = (right_knee - left_knee).normalize_or(Vec3::X);
        let right = (hip_lat - up * hip_lat.dot(up)).normalize_or(Vec3::X);
        let fwd = right.cross(up).normalize_or(Vec3::NEG_Z);

        let pelvis_world = Quat::from_mat3(&Mat3::from_cols(right, up, -fwd)).normalize();
        p.pelvis = if pelvis_world.is_nan() {
            Quat::IDENTITY
        } else {
            (inv_root * pelvis_world).normalize()
        };

        // Spine and neck tilt relative to pelvis
        let spine_dir = (head - spine).normalize_or(up);
        let spine_local = pelvis_world.inverse() * spine_dir;
        let spine_pitch = spine_local.z.atan2(spine_local.y).clamp(-1.2, 1.2);
        let spine_roll = (-spine_local.x).clamp(-0.8, 0.8);
        p.spine = Quat::from_rotation_z(spine_roll) * Quat::from_rotation_x(spine_pitch);
        p.neck = Quat::from_rotation_x(spine_pitch * 0.6);

        // Arms reach: in rig::apply, body_to_root rotates (reach.target - HIP_Y) by p.pelvis.
        // Therefore, reach.target and reach.elbow must be in pelvis-local space.
        for side in 0..2 {
            let arm_idx = if side == 0 { RagdollBone::LeftLowerArm.index() } else { RagdollBone::RightLowerArm.index() };
            let elbow_idx = if side == 0 { RagdollBone::LeftUpperArm.index() } else { RagdollBone::RightUpperArm.index() };
            let hand_world = self.nodes[arm_idx].pos;
            let elbow_world = self.nodes[elbow_idx].pos;

            let hand_root = inv_root * (hand_world - hips);
            let elbow_root = inv_root * (elbow_world - hips);

            let target = Vec3::Y * (0.98 * self.scale) + p.pelvis.inverse() * hand_root;
            let elbow = p.pelvis.inverse() * elbow_root;

            p.arms[side] = crate::rig::Reach {
                target,
                elbow,
                hand: None,
            };
        }

        // Legs angles: Bone::Thigh is a child of Bone::Pelvis, rotated by
        // Quat::from_rotation_z(-sx * splay) * Quat::from_rotation_x(thigh).
        // Therefore, thigh_vec and shin_vec must be in pelvis-local coordinates.
        let inv_pelvis_world = pelvis_world.inverse();
        for side in 0..2 {
            let thigh_idx = if side == 0 { RagdollBone::LeftUpperLeg.index() } else { RagdollBone::RightUpperLeg.index() };
            let shin_idx = if side == 0 { RagdollBone::LeftLowerLeg.index() } else { RagdollBone::RightLowerLeg.index() };

            let knee_pos = self.nodes[thigh_idx].pos;
            let foot_pos = self.nodes[shin_idx].pos;

            let thigh_vec = inv_pelvis_world * (knee_pos - hips);
            let shin_vec = inv_pelvis_world * (foot_pos - knee_pos);

            let thigh_len = thigh_vec.length().max(1e-4);
            let shin_len = shin_vec.length().max(1e-4);

            let dot = (thigh_vec / thigh_len).dot(shin_vec / shin_len).clamp(-1.0, 1.0);
            p.knee[side] = dot.acos().clamp(0.0, 2.4);
            p.thigh[side] = (-thigh_vec.z).atan2(-thigh_vec.y);
            p.splay[side] = if side == 0 { -thigh_vec.x * 0.5 } else { thigh_vec.x * 0.5 };
        }

        p
    }
}

// ---------------------------------------------------------------------------
// Alive Physical Animation Controller (SlaveController)
// ---------------------------------------------------------------------------

#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SlaveMode {
    Kinematic,
    PartialRagdoll,
    PoiseBroken,
}

/// Active physical animation controller for living enemies.
/// When hit or poise-broken, the upper body physically recoils while navigation continues.
#[derive(Component)]
pub struct SlaveController {
    pub mode: SlaveMode,
    pub muscle_stiffness: f32,
    pub muscle_damping: f32,
    pub upper_recoil: Vec3,
    pub recoil_vel: Vec3,
    pub flinch_timer: f32,
}

impl Default for SlaveController {
    fn default() -> Self {
        Self {
            mode: SlaveMode::Kinematic,
            muscle_stiffness: 120.0,
            muscle_damping: 18.0,
            upper_recoil: Vec3::ZERO,
            recoil_vel: Vec3::ZERO,
            flinch_timer: 0.0,
        }
    }
}

impl SlaveController {
    pub fn apply_hit(&mut self, impulse: Vec3, stun: f32) {
        self.mode = SlaveMode::PartialRagdoll;
        self.recoil_vel += impulse * 0.12;
        self.flinch_timer = self.flinch_timer.max(stun.max(0.20));
    }

    pub fn update(&mut self, dt: f32) {
        if self.mode == SlaveMode::Kinematic {
            return;
        }

        self.flinch_timer = (self.flinch_timer - dt).max(0.0);

        // Spring-damper physics returning recoil to zero
        let spring = -self.upper_recoil * self.muscle_stiffness;
        let damping = -self.recoil_vel * self.muscle_damping;
        let force = spring + damping;

        self.recoil_vel += force * dt;
        self.upper_recoil += self.recoil_vel * dt;

        if self.flinch_timer <= 0.0 && self.upper_recoil.length_squared() < 1e-4 && self.recoil_vel.length_squared() < 1e-4 {
            self.upper_recoil = Vec3::ZERO;
            self.recoil_vel = Vec3::ZERO;
            self.mode = SlaveMode::Kinematic;
        }
    }
}

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

/// Updates active physical ragdolls every frame.
pub fn update_active_ragdolls(
    mut commands: Commands,
    time: Res<Time>,
    colliders: Query<(&Transform, &Collider), Without<ActiveRagdoll>>,
    mut ragdolls: Query<(Entity, &mut ActiveRagdoll, &mut Transform)>,
) {
    if ragdolls.is_empty() {
        return;
    }

    let dt = time.delta_secs().clamp(1e-4, 0.05);
    let boxes = collect_boxes(colliders.iter());

    for (e, mut ragdoll, mut tf) in &mut ragdolls {
        match ragdoll.state {
            RagdollState::Active => {
                ragdoll.step(dt, tf.rotation, &boxes);

                // Update root translation to follow the physical hips
                let hip_pos = ragdoll.nodes[RagdollBone::Hips.index()].pos;
                tf.translation.x = hip_pos.x;
                tf.translation.z = hip_pos.z;
                tf.translation.y = hip_pos.y - (0.98 * ragdoll.scale);

                if ragdoll.elapsed > LIE {
                    ragdoll.state = RagdollState::Sinking;
                }
            }
            RagdollState::Settled => {
                ragdoll.elapsed += dt;
                if ragdoll.elapsed > LIE {
                    ragdoll.state = RagdollState::Sinking;
                }
            }
            RagdollState::Sinking => {
                ragdoll.elapsed += dt;
                tf.translation.y -= dt * (0.6 / SINK);
                if ragdoll.elapsed > LIE + SINK {
                    commands.entity(e).despawn();
                }
            }
        }
    }
}

/// Updates living active physical animation hit reactions.
pub fn update_slave_controllers(
    mut commands: Commands,
    time: Res<Time>,
    mut controllers: Query<(Entity, &mut SlaveController, Option<&HitImpulse>)>,
) {
    let dt = time.delta_secs().clamp(1e-4, 0.05);
    for (e, mut ctrl, maybe_hit) in &mut controllers {
        if let Some(hit) = maybe_hit {
            ctrl.apply_hit(hit.impulse, hit.stun);
            commands.entity(e).remove::<HitImpulse>();
        }
        ctrl.update(dt);
    }
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ragdoll_initialization_and_bone_distances() {
        let ragdoll = ActiveRagdoll::new(Vec3::new(0.0, 0.0, 0.0), Quat::IDENTITY, 1.0, false);
        assert_eq!(ragdoll.nodes.len(), 11);
        assert!(ragdoll.constraints.len() >= 10);

        // Distance between hips and spine should match rest distance
        let hips_pos = ragdoll.nodes[RagdollBone::Hips.index()].pos;
        let spine_pos = ragdoll.nodes[RagdollBone::Spine.index()].pos;
        let dist = (spine_pos - hips_pos).length();
        assert!((dist - 0.38).abs() < 0.05, "Spine distance was {}", dist);
    }

    #[test]
    fn test_distance_constraint_satisfaction() {
        let mut ragdoll = ActiveRagdoll::new(Vec3::ZERO, Quat::IDENTITY, 1.0, false);
        // Perturb head position far upward
        ragdoll.nodes[RagdollBone::Head.index()].pos.y += 2.0;

        let boxes = Vec::new();
        for _ in 0..10 {
            ragdoll.step(0.016, Quat::IDENTITY, &boxes);
        }

        // Spine to head distance should be restored
        let spine_pos = ragdoll.nodes[RagdollBone::Spine.index()].pos;
        let head_pos = ragdoll.nodes[RagdollBone::Head.index()].pos;
        let dist = (head_pos - spine_pos).length();
        let expected = (ragdoll.target_local_positions[RagdollBone::Head.index()]
            - ragdoll.target_local_positions[RagdollBone::Spine.index()])
        .length();
        assert!((dist - expected).abs() < 0.15, "Head constraint not satisfied: dist={}, expected={}", dist, expected);
    }

    #[test]
    fn test_floor_collision_and_resting() {
        let mut ragdoll = ActiveRagdoll::new(Vec3::new(0.0, 2.0, 0.0), Quat::IDENTITY, 1.0, false);
        let boxes = Vec::new(); // floor is at y = 0

        for _ in 0..120 {
            ragdoll.step(0.016, Quat::IDENTITY, &boxes);
        }

        // All nodes should be on or above the floor (y >= radius)
        for (i, node) in ragdoll.nodes.iter().enumerate() {
            assert!(
                node.pos.y >= node.radius - 0.05,
                "Node {} at y={} penetrated floor (radius={})",
                i,
                node.pos.y,
                node.radius
            );
        }
    }

    #[test]
    fn test_headshot_impulse_whips_head_back() {
        let mut ragdoll = ActiveRagdoll::new(Vec3::ZERO, Quat::IDENTITY, 1.0, false);
        let bullet_dir = Vec3::new(0.0, 0.0, 1.0); // shot from front to back (+Z)

        ragdoll.apply_impulse(&DeathImpulse {
            hit_zone: HitZone::Head,
            point_of_impact: ragdoll.nodes[RagdollBone::Head.index()].pos,
            impulse: bullet_dir * 12.0,
            headshot: true,
        });

        assert!(
            ragdoll.nodes[RagdollBone::Head.index()].vel.z > 10.0,
            "Head node did not receive backward velocity"
        );
        assert!(
            ragdoll.nodes[RagdollBone::Head.index()].vel.y > 2.0,
            "Head node did not receive upward whip lift"
        );
    }

    #[test]
    fn test_explosion_blast_launches_ragdoll() {
        let mut ragdoll = ActiveRagdoll::new(Vec3::new(0.0, 0.0, 2.0), Quat::IDENTITY, 1.0, false);
        let blast_origin = Vec3::new(0.0, 0.0, 0.0);

        ragdoll.apply_blast(blast_origin, 5.0, 15.0);

        // Hips should be blasted away from origin along +Z
        assert!(
            ragdoll.nodes[RagdollBone::Hips.index()].vel.z > 0.0,
            "Hips did not receive blast velocity along +Z"
        );
        assert!(
            ragdoll.nodes[RagdollBone::Hips.index()].vel.y > 0.0,
            "Hips did not receive upward blast lift"
        );
    }

    #[test]
    fn test_slave_controller_spring_recovery() {
        let mut ctrl = SlaveController::default();
        ctrl.apply_hit(Vec3::new(0.0, 0.0, -10.0), 0.3);

        assert_eq!(ctrl.mode, SlaveMode::PartialRagdoll);
        assert!(ctrl.recoil_vel.length() > 0.0);

        // Advance simulation for 0.8s
        for _ in 0..50 {
            ctrl.update(0.016);
        }

        // Should recover back to Kinematic
        assert_eq!(ctrl.mode, SlaveMode::Kinematic);
        assert!(ctrl.upper_recoil.length() < 1e-3);
    }

    #[test]
    fn test_ragdoll_obstacle_collision_and_resting_on_crate() {
        // Crate at x = 0, z = 0, y = 0.5 (half 0.5, top at 1.0)
        let crate_center = Vec3::new(0.0, 0.5, 0.0);
        let crate_half = Vec3::splat(0.5);
        let boxes: Boxes = vec![(crate_center, crate_half)];

        // Spawn ragdoll above the crate
        let mut ragdoll = ActiveRagdoll::new(Vec3::new(0.0, 2.0, 0.0), Quat::IDENTITY, 1.0, false);

        for _ in 0..120 {
            ragdoll.step(0.016, Quat::IDENTITY, &boxes);
        }

        // Hips node should rest on top of the crate (top y = 1.0, so y >= 1.0)
        let hip_y = ragdoll.nodes[RagdollBone::Hips.index()].pos.y;
        assert!(
            hip_y >= 0.95,
            "Ragdoll hips penetrated into the crate: hip_y={}",
            hip_y
        );
    }

    #[test]
    fn test_ragdoll_extract_pose_validity() {
        let mut ragdoll = ActiveRagdoll::new(Vec3::new(0.0, 1.0, 0.0), Quat::IDENTITY, 1.0, false);
        let boxes = Vec::new();
        ragdoll.step(0.016, Quat::IDENTITY, &boxes);

        let pose = ragdoll.extract_pose(Quat::IDENTITY);

        // Pelvis rotation should be a valid unit quaternion without NaN
        assert!(!pose.pelvis.is_nan());
        assert!((pose.pelvis.length() - 1.0).abs() < 1e-3);

        // Spine and neck rotations should not be NaN
        assert!(!pose.spine.is_nan());
        assert!(!pose.neck.is_nan());

        // Knees should be within anatomical bounds [0, 2.5]
        assert!(pose.knee[0] >= 0.0 && pose.knee[0] <= 2.5);
        assert!(pose.knee[1] >= 0.0 && pose.knee[1] <= 2.5);
    }

    #[test]
    fn test_shotgun_torso_impulse_blasts_pelvis_and_spine() {
        let mut ragdoll = ActiveRagdoll::new(Vec3::ZERO, Quat::IDENTITY, 1.0, false);
        let blast_dir = Vec3::new(0.0, 0.0, -1.0);

        ragdoll.apply_impulse(&DeathImpulse {
            hit_zone: HitZone::Torso,
            point_of_impact: ragdoll.nodes[RagdollBone::Spine.index()].pos,
            impulse: blast_dir * 16.0,
            headshot: false,
        });

        // Spine and Hips should both be blasted backward
        assert!(ragdoll.nodes[RagdollBone::Spine.index()].vel.z < -8.0);
        assert!(ragdoll.nodes[RagdollBone::Hips.index()].vel.z < -8.0);
    }

    #[test]
    fn test_active_ragdoll_settles_after_rest() {
        let mut ragdoll = ActiveRagdoll::new(Vec3::new(0.0, 0.5, 0.0), Quat::IDENTITY, 1.0, false);
        let boxes = Vec::new();

        // Step until settled
        for _ in 0..250 {
            ragdoll.step(0.016, Quat::IDENTITY, &boxes);
        }

        assert_eq!(ragdoll.state, RagdollState::Settled);
    }

    #[test]
    fn test_rotated_ragdoll_extract_pose_validity() {
        // When ragdoll falls backwards (e.g. 90 degree pitch)
        let rot = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
        let mut ragdoll = ActiveRagdoll::new(Vec3::new(0.0, 0.5, 0.0), rot, 1.0, false);
        let boxes = Vec::new();
        ragdoll.step(0.016, rot, &boxes);

        let pose = ragdoll.extract_pose(rot);

        // Pelvis, spine, neck rotations should remain valid (non-NaN)
        assert!(!pose.pelvis.is_nan());
        assert!(!pose.spine.is_nan());
        assert!(!pose.neck.is_nan());

        // Reach target and elbow should be finite and non-NaN
        assert!(!pose.arms[0].target.is_nan());
        assert!(!pose.arms[1].target.is_nan());
        assert!(!pose.arms[0].elbow.is_nan());
        assert!(!pose.arms[1].elbow.is_nan());

        // Knee bends should remain within realistic ranges
        assert!(pose.knee[0] >= 0.0 && pose.knee[0] <= 2.5);
        assert!(pose.knee[1] >= 0.0 && pose.knee[1] <= 2.5);
    }
}
