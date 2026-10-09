//! Generic Bevy radial menu module wrapping `radial-menu-rs`.
//!
//! Provides:
//! - Re-exports of `radial-menu-rs` core types (`RadialMenu`, `RadialItem`, `RadialPointer`,
//!   `PolarSample`, `RadialStatus`, `GamepadStick`, `Spring1D`, `AngleSpring`, `SliceAnimation`, etc.).
//! - Helper mathematical functions to compute 2D screen positions for slice centroids
//!   using `SectorArc::centroid`.
//! - Smooth cursor offset tracking for relative mouse deltas or absolute screen coordinates.
//! - Smooth gamepad thumbstick tracking with deadzone filtering, angle spring wrapping,
//!   and deflection damping.
//! - High-level `RadialController<T>` component/resource for managing wheel state and slice animations.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// 1. Re-exports from radial-menu-rs
// ---------------------------------------------------------------------------

pub use radial_menu_rs::{
    lerp_color, AngleSpring, EasingType, GamepadInput, GamepadStick, PolarSample, RadialItem,
    RadialMenu, RadialPointer, RadialStatus, SectorArc, SliceAnimation, Spring1D, TWO_PI,
};

pub const HALF_PI: f32 = core::f32::consts::FRAC_PI_2;

// ---------------------------------------------------------------------------
// 2. Helper Mathematical Functions for Slice Centroids
// ---------------------------------------------------------------------------

/// Computes the 2D local Cartesian offset vector from the menu center to a slice's centroid.
///
/// Uses [`SectorArc::centroid`] with the provided inner/outer radii and clock offset.
#[inline]
pub fn slice_centroid_offset(
    arc: &SectorArc,
    inner_radius: f32,
    outer_radius: f32,
    clock_offset: f32,
) -> Vec2 {
    arc.centroid(inner_radius, outer_radius, clock_offset)
}

/// Computes the 2D screen position of a slice's centroid given an origin center.
///
/// Screen position is defined as `origin + offset`, where the offset is derived
/// from [`SectorArc::centroid`].
#[inline]
pub fn slice_screen_position(
    origin: Vec2,
    arc: &SectorArc,
    inner_radius: f32,
    outer_radius: f32,
    clock_offset: f32,
) -> Vec2 {
    origin + slice_centroid_offset(arc, inner_radius, outer_radius, clock_offset)
}

/// Computes the 2D screen position with an inverted vertical axis.
///
/// Useful when converting between coordinate systems where the vertical axis convention differs
/// (e.g. screen space where +Y is down vs. Bevy 2D world / UI space where +Y is up).
#[inline]
pub fn slice_screen_position_flipped_y(
    origin: Vec2,
    arc: &SectorArc,
    inner_radius: f32,
    outer_radius: f32,
    clock_offset: f32,
) -> Vec2 {
    let offset = slice_centroid_offset(arc, inner_radius, outer_radius, clock_offset);
    Vec2::new(origin.x + offset.x, origin.y - offset.y)
}

/// Computes the 2D screen position for a specific slice index in a [`RadialMenu`].
///
/// Uses `menu.pointer.origin` and `menu.pointer.clock_offset`.
pub fn menu_slice_centroid<T>(
    menu: &RadialMenu<T>,
    slice_index: usize,
    inner_radius: f32,
    outer_radius: f32,
) -> Option<Vec2> {
    let arc = menu.slice_arc(slice_index)?;
    Some(slice_screen_position(
        menu.pointer.origin,
        &arc,
        inner_radius,
        outer_radius,
        menu.pointer.clock_offset,
    ))
}

/// Computes the 2D screen position for a slice using `pointer.min_radius` and `pointer.max_radius`.
pub fn menu_slice_centroid_default_radii<T>(
    menu: &RadialMenu<T>,
    slice_index: usize,
) -> Option<Vec2> {
    menu_slice_centroid(
        menu,
        slice_index,
        menu.pointer.min_radius,
        menu.pointer.max_radius,
    )
}

/// Computes the 2D screen centroid positions for all slices in a [`RadialMenu`].
pub fn all_menu_slice_centroids<T>(
    menu: &RadialMenu<T>,
    inner_radius: f32,
    outer_radius: f32,
) -> Vec<Vec2> {
    (0..menu.slice_count())
        .filter_map(|idx| menu_slice_centroid(menu, idx, inner_radius, outer_radius))
        .collect()
}

/// Extension trait providing centroid computation directly on [`RadialMenu`].
pub trait RadialMenuCentroidExt {
    /// Computes the centroid screen position for a slice index with custom radii.
    fn slice_centroid(
        &self,
        slice_index: usize,
        inner_radius: f32,
        outer_radius: f32,
    ) -> Option<Vec2>;

    /// Computes the centroid screen position using pointer's `min_radius` and `max_radius`.
    fn slice_centroid_default(&self, slice_index: usize) -> Option<Vec2>;

    /// Computes screen centroids for all slices in the menu.
    fn all_slice_centroids(&self, inner_radius: f32, outer_radius: f32) -> Vec<Vec2>;
}

impl<T> RadialMenuCentroidExt for RadialMenu<T> {
    #[inline]
    fn slice_centroid(
        &self,
        slice_index: usize,
        inner_radius: f32,
        outer_radius: f32,
    ) -> Option<Vec2> {
        menu_slice_centroid(self, slice_index, inner_radius, outer_radius)
    }

    #[inline]
    fn slice_centroid_default(&self, slice_index: usize) -> Option<Vec2> {
        menu_slice_centroid_default_radii(self, slice_index)
    }

    #[inline]
    fn all_slice_centroids(&self, inner_radius: f32, outer_radius: f32) -> Vec<Vec2> {
        all_menu_slice_centroids(self, inner_radius, outer_radius)
    }
}

// ---------------------------------------------------------------------------
// 3. Smooth Cursor Offset Tracking
// ---------------------------------------------------------------------------

/// Smooth cursor offset tracker for radial menus and weapon wheels.
///
/// Can operate in two modes:
/// 1. **Relative / Virtual Motion**: Accumulates raw mouse delta movements
///    (e.g., when the cursor is locked or hidden).
/// 2. **Absolute Screen Tracking**: Smooths towards an absolute screen cursor position.
///
/// Provides both exponential decay lerping and second-order damped physics springs
/// via [`Spring1D`].
#[derive(Component, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SmoothRadialCursor {
    /// Current smoothed offset vector relative to the menu origin.
    pub current_offset: Vec2,
    /// Target offset vector relative to the menu origin.
    pub target_offset: Vec2,
    /// Mouse sensitivity multiplier applied to accumulated mouse motion deltas.
    pub sensitivity: f32,
    /// Smoothing speed for exponential decay lerping (units: 1/sec).
    pub smoothing_speed: f32,
    /// Maximum clamped offset distance from origin (e.g. `pointer.max_radius`).
    pub max_radius: f32,
    /// If true, uses 2D damped spring physics instead of exponential lerp.
    pub use_spring: bool,
    /// Physics spring for the X axis.
    pub spring_x: Spring1D,
    /// Physics spring for the Y axis.
    pub spring_y: Spring1D,
}

impl Default for SmoothRadialCursor {
    fn default() -> Self {
        Self {
            current_offset: Vec2::ZERO,
            target_offset: Vec2::ZERO,
            sensitivity: 1.0,
            smoothing_speed: 25.0,
            max_radius: 200.0,
            use_spring: false,
            spring_x: Spring1D::new(0.0),
            spring_y: Spring1D::new(0.0),
        }
    }
}

impl SmoothRadialCursor {
    /// Creates a new cursor tracker with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Configures the maximum distance threshold from the origin.
    pub fn with_max_radius(mut self, max_radius: f32) -> Self {
        self.max_radius = max_radius;
        self
    }

    /// Sets the sensitivity multiplier for motion deltas.
    pub fn with_sensitivity(mut self, sensitivity: f32) -> Self {
        self.sensitivity = sensitivity;
        self
    }

    /// Sets the exponential smoothing speed.
    pub fn with_smoothing_speed(mut self, speed: f32) -> Self {
        self.smoothing_speed = speed;
        self
    }

    /// Enables physics spring smoothing with custom stiffness and damping.
    pub fn with_spring(mut self, stiffness: f32, damping: f32) -> Self {
        self.use_spring = true;
        self.spring_x.stiffness = stiffness;
        self.spring_x.damping = damping;
        self.spring_y.stiffness = stiffness;
        self.spring_y.damping = damping;
        self
    }

    /// Adds relative mouse motion delta (e.g., from `AccumulatedMouseMotion`).
    ///
    /// Multiplies delta by sensitivity and clamps the accumulated vector to `max_radius`.
    pub fn add_motion_delta(&mut self, delta: Vec2) {
        let new_target = self.target_offset + delta * self.sensitivity;
        let len = new_target.length();
        self.target_offset = if len > self.max_radius && len > 1e-5 {
            new_target * (self.max_radius / len)
        } else {
            new_target
        };

        if self.use_spring {
            self.spring_x.set_target(self.target_offset.x);
            self.spring_y.set_target(self.target_offset.y);
        }
    }

    /// Sets the target offset directly, clamping it to `max_radius`.
    pub fn set_target_offset(&mut self, offset: Vec2) {
        let len = offset.length();
        self.target_offset = if len > self.max_radius && len > 1e-5 {
            offset * (self.max_radius / len)
        } else {
            offset
        };

        if self.use_spring {
            self.spring_x.set_target(self.target_offset.x);
            self.spring_y.set_target(self.target_offset.y);
        }
    }

    /// Sets the target position from an absolute screen coordinate and menu origin.
    pub fn set_target_screen_pos(&mut self, screen_pos: Vec2, origin: Vec2) {
        self.set_target_offset(screen_pos - origin);
    }

    /// Advances the smoothing simulation by `dt` seconds.
    pub fn update(&mut self, dt: f32) {
        if self.use_spring {
            self.spring_x.update(dt);
            self.spring_y.update(dt);
            self.current_offset = Vec2::new(self.spring_x.position, self.spring_y.position);
        } else {
            let t = (self.smoothing_speed * dt).clamp(0.0, 1.0);
            self.current_offset = self.current_offset.lerp(self.target_offset, t);
        }
    }

    /// Returns the current smoothed absolute screen position.
    #[inline]
    pub fn current_screen_pos(&self, origin: Vec2) -> Vec2 {
        origin + self.current_offset
    }

    /// Evaluates the current smoothed pointer state against a [`RadialPointer`].
    pub fn evaluate(&self, pointer: &RadialPointer) -> PolarSample {
        let screen_pos = pointer.origin + self.current_offset;
        pointer.evaluate(screen_pos)
    }

    /// Returns true if the pointer is currently within deadzone radius.
    #[inline]
    pub fn is_neutral(&self, min_radius: f32) -> bool {
        self.current_offset.length() < min_radius
    }

    /// Resets the tracker back to the center origin.
    pub fn reset(&mut self) {
        self.current_offset = Vec2::ZERO;
        self.target_offset = Vec2::ZERO;
        self.spring_x.position = 0.0;
        self.spring_x.velocity = 0.0;
        self.spring_x.target = 0.0;
        self.spring_y.position = 0.0;
        self.spring_y.velocity = 0.0;
        self.spring_y.target = 0.0;
    }
}

// ---------------------------------------------------------------------------
// 4. Smooth Gamepad Stick Tracking
// ---------------------------------------------------------------------------

/// Gamepad thumbstick input tracker with deadzone filtering, angle wrapping smoothing,
/// and deflection magnitude dynamics.
#[derive(Component, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SmoothRadialStick {
    /// Thumbstick configuration from `radial-menu-rs`.
    pub stick_config: GamepadStick,
    /// Smoothed 2D stick vector in $[-1.0, 1.0]^2$.
    pub smoothed_stick: Vec2,
    /// Angular spring tracking clockwise angle in $[0, 2\pi)$ without wrap-around jitter.
    pub angle_spring: AngleSpring,
    /// Damped spring tracking deflection magnitude in $[0.0, 1.0]$.
    pub magnitude_spring: Spring1D,
    /// Vector lerping speed.
    pub smoothing_speed: f32,
    /// Last processed input event from `radial-menu-rs`.
    pub last_input: Option<GamepadInput>,
    /// Last evaluated polar sample.
    pub last_sample: Option<PolarSample>,
    /// Whether the stick was in the neutral deadzone in the most recent evaluation.
    pub is_neutral: bool,
}

impl Default for SmoothRadialStick {
    fn default() -> Self {
        Self::new(GamepadStick::default())
    }
}

impl SmoothRadialStick {
    /// Creates a new stick tracker with the specified stick configuration.
    pub fn new(stick_config: GamepadStick) -> Self {
        Self {
            stick_config,
            smoothed_stick: Vec2::ZERO,
            angle_spring: AngleSpring::new(0.0),
            magnitude_spring: Spring1D::new(0.0),
            smoothing_speed: 25.0,
            last_input: None,
            last_sample: None,
            is_neutral: true,
        }
    }

    /// Configures the vector smoothing speed.
    pub fn with_smoothing_speed(mut self, speed: f32) -> Self {
        self.smoothing_speed = speed;
        self
    }

    /// Configures the angle spring stiffness and damping.
    pub fn with_angle_spring(mut self, stiffness: f32, damping: f32) -> Self {
        self.angle_spring.stiffness = stiffness;
        self.angle_spring.damping = damping;
        self
    }

    /// Configures the deflection magnitude spring stiffness and damping.
    pub fn with_magnitude_spring(mut self, stiffness: f32, damping: f32) -> Self {
        self.magnitude_spring.stiffness = stiffness;
        self.magnitude_spring.damping = damping;
        self
    }

    /// Processes raw stick axes $(x, y) \in [-1.0, 1.0]$ and updates smoothing physics.
    pub fn update(
        &mut self,
        raw_stick: Vec2,
        pointer: &RadialPointer,
        dt: f32,
    ) -> GamepadInput {
        let input = self.stick_config.process(raw_stick, pointer);
        self.is_neutral = input.is_neutral;
        self.last_sample = Some(input.polar_sample);

        if !input.is_neutral {
            self.angle_spring.set_target(input.clock_angle);
            self.magnitude_spring.set_target(input.magnitude);
        } else {
            self.magnitude_spring.set_target(0.0);
        }

        let t = (self.smoothing_speed * dt).clamp(0.0, 1.0);
        let target_vec = if input.is_neutral {
            Vec2::ZERO
        } else {
            input.filtered
        };
        self.smoothed_stick = self.smoothed_stick.lerp(target_vec, t);

        self.angle_spring.update(dt);
        self.magnitude_spring.update(dt);

        self.last_input = Some(input);
        input
    }

    /// Current smoothed clock angle in radians $[0, 2\pi)$.
    #[inline]
    pub fn smoothed_angle(&self) -> f32 {
        self.angle_spring.angle
    }

    /// Current smoothed deflection magnitude clamped to $[0.0, 1.0]$.
    #[inline]
    pub fn smoothed_magnitude(&self) -> f32 {
        self.magnitude_spring.position.clamp(0.0, 1.0)
    }

    /// Computes the smoothed virtual screen cursor position based on angle and magnitude.
    pub fn virtual_cursor_pos(&self, pointer: &RadialPointer) -> Vec2 {
        let angle = self.smoothed_angle();
        let mag = self.smoothed_magnitude();
        let r = pointer.min_radius + mag * (pointer.max_radius - pointer.min_radius);
        let dir = RadialPointer::vector_from_clock_angle(angle, pointer.clock_offset);
        pointer.origin + dir * r
    }

    /// Evaluates the current smoothed state against a [`RadialPointer`].
    pub fn evaluate(&self, pointer: &RadialPointer) -> PolarSample {
        let pos = self.virtual_cursor_pos(pointer);
        pointer.evaluate(pos)
    }

    /// Resets stick tracker state to neutral.
    pub fn reset(&mut self) {
        self.smoothed_stick = Vec2::ZERO;
        self.angle_spring.velocity = 0.0;
        self.magnitude_spring.position = 0.0;
        self.magnitude_spring.velocity = 0.0;
        self.magnitude_spring.target = 0.0;
        self.last_input = None;
        self.last_sample = None;
        self.is_neutral = true;
    }
}

// ---------------------------------------------------------------------------
// 5. High-Level Radial Wheel Controller Component / Resource
// ---------------------------------------------------------------------------

/// Complete controller managing a [`RadialMenu`], smoothed inputs, and per-slice animations.
#[derive(Component, Debug, Clone)]
pub struct RadialController<T = ()> {
    /// Underlying radial menu structure and items.
    pub menu: RadialMenu<T>,
    /// Smoothed mouse cursor tracker.
    pub cursor_tracker: SmoothRadialCursor,
    /// Smoothed gamepad thumbstick tracker.
    pub stick_tracker: SmoothRadialStick,
    /// Per-slice animations for scale and highlight glow.
    pub slice_animations: Vec<SliceAnimation>,
    /// Currently selected slice index (if outside deadzone).
    pub active_slice: Option<usize>,
    /// Previously selected slice index (for detecting hover sound/change triggers).
    pub previous_slice: Option<usize>,
}

impl<T> RadialController<T> {
    /// Creates a new controller wrapping the given [`RadialMenu`].
    pub fn new(menu: RadialMenu<T>) -> Self {
        let slice_count = menu.slice_count();
        let cursor_tracker = SmoothRadialCursor::new().with_max_radius(menu.pointer.max_radius);
        let stick_tracker = SmoothRadialStick::default();
        let slice_animations = (0..slice_count).map(|_| SliceAnimation::new()).collect();

        Self {
            menu,
            cursor_tracker,
            stick_tracker,
            slice_animations,
            active_slice: None,
            previous_slice: None,
        }
    }

    /// Feeds relative mouse delta movement and updates slice selection.
    pub fn update_mouse_delta(&mut self, delta: Vec2, dt: f32) -> Option<usize> {
        self.cursor_tracker.add_motion_delta(delta);
        self.cursor_tracker.update(dt);
        let sample = self.cursor_tracker.evaluate(&self.menu.pointer);
        self.set_active_from_sample(sample)
    }

    /// Feeds absolute screen cursor coordinates and updates slice selection.
    pub fn update_cursor_pos(&mut self, cursor_pos: Vec2, dt: f32) -> Option<usize> {
        self.cursor_tracker.set_target_screen_pos(cursor_pos, self.menu.pointer.origin);
        self.cursor_tracker.update(dt);
        let sample = self.cursor_tracker.evaluate(&self.menu.pointer);
        self.set_active_from_sample(sample)
    }

    /// Feeds gamepad thumbstick axes $(x, y) \in [-1.0, 1.0]$ and updates slice selection.
    pub fn update_stick(&mut self, stick_axes: Vec2, dt: f32) -> Option<usize> {
        self.stick_tracker.update(stick_axes, &self.menu.pointer, dt);
        if self.stick_tracker.is_neutral {
            self.set_selection(None)
        } else {
            let sample = self.stick_tracker.evaluate(&self.menu.pointer);
            self.set_active_from_sample(sample)
        }
    }

    fn set_active_from_sample(&mut self, sample: PolarSample) -> Option<usize> {
        let new_idx = if sample.status.is_selectable() {
            self.menu.slice_index_from_clock_angle(sample.angle_clock)
        } else {
            None
        };
        self.set_selection(new_idx)
    }

    fn set_selection(&mut self, new_idx: Option<usize>) -> Option<usize> {
        self.previous_slice = self.active_slice;
        self.active_slice = new_idx;
        self.active_slice
    }

    /// Returns true if the selected slice changed during the most recent update.
    #[inline]
    pub fn has_selection_changed(&self) -> bool {
        self.active_slice != self.previous_slice
    }

    /// Advances slice scale and glow animations.
    pub fn update_animations(&mut self, dt: f32, active_scale: f32) {
        for (i, anim) in self.slice_animations.iter_mut().enumerate() {
            let is_active = self.active_slice == Some(i);
            anim.set_active(is_active, active_scale);
            anim.update(dt);
        }
    }

    /// Returns a reference to the active [`RadialItem`] if one is selected.
    pub fn active_item(&self) -> Option<&RadialItem<T>> {
        self.active_slice.and_then(|idx| self.menu.items.get(idx))
    }

    /// Returns a mutable reference to the active [`RadialItem`] if one is selected.
    pub fn active_item_mut(&mut self) -> Option<&mut RadialItem<T>> {
        self.active_slice.and_then(|idx| self.menu.items.get_mut(idx))
    }

    /// Resets all inputs and active selection to neutral.
    pub fn reset(&mut self) {
        self.cursor_tracker.reset();
        self.stick_tracker.reset();
        self.active_slice = None;
        self.previous_slice = None;
        for anim in &mut self.slice_animations {
            anim.set_active(false, 1.0);
        }
    }
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slice_centroid_screen_position() {
        let pointer = RadialPointer::new(Vec2::new(500.0, 500.0), 50.0, 150.0);
        let items: Vec<RadialItem<()>> = vec![
            RadialItem::new("item1", "Item 1"),
            RadialItem::new("item2", "Item 2"),
            RadialItem::new("item3", "Item 3"),
            RadialItem::new("item4", "Item 4"),
        ];
        let menu = RadialMenu::new(items, pointer);

        // Menu has 4 slices: 0: top, 1: right, 2: bottom, 3: left.
        // Mid-radius is (50 + 150) / 2 = 100.
        let top_pos = menu.slice_centroid_default(0).expect("Slice 0 centroid");
        assert!((top_pos.x - 500.0).abs() < 1e-3);
        assert!((top_pos.y - 400.0).abs() < 1e-3); // 500 + (-100) = 400

        let right_pos = menu.slice_centroid_default(1).expect("Slice 1 centroid");
        assert!((right_pos.x - 600.0).abs() < 1e-3); // 500 + 100 = 600
        assert!((right_pos.y - 500.0).abs() < 1e-3);

        let centroids = menu.all_slice_centroids(50.0, 150.0);
        assert_eq!(centroids.len(), 4);
    }

    #[test]
    fn test_smooth_cursor_tracking() {
        let mut cursor = SmoothRadialCursor::new()
            .with_max_radius(200.0)
            .with_smoothing_speed(50.0);

        cursor.add_motion_delta(Vec2::new(100.0, 0.0));
        assert_eq!(cursor.target_offset, Vec2::new(100.0, 0.0));

        // Advance simulation
        cursor.update(0.05);
        assert!(cursor.current_offset.x > 0.0);

        // Clamping to max_radius
        cursor.add_motion_delta(Vec2::new(500.0, 0.0));
        assert!((cursor.target_offset.x - 200.0).abs() < 1e-3);
        assert_eq!(cursor.target_offset.y, 0.0);

        cursor.reset();
        assert_eq!(cursor.current_offset, Vec2::ZERO);
        assert_eq!(cursor.target_offset, Vec2::ZERO);
    }

    #[test]
    fn test_smooth_stick_tracking() {
        let pointer = RadialPointer::new(Vec2::ZERO, 30.0, 200.0);
        let mut stick = SmoothRadialStick::default();

        // Inside deadzone (0.1 < 0.22)
        let input = stick.update(Vec2::new(0.1, 0.0), &pointer, 0.016);
        assert!(input.is_neutral);
        assert!(stick.is_neutral);

        // Deflected right (1.0, 0.0)
        let input = stick.update(Vec2::new(1.0, 0.0), &pointer, 0.016);
        assert!(!input.is_neutral);
        assert!(!stick.is_neutral);

        stick.reset();
        assert!(stick.is_neutral);
        assert_eq!(stick.smoothed_stick, Vec2::ZERO);
    }

    #[test]
    fn test_radial_controller() {
        let pointer = RadialPointer::new(Vec2::new(400.0, 300.0), 30.0, 150.0);
        let items: Vec<RadialItem<()>> = vec![
            RadialItem::new("item1", "Item 1"),
            RadialItem::new("item2", "Item 2"),
        ];
        let menu = RadialMenu::new(items, pointer);
        let mut controller = RadialController::new(menu);

        assert_eq!(controller.slice_animations.len(), 2);
        assert_eq!(controller.active_slice, None);

        // Move cursor up outside deadzone (towards 12 o'clock, slice 0)
        controller.update_cursor_pos(Vec2::new(400.0, 200.0), 1.0);
        assert_eq!(controller.active_slice, Some(0));
        assert!(controller.has_selection_changed());
        assert_eq!(controller.active_item().map(|it| it.id.as_str()), Some("item1"));

        controller.update_animations(0.016, 1.2);
        assert!(controller.slice_animations[0].current_scale() >= 1.0);
    }
}
