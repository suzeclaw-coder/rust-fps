//! The in-game HUD: crosshair, health, level and XP, points, ammo, ability
//! cooldowns, perks, prompts for the box and perk machines, power-up and
//! round banners, the boss bar, the teleporter, the scoreboard, the level-up
//! picker and the end screen.

/// Quiet text colour for the sandbox hint.
const DIM_HINT: Color = Color::srgba(1.0, 1.0, 1.0, 0.55);

use bevy::prelude::*;
use bevy::text::Justify;

use crate::config::{key_name, Action, Settings};
use crate::data::{
    boss_name, elements_in, gun_def, has_perk, skin_def, xp_to_next, Augment, Element, Perk,
    Stat, Upgrade, AMMO_COST, BOX_COST, MAX_AUGMENT, alt_fire,
    tier_name, AltFire, FINAL_BOSS_NAME, MAX_LEVEL, ROUNDS_PER_STAGE, STAGES,
};
use crate::sim::TELEPORT_HOLD;
use crate::game::{match_ended, MatchResult, Overlay};
use crate::maps::{map_name, CurrentMap};
use crate::ui::{button_sized, StationCard, UiAction, ACCENT, PANEL};
use crate::weapons::Loadout;
use crate::{AppState, BoxState, Enemy, InGameEntity, MatchState, Phase, Roster, Session};

const INTERACT_RANGE: f32 = 2.4;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn_hud)
            .add_systems(
                Update,
                (
                    update_hud,
                    update_hitmarker,
                    update_sights,
                    update_prompt,
                    update_banner,
                    update_scoreboard,
                    upgrade_panel,
                    end_screen,
                    boss_bar,
                )
                    .in_set(Phase::Present)
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

/// Which HUD text a node shows.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
enum HudText {
    Round,
    Info,
    Health,
    Level,
    Perks,
    Points,
    PointsDelta,
    Gun,
    Ammo,
    OtherGun,
    Ability(usize),
    Fps,
}

/// Which bar a node fills.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
enum HudFill {
    Health,
    GhostHealth,
    Xp,
    Ability(usize),
}

#[derive(Component)]
struct AbilityBox(usize);
#[derive(Component)]
struct Hitmarker;

/// One of the four slashes of the hit marker.
#[derive(Component)]
struct HitmarkerBar(usize);
/// Central red flash on hit.
#[derive(Component)]
struct HitmarkerCenter;
/// Lethal kill confirmation emblem/flash.
#[derive(Component)]
struct HitmarkerKill;
#[derive(Component)]
struct PromptText;
#[derive(Component)]
struct BannerText;
#[derive(Component)]
struct HurtFlash;
#[derive(Component)]
struct Scoreboard;
#[derive(Component)]
struct ScoreboardText;
#[derive(Component)]
struct UpgradePanel;
#[derive(Component)]
struct EndPanel;
/// The boss's name and health bar at the top of the screen.
#[derive(Component)]
struct BossBar;
#[derive(Component)]
struct BossName;
#[derive(Component)]
struct BossFill;

fn text(value: impl Into<String>, size: f32, color: Color) -> (Text, TextFont, TextColor) {
    (
        Text::new(value),
        TextFont {
            font_size: size.into(),
            ..default()
        },
        TextColor(color),
    )
}

/// Direction of each crosshair line for dynamic bloom.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
enum CrosshairLineDir {
    Top,
    Bottom,
    Left,
    Right,
    Center,
}

/// The four crosshair lines (hidden while aiming down sights).
#[derive(Component)]
struct CrosshairLine(CrosshairLineDir);

/// The black scope view shown while looking through a magnified scope.
#[derive(Component)]
struct ScopeOverlay;

/// A scope's view: clear in a circle with fine crosshairs and range marks,
/// black outside.
fn scope_image() -> Image {
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    const N: usize = 512;
    let mut data = vec![0u8; N * N * 4];
    let c = N as f32 / 2.0;
    for y in 0..N {
        for x in 0..N {
            let (dx, dy) = (x as f32 + 0.5 - c, y as f32 + 0.5 - c);
            let r = (dx * dx + dy * dy).sqrt() / c;
            // Dark outside the lens, with a soft shaded rim inside it.
            let mut alpha = ((r - 0.93) / 0.03).clamp(0.0, 1.0);
            alpha = alpha.max(((r - 0.75) / 0.18).clamp(0.0, 1.0).powi(3) * 0.55);
            // Thin crosshair lines, thicker posts towards the edge.
            let thick = if r > 0.55 { 3.0 } else { 0.9 };
            if dx.abs() < thick || dy.abs() < thick {
                alpha = alpha.max(0.95);
            }
            // Range marks along the lines.
            for k in 1..6 {
                let m = k as f32 * 0.08 * c;
                if (dx.abs() < 1.2 && (dy.abs() - m).abs() < 1.6 && dy.abs() < 0.5 * c)
                    || (dy.abs() < 1.2 && (dx.abs() - m).abs() < 1.6 && dx.abs() < 0.5 * c)
                {
                    alpha = alpha.max(0.9);
                }
            }
            let i = (y * N + x) * 4;
            data[i + 3] = (alpha * 255.0) as u8;
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

/// Hides the crosshair while aiming, shows scope overlay, and updates dynamic bloom.
fn update_sights(
    time: Res<Time>,
    aim: Res<crate::weapons::Aim>,
    loadout: Res<crate::weapons::Loadout>,
    player: Single<&crate::player::LocalPlayer>,
    mut lines: Query<(&CrosshairLine, &mut Node, &mut Visibility, &mut BackgroundColor), Without<ScopeOverlay>>,
    mut scope: Query<&mut Visibility, (With<ScopeOverlay>, Without<CrosshairLine>)>,
    mut bloom_smooth: Local<f32>,
) {
    // Smoothly fade out crosshair as sights come up; completely hidden by 35% ADS
    let ads_fade = (1.0 - (aim.amount / 0.35).min(1.0)).powi(2);
    let hide = ads_fade <= 0.002 || player.third_person();
    for (_, _, mut v, mut bg) in &mut lines {
        *v = if hide {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        if !hide {
            bg.0 = Color::srgba(1.0, 1.0, 1.0, 0.85 * ads_fade);
        }
    }
    for mut v in &mut scope {
        *v = if aim.scoped {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    // Dynamic bloom calculation:
    // Expand during continuous fire (spray/recoil) and player movement/jumping.
    let speed = player.horizontal_speed();
    let move_factor = if !player.on_ground {
        1.8 // jumping / airborne bloom
    } else if player.sprinting {
        1.3
    } else if speed > 1.0 {
        (speed / 6.0).min(1.0) * 0.7
    } else if player.crouching {
        -0.3 // tighter when crouched
    } else {
        0.0
    };

    // Continuous fire bloom from weapon recoil, spray counter, and continuous fire spread
    let fire_factor = (loadout.recoil * 1.5 + (loadout.spray as f32).min(12.0) * 0.15 + loadout.continuous_spread * 15.0).min(3.0);

    let target_bloom = (move_factor + fire_factor).clamp(0.0, 3.5);
    let dt = time.delta_secs();
    // Quick pop out on shot/jump, smooth precision recovery when stationary
    let recovery_speed = if target_bloom > *bloom_smooth { 22.0 } else { 12.0 };
    *bloom_smooth += (target_bloom - *bloom_smooth) * (1.0 - (-recovery_speed * dt).exp());

    let offset = 9.0 * 2.0 + *bloom_smooth * 12.0;

    for (CrosshairLine(dir), mut node, _, _) in &mut lines {
        match dir {
            CrosshairLineDir::Top => {
                node.margin.left = Val::Px(0.0);
                node.margin.top = Val::Px(-offset);
            }
            CrosshairLineDir::Bottom => {
                node.margin.left = Val::Px(0.0);
                node.margin.top = Val::Px(offset);
            }
            CrosshairLineDir::Left => {
                node.margin.left = Val::Px(-offset);
                node.margin.top = Val::Px(0.0);
            }
            CrosshairLineDir::Right => {
                node.margin.left = Val::Px(offset);
                node.margin.top = Val::Px(0.0);
            }
            CrosshairLineDir::Center => {
                node.margin.left = Val::Px(0.0);
                node.margin.top = Val::Px(0.0);
            }
        }
    }
}

fn spawn_hud(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let damage_arc = images.add(arc_image());
    // Scope view: black bars either side of a square lens image.
    let lens = images.add(scope_image());
    commands
        .spawn((
            InGameEntity,
            ScopeOverlay,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                ..default()
            },
            Visibility::Hidden,
            Pickable::IGNORE,
        ))
        .with_children(|o| {
            o.spawn((
                Node {
                    flex_grow: 1.0,
                    ..default()
                },
                BackgroundColor(Color::BLACK),
            ));
            o.spawn((
                Node {
                    height: Val::Percent(100.0),
                    aspect_ratio: Some(1.0),
                    ..default()
                },
                ImageNode::new(lens),
            ));
            o.spawn((
                Node {
                    flex_grow: 1.0,
                    ..default()
                },
                BackgroundColor(Color::BLACK),
            ));
        });
    let white = Color::WHITE;
    let dim = Color::srgb(0.75, 0.78, 0.85);

    // Red flash when you get hurt.
    commands.spawn((
        InGameEntity,
        HurtFlash,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.8, 0.0, 0.0, 0.0)),
        Pickable::IGNORE,
    ));

    // Crosshair and hit marker.
    commands
        .spawn((
            InGameEntity,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            Transform::default(),
            Pickable::IGNORE,
        ))
        .with_children(|c| {
            for (w, h, x, y, dir) in [
                (2.0, 7.0, 0.0, -9.0, CrosshairLineDir::Top),
                (2.0, 7.0, 0.0, 9.0, CrosshairLineDir::Bottom),
                (7.0, 2.0, -9.0, 0.0, CrosshairLineDir::Left),
                (7.0, 2.0, 9.0, 0.0, CrosshairLineDir::Right),
                (2.0, 2.0, 0.0, 0.0, CrosshairLineDir::Center),
            ] {
                c.spawn((
                    CrosshairLine(dir),
                    Node {
                        position_type: PositionType::Absolute,
                        width: Val::Px(w),
                        height: Val::Px(h),
                        margin: UiRect {
                            left: Val::Px(x * 2.0),
                            top: Val::Px(y * 2.0),
                            ..default()
                        },
                        ..default()
                    },
                    BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.85)),
                ));
            }
            // Hit marker: four sharp diagonal slashes around the crosshair + center flash + kill confirmation
            c.spawn((
                Hitmarker,
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(56.0),
                    height: Val::Px(56.0),
                    ..default()
                },
                Transform::default(),
                Visibility::Hidden,
            ))
            .with_children(|h| {
                // Center hit flash dot (flashes on body hit / crit)
                h.spawn((
                    HitmarkerCenter,
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(28.0 - 3.5),
                        top: Val::Px(28.0 - 3.5),
                        width: Val::Px(7.0),
                        height: Val::Px(7.0),
                        border_radius: BorderRadius::all(Val::Px(3.5)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.9, 0.1, 0.1, 0.0)),
                ));
                // Kill confirmation emblem (shows on lethal kill)
                h.spawn((
                    HitmarkerKill,
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(28.0 - 7.0),
                        top: Val::Px(28.0 - 7.0),
                        width: Val::Px(14.0),
                        height: Val::Px(14.0),
                        border: UiRect::all(Val::Px(2.0)),
                        border_radius: BorderRadius::all(Val::Px(2.0)),
                        ..default()
                    },
                    Transform::from_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_4)),
                    BorderColor::all(Color::srgba(1.0, 0.15, 0.1, 0.0)),
                    BackgroundColor(Color::srgba(0.85, 0.05, 0.05, 0.0)),
                    Visibility::Hidden,
                ));
                // Four sharp diagonal ticks
                for (i, (x, y, a)) in [
                    (-1.0, -1.0, 1.0),
                    (1.0, -1.0, -1.0),
                    (-1.0, 1.0, -1.0),
                    (1.0, 1.0, 1.0),
                ]
                .iter()
                .enumerate()
                {
                    h.spawn((
                        HitmarkerBar(i),
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(28.0 + x * 10.0 - 1.5),
                            top: Val::Px(28.0 + y * 10.0 - 6.0),
                            width: Val::Px(2.5),
                            height: Val::Px(12.0),
                            border_radius: BorderRadius::all(Val::Px(1.0)),
                            ..default()
                        },
                        Transform::from_rotation(Quat::from_rotation_z(
                            a * std::f32::consts::FRAC_PI_4,
                        )),
                        BackgroundColor(Color::WHITE),
                    ));
                }
            });
            for _ in 0..4 {
                c.spawn((
                    DamageArc { timer: 0.0, angle: 0.0 },
                    Node {
                        position_type: PositionType::Absolute,
                        width: Val::Px(256.0),
                        height: Val::Px(256.0),
                        ..default()
                    },
                    Transform::default(),
                    Visibility::Hidden,
                    ImageNode::new(damage_arc.clone()),
                ));
            }
        });

    // Top left: round and info.
    commands
        .spawn((
            InGameEntity,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(18.0),
                top: Val::Px(12.0),
                flex_direction: FlexDirection::Column,
                ..default()
            },
        ))
        .with_children(|c| {
            c.spawn((HudText::Round, text("", 52.0, Color::srgb(0.85, 0.12, 0.1))));
            c.spawn((HudText::Info, text("", 20.0, dim)));
        });

    // Top right: FPS counter and frametime (toggle with F3).
    commands
        .spawn((
            InGameEntity,
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(18.0),
                top: Val::Px(12.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::FlexEnd,
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|c| {
            c.spawn((
                HudText::Fps,
                text("... FPS", 16.0, Color::srgb(0.2, 0.9, 0.4)),
            ));
        });

    // Top centre: the boss's health bar (hidden until a boss is out).
    commands
        .spawn((
            InGameEntity,
            BossBar,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                top: Val::Px(10.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(4.0),
                ..default()
            },
            Visibility::Hidden,
            Pickable::IGNORE,
        ))
        .with_children(|c| {
            c.spawn((BossName, text("", 28.0, Color::srgb(1.0, 0.55, 0.4))));
            bar(c, 560.0, 18.0, Color::srgb(0.85, 0.18, 0.12), BossFill);
        });

    // Top centre banner and centre prompt.
    commands
        .spawn((
            InGameEntity,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                top: Val::Px(70.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|c| {
            c.spawn((
                BannerText,
                text("", 36.0, ACCENT),
                TextLayout::justify(Justify::Center),
            ));
        });
    commands
        .spawn((
            InGameEntity,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                top: Val::Percent(58.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|c| {
            c.spawn((
                PromptText,
                text("", 24.0, white),
                TextLayout::justify(Justify::Center),
            ));
        });

    // Bottom left: health, level, perks.
    commands
        .spawn((
            InGameEntity,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(20.0),
                bottom: Val::Px(224.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(4.0),
                ..default()
            },
        ))
        .with_children(|c| {
            c.spawn((HudText::Perks, text("", 17.0, white)));
            c.spawn((HudText::Level, text("", 18.0, dim)));
            bar(c, 260.0, 7.0, Color::srgb(0.45, 0.7, 1.0), HudFill::Xp);
            c.spawn((HudText::Health, text("", 22.0, white)));
            c.spawn((
                Node {
                    width: Val::Px(260.0),
                    height: Val::Px(16.0),
                    border_radius: BorderRadius::all(Val::Px(3.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
            )).with_children(|b| {
                b.spawn((
                    HudFill::GhostHealth,
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        border_radius: BorderRadius::all(Val::Px(3.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(1.0, 0.9, 0.5)),
                ));
                b.spawn((
                    HudFill::Health,
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        position_type: PositionType::Absolute,
                        border_radius: BorderRadius::all(Val::Px(3.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.25, 0.85, 0.35)),
                ));
                // 50% Auto-Regen Threshold Notch
                b.spawn((
                    Node {
                        width: Val::Px(2.0),
                        height: Val::Percent(100.0),
                        position_type: PositionType::Absolute,
                        left: Val::Percent(50.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.45)),
                ));
            });
        });

    // Bottom centre: abilities.
    commands
        .spawn((
            InGameEntity,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                bottom: Val::Px(16.0),
                justify_content: JustifyContent::Center,
                column_gap: Val::Px(10.0),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|c| {
            // Class abilities: 0 = Q, 1 = E, 2 = R
            for i in [0, 1, 2] {
                c.spawn((
                    AbilityBox(i),
                    Node {
                        width: Val::Px(160.0),
                        height: Val::Px(50.0),
                        border: UiRect::all(Val::Px(2.0)),
                        border_radius: BorderRadius::all(Val::Px(6.0)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        overflow: Overflow::clip(),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.05, 0.06, 0.1, 0.75)),
                    BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.3)),
                ))
                .with_children(|b| {
                    b.spawn((
                        HudFill::Ability(i),
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(0.0),
                            bottom: Val::Px(0.0),
                            width: Val::Percent(100.0),
                            height: Val::Percent(0.0),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.3, 0.6, 1.0, 0.35)),
                    ));
                    b.spawn((
                        HudText::Ability(i),
                        text("", 15.0, white),
                        TextLayout::justify(Justify::Center),
                    ));
                });
            }
        });

    // Bottom right: points and guns.
    commands
        .spawn((
            InGameEntity,
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(18.0),
                bottom: Val::Px(16.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::FlexEnd,
                row_gap: Val::Px(2.0),
                ..default()
            },
        ))
        .with_children(|c| {
            c.spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Baseline,
                column_gap: Val::Px(8.0),
                ..default()
            })
            .with_children(|r| {
                r.spawn((
                    HudText::PointsDelta,
                    text("", 20.0, Color::srgba(1.0, 0.92, 0.35, 0.0)),
                ));
                r.spawn((HudText::Points, text("", 36.0, Color::srgb(1.0, 0.85, 0.3))));
            });
            c.spawn((HudText::OtherGun, text("", 18.0, dim)));
            c.spawn((HudText::Gun, text("", 22.0, white)));
            c.spawn((HudText::Ammo, text("", 42.0, white)));
        });

    // Scoreboard (hold Tab).
    commands
        .spawn((
            InGameEntity,
            Scoreboard,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                top: Val::Px(120.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
            Visibility::Hidden,
        ))
        .with_children(|c| {
            c.spawn((
                Node {
                    padding: UiRect::all(Val::Px(16.0)),
                    border_radius: BorderRadius::all(Val::Px(8.0)),
                    ..default()
                },
                BackgroundColor(PANEL),
            ))
            .with_children(|p| {
                p.spawn((ScoreboardText, text("", 16.0, white)));
            });
        });
}

fn bar(
    c: &mut ChildSpawnerCommands,
    width: f32,
    height: f32,
    color: Color,
    marker: impl Component,
) {
    c.spawn((
        Node {
            width: Val::Px(width),
            height: Val::Px(height),
            border_radius: BorderRadius::all(Val::Px(3.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
    ))
    .with_children(|b| {
        b.spawn((
            marker,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                border_radius: BorderRadius::all(Val::Px(3.0)),
                ..default()
            },
            BackgroundColor(color),
        ));
    });
}

fn set(text: &mut Text, value: String) {
    if text.0 != value {
        text.0 = value;
    }
}

fn set_color(tc: &mut TextColor, color: Color) {
    if tc.0 != color {
        tc.0 = color;
    }
}

#[derive(Default)]
struct HudAnimState {
    last_health: f32,
    ghost_health: f32,
    ghost_timer: f32,
    flash: f32,
    bar_flash: f32,
    last_points: u32,
    points_flash: f32,
    points_gain: i32,
    last_level: u32,
    level_flash: f32,
    ready_prev: [bool; 5],
    ready_flash: [f32; 5],
    was_low_ammo: bool,
    fps_smoothed: f32,
    show_fps: bool,
    fps_initialized: bool,
}

#[allow(clippy::too_many_arguments)]
fn tally_marks(mut n: u32) -> String {
    let mut s = String::new();
    while n >= 5 {
        s.push_str("I̸I̸I̸I̸ ");
        n -= 5;
    }
    for _ in 0..n {
        s.push('I');
    }
    s
}

#[allow(clippy::too_many_arguments)]
fn update_hud(
    time: Res<Time>,
    session: Res<Session>,
    roster: Res<Roster>,
    state: Res<MatchState>,
    loadout: Res<Loadout>,
    settings: Res<Settings>,
    keyboard: Res<ButtonInput<KeyCode>>,
    enemies: Query<&Transform, (With<Enemy>, Without<DamageArc>)>,
    mut texts: Query<(&HudText, &mut Text, &mut TextColor)>,
    mut fills: Query<
        (&HudFill, &mut Node, &mut BackgroundColor),
        (With<HudFill>, Without<AbilityBox>, Without<HurtFlash>),
    >,
    mut ability_box: Query<
        (&AbilityBox, &mut BorderColor, &mut BackgroundColor),
        (With<AbilityBox>, Without<HudFill>, Without<HurtFlash>),
    >,
    mut hurt: Single<
        &mut BackgroundColor,
        (With<HurtFlash>, Without<HudFill>, Without<AbilityBox>),
    >,
    mut anim: Local<HudAnimState>,
    mut sounds: ResMut<crate::audio::SoundQueue>,
    mut damage_arcs: Query<
        (&mut DamageArc, &mut Transform, &mut Visibility, &mut ImageNode),
        (With<DamageArc>, Without<Enemy>),
    >,
) {
    if !anim.fps_initialized {
        anim.show_fps = true;
        anim.fps_smoothed = 60.0;
        anim.fps_initialized = true;
    }
    if keyboard.just_pressed(KeyCode::F3) {
        anim.show_fps = !anim.show_fps;
    }
    let dt = time.delta_secs();
    if dt > 0.0001 {
        let current_fps = 1.0 / dt;
        anim.fps_smoothed = anim.fps_smoothed * 0.92 + current_fps * 0.08;
    }

    let Some(me) = roster.me(&session) else {
        return;
    };
    let enemy_count = enemies.iter().count() as u32;
    let max = me.max_health();
    let abilities = me.kit;
    let keys = [
        Action::Ability1,
        Action::Ability2,
        Action::Ultimate,
        Action::WeaponAbility1,
        Action::WeaponAbility2,
    ];
    let ready = |i: usize| {
        if i >= me.kit.len() || me.kit[i].is_none() {
            return false;
        }
        match i {
            2 => me.ult_charge >= 100.0,
            _ => me.charges[i] > 0,
        }
    };

    let white = Color::WHITE;
    let dim = Color::srgb(0.75, 0.78, 0.85);

    // Track level changes for XP shimmer/flash
    if anim.last_level == 0 && me.level > 0 {
        anim.last_level = me.level;
    } else if me.level > anim.last_level {
        anim.level_flash = 1.6;
        anim.last_level = me.level;
    } else if me.level < anim.last_level {
        anim.last_level = me.level;
    }
    if anim.level_flash > 0.0 {
        anim.level_flash = (anim.level_flash - time.delta_secs()).max(0.0);
    }

    // Track points changes for gold flash and delta popup
    if anim.last_points == 0 && me.points > 0 {
        anim.last_points = me.points;
    } else if me.points > anim.last_points {
        let diff = (me.points - anim.last_points) as i32;
        anim.points_gain += diff;
        anim.points_flash = 0.65;
        anim.last_points = me.points;
    } else if me.points < anim.last_points {
        anim.last_points = me.points;
    }
    if anim.points_flash > 0.0 {
        anim.points_flash = (anim.points_flash - time.delta_secs()).max(0.0);
        if anim.points_flash <= 0.0 {
            anim.points_gain = 0;
        }
    }

    // Track ability readiness transitions for border glow
    for i in 0..3 {
        let is_ready = ready(i);
        if is_ready && !anim.ready_prev[i] {
            anim.ready_flash[i] = 0.75;
        }
        anim.ready_prev[i] = is_ready;
        if anim.ready_flash[i] > 0.0 {
            anim.ready_flash[i] = (anim.ready_flash[i] - time.delta_secs()).max(0.0);
        }
    }

    for (kind, mut t, mut tc) in &mut texts {
        let (value, col) = match *kind {
            HudText::Round => {
                let v = if state.round == 0 {
                    String::new()
                } else {
                    format!("{}  {}", tally_marks(state.round), state.round)
                };
                (v, Color::srgb(0.85, 0.12, 0.1))
            }
            HudText::Info => {
                let mut info = map_name(state.map).to_string();
                if !state.sandbox.on {
                    info += &format!("  -  map {} of {}", state.stage + 1, STAGES);
                    if state.teleport {
                        info += &format!("\nBoss down: on to map {}", state.stage + 2);
                    } else if state.stage_round > ROUNDS_PER_STAGE {
                        info += "\n☠ BOSS ROUND ☠";
                    } else if state.stage_round > 0 {
                        info += &format!(
                            "\nRound {} of {}, then the boss",
                            state.stage_round, ROUNDS_PER_STAGE
                        );
                    }
                }
                if state.round > 0 && !state.teleport {
                    info += &format!("\nEnemies left: {}", enemy_count + state.to_spawn);
                }
                if state.insta_kill > 0.0 {
                    info += &format!("\n⚡ INSTA-KILL: {:.0}s", state.insta_kill);
                }
                if state.double_points > 0.0 {
                    info += &format!("\n💰 DOUBLE POINTS: {:.0}s", state.double_points);
                }
                if me.chain > 0.0 {
                    info += &format!("\n🔗 CHAIN REACTION: {:.0}s", me.chain);
                }
                if me.guard > 0.0 {
                    info += &format!("\n🛡 GUARD {:.0}%: {:.0}s", me.guard_cut * 100.0, me.guard);
                }
                if !session.status.is_empty() && session.role == crate::Role::Host {
                    info += &format!("\n{}", session.status);
                }
                (info, dim)
            }
            HudText::Health => {
                let pct = (me.health / max).clamp(0.0, 1.0);
                let v = format!("{}  {:.0} / {:.0}", me.name, me.health.max(0.0), max);
                let c = if anim.bar_flash > 0.0 {
                    Color::srgb(1.0, 0.95, 0.95)
                } else if pct > 0.6 {
                    white
                } else if pct >= 0.3 {
                    Color::srgb(1.0, 0.75, 0.3)
                } else {
                    let pulse = (time.elapsed_secs() * 8.0).sin() * 0.5 + 0.5;
                    Color::srgb(1.0, 0.2 + 0.2 * pulse, 0.2 + 0.2 * pulse)
                };
                (v, c)
            }
            HudText::Level => {
                if anim.level_flash > 0.0 {
                    (
                        format!("★ LEVEL UP! LEVEL {} ★", me.level),
                        Color::srgb(1.0, 0.90, 0.3),
                    )
                } else if me.level >= MAX_LEVEL {
                    (format!("Level {} (max)", me.level), dim)
                } else {
                    (
                        format!(
                            "Level {}  -  {} / {} XP",
                            me.level,
                            me.xp,
                            xp_to_next(me.level)
                        ),
                        dim,
                    )
                }
            }
            HudText::Perks => {
                let perks: Vec<&str> = Perk::ALL
                    .iter()
                    .filter(|p| has_perk(me.perks, **p))
                    .map(|p| p.name())
                    .collect();
                let mut lines = Vec::new();
                if !perks.is_empty() {
                    lines.push(format!("Perks: {}", perks.join(", ")));
                }
                let guns: Vec<&str> = elements_in(me.gun_elements).map(|e| e.name()).collect();
                if !guns.is_empty() {
                    lines.push(format!("Gun elements: {}", guns.join(", ")));
                }
                let abil: Vec<&str> = elements_in(me.ability_elements).map(|e| e.name()).collect();
                if !abil.is_empty() {
                    lines.push(format!("Ability elements: {}", abil.join(", ")));
                }
                (lines.join("\n"), white)
            }
            HudText::Points => {
                let v = format!("{} pts", me.points);
                let c = if anim.points_flash > 0.0 {
                    let t = (anim.points_flash / 0.65).clamp(0.0, 1.0);
                    Color::srgb(1.0, 0.85, 0.3).mix(&Color::srgb(1.0, 1.0, 0.9), t)
                } else {
                    Color::srgb(1.0, 0.85, 0.3)
                };
                (v, c)
            }
            HudText::PointsDelta => {
                if anim.points_flash > 0.0 && anim.points_gain > 0 {
                    let t = (anim.points_flash / 0.65).clamp(0.0, 1.0);
                    (
                        format!("+{}", anim.points_gain),
                        Color::srgba(1.0, 0.92, 0.35, t.powf(0.5)),
                    )
                } else {
                    (String::new(), Color::srgba(0.0, 0.0, 0.0, 0.0))
                }
            }
            HudText::Gun => {
                let buff_str = me
                    .buff
                    .filter(|_| me.buff_time > 0.0)
                    .map(|b| format!("\nBUFF: {} ({:.1}s)", b.name(), me.buff_time))
                    .unwrap_or_default();
                let v = loadout
                    .current()
                    .map(|g| {
                        let parts = g.attach.names();
                        let extra = if parts.is_empty() {
                            String::new()
                        } else {
                            format!("\n{}", parts.join(" + "))
                        };
                        let alt = alt_fire(g.id);
                        let alt_line = match alt {
                            AltFire::Grenade => {
                                let cd = loadout.grenade_cd.max(me.grenade_cd);
                                if cd > 0.0 {
                                    format!("\n[RMB] {} ({cd:.1}s)", alt.describe())
                                } else {
                                    format!("\n[RMB] {} (ready)", alt.describe())
                                }
                            }
                            a => format!("\n[RMB] {}", a.describe()),
                        };
                        format!(
                            "{}{}  ({}){extra}{alt_line}{buff_str}",
                            gun_def(g.id).name,
                            tier_name(g.tier),
                            skin_def(me.skin_for(g.id)).name
                        )
                    })
                    .unwrap_or_default();
                (v, white)
            }
            HudText::Ammo => {
                let mut is_low = false;
                let v = match loadout.current() {
                    Some(_) if me.gun_buff().free_ammo => "INFINITE".to_string(),
                    Some(_) if loadout.reload > 0.0 => "Reloading...".to_string(),
                    Some(g) => {
                        let max_mag = crate::data::gun_def(g.id).mag as f32;
                        if g.mag as f32 <= max_mag * 0.25 {
                            is_low = true;
                        }
                        format!("{} / {}", g.mag, g.reserve)
                    },
                    None => String::new(),
                };
                if is_low && !anim.was_low_ammo {
                    sounds.here(crate::audio::Snd::DryFire);
                }
                anim.was_low_ammo = is_low;
                let c = match loadout.current() {
                    Some(_) if me.gun_buff().free_ammo => Color::srgb(0.3, 0.9, 1.0),
                    Some(_) if is_low => {
                        let pulse = (time.elapsed_secs() * 10.0).sin() * 0.5 + 0.5;
                        Color::srgb(1.0, 0.75, 0.1).mix(&Color::srgb(1.0, 0.1, 0.1), pulse)
                    },
                    Some(g) if g.mag <= 3 => Color::srgb(1.0, 0.3, 0.25),
                    _ => white,
                };
                (v, c)
            }
            HudText::OtherGun => {
                let v = loadout.slots[1 - loadout.active]
                    .map(|g| {
                        format!(
                            "[{}] {}",
                            key_name(settings.key(Action::SwapWeapon)),
                            gun_def(g.id).name
                        )
                    })
                    .unwrap_or_default();
                (v, dim)
            }
            HudText::Ability(i) => {
                if let Some(ability) = abilities.get(i).copied().flatten() {
                    let (status, col) = if i == 2 {
                        if ready(2) {
                            ("READY".to_string(), Color::srgb(0.9, 1.0, 0.95))
                        } else {
                            (
                                format!("{:.0}%", me.ult_charge),
                                Color::srgb(0.75, 0.78, 0.85),
                            )
                        }
                    } else if me.max_charges(i) > 1 {
                        let mut t = format!("{}/{}", me.charges[i], me.max_charges(i));
                        if me.cooldowns[i] > 0.0 {
                            t += &format!("  {:.1}s", me.cooldowns[i]);
                        }
                        let c = if ready(i) {
                            Color::srgb(0.9, 1.0, 0.95)
                        } else {
                            Color::srgb(0.75, 0.78, 0.85)
                        };
                        (t, c)
                    } else if ready(i) {
                        ("READY".to_string(), Color::srgb(0.9, 1.0, 0.95))
                    } else {
                        (
                            format!("{:.1}s", me.cooldowns[i]),
                            Color::srgb(0.75, 0.78, 0.85),
                        )
                    };
                    let roman = ["I", "II", "III", "IV", "V", "VI"][(me.tiers[i] as usize).min(5)];
                    let copies = me.copies(i);
                    let many = if copies > 1 { format!(" x{copies}") } else { String::new() };
                    let key_str = if i < keys.len() {
                        key_name(settings.key(keys[i]))
                    } else {
                        "?".into()
                    };
                    (
                        format!(
                            "{} {roman}\n[{}] {}{many}",
                            ability.name(),
                            key_str,
                            status
                        ),
                        col,
                    )
                } else {
                    let key_str = if i < keys.len() {
                        key_name(settings.key(keys[i]))
                    } else {
                        "?".into()
                    };
                    (
                        format!("[{}]\nEMPTY", key_str),
                        Color::srgba(0.6, 0.6, 0.6, 0.5),
                    )
                }
            }
            HudText::Fps => {
                if !anim.show_fps {
                    (String::new(), Color::NONE)
                } else {
                    let fps = anim.fps_smoothed;
                    let ms = if fps > 0.0 { 1000.0 / fps } else { 0.0 };
                    let col = if fps >= 55.0 {
                        Color::srgb(0.2, 0.9, 0.4)
                    } else if fps >= 30.0 {
                        Color::srgb(1.0, 0.8, 0.2)
                    } else {
                        Color::srgb(1.0, 0.25, 0.2)
                    };
                    (format!("{:.0} FPS ({:.1} ms) [F3]", fps, ms), col)
                }
            }
        };
        set(&mut t, value);
        set_color(&mut tc, col);
    }

    for (kind, mut node, mut bg) in &mut fills {
        match *kind {
            HudFill::Health => {
                let pct = (me.health / max).clamp(0.0, 1.0);
                node.width = Val::Percent(pct * 100.0);

                let emerald = Color::srgb(0.20, 0.85, 0.38);
                let amber = Color::srgb(0.95, 0.65, 0.15);
                let crimson_base = Color::srgb(0.95, 0.12, 0.12);
                let crimson_dark = Color::srgb(0.55, 0.06, 0.06);

                let mut col = if pct > 0.6 {
                    let t = (pct - 0.6) / 0.4;
                    amber.mix(&emerald, t)
                } else if pct >= 0.3 {
                    let t = (pct - 0.3) / 0.3;
                    crimson_base.mix(&amber, t)
                } else {
                    let pulse = (time.elapsed_secs() * 8.0).sin() * 0.5 + 0.5;
                    crimson_dark.mix(&crimson_base, pulse)
                };

                if anim.bar_flash > 0.0 {
                    let flash_t = (anim.bar_flash / 0.30).clamp(0.0, 1.0);
                    col = col.mix(&Color::srgb(1.0, 0.95, 0.95), flash_t * 0.75);
                }
                bg.0 = col;
            }
            HudFill::GhostHealth => {
                let pct = (anim.ghost_health / max).clamp(0.0, 1.0);
                node.width = Val::Percent(pct * 100.0);
                bg.0 = Color::srgb(1.0, 0.95, 0.5);
            }
            HudFill::Xp => {
                let pct = if me.level >= MAX_LEVEL {
                    100.0
                } else {
                    (me.xp as f32 / xp_to_next(me.level) as f32 * 100.0).clamp(0.0, 100.0)
                };
                node.width = Val::Percent(pct);
                if anim.level_flash > 0.0 {
                    let shimmer = (time.elapsed_secs() * 14.0).sin() * 0.5 + 0.5;
                    let gold = Color::srgb(1.0, 0.85, 0.2);
                    let white = Color::srgb(1.0, 1.0, 0.95);
                    bg.0 = gold.mix(&white, shimmer);
                } else {
                    bg.0 = Color::srgb(0.32, 0.65, 1.0);
                }
            }
            HudFill::Ability(i) => {
                let frac = if i < me.kit.len() && me.kit[i].is_some() {
                    if i == 2 {
                        me.ult_charge / 100.0
                    } else if me.cooldowns[i] <= 0.0 {
                        1.0
                    } else {
                        1.0 - me.cooldowns[i] / me.ability_cooldown(i)
                    }
                } else {
                    0.0
                };
                node.height = Val::Percent(frac.clamp(0.0, 1.0) * 100.0);
                bg.0 = if ready(i) {
                    Color::srgba(0.2, 0.9, 0.45, 0.30)
                } else {
                    Color::srgba(0.25, 0.55, 0.9, 0.30)
                };
            }
        }
    }

    for (AbilityBox(i), mut border, mut box_bg) in &mut ability_box {
        let is_ready = ready(*i);
        let ability_opt = me.kit.get(*i).copied().flatten();
        if let Some(a) = ability_opt {
            if is_ready {
                let flash_t = (anim.ready_flash.get(*i).copied().unwrap_or(0.0) / 0.75).clamp(0.0, 1.0);
                let base_glow = a.color();
                if flash_t > 0.0 {
                    let glow = base_glow.mix(&Color::WHITE, flash_t);
                    let srgba = glow.to_srgba();
                    border.set_all(Color::srgba(srgba.red, srgba.green, srgba.blue, 0.95));
                    box_bg.0 = Color::srgba(0.06, 0.14, 0.10, 0.90);
                } else {
                    let breathe = (time.elapsed_secs() * 3.0).sin() * 0.5 + 0.5;
                    let srgba = base_glow.to_srgba();
                    border.set_all(Color::srgba(srgba.red, srgba.green, srgba.blue, 0.75 + 0.20 * breathe));
                    box_bg.0 = Color::srgba(0.04, 0.08, 0.06, 0.80);
                }
            } else {
                border.set_all(Color::srgba(0.35, 0.40, 0.50, 0.35));
                box_bg.0 = Color::srgba(0.03, 0.04, 0.06, 0.85);
            }
        } else {
            // Empty slot: dark/dimmed
            border.set_all(Color::srgba(0.2, 0.2, 0.25, 0.25));
            box_bg.0 = Color::srgba(0.02, 0.02, 0.03, 0.75);
        }
    }

    if anim.last_health == 0.0 && me.health > 0.0 {
        anim.last_health = me.health;
        anim.ghost_health = me.health;
    } else if me.health > anim.last_health + 0.5 {
        anim.ghost_health = me.health;
    }
    if me.health < anim.last_health - 0.5 {
        anim.flash = 0.35;
        anim.bar_flash = 0.30;
        anim.ghost_timer = 0.4;

        let player_pos = Vec3::from_array(me.pos);
        if let Some(t) = enemies.iter().min_by_key(|t| (t.translation.distance_squared(player_pos) * 1000.0) as i32) {
            let dx = t.translation.x - player_pos.x;
            let dz = t.translation.z - player_pos.z;
            let world_angle = dx.atan2(-dz); 
            let rel_angle = world_angle - me.yaw;
            if let Some((mut arc, mut tf, _, _)) = damage_arcs.iter_mut().min_by_key(|(a, _, _, _)| (a.timer * 1000.0) as i32) {
                arc.timer = 1.5;
                arc.angle = rel_angle;
                tf.rotation = Quat::from_rotation_z(-rel_angle);
            }
        }
    }
    anim.last_health = me.health;
    anim.flash = (anim.flash - time.delta_secs()).max(0.0);
    anim.bar_flash = (anim.bar_flash - time.delta_secs()).max(0.0);

    if anim.ghost_timer > 0.0 {
        anim.ghost_timer = (anim.ghost_timer - time.delta_secs()).max(0.0);
    } else if anim.ghost_health > me.health {
        anim.ghost_health = (anim.ghost_health - 150.0 * time.delta_secs()).max(me.health);
    } else {
        anim.ghost_health = me.health;
    }

    for (mut arc, _tf, mut vis, mut img) in &mut damage_arcs {
        if arc.timer > 0.0 {
            arc.timer = (arc.timer - time.delta_secs()).max(0.0);
            *vis = Visibility::Inherited;
            let alpha = (arc.timer / 1.5).min(1.0);
            img.color = Color::srgba(1.0, 1.0, 1.0, alpha);
        } else {
            *vis = Visibility::Hidden;
        }
    }

    let downed = if me.alive { 0.0 } else { 0.25 };
    hurt.0 = Color::srgba(0.8, 0.0, 0.0, anim.flash.max(downed));
}

/// CoD Zombies Hitmarker:
/// 4 sharp diagonal ticks snapping inward/outward with intense visual feedback.
/// Body hit: crisp white/silver marker with red center flash.
/// Headshot / Critical hit: bold crimson-amber marker with distinct enlarged ticks.
/// Lethal kill: distinct red skull/kill confirmation flash.
fn update_hitmarker(
    loadout: Res<Loadout>,
    hitmarker: Single<(&mut Visibility, &mut Transform), (With<Hitmarker>, Without<HitmarkerKill>)>,
    mut hitmarker_bars: Query<
        (&HitmarkerBar, &mut BackgroundColor, &mut Node),
        (Without<HurtFlash>, Without<HudFill>, Without<HitmarkerCenter>, Without<HitmarkerKill>, Without<Hitmarker>),
    >,
    mut hitmarker_center: Query<
        &mut BackgroundColor,
        (With<HitmarkerCenter>, Without<HitmarkerBar>, Without<HitmarkerKill>, Without<HurtFlash>, Without<HudFill>, Without<Hitmarker>),
    >,
    mut hitmarker_kill: Query<
        (&mut BackgroundColor, &mut BorderColor, &mut Visibility),
        (With<HitmarkerKill>, Without<Hitmarker>, Without<HitmarkerBar>, Without<HitmarkerCenter>, Without<HurtFlash>, Without<HudFill>),
    >,
) {
    let (mut vis, mut tf) = hitmarker.into_inner();
    let kill = loadout.kill_marker > 0.0;
    let showing = loadout.hitmarker > 0.0 || kill;
    *vis = if showing {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };

    let hit_t = (loadout.hitmarker / 0.14).clamp(0.0, 1.0);
    let head_t = (loadout.hitmarker / 0.20).clamp(0.0, 1.0);
    let kill_t = (loadout.kill_marker / 0.35).clamp(0.0, 1.0);

    let pop = if kill {
        1.0 + kill_t * 1.5
    } else if loadout.headshot {
        1.15 + head_t * 2.2
    } else {
        1.0 + hit_t * 1.6
    };
    tf.scale = Vec3::splat(pop);

    let (color, tick_w, tick_h, base_dist, snap) = if kill {
        (
            Color::srgb(1.0, 0.08, 0.08),
            3.8,
            16.0,
            13.0,
            kill_t.powf(0.5) * 6.0,
        )
    } else if loadout.headshot {
        (
            Color::srgb(1.0, 0.45, 0.12),
            3.5,
            16.0,
            12.0,
            head_t.powf(0.5) * 5.0,
        )
    } else {
        (
            Color::srgba(0.95, 0.96, 1.0, 0.95),
            2.5,
            12.0,
            9.5,
            hit_t.powf(0.5) * 3.5,
        )
    };

    let dist = base_dist + snap;
    for (HitmarkerBar(i), mut bg, mut node) in &mut hitmarker_bars {
        bg.0 = color;
        let (x, y) = match *i {
            0 => (-1.0, -1.0),
            1 => (1.0, -1.0),
            2 => (-1.0, 1.0),
            _ => (1.0, 1.0),
        };
        node.left = Val::Px(28.0 + x * dist - tick_w * 0.5);
        node.top = Val::Px(28.0 + y * dist - tick_h * 0.5);
        node.width = Val::Px(tick_w);
        node.height = Val::Px(tick_h);
    }

    // Red center hit flash dot
    for mut bg in &mut hitmarker_center {
        bg.0 = if kill {
            Color::srgba(1.0, 0.05, 0.05, kill_t)
        } else if loadout.headshot {
            Color::srgba(1.0, 0.38, 0.08, head_t * 0.95)
        } else if showing {
            Color::srgba(0.95, 0.10, 0.10, hit_t * 0.9)
        } else {
            Color::srgba(0.0, 0.0, 0.0, 0.0)
        };
    }

    // Lethal kill confirmation emblem
    for (mut bg, mut border, mut k_vis) in &mut hitmarker_kill {
        *k_vis = if kill {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        bg.0 = Color::srgba(0.85, 0.04, 0.04, kill_t * 0.55);
        border.set_all(Color::srgba(1.0, 0.15, 0.1, kill_t * 0.95));
    }
}

fn update_prompt(
    session: Res<Session>,
    roster: Res<Roster>,
    state: Res<MatchState>,
    settings: Res<Settings>,
    map: Option<Res<CurrentMap>>,
    overlay: Res<Overlay>,
    cursor: Single<&bevy::window::CursorOptions, With<bevy::window::PrimaryWindow>>,
    mut prompt: Single<&mut Text, With<PromptText>>,
) {
    let (Some(me), Some(map)) = (roster.me(&session), map) else {
        return;
    };
    let key = key_name(settings.key(Action::Interact));
    let mut msg = String::new();
    if match_ended(&state) {
        // The end screen says it all.
    } else if *overlay == Overlay::None && !crate::cursor_locked(&cursor) {
        msg = if session.role == crate::Role::Solo {
            "Paused - click to play".into()
        } else {
            "Click to play".into()
        };
    } else if !me.alive {
        msg = "You are down! You'll get back up next round if a teammate survives.".into();
    } else {
        let feet = me.feet().with_y(0.0);
        let box_pos = map.0.box_spots[(state.box_spot as usize).min(4)];
        if feet.distance(box_pos) < INTERACT_RANGE {
            msg = match state.box_state {
                BoxState::Idle => {
                    if me.points >= BOX_COST {
                        format!("[{key}] Armory - new attachments for the gun in your hands ({BOX_COST} pts)")
                    } else {
                        format!("Armory - need {BOX_COST} pts")
                    }
                }
                BoxState::Rolling { player, .. } => {
                    if player == me.id {
                        "Rolling...".into()
                    } else {
                        "Someone is using the Armory".into()
                    }
                }
                BoxState::Offer {
                    player,
                    gun,
                    attach,
                    ..
                } => {
                    let def = gun_def(gun);
                    let rare = if def.rare { "RARE! " } else { "" };
                    let parts = attach.names();
                    let kit = if parts.is_empty() {
                        String::new()
                    } else {
                        format!("\nwith {}", parts.join(", "))
                    };
                    let held = me.guns.contains(&Some(gun));
                    match (player == me.id, held) {
                        (true, true) => format!(
                            "[{key}] Fit to your {}{kit}",
                            def.name
                        ),
                        (true, false) => {
                            let cur = me.guns[me.active_slot as usize]
                                .map(|g| gun_def(g).name)
                                .unwrap_or("");
                            format!("[{key}] Take {rare}{} (replacing your {cur}){kit}", def.name)
                        }
                        _ => format!("{rare}{} - not yours{kit}", def.name),
                    }
                }
                BoxState::Moving { .. } => "The Armory moved! Find where it went.".into(),
            };
        }
        for (spot, perk) in map.0.perk_spots.iter().zip(Perk::ALL) {
            if feet.distance(*spot) < INTERACT_RANGE {
                msg = if has_perk(me.perks, perk) {
                    format!("{} - you already have it", perk.name())
                } else if me.points >= perk.cost() {
                    format!(
                        "[{key}] Buy {} ({} pts)\n{}",
                        perk.name(),
                        perk.cost(),
                        perk.description()
                    )
                } else {
                    format!(
                        "{} - need {} pts\n{}",
                        perk.name(),
                        perk.cost(),
                        perk.description()
                    )
                };
            }
        }
        let feet3 = me.feet();
        if map.0.wall_buys.iter().any(|w| w.near(feet3)) {
            msg = if me.points >= AMMO_COST {
                format!("[{key}] Ammo cache - refill both guns ({AMMO_COST} pts)")
            } else {
                format!("Ammo cache - need {AMMO_COST} pts")
            };
        }
        if map.0.near_upgrade_station(feet3) {
            msg = format!("[{key}] Upgrade Station - Upgrade weapons, stats & abilities");
        }
    }
    set(&mut prompt, msg);
}

fn update_banner(
    time: Res<Time>,
    state: Res<MatchState>,
    session: Res<Session>,
    roster: Res<Roster>,
    map: Option<Res<CurrentMap>>,
    banner: Single<(&mut Text, &mut TextColor), With<BannerText>>,
    mut last_seq: Local<Option<u32>>,
    mut show: Local<f32>,
    mut last_round: Local<u32>,
    mut round_banner: Local<f32>,
) {
    let (mut text, mut color) = banner.into_inner();
    if *last_seq != Some(state.powerup_seq) {
        if last_seq.is_some() && state.last_powerup.is_some() {
            *show = 2.8;
        }
        *last_seq = Some(state.powerup_seq);
    }
    *show -= time.delta_secs();

    // Round banner tracking
    if *last_round == 0 && state.round > 0 {
        *last_round = state.round;
    } else if state.round > *last_round {
        *round_banner = 2.8;
        *last_round = state.round;
    } else if state.round < *last_round {
        *last_round = state.round;
    }
    if *round_banner > 0.0 {
        *round_banner = (*round_banner - time.delta_secs()).max(0.0);
    }

    if match_ended(&state) {
        set(&mut text, String::new());
        return;
    }
    if *show > 0.0 {
        if let Some(p) = state.last_powerup {
            set(
                &mut text,
                format!("★ {} ★\n{}", p.name().to_uppercase(), p.description()),
            );
            color.0 = p.color();
            return;
        }
    }
    if state.teleport {
        let dist = match (roster.me(&session), &map) {
            (Some(me), Some(map)) => me.feet().with_y(0.0).distance(map.0.extraction),
            _ => 0.0,
        };
        let hold = if state.teleport_hold > 0.0 {
            format!("\nTeleporting... {:.1} / {:.1}", state.teleport_hold, TELEPORT_HOLD)
        } else {
            String::new()
        };
        set(
            &mut text,
            format!(
                "BOSS DOWN! Teleporter open ({:.0}m)\nWhole team into the purple light{hold}",
                dist
            ),
        );
        color.0 = Color::srgb(0.75, 0.55, 1.0);
        return;
    }
    if state.sandbox.on && !state.sandbox.waves {
        set(&mut text, "Sandbox - F1 for tools".to_string());
        color.0 = DIM_HINT;
        return;
    }
    if state.stage_round == 0 {
        let lead = if state.sandbox.on {
            String::new()
        } else {
            format!("Map {} of {}: {}\n", state.stage + 1, STAGES, map_name(state.map))
        };
        set(
            &mut text,
            format!("{lead}Get ready... {:.0}", state.intermission.max(0.0).ceil()),
        );
        color.0 = ACCENT;
        return;
    }
    if state.to_spawn == 0 && state.intermission > 0.0 && state.intermission < 7.5 {
        let boss_next = !state.sandbox.on && state.stage_round == ROUNDS_PER_STAGE;
        if boss_next {
            let name = boss_name(state.map, state.stage + 1 >= STAGES);
            set(
                &mut text,
                format!(
                    "ROUND {} CLEARED!\n⚠️ {} INCOMING: {:.0}s",
                    state.round,
                    name,
                    state.intermission.ceil()
                ),
            );
            color.0 = Color::srgb(1.0, 0.45, 0.25);
        } else if state.intermission < 4.5 {
            set(&mut text, format!("ROUND {} CLEARED!", state.round));
            color.0 = Color::srgb(0.3, 0.95, 0.5);
        } else {
            set(&mut text, String::new());
        }
        return;
    }
    if *round_banner > 0.0 && state.intermission <= 0.0 {
        let boss_round = !state.sandbox.on && state.stage_round > ROUNDS_PER_STAGE;
        if boss_round {
            let name = boss_name(state.map, state.stage + 1 >= STAGES);
            set(&mut text, format!("☠ BOSS ROUND ☠\n{}", name.to_uppercase()));
            color.0 = Color::srgb(1.0, 0.2, 0.18);
        } else {
            set(&mut text, format!("— ROUND {} —", state.round));
            color.0 = Color::srgb(0.95, 0.22, 0.15);
        }
        return;
    }
    set(&mut text, String::new());
}

fn update_scoreboard(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
    state: Res<MatchState>,
    roster: Res<Roster>,
    session: Res<Session>,
    mut board: Single<&mut Visibility, With<Scoreboard>>,
    mut text: Single<&mut Text, With<ScoreboardText>>,
) {
    let show = keys.pressed(settings.key(Action::Scoreboard)) && !match_ended(&state);
    **board = if show {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    if !show {
        return;
    }
    let my_id = session.my_id;
    let mut s = format!(
        "═ MISSION STATUS: {} (ROUND {}) ═\n",
        map_name(state.map).to_uppercase(),
        state.round
    );
    s += "┌──────────────────┬──────────┬───────┬───────┬─────────┬─────────┬──────────┐\n";
    s += "│ Player           │ Hero     │ Level │ Kills │ Score   │ Points  │ Status   │\n";
    s += "├──────────────────┼──────────┼───────┼───────┼─────────┼─────────┼──────────┤\n";
    let mut players: Vec<_> = roster.0.values().collect();
    players.sort_by_key(|p| std::cmp::Reverse(p.score));
    let mut total_kills = 0;
    let mut total_score = 0;
    for p in &players {
        total_kills += p.kills;
        total_score += p.score;
        let is_me = p.id == my_id;
        let name_tag = if is_me {
            format!("★ {}", p.name)
        } else {
            p.name.clone()
        };
        let status = if p.alive { "ACTIVE" } else { "DOWNED" };
        s += &format!(
            "│ {:<16} │ {:<8} │ {:>5} │ {:>5} │ {:>7} │ {:>7} │ {:<8} │\n",
            if name_tag.len() > 16 { &name_tag[..16] } else { &name_tag },
            p.character.name(),
            p.level,
            p.kills,
            p.score,
            p.points,
            status
        );
    }
    s += "└──────────────────┴──────────┴───────┴───────┴─────────┴─────────┴──────────┘\n";
    s += &format!("  TEAM TOTALS: {} Kills  |  {} Score", total_kills, total_score);
    set(&mut text, s);
}

#[derive(Default, PartialEq, Eq, Clone)]
struct StationSnapshot {
    points: u32,
    pending_picks: u8,
    guns: [Option<u8>; 2],
    gun_tiers: [u8; 2],
    stats: [u8; 5],
    gun_elements: u8,
    ability_elements: u8,
    tiers: [u8; 3],
    augments: [u8; 2],
    kit: [Option<crate::data::Ability>; 3],
}

fn station_card(
    p: &mut ChildSpawnerCommands,
    title: impl Into<String>,
    desc: impl Into<String>,
    cost: u32,
    affordable: bool,
    maxed: bool,
    action: UiAction,
) {
    let title_str = title.into();
    let desc_str = desc.into();
    let (bg_color, hover_bg, border_color, cost_color, cost_text, button_action) = if maxed {
        (
            Color::srgba(0.08, 0.1, 0.14, 0.45),
            Color::srgba(0.08, 0.1, 0.14, 0.45),
            Color::srgba(0.28, 0.32, 0.4, 0.35),
            Color::srgb(0.55, 0.6, 0.7),
            "MAXED".to_string(),
            UiAction::Locked,
        )
    } else if affordable {
        (
            Color::srgba(0.08, 0.18, 0.12, 0.82),
            Color::srgba(0.12, 0.28, 0.18, 0.95),
            Color::srgb(0.22, 0.85, 0.42),
            Color::srgb(0.3, 1.0, 0.5),
            if cost == 0 { "FREE".to_string() } else { format!("{cost} PTS") },
            action,
        )
    } else {
        (
            Color::srgba(0.16, 0.08, 0.08, 0.65),
            Color::srgba(0.18, 0.10, 0.10, 0.75),
            Color::srgba(0.72, 0.22, 0.22, 0.45),
            Color::srgb(1.0, 0.4, 0.4),
            format!("{cost} PTS"),
            UiAction::Locked,
        )
    };

    p.spawn((
        Button,
        button_action,
        StationCard {
            base_bg: bg_color,
            hover_bg,
        },
        Node {
            width: Val::Px(265.0),
            min_height: Val::Px(64.0),
            padding: UiRect::all(Val::Px(8.0)),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::SpaceBetween,
            border: UiRect::all(Val::Px(1.5)),
            border_radius: BorderRadius::all(Val::Px(6.0)),
            ..default()
        },
        BackgroundColor(bg_color),
        BorderColor::all(border_color),
    ))
    .with_children(|b| {
        // Top row: Title + Cost
        b.spawn(Node {
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            width: Val::Percent(100.0),
            ..default()
        })
        .with_children(|header| {
            header.spawn((
                Text::new(title_str),
                TextFont {
                    font_size: 13.5.into(),
                    ..default()
                },
                TextColor(if maxed { Color::srgb(0.6, 0.65, 0.72) } else { Color::WHITE }),
            ));
            header.spawn((
                Text::new(cost_text),
                TextFont {
                    font_size: 13.0.into(),
                    ..default()
                },
                TextColor(cost_color),
            ));
        });
        // Bottom text: description
        b.spawn((
            Text::new(desc_str),
            TextFont {
                font_size: 11.0.into(),
                ..default()
            },
            TextColor(Color::srgb(0.72, 0.76, 0.84)),
        ));
    });
}

fn spawn_station_column(
    p: &mut ChildSpawnerCommands,
    title: &str,
    color: Color,
    content: impl FnOnce(&mut ChildSpawnerCommands),
) {
    p.spawn(Node {
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(8.0),
        width: Val::Px(265.0),
        align_items: AlignItems::Center,
        ..default()
    })
    .with_children(|col| {
        col.spawn((
            Node {
                padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                margin: UiRect::bottom(Val::Px(2.0)),
                ..default()
            },
        ))
        .with_children(|hdr| {
            hdr.spawn(text(title, 16.0, color));
        });
        content(col);
    });
}

/// The upgrade station terminal: purchasable weapon, stat, elemental, and ability upgrades.
fn upgrade_panel(
    mut commands: Commands,
    overlay: Res<Overlay>,
    session: Res<Session>,
    roster: Res<Roster>,
    panels: Query<Entity, With<UpgradePanel>>,
    mut shown: Local<Option<StationSnapshot>>,
) {
    if *overlay != Overlay::Upgrades {
        if shown.is_some() {
            *shown = None;
            for e in &panels {
                commands.entity(e).despawn();
            }
        }
        return;
    }
    let Some(me) = roster.me(&session) else {
        return;
    };
    let snap = StationSnapshot {
        points: me.points,
        pending_picks: me.pending_picks,
        guns: me.guns,
        gun_tiers: me.gun_tiers,
        stats: me.stats,
        gun_elements: me.gun_elements,
        ability_elements: me.ability_elements,
        tiers: me.tiers,
        augments: me.augments,
        kit: me.kit,
    };
    if *shown == Some(snap.clone()) {
        return;
    }
    *shown = Some(snap);
    for e in &panels {
        commands.entity(e).despawn();
    }

    commands
        .spawn((
            InGameEntity,
            UpgradePanel,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.65)),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(12.0),
                    padding: UiRect::axes(Val::Px(24.0), Val::Px(18.0)),
                    min_width: Val::Px(1120.0),
                    max_width: Val::Px(1180.0),
                    border: UiRect::all(Val::Px(2.0)),
                    border_radius: BorderRadius::all(Val::Px(12.0)),
                    ..default()
                },
                BackgroundColor(PANEL),
                BorderColor::all(Color::srgb(0.2, 0.65, 0.9)),
            ))
            .with_children(|p| {
                // Header
                p.spawn(text("UPGRADE STATION", 28.0, ACCENT));

                // Resources Bar
                p.spawn((
                    Node {
                        padding: UiRect::axes(Val::Px(18.0), Val::Px(6.0)),
                        border_radius: BorderRadius::all(Val::Px(8.0)),
                        border: UiRect::all(Val::Px(1.5)),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        column_gap: Val::Px(12.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.05, 0.15, 0.1, 0.9)),
                    BorderColor::all(Color::srgb(0.2, 0.8, 0.4)),
                ))
                .with_children(|res| {
                    let res_text = if me.pending_picks > 0 {
                        format!("RESOURCES: {} PTS   |   ⭐ {} FREE TOKENS", me.points, me.pending_picks)
                    } else {
                        format!("RESOURCES: {} PTS", me.points)
                    };
                    res.spawn(text(res_text, 20.0, Color::srgb(0.3, 1.0, 0.5)));
                });

                // 4 Category Columns
                p.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(14.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::FlexStart,
                    ..default()
                })
                .with_children(|cols| {
                    // Column 1: WEAPON UPGRADES
                    spawn_station_column(cols, "⚔️ WEAPON UPGRADES", Color::srgb(1.0, 0.65, 0.3), |col| {
                        // Slot 0 (Primary Gun)
                        if let Some(gun) = me.guns[0] {
                            let def = gun_def(gun);
                            let upgrade = Upgrade::Weapon(0);
                            let cost = upgrade.cost(me);
                            let can_buy = upgrade.can_purchase(me);
                            let maxed = upgrade.is_maxed(me);
                            let title = if maxed {
                                format!("{}: {}", def.name, tier_name(me.gun_tiers[0]))
                            } else {
                                format!("{}: {}", def.name, tier_name(me.gun_tiers[0] + 1))
                            };
                            let desc = if maxed { "Max tier reached" } else { "+25% damage and magazine size" };
                            station_card(col, title, desc, cost, can_buy, maxed, UiAction::BuyUpgrade(upgrade));
                        }

                        // Slot 1 (Secondary Gun)
                        if let Some(gun) = me.guns[1] {
                            let def = gun_def(gun);
                            let upgrade = Upgrade::Weapon(1);
                            let cost = upgrade.cost(me);
                            let can_buy = upgrade.can_purchase(me);
                            let maxed = upgrade.is_maxed(me);
                            let title = if maxed {
                                format!("{}: {}", def.name, tier_name(me.gun_tiers[1]))
                            } else {
                                format!("{}: {}", def.name, tier_name(me.gun_tiers[1] + 1))
                            };
                            let desc = if maxed { "Max tier reached" } else { "+25% damage and magazine size" };
                            station_card(col, title, desc, cost, can_buy, maxed, UiAction::BuyUpgrade(upgrade));
                        } else {
                            let upgrade = Upgrade::UnlockSecondary(10);
                            let cost = upgrade.cost(me);
                            let can_buy = upgrade.can_purchase(me);
                            station_card(col, "Unlock Breacher 12", "Equip secondary shotgun in slot 2", cost, can_buy, false, UiAction::BuyUpgrade(upgrade));
                        }
                    });

                    // Column 2: STAT BOOSTS
                    spawn_station_column(cols, "🛡️ STAT BOOSTS", Color::srgb(0.3, 0.85, 1.0), |col| {
                        for st in Stat::ALL {
                            let upgrade = Upgrade::Stat(st);
                            let cost = upgrade.cost(me);
                            let can_buy = upgrade.can_purchase(me);
                            let maxed = upgrade.is_maxed(me);
                            let current = me.stats[st as usize];
                            let title = format!("{} ({}/{})", st.name(), current, Stat::MAX_STACKS);
                            let desc = st.effect();
                            station_card(col, title, desc, cost, can_buy, maxed, UiAction::BuyUpgrade(upgrade));
                        }
                    });

                    // Column 3: ELEMENTAL INFUSIONS
                    spawn_station_column(cols, "⚡ ELEMENTAL INFUSION", Color::srgb(0.9, 0.45, 1.0), |col| {
                        for (i, el) in Element::ALL.iter().enumerate() {
                            let upgrade = Upgrade::GunElement(i as u8);
                            let cost = upgrade.cost(me);
                            let can_buy = upgrade.can_purchase(me);
                            let owned = (me.gun_elements & el.bit()) != 0;
                            let title = format!("{} Rounds", el.name());
                            let desc = if owned {
                                format!("ACTIVE: {}", el.effect())
                            } else {
                                format!("Infuse bullets: {}", el.effect())
                            };
                            station_card(col, title, desc, cost, can_buy, owned, UiAction::BuyUpgrade(upgrade));
                        }
                        for (i, el) in Element::ALL.iter().enumerate() {
                            let upgrade = Upgrade::AbilityElement(i as u8);
                            let cost = upgrade.cost(me);
                            let can_buy = upgrade.can_purchase(me);
                            let owned = (me.ability_elements & el.bit()) != 0;
                            let title = format!("{} Abilities", el.name());
                            let desc = if owned {
                                format!("ACTIVE: abilities {}", el.effect())
                            } else {
                                format!("Infuse spells: {}", el.effect())
                            };
                            station_card(col, title, desc, cost, can_buy, owned, UiAction::BuyUpgrade(upgrade));
                        }
                    });

                    // Column 4: ABILITY UPGRADES
                    spawn_station_column(cols, "✨ ABILITY UPGRADES", Color::srgb(1.0, 0.85, 0.25), |col| {
                        for s in 0..3 {
                            let slot_name = match s {
                                0 => "Ability 1 [Q]",
                                1 => "Ability 2 [E]",
                                _ => "Ultimate [X]",
                            };
                            if let Some(ability) = me.kit[s] {
                                let upgrade = Upgrade::Ability(s as u8);
                                let cost = upgrade.cost(me);
                                let can_buy = upgrade.can_purchase(me);
                                let maxed = upgrade.is_maxed(me);
                                let current_tier = me.tiers[s];
                                let title = if maxed {
                                    format!("{slot_name}: {} (MAX)", ability.name())
                                } else {
                                    format!("{slot_name}: {} Mk {}", ability.name(), current_tier + 2)
                                };
                                let desc = if maxed { "Max tier reached" } else { "-15% cooldown & boosted power" };
                                station_card(col, title, desc, cost, can_buy, maxed, UiAction::BuyUpgrade(upgrade));
                            } else {
                                let title = format!("{slot_name}: None");
                                let desc = "Find ability drops in world";
                                station_card(col, title, desc, 0, false, true, UiAction::Locked);
                            }
                        }

                        // Augments for tactical abilities
                        for s in 0..2 {
                            if let Some(ability) = me.kit[s] {
                                let upgrade = Upgrade::Augment(s as u8);
                                let cost = upgrade.cost(me);
                                let can_buy = upgrade.can_purchase(me);
                                let maxed = upgrade.is_maxed(me);
                                let title = format!("Augment: {} ({}/{})", ability.name(), me.augments[s] + 1, MAX_AUGMENT + 1);
                                let desc = match ability.augment() {
                                    Augment::Charges => "+1 ability charge",
                                    Augment::Copies => "Fires extra copies simultaneously",
                                };
                                station_card(col, title, desc, cost, can_buy, maxed, UiAction::BuyUpgrade(upgrade));
                            }
                        }
                    });
                });

                // Footer
                p.spawn(text(
                    "Press [F] or [Esc] to exit terminal",
                    14.0,
                    Color::srgb(0.65, 0.7, 0.8),
                ));
            });
        });
}

fn end_screen(
    mut commands: Commands,
    state: Res<MatchState>,
    session: Res<Session>,
    roster: Res<Roster>,
    result: Res<MatchResult>,
    profile: Res<crate::config::Profile>,
    panels: Query<Entity, With<EndPanel>>,
    mut shown: Local<bool>,
) {
    let want = match_ended(&state) && result.awarded;
    if want == *shown {
        return;
    }
    *shown = want;
    for e in &panels {
        commands.entity(e).despawn();
    }
    if !want {
        return;
    }
    let me = roster.me(&session);
    let (title, color) = if state.won {
        ("RUN COMPLETE!", Color::srgb(0.75, 0.55, 1.0))
    } else {
        ("GAME OVER", Color::srgb(0.95, 0.2, 0.15))
    };
    let how_far = if state.won {
        format!("All {STAGES} maps cleared and {FINAL_BOSS_NAME} defeated")
    } else if state.sandbox.on {
        String::new()
    } else {
        format!("Fell on map {} of {} ({})", state.stage + 1, STAGES, map_name(state.map))
    };
    commands
        .spawn((
            InGameEntity,
            EndPanel,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(10.0),
                    padding: UiRect::all(Val::Px(28.0)),
                    min_width: Val::Px(500.0),
                    border_radius: BorderRadius::all(Val::Px(10.0)),
                    ..default()
                },
                BackgroundColor(PANEL),
            ))
            .with_children(|p| {
                p.spawn(text(title, 60.0, color));
                if !how_far.is_empty() {
                    p.spawn(text(how_far.clone(), 22.0, Color::srgb(0.85, 0.85, 0.95)));
                }
                p.spawn(text(
                    format!("Rounds survived: {}", result.round),
                    24.0,
                    Color::WHITE,
                ));
                if let Some(me) = me {
                    p.spawn(text(
                        format!(
                            "Kills {}   Score {}   Level {}",
                            me.kills, me.score, me.level
                        ),
                        20.0,
                        Color::srgb(0.8, 0.82, 0.9),
                    ));
                }
                if result.new_best {
                    p.spawn(text("New personal best!", 20.0, ACCENT));
                }
                let spins = if result.spins > 0 {
                    format!("+{} gacha spins", result.spins)
                } else {
                    "No spins this time - reach round 5 to earn spins".into()
                };
                p.spawn(text(spins, 24.0, Color::srgb(1.0, 0.8, 0.3)));
                if result.premium_quarters > 0 {
                    p.spawn(text(
                        format!("+{} premium spin", crate::config::quarters_text(result.premium_quarters)),
                        22.0,
                        Color::srgb(1.0, 0.55, 0.9),
                    ));
                }
                p.spawn(text(format!("+{} career XP", result.xp), 22.0, Color::srgb(0.55, 0.85, 1.0)));
                let (before, after) = result.levels;
                if after > before {
                    let unlocked: Vec<String> = (before + 1..=after)
                        .filter_map(|l| crate::progression::UNLOCKS.get(l as usize - 2))
                        .map(|u| match *u {
                            crate::progression::Unlock::Gun(g) => gun_def(g).name.to_string(),
                            crate::progression::Unlock::Attachment(i) => crate::data::ATTACHMENTS[i].name.to_string(),
                        })
                        .collect();
                    p.spawn(text(
                        format!("Career level {after}! Unlocked for your loadout: {}", unlocked.join(", ")),
                        20.0,
                        ACCENT,
                    ));
                }
                let (before, after) = result.char_levels;
                if after > before {
                    let c = result.character;
                    let new: Vec<&str> = c
                        .pool()
                        .iter()
                        .filter(|a| a.def().unlock > before && a.def().unlock <= after)
                        .map(|a| a.name())
                        .collect();
                    let msg = if new.is_empty() {
                        format!("{} reached level {after}!", c.name())
                    } else {
                        format!(
                            "{} reached level {after}! New abilities to pick in Characters: {}",
                            c.name(),
                            new.join(", ")
                        )
                    };
                    p.spawn(text(msg, 20.0, ACCENT));
                }
                p.spawn(text(
                    format!(
                        "You have {} spins and {} premium spins. Open them in Gun Crates on the main menu.",
                        profile.spins,
                        crate::config::quarters_text(profile.premium_quarters)
                    ),
                    16.0,
                    Color::srgb(0.7, 0.72, 0.8),
                ));
                if session.is_authority() {
                    button_sized(p, "Play again", UiAction::BackToLobby, Some(380.0), false);
                } else {
                    p.spawn(text("Waiting for the host to start again...", 18.0, Color::srgb(0.7, 0.75, 0.85)));
                }
                button_sized(p, "Quit to main menu", UiAction::Leave, Some(380.0), false);
            });
        });
}

/// Shows the boss's name and health while one is out.
fn boss_bar(
    state: Res<MatchState>,
    mut bar: Single<&mut Visibility, With<BossBar>>,
    mut name: Single<&mut Text, With<BossName>>,
    mut fill: Single<&mut Node, With<BossFill>>,
) {
    let show = state.boss != 0 && !match_ended(&state);
    let want = if show {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    if **bar != want {
        **bar = want;
    }
    if !show {
        return;
    }
    set(&mut name, boss_name(state.map, state.boss == 2).to_uppercase());
    fill.width = Val::Percent(state.boss_hp * 100.0);
}

#[derive(Component)]
struct DamageArc {
    timer: f32,
    angle: f32,
}

fn arc_image() -> Image {
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    const N: usize = 256;
    let mut data = vec![0u8; N * N * 4];
    let c = N as f32 / 2.0;
    for y in 0..N {
        for x in 0..N {
            let (dx, dy) = (x as f32 + 0.5 - c, y as f32 + 0.5 - c);
            let r = (dx * dx + dy * dy).sqrt();
            let angle = dy.atan2(dx);
            
            let mut alpha = 0.0;
            if r > c * 0.7 && r < c * 0.9 {
                let mut dist = angle - (-std::f32::consts::FRAC_PI_2);
                while dist > std::f32::consts::PI { dist -= std::f32::consts::TAU; }
                while dist < -std::f32::consts::PI { dist += std::f32::consts::TAU; }
                dist = dist.abs();
                if dist < 0.6 {
                    alpha = (1.0 - (r - c * 0.8).abs() / (c * 0.1)) * (1.0 - dist / 0.6).powi(2);
                }
            }
            let i = (y * N + x) * 4;
            data[i] = 220;
            data[i+1] = 20;
            data[i+2] = 20;
            data[i+3] = (alpha.clamp(0.0, 1.0) * 255.0) as u8;
        }
    }
    Image::new(Extent3d { width: N as u32, height: N as u32, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD)
}
