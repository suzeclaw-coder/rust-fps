//! Visual effects: bullet tracers, explosions (fireball, shockwave, smoke,
//! debris, sparks, scorch mark), Heal Pulse (green ring, light column and
//! rising crosses), Dash afterimages and lightning arcs. The host broadcasts
//! effects so every player sees them.

pub mod auras;
pub mod casings;
pub mod debris;
mod spells;

use bevy::pbr::NotShadowCaster;
use bevy::prelude::*;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::f32::consts::TAU;

use crate::kit::{c, Kit};
use crate::sim::powers::{zone as zk, PLAGUE_GROW, PLAGUE_MAX};
use crate::{AppState, InGameEntity, Phase};

pub struct FxPlugin;

impl Plugin for FxPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FxQueue>()
            .init_resource::<FxOutbox>()
            .init_resource::<Lines>()
            .add_systems(
                Update,
                zone_fx
                    .in_set(Phase::Present)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(PreStartup, (setup, spells::setup))
            .add_systems(
                Update,
                (
                    play,
                    animate,
                    bullets,
                    particles,
                    debris::update_debris,
                    casings::update_shell_casings,
                    spells::fall,
                    spells::warns,
                    spells::twirls,
                    spells::hooks,
                    spells::trails,
                    draw_lines,
                )
                    .chain()
                    .in_set(Phase::Present)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(OnExit(AppState::InGame), clear);
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Fx {
    /// Bullet tracer from a player (`shooter`) so they can skip their own.
    Tracer {
        shooter: u8,
        a: [f32; 3],
        b: [f32; 3],
    },
    Explosion {
        pos: [f32; 3],
        radius: f32,
        color: [f32; 3],
    },
    Ring {
        pos: [f32; 3],
        radius: f32,
        color: [f32; 3],
    },
    Lightning {
        a: [f32; 3],
        b: [f32; 3],
    },
    /// Valkyrie's Storm Leap crashing down.
    Slam {
        pos: [f32; 3],
        radius: f32,
    },
    Heal {
        pos: [f32; 3],
        radius: f32,
    },
    /// A player dashed from `a` to `b` (skipped by the dasher, who plays it
    /// locally straight away).
    Dash {
        player: u8,
        a: [f32; 3],
        b: [f32; 3],
    },
    /// A sword cut across an arc in front of `pos`.
    Slash {
        pos: [f32; 3],
        dir: [f32; 3],
        radius: f32,
    },
    /// A lasting area effect (see `sim::powers::zone` for the kinds).
    /// `follow` is a player id (255 stays put).
    Zone {
        pos: [f32; 3],
        radius: f32,
        life: f32,
        follow: u8,
        kind: u8,
    },
    /// A zone of this kind at `pos` was used up early (Catalyst).
    ZoneEnd {
        pos: [f32; 3],
        kind: u8,
    },
    /// A player used an ability (for their cast animation and aura).
    Cast {
        player: u8,
        slot: u8,
    },
    /// The look of an ability going off (see spells.rs).
    Spell {
        ability: crate::data::Ability,
        pos: [f32; 3],
        dir: [f32; 3],
        size: f32,
    },
    /// Something dropping out of the sky onto `to`, landing after `time`
    /// seconds (see `sim::powers::falling`).
    Falling {
        kind: u8,
        from: [f32; 3],
        to: [f32; 3],
        time: f32,
    },
    /// A player pinged a spot or an enemy (see pings.rs).
    Ping {
        player: u8,
        pos: [f32; 3],
        target: u32,
    },
    /// Blood splatter and flesh gore from bullet or damage impact on a zombie.
    Blood {
        pos: [f32; 3],
        dir: [f32; 3],
        headshot: bool,
    },
    /// Decapitation blood fountain spraying upward when a zombie dies from a headshot.
    Decapitation {
        pos: [f32; 3],
    },
    /// Metallic/energy sparks emitted on poise break or heavy shield hit.
    Sparks {
        pos: [f32; 3],
        count: u32,
    },
}

/// Effects to show on this machine this frame.
#[derive(Resource, Default)]
pub struct FxQueue(pub Vec<Fx>);

/// Host only: effects to send to clients with the next snapshot.
#[derive(Resource, Default)]
pub struct FxOutbox(pub Vec<Fx>);

/// Short-lived lines: (start, end, color, time left).
#[derive(Resource, Default)]
pub struct Lines(Vec<(Vec3, Vec3, Color, f32)>);

#[derive(Resource)]
pub struct FxAssets {
    ball: Handle<Mesh>,
    disc: Handle<Mesh>,
    beam: Handle<Mesh>,
    ring: Handle<Mesh>,
    cube: Handle<Mesh>,
    cross: Handle<Mesh>,
    marker: Handle<Mesh>,
    ghost: Handle<Mesh>,
    fire: Handle<StandardMaterial>,
    smoke: Handle<StandardMaterial>,
    debris: Handle<StandardMaterial>,
    spark: Handle<StandardMaterial>,
    scorch: Handle<StandardMaterial>,
    heal: Handle<StandardMaterial>,
    heal_soft: Handle<StandardMaterial>,
    beam_core: Handle<StandardMaterial>,
    ghost_mat: Handle<StandardMaterial>,
    tracer: Handle<StandardMaterial>,
    tracer_mesh: Handle<Mesh>,
    bolt: Handle<StandardMaterial>,
    blood: Handle<StandardMaterial>,
    blood_mist: Handle<StandardMaterial>,
    flesh: Handle<StandardMaterial>,
    bone: Handle<StandardMaterial>,
    casing_mesh: Handle<Mesh>,
    casing_mat: Handle<StandardMaterial>,
}

/// A jagged lightning bolt from `from` to `to`, made of glowing segments.
fn bolt(
    commands: &mut Commands,
    a: &FxAssets,
    from: Vec3,
    to: Vec3,
    width: f32,
    rng: &mut impl Rng,
) {
    let n = ((from.distance(to) / 1.2) as usize).clamp(3, 14);
    let jitter = (from.distance(to) * 0.06).clamp(0.15, 0.9);
    let mut prev = from;
    for i in 1..=n {
        let f = i as f32 / n as f32;
        let mut q = from.lerp(to, f);
        if i < n {
            q += Vec3::new(
                rng.gen_range(-1.0..1.0),
                rng.gen_range(-1.0..1.0),
                rng.gen_range(-1.0..1.0),
            ) * jitter;
        }
        let d = q - prev;
        let len = d.length().max(0.01);
        burst(
            commands,
            &a.cube,
            a.bolt.clone(),
            Transform::from_translation((prev + q) / 2.0)
                .with_rotation(Quat::from_rotation_arc(Vec3::Y, d / len))
                .with_scale(Vec3::new(width, len, width)),
            0.18,
            Grow::Bolt(width),
        );
        prev = q;
    }
}

/// A bullet streak flying from the muzzle to where the shot landed.
#[derive(Component)]
struct Bullet {
    a: Vec3,
    b: Vec3,
    travelled: f32,
}

/// How fast tracers fly and how long the glowing streak is.
const TRACER_SPEED: f32 = 260.0;
const TRACER_LEN: f32 = 3.2;

/// A lasting area effect on screen (see `Fx::Zone`).
#[derive(Component)]
pub struct ZoneFx {
    life: f32,
    max: f32,
    radius: f32,
    follow: Option<u8>,
    kind: u8,
    emit: f32,
    /// The radius its rings were built at (Plague Bloom grows from it).
    start: f32,
}

/// One of the Reaper's scythes: its zone and its starting angle.
#[derive(Component)]
struct Scythe(Entity, f32);

/// A zone's colour and the colour of its light.
fn zone_colors(kind: u8) -> ([f32; 3], Color) {
    let c = match kind {
        zk::FORTRESS => [1.0, 0.8, 0.3],
        zk::HEAL => [0.35, 1.0, 0.5],
        zk::REAPER => [0.4, 1.0, 0.8],
        zk::ACID => [0.55, 1.0, 0.15],
        zk::TOXIC => [0.7, 0.85, 0.2],
        zk::PLAGUE => [0.45, 0.85, 0.1],
        _ => [1.0, 0.45, 0.1],
    };
    (c, Color::srgb(c[0], c[1], c[2]))
}

/// How a one-shot shape changes over its life.
#[derive(Clone, Copy)]
enum Grow {
    /// Sphere growing to this radius.
    Ball(f32),
    /// Flat shape (ring, disc) spreading to this radius.
    Flat(f32),
    /// Keeps its size, shrinks away at the end (scorch marks).
    Hold,
    /// A tall column that thins out.
    Column,
    /// A light that dims.
    Light,
    /// A beam that hits at full width and narrows.
    Column2(f32),
    /// A lightning segment: full brightness, then thins out (width).
    Bolt(f32),
    /// A crescent cut: flashes out a little bigger and thins away.
    Cut(f32),
}

/// A growing, fading one-shot shape.
#[derive(Component)]
struct Burst {
    life: f32,
    max: f32,
    grow: Grow,
}

/// A moving particle that changes size over its life.
#[derive(Component)]
struct Particle {
    vel: Vec3,
    life: f32,
    max: f32,
    gravity: f32,
    drag: f32,
    size: (f32, f32),
    /// Fraction of the life spent growing in at the start.
    pop: f32,
    spin: Vec3,
    /// Stays put once it hits the ground.
    lands: bool,
}

/// An ice spike: shoots up out of the ground, then sinks back.
#[derive(Component)]
struct Spike {
    life: f32,
    max: f32,
    scale: Vec3,
    base: Vec3,
    /// Seconds before it bursts up.
    delay: f32,
}

fn cross_kit() -> Kit {
    let mut k = Kit::new();
    let g = c(0.4, 1.0, 0.55);
    k.cuboid(Vec3::ZERO, Vec3::new(0.3, 0.1, 0.1), g);
    k.cuboid(Vec3::ZERO, Vec3::new(0.1, 0.3, 0.1), g);
    k
}

/// Orbital target: a ring of segments with corner ticks and a centre dot.
fn marker_kit() -> Kit {
    let mut k = Kit::new();
    let red = c(1.0, 0.2, 0.15);
    for i in 0..12 {
        let a = i as f32 / 12.0 * TAU;
        k.cuboid_rot(
            Vec3::new(a.cos(), 0.0, a.sin()),
            Vec3::new(0.06, 0.02, 0.38),
            Quat::from_rotation_y(-a),
            red,
        );
    }
    for i in 0..4 {
        let a = i as f32 / 4.0 * TAU + 0.785;
        k.cuboid_rot(
            Vec3::new(a.cos() * 0.75, 0.0, a.sin() * 0.75),
            Vec3::new(0.3, 0.02, 0.05),
            Quat::from_rotation_y(-a),
            red,
        );
    }
    k.cyl(Vec3::ZERO, 0.08, 0.02, Quat::IDENTITY, red);
    k
}

fn unlit(materials: &mut Assets<StandardMaterial>, color: Color) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: color,
        unlit: true,
        alpha_mode: if color.alpha() < 1.0 {
            AlphaMode::Blend
        } else {
            AlphaMode::Opaque
        },
        ..default()
    })
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let assets = FxAssets {
        ball: meshes.add(Sphere::new(1.0).mesh().ico(2).unwrap()),
        disc: meshes.add(Cylinder::new(1.0, 0.04).mesh().resolution(32)),
        beam: meshes.add(Cylinder::new(1.0, 60.0).mesh().resolution(20)),
        ring: meshes.add(
            Torus::new(0.94, 1.0)
                .mesh()
                .major_resolution(40)
                .minor_resolution(6),
        ),
        cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        tracer_mesh: meshes.add(Capsule3d::new(0.5, 1.0).mesh().latitudes(4).longitudes(8)),
        tracer: materials.add(StandardMaterial {
            base_color: LinearRgba::rgb(9.0, 5.5, 1.8).into(),
            unlit: true,
            ..default()
        }),
        bolt: materials.add(StandardMaterial {
            base_color: LinearRgba::rgb(5.0, 7.0, 12.0).into(),
            unlit: true,
            ..default()
        }),
        cross: meshes.add(cross_kit().build_or_empty()),
        marker: meshes.add(marker_kit().build_or_empty()),
        ghost: meshes.add(Capsule3d::new(0.35, 1.1)),
        fire: unlit(&mut materials, Color::srgb(1.0, 0.6, 0.18)),
        smoke: materials.add(StandardMaterial {
            base_color: Color::srgba(0.16, 0.15, 0.15, 0.55),
            alpha_mode: AlphaMode::Blend,
            perceptual_roughness: 1.0,
            ..default()
        }),
        debris: materials.add(StandardMaterial {
            base_color: Color::srgb(0.12, 0.1, 0.09),
            perceptual_roughness: 0.9,
            ..default()
        }),
        spark: unlit(&mut materials, Color::srgb(1.0, 0.85, 0.4)),
        scorch: materials.add(StandardMaterial {
            base_color: Color::srgba(0.02, 0.02, 0.02, 0.75),
            alpha_mode: AlphaMode::Blend,
            perceptual_roughness: 1.0,
            ..default()
        }),
        heal: unlit(&mut materials, Color::srgb(0.35, 1.0, 0.5)),
        heal_soft: unlit(&mut materials, Color::srgba(0.35, 1.0, 0.5, 0.25)),
        beam_core: unlit(&mut materials, Color::srgb(1.0, 0.95, 0.85)),
        ghost_mat: unlit(&mut materials, Color::srgba(0.4, 0.75, 1.0, 0.3)),
        blood: materials.add(StandardMaterial {
            base_color: Color::srgb(0.55, 0.03, 0.03),
            perceptual_roughness: 0.25,
            reflectance: 0.85,
            emissive: LinearRgba::rgb(0.08, 0.005, 0.005),
            ..default()
        }),
        blood_mist: materials.add(StandardMaterial {
            base_color: Color::srgba(0.60, 0.03, 0.03, 0.65),
            alpha_mode: AlphaMode::Blend,
            perceptual_roughness: 1.0,
            ..default()
        }),
        flesh: materials.add(StandardMaterial {
            base_color: Color::srgb(0.35, 0.04, 0.04),
            perceptual_roughness: 0.5,
            reflectance: 0.6,
            ..default()
        }),
        bone: materials.add(StandardMaterial {
            base_color: Color::srgb(0.88, 0.85, 0.78),
            perceptual_roughness: 0.7,
            reflectance: 0.35,
            ..default()
        }),
        casing_mesh: meshes.add(casings::casing_mesh()),
        casing_mat: materials.add(casings::casing_material()),
    };
    commands.insert_resource(assets);
}

pub fn rgb(c: Color) -> [f32; 3] {
    let s = c.to_srgba();
    [s.red, s.green, s.blue]
}

fn glow_material(
    materials: &mut Assets<StandardMaterial>,
    color: [f32; 3],
    alpha: f32,
) -> Handle<StandardMaterial> {
    let c = Color::srgba(color[0], color[1], color[2], alpha);
    materials.add(StandardMaterial {
        base_color: c,
        emissive: LinearRgba::rgb(color[0], color[1], color[2]) * 6.0,
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    })
}

fn rand_dir(rng: &mut impl Rng) -> Vec3 {
    let a = rng.gen_range(0.0..TAU);
    let z: f32 = rng.gen_range(-1.0..1.0);
    let r = (1.0 - z * z).sqrt();
    Vec3::new(r * a.cos(), z, r * a.sin())
}

fn particle(
    commands: &mut Commands,
    mesh: &Handle<Mesh>,
    mat: &Handle<StandardMaterial>,
    pos: Vec3,
    p: Particle,
) {
    commands.spawn((
        InGameEntity,
        Mesh3d(mesh.clone()),
        MeshMaterial3d(mat.clone()),
        Transform::from_translation(pos).with_scale(Vec3::splat(p.size.0.max(0.001))),
        NotShadowCaster,
        p,
    ));
}

fn burst(
    commands: &mut Commands,
    mesh: &Handle<Mesh>,
    mat: Handle<StandardMaterial>,
    tf: Transform,
    life: f32,
    grow: Grow,
) {
    commands.spawn((
        InGameEntity,
        Burst {
            life,
            max: life,
            grow,
        },
        Mesh3d(mesh.clone()),
        MeshMaterial3d(mat),
        tf,
        NotShadowCaster,
    ));
}

fn flash_light(
    commands: &mut Commands,
    pos: Vec3,
    color: Color,
    intensity: f32,
    range: f32,
    life: f32,
) {
    commands.spawn((
        InGameEntity,
        Burst {
            life,
            max: life,
            grow: Grow::Light,
        },
        PointLight {
            intensity,
            color,
            range,
            ..default()
        },
        Transform::from_translation(pos),
    ));
}

/// Fireball, shockwave, smoke, debris, sparks and a scorch mark.
fn explosion(
    commands: &mut Commands,
    a: &FxAssets,
    materials: &mut Assets<StandardMaterial>,
    pos: Vec3,
    radius: f32,
    color: [f32; 3],
) {
    let mut rng = rand::thread_rng();
    let ground = pos.with_y(0.0);
    let col = Color::srgb(color[0], color[1], color[2]);
    burst(
        commands,
        &a.ball,
        glow_material(materials, color, 0.75),
        Transform::from_translation(pos).with_scale(Vec3::splat(0.2)),
        0.4,
        Grow::Ball(radius * 0.8),
    );
    burst(
        commands,
        &a.ring,
        glow_material(materials, color, 0.6),
        Transform::from_translation(ground + Vec3::Y * 0.15).with_scale(Vec3::new(0.3, 3.0, 0.3)),
        0.35,
        Grow::Flat(radius * 1.25),
    );
    flash_light(commands, pos + Vec3::Y, col, 500_000.0, radius * 4.0, 0.3);
    let n = (radius * 2.0) as i32 + 4;
    for _ in 0..n {
        let d = rand_dir(&mut rng);
        let d = Vec3::new(d.x, d.y.abs() * 0.8 + 0.2, d.z);
        particle(
            commands,
            &a.ball,
            &a.fire,
            pos + d * 0.3,
            Particle {
                vel: d * rng.gen_range(2.0..5.0) * radius / 4.0,
                life: rng.gen_range(0.35..0.6),
                max: 0.6,
                gravity: -1.0,
                drag: 3.0,
                size: (radius * 0.18, radius * 0.32),
                pop: 0.1,
                spin: Vec3::ZERO,
                lands: false,
            },
        );
    }
    for _ in 0..n + 4 {
        let d = rand_dir(&mut rng);
        let start = pos + Vec3::new(d.x, d.y.abs(), d.z) * radius * 0.3;
        particle(
            commands,
            &a.ball,
            &a.smoke,
            start,
            Particle {
                vel: Vec3::new(d.x * 1.5, rng.gen_range(1.0..2.8), d.z * 1.5),
                life: rng.gen_range(1.4..2.4),
                max: 2.4,
                gravity: -0.3,
                drag: 1.2,
                size: (radius * 0.15, radius * rng.gen_range(0.35..0.55)),
                pop: 0.15,
                spin: Vec3::ZERO,
                lands: false,
            },
        );
    }
    let debris_count = rng.gen_range(6..=12);
    debris::spawn_explosion_debris(
        commands,
        pos,
        radius,
        debris_count,
        color,
        a.cube.clone(),
        a.debris.clone(),
    );
    for _ in 0..14 {
        let d = rand_dir(&mut rng);
        particle(
            commands,
            &a.cube,
            &a.spark,
            pos,
            Particle {
                vel: Vec3::new(d.x, d.y.abs(), d.z) * rng.gen_range(8.0..15.0),
                life: rng.gen_range(0.25..0.5),
                max: 0.5,
                gravity: 12.0,
                drag: 1.0,
                size: (0.05, 0.02),
                pop: 0.0,
                spin: Vec3::ZERO,
                lands: false,
            },
        );
    }
    // Scorch mark, gone after a while.
    burst(
        commands,
        &a.disc,
        a.scorch.clone(),
        Transform::from_translation(ground + Vec3::Y * 0.02).with_scale(Vec3::new(
            radius * 0.55,
            1.0,
            radius * 0.55,
        )),
        7.0,
        Grow::Hold,
    );
}

/// Spawns visceral blood splatter, flesh gore chunks, mist, and bone chips.
fn blood_splatter(
    commands: &mut Commands,
    a: &FxAssets,
    pos: Vec3,
    dir: Vec3,
    headshot: bool,
    rng: &mut impl Rng,
) {
    let dir = dir.normalize_or(Vec3::Z);
    if headshot {
        // Dramatic explosive red blood mist burst
        for _ in 0..8 {
            let d = rand_dir(rng);
            particle(
                commands,
                &a.ball,
                &a.blood_mist,
                pos + d * rng.gen_range(0.05..0.2),
                Particle {
                    vel: d * rng.gen_range(1.5..4.5) + Vec3::Y * rng.gen_range(0.5..1.8),
                    life: rng.gen_range(0.35..0.65),
                    max: 0.65,
                    gravity: -0.4,
                    drag: 2.2,
                    size: (rng.gen_range(0.12..0.22), rng.gen_range(0.35..0.65)),
                    pop: 0.15,
                    spin: Vec3::ZERO,
                    lands: false,
                },
            );
        }
        // Skull bone chips flying outward violently
        for _ in 0..12 {
            let d = rand_dir(rng);
            let s = rng.gen_range(0.035..0.065);
            particle(
                commands,
                &a.cube,
                &a.bone,
                pos,
                Particle {
                    vel: (d * 1.5 + Vec3::Y * 0.8).normalize_or_zero() * rng.gen_range(5.0..11.0),
                    life: rng.gen_range(0.8..1.5),
                    max: 1.5,
                    gravity: 16.0,
                    drag: 0.3,
                    size: (s, s),
                    pop: 0.0,
                    spin: rand_dir(rng) * 22.0,
                    lands: true,
                },
            );
        }
        // Crimson arterial droplets flying outward in an explosive arc
        for _ in 0..24 {
            let d = (dir * 1.5 + rand_dir(rng)).normalize_or_zero();
            let s = rng.gen_range(0.04..0.09);
            particle(
                commands,
                &a.ball,
                &a.blood,
                pos,
                Particle {
                    vel: d * rng.gen_range(4.0..12.0) + Vec3::Y * rng.gen_range(1.0..3.5),
                    life: rng.gen_range(0.6..1.2),
                    max: 1.2,
                    gravity: 15.0,
                    drag: 0.8,
                    size: (s, s * 0.4),
                    pop: 0.05,
                    spin: Vec3::ZERO,
                    lands: true,
                },
            );
        }
        // Flesh / gore chunks
        for _ in 0..8 {
            let d = rand_dir(rng);
            let s = rng.gen_range(0.05..0.10);
            particle(
                commands,
                &a.cube,
                &a.flesh,
                pos,
                Particle {
                    vel: (d + dir * 0.8).normalize_or_zero() * rng.gen_range(3.0..8.0)
                        + Vec3::Y * rng.gen_range(1.0..3.0),
                    life: rng.gen_range(0.8..1.6),
                    max: 1.6,
                    gravity: 15.0,
                    drag: 0.4,
                    size: (s, s * 0.7),
                    pop: 0.0,
                    spin: rand_dir(rng) * 15.0,
                    lands: true,
                },
            );
        }
        // Visceral crimson light flash illuminating surroundings
        flash_light(
            commands,
            pos + Vec3::Y * 0.2,
            Color::srgb(0.85, 0.04, 0.04),
            85_000.0,
            6.5,
            0.18,
        );
        // Ground blood stain
        let ground = pos.with_y(0.02);
        let puddle_r = rng.gen_range(0.45..0.85);
        burst(
            commands,
            &a.disc,
            a.blood.clone(),
            Transform::from_translation(ground).with_scale(Vec3::new(puddle_r, 1.0, puddle_r)),
            7.0,
            Grow::Hold,
        );
    } else {
        // Standard body hit:
        // Directional cone of blood droplets in bullet direction (wound exit)
        for _ in 0..12 {
            let spread = Vec3::new(
                rng.gen_range(-0.35..0.35),
                rng.gen_range(-0.25..0.4),
                rng.gen_range(-0.35..0.35),
            );
            let d = (dir + spread).normalize_or_zero();
            let s = rng.gen_range(0.03..0.06);
            particle(
                commands,
                &a.ball,
                &a.blood,
                pos + d * 0.05,
                Particle {
                    vel: d * rng.gen_range(3.5..8.5) + Vec3::Y * rng.gen_range(0.5..2.0),
                    life: rng.gen_range(0.4..0.9),
                    max: 0.9,
                    gravity: 14.0,
                    drag: 1.2,
                    size: (s, s * 0.5),
                    pop: 0.05,
                    spin: Vec3::ZERO,
                    lands: true,
                },
            );
        }
        // Back-splatter from entrance wound
        for _ in 0..5 {
            let back = -dir
                + Vec3::new(
                    rng.gen_range(-0.4..0.4),
                    rng.gen_range(0.0..0.4),
                    rng.gen_range(-0.4..0.4),
                );
            let s = rng.gen_range(0.025..0.045);
            particle(
                commands,
                &a.ball,
                &a.blood,
                pos,
                Particle {
                    vel: back.normalize_or_zero() * rng.gen_range(2.0..5.0) + Vec3::Y * 0.8,
                    life: rng.gen_range(0.3..0.6),
                    max: 0.6,
                    gravity: 12.0,
                    drag: 1.8,
                    size: (s, s * 0.3),
                    pop: 0.05,
                    spin: Vec3::ZERO,
                    lands: true,
                },
            );
        }
        // Flesh spray
        for _ in 0..5 {
            let d = (dir
                + Vec3::new(
                    rng.gen_range(-0.5..0.5),
                    rng.gen_range(-0.2..0.6),
                    rng.gen_range(-0.5..0.5),
                ))
            .normalize_or_zero();
            let s = rng.gen_range(0.035..0.065);
            particle(
                commands,
                &a.cube,
                &a.flesh,
                pos,
                Particle {
                    vel: d * rng.gen_range(2.5..6.0) + Vec3::Y * rng.gen_range(0.5..1.8),
                    life: rng.gen_range(0.5..1.1),
                    max: 1.1,
                    gravity: 15.0,
                    drag: 0.6,
                    size: (s, s * 0.6),
                    pop: 0.0,
                    spin: rand_dir(rng) * 12.0,
                    lands: true,
                },
            );
        }
        // Blood mist puff
        for _ in 0..2 {
            particle(
                commands,
                &a.ball,
                &a.blood_mist,
                pos,
                Particle {
                    vel: dir * 1.2 + Vec3::Y * rng.gen_range(0.3..0.8),
                    life: rng.gen_range(0.25..0.45),
                    max: 0.45,
                    gravity: -0.2,
                    drag: 2.5,
                    size: (0.08, 0.24),
                    pop: 0.1,
                    spin: Vec3::ZERO,
                    lands: false,
                },
            );
        }
        // Small ground stain
        if rng.gen_bool(0.4) {
            let ground = pos.with_y(0.02);
            let puddle_r = rng.gen_range(0.25..0.45);
            burst(
                commands,
                &a.disc,
                a.blood.clone(),
                Transform::from_translation(ground).with_scale(Vec3::new(puddle_r, 1.0, puddle_r)),
                5.0,
                Grow::Hold,
            );
        }
    }
}

/// Decapitation blood fountain spraying upward from the neck stump when a zombie dies from a headshot.
fn decapitation_fountain(
    commands: &mut Commands,
    a: &FxAssets,
    pos: Vec3,
    rng: &mut impl Rng,
) {
    // Geyser of arterial blood droplets
    for _ in 0..36 {
        let spread_x = rng.gen_range(-0.5..0.5);
        let spread_z = rng.gen_range(-0.5..0.5);
        let up_speed = rng.gen_range(5.5..11.0);
        let s = rng.gen_range(0.04..0.085);
        particle(
            commands,
            &a.ball,
            &a.blood,
            pos + Vec3::new(rng.gen_range(-0.08..0.08), 0.0, rng.gen_range(-0.08..0.08)),
            Particle {
                vel: Vec3::new(spread_x * 2.8, up_speed, spread_z * 2.8),
                life: rng.gen_range(0.7..1.4),
                max: 1.4,
                gravity: 16.0,
                drag: 0.5,
                size: (s, s * 0.4),
                pop: 0.05,
                spin: Vec3::ZERO,
                lands: true,
            },
        );
    }
    // High-pressure arterial spurts (taller, thinner)
    for _ in 0..12 {
        let spread_x = rng.gen_range(-0.25..0.25);
        let spread_z = rng.gen_range(-0.25..0.25);
        let up_speed = rng.gen_range(8.0..14.0);
        let s = rng.gen_range(0.03..0.06);
        particle(
            commands,
            &a.ball,
            &a.blood,
            pos,
            Particle {
                vel: Vec3::new(spread_x * 1.5, up_speed, spread_z * 1.5),
                life: rng.gen_range(0.9..1.6),
                max: 1.6,
                gravity: 18.0,
                drag: 0.4,
                size: (s, s * 0.3),
                pop: 0.05,
                spin: Vec3::ZERO,
                lands: true,
            },
        );
    }
    // Throat / neck flesh chunks spraying up and tumbling
    for _ in 0..10 {
        let spread_x = rng.gen_range(-0.6..0.6);
        let spread_z = rng.gen_range(-0.6..0.6);
        let up_speed = rng.gen_range(3.5..7.5);
        let s = rng.gen_range(0.06..0.12);
        particle(
            commands,
            &a.cube,
            &a.flesh,
            pos,
            Particle {
                vel: Vec3::new(spread_x * 3.0, up_speed, spread_z * 3.0),
                life: rng.gen_range(1.0..1.8),
                max: 1.8,
                gravity: 16.0,
                drag: 0.3,
                size: (s, s * 0.6),
                pop: 0.0,
                spin: rand_dir(rng) * 16.0,
                lands: true,
            },
        );
    }
    // Neck vertebrae / bone splinters
    for _ in 0..8 {
        let s = rng.gen_range(0.035..0.065);
        particle(
            commands,
            &a.cube,
            &a.bone,
            pos,
            Particle {
                vel: Vec3::new(
                    rng.gen_range(-2.0..2.0),
                    rng.gen_range(4.0..8.0),
                    rng.gen_range(-2.0..2.0),
                ),
                life: rng.gen_range(0.8..1.5),
                max: 1.5,
                gravity: 17.0,
                drag: 0.25,
                size: (s, s),
                pop: 0.0,
                spin: rand_dir(rng) * 20.0,
                lands: true,
            },
        );
    }
    // Rising blood mist column
    for _ in 0..6 {
        particle(
            commands,
            &a.ball,
            &a.blood_mist,
            pos + Vec3::Y * rng.gen_range(0.1..0.5),
            Particle {
                vel: Vec3::new(
                    rng.gen_range(-0.4..0.4),
                    rng.gen_range(2.0..4.0),
                    rng.gen_range(-0.4..0.4),
                ),
                life: rng.gen_range(0.5..0.9),
                max: 0.9,
                gravity: -0.2,
                drag: 1.6,
                size: (0.15, rng.gen_range(0.4..0.7)),
                pop: 0.2,
                spin: Vec3::ZERO,
                lands: false,
            },
        );
    }
    // Deep crimson flash light at the severed neck
    flash_light(
        commands,
        pos + Vec3::Y * 0.3,
        Color::srgb(0.75, 0.02, 0.02),
        110_000.0,
        7.0,
        0.25,
    );
    // Large spreading ground puddle at the zombie's feet
    let ground = pos.with_y(0.02);
    burst(
        commands,
        &a.disc,
        a.blood.clone(),
        Transform::from_translation(ground).with_scale(Vec3::new(0.9, 1.0, 0.9)),
        8.0,
        Grow::Hold,
    );
}

pub fn play(
    mut commands: Commands,
    mut queue: ResMut<FxQueue>,
    mut lines: ResMut<Lines>,
    assets: Res<FxAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    roster: Res<crate::Roster>,
    mut auras: ResMut<crate::auras::Auras>,
    spell_assets: Res<spells::SpellAssets>,
    mut zone_q: Query<(&Transform, &mut ZoneFx)>,
) {
    let mut rng = rand::thread_rng();
    let a = &*assets;
    let sa = &*spell_assets;
    let mut casing_origins: Vec<Vec3> = Vec::new();
    for fx in queue.0.drain(..) {
        match fx {
            Fx::Tracer { a: from, b: to, .. } => {
                let (from, to) = (Vec3::from_array(from), Vec3::from_array(to));
                // A faint smoke line where it passed, and the glowing round.
                lines
                    .0
                    .push((from, to, Color::srgba(0.85, 0.85, 0.8, 0.18), 0.12));
                commands.spawn((
                    InGameEntity,
                    Bullet {
                        a: from,
                        b: to,
                        travelled: 0.0,
                    },
                    Mesh3d(a.tracer_mesh.clone()),
                    MeshMaterial3d(a.tracer.clone()),
                    Transform::from_translation(from).with_scale(Vec3::ZERO),
                    NotShadowCaster,
                ));
                let shot_dir = (to - from).normalize_or(Vec3::Z);
                if !casing_origins.iter().any(|&prev| prev.distance_squared(from) < 0.04 * 0.04) {
                    casing_origins.push(from);
                    casings::spawn_shell_casing(
                        &mut commands,
                        from,
                        shot_dir,
                        a.casing_mesh.clone(),
                        a.casing_mat.clone(),
                    );
                }
            }
            Fx::Explosion { pos, radius, color } => {
                explosion(
                    &mut commands,
                    a,
                    &mut materials,
                    Vec3::from_array(pos),
                    radius,
                    color,
                );
            }
            Fx::Ring { pos, radius, color } => {
                burst(
                    &mut commands,
                    &a.ring,
                    glow_material(&mut materials, color, 0.6),
                    Transform::from_translation(Vec3::from_array(pos) + Vec3::Y * 0.1)
                        .with_scale(Vec3::new(0.3, 2.0, 0.3)),
                    0.6,
                    Grow::Flat(radius),
                );
            }
            Fx::Heal { pos, radius } => {
                let p = Vec3::from_array(pos);
                burst(
                    &mut commands,
                    &a.ring,
                    a.heal.clone(),
                    Transform::from_translation(p + Vec3::Y * 0.12)
                        .with_scale(Vec3::new(0.3, 3.0, 0.3)),
                    0.7,
                    Grow::Flat(radius),
                );
                burst(
                    &mut commands,
                    &a.disc,
                    a.heal_soft.clone(),
                    Transform::from_translation(p + Vec3::Y * 0.05)
                        .with_scale(Vec3::new(0.3, 1.0, 0.3)),
                    0.8,
                    Grow::Flat(radius),
                );
                // Light column.
                commands.spawn((
                    InGameEntity,
                    Burst {
                        life: 0.9,
                        max: 0.9,
                        grow: Grow::Column,
                    },
                    Mesh3d(a.beam.clone()),
                    MeshMaterial3d(a.heal_soft.clone()),
                    Transform::from_translation(p + Vec3::Y * 30.0)
                        .with_scale(Vec3::new(0.9, 1.0, 0.9)),
                    NotShadowCaster,
                ));
                flash_light(
                    &mut commands,
                    p + Vec3::Y * 1.5,
                    Color::srgb(0.4, 1.0, 0.5),
                    300_000.0,
                    radius * 2.0,
                    0.8,
                );
                for _ in 0..18 {
                    let ang = rng.gen_range(0.0..TAU);
                    let r = radius * rng.gen_range(0.0f32..1.0).sqrt();
                    let start =
                        p + Vec3::new(ang.cos() * r, rng.gen_range(0.2..1.2), ang.sin() * r);
                    particle(
                        &mut commands,
                        &a.cross,
                        &a.heal,
                        start,
                        Particle {
                            vel: Vec3::Y * rng.gen_range(1.2..2.6),
                            life: rng.gen_range(0.9..1.5),
                            max: 1.5,
                            gravity: 0.0,
                            drag: 0.5,
                            size: (0.0, rng.gen_range(0.8..1.3)),
                            pop: 0.2,
                            spin: Vec3::Y * 2.0,
                            lands: false,
                        },
                    );
                }
            }
            Fx::Lightning { a: from, b: to } => {
                let (from, to) = (Vec3::from_array(from), Vec3::from_array(to));
                bolt(&mut commands, a, from, to, 0.05, &mut rng);
                // A thinner fork off to the side.
                let mid = from.lerp(to, rng.gen_range(0.3..0.6));
                let fork = mid
                    + Vec3::new(
                        rng.gen_range(-1.5..1.5),
                        rng.gen_range(-1.0..0.5),
                        rng.gen_range(-1.5..1.5),
                    );
                bolt(&mut commands, a, mid, fork, 0.025, &mut rng);
                flash_light(
                    &mut commands,
                    to + Vec3::Y * 0.5,
                    Color::srgb(0.6, 0.8, 1.0),
                    40_000.0,
                    7.0,
                    0.15,
                );
            }
            Fx::Slam { pos, radius } => {
                let p = Vec3::from_array(pos);
                for _ in 0..4 {
                    let off = Vec3::new(rng.gen_range(-1.0..1.0), 0.0, rng.gen_range(-1.0..1.0))
                        * radius
                        * 0.6;
                    bolt(
                        &mut commands,
                        a,
                        p + off + Vec3::Y * 14.0,
                        p + off * 0.3 + Vec3::Y * 0.1,
                        0.06,
                        &mut rng,
                    );
                }
                burst(
                    &mut commands,
                    &a.ring,
                    glow_material(&mut materials, [0.55, 0.85, 1.0], 0.7),
                    Transform::from_translation(p + Vec3::Y * 0.1)
                        .with_scale(Vec3::new(0.3, 2.5, 0.3)),
                    0.5,
                    Grow::Flat(radius),
                );
                for _ in 0..16 {
                    let d = rand_dir(&mut rng);
                    particle(
                        &mut commands,
                        &a.cube,
                        &a.debris,
                        p + Vec3::Y * 0.2,
                        Particle {
                            vel: Vec3::new(d.x, d.y.abs() + 0.5, d.z) * rng.gen_range(3.0..7.0),
                            life: rng.gen_range(0.6..1.1),
                            max: 1.1,
                            gravity: 14.0,
                            drag: 0.5,
                            size: (0.12, 0.08),
                            pop: 0.0,
                            spin: rand_dir(&mut rng) * 8.0,
                            lands: true,
                        },
                    );
                }
                flash_light(
                    &mut commands,
                    p + Vec3::Y,
                    Color::srgb(0.6, 0.8, 1.0),
                    120_000.0,
                    12.0,
                    0.3,
                );
            }
            Fx::Slash { pos, dir, radius } => {
                let (p, d) = (Vec3::from_array(pos), Vec3::from_array(dir));
                let a0 = d.z.atan2(d.x);
                // Layered crescents of light sweeping across the arc.
                for k in 0..3 {
                    let roll = [0.12, -0.2, 0.35][k];
                    let col = [[1.0, 0.95, 0.75], [1.0, 0.75, 0.3], [1.0, 0.85, 0.5]][k];
                    spells::cut(
                        &mut commands,
                        sa,
                        &mut materials,
                        p + Vec3::Y * (k as f32 * 0.25 - 0.25),
                        d,
                        roll,
                        radius * (0.55 + 0.12 * k as f32),
                        col,
                        0.22 + 0.05 * k as f32,
                    );
                }
                for _ in 0..20 {
                    let ang = a0 + rng.gen_range(-1.1..1.1);
                    let out = Vec3::new(ang.cos(), 0.0, ang.sin());
                    particle(
                        &mut commands,
                        &a.ball,
                        &a.spark,
                        p + out * radius * rng.gen_range(0.3..0.9),
                        Particle {
                            vel: out * rng.gen_range(2.0..6.0) + Vec3::Y * rng.gen_range(0.0..2.0),
                            life: rng.gen_range(0.2..0.45),
                            max: 0.45,
                            gravity: 6.0,
                            drag: 2.0,
                            size: (0.05, 0.0),
                            pop: 0.0,
                            spin: Vec3::ZERO,
                            lands: false,
                        },
                    );
                }
                flash_light(
                    &mut commands,
                    p,
                    Color::srgb(1.0, 0.85, 0.5),
                    80_000.0,
                    radius * 1.5,
                    0.25,
                );
            }
            Fx::Zone {
                pos,
                radius,
                life,
                follow,
                kind,
            } => {
                let p = Vec3::from_array(pos);
                let zone = commands
                    .spawn((
                        InGameEntity,
                        ZoneFx {
                            life,
                            max: life,
                            radius,
                            follow: (follow != 255).then_some(follow),
                            kind,
                            emit: 0.0,
                            start: radius,
                        },
                        Transform::from_translation(p),
                        Visibility::default(),
                    ))
                    .id();
                let (color, light) = zone_colors(kind);
                // Zones that follow a player keep their aura lit.
                if follow != 255 {
                    auras.hold(follow, light, life);
                }
                commands.entity(zone).with_children(|z| {
                    z.spawn((
                        Mesh3d(a.ring.clone()),
                        MeshMaterial3d(glow_material(&mut materials, color, 0.55)),
                        Transform::from_xyz(0.0, 0.08, 0.0)
                            .with_scale(Vec3::new(radius, 2.0, radius)),
                        NotShadowCaster,
                    ));
                    let floor = match kind {
                        zk::FIRE => a.scorch.clone(),
                        zk::TOXIC => glow_material(&mut materials, color, 0.06),
                        zk::ACID | zk::PLAGUE => glow_material(&mut materials, color, 0.3),
                        _ => glow_material(&mut materials, color, 0.12),
                    };
                    z.spawn((
                        Mesh3d(a.disc.clone()),
                        MeshMaterial3d(floor),
                        Transform::from_xyz(0.0, 0.03, 0.0)
                            .with_scale(Vec3::new(radius, 1.0, radius)),
                        NotShadowCaster,
                    ));
                    z.spawn((
                        PointLight {
                            intensity: 120_000.0,
                            color: light,
                            range: radius * 2.0,
                            ..default()
                        },
                        Transform::from_xyz(0.0, 1.5, 0.0),
                    ));
                    match kind {
                        zk::FORTRESS => {
                            // A see-through golden shell banded with rings.
                            z.spawn((
                                Mesh3d(a.ball.clone()),
                                MeshMaterial3d(glow_material(&mut materials, color, 0.1)),
                                Transform::from_scale(Vec3::new(radius, radius * 0.7, radius)),
                                NotShadowCaster,
                            ));
                            for i in 1..4 {
                                let h = i as f32 * 0.22;
                                let r = (1.0 - h * h).sqrt() * radius;
                                z.spawn((
                                    Mesh3d(a.ring.clone()),
                                    MeshMaterial3d(glow_material(&mut materials, color, 0.45)),
                                    Transform::from_xyz(0.0, h * radius * 0.7, 0.0)
                                        .with_scale(Vec3::new(r, 1.0, r)),
                                    NotShadowCaster,
                                ));
                            }
                        }
                        zk::REAPER => {
                            // Three spectral scythes whirling round.
                            for i in 0..3 {
                                z.spawn((
                                    Scythe(zone, i as f32 / 3.0 * TAU),
                                    Mesh3d(sa.scythe.clone()),
                                    MeshMaterial3d(sa.teal.clone()),
                                    Transform::default(),
                                    NotShadowCaster,
                                ));
                            }
                        }
                        _ => {}
                    }
                });
            }
            Fx::ZoneEnd { pos, kind } => {
                // A zone used up early (Catalyst): the host already showed
                // the blast, so just fade it out.
                let p = Vec3::from_array(pos);
                if let Some((_, mut z)) = zone_q
                    .iter_mut()
                    .filter(|(tf, z)| z.kind == kind && z.follow.is_none() && tf.translation.distance(p) < 1.0)
                    .min_by(|a, b| a.0.translation.distance(p).total_cmp(&b.0.translation.distance(p)))
                {
                    z.life = z.life.min(0.3);
                }
            }
            Fx::Ping { .. } => {}
            Fx::Spell {
                ability,
                pos,
                dir,
                size,
            } => spells::spell(
                &mut commands,
                a,
                sa,
                &mut materials,
                &mut lines,
                ability,
                Vec3::from_array(pos),
                Vec3::from_array(dir),
                size,
            ),
            Fx::Falling {
                kind,
                from,
                to,
                time,
            } => spells::falling(
                &mut commands,
                sa,
                kind,
                Vec3::from_array(from),
                Vec3::from_array(to),
                time,
            ),
            Fx::Cast { player, slot } => {
                if let Some(p) = roster.0.get(&player) {
                    auras.cast(player, p.kit[slot.min(2) as usize]);
                }
            }
            Fx::Dash { a: from, b: to, .. } => {
                let (from, to) = (Vec3::from_array(from), Vec3::from_array(to));
                for i in 0..5 {
                    let t = i as f32 / 5.0;
                    let pos = from.lerp(to, t) + Vec3::Y * 0.9;
                    particle(
                        &mut commands,
                        &a.ghost,
                        &a.ghost_mat,
                        pos,
                        Particle {
                            vel: Vec3::ZERO,
                            life: 0.2 + t * 0.25,
                            max: 0.45,
                            gravity: 0.0,
                            drag: 0.0,
                            size: (1.0, 0.6),
                            pop: 0.0,
                            spin: Vec3::ZERO,
                            lands: false,
                        },
                    );
                }
                for _ in 0..10 {
                    let off = Vec3::new(
                        rng.gen_range(-0.5..0.5),
                        rng.gen_range(0.2..1.7),
                        rng.gen_range(-0.5..0.5),
                    );
                    lines.0.push((
                        from + off,
                        to + off * 0.6,
                        Color::srgba(0.6, 0.85, 1.0, 0.8),
                        0.18,
                    ));
                }
            }
            Fx::Blood { pos, dir, headshot } => {
                blood_splatter(
                    &mut commands,
                    a,
                    Vec3::from_array(pos),
                    Vec3::from_array(dir),
                    headshot,
                    &mut rng,
                );
            }
            Fx::Decapitation { pos } => {
                decapitation_fountain(
                    &mut commands,
                    a,
                    Vec3::from_array(pos),
                    &mut rng,
                );
            }
            Fx::Sparks { pos, count } => {
                let p = Vec3::from_array(pos);
                for _ in 0..count {
                    let dir = rand_dir(&mut rng);
                    particle(
                        &mut commands,
                        &a.cube,
                        &a.spark,
                        p,
                        Particle {
                            vel: dir * rng.gen_range(3.0..8.0) + Vec3::Y * 2.0,
                            life: 0.35,
                            max: 0.35,
                            gravity: 9.8,
                            drag: 1.0,
                            size: (0.06, 0.02),
                            pop: 0.0,
                            spin: rand_dir(&mut rng) * 10.0,
                            lands: true,
                        },
                    );
                }
            }
        }
    }
}

fn animate(
    mut commands: Commands,
    time: Res<Time>,
    mut bursts: Query<(Entity, &mut Burst, &mut Transform, Option<&mut PointLight>)>,
    mut spikes: Query<(Entity, &mut Spike, &mut Transform), Without<Burst>>,
) {
    let dt = time.delta_secs();
    for (e, mut b, mut tf, light) in &mut bursts {
        b.life -= dt;
        if b.life <= 0.0 {
            commands.entity(e).despawn();
            continue;
        }
        let t = 1.0 - b.life / b.max;
        let flat = |tf: &mut Transform, r: f32| {
            let r = r.max(0.001);
            tf.scale = Vec3::new(r, tf.scale.y, r);
        };
        match b.grow {
            Grow::Light => {
                if let Some(mut light) = light {
                    light.intensity *= 0.85;
                }
            }
            Grow::Ball(r) => tf.scale = Vec3::splat(r * (0.3 + 0.7 * t.sqrt())),
            Grow::Flat(r) => flat(&mut tf, r * t.max(0.05)),
            Grow::Hold => {
                let r = tf.scale.x;
                if t > 0.75 {
                    flat(&mut tf, r * (1.0 - dt * 4.0));
                }
            }
            Grow::Column => {
                let w = (0.9 * (1.0 - t)).max(0.001);
                tf.scale = Vec3::new(w, 1.0, w);
            }
            Grow::Column2(r) => {
                // Slams in at full width, then narrows away.
                let w = (r * (1.0 - t * t)).max(0.001);
                tf.scale = Vec3::new(w, 1.0, w);
            }
            Grow::Bolt(w) => {
                let w = (w * (1.0 - t)).max(0.001);
                tf.scale = Vec3::new(w, tf.scale.y, w);
            }
            Grow::Cut(k) => {
                let s = k * (0.75 + 0.45 * t.sqrt());
                let thin = (1.0 - t).powf(1.5).max(0.001);
                tf.scale = Vec3::new(s, s * thin, s * (0.4 + 0.6 * thin));
            }
        }
    }
    for (e, mut s, mut tf) in &mut spikes {
        s.life -= dt;
        if s.life <= 0.0 {
            commands.entity(e).despawn();
            continue;
        }
        let age = s.max - s.life - s.delay;
        let grow = (age / 0.1).clamp(0.0, 1.0);
        let sink = (s.life / 0.4).clamp(0.0, 1.0);
        let k = grow.min(sink);
        tf.scale = s.scale * k.max(0.001);
        tf.translation = s.base - Vec3::Y * (1.0 - sink) * 0.5;
    }
}

/// Moves tracer rounds along their path; at the end they kick up sparks.
fn bullets(
    mut commands: Commands,
    time: Res<Time>,
    a: Res<FxAssets>,
    mut q: Query<(Entity, &mut Bullet, &mut Transform)>,
) {
    let mut rng = rand::thread_rng();
    for (e, mut b, mut tf) in &mut q {
        let path = b.b - b.a;
        let dist = path.length();
        let dir = path / dist.max(1e-4);
        let step = TRACER_SPEED * time.delta_secs();
        b.travelled += step;
        let arrived = b.travelled >= dist;
        let just_arrived = arrived && b.travelled - step < dist;
        let head = b.travelled.min(dist);
        let tail = (b.travelled - TRACER_LEN).clamp(0.0, dist);
        if tail >= dist - 1e-3 {
            commands.entity(e).despawn();
            continue;
        }
        let len = (head - tail).max(0.01);
        tf.translation = b.a + dir * (head + tail) / 2.0;
        tf.rotation = Quat::from_rotation_arc(Vec3::Y, dir);
        // Thin and long; thicker for a moment as it leaves the muzzle.
        let w = 0.016 + 0.02 * (1.0 - (b.travelled / 6.0).min(1.0));
        tf.scale = Vec3::new(w, len / 2.0, w);
        if just_arrived && dist < GUN_RANGE_FX {
            for _ in 0..5 {
                let d = rand_dir(&mut rng);
                particle(
                    &mut commands,
                    &a.cube,
                    &a.spark,
                    b.b - dir * 0.05,
                    Particle {
                        vel: (d - dir * 1.2).normalize_or_zero() * rng.gen_range(3.0..7.0),
                        life: rng.gen_range(0.12..0.3),
                        max: 0.3,
                        gravity: 9.0,
                        drag: 2.0,
                        size: (0.05, 0.02),
                        pop: 0.0,
                        spin: Vec3::ZERO,
                        lands: false,
                    },
                );
            }
            particle(
                &mut commands,
                &a.ball,
                &a.smoke,
                b.b - dir * 0.08,
                Particle {
                    vel: -dir * 0.6 + Vec3::Y * 0.3,
                    life: 0.5,
                    max: 0.5,
                    gravity: -0.5,
                    drag: 3.0,
                    size: (0.04, 0.16),
                    pop: 0.1,
                    spin: Vec3::ZERO,
                    lands: false,
                },
            );
        }
    }
}

/// Shots that hit nothing end at full range: no sparks out there.
const GUN_RANGE_FX: f32 = crate::weapons::GUN_RANGE - 1.0;

fn particles(
    mut commands: Commands,
    time: Res<Time>,
    mut q: Query<(Entity, &mut Particle, &mut Transform)>,
) {
    let dt = time.delta_secs();
    for (e, mut p, mut tf) in &mut q {
        p.life -= dt;
        if p.life <= 0.0 {
            commands.entity(e).despawn();
            continue;
        }
        let g = p.gravity;
        p.vel.y -= g * dt;
        let drag = (1.0 - p.drag * dt).max(0.0);
        p.vel *= drag;
        tf.translation += p.vel * dt;
        if p.lands && tf.translation.y < 0.05 {
            tf.translation.y = 0.05;
            p.vel = Vec3::ZERO;
            p.spin = Vec3::ZERO;
        }
        if p.spin != Vec3::ZERO {
            let s = p.spin * dt;
            tf.rotate(Quat::from_euler(EulerRot::XYZ, s.x, s.y, s.z));
        }
        let t = 1.0 - p.life / p.max;
        let size = p.size.0 + (p.size.1 - p.size.0) * t.min(1.0);
        let pop = if p.pop > 0.0 {
            (t / p.pop).min(1.0)
        } else {
            1.0
        };
        let fade = (p.life / (p.max * 0.3)).min(1.0);
        let s = (size * pop * fade).max(0.001);
        if p.size == (0.05, 0.02) {
            // Sparks stretch along their motion.
            let dir = p.vel.normalize_or_zero();
            if dir != Vec3::ZERO {
                tf.rotation = Quat::from_rotation_arc(Vec3::Z, dir);
            }
            tf.scale = Vec3::new(0.025, 0.025, 0.12 + p.vel.length() * 0.02) * fade;
        } else {
            tf.scale = Vec3::splat(s);
        }
    }
}

/// Moves lasting zones with their player, grows the Plague Bloom, whirls
/// the Reaper's scythes and keeps flames, gas and motes coming.
#[allow(clippy::too_many_arguments)]
fn zone_fx(
    mut commands: Commands,
    time: Res<Time>,
    roster: Res<crate::Roster>,
    assets: Res<FxAssets>,
    mut lines: ResMut<Lines>,
    mut zones: Query<(Entity, &mut ZoneFx, &mut Transform), Without<Scythe>>,
    mut scythes: Query<(&Scythe, &mut Transform, &GlobalTransform), Without<ZoneFx>>,
    sa: Res<spells::SpellAssets>,
) {
    let dt = time.delta_secs();
    let t = time.elapsed_secs();
    let a = &*assets;
    let mut rng = rand::thread_rng();
    let mut live = Vec::new();
    for (e, mut z, mut tf) in &mut zones {
        z.life -= dt;
        if z.life <= 0.0 {
            commands.entity(e).despawn();
            continue;
        }
        if let Some(p) = z.follow.and_then(|id| roster.0.get(&id)) {
            tf.translation = p.feet();
        }
        if z.kind == zk::PLAGUE {
            z.radius = (z.radius + PLAGUE_GROW * dt).min(PLAGUE_MAX);
        }
        let fade = (z.life / 0.4)
            .min(1.0)
            .min((z.max - z.life) / 0.25 + 0.2)
            .min(1.0);
        let grow = z.radius / z.start.max(0.1);
        tf.scale = Vec3::new(fade * grow, 1.0, fade * grow);
        live.push((e, z.radius));
        z.emit -= dt;
        if z.emit > 0.0 {
            continue;
        }
        let p = tf.translation;
        let ang = rng.gen_range(0.0..TAU);
        let out = Vec3::new(ang.cos(), 0.0, ang.sin());
        // A random spot inside the zone.
        let inside = p + out * z.radius * rng.gen_range(0.0f32..1.0).sqrt();
        let mote = |vel: Vec3, life: f32, size: (f32, f32)| Particle {
            vel,
            life,
            max: life,
            gravity: 0.0,
            drag: 0.5,
            size,
            pop: 0.15,
            spin: Vec3::ZERO,
            lands: false,
        };
        match z.kind {
            zk::FORTRESS => {
                // Gold sparks rising and embers licking round the rim.
                z.emit = 0.04;
                particle(
                    &mut commands,
                    &a.cube,
                    &a.spark,
                    inside + Vec3::Y * 0.1,
                    mote(Vec3::Y * rng.gen_range(2.0..4.0), 0.8, (0.05, 0.0)),
                );
                let q = p + out * z.radius * 0.95 + Vec3::Y * 0.1;
                particle(&mut commands, &a.ball, &a.fire, q, mote(Vec3::Y * 2.5, 0.5, (0.25, 0.05)));
            }
            zk::HEAL => {
                // Crosses and soft green motes drifting up.
                z.emit = 0.12;
                particle(
                    &mut commands,
                    &a.cross,
                    &a.heal,
                    inside + Vec3::Y * 0.2,
                    Particle {
                        spin: Vec3::Y * 2.0,
                        ..mote(Vec3::Y * rng.gen_range(0.8..1.6), 1.2, (0.0, 0.5))
                    },
                );
                particle(&mut commands, &a.ball, &a.heal_soft, inside, mote(Vec3::Y * 0.6, 1.4, (0.2, 0.6)));
            }
            zk::REAPER => {
                // Spectral mist curling round the edge.
                z.emit = 0.05;
                let tan = Vec3::new(-ang.sin(), 0.0, ang.cos());
                particle(
                    &mut commands,
                    &a.ball,
                    &sa.spirit,
                    p + out * z.radius * rng.gen_range(0.6..1.0) + Vec3::Y * rng.gen_range(0.2..1.4),
                    mote(tan * 3.0 + Vec3::Y * 0.3, 0.9, (0.3, 0.8)),
                );
            }
            zk::ACID | zk::PLAGUE => {
                // Bubbles that swell and pop, and a little gas.
                z.emit = if z.kind == zk::PLAGUE { 0.03 } else { 0.06 };
                particle(
                    &mut commands,
                    &a.ball,
                    &sa.toxic,
                    inside + Vec3::Y * 0.05,
                    mote(Vec3::Y * 0.3, rng.gen_range(0.4..0.8), (0.05, rng.gen_range(0.12..0.25))),
                );
                if rng.gen_bool(0.35) {
                    particle(
                        &mut commands,
                        &a.ball,
                        &sa.gas,
                        inside + Vec3::Y * 0.3,
                        mote(Vec3::Y * rng.gen_range(0.4..1.0), 1.6, (0.4, 1.2)),
                    );
                }
                if z.kind == zk::PLAGUE && rng.gen_bool(0.3) {
                    // Rot creeping out along the ground.
                    let a0 = p + out * z.radius * 0.3 + Vec3::Y * 0.05;
                    let a1 = p + Quat::from_rotation_y(rng.gen_range(-0.3..0.3)) * out * z.radius + Vec3::Y * 0.05;
                    lines.0.push((a0, a1, Color::srgb(0.4, 0.75, 0.1), 0.4));
                }
            }
            zk::TOXIC => {
                // Rolling yellow-green gas, kept low so it doesn't blind you.
                z.emit = 0.1;
                particle(
                    &mut commands,
                    &a.ball,
                    &sa.gas,
                    inside + Vec3::Y * rng.gen_range(0.2..0.9),
                    Particle {
                        drag: 0.3,
                        pop: 0.25,
                        ..mote(
                            Vec3::new(rng.gen_range(-0.4..0.4), rng.gen_range(0.1..0.4), rng.gen_range(-0.4..0.4)),
                            2.0,
                            (0.5, 1.3),
                        )
                    },
                );
            }
            _ => {
                // A pool of fire.
                z.emit = 0.025;
                for _ in 0..2 {
                    let ang = rng.gen_range(0.0..TAU);
                    let r = z.radius * rng.gen_range(0.0f32..1.0).sqrt();
                    let q = p + Vec3::new(ang.cos() * r, 0.1, ang.sin() * r);
                    particle(
                        &mut commands,
                        &a.ball,
                        &a.fire,
                        q,
                        Particle {
                            gravity: -1.0,
                            pop: 0.1,
                            ..mote(Vec3::Y * rng.gen_range(1.5..3.5), rng.gen_range(0.35..0.7), (0.3, 0.05))
                        },
                    );
                }
            }
        }
    }
    // The Reaper's scythes whirl round, handles pointing in, leaving
    // spectral trails.
    let trail = rng.gen_bool((dt * 30.0).min(1.0) as f64);
    for (s, mut tf, gt) in &mut scythes {
        let Some(&(_, r)) = live.iter().find(|(e, _)| *e == s.0) else {
            continue;
        };
        let ang = s.1 + t * 7.0;
        let out = Vec3::new(ang.cos(), 0.0, ang.sin());
        tf.translation = out * r * 0.55 + Vec3::Y * (1.0 + 0.25 * (t * 3.0 + s.1).sin());
        tf.rotation = Transform::IDENTITY.looking_to(out, Vec3::Y).rotation * Quat::from_rotation_z(0.25);
        tf.scale = Vec3::splat(1.6);
        if trail {
            particle(
                &mut commands,
                &a.ball,
                &sa.spirit,
                gt.translation(),
                Particle {
                    vel: Vec3::ZERO,
                    life: 0.3,
                    max: 0.3,
                    gravity: 0.0,
                    drag: 0.0,
                    size: (0.25, 0.0),
                    pop: 0.0,
                    spin: Vec3::ZERO,
                    lands: false,
                },
            );
        }
    }
}

fn draw_lines(time: Res<Time>, mut lines: ResMut<Lines>, mut gizmos: Gizmos) {
    let dt = time.delta_secs();
    for (a, b, color, life) in lines.0.iter_mut() {
        gizmos.line(*a, *b, *color);
        *life -= dt;
    }
    lines.0.retain(|l| l.3 > 0.0);
}

fn clear(mut lines: ResMut<Lines>, mut queue: ResMut<FxQueue>, mut out: ResMut<FxOutbox>) {
    lines.0.clear();
    queue.0.clear();
    out.0.clear();
}

/// Host helper: show an effect here and send it to everyone else.
pub fn emit(queue: &mut FxQueue, out: &mut FxOutbox, fx: Fx) {
    queue.0.push(fx.clone());
    out.0.push(fx);
}
