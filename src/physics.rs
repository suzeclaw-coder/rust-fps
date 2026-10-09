//! Tiny hand-rolled collision and ray-cast helpers. The level is made only of
//! axis-aligned boxes, players are vertical circles and enemies are capsules.

use bevy::prelude::*;

use crate::Collider;

pub type Boxes = Vec<(Vec3, Vec3)>;

pub fn collect_boxes<'a>(iter: impl Iterator<Item = (&'a Transform, &'a Collider)>) -> Boxes {
    iter.map(|(t, c)| (t.translation, c.half)).collect()
}

pub const MAX_STEP_HEIGHT: f32 = 0.35;
pub const STEP_CLEARANCE_HEIGHT: f32 = 1.8;

/// Checks whether standing at `step_top` has overhead clearance (no low ceiling or beam blocking head).
pub fn has_overhead_clearance(pos: Vec2, radius: f32, step_top: f32, colliders: &Boxes) -> bool {
    let head_y = step_top + STEP_CLEARANCE_HEIGHT;
    for (center, half) in colliders {
        let c_top = center.y + half.y;
        let c_bottom = center.y - half.y;
        // An overhead obstacle hangs above the step (bottom >= step_top - 0.05) and below head height
        if c_bottom < head_y && c_bottom >= step_top - 0.05 && c_top > step_top + 0.5 {
            let closest = Vec2::new(
                pos.x.clamp(center.x - half.x, center.x + half.x),
                pos.y.clamp(center.z - half.z, center.z + half.z),
            );
            if (pos - closest).length_squared() < radius * radius {
                return false;
            }
        }
    }
    true
}

/// Pushes a circle (in XZ) out of every box it overlaps.
/// If `max_step > 0.0`, allows stepping up onto obstacles up to `max_step` high
/// if overhead clearance is clear.
/// Runs multiple collision resolution passes (3 iterations) so that corners between
/// two boxes don't cause jitter or push the player into another box.
/// Returns the highest ground level under the circle.
pub fn resolve_collisions_with_step(
    pos: &mut Vec3,
    radius: f32,
    feet_y: f32,
    max_step: f32,
    colliders: &Boxes,
) -> f32 {
    let mut highest_ground = feet_y;
    let passes = 3;

    for _ in 0..passes {
        let mut pushed = false;
        for (center, half) in colliders {
            let top = center.y + half.y;
            let bottom = center.y - half.y;

            if highest_ground >= top - 0.05 {
                continue;
            }
            if bottom > highest_ground + 2.0 {
                continue;
            }

            let closest = Vec2::new(
                pos.x.clamp(center.x - half.x, center.x + half.x),
                pos.z.clamp(center.z - half.z, center.z + half.z),
            );
            let p = Vec2::new(pos.x, pos.z);
            let diff = p - closest;
            let dist = diff.length();

            if dist < radius {
                if max_step > 0.0
                    && top > feet_y
                    && top <= feet_y + max_step
                    && has_overhead_clearance(p, radius, top, colliders)
                {
                    highest_ground = highest_ground.max(top);
                    continue;
                }

                let push = if dist > 1e-4 {
                    diff / dist * (radius - dist)
                } else {
                    let dx = half.x - (pos.x - center.x).abs();
                    let dz = half.z - (pos.z - center.z).abs();
                    if dx < dz {
                        Vec2::new((dx + radius) * (pos.x - center.x).signum(), 0.0)
                    } else {
                        Vec2::new(0.0, (dz + radius) * (pos.z - center.z).signum())
                    }
                };
                pos.x += push.x;
                pos.z += push.y;
                pushed = true;
            }
        }
        if !pushed {
            break;
        }
    }

    if max_step > 0.0 {
        pos.y = pos.y.max(highest_ground);
    }
    highest_ground
}

/// Pushes a circle (in XZ) out of every box it overlaps, ignoring boxes whose
/// top is below `feet_y` (so you can stand on crates) and boxes overhead
/// (crane beams, roofs).
pub fn resolve_collisions(pos: &mut Vec3, radius: f32, feet_y: f32, colliders: &Boxes) {
    resolve_collisions_with_step(pos, radius, feet_y, 0.0, colliders);
}

/// Highest box top under the circle that the feet are above (0 = floor).
pub fn ground_height(pos: Vec3, radius: f32, feet_y: f32, colliders: &Boxes) -> f32 {
    let mut ground: f32 = 0.0;
    for (center, half) in colliders {
        let top = center.y + half.y;
        let inside_x = (pos.x - center.x).abs() < half.x + radius * 0.7;
        let inside_z = (pos.z - center.z).abs() < half.z + radius * 0.7;
        if inside_x && inside_z && feet_y >= top - (MAX_STEP_HEIGHT + 0.05) {
            ground = ground.max(top);
        }
    }
    ground
}

pub const MAX_PENETRATION_THICKNESS: f32 = 0.38;
pub const PENETRATION_DAMAGE_FACTOR: f32 = 0.55;

/// Ray vs axis-aligned box (slab method). Returns entry distance `tmin` and exit distance `tmax`.
pub fn ray_aabb_interval(origin: Vec3, dir: Vec3, center: Vec3, half: Vec3) -> Option<(f32, f32)> {
    let min = center - half;
    let max = center + half;
    let inv = dir.recip();
    let t1 = (min - origin) * inv;
    let t2 = (max - origin) * inv;
    let tmin = t1.min(t2).max_element();
    let tmax = t1.max(t2).min_element();
    (tmax >= tmin.max(0.0)).then_some((tmin.max(0.0), tmax))
}

/// Ray vs axis-aligned box (slab method). Returns hit distance.
pub fn ray_aabb(origin: Vec3, dir: Vec3, center: Vec3, half: Vec3) -> Option<f32> {
    ray_aabb_interval(origin, dir, center, half).map(|(tmin, _)| tmin)
}

/// Ray vs axis-aligned box (slab method). Returns hit distance and surface normal.
#[allow(dead_code)]
pub fn ray_aabb_hit(origin: Vec3, dir: Vec3, center: Vec3, half: Vec3) -> Option<(f32, Vec3)> {
    let min = center - half;
    let max = center + half;
    let inv = dir.recip();
    let t1 = (min - origin) * inv;
    let t2 = (max - origin) * inv;
    let t_enter = t1.min(t2);
    let t_exit = t1.max(t2);
    let tmin = t_enter.max_element();
    let tmax = t_exit.min_element();
    if tmax < tmin.max(0.0) {
        return None;
    }
    let t = tmin.max(0.0);
    let normal = if tmin > 0.0 {
        if (t_enter.x - tmin).abs() <= 1e-4 {
            Vec3::new(-dir.x.signum(), 0.0, 0.0)
        } else if (t_enter.y - tmin).abs() <= 1e-4 {
            Vec3::new(0.0, -dir.y.signum(), 0.0)
        } else {
            Vec3::new(0.0, 0.0, -dir.z.signum())
        }
    } else {
        let d = origin - center;
        let overhang = half - d.abs();
        if overhang.x <= overhang.y && overhang.x <= overhang.z {
            Vec3::new(d.x.signum(), 0.0, 0.0)
        } else if overhang.y <= overhang.x && overhang.y <= overhang.z {
            Vec3::new(0.0, d.y.signum(), 0.0)
        } else {
            Vec3::new(0.0, 0.0, d.z.signum())
        }
    };
    Some((t, normal))
}

/// Ray vs sphere. Returns hit distance.
pub fn ray_sphere(origin: Vec3, dir: Vec3, center: Vec3, radius: f32) -> Option<f32> {
    let oc = origin - center;
    let b = oc.dot(dir);
    let c = oc.length_squared() - radius * radius;
    let disc = b * b - c;
    if disc < 0.0 {
        return None;
    }
    let t = -b - disc.sqrt();
    (t >= 0.0).then_some(t)
}

/// Enemies are person-shaped with their origin at the feet. Returns the hit
/// distance and whether it was a headshot.
pub fn ray_enemy(
    origin: Vec3,
    dir: Vec3,
    feet: Vec3,
    scale: f32,
    crawl: bool,
) -> Option<(f32, Zone)> {
    if crawl {
        // Lying on the ground: the head is low and the body is a long lump.
        let head = ray_sphere(origin, dir, feet + Vec3::Y * 0.42 * scale, 0.24 * scale);
        let body = ray_sphere(origin, dir, feet + Vec3::Y * 0.3 * scale, 0.5 * scale);
        return match (head, body) {
            (Some(h), Some(b)) if b < h - 0.15 => Some((b, Zone::Body)),
            (Some(h), _) => Some((h, Zone::Head)),
            (None, Some(b)) => Some((b, Zone::Body)),
            _ => None,
        };
    }
    let head = ray_sphere(origin, dir, feet + Vec3::Y * 1.78 * scale, 0.24 * scale);
    let parts = [
        (1.3, 0.36, Zone::Body),
        (0.95, 0.34, Zone::Body),
        (0.62, 0.26, Zone::Legs),
        (0.28, 0.24, Zone::Legs),
    ];
    let body = parts
        .iter()
        .filter_map(|(y, r, z)| {
            ray_sphere(origin, dir, feet + Vec3::Y * y * scale, r * scale).map(|t| (t, *z))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0));
    match (head, body) {
        (Some(h), Some(b)) if b.0 < h => Some(b),
        (Some(h), _) => Some((h, Zone::Head)),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

/// Where a shot hit an enemy.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Zone {
    Head,
    Body,
    Legs,
}

/// Distance to the first wall or floor hit along the ray.
pub fn ray_world(origin: Vec3, dir: Vec3, max: f32, colliders: &Boxes) -> f32 {
    let mut nearest = max;
    for (center, half) in colliders {
        if let Some(t) = ray_aabb(origin, dir, *center, *half) {
            nearest = nearest.min(t);
        }
    }
    if dir.y < 0.0 {
        nearest = nearest.min(-origin.y / dir.y);
    }
    nearest
}

/// Distance and surface normal to the first wall, obstacle, or floor hit along the ray.
#[allow(dead_code)]
pub fn ray_world_normal(origin: Vec3, dir: Vec3, max: f32, colliders: &Boxes) -> Option<(f32, Vec3)> {
    let mut nearest = max;
    let mut hit_normal = None;
    for (center, half) in colliders {
        if let Some((t, normal)) = ray_aabb_hit(origin, dir, *center, *half) {
            if t < nearest {
                nearest = t;
                hit_normal = Some(normal);
            }
        }
    }
    if dir.y < -1e-6 {
        let t_floor = -origin.y / dir.y;
        if t_floor >= 0.0 && t_floor < nearest {
            nearest = t_floor;
            hit_normal = Some(Vec3::Y);
        }
    }
    hit_normal.map(|n| (nearest, n))
}

/// Clear line between two points?
pub fn line_of_sight(a: Vec3, b: Vec3, colliders: &Boxes) -> bool {
    let d = b - a;
    let len = d.length();
    if len < 1e-3 {
        return true;
    }
    ray_world(a, d / len, len, colliders) >= len - 0.05
}

#[derive(Clone, Copy, Debug)]
pub struct ShotHit {
    pub dist: f32,
    /// The enemy hit and whether it was a headshot.
    pub enemy: Option<(Entity, bool)>,
    pub legs: bool,
    pub penetrated: bool,
}

impl Default for ShotHit {
    fn default() -> Self {
        Self {
            dist: 0.0,
            enemy: None,
            legs: false,
            penetrated: false,
        }
    }
}

/// Finds the closest enemy hit along the ray, penetrating thin obstacles.
pub fn trace_shot(
    origin: Vec3,
    dir: Vec3,
    max: f32,
    colliders: &Boxes,
    enemies: impl Iterator<Item = (Entity, Vec3, f32, bool)>,
) -> ShotHit {
    let mut first_wall = max;
    let mut obstacle: Option<(usize, f32, f32)> = None;

    for (i, (center, half)) in colliders.iter().enumerate() {
        if let Some((t_enter, t_exit)) = ray_aabb_interval(origin, dir, *center, *half) {
            if t_enter < first_wall {
                first_wall = t_enter;
                obstacle = Some((i, t_enter, t_exit));
            }
        }
    }

    if dir.y < 0.0 {
        let t_floor = -origin.y / dir.y;
        if t_floor >= 0.0 && t_floor < first_wall {
            first_wall = t_floor;
            obstacle = None;
        }
    }

    let mut can_penetrate = false;
    let mut second_wall = max;

    if let Some((obs_idx, t_enter, t_exit)) = obstacle {
        let thickness = t_exit - t_enter;
        if thickness <= MAX_PENETRATION_THICKNESS {
            for (i, (center, half)) in colliders.iter().enumerate() {
                if i == obs_idx {
                    continue;
                }
                if let Some((t2_enter, t2_exit)) = ray_aabb_interval(origin, dir, *center, *half) {
                    if t2_exit > t_exit {
                        let block = t2_enter.max(t_exit);
                        second_wall = second_wall.min(block);
                    }
                }
            }
            if dir.y < 0.0 {
                let t_floor = -origin.y / dir.y;
                if t_floor >= 0.0 {
                    let block = t_floor.max(0.0);
                    if block > t_exit {
                        second_wall = second_wall.min(block);
                    } else {
                        second_wall = t_exit;
                    }
                }
            }
            if second_wall > t_exit + 1e-4 {
                can_penetrate = true;
            }
        }
    }

    let max_trace = if can_penetrate { second_wall } else { first_wall };
    let mut best = ShotHit {
        dist: max_trace,
        enemy: None,
        legs: false,
        penetrated: false,
    };

    for (e, feet, scale, crawl) in enemies {
        if let Some((t, zone)) = ray_enemy(origin, dir, feet, scale, crawl) {
            if t < best.dist {
                let penetrated = t >= first_wall;
                best = ShotHit {
                    dist: t,
                    enemy: Some((e, zone == Zone::Head)),
                    legs: zone == Zone::Legs,
                    penetrated,
                };
            }
        }
    }

    best
}

/// Shortest distance between the segments a-b and c-d.
/// Uses the analytical closest-points algorithm between two 3D line segments.
pub fn segment_distance(a: Vec3, b: Vec3, c: Vec3, d: Vec3) -> f32 {
    let (u, v, w) = (b - a, d - c, a - c);
    let (uu, uv, vv, uw, vw) = (u.dot(u), u.dot(v), v.dot(v), u.dot(w), v.dot(w));
    let denom = uu * vv - uv * uv;
    // Closest point on a-b to the line through c-d, then on c-d to that, then back.
    let mut s = if denom > 1e-8 {
        ((uv * vw - vv * uw) / denom).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let t = if vv > 1e-8 {
        ((uv * s + vw) / vv).clamp(0.0, 1.0)
    } else {
        0.0
    };
    if uu > 1e-8 {
        s = ((uv * t - uw) / uu).clamp(0.0, 1.0);
    }
    (a + u * s).distance(c + v * t)
}

/// Checks whether two capsules defined by base, top, and radius intersect.
#[allow(dead_code)]
pub fn capsule_capsule_intersect(
    cap_a_base: Vec3,
    cap_a_top: Vec3,
    radius_a: f32,
    cap_b_base: Vec3,
    cap_b_top: Vec3,
    radius_b: f32,
) -> bool {
    segment_distance(cap_a_base, cap_a_top, cap_b_base, cap_b_top) <= radius_a + radius_b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_step_up_low_curb() {
        // Curb at z = -1.0, width 4 (half 2), height 0.20 (half 0.10, center.y 0.10)
        let colliders: Boxes = vec![(Vec3::new(0.0, 0.10, -1.0), Vec3::new(2.0, 0.10, 0.5))];
        let radius = 0.4;
        let mut pos = Vec3::new(0.0, 0.0, -0.6); // overlaps the curb front edge at z = -0.5
        let initial_z = pos.z;

        let ground = resolve_collisions_with_step(
            &mut pos,
            radius,
            0.0,
            MAX_STEP_HEIGHT,
            &colliders,
        );

        // Player should be elevated onto the curb (0.20) and not pushed back in Z
        assert!((ground - 0.20).abs() < 1e-4);
        assert!((pos.y - 0.20).abs() < 1e-4);
        assert!((pos.z - initial_z).abs() < 1e-4);
    }

    #[test]
    fn test_tall_wall_pushes_back() {
        // Wall at z = -1.0, height 2.0 (center.y 1.0, half.y 1.0 -> top 2.0)
        let colliders: Boxes = vec![(Vec3::new(0.0, 1.0, -1.0), Vec3::new(2.0, 1.0, 0.5))];
        let radius = 0.4;
        let mut pos = Vec3::new(0.0, 0.0, -0.6);

        let ground = resolve_collisions_with_step(
            &mut pos,
            radius,
            0.0,
            MAX_STEP_HEIGHT,
            &colliders,
        );

        // Wall is too tall to step over: player pushed back in Z and stays at ground 0.0
        assert_eq!(ground, 0.0);
        assert_eq!(pos.y, 0.0);
        assert!(pos.z > -0.6); // pushed away from z = -0.5
        assert!((pos.z - (-0.1)).abs() < 1e-4); // edge is -0.5, pos pushed to -0.5 + 0.4 = -0.1
    }

    #[test]
    fn test_overhead_clearance_blocks_step() {
        // Curb at z = -1.0 (top = 0.20), but ceiling beam directly overhead at y = 1.5 (below 0.20 + 1.8)
        let colliders: Boxes = vec![
            (Vec3::new(0.0, 0.10, -1.0), Vec3::new(2.0, 0.10, 0.5)),
            (Vec3::new(0.0, 2.0, -1.0), Vec3::new(2.0, 0.5, 0.5)), // bottom is 1.5, top is 2.5
        ];
        let radius = 0.4;
        let mut pos = Vec3::new(0.0, 0.0, -0.6);

        let ground = resolve_collisions_with_step(
            &mut pos,
            radius,
            0.0,
            MAX_STEP_HEIGHT,
            &colliders,
        );

        // Stepping up is blocked by overhead ceiling; pushed back in Z
        assert_eq!(ground, 0.0);
        assert_eq!(pos.y, 0.0);
        assert!(pos.z > -0.6);
    }

    #[test]
    fn test_multi_pass_corner_resolution() {
        // Corner between wall at x = -1.0 (edge at -0.5) and wall at z = -1.0 (edge at -0.5)
        let colliders: Boxes = vec![
            (Vec3::new(-1.0, 1.0, 0.0), Vec3::new(0.5, 1.0, 2.0)),
            (Vec3::new(0.0, 1.0, -1.0), Vec3::new(2.0, 1.0, 0.5)),
        ];
        let radius = 0.4;
        let mut pos = Vec3::new(-0.4, 0.0, -0.4);

        resolve_collisions_with_step(
            &mut pos,
            radius,
            0.0,
            MAX_STEP_HEIGHT,
            &colliders,
        );

        // Player should be pushed out of both walls
        assert!(pos.x >= -0.1 - 1e-4);
        assert!(pos.z >= -0.1 - 1e-4);
    }

    #[test]
    fn test_zero_max_step_pushes_back() {
        // A low curb, but max_step is 0.0 (like default resolve_collisions)
        let colliders: Boxes = vec![(Vec3::new(0.0, 0.10, -1.0), Vec3::new(2.0, 0.10, 0.5))];
        let radius = 0.4;
        let mut pos = Vec3::new(0.0, 0.0, -0.6);

        resolve_collisions(&mut pos, radius, 0.0, &colliders);

        // Pushed back horizontally
        assert!(pos.z > -0.6);
        assert_eq!(pos.y, 0.0);
    }

    #[test]
    fn test_ground_height_with_step() {
        let colliders: Boxes = vec![(Vec3::new(0.0, 0.15, 0.0), Vec3::new(1.0, 0.15, 1.0))];
        let radius = 0.4;
        let pos = Vec3::new(0.0, 0.0, 0.0);

        let gh = ground_height(pos, radius, 0.0, &colliders);
        assert!((gh - 0.30).abs() < 1e-4);
    }

    #[test]
    fn test_ray_aabb_hit_and_normal() {
        // Box at (5.0, 2.0, 0.0) with half (1.0, 1.0, 1.0) -> x from 4 to 6
        let center = Vec3::new(5.0, 2.0, 0.0);
        let half = Vec3::splat(1.0);
        let origin = Vec3::new(0.0, 2.0, 0.0);
        let dir = Vec3::X;

        let hit = ray_aabb_hit(origin, dir, center, half);
        assert!(hit.is_some());
        let (dist, normal) = hit.unwrap();
        assert!((dist - 4.0).abs() < 1e-4);
        assert_eq!(normal, Vec3::NEG_X);

        // Ray from above
        let origin_top = Vec3::new(5.0, 5.0, 0.0);
        let dir_down = Vec3::NEG_Y;
        let hit_top = ray_aabb_hit(origin_top, dir_down, center, half);
        assert!(hit_top.is_some());
        let (dist_top, normal_top) = hit_top.unwrap();
        assert!((dist_top - 2.0).abs() < 1e-4);
        assert_eq!(normal_top, Vec3::Y);
    }

    #[test]
    fn test_ray_world_normal_wall_and_floor() {
        let colliders: Boxes = vec![(Vec3::new(10.0, 1.0, 0.0), Vec3::new(0.5, 1.0, 5.0))];
        // Hitting the wall at x = 9.5
        let hit = ray_world_normal(Vec3::new(0.0, 1.0, 0.0), Vec3::X, 20.0, &colliders);
        assert!(hit.is_some());
        let (dist, normal) = hit.unwrap();
        assert!((dist - 9.5).abs() < 1e-4);
        assert_eq!(normal, Vec3::NEG_X);

        // Hitting the floor
        let hit_floor = ray_world_normal(Vec3::new(0.0, 5.0, 0.0), Vec3::NEG_Y, 20.0, &colliders);
        assert!(hit_floor.is_some());
        let (dist_f, normal_f) = hit_floor.unwrap();
        assert!((dist_f - 5.0).abs() < 1e-4);
        assert_eq!(normal_f, Vec3::Y);
    }

    #[test]
    fn test_ray_aabb_interval() {
        let center = Vec3::new(5.0, 2.0, 0.0);
        let half = Vec3::splat(1.0);
        let origin = Vec3::new(0.0, 2.0, 0.0);
        let dir = Vec3::X;

        let interval = ray_aabb_interval(origin, dir, center, half);
        assert!(interval.is_some());
        let (t_enter, t_exit) = interval.unwrap();
        assert!((t_enter - 4.0).abs() < 1e-4);
        assert!((t_exit - 6.0).abs() < 1e-4);
    }

    #[test]
    fn test_bullet_penetrates_thin_wall_to_hit_enemy() {
        // Thin partition: 0.2m thick along Z (half.z = 0.1) centered at z = 2.0 (range [1.9, 2.1])
        let colliders: Boxes = vec![(Vec3::new(0.0, 1.0, 2.0), Vec3::new(2.0, 1.0, 0.1))];
        let origin = Vec3::new(0.0, 1.2, 0.0);
        let dir = Vec3::Z;
        let enemy_entity = Entity::from_raw_u32(1).unwrap();
        let enemy_feet = Vec3::new(0.0, 0.0, 5.0);
        let enemies = vec![(enemy_entity, enemy_feet, 1.0, false)];

        let hit = trace_shot(origin, dir, 50.0, &colliders, enemies.into_iter());
        assert!(hit.enemy.is_some(), "Enemy behind thin wall should be hit");
        let (e, _) = hit.enemy.unwrap();
        assert_eq!(e, enemy_entity);
        assert!(hit.penetrated, "Shot should be marked penetrated");
        assert!(hit.dist > 2.1, "Hit distance should be past the thin wall");
    }

    #[test]
    fn test_bullet_blocked_by_thick_wall() {
        // Thick obstacle: 1.2m thick along Z (half.z = 0.6) centered at z = 2.0 (range [1.4, 2.6])
        let colliders: Boxes = vec![(Vec3::new(0.0, 1.0, 2.0), Vec3::new(2.0, 1.0, 0.6))];
        let origin = Vec3::new(0.0, 1.2, 0.0);
        let dir = Vec3::Z;
        let enemy_entity = Entity::from_raw_u32(1).unwrap();
        let enemy_feet = Vec3::new(0.0, 0.0, 5.0);
        let enemies = vec![(enemy_entity, enemy_feet, 1.0, false)];

        let hit = trace_shot(origin, dir, 50.0, &colliders, enemies.into_iter());
        assert!(hit.enemy.is_none(), "Enemy behind thick wall should not be hit");
        assert_eq!(hit.penetrated, false, "Thick obstacle stops penetration");
        assert!((hit.dist - 1.4).abs() < 1e-4, "Bullet should stop at wall front (1.4)");
    }

    #[test]
    fn test_bullet_blocked_by_double_wall() {
        // Two 0.25m partitions separated by a gap
        // Wall 1: centered at z = 2.0, half.z = 0.125 -> range [1.875, 2.125]
        // Wall 2: centered at z = 3.5, half.z = 0.125 -> range [3.375, 3.625]
        let colliders: Boxes = vec![
            (Vec3::new(0.0, 1.0, 2.0), Vec3::new(2.0, 1.0, 0.125)),
            (Vec3::new(0.0, 1.0, 3.5), Vec3::new(2.0, 1.0, 0.125)),
        ];
        let origin = Vec3::new(0.0, 1.2, 0.0);
        let dir = Vec3::Z;
        let enemy_entity = Entity::from_raw_u32(1).unwrap();
        let enemy_feet = Vec3::new(0.0, 0.0, 5.0);
        let enemies = vec![(enemy_entity, enemy_feet, 1.0, false)];

        let hit = trace_shot(origin, dir, 50.0, &colliders, enemies.into_iter());
        assert!(hit.enemy.is_none(), "Double wall should stop bullet from reaching enemy");
        assert_eq!(hit.penetrated, false, "Bullet blocked by second wall");
        assert!((hit.dist - 3.375).abs() < 1e-4, "Bullet should stop at the second wall front (3.375)");
    }

    #[test]
    fn test_segment_distance_intersecting() {
        // Two segments crossing at (0, 0, 0)
        let a = Vec3::new(-1.0, 0.0, 0.0);
        let b = Vec3::new(1.0, 0.0, 0.0);
        let c = Vec3::new(0.0, -1.0, 0.0);
        let d = Vec3::new(0.0, 1.0, 0.0);
        let dist = segment_distance(a, b, c, d);
        assert!(dist < 1e-5, "Intersecting segments should have distance 0, got {}", dist);
    }

    #[test]
    fn test_segment_distance_parallel_overlapping_and_separated() {
        // Parallel along X, separated by 2 units in Y
        let a = Vec3::new(0.0, 0.0, 0.0);
        let b = Vec3::new(2.0, 0.0, 0.0);
        let c = Vec3::new(0.0, 2.0, 0.0);
        let d = Vec3::new(2.0, 2.0, 0.0);
        let dist = segment_distance(a, b, c, d);
        assert!((dist - 2.0).abs() < 1e-5, "Parallel segments distance should be 2.0, got {}", dist);

        // Collinear disjoint segments along X
        let c2 = Vec3::new(3.0, 0.0, 0.0);
        let d2 = Vec3::new(5.0, 0.0, 0.0);
        let dist2 = segment_distance(a, b, c2, d2);
        assert!((dist2 - 1.0).abs() < 1e-5, "Collinear separated segments distance should be 1.0, got {}", dist2);
    }

    #[test]
    fn test_segment_distance_perpendicular_skew() {
        // Skew perpendicular segments: one along X at y=0, z=0; one along Y at x=0, z=3
        let a = Vec3::new(-2.0, 0.0, 0.0);
        let b = Vec3::new(2.0, 0.0, 0.0);
        let c = Vec3::new(0.0, -2.0, 3.0);
        let d = Vec3::new(0.0, 2.0, 3.0);
        let dist = segment_distance(a, b, c, d);
        assert!((dist - 3.0).abs() < 1e-5, "Perpendicular skew segments shortest distance should be 3.0, got {}", dist);

        // Skew perpendicular segments where closest point is an endpoint
        let c_off = Vec3::new(5.0, 0.0, 3.0);
        let d_off = Vec3::new(5.0, 4.0, 3.0);
        let dist_off = segment_distance(a, b, c_off, d_off);
        // Closest point on AB is (2, 0, 0). Closest point on CD is (5, 0, 3).
        // Distance is sqrt((5-2)^2 + 0 + 3^2) = sqrt(9 + 9) = sqrt(18) ≈ 4.24264
        let expected = (3.0_f32.powi(2) + 3.0_f32.powi(2)).sqrt();
        assert!((dist_off - expected).abs() < 1e-4, "Endpoint clamp distance should be {}, got {}", expected, dist_off);
    }

    #[test]
    fn test_capsule_capsule_intersect() {
        // Capsule A: vertical segment from (0, 0, 0) to (0, 2, 0), radius 0.5
        let cap_a_base = Vec3::new(0.0, 0.0, 0.0);
        let cap_a_top = Vec3::new(0.0, 2.0, 0.0);
        let radius_a = 0.5;

        // Capsule B: vertical segment from (0.8, 0.0, 0.0) to (0.8, 2.0, 0.0), radius 0.5
        // Distance between axes is 0.8. Sum of radii is 1.0 -> should intersect
        let cap_b_base = Vec3::new(0.8, 0.0, 0.0);
        let cap_b_top = Vec3::new(0.8, 2.0, 0.0);
        let radius_b = 0.5;
        assert!(capsule_capsule_intersect(cap_a_base, cap_a_top, radius_a, cap_b_base, cap_b_top, radius_b));

        // Capsule C: separated further at x = 1.2 -> distance 1.2 > 1.0 -> no intersection
        let cap_c_base = Vec3::new(1.2, 0.0, 0.0);
        let cap_c_top = Vec3::new(1.2, 2.0, 0.0);
        assert!(!capsule_capsule_intersect(cap_a_base, cap_a_top, radius_a, cap_c_base, cap_c_top, radius_b));

        // Horizontal capsule cutting across vertical capsule (e.g. sword swing through body)
        let swing_base = Vec3::new(-1.0, 1.0, 0.0);
        let swing_top = Vec3::new(1.0, 1.0, 0.0);
        let swing_radius = 0.2;
        assert!(capsule_capsule_intersect(cap_a_base, cap_a_top, radius_a, swing_base, swing_top, swing_radius));
    }
}

