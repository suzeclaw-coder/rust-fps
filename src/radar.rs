//! GTA V style radar, minimap projection, blip clamping, GPS route navigation,
//! and HUD ring meters powered by `vhud-rs`.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use std::f32::consts::FRAC_PI_2;

use vhud_rs::{
    AltitudeRelation, Blip, BlipColor, BlipManager, BlipType, GpsRoute, HudRingSystem,
    RadarConfig, RadarProjection, RenderedArcMeter, RouteNavigator, VehicleState,
};

use crate::maps::CurrentMap;
use crate::pings::Ping;
use crate::player::LocalPlayer;
use crate::{AppState, Enemy, InGameEntity, MatchState, Phase, Replicated, Roster, Session};

pub const RADAR_SIZE: f32 = 192.0;
pub const RADAR_RADIUS: f32 = 72.0;
pub const RADAR_CENTER: Vec2 = Vec2::new(96.0, 96.0);
pub const TEX_SIZE: usize = 256;

const MAX_BLIP_SLOTS: usize = 64;
const MAX_GPS_SEGMENTS: usize = 16;

const BLIP_BOX_ID: u64 = 9_000_000;
const BLIP_EXTRACT_ID: u64 = 9_000_001;
const BLIP_PERK_BASE: u64 = 9_100_000;
const BLIP_UPGRADE_BASE: u64 = 9_200_000;
const BLIP_PING_BASE: u64 = 9_300_000;
const BLIP_TEAM_BASE: u64 = 9_400_000;

pub struct RadarPlugin;

impl Plugin for RadarPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn_radar)
            .add_systems(
                Update,
                update_radar
                    .in_set(Phase::Present)
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

/// Persistent engine state for the vhud-rs radar projection and meters.
#[derive(Resource)]
pub struct RadarState {
    pub projection: RadarProjection,
    pub ring_system: HudRingSystem,
    pub route_navigator: RouteNavigator,
    pub blip_manager: BlipManager,
    pub ring_texture: Handle<Image>,
    pub ring_image_data: Vec<u8>,
    pub last_health_pct: f32,
    pub last_ult_pct: f32,
}

#[derive(Component)]
struct RadarRoot;

#[derive(Component)]
struct RadarRingDisplay;

#[derive(Component)]
struct RadarNorthBadge;

#[derive(Component)]
struct RadarGpsLineSlot(usize);

#[derive(Component)]
struct RadarBlipSlot(usize);

#[derive(Component)]
struct RadarBlipIcon(usize);

fn spawn_radar(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
) {
    let bg_texture = images.add(create_radar_background_image());

    let ring_data = vec![0u8; TEX_SIZE * TEX_SIZE * 4];
    let ring_img = Image::new(
        Extent3d {
            width: TEX_SIZE as u32,
            height: TEX_SIZE as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        ring_data.clone(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    let ring_texture = images.add(ring_img);

    let config = RadarConfig::new_circular(RADAR_CENTER, RADAR_RADIUS);
    let mut projection = RadarProjection::new(config);
    projection.zoom.on_foot_zoom = 1.35;
    projection.zoom.vehicle_normal_zoom = 0.95;
    projection.zoom.set_immediate(1.35);

    let mut blip_manager = BlipManager::new();
    blip_manager.border_margin = 6.0;

    let ring_system = HudRingSystem::new_gta_v_circular(
        RADAR_RADIUS * (TEX_SIZE as f32 / RADAR_SIZE),
        11.0,
    );

    let radar_state = RadarState {
        projection,
        ring_system,
        route_navigator: RouteNavigator::new(),
        blip_manager,
        ring_texture: ring_texture.clone(),
        ring_image_data: ring_data,
        last_health_pct: 1.0,
        last_ult_pct: 0.0,
    };
    commands.insert_resource(radar_state);

    // Root radar container in the bottom-left corner
    commands
        .spawn((
            InGameEntity,
            RadarRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(20.0),
                bottom: Val::Px(20.0),
                width: Val::Px(RADAR_SIZE),
                height: Val::Px(RADAR_SIZE),
                overflow: Overflow::clip(),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|root| {
            // Layer 1: Radar circular background (grid, range rings, rim border)
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    width: Val::Px(RADAR_SIZE),
                    height: Val::Px(RADAR_SIZE),
                    ..default()
                },
                ImageNode::new(bg_texture),
            ));

            // Layer 2: GPS Route Segments Pool
            for i in 0..MAX_GPS_SEGMENTS {
                root.spawn((
                    RadarGpsLineSlot(i),
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(0.0),
                        top: Val::Px(0.0),
                        width: Val::Px(0.0),
                        height: Val::Px(3.0),
                        border_radius: BorderRadius::all(Val::Px(1.5)),
                        ..default()
                    },
                    Transform::default(),
                    BackgroundColor(Color::srgba(0.25, 0.75, 1.0, 0.85)),
                    Visibility::Hidden,
                ));
            }

            // Layer 3: Blip items pool
            for i in 0..MAX_BLIP_SLOTS {
                root.spawn((
                    RadarBlipSlot(i),
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(0.0),
                        top: Val::Px(0.0),
                        width: Val::Px(8.0),
                        height: Val::Px(8.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border_radius: BorderRadius::all(Val::Px(4.0)),
                        ..default()
                    },
                    Transform::default(),
                    BackgroundColor(Color::WHITE),
                    Visibility::Hidden,
                ))
                .with_children(|blip_node| {
                    blip_node.spawn((
                        RadarBlipIcon(i),
                        Text::new(""),
                        TextFont {
                            font_size: 9.0.into(),
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                });
            }

            // Layer 4: HUD Ring Meters (Health Arc & Special Ability Arc)
            root.spawn((
                RadarRingDisplay,
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    width: Val::Px(RADAR_SIZE),
                    height: Val::Px(RADAR_SIZE),
                    ..default()
                },
                ImageNode::new(ring_texture),
            ));

            // Layer 5: North Cardinal Compass Indicator ("N")
            root.spawn((
                RadarNorthBadge,
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(RADAR_CENTER.x - 7.0),
                    top: Val::Px(RADAR_CENTER.y - RADAR_RADIUS + 2.0),
                    width: Val::Px(14.0),
                    height: Val::Px(14.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border_radius: BorderRadius::all(Val::Px(7.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.06, 0.10, 0.16, 0.9)),
                BorderColor::all(Color::srgba(0.4, 0.7, 1.0, 0.8)),
            ))
            .with_children(|b| {
                b.spawn((
                    Text::new("N"),
                    TextFont {
                        font_size: 9.0.into(),
                        ..default()
                    },
                    TextColor(Color::srgb(0.9, 0.95, 1.0)),
                ));
            });

            // Layer 6: Player Arrow at exact center pointing forward (UP)
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(RADAR_CENTER.x - 6.0),
                    top: Val::Px(RADAR_CENTER.y - 7.0),
                    width: Val::Px(12.0),
                    height: Val::Px(14.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
            ))
            .with_children(|p| {
                p.spawn((
                    Text::new("▲"),
                    TextFont {
                        font_size: 13.0.into(),
                        ..default()
                    },
                    TextColor(Color::WHITE),
                ));
            });
        });
}

#[allow(clippy::too_many_arguments)]
fn update_radar(
    time: Res<Time>,
    session: Res<Session>,
    roster: Res<Roster>,
    state: Res<MatchState>,
    map: Option<Res<CurrentMap>>,
    player_q: Single<&LocalPlayer>,
    enemies: Query<(Entity, &Transform, Option<&Replicated>), With<Enemy>>,
    pings: Query<&Ping>,
    mut radar_state: ResMut<RadarState>,
    mut images: ResMut<Assets<Image>>,
    mut north_node: Single<&mut Node, (With<RadarNorthBadge>, Without<RadarBlipSlot>, Without<RadarGpsLineSlot>)>,
    mut blip_nodes: Query<
        (&RadarBlipSlot, &mut Node, &mut BackgroundColor, &mut Visibility),
        (Without<RadarNorthBadge>, Without<RadarGpsLineSlot>),
    >,
    mut blip_icons: Query<(&RadarBlipIcon, &mut Text, &mut TextColor)>,
    mut gps_lines: Query<
        (&RadarGpsLineSlot, &mut Node, &mut Transform, &mut BackgroundColor, &mut Visibility),
        (Without<RadarNorthBadge>, Without<RadarBlipSlot>, Without<Enemy>),
    >,
) {
    let player = *player_q;
    let dt = time.delta_secs();

    // Map rust-fps coordinates: X -> X, -Z -> Y, Y (elevation) -> Z
    let player_feet = player.feet;
    let player_pos_vhud = Vec3::new(player_feet.x, -player_feet.z, player_feet.y);
    let player_heading = player.yaw;

    // 1. Dynamic Zoom Update
    let speed_kmh = player.horizontal_speed() * 3.6;
    let vehicle_state = if speed_kmh > 12.0 {
        VehicleState::InVehicle { speed_kmh }
    } else {
        VehicleState::OnFoot
    };
    radar_state.projection.zoom.update(vehicle_state, dt);

    // 2. Collect World Blips
    radar_state.blip_manager.clear();

    // 2a. Enemies
    for (entity, tf, rep) in &enemies {
        let e_pos = tf.translation;
        let vhud_pos = Vec3::new(e_pos.x, -e_pos.z, e_pos.y);
        let id = entity.to_bits();

        let is_boss = rep.is_some_and(|r| r.kind.is_boss());
        let is_brute = rep.is_some_and(|r| matches!(r.kind, crate::NetKind::Brute));
        let is_shooter = rep.is_some_and(|r| matches!(r.kind, crate::NetKind::Shooter));

        let (color, priority, scale) = if is_boss {
            (BlipColor::new(1.0, 0.15, 0.15, 1.0), 30, 1.4)
        } else if is_brute {
            (BlipColor::new(0.95, 0.35, 0.15, 1.0), 12, 1.15)
        } else if is_shooter {
            (BlipColor::new(0.95, 0.20, 0.40, 1.0), 10, 1.0)
        } else {
            (BlipColor::new(0.92, 0.20, 0.20, 0.95), 5, 0.85)
        };

        let mut blip = Blip::new(id, vhud_pos, BlipType::Enemy)
            .with_color(color)
            .with_priority(priority)
            .with_scale(scale)
            .with_clamp(true)
            .with_flashing(is_boss);
        blip.altitude_threshold = 2.4;
        radar_state.blip_manager.add_or_update(blip);
    }

    // 2b. Map Points of Interest
    if let Some(map) = &map {
        // Active Mystery Box
        let box_idx = (state.box_spot as usize).min(map.0.box_spots.len().saturating_sub(1));
        let box_pos = map.0.box_spots[box_idx];
        if box_pos != Vec3::ZERO {
            let vhud_box = Vec3::new(box_pos.x, -box_pos.z, box_pos.y);
            let blip = Blip::new(BLIP_BOX_ID, vhud_box, BlipType::Custom("Box".into()))
                .with_color(BlipColor::new(0.2, 0.9, 1.0, 1.0))
                .with_priority(25)
                .with_scale(1.2)
                .with_clamp(true);
            radar_state.blip_manager.add_or_update(blip);
        }

        // Extraction Beacon
        let ext_pos = map.0.extraction;
        if ext_pos != Vec3::ZERO {
            let vhud_ext = Vec3::new(ext_pos.x, -ext_pos.z, ext_pos.y);
            let blip = Blip::new(BLIP_EXTRACT_ID, vhud_ext, BlipType::Mission)
                .with_color(BlipColor::new(0.25, 0.95, 0.45, 1.0))
                .with_priority(28)
                .with_scale(1.25)
                .with_clamp(true)
                .with_flashing(state.teleport);
            radar_state.blip_manager.add_or_update(blip);
        }

        // Perk Machines
        for (i, &perk_pos) in map.0.perk_spots.iter().enumerate() {
            if perk_pos != Vec3::ZERO {
                let vhud_p = Vec3::new(perk_pos.x, -perk_pos.z, perk_pos.y);
                let color = match i {
                    0 => BlipColor::new(0.3, 0.6, 1.0, 0.85),  // Quick Revive (blue)
                    1 => BlipColor::new(1.0, 0.3, 0.3, 0.85),  // Juggernaut (red)
                    2 => BlipColor::new(0.3, 0.9, 0.45, 0.85), // Speed Cola (green)
                    3 => BlipColor::new(1.0, 0.65, 0.2, 0.85), // Double Tap (orange)
                    _ => BlipColor::new(1.0, 0.9, 0.25, 0.85), // Stamin-Up (yellow)
                };
                let blip = Blip::new(
                    BLIP_PERK_BASE + i as u64,
                    vhud_p,
                    BlipType::Custom("Perk".into()),
                )
                .with_color(color)
                .with_priority(8)
                .with_scale(0.8)
                .with_clamp(false);
                radar_state.blip_manager.add_or_update(blip);
            }
        }

        // Upgrade Stations
        for (i, &upg_pos) in map.0.upgrade_stations.iter().enumerate() {
            if upg_pos != Vec3::ZERO {
                let vhud_upg = Vec3::new(upg_pos.x, -upg_pos.z, upg_pos.y);
                let blip = Blip::new(
                    BLIP_UPGRADE_BASE + i as u64,
                    vhud_upg,
                    BlipType::Custom("Upgrade".into()),
                )
                .with_color(BlipColor::new(0.85, 0.35, 1.0, 0.9)) // Purple
                .with_priority(15)
                .with_scale(1.0)
                .with_clamp(true);
                radar_state.blip_manager.add_or_update(blip);
            }
        }
    }

    // 2c. Tactical Pings
    let mut latest_ping_pos = None;
    for (i, ping) in pings.iter().enumerate() {
        let vhud_ping = Vec3::new(ping.pos.x, -ping.pos.z, ping.pos.y);
        latest_ping_pos = Some(vhud_ping);
        let c = ping.kind.color().to_srgba();
        let blip_col = BlipColor::new(c.red, c.green, c.blue, c.alpha);
        let blip = Blip::new(
            BLIP_PING_BASE + i as u64,
            vhud_ping,
            BlipType::Waypoint,
        )
        .with_color(blip_col)
        .with_priority(26)
        .with_scale(1.15)
        .with_clamp(true);
        radar_state.blip_manager.add_or_update(blip);
    }

    // 2d. Co-op Teammates
    for (team_id, player_info) in &roster.0 {
        if *team_id != session.my_id && player_info.alive {
            let feet = player_info.feet();
            let vhud_team = Vec3::new(feet.x, -feet.z, feet.y);
            let blip = Blip::new(
                BLIP_TEAM_BASE + *team_id as u64,
                vhud_team,
                BlipType::Neutral,
            )
            .with_color(BlipColor::new(0.3, 0.75, 1.0, 1.0))
            .with_priority(20)
            .with_scale(1.05)
            .with_clamp(true);
            radar_state.blip_manager.add_or_update(blip);
        }
    }

    // 3. Process Blips with vhud-rs
    radar_state.blip_manager.tick(dt);
    let rendered_blips = radar_state
        .blip_manager
        .process_blips(player_pos_vhud, player_heading, &radar_state.projection);

    // 4. Update Blip Pool UI
    let mut slot_map: std::collections::BTreeMap<usize, &vhud_rs::RenderedBlip> =
        std::collections::BTreeMap::new();
    for (idx, blip) in rendered_blips.iter().rev().enumerate() {
        if idx < MAX_BLIP_SLOTS {
            slot_map.insert(idx, blip);
        }
    }

    for (slot, mut node, mut bg, mut vis) in &mut blip_nodes {
        if let Some(blip) = slot_map.get(&slot.0) {
            *vis = Visibility::Inherited;
            let size = match blip.blip_type {
                BlipType::Mission | BlipType::Waypoint => 12.0 * blip.scale,
                BlipType::Custom(_) => 10.0 * blip.scale,
                BlipType::Enemy => {
                    if blip.scale > 1.2 {
                        13.0
                    } else {
                        8.5 * blip.scale
                    }
                }
                _ => 8.0 * blip.scale,
            };

            node.left = Val::Px(blip.screen_pos.x - size * 0.5);
            node.top = Val::Px(blip.screen_pos.y - size * 0.5);
            node.width = Val::Px(size);
            node.height = Val::Px(size);
            node.border_radius = match blip.blip_type {
                BlipType::Waypoint | BlipType::Custom(_) => BorderRadius::all(Val::Px(2.5)),
                _ => BorderRadius::all(Val::Px(size * 0.5)),
            };

            bg.0 = Color::srgba(
                blip.effective_color.r,
                blip.effective_color.g,
                blip.effective_color.b,
                blip.effective_color.a,
            );
        } else {
            *vis = Visibility::Hidden;
        }
    }

    for (icon_slot, mut text, mut color) in &mut blip_icons {
        if let Some(blip) = slot_map.get(&icon_slot.0) {
            match blip.blip_type {
                BlipType::Custom(ref s) if s == "Box" => {
                    **text = "?".to_string();
                    color.0 = Color::BLACK;
                }
                BlipType::Custom(ref s) if s == "Upgrade" => {
                    **text = "★".to_string();
                    color.0 = Color::WHITE;
                }
                BlipType::Enemy => {
                    match blip.altitude {
                        AltitudeRelation::Above => {
                            **text = "▲".to_string();
                            color.0 = Color::WHITE;
                        }
                        AltitudeRelation::Below => {
                            **text = "▼".to_string();
                            color.0 = Color::WHITE;
                        }
                        AltitudeRelation::Level => {
                            **text = String::new();
                        }
                    }
                }
                _ => {
                    **text = String::new();
                }
            }
        } else {
            **text = String::new();
        }
    }

    // 5. GPS Route Navigation
    let route_target = if let Some(ping_pos) = latest_ping_pos {
        Some(ping_pos)
    } else if let Some(map) = &map {
        if state.teleport && map.0.extraction != Vec3::ZERO {
            Some(Vec3::new(map.0.extraction.x, -map.0.extraction.z, map.0.extraction.y))
        } else {
            let box_idx = (state.box_spot as usize).min(map.0.box_spots.len().saturating_sub(1));
            let b = map.0.box_spots[box_idx];
            if b != Vec3::ZERO {
                Some(Vec3::new(b.x, -b.z, b.y))
            } else {
                None
            }
        }
    } else {
        None
    };

    if let Some(target) = route_target {
        let route = GpsRoute::new(vec![player_pos_vhud, target]);
        radar_state.route_navigator.set_route(route);
    } else {
        radar_state.route_navigator.clear_route();
    }

    let rendered_route = radar_state
        .route_navigator
        .render_route(player_pos_vhud, player_heading, &radar_state.projection);

    for (line_slot, mut node, mut tf, mut bg, mut vis) in &mut gps_lines {
        if let Some(ref route) = rendered_route {
            if line_slot.0 < route.segments.len() {
                let seg = route.segments[line_slot.0];
                let delta = seg.end - seg.start;
                let length = delta.length();
                if length > 1.0 {
                    let mid = (seg.start + seg.end) * 0.5;
                    let angle = delta.y.atan2(delta.x);
                    *vis = Visibility::Inherited;
                    node.left = Val::Px(mid.x - length * 0.5);
                    node.top = Val::Px(mid.y - 1.5);
                    node.width = Val::Px(length);
                    node.height = Val::Px(3.0);
                    tf.rotation = Quat::from_rotation_z(angle);
                    bg.0 = Color::srgba(route.color.r, route.color.g, route.color.b, 0.85);
                } else {
                    *vis = Visibility::Hidden;
                }
            } else {
                *vis = Visibility::Hidden;
            }
        } else {
            *vis = Visibility::Hidden;
        }
    }

    // 6. Rotating North Cardinal Indicator
    let north_angle = -player_heading - FRAC_PI_2;
    let rim_radius = RADAR_RADIUS - 1.5;
    let nx = RADAR_CENTER.x + north_angle.cos() * rim_radius;
    let ny = RADAR_CENTER.y + north_angle.sin() * rim_radius;
    north_node.left = Val::Px(nx - 7.0);
    north_node.top = Val::Px(ny - 7.0);

    // 7. Update GTA V HUD Ring Meters (Health & Ultimate Ability)
    let (health_pct, ult_pct) = if let Some(me) = roster.me(&session) {
        let max_hp = me.max_health().max(1.0);
        let hp_pct = (me.health / max_hp).clamp(0.0, 1.0);
        let u_pct = (me.ult_charge / 100.0).clamp(0.0, 1.0);
        (hp_pct, u_pct)
    } else {
        (1.0, 0.0)
    };

    radar_state.ring_system.tick(dt);
    radar_state.ring_system.set_health(health_pct * 100.0);
    radar_state.ring_system.set_special_ability(ult_pct * 100.0);

    let health_changed = (health_pct - radar_state.last_health_pct).abs() > 0.005;
    let ult_changed = (ult_pct - radar_state.last_ult_pct).abs() > 0.005;
    let is_pulsing = health_pct <= 0.25 || ult_pct >= 0.99;

    if health_changed || ult_changed || is_pulsing {
        radar_state.last_health_pct = health_pct;
        radar_state.last_ult_pct = ult_pct;

        let meters = radar_state
            .ring_system
            .process_meters(&radar_state.projection.config);
        render_rings_into_buffer(&mut radar_state.ring_image_data, &meters);

        if let Some(mut img) = images.get_mut(&radar_state.ring_texture) {
            img.data = Some(radar_state.ring_image_data.clone());
        }
    }
}

/// Generates the static radar background texture: dark blueprint disk, range ring,
/// crosshair axes with center deadzone, and outer border ring.
fn create_radar_background_image() -> Image {
    const N: usize = TEX_SIZE;
    const C: f32 = N as f32 * 0.5;
    const R: f32 = 96.0; // Corresponds to RADAR_RADIUS (72.0) scaled by 256/192

    let mut data = vec![0u8; N * N * 4];

    for y in 0..N {
        let dy = y as f32 + 0.5 - C;
        for x in 0..N {
            let dx = x as f32 + 0.5 - C;
            let r2 = dx * dx + dy * dy;
            let r = r2.sqrt();
            let idx = (y * N + x) * 4;

            if r > R + 1.5 {
                continue;
            }

            // Anti-aliased outer rim falloff
            let outer_alpha = (R + 1.0 - r).clamp(0.0, 1.0);

            // 1. Dark circular disk
            let mut red = 10u8;
            let mut green = 14u8;
            let mut blue = 22u8;
            let mut alpha = (215.0 * outer_alpha) as u8;

            // 2. Faint 50% range ring
            let r_mid = R * 0.5;
            let dist_mid = (r - r_mid).abs();
            if dist_mid < 1.2 {
                let mid_a = (1.0 - dist_mid / 1.2).powi(2) * 0.35;
                red = (red as f32 * (1.0 - mid_a) + 85.0 * mid_a) as u8;
                green = (green as f32 * (1.0 - mid_a) + 120.0 * mid_a) as u8;
                blue = (blue as f32 * (1.0 - mid_a) + 165.0 * mid_a) as u8;
                alpha = alpha.max((235.0 * mid_a) as u8);
            }

            // 3. Subtle Crosshair lines with 14px center deadzone
            let in_deadzone = r < 14.0;
            if !in_deadzone && r < R - 2.0 {
                let on_h_axis = dy.abs() < 0.9;
                let on_v_axis = dx.abs() < 0.9;
                if on_h_axis || on_v_axis {
                    let axis_a = 0.28;
                    red = (red as f32 * (1.0 - axis_a) + 100.0 * axis_a) as u8;
                    green = (green as f32 * (1.0 - axis_a) + 140.0 * axis_a) as u8;
                    blue = (blue as f32 * (1.0 - axis_a) + 185.0 * axis_a) as u8;
                }
            }

            // 4. Outer perimeter border ring
            if r >= R - 2.5 && r <= R + 0.5 {
                let border_a = 0.75 * outer_alpha;
                red = (red as f32 * (1.0 - border_a) + 155.0 * border_a) as u8;
                green = (green as f32 * (1.0 - border_a) + 190.0 * border_a) as u8;
                blue = (blue as f32 * (1.0 - border_a) + 230.0 * border_a) as u8;
                alpha = alpha.max((255.0 * border_a) as u8);
            }

            // 5. Cardinal tick marks on the perimeter
            let is_cardinal_tick = (dx.abs() < 1.2 && (r > R - 5.0 && r < R + 0.5))
                || (dy.abs() < 1.2 && (r > R - 5.0 && r < R + 0.5));
            if is_cardinal_tick {
                red = 210;
                green = 230;
                blue = 255;
                alpha = (255.0 * outer_alpha) as u8;
            }

            data[idx] = red;
            data[idx + 1] = green;
            data[idx + 2] = blue;
            data[idx + 3] = alpha;
        }
    }

    Image::new(
        Extent3d {
            width: N as u32,
            height: N as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// Renders the HUD arc meters into an RGBA byte buffer.
fn render_rings_into_buffer(data: &mut [u8], meters: &[RenderedArcMeter]) {
    const N: usize = TEX_SIZE;
    const C: f32 = N as f32 * 0.5;

    data.fill(0);

    for m in meters {
        let (r_in, r_out) = (m.inner_radius, m.outer_radius);
        let (total_start, total_end) = m.total_arc;
        let (fill_start, fill_end) = m.fill_arc;

        // Meters span the bottom half of the circle
        let y_start = C as usize;
        let y_end = ((C + r_out + 2.0).min(N as f32 - 1.0)) as usize;

        for y in y_start..=y_end {
            let dy = y as f32 + 0.5 - C;
            for x in 0..N {
                let dx = x as f32 + 0.5 - C;
                let r2 = dx * dx + dy * dy;
                if r2 < (r_in - 1.0) * (r_in - 1.0) || r2 > (r_out + 1.0) * (r_out + 1.0) {
                    continue;
                }
                let r = r2.sqrt();

                let edge_alpha = ((r - r_in + 0.5).clamp(0.0, 1.0)
                    * (r_out + 0.5 - r).clamp(0.0, 1.0))
                .clamp(0.0, 1.0);
                if edge_alpha <= 0.01 {
                    continue;
                }

                let angle = dy.atan2(dx);
                if angle > total_start || angle < total_end {
                    continue;
                }

                let is_fill = angle >= fill_end && angle <= fill_start;
                let color = if is_fill {
                    m.active_color
                } else {
                    m.background_color
                };

                let idx = (y * N + x) * 4;
                let a = (color.a * edge_alpha * 255.0).clamp(0.0, 255.0) as u8;
                if a > data[idx + 3] {
                    data[idx] = (color.r * 255.0) as u8;
                    data[idx + 1] = (color.g * 255.0) as u8;
                    data[idx + 2] = (color.b * 255.0) as u8;
                    data[idx + 3] = a;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_radar_projection_forward_alignment() {
        let config = RadarConfig::new_circular(RADAR_CENTER, RADAR_RADIUS);
        let radar = RadarProjection::new(config);

        // Player at origin, facing North in rust-fps (yaw = 0.0)
        let player_pos_vhud = Vec3::new(0.0, 0.0, 0.0);
        let player_heading = 0.0;

        // Target 20m in front of player (rust-fps 3D: (0, 0, -20) -> vhud: (0, 20, 0))
        let target_vhud = Vec3::new(0.0, 20.0, 0.0);

        let screen_pos = radar.world3d_to_screen(target_vhud, player_pos_vhud, player_heading);

        // Should be centered horizontally and strictly above center vertically (smaller screen Y)
        assert!((screen_pos.x - RADAR_CENTER.x).abs() < 1e-3);
        assert!(screen_pos.y < RADAR_CENTER.y);
    }

    #[test]
    fn test_radar_blip_clamping_and_altitude() {
        let config = RadarConfig::new_circular(RADAR_CENTER, RADAR_RADIUS);
        let radar = RadarProjection::new(config);

        let mut mgr = BlipManager::new();
        mgr.border_margin = 6.0;

        // Enemy far away (150m) and 10m above player
        let distant_enemy = Blip::new(1, Vec3::new(0.0, 150.0, 10.0), BlipType::Enemy)
            .with_clamp(true);
        mgr.add_or_update(distant_enemy);

        let player_pos = Vec3::ZERO;
        let player_heading = 0.0;
        let rendered = mgr.process_blips(player_pos, player_heading, &radar);

        assert_eq!(rendered.len(), 1);
        let blip = &rendered[0];
        assert!(blip.is_clamped);
        assert_eq!(blip.altitude, AltitudeRelation::Above);

        let dist_from_center = (blip.screen_pos - RADAR_CENTER).length();
        let expected_radius = RADAR_RADIUS - mgr.border_margin;
        assert!((dist_from_center - expected_radius).abs() < 1e-3);
    }

    #[test]
    fn test_ring_buffer_rendering_no_panic() {
        let ring_system = HudRingSystem::new_gta_v_circular(
            RADAR_RADIUS * (TEX_SIZE as f32 / RADAR_SIZE),
            11.0,
        );
        let config = RadarConfig::new_circular(RADAR_CENTER, RADAR_RADIUS);
        let meters = ring_system.process_meters(&config);

        let mut data = vec![0u8; TEX_SIZE * TEX_SIZE * 4];
        render_rings_into_buffer(&mut data, &meters);

        // Verify that some non-zero pixels were generated in the ring area
        let non_zero_pixels = data.chunks_exact(4).filter(|p| p[3] > 0).count();
        assert!(non_zero_pixels > 100);
    }

    #[test]
    fn test_gps_route_circular_clip() {
        let config = RadarConfig::new_circular(RADAR_CENTER, RADAR_RADIUS);
        let radar = RadarProjection::new(config);

        let mut nav = RouteNavigator::new();
        let route = GpsRoute::new(vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 100.0, 0.0),
        ]);
        nav.set_route(route);

        let rendered = nav.render_route(Vec3::ZERO, 0.0, &radar);
        assert!(rendered.is_some());
        let res = rendered.unwrap();
        assert!(!res.segments.is_empty());

        for seg in &res.segments {
            assert!((seg.start - RADAR_CENTER).length() <= RADAR_RADIUS + 0.1);
            assert!((seg.end - RADAR_CENTER).length() <= RADAR_RADIUS + 0.1);
        }
    }
}
