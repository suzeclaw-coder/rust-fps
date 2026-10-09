use bevy::prelude::*;
use std::f32::consts::{FRAC_PI_2, PI};

use crate::kit::c;
use crate::maps::interiors::Floor;
use crate::maps::props::{boxr, shade, v, Art};
use crate::maps::{Ground, MapLayout};

// ---------------------------------------------------------------------------
// 10 Bahamian Architectural Pastels (exact hex match)
// ---------------------------------------------------------------------------
pub const COL_TURQUOISE: Color = Color::srgb(0x5d as f32 / 255.0, 0xbc as f32 / 255.0, 0xb0 as f32 / 255.0); // #5dbcb0
pub const COL_CORAL:     Color = Color::srgb(0xec as f32 / 255.0, 0x8b as f32 / 255.0, 0x76 as f32 / 255.0); // #ec8b76
pub const COL_CREAM:     Color = Color::srgb(0xef as f32 / 255.0, 0xe2 as f32 / 255.0, 0xc2 as f32 / 255.0); // #efe2c2
pub const COL_SKY:       Color = Color::srgb(0x8c as f32 / 255.0, 0xc2 as f32 / 255.0, 0xe0 as f32 / 255.0); // #8cc2e0
pub const COL_YELLOW:    Color = Color::srgb(0xf0 as f32 / 255.0, 0xcf as f32 / 255.0, 0x7c as f32 / 255.0); // #f0cf7c
pub const COL_MINT:      Color = Color::srgb(0xa9 as f32 / 255.0, 0xdc as f32 / 255.0, 0xbf as f32 / 255.0); // #a9dcbf
pub const COL_PINK:      Color = Color::srgb(0xf2 as f32 / 255.0, 0xb3 as f32 / 255.0, 0xaa as f32 / 255.0); // #f2b3aa
pub const COL_WHITE:     Color = Color::srgb(0xf1 as f32 / 255.0, 0xed as f32 / 255.0, 0xe2 as f32 / 255.0); // #f1ede2
pub const COL_LAVENDER:  Color = Color::srgb(0xbd as f32 / 255.0, 0xb3 as f32 / 255.0, 0xda as f32 / 255.0); // #bdb3da
pub const COL_SEA:       Color = Color::srgb(0x6a as f32 / 255.0, 0xa6 as f32 / 255.0, 0xb8 as f32 / 255.0); // #6aa6b8

// Material Palette
pub const COL_TRIM_WHITE:  Color = Color::srgb(0.96, 0.95, 0.92);
pub const COL_THATCH:      Color = Color::srgb(0.68, 0.58, 0.38);
pub const COL_THATCH_DARK: Color = Color::srgb(0.50, 0.42, 0.26);
pub const COL_TIN_ROOF:    Color = Color::srgb(0.52, 0.56, 0.58);
pub const COL_RUST:        Color = Color::srgb(0.65, 0.32, 0.18);
pub const COL_WOOD_DECK:   Color = Color::srgb(0.62, 0.52, 0.40);
pub const COL_DARK_TIMBER: Color = Color::srgb(0.42, 0.32, 0.22);
pub const COL_RAW_TIMBER:  Color = Color::srgb(0.48, 0.38, 0.28);
pub const COL_GLASS:       Color = Color::srgb(0.45, 0.70, 0.78);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RoofShape {
    Gable,
    Hip,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RoofCover {
    ThatchFringe,
    CorrugatedRust,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PorchRail {
    Balusters,
    XRails,
}

pub struct CottageSpec {
    pub _name: &'static str,
    pub pos: Vec3,
    pub width: f32,
    pub depth: f32,
    pub height: f32,
    pub yaw: f32,
    pub pastel: Color,
    pub trim: Color,
    pub roof_shape: RoofShape,
    pub roof_cover: RoofCover,
    pub rail_type: PorchRail,
    pub two_storey: bool,
    pub porch_depth: f32,
}

pub fn tidewater() -> MapLayout {
    let mut m = MapLayout::new(
        Color::srgb(0.76, 0.7, 0.5),
        Ground::Sand,
        Color::srgb(0.5, 0.7, 0.95),
        10000.0,
    );

    // Player Spawns (Boardwalk near village entrance)
    m.player_spawns = vec![
        v(-1.2, 0.0, -2.0), v(0.0, 0.0, -2.0), v(1.2, 0.0, -2.0),
        v(-1.2, 0.0, 0.0),  v(0.0, 0.0, 0.0),  v(1.2, 0.0, 0.0),
        v(-0.8, 0.0, 2.0),  v(0.8, 0.0, 2.0),
    ];

    // Extraction Zone on Pier T-head deck
    m.extraction = v(0.0, 0.0, -46.5);

    // 5 Mystery Box Spots (logically placed across zones)
    m.box_spots = [
        v(4.0, 0.0, -48.5),   // 1. Pier T-head East
        v(-12.0, 0.0, -14.0), // 2. Stilt Hut S1 (Beachfront West)
        v(6.0, 0.0, 14.0),    // 3. Market Pavilion East (Village Plaza)
        v(12.0, 0.0, 53.0),   // 4. Upper Tool Shed 4 (Upper North)
        v(-20.0, 0.0, 14.0),  // 5. Cottage C Interior (2-Storey West Plaza)
    ];

    // 5 Perk Machines with matching orientations
    m.perk(0, -4.0, -48.5, FRAC_PI_2); // 1. Pier T-head West
    m.perk(1, -28.0, -11.5, 0.0);      // 2. Boathouse Interior
    m.perk(2, -6.0, 14.0, -FRAC_PI_2);  // 3. Market Pavilion West
    m.perk(3, 24.0, 36.0, PI);          // 4. Outside Cottage M (Upper East Street)
    m.perk(4, -28.0, 36.0, PI);         // 5. Outside Cottage K (Upper West Street)

    // 2 Upgrade Stations (Pack-a-Punch)
    m.upgrade_stations = vec![
        v(0.0, 0.0, -50.5), // Pier T-head Center
        v(0.0, 0.0, 6.0),   // Village Plaza boardwalk junction
    ];

    // 9 Wall Buys mounted on exterior walls facing walkable corridors
    m.wall_gun(-24.1, -14.0, FRAC_PI_2, 3);   // Boathouse East wall (faces East)
    m.wall_gun(-15.8, -2.0, FRAC_PI_2, 6);    // Cottage A East wall (faces East)
    m.wall_gun(15.8, -2.0, -FRAC_PI_2, 2);    // Cottage B West wall (faces West)
    m.wall_gun(-24.6, 14.0, -FRAC_PI_2, 10);  // Cottage C West wall (faces West)
    m.wall_gun(24.4, 14.0, FRAC_PI_2, 17);    // Cottage D East wall (faces East)
    m.wall_gun(-20.6, 28.0, -FRAC_PI_2, 13);  // Cottage G West wall (faces West)
    m.wall_gun(20.4, 26.0, FRAC_PI_2, 9);     // Cottage H East wall (faces East)
    m.wall_gun(-41.7, 22.0, FRAC_PI_2, 11);   // Shed 2 East wall (faces East)
    m.wall_gun(12.0, 49.8, PI, 5);            // Shed 4 South wall (faces South)

    // Boundaries & Horizon
    m.coastal_boundary();

    // -----------------------------------------------------------------------
    // Coastal Infrastructure: Pier & Curved Elevated Boardwalk
    // -----------------------------------------------------------------------
    build_pier_and_boardwalk(&mut m);

    // -----------------------------------------------------------------------
    // Shoreline Structures: Boathouse, Stilt Huts, Tool Shed 1
    // -----------------------------------------------------------------------
    build_boathouse(&mut m, v(-28.0, 0.0, -14.0));
    build_stilt_hut(&mut m, 1, v(-12.0, 0.0, -14.0), 0.0, COL_MINT);
    build_stilt_hut(&mut m, 2, v(14.0, 0.0, -14.0), 0.0, COL_CORAL);
    build_stilt_hut(&mut m, 3, v(30.0, 0.0, -12.0), 0.0, COL_YELLOW);
    build_tool_shed(&mut m, 1, v(-36.0, 0.0, -12.0), 0.0, c(0.77, 0.19, 0.17)); // Red door

    // -----------------------------------------------------------------------
    // Village Plaza: Market Stall Pavilion & Tool Sheds 2, 3
    // -----------------------------------------------------------------------
    build_market_pavilion(&mut m, v(0.0, 0.0, 14.0));
    build_tool_shed(&mut m, 2, v(-44.0, 0.0, 22.0), FRAC_PI_2, c(0.17, 0.58, 0.54)); // Teal door
    build_tool_shed(&mut m, 3, v(44.0, 0.0, 22.0), -FRAC_PI_2, c(0.90, 0.63, 0.09)); // Yellow door

    // -----------------------------------------------------------------------
    // All 14 Cottages (A through N) with exact pastels, trims, roofs & porches
    // -----------------------------------------------------------------------
    // Cottage A: Turquoise, 1-storey, hip corrugated metal with rust, baluster porch
    build_cottage(&mut m, CottageSpec {
        _name: "Cottage A",
        pos: v(-20.0, 0.0, -2.0),
        width: 8.0,
        depth: 7.0,
        height: 4.0,
        yaw: 0.0,
        pastel: COL_TURQUOISE,
        trim: COL_TRIM_WHITE,
        roof_shape: RoofShape::Hip,
        roof_cover: RoofCover::CorrugatedRust,
        rail_type: PorchRail::Balusters,
        two_storey: false,
        porch_depth: 1.8,
    });

    // Cottage B: Coral, 1-storey, gable thatch with fringe, X-rail porch
    build_cottage(&mut m, CottageSpec {
        _name: "Cottage B",
        pos: v(20.0, 0.0, -2.0),
        width: 8.0,
        depth: 7.0,
        height: 4.0,
        yaw: 0.0,
        pastel: COL_CORAL,
        trim: COL_CREAM,
        roof_shape: RoofShape::Gable,
        roof_cover: RoofCover::ThatchFringe,
        rail_type: PorchRail::XRails,
        two_storey: false,
        porch_depth: 1.8,
    });

    // Cottage C: Sky, 2-STOREY, gable corrugated metal with rust, baluster porch
    build_cottage(&mut m, CottageSpec {
        _name: "Cottage C",
        pos: v(-20.0, 0.0, 14.0),
        width: 9.0,
        depth: 8.0,
        height: 7.0,
        yaw: 0.0,
        pastel: COL_SKY,
        trim: COL_TRIM_WHITE,
        roof_shape: RoofShape::Gable,
        roof_cover: RoofCover::CorrugatedRust,
        rail_type: PorchRail::Balusters,
        two_storey: true,
        porch_depth: 1.8,
    });

    // Cottage D: Yellow, 1-storey, hip thatched with fringe, X-rail porch
    build_cottage(&mut m, CottageSpec {
        _name: "Cottage D",
        pos: v(20.0, 0.0, 14.0),
        width: 8.5,
        depth: 7.5,
        height: 4.0,
        yaw: 0.0,
        pastel: COL_YELLOW,
        trim: COL_TRIM_WHITE,
        roof_shape: RoofShape::Hip,
        roof_cover: RoofCover::ThatchFringe,
        rail_type: PorchRail::XRails,
        two_storey: false,
        porch_depth: 1.8,
    });

    // Cottage E: Mint, 1-storey, gable thatch with fringe, baluster porch
    build_cottage(&mut m, CottageSpec {
        _name: "Cottage E",
        pos: v(-36.0, 0.0, 8.0),
        width: 8.0,
        depth: 7.0,
        height: 4.0,
        yaw: FRAC_PI_2,
        pastel: COL_MINT,
        trim: COL_TRIM_WHITE,
        roof_shape: RoofShape::Gable,
        roof_cover: RoofCover::ThatchFringe,
        rail_type: PorchRail::Balusters,
        two_storey: false,
        porch_depth: 1.8,
    });

    // Cottage F: Pink, 1-storey, hip corrugated metal with rust, X-rail porch
    build_cottage(&mut m, CottageSpec {
        _name: "Cottage F",
        pos: v(36.0, 0.0, 8.0),
        width: 8.0,
        depth: 7.0,
        height: 4.0,
        yaw: -FRAC_PI_2,
        pastel: COL_PINK,
        trim: COL_CREAM,
        roof_shape: RoofShape::Hip,
        roof_cover: RoofCover::CorrugatedRust,
        rail_type: PorchRail::XRails,
        two_storey: false,
        porch_depth: 1.8,
    });

    // Cottage G: Cream, 2-STOREY, hip thatch with fringe, baluster porch
    build_cottage(&mut m, CottageSpec {
        _name: "Cottage G",
        pos: v(-16.0, 0.0, 28.0),
        width: 9.0,
        depth: 8.0,
        height: 7.0,
        yaw: 0.0,
        pastel: COL_CREAM,
        trim: COL_SEA,
        roof_shape: RoofShape::Hip,
        roof_cover: RoofCover::ThatchFringe,
        rail_type: PorchRail::Balusters,
        two_storey: true,
        porch_depth: 1.8,
    });

    // Cottage H: Sea, 1-storey, gable corrugated metal with rust, X-rail porch
    build_cottage(&mut m, CottageSpec {
        _name: "Cottage H",
        pos: v(16.0, 0.0, 26.0),
        width: 8.5,
        depth: 7.0,
        height: 4.0,
        yaw: 0.0,
        pastel: COL_SEA,
        trim: COL_TRIM_WHITE,
        roof_shape: RoofShape::Gable,
        roof_cover: RoofCover::CorrugatedRust,
        rail_type: PorchRail::XRails,
        two_storey: false,
        porch_depth: 1.8,
    });

    // Cottage I: Lavender, 1-storey, gable thatch with fringe, baluster porch
    build_cottage(&mut m, CottageSpec {
        _name: "Cottage I",
        pos: v(-34.0, 0.0, 32.0),
        width: 8.0,
        depth: 7.0,
        height: 4.0,
        yaw: FRAC_PI_2,
        pastel: COL_LAVENDER,
        trim: COL_TRIM_WHITE,
        roof_shape: RoofShape::Gable,
        roof_cover: RoofCover::ThatchFringe,
        rail_type: PorchRail::Balusters,
        two_storey: false,
        porch_depth: 1.8,
    });

    // Cottage J: White, 1-storey, hip corrugated metal with rust, X-rail porch
    build_cottage(&mut m, CottageSpec {
        _name: "Cottage J",
        pos: v(34.0, 0.0, 32.0),
        width: 8.0,
        depth: 7.0,
        height: 4.0,
        yaw: -FRAC_PI_2,
        pastel: COL_WHITE,
        trim: COL_TURQUOISE,
        roof_shape: RoofShape::Hip,
        roof_cover: RoofCover::CorrugatedRust,
        rail_type: PorchRail::XRails,
        two_storey: false,
        porch_depth: 1.8,
    });

    // Cottage K: Coral, 1-storey, hip corrugated metal with rust, baluster porch
    build_cottage(&mut m, CottageSpec {
        _name: "Cottage K",
        pos: v(-24.0, 0.0, 42.0),
        width: 8.0,
        depth: 7.0,
        height: 4.0,
        yaw: 0.0,
        pastel: COL_CORAL,
        trim: COL_CREAM,
        roof_shape: RoofShape::Hip,
        roof_cover: RoofCover::CorrugatedRust,
        rail_type: PorchRail::Balusters,
        two_storey: false,
        porch_depth: 1.8,
    });

    // Cottage L: Turquoise, 2-STOREY (Grand Central Manor), gable thatch with fringe, baluster porch
    build_cottage(&mut m, CottageSpec {
        _name: "Cottage L",
        pos: v(0.0, 0.0, 42.0),
        width: 9.5,
        depth: 8.5,
        height: 7.2,
        yaw: 0.0,
        pastel: COL_TURQUOISE,
        trim: COL_TRIM_WHITE,
        roof_shape: RoofShape::Gable,
        roof_cover: RoofCover::ThatchFringe,
        rail_type: PorchRail::Balusters,
        two_storey: true,
        porch_depth: 1.8,
    });

    // Cottage M: Sky, 1-storey, gable corrugated metal with rust, X-rail porch
    build_cottage(&mut m, CottageSpec {
        _name: "Cottage M",
        pos: v(24.0, 0.0, 42.0),
        width: 8.0,
        depth: 7.0,
        height: 4.0,
        yaw: 0.0,
        pastel: COL_SKY,
        trim: COL_TRIM_WHITE,
        roof_shape: RoofShape::Gable,
        roof_cover: RoofCover::CorrugatedRust,
        rail_type: PorchRail::XRails,
        two_storey: false,
        porch_depth: 1.8,
    });

    // Cottage N: Yellow, 1-storey, hip thatched with fringe, baluster porch
    build_cottage(&mut m, CottageSpec {
        _name: "Cottage N",
        pos: v(-12.0, 0.0, 52.0),
        width: 7.5,
        depth: 6.5,
        height: 4.0,
        yaw: 0.0,
        pastel: COL_YELLOW,
        trim: COL_TRIM_WHITE,
        roof_shape: RoofShape::Hip,
        roof_cover: RoofCover::ThatchFringe,
        rail_type: PorchRail::Balusters,
        two_storey: false,
        porch_depth: 1.8,
    });

    // Upper Tool Shed 4: Blue door
    build_tool_shed(&mut m, 4, v(12.0, 0.0, 52.0), 0.0, c(0.17, 0.36, 0.58));

    // -----------------------------------------------------------------------
    // Doorways connecting areas for audio and nav registration
    // -----------------------------------------------------------------------
    add_doors(&mut m);

    // -----------------------------------------------------------------------
    // Environmental Dressing, Props & Nautical Decor
    // -----------------------------------------------------------------------
    // Moored Downeast fishing boat at pier T-head East face
    m.fishing_boat(v(9.8, 0.0, -48.5), 0.0, c(0.18, 0.38, 0.55));

    // Upturned rowboats resting tilted on beach sand
    m.upturned_rowboat(v(-18.0, 0.0, -10.0), 0.25, c(0.75, 0.25, 0.2));
    m.upturned_rowboat(v(22.0, 0.0, -10.0), -0.3, c(0.25, 0.48, 0.55));
    m.rowboat(v(-8.0, 0.0, -10.0), 0.4, c(0.68, 0.24, 0.18));
    m.rowboat(v(8.0, 0.0, -10.0), -0.3, c(0.20, 0.45, 0.30));

    // Fish cleaning table, drying racks, lobster traps, fish crates, net piles
    m.fish_cleaning_table(v(-5.0, 0.0, -48.5), 0.0);
    // Drying racks placed at perimeter beach sand away from cottage doorways
    m.fish_drying_rack(v(-28.0, 0.0, -10.0), 0.2);
    m.fish_drying_rack(v(28.0, 0.0, -10.0), -0.2);
    m.lobster_trap(v(-6.0, 0.0, -36.0), 0.1);
    m.lobster_trap(v(6.0, 0.0, -36.0), -0.15);
    m.lobster_trap(v(5.5, 0.0, -49.0), 0.3);
    m.net_pile(v(-3.0, 0.0, -46.5), c(0.25, 0.45, 0.35));
    m.net_pile(v(3.0, 0.0, -46.5), c(0.55, 0.45, 0.25));
    m.fish_crate(v(-6.0, 0.0, -47.0), 0.2, true);
    m.fish_crate(v(-6.0, 0.0, -48.0), -0.1, true);
    m.buoy_string(v(-1.45, 2.0, -22.0), 4);
    m.buoy_string(v(1.45, 2.0, -22.0), 4);

    // Weathered pier lamp posts with cantilever iron brackets
    m.pier_lamp_post(v(-1.6, 0.0, -22.0), FRAC_PI_2);
    m.pier_lamp_post(v(1.6, 0.0, -22.0), -FRAC_PI_2);
    m.pier_lamp_post(v(-1.6, 0.0, -34.0), FRAC_PI_2);
    m.pier_lamp_post(v(1.6, 0.0, -34.0), -FRAC_PI_2);
    m.pier_lamp_post(v(-6.8, 0.0, -48.0), 0.0);
    m.pier_lamp_post(v(6.8, 0.0, -48.0), 0.0);

    // Coconut palms across beach and village
    m.palm_tree(v(-28.0, 0.0, -4.0), 6.5, 1.2, 0.4);
    m.palm_tree(v(28.0, 0.0, -4.0), 7.0, -1.0, -0.3);
    m.palm_tree(v(-36.0, 0.0, 16.0), 6.0, 0.8, 1.1);
    m.palm_tree(v(36.0, 0.0, 16.0), 6.5, -0.9, -1.2);
    m.palm_tree(v(-46.0, 0.0, -18.0), 7.5, 1.4, 0.2);
    m.palm_tree(v(46.0, 0.0, -18.0), 7.0, -1.3, -0.5);
    m.palm_tree(v(-35.0, 0.0, 48.0), 6.8, 0.6, 2.0);
    m.palm_tree(v(35.0, 0.0, 48.0), 6.5, -0.7, -2.1);

    // Driftwood and coastal rocks along beachfront
    m.beach_driftwood(v(-18.0, 0.0, -12.0), 0.3);
    m.beach_driftwood(v(18.0, 0.0, -12.0), -0.5);
    m.coastal_rock(v(-32.0, 0.0, -18.0), v(2.5, 1.2, 2.0), 0.2);
    m.coastal_rock(v(34.0, 0.0, -18.0), v(2.2, 1.4, 2.4), -0.4);

    // Distant rocky sea stacks rising from the turquoise ocean bay
    m.sea_stack(v(-42.0, 0.0, -78.0), 7.5, 16.0);
    m.sea_stack(v(45.0, 0.0, -75.0), 8.5, 18.0);
    m.sea_stack(v(32.0, 0.0, -64.0), 5.0, 12.0);

    // -----------------------------------------------------------------------
    // 22 Zombie Spawns Across 4 Progressive Zones
    // -----------------------------------------------------------------------
    // Zone 0: Pier & Shoreline
    m.spawns(0, &[
        (-8.0, -20.0), (8.0, -20.0), (0.0, -32.0), (0.0, -40.0), (0.0, -48.0),
    ]);
    // Zone 1: Boardwalk & Lower Beach
    m.spawns(1, &[
        (-14.0, -8.0), (14.0, -8.0), (-26.0, -6.0), (26.0, -6.0), (0.0, -8.0),
    ]);
    // Zone 2: Village Plaza & Market
    m.spawns(2, &[
        (-12.0, 10.0), (12.0, 10.0), (0.0, 20.0), (-30.0, 16.0), (30.0, 16.0), (0.0, 8.0),
    ]);
    // Zone 3: Upper Village / Residential
    m.spawns(3, &[
        (-16.0, 36.0), (16.0, 36.0), (-36.0, 40.0), (36.0, 40.0), (0.0, 50.0), (-24.0, 50.0),
    ]);

    m
}

// ---------------------------------------------------------------------------
// Doorway Definitions
// ---------------------------------------------------------------------------
pub fn add_doors(m: &mut MapLayout) {
    m.door_x(-28.0, -18.75); // Boathouse entrance
    m.door_x(-20.0, -5.5);   // Cottage A
    m.door_x(20.0, -5.5);    // Cottage B
    m.door_x(-20.0, 10.0);   // Cottage C
    m.door_x(20.0, 10.25);   // Cottage D
    m.door_x(-16.0, 24.0);   // Cottage G
    m.door_x(16.0, 22.5);    // Cottage H
    m.door_x(0.0, 37.75);    // Cottage L
    m.door_x(-24.0, 38.5);   // Cottage K
}

// ---------------------------------------------------------------------------
// Pier Structure & Curved Elevated Boardwalk Generator
// ---------------------------------------------------------------------------
fn build_pier_and_boardwalk(m: &mut MapLayout) {
    let mut a = Art::default();

    // 1. Pier Walkway (2.6m wide deck at Y=2.3m / pile bents down to seabed -1.4m)
    let pier_z_shore = -18.0;
    let pier_z_end = -45.0;
    let pier_w = 2.8;
    let half_w = pier_w / 2.0;

    // Deck planks
    let mut z = pier_z_shore;
    while z >= pier_z_end {
        boxr(&mut a.paint, v(-half_w, 0.02, z - 0.12), v(half_w, 0.08, z + 0.12), COL_WOOD_DECK);
        z -= 0.28;
    }

    // Longitudinal stringers underneath
    for &x in &[-half_w + 0.15, 0.0, half_w - 0.15] {
        boxr(&mut a.paint, v(x - 0.08, -0.22, pier_z_end), v(x + 0.08, 0.02, pier_z_shore), COL_DARK_TIMBER);
    }

    // Pile bents every 3.5m driving down to seabed (-1.4m)
    let mut bent_z = pier_z_shore - 1.5;
    while bent_z >= pier_z_end {
        // Round timber piles
        a.paint.cyl(v(-half_w + 0.15, -0.65, bent_z), 0.18, 1.7, Quat::IDENTITY, COL_RAW_TIMBER);
        a.paint.cyl(v(half_w - 0.15, -0.65, bent_z), 0.18, 1.7, Quat::IDENTITY, COL_RAW_TIMBER);
        // Transverse cap beam
        boxr(&mut a.paint, v(-half_w + 0.05, -0.32, bent_z - 0.1), v(half_w - 0.05, -0.16, bent_z + 0.1), COL_RAW_TIMBER);
        // Diagonal cross-brace
        a.paint.cyl(v(0.0, -0.7, bent_z), 0.05, 2.2, Quat::from_rotation_z(0.65), COL_DARK_TIMBER);
        bent_z -= 3.5;
    }

    // Visual Pier Guardrails along both sides of pier walkway
    for &gx in &[-half_w - 0.05, half_w + 0.05] {
        // Top rail
        boxr(&mut a.paint, v(gx - 0.05, 1.02, pier_z_end), v(gx + 0.05, 1.10, pier_z_shore), COL_RAW_TIMBER);
        // Mid rail
        boxr(&mut a.paint, v(gx - 0.04, 0.52, pier_z_end), v(gx + 0.04, 0.60, pier_z_shore), COL_RAW_TIMBER);
        // Posts every 2.5m
        let mut pz = pier_z_shore;
        while pz >= pier_z_end - 0.1 {
            boxr(&mut a.paint, v(gx - 0.07, 0.0, pz - 0.07), v(gx + 0.07, 1.12, pz + 0.07), COL_RAW_TIMBER);
            pz -= 2.5;
        }
    }

    // Pier Entrance Arch at Z = -18.0
    boxr(&mut a.paint, v(-half_w - 0.1, 0.0, -18.1), v(-half_w + 0.1, 3.8, -17.9), COL_RAW_TIMBER);
    boxr(&mut a.paint, v(half_w - 0.1, 0.0, -18.1), v(half_w + 0.1, 3.8, -17.9), COL_RAW_TIMBER);
    boxr(&mut a.paint, v(-half_w - 0.2, 3.7, -18.15), v(half_w + 0.2, 3.95, -17.85), COL_RAW_TIMBER);
    // Painted fish signboard hanging from arch
    boxr(&mut a.paint, v(-0.8, 3.0, -18.04), v(0.8, 3.55, -17.96), COL_TURQUOISE);
    boxr(&mut a.paint, v(-0.7, 3.1, -18.06), v(0.7, 3.45, -17.94), COL_CREAM);
    // Cage lanterns on posts
    m.pier_lantern(v(-half_w, 2.5, -18.0));
    m.pier_lantern(v(half_w, 2.5, -18.0));

    // 2. Pier T-Head (14m wide x 7m deep: X in [-7.0, 7.0], Z in [-52.0, -45.0])
    let t_x0 = -7.0;
    let t_x1 = 7.0;
    let t_z0 = -52.0;
    let t_z1 = -45.0;

    // Deck planks on T-Head
    let mut tz = t_z1;
    while tz >= t_z0 {
        boxr(&mut a.paint, v(t_x0, 0.02, tz - 0.12), v(t_x1, 0.08, tz + 0.12), COL_WOOD_DECK);
        tz -= 0.28;
    }
    // Stringers underneath T-Head
    let mut sx = t_x0 + 0.5;
    while sx <= t_x1 - 0.4 {
        boxr(&mut a.paint, v(sx - 0.08, -0.22, t_z0), v(sx + 0.08, 0.02, t_z1), COL_DARK_TIMBER);
        sx += 2.0;
    }
    // Pile grid underneath T-Head
    for px in [-6.0, -3.0, 0.0, 3.0, 6.0] {
        for pz in [-51.0, -48.0, -45.5] {
            a.paint.cyl(v(px, -0.65, pz), 0.20, 1.7, Quat::IDENTITY, COL_RAW_TIMBER);
        }
    }

    // Visual T-head guardrails on West, South, and North edges
    for (gx0, gz0, gx1, gz1) in [
        (-7.0, -52.0, -7.0, -45.0), // West edge
        (-7.0, -52.0, -1.5, -52.0), // South West
        (1.5, -52.0, 7.0, -52.0),   // South East
        (-7.0, -45.0, -1.6, -45.0), // North West return
        (1.6, -45.0, 7.0, -45.0),   // North East return
    ] {
        boxr(&mut a.paint, v(gx0 - 0.05, 1.02, gz0 - 0.05), v(gx1 + 0.05, 1.10, gz1 + 0.05), COL_RAW_TIMBER);
        boxr(&mut a.paint, v(gx0 - 0.04, 0.52, gz0 - 0.04), v(gx1 + 0.04, 0.60, gz1 + 0.04), COL_RAW_TIMBER);
    }

    // 3. Berthing Face on East Edge of T-Head (X = 7.0, Z in [-52.0, -45.0])
    // Heavy vertical timber rubbing strakes
    let mut bz = t_z0 + 0.6;
    while bz <= t_z1 - 0.5 {
        boxr(&mut a.paint, v(t_x1 - 0.05, -1.2, bz - 0.15), v(t_x1 + 0.25, 0.8, bz + 0.15), COL_RAW_TIMBER);
        // Hanging black rubber tyre fenders
        a.paint.torus(v(t_x1 + 0.28, -0.2, bz), 0.12, 0.45, Quat::from_rotation_y(FRAC_PI_2), Color::srgb(0.12, 0.12, 0.14));
        // Fender support chains
        a.metal.cyl(v(t_x1 + 0.2, 0.4, bz), 0.02, 0.8, Quat::IDENTITY, Color::srgb(0.3, 0.3, 0.35));
        bz += 2.2;
    }
    // Mooring bollards & cleats along East edge
    m.bollard(v(t_x1 - 0.4, 0.0, -51.0));
    m.bollard(v(t_x1 - 0.4, 0.0, -46.0));
    m.cleat(v(t_x1 - 0.4, 0.0, -48.5), 0.0);
    // Vertical iron ladder climbing down into water
    for ly in [-1.0, -0.6, -0.2, 0.2, 0.6] {
        a.metal.cyl(v(t_x1 + 0.1, ly, -49.5), 0.02, 0.45, Quat::from_rotation_z(FRAC_PI_2), Color::srgb(0.25, 0.25, 0.28));
    }
    a.metal.cyl(v(t_x1 + 0.1, -0.2, -49.7), 0.02, 1.8, Quat::IDENTITY, Color::srgb(0.25, 0.25, 0.28));
    a.metal.cyl(v(t_x1 + 0.1, -0.2, -49.3), 0.02, 1.8, Quat::IDENTITY, Color::srgb(0.25, 0.25, 0.28));

    // Life rings & coils on T-head
    m.life_ring(v(-6.6, 0.8, -48.5), FRAC_PI_2);
    m.rope_coil(v(-4.5, 0.0, -46.5), 0.4);
    m.rope_coil(v(5.5, 0.0, -46.5), 0.35);

    // 4. Curved Elevated Boardwalk (Catmull-Rom spline from Pier Foot to Village Plaza)
    let spline_pts = [
        Vec2::new(0.0, -22.0),
        Vec2::new(0.0, -18.0),
        Vec2::new(-1.8, -12.0),
        Vec2::new(-1.0, -4.0),
        Vec2::new(1.5, 2.0),
        Vec2::new(0.0, 8.0),
        Vec2::new(0.0, 14.0),
    ];

    let bw_w = 3.2;
    let bw_half = bw_w / 2.0;
    let steps = 30;

    for i in 0..steps {
        let t_norm = i as f32 / steps as f32;
        let p_prev = eval_spline(&spline_pts, (t_norm - 0.02).clamp(0.0, 1.0));
        let p_cur = eval_spline(&spline_pts, t_norm);
        let p_next = eval_spline(&spline_pts, (t_norm + 0.02).clamp(0.0, 1.0));

        let dir = (p_next - p_prev).normalize_or_zero();
        let normal = Vec2::new(-dir.y, dir.x);

        let left = p_cur - normal * bw_half;
        let right = p_cur + normal * bw_half;

        // Timber deck plank
        let plank_dir = (right - left).normalize_or_zero();
        let plank_len = (right - left).length();
        let rot_y = (-plank_dir.y).atan2(plank_dir.x);
        a.paint.cuboid_rot(
            v(p_cur.x, 0.05, p_cur.y),
            v(plank_len, 0.06, 0.26),
            Quat::from_rotation_y(rot_y),
            COL_WOOD_DECK,
        );

        // Raised kickboards (curb rim) on left and right edges
        boxr(&mut a.paint, v(left.x - 0.06, 0.05, left.y - 0.06), v(left.x + 0.06, 0.16, left.y + 0.06), COL_RAW_TIMBER);
        boxr(&mut a.paint, v(right.x - 0.06, 0.05, right.y - 0.06), v(right.x + 0.06, 0.16, right.y + 0.06), COL_RAW_TIMBER);

        // Support posts driving into sand every few steps
        if i % 4 == 0 {
            a.paint.cyl(v(left.x, -0.3, left.y), 0.12, 1.0, Quat::IDENTITY, COL_RAW_TIMBER);
            a.paint.cyl(v(right.x, -0.3, right.y), 0.12, 1.0, Quat::IDENTITY, COL_RAW_TIMBER);
        }
    }

    // Side boardwalk branches to Shoreline & Cottages
    // Branch West towards Boathouse & Stilt Hut S1
    m.floor(-25.0, -14.0, -1.5, -11.0, Floor::Boards(COL_WOOD_DECK));
    // Branch East towards Stilt Huts S2 & S3
    m.floor(1.5, -14.0, 28.0, -11.0, Floor::Boards(COL_WOOD_DECK));
    // Central Plaza deck and promenade
    m.floor(-12.0, 6.0, 12.0, 24.0, Floor::Boards(COL_WOOD_DECK));
    m.floor(-30.0, 8.0, -12.0, 14.0, Floor::Boards(COL_WOOD_DECK));
    m.floor(12.0, 8.0, 30.0, 14.0, Floor::Boards(COL_WOOD_DECK));
    m.floor(-24.0, 24.0, 24.0, 30.0, Floor::Boards(COL_WOOD_DECK));
    m.floor(-4.0, 30.0, 4.0, 44.0, Floor::Boards(COL_WOOD_DECK));

    m.place(a, Vec3::ZERO, 0.0);

    // Guardrail colliders:
    // Placed at X = -2.1 and X = 2.1 so that cells 57 and 58 remain 100% unblocked in navgrid!
    let pier_len = pier_z_shore - pier_z_end;
    let pier_mid_z = (pier_z_shore + pier_z_end) / 2.0;
    m.collide(v(-2.1, 0.55, pier_mid_z), v(0.2, 1.1, pier_len));
    m.collide(v(2.1, 0.55, pier_mid_z), v(0.2, 1.1, pier_len));

    // T-head perimeter colliders
    m.collide(v(-7.1, 0.55, -48.5), v(0.2, 1.1, 7.0)); // West edge
    // South edge (split by center extraction zone)
    m.collide(v(-4.2, 0.55, -52.1), v(5.6, 1.1, 0.2));
    m.collide(v(4.2, 0.55, -52.1), v(5.6, 1.1, 0.2));
    // North return edges of T-head
    m.collide(v(-4.5, 0.55, -44.9), v(5.0, 1.1, 0.2));
    m.collide(v(4.5, 0.55, -44.9), v(5.0, 1.1, 0.2));
}

fn eval_spline(pts: &[Vec2], t: f32) -> Vec2 {
    let n = pts.len() - 3;
    let p = t * (n as f32);
    let idx = (p.floor() as usize).min(n - 1);
    let local_t = p - idx as f32;

    let p0 = pts[idx];
    let p1 = pts[idx + 1];
    let p2 = pts[idx + 2];
    let p3 = pts[idx + 3];

    let t2 = local_t * local_t;
    let t3 = t2 * local_t;

    0.5 * (
        (2.0 * p1) +
        (-p0 + p2) * local_t +
        (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2 +
        (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3
    )
}

// ---------------------------------------------------------------------------
// Procedural Cottage Generator
// ---------------------------------------------------------------------------
fn build_cottage(m: &mut MapLayout, spec: CottageSpec) {
    let mut a = Art::default();
    let w = spec.width;
    let d = spec.depth;
    let h = spec.height;
    let half_w = w / 2.0;
    let half_d = d / 2.0;

    let wall_col = spec.pastel;
    let trim_col = spec.trim;
    let dark_trim = shade(trim_col, 0.7);

    // 1. Exterior Walls with Horizontal Siding Boards
    // Back wall
    boxr(&mut a.paint, v(-half_w, 0.0, half_d - 0.25), v(half_w, h, half_d), wall_col);
    // Left & Right side walls
    boxr(&mut a.paint, v(-half_w, 0.0, -half_d), v(-half_w + 0.25, h, half_d), wall_col);
    boxr(&mut a.paint, v(half_w - 0.25, 0.0, -half_d), v(half_w, h, half_d), wall_col);

    // Front wall: door opening width 3.6m (-1.8 to 1.8)
    let door_w = 3.6;
    let half_door = door_w / 2.0;
    boxr(&mut a.paint, v(-half_w, 0.0, -half_d), v(-half_door, h, -half_d + 0.25), wall_col);
    boxr(&mut a.paint, v(half_door, 0.0, -half_d), v(half_w, h, -half_d + 0.25), wall_col);
    // Door lintel
    boxr(&mut a.paint, v(-half_door, 2.6, -half_d), v(half_door, h, -half_d + 0.25), wall_col);

    // Siding texture strips across main walls
    let layers = (h / 0.35).floor() as usize;
    for i in 0..layers {
        let y = (i as f32) * 0.35;
        boxr(&mut a.paint, v(-half_w - 0.02, y, -half_d - 0.02), v(-half_w + 0.27, y + 0.04, half_d + 0.02), shade(wall_col, 0.92));
        boxr(&mut a.paint, v(half_w - 0.27, y, -half_d - 0.02), v(half_w + 0.02, y + 0.04, half_d + 0.02), shade(wall_col, 0.92));
    }

    // Corner boards & Frieze top trims
    for &cx in &[-half_w, half_w - 0.16] {
        for &cz in &[-half_d, half_d - 0.16] {
            boxr(&mut a.paint, v(cx, 0.0, cz), v(cx + 0.16, h, cz + 0.16), trim_col);
        }
    }
    // Top frieze board along roofline
    boxr(&mut a.paint, v(-half_w - 0.08, h - 0.18, -half_d - 0.08), v(half_w + 0.08, h, half_d + 0.08), trim_col);

    // If 2-Storey: Middle Belt Trim & Second Floor Details
    if spec.two_storey {
        let mid_y = h * 0.5;
        boxr(&mut a.paint, v(-half_w - 0.08, mid_y - 0.12, -half_d - 0.08), v(half_w + 0.08, mid_y + 0.08, half_d + 0.08), trim_col);

        // Second floor front windows with Bahama shutters
        for &wx in &[(-half_w - half_door) / 2.0, (half_w + half_door) / 2.0] {
            build_window(&mut a, v(wx, mid_y + 1.2, -half_d - 0.02), 1.2, 1.4, trim_col, dark_trim);
        }
    }

    // Ground floor front windows
    build_window(&mut a, v((-half_w - half_door) / 2.0, 1.4, -half_d - 0.02), 1.2, 1.4, trim_col, dark_trim);
    build_window(&mut a, v((half_w + half_door) / 2.0, 1.4, -half_d - 0.02), 1.2, 1.4, trim_col, dark_trim);

    // 2. Front Porch (Timber deck, Chamfered posts, Roof, Balusters / X-Rails, Stairs)
    let p_depth = spec.porch_depth;
    let p_z_front = -half_d - p_depth;
    let p_h = if spec.two_storey { h * 0.5 } else { 2.8 };

    // Porch floor deck (weathered boards)
    boxr(&mut a.paint, v(-half_w, 0.04, p_z_front), v(half_w, 0.16, -half_d), COL_WOOD_DECK);

    // Porch posts (4 posts across front flanking opening)
    let post_w = 0.16;
    for &px in &[-half_w + 0.08, -half_door, half_door - post_w, half_w - 0.08 - post_w] {
        boxr(&mut a.paint, v(px, 0.16, p_z_front), v(px + post_w, p_h, p_z_front + post_w), trim_col);
    }
    // Porch roof eaves beam
    boxr(&mut a.paint, v(-half_w - 0.1, p_h - 0.16, p_z_front - 0.1), v(half_w + 0.1, p_h, p_z_front + post_w + 0.05), trim_col);

    // Slanted porch roof shed
    let porch_roof_col = match spec.roof_cover {
        RoofCover::ThatchFringe => COL_THATCH,
        RoofCover::CorrugatedRust => COL_TIN_ROOF,
    };
    boxr(&mut a.paint, v(-half_w - 0.2, p_h - 0.05, p_z_front - 0.2), v(half_w + 0.2, p_h + 0.45, -half_d), porch_roof_col);

    // Porch Railings (Balusters or X-Rails flanking the 3.6m opening)
    match spec.rail_type {
        PorchRail::Balusters => {
            build_baluster_run(&mut a, v(-half_w + 0.2, 0.16, p_z_front + 0.08), v(-half_door - 0.05, 0.16, p_z_front + 0.08), trim_col);
            build_baluster_run(&mut a, v(half_door + 0.05, 0.16, p_z_front + 0.08), v(half_w - 0.2, 0.16, p_z_front + 0.08), trim_col);
            build_baluster_run(&mut a, v(-half_w + 0.08, 0.16, -half_d), v(-half_w + 0.08, 0.16, p_z_front + 0.2), trim_col);
            build_baluster_run(&mut a, v(half_w - 0.08, 0.16, -half_d), v(half_w - 0.08, 0.16, p_z_front + 0.2), trim_col);
        }
        PorchRail::XRails => {
            build_x_rail(&mut a, v(-half_w + 0.18, 0.16, p_z_front + 0.08), v(-half_door - 0.05, 0.95, p_z_front + 0.08), trim_col);
            build_x_rail(&mut a, v(half_door + 0.05, 0.16, p_z_front + 0.08), v(half_w - 0.18, 0.95, p_z_front + 0.08), trim_col);
            build_x_rail(&mut a, v(-half_w + 0.08, 0.16, -half_d), v(-half_w + 0.08, 0.95, p_z_front + 0.2), trim_col);
            build_x_rail(&mut a, v(half_w - 0.08, 0.16, -half_d), v(half_w - 0.08, 0.95, p_z_front + 0.2), trim_col);
        }
    }

    // Porch Stairs (Central opening -half_door to half_door leading down to ground)
    boxr(&mut a.paint, v(-half_door + 0.1, 0.02, p_z_front - 0.4), v(half_door - 0.1, 0.08, p_z_front), COL_WOOD_DECK);
    boxr(&mut a.paint, v(-half_door + 0.1, 0.08, p_z_front - 0.2), v(half_door - 0.1, 0.14, p_z_front), COL_WOOD_DECK);

    // 3. Roof System (Gable or Hip, with Thatch Fringe or Corrugated Metal with Rust)
    let rh = (d * 0.28).min(3.2);
    let slope = (rh / half_d).atan();
    let slab_len = (half_d.powi(2) + rh * rh).sqrt() + 0.6;

    let roof_base_col = match spec.roof_cover {
        RoofCover::ThatchFringe => COL_THATCH,
        RoofCover::CorrugatedRust => COL_TIN_ROOF,
    };

    match spec.roof_shape {
        RoofShape::Gable => {
            // Gable wall wedges on left & right
            a.paint.wedge(v(-half_w + 0.1, h + rh / 2.0, 0.0), v(d, rh, 0.2), Quat::from_rotation_y(FRAC_PI_2), wall_col);
            a.paint.wedge(v(half_w - 0.1, h + rh / 2.0, 0.0), v(d, rh, 0.2), Quat::from_rotation_y(-FRAC_PI_2), wall_col);
            // Sloping roof slabs front and back
            for &s in &[-1.0f32, 1.0] {
                let rot = Quat::from_rotation_x(s * slope);
                let at = v(0.0, h + rh / 2.0 + 0.08, s * (half_d / 2.0) + s * 0.12);
                a.paint.cuboid_rot(at, v(w + 0.8, 0.14, slab_len), rot, roof_base_col);
            }
        }
        RoofShape::Hip => {
            for &s in &[-1.0f32, 1.0] {
                let rot = Quat::from_rotation_x(s * slope);
                let at = v(0.0, h + rh / 2.0 + 0.08, s * (half_d / 2.0) + s * 0.12);
                a.paint.cuboid_rot(at, v(w + 0.8, 0.14, slab_len), rot, roof_base_col);
            }
            a.paint.wedge(v(-half_w + 0.1, h + rh / 2.0, 0.0), v(d, rh, 0.2), Quat::from_rotation_y(FRAC_PI_2), roof_base_col);
            a.paint.wedge(v(half_w - 0.1, h + rh / 2.0, 0.0), v(d, rh, 0.2), Quat::from_rotation_y(-FRAC_PI_2), roof_base_col);
        }
    }

    // Surface detailing based on roof cover
    match spec.roof_cover {
        RoofCover::ThatchFringe => {
            // Dangling Thatch Fringe along front and back eaves
            for &s in &[-1.0f32, 1.0] {
                let fringe_z = s * (half_d + 0.35);
                let mut fx = -half_w - 0.3;
                while fx <= half_w + 0.3 {
                    boxr(&mut a.paint, v(fx - 0.08, h - 0.16, fringe_z - 0.04), v(fx + 0.08, h + 0.06, fringe_z + 0.04), COL_THATCH_DARK);
                    fx += 0.28;
                }
            }
            // Braided thatch ridge cap
            boxr(&mut a.paint, v(-half_w - 0.4, h + rh - 0.06, -0.22), v(half_w + 0.4, h + rh + 0.16, 0.22), shade(COL_THATCH, 0.9));
        }
        RoofCover::CorrugatedRust => {
            for &s in &[-1.0f32, 1.0] {
                let rot = Quat::from_rotation_x(s * slope);
                let at = v(0.0, h + rh / 2.0 + 0.08, s * (half_d / 2.0) + s * 0.12);
                // Longitudinal corrugated ridges
                for r in 0..6 {
                    let rx = (-w / 2.0 + 0.5) + (r as f32) * (w - 1.0) / 5.0;
                    a.paint.cuboid_rot(at + rot * v(rx, 0.08, 0.0), v(0.06, 0.05, slab_len), rot, shade(COL_TIN_ROOF, 0.85));
                }
                // Oxide Rust patches & streaks
                a.paint.cuboid_rot(at + rot * v(s * 1.2, 0.08, 0.3), v(1.8, 0.02, 1.2), rot, COL_RUST);
            }
            // Metal ridge cap
            boxr(&mut a.paint, v(-half_w - 0.3, h + rh - 0.04, -0.16), v(half_w + 0.3, h + rh + 0.12, 0.16), COL_TIN_ROOF);
        }
    }

    // Place building Art
    m.place(a, spec.pos, spec.yaw);

    // Wall Colliders (leaving 3.6m door opening & porch steps open)
    let front_seg_w = half_w - half_door;
    // Back wall
    m.collide_local(spec.pos, spec.yaw, v(0.0, h / 2.0, half_d - 0.15), v(w, h, 0.3));
    // Left & Right walls
    m.collide_local(spec.pos, spec.yaw, v(-half_w + 0.15, h / 2.0, 0.0), v(0.3, h, d));
    m.collide_local(spec.pos, spec.yaw, v(half_w - 0.15, h / 2.0, 0.0), v(0.3, h, d));
    // Front wall left & right segments
    m.collide_local(spec.pos, spec.yaw, v(-half_w + front_seg_w / 2.0, h / 2.0, -half_d + 0.15), v(front_seg_w, h, 0.3));
    m.collide_local(spec.pos, spec.yaw, v(half_w - front_seg_w / 2.0, h / 2.0, -half_d + 0.15), v(front_seg_w, h, 0.3));
    // Porch railings
    m.collide_local(spec.pos, spec.yaw, v(-half_w + front_seg_w / 2.0, 0.5, p_z_front + 0.1), v(front_seg_w, 1.0, 0.2));
    m.collide_local(spec.pos, spec.yaw, v(half_w - front_seg_w / 2.0, 0.5, p_z_front + 0.1), v(front_seg_w, 1.0, 0.2));
    m.collide_local(spec.pos, spec.yaw, v(-half_w + 0.1, 0.5, -half_d - p_depth / 2.0), v(0.2, 1.0, p_depth));
    m.collide_local(spec.pos, spec.yaw, v(half_w - 0.1, 0.5, -half_d - p_depth / 2.0), v(0.2, 1.0, p_depth));

    // Register indoor area for audio acoustics
    let min_x = spec.pos.x - half_w;
    let max_x = spec.pos.x + half_w;
    let min_z = spec.pos.z - half_d;
    let max_z = spec.pos.z + half_d;
    m.indoor_areas.push([min_x.min(max_x), min_z.min(max_z), min_x.max(max_x), min_z.max(max_z)]);

    // Interior floor & ceiling lamp
    m.floor(min_x, min_z, max_x, max_z, Floor::Boards(COL_WOOD_DECK));
    m.lamp(spec.pos.x, spec.pos.z, 3.2, c(1.0, 0.88, 0.65), 35_000.0);
}

// Window with Bahama / board shutters
fn build_window(a: &mut Art, center: Vec3, width: f32, height: f32, frame_col: Color, shutter_col: Color) {
    let hw = width / 2.0;
    let hh = height / 2.0;
    // Window frame
    boxr(&mut a.paint, v(center.x - hw, center.y - hh, center.z - 0.04), v(center.x + hw, center.y + hh, center.z + 0.04), frame_col);
    // Glass pane
    a.glass.cuboid(center, v(width - 0.2, height - 0.2, 0.02), COL_GLASS);
    // White divided light mullions
    boxr(&mut a.paint, v(center.x - 0.02, center.y - hh, center.z), v(center.x + 0.02, center.y + hh, center.z + 0.05), frame_col);
    boxr(&mut a.paint, v(center.x - hw, center.y - 0.02, center.z), v(center.x + hw, center.y + 0.02, center.z + 0.05), frame_col);

    // Bahama top-hinged shutter propped open at angle
    let rot = Quat::from_rotation_x(0.38);
    a.paint.cuboid_rot(v(center.x, center.y + hh + 0.1, center.z - 0.25), v(width + 0.1, height * 0.9, 0.05), rot, shutter_col);
    // Shutter prop arms
    a.paint.cyl(v(center.x - hw + 0.08, center.y, center.z - 0.18), 0.015, 0.5, Quat::from_rotation_x(0.6), frame_col);
    a.paint.cyl(v(center.x + hw - 0.08, center.y, center.z - 0.18), 0.015, 0.5, Quat::from_rotation_x(0.6), frame_col);
}

// Vertical spindle balusters
fn build_baluster_run(a: &mut Art, p0: Vec3, p1: Vec3, col: Color) {
    // Top and bottom rails
    boxr(&mut a.paint, v(p0.x, 0.88, p0.z - 0.05), v(p1.x, 0.96, p1.z + 0.05), col);
    boxr(&mut a.paint, v(p0.x, 0.16, p0.z - 0.04), v(p1.x, 0.24, p1.z + 0.04), col);

    let len = (p1 - p0).length();
    let count = (len / 0.22).floor() as usize;
    for i in 1..count {
        let t = i as f32 / count as f32;
        let pos = p0.lerp(p1, t);
        boxr(&mut a.paint, v(pos.x - 0.025, 0.24, pos.z - 0.025), v(pos.x + 0.025, 0.88, pos.z + 0.025), col);
    }
}

// Bahamian cross-braced X-railings
fn build_x_rail(a: &mut Art, p0: Vec3, p1: Vec3, col: Color) {
    boxr(&mut a.paint, v(p0.x, p1.y - 0.08, p0.z - 0.05), v(p1.x, p1.y, p1.z + 0.05), col);
    boxr(&mut a.paint, v(p0.x, p0.y, p0.z - 0.04), v(p1.x, p0.y + 0.08, p1.z + 0.04), col);

    let mid = (p0 + p1) / 2.0;
    let dx = p1.x - p0.x;
    let dz = p1.z - p0.z;
    let len = (dx * dx + dz * dz).sqrt();
    let ang = (-dz).atan2(dx);

    let h = p1.y - p0.y;
    let diag_len = (len * len + h * h).sqrt();
    let pitch = (h / len).atan();

    let rot1 = Quat::from_rotation_y(ang) * Quat::from_rotation_z(pitch);
    let rot2 = Quat::from_rotation_y(ang) * Quat::from_rotation_z(-pitch);

    a.paint.cuboid_rot(v(mid.x, mid.y, mid.z), v(diag_len, 0.06, 0.06), rot1, col);
    a.paint.cuboid_rot(v(mid.x, mid.y, mid.z), v(diag_len, 0.06, 0.06), rot2, col);
}

// ---------------------------------------------------------------------------
// Stilt Fishing Huts (S1, S2, S3)
// ---------------------------------------------------------------------------
fn build_stilt_hut(m: &mut MapLayout, _id: usize, pos: Vec3, yaw: f32, siding_col: Color) {
    let mut a = Art::default();
    let w = 6.0;
    let d = 5.5;
    let half_w = w / 2.0;
    let half_d = d / 2.0;

    // Timber Stilts elevated at Y = 3.0m - 3.15m
    for &px in &[-half_w + 0.2, 0.0, half_w - 0.2] {
        for &pz in &[-half_d - 1.6, -half_d + 0.2, half_d - 0.2] {
            a.paint.cyl(v(px, 1.5, pz), 0.18, 3.15, Quat::IDENTITY, COL_RAW_TIMBER);
        }
    }
    // Sway cross-braces between pilings
    a.paint.cyl(v(-half_w / 2.0, 1.2, -half_d), 0.05, 3.2, Quat::from_rotation_z(0.7), COL_DARK_TIMBER);
    a.paint.cyl(v(half_w / 2.0, 1.2, -half_d), 0.05, 3.2, Quat::from_rotation_z(-0.7), COL_DARK_TIMBER);

    // Raw timber rim framing the deck
    boxr(&mut a.paint, v(-half_w - 0.1, 0.05, -half_d - 1.8), v(half_w + 0.1, 0.25, half_d + 0.1), COL_RAW_TIMBER);
    // Plank floor deck
    boxr(&mut a.paint, v(-half_w, 0.12, -half_d - 1.7), v(half_w, 0.24, half_d), COL_WOOD_DECK);

    // Weathered timber walls
    boxr(&mut a.paint, v(-half_w, 0.24, half_d - 0.2), v(half_w, 2.9, half_d), siding_col);
    boxr(&mut a.paint, v(-half_w, 0.24, -half_d), v(-half_w + 0.2, 2.9, half_d), siding_col);
    boxr(&mut a.paint, v(half_w - 0.2, 0.24, -half_d), v(half_w, 2.9, half_d), siding_col);

    // Front wall with 3.2m central opening (-1.6 to 1.6)
    let opening_half = 1.6;
    boxr(&mut a.paint, v(-half_w, 0.24, -half_d), v(-opening_half, 2.9, -half_d + 0.2), siding_col);
    boxr(&mut a.paint, v(opening_half, 0.24, -half_d), v(half_w, 2.9, -half_d + 0.2), siding_col);
    boxr(&mut a.paint, v(-opening_half, 2.5, -half_d), v(opening_half, 2.9, -half_d + 0.2), siding_col);

    // X-Rail Porch with draped fishing nets
    let p_z = -half_d - 1.6;
    build_x_rail(&mut a, v(-half_w + 0.1, 0.24, p_z), v(-opening_half, 1.05, p_z), COL_RAW_TIMBER);
    build_x_rail(&mut a, v(opening_half, 0.24, p_z), v(half_w - 0.1, 1.05, p_z), COL_RAW_TIMBER);
    build_x_rail(&mut a, v(-half_w, 0.24, -half_d), v(-half_w, 1.05, p_z), COL_RAW_TIMBER);
    build_x_rail(&mut a, v(half_w, 0.24, -half_d), v(half_w, 1.05, p_z), COL_RAW_TIMBER);

    // Draped colorful fishing nets hanging over railings
    a.paint.blob(v(-half_w + 0.8, 0.7, p_z - 0.05), v(1.0, 0.5, 0.15), Color::srgb(0.25, 0.45, 0.35));
    a.paint.blob(v(half_w - 0.8, 0.7, p_z - 0.05), v(1.0, 0.5, 0.15), Color::srgb(0.65, 0.55, 0.35));

    // Wooden stairs leading down to sand
    boxr(&mut a.paint, v(-opening_half + 0.1, 0.04, p_z - 0.4), v(opening_half - 0.1, 0.12, p_z), COL_WOOD_DECK);
    boxr(&mut a.paint, v(-opening_half + 0.1, 0.12, p_z - 0.2), v(opening_half - 0.1, 0.20, p_z), COL_WOOD_DECK);

    // Thatched hip roof
    let rh = 1.8;
    let slope = (rh / half_d).atan();
    let slab_len = (half_d.powi(2) + rh * rh).sqrt() + 0.6;
    for &s in &[-1.0f32, 1.0] {
        let rot = Quat::from_rotation_x(s * slope);
        let at = v(0.0, 2.9 + rh / 2.0, s * (half_d / 2.0));
        a.paint.cuboid_rot(at, v(w + 0.7, 0.18, slab_len), rot, COL_THATCH);
    }

    m.place(a, pos, yaw);

    // Colliders (leaving 3.2m doorway and stairs open)
    m.collide_local(pos, yaw, v(0.0, 1.5, half_d - 0.1), v(w, 2.8, 0.3));
    m.collide_local(pos, yaw, v(-half_w + 0.1, 1.5, 0.0), v(0.3, 2.8, d));
    m.collide_local(pos, yaw, v(half_w - 0.1, 1.5, 0.0), v(0.3, 2.8, d));
    let f_seg = half_w - opening_half;
    m.collide_local(pos, yaw, v(-half_w + f_seg / 2.0, 1.5, -half_d + 0.1), v(f_seg, 2.8, 0.3));
    m.collide_local(pos, yaw, v(half_w - f_seg / 2.0, 1.5, -half_d + 0.1), v(f_seg, 2.8, 0.3));
    m.collide_local(pos, yaw, v(-half_w + f_seg / 2.0, 0.5, p_z + 0.05), v(f_seg, 1.0, 0.2));
    m.collide_local(pos, yaw, v(half_w - f_seg / 2.0, 0.5, p_z + 0.05), v(f_seg, 1.0, 0.2));

    // Register indoor area & interior lamp
    let min_x = pos.x - half_w;
    let max_x = pos.x + half_w;
    let min_z = pos.z - half_d;
    let max_z = pos.z + half_d;
    m.indoor_areas.push([min_x.min(max_x), min_z.min(max_z), min_x.max(max_x), min_z.max(max_z)]);
    m.lamp(pos.x, pos.z, 2.8, c(1.0, 0.88, 0.65), 25_000.0);
}

// ---------------------------------------------------------------------------
// Tool Sheds with Lean-To Roofs & Colored Z-Braced Doors
// ---------------------------------------------------------------------------
fn build_tool_shed(m: &mut MapLayout, _id: usize, pos: Vec3, yaw: f32, door_col: Color) {
    let mut a = Art::default();
    let w = 4.8;
    let d = 4.0;
    let half_w = w / 2.0;
    let half_d = d / 2.0;
    let h_front = 3.2;
    let h_back = 2.4;

    let wood_col = c(0.48, 0.38, 0.28);

    // Weathered timber walls
    boxr(&mut a.paint, v(-half_w, 0.0, half_d - 0.2), v(half_w, h_back, half_d), wood_col);
    boxr(&mut a.paint, v(-half_w, 0.0, -half_d), v(-half_w + 0.2, h_front, half_d), wood_col);
    boxr(&mut a.paint, v(half_w - 0.2, 0.0, -half_d), v(half_w, h_front, half_d), wood_col);

    // Front wall with 2.4m door opening (-1.2 to 1.2)
    let d_open = 1.2;
    boxr(&mut a.paint, v(-half_w, 0.0, -half_d), v(-d_open, h_front, -half_d + 0.2), wood_col);
    boxr(&mut a.paint, v(d_open, 0.0, -half_d), v(half_w, h_front, -half_d + 0.2), wood_col);
    boxr(&mut a.paint, v(-d_open, 2.4, -half_d), v(d_open, h_front, -half_d + 0.2), wood_col);

    // Colored door with Z-bracing swung slightly ajar
    let door_rot = Quat::from_rotation_y(0.45);
    let door_at = v(-1.15, 1.15, -half_d - 0.2);
    a.paint.cuboid_rot(door_at, v(1.4, 2.2, 0.06), door_rot, door_col);
    // Diagonal Z-brace battens on door
    a.paint.cuboid_rot(door_at + door_rot * v(0.0, 0.9, 0.04), v(1.3, 0.12, 0.04), door_rot, shade(door_col, 0.8));
    a.paint.cuboid_rot(door_at + door_rot * v(0.0, -0.9, 0.04), v(1.3, 0.12, 0.04), door_rot, shade(door_col, 0.8));
    a.paint.cuboid_rot(door_at + door_rot * v(0.0, 0.0, 0.04), v(1.9, 0.10, 0.04), door_rot * Quat::from_rotation_z(0.85), shade(door_col, 0.8));

    // Lean-to sloping corrugated metal roof
    let slope = ((h_front - h_back) / d).atan();
    let slab_len = (d * d + (h_front - h_back).powi(2)).sqrt() + 0.6;
    let rot = Quat::from_rotation_x(slope);
    a.paint.cuboid_rot(v(0.0, (h_front + h_back) / 2.0 + 0.12, 0.0), v(w + 0.6, 0.12, slab_len), rot, COL_TIN_ROOF);

    m.place(a, pos, yaw);

    // Colliders (door opening unblocked)
    m.collide_local(pos, yaw, v(0.0, h_back / 2.0, half_d - 0.1), v(w, h_back, 0.3));
    m.collide_local(pos, yaw, v(-half_w + 0.1, h_front / 2.0, 0.0), v(0.3, h_front, d));
    m.collide_local(pos, yaw, v(half_w - 0.1, h_front / 2.0, 0.0), v(0.3, h_front, d));
    let f_seg = half_w - d_open;
    m.collide_local(pos, yaw, v(-half_w + f_seg / 2.0, h_front / 2.0, -half_d + 0.1), v(f_seg, h_front, 0.3));
    m.collide_local(pos, yaw, v(half_w - f_seg / 2.0, h_front / 2.0, -half_d + 0.1), v(f_seg, h_front, 0.3));

    // Register indoor area & interior lamp
    let min_x = pos.x - half_w;
    let max_x = pos.x + half_w;
    let min_z = pos.z - half_d;
    let max_z = pos.z + half_d;
    m.indoor_areas.push([min_x.min(max_x), min_z.min(max_z), min_x.max(max_x), min_z.max(max_z)]);
    m.lamp(pos.x, pos.z, 2.5, c(1.0, 0.88, 0.65), 20_000.0);
}

// ---------------------------------------------------------------------------
// Boathouse with Slipway Rails, Flagpole & Internal Boat Cradle
// ---------------------------------------------------------------------------
fn build_boathouse(m: &mut MapLayout, pos: Vec3) {
    let mut a = Art::default();
    let w = 7.5;
    let d = 9.5;
    let half_w = w / 2.0;
    let half_d = d / 2.0;
    let h = 4.8;
    let wood_col = c(0.48, 0.38, 0.28);

    // Back wall (North)
    boxr(&mut a.paint, v(-half_w, 0.0, half_d - 0.25), v(half_w, h, half_d), wood_col);
    // Left & Right walls
    boxr(&mut a.paint, v(-half_w, 0.0, -half_d), v(-half_w + 0.25, h, half_d), wood_col);
    boxr(&mut a.paint, v(half_w - 0.25, 0.0, -half_d), v(half_w, h, half_d), wood_col);

    // Open front facing South (waterfront): 4.5m wide open boat bay
    boxr(&mut a.paint, v(-half_w, 0.0, -half_d), v(-2.25, h, -half_d + 0.25), wood_col);
    boxr(&mut a.paint, v(2.25, 0.0, -half_d), v(half_w, h, -half_d + 0.25), wood_col);
    boxr(&mut a.paint, v(-2.25, 3.8, -half_d), v(2.25, h, -half_d + 0.25), wood_col);

    // Gable triangular end wall apex
    let rh = 2.4;
    a.paint.wedge(v(0.0, h + rh / 2.0, -half_d + 0.1), v(w, rh, 0.2), Quat::IDENTITY, wood_col);
    a.paint.wedge(v(0.0, h + rh / 2.0, half_d - 0.1), v(w, rh, 0.2), Quat::from_rotation_y(PI), wood_col);

    // Corrugated metal roof
    let slope = (rh / half_w).atan();
    let slab_len = (half_w.powi(2) + rh * rh).sqrt() + 0.6;
    for &s in &[-1.0f32, 1.0] {
        let rot = Quat::from_rotation_z(s * slope);
        let at = v(s * (half_w / 2.0), h + rh / 2.0 + 0.1, 0.0);
        a.paint.cuboid_rot(at, v(slab_len, 0.14, d + 0.8), rot, COL_TIN_ROOF);
        // Rust streaks
        a.paint.cuboid_rot(at + v(0.0, 0.08, -2.0), v(slab_len * 0.8, 0.02, 1.8), rot, COL_RUST);
    }

    // Slipway rails extending towards water from Z = -half_d to Z = -half_d - 7.0
    for &rx in &[-1.1, 1.1] {
        boxr(&mut a.paint, v(rx - 0.08, 0.04, -half_d - 7.0), v(rx + 0.08, 0.16, half_d - 1.0), COL_DARK_TIMBER);
    }
    let mut tie_z = -half_d - 6.8;
    while tie_z <= half_d - 1.2 {
        boxr(&mut a.paint, v(-1.4, 0.02, tie_z - 0.08), v(1.4, 0.08, tie_z + 0.08), COL_RAW_TIMBER);
        tie_z += 1.2;
    }

    // Internal Boat Cradle
    boxr(&mut a.paint, v(-1.3, 0.14, -2.5), v(1.3, 0.35, 2.5), COL_RAW_TIMBER);
    boxr(&mut a.paint, v(-1.2, 0.35, -2.0), v(-0.8, 0.85, -1.7), wood_col);
    boxr(&mut a.paint, v(0.8, 0.35, -2.0), v(1.2, 0.85, -1.7), wood_col);
    boxr(&mut a.paint, v(-1.2, 0.35, 1.7), v(-0.8, 0.85, 2.0), wood_col);
    boxr(&mut a.paint, v(0.8, 0.35, 1.7), v(1.2, 0.85, 2.0), wood_col);

    // Workbench along West wall
    boxr(&mut a.paint, v(-half_w + 0.3, 0.0, -1.8), v(-half_w + 1.2, 0.95, 2.2), c(0.55, 0.45, 0.35));
    // Vice on workbench
    a.metal.cuboid(v(-half_w + 0.7, 1.05, 1.8), v(0.25, 0.2, 0.25), Color::srgb(0.2, 0.2, 0.22));

    // Stately Flagpole mounted on South gable apex
    a.paint.cyl(v(0.0, h + rh + 2.1, -half_d), 0.08, 4.2, Quat::IDENTITY, COL_WHITE);
    // Gold truck on top
    a.paint.blob(v(0.0, h + rh + 4.25, -half_d), v(0.2, 0.2, 0.2), Color::srgb(0.85, 0.75, 0.25));
    // Nautical signal pennant flying in the breeze
    a.paint.wedge(v(0.6, h + rh + 3.6, -half_d), v(1.2, 0.6, 0.04), Quat::from_rotation_z(FRAC_PI_2), Color::srgb(0.85, 0.15, 0.15));

    m.place(a, pos, 0.0);

    // Colliders (leaving 4.5m front opening unblocked)
    m.collide(pos + v(0.0, h / 2.0, half_d - 0.15), v(w, h, 0.3));
    m.collide(pos + v(-half_w + 0.15, h / 2.0, 0.0), v(0.3, h, d));
    m.collide(pos + v(half_w - 0.15, h / 2.0, 0.0), v(0.3, h, d));
    m.collide(pos + v((-half_w - 2.25) / 2.0, h / 2.0, -half_d + 0.15), v(half_w - 2.25, h, 0.3));
    m.collide(pos + v((half_w + 2.25) / 2.0, h / 2.0, -half_d + 0.15), v(half_w - 2.25, h, 0.3));

    // Register indoor area & interior lamp
    let min_x = pos.x - half_w;
    let max_x = pos.x + half_w;
    let min_z = pos.z - half_d;
    let max_z = pos.z + half_d;
    m.indoor_areas.push([min_x.min(max_x), min_z.min(max_z), min_x.max(max_x), min_z.max(max_z)]);
    m.floor(min_x, min_z, max_x, max_z, Floor::Boards(COL_WOOD_DECK));
    m.lamp(pos.x, pos.z, 3.8, c(1.0, 0.88, 0.65), 35_000.0);
}

// ---------------------------------------------------------------------------
// Open-Air Market Stall Pavilion
// ---------------------------------------------------------------------------
fn build_market_pavilion(m: &mut MapLayout, pos: Vec3) {
    let mut a = Art::default();
    let w = 6.2;
    let d = 5.2;
    let half_w = w / 2.0;
    let half_d = d / 2.0;
    let h = 3.6;

    // 6 Open timber columns supporting perimeter tie-beams
    for &px in &[-half_w + 0.3, 0.0, half_w - 0.3] {
        for &pz in &[-half_d + 0.3, half_d - 0.3] {
            a.paint.cyl(v(px, h / 2.0, pz), 0.16, h, Quat::IDENTITY, COL_RAW_TIMBER);
        }
    }
    // Perimeter header beams
    boxr(&mut a.paint, v(-half_w + 0.1, h - 0.22, -half_d + 0.1), v(half_w - 0.1, h, -half_d + 0.5), COL_RAW_TIMBER);
    boxr(&mut a.paint, v(-half_w + 0.1, h - 0.22, half_d - 0.5), v(half_w - 0.1, h, half_d - 0.1), COL_RAW_TIMBER);
    boxr(&mut a.paint, v(-half_w + 0.1, h - 0.22, -half_d + 0.1), v(-half_w + 0.5, h, half_d - 0.1), COL_RAW_TIMBER);
    boxr(&mut a.paint, v(half_w - 0.5, h - 0.22, -half_d + 0.1), v(half_w - 0.1, h, half_d - 0.1), COL_RAW_TIMBER);

    // Central roof tie beam
    boxr(&mut a.paint, v(0.0 - 0.1, h - 0.22, -half_d + 0.1), v(0.0 + 0.1, h, half_d - 0.1), COL_RAW_TIMBER);

    // Hip Thatch Roof with Stepped Overhang and Hanging Fringe
    let rh = 1.9;
    let slope = (rh / half_d).atan();
    let slab_len = (half_d.powi(2) + rh * rh).sqrt() + 0.7;
    for &s in &[-1.0f32, 1.0] {
        let rot = Quat::from_rotation_x(s * slope);
        let at = v(0.0, h + rh / 2.0 + 0.1, s * (half_d / 2.0));
        a.paint.cuboid_rot(at, v(w + 0.9, 0.22, slab_len), rot, COL_THATCH);
        // Hanging fringe
        let fringe_z = s * (half_d + 0.4);
        let mut fx = -half_w - 0.2;
        while fx <= half_w + 0.2 {
            boxr(&mut a.paint, v(fx - 0.08, h - 0.22, fringe_z - 0.04), v(fx + 0.08, h + 0.04, fringe_z + 0.04), COL_THATCH_DARK);
            fx += 0.26;
        }
    }
    // Ridge cap
    boxr(&mut a.paint, v(-half_w - 0.3, h + rh - 0.06, -0.25), v(half_w + 0.3, h + rh + 0.16, 0.25), shade(COL_THATCH, 0.9));

    // Turquoise Counter Fascia (U-shaped market display counter)
    boxr(&mut a.paint, v(-2.2, 0.0, -1.2), v(2.2, 0.95, -0.4), COL_TURQUOISE);
    boxr(&mut a.paint, v(-2.2, 0.0, -0.4), v(-1.4, 0.95, 1.4), COL_TURQUOISE);
    boxr(&mut a.paint, v(1.4, 0.0, -0.4), v(2.2, 0.95, 1.4), COL_TURQUOISE);
    // Smooth timber counter tops
    boxr(&mut a.paint, v(-2.3, 0.92, -1.3), v(2.3, 1.02, -0.3), c(0.85, 0.78, 0.65));
    boxr(&mut a.paint, v(-2.3, 0.92, -0.3), v(-1.3, 1.02, 1.5), c(0.85, 0.78, 0.65));
    boxr(&mut a.paint, v(1.3, 0.92, -0.3), v(2.3, 1.02, 1.5), c(0.85, 0.78, 0.65));

    // 3 Fish Display Tubs (Crushed ice, banana leaves, whole fish)
    let tub_positions = [v(-1.2, 1.05, -0.85), v(0.0, 1.05, -0.85), v(1.2, 1.05, -0.85)];
    for &tp in &tub_positions {
        // Timber round tub
        a.paint.cyl(tp, 0.42, 0.3, Quat::IDENTITY, COL_DARK_TIMBER);
        // Crushed ice bed
        a.paint.blob(tp + v(0.0, 0.12, 0.0), v(0.75, 0.1, 0.75), Color::srgb(0.85, 0.92, 0.96));
        // Layered banana leaves
        a.paint.blob(tp + v(0.08, 0.15, 0.0), v(0.65, 0.04, 0.45), Color::srgb(0.18, 0.55, 0.18));
        // Fresh catch fish
        a.paint.blob(tp + v(0.0, 0.20, 0.0), v(0.42, 0.12, 0.18), Color::srgb(0.55, 0.65, 0.75));
    }

    // Hung Trophy Fish (2.4m sailfish / giant pelagic catch hanging from tie beam)
    let fish_at = v(0.0, h - 0.7, 0.0);
    // Streamlined body
    a.paint.blob(fish_at, v(1.8, 0.4, 0.25), Color::srgb(0.25, 0.45, 0.65));
    // Bill / snout rostrum
    a.paint.cyl(fish_at + v(-1.1, 0.0, 0.0), 0.03, 0.6, Quat::from_rotation_z(FRAC_PI_2), Color::srgb(0.18, 0.25, 0.35));
    // Crescent dorsal sail / fin
    a.paint.wedge(fish_at + v(0.1, 0.35, 0.0), v(1.0, 0.5, 0.04), Quat::from_rotation_z(FRAC_PI_2), Color::srgb(0.15, 0.35, 0.55));
    // Crescent caudal tail
    a.paint.wedge(fish_at + v(1.0, 0.0, 0.0), v(0.6, 0.7, 0.04), Quat::IDENTITY, Color::srgb(0.2, 0.35, 0.5));
    // Rope tackle halters suspending fish from tie-beam
    a.paint.cyl(fish_at + v(-0.4, 0.35, 0.0), 0.02, 0.7, Quat::IDENTITY, c(0.65, 0.55, 0.35));
    a.paint.cyl(fish_at + v(0.6, 0.35, 0.0), 0.02, 0.7, Quat::IDENTITY, c(0.65, 0.55, 0.35));

    m.place(a, pos, 0.0);

    // Column Colliders (open-air structure)
    for &px in &[-half_w + 0.3, half_w - 0.3] {
        for &pz in &[-half_d + 0.3, half_d - 0.3] {
            m.collide(pos + v(px, h / 2.0, pz), v(0.35, h, 0.35));
        }
    }
    // Counter collider
    m.collide(pos + v(0.0, 0.5, -0.8), v(4.4, 1.0, 0.8));

    // Register indoor area & overhead lamps
    let min_x = pos.x - half_w;
    let max_x = pos.x + half_w;
    let min_z = pos.z - half_d;
    let max_z = pos.z + half_d;
    m.indoor_areas.push([min_x.min(max_x), min_z.min(max_z), min_x.max(max_x), min_z.max(max_z)]);
    m.lamp(pos.x, pos.z, h - 0.3, c(1.0, 0.88, 0.65), 35_000.0);
}
