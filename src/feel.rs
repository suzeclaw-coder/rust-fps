//! Game feel: camera shake, the dust (or pollen, or ash) drifting in the
//! air, and warming up the effects at the start of a match so the first
//! grenade doesn't stutter while its shaders are built.

use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use rand::Rng;
use std::collections::HashMap;

use crate::config::Settings;
use crate::fx::{Fx, FxQueue};
use crate::player::LocalPlayer;
use crate::rig::Rig;
use crate::{AppState, InGameEntity, MatchState, Phase, Replicated};

/// Camera shake, 0 to 1. Anything can add to it; it fades on its own.
#[derive(Resource, Default)]
pub struct Shake(pub f32);

impl Shake {
    pub fn add(&mut self, amount: f32) {
        self.0 = (self.0 + amount.max(0.0)).min(1.0);
    }
}

/// HitStop (Impact Freeze) resource for combat feel and physical impact weight.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct HitStop {
    pub timer: f32,
    pub time_scale: f32,
}

impl HitStop {
    #[allow(dead_code)]
    pub fn trigger(&mut self, duration: f32) {
        self.timer = self.timer.max(duration);
        self.time_scale = 0.0;
    }

    pub fn update(&mut self, dt: f32) {
        if self.timer > 0.0 {
            self.timer = (self.timer - dt).max(0.0);
            if self.timer == 0.0 {
                self.time_scale = 1.0;
            }
        } else {
            self.time_scale = 1.0;
        }
    }

    #[allow(dead_code)]
    pub fn is_frozen(&self) -> bool {
        self.timer > 0.0
    }
}

/// Landing dip & spring compression when hitting the ground after jumping or falling.
#[derive(Resource, Default)]
pub struct LandingDip {
    /// Vertical displacement in meters (negative is downward compression).
    pub offset_y: f32,
    /// Pitch tilt offset in radians (negative pitches camera downward on landing).
    pub pitch_offset: f32,
    /// Vertical velocity of the damped spring.
    pub velocity: f32,
    /// Previous on_ground state for edge detection.
    pub was_grounded: bool,
}

/// Low-health dynamic feedback (< 30% HP): arterial pulse & vignette intensity.
#[derive(Resource, Default)]
pub struct LowHealthFx {
    /// Urgency factor: 0.0 at >= 30% HP, scaling to 1.0 at 0% HP.
    pub intensity: f32,
    /// Rhythmic double-beat pulse waveform (0.0 to 1.0).
    pub pulse: f32,
    /// Heartbeat phase timer (0.0 to 1.0).
    pub heartbeat_timer: f32,
}

/// Dynamic red vignette overlay for critical low health.
#[derive(Component)]
pub struct LowHealthVignette;

#[derive(Resource)]
pub struct VignetteAssets {
    pub image: Handle<Image>,
}

/// How far the camera moves at full shake (it goes with the square, so
/// small shakes stay small).
const SHAKE_MAX: f32 = 0.09;
/// Shake lost per second.
const SHAKE_FADE: f32 = 2.5;

pub struct FeelPlugin;

impl Plugin for FeelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Shake>()
            .init_resource::<HitStop>()
            .init_resource::<LandingDip>()
            .init_resource::<LowHealthFx>()
            .add_systems(Startup, setup_vignette_assets)
            .add_systems(
                Update,
                (
                    update_hit_stop,
                    shake_sources.before(crate::fx::play),
                    update_low_health,
                    drift,
                    warm_up.before(crate::fx::play),
                )
                    .in_set(Phase::Present)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                OnEnter(AppState::InGame),
                (
                    spawn_motes.after(crate::game::start_match),
                    spawn_low_health_vignette,
                    start_warm_up,
                ),
            )
            .add_systems(OnExit(AppState::InGame), (reset_feel, reset_dip))
            .add_systems(
                PostUpdate,
                apply_camera_feel
                    .before(bevy::transform::TransformSystems::Propagate)
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

pub fn update_hit_stop(time: Res<Time>, mut hit_stop: ResMut<HitStop>) {
    hit_stop.update(time.delta_secs());
}

fn reset_feel(mut shake: ResMut<Shake>, mut fx: ResMut<LowHealthFx>) {
    shake.0 = 0.0;
    fx.intensity = 0.0;
    fx.pulse = 0.0;
}

fn reset_dip(mut dip: ResMut<LandingDip>) {
    dip.offset_y = 0.0;
    dip.pitch_offset = 0.0;
    dip.velocity = 0.0;
    dip.was_grounded = true;
}

/// Creates a soft procedural radial vignette image texture.
fn create_vignette_image() -> Image {
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    const N: u32 = 128;
    let mut data = Vec::with_capacity((N * N * 4) as usize);
    for y in 0..N {
        for x in 0..N {
            let u = (x as f32 + 0.5) / N as f32 * 2.0 - 1.0;
            let v = (y as f32 + 0.5) / N as f32 * 2.0 - 1.0;
            // Elliptical falloff matching 16:9 displays
            let d = (u * u * 0.82 + v * v).sqrt();
            let t = ((d - 0.45) / 0.70).clamp(0.0, 1.0);
            let alpha = t * t * (3.0 - 2.0 * t); // smoothstep
            data.extend_from_slice(&[255, 255, 255, (alpha * 255.0) as u8]);
        }
    }
    Image::new(
        Extent3d {
            width: N,
            height: N,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

fn setup_vignette_assets(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let image = images.add(create_vignette_image());
    commands.insert_resource(VignetteAssets { image });
}

fn spawn_low_health_vignette(mut commands: Commands, assets: Res<VignetteAssets>) {
    commands.spawn((
        InGameEntity,
        LowHealthVignette,
        ImageNode::new(assets.image.clone()).with_color(Color::NONE),
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        Pickable::IGNORE,
        GlobalZIndex(-4),
    ));
}

/// Updates low health heartbeat and arterial screen darkening vignette when health < 30%.
fn update_low_health(
    time: Res<Time>,
    session: Res<crate::Session>,
    roster: Res<crate::Roster>,
    mut fx: ResMut<LowHealthFx>,
    mut vignette: Query<&mut ImageNode, With<LowHealthVignette>>,
) {
    let dt = time.delta_secs();
    let Some(me) = roster.me(&session) else {
        fx.intensity = (fx.intensity - 3.0 * dt).max(0.0);
        return;
    };
    let max_hp = me.max_health();
    let hp_ratio = (me.health / max_hp).clamp(0.0, 1.0);

    if hp_ratio < 0.30 && me.alive {
        // Urgency factor: 0.0 at 30% HP, ramping to 1.0 near death
        let target_urgency = (1.0 - hp_ratio / 0.30).clamp(0.0, 1.0);
        fx.intensity += (target_urgency - fx.intensity) * (1.0 - (-8.0 * dt).exp());

        // Heartbeat accelerates with urgency: 72 bpm (1.2 Hz) -> 138 bpm (2.3 Hz)
        let bps = 1.2 + fx.intensity * 1.1;
        fx.heartbeat_timer = (fx.heartbeat_timer + dt * bps).fract();

        // Realistic double-thump "lub-dub" heartbeat pulse
        let t = fx.heartbeat_timer;
        let p1 = (-((t - 0.07) / 0.05).powi(2)).exp();
        let p2 = 0.65 * (-((t - 0.23) / 0.055).powi(2)).exp();
        fx.pulse = (p1 + p2).clamp(0.0, 1.0);
    } else {
        fx.intensity = (fx.intensity - 3.0 * dt).max(0.0);
        fx.pulse = (fx.pulse - 5.0 * dt).max(0.0);
    }

    for mut node in &mut vignette {
        if fx.intensity > 0.001 {
            let pulse_boost = fx.pulse * 0.32 * fx.intensity;
            let alpha = (fx.intensity * 0.42 + pulse_boost).clamp(0.0, 0.88);
            // Saturated deep arterial blood vignette
            node.color = Color::srgba(0.82, 0.02, 0.03, alpha);
        } else {
            node.color = Color::NONE;
        }
    }
}

/// Explosions, slams, zombie hit reactions, and brute/boss stomps.
fn shake_sources(
    queue: Res<FxQueue>,
    mut shake: ResMut<Shake>,
    player: Option<Single<&LocalPlayer>>,
    brutes: Query<(Entity, &Replicated, &Transform, &Rig)>,
    roster: Res<crate::Roster>,
    session: Res<crate::Session>,
    mut last_hp: Local<f32>,
    mut swings: Local<HashMap<Entity, f32>>,
) {
    let Some(player) = player else { return };
    let eye = player.eye_pos();

    // Smooth quadratic hermite falloff (smoothstep): tight punch near epicenter, natural rolloff
    let near = |pos: [f32; 3], reach: f32| {
        let dist = Vec3::from_array(pos).distance(eye);
        if dist >= reach {
            return 0.0;
        }
        let norm = (1.0 - dist / reach).clamp(0.0, 1.0);
        norm * norm * (3.0 - 2.0 * norm)
    };

    // Queued FX
    for fx in &queue.0 {
        match *fx {
            Fx::Explosion { pos, radius, .. } => {
                let size = (radius / 5.0).min(1.0);
                shake.add(0.85 * size * near(pos, radius * 3.5 + 10.0));
            }
            Fx::Slam { pos, radius } => shake.add(0.95 * near(pos, radius * 3.5 + 10.0)),
            Fx::Blood { pos, headshot: true, .. } => {
                shake.add(0.10 * near(pos, 8.0));
            }
            Fx::Decapitation { pos } => {
                shake.add(0.16 * near(pos, 10.0));
            }
            _ => {}
        }
    }

    // Direct zombie damage hits to the local player
    if let Some(me) = roster.me(&session) {
        if *last_hp > 0.0 && me.health < *last_hp - 0.5 && me.alive {
            let damage = *last_hp - me.health;
            let hit_shake = (damage / 32.0).clamp(0.15, 0.45);
            shake.add(hit_shake);
        }
        *last_hp = me.health;
    }

    // Brutes & Bosses: slam windups
    swings.retain(|e, _| brutes.contains(*e));
    for (e, r, tf, rig) in &brutes {
        let is_boss = r.kind.is_boss();
        let big = if is_boss { 1.8 } else { 1.0 };
        if r.kind.slams() {
            let windup = crate::sim::slam_spec(r.kind).0;
            let before = swings.insert(e, rig.swing).unwrap_or(9.0);
            if before < windup && rig.swing >= windup && rig.dying.is_none() {
                shake.add(0.65 * big * near(tf.translation.to_array(), 9.0 * big));
            }
        }
    }
}

/// Applies landing dip spring compression, translational shake, and rotational micro-tremors.
fn apply_camera_feel(
    time: Res<Time>,
    settings: Res<Settings>,
    mut shake: ResMut<Shake>,
    mut dip: ResMut<LandingDip>,
    camera: Option<Single<(&mut Transform, &LocalPlayer)>>,
) {
    let dt = time.delta_secs();
    shake.0 = (shake.0 - SHAKE_FADE * dt).max(0.0);

    let (Some(cam), true) = (camera, settings.camera_shake) else {
        return;
    };
    let (mut tf, p) = cam.into_inner();

    // 1. Landing Dip / Spring Compression
    if !dip.was_grounded && p.on_ground {
        // Player just landed. Trigger dip impulse proportional to fall air time
        if p.last_air > 0.16 {
            let impulse = (p.last_air * 8.5).clamp(1.0, 5.0);
            dip.velocity -= impulse * 0.055;
        }
    }
    dip.was_grounded = p.on_ground;

    // Damped harmonic oscillator (snappy retro-tactical feel)
    const SPRING_STIFF: f32 = 175.0;
    const SPRING_DAMP: f32 = 19.5;
    let accel = -SPRING_STIFF * dip.offset_y - SPRING_DAMP * dip.velocity;
    dip.velocity += accel * dt;
    dip.offset_y += dip.velocity * dt;
    dip.pitch_offset = (dip.offset_y * 0.28).clamp(-0.06, 0.03);

    tf.translation.y += dip.offset_y;
    tf.rotation *= Quat::from_rotation_x(dip.pitch_offset);

    // 2. Camera Shake: multi-sine translation + rotational micro-tremors (pitch + roll)
    if shake.0 > 0.0 {
        let t = time.elapsed_secs();
        let s = shake.0 * shake.0;

        // Smooth multi-frequency translation
        let wobble = Vec3::new(
            (t * 24.0).sin() + 0.5 * (t * 43.0 + 1.3).sin(),
            (t * 20.0 + 0.7).sin() + 0.5 * (t * 38.0 + 2.1).sin(),
            (t * 31.0 + 2.9).sin() * 0.5,
        ) / 1.5;
        tf.translation += wobble * s * SHAKE_MAX;

        // Visceral rotational micro-tremors
        let roll = ((t * 33.0).sin() * 0.016 + (t * 51.0).cos() * 0.008) * s;
        let pitch = ((t * 27.0 + 0.8).cos() * 0.014) * s;
        tf.rotation *= Quat::from_euler(EulerRot::YXZ, 0.0, pitch, roll);
    }
}

/// A speck in the air, drifting.
#[derive(Component)]
struct Mote {
    /// Where it is relative to the camera's box.
    pos: Vec3,
    vel: Vec3,
    phase: f32,
    kind: MoteKind,
    pulse_speed: f32,
    base_scale: f32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MoteKind {
    Pollen,
    Firefly,
    Ash,
    Ember,
    SeaMist,
}

/// The box of air round the camera the specks fill (they wrap round it).
const MOTE_BOX: f32 = 22.0;

fn spawn_motes(
    mut commands: Commands,
    settings: Res<Settings>,
    state: Res<MatchState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if !settings.particles {
        return;
    }
    let map = state.map as usize;
    let night = state.night;
    let mut rng = rand::thread_rng();

    // Map 0: Shipping Yard (Sea Mist)
    // Map 1: Central Park (Day: Pollen, Night: Fireflies)
    // Map 2: The Neighborhood (Ash flakes + Burning embers)
    match (map, night) {
        // Central Park - Night: Bioluminescent Fireflies
        (1, true) => {
            let mesh = meshes.add(Sphere::new(0.024).mesh().ico(1).unwrap());
            let material = materials.add(StandardMaterial {
                base_color: Color::LinearRgba(LinearRgba::rgb(1.3, 1.9, 0.35)),
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                ..default()
            });
            for _ in 0..90 {
                let pos = Vec3::new(
                    rng.gen_range(0.0..MOTE_BOX),
                    rng.gen_range(0.5..MOTE_BOX * 0.45),
                    rng.gen_range(0.0..MOTE_BOX),
                );
                let vel = Vec3::new(
                    rng.gen_range(-0.12..0.12),
                    rng.gen_range(-0.06..0.08),
                    rng.gen_range(-0.12..0.12),
                );
                commands.spawn((
                    InGameEntity,
                    Mote {
                        pos,
                        vel,
                        phase: rng.gen_range(0.0..std::f32::consts::TAU),
                        kind: MoteKind::Firefly,
                        pulse_speed: rng.gen_range(2.0..3.8),
                        base_scale: rng.gen_range(0.8..1.25),
                    },
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(material.clone()),
                    Transform::from_translation(pos),
                    NotShadowCaster,
                    NotShadowReceiver,
                ));
            }
        }
        // Central Park - Day: Golden Pollen & Plant Spores
        (1, false) => {
            let mesh = meshes.add(Sphere::new(0.016).mesh().ico(1).unwrap());
            let material = materials.add(StandardMaterial {
                base_color: Color::srgba(1.0, 0.95, 0.70, 0.50),
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                ..default()
            });
            let wind = Vec3::new(0.22, 0.03, 0.12);
            for _ in 0..150 {
                let pos = Vec3::new(
                    rng.gen_range(0.0..MOTE_BOX),
                    rng.gen_range(0.0..MOTE_BOX * 0.42),
                    rng.gen_range(0.0..MOTE_BOX),
                );
                let vel = wind * rng.gen_range(0.7..1.3)
                    + Vec3::new(
                        rng.gen_range(-0.04..0.04),
                        rng.gen_range(-0.02..0.03),
                        rng.gen_range(-0.04..0.04),
                    );
                commands.spawn((
                    InGameEntity,
                    Mote {
                        pos,
                        vel,
                        phase: rng.gen_range(0.0..std::f32::consts::TAU),
                        kind: MoteKind::Pollen,
                        pulse_speed: 1.0,
                        base_scale: rng.gen_range(0.7..1.2),
                    },
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(material.clone()),
                    Transform::from_translation(pos),
                    NotShadowCaster,
                    NotShadowReceiver,
                ));
            }
        }
        // The Neighborhood: Floating Ash flakes + Burning glowing Embers
        (2, _) => {
            // Dark Ash flakes
            let ash_mesh = meshes.add(Sphere::new(0.018).mesh().ico(1).unwrap());
            let ash_mat = materials.add(StandardMaterial {
                base_color: Color::srgba(0.55, 0.52, 0.50, 0.48),
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                ..default()
            });
            for _ in 0..95 {
                let pos = Vec3::new(
                    rng.gen_range(0.0..MOTE_BOX),
                    rng.gen_range(0.0..MOTE_BOX * 0.42),
                    rng.gen_range(0.0..MOTE_BOX),
                );
                let vel = Vec3::new(
                    rng.gen_range(0.08..0.22),
                    rng.gen_range(-0.16..-0.06), // ash falls gently
                    rng.gen_range(-0.05..0.08),
                );
                commands.spawn((
                    InGameEntity,
                    Mote {
                        pos,
                        vel,
                        phase: rng.gen_range(0.0..std::f32::consts::TAU),
                        kind: MoteKind::Ash,
                        pulse_speed: rng.gen_range(1.2..2.5),
                        base_scale: rng.gen_range(0.8..1.3),
                    },
                    Mesh3d(ash_mesh.clone()),
                    MeshMaterial3d(ash_mat.clone()),
                    Transform::from_translation(pos),
                    NotShadowCaster,
                    NotShadowReceiver,
                ));
            }
            // Burning Glowing Embers (emissive bloom)
            let ember_mesh = meshes.add(Sphere::new(0.020).mesh().ico(1).unwrap());
            let ember_mat = materials.add(StandardMaterial {
                base_color: Color::LinearRgba(LinearRgba::rgb(3.2, 0.95, 0.15)),
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                ..default()
            });
            for _ in 0..45 {
                let pos = Vec3::new(
                    rng.gen_range(0.0..MOTE_BOX),
                    rng.gen_range(0.0..MOTE_BOX * 0.38),
                    rng.gen_range(0.0..MOTE_BOX),
                );
                let vel = Vec3::new(
                    rng.gen_range(0.15..0.30),
                    rng.gen_range(0.04..0.15), // thermal updraft
                    rng.gen_range(-0.05..0.10),
                );
                commands.spawn((
                    InGameEntity,
                    Mote {
                        pos,
                        vel,
                        phase: rng.gen_range(0.0..std::f32::consts::TAU),
                        kind: MoteKind::Ember,
                        pulse_speed: rng.gen_range(3.0..6.0),
                        base_scale: rng.gen_range(0.7..1.2),
                    },
                    Mesh3d(ember_mesh.clone()),
                    MeshMaterial3d(ember_mat.clone()),
                    Transform::from_translation(pos),
                    NotShadowCaster,
                    NotShadowReceiver,
                ));
            }
        }
        // Shipping Yard (or default): Maritime Sea Mist & Windblown Spray
        _ => {
            let mesh = meshes.add(Sphere::new(0.030).mesh().ico(1).unwrap());
            let material = materials.add(StandardMaterial {
                base_color: Color::srgba(0.80, 0.86, 0.92, 0.32),
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                ..default()
            });
            let wind = Vec3::new(0.48, -0.01, 0.22);
            for _ in 0..130 {
                let pos = Vec3::new(
                    rng.gen_range(0.0..MOTE_BOX),
                    rng.gen_range(0.0..MOTE_BOX * 0.40),
                    rng.gen_range(0.0..MOTE_BOX),
                );
                let vel = wind * rng.gen_range(0.75..1.25)
                    + Vec3::new(
                        rng.gen_range(-0.06..0.06),
                        rng.gen_range(-0.02..0.02),
                        rng.gen_range(-0.06..0.06),
                    );
                commands.spawn((
                    InGameEntity,
                    Mote {
                        pos,
                        vel,
                        phase: rng.gen_range(0.0..std::f32::consts::TAU),
                        kind: MoteKind::SeaMist,
                        pulse_speed: 1.0,
                        base_scale: rng.gen_range(0.8..1.4),
                    },
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(material.clone()),
                    Transform::from_translation(pos),
                    NotShadowCaster,
                    NotShadowReceiver,
                ));
            }
        }
    }
}

fn drift(
    time: Res<Time>,
    settings: Res<Settings>,
    camera: Option<Single<&Transform, (With<LocalPlayer>, Without<Mote>)>>,
    mut motes: Query<(&mut Mote, &mut Transform, &mut Visibility)>,
) {
    let Some(cam) = camera else { return };
    let dt = time.delta_secs();
    let t = time.elapsed_secs();
    let size = Vec3::new(MOTE_BOX, MOTE_BOX * 0.42, MOTE_BOX);
    let corner = cam.translation - Vec3::new(MOTE_BOX * 0.5, MOTE_BOX * 0.21 + 1.0, MOTE_BOX * 0.5);
    let show = if settings.particles {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };

    for (mut m, mut tf, mut vis) in &mut motes {
        if *vis != show {
            *vis = show;
        }

        // Map-specific dynamic wobble / bio-flight
        let wobble = match m.kind {
            MoteKind::Firefly => {
                // Organic meandering darting flight in arcs
                Vec3::new(
                    (t * 1.4 + m.phase).sin() * 0.22,
                    (t * 1.8 + m.phase * 1.5).cos() * 0.15,
                    (t * 1.2 + m.phase * 2.1).sin() * 0.22,
                )
            }
            MoteKind::Ember => {
                // Flickering buoyant thermal updrafts
                Vec3::new(
                    (t * 2.5 + m.phase).sin() * 0.12,
                    (t * 3.0 + m.phase * 1.7).cos() * 0.08,
                    (t * 2.2 + m.phase * 2.3).sin() * 0.10,
                )
            }
            MoteKind::Ash => {
                // Fluttering falling leaf tumble
                Vec3::new(
                    (t * 1.1 + m.phase).sin() * 0.10,
                    (t * 0.8 + m.phase * 1.2).cos() * 0.04,
                    (t * 0.9 + m.phase * 1.8).cos() * 0.08,
                )
            }
            _ => {
                // Smooth atmospheric wind turbulence
                Vec3::new(
                    (t * 0.7 + m.phase).sin() * 0.08,
                    (t * 0.9 + m.phase * 1.7).sin() * 0.05,
                    (t * 0.6 + m.phase * 2.3).cos() * 0.08,
                )
            }
        };

        let step = (m.vel + wobble) * dt;
        m.pos += step;

        // Wrap around moving box centered on camera
        let rel = (m.pos - corner).rem_euclid(size);
        m.pos = corner + rel;
        tf.translation = m.pos;

        // Dynamic scale & combat readability:
        let cam_dist = m.pos.distance(cam.translation);
        // Fade/shrink motes closer than 2.0m to prevent obscuring view
        let prox_fade = ((cam_dist - 0.5) / 1.8).clamp(0.0, 1.0);

        // Blinking bioluminescence for fireflies & flickering for embers
        let pulse_factor = match m.kind {
            MoteKind::Firefly => {
                ((t * m.pulse_speed + m.phase).sin() * 2.2 - 0.6).clamp(0.1, 1.2)
            }
            MoteKind::Ember => {
                0.8 + 0.3 * (t * m.pulse_speed + m.phase).sin()
            }
            _ => 1.0,
        };

        tf.scale = Vec3::splat(m.base_scale * prox_fade * pulse_factor);
    }
}

/// Counts down a few frames into the match before warming up, so the
/// camera is in place.
#[derive(Resource)]
struct WarmUp(u8);

fn start_warm_up(mut commands: Commands) {
    commands.insert_resource(WarmUp(3));
}

/// Sets off one of every effect far away below the horizon (out of sight and
/// earshot) so their shaders are ready before they're needed.
fn warm_up(
    mut commands: Commands,
    warm: Option<ResMut<WarmUp>>,
    mut queue: ResMut<FxQueue>,
    camera: Option<Single<&Transform, With<LocalPlayer>>>,
) {
    let (Some(mut warm), Some(cam)) = (warm, camera) else {
        return;
    };
    if warm.0 > 0 {
        warm.0 -= 1;
        return;
    }
    commands.remove_resource::<WarmUp>();
    let flat = cam.forward().with_y(0.0).normalize_or(Vec3::NEG_Z);
    let at = (cam.translation + flat * 400.0 - Vec3::Y * 70.0).to_array();
    let dir = flat.to_array();
    let fx = &mut queue.0;
    fx.extend([
        Fx::Explosion {
            pos: at,
            radius: 4.0,
            color: [1.0, 0.5, 0.2],
        },
        Fx::Ring {
            pos: at,
            radius: 4.0,
            color: [0.5, 0.8, 1.0],
        },
        Fx::Slam {
            pos: at,
            radius: 4.0,
        },
        Fx::Heal {
            pos: at,
            radius: 4.0,
        },
        Fx::Slash {
            pos: at,
            dir,
            radius: 3.0,
        },
        Fx::Blood {
            pos: at,
            dir,
            headshot: false,
        },
        Fx::Blood {
            pos: at,
            dir,
            headshot: true,
        },
        Fx::Decapitation {
            pos: at,
        },
    ]);
    for ability in crate::data::Ability::ALL {
        fx.push(Fx::Spell {
            ability,
            pos: at,
            dir,
            size: 4.0,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hit_stop_trigger_and_countdown() {
        let mut hit_stop = HitStop::default();
        assert_eq!(hit_stop.timer, 0.0);
        assert_eq!(hit_stop.time_scale, 0.0);
        assert!(!hit_stop.is_frozen());

        // Update when not active should maintain time_scale = 1.0
        hit_stop.update(0.016);
        assert_eq!(hit_stop.timer, 0.0);
        assert_eq!(hit_stop.time_scale, 1.0);
        assert!(!hit_stop.is_frozen());

        // Trigger hit stop
        hit_stop.trigger(0.1);
        assert_eq!(hit_stop.timer, 0.1);
        assert_eq!(hit_stop.time_scale, 0.0);
        assert!(hit_stop.is_frozen());

        // Partial update: countdown continues and still frozen
        hit_stop.update(0.04);
        assert!((hit_stop.timer - 0.06).abs() < 1e-5);
        assert_eq!(hit_stop.time_scale, 0.0);
        assert!(hit_stop.is_frozen());

        // Trigger with larger duration extends, smaller does not shorten
        hit_stop.trigger(0.02);
        assert!((hit_stop.timer - 0.06).abs() < 1e-5);
        hit_stop.trigger(0.12);
        assert_eq!(hit_stop.timer, 0.12);

        // Advance to zero
        hit_stop.update(0.12);
        assert_eq!(hit_stop.timer, 0.0);
        assert_eq!(hit_stop.time_scale, 1.0);
        assert!(!hit_stop.is_frozen());

        // Overshoot dt clamped to 0.0
        hit_stop.trigger(0.05);
        hit_stop.update(0.10);
        assert_eq!(hit_stop.timer, 0.0);
        assert_eq!(hit_stop.time_scale, 1.0);
        assert!(!hit_stop.is_frozen());
    }
}
