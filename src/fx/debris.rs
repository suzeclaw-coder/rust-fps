//! Dynamic 3D physical debris and shrapnel simulation for explosions.
//!
//! Spawns tumbling 3D physical cube/shard debris entities at blast origins that scatter,
//! arc under gravity, bounce off the floor with restitution and ground friction, and settle.

use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use rand::Rng;

use crate::physics::{collect_boxes, ground_height};
use crate::{Collider, InGameEntity};

pub const GRAVITY: f32 = 18.0;
pub const RESTITUTION: f32 = 0.4;
pub const GROUND_FRICTION: f32 = 0.7;
pub const MAX_LIFETIME: f32 = 3.0;
pub const SHRINK_DURATION: f32 = 0.5;

/// Component for physical debris chunks produced by explosions and heavy impacts.
#[derive(Component, Debug, Clone)]
pub struct DebrisChunk {
    pub velocity: Vec3,
    pub ang_vel: Vec3,
    pub lifetime: f32,
    #[allow(dead_code)]
    pub max_lifetime: f32,
    pub bounce_count: u8,
}

impl DebrisChunk {
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

/// Stores the unscaled size of a debris piece for smooth fade-out shrinking.
#[derive(Component, Debug, Clone, Copy)]
pub struct DebrisInitialScale(pub Vec3);

/// Spawns 6–12 small physical cube/shard debris entities at the blast origin.
///
/// Radial outward and upward velocities are assigned along with random 3D angular rotation.
pub fn spawn_explosion_debris(
    commands: &mut Commands,
    pos: Vec3,
    radius: f32,
    count: usize,
    _color: [f32; 3],
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
) {
    let mut rng = rand::thread_rng();
    let n = if count > 0 {
        count
    } else {
        rng.gen_range(6..=12)
    };

    let scale_mult = (radius / 3.0).clamp(0.7, 1.4);

    for _ in 0..n {
        let angle = rng.gen_range(0.0..std::f32::consts::TAU);
        let elevation = rng.gen_range(0.1..0.6);
        let dir = Vec3::new(angle.cos(), elevation, angle.sin()).normalize();
        let speed = rng.gen_range(6.0..14.0) * (radius / 3.0).clamp(0.8, 1.6);
        let vel = (dir + Vec3::Y * 0.5).normalize() * speed;

        let ang_vel = Vec3::new(
            rng.gen_range(-12.0..12.0),
            rng.gen_range(-12.0..12.0),
            rng.gen_range(-12.0..12.0),
        );

        let scale = Vec3::new(
            rng.gen_range(0.06..0.18) * scale_mult,
            rng.gen_range(0.06..0.18) * scale_mult,
            rng.gen_range(0.06..0.18) * scale_mult,
        );

        let rot = Quat::from_euler(
            EulerRot::XYZ,
            rng.gen_range(0.0..std::f32::consts::TAU),
            rng.gen_range(0.0..std::f32::consts::TAU),
            rng.gen_range(0.0..std::f32::consts::TAU),
        );

        commands.spawn((
            InGameEntity,
            DebrisChunk::new(vel, ang_vel, MAX_LIFETIME),
            DebrisInitialScale(scale),
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            Transform::from_translation(pos + Vec3::Y * 0.05)
                .with_rotation(rot)
                .with_scale(scale),
            NotShadowCaster,
        ));
    }
}

/// Updates debris physics (gravity, tumbling, ground collision, bounce restitution, friction)
/// and handles lifetime shrink and despawn. Runs in `Phase::Present`.
pub fn update_debris(
    mut commands: Commands,
    time: Res<Time>,
    colliders: Query<(&Transform, &Collider), Without<DebrisChunk>>,
    mut q: Query<
        (
            Entity,
            &mut DebrisChunk,
            &mut Transform,
            Option<&DebrisInitialScale>,
        ),
        Without<Collider>,
    >,
) {
    if q.is_empty() {
        return;
    }
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    let boxes = collect_boxes(colliders.iter());

    for (e, mut chunk, mut tf, init_scale) in &mut q {
        let base_scale = match init_scale {
            Some(s) => s.0,
            None => {
                let s = tf.scale;
                commands.entity(e).insert(DebrisInitialScale(s));
                s
            }
        };

        chunk.lifetime -= dt;
        if chunk.lifetime <= 0.0 {
            commands.entity(e).despawn();
            continue;
        }

        // Shrinks scale in the last 0.5s of lifetime
        if chunk.lifetime < SHRINK_DURATION {
            let factor = (chunk.lifetime / SHRINK_DURATION).clamp(0.0, 1.0);
            tf.scale = base_scale * factor;
        }

        let speed_sq = chunk.velocity.length_squared();
        let ang_speed_sq = chunk.ang_vel.length_squared();

        // If settled (speed and angular speed are zero), skip simulation
        if speed_sq < 1e-6 && ang_speed_sq < 1e-6 {
            continue;
        }

        // Applies gravity (GRAVITY = 18.0)
        chunk.velocity.y -= GRAVITY * dt;

        // Rotates transform using ang_vel * dt
        let rot_delta = chunk.ang_vel * dt;
        let angle = rot_delta.length();
        if angle > 1e-5 {
            tf.rotate(Quat::from_axis_angle(rot_delta / angle, angle));
        }

        let mut next_pos = tf.translation + chunk.velocity * dt;

        // Checks collision against ground using ground_height
        let radius = 0.08;
        let floor_y = ground_height(next_pos, radius, tf.translation.y, &boxes);

        if next_pos.y <= floor_y {
            next_pos.y = floor_y;

            if chunk.velocity.y < 0.0 {
                chunk.bounce_count = chunk.bounce_count.saturating_add(1);

                // On floor hit: bounces with restitution (0.4), ground friction (0.7), and reduces angular velocity
                chunk.velocity.y = -chunk.velocity.y * RESTITUTION;
                chunk.velocity.x *= GROUND_FRICTION;
                chunk.velocity.z *= GROUND_FRICTION;
                chunk.ang_vel *= 0.5;

                // When speed is low or after 2-3 bounces, settles
                let current_speed = chunk.velocity.length();
                if chunk.bounce_count >= 3 || current_speed < 0.6 {
                    chunk.velocity = Vec3::ZERO;
                    chunk.ang_vel = Vec3::ZERO;
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
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(50)));
        app.add_systems(Update, update_debris);
        app.update();
        app
    }

    #[test]
    fn test_debris_chunk_creation() {
        let chunk = DebrisChunk::new(Vec3::new(1.0, 2.0, 3.0), Vec3::new(0.5, 0.5, 0.5), 3.0);
        assert_eq!(chunk.velocity, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(chunk.ang_vel, Vec3::new(0.5, 0.5, 0.5));
        assert_eq!(chunk.lifetime, 3.0);
        assert_eq!(chunk.max_lifetime, 3.0);
        assert_eq!(chunk.bounce_count, 0);
    }

    #[test]
    fn test_spawn_explosion_debris() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<StandardMaterial>>();

        let mut meshes = app.world_mut().resource_mut::<Assets<Mesh>>();
        let mesh = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
        let mut materials = app.world_mut().resource_mut::<Assets<StandardMaterial>>();
        let material = materials.add(StandardMaterial::default());

        let mut commands = app.world_mut().commands();
        spawn_explosion_debris(
            &mut commands,
            Vec3::new(0.0, 1.0, 0.0),
            4.0,
            8,
            [1.0, 0.5, 0.2],
            mesh,
            material,
        );

        app.update();

        let mut query = app.world_mut().query::<(&DebrisChunk, &Transform)>();
        let chunks: Vec<_> = query.iter(app.world()).collect();
        assert_eq!(chunks.len(), 8);

        for (chunk, tf) in chunks {
            assert_eq!(chunk.bounce_count, 0);
            assert_eq!(chunk.lifetime, MAX_LIFETIME);
            assert!(chunk.velocity.y > 0.0);
            assert!(chunk.velocity.length() > 3.0);
            assert!(chunk.ang_vel.length() > 0.0);
            assert!(tf.scale.x > 0.0 && tf.scale.y > 0.0 && tf.scale.z > 0.0);
        }
    }

    #[test]
    fn test_debris_gravity_and_tumbling() {
        let mut app = setup_test_app();
        let entity = app
            .world_mut()
            .spawn((
                DebrisChunk::new(Vec3::new(0.0, 5.0, 0.0), Vec3::new(2.0, 0.0, 0.0), 3.0),
                Transform::from_xyz(0.0, 10.0, 0.0),
            ))
            .id();

        app.update();

        let chunk = app.world().get::<DebrisChunk>(entity).unwrap();
        let tf = app.world().get::<Transform>(entity).unwrap();

        // Gravity 18.0 * 0.05 = 0.9 -> expected velocity.y ~ 5.0 - 0.9 = 4.1
        assert!((chunk.velocity.y - 4.1).abs() < 1e-3);
        assert!(tf.translation.y > 10.0);
        // Tumbling: rotation should not be identity
        assert_ne!(tf.rotation, Quat::IDENTITY);
    }

    #[test]
    fn test_debris_floor_bounce_restitution_and_friction() {
        let mut app = setup_test_app();
        let entity = app
            .world_mut()
            .spawn((
                DebrisChunk {
                    velocity: Vec3::new(4.0, -10.0, 2.0),
                    ang_vel: Vec3::new(5.0, 5.0, 5.0),
                    lifetime: 3.0,
                    max_lifetime: 3.0,
                    bounce_count: 0,
                },
                Transform::from_xyz(0.0, 0.1, 0.0),
            ))
            .id();

        app.update();

        let chunk = app.world().get::<DebrisChunk>(entity).unwrap();
        let tf = app.world().get::<Transform>(entity).unwrap();

        assert_eq!(tf.translation.y, 0.0);
        assert_eq!(chunk.bounce_count, 1);
        // Restitution: vel.y was inverted and scaled by RESTITUTION (0.4)
        assert!(chunk.velocity.y > 0.0);
        // Friction: horizontal speed reduced by GROUND_FRICTION (0.7)
        assert!((chunk.velocity.x - 4.0 * GROUND_FRICTION).abs() < 1e-3);
        assert!((chunk.velocity.z - 2.0 * GROUND_FRICTION).abs() < 1e-3);
        // Ang vel reduced
        assert!(chunk.ang_vel.length() < Vec3::new(5.0, 5.0, 5.0).length());
    }

    #[test]
    fn test_debris_settles_after_bounces() {
        let mut app = setup_test_app();
        let entity = app
            .world_mut()
            .spawn((
                DebrisChunk {
                    velocity: Vec3::new(2.0, -5.0, 0.0),
                    ang_vel: Vec3::new(3.0, 0.0, 0.0),
                    lifetime: 3.0,
                    max_lifetime: 3.0,
                    bounce_count: 2, // 3rd bounce will trigger settle
                },
                Transform::from_xyz(0.0, 0.1, 0.0),
            ))
            .id();

        app.update();

        let chunk = app.world().get::<DebrisChunk>(entity).unwrap();
        let tf = app.world().get::<Transform>(entity).unwrap();

        assert_eq!(chunk.bounce_count, 3);
        assert_eq!(chunk.velocity, Vec3::ZERO);
        assert_eq!(chunk.ang_vel, Vec3::ZERO);
        assert_eq!(tf.translation.y, 0.0);
    }

    #[test]
    fn test_debris_settles_on_low_speed() {
        let mut app = setup_test_app();
        let entity = app
            .world_mut()
            .spawn((
                DebrisChunk {
                    velocity: Vec3::new(0.1, -0.3, 0.1),
                    ang_vel: Vec3::new(0.5, 0.0, 0.0),
                    lifetime: 3.0,
                    max_lifetime: 3.0,
                    bounce_count: 0,
                },
                Transform::from_xyz(0.0, 0.01, 0.0),
            ))
            .id();

        app.update();

        let chunk = app.world().get::<DebrisChunk>(entity).unwrap();
        assert_eq!(chunk.velocity, Vec3::ZERO);
        assert_eq!(chunk.ang_vel, Vec3::ZERO);
    }

    #[test]
    fn test_debris_shrink_and_despawn() {
        let mut app = setup_test_app();
        let entity = app
            .world_mut()
            .spawn((
                DebrisChunk {
                    velocity: Vec3::ZERO,
                    ang_vel: Vec3::ZERO,
                    lifetime: 0.25, // half of SHRINK_DURATION (0.5)
                    max_lifetime: 3.0,
                    bounce_count: 3,
                },
                DebrisInitialScale(Vec3::ONE),
                Transform::from_xyz(0.0, 0.0, 0.0).with_scale(Vec3::ONE),
            ))
            .id();

        app.update(); // dt = 0.05, lifetime becomes 0.20

        let tf = app.world().get::<Transform>(entity).unwrap();
        let chunk = app.world().get::<DebrisChunk>(entity).unwrap();
        assert!((chunk.lifetime - 0.20).abs() < 1e-3);
        // Scale should shrink to 0.20 / 0.50 = 0.40
        assert!((tf.scale.x - 0.4).abs() < 1e-3);

        // Advance frames so lifetime reaches 0.0 and entity despawns
        for _ in 0..5 {
            app.update();
        }
        assert!(app.world().get_entity(entity).is_err());
    }
}
