//! Emotes: press/hold G to open the radial emote wheel, then deflect mouse
//! (or thumbstick) and release G (or press 1-5 / left-click) to dance,
//! wave, flip off, point or backflip. Powered by `radial-menu-rs`.
//! The camera pulls out to show your whole character while it plays;
//! moving, shooting or jumping stops it.

use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorOptions, PrimaryWindow};

use radial_menu_rs::{
    lerp_color, GamepadStick, PolarSample, RadialItem, RadialMenu, RadialPointer,
    SliceAnimation, Spring1D,
};

use crate::config::{Action, InputExt, Settings};
use crate::game::Paused;
use crate::physics::{collect_boxes, ray_world};
use crate::player::{can_act, LocalPlayer};
use crate::rig::{emote_length, EMOTES};
use crate::{AppState, Collider, InGameEntity, MatchState, Phase, Roster, Session};

pub struct EmotePlugin;

impl Plugin for EmotePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EmoteMenu>()
            .add_systems(OnEnter(AppState::InGame), spawn_menu)
            .add_systems(
                Update,
                emote_input
                    .in_set(Phase::Local)
                    .before(crate::player::movement),
            )
            .add_systems(
                Update,
                (
                    emote_camera
                        .after(crate::player::movement)
                        .in_set(Phase::Local),
                    update_wheel_ui.in_set(Phase::Present),
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

/// Builds the 5-item `RadialMenu` for emotes with origin centered.
pub fn build_emote_radial_menu(center: Vec2) -> RadialMenu<u8> {
    let items = EMOTES
        .iter()
        .enumerate()
        .map(|(i, (name, _))| {
            RadialItem::with_payload(
                name.to_lowercase(),
                *name,
                (i + 1) as u8,
            )
        })
        .collect::<Vec<_>>();
    let pointer = RadialPointer::new(center, 38.0, 160.0);
    RadialMenu::new(items, pointer)
}

/// Radial emote wheel resource tracking menu state, sector physics, and cursor deflection.
#[derive(Resource)]
pub struct EmoteMenu {
    /// Whether the radial emote wheel is active/open.
    pub open: bool,
    /// Sector partitioning and angular evaluation engine.
    pub radial: RadialMenu<u8>,
    /// Per-slice hover pop and glow spring animations.
    pub animations: Vec<SliceAnimation>,
    /// Overall wheel scale pop entrance spring.
    pub wheel_spring: Spring1D,
    /// Mouse / thumbstick deflection offset from center.
    pub deflection: Vec2,
    /// Absolute cursor position in screen space.
    pub cursor_pos: Vec2,
    /// Currently selected sector index (0..5), if outside deadzone.
    pub selected_slice: Option<usize>,
    /// Most recent polar sample evaluation.
    pub last_sample: Option<PolarSample>,
    /// Gamepad thumbstick deadzone processor.
    pub stick_processor: GamepadStick,
}

impl Default for EmoteMenu {
    fn default() -> Self {
        let radial = build_emote_radial_menu(Vec2::ZERO);
        let animations = (0..EMOTES.len())
            .map(|_| SliceAnimation::new())
            .collect();
        let mut wheel_spring = Spring1D::new(0.0);
        wheel_spring.stiffness = 240.0;
        wheel_spring.damping = 22.0;

        Self {
            open: false,
            radial,
            animations,
            wheel_spring,
            deflection: Vec2::ZERO,
            cursor_pos: Vec2::ZERO,
            selected_slice: None,
            last_sample: None,
            stick_processor: GamepadStick::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// UI Components
// ---------------------------------------------------------------------------

#[derive(Component)]
pub struct EmoteWheelRoot;

#[derive(Component)]
pub struct EmoteWheelSliceNode {
    pub index: usize,
}

#[derive(Component)]
pub struct EmoteWheelSliceTitle {
    pub index: usize,
}

#[derive(Component)]
pub struct EmoteWheelCenterTitle;

#[derive(Component)]
pub struct EmoteWheelCenterSubtitle;

#[derive(Component)]
pub struct EmoteWheelPip;

const CAMERA_DISTANCE: f32 = 2.7;
const DIGITS: [KeyCode; 5] = [
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
];

// ---------------------------------------------------------------------------
// Wheel Spawning
// ---------------------------------------------------------------------------

fn spawn_menu(mut commands: Commands, mut menu: ResMut<EmoteMenu>) {
    menu.open = false;
    menu.deflection = Vec2::ZERO;
    menu.selected_slice = None;
    menu.wheel_spring.position = 0.0;
    menu.wheel_spring.target = 0.0;

    let radial = build_emote_radial_menu(Vec2::ZERO);

    commands
        .spawn((
            InGameEntity,
            EmoteWheelRoot,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            Transform::default(),
            Visibility::Hidden,
        ))
        .with_children(|root| {
            // Centered anchor node
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
            .with_children(|anchor| {
                // Outer perimeter boundary ring
                anchor.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(-165.0),
                        top: Val::Px(-165.0),
                        width: Val::Px(330.0),
                        height: Val::Px(330.0),
                        border: UiRect::all(Val::Px(1.5)),
                        border_radius: BorderRadius::all(Val::Percent(50.0)),
                        ..default()
                    },
                    BorderColor::all(Color::srgba(0.35, 0.45, 0.65, 0.25)),
                    BackgroundColor(Color::srgba(0.03, 0.04, 0.07, 0.50)),
                ));

                // Inner deadzone threshold ring
                anchor.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(-38.0),
                        top: Val::Px(-38.0),
                        width: Val::Px(76.0),
                        height: Val::Px(76.0),
                        border: UiRect::all(Val::Px(1.0)),
                        border_radius: BorderRadius::all(Val::Percent(50.0)),
                        ..default()
                    },
                    BorderColor::all(Color::srgba(0.55, 0.65, 0.80, 0.30)),
                    BackgroundColor(Color::NONE),
                ));

                // Center Reticle Hub
                anchor
                    .spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(-46.0),
                            top: Val::Px(-46.0),
                            width: Val::Px(92.0),
                            height: Val::Px(92.0),
                            border: UiRect::all(Val::Px(2.0)),
                            border_radius: BorderRadius::all(Val::Percent(50.0)),
                            flex_direction: FlexDirection::Column,
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            padding: UiRect::all(Val::Px(4.0)),
                            ..default()
                        },
                        BorderColor::all(Color::srgba(0.65, 0.72, 0.85, 0.55)),
                        BackgroundColor(Color::srgba(0.05, 0.07, 0.11, 0.95)),
                    ))
                    .with_children(|hub| {
                        hub.spawn((
                            EmoteWheelCenterTitle,
                            Text::new("EMOTES"),
                            TextFont {
                                font_size: 13.0.into(),
                                ..default()
                            },
                            TextColor(Color::srgb(1.0, 0.82, 0.28)),
                        ));
                        hub.spawn((
                            EmoteWheelCenterSubtitle,
                            Text::new("MOVE MOUSE"),
                            TextFont {
                                font_size: 9.0.into(),
                                ..default()
                            },
                            TextColor(Color::srgb(0.60, 0.66, 0.76)),
                        ));
                    });

                // Spawn 5 Sector Slice Cards positioned along radial sector centroids
                for i in 0..5 {
                    let arc = radial.slice_arc(i).expect("slice arc exists");
                    let centroid = arc.centroid(75.0, 165.0, radial.pointer.clock_offset);
                    let (name, dur) = EMOTES[i];
                    let card_w = 118.0;
                    let card_h = 52.0;

                    anchor
                        .spawn((
                            EmoteWheelSliceNode { index: i },
                            Transform::default(),
                            Node {
                                position_type: PositionType::Absolute,
                                left: Val::Px(centroid.x - card_w * 0.5),
                                top: Val::Px(centroid.y - card_h * 0.5),
                                width: Val::Px(card_w),
                                height: Val::Px(card_h),
                                border: UiRect::all(Val::Px(2.0)),
                                border_radius: BorderRadius::all(Val::Px(10.0)),
                                flex_direction: FlexDirection::Column,
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                                padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                                row_gap: Val::Px(2.0),
                                ..default()
                            },
                            BorderColor::all(Color::srgba(0.30, 0.36, 0.48, 0.45)),
                            BackgroundColor(Color::srgba(0.06, 0.08, 0.13, 0.88)),
                        ))
                        .with_children(|slice| {
                            // Top metadata row: [digit] and duration
                            slice
                                .spawn(Node {
                                    width: Val::Percent(100.0),
                                    justify_content: JustifyContent::SpaceBetween,
                                    align_items: AlignItems::Center,
                                    ..default()
                                })
                                .with_children(|row| {
                                    row.spawn((
                                        Text::new(format!("[{}]", i + 1)),
                                        TextFont {
                                            font_size: 11.0.into(),
                                            ..default()
                                        },
                                        TextColor(Color::srgb(1.0, 0.78, 0.25)),
                                    ));
                                    row.spawn((
                                        Text::new(format!("{:.1}s", dur)),
                                        TextFont {
                                            font_size: 10.0.into(),
                                            ..default()
                                        },
                                        TextColor(Color::srgb(0.55, 0.60, 0.70)),
                                    ));
                                });

                            // Emote Title
                            slice.spawn((
                                EmoteWheelSliceTitle { index: i },
                                Text::new(name),
                                TextFont {
                                    font_size: 14.5.into(),
                                    ..default()
                                },
                                TextColor(Color::WHITE),
                            ));
                        });
                }

                // Interactive Deflection Cursor Pip
                anchor.spawn((
                    EmoteWheelPip,
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(-6.0),
                        top: Val::Px(-6.0),
                        width: Val::Px(12.0),
                        height: Val::Px(12.0),
                        border: UiRect::all(Val::Px(1.5)),
                        border_radius: BorderRadius::all(Val::Percent(50.0)),
                        ..default()
                    },
                    BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.90)),
                    BackgroundColor(Color::srgba(1.0, 0.84, 0.20, 0.95)),
                ));

                // Bottom instructions footer
                anchor
                    .spawn(Node {
                        position_type: PositionType::Absolute,
                        top: Val::Px(188.0),
                        left: Val::Px(-200.0),
                        width: Val::Px(400.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    })
                    .with_children(|footer| {
                        footer.spawn((
                            Text::new("[G] RELEASE TO PLAY   •   [LMB] CONFIRM   •   [1-5] QUICK SELECT"),
                            TextFont {
                                font_size: 11.0.into(),
                                ..default()
                            },
                            TextColor(Color::srgb(0.60, 0.66, 0.78)),
                        ));
                    });
            });
        });
}

// ---------------------------------------------------------------------------
// UI Update & Animation Presentation
// ---------------------------------------------------------------------------

fn update_wheel_ui(
    menu: Res<EmoteMenu>,
    mut root_query: Query<&mut Visibility, With<EmoteWheelRoot>>,
    mut slices_query: Query<(
        &EmoteWheelSliceNode,
        &mut Transform,
        &mut BackgroundColor,
        &mut BorderColor,
    )>,
    mut titles_query: Query<(&EmoteWheelSliceTitle, &mut TextColor)>,
    mut center_title: Query<
        &mut Text,
        (
            With<EmoteWheelCenterTitle>,
            Without<EmoteWheelCenterSubtitle>,
            Without<EmoteWheelSliceTitle>,
        ),
    >,
    mut center_sub: Query<
        &mut Text,
        (
            With<EmoteWheelCenterSubtitle>,
            Without<EmoteWheelCenterTitle>,
            Without<EmoteWheelSliceTitle>,
        ),
    >,
    mut pip_query: Query<(&mut Node, &mut BackgroundColor), (With<EmoteWheelPip>, Without<EmoteWheelSliceNode>)>,
) {
    let wheel_visible = menu.open || menu.wheel_spring.position > 0.02;
    for mut vis in &mut root_query {
        let want = if wheel_visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *vis != want {
            *vis = want;
        }
    }

    if !wheel_visible {
        return;
    }

    let wheel_scale = menu.wheel_spring.position.clamp(0.0, 1.2);

    // Animate each radial slice card (scale spring + color glow)
    for (slice_node, mut tf, mut bg, mut border) in &mut slices_query {
        if let Some(anim) = menu.animations.get(slice_node.index) {
            let scale = anim.current_scale() * wheel_scale;
            tf.scale = Vec3::splat(scale);

            let glow = anim.current_glow();
            let bg_vec = lerp_color(
                Vec4::new(0.06, 0.08, 0.13, 0.88),
                Vec4::new(0.32, 0.25, 0.08, 0.98),
                glow,
            );
            bg.0 = Color::srgba(bg_vec.x, bg_vec.y, bg_vec.z, bg_vec.w);

            let border_vec = lerp_color(
                Vec4::new(0.30, 0.36, 0.48, 0.45),
                Vec4::new(1.0, 0.85, 0.28, 0.98),
                glow,
            );
            *border = BorderColor::all(Color::srgba(border_vec.x, border_vec.y, border_vec.z, border_vec.w));
        }
    }

    // Animate slice title text colors
    for (title_node, mut text_color) in &mut titles_query {
        if let Some(anim) = menu.animations.get(title_node.index) {
            let glow = anim.current_glow();
            let color_vec = lerp_color(
                Vec4::new(0.90, 0.92, 0.96, 1.0),
                Vec4::new(1.0, 0.92, 0.40, 1.0),
                glow,
            );
            text_color.0 = Color::srgba(color_vec.x, color_vec.y, color_vec.z, color_vec.w);
        }
    }

    // Update center hub reticle text
    if let Ok(mut t) = center_title.single_mut() {
        if let Some(idx) = menu.selected_slice {
            t.0 = EMOTES[idx].0.to_uppercase();
        } else {
            t.0 = "EMOTES".into();
        }
    }
    if let Ok(mut sub) = center_sub.single_mut() {
        if menu.selected_slice.is_some() {
            sub.0 = "RELEASE G".into();
        } else {
            sub.0 = "DEFLECT MOUSE".into();
        }
    }

    // Update deflection cursor pip position and glow
    if let Ok((mut node, mut pip_bg)) = pip_query.single_mut() {
        node.left = Val::Px(menu.deflection.x - 6.0);
        node.top = Val::Px(menu.deflection.y - 6.0);
        if menu.selected_slice.is_some() {
            pip_bg.0 = Color::srgba(1.0, 0.88, 0.25, 0.98);
        } else {
            pip_bg.0 = Color::srgba(0.80, 0.86, 0.96, 0.75);
        }
    }
}

// ---------------------------------------------------------------------------
// Input & Interaction System
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn emote_input(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    settings: Res<Settings>,
    cursor: Single<&CursorOptions, With<PrimaryWindow>>,
    window: Single<&Window, With<PrimaryWindow>>,
    gamepads: Query<&Gamepad>,
    session: Res<Session>,
    roster: Res<Roster>,
    state: Res<MatchState>,
    paused: Res<Paused>,
    app_state: Res<State<AppState>>,
    mut menu: ResMut<EmoteMenu>,
    mut player: Single<&mut LocalPlayer>,
    mut hold_timer: Local<f32>,
) {
    let p = &mut **player;

    // Cancellation check
    if *app_state.get() != AppState::InGame || !can_act(&session, &roster, &state, &paused, &cursor)
    {
        menu.open = false;
        menu.selected_slice = None;
        menu.deflection = Vec2::ZERO;
        *hold_timer = 0.0;
        p.emote = None;
        return;
    }

    let dt = time.delta_secs();

    // Center the menu pointer on window center
    let center = Vec2::new(window.width() * 0.5, window.height() * 0.5);
    menu.radial.pointer.origin = center;

    let emote_key = settings.key(Action::Emote);
    let just_pressed_emote = keys.just_pressed(emote_key);
    let held_emote = keys.pressed(emote_key);
    let released_emote = keys.just_released(emote_key);

    // Toggle/open emote wheel
    if just_pressed_emote {
        if !menu.open {
            menu.open = true;
            menu.deflection = Vec2::ZERO;
            menu.selected_slice = None;
            *hold_timer = 0.0;
            for anim in &mut menu.animations {
                *anim = SliceAnimation::new();
            }
        } else {
            // Tapping G while already open closes it
            menu.open = false;
            menu.selected_slice = None;
            menu.deflection = Vec2::ZERO;
        }
    }

    // Helper to trigger an emote
    let trigger_emote = |which: u8, p: &mut LocalPlayer| {
        p.emote = Some((which, emote_length(which)));
        p.emote_seq = p.emote_seq.wrapping_add(1);
        p.orbit = Vec2::new(0.0, 0.25);
        if which == 5 && p.on_ground {
            p.vel.y = 7.5;
        }
    };

    if menu.open {
        if held_emote {
            *hold_timer += dt;
        }
        menu.wheel_spring.set_target(1.0);

        // 1. Track relative mouse motion deflection
        if motion.delta != Vec2::ZERO {
            menu.deflection += motion.delta;
        }

        // 2. Track gamepad thumbstick deflection
        for gp in &gamepads {
            let stick_x = gp
                .get(GamepadAxis::LeftStickX)
                .or_else(|| gp.get(GamepadAxis::RightStickX))
                .unwrap_or(0.0);
            let stick_y = gp
                .get(GamepadAxis::LeftStickY)
                .or_else(|| gp.get(GamepadAxis::RightStickY))
                .unwrap_or(0.0);
            let raw_stick = Vec2::new(stick_x, -stick_y); // Stick up (-Y) is top screen
            let gp_input = menu.stick_processor.process(raw_stick, &menu.radial.pointer);
            if !gp_input.is_neutral {
                menu.deflection = gp_input.filtered * menu.radial.pointer.max_radius;
            }
        }

        // Clamp deflection within comfortable boundary
        let max_radius = menu.radial.pointer.max_radius * 1.35;
        let r = menu.deflection.length();
        if r > max_radius {
            menu.deflection = (menu.deflection / r) * max_radius;
        }

        // Calculate cursor position and evaluate slice selection
        menu.cursor_pos = center + menu.deflection;
        let (sample, slice_idx) = menu.radial.select_slice(menu.cursor_pos);
        menu.selected_slice = slice_idx;
        menu.last_sample = Some(sample);

        // Advance slice animations
        for (i, anim) in menu.animations.iter_mut().enumerate() {
            let is_active = Some(i) == slice_idx;
            anim.set_active(is_active, 1.20);
            anim.update(dt);
        }
        menu.wheel_spring.update(dt);

        // Selection Trigger Conditions:
        // A. Number keys 1-5
        if let Some(i) = DIGITS.iter().position(|k| keys.just_pressed(*k)) {
            let which = (i + 1) as u8;
            trigger_emote(which, p);
            menu.open = false;
            menu.selected_slice = None;
            menu.deflection = Vec2::ZERO;
        }
        // B. Mouse Left Click
        else if mouse.just_pressed(MouseButton::Left) {
            if let Some(idx) = menu.selected_slice {
                let which = (idx + 1) as u8;
                trigger_emote(which, p);
            }
            menu.open = false;
            menu.selected_slice = None;
            menu.deflection = Vec2::ZERO;
        }
        // C. Emote Key (G) Released
        else if released_emote {
            if let Some(idx) = menu.selected_slice {
                let which = (idx + 1) as u8;
                trigger_emote(which, p);
                menu.open = false;
                menu.selected_slice = None;
                menu.deflection = Vec2::ZERO;
            } else if *hold_timer > 0.18 {
                // Cancelled / released in deadzone
                menu.open = false;
                menu.selected_slice = None;
                menu.deflection = Vec2::ZERO;
            }
        }
    } else {
        menu.wheel_spring.set_target(0.0);
        menu.wheel_spring.update(dt);
        for anim in &mut menu.animations {
            anim.set_active(false, 1.0);
            anim.update(dt);
        }
    }

    // Active emote playback & interrupt checks
    let Some((which, left)) = p.emote else { return };
    let left = left - dt;

    // Moving, jumping, shooting or using anything stops the emote (the
    // backflip carries on through its own jump).
    let moved = [
        Action::Forward,
        Action::Back,
        Action::Left,
        Action::Right,
        Action::Jump,
        Action::Crouch,
    ]
    .iter()
    .any(|a| keys.held(&settings, *a))
        && which != 5;

    let busy = [
        Action::Ability1,
        Action::Ability2,
        Action::Ultimate,
        Action::Reload,
        Action::Interact,
    ]
    .iter()
    .any(|a| keys.tapped(&settings, *a))
        || mouse.just_pressed(MouseButton::Left)
        || mouse.just_pressed(MouseButton::Right);

    p.emote = if left <= 0.0 || moved || busy {
        None
    } else {
        Some((which, left))
    };
}

// ---------------------------------------------------------------------------
// Camera Orbit During Emote
// ---------------------------------------------------------------------------

/// Computes the shortest signed angular difference from angle `from` to `to` in radians.
/// Output is in the range `[-PI, PI]`.
#[allow(dead_code)]
pub fn angle_diff(from: f32, to: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    (to - from + PI).rem_euclid(TAU) - PI
}

/// Spring-arm camera state smoothing pivot follow and collision distance easing.
#[derive(Default, Debug, Clone, Copy)]
pub struct SpringArmState {
    pub pivot: Vec3,
    pub current_dist: f32,
    pub pivot_initialized: bool,
    pub dist_initialized: bool,
}

impl SpringArmState {
    pub const HORIZONTAL_FOLLOW_RATE: f32 = 14.0;
    pub const VERTICAL_FOLLOW_RATE: f32 = 6.0;
    pub const COLLISION_SNAP_RATE: f32 = 24.0;
    pub const CLEARANCE_EASE_RATE: f32 = 8.0;

    pub fn reset(&mut self) {
        self.pivot_initialized = false;
        self.dist_initialized = false;
    }

    pub fn update_pivot(&mut self, target: Vec3, dt: f32) -> Vec3 {
        if !self.pivot_initialized {
            self.pivot = target;
            self.pivot_initialized = true;
            return self.pivot;
        }
        let h_blend = 1.0 - (-Self::HORIZONTAL_FOLLOW_RATE * dt).exp();
        let v_blend = 1.0 - (-Self::VERTICAL_FOLLOW_RATE * dt).exp();
        self.pivot.x += (target.x - self.pivot.x) * h_blend;
        self.pivot.z += (target.z - self.pivot.z) * h_blend;
        self.pivot.y += (target.y - self.pivot.y) * v_blend;
        self.pivot
    }

    pub fn update_dist(&mut self, target_dist: f32, dt: f32) -> f32 {
        if !self.dist_initialized {
            self.current_dist = target_dist;
            self.dist_initialized = true;
            return self.current_dist;
        }
        let rate = if target_dist < self.current_dist {
            Self::COLLISION_SNAP_RATE
        } else {
            Self::CLEARANCE_EASE_RATE
        };
        let blend = 1.0 - (-rate * dt).exp();
        self.current_dist += (target_dist - self.current_dist) * blend;
        self.current_dist
    }
}

/// Pulls the camera out in front of you while you emote, so you can
/// see your character; the mouse orbits it.
fn emote_camera(
    time: Res<Time>,
    motion: Res<AccumulatedMouseMotion>,
    settings: Res<Settings>,
    player: Single<(&mut Transform, &mut LocalPlayer)>,
    colliders: Query<(&Transform, &Collider), Without<LocalPlayer>>,
    mut arm: Local<SpringArmState>,
) {
    let (mut tf, mut p) = player.into_inner();
    let dt = time.delta_secs();
    let target = if p.emoting() { 1.0 } else { 0.0 };
    p.cam_out = if target > 0.5 {
        (p.cam_out + dt * 3.5).min(1.0)
    } else {
        (p.cam_out - dt * 5.0).max(0.0)
    };
    if p.cam_out <= 0.0 {
        arm.reset();
        return;
    }
    if p.emoting() {
        let s = crate::player::MOUSE_SCALE * settings.sensitivity;
        p.orbit.x -= motion.delta.x * s;
        p.orbit.y = (p.orbit.y + motion.delta.y * s).clamp(-0.3, 1.2);
    }
    let out = p.cam_out * p.cam_out * (3.0 - 2.0 * p.cam_out);
    // Start in front of the character, looking back at them.
    let yaw = p.yaw + p.orbit.x;
    let pitch = p.orbit.y;
    let dir = Vec3::new(
        -yaw.sin() * pitch.cos(),
        pitch.sin(),
        -yaw.cos() * pitch.cos(),
    );
    let target_focus = p.feet + Vec3::Y * 1.15;
    let focus = arm.update_pivot(target_focus, dt);

    let boxes = collect_boxes(colliders.iter());
    let target_dist =
        (ray_world(focus, dir, CAMERA_DISTANCE, &boxes) - 0.25).clamp(0.6, CAMERA_DISTANCE);
    let dist = arm.update_dist(target_dist, dt);

    let cam = focus + dir * dist;
    let look = Transform::from_translation(cam).looking_at(focus, Vec3::Y);
    tf.translation = tf.translation.lerp(cam, out);
    tf.rotation = tf.rotation.slerp(look.rotation, out);
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use radial_menu_rs::RadialStatus;
    use std::f32::consts::PI;

    #[test]
    fn test_emote_radial_menu_initialization() {
        let menu = build_emote_radial_menu(Vec2::new(960.0, 540.0));
        assert_eq!(menu.slice_count(), 5);
        assert_eq!(menu.items[0].id, "dance");
        assert_eq!(menu.items[0].payload, 1);
        assert_eq!(menu.items[1].id, "wave");
        assert_eq!(menu.items[1].payload, 2);
        assert_eq!(menu.items[2].id, "flip off");
        assert_eq!(menu.items[2].payload, 3);
        assert_eq!(menu.items[3].id, "point");
        assert_eq!(menu.items[3].payload, 4);
        assert_eq!(menu.items[4].id, "backflip");
        assert_eq!(menu.items[4].payload, 5);

        // Angular width for 5 slices is 2*PI / 5
        let expected_width = std::f32::consts::TAU / 5.0;
        assert!((menu.angular_width() - expected_width).abs() < 1e-5);
    }

    #[test]
    fn test_radial_slice_selection_directions() {
        let origin = Vec2::new(500.0, 500.0);
        let menu = build_emote_radial_menu(origin);

        // 1. Deadzone: at center origin
        let (sample, slice) = menu.select_slice(origin);
        assert_eq!(sample.status, RadialStatus::Neutral);
        assert_eq!(slice, None);

        // 2. Straight UP (12 o'clock, top): delta = (0, -100) -> Slice 0 ("dance")
        let up = origin + Vec2::new(0.0, -100.0);
        let (sample, slice) = menu.select_slice(up);
        assert!(sample.status.is_selectable());
        assert_eq!(slice, Some(0));

        // 3. Down-Right: delta = (80, 80) -> atan2 is positive, clock angle is ~135° -> Slice 2 ("flip off")
        let down_right = origin + Vec2::new(80.0, 80.0);
        let (_, slice) = menu.select_slice(down_right);
        assert_eq!(slice, Some(2));

        // 4. Down-Left: delta = (-80, 80) -> clock angle is ~225° -> Slice 3 ("point")
        let down_left = origin + Vec2::new(-80.0, 80.0);
        let (_, slice) = menu.select_slice(down_left);
        assert_eq!(slice, Some(3));
    }

    #[test]
    fn test_slice_animation_spring_scaling() {
        let mut anim = SliceAnimation::new();
        assert_eq!(anim.current_scale(), 1.0);
        assert_eq!(anim.current_glow(), 0.0);

        // Activate hover
        anim.set_active(true, 1.25);
        for _ in 0..60 {
            anim.update(0.016);
        }
        // Should have scaled up toward 1.25
        assert!((anim.current_scale() - 1.25).abs() < 0.05);
        assert!((anim.current_glow() - 1.0).abs() < 0.05);

        // Deactivate
        anim.set_active(false, 1.0);
        for _ in 0..60 {
            anim.update(0.016);
        }
        // Should settle back to 1.0
        assert!((anim.current_scale() - 1.0).abs() < 0.05);
        assert!((anim.current_glow() - 0.0).abs() < 0.05);
    }

    #[test]
    fn test_angle_diff_shortest_arc() {
        // Same angle
        assert!((angle_diff(1.0, 1.0)).abs() < 1e-6);

        // Small positive & negative angles
        assert!((angle_diff(0.0, 0.5) - 0.5).abs() < 1e-6);
        assert!((angle_diff(0.5, 0.0) - (-0.5)).abs() < 1e-6);

        // Circular wrap-around across boundary
        let diff = angle_diff(3.10, -3.10);
        let expected = (-3.10 - 3.10) + std::f32::consts::TAU;
        assert!((diff - expected).abs() < 1e-5);

        // Opposite directions: should be +/- PI
        assert!((angle_diff(0.0, PI).abs() - PI).abs() < 1e-6);
        assert!((angle_diff(PI, 0.0).abs() - PI).abs() < 1e-6);

        // Full circle rotations
        assert!((angle_diff(0.0, std::f32::consts::TAU)).abs() < 1e-6);
    }

    #[test]
    fn test_spring_arm_decoupled_pivot_follow() {
        let mut arm = SpringArmState::default();
        let target1 = Vec3::new(10.0, 5.0, -10.0);

        // First update initializes immediately
        let p1 = arm.update_pivot(target1, 0.016);
        assert_eq!(p1, target1);
        assert!(arm.pivot_initialized);

        // Step change on target
        let target2 = Vec3::new(20.0, 15.0, -20.0);
        let dt = 0.05;
        let p2 = arm.update_pivot(target2, dt);

        let h_expected_factor = 1.0 - (-SpringArmState::HORIZONTAL_FOLLOW_RATE * dt).exp();
        let v_expected_factor = 1.0 - (-SpringArmState::VERTICAL_FOLLOW_RATE * dt).exp();

        assert!(h_expected_factor > v_expected_factor);

        let expected_x = 10.0 + (20.0 - 10.0) * h_expected_factor;
        let expected_z = -10.0 + (-20.0 - (-10.0)) * h_expected_factor;
        let expected_y = 5.0 + (15.0 - 5.0) * v_expected_factor;

        assert!((p2.x - expected_x).abs() < 1e-5);
        assert!((p2.z - expected_z).abs() < 1e-5);
        assert!((p2.y - expected_y).abs() < 1e-5);
    }

    #[test]
    fn test_spring_arm_asymmetric_collision_distance_easing() {
        let mut arm = SpringArmState::default();
        arm.update_pivot(Vec3::ZERO, 0.016);
        arm.update_dist(2.7, 0.016);
        assert_eq!(arm.current_dist, 2.7);

        let dt = 0.05;
        let obstructed_dist = 1.0;
        let dist_after_obstruction = arm.update_dist(obstructed_dist, dt);
        let snap_factor = 1.0 - (-SpringArmState::COLLISION_SNAP_RATE * dt).exp();
        let expected_obstructed = 2.7 + (1.0 - 2.7) * snap_factor;
        assert!((dist_after_obstruction - expected_obstructed).abs() < 1e-5);

        let cleared_dist = 2.7;
        let dist_before_clearance = arm.current_dist;
        let dist_after_clearance = arm.update_dist(cleared_dist, dt);
        let ease_factor = 1.0 - (-SpringArmState::CLEARANCE_EASE_RATE * dt).exp();
        let expected_cleared =
            dist_before_clearance + (cleared_dist - dist_before_clearance) * ease_factor;
        assert!((dist_after_clearance - expected_cleared).abs() < 1e-5);

        assert!(snap_factor > ease_factor);
    }
}
