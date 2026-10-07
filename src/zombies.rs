//! Zombie bodies: drives the rig from what the host says about each zombie
//! (crawling, swinging, flinching) and plays the death animation before the
//! body sinks away.

use bevy::prelude::*;
use rand::Rng;

use crate::rig::Rig;
use crate::{AppState, Collider, Enemy, EnemyStatus, Phase, Replicated};

pub struct ZombiePlugin;

impl Plugin for ZombiePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (start_dying, sync_rigs, dying)
                .chain()
                .in_set(Phase::Present)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

/// Marks something the game is done with. Zombies fall over first; anything
/// else is removed straight away.
#[derive(Component)]
struct Killed;

/// A zombie playing its death animation.
#[derive(Component)]
struct Dying {
    t: f32,
    kind: u8,
}

/// How long a body lies there before it starts sinking, and how long it sinks.
const LIE: f32 = 3.0;
const SINK: f32 = 1.0;

/// Removes `e` from play: zombies collapse, everything else just goes.
pub fn kill(commands: &mut Commands, e: Entity) {
    if let Ok(mut ent) = commands.get_entity(e) {
        ent.try_insert(Killed);
    }
}

/// Removes `e` from play with a directional ballistic death impulse.
pub fn kill_with_impulse(commands: &mut Commands, e: Entity, impulse: crate::ragdoll::DeathImpulse) {
    if let Ok(mut ent) = commands.get_entity(e) {
        ent.try_insert((Killed, impulse));
    }
}

fn start_dying(
    mut commands: Commands,
    killed: Query<(
        Entity,
        &Transform,
        Has<Enemy>,
        Has<Rig>,
        Option<&crate::sim::EnemyBrain>,
        Option<&crate::ragdoll::DeathImpulse>,
    ), Added<Killed>>,
) {
    let mut rng = rand::thread_rng();
    for (e, tf, enemy, rig, brain, death_impulse) in &killed {
        if enemy && rig {
            let is_sprinter = brain.is_some_and(|b| b.is_sprinter);
            let crawler = brain.is_some_and(|b| b.crawler);
            let scale = tf.scale.x;

            let mut ragdoll = crate::ragdoll::ActiveRagdoll::new(
                tf.translation,
                tf.rotation,
                scale,
                crawler,
            );

            if let Some(impulse) = death_impulse {
                ragdoll.apply_impulse(impulse);
            } else if let Some(b) = brain {
                if b.knockback.length_squared() > 0.01 {
                    ragdoll.apply_impulse(&crate::ragdoll::DeathImpulse {
                        hit_zone: crate::ragdoll::HitZone::Torso,
                        point_of_impact: tf.translation + Vec3::Y * 0.9,
                        impulse: b.knockback,
                        headshot: false,
                    });
                }
            }

            if is_sprinter {
                // High-momentum sprinters carry forward velocity into the active ragdoll
                let fwd = tf.rotation * Vec3::NEG_Z;
                ragdoll.nodes[crate::ragdoll::RagdollBone::Hips.index()].vel += fwd * 4.5;
                ragdoll.nodes[crate::ragdoll::RagdollBone::Head.index()].vel += fwd * 5.0;
            }

            let kind = if is_sprinter && rng.gen_bool(0.65) {
                1
            } else {
                rng.gen_range(0..3)
            };

            // Out of the game straight away (no more shots, nav, snapshots),
            // but the body stays for its active ragdoll simulation.
            commands
                .entity(e)
                .remove::<(
                    Enemy,
                    Replicated,
                    Collider,
                    crate::sim::EnemyBrain,
                    crate::net::Puppet,
                )>()
                .insert((
                    ragdoll,
                    Dying {
                        t: 0.0,
                        kind,
                    },
                ));
        } else {
            commands.entity(e).despawn();
        }
    }
}

/// Copies the host's status onto the rig so every machine animates the same.
fn sync_rigs(
    mut zombies: Query<(
        &EnemyStatus,
        &Replicated,
        &mut Rig,
        Option<&crate::sim::EnemyBrain>,
    ), (With<Enemy>, Without<Dying>)>,
) {
    for (status, r, mut rig, brain) in &mut zombies {
        rig.crawl = status.crawler;
        let is_sprinter = brain.is_some_and(|b| b.is_sprinter);
        let speed_mult = if is_sprinter { 1.45 } else { 1.0 };
        // The smash lands when the host's slam does (fast sprinters swing with rabid frenzy).
        rig.swing_rate = (crate::sim::BRUTE_WINDUP / crate::sim::slam_spec(r.kind).0) * speed_mult;
        if status.attacking && rig.swing > 0.6 / speed_mult {
            rig.swing = 0.0;
            rig.attack_variant = if is_sprinter {
                if rig.attack_variant == 1 { 2 } else { 1 }
            } else {
                (rig.attack_variant + 1) % 3
            };
        }
        if status.flash > 0.0 {
            rig.flinch = 1.0;
        }
    }
}

fn dying(
    mut commands: Commands,
    time: Res<Time>,
    mut bodies: Query<(
        Entity,
        &mut Dying,
        &mut Rig,
        &mut Transform,
        Option<&crate::ragdoll::ActiveRagdoll>,
    )>,
) {
    let dt = time.delta_secs();
    for (e, mut d, mut rig, mut tf, maybe_ragdoll) in &mut bodies {
        d.t += dt;
        if maybe_ragdoll.is_none() {
            rig.dying = Some((d.t, d.kind));
            if d.t > LIE {
                tf.translation.y -= dt * 0.6 / SINK;
            }
            if d.t > LIE + SINK {
                commands.entity(e).despawn();
            }
        }
    }
}

