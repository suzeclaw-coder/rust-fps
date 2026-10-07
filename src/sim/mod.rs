//! Host-side game simulation: rounds, enemy AI, damage, elements, points, XP
//! and levels, abilities, the mystery box, perks, power-ups, bosses and the
//! teleporter between the maps of a run.
//! Only the host (or a solo player) runs this; clients mirror its results.

use bevy::prelude::*;
use rand::Rng;
use std::collections::HashMap;

use crate::avatars::spawn_replicated;
use crate::data::{
    alt_fire, elements_in, gun_def, has_perk, roll_armory, tier_mult, AltFire, Attach,
    GRENADE_RECHARGE,
    xp_to_next, Element, GunSpecial, Perk, PowerUp, Stat, Upgrade, AMMO_COST, BOX_COST, MAX_LEVEL,
    ROUNDS_PER_STAGE, STAGES,
    MAX_AUGMENT, MAX_GUN_TIER, MAX_TIER,
};
use crate::fx::{emit, rgb, Fx, FxOutbox, FxQueue};
use crate::maps::{CurrentMap, EXTRACT_RADIUS};
use crate::nav::NavGrid;
use crate::physics::{
    collect_boxes, line_of_sight, resolve_collisions, trace_shot, Boxes,
};
use crate::weapons::GUN_RANGE;
use crate::{
    ActionQueue, AppState, BoxState, Collider, EnemyStatus, MatchState, NetKind, Phase,
    PlayerAction, PlayerInfo, Replicated, Roster, Session, ShotQueue, EYE_HEIGHT,
};

const REVIVE_HEALTH_FRACTION: f32 = 0.5;
const INTERACT_RANGE: f32 = 2.6;
const POWERUP_CHANCE: f64 = 0.03;
/// Seconds after a drop before another can drop.
const POWERUP_GAP: f32 = 25.0;
const REGEN_DELAY: f32 = 4.0;
const REGEN_RATE: f32 = 20.0;

pub mod powers;

pub struct SimPlugin;

impl Plugin for SimPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DamageQueue>()
            .init_resource::<DevQueue>()
            .init_resource::<Strikes>()
            .init_resource::<Zones>()
            .init_resource::<powers::Forces>()
            .init_resource::<LastHurt>()
            .init_resource::<Summons>()
            .init_resource::<SubWavePacer>()
            .add_systems(
                Update,
                (
                    sandbox,
                    resolve_shots,
                    player_timers,
                    status_effects,
                    projectiles,
                    powers::grenades,
                    powers::missiles,
                    powers::mines,
                    powers::strikes,
                    zones,
                    powers::turrets,
                    powers::timers,
                    apply_damage,
                    rounds,
                    enemy_ai,
                    powerups,
                    mystery_box,
                    teleporter,
                    check_game_over,
                )
                    .chain()
                    .in_set(Phase::Sim),
            )
            // Actions also run while a solo game is paused, so level-up
            // picks made in the menu apply straight away.
            .add_systems(
                Update,
                process_actions
                    .after(Phase::Local)
                    .before(Phase::Sim)
                    .run_if(in_state(AppState::InGame).and(is_authority)),
            )
            .add_systems(OnEnter(AppState::InGame), clear_sim);
    }
}

fn is_authority(session: Res<Session>) -> bool {
    session.is_authority()
}

pub fn enemy_scale(kind: NetKind) -> f32 {
    match kind {
        NetKind::Brute => 1.35,
        NetKind::Boss(0) => 2.3,
        NetKind::Boss(_) => 3.0,
        NetKind::Shooter => 1.05,
        _ => 1.0,
    }
}

#[derive(Component)]
pub struct EnemyBrain {
    pub kind: NetKind,
    pub health: f32,
    pub knockback: Vec3,
    speed: f32,
    attack_timer: f32,
    burn: f32,
    burn_dps: f32,
    burn_by: Option<u8>,
    slow: f32,
    max_health: f32,
    leg_damage: f32,
    /// Legs shot out: crawls along the ground.
    pub crawler: bool,
    /// Seconds since the last swing (for the attack animation).
    pub swing: f32,
    /// Stunned: can't move or attack.
    pub stun: f32,
    /// Brief flinch / stagger from heavy single-hit damage or critical hits.
    pub flinch: f32,
    /// A Brute slam is winding up (it lands `slam_spec` windup after the swing starts).
    slam: bool,
    /// Bosses: seconds to the next fireball volley, and summons used (at
    /// two-thirds and one-third health).
    volley: f32,
    summons: u8,
    /// Poisoned (Chemist and Medic): damage over time while above zero.
    poison: f32,
    poison_dps: f32,
    poison_by: Option<u8>,
    /// Hunter's Mark: takes extra damage while above zero.
    marked: f32,
    /// Fast sprinter variant (1.4x speed, 0.7x HP, faster attack swings).
    pub is_sprinter: bool,
    /// Lateral flank offset bias (-0.6..0.6) to spread swarm out.
    pub flank_bias: f32,
    /// Attack telegraph windup timer (seconds remaining before a melee bite/strike lands, target_id).
    pub attack_windup: Option<(f32, u8)>,
    pub poise: f32,
    pub max_poise: f32,
    pub poise_broken: f32,
    pub since_hit: f32,
}

/// How a Brute or boss slam lands: windup seconds, radius, how far ahead
/// of it the circle is, and damage.
pub fn slam_spec(kind: NetKind) -> (f32, f32, f32, f32) {
    match kind {
        NetKind::Boss(0) => (0.7, 3.4, 1.9, 45.0),
        NetKind::Boss(_) => (0.6, 4.2, 2.4, 60.0),
        _ => (BRUTE_WINDUP, BRUTE_SLAM_RADIUS, BRUTE_SLAM_AHEAD, 35.0),
    }
}

/// Zombies a boss calls in (handled by `rounds`, which can spawn them).
#[derive(Resource, Default)]
struct Summons(Vec<(Vec3, NetKind)>);

/// Pacing manager for sub-wave burst spawning and micro-breathers.
#[derive(Resource, Default)]
pub struct SubWavePacer {
    /// How many zombies have been spawned in the current sub-wave burst.
    pub spawned_in_burst: u32,
    /// Target burst size before triggering a micro-breather.
    pub burst_target: u32,
    /// Active micro-breather cooldown.
    pub breather_timer: f32,
}

/// Zombie hits get harder on each map of a run.
fn stage_damage(state: &MatchState) -> f32 {
    1.0 + 0.15 * state.stage as f32
}

/// How long a Brute's slam takes to land, so players can see it coming.
pub const BRUTE_WINDUP: f32 = 0.45;
/// The slam hits everyone in this circle, `BRUTE_SLAM_AHEAD` in front of the Brute.
pub const BRUTE_SLAM_RADIUS: f32 = 2.0;
pub const BRUTE_SLAM_AHEAD: f32 = 1.0;
/// Shooters' fireballs fly straight at this speed.
pub const FIREBALL_SPEED: f32 = 15.0;
/// Ballistic gravity acceleration pulling projectiles downward over distance.
pub const PROJECTILE_GRAVITY: f32 = 4.0;

#[derive(Component)]
pub struct FireballBrain {
    pub velocity: Vec3,
    pub life: f32,
    pub damage: f32,
}

impl FireballBrain {
    #[allow(dead_code)]
    pub fn new(velocity: Vec3, life: f32, damage: f32) -> Self {
        Self {
            velocity,
            life,
            damage,
        }
    }
}

/// A lasting damage area (see `Fx::Zone` for the kinds).
struct Zone {
    owner: u8,
    pos: Vec3,
    follow: Option<u8>,
    radius: f32,
    damage: f32,
    interval: f32,
    timer: f32,
    life: f32,
    elements: u8,
    kind: u8,
    /// A model that goes away with the zone.
    model: Option<Entity>,
    /// Heals players inside by this much each tick.
    heal: f32,
    /// Spreads this many metres a second (Plague Bloom), up to `max_radius`.
    grow: f32,
    max_radius: f32,
}

#[derive(Resource, Default)]
struct Zones(Vec<Zone>);

#[derive(Component)]
pub struct PowerUpBrain {
    kind: PowerUp,
    life: f32,
}

struct DamageEvent {
    target: Entity,
    amount: f32,
    from: Option<u8>,
    headshot: bool,
    /// Hit in the legs (enough of these make it a crawler).
    legs: bool,
    elements: u8,
    chained: bool,
    /// Seconds the target is stunned (frozen, choking, knocked down).
    stun: f32,
    pub knockback: Vec3,
}

#[derive(Resource, Default)]
struct DamageQueue(Vec<DamageEvent>);

/// Delayed hits: orbital strikes, bombs, meteors, cuts (see powers.rs).
#[derive(Resource, Default)]
struct Strikes(Vec<powers::Strike>);

/// Sandbox tools waiting to run: (player, tool).
#[derive(Resource, Default)]
struct DevQueue(Vec<(u8, crate::DevCmd)>);

/// When each player last took damage (for health regeneration).
#[derive(Resource, Default)]
struct LastHurt(HashMap<u8, f32>);

fn clear_sim(
    mut damage: ResMut<DamageQueue>,
    mut strikes: ResMut<Strikes>,
    mut hurt: ResMut<LastHurt>,
    mut zones: ResMut<Zones>,
    mut forces: ResMut<powers::Forces>,
    mut pacer: ResMut<SubWavePacer>,
) {
    zones.0.clear();
    forces.0.clear();
    damage.0.clear();
    strikes.0.clear();
    hurt.0.clear();
    pacer.spawned_in_burst = 0;
    pacer.burst_target = 0;
    pacer.breather_timer = 0.0;
}

fn hurt_player(p: &mut PlayerInfo, amount: f32, hurt: &mut LastHurt) {
    p.damage(amount);
    hurt.0.insert(p.id, 0.0);
}

/// Damage every enemy within `radius` of `pos`.
fn explode(
    pos: Vec3,
    radius: f32,
    damage: f32,
    from: Option<u8>,
    elements: u8,
    enemies: &[(Entity, Vec3)],
    queue: &mut DamageQueue,
    fx: &mut FxQueue,
    out: &mut FxOutbox,
    color: Color,
) {
    for (e, p) in enemies {
        let d = (*p + Vec3::Y).distance(pos);
        if d < radius {
            let falloff = 1.0 - 0.5 * (d / radius);
            let dir = ((*p + Vec3::Y * 0.5) - pos).normalize_or(Vec3::Y);
            let force = (1.0 - (d / radius).clamp(0.0, 1.0)) * 14.0;
            queue.0.push(DamageEvent {
                target: *e,
                amount: damage * falloff,
                from,
                headshot: false,
                legs: false,
                elements,
                chained: false,
                stun: 0.0,
                knockback: dir * force,
            });
        }
    }
    emit(
        fx,
        out,
        Fx::Explosion {
            pos: pos.to_array(),
            radius,
            color: rgb(color),
        },
    );
}

fn level_multiplier(p: &PlayerInfo) -> f32 {
    1.0 + 0.03 * (p.level.saturating_sub(1)) as f32
}

/// Three random level-up rewards. Every tenth level always includes an
/// element if one is left.
fn roll_choices(p: &PlayerInfo) -> Vec<Upgrade> {
    let mut rng = rand::thread_rng();
    let mut abilities: Vec<Upgrade> = (0..3u8)
        .filter(|s| p.tiers[*s as usize] < MAX_TIER)
        .map(Upgrade::Ability)
        .collect();
    let mut elements: Vec<Upgrade> = Vec::new();
    for (i, e) in Element::ALL.iter().enumerate() {
        if p.gun_elements & e.bit() == 0 {
            elements.push(Upgrade::GunElement(i as u8));
        }
        if p.ability_elements & e.bit() == 0 {
            elements.push(Upgrade::AbilityElement(i as u8));
        }
    }
    let stats = Stat::ALL
        .into_iter()
        .filter(|s| p.stats[*s as usize] < Stat::MAX_STACKS)
        .map(Upgrade::Stat);
    let augments = (0..2u8)
        .filter(|s| p.augments[*s as usize] < MAX_AUGMENT)
        .map(Upgrade::Augment);
    let weapons = (0..2u8)
        .filter(|s| p.guns[*s as usize].is_some() && p.gun_tiers[*s as usize] < MAX_GUN_TIER)
        .map(Upgrade::Weapon);
    let mut picks = Vec::new();
    if p.level % 10 == 0 && !elements.is_empty() {
        picks.push(elements.swap_remove(rng.gen_range(0..elements.len())));
    }
    let mut pool: Vec<Upgrade> = abilities.drain(..).chain(elements).chain(stats).chain(weapons).chain(augments).collect();
    while picks.len() < 3 && !pool.is_empty() {
        picks.push(pool.swap_remove(rng.gen_range(0..pool.len())));
    }
    picks
}

fn give_xp(p: &mut PlayerInfo, amount: u32) {
    if p.level >= MAX_LEVEL {
        return;
    }
    p.xp += amount;
    while p.level < MAX_LEVEL && p.xp >= xp_to_next(p.level) {
        p.xp -= xp_to_next(p.level);
        p.level += 1;
        if p.level % 2 == 0 {
            p.pending_picks += 1;
        }
    }
    offer_picks(p);
}

/// Rolls the next three choices if a pick is waiting.
fn offer_picks(p: &mut PlayerInfo) {
    if p.pending_picks > 0 && p.choices.is_empty() {
        p.choices = roll_choices(p);
        if p.choices.is_empty() {
            p.pending_picks = 0;
        }
    }
}

fn give_points(p: &mut PlayerInfo, amount: u32, state: &MatchState) {
    let amount = if state.double_points > 0.0 {
        amount * 2
    } else {
        amount
    };
    p.points += amount;
    p.score += amount;
}

// ---------------------------------------------------------------------------
// Player actions: interact, abilities, level-up picks
// ---------------------------------------------------------------------------

fn process_actions(
    mut commands: Commands,
    mut actions: ResMut<ActionQueue>,
    mut roster: ResMut<Roster>,
    mut state: ResMut<MatchState>,
    map: Res<CurrentMap>,
    mut damage: ResMut<DamageQueue>,
    mut strikes: ResMut<Strikes>,
    mut zones: ResMut<Zones>,
    mut forces: ResMut<powers::Forces>,
    mut fx: ResMut<FxQueue>,
    mut out: ResMut<FxOutbox>,
    enemies: Query<(Entity, &Transform, &EnemyBrain)>,
    colliders: Query<(&Transform, &Collider)>,
    mut dev: ResMut<DevQueue>,
) {
    if actions.0.is_empty() {
        return;
    }
    let enemy_list: Vec<(Entity, Vec3)> =
        enemies.iter().map(|(e, t, _)| (e, t.translation)).collect();
    for (id, seq, action) in std::mem::take(&mut actions.0) {
        let Some(p) = roster.0.get_mut(&id) else {
            continue;
        };
        if seq <= p.action_ack {
            continue;
        }
        p.action_ack = seq;
        if state.game_over || state.won {
            continue;
        }
        match action {
            PlayerAction::Dev(cmd) => {
                if !state.sandbox.on {
                    continue;
                }
                use crate::DevCmd;
                match cmd {
                    DevCmd::Toggle(n) => {
                        let s = &mut state.sandbox;
                        match n {
                            0 => s.waves = !s.waves,
                            1 => s.god = !s.god,
                            _ => s.free_abilities = !s.free_abilities,
                        }
                    }
                    DevCmd::NextRound => state.round += 1,
                    DevCmd::Points => p.points += 10_000,
                    DevCmd::LevelUp => {
                        let need = (p.level * 100).max(100);
                        give_xp(p, need);
                    }
                    DevCmd::Gun { gun, attach } => {
                        if (gun as usize) < crate::data::GUNS.len()
                            && crate::data::attach_options_for(gun).fits(attach)
                        {
                            let slot = if let Some(i) = p.guns.iter().position(|g| *g == Some(gun))
                            {
                                i
                            } else if p.guns[1].is_none() {
                                1
                            } else {
                                p.active_slot as usize
                            };
                            p.guns[slot] = Some(gun);
                            p.attach[slot] = attach;
                            p.active_slot = slot as u8;
                            p.supply_seq = p.supply_seq.wrapping_add(1);
                        }
                    }
                    DevCmd::Spawn { .. } | DevCmd::KillAll => dev.0.push((id, cmd)),
                }
            }
            PlayerAction::Choose(i) => {
                let Some(choice) = p.choices.get(i as usize).copied() else {
                    continue;
                };
                match choice {
                    Upgrade::Ability(s) => {
                        let t = &mut p.tiers[s as usize];
                        *t = (*t + 1).min(MAX_TIER);
                    }
                    Upgrade::GunElement(e) => p.gun_elements |= Element::ALL[e as usize].bit(),
                    Upgrade::AbilityElement(e) => {
                        p.ability_elements |= Element::ALL[e as usize].bit()
                    }
                    Upgrade::Stat(st) => {
                        let n = &mut p.stats[st as usize];
                        *n = (*n + 1).min(Stat::MAX_STACKS);
                    }
                    Upgrade::Weapon(s) => {
                        let t = &mut p.gun_tiers[s as usize & 1];
                        *t = (*t + 1).min(MAX_GUN_TIER);
                    }
                    Upgrade::Augment(s) => {
                        let s = s as usize & 1;
                        if p.augments[s] < MAX_AUGMENT {
                            p.augments[s] += 1;
                            if p.kit[s].augment() == crate::data::Augment::Charges {
                                p.charges[s] += 1;
                            }
                        }
                    }
                }
                p.pending_picks = p.pending_picks.saturating_sub(1);
                p.choices = if p.pending_picks > 0 {
                    roll_choices(p)
                } else {
                    Vec::new()
                };
            }
            PlayerAction::WeaponAbility(i) => {
                let i = i as usize;
                if !p.alive || i > 1 || p.weapon_cd[i] > 0.0 {
                    continue;
                }
                let ability = p.character.weapon_abilities()[i];
                let focus = 1.0 - p.stat(Stat::Focus) * Stat::Focus.per_stack();
                p.weapon_cd[i] = ability.cooldown() * focus;
                p.buff = Some(ability);
                p.buff_time = ability.def().duration;
                emit(
                    &mut fx,
                    &mut out,
                    Fx::Ring {
                        pos: p.pos,
                        radius: 2.5,
                        color: ability.def().color,
                    },
                );
            }
            PlayerAction::Ping { pos, target } => {
                if pos.iter().all(|v| v.is_finite()) {
                    emit(
                        &mut fx,
                        &mut out,
                        Fx::Ping {
                            player: id,
                            pos,
                            target,
                        },
                    );
                }
            }
            PlayerAction::Melee { origin, dir } => {
                let origin = Vec3::from_array(origin);
                let dir = Vec3::from_array(dir).normalize_or_zero();
                if !p.alive || !origin.is_finite() || dir == Vec3::ZERO {
                    continue;
                }
                // The nearest zombie in reach takes the slash.
                let target = enemies
                    .iter()
                    .filter(|(_, _, b)| b.health > 0.0)
                    .filter_map(|(e, t, b)| {
                        crate::weapons::melee_reach(
                            origin,
                            dir,
                            t.translation,
                            enemy_scale(b.kind),
                            b.crawler,
                        )
                        .map(|d| (e, d, b, t.translation))
                    })
                    .min_by(|a, b| a.1.total_cmp(&b.1));
                if let Some((e, _, b, e_pos)) = target {
                    // Bosses only lose a sliver to a knife.
                    let share = if b.kind.is_boss() { 0.01 } else { 0.2 };
                    let amount = (150.0 + share * b.max_health) * level_multiplier(p);
                    // A melee kill is worth more than a shot kill.
                    if amount >= b.health || state.insta_kill > 0.0 {
                        give_points(p, 70, &state);
                    }
                    emit(
                        &mut fx,
                        &mut out,
                        Fx::Blood {
                            pos: (e_pos + Vec3::Y * 1.1).to_array(),
                            dir: dir.to_array(),
                            headshot: false,
                        },
                    );
                    damage.0.push(DamageEvent {
                        target: e,
                        amount,
                        from: Some(id),
                        headshot: false,
                        legs: false,
                        elements: 0,
                        chained: false,
                        stun: 0.0,
                        knockback: dir * 6.0,
                    });
                }
            }
            PlayerAction::Interact => {
                if !p.alive {
                    continue;
                }
                let feet = p.feet();
                let box_pos = map.0.box_spots[state.box_spot as usize];
                let near_box = feet.with_y(0.0).distance(box_pos) < INTERACT_RANGE;
                match state.box_state {
                    BoxState::Offer {
                        player,
                        gun,
                        attach,
                        ..
                    } if near_box && player == id => {
                        // New attachments go on that gun; a wonder weapon
                        // replaces the gun in your hands.
                        let slot = match p.guns.iter().position(|g| *g == Some(gun)) {
                            Some(i) => i,
                            None => {
                                let i = p.active_slot as usize;
                                p.gun_tiers[i] = 0;
                                i
                            }
                        };
                        p.guns[slot] = Some(gun);
                        p.attach[slot] = attach;
                        p.active_slot = slot as u8;
                        p.supply_seq = p.supply_seq.wrapping_add(1);
                        state.box_state = BoxState::Idle;
                        continue;
                    }
                    BoxState::Idle if near_box => {
                        if p.points >= BOX_COST {
                            p.points -= BOX_COST;
                            state.box_uses += 1;
                            state.box_state = BoxState::Rolling {
                                player: id,
                                time: 3.0,
                            };
                        }
                        continue;
                    }
                    _ => {}
                }
                // Ammo caches on the walls refill both guns.
                if map.0.wall_buys.iter().any(|w| w.near(feet)) {
                    if p.points >= AMMO_COST {
                        p.points -= AMMO_COST;
                        p.supply_seq = p.supply_seq.wrapping_add(1);
                    }
                    continue;
                }
                for (spot, perk) in map.0.perk_spots.iter().zip(Perk::ALL) {
                    if feet.with_y(0.0).distance(*spot) < INTERACT_RANGE
                        && !has_perk(p.perks, perk)
                        && p.points >= perk.cost()
                    {
                        p.points -= perk.cost();
                        p.perks |= perk.bit();
                        if perk == Perk::Juggernaut {
                            p.health = p.max_health();
                        }
                    }
                }
            }
            PlayerAction::Ability {
                slot,
                origin,
                dir,
                charge,
            } => {
                if !p.alive || slot > 2 {
                    continue;
                }
                let s = slot as usize;
                let copies = p.copies(s);
                if slot == 2 {
                    if p.ult_charge < 100.0 {
                        continue;
                    }
                    p.ult_charge = 0.0;
                } else {
                    if p.charges[s] == 0 {
                        continue;
                    }
                    p.charges[s] -= 1;
                    if p.cooldowns[s] <= 0.0 {
                        p.cooldowns[s] = p.ability_cooldown(s);
                    }
                }
                let origin = Vec3::from_array(origin);
                let dir = Vec3::from_array(dir).normalize_or_zero();
                if !origin.is_finite() || dir == Vec3::ZERO {
                    continue;
                }
                let charge = if charge.is_finite() {
                    charge.clamp(0.0, 1.0)
                } else {
                    0.0
                };
                emit(&mut fx, &mut out, Fx::Cast { player: id, slot });
                let boxes = collect_boxes(colliders.iter());
                let mut world = powers::World {
                    commands: &mut commands,
                    state: &mut state,
                    damage: &mut damage,
                    strikes: &mut strikes,
                    zones: &mut zones,
                    forces: &mut forces,
                    fx: &mut fx,
                    out: &mut out,
                    enemies: &enemy_list,
                    boxes: &boxes,
                };
                // Augmented casts fan out extra copies either side.
                for i in 0..copies {
                    let side = if i % 2 == 1 { 1.0 } else { -1.0 };
                    let turn = side * ((i + 1) / 2) as f32 * crate::data::COPY_SPREAD;
                    let d = Quat::from_rotation_y(turn) * dir;
                    powers::cast(&mut world, &mut roster, id, slot, origin, d, charge);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Shooting and damage
// ---------------------------------------------------------------------------

fn resolve_shots(
    session: Res<Session>,
    state: Res<MatchState>,
    mut roster: ResMut<Roster>,
    mut shots: ResMut<ShotQueue>,
    mut damage: ResMut<DamageQueue>,
    mut fx: ResMut<FxQueue>,
    mut out: ResMut<FxOutbox>,
    enemies: Query<(Entity, &Transform, &EnemyBrain)>,
    colliders: Query<(&Transform, &Collider)>,
) {
    if shots.0.is_empty() {
        return;
    }
    let boxes = collect_boxes(colliders.iter());
    let enemy_list: Vec<(Entity, Vec3)> =
        enemies.iter().map(|(e, t, _)| (e, t.translation)).collect();
    let mut rng = rand::thread_rng();
    for (shooter, shot) in shots.0.drain(..) {
        let Some(p) = roster.0.get_mut(&shooter) else {
            continue;
        };
        // Only guns you actually hold.
        if !p.alive || !p.guns.contains(&Some(shot.gun)) {
            continue;
        }
        let alt = if shot.alt { alt_fire(shot.gun) } else { AltFire::Sights };
        if alt == AltFire::Grenade {
            if p.grenade_cd > 0.0 {
                continue;
            }
            p.grenade_cd = GRENADE_RECHARGE;
        }
        let p = &*p;
        let origin = Vec3::from_array(shot.origin);
        let dir = Vec3::from_array(shot.dir).normalize_or_zero();
        if !origin.is_finite() || dir == Vec3::ZERO {
            continue;
        }
        let def = gun_def(shot.gun);
        let hit = trace_shot(
            origin,
            dir,
            GUN_RANGE,
            &boxes,
            enemies
                .iter()
                .filter(|(_, _, b)| b.health > 0.0)
                .map(|(e, t, b)| (e, t.translation, enemy_scale(b.kind), b.crawler)),
        );
        let end = origin + dir * hit.dist;
        let tracer = Fx::Tracer {
            shooter,
            a: (origin - Vec3::Y * 0.25).to_array(),
            b: end.to_array(),
        };
        if shooter == session.my_id {
            out.0.push(tracer);
        } else {
            emit(&mut fx, &mut out, tracer);
        }

        let slot = p
            .guns
            .iter()
            .position(|g| *g == Some(shot.gun))
            .unwrap_or(0);
        let buff = p.gun_buff();
        let mult = level_multiplier(p)
            * p.attach[slot].handling(shot.gun).damage
            * (1.0 + p.stat(Stat::Firepower) * Stat::Firepower.per_stack())
            * tier_mult(p.gun_tiers[slot])
            * buff.damage;
        let elements = p.gun_elements | buff.elements;
        if alt == AltFire::Grenade {
            explode(
                end - dir * 0.3,
                4.5,
                (250.0 + 30.0 * state.round as f32) * mult,
                Some(shooter),
                elements,
                &enemy_list,
                &mut damage,
                &mut fx,
                &mut out,
                Color::srgb(1.0, 0.55, 0.2),
            );
            continue;
        }
        if buff.blast > 0.0 {
            explode(
                end - dir * 0.3,
                buff.blast,
                def.damage * mult * 0.5,
                Some(shooter),
                elements,
                &enemy_list,
                &mut damage,
                &mut fx,
                &mut out,
                Color::srgb(1.0, 0.4, 0.1),
            );
        }
        if let GunSpecial::Explosive { radius } = def.special {
            explode(
                end - dir * 0.3,
                radius,
                def.damage * mult,
                Some(shooter),
                elements,
                &enemy_list,
                &mut damage,
                &mut fx,
                &mut out,
                Color::srgb(0.3, 1.0, 0.4),
            );
            continue;
        }
        let Some((entity, headshot)) = hit.enemy else {
            continue;
        };
        emit(
            &mut fx,
            &mut out,
            Fx::Blood {
                pos: end.to_array(),
                dir: dir.to_array(),
                headshot,
            },
        );
        // A slug hits as hard as the whole spread, a little less.
        let pellets = if alt == AltFire::Slug {
            def.pellets as f32 * 0.9
        } else {
            1.0
        };
        let mut amount = def.damage * mult * pellets * if headshot { def.headshot } else { 1.0 };
        if hit.penetrated {
            amount *= crate::physics::PENETRATION_DAMAGE_FACTOR;
        }
        if buff.execute > 0.0 {
            if let Ok((_, _, b)) = enemies.get(entity) {
                if !b.kind.is_boss() && b.health - amount < buff.execute * b.max_health {
                    amount = amount.max(b.health);
                }
            }
        }
        damage.0.push(DamageEvent {
            target: entity,
            amount,
            from: Some(shooter),
            headshot,
            legs: hit.legs,
            elements,
            chained: false,
            stun: buff.stun,
            knockback: Vec3::ZERO,
        });
        let jumps = match def.special {
            GunSpecial::Chain { jumps } => jumps as u8 + buff.chain,
            _ => buff.chain,
        };
        if jumps > 0 {
            let mut from = end;
            let mut others: Vec<(Entity, Vec3)> = enemy_list
                .iter()
                .filter(|(e, _)| *e != entity)
                .copied()
                .collect();
            others.sort_by(|a, b| a.1.distance(end).total_cmp(&b.1.distance(end)));
            for (e, pos) in others.into_iter().take(jumps as usize) {
                if pos.distance(end) > 14.0 {
                    break;
                }
                let to = pos + Vec3::Y * 1.2;
                emit(
                    &mut fx,
                    &mut out,
                    Fx::Lightning {
                        a: from.to_array(),
                        b: to.to_array(),
                    },
                );
                from = to;
                damage.0.push(DamageEvent {
                    target: e,
                    amount: def.damage * mult * pellets * 0.8,
                    from: Some(shooter),
                    headshot: false,
                    legs: false,
                    elements,
                    chained: true,
                    stun: 0.0,
                    knockback: Vec3::ZERO,
                });
            }
        }
        if headshot && has_perk(p.perks, Perk::BoomShot) && rng.gen_bool(0.3) {
            explode(
                end,
                3.0,
                (100.0 + 10.0 * state.round as f32) * mult,
                Some(shooter),
                elements,
                &enemy_list,
                &mut damage,
                &mut fx,
                &mut out,
                Color::srgb(1.0, 0.5, 0.1),
            );
        }
        // Soul Siphon: hits heal the shooter.
        if buff.lifesteal > 0.0 {
            if let Some(p) = roster.0.get_mut(&shooter) {
                p.health = (p.health + amount * buff.lifesteal).min(p.max_health());
            }
        }
    }
}

fn status_effects(
    time: Res<Time>,
    mut damage: ResMut<DamageQueue>,
    mut enemies: Query<(Entity, &mut EnemyBrain, &mut EnemyStatus)>,
) {
    let dt = time.delta_secs();
    for (e, mut b, mut status) in &mut enemies {
        status.flash -= dt;
        b.slow -= dt;
        b.stun -= dt;
        b.flinch -= dt;
        b.since_hit += dt;
        if b.poise_broken > 0.0 {
            b.poise_broken -= dt;
            if b.poise_broken <= 0.0 {
                b.poise = b.max_poise;
            }
        } else if b.since_hit >= 5.0 {
            b.poise = (b.poise + 10.0 * dt).min(b.max_poise);
        }
        if b.burn > 0.0 {
            b.burn -= dt;
            damage.0.push(DamageEvent {
                target: e,
                amount: b.burn_dps * dt,
                from: b.burn_by,
                headshot: false,
                legs: false,
                elements: 0,
                chained: true,
                stun: 0.0,
                knockback: Vec3::ZERO,
            });
        }
        if b.poison > 0.0 {
            b.poison -= dt;
            damage.0.push(DamageEvent {
                target: e,
                amount: b.poison_dps * dt,
                from: b.poison_by,
                headshot: false,
                legs: false,
                elements: 0,
                chained: true,
                stun: 0.0,
                knockback: Vec3::ZERO,
            });
        }
        b.marked -= dt;
        status.burning = b.burn > 0.0;
        status.poisoned = b.poison > 0.0;
        status.marked = b.marked > 0.0;
        status.slowed = b.slow > 0.0;
        status.stunned = b.stun > 0.0;
        status.crawler = b.crawler;
        status.attacking = b.swing < 0.6;
    }
}

fn apply_damage(
    mut commands: Commands,
    mut queue: ResMut<DamageQueue>,
    mut roster: ResMut<Roster>,
    mut state: ResMut<MatchState>,
    mut fx: ResMut<FxQueue>,
    mut out: ResMut<FxOutbox>,
    mut enemies: Query<(Entity, &mut Transform, &mut EnemyBrain, &mut EnemyStatus)>,
    time: Res<Time>,
    mut last_drop: Local<Option<f32>>,
) {
    if queue.0.is_empty() {
        return;
    }
    let positions: Vec<(Entity, Vec3)> = enemies
        .iter()
        .map(|(e, t, _, _)| (e, t.translation))
        .collect();
    let mut rng = rand::thread_rng();
    let round = state.round;
    let mut boss_down = None;
    let mut i = 0;
    while i < queue.0.len() {
        let ev = &queue.0[i];
        let (target, from, headshot, legs, elements, chained, stun) = (
            ev.target,
            ev.from,
            ev.headshot,
            ev.legs,
            ev.elements,
            ev.chained,
            ev.stun,
        );
        let mut amount = ev.amount;
        i += 1;
        let Ok((_, mut tf, mut brain, mut status)) = enemies.get_mut(target) else {
            continue;
        };
        if brain.health <= 0.0 {
            continue;
        }
        let pos = tf.translation;
        if state.insta_kill > 0.0 && from.is_some() && !chained && !brain.kind.is_boss() {
            amount = amount.max(brain.health);
        }
        if brain.poise_broken > 0.0 {
            amount *= 1.5;
        }
        if brain.marked > 0.0 {
            amount *= powers::MARK_BONUS;
        }
        if elements & powers::effect::MARK != 0 {
            brain.marked = powers::MARK_TIME;
        }
        if elements & powers::effect::POISON != 0 {
            brain.poison = 5.0;
            brain.poison_dps = 18.0 + 4.0 * round as f32;
            brain.poison_by = from;
            brain.slow = brain.slow.max(1.0);
        }
        brain.health -= amount;
        // Brutes shrug off half of it, bosses nearly all.
        let resist = match brain.kind {
            NetKind::Brute => 0.5,
            NetKind::Boss(_) => 0.15,
            _ => 1.0,
        };
        brain.stun = brain.stun.max(stun * resist);
        let poise_dmg = amount * (if headshot { 1.5 } else { 1.0 });
        brain.since_hit = 0.0;
        if brain.poise_broken <= 0.0 {
            brain.poise -= poise_dmg;
            if brain.poise <= 0.0 {
                let stagger = match brain.kind {
                    NetKind::Boss(_) => 2.5,
                    NetKind::Brute => 1.8,
                    _ => 1.2,
                };
                brain.poise_broken = stagger;
                brain.stun = brain.stun.max(stagger);
                brain.flinch = brain.flinch.max(stagger.min(0.5));
                brain.poise = 0.0;
                emit(
                    &mut fx,
                    &mut out,
                    Fx::Sparks {
                        pos: (pos + Vec3::Y * (1.1 * enemy_scale(brain.kind))).to_array(),
                        count: if brain.kind.is_boss() { 36 } else { 20 },
                    },
                );
            }
        }
        if ev.knockback != Vec3::ZERO {
            brain.knockback += ev.knockback * resist;
        }
        // Heavy single-hit damage (slugs, sniper rifle shots, high-impact hits)
        // or critical hits trigger a micro-stagger / momentary flinch so impactful shots feel weighty.
        let heavy_threshold = 75.0;
        let is_heavy_hit = amount >= heavy_threshold || (headshot && amount >= 40.0);
        if is_heavy_hit && !brain.kind.is_boss() {
            let flinch_duration = if headshot { 0.18 } else { 0.12 } * resist;
            brain.flinch = brain.flinch.max(flinch_duration);
        }
        if is_heavy_hit {
            if let Some(p) = from.and_then(|id| roster.0.get(&id)) {
                let shooter_feet = p.feet();
                let push_dir = (pos - shooter_feet).with_y(0.0).normalize_or_zero();
                brain.knockback += push_dir * (amount * 0.08 * resist);
            }
        }
        if !chained || amount > 5.0 {
            status.flash = if headshot { 0.12 } else { 0.08 };
        }
        for el in elements_in(elements) {
            match el {
                Element::Fire => {
                    brain.burn = 3.0;
                    brain.burn_dps = 15.0 + 3.0 * round as f32;
                    brain.burn_by = from;
                }
                Element::Ice => brain.slow = 2.5,
                Element::Shock if !chained => {
                    let mut near: Vec<&(Entity, Vec3)> = positions
                        .iter()
                        .filter(|(e, p)| *e != target && p.distance(pos) < 6.0)
                        .collect();
                    near.sort_by(|a, b| a.1.distance(pos).total_cmp(&b.1.distance(pos)));
                    for (e, p) in near.into_iter().take(2) {
                        emit(
                            &mut fx,
                            &mut out,
                            Fx::Lightning {
                                a: (pos + Vec3::Y * 1.2).to_array(),
                                b: (*p + Vec3::Y * 1.2).to_array(),
                            },
                        );
                        queue.0.push(DamageEvent {
                            target: *e,
                            amount: amount * 0.6,
                            from,
                            headshot: false,
                            legs: false,
                            elements: 0,
                            chained: true,
                            stun: 0.0,
                            knockback: Vec3::ZERO,
                        });
                    }
                }
                _ => {}
            }
        }
        let killed = brain.health <= 0.0;
        // Enough damage to the legs and it goes down and crawls.
        if legs && !killed && !brain.kind.is_boss() {
            brain.leg_damage += amount;
            // Leg shots trigger a brief stumble/trip gait
            brain.flinch = brain.flinch.max(0.42);
            let needed = if brain.kind == NetKind::Brute {
                0.55
            } else {
                0.35
            };
            if !brain.crawler && brain.leg_damage >= needed * brain.max_health {
                brain.crawler = true;
                brain.speed *= 0.45;
            }
        }
        if let Some(pid) = from {
            if let Some(p) = roster.0.get_mut(&pid) {
                // Scythe cuts heal the Revenant.
                if elements & powers::effect::DRAIN != 0 && p.alive {
                    p.health = (p.health + amount.min(brain.max_health) * powers::DRAIN_FRACTION).min(p.max_health());
                }
                // Chain Reaction: the kill goes off like a bomb.
                if killed && p.chain > 0.0 {
                    let radius = 4.0;
                    let blast = 160.0 + 40.0 * round as f32;
                    for (e, q) in &positions {
                        if *e != target {
                            let d = (*q + Vec3::Y).distance(pos);
                            if d < radius {
                                let dir = ((*q + Vec3::Y * 0.5) - pos).normalize_or(Vec3::Y);
                                let force = (1.0 - (d / radius).clamp(0.0, 1.0)) * 14.0;
                                queue.0.push(DamageEvent {
                                    target: *e,
                                    amount: blast,
                                    from,
                                    headshot: false,
                                    legs: false,
                                    elements: 0,
                                    chained: true,
                                    stun: 0.3,
                                    knockback: dir * force,
                                });
                            }
                        }
                    }
                    emit(
                        &mut fx,
                        &mut out,
                        Fx::Explosion {
                            pos: (pos + Vec3::Y * 0.8).to_array(),
                            radius,
                            color: [1.0, 0.7, 0.2],
                        },
                    );
                }
                if killed {
                    let bonus = if headshot { 40 } else { 0 };
                    let (base, xp) = match brain.kind {
                        NetKind::Brute => (120, 60),
                        NetKind::Boss(_) => (1500, 400),
                        _ => (60, 25),
                    };
                    give_points(p, base + bonus, &state);
                    p.kills += 1;
                    p.ult_charge = (p.ult_charge + 3.0).min(100.0);
                    let xp = xp + if headshot { 15 } else { 0 };
                    give_xp(p, xp);
                } else if !chained {
                    give_points(p, 10, &state);
                }
            }
        }
        if !killed {
            let hit_zone = if headshot {
                crate::ragdoll::HitZone::Head
            } else if legs {
                crate::ragdoll::HitZone::LeftLeg
            } else {
                crate::ragdoll::HitZone::Torso
            };
            let impulse_vec = if brain.knockback.length_squared() > 0.01 {
                brain.knockback
            } else if let Some(pid) = from.and_then(|id| roster.0.get(&id)) {
                (pos - pid.feet()).with_y(0.0).normalize_or_zero() * (amount * 0.08).clamp(2.0, 10.0)
            } else {
                Vec3::ZERO
            };
            if impulse_vec != Vec3::ZERO {
                commands.entity(target).try_insert(crate::ragdoll::HitImpulse {
                    hit_zone,
                    impulse: impulse_vec,
                    stun,
                });
            }
        }
        if killed {
            let hit_h = if headshot { 1.5 } else if legs { 0.4 } else { 1.0 };
            let hit_pos = pos + Vec3::Y * (hit_h * enemy_scale(brain.kind));
            let recoil_dir = if brain.knockback.length_squared() > 0.01 {
                brain.knockback.with_y(0.0).normalize_or_zero()
            } else if let Some(pid) = from.and_then(|id| roster.0.get(&id)) {
                (pos - pid.feet()).with_y(0.0).normalize_or_zero()
            } else {
                Vec3::ZERO
            };
            if recoil_dir != Vec3::ZERO {
                tf.rotation = Quat::from_rotation_arc(Vec3::NEG_Z, -recoil_dir);
            }
            let shot_dir = if recoil_dir != Vec3::ZERO {
                recoil_dir
            } else {
                Vec3::Y
            };
            emit(
                &mut fx,
                &mut out,
                Fx::Blood {
                    pos: hit_pos.to_array(),
                    dir: shot_dir.to_array(),
                    headshot,
                },
            );
            if headshot && !brain.kind.is_boss() {
                let neck_pos = pos + Vec3::Y * (1.35 * enemy_scale(brain.kind));
                emit(
                    &mut fx,
                    &mut out,
                    Fx::Decapitation {
                        pos: neck_pos.to_array(),
                    },
                );
            }
            let hit_zone = if headshot {
                crate::ragdoll::HitZone::Head
            } else if legs {
                crate::ragdoll::HitZone::LeftLeg
            } else if brain.knockback.length() > 8.0 {
                crate::ragdoll::HitZone::FullBodyExplosion
            } else {
                crate::ragdoll::HitZone::Torso
            };
            let force = if brain.knockback.length() > 0.1 {
                brain.knockback.length().clamp(6.0, 24.0)
            } else if headshot {
                14.0
            } else {
                10.0
            };
            let impulse_vec = shot_dir * force;
            crate::zombies::kill_with_impulse(
                &mut commands,
                target,
                crate::ragdoll::DeathImpulse {
                    hit_zone,
                    point_of_impact: hit_pos,
                    impulse: impulse_vec,
                    headshot,
                },
            );
            if let NetKind::Boss(level) = brain.kind {
                boss_down = Some(level);
                continue;
            }
            let now = time.elapsed_secs();
            let ready = last_drop.map_or(true, |t| now - t >= POWERUP_GAP || now < t);
            if ready && rng.gen_bool(POWERUP_CHANCE) {
                *last_drop = Some(now);
                let kind = PowerUp::ALL[rng.gen_range(0..PowerUp::ALL.len())];
                let id = state.next_net_id;
                state.next_net_id += 1;
                commands.spawn((
                    crate::InGameEntity,
                    Replicated {
                        id,
                        kind: NetKind::PowerUp(kind),
                    },
                    PowerUpBrain { kind, life: 25.0 },
                    Transform::from_translation(pos.with_y(0.9)),
                ));
            }
        }
    }
    queue.0.clear();
    if let Some(level) = boss_down {
        boss_defeated(&mut state, &mut roster, level);
        for (e, _, mut b, _) in &mut enemies {
            if b.health > 0.0 {
                b.health = 0.0;
                crate::zombies::kill(&mut commands, e);
            }
        }
    }
}

/// The boss is dead: its zombies fall with it, everyone gets points, XP and
/// a free pick, and the teleporter opens (or the run is won).
fn boss_defeated(state: &mut MatchState, roster: &mut Roster, level: u8) {
    if state.sandbox.on {
        return;
    }
    state.boss = 0;
    state.boss_hp = 0.0;
    state.to_spawn = 0;
    for p in roster.0.values_mut() {
        give_points(p, 500, state);
        give_xp(p, 300);
        p.pending_picks += 1;
        offer_picks(p);
    }
    if level >= 1 || state.stage + 1 >= STAGES {
        state.won = true;
    } else {
        state.teleport = true;
        state.teleport_hold = 0.0;
    }
}

// ---------------------------------------------------------------------------
// Players
// ---------------------------------------------------------------------------

fn player_timers(
    time: Res<Time>,
    mut roster: ResMut<Roster>,
    mut state: ResMut<MatchState>,
    mut hurt: ResMut<LastHurt>,
) {
    let dt = time.delta_secs();
    for p in roster.0.values_mut() {
        // Charges come back one at a time.
        for s in 0..2 {
            let max = p.max_charges(s);
            if p.charges[s] >= max {
                p.charges[s] = max;
                p.cooldowns[s] = 0.0;
                continue;
            }
            if p.cooldowns[s] <= 0.0 {
                p.cooldowns[s] = p.ability_cooldown(s);
            }
            p.cooldowns[s] -= dt;
            if p.cooldowns[s] <= 0.0 {
                p.charges[s] += 1;
                p.cooldowns[s] = if p.charges[s] < max {
                    p.ability_cooldown(s)
                } else {
                    0.0
                };
            }
        }
        p.chain = (p.chain - dt).max(0.0);
        for cd in p.weapon_cd.iter_mut() {
            *cd = (*cd - dt).max(0.0);
        }
        p.grenade_cd = (p.grenade_cd - dt).max(0.0);
        p.buff_time = (p.buff_time - dt).max(0.0);
        if p.alive && state.started && !state.game_over {
            p.ult_charge = (p.ult_charge + dt * 0.8).min(100.0);
            let since = hurt.0.entry(p.id).or_insert(99.0);
            *since += dt;
            if *since > REGEN_DELAY {
                p.health = (p.health + REGEN_RATE * dt).min(p.max_health());
            }
        }
    }
    state.insta_kill = (state.insta_kill - dt).max(0.0);
    state.double_points = (state.double_points - dt).max(0.0);
}

// ---------------------------------------------------------------------------
// Rounds and spawning
// ---------------------------------------------------------------------------

fn rounds(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<crate::avatars::ReplicatedAssets>,
    rigs: Res<crate::rig::RigAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    map: Res<CurrentMap>,
    mut state: ResMut<MatchState>,
    mut roster: ResMut<Roster>,
    mut summons: ResMut<Summons>,
    mut pacer: ResMut<SubWavePacer>,
    enemies: Query<&EnemyBrain>,
) {
    if state.game_over || state.won || !state.started {
        return;
    }
    let players = roster.0.len().max(1) as u32;
    for (pos, kind) in std::mem::take(&mut summons.0) {
        spawn_zombie(
            &mut commands,
            &assets,
            &rigs,
            &mut materials,
            &mut state,
            kind,
            pos,
            false,
            1.0,
            false,
        );
    }
    // The boss health bar.
    match enemies.iter().find(|b| b.kind.is_boss() && b.health > 0.0) {
        Some(b) => {
            state.boss = if b.kind == NetKind::Boss(0) { 1 } else { 2 };
            state.boss_hp = (b.health / b.max_health).clamp(0.0, 1.0);
        }
        None => {
            state.boss = 0;
            state.boss_hp = 0.0;
        }
    }
    if state.sandbox.on && !state.sandbox.waves {
        return;
    }
    // The boss is dead: waiting for the team at the teleporter.
    if state.teleport {
        return;
    }
    let dt = time.delta_secs();
    let alive_enemies = enemies.iter().filter(|b| b.health > 0.0).count() as u32;
    let run = !state.sandbox.on;

    if state.to_spawn == 0 && alive_enemies == 0 {
        if state.intermission <= 0.0 {
            // Round cleared; a longer breather before a boss.
            state.intermission = if run && state.stage_round == ROUNDS_PER_STAGE {
                7.0
            } else {
                4.0
            };
        }
        state.intermission -= dt;
        if state.intermission <= 0.0 {
            state.round += 1;
            state.stage_round += 1;
            let r = state.round;
            state.to_spawn = 5 + 3 * r + (players - 1) * (2 + r);
            state.spawn_timer = 0.5;
            pacer.spawned_in_burst = 0;
            pacer.breather_timer = 0.0;
            pacer.burst_target = if players == 1 {
                (3 + (r / 3).min(2)).clamp(3, 5)
            } else {
                (4 + players).min(8)
            };
            if run && state.stage_round > ROUNDS_PER_STAGE {
                // Boss round: the boss, and zombies trickling in with it.
                state.to_spawn = 8 + 3 * state.stage as u32 + (players - 1) * 3;
                state.spawn_timer = 4.0;
                let final_boss = state.stage + 1 >= STAGES;
                let pos = farthest_spawn(&map, &roster);
                let toughness = 1.0 + 0.75 * (players - 1) as f32;
                spawn_zombie(
                    &mut commands,
                    &assets,
                    &rigs,
                    &mut materials,
                    &mut state,
                    NetKind::Boss(final_boss as u8),
                    pos,
                    false,
                    toughness,
                    false,
                );
            }
            // Downed players get back up at the start of each round.
            for p in roster.0.values_mut().filter(|p| !p.alive) {
                p.alive = true;
                p.health = p.max_health() * REVIVE_HEALTH_FRACTION;
                p.spawn_seq += 1;
            }
        }
        return;
    }

    if state.to_spawn == 0 {
        return;
    }
    if alive_enemies >= 24 + 4 * players {
        return;
    }

    // Micro-breather countdown between sub-waves
    if pacer.breather_timer > 0.0 {
        pacer.breather_timer -= dt;
        // If the player wipes out all active enemies quickly, end the breather early to resume action
        if alive_enemies == 0 && pacer.breather_timer > 0.5 {
            pacer.breather_timer = 0.5;
        }
        return;
    }

    state.spawn_timer -= dt;
    if state.spawn_timer > 0.0 {
        return;
    }
    let r = state.round;
    let boss_round = state.boss != 0;
    let mut rng = rand::thread_rng();

    if boss_round {
        state.spawn_timer = (2.6 - 0.2 * state.stage as f32).max(1.2);
    } else {
        // Fast burst cadence within sub-wave (0.40s - 0.75s) to group zombies into rushing packs:
        state.spawn_timer = (0.75 - 0.03 * r as f32).clamp(0.40, 0.75);
        pacer.spawned_in_burst += 1;
        if pacer.spawned_in_burst >= pacer.burst_target {
            // Trigger tactical micro-breather (2.4-3.2s solo, 1.2-1.6s co-op) to allow weapon reloads and retreat
            pacer.breather_timer = if players == 1 {
                2.4 + rng.gen_range(0.0..0.8)
            } else {
                1.2 + rng.gen_range(0.0..0.4)
            };
            pacer.spawned_in_burst = 0;
            pacer.burst_target = if players == 1 {
                (3 + (r / 3).min(2)).clamp(3, 5)
            } else {
                (4 + players).min(8)
            };
        }
    }
    state.to_spawn -= 1;

    let living: Vec<Vec3> = roster
        .0
        .values()
        .filter(|p| p.alive)
        .map(|p| p.feet())
        .collect();
    let open: Vec<Vec3> = map.0.enemy_spawns.iter().map(|(p, _)| *p).collect();
    let far: Vec<Vec3> = open
        .iter()
        .copied()
        .filter(|s| living.iter().all(|p| p.distance(*s) > 14.0))
        .collect();
    let pool = if far.is_empty() { &open } else { &far };
    let base = pool[rng.gen_range(0..pool.len())];
    let pos = base + Vec3::new(rng.gen_range(-1.5..1.5), 0.0, rng.gen_range(-1.5..1.5));

    // Later maps send more Brutes and Shooters.
    let stage = state.stage as f64;
    let kind = if r >= 6 && rng.gen_bool(0.1 + 0.03 * stage) {
        NetKind::Brute
    } else if r >= 3 && rng.gen_bool(0.2 + 0.03 * stage) {
        NetKind::Shooter
    } else {
        NetKind::Grunt
    };

    // Fast Sprinter / Runner variant: in mid-to-late rounds (round >= 3),
    // a portion of regular grunts spawn as aggressive sprinters to break up horde clusters.
    let is_sprinter = kind == NetKind::Grunt
        && r >= 3
        && rng.gen_bool((0.12 + 0.04 * (r - 2) as f64).min(0.40));

    spawn_zombie(
        &mut commands,
        &assets,
        &rigs,
        &mut materials,
        &mut state,
        kind,
        pos,
        false,
        1.0,
        is_sprinter,
    );
}

/// The enemy spawn furthest from every living player (where the boss comes in).
fn farthest_spawn(map: &CurrentMap, roster: &Roster) -> Vec3 {
    let living: Vec<Vec3> = roster
        .0
        .values()
        .filter(|p| p.alive)
        .map(|p| p.feet())
        .collect();
    map.0
        .enemy_spawns
        .iter()
        .map(|(p, _)| *p)
        .max_by(|a, b| {
            let near = |s: &Vec3| {
                living
                    .iter()
                    .map(|p| p.distance(*s))
                    .fold(f32::MAX, f32::min)
            };
            near(a).total_cmp(&near(b))
        })
        .unwrap_or(Vec3::ZERO)
}

/// Spawns a zombie of `kind` at `pos`, as tough as the current round makes it.
#[allow(clippy::too_many_arguments)]
fn spawn_zombie(
    commands: &mut Commands,
    assets: &crate::avatars::ReplicatedAssets,
    rigs: &crate::rig::RigAssets,
    materials: &mut Assets<StandardMaterial>,
    state: &mut MatchState,
    kind: NetKind,
    pos: Vec3,
    crawler: bool,
    toughness: f32,
    is_sprinter: bool,
) {
    let mut rng = rand::thread_rng();
    let r = state.round.max(1);
    let base_hp = if r <= 10 {
        75.0 + 30.0 * r as f32
    } else {
        375.0 * 1.1f32.powi(r as i32 - 10)
    } * (1.0 + 0.1 * state.stage as f32);
    let (health, speed) = match kind {
        NetKind::Shooter => (base_hp * 0.7, 2.6),
        NetKind::Brute => (base_hp * 3.0, (2.4 + 0.1 * r as f32).min(4.5)),
        NetKind::Boss(0) => (base_hp * 35.0 * toughness, 3.0),
        NetKind::Boss(_) => (base_hp * 70.0 * toughness, 3.4),
        _ => {
            let normal_speed = (2.8 + 0.25 * r as f32).min(7.0) * rng.gen_range(0.9..1.1);
            if is_sprinter {
                // Sprinter variant: 0.7x HP, 1.4x movement speed, rabid claw attacks
                (base_hp * 0.70, (normal_speed * 1.40).clamp(4.2, 9.5))
            } else {
                (base_hp, normal_speed)
            }
        }
    };
    let id = state.next_net_id;
    state.next_net_id += 1;
    let e = spawn_replicated(commands, assets, rigs, materials, id, kind, pos);
    let flank_bias = rng.gen_range(-0.55..0.55);
    let max_poise = match kind {
        NetKind::Shooter => 25.0,
        NetKind::Brute => 120.0,
        NetKind::Boss(_) => 300.0,
        _ => if is_sprinter { 20.0 } else { 35.0 },
    };
    commands.entity(e).insert(EnemyBrain {
        kind,
        health,
        knockback: Vec3::ZERO,
        speed,
        attack_timer: 1.0,
        burn: 0.0,
        burn_dps: 0.0,
        burn_by: None,
        slow: 0.0,
        max_health: health,
        leg_damage: 0.0,
        crawler,
        swing: 9.0,
        stun: 0.0,
        flinch: 0.0,
        slam: false,
        volley: 3.0,
        summons: 0,
        poison: 0.0,
        poison_dps: 0.0,
        poison_by: None,
        marked: 0.0,
        is_sprinter,
        flank_bias,
        attack_windup: None,
        poise: max_poise,
        max_poise,
        poise_broken: 0.0,
        since_hit: 5.0,
    });
}

/// Runs sandbox tools, and keeps god mode and free abilities going.
#[allow(clippy::too_many_arguments)]
fn sandbox(
    mut commands: Commands,
    assets: Res<crate::avatars::ReplicatedAssets>,
    rigs: Res<crate::rig::RigAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut state: ResMut<MatchState>,
    mut roster: ResMut<Roster>,
    mut dev: ResMut<DevQueue>,
    enemies: Query<Entity, With<EnemyBrain>>,
) {
    if !state.sandbox.on {
        dev.0.clear();
        return;
    }
    use crate::DevCmd;
    for (_, cmd) in std::mem::take(&mut dev.0) {
        match cmd {
            DevCmd::Spawn {
                kind,
                count,
                at,
                dir,
            } => {
                let at = Vec3::from_array(at);
                let fwd = Vec3::from_array(dir).with_y(0.0).normalize_or(Vec3::NEG_Z);
                let side = Vec3::new(-fwd.z, 0.0, fwd.x);
                let is_sprinter = kind == 6;
                let net = match kind {
                    1 => NetKind::Shooter,
                    2 => NetKind::Brute,
                    4 => NetKind::Boss(0),
                    5 => NetKind::Boss(1),
                    _ => NetKind::Grunt,
                };
                let count = if net.is_boss() { 1 } else { count };
                for i in 0..count.min(20) {
                    let spread = (i as f32 - (count as f32 - 1.0) / 2.0) * 1.4;
                    let pos = (at + fwd * 9.0 + side * spread).with_y(0.0);
                    spawn_zombie(
                        &mut commands,
                        &assets,
                        &rigs,
                        &mut materials,
                        &mut state,
                        net,
                        pos,
                        kind == 3,
                        1.0,
                        is_sprinter,
                    );
                }
            }
            DevCmd::KillAll => {
                for e in &enemies {
                    crate::zombies::kill(&mut commands, e);
                }
            }
            _ => {}
        }
    }
    let sb = state.sandbox;
    for p in roster.0.values_mut() {
        if sb.god {
            p.health = p.max_health();
        }
        if sb.free_abilities {
            p.cooldowns = [0.0; 2];
            p.charges = [p.max_charges(0), p.max_charges(1)];
            p.weapon_cd = [0.0; 2];
            p.ult_charge = 100.0;
        }
    }
}

/// Base collision radius for standard human-sized enemies (0.45m).
pub const STANDARD_ENEMY_RADIUS: f32 = 0.45;
/// Maximum soft-body repulsive separation velocity to prevent explosive jitter (12.0 m/s).
pub const MAX_SEPARATION_FORCE: f32 = 12.0;
/// Separation force stiffness multiplier.
pub const SEPARATION_STIFFNESS: f32 = 8.0;

/// Standard radius for crowd separation: ~0.45m * enemy_scale.
pub fn enemy_radius(kind: NetKind) -> f32 {
    STANDARD_ENEMY_RADIUS * enemy_scale(kind)
}

/// Mass / weight hierarchy for crowd separation:
/// Brutes (3.5) and Bosses (6.0 - 8.0) have significantly higher mass than standard zombies (1.0),
/// allowing them to push lighter enemies aside like a bowling ball while remaining unaffected by lighter enemies.
pub fn enemy_mass(kind: NetKind) -> f32 {
    match kind {
        NetKind::Boss(0) => 6.0,
        NetKind::Boss(_) => 8.0,
        NetKind::Brute => 3.5,
        _ => 1.0,
    }
}

/// Spatial hash grid for accelerated broadphase crowd neighbor queries.
pub struct CrowdSpatialGrid {
    pub cell_size: f32,
    pub cells: HashMap<(i32, i32), Vec<usize>>,
}

impl CrowdSpatialGrid {
    pub fn new(cell_size: f32, capacity: usize) -> Self {
        Self {
            cell_size,
            cells: HashMap::with_capacity(capacity),
        }
    }

    pub fn insert(&mut self, index: usize, pos: Vec3) {
        let cx = (pos.x / self.cell_size).floor() as i32;
        let cz = (pos.z / self.cell_size).floor() as i32;
        self.cells.entry((cx, cz)).or_default().push(index);
    }

    pub fn query_neighbors<'a>(&'a self, pos: Vec3, radius: f32) -> impl Iterator<Item = usize> + 'a {
        let min_cx = ((pos.x - radius) / self.cell_size).floor() as i32;
        let max_cx = ((pos.x + radius) / self.cell_size).floor() as i32;
        let min_cz = ((pos.z - radius) / self.cell_size).floor() as i32;
        let max_cz = ((pos.z + radius) / self.cell_size).floor() as i32;

        (min_cx..=max_cx).flat_map(move |cx| {
            (min_cz..=max_cz).flat_map(move |cz| {
                self.cells.get(&(cx, cz)).into_iter().flat_map(|v| v.iter().copied())
            })
        })
    }
}

/// Computes crowd separation force using spatial grid acceleration.
pub fn compute_crowd_separation_grid(
    entity: Entity,
    pos: Vec3,
    kind: NetKind,
    flank_bias: f32,
    crowd: &[(Entity, Vec3, NetKind)],
    grid: &CrowdSpatialGrid,
) -> Vec3 {
    let mut total_force = Vec3::ZERO;
    let r1 = enemy_radius(kind);
    let m1 = enemy_mass(kind);
    let max_radius = enemy_radius(NetKind::Boss(1));
    let max_search_dist = r1 + max_radius;

    for other_idx in grid.query_neighbors(pos, max_search_dist) {
        if other_idx >= crowd.len() {
            continue;
        }
        let (other_entity, other_pos, other_kind) = crowd[other_idx];
        if other_entity == entity {
            continue;
        }

        let delta = (pos - other_pos).with_y(0.0);
        let dist_sq = delta.length_squared();
        let r2 = enemy_radius(other_kind);
        let mutual_radius = r1 + r2;

        if dist_sq >= mutual_radius * mutual_radius {
            continue;
        }

        let dist = dist_sq.sqrt();
        let overlap = mutual_radius - dist;
        let dir = if dist > 1e-4 {
            delta / dist
        } else {
            let angle = (entity.index() as f32 * 2.399).sin();
            Vec3::new(angle.cos(), 0.0, angle.sin()).normalize_or_zero()
        };

        // Separation vector: push = normalize(pos - other_pos) * (r1 + r2 - dist)
        let push = dir * overlap;

        // Mass / weight hierarchy:
        // Brutes and Bosses push lighter zombies without being pushed back
        let m2 = enemy_mass(other_kind);
        let mass_weight = if m2 > m1 {
            m2 / m1
        } else if (m2 - m1).abs() < 1e-4 {
            1.0
        } else {
            0.0
        };

        if mass_weight <= 0.0 {
            continue;
        }

        let lateral = if flank_bias.abs() > 1e-4 {
            let perp = Vec3::new(-dir.z, 0.0, dir.x) * flank_bias.signum();
            perp * (overlap * 2.5)
        } else {
            Vec3::ZERO
        };

        let pair_force = (push * SEPARATION_STIFFNESS + lateral) * mass_weight;
        total_force += pair_force;
    }

    if total_force.length_squared() > MAX_SEPARATION_FORCE * MAX_SEPARATION_FORCE {
        total_force = total_force.normalize() * MAX_SEPARATION_FORCE;
    }

    total_force
}

/// Computes mutual repulsive crowd separation force for an enemy among nearby enemies.
/// Filters enemies by distance, applies mass/weight hierarchy, lateral flank flow,
/// and clamps maximum force to prevent jitter.
pub fn compute_crowd_separation(
    entity: Entity,
    pos: Vec3,
    kind: NetKind,
    flank_bias: f32,
    crowd: &[(Entity, Vec3, NetKind)],
) -> Vec3 {
    let mut total_force = Vec3::ZERO;
    let r1 = enemy_radius(kind);
    let m1 = enemy_mass(kind);
    let max_radius = enemy_radius(NetKind::Boss(1));
    let max_search_dist = r1 + max_radius;
    let max_search_dist_sq = max_search_dist * max_search_dist;

    for &(other_entity, other_pos, other_kind) in crowd {
        if other_entity == entity {
            continue;
        }

        let delta = (pos - other_pos).with_y(0.0);
        let dist_sq = delta.length_squared();

        if dist_sq >= max_search_dist_sq {
            continue;
        }

        let r2 = enemy_radius(other_kind);
        let mutual_radius = r1 + r2;

        if dist_sq >= mutual_radius * mutual_radius {
            continue;
        }

        let dist = dist_sq.sqrt();
        let overlap = mutual_radius - dist;
        let dir = if dist > 1e-4 {
            delta / dist
        } else {
            let angle = (entity.index() as f32 * 2.399).sin();
            Vec3::new(angle.cos(), 0.0, angle.sin()).normalize_or_zero()
        };

        let push = dir * overlap;

        let m2 = enemy_mass(other_kind);
        let mass_weight = if m2 > m1 {
            m2 / m1
        } else if (m2 - m1).abs() < 1e-4 {
            1.0
        } else {
            0.0
        };

        if mass_weight <= 0.0 {
            continue;
        }

        let lateral = if flank_bias.abs() > 1e-4 {
            let perp = Vec3::new(-dir.z, 0.0, dir.x) * flank_bias.signum();
            perp * (overlap * 2.5)
        } else {
            Vec3::ZERO
        };

        let pair_force = (push * SEPARATION_STIFFNESS + lateral) * mass_weight;
        total_force += pair_force;
    }

    if total_force.length_squared() > MAX_SEPARATION_FORCE * MAX_SEPARATION_FORCE {
        total_force = total_force.normalize() * MAX_SEPARATION_FORCE;
    }

    total_force
}

/// Crowd Separation Physics & Movement Integration for an enemy:
/// Accumulates soft-body mutual repulsive separation forces into the enemy's velocity vector,
/// applying mass hierarchy and clamping to prevent clumping and explosive jitter.
pub fn move_enemies(
    entity: Entity,
    pos: Vec3,
    kind: NetKind,
    flank_bias: f32,
    velocity: &mut Vec3,
    crowd: &[(Entity, Vec3, NetKind)],
) {
    let separation = compute_crowd_separation(entity, pos, kind, flank_bias, crowd);
    *velocity += separation;
}

/// Fast version of move_enemies using a pre-constructed spatial grid.
pub fn move_enemies_with_grid(
    entity: Entity,
    pos: Vec3,
    kind: NetKind,
    flank_bias: f32,
    velocity: &mut Vec3,
    crowd: &[(Entity, Vec3, NetKind)],
    grid: &CrowdSpatialGrid,
) {
    let separation = compute_crowd_separation_grid(entity, pos, kind, flank_bias, crowd, grid);
    *velocity += separation;
}

fn enemy_ai(
    mut commands: Commands,
    time: Res<Time>,
    map: Res<CurrentMap>,
    mut nav: ResMut<NavGrid>,
    mut state: ResMut<MatchState>,
    mut roster: ResMut<Roster>,
    mut hurt: ResMut<LastHurt>,
    forces: Res<powers::Forces>,
    mut fx: ResMut<FxQueue>,
    mut out: ResMut<FxOutbox>,
    mut summons: ResMut<Summons>,
    mut enemies: Query<(Entity, &mut Transform, &mut EnemyBrain)>,
    colliders: Query<(&Transform, &Collider), Without<EnemyBrain>>,
) {
    let dt = time.delta_secs();
    let boxes: Boxes = collect_boxes(colliders.iter());
    // Players who have vanished (Thousand Cuts) can't be chased.
    let targets: Vec<(u8, Vec3)> = roster
        .0
        .values()
        .filter(|p| p.alive && p.vanish <= 0.0)
        .map(|p| (p.id, p.feet()))
        .collect();
    let target_points: Vec<Vec3> = targets.iter().map(|t| t.1).collect();
    nav.update(dt, &boxes, &target_points);
    let crowd: Vec<(Entity, Vec3, NetKind)> = enemies
        .iter()
        .map(|(e, t, b)| (e, t.translation, b.kind))
        .collect();
    let grid = if crowd.len() > 16 {
        let mut g = CrowdSpatialGrid::new(2.5, crowd.len());
        for (i, &(_, p, _)) in crowd.iter().enumerate() {
            g.insert(i, p);
        }
        Some(g)
    } else {
        None
    };
    let half = map.0.half;

    for (entity, mut tf, mut enemy) in &mut enemies {
        let decay = (-8.0 * dt).exp();
        enemy.knockback *= decay;

        let pos = tf.translation;
        let scale = enemy_scale(enemy.kind);
        let target = targets.iter().min_by(|a, b| {
            a.1.distance_squared(pos)
                .total_cmp(&b.1.distance_squared(pos))
        });
        let mut velocity = Vec3::ZERO;
        let Some(&(target_id, target_feet)) = target else {
            if enemy.knockback.length_squared() > 1e-4 {
                let mut new_pos = pos + enemy.knockback * dt;
                new_pos.y = 0.0;
                resolve_collisions(&mut new_pos, 0.38 * scale, 0.0, &boxes);
                new_pos.x = new_pos.x.clamp(-half + 0.5, half - 0.5);
                new_pos.z = new_pos.z.clamp(-half + 0.5, half - 0.5);
                tf.translation = new_pos;
            }
            continue;
        };
        let to_target = (target_feet - pos).with_y(0.0);
        let dist = to_target.length();
        let direct = to_target.normalize_or_zero();
        let chest = Vec3::Y * 1.3;
        let sees = dist < 30.0 && line_of_sight(pos + chest, target_feet + chest, &boxes);
        let path = if sees && dist < 18.0 {
            direct
        } else {
            nav.direction(pos).unwrap_or(direct)
        };

        // Lateral approach spread: compute perpendicular tangent in XZ plane
        // to fan zombies out around the target and prevent single-file conga lines
        let tangent = Vec3::new(-direct.z, 0.0, direct.x);
        let flank_weight = if sees {
            ((22.0 - dist) / 18.0).clamp(0.10, 0.55)
        } else {
            0.08
        };
        let steer = (path + tangent * (enemy.flank_bias * flank_weight)).normalize_or_zero();

        // The final boss gets faster when it's nearly dead.
        let enraged = enemy.kind == NetKind::Boss(1) && enemy.health < enemy.max_health * 0.3;
        // Apply brief speed dampening / momentary stagger when flinching from heavy/critical shots
        let flinch_factor = if enemy.flinch > 0.0 { 0.25 } else { 1.0 };
        // Slow down slightly while committed to winding up an attack
        let windup_factor = if enemy.attack_windup.is_some() { 0.35 } else { 1.0 };
        let speed = enemy.speed
            * if enemy.slow > 0.0 { 0.5 } else { 1.0 }
            * if enraged { 1.4 } else { 1.0 }
            * flinch_factor
            * windup_factor;
        let (windup, slam_radius, slam_ahead, slam_damage) = slam_spec(enemy.kind);
        let desired = match enemy.kind {
            NetKind::Shooter if sees => 12.0,
            NetKind::Brute => 1.6,
            NetKind::Boss(_) => slam_ahead + 0.6,
            _ => 1.1,
        };
        if dist > desired {
            velocity = steer * speed;
        } else if enemy.kind == NetKind::Shooter && dist < desired - 4.0 {
            velocity = -direct * speed * 0.6;
        }
        if enemy.stun > 0.0 || enemy.slam {
            velocity = Vec3::ZERO;
        }
        // Soul Chains pull, Shield Charge shoves aside.
        for f in &forces.0 {
            if f.only.is_some_and(|only| only != entity) {
                continue;
            }
            let to = (f.pos - pos).with_y(0.0);
            let d = to.length();
            if d < f.radius && d > 1e-3 {
                let dir = to / d;
                if f.strength > 0.0 {
                    velocity = velocity * 0.3 + dir * f.strength * (0.4 + 0.6 * d / f.radius);
                } else {
                    let inward = velocity.dot(dir).max(0.0);
                    velocity += -dir * (inward - f.strength * (1.0 - d / f.radius) * 2.0);
                }
            }
        }
        // Soft-body crowd separation physics with mass hierarchy and spatial acceleration
        if let Some(ref g) = grid {
            move_enemies_with_grid(
                entity,
                pos,
                enemy.kind,
                enemy.flank_bias,
                &mut velocity,
                &crowd,
                g,
            );
        } else {
            move_enemies(
                entity,
                pos,
                enemy.kind,
                enemy.flank_bias,
                &mut velocity,
                &crowd,
            );
        }
        let mut new_pos = pos + (velocity + enemy.knockback) * dt;
        new_pos.y = 0.0;
        resolve_collisions(&mut new_pos, 0.38 * scale, 0.0, &boxes);
        new_pos.x = new_pos.x.clamp(-half + 0.5, half - 0.5);
        new_pos.z = new_pos.z.clamp(-half + 0.5, half - 0.5);
        tf.translation = new_pos;
        let face = if sees || velocity.length_squared() < 0.01 {
            direct
        } else {
            velocity.with_y(0.0).normalize_or_zero()
        };
        // A winding-up Brute is committed to where it's facing.
        if face != Vec3::ZERO && !enemy.slam {
            let look = Quat::from_rotation_arc(Vec3::NEG_Z, face);
            tf.rotation = tf.rotation.slerp(look, (dt * 10.0).min(1.0));
        }

        enemy.attack_timer -= dt;
        enemy.swing += dt;
        if enemy.stun > 0.0 || enemy.flinch > 0.0 {
            enemy.slam = false;
            enemy.attack_windup = None;
            continue;
        }

        // Resolve active attack windup / telegraph for melee bites/claws
        if let Some((ref mut timer, tid)) = enemy.attack_windup {
            *timer -= dt;
            if *timer <= 0.0 {
                enemy.attack_windup = None;
                if let Some(p) = roster.0.get_mut(&tid) {
                    let reach = match enemy.kind {
                        NetKind::Brute => 2.1,
                        NetKind::Boss(_) => slam_ahead + slam_radius * 0.6,
                        _ => 1.6,
                    };
                    let p_dist = (p.feet() - new_pos).with_y(0.0).length();
                    // Reaction / dodge window: if player slid, dashed, or sprinted out of range (+0.35m margin), attack whiffs!
                    if p.alive && p_dist <= reach + 0.35 {
                        let hits = stage_damage(&state);
                        let dmg = if enemy.is_sprinter { 12.0 } else { 15.0 };
                        hurt_player(p, dmg * hits, &mut hurt);
                    }
                }
            }
        }
        let hits = stage_damage(&state);
        if enemy.slam && enemy.swing >= windup {
            enemy.slam = false;
            let at = new_pos + tf.rotation * Vec3::NEG_Z * slam_ahead;
            for p in roster.0.values_mut().filter(|p| p.alive) {
                if p.feet().with_y(0.0).distance(at.with_y(0.0)) < slam_radius + 0.3 {
                    hurt_player(p, slam_damage * hits, &mut hurt);
                }
            }
            if enemy.kind.is_boss() {
                emit(
                    &mut fx,
                    &mut out,
                    Fx::Slam {
                        pos: at.to_array(),
                        radius: slam_radius,
                    },
                );
            }
        }
        if let NetKind::Boss(level) = enemy.kind {
            // Calls in zombies at two-thirds and one-third health.
            let due = if enemy.health < enemy.max_health / 3.0 {
                2
            } else if enemy.health < enemy.max_health * 2.0 / 3.0 {
                1
            } else {
                0
            };
            if enemy.summons < due {
                enemy.summons += 1;
                let n = 5 + 2 * level as usize;
                for i in 0..n {
                    let a = i as f32 / n as f32 * std::f32::consts::TAU;
                    let at = new_pos + Vec3::new(a.cos(), 0.0, a.sin()) * 3.5;
                    let kind = if level == 1 && i % 3 == 0 {
                        NetKind::Brute
                    } else {
                        NetKind::Grunt
                    };
                    summons.0.push((at, kind));
                }
            }
            // A fan of fireballs at whoever it's after.
            enemy.volley -= dt;
            if sees && dist < 35.0 && enemy.volley <= 0.0 && !enemy.slam {
                enemy.volley = match (level, enraged) {
                    (0, _) => 5.0,
                    (_, false) => 3.5,
                    (_, true) => 2.2,
                };
                let count = 5 + 2 * level as i32;
                let start = new_pos + Vec3::Y * 1.6 * scale + direct * 0.8 * scale;
                let eye = target_feet + Vec3::Y * (EYE_HEIGHT - 0.3);
                let aim = (eye - start).normalize_or_zero();
                for i in 0..count {
                    let off = (i as f32 - (count - 1) as f32 / 2.0) * 0.16;
                    let dir = Quat::from_rotation_y(off) * aim;
                    let id = state.next_net_id;
                    state.next_net_id += 1;
                    commands.spawn((
                        crate::InGameEntity,
                        Replicated {
                            id,
                            kind: NetKind::Fireball,
                        },
                        FireballBrain {
                            velocity: dir * FIREBALL_SPEED * 0.85,
                            life: 4.0,
                            damage: (10.0 + state.round as f32 * 0.4) * hits,
                        },
                        Transform::from_translation(start),
                    ));
                }
            }
        }
        match enemy.kind {
            NetKind::Shooter => {
                if sees && dist < 28.0 && enemy.attack_timer <= 0.0 {
                    enemy.attack_timer = 2.4;
                    enemy.swing = 0.0;
                    let start = new_pos + Vec3::Y * 1.5 * scale + direct * 0.5;
                    let eye = target_feet + Vec3::Y * (EYE_HEIGHT - 0.3);
                    let aim = (eye - start).normalize_or_zero();
                    let id = state.next_net_id;
                    state.next_net_id += 1;
                    commands.spawn((
                        crate::InGameEntity,
                        Replicated {
                            id,
                            kind: NetKind::Fireball,
                        },
                        FireballBrain {
                            velocity: aim * FIREBALL_SPEED,
                            life: 4.0,
                            damage: (12.0 + state.round as f32 * 0.5) * hits,
                        },
                        Transform::from_translation(start),
                    ));
                }
            }
            kind => {
                let reach = match kind {
                    NetKind::Brute => 2.1,
                    NetKind::Boss(_) => slam_ahead + slam_radius * 0.6,
                    _ => 1.6,
                };
                if dist < reach && enemy.attack_timer <= 0.0 && enemy.attack_windup.is_none() {
                    enemy.swing = 0.0;
                    if kind.slams() {
                        // Winds up first; the slam lands above.
                        enemy.attack_timer = if kind.is_boss() { 1.1 } else { 1.4 } + windup;
                        enemy.slam = true;
                    } else {
                        // Telegraph windup window: 0.18s for fast sprinters, 0.26s for grunts, 0.30s for crawlers
                        let windup_time = if enemy.crawler {
                            0.30
                        } else if enemy.is_sprinter {
                            0.18
                        } else {
                            0.26
                        };
                        enemy.attack_windup = Some((windup_time, target_id));
                        enemy.attack_timer = if enemy.is_sprinter { 0.75 } else { 0.95 };
                    }
                }
            }
        }
    }
}

fn projectiles(
    mut commands: Commands,
    time: Res<Time>,
    mut roster: ResMut<Roster>,
    mut hurt: ResMut<LastHurt>,
    mut fx: ResMut<FxQueue>,
    mut out: ResMut<FxOutbox>,
    mut shots: Query<(Entity, &mut Transform, &mut FireballBrain)>,
    colliders: Query<(&Transform, &Collider), Without<FireballBrain>>,
) {
    let dt = time.delta_secs();
    for (e, mut tf, mut shot) in &mut shots {
        shot.velocity.y -= PROJECTILE_GRAVITY * dt;
        tf.translation += shot.velocity * dt;
        shot.life -= dt;
        let mut pos = tf.translation;
        let mut hit = false;
        let mut hit_player_id = None;
        for p in roster.0.values_mut().filter(|p| p.alive) {
            if pos.distance(p.feet() + Vec3::Y * 1.0) < 0.9 {
                hurt_player(p, shot.damage, &mut hurt);
                hit_player_id = Some(p.id);
                hit = true;
                break;
            }
        }
        let hit_wall = pos.y < 0.0
            || colliders.iter().any(|(ct, c)| {
                let d = (pos - ct.translation).abs();
                d.x < c.half.x && d.y < c.half.y && d.z < c.half.z
            });
        if hit || hit_wall {
            if pos.y < 0.0 {
                pos.y = 0.0;
            }
            emit(
                &mut fx,
                &mut out,
                Fx::Explosion {
                    pos: pos.to_array(),
                    radius: 2.0,
                    color: [1.0, 0.4, 0.1],
                },
            );
            for p in roster.0.values_mut().filter(|p| p.alive) {
                if pos.distance(p.feet()) < 2.2 {
                    if hit_player_id != Some(p.id) {
                        hurt_player(p, shot.damage * 0.6, &mut hurt);
                    }
                    let blast_dir = ((p.feet() + Vec3::Y * 0.5) - pos).normalize_or(Vec3::Y);
                    p.pos = (p.feet() + blast_dir * 0.4).to_array();
                }
            }
            commands.entity(e).despawn();
        } else if shot.life <= 0.0 {
            commands.entity(e).despawn();
        }
    }
}


fn zones(
    mut commands: Commands,
    time: Res<Time>,
    mut roster: ResMut<Roster>,
    mut zones: ResMut<Zones>,
    mut damage: ResMut<DamageQueue>,
    enemies: Query<(Entity, &Transform), With<EnemyBrain>>,
) {
    let dt = time.delta_secs();
    for z in zones.0.iter_mut() {
        z.life -= dt;
        if z.grow > 0.0 {
            z.radius = (z.radius + z.grow * dt).min(z.max_radius);
        }
        if let Some(p) = z.follow.and_then(|id| roster.0.get(&id)) {
            if p.alive {
                z.pos = p.feet();
            } else {
                z.life = z.life.min(0.0);
            }
        }
        z.timer -= dt;
        if z.timer > 0.0 || z.life <= 0.0 {
            continue;
        }
        z.timer += z.interval;
        if z.heal > 0.0 {
            for p in roster.0.values_mut() {
                if p.alive && p.feet().distance(z.pos) < z.radius {
                    p.health = (p.health + z.heal).min(p.max_health());
                }
            }
        }
        if z.damage <= 0.0 {
            continue;
        }
        for (e, t) in &enemies {
            let d = t.translation.with_y(0.0).distance(z.pos.with_y(0.0));
            if d < z.radius && (t.translation.y - z.pos.y).abs() < 3.0 {
                damage.0.push(DamageEvent {
                    target: e,
                    amount: z.damage,
                    from: Some(z.owner),
                    headshot: false,
                    legs: false,
                    elements: z.elements,
                    chained: true,
                    stun: 0.0,
                    knockback: Vec3::ZERO,
                });
            }
        }
    }
    zones.0.retain(|z| {
        if z.life > 0.0 {
            return true;
        }
        if let Some(m) = z.model {
            if let Ok(mut e) = commands.get_entity(m) {
                e.try_despawn();
            }
        }
        false
    });
}



// ---------------------------------------------------------------------------
// Power-ups, mystery box, extraction, game over
// ---------------------------------------------------------------------------

fn powerups(
    mut commands: Commands,
    time: Res<Time>,
    mut state: ResMut<MatchState>,
    mut roster: ResMut<Roster>,
    mut damage: ResMut<DamageQueue>,
    mut fx: ResMut<FxQueue>,
    mut out: ResMut<FxOutbox>,
    mut pickups: Query<(Entity, &Transform, &mut PowerUpBrain)>,
    enemies: Query<(Entity, &Transform, &EnemyBrain)>,
) {
    let dt = time.delta_secs();
    for (e, tf, mut p) in &mut pickups {
        p.life -= dt;
        if p.life <= 0.0 {
            commands.entity(e).despawn();
            continue;
        }
        let grabbed = roster
            .0
            .values()
            .any(|pl| pl.alive && pl.feet().with_y(0.0).distance(tf.translation.with_y(0.0)) < 1.6);
        if !grabbed {
            continue;
        }
        commands.entity(e).despawn();
        state.last_powerup = Some(p.kind);
        state.powerup_seq += 1;
        match p.kind {
            PowerUp::Nuke => {
                // Bosses shrug it off.
                for (enemy, t, _) in enemies.iter().filter(|(_, _, b)| !b.kind.is_boss()) {
                    damage.0.push(DamageEvent {
                        target: enemy,
                        amount: f32::MAX / 4.0,
                        from: None,
                        headshot: false,
                        legs: false,
                        elements: 0,
                        chained: true,
                        stun: 0.0,
                        knockback: Vec3::ZERO,
                    });
                    emit(
                        &mut fx,
                        &mut out,
                        Fx::Explosion {
                            pos: (t.translation + Vec3::Y).to_array(),
                            radius: 1.5,
                            color: [1.0, 0.6, 0.2],
                        },
                    );
                }
                for pl in roster.0.values_mut() {
                    give_points(pl, 400, &state);
                }
            }
            PowerUp::InstaKill => state.insta_kill = 30.0,
            PowerUp::DoublePoints => state.double_points = 30.0,
            PowerUp::MaxAmmo => state.max_ammo_seq += 1,
        }
    }
}

fn mystery_box(
    time: Res<Time>,
    map: Res<CurrentMap>,
    mut state: ResMut<MatchState>,
    mut roster: ResMut<Roster>,
) {
    let dt = time.delta_secs();
    let mut rng = rand::thread_rng();
    state.box_state = match state.box_state {
        BoxState::Rolling { player, time } if time - dt <= 0.0 => {
            if state.box_uses >= state.box_move_after && rng.gen_bool(0.5) {
                // The box flies away; refund the spin.
                if let Some(p) = roster.0.get_mut(&player) {
                    p.points += BOX_COST;
                }
                let mut spot = state.box_spot;
                while spot == state.box_spot {
                    spot = rng.gen_range(0..map.0.box_spots.len() as u8);
                }
                state.box_spot = spot;
                state.box_uses = 0;
                state.box_move_after = rng.gen_range(4..9);
                BoxState::Moving { time: 4.0 }
            } else {
                let (held, fitted) = roster
                    .0
                    .get(&player)
                    .and_then(|p| {
                        let s = p.active_slot as usize;
                        p.guns[s].map(|g| (g, p.attach[s]))
                    })
                    .unwrap_or((0, Attach::NONE));
                let (gun, attach) = roll_armory(&mut rng, held, fitted);
                BoxState::Offer {
                    player,
                    gun,
                    attach,
                    time: 9.0,
                }
            }
        }
        BoxState::Rolling { player, time } => BoxState::Rolling {
            player,
            time: time - dt,
        },
        BoxState::Offer { time, .. } if time - dt <= 0.0 => BoxState::Idle,
        BoxState::Offer {
            player,
            gun,
            attach,
            time,
        } => BoxState::Offer {
            player,
            gun,
            attach,
            time: time - dt,
        },
        BoxState::Moving { time } if time - dt <= 0.0 => BoxState::Idle,
        BoxState::Moving { time } => BoxState::Moving { time: time - dt },
        BoxState::Idle => BoxState::Idle,
    };
}

/// Once the boss is dead, the whole team standing in the teleporter for
/// `TELEPORT_HOLD` seconds moves the run on to the next map.
fn teleporter(
    time: Res<Time>,
    map: Res<CurrentMap>,
    mut state: ResMut<MatchState>,
    mut roster: ResMut<Roster>,
) {
    if !state.teleport || state.game_over {
        return;
    }
    let living: Vec<&PlayerInfo> = roster.0.values().filter(|p| p.alive).collect();
    let all_in = !living.is_empty()
        && living
            .iter()
            .all(|p| p.feet().with_y(0.0).distance(map.0.extraction) < EXTRACT_RADIUS);
    if !all_in {
        state.teleport_hold = 0.0;
        return;
    }
    state.teleport_hold += time.delta_secs();
    if state.teleport_hold < TELEPORT_HOLD {
        return;
    }
    next_stage(&mut state, &mut roster);
}

/// Seconds the team has to stand in the teleporter.
pub const TELEPORT_HOLD: f32 = 3.0;

/// On to the next map of the run. Everyone keeps their guns, levels and
/// upgrades, and comes back up at full health.
fn next_stage(state: &mut MatchState, roster: &mut Roster) {
    let mut rng = rand::thread_rng();
    state.teleport = false;
    state.teleport_hold = 0.0;
    state.stage += 1;
    state.stage_round = 0;
    state.map = (state.map + 1) % crate::maps::MAP_NAMES.len() as u8;
    // The second time round the maps, it's the other time of day.
    if state.stage == crate::maps::MAP_NAMES.len() as u8 {
        state.night = !state.night;
    }
    state.to_spawn = 0;
    state.intermission = 8.0;
    state.box_spot = rng.gen_range(0..5);
    state.box_state = crate::BoxState::Idle;
    state.box_uses = 0;
    state.box_move_after = rng.gen_range(4..9);
    for p in roster.0.values_mut() {
        p.alive = true;
        p.health = p.max_health();
        p.spawn_seq += 1;
    }
}

fn check_game_over(roster: Res<Roster>, mut state: ResMut<MatchState>) {
    if state.started
        && !state.game_over
        && !roster.0.is_empty()
        && roster.0.values().all(|p| !p.alive)
    {
        state.game_over = true;
    }
}

/// Fresh match state for the host when a match begins.
pub fn new_match(state: &mut MatchState, roster: &mut Roster, map: u8) {
    let mut rng = rand::thread_rng();
    let (night, sandbox) = (state.night, state.sandbox);
    *state = MatchState::new(map);
    state.night = night;
    state.sandbox = sandbox;
    state.started = true;
    state.box_spot = rng.gen_range(0..5);
    state.box_move_after = rng.gen_range(4..9);
    for p in roster.0.values_mut() {
        p.reset_for_match();
        if sandbox.on {
            // Plenty of points to try things with.
            p.points = 50_000;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_knockback_exponential_decay() {
        let mut knockback = Vec3::new(12.0, 0.0, -8.0);
        let dt: f32 = 1.0 / 60.0;
        let initial_speed = knockback.length();

        // 1 frame of decay
        let decay = (-8.0f32 * dt).exp();
        knockback *= decay;
        assert!(knockback.length() < initial_speed);

        // After 0.5 seconds of frames, knockback should decay substantially (~e^-4 ≈ 0.018)
        for _ in 1..30 {
            knockback *= (-8.0f32 * dt).exp();
        }
        assert!(knockback.length() < initial_speed * 0.03);
    }

    #[test]
    fn test_explode_knockback_impulse_generation() {
        let mut queue = DamageQueue::default();
        let mut fx = FxQueue::default();
        let mut out = FxOutbox::default();

        let blast_pos = Vec3::new(0.0, 0.0, 0.0);
        let blast_radius = 5.0;
        let blast_damage = 100.0;

        let close_enemy = (Entity::from_raw(1), Vec3::new(1.0, 0.0, 0.0));
        let far_enemy = (Entity::from_raw(2), Vec3::new(4.0, 0.0, 0.0));
        let outside_enemy = (Entity::from_raw(3), Vec3::new(10.0, 0.0, 0.0));

        let enemies = vec![close_enemy, far_enemy, outside_enemy];
        explode(
            blast_pos,
            blast_radius,
            blast_damage,
            Some(1),
            0,
            &enemies,
            &mut queue,
            &mut fx,
            &mut out,
            Color::WHITE,
        );

        // 2 enemies inside blast, 1 outside
        assert_eq!(queue.0.len(), 2);

        let close_ev = &queue.0[0];
        assert_eq!(close_ev.target, close_enemy.0);
        assert!(close_ev.knockback.length() > 0.0);
        // Direction should push horizontally away from center (+X)
        assert!(close_ev.knockback.x > 0.0);

        let far_ev = &queue.0[1];
        assert_eq!(far_ev.target, far_enemy.0);
        // Closer enemy receives more impulse force
        assert!(close_ev.knockback.length() > far_ev.knockback.length());
    }

    #[test]
    fn test_heavy_hit_directional_impulse_and_resistance() {
        let shooter_feet = Vec3::new(0.0, 0.0, 0.0);
        let target_pos = Vec3::new(0.0, 0.0, 5.0);
        let push_dir = (target_pos - shooter_feet).with_y(0.0).normalize_or_zero();
        assert_eq!(push_dir, Vec3::new(0.0, 0.0, 1.0));

        let amount = 100.0; // heavy hit >= 75.0
        let normal_resist = 1.0;
        let brute_resist = 0.5;
        let boss_resist = 0.15;

        let normal_impulse = push_dir * (amount * 0.08 * normal_resist);
        let brute_impulse = push_dir * (amount * 0.08 * brute_resist);
        let boss_impulse = push_dir * (amount * 0.08 * boss_resist);

        assert!((normal_impulse.z - 8.0).abs() < 1e-5);
        assert!((brute_impulse.z - 4.0).abs() < 1e-5);
        assert!((boss_impulse.z - 1.2).abs() < 1e-5);
    }

    #[test]
    fn test_death_recoil_orientation() {
        // Shot came from (0, 0, -5) hitting target at (0, 0, 0) -> shot pushes toward +Z
        let shooter_feet = Vec3::new(0.0, 0.0, -5.0);
        let pos = Vec3::ZERO;
        let recoil_dir = (pos - shooter_feet).with_y(0.0).normalize_or_zero();
        assert_eq!(recoil_dir, Vec3::Z);

        // Target faces opposite to recoil direction so falling backward falls along recoil_dir (+Z)
        let face = -recoil_dir;
        let rot = Quat::from_rotation_arc(Vec3::NEG_Z, face);
        let forward = rot * Vec3::NEG_Z;
        assert!((forward - face).length() < 1e-5);
    }

    #[test]
    fn test_crowd_separation_overlapping_enemies_push_apart() {
        let e1 = Entity::from_raw(1);
        let e2 = Entity::from_raw(2);
        let crowd = vec![
            (e1, Vec3::new(0.0, 0.0, 0.0), NetKind::Grunt),
            (e2, Vec3::new(0.3, 0.0, 0.0), NetKind::Grunt),
        ];

        let force1 = compute_crowd_separation(e1, crowd[0].1, crowd[0].2, 0.0, &crowd);
        let force2 = compute_crowd_separation(e2, crowd[1].1, crowd[1].2, 0.0, &crowd);

        // Both enemies should push apart horizontally in opposite directions
        assert!(force1.x < -0.5, "e1 should be pushed in -X direction away from e2");
        assert!(force2.x > 0.5, "e2 should be pushed in +X direction away from e1");
        assert_eq!(force1.y, 0.0);
        assert_eq!(force2.y, 0.0);
        assert!((force1.x + force2.x).abs() < 1e-4, "Equal mass enemies push with equal and opposite force");
    }

    #[test]
    fn test_crowd_separation_mass_hierarchy_brute_pushes_standard() {
        let brute = Entity::from_raw(1);
        let grunt = Entity::from_raw(2);
        let crowd = vec![
            (brute, Vec3::new(0.0, 0.0, 0.0), NetKind::Brute),
            (grunt, Vec3::new(0.3, 0.0, 0.0), NetKind::Grunt),
        ];

        let force_brute = compute_crowd_separation(brute, crowd[0].1, crowd[0].2, 0.0, &crowd);
        let force_grunt = compute_crowd_separation(grunt, crowd[1].1, crowd[1].2, 0.0, &crowd);

        // Brute has higher mass: remains unaffected by lighter standard zombie
        assert_eq!(force_brute, Vec3::ZERO, "Brute should be unaffected by lighter standard zombie");

        // Grunt is strongly pushed away (+X)
        assert!(force_grunt.x > 1.0, "Grunt should be pushed away from brute");

        // Compare against grunt vs grunt at same distance: brute pushes grunt much harder
        let two_grunts = vec![
            (Entity::from_raw(3), Vec3::new(0.0, 0.0, 0.0), NetKind::Grunt),
            (grunt, Vec3::new(0.3, 0.0, 0.0), NetKind::Grunt),
        ];
        let force_grunt_vs_grunt = compute_crowd_separation(grunt, two_grunts[1].1, two_grunts[1].2, 0.0, &two_grunts);
        assert!(force_grunt.x > force_grunt_vs_grunt.x * 2.0, "Brute's mass hierarchy should significantly amplify push on grunt");
    }

    #[test]
    fn test_crowd_separation_boss_pushes_brute_and_boss_hierarchy() {
        let boss = Entity::from_raw(1);
        let brute = Entity::from_raw(2);
        let crowd = vec![
            (boss, Vec3::new(0.0, 0.0, 0.0), NetKind::Boss(1)),
            (brute, Vec3::new(0.6, 0.0, 0.0), NetKind::Brute),
        ];

        let force_boss = compute_crowd_separation(boss, crowd[0].1, crowd[0].2, 0.0, &crowd);
        let force_brute = compute_crowd_separation(brute, crowd[1].1, crowd[1].2, 0.0, &crowd);

        // Boss (mass 8.0) is unaffected by Brute (mass 3.5)
        assert_eq!(force_boss, Vec3::ZERO, "Boss should remain unaffected by Brute");
        // Brute is pushed away from Boss
        assert!(force_brute.x > 0.0, "Brute should be pushed away from Boss");
    }

    #[test]
    fn test_crowd_separation_non_overlapping_enemies_no_push() {
        let e1 = Entity::from_raw(1);
        let e2 = Entity::from_raw(2);
        // Standard grunt mutual radius is 0.45 + 0.45 = 0.90m. At 2.0m, no separation.
        let crowd = vec![
            (e1, Vec3::new(0.0, 0.0, 0.0), NetKind::Grunt),
            (e2, Vec3::new(2.0, 0.0, 0.0), NetKind::Grunt),
        ];

        let force1 = compute_crowd_separation(e1, crowd[0].1, crowd[0].2, 0.0, &crowd);
        let force2 = compute_crowd_separation(e2, crowd[1].1, crowd[1].2, 0.0, &crowd);

        assert_eq!(force1, Vec3::ZERO);
        assert_eq!(force2, Vec3::ZERO);
    }

    #[test]
    fn test_crowd_separation_force_clamping_prevents_jitter() {
        let center = Entity::from_raw(100);
        let mut crowd = vec![(center, Vec3::new(0.0, 0.0, 0.0), NetKind::Grunt)];

        // Crowd 25 grunts on one side all pressing against center
        for i in 1..=25 {
            crowd.push((
                Entity::from_raw(i),
                Vec3::new(0.1 + (i as f32) * 0.01, 0.0, 0.0),
                NetKind::Grunt,
            ));
        }

        let force = compute_crowd_separation(center, crowd[0].1, crowd[0].2, 0.0, &crowd);
        assert!(
            force.length() <= MAX_SEPARATION_FORCE + 1e-4,
            "Separation force should be clamped to MAX_SEPARATION_FORCE ({}), got {}",
            MAX_SEPARATION_FORCE,
            force.length()
        );
        assert!(force.x < 0.0, "Force should push away from the horde");
    }

    #[test]
    fn test_move_enemies_accumulates_separation_velocity() {
        let e1 = Entity::from_raw(1);
        let e2 = Entity::from_raw(2);
        let crowd = vec![
            (e1, Vec3::new(0.0, 0.0, 0.0), NetKind::Grunt),
            (e2, Vec3::new(0.3, 0.0, 0.0), NetKind::Grunt),
        ];

        let mut vel = Vec3::new(2.0, 0.0, 0.0);
        move_enemies(e1, crowd[0].1, crowd[0].2, 0.0, &mut vel, &crowd);

        // e2 is in +X direction, so repulsive push pushes e1 in -X, reducing velocity
        assert!(vel.x < 2.0);
    }

    #[test]
    fn test_crowd_spatial_grid_matches_direct() {
        let e1 = Entity::from_raw(1);
        let e2 = Entity::from_raw(2);
        let e3 = Entity::from_raw(3);
        let crowd = vec![
            (e1, Vec3::new(0.0, 0.0, 0.0), NetKind::Grunt),
            (e2, Vec3::new(0.4, 0.0, 0.0), NetKind::Grunt),
            (e3, Vec3::new(10.0, 0.0, 10.0), NetKind::Brute), // Far away
        ];

        let mut grid = CrowdSpatialGrid::new(2.5, crowd.len());
        for (i, &(_, p, _)) in crowd.iter().enumerate() {
            grid.insert(i, p);
        }

        let direct_force = compute_crowd_separation(e1, crowd[0].1, crowd[0].2, 0.0, &crowd);
        let grid_force = compute_crowd_separation_grid(e1, crowd[0].1, crowd[0].2, 0.0, &crowd, &grid);

        assert!(
            (direct_force - grid_force).length() < 1e-4,
            "Spatial grid force {:?} should match direct calculation {:?}",
            grid_force,
            direct_force
        );
    }

    fn setup_projectile_app() -> App {
        use bevy::time::TimeUpdateStrategy;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(TimeUpdateStrategy::ManualDuration(std::time::Duration::from_millis(50)));
        app.init_resource::<Roster>();
        app.init_resource::<LastHurt>();
        app.init_resource::<FxQueue>();
        app.init_resource::<FxOutbox>();
        app.add_systems(Update, projectiles);
        app.update();
        app
    }

    #[test]
    fn test_projectile_gravity_curves_velocity_downward() {
        let mut app = setup_projectile_app();
        let shot = app.world_mut().spawn((
            Transform::from_xyz(0.0, 10.0, 0.0),
            FireballBrain::new(Vec3::new(10.0, 0.0, 0.0), 4.0, 20.0),
        )).id();

        // 1 tick: dt = 0.05
        app.update();

        let brain = app.world().get::<FireballBrain>(shot).unwrap();
        let tf = *app.world().get::<Transform>(shot).unwrap();

        // Gravity: PROJECTILE_GRAVITY * dt = 4.0 * 0.05 = 0.2 downward
        assert!((brain.velocity.y - (-0.2)).abs() < 1e-4);
        assert!((brain.velocity.x - 10.0).abs() < 1e-4);
        assert!(tf.translation.y < 10.0);
        assert!((tf.translation.x - 0.5).abs() < 1e-4);

        // Run 10 more ticks (0.5s total): velocity curves downward further
        for _ in 0..10 {
            app.update();
        }
        let brain = app.world().get::<FireballBrain>(shot).unwrap();
        // 11 ticks * 0.05 = 0.55s => vel.y = -4.0 * 0.55 = -2.2
        assert!((brain.velocity.y - (-2.2)).abs() < 1e-3);
    }

    #[test]
    fn test_projectile_ground_collision_and_splash_physics() {
        let mut app = setup_projectile_app();

        // Spawn a player within splash radius (< 2.2m)
        let mut roster = app.world_mut().resource_mut::<Roster>();
        let mut player = PlayerInfo::new(1, "Player1".to_string(), crate::Character::Bulwark, 0);
        player.pos = [1.0, 0.0, 0.0];
        player.health = 100.0;
        let initial_health = player.health;
        let initial_pos = player.feet();
        roster.0.insert(player.id, player);

        // Spawn a distant player outside splash radius (> 2.2m)
        let mut far_player = PlayerInfo::new(2, "Player2".to_string(), crate::Character::Bulwark, 0);
        far_player.pos = [10.0, 0.0, 0.0];
        far_player.health = 100.0;
        roster.0.insert(far_player.id, far_player);

        // Projectile positioned just above ground, moving downward to penetrate pos.y < 0.0
        let shot = app.world_mut().spawn((
            Transform::from_xyz(0.0, 0.05, 0.0),
            FireballBrain::new(Vec3::new(0.0, -2.0, 0.0), 4.0, 30.0),
        )).id();

        app.update();

        // Entity must be cleanly despawned on impact
        assert!(app.world().get_entity(shot).is_err() || app.world().get::<FireballBrain>(shot).is_none());

        // Explosion effect emitted
        let fx = app.world().resource::<FxQueue>();
        let had_explosion = fx.0.iter().any(|f| matches!(f, Fx::Explosion { radius, .. } if (*radius - 2.0).abs() < 1e-4));
        assert!(had_explosion, "Ground impact must emit Fx::Explosion with radius 2.0");

        // Player within blast radius takes splash damage (30.0 * 0.6 = 18.0) and knockback
        let roster = app.world().resource::<Roster>();
        let p1 = roster.0.get(&1).unwrap();
        assert!((p1.health - (initial_health - 18.0)).abs() < 1e-3, "Player within blast radius takes 0.6x splash damage");
        assert!(p1.feet() != initial_pos, "Player within blast radius must receive knockback");
        assert!(p1.feet().x > initial_pos.x, "Blast knockback pushes player away from ground blast origin");

        // Far player unharmed and unpushed
        let p2 = roster.0.get(&2).unwrap();
        assert_eq!(p2.health, 100.0);
        assert_eq!(p2.feet(), Vec3::new(10.0, 0.0, 0.0));
    }

    #[test]
    fn test_projectile_wall_collider_collision_and_splash() {
        let mut app = setup_projectile_app();

        // Spawn a box collider representing a wall/crate
        app.world_mut().spawn((
            Transform::from_xyz(5.0, 2.0, 0.0),
            Collider { half: Vec3::new(1.0, 1.0, 1.0) },
        ));

        // Spawn a projectile flying directly into the collider
        let shot = app.world_mut().spawn((
            Transform::from_xyz(4.5, 2.0, 0.0),
            FireballBrain::new(Vec3::new(10.0, 0.0, 0.0), 4.0, 25.0),
        )).id();

        app.update();

        // Entity despawns on wall collision
        assert!(app.world().get_entity(shot).is_err() || app.world().get::<FireballBrain>(shot).is_none());

        let fx = app.world().resource::<FxQueue>();
        let had_explosion = fx.0.iter().any(|f| matches!(f, Fx::Explosion { radius, .. } if (*radius - 2.0).abs() < 1e-4));
        assert!(had_explosion, "Wall impact must emit Fx::Explosion");
    }

    #[test]
    fn test_projectile_player_direct_hit_and_splash_bystander() {
        let mut app = setup_projectile_app();

        let mut roster = app.world_mut().resource_mut::<Roster>();
        // Target player right at (2.0, 0.0, 0.0) -> chest height at y=1.0
        let mut p_target = PlayerInfo::new(1, "Target".to_string(), crate::Character::Bulwark, 0);
        p_target.pos = [2.0, 0.0, 0.0];
        p_target.health = 100.0;
        let initial_target_pos = p_target.feet();
        roster.0.insert(p_target.id, p_target);

        // Nearby bystander at (2.8, 0.0, 0.0) - within 2.2m of hit
        let mut p_bystander = PlayerInfo::new(2, "Bystander".to_string(), crate::Character::Bulwark, 0);
        p_bystander.pos = [2.8, 0.0, 0.0];
        p_bystander.health = 100.0;
        let initial_bystander_pos = p_bystander.feet();
        roster.0.insert(p_bystander.id, p_bystander);

        // Projectile hitting target player at chest height
        let shot = app.world_mut().spawn((
            Transform::from_xyz(2.0, 1.0, 0.0),
            FireballBrain::new(Vec3::ZERO, 4.0, 40.0),
        )).id();

        app.update();

        // Projectile despawned
        assert!(app.world().get_entity(shot).is_err() || app.world().get::<FireballBrain>(shot).is_none());

        let roster = app.world().resource::<Roster>();
        let target = roster.0.get(&1).unwrap();
        let bystander = roster.0.get(&2).unwrap();

        // Target took direct hit damage (40.0)
        assert_eq!(target.health, 60.0);
        assert!(target.feet() != initial_target_pos);

        // Bystander took splash damage (40.0 * 0.6 = 24.0)
        assert_eq!(bystander.health, 76.0);
        assert!(bystander.feet() != initial_bystander_pos);
    }

    #[test]
    fn test_penetrated_bullet_deals_attenuated_damage() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(Session::default());
        app.insert_resource(MatchState::new(0));
        let mut roster = Roster::default();
        let mut player = PlayerInfo::new(0, "Shooter".to_string(), crate::Character::Bulwark, 0);
        player.guns[0] = Some(0);
        roster.0.insert(0, player);
        app.insert_resource(roster);
        app.insert_resource(ShotQueue::default());
        app.insert_resource(DamageQueue::default());
        app.insert_resource(FxQueue::default());
        app.insert_resource(FxOutbox::default());
        app.add_systems(Update, resolve_shots);

        // Spawn thin obstacle at z = 2.0 (0.2m thick)
        app.world_mut().spawn((
            Transform::from_xyz(0.0, 1.0, 2.0),
            Collider {
                half: Vec3::new(2.0, 1.0, 0.1),
            },
        ));

        // Spawn enemy behind thin obstacle at z = 4.0
        let enemy = app.world_mut().spawn((
            Transform::from_xyz(0.0, 0.0, 4.0),
            EnemyBrain {
                kind: NetKind::Grunt,
                health: 100.0,
                knockback: Vec3::ZERO,
                speed: 2.0,
                attack_timer: 1.0,
                burn: 0.0,
                burn_dps: 0.0,
                burn_by: None,
                slow: 0.0,
                max_health: 100.0,
                leg_damage: 0.0,
                crawler: false,
                swing: 0.0,
                stun: 0.0,
                flinch: 0.0,
                slam: false,
                volley: 0.0,
                summons: 0,
                poison: 0.0,
                poison_dps: 0.0,
                poison_by: None,
                marked: 0.0,
                is_sprinter: false,
                flank_bias: 0.0,
                attack_windup: None,
                poise: 35.0,
                max_poise: 35.0,
                poise_broken: 0.0,
                since_hit: 5.0,
            },
        )).id();

        // Push shot aimed straight at the enemy through the wall
        let mut shots = app.world_mut().resource_mut::<ShotQueue>();
        shots.0.push((
            0,
            crate::Shot {
                origin: [0.0, 1.0, 0.0],
                dir: [0.0, 0.0, 1.0],
                gun: 0,
                alt: false,
            },
        ));

        app.update();

        let dmg_queue = app.world().resource::<DamageQueue>();
        assert_eq!(dmg_queue.0.len(), 1);
        let dmg = &dmg_queue.0[0];
        assert_eq!(dmg.target, enemy);
        let def = crate::data::gun_def(0);
        let expected_unattenuated = def.damage;
        let expected_attenuated = expected_unattenuated * crate::physics::PENETRATION_DAMAGE_FACTOR;
        assert!(
            (dmg.amount - expected_attenuated).abs() < 1e-3,
            "Damage ({}) should equal attenuated ({})",
            dmg.amount,
            expected_attenuated
        );
    }

    #[test]
    fn test_poise_damage_accumulates_and_posture_break() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(MatchState::new(0));
        app.insert_resource(Roster::default());
        app.insert_resource(DamageQueue::default());
        app.insert_resource(FxQueue::default());
        app.insert_resource(FxOutbox::default());
        app.add_systems(Update, apply_damage);

        let enemy = app.world_mut().spawn((
            Transform::from_xyz(0.0, 0.0, 0.0),
            EnemyStatus::default(),
            EnemyBrain {
                kind: NetKind::Grunt,
                health: 500.0,
                knockback: Vec3::ZERO,
                speed: 2.0,
                attack_timer: 1.0,
                burn: 0.0,
                burn_dps: 0.0,
                burn_by: None,
                slow: 0.0,
                max_health: 500.0,
                leg_damage: 0.0,
                crawler: false,
                swing: 0.0,
                stun: 0.0,
                flinch: 0.0,
                slam: false,
                volley: 0.0,
                summons: 0,
                poison: 0.0,
                poison_dps: 0.0,
                poison_by: None,
                marked: 0.0,
                is_sprinter: false,
                flank_bias: 0.0,
                attack_windup: None,
                poise: 35.0,
                max_poise: 35.0,
                poise_broken: 0.0,
                since_hit: 5.0,
            },
        )).id();

        // 1st hit: 10 body damage -> 10 poise damage. Poise should accumulate (35 - 10 = 25).
        let mut queue = app.world_mut().resource_mut::<DamageQueue>();
        queue.0.push(DamageEvent {
            target: enemy,
            amount: 10.0,
            from: None,
            headshot: false,
            legs: false,
            elements: 0,
            chained: false,
            stun: 0.0,
            knockback: Vec3::ZERO,
        });
        app.update();

        let brain = app.world().get::<EnemyBrain>(enemy).unwrap();
        assert!((brain.poise - 25.0).abs() < 1e-4, "Poise should be 25.0 after 10 poise damage, got {}", brain.poise);
        assert_eq!(brain.poise_broken, 0.0);
        assert_eq!(brain.since_hit, 0.0);

        // 2nd hit: 10 headshot damage -> 15 poise damage (1.5x headshot multiplier).
        // 25 - 15 = 10 poise remaining.
        let mut queue = app.world_mut().resource_mut::<DamageQueue>();
        queue.0.push(DamageEvent {
            target: enemy,
            amount: 10.0,
            from: None,
            headshot: true,
            legs: false,
            elements: 0,
            chained: false,
            stun: 0.0,
            knockback: Vec3::ZERO,
        });
        app.update();

        let brain = app.world().get::<EnemyBrain>(enemy).unwrap();
        assert!((brain.poise - 10.0).abs() < 1e-4, "Poise should be 10.0 after 15 poise damage, got {}", brain.poise);
        assert_eq!(brain.poise_broken, 0.0);
        assert_eq!(brain.since_hit, 0.0);

        // 3rd hit: 15 body damage -> exceeds remaining 10 poise -> Posture Break!
        let mut queue = app.world_mut().resource_mut::<DamageQueue>();
        queue.0.push(DamageEvent {
            target: enemy,
            amount: 15.0,
            from: None,
            headshot: false,
            legs: false,
            elements: 0,
            chained: false,
            stun: 0.0,
            knockback: Vec3::ZERO,
        });
        app.update();

        let brain = app.world().get::<EnemyBrain>(enemy).unwrap();
        assert_eq!(brain.poise, 0.0, "Poise should be 0 on posture break");
        assert_eq!(brain.poise_broken, 1.2, "Poise broken timer should be set to 1.2s for normal enemy");
        assert!(brain.stun >= 1.2, "Stun duration should be set to at least 1.2s on posture break");

        // Verify that posture break triggered spark FX emission
        let fx_queue = app.world().resource::<FxQueue>();
        let spark_fx = fx_queue.0.iter().find(|fx| matches!(fx, Fx::Sparks { count: 20, .. }));
        assert!(spark_fx.is_some(), "Posture break should emit high-impact spark particle burst");
    }

    #[test]
    fn test_poise_broken_vulnerability_multiplier() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(MatchState::new(0));
        app.insert_resource(Roster::default());
        app.insert_resource(DamageQueue::default());
        app.insert_resource(FxQueue::default());
        app.insert_resource(FxOutbox::default());
        app.add_systems(Update, apply_damage);

        let initial_health = 100.0;
        let enemy = app.world_mut().spawn((
            Transform::from_xyz(0.0, 0.0, 0.0),
            EnemyStatus::default(),
            EnemyBrain {
                kind: NetKind::Grunt,
                health: initial_health,
                knockback: Vec3::ZERO,
                speed: 2.0,
                attack_timer: 1.0,
                burn: 0.0,
                burn_dps: 0.0,
                burn_by: None,
                slow: 0.0,
                max_health: initial_health,
                leg_damage: 0.0,
                crawler: false,
                swing: 0.0,
                stun: 1.0,
                flinch: 0.0,
                slam: false,
                volley: 0.0,
                summons: 0,
                poison: 0.0,
                poison_dps: 0.0,
                poison_by: None,
                marked: 0.0,
                is_sprinter: false,
                flank_bias: 0.0,
                attack_windup: None,
                poise: 0.0,
                max_poise: 35.0,
                poise_broken: 1.5, // Currently in a broken posture
                since_hit: 0.0,
            },
        )).id();

        let base_damage = 20.0;
        let mut queue = app.world_mut().resource_mut::<DamageQueue>();
        queue.0.push(DamageEvent {
            target: enemy,
            amount: base_damage,
            from: None,
            headshot: false,
            legs: false,
            elements: 0,
            chained: false,
            stun: 0.0,
            knockback: Vec3::ZERO,
        });
        app.update();

        let brain = app.world().get::<EnemyBrain>(enemy).unwrap();
        // Base damage 20.0 with 1.5x vulnerability multiplier = 30.0 damage dealt
        let expected_health = initial_health - base_damage * 1.5;
        assert!(
            (brain.health - expected_health).abs() < 1e-4,
            "Enemy in broken posture should take 1.5x damage: expected health {}, got {}",
            expected_health,
            brain.health
        );
    }

    #[test]
    fn test_poise_broken_boss_fx() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(MatchState::new(0));
        app.insert_resource(Roster::default());
        app.insert_resource(DamageQueue::default());
        app.insert_resource(FxQueue::default());
        app.insert_resource(FxOutbox::default());
        app.add_systems(Update, apply_damage);

        let boss = app.world_mut().spawn((
            Transform::from_xyz(10.0, 0.0, 5.0),
            EnemyStatus::default(),
            EnemyBrain {
                kind: NetKind::Boss(0),
                health: 1000.0,
                knockback: Vec3::ZERO,
                speed: 3.0,
                attack_timer: 1.0,
                burn: 0.0,
                burn_dps: 0.0,
                burn_by: None,
                slow: 0.0,
                max_health: 1000.0,
                leg_damage: 0.0,
                crawler: false,
                swing: 0.0,
                stun: 0.0,
                flinch: 0.0,
                slam: false,
                volley: 0.0,
                summons: 0,
                poison: 0.0,
                poison_dps: 0.0,
                poison_by: None,
                marked: 0.0,
                is_sprinter: false,
                flank_bias: 0.0,
                attack_windup: None,
                poise: 10.0,
                max_poise: 300.0,
                poise_broken: 0.0,
                since_hit: 0.0,
            },
        )).id();

        let mut queue = app.world_mut().resource_mut::<DamageQueue>();
        queue.0.push(DamageEvent {
            target: boss,
            amount: 20.0,
            from: None,
            headshot: false,
            legs: false,
            elements: 0,
            chained: false,
            stun: 0.0,
            knockback: Vec3::ZERO,
        });
        app.update();

        let brain = app.world().get::<EnemyBrain>(boss).unwrap();
        assert_eq!(brain.poise, 0.0);
        assert_eq!(brain.poise_broken, 2.5);

        let fx_queue = app.world().resource::<FxQueue>();
        let spark_fx = fx_queue.0.iter().find(|fx| matches!(fx, Fx::Sparks { count: 36, .. }));
        assert!(spark_fx.is_some(), "Boss posture break should emit 36 sparks");
    }

    #[test]
    fn test_poise_regeneration_after_delay() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(std::time::Duration::from_millis(100)));
        app.insert_resource(DamageQueue::default());
        app.add_systems(Update, status_effects);
        app.update(); // Initialize Bevy time delta

        let enemy = app.world_mut().spawn((
            EnemyStatus::default(),
            EnemyBrain {
                kind: NetKind::Grunt,
                health: 100.0,
                knockback: Vec3::ZERO,
                speed: 2.0,
                attack_timer: 1.0,
                burn: 0.0,
                burn_dps: 0.0,
                burn_by: None,
                slow: 0.0,
                max_health: 100.0,
                leg_damage: 0.0,
                crawler: false,
                swing: 0.0,
                stun: 0.0,
                flinch: 0.0,
                slam: false,
                volley: 0.0,
                summons: 0,
                poison: 0.0,
                poison_dps: 0.0,
                poison_by: None,
                marked: 0.0,
                is_sprinter: false,
                flank_bias: 0.0,
                attack_windup: None,
                poise: 15.0,
                max_poise: 35.0,
                poise_broken: 0.0,
                since_hit: 0.0, // Just took a hit
            },
        )).id();

        // 40 updates of 0.1s = 4.0s (less than 5.0s delay)
        for _ in 0..40 {
            app.update();
        }

        let brain = app.world().get::<EnemyBrain>(enemy).unwrap();
        assert_eq!(brain.poise, 15.0, "Poise should not regenerate before 5.0 seconds delay has passed");
        assert!((brain.since_hit - 4.0).abs() < 1e-3, "since_hit expected 4.0, got {}", brain.since_hit);

        // 15 updates of 0.1s = 1.5s (since_hit goes from 4.0s to 5.5s).
        // Since delay is 5.0s, regeneration occurred during the 5 frames where since_hit >= 5.0 (0.5s of regen).
        // At 10.0 poise/s, +5.0 poise regenerated -> 15.0 + 5.0 = 20.0 poise.
        for _ in 0..15 {
            app.update();
        }

        let brain = app.world().get::<EnemyBrain>(enemy).unwrap();
        assert!(brain.poise > 15.0, "Poise should regenerate once since_hit >= 5.0s, got {}", brain.poise);
        assert!((brain.poise - 20.0).abs() < 1e-3, "Expected poise 20.0 (15 + 0.5 * 10), got {}", brain.poise);

        // Another 20 updates of 0.1s = 2.0s -> 20.0 + 20.0 = 40.0, clamped at max_poise (35.0)
        for _ in 0..20 {
            app.update();
        }

        let brain = app.world().get::<EnemyBrain>(enemy).unwrap();
        assert_eq!(brain.poise, 35.0, "Poise should be clamped at max_poise");
    }

    #[test]
    fn test_poise_broken_recovery() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(std::time::Duration::from_millis(100)));
        app.insert_resource(DamageQueue::default());
        app.add_systems(Update, status_effects);
        app.update(); // Initialize Bevy time delta

        let enemy = app.world_mut().spawn((
            EnemyStatus::default(),
            EnemyBrain {
                kind: NetKind::Grunt,
                health: 100.0,
                knockback: Vec3::ZERO,
                speed: 2.0,
                attack_timer: 1.0,
                burn: 0.0,
                burn_dps: 0.0,
                burn_by: None,
                slow: 0.0,
                max_health: 100.0,
                leg_damage: 0.0,
                crawler: false,
                swing: 0.0,
                stun: 1.2,
                flinch: 0.0,
                slam: false,
                volley: 0.0,
                summons: 0,
                poison: 0.0,
                poison_dps: 0.0,
                poison_by: None,
                marked: 0.0,
                is_sprinter: false,
                flank_bias: 0.0,
                attack_windup: None,
                poise: 0.0,
                max_poise: 35.0,
                poise_broken: 1.2,
                since_hit: 0.0,
            },
        )).id();

        // Advance 6 * 0.1s = 0.6s -> halfway through stagger
        for _ in 0..6 {
            app.update();
        }

        let brain = app.world().get::<EnemyBrain>(enemy).unwrap();
        assert!((brain.poise_broken - 0.6).abs() < 1e-4);
        assert_eq!(brain.poise, 0.0);

        // Advance 7 * 0.1s = 0.7s -> stagger expires, poise should restore to max_poise
        for _ in 0..7 {
            app.update();
        }

        let brain = app.world().get::<EnemyBrain>(enemy).unwrap();
        assert!(brain.poise_broken <= 0.0);
        assert_eq!(brain.poise, brain.max_poise, "Poise should be restored to max_poise when posture break expires");
    }
}

