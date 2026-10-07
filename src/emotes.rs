//! Emotes: press G to open the emote wheel, then 1-5 (or click) to dance,
//! wave, flip off, point or backflip. The camera pulls out to show your whole
//! character while it plays; moving, shooting or jumping stops it.

use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

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
                    show_menu.in_set(Phase::Present),
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

/// Whether the emote list is open.
#[derive(Resource, Default)]
pub struct EmoteMenu {
    pub open: bool,
}

#[derive(Component)]
struct MenuPanel;

const CAMERA_DISTANCE: f32 = 2.7;
const DIGITS: [KeyCode; 5] = [
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
];

fn spawn_menu(mut commands: Commands, mut menu: ResMut<EmoteMenu>) {
    menu.open = false;
    commands
        .spawn((
            InGameEntity,
            MenuPanel,
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(24.0),
                top: Val::Percent(32.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(12.0)),
                row_gap: Val::Px(6.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.05, 0.06, 0.09, 0.85)),
            BorderRadius::all(Val::Px(8.0)),
            Visibility::Hidden,
        ))
        .with_children(|p| {
            p.spawn((
                Text::new("EMOTES"),
                TextFont {
                    font_size: 16.0,
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.8, 0.3)),
            ));
            for (i, (name, _)) in EMOTES.iter().enumerate() {
                p.spawn((
                    Text::new(format!("{}  {}", i + 1, name)),
                    TextFont {
                        font_size: 18.0,
                        ..default()
                    },
                    TextColor(Color::WHITE),
                ));
            }
            p.spawn((
                Text::new("G to close"),
                TextFont {
                    font_size: 13.0,
                    ..default()
                },
                TextColor(Color::srgb(0.6, 0.65, 0.75)),
            ));
        });
}

fn show_menu(menu: Res<EmoteMenu>, mut panel: Query<&mut Visibility, With<MenuPanel>>) {
    for mut vis in &mut panel {
        let want = if menu.open {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *vis != want {
            *vis = want;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn emote_input(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    settings: Res<Settings>,
    window: Single<&Window, With<PrimaryWindow>>,
    session: Res<Session>,
    roster: Res<Roster>,
    state: Res<MatchState>,
    paused: Res<Paused>,
    app_state: Res<State<AppState>>,
    mut menu: ResMut<EmoteMenu>,
    mut player: Single<&mut LocalPlayer>,
) {
    let p = &mut **player;
    if *app_state.get() != AppState::InGame || !can_act(&session, &roster, &state, &paused, &window)
    {
        menu.open = false;
        p.emote = None;
        return;
    }
    if keys.tapped(&settings, Action::Emote) {
        menu.open = !menu.open;
    }
    if menu.open {
        if let Some(i) = DIGITS.iter().position(|k| keys.just_pressed(*k)) {
            let which = i as u8 + 1;
            menu.open = false;
            p.emote = Some((which, emote_length(which)));
            p.emote_seq = p.emote_seq.wrapping_add(1);
            p.orbit = Vec2::new(0.0, 0.25);
            if which == 5 && p.on_ground {
                p.vel.y = 7.5;
            }
        }
    }
    let Some((which, left)) = p.emote else { return };
    let left = left - time.delta_secs();
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    #[test]
    fn test_angle_diff_shortest_arc() {
        // Same angle
        assert!((angle_diff(1.0, 1.0)).abs() < 1e-6);

        // Small positive & negative angles
        assert!((angle_diff(0.0, 0.5) - 0.5).abs() < 1e-6);
        assert!((angle_diff(0.5, 0.0) - (-0.5)).abs() < 1e-6);

        // Circular wrap-around across boundary
        // from ~3.10 rad to -3.10 rad: counter-clockwise difference is ~0.083 rad
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

        // Horizontal follow rate (14.0) should be noticeably faster than vertical (6.0)
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
        // Initialize
        arm.update_pivot(Vec3::ZERO, 0.016);
        arm.update_dist(2.7, 0.016);
        assert_eq!(arm.current_dist, 2.7);

        let dt = 0.05;
        // 1. Obstruction detected: target distance drops (snap-in at 24.0)
        let obstructed_dist = 1.0;
        let dist_after_obstruction = arm.update_dist(obstructed_dist, dt);
        let snap_factor = 1.0 - (-SpringArmState::COLLISION_SNAP_RATE * dt).exp();
        let expected_obstructed = 2.7 + (1.0 - 2.7) * snap_factor;
        assert!((dist_after_obstruction - expected_obstructed).abs() < 1e-5);

        // 2. Clearance restored: target distance increases (smooth ease-out at 8.0)
        let cleared_dist = 2.7;
        let dist_before_clearance = arm.current_dist;
        let dist_after_clearance = arm.update_dist(cleared_dist, dt);
        let ease_factor = 1.0 - (-SpringArmState::CLEARANCE_EASE_RATE * dt).exp();
        let expected_cleared =
            dist_before_clearance + (cleared_dist - dist_before_clearance) * ease_factor;
        assert!((dist_after_clearance - expected_cleared).abs() < 1e-5);

        // Verification: snap-in rate (24.0) reacts much faster per unit time than ease-out (8.0)
        assert!(snap_factor > ease_factor);
    }
}

