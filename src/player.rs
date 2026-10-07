//! The local player: camera, mouse look and movement (sprint, crouch, slide,
//! jump and bunny hopping with air strafing).

use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::config::{Action, InputExt, Settings};
use crate::data::{has_perk, Perk, Stat};
use crate::game::Paused;
use crate::maps::{player_spawn, CurrentMap};
use crate::physics::{
    collect_boxes, ground_height, has_overhead_clearance, resolve_collisions_with_step,
    MAX_STEP_HEIGHT,
};
use crate::{
    cursor_locked, AppState, Collider, MatchState, Phase, Roster, Session, CROUCH_EYE_HEIGHT,
    EYE_HEIGHT, PLAYER_RADIUS,
};

const WALK_SPEED: f32 = 6.0;
const SPRINT_SPEED: f32 = 8.5;
const CROUCH_SPEED: f32 = 3.0;
const GROUND_ACCEL: f32 = 60.0;
const AIR_ACCEL: f32 = 80.0;
/// Small air wish speed: lets you gain speed by strafing in the air.
const AIR_WISH: f32 = 1.2;
const FRICTION: f32 = 8.0;
const SLIDE_FRICTION: f32 = 0.7;
const JUMP_SPEED: f32 = 6.8;
const GRAVITY: f32 = 18.0;
const MAX_SPEED: f32 = 20.0;
const SLIDE_TIME: f32 = 0.9;
const SLIDE_MIN_SPEED: f32 = 5.0;
/// Each well-timed hop adds this much speed, up to BHOP_MAX.
const BHOP_GAIN: f32 = 0.6;
/// A jump press this long before landing still counts as a perfect hop.
const HOP_EARLY: f32 = 0.2;
/// After landing from a jump, ground friction waits this long so a hop
/// pressed just after touching down still keeps its speed.
const HOP_LATE: f32 = 0.09;
const BHOP_MAX: f32 = 13.0;
pub const MOUSE_SCALE: f32 = 0.0022;

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera)
            .add_systems(
                Update,
                (respawn, mouse_look, movement, sync_to_roster)
                    .chain()
                    .in_set(Phase::Local),
            )
            .add_systems(Update, apply_fov)
            .add_systems(OnExit(AppState::InGame), reset_camera);
    }
}

#[derive(Component)]
pub struct LocalPlayer {
    pub yaw: f32,
    pub pitch: f32,
    /// Recoil kick added on top of pitch.
    pub kick: f32,
    /// Camera bank/roll angle (slide tilt, strafe bank).
    pub roll: f32,
    pub feet: Vec3,
    pub vel: Vec3,
    pub on_ground: bool,
    pub crouching: bool,
    pub sliding: f32,
    slide_cd: f32,
    eye: f32,
    pub sprinting: bool,
    /// Set by the Dash ability.
    pub dash_time: f32,
    pub dash_dir: Vec3,
    last_spawn_seq: Option<u32>,
    pub air_time: f32,
    pub ground_time: f32,
    pub last_air: f32,
    /// Time left on a jump press waiting for you to land.
    jump_buffer: f32,
    /// Active vault/mantle time remaining (smooth pull onto ledge).
    pub mantle_time: f32,
    /// Cooldown timer before next mantle can trigger.
    pub mantle_cd: f32,
    /// Target Y height of the ledge currently being mantled.
    pub mantle_target_y: f32,
    /// The emote playing: (which, seconds left).
    pub emote: Option<(u8, f32)>,
    /// Bumped each time an emote starts (so repeats restart for others).
    pub emote_seq: u8,
    /// How far the camera has pulled out for an emote (0-1), and the orbit
    /// (yaw offset, pitch) the mouse has moved it to.
    pub cam_out: f32,
    pub orbit: Vec2,
}

impl LocalPlayer {
    pub fn eye_pos(&self) -> Vec3 {
        self.feet + Vec3::Y * self.eye
    }

    pub fn stance(&self) -> u8 {
        if self.sliding > 0.0 {
            2
        } else if self.crouching {
            1
        } else {
            0
        }
    }

    pub fn emoting(&self) -> bool {
        self.emote.is_some()
    }

    /// The camera is out behind the character (emoting, or easing back in).
    pub fn third_person(&self) -> bool {
        self.emote.is_some() || self.cam_out > 0.0
    }

    pub fn horizontal_speed(&self) -> f32 {
        self.vel.with_y(0.0).length()
    }
}

pub fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        LocalPlayer {
            yaw: 0.0,
            pitch: 0.0,
            kick: 0.0,
            roll: 0.0,
            feet: Vec3::ZERO,
            vel: Vec3::ZERO,
            on_ground: true,
            crouching: false,
            sliding: 0.0,
            slide_cd: 0.0,
            eye: EYE_HEIGHT,
            sprinting: false,
            dash_time: 0.0,
            dash_dir: Vec3::ZERO,
            last_spawn_seq: None,
            air_time: 0.0,
            ground_time: 0.0,
            last_air: 0.0,
            jump_buffer: 0.0,
            mantle_time: 0.0,
            mantle_cd: 0.0,
            mantle_target_y: 0.0,
            emote: None,
            emote_seq: 0,
            cam_out: 0.0,
            orbit: Vec2::ZERO,
        },
        Camera3d::default(),
        bevy::audio::SpatialListener::new(0.25),
        Projection::from(PerspectiveProjection {
            fov: 80f32.to_radians(),
            near: 0.02,
            ..default()
        }),
        Transform::from_xyz(0.0, EYE_HEIGHT, 0.0),
    ));
}

fn reset_camera(mut player: Single<(&mut Transform, &mut LocalPlayer)>) {
    let (tf, p) = &mut *player;
    p.last_spawn_seq = None;
    p.vel = Vec3::ZERO;
    p.sliding = 0.0;
    p.dash_time = 0.0;
    p.mantle_time = 0.0;
    p.mantle_cd = 0.0;
    p.mantle_target_y = 0.0;
    p.air_time = 0.0;
    p.last_air = 0.0;
    p.roll = 0.0;
    **tf = Transform::from_xyz(0.0, EYE_HEIGHT, 0.0);
}

fn apply_fov(
    time: Res<Time>,
    settings: Res<Settings>,
    aim: Res<crate::weapons::Aim>,
    player: Single<(&LocalPlayer, &mut Projection)>,
) {
    let (p, mut proj) = player.into_inner();
    if let Projection::Perspective(persp) = &mut *proj {
        let boost = if p.sprinting || p.sliding > 0.0 || p.dash_time > 0.0 {
            8.0
        } else {
            0.0
        };
        let target = (settings.fov + boost).to_radians() * aim.fov_scale();
        // Smooth exponential FOV transition: fast snappy optical zoom when raising sights,
        // smooth cinematic ease when lowering or sprinting, with zero pop or hitching.
        let rate = if aim.amount > 0.0 { 18.0 } else { 12.0 };
        persp.fov += (target - persp.fov) * (1.0 - (-rate * time.delta_secs()).exp());
    }
}

/// Teleports to spawn whenever the host bumps our spawn counter (match
/// start, revive).
fn respawn(
    session: Res<Session>,
    roster: Res<Roster>,
    map: Option<Res<CurrentMap>>,
    player: Single<(&mut LocalPlayer, &mut Transform)>,
    colliders: Query<(&Transform, &Collider), Without<LocalPlayer>>,
) {
    let (mut player, mut tf) = player.into_inner();
    let (Some(me), Some(map)) = (roster.me(&session), map) else {
        return;
    };
    if player.last_spawn_seq == Some(me.spawn_seq) {
        return;
    }
    player.last_spawn_seq = Some(me.spawn_seq);
    player.feet = player_spawn(&map.0, session.my_id);
    // Face the most open way rather than into a wall.
    let boxes = crate::physics::collect_boxes(colliders.iter());
    let eye = player.feet + Vec3::Y * 1.5;
    player.yaw = (0..8)
        .map(|i| i as f32 * std::f32::consts::TAU / 8.0)
        .max_by(|a, b| {
            let room = |yaw: f32| {
                let dir = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
                crate::physics::ray_world(eye, dir, 30.0, &boxes)
            };
            room(*a).total_cmp(&room(*b))
        })
        .unwrap_or(0.0);
    player.vel = Vec3::ZERO;
    player.pitch = 0.0;
    player.roll = 0.0;
    player.sliding = 0.0;
    // Show the spawn even before the first click to play.
    *tf = Transform::from_translation(player.feet + Vec3::Y * player.eye)
        .with_rotation(Quat::from_rotation_y(player.yaw));
}

/// Is the local player allowed to act (alive, playing, not in a menu)?
pub fn can_act(
    session: &Session,
    roster: &Roster,
    state: &MatchState,
    paused: &Paused,
    window: &Window,
) -> bool {
    !state.game_over
        && !state.won
        && !paused.0
        && cursor_locked(window)
        && roster.me(session).is_none_or(|me| me.alive)
}

fn mouse_look(
    motion: Res<AccumulatedMouseMotion>,
    settings: Res<Settings>,
    window: Single<&Window, With<PrimaryWindow>>,
    paused: Res<Paused>,
    aim: Res<crate::weapons::Aim>,
    mut player: Single<&mut LocalPlayer>,
) {
    if !cursor_locked(&window) || paused.0 || player.emoting() {
        return;
    }
    // Slower turning when zoomed in, so aim feels the same.
    let s = MOUSE_SCALE * settings.sensitivity * aim.fov_scale();
    player.yaw -= motion.delta.x * s;
    player.pitch = (player.pitch - motion.delta.y * s).clamp(-1.5, 1.5);
}

/// Quake-style acceleration: only adds speed up to `wish_speed` along `wish`.
fn accelerate(vel: &mut Vec3, wish: Vec3, wish_speed: f32, accel: f32, dt: f32) {
    let current = vel.dot(wish);
    let add = wish_speed - current;
    if add <= 0.0 {
        return;
    }
    let step = (accel * dt * wish_speed.max(4.0)).min(add);
    *vel += wish * step;
}

pub fn movement(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
    window: Single<&Window, With<PrimaryWindow>>,
    session: Res<Session>,
    roster: Res<Roster>,
    state: Res<MatchState>,
    paused: Res<Paused>,
    aim: Res<crate::weapons::Aim>,
    player: Single<(&mut Transform, &mut LocalPlayer)>,
    colliders: Query<(&Transform, &Collider), Without<LocalPlayer>>,
) {
    // Solo pause freezes everything.
    if paused.0 && session.role == crate::Role::Solo {
        return;
    }
    let dt = time.delta_secs().min(0.05);
    let (mut tf, mut p) = player.into_inner();
    let boxes = collect_boxes(colliders.iter());
    let me = roster.me(&session);
    let alive = me.is_none_or(|m| m.alive);
    let active = can_act(&session, &roster, &state, &paused, &window);
    let perks = me.map(|m| m.perks).unwrap_or(0);
    let stamina = if has_perk(perks, Perk::Stamina) {
        1.3
    } else {
        1.0
    };
    // Rally Cry: a third faster on your feet.
    let swift = 1.0
        + me.map_or(0.0, |m| m.stat(Stat::Swift)) * Stat::Swift.per_stack();
    let stim = if me.is_some_and(|m| m.stim > 0.0) {
        1.3
    } else {
        1.0
    };

    let held = |a| active && keys.held(&settings, a);
    let tapped = |a| active && keys.tapped(&settings, a);

    let forward = Vec3::new(-p.yaw.sin(), 0.0, -p.yaw.cos());
    let right = Vec3::new(-forward.z, 0.0, forward.x);
    let mut wish = Vec3::ZERO;
    if held(Action::Forward) {
        wish += forward;
    }
    if held(Action::Back) {
        wish -= forward;
    }
    if held(Action::Right) {
        wish += right;
    }
    if held(Action::Left) {
        wish -= right;
    }
    let wish = wish.normalize_or_zero();

    p.slide_cd -= dt;
    p.mantle_cd -= dt;
    if p.on_ground {
        p.mantle_time = 0.0;
    }
    let crouch_held = held(Action::Crouch);
    let speed = p.horizontal_speed();

    // Start a slide: crouch while moving fast on the ground (also when landing
    // with crouch held, so jump-slide-jump-slide chains work).
    let want_slide = tapped(Action::Crouch)
        || (crouch_held && p.on_ground && p.sliding <= 0.0 && p.slide_cd <= 0.0 && speed > 7.5);
    if want_slide && p.on_ground && speed > SLIDE_MIN_SPEED && p.slide_cd <= 0.0 && p.sliding <= 0.0
    {
        p.sliding = SLIDE_TIME;
        p.slide_cd = 0.5;
        let dir = p.vel.with_y(0.0).normalize_or_zero();
        let boosted = ((speed + 3.0) * stamina)
            .max(11.0 * stamina)
            .min(16.0 * stamina);
        p.vel = dir * boosted + Vec3::Y * p.vel.y;
    }
    if p.sliding > 0.0 {
        p.sliding -= dt;
        if !crouch_held || speed < 3.5 {
            p.sliding = 0.0;
        }
    }
    p.crouching = crouch_held || p.sliding > 0.0;
    let aiming = aim.amount > 0.3;
    p.sprinting = held(Action::Sprint) && !p.crouching && !aiming && wish.dot(forward) > 0.5;

    let max_speed = if p.crouching {
        CROUCH_SPEED
    } else if p.sprinting {
        SPRINT_SPEED * stamina
    } else {
        WALK_SPEED
    } * stim
        * swift
        * (1.0 - 0.4 * aim.amount);

    // Jumping takes a fresh press: holding the key doesn't hop again. A press
    // shortly before landing waits for touchdown, and friction holds off for
    // a moment after landing, so a well-timed tap keeps (and builds) speed.
    let mut jumped = false;
    if tapped(Action::Jump) {
        p.jump_buffer = HOP_EARLY;
    }
    p.jump_buffer -= dt;
    if p.on_ground && p.jump_buffer > 0.0 {
        p.jump_buffer = 0.0;
        // Take off at full running speed in the direction you're holding.
        let mut v = p.vel;
        accelerate(&mut v, wish, max_speed, GROUND_ACCEL, dt);
        // Hopping again right as you land keeps building speed.
        let chained = p.last_air > 0.3 && p.ground_time < HOP_LATE + 0.02;
        let h = v.with_y(0.0);
        let hs = h.length();
        if chained && wish != Vec3::ZERO && hs > 1.0 {
            let cap = BHOP_MAX * stamina;
            let new = (hs + BHOP_GAIN).min(cap.max(hs));
            v = h / hs * new + Vec3::Y * v.y;
        }
        p.vel = v;
        p.vel.y = JUMP_SPEED;
        p.on_ground = false;
        p.sliding = 0.0;
        jumped = true;
    }

    if p.dash_time > 0.0 {
        p.dash_time -= dt;
        let dash = p.dash_dir * 22.0;
        p.vel = Vec3::new(dash.x, p.vel.y.max(0.0), dash.z);
        if p.dash_time <= 0.0 {
            let keep = p.vel.with_y(0.0).normalize_or_zero() * 9.0;
            p.vel = Vec3::new(keep.x, p.vel.y, keep.z);
        }
    } else if p.on_ground && !jumped {
        let landing_grace = p.last_air > 0.3 && p.ground_time < HOP_LATE && p.sliding <= 0.0;
        let friction = if landing_grace {
            0.0
        } else if p.sliding > 0.0 {
            SLIDE_FRICTION
        } else {
            FRICTION
        };
        let h = p.vel.with_y(0.0);
        let hs = h.length();
        if hs > 0.0 {
            let drop = hs.max(1.0) * friction * dt;
            let new = (hs - drop).max(0.0);
            p.vel = h * (new / hs) + Vec3::Y * p.vel.y;
        }
        if p.sliding > 0.0 {
            let mut v = p.vel;
            accelerate(&mut v, wish, 2.0, 10.0, dt);
            p.vel = v;
        } else {
            let mut v = p.vel;
            accelerate(&mut v, wish, max_speed, GROUND_ACCEL, dt);
            p.vel = v;
        }
    } else {
        let mut v = p.vel;
        accelerate(&mut v, wish, AIR_WISH, AIR_ACCEL, dt);
        p.vel = v;
    }

    // Ledge Mantle / Vault: when airborne and moving forward or holding jump towards a reachable ledge
    let jump_held = held(Action::Jump) || tapped(Action::Jump) || p.jump_buffer > 0.0;
    let forward_held = held(Action::Forward) || wish.dot(forward) > 0.3;
    let want_mantle = jump_held || forward_held;

    if !p.on_ground && alive && active && p.dash_time <= 0.0 && want_mantle && p.mantle_cd <= 0.0 {
        let mut best_ledge: Option<(f32, Vec2)> = None;
        let mut min_dist = f32::MAX;
        for (center, half) in &boxes {
            let top = center.y + half.y;
            let diff = top - p.feet.y;
            // Reachable height: 0.35m to 1.2m above feet
            if diff < 0.35 || diff > 1.20 {
                continue;
            }
            let px = p.feet.x;
            let pz = p.feet.z;
            let cx = px.clamp(center.x - half.x, center.x + half.x);
            let cz = pz.clamp(center.z - half.z, center.z + half.z);
            let to_box = Vec2::new(cx - px, cz - pz);
            let dist = to_box.length();
            if dist > PLAYER_RADIUS + 0.45 {
                continue;
            }
            let forward_2d = Vec2::new(forward.x, forward.z).normalize_or_zero();
            if dist > 1e-3 && (to_box / dist).dot(forward_2d) < 0.35 {
                continue;
            }
            let target_xz = Vec2::new(cx, cz) + forward_2d * (PLAYER_RADIUS * 0.5);
            if !has_overhead_clearance(target_xz, PLAYER_RADIUS, top, &boxes) {
                continue;
            }
            if dist < min_dist {
                min_dist = dist;
                best_ledge = Some((top, target_xz));
            }
        }
        if let Some((top, _target_xz)) = best_ledge {
            p.vel.y = p.vel.y.max(4.5);
            p.mantle_time = 0.28;
            p.mantle_cd = 0.45;
            p.mantle_target_y = top;
        }
    }

    if p.mantle_time > 0.0 {
        p.mantle_time -= dt;
        let forward_speed = p.vel.dot(forward);
        let target_forward = 4.5f32.max(forward_speed);
        let pull_accel = (target_forward - forward_speed).max(0.0) * (18.0 * dt).min(1.0);
        p.vel += forward * pull_accel;
    }

    // Cap horizontal speed.
    let h = p.vel.with_y(0.0);
    if h.length() > MAX_SPEED {
        let capped = h.normalize() * MAX_SPEED;
        p.vel = capped + Vec3::Y * p.vel.y;
    }

    p.vel.y -= GRAVITY * dt;
    let before = p.feet + p.vel * dt;
    let mut feet = before;

    // Crouch-jump mechanics: tuck legs upward by ~0.4m when airborne crouched.
    // Also tuck legs during ledge mantling so the player vaults cleanly over the ledge rim.
    let mantle_raise = if p.mantle_time > 0.0 {
        (p.mantle_target_y - feet.y + 0.05).clamp(0.0, 0.45)
    } else {
        0.0
    };
    let effective_feet_y = if !p.on_ground && p.crouching {
        (feet.y + 0.40).max(feet.y + mantle_raise)
    } else {
        feet.y + mantle_raise
    };

    let max_step = if p.on_ground { MAX_STEP_HEIGHT } else { 0.0 };
    let stepped_ground = if p.on_ground {
        resolve_collisions_with_step(&mut feet, PLAYER_RADIUS, effective_feet_y, max_step, &boxes)
    } else {
        resolve_collisions_with_step(&mut feet, PLAYER_RADIUS, effective_feet_y, 0.0, &boxes);
        feet.y = before.y;
        0.0
    };

    // Stop moving into walls we bumped (keeps sliding along them smooth).
    let push = (feet - before).with_y(0.0);
    if push.length_squared() > 1e-8 {
        let n = push.normalize();
        let into = p.vel.dot(n);
        if into < 0.0 {
            let v = p.vel - n * into;
            p.vel = v;
        }
    }
    let ground = ground_height(feet, PLAYER_RADIUS, effective_feet_y, &boxes).max(stepped_ground);
    if feet.y <= ground {
        if p.on_ground && ground > p.feet.y && ground - p.feet.y <= MAX_STEP_HEIGHT + 0.05 {
            // Smoothly elevate p.feet.y onto the step so moving over low curbs,
            // stairs, and small crates feels seamless and fluid like the Source/Quake engine.
            let diff = ground - p.feet.y;
            let step_speed = (diff * 25.0).max(8.0);
            feet.y = (p.feet.y + step_speed * dt).min(ground);
        } else {
            feet.y = ground;
        }
        p.vel.y = 0.0;
        p.on_ground = true;
        p.mantle_time = 0.0;
    } else {
        p.on_ground = feet.y - ground < 0.05 && p.vel.y <= 0.0;
        if p.on_ground {
            p.mantle_time = 0.0;
        }
    }
    p.feet = feet;
    if p.on_ground {
        if p.air_time > 0.0 {
            p.last_air = p.air_time;
            p.air_time = 0.0;
            p.ground_time = 0.0;
        }
        p.ground_time += dt;
    } else {
        p.air_time += dt;
    }

    // Camera.
    let target_eye = if !alive {
        0.4
    } else if p.crouching {
        CROUCH_EYE_HEIGHT
    } else {
        EYE_HEIGHT
    };
    p.eye += (target_eye - p.eye) * (1.0 - (-14.0 * dt).exp());
    // Smooth camera recoil recovery: quick initial snapback smoothly decelerating to rest
    p.kick *= (-14.0 * dt).exp();
    if p.kick.abs() < 1e-4 {
        p.kick = 0.0;
    }
    // Subtle inertial camera roll: bank into slides, lean on ground strafe
    let slide_roll = if p.sliding > 0.0 {
        -0.075
    } else {
        0.0
    };
    let strafe_roll = if p.on_ground {
        let right_dot = p.vel.dot(right);
        (-right_dot * 0.0025).clamp(-0.02, 0.02)
    } else {
        0.0
    };
    let target_roll = slide_roll + strafe_roll;
    p.roll += (target_roll - p.roll) * (1.0 - (-12.0 * dt).exp());
    if p.roll.abs() < 1e-5 {
        p.roll = 0.0;
    }
    tf.translation = p.eye_pos();
    tf.rotation = Quat::from_euler(EulerRot::YXZ, p.yaw, (p.pitch + p.kick).min(1.5), p.roll);
}

/// Copies our position into the roster so the host (and others) see it.
fn sync_to_roster(session: Res<Session>, mut roster: ResMut<Roster>, player: Single<&LocalPlayer>) {
    if let Some(me) = roster.0.get_mut(&session.my_id) {
        me.pos = player.feet.to_array();
        me.yaw = player.yaw;
        me.pitch = player.pitch;
        me.stance = player.stance();
        me.emote = player.emote.map_or(0, |e| e.0);
        me.emote_seq = player.emote_seq;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::{ground_height, has_overhead_clearance, resolve_collisions_with_step, Boxes};

    fn make_test_player() -> LocalPlayer {
        LocalPlayer {
            yaw: 0.0, // facing -Z
            pitch: 0.0,
            kick: 0.0,
            roll: 0.0,
            feet: Vec3::ZERO,
            vel: Vec3::ZERO,
            on_ground: false,
            crouching: false,
            sliding: 0.0,
            slide_cd: 0.0,
            eye: EYE_HEIGHT,
            sprinting: false,
            dash_time: 0.0,
            dash_dir: Vec3::ZERO,
            last_spawn_seq: None,
            air_time: 0.5,
            ground_time: 0.0,
            last_air: 0.5,
            jump_buffer: 0.0,
            mantle_time: 0.0,
            mantle_cd: 0.0,
            mantle_target_y: 0.0,
            emote: None,
            emote_seq: 0,
            cam_out: 0.0,
            orbit: Vec2::ZERO,
        }
    }

    #[test]
    fn test_crouch_jump_effective_feet_height() {
        let mut p = make_test_player();
        p.feet = Vec3::new(0.0, 0.40, 0.0);
        p.on_ground = false;
        p.crouching = false;

        // Standing airborne feet
        let effective_normal = if !p.on_ground && p.crouching {
            p.feet.y + 0.40
        } else {
            p.feet.y
        };
        assert_eq!(effective_normal, 0.40);

        // Airborne crouched: tuck legs up by 0.40m
        p.crouching = true;
        let effective_crouch = if !p.on_ground && p.crouching {
            p.feet.y + 0.40
        } else {
            p.feet.y
        };
        assert_eq!(effective_crouch, 0.80);
    }

    #[test]
    fn test_crouch_jump_clears_waist_high_crate() {
        // Crate at z = -1.5, half-extents (1.0, 0.30, 0.5) -> top = 0.60m, front edge at z = -1.0
        let boxes: Boxes = vec![(Vec3::new(0.0, 0.30, -1.5), Vec3::new(1.0, 0.30, 0.5))];
        let radius = PLAYER_RADIUS;

        // Player airborne at feet.y = 0.35m, moving forward into the crate at z = -0.8
        let mut feet_standing = Vec3::new(0.0, 0.35, -0.8);
        let eff_standing = feet_standing.y; // 0.35m

        // Without crouch-jump, top (0.60) is higher than eff_standing (0.35), so collision pushes player back
        resolve_collisions_with_step(&mut feet_standing, radius, eff_standing, 0.0, &boxes);
        assert!(feet_standing.z > -0.8); // pushed back away from z = -1.0

        // With crouch-jump, tucking legs raises effective feet to 0.35 + 0.40 = 0.75m > 0.60m
        let mut feet_crouched = Vec3::new(0.0, 0.35, -0.8);
        let eff_crouched = feet_crouched.y + 0.40;
        resolve_collisions_with_step(&mut feet_crouched, radius, eff_crouched, 0.0, &boxes);
        // Not pushed back! The tucked legs clear the crate top horizontally
        assert_eq!(feet_crouched.z, -0.8);

        // Moving above the crate detects the crate top for landing
        let over_crate = Vec3::new(0.0, 0.35, -1.3);
        let ground = ground_height(over_crate, radius, eff_crouched, &boxes);
        assert_eq!(ground, 0.60);
    }

    #[test]
    fn test_ledge_mantle_detection_and_impulse() {
        // Crate in front at z = -1.0, half (1.0, 0.45, 0.5) -> top = 0.90m, front face at z = -0.50
        let boxes: Boxes = vec![(Vec3::new(0.0, 0.45, -1.0), Vec3::new(1.0, 0.45, 0.5))];

        let mut p = make_test_player();
        p.feet = Vec3::new(0.0, 0.20, -0.05); // near edge: dist to -0.50 is 0.45m (~PLAYER_RADIUS + 0.05)
        p.vel = Vec3::new(0.0, 1.0, -3.0); // moving forward (-Z)
        let forward = Vec3::new(0.0, 0.0, -1.0);

        // Check ledge reachability: top is 0.90, diff is 0.70m (within 0.35m..1.20m)
        let mut best_ledge: Option<(f32, Vec2)> = None;
        let mut min_dist = f32::MAX;
        for (center, half) in &boxes {
            let top = center.y + half.y;
            let diff = top - p.feet.y;
            if diff < 0.35 || diff > 1.20 {
                continue;
            }
            let px = p.feet.x;
            let pz = p.feet.z;
            let cx = px.clamp(center.x - half.x, center.x + half.x);
            let cz = pz.clamp(center.z - half.z, center.z + half.z);
            let to_box = Vec2::new(cx - px, cz - pz);
            let dist = to_box.length();
            if dist > PLAYER_RADIUS + 0.45 {
                continue;
            }
            let forward_2d = Vec2::new(forward.x, forward.z).normalize_or_zero();
            if dist > 1e-3 && (to_box / dist).dot(forward_2d) < 0.35 {
                continue;
            }
            let target_xz = Vec2::new(cx, cz) + forward_2d * (PLAYER_RADIUS * 0.5);
            if !has_overhead_clearance(target_xz, PLAYER_RADIUS, top, &boxes) {
                continue;
            }
            if dist < min_dist {
                min_dist = dist;
                best_ledge = Some((top, target_xz));
            }
        }

        assert!(best_ledge.is_some());
        let (top, _) = best_ledge.unwrap();
        assert_eq!(top, 0.90);

        // Apply mantle impulse
        p.vel.y = p.vel.y.max(4.5);
        p.mantle_time = 0.28;
        p.mantle_cd = 0.45;
        p.mantle_target_y = top;

        assert_eq!(p.vel.y, 4.5);
        assert_eq!(p.mantle_time, 0.28);
        assert_eq!(p.mantle_target_y, 0.90);
    }

    #[test]
    fn test_ledge_mantle_blocked_by_overhead_obstacle() {
        // Crate at z = -1.0 with top = 0.90m, but ceiling beam directly overhead at y = 1.8 (head clearance < 1.8 + 0.9)
        let boxes: Boxes = vec![
            (Vec3::new(0.0, 0.45, -1.0), Vec3::new(1.0, 0.45, 0.5)),
            (Vec3::new(0.0, 2.0, -1.0), Vec3::new(1.0, 0.4, 0.5)), // bottom is 1.6m
        ];

        let p_feet = Vec3::new(0.0, 0.20, -0.05);
        let forward = Vec3::new(0.0, 0.0, -1.0);
        let mut best_ledge: Option<(f32, Vec2)> = None;

        for (center, half) in &boxes {
            let top = center.y + half.y;
            let diff = top - p_feet.y;
            if diff < 0.35 || diff > 1.20 {
                continue;
            }
            let forward_2d = Vec2::new(forward.x, forward.z).normalize_or_zero();
            let target_xz = Vec2::new(0.0, -0.50) + forward_2d * (PLAYER_RADIUS * 0.5);
            if !has_overhead_clearance(target_xz, PLAYER_RADIUS, top, &boxes) {
                continue;
            }
            best_ledge = Some((top, target_xz));
        }

        // Mantle blocked because ceiling beam is overhead
        assert!(best_ledge.is_none());
    }

    #[test]
    fn test_ledge_mantle_ignores_tall_wall() {
        // Tall wall (height 3.0m, top = 3.0m)
        let boxes: Boxes = vec![(Vec3::new(0.0, 1.5, -1.0), Vec3::new(1.0, 1.5, 0.5))];
        let p_feet = Vec3::new(0.0, 0.0, 0.0);
        let mut best_ledge: Option<(f32, Vec2)> = None;

        for (center, half) in &boxes {
            let top = center.y + half.y;
            let diff = top - p_feet.y;
            if diff < 0.35 || diff > 1.20 {
                continue;
            }
            best_ledge = Some((top, Vec2::ZERO));
        }

        // Diff 3.0m is well beyond reachable 1.20m mantle threshold, so no ledge is found
        assert!(best_ledge.is_none());
    }
}

