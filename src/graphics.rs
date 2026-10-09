//! Applies the graphics settings (shadows, smoothed edges, glow and soft
//! corner shading) to the camera and sun, and the look of the picture:
//! film-like tone mapping, colour grading per map, the gradient sky and
//! distance haze. See docs/art-direction.md for the numbers.

use bevy::asset::RenderAssetUsages;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::{
    CascadeShadowConfig, CascadeShadowConfigBuilder, DirectionalLightShadowMap, NotShadowCaster,
    NotShadowReceiver,
};
use bevy::pbr::{DistanceFog, FogFalloff, ScreenSpaceAmbientOcclusion};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::view::{ColorGrading, Msaa};

use crate::config::Settings;
use crate::{AppState, MatchState};

pub struct GraphicsPlugin;

impl Plugin for GraphicsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(DirectionalLightShadowMap { size: 2048 })
            .add_systems(
                Update,
                (apply_camera, apply_sun, update_camera_mood, update_flicker_lights, update_ocean_waves),
            )
            .add_systems(
                PostUpdate,
                follow_sky.before(bevy::transform::TransformSystems::Propagate),
            );
    }
}

/// Dynamic 3D undulating ocean waves component.
#[derive(Component)]
pub struct OceanWaves {
    pub base_y: f32,
}

pub fn update_ocean_waves(
    time: Res<Time>,
    mut meshes: ResMut<Assets<Mesh>>,
    query: Query<(&Mesh3d, &OceanWaves)>,
) {
    let t = time.elapsed_secs();
    for (mesh_handle, waves) in &query {
        let Some(mut mesh) = meshes.get_mut(&mesh_handle.0) else { continue };
        let Some(bevy::render::mesh::VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
        else {
            continue;
        };

        for pos in positions.iter_mut() {
            let x = pos[0];
            let z = pos[2];
            // Primary rolling swell rolling into the bay from south (+z direction in local coords)
            let w1 = (z * 0.14 + x * 0.04 - t * 2.2).sin() * 0.16;
            // Secondary cross swell
            let w2 = (z * 0.28 - x * 0.12 - t * 3.4).sin() * 0.08;
            // High frequency surface chop
            let w3 = (x * 0.45 + z * 0.35 + t * 4.8).cos() * 0.035;
            pos[1] = waves.base_y + w1 + w2 + w3;
        }
    }
}

/// Organic micro-pulsing luminance flicker for street lamps, lanterns and barrel fires.
#[derive(Component)]
pub struct FlickerLight {
    pub base: f32,
    pub speed: f32,
    pub amplitude: f32,
    pub phase: f32,
}

pub fn update_flicker_lights(
    time: Res<Time>,
    mut lights: Query<(&mut PointLight, &FlickerLight)>,
    mut timer: Local<f32>,
) {
    *timer += time.delta_secs();
    if *timer < 0.033 {
        return;
    }
    *timer = 0.0;
    let t = time.elapsed_secs();
    for (mut light, f) in &mut lights {
        let wave = (t * f.speed + f.phase).sin() * 0.52
            + (t * f.speed * 1.63 + f.phase * 1.3).sin() * 0.31
            + (t * f.speed * 2.71 + f.phase * 0.7).sin() * 0.17;
        light.intensity = (f.base * (1.0 + wave * f.amplitude)).max(0.0);
    }
}

/// The weak, shadowless cool light from opposite the sun that keeps
/// shadows from going black.
#[derive(Component)]
pub struct FillLight;

/// The sky dome; it stays centred on the camera.
#[derive(Component)]
struct SkyDome;

/// Horizon and zenith colours for a map's sky colour. The horizon is lighter
/// and warmer, the zenith deeper and bluer. The fog uses the horizon.
pub fn sky_colors(sky: Color, night: bool) -> (Color, Color) {
    let s = sky.to_srgba();
    let mix = |a: Srgba, b: Srgba, t: f32| {
        Srgba::rgb(
            a.red + (b.red - a.red) * t,
            a.green + (b.green - a.green) * t,
            a.blue + (b.blue - a.blue) * t,
        )
    };
    if night {
        let h = mix(s, Srgba::rgb(0.09, 0.1, 0.14), 0.5);
        let z = mix(s, Srgba::rgb(0.0, 0.0, 0.02), 0.6);
        return (h.into(), z.into());
    }
    let h = mix(s, Srgba::rgb(1.0, 0.95, 0.86), 0.2);
    let z = mix(s, Srgba::rgb(0.2, 0.34, 0.66), 0.35) * 0.85;
    (h.into(), Srgba::rgb(z.red, z.green, z.blue).into())
}

/// Saturation after tone mapping. AgX already eases bright colours
/// towards white, so this is a little above the 0.85 first planned.
const SATURATION: f32 = 0.95;

/// How each map is graded: (white balance, saturation). The Neighborhood
/// leans warm, the Shipping Yard cool.
fn map_grade(map: u8, night: bool) -> (f32, f32) {
    // Bevy's white balance is strong: 0.1 is already a heavy tint.
    let (warmth, sat) = match map {
        0 => (-0.015, SATURATION),
        1 => (0.005, SATURATION),
        3 => (0.022, 1.05), // rich tropical golden hour warmth & vivid ocean saturation
        _ => (0.025, SATURATION),
    };
    if night {
        (warmth * 0.5 - 0.01, sat)
    } else {
        (warmth, sat)
    }
}

/// The sky: a big dome shaded from horizon to zenith, with a sun (or moon)
/// disc. Unlit and outside the fog.
pub fn spawn_sky(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    sky: Color,
    night: bool,
    sun_dir: Vec3,
) {
    const R: f32 = 600.0;
    let (horizon, zenith) = sky_colors(sky, night);
    let (h, z) = (horizon.to_linear(), zenith.to_linear());
    let (rings, segs) = (24usize, 32usize);
    let mut pos = Vec::new();
    let mut col = Vec::new();
    for i in 0..=rings {
        // From straight down to straight up.
        let el = -std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * i as f32 / rings as f32;
        let up = el.sin();
        // Most of the change happens in the lower third of the sky.
        let t = (up.max(0.0) / 0.6).min(1.0).powf(0.8);
        let below = (-up).max(0.0).min(1.0);
        for j in 0..=segs {
            let a = std::f32::consts::TAU * j as f32 / segs as f32;
            pos.push([el.cos() * a.cos() * R, up * R, el.cos() * a.sin() * R]);
            let c = h.mix(&z, t) * (1.0 - 0.25 * below);
            col.push([c.red, c.green, c.blue, 1.0]);
        }
    }
    let mut idx = Vec::new();
    for i in 0..rings as u32 {
        for j in 0..segs as u32 {
            let a = i * (segs as u32 + 1) + j;
            let b = a + segs as u32 + 1;
            idx.extend([a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    let n = pos.len();
    let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; n])
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
        .with_inserted_indices(Indices::U32(idx));
    let dome = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        unlit: true,
        fog_enabled: false,
        cull_mode: None,
        ..default()
    });
    // The sun is bright enough for the bloom to give it a soft halo.
    let (disc, glow) = if night {
        (8.0, LinearRgba::rgb(1.4, 1.5, 1.7))
    } else {
        (14.0, LinearRgba::rgb(9.0, 8.2, 6.8))
    };
    let disc_mat = materials.add(StandardMaterial {
        base_color: Color::LinearRgba(glow),
        unlit: true,
        fog_enabled: false,
        ..default()
    });
    commands
        .spawn((
            crate::InGameEntity,
            SkyDome,
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(dome),
            NotShadowCaster,
            NotShadowReceiver,
            Transform::default(),
            Visibility::default(),
        ))
        .with_child((
            Mesh3d(meshes.add(Sphere::new(disc).mesh().ico(3).unwrap())),
            MeshMaterial3d(disc_mat),
            NotShadowCaster,
            NotShadowReceiver,
            Transform::from_translation(sun_dir.normalize() * (R - 40.0)),
            Visibility::default(),
        ));
}

fn follow_sky(
    cam: Query<&GlobalTransform, With<Camera3d>>,
    mut sky: Query<&mut Transform, With<SkyDome>>,
) {
    let Some(cam) = cam.iter().next() else { return };
    for mut tf in &mut sky {
        tf.translation = cam.translation();
    }
}

fn apply_camera(
    mut commands: Commands,
    settings: Res<Settings>,
    app_state: Res<State<AppState>>,
    state: Res<MatchState>,
    clear: Res<ClearColor>,
    cams: Query<(Entity, Ref<Camera3d>)>,
) {
    let changed = settings.is_changed() || app_state.is_changed() || clear.is_changed();
    for (e, added) in &cams {
        if !changed && !added.is_added() {
            continue;
        }
        let mut ec = commands.entity(e);
        // Soft corner shading needs the edge smoothing off (the settings
        // screen keeps them exclusive).
        let ao = settings.ambient_occlusion;
        ec.insert(if settings.antialias && !ao { Msaa::Sample4 } else { Msaa::Off });
        if ao {
            ec.insert(ScreenSpaceAmbientOcclusion::default());
        } else {
            ec.remove::<ScreenSpaceAmbientOcclusion>();
        }
        // Always HDR, rolled off to the screen like film.
        ec.insert(bevy::camera::Hdr);
        ec.insert(Tonemapping::AgX);
        let (temperature, saturation) = if *app_state.get() == AppState::InGame {
            map_grade(state.map, state.night)
        } else {
            (0.0, SATURATION)
        };
        let mut grade = ColorGrading::default();
        grade.global.temperature = temperature;
        grade.global.post_saturation = saturation;
        grade.midtones.contrast = 1.06;
        ec.insert(grade);
        if settings.bloom {
            ec.insert(Bloom {
                intensity: 0.1,
                ..Bloom::NATURAL
            });
        } else {
            ec.remove::<Bloom>();
        }
        // Atmospheric distance fog tailored per map and time of day,
        // with natural exponential-squared falloff and directional light scattering.
        if *app_state.get() == AppState::InGame {
            ec.insert(map_fog(state.map, state.night, clear.0));
        } else {
            ec.remove::<DistanceFog>();
        }
    }
}

/// Atmospheric distance fog specifications per map and night condition.
pub fn map_fog(map: u8, night: bool, clear_color: Color) -> DistanceFog {
    match (map, night) {
        // Shipping Yard (Map 0): Maritime sea mist & dockside humidity
        (0, false) => DistanceFog {
            color: Color::srgb(0.70, 0.75, 0.82),
            directional_light_color: Color::srgba(1.0, 0.98, 0.92, 0.5),
            directional_light_exponent: 18.0,
            falloff: FogFalloff::from_visibility_squared(85.0),
        },
        (0, true) => DistanceFog {
            color: Color::srgb(0.02, 0.035, 0.06),
            directional_light_color: Color::srgba(0.2, 0.32, 0.5, 0.7),
            directional_light_exponent: 12.0,
            falloff: FogFalloff::from_visibility_squared(44.0),
        },
        // Central Park (Map 1): Canopy haze by day, dense cemetery mist by night
        (1, false) => DistanceFog {
            color: Color::srgb(0.76, 0.75, 0.68),
            directional_light_color: Color::srgba(1.1, 1.02, 0.84, 0.55),
            directional_light_exponent: 14.0,
            falloff: FogFalloff::from_visibility_squared(105.0),
        },
        (1, true) => DistanceFog {
            color: Color::srgb(0.022, 0.028, 0.05),
            directional_light_color: Color::srgba(0.22, 0.32, 0.48, 0.75),
            directional_light_exponent: 10.0,
            falloff: FogFalloff::from_visibility_squared(38.0),
        },
        // The Neighborhood (Map 2): Apocalyptic ash, smoke, and claustrophobic dread
        (2, false) => DistanceFog {
            color: Color::srgb(0.74, 0.70, 0.64),
            directional_light_color: Color::srgba(1.05, 0.85, 0.62, 0.6),
            directional_light_exponent: 20.0,
            falloff: FogFalloff::from_visibility_squared(68.0),
        },
        (2, true) => DistanceFog {
            color: Color::srgb(0.016, 0.018, 0.03),
            directional_light_color: Color::srgba(0.14, 0.16, 0.24, 0.65),
            directional_light_exponent: 16.0,
            falloff: FogFalloff::from_visibility_squared(32.0),
        },
        // Tidewater (Map 3): Crystal-clear Caribbean tropical atmosphere, long ocean horizon visibility
        (3, false) => DistanceFog {
            color: Color::srgb(0.55, 0.78, 0.92),
            directional_light_color: Color::srgba(1.0, 0.96, 0.88, 0.25),
            directional_light_exponent: 14.0,
            falloff: FogFalloff::Linear {
                start: 220.0,
                end: 650.0,
            },
        },
        (3, true) => DistanceFog {
            color: Color::srgb(0.012, 0.02, 0.045),
            directional_light_color: Color::srgba(0.25, 0.38, 0.55, 0.6),
            directional_light_exponent: 12.0,
            falloff: FogFalloff::Linear {
                start: 120.0,
                end: 380.0,
            },
        },
        _ => DistanceFog {
            color: clear_color,
            directional_light_color: Color::NONE,
            directional_light_exponent: 8.0,
            falloff: FogFalloff::from_visibility_squared(if night { 40.0 } else { 90.0 }),
        },
    }
}

/// Dynamically adjusts camera color grading mood based on player health state (< 30% HP).
fn update_camera_mood(
    app_state: Res<State<AppState>>,
    state: Res<MatchState>,
    low_health: Option<Res<crate::feel::LowHealthFx>>,
    mut grades: Query<&mut ColorGrading, With<Camera3d>>,
) {
    if *app_state.get() != AppState::InGame {
        return;
    }
    let (base_temp, base_sat) = map_grade(state.map, state.night);
    let intensity = low_health.as_ref().map_or(0.0, |h| h.intensity);

    for mut grade in &mut grades {
        // Desaturate progressively as health drops below 30% (tunnel vision shock)
        grade.global.post_saturation = base_sat * (1.0 - 0.55 * intensity);
        // Cool down temperature (cold extremities dread)
        grade.global.temperature = base_temp - 0.02 * intensity;
        // Increase midtone contrast (stark claustrophobic shadows)
        grade.midtones.contrast = 1.06 + 0.18 * intensity;
        // Drain shadow saturation
        grade.shadows.saturation = (1.0 - 0.75 * intensity).max(0.1);
    }
}

fn apply_sun(
    settings: Res<Settings>,
    mut map: ResMut<DirectionalLightShadowMap>,
    mut suns: Query<(&mut DirectionalLight, &mut CascadeShadowConfig), Without<FillLight>>,
) {
    let size = if settings.shadows >= 2 { 4096 } else { 2048 };
    if map.size != size {
        map.size = size;
    }
    for (mut sun, mut cascades) in &mut suns {
        if !settings.is_changed() && !sun.is_added() {
            continue;
        }
        sun.shadow_maps_enabled = settings.shadows > 0;
        *cascades = if settings.shadows >= 2 {
            CascadeShadowConfigBuilder {
                num_cascades: 4,
                first_cascade_far_bound: 8.0,
                maximum_distance: 120.0,
                ..default()
            }
        } else {
            CascadeShadowConfigBuilder {
                num_cascades: 2,
                first_cascade_far_bound: 15.0,
                maximum_distance: 60.0,
                ..default()
            }
        }
        .build();
    }
}
