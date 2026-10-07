//! Physical ejected brass shell casings for firearms.
//!
//! Spawns spent brass shell casings upon firing with an impulse directed sideways and
//! slightly upward from the weapon, tumbling with random 3D angular spin, arcing under
//! gravity, and bouncing off the ground and elevated obstacles before settling.

use bevy::pbr::NotShadowCaster;
use bevy::prelude::*;
use rand::Rng;
use std::collections::HashSet;

use crate::kit::{Kit, BRASS};
use crate::physics::{collect_boxes, ground_height};
use crate::{Collider, InGameEntity};

pub const GRAVITY: f32 = 18.0;
pub const RESTITUTION: f32 = 0.35;
pub const GROUND_FRICTION: f32 = 0.60;
pub const LOW_SPEED_THRESHOLD: f32 = 0.25;
pub const MAX_BOUNCES: u8 = 3;
pub const DEFAULT_MAX_LIFETIME: f32 = 4.0;
pub const MAX_CASINGS: usize = 64;
pub const CASING_GROUND_RADIUS: f32 = 0.05;

/// Component representing an ejected physical brass shell casing.
#[derive(Component, Debug, Clone)]
pub struct ShellCasing {
    pub velocity: Vec3,
    pub ang_vel: Vec3,
    pub lifetime: f32,
    #[allow(dead_code)]
    pub max_lifetime: f32,
    pub bounce_count: u8,
}

impl ShellCasing {
    pub fn new(velocity: Vec3, ang_vel: Vec3, max_lifetime: f32) -> Self {
        Self {
            velocity,
            ang_vel,
            lifetime: max_lifetime,
            max_lifetime,
            bounce_count: 0,
        }
    }
}

/// Builds a small detailed brass shell casing mesh with an extractor rim.
pub fn casing_mesh() -> Mesh {
    let mut k = Kit::new();
    // Cylindrical casing body
    k.cyl(Vec3::ZERO, 0.012, 0.045, Quat::IDENTITY, BRASS);
    // Extractor rim base
    k.cyl(
        Vec3::new(0.0, -0.022, 0.0),
        0.014,
        0.005,
        Quat::IDENTITY,
        BRASS,
    );
    k.build_or_empty()
}

/// Standard metallic brass material for casings.
pub fn casing_material() -> StandardMaterial {
    StandardMaterial {
        base_color: Color::srgb(0.85, 0.68, 0.22),
        metallic: 0.85,
        perceptual_roughness: 0.35,
        reflectance: 0.7,
        ..default()
    }
}

/// Calculates the normalized ejection direction from weapon aim vector.
///
/// Ejects sideways (to the weapon's right) and slightly upward and rearward.
pub fn calculate_ejection_dir(aim_dir: Vec3) -> Vec3 {
    let right = aim_dir.cross(Vec3::Y).normalize_or(Vec3::X);
    (right * 0.85 + Vec3::Y * 0.45 + aim_dir * -0.15).normalize()
}

/// Spawns a physical brass shell casing near `pos` directed relative to `aim_dir`.
pub fn spawn_shell_casing(
    commands: &mut Commands,
    pos: Vec3,
    aim_dir: Vec3,
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
) -> Entity {
    let mut rng = rand::thread_rng();
    let speed = rng.gen_range(3.5..5.5);
    let eject_dir = calculate_ejection_dir(aim_dir);
    let velocity = eject_dir * speed;

    let ang_vel = Vec3::new(
        rng.gen_range(-25.0..25.0),
        rng.gen_range(-25.0..25.0),
        rng.gen_range(-25.0..25.0),
    );

    let rot = Quat::from_euler(
        EulerRot::XYZ,
        rng.gen_range(0.0..std::f32::consts::TAU),
        rng.gen_range(0.0..std::f32::consts::TAU),
        rng.gen_range(0.0..std::f32::consts::TAU),
    );

    commands
        .spawn((
            InGameEntity,
            ShellCasing::new(velocity, ang_vel, DEFAULT_MAX_LIFETIME),
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::from_translation(pos).with_rotation(rot),
            NotShadowCaster,
        ))
        .id()
}

/// Updates casing physics: gravity, 3D tumbling, ground/crate collisions, bounce damping, settling,
/// lifetime expiration, and maximum concurrent casing capping.
pub fn update_shell_casings(
    mut commands: Commands,
    time: Res<Time>,
    colliders: Query<(&Transform, &Collider), Without<ShellCasing>>,
    mut q: Query<(Entity, &mut ShellCasing, &mut Transform), Without<Collider>>,
) {
    if q.is_empty() {
        return;
    }
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    // Cap maximum concurrent active casings by pruning oldest (lowest remaining lifetime)
    let total = q.iter().count();
    let mut pruned = HashSet::new();
    if total > MAX_CASINGS {
        let mut by_age: Vec<(Entity, f32)> = q.iter().map(|(e, c, _)| (e, c.lifetime)).collect();
        by_age.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        for (e, _) in by_age.into_iter().take(total - MAX_CASINGS) {
            commands.entity(e).despawn();
            pruned.insert(e);
        }
    }

    let boxes = collect_boxes(colliders.iter());

    for (e, mut casing, mut tf) in &mut q {
        if pruned.contains(&e) {
            continue;
        }

        casing.lifetime -= dt;
        if casing.lifetime <= 0.0 {
            commands.entity(e).despawn();
            continue;
        }

        let speed_sq = casing.velocity.length_squared();
        let ang_speed_sq = casing.ang_vel.length_squared();

        // If settled, skip simulation
        if speed_sq < 1e-6 && ang_speed_sq < 1e-6 {
            continue;
        }

        // Applies gravity (GRAVITY = 18.0)
        casing.velocity.y -= GRAVITY * dt;

        // Tumbles transform
        tf.rotate_local_x(casing.ang_vel.x * dt);
        tf.rotate_local_y(casing.ang_vel.y * dt);
        tf.rotate_local_z(casing.ang_vel.z * dt);

        let mut next_pos = tf.translation + casing.velocity * dt;

        // Checks collision with ground or obstacle tops using crate::physics::ground_height
        let floor_y = ground_height(next_pos, CASING_GROUND_RADIUS, tf.translation.y, &boxes);

        if next_pos.y <= floor_y {
            next_pos.y = floor_y;

            if casing.velocity.y < 0.0 {
                casing.bounce_count = casing.bounce_count.saturating_add(1);

                // On ground contact: bounces with restitution (0.35), applies rolling ground friction (0.60), reduces angular velocity
                casing.velocity.y = -casing.velocity.y * RESTITUTION;
                casing.velocity.x *= GROUND_FRICTION;
                casing.velocity.z *= GROUND_FRICTION;
                casing.ang_vel *= 0.5;

                // When speed is low (< 0.25 m/s) or after 3 bounces: settles
                let speed = casing.velocity.length();
                if casing.bounce_count >= MAX_BOUNCES || speed < LOW_SPEED_THRESHOLD {
                    casing.velocity = Vec3::ZERO;
                    casing.ang_vel = Vec3::ZERO;
                }
            }
        }

        tf.translation = next_pos;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    fn setup_test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(16)));
        app.add_systems(Update, update_shell_casings);
        app.update();
        app
    }

    #[test]
    fn test_casing_component_creation() {
        let casing = ShellCasing::new(Vec3::new(1.0, 2.0, 3.0), Vec3::new(4.0, 5.0, 6.0), 4.0);
        assert_eq!(casing.velocity, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(casing.ang_vel, Vec3::new(4.0, 5.0, 6.0));
        assert_eq!(casing.lifetime, 4.0);
        assert_eq!(casing.max_lifetime, 4.0);
        assert_eq!(casing.bounce_count, 0);
    }

    #[test]
    fn test_ejection_direction_calculation() {
        let aim_dir = Vec3::new(0.0, 0.0, 1.0);
        let eject_dir = calculate_ejection_dir(aim_dir);

        // Right vector from aim_dir (0, 0, 1) x Y is (-1, 0, 0)
        // eject_dir should have negative x (sideways right), positive y (upward), and negative z (rearward relative to aim)
        assert!(eject_dir.x < 0.0);
        assert!(eject_dir.y > 0.0);
        assert!(eject_dir.z < 0.0);
        assert!((eject_dir.length() - 1.0).abs() < 1e-4);

        // Vertical aim handles fallback cleanly
        let up_aim = Vec3::Y;
        let up_eject = calculate_ejection_dir(up_aim);
        assert!((up_eject.length() - 1.0).abs() < 1e-4);
    }

    #[test]
    fn test_casing_mesh_and_material_generation() {
        let mesh = casing_mesh();
        assert!(mesh.count_vertices() > 0);
        let mat = casing_material();
        assert_eq!(mat.metallic, 0.85);
    }

    #[test]
    fn test_casing_gravity_and_tumbling() {
        let mut app = setup_test_app();
        let entity = app
            .world_mut()
            .spawn((
                ShellCasing::new(Vec3::new(0.0, 5.0, 0.0), Vec3::new(5.0, 0.0, 0.0), 4.0),
                Transform::from_xyz(0.0, 10.0, 0.0),
            ))
            .id();

        app.update(); // dt = 0.016

        let casing = app.world().get::<ShellCasing>(entity).unwrap();
        let tf = app.world().get::<Transform>(entity).unwrap();

        // Gravity: 18.0 * 0.016 = 0.288 -> expected vel.y ~ 5.0 - 0.288 = 4.712
        assert!((casing.velocity.y - 4.712).abs() < 1e-3);
        assert!(tf.translation.y > 10.0);
        // Tumbling: rotation should have changed from identity
        assert_ne!(tf.rotation, Quat::IDENTITY);
    }

    #[test]
    fn test_casing_floor_bounce_restitution_and_friction() {
        let mut app = setup_test_app();
        let entity = app
            .world_mut()
            .spawn((
                ShellCasing {
                    velocity: Vec3::new(3.0, -10.0, 2.0),
                    ang_vel: Vec3::new(6.0, 6.0, 6.0),
                    lifetime: 4.0,
                    max_lifetime: 4.0,
                    bounce_count: 0,
                },
                Transform::from_xyz(0.0, 0.05, 0.0),
            ))
            .id();

        app.update(); // dt = 0.016

        let casing = app.world().get::<ShellCasing>(entity).unwrap();
        let tf = app.world().get::<Transform>(entity).unwrap();

        assert_eq!(tf.translation.y, 0.0);
        assert_eq!(casing.bounce_count, 1);
        // Restitution (0.35): vel.y after gravity was -(10.0 + 18.0 * 0.016), reflected to ~ 10.288 * 0.35 = 3.6008
        let expected_vy = (10.0 + GRAVITY * 0.016) * RESTITUTION;
        assert!((casing.velocity.y - expected_vy).abs() < 1e-3);
        // Rolling ground friction (0.60): horizontal components damped
        assert!((casing.velocity.x - 3.0 * GROUND_FRICTION).abs() < 1e-3);
        assert!((casing.velocity.z - 2.0 * GROUND_FRICTION).abs() < 1e-3);
        // Angular velocity reduced by 0.5
        assert_eq!(casing.ang_vel, Vec3::new(3.0, 3.0, 3.0));
    }

    #[test]
    fn test_casing_settles_on_low_speed() {
        let mut app = setup_test_app();
        let entity = app
            .world_mut()
            .spawn((
                ShellCasing {
                    velocity: Vec3::new(0.02, 0.0, 0.02),
                    ang_vel: Vec3::new(0.5, 0.5, 0.5),
                    lifetime: 4.0,
                    max_lifetime: 4.0,
                    bounce_count: 0,
                },
                Transform::from_xyz(0.0, 0.001, 0.0),
            ))
            .id();

        app.update();

        let casing = app.world().get::<ShellCasing>(entity).unwrap();
        assert_eq!(casing.velocity, Vec3::ZERO);
        assert_eq!(casing.ang_vel, Vec3::ZERO);
    }

    #[test]
    fn test_casing_settles_after_3_bounces() {
        let mut app = setup_test_app();
        let entity = app
            .world_mut()
            .spawn((
                ShellCasing {
                    velocity: Vec3::new(2.0, -8.0, 0.0),
                    ang_vel: Vec3::new(4.0, 0.0, 0.0),
                    lifetime: 4.0,
                    max_lifetime: 4.0,
                    bounce_count: 2, // Next bounce will be 3rd
                },
                Transform::from_xyz(0.0, 0.05, 0.0),
            ))
            .id();

        app.update();

        let casing = app.world().get::<ShellCasing>(entity).unwrap();
        assert_eq!(casing.bounce_count, 3);
        assert_eq!(casing.velocity, Vec3::ZERO);
        assert_eq!(casing.ang_vel, Vec3::ZERO);
    }

    #[test]
    fn test_casing_bounce_on_elevated_crate() {
        let mut app = setup_test_app();
        // Spawn a crate collider with top at y = 1.0 (center y=0.5, half.y=0.5)
        app.world_mut().spawn((
            Transform::from_xyz(0.0, 0.5, 0.0),
            Collider {
                half: Vec3::new(1.0, 0.5, 1.0),
            },
        ));

        let entity = app
            .world_mut()
            .spawn((
                ShellCasing {
                    velocity: Vec3::new(0.0, -5.0, 0.0),
                    ang_vel: Vec3::new(2.0, 0.0, 0.0),
                    lifetime: 4.0,
                    max_lifetime: 4.0,
                    bounce_count: 0,
                },
                Transform::from_xyz(0.0, 1.05, 0.0),
            ))
            .id();

        app.update();

        let casing = app.world().get::<ShellCasing>(entity).unwrap();
        let tf = app.world().get::<Transform>(entity).unwrap();

        assert_eq!(tf.translation.y, 1.0);
        assert_eq!(casing.bounce_count, 1);
        assert!(casing.velocity.y > 0.0);
    }

    #[test]
    fn test_casing_lifetime_expiration() {
        let mut app = setup_test_app();
        let entity = app
            .world_mut()
            .spawn((
                ShellCasing {
                    velocity: Vec3::ZERO,
                    ang_vel: Vec3::ZERO,
                    lifetime: 0.025,
                    max_lifetime: 4.0,
                    bounce_count: 3,
                },
                Transform::from_xyz(0.0, 0.0, 0.0),
            ))
            .id();

        app.update(); // dt = 0.016, lifetime becomes 0.009
        assert!(app.world().get_entity(entity).is_ok());

        app.update(); // dt = 0.016, lifetime reaches <= 0.0, despawns
        assert!(app.world().get_entity(entity).is_err());
    }

    #[test]
    fn test_max_active_casings_cap() {
        let mut app = setup_test_app();
        // Spawn 70 settled casings with different lifetimes
        let mut entities = Vec::new();
        for i in 0..70 {
            let e = app
                .world_mut()
                .spawn((
                    ShellCasing {
                        velocity: Vec3::ZERO,
                        ang_vel: Vec3::ZERO,
                        lifetime: 1.0 + (i as f32) * 0.05, // lowest lifetime = oldest
                        max_lifetime: 4.0,
                        bounce_count: 3,
                    },
                    Transform::from_xyz(0.0, 0.0, 0.0),
                ))
                .id();
            entities.push(e);
        }

        app.update();

        let remaining = app
            .world_mut()
            .query::<&ShellCasing>()
            .iter(app.world())
            .count();
        assert_eq!(remaining, MAX_CASINGS);

        // The 6 oldest (lowest lifetime: indices 0..6) should have been pruned
        for e in entities.iter().take(6) {
            assert!(app.world().get_entity(*e).is_err());
        }
        // Newer ones should remain
        assert!(app.world().get_entity(entities[69]).is_ok());
    }
}
