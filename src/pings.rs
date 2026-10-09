//! Pings: press Z (or the middle mouse button) to mark the spot you are
//! looking at for your whole party. Quick-tapping drops a quick location or
//! enemy ping. Holding opens a tactical radial ping wheel powered by `radial-menu-rs`
//! with spring-animated hover responses and distinct tactical callouts.

use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::window::{CursorOptions, PrimaryWindow};
use radial_menu_rs::{RadialItem, RadialMenu, RadialPointer, Spring1D, TWO_PI};
use serde::{Deserialize, Serialize};

use crate::abilities::{queue_action, ActionCounter};
use crate::config::{Action, InputExt, Settings};
use crate::fx::{Fx, FxQueue};
use crate::game::Paused;
use crate::kit::{c, Kit};
use crate::physics::{collect_boxes, trace_shot};
use crate::player::{can_act, LocalPlayer};
use crate::{
    ActionQueue, AppState, Collider, Enemy, InGameEntity, MatchState, Phase, PlayerAction,
    Replicated, Roster, Session,
};

pub struct PingPlugin;

impl Plugin for PingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PingWheel>()
            .add_systems(Startup, setup)
            .add_systems(OnEnter(AppState::InGame), spawn_wheel_ui)
            .add_systems(
                Update,
                send_ping
                    .in_set(Phase::Local)
                    .before(crate::player::movement),
            )
            .add_systems(
                Update,
                (
                    update_wheel_ui,
                    spawn_pings.before(crate::fx::play),
                    place_pings.after(crate::fx::play),
                )
                    .in_set(Phase::Present)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(OnExit(AppState::InGame), reset_wheel);
    }
}

const PING_LIFE: f32 = 7.0;
const PING_RANGE: f32 = 120.0;
const HOLD_THRESHOLD: f32 = 0.18;

/// Tactical ping kinds available in the game and radial ping wheel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum PingKind {
    Standard = 0,
    Enemy = 1,
    Danger = 2,
    OnMyWay = 3,
    NeedHelp = 4,
    Regroup = 5,
    WatchHere = 6,
    AllClear = 7,
}

impl PingKind {
    pub const TACTICAL_OPTIONS: [PingKind; 6] = [
        PingKind::Danger,
        PingKind::OnMyWay,
        PingKind::NeedHelp,
        PingKind::Regroup,
        PingKind::WatchHere,
        PingKind::AllClear,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            PingKind::Standard => "Ping",
            PingKind::Enemy => "Enemy",
            PingKind::Danger => "Danger",
            PingKind::OnMyWay => "On My Way",
            PingKind::NeedHelp => "Need Help",
            PingKind::Regroup => "Regroup",
            PingKind::WatchHere => "Watch Here",
            PingKind::AllClear => "All Clear",
        }
    }

    pub fn color(&self) -> Color {
        match self {
            PingKind::Standard => Color::srgb(1.0, 0.85, 0.2),   // Amber gold
            PingKind::Enemy => Color::srgb(1.0, 0.22, 0.18),      // Red
            PingKind::Danger => Color::srgb(0.95, 0.15, 0.15),   // Bold Crimson
            PingKind::OnMyWay => Color::srgb(0.2, 0.85, 1.0),    // Cyan
            PingKind::NeedHelp => Color::srgb(1.0, 0.55, 0.12),  // Orange
            PingKind::Regroup => Color::srgb(0.85, 0.35, 1.0),   // Violet
            PingKind::WatchHere => Color::srgb(1.0, 0.88, 0.25), // Yellow
            PingKind::AllClear => Color::srgb(0.25, 0.95, 0.45), // Emerald green
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            PingKind::Standard => "📍",
            PingKind::Enemy => "⚔️",
            PingKind::Danger => "⚠️",
            PingKind::OnMyWay => "🏃",
            PingKind::NeedHelp => "🆘",
            PingKind::Regroup => "🛡️",
            PingKind::WatchHere => "👁️",
            PingKind::AllClear => "✅",
        }
    }
}

impl From<u8> for PingKind {
    fn from(val: u8) -> Self {
        match val {
            1 => PingKind::Enemy,
            2 => PingKind::Danger,
            3 => PingKind::OnMyWay,
            4 => PingKind::NeedHelp,
            5 => PingKind::Regroup,
            6 => PingKind::WatchHere,
            7 => PingKind::AllClear,
            _ => PingKind::Standard,
        }
    }
}

/// Resource managing the tactical radial ping wheel state and animation.
#[derive(Resource)]
pub struct PingWheel {
    pub is_holding: bool,
    pub hold_time: f32,
    pub open: bool,
    pub deflection: Vec2,
    pub selected_slice: Option<usize>,
    pub springs: [Spring1D; 6],
    pub menu: RadialMenu<PingKind>,
    pub cooldown: f32,
}

impl Default for PingWheel {
    fn default() -> Self {
        let pointer = RadialPointer::new(Vec2::ZERO, 38.0, 180.0);
        let items = vec![
            RadialItem::with_payload("danger", "Danger", PingKind::Danger),
            RadialItem::with_payload("on_my_way", "On My Way", PingKind::OnMyWay),
            RadialItem::with_payload("need_help", "Need Help", PingKind::NeedHelp),
            RadialItem::with_payload("regroup", "Regroup", PingKind::Regroup),
            RadialItem::with_payload("watch_here", "Watch Here", PingKind::WatchHere),
            RadialItem::with_payload("all_clear", "All Clear", PingKind::AllClear),
        ];
        let menu = RadialMenu::new(items, pointer);
        let springs = [
            Spring1D::new(1.0),
            Spring1D::new(1.0),
            Spring1D::new(1.0),
            Spring1D::new(1.0),
            Spring1D::new(1.0),
            Spring1D::new(1.0),
        ];
        Self {
            is_holding: false,
            hold_time: 0.0,
            open: false,
            deflection: Vec2::ZERO,
            selected_slice: None,
            springs,
            menu,
            cooldown: 0.0,
        }
    }
}

fn reset_wheel(mut wheel: ResMut<PingWheel>) {
    wheel.is_holding = false;
    wheel.open = false;
    wheel.hold_time = 0.0;
    wheel.selected_slice = None;
    wheel.deflection = Vec2::ZERO;
}

#[derive(Resource)]
struct PingAssets {
    marker: Handle<Mesh>,
    beam: Handle<Mesh>,
    materials: [Handle<StandardMaterial>; 8],
}

#[derive(Component)]
pub struct Ping {
    pub player: u8,
    pub kind: PingKind,
    pub pos: Vec3,
    pub target: Option<u32>,
    pub life: f32,
    pub label: Entity,
}

#[derive(Component)]
struct PingLabel;

// Marker components for radial wheel UI
#[derive(Component)]
struct PingWheelRoot;

#[derive(Component)]
struct PingWheelCenterTitle;

#[derive(Component)]
struct PingWheelCenterHint;

#[derive(Component)]
struct PingWheelCursorReticle;

#[derive(Component)]
struct PingWheelSliceCard {
    index: usize,
}

#[derive(Component)]
struct PingWheelSliceText {
    index: usize,
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // An upside-down diamond with a ring under it.
    let mut k = Kit::new();
    k.cone(
        Vec3::new(0.0, 0.0, 0.0),
        0.28,
        0.5,
        Quat::from_rotation_x(std::f32::consts::PI),
        c(1.0, 1.0, 1.0),
    );
    k.cone(
        Vec3::new(0.0, 0.42, 0.0),
        0.28,
        0.34,
        Quat::IDENTITY,
        c(1.0, 1.0, 1.0),
    );
    let glow = |materials: &mut Assets<StandardMaterial>, color: Color| {
        materials.add(StandardMaterial {
            base_color: color.with_alpha(0.85),
            emissive: LinearRgba::from(color) * 4.0,
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        })
    };
    let mats = [
        glow(&mut materials, PingKind::Standard.color()),
        glow(&mut materials, PingKind::Enemy.color()),
        glow(&mut materials, PingKind::Danger.color()),
        glow(&mut materials, PingKind::OnMyWay.color()),
        glow(&mut materials, PingKind::NeedHelp.color()),
        glow(&mut materials, PingKind::Regroup.color()),
        glow(&mut materials, PingKind::WatchHere.color()),
        glow(&mut materials, PingKind::AllClear.color()),
    ];
    commands.insert_resource(PingAssets {
        marker: meshes.add(k.build_or_empty()),
        beam: meshes.add(Cylinder::new(0.04, 1.0)),
        materials: mats,
    });
}

fn spawn_wheel_ui(mut commands: Commands) {
    let tactical_items = PingKind::TACTICAL_OPTIONS;
    let radius = 142.0;

    commands
        .spawn((
            InGameEntity,
            PingWheelRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(0.0),
                right: Val::Percent(0.0),
                top: Val::Percent(0.0),
                bottom: Val::Percent(0.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            Transform::default(),
            Visibility::Hidden,
            GlobalZIndex(60),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    position_type: PositionType::Relative,
                    width: Val::Px(0.0),
                    height: Val::Px(0.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                Transform::default(),
            ))
            .with_children(|center| {
                // Subtle boundary ring
                center.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(-165.0),
                        top: Val::Px(-165.0),
                        width: Val::Px(330.0),
                        height: Val::Px(330.0),
                        border_radius: BorderRadius::all(Val::Px(165.0)),
                        border: UiRect::all(Val::Px(1.5)),
                        ..default()
                    },
                    BorderColor::all(Color::srgba(0.35, 0.45, 0.6, 0.25)),
                ));

                // Central Deadzone Hub
                center
                    .spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(-56.0),
                            top: Val::Px(-56.0),
                            width: Val::Px(112.0),
                            height: Val::Px(112.0),
                            border_radius: BorderRadius::all(Val::Px(56.0)),
                            border: UiRect::all(Val::Px(2.0)),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(2.0),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.04, 0.05, 0.09, 0.92)),
                        BorderColor::all(Color::srgba(0.45, 0.55, 0.7, 0.6)),
                    ))
                    .with_children(|hub| {
                        hub.spawn((
                            PingWheelCenterTitle,
                            Text::new("TACTICAL"),
                            TextFont {
                                font_size: 13.0.into(),
                                ..default()
                            },
                            TextColor(Color::WHITE),
                            TextLayout::justify(Justify::Center),
                        ));
                        hub.spawn((
                            PingWheelCenterHint,
                            Text::new("Release to Cancel"),
                            TextFont {
                                font_size: 9.5.into(),
                                ..default()
                            },
                            TextColor(Color::srgb(0.65, 0.7, 0.8)),
                            TextLayout::justify(Justify::Center),
                        ));
                    });

                // Deflection pointer reticle
                center.spawn((
                    PingWheelCursorReticle,
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(-6.0),
                        top: Val::Px(-6.0),
                        width: Val::Px(12.0),
                        height: Val::Px(12.0),
                        border_radius: BorderRadius::all(Val::Px(6.0)),
                        border: UiRect::all(Val::Px(1.5)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.95)),
                    BorderColor::all(Color::srgb(0.2, 0.85, 1.0)),
                ));

                // 6 Radial Slice Badges
                for (i, kind) in tactical_items.iter().enumerate() {
                    let angle = (i as f32) * (TWO_PI / 6.0);
                    let dx = angle.sin() * radius;
                    let dy = -angle.cos() * radius;
                    let card_w = 126.0;
                    let card_h = 40.0;

                    center
                        .spawn((
                            PingWheelSliceCard { index: i },
                            Node {
                                position_type: PositionType::Absolute,
                                left: Val::Px(dx - card_w * 0.5),
                                top: Val::Px(dy - card_h * 0.5),
                                width: Val::Px(card_w),
                                height: Val::Px(card_h),
                                border_radius: BorderRadius::all(Val::Px(8.0)),
                                border: UiRect::all(Val::Px(1.5)),
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                                flex_direction: FlexDirection::Row,
                                column_gap: Val::Px(6.0),
                                ..default()
                            },
                            BackgroundColor(Color::srgba(0.06, 0.08, 0.13, 0.88)),
                            BorderColor::all(kind.color().with_alpha(0.5)),
                            Transform::default(),
                        ))
                        .with_children(|card| {
                            card.spawn((
                                PingWheelSliceText { index: i },
                                Text::new(format!("{} {}", kind.icon(), kind.label())),
                                TextFont {
                                    font_size: 13.0.into(),
                                    ..default()
                                },
                                TextColor(Color::srgb(0.9, 0.93, 0.97)),
                            ));
                        });
                }
            });
        });
}

fn update_wheel_ui(
    wheel: Res<PingWheel>,
    mut root_q: Query<&mut Visibility, With<PingWheelRoot>>,
    mut center_title: Query<&mut Text, (With<PingWheelCenterTitle>, Without<PingWheelCenterHint>)>,
    mut center_hint: Query<&mut Text, (With<PingWheelCenterHint>, Without<PingWheelCenterTitle>)>,
    mut cursor_reticle: Query<&mut Node, With<PingWheelCursorReticle>>,
    mut slice_cards: Query<
        (&PingWheelSliceCard, &mut Transform, &mut BackgroundColor, &mut BorderColor),
        Without<PingWheelCursorReticle>,
    >,
    mut text_q: Query<(&PingWheelSliceText, &mut TextColor)>,
) {
    for mut root_vis in &mut root_q {
        let want = if wheel.open {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *root_vis != want {
            *root_vis = want;
        }
    }

    if !wheel.open {
        return;
    }

    // Reticle cursor position
    for mut node in &mut cursor_reticle {
        node.left = Val::Px(wheel.deflection.x - 6.0);
        node.top = Val::Px(wheel.deflection.y - 6.0);
    }

    // Center Hub Text
    for mut title in &mut center_title {
        if let Some(idx) = wheel.selected_slice {
            let kind = wheel.menu.items[idx].payload;
            title.0 = kind.label().to_uppercase();
        } else {
            title.0 = "CANCEL".to_string();
        }
    }
    for mut hint in &mut center_hint {
        if wheel.selected_slice.is_some() {
            hint.0 = "Release to Ping".to_string();
        } else {
            hint.0 = "Neutral: Cancel".to_string();
        }
    }

    // Slice badges: smooth scale via Spring1D + active glow
    for (card, mut tf, mut bg, mut border) in &mut slice_cards {
        let is_selected = wheel.selected_slice == Some(card.index);
        let spring_scale = wheel.springs.get(card.index).map_or(1.0, |s| s.position);
        tf.scale = Vec3::splat(spring_scale);

        let kind = PingKind::TACTICAL_OPTIONS[card.index.min(5)];
        if is_selected {
            *bg = BackgroundColor(kind.color().with_alpha(0.35));
            *border = BorderColor::all(kind.color());
        } else {
            *bg = BackgroundColor(Color::srgba(0.06, 0.08, 0.13, 0.88));
            *border = BorderColor::all(kind.color().with_alpha(0.4));
        }
    }

    for (slice_text, mut text_color) in &mut text_q {
        let is_selected = wheel.selected_slice == Some(slice_text.index);
        if is_selected {
            text_color.0 = Color::WHITE;
        } else {
            text_color.0 = Color::srgb(0.88, 0.91, 0.95);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn send_ping(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    settings: Res<Settings>,
    cursor: Single<&CursorOptions, With<PrimaryWindow>>,
    session: Res<Session>,
    roster: Res<Roster>,
    state: Res<MatchState>,
    paused: Res<Paused>,
    mut counter: ResMut<ActionCounter>,
    mut queue: ResMut<ActionQueue>,
    player: Single<(&Transform, &LocalPlayer)>,
    colliders: Query<(&Transform, &Collider), Without<LocalPlayer>>,
    enemies: Query<(Entity, &Transform, &Replicated), With<Enemy>>,
    mut wheel: ResMut<PingWheel>,
) {
    let dt = time.delta_secs();
    wheel.cooldown -= dt;

    if !can_act(&session, &roster, &state, &paused, &cursor) {
        if wheel.is_holding || wheel.open {
            wheel.is_holding = false;
            wheel.open = false;
            wheel.selected_slice = None;
            wheel.deflection = Vec2::ZERO;
        }
        return;
    }

    let ping_down = keys.held(&settings, Action::Ping) || mouse.pressed(MouseButton::Middle);
    let ping_just_pressed =
        keys.tapped(&settings, Action::Ping) || mouse.just_pressed(MouseButton::Middle);

    if !wheel.is_holding && ping_just_pressed && wheel.cooldown <= 0.0 {
        wheel.is_holding = true;
        wheel.hold_time = 0.0;
        wheel.open = false;
        wheel.deflection = Vec2::ZERO;
        wheel.selected_slice = None;
    }

    if wheel.is_holding {
        if ping_down {
            wheel.hold_time += dt;
            if wheel.hold_time >= HOLD_THRESHOLD {
                wheel.open = true;
                wheel.deflection += motion.delta;
                let len = wheel.deflection.length();
                let max_r = 190.0;
                if len > max_r {
                    wheel.deflection *= max_r / len;
                }
                let (_sample, slice) = wheel.menu.select_slice(wheel.deflection);
                wheel.selected_slice = slice;
            }

            let is_open = wheel.open;
            let selected_slice = wheel.selected_slice;
            for (i, spring) in wheel.springs.iter_mut().enumerate() {
                let target = if is_open && selected_slice == Some(i) {
                    1.22
                } else {
                    1.0
                };
                spring.set_target(target);
                spring.update(dt);
            }
        } else {
            // Button released: decide quick ping vs tactical radial ping
            wheel.is_holding = false;
            let was_open = wheel.open;
            wheel.open = false;

            let (cam, p) = player.into_inner();
            let origin = if p.third_person() {
                p.eye_pos()
            } else {
                cam.translation
            };
            let dir = if p.third_person() {
                let (yaw, pitch) = (p.yaw, p.pitch);
                Vec3::new(
                    -yaw.sin() * pitch.cos(),
                    pitch.sin(),
                    -yaw.cos() * pitch.cos(),
                )
            } else {
                cam.forward().as_vec3()
            };
            let boxes = collect_boxes(colliders.iter());
            let hit = trace_shot(
                origin,
                dir,
                PING_RANGE,
                &boxes,
                enemies
                    .iter()
                    .map(|(e, t, r)| (e, t.translation, crate::sim::enemy_scale(r.kind), false)),
            );
            let target = hit
                .enemy
                .and_then(|(e, _)| enemies.get(e).ok())
                .map(|(_, _, r)| r.id);
            let pos = origin + dir * (hit.dist - 0.05).max(0.5);

            if !was_open {
                // Quick tap: standard or enemy ping
                wheel.cooldown = 0.35;
                let kind = if target.is_some() {
                    PingKind::Enemy
                } else {
                    PingKind::Standard
                };
                queue_action(
                    &session,
                    &mut counter,
                    &mut queue,
                    PlayerAction::Ping {
                        pos: pos.to_array(),
                        target: target.unwrap_or(u32::MAX),
                        kind: kind as u8,
                    },
                );
            } else if let Some(slice_idx) = wheel.selected_slice {
                // Tactical radial ping release outside deadzone
                wheel.cooldown = 0.35;
                let tactical_kind = wheel.menu.items[slice_idx].payload;
                queue_action(
                    &session,
                    &mut counter,
                    &mut queue,
                    PlayerAction::Ping {
                        pos: pos.to_array(),
                        target: target.unwrap_or(u32::MAX),
                        kind: tactical_kind as u8,
                    },
                );
            } else {
                // Released in deadzone: canceled
            }

            wheel.selected_slice = None;
            wheel.deflection = Vec2::ZERO;
        }
    } else {
        // Return springs to rest
        for spring in wheel.springs.iter_mut() {
            spring.set_target(1.0);
            spring.update(dt);
        }
    }
}

/// Turns ping effects into markers (one per player).
fn spawn_pings(
    mut commands: Commands,
    queue: Res<FxQueue>,
    assets: Res<PingAssets>,
    roster: Res<Roster>,
    existing: Query<(Entity, &Ping)>,
) {
    for fx in &queue.0 {
        let Fx::Ping {
            player,
            pos,
            target,
            kind: kind_u8,
        } = *fx
        else {
            continue;
        };
        for (e, ping) in &existing {
            if ping.player == player {
                commands.entity(ping.label).despawn();
                commands.entity(e).despawn();
            }
        }
        let kind = PingKind::from(kind_u8);
        let enemy = target != u32::MAX;
        let p_name = roster
            .0
            .get(&player)
            .map_or("?".to_string(), |p| p.name.clone());
        let color = kind.color();
        let display_title = if kind == PingKind::Standard {
            p_name
        } else {
            format!("{} {}", kind.icon(), kind.label())
        };
        let label = commands
            .spawn((
                InGameEntity,
                PingLabel,
                Text::new(display_title),
                TextFont {
                    font_size: 14.0.into(),
                    ..default()
                },
                TextColor(color),
                TextLayout::justify(Justify::Center),
                Node {
                    position_type: PositionType::Absolute,
                    ..default()
                },
                Visibility::Hidden,
            ))
            .id();
        let mat = assets.materials[(kind as usize).min(7)].clone();
        commands
            .spawn((
                InGameEntity,
                Ping {
                    player,
                    kind,
                    pos: Vec3::from_array(pos),
                    target: enemy.then_some(target),
                    life: PING_LIFE,
                    label,
                },
                Transform::from_translation(Vec3::from_array(pos)),
                Visibility::default(),
            ))
            .with_children(|p| {
                p.spawn((
                    Mesh3d(assets.marker.clone()),
                    MeshMaterial3d(mat.clone()),
                    Transform::from_xyz(0.0, 0.6, 0.0),
                    NotShadowCaster,
                ));
                if !enemy {
                    p.spawn((
                        Mesh3d(assets.beam.clone()),
                        MeshMaterial3d(mat),
                        Transform::from_xyz(0.0, 0.0, 0.0).with_scale(Vec3::new(1.0, 1.0, 1.0)),
                        NotShadowCaster,
                    ));
                }
            });
    }
}

/// Keeps markers on their enemy, bobbing and big enough to see from afar,
/// with the pinger's name and the distance on screen.
#[allow(clippy::type_complexity)]
fn place_pings(
    mut commands: Commands,
    time: Res<Time>,
    camera: Single<(&Camera, &GlobalTransform), With<LocalPlayer>>,
    targets: Query<(&Replicated, &Transform), (Without<Ping>, Without<LocalPlayer>)>,
    mut pings: Query<(Entity, &mut Ping, &mut Transform, &Children)>,
    mut parts: Query<&mut Transform, (Without<Ping>, Without<Replicated>, Without<LocalPlayer>)>,
    mut labels: Query<(&mut Node, &mut Visibility, &mut Text, &ComputedNode), With<PingLabel>>,
    roster: Res<Roster>,
) {
    let dt = time.delta_secs();
    let t = time.elapsed_secs();
    let (cam, cam_tf) = *camera;
    for (e, mut ping, mut tf, children) in &mut pings {
        ping.life -= dt;
        if let Some(id) = ping.target {
            match targets.iter().find(|(r, _)| r.id == id) {
                Some((_, et)) => ping.pos = et.translation + Vec3::Y * 2.1 * et.scale.y,
                // The enemy died: the ping goes with it.
                None => ping.life = ping.life.min(0.3),
            }
        }
        if ping.life <= 0.0 {
            commands.entity(ping.label).despawn();
            commands.entity(e).despawn();
            continue;
        }
        tf.translation = ping.pos;
        let dist = cam_tf.translation().distance(ping.pos);
        // Stay a readable size at range.
        let size = (dist / 14.0).clamp(0.6, 4.0) * (ping.life / 0.3).min(1.0);
        for (i, child) in children.iter().enumerate() {
            if let Ok(mut ctf) = parts.get_mut(child) {
                if i == 0 {
                    ctf.translation.y = 0.5 * size + 0.12 * size * (t * 4.0).sin();
                    ctf.scale = Vec3::splat(size);
                    ctf.rotation = Quat::from_rotation_y(t * 2.0);
                } else {
                    ctf.scale = Vec3::new(size.min(2.0), 60.0, size.min(2.0));
                    ctf.translation.y = 30.0;
                }
            }
        }
        let Ok((mut node, mut vis, mut text, computed)) = labels.get_mut(ping.label) else {
            continue;
        };
        let head = ping.pos + Vec3::Y * 1.3 * size;
        let in_front = cam_tf.forward().dot(head - cam_tf.translation()) > 0.0;
        match cam.world_to_viewport(cam_tf, head) {
            Ok(screen) if in_front => {
                let name = roster.0.get(&ping.player).map_or("?", |p| p.name.as_str());
                let kind_tag = ping.kind.label();
                let label = if ping.kind == PingKind::Standard {
                    format!("{name}\n{dist:.0} m")
                } else {
                    format!("{kind_tag}\n{name} ({dist:.0}m)")
                };
                if text.0 != label {
                    text.0 = label;
                }
                let size = computed.size() * computed.inverse_scale_factor();
                node.left = Val::Px(screen.x - size.x / 2.0);
                node.top = Val::Px(screen.y - size.y);
                *vis = Visibility::Inherited;
            }
            _ => *vis = Visibility::Hidden,
        }
    }
}
