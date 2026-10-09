use bevy::prelude::*;
use std::f32::consts::{FRAC_PI_2, PI, TAU};

use crate::kit::c;
use crate::maps::MapLayout;
use crate::maps::props::{boxr, shade, v, Art};

#[allow(dead_code)]
impl MapLayout {
    #[allow(dead_code)]
    pub fn pier_section(&mut self, center: Vec3, length: f32, width: f32, height: f32, piles: bool) {
        let mut a = Art::default();
        let wood = c(0.42, 0.32, 0.22);
        let dark = shade(wood, 0.6);
        let plank_w = 0.25;
        let count = (length / plank_w).floor() as usize;

        for i in 0..count {
            let px = -length / 2.0 + (i as f32) * plank_w;
            boxr(&mut a.paint, v(px + 0.02, height - 0.1, -width / 2.0), v(px + plank_w - 0.02, height, width / 2.0), wood);
        }

        boxr(&mut a.paint, v(-length / 2.0, height - 0.35, -width / 2.0 + 0.2), v(length / 2.0, height - 0.1, -width / 2.0 + 0.4), dark);
        boxr(&mut a.paint, v(-length / 2.0, height - 0.35, width / 2.0 - 0.4), v(length / 2.0, height - 0.1, width / 2.0 - 0.2), dark);
        boxr(&mut a.paint, v(-length / 2.0, height - 0.4, -0.2), v(length / 2.0, height - 0.1, 0.2), dark);

        if piles {
            for x in [-length / 2.0 + 0.5, length / 2.0 - 0.5] {
                for z in [-width / 2.0 + 0.3, width / 2.0 - 0.3] {
                    a.paint.cyl(v(x, height / 2.0, z), 0.25, height + 0.5, Quat::IDENTITY, dark);
                }
            }
        }

        for z in [-width / 2.0 + 0.1, width / 2.0 - 0.1] {
            boxr(&mut a.paint, v(-length / 2.0, height, z - 0.05), v(length / 2.0, height + 1.0, z + 0.05), wood);
        }

        self.place(a, center, 0.0);
        self.collide(center + v(0.0, height / 2.0, 0.0), v(length, height, width));
    }

    pub fn bollard(&mut self, pos: Vec3) {
        let mut a = Art::default();
        let iron = c(0.18, 0.18, 0.18);
        a.metal.cyl(v(0.0, 0.3, 0.0), 0.15, 0.6, Quat::IDENTITY, iron);
        a.metal.cyl(v(0.0, 0.45, 0.0), 0.06, 0.4, Quat::from_rotation_x(FRAC_PI_2), iron);
        a.metal.blob(v(0.0, 0.65, 0.0), v(0.2, 0.1, 0.2), iron);
        self.place(a, pos, 0.0);
        self.collide(pos + v(0.0, 0.35, 0.0), v(0.4, 0.7, 0.4));
    }

    pub fn cleat(&mut self, pos: Vec3, yaw: f32) {
        let mut a = Art::default();
        let brass = c(0.65, 0.55, 0.25);
        a.metal.cuboid(v(0.0, 0.05, 0.0), v(0.4, 0.04, 0.1), brass);
        a.metal.cyl(v(-0.1, 0.1, 0.0), 0.03, 0.1, Quat::IDENTITY, brass);
        a.metal.cyl(v(0.1, 0.1, 0.0), 0.03, 0.1, Quat::IDENTITY, brass);
        a.metal.cyl(v(0.0, 0.15, 0.0), 0.03, 0.5, Quat::from_rotation_z(FRAC_PI_2), brass);
        self.place(a, pos, yaw);
    }

    pub fn life_ring(&mut self, pos: Vec3, yaw: f32) {
        let mut a = Art::default();
        let red = c(0.85, 0.15, 0.15);
        let white = c(0.9, 0.9, 0.9);
        a.paint.torus(v(0.0, 0.0, 0.0), 0.08, 0.35, Quat::from_rotation_x(FRAC_PI_2), white);
        for i in 0..4 {
            let ang = (i as f32) * FRAC_PI_2;
            let rot = Quat::from_rotation_z(ang) * Quat::from_rotation_x(FRAC_PI_2);
            a.paint.cyl(v(0.0, 0.0, 0.0) + rot * v(0.35, 0.0, 0.0), 0.09, 0.15, rot * Quat::from_rotation_z(FRAC_PI_2), red);
        }
        self.place(a, pos, yaw);
    }

    pub fn rope_coil(&mut self, pos: Vec3, radius: f32) {
        let mut a = Art::default();
        let hemp = c(0.65, 0.55, 0.35);
        a.paint.torus(v(0.0, 0.04, 0.0), 0.04, radius, Quat::IDENTITY, hemp);
        a.paint.torus(v(0.0, 0.08, 0.0), 0.03, radius * 0.8, Quat::IDENTITY, hemp);
        a.paint.torus(v(0.0, 0.12, 0.0), 0.03, radius * 0.9, Quat::IDENTITY, hemp);
        self.place(a, pos, 0.0);
    }

    pub fn fishing_boat(&mut self, pos: Vec3, yaw: f32, hull_color: Color) {
        let mut a = Art::default();
        let wood = c(0.5, 0.35, 0.2);
        let dark = c(0.2, 0.2, 0.2);
        a.paint.wedge(v(0.0, 0.5, -2.5), v(2.4, 1.0, 1.5), Quat::from_rotation_y(FRAC_PI_2) * Quat::from_rotation_z(FRAC_PI_2), hull_color);
        boxr(&mut a.paint, v(-1.2, 0.0, -1.0), v(1.2, 1.0, 2.5), hull_color);
        
        boxr(&mut a.paint, v(-1.2, 0.95, -2.4), v(1.2, 1.05, 2.5), wood);
        
        boxr(&mut a.paint, v(-1.0, 1.0, -0.5), v(1.0, 2.5, 1.5), c(0.85, 0.85, 0.85));
        a.glass.cuboid(v(0.0, 1.8, 1.55), v(1.8, 0.8, 0.1), c(0.3, 0.5, 0.6));
        a.glass.cuboid(v(-1.05, 1.8, 0.5), v(0.1, 0.6, 1.0), c(0.3, 0.5, 0.6));
        a.glass.cuboid(v(1.05, 1.8, 0.5), v(0.1, 0.6, 1.0), c(0.3, 0.5, 0.6));
        
        boxr(&mut a.paint, v(-1.0, 0.4, -2.2), v(1.0, 0.5, -1.6), wood);
        
        a.paint.cyl(v(0.0, 2.5, 0.5), 0.06, 3.5, Quat::IDENTITY, wood);
        a.paint.cyl(v(0.0, 1.0, 2.6), 0.15, 0.8, Quat::IDENTITY, dark);
        
        self.place(a, pos, yaw);
        self.collide_local(pos, yaw, v(0.0, 1.0, 0.0), v(2.4, 2.5, 5.0));
    }

    pub fn rowboat(&mut self, pos: Vec3, yaw: f32, color: Color) {
        let mut a = Art::default();
        let wood = c(0.65, 0.45, 0.25);
        a.paint.wedge(v(0.0, 0.3, -1.2), v(1.2, 0.6, 1.2), Quat::from_rotation_y(FRAC_PI_2) * Quat::from_rotation_z(FRAC_PI_2), color);
        boxr(&mut a.paint, v(-0.6, 0.0, 0.0), v(0.6, 0.6, 1.2), color);
        
        for z in [0.2, 0.7] {
            boxr(&mut a.paint, v(-0.5, 0.4, z), v(0.5, 0.45, z+0.2), wood);
        }
        a.paint.cyl(v(-0.7, 0.5, 0.5), 0.03, 1.8, Quat::from_rotation_x(1.0) * Quat::from_rotation_z(0.3), wood);
        a.paint.cyl(v(0.7, 0.5, 0.5), 0.03, 1.8, Quat::from_rotation_x(1.0) * Quat::from_rotation_z(-0.3), wood);
        
        self.place(a, pos, yaw);
        self.collide_local(pos, yaw, v(0.0, 0.3, 0.0), v(1.2, 0.6, 2.4));
    }

    /// Upgraded fish cleaning table: heavy timber butcher table with cutting board,
    /// split fish with head/trunk, blood smear, fillet knife, whole tuna, and bucket on bottom shelf.
    pub fn fish_cleaning_table(&mut self, pos: Vec3, yaw: f32) {
        let mut a = Art::default();
        let wood = c(0.55, 0.45, 0.35);
        let dark_timber = c(0.38, 0.30, 0.22);
        let cutting_board = c(0.78, 0.70, 0.56);
        let blood = c(0.48, 0.08, 0.08);

        // 1. Heavy timber legs (4x4s with notched framing)
        for x in [-0.88f32, 0.88] {
            for z in [-0.38f32, 0.38] {
                boxr(&mut a.paint, v(x - 0.05, 0.0, z - 0.05), v(x + 0.05, 0.95, z + 0.05), dark_timber);
            }
        }

        // Horizontal stretchers connecting legs
        for x in [-0.88f32, 0.88] {
            boxr(&mut a.paint, v(x - 0.04, 0.22, -0.38), v(x + 0.04, 0.30, 0.38), dark_timber);
        }
        for z in [-0.38f32, 0.38] {
            boxr(&mut a.paint, v(-0.88, 0.22, z - 0.04), v(0.88, 0.30, z + 0.04), dark_timber);
            boxr(&mut a.paint, v(-0.88, 0.85, z - 0.04), v(0.88, 0.95, z + 0.04), dark_timber);
        }

        // 2. Slatted bottom shelf
        for i in 0..5 {
            let sz = -0.32 + (i as f32) * 0.16;
            boxr(&mut a.paint, v(-0.85, 0.28, sz - 0.05), v(0.85, 0.32, sz + 0.05), wood);
        }

        // 3. Heavy timber butcher block tabletop
        boxr(&mut a.paint, v(-1.05, 0.95, -0.48), v(1.05, 1.05, 0.48), wood);
        // Back splashback lip along rear rim
        boxr(&mut a.paint, v(-1.05, 1.05, 0.40), v(1.05, 1.22, 0.48), dark_timber);
        // Drain gutter bevel along front
        boxr(&mut a.paint, v(-1.0, 1.03, -0.46), v(1.0, 1.05, -0.42), dark_timber);

        // 4. Thick end-grain cutting board on tabletop
        boxr(&mut a.paint, v(-0.35, 1.05, -0.28), v(0.35, 1.11, 0.28), cutting_board);

        // 5. Blood smear & cut residue on cutting board
        boxr(&mut a.paint, v(-0.25, 1.111, -0.18), v(0.20, 1.113, 0.14), blood);
        boxr(&mut a.paint, v(-0.05, 1.111, -0.26), v(0.12, 1.113, -0.16), c(0.58, 0.12, 0.12));
        boxr(&mut a.paint, v(0.02, 1.051, -0.45), v(0.08, 1.053, -0.28), blood); // drip toward gutter

        // 6. Split fish lying open on cutting board
        let fish_silver = c(0.72, 0.75, 0.80);
        let fish_flesh = c(0.85, 0.26, 0.24);
        let spine_bone = c(0.92, 0.90, 0.82);
        // Fish head
        a.paint.cone(v(-0.20, 1.14, 0.0), 0.07, 0.16, Quat::from_rotation_z(FRAC_PI_2), fish_silver);
        // Open split trunk halves
        a.paint.cuboid_rot(v(0.02, 1.13, 0.07), v(0.26, 0.025, 0.09), Quat::from_rotation_x(0.2), fish_flesh);
        a.paint.cuboid_rot(v(0.02, 1.13, -0.07), v(0.26, 0.025, 0.09), Quat::from_rotation_x(-0.2), fish_flesh);
        // Exposed spine bone down the center
        a.paint.cyl(v(0.02, 1.14, 0.0), 0.012, 0.25, Quat::from_rotation_z(FRAC_PI_2), spine_bone);
        // Forked tail fin
        a.paint.wedge(v(0.18, 1.13, 0.0), v(0.08, 0.02, 0.12), Quat::IDENTITY, fish_silver);

        // 7. Fillet knife stuck into cutting board
        let steel = c(0.85, 0.88, 0.92);
        let knife_handle = c(0.22, 0.15, 0.12);
        a.metal.beam(v(0.22, 1.11, 0.12), v(0.38, 1.24, 0.18), Vec2::new(0.006, 0.035), steel);
        a.paint.cyl_between(v(0.38, 1.24, 0.18), v(0.48, 1.32, 0.23), 0.016, knife_handle);
        a.metal.cyl_between(v(0.37, 1.23, 0.17), v(0.39, 1.25, 0.19), 0.018, c(0.75, 0.65, 0.3)); // brass bolster

        // 8. Whole large tuna resting on the table next to the board
        let tuna_dark = c(0.16, 0.26, 0.42); // deep ocean blue back
        let tuna_belly = c(0.85, 0.88, 0.90);
        let fin_yellow = c(0.95, 0.82, 0.15);
        let tpos = v(-0.68, 1.14, -0.05);
        a.paint.blob_rot(tpos, v(0.42, 0.14, 0.13), Quat::from_rotation_y(0.1), tuna_dark);
        a.paint.blob_rot(tpos + v(0.0, -0.03, 0.02), v(0.38, 0.09, 0.11), Quat::from_rotation_y(0.1), tuna_belly);
        // Tuna yellow finlets and tail
        a.paint.wedge(tpos + v(0.42, 0.0, 0.0), v(0.12, 0.02, 0.18), Quat::from_rotation_y(-FRAC_PI_2), tuna_dark);
        for k in 0..3 {
            let kx = 0.22 + (k as f32) * 0.06;
            a.paint.cone(tpos + v(kx, 0.10, 0.0), 0.02, 0.05, Quat::IDENTITY, fin_yellow);
            a.paint.cone(tpos + v(kx, -0.08, 0.0), 0.02, 0.05, Quat::from_rotation_x(PI), fin_yellow);
        }

        // 9. Galvanized wash bucket on bottom shelf
        let bpos = v(0.55, 0.32, 0.0);
        a.metal.frustum(bpos + v(0.0, 0.16, 0.0), 0.18, 0.14, 0.32, Quat::IDENTITY, c(0.65, 0.68, 0.72));
        a.glass.blob(bpos + v(0.0, 0.26, 0.0), v(0.16, 0.05, 0.16), c(0.35, 0.65, 0.75)); // water
        a.metal.torus(bpos + v(0.0, 0.30, 0.0), 0.008, 0.18, Quat::from_rotation_x(0.6), c(0.4, 0.4, 0.4)); // handle

        self.place(a, pos, yaw);
        self.collide_local(pos, yaw, v(0.0, 0.6, 0.0), v(2.2, 1.2, 1.1));
    }

    /// Upgraded A-frame fish drying rack: heavy timber A-frame bents with
    /// horizontal cross-poles and rows of split stockfish hanging by their tails.
    pub fn fish_drying_rack(&mut self, pos: Vec3, yaw: f32) {
        let mut a = Art::default();
        let timber = c(0.48, 0.38, 0.28);
        let pole_wood = c(0.62, 0.52, 0.40);
        let stockfish = c(0.65, 0.52, 0.34);
        let twine = c(0.78, 0.72, 0.58);

        // Heavy A-frame timber end frames
        for x in [-1.5f32, 1.5] {
            // Crossed legs
            a.paint.beam(v(x, 0.0, -0.65), v(x, 2.05, 0.0), Vec2::new(0.08, 0.08), timber);
            a.paint.beam(v(x, 0.0, 0.65), v(x, 2.05, 0.0), Vec2::new(0.08, 0.08), timber);
            // Lower horizontal tie spreader
            a.paint.beam(v(x, 0.85, -0.5), v(x, 0.85, 0.5), Vec2::new(0.06, 0.06), timber);
            // Upper collar tie
            a.paint.beam(v(x, 1.55, -0.25), v(x, 1.55, 0.25), Vec2::new(0.06, 0.06), timber);
            // Peg pin at apex
            a.paint.cyl(v(x, 1.95, 0.0), 0.02, 0.14, Quat::from_rotation_z(FRAC_PI_2), c(0.3, 0.25, 0.2));
        }

        // Long horizontal drying poles spanning between A-frames
        a.paint.cyl(v(0.0, 1.95, 0.0), 0.045, 3.4, Quat::from_rotation_z(FRAC_PI_2), pole_wood); // Ridge pole
        a.paint.cyl(v(0.0, 1.35, -0.38), 0.038, 3.3, Quat::from_rotation_z(FRAC_PI_2), pole_wood); // Front lower pole
        a.paint.cyl(v(0.0, 1.35, 0.38), 0.038, 3.3, Quat::from_rotation_z(FRAC_PI_2), pole_wood); // Rear lower pole

        // Rows of split dried fish hanging in pairs tied at tails
        // Ridge pole row
        for i in 0..7 {
            let fx = -1.2 + (i as f32) * 0.40;
            // Twine loop
            a.paint.torus(v(fx, 1.95, 0.0), 0.008, 0.05, Quat::IDENTITY, twine);
            // Hanging split fish bodies
            a.paint.blob_rot(v(fx - 0.03, 1.62, -0.06), v(0.06, 0.28, 0.09), Quat::from_rotation_x(0.12), stockfish);
            a.paint.blob_rot(v(fx + 0.03, 1.62, 0.06), v(0.06, 0.28, 0.09), Quat::from_rotation_x(-0.12), stockfish);
        }

        // Lower pole rows
        for i in 0..5 {
            let fx = -1.0 + (i as f32) * 0.50;
            a.paint.blob(v(fx, 1.05, -0.38), v(0.07, 0.24, 0.10), stockfish);
            a.paint.blob(v(fx + 0.2, 1.05, 0.38), v(0.07, 0.24, 0.10), stockfish);
        }

        self.place(a, pos, yaw);
        self.collide_local(pos, yaw, v(0.0, 1.0, 0.0), v(3.4, 2.0, 1.4));
    }

    pub fn net_pile(&mut self, pos: Vec3, color: Color) {
        let mut a = Art::default();
        a.paint.blob(v(0.0, 0.3, 0.0), v(0.8, 0.4, 0.8), color);
        a.paint.blob(v(0.2, 0.2, 0.3), v(0.6, 0.3, 0.7), shade(color, 0.8));
        self.place(a, pos, 0.0);
    }

    pub fn lobster_trap(&mut self, pos: Vec3, yaw: f32) {
        let mut a = Art::default();
        let wood = c(0.6, 0.5, 0.35);
        let net = c(0.2, 0.2, 0.2);
        boxr(&mut a.paint, v(-0.4, 0.0, -0.3), v(0.4, 0.05, 0.3), wood);
        boxr(&mut a.paint, v(-0.4, 0.35, -0.3), v(0.4, 0.4, 0.3), wood);
        for x in [-0.35, 0.35] {
            boxr(&mut a.paint, v(x-0.05, 0.0, -0.3), v(x+0.05, 0.4, 0.3), wood);
        }
        a.paint.cyl(v(0.0, 0.2, 0.0), 0.15, 0.4, Quat::from_rotation_z(FRAC_PI_2), net);
        
        self.place(a, pos, yaw);
        self.collide_local(pos, yaw, v(0.0, 0.2, 0.0), v(0.8, 0.4, 0.6));
    }

    pub fn buoy_string(&mut self, pos: Vec3, count: usize) {
        let mut a = Art::default();
        let colors = [c(0.85, 0.15, 0.15), c(0.9, 0.85, 0.15), c(0.15, 0.35, 0.85), c(0.9, 0.9, 0.9)];
        for i in 0..count {
            let col = colors[i % colors.len()];
            a.paint.cyl(v(0.0, -0.2 - (i as f32)*0.35, 0.0), 0.1, 0.25, Quat::IDENTITY, col);
        }
        self.place(a, pos, 0.0);
    }

    pub fn fish_crate(&mut self, pos: Vec3, yaw: f32, filled: bool) {
        let mut a = Art::default();
        let wood = c(0.65, 0.55, 0.4);
        boxr(&mut a.paint, v(-0.35, 0.0, -0.25), v(0.35, 0.25, 0.25), wood);
        boxr(&mut a.paint, v(-0.3, 0.05, -0.2), v(0.3, 0.26, 0.2), c(0.1, 0.1, 0.1));
        if filled {
            boxr(&mut a.paint, v(-0.3, 0.1, -0.2), v(0.3, 0.25, 0.2), c(0.85, 0.9, 0.9)); // Ice
            a.paint.blob(v(0.0, 0.25, 0.0), v(0.2, 0.05, 0.1), c(0.6, 0.6, 0.65));
            a.paint.blob(v(-0.1, 0.25, 0.1), v(0.2, 0.05, 0.1), c(0.6, 0.6, 0.65));
        }
        self.place(a, pos, yaw);
        self.collide_local(pos, yaw, v(0.0, 0.12, 0.0), v(0.7, 0.25, 0.5));
    }

    pub fn market_stall(&mut self, pos: Vec3, yaw: f32, _name: &str, awning_color: Color) {
        let mut a = Art::default();
        let wood = c(0.45, 0.35, 0.25);
        boxr(&mut a.paint, v(-1.6, 0.0, -0.6), v(1.6, 0.9, 0.6), wood);
        
        for x in [-1.5, 1.5] {
            for z in [-0.5, 0.5] {
                a.paint.cyl(v(x, 1.5, z), 0.04, 3.0, Quat::IDENTITY, wood);
            }
        }
        boxr(&mut a.paint, v(-1.8, 2.5, -1.2), v(1.8, 2.6, 1.2), awning_color);
        boxr(&mut a.paint, v(-1.7, 2.4, 1.1), v(1.7, 2.6, 1.2), c(0.9, 0.9, 0.9)); 
        
        a.metal.cyl(v(-1.0, 2.2, 0.0), 0.02, 0.6, Quat::IDENTITY, c(0.3, 0.3, 0.3));
        a.metal.cyl(v(-1.0, 1.9, 0.0), 0.15, 0.05, Quat::IDENTITY, c(0.4, 0.4, 0.4));
        
        a.metal.cyl(v(1.0, 2.3, 0.0), 0.05, 0.2, Quat::IDENTITY, c(0.6, 0.5, 0.2));
        a.glow.blob(v(1.0, 2.1, 0.0), v(0.08, 0.1, 0.08), c(1.0, 0.85, 0.5));
        
        boxr(&mut a.paint, v(-1.0, 2.3, 0.6), v(1.0, 2.5, 0.65), wood);
        
        self.place(a, pos, yaw);
        self.collide_local(pos, yaw, v(0.0, 0.45, 0.0), v(3.2, 0.9, 1.2));
        self.collide_local(pos, yaw, v(1.5, 1.5, 0.0), v(0.2, 3.0, 0.2));
        self.collide_local(pos, yaw, v(-1.5, 1.5, 0.0), v(0.2, 3.0, 0.2));
    }

    /// Upgraded natural coconut palm: curved/leaning trunk with root flare,
    /// textured bark rings, 3-tiered arching fronds, and coconut bunches beneath the crown.
    pub fn palm_tree(&mut self, pos: Vec3, height: f32, lean: f32, yaw: f32) {
        let mut a = Art::default();
        let bark_dark = c(0.42, 0.36, 0.30);
        let bark_light = c(0.50, 0.44, 0.36);
        let root_col = c(0.38, 0.32, 0.25);
        let frond_deep = c(0.20, 0.48, 0.18);
        let frond_bright = c(0.28, 0.58, 0.22);
        let coconut_green = c(0.48, 0.58, 0.22);
        let coconut_brown = c(0.40, 0.32, 0.18);

        // 1. Root flare at trunk base (wide base tapering into main trunk with root buttresses)
        a.paint.frustum(v(0.0, 0.35, 0.0), 0.27, 0.44, 0.70, Quat::IDENTITY, root_col);
        for i in 0..5 {
            let rang = (i as f32) * TAU / 5.0;
            let rx = rang.cos() * 0.34;
            let rz = rang.sin() * 0.34;
            a.paint.blob(v(rx, 0.15, rz), v(0.22, 0.18, 0.22), root_col);
        }

        // 2. Segmented curving trunk
        let segments = 14;
        let seg_h = (height - 0.7) / (segments as f32);
        let mut cur = v(0.0, 0.7, 0.0);
        let mut cum_angle = 0.0f32;

        for i in 0..segments {
            let t = i as f32 / segments as f32;
            let step_angle = lean * 0.05 * (1.0 + t * 0.8);
            cum_angle += step_angle;
            let rot = Quat::from_rotation_z(-cum_angle);
            let dir = rot * Vec3::Y;
            let next = cur + dir * seg_h;

            let rad = 0.26 - t * 0.10;
            let col = if i % 2 == 0 { bark_dark } else { bark_light };
            a.paint.cyl((cur + next) * 0.5, rad, seg_h * 1.08, rot, col);
            // Bark ring ridge
            a.paint.torus((cur + next) * 0.5, 0.015, rad * 1.04, rot * Quat::from_rotation_x(FRAC_PI_2), bark_dark);

            cur = next;
        }

        // Crown bulb at apex of trunk
        a.paint.blob(cur, v(0.35, 0.40, 0.35), bark_light);

        // 3. Coconut bunches tightly clustered under the crown
        for b in 0..3 {
            let bang = (b as f32) * TAU / 3.0 + 0.3;
            let bpos = cur + v(bang.cos() * 0.28, -0.22, bang.sin() * 0.28);
            // 3-4 coconuts per bunch
            for c_idx in 0..3 {
                let cang = (c_idx as f32) * TAU / 3.0;
                let cpos = bpos + v(cang.cos() * 0.14, (c_idx as f32) * -0.06, cang.sin() * 0.14);
                let c_col = if c_idx % 2 == 0 { coconut_green } else { coconut_brown };
                a.paint.blob(cpos, v(0.13, 0.17, 0.13), c_col);
            }
        }

        // 4. Frond tiers (3 tiers: drooping lower, arching middle, upright crown)
        // Tier 1: Lower drooping fronds (8 fronds)
        for i in 0..8 {
            let ang = (i as f32) * TAU / 8.0 + 0.15;
            let rot_base = Quat::from_rotation_y(ang) * Quat::from_rotation_x(0.82);
            let stem_len = 3.0;
            let stem_mid = cur + rot_base * v(0.0, 0.0, stem_len * 0.5);
            let stem_tip = cur + rot_base * v(0.0, -0.4, stem_len);
            // Arching central rachis / stem
            a.paint.cyl_between(cur, stem_mid, 0.035, frond_deep);
            a.paint.cyl_between(stem_mid, stem_tip, 0.022, frond_deep);
            // Feathery leaflet blobs along rachis
            a.paint.blob_rot(stem_mid + v(0.0, -0.1, 0.0), v(0.75, 0.08, 1.8), rot_base, frond_deep);
            a.paint.blob_rot(stem_tip + v(0.0, -0.15, 0.0), v(0.55, 0.06, 1.4), rot_base, frond_deep);
        }

        // Tier 2: Middle spreading fronds (9 fronds)
        for i in 0..9 {
            let ang = (i as f32) * TAU / 9.0 + 0.45;
            let rot_base = Quat::from_rotation_y(ang) * Quat::from_rotation_x(0.48);
            let stem_len = 3.4;
            let stem_mid = cur + rot_base * v(0.0, 0.0, stem_len * 0.5);
            let stem_tip = cur + rot_base * v(0.0, -0.25, stem_len);
            a.paint.cyl_between(cur, stem_mid, 0.038, frond_bright);
            a.paint.cyl_between(stem_mid, stem_tip, 0.024, frond_bright);
            a.paint.blob_rot(stem_mid, v(0.85, 0.10, 2.0), rot_base, frond_bright);
            a.paint.blob_rot(stem_tip, v(0.60, 0.08, 1.5), rot_base, frond_bright);
        }

        // Tier 3: Upper young fronds (6 fronds reaching upward)
        for i in 0..6 {
            let ang = (i as f32) * TAU / 6.0;
            let rot_base = Quat::from_rotation_y(ang) * Quat::from_rotation_x(0.22);
            let stem_len = 2.4;
            let stem_mid = cur + rot_base * v(0.0, 0.0, stem_len * 0.5);
            let stem_tip = cur + rot_base * v(0.0, 0.0, stem_len);
            a.paint.cyl_between(cur, stem_mid, 0.030, frond_bright);
            a.paint.cyl_between(stem_mid, stem_tip, 0.020, frond_bright);
            a.paint.blob_rot(stem_mid, v(0.65, 0.08, 1.6), rot_base, frond_bright);
        }

        self.place(a, pos, yaw);
        // Collider at base and trunk
        self.collide(pos + v(0.0, 1.2, 0.0), v(0.9, 2.4, 0.9));
        self.collide(pos + v(lean * 0.5, height * 0.6, 0.0), v(0.7, height * 0.8, 0.7));
    }

    pub fn beach_driftwood(&mut self, pos: Vec3, yaw: f32) {
        let mut a = Art::default();
        let grey = c(0.65, 0.65, 0.63);
        a.paint.cyl(v(0.0, 0.2, 0.0), 0.2, 2.5, Quat::from_rotation_z(FRAC_PI_2), grey);
        a.paint.cyl(v(0.6, 0.2, 0.3), 0.12, 1.2, Quat::from_rotation_z(FRAC_PI_2) * Quat::from_rotation_y(0.6), grey);
        a.paint.cyl(v(-0.4, 0.2, -0.2), 0.08, 0.8, Quat::from_rotation_z(FRAC_PI_2) * Quat::from_rotation_y(-0.4), grey);
        self.place(a, pos, yaw);
    }

    pub fn coastal_rock(&mut self, pos: Vec3, scale: Vec3, yaw: f32) {
        let mut a = Art::default();
        let rock = c(0.45, 0.45, 0.48);
        a.paint.blob(v(0.0, scale.y/2.0, 0.0), scale, rock);
        self.place(a, pos, yaw);
        self.collide_local(pos, yaw, v(0.0, scale.y/2.0, 0.0), scale);
    }

    pub fn pier_lantern(&mut self, pos: Vec3) {
        let mut a = Art::default();
        let brass = c(0.75, 0.65, 0.35);
        a.metal.cyl(v(0.0, 0.1, 0.0), 0.1, 0.2, Quat::IDENTITY, brass);
        a.glow.blob(v(0.0, 0.3, 0.0), v(0.12, 0.18, 0.12), c(1.0, 0.85, 0.5));
        a.metal.cyl(v(0.0, 0.5, 0.0), 0.12, 0.05, Quat::IDENTITY, brass);
        
        for i in 0..4 {
            let ang = (i as f32) * FRAC_PI_2;
            let rot = Quat::from_rotation_y(ang);
            a.metal.cyl(v(0.0, 0.3, 0.0) + rot * v(0.12, 0.0, 0.0), 0.01, 0.4, Quat::IDENTITY, brass);
        }
        
        self.place(a, pos, 0.0);
        self.light(pos + v(0.0, 0.3, 0.0), c(1.0, 0.85, 0.5), 15_000.0);
    }

    /// Weathered timber pier lamp post with cantilever arm and hanging lantern (matches screenshot.jpg).
    pub fn pier_lamp_post(&mut self, pos: Vec3, yaw: f32) {
        let mut a = Art::default();
        let post_wood = c(0.85, 0.82, 0.76); // Weathered sun-bleached cream timber
        let iron = c(0.18, 0.18, 0.2);
        
        // Upright post
        boxr(&mut a.paint, v(-0.08, 0.0, -0.08), v(0.08, 3.4, 0.08), post_wood);
        // Cantilever iron bracket arm
        boxr(&mut a.metal, v(-0.02, 3.3, 0.0), v(0.02, 3.36, 1.1), iron);
        a.metal.cyl(v(0.0, 3.0, 0.5), 0.015, 0.9, Quat::from_rotation_x(0.78), iron); // diagonal strut
        // Hanging cord / chain
        a.metal.cyl(v(0.0, 3.05, 1.0), 0.012, 0.45, Quat::IDENTITY, iron);
        // Hanging cage lantern
        a.metal.cyl(v(0.0, 2.8, 1.0), 0.12, 0.06, Quat::IDENTITY, iron);
        a.glow.blob(v(0.0, 2.6, 1.0), v(0.14, 0.22, 0.14), c(1.0, 0.92, 0.65));
        a.metal.cyl(v(0.0, 2.45, 1.0), 0.12, 0.06, Quat::IDENTITY, iron);
        for i in 0..4 {
            let ang = (i as f32) * FRAC_PI_2;
            let rot = Quat::from_rotation_y(ang);
            a.metal.cyl(v(0.0, 2.62, 1.0) + rot * v(0.12, 0.0, 0.0), 0.012, 0.35, Quat::IDENTITY, iron);
        }

        self.place(a, pos, yaw);
        self.collide_local(pos, yaw, v(0.0, 1.7, 0.0), v(0.3, 3.4, 0.3));
        let fwd = Quat::from_rotation_y(yaw) * v(0.0, 2.6, 1.0);
        self.light(pos + fwd, c(1.0, 0.92, 0.68), 28_000.0);
    }

    /// Wooden pier guardrails running along Z (matches screenshot.jpg).
    pub fn pier_guardrails(&mut self, x: f32, z0: f32, z1: f32) {
        let mut a = Art::default();
        let wood = c(0.78, 0.74, 0.68); // Sun-bleached pier timber
        let (z_min, z_max) = (z0.min(z1), z0.max(z1));
        
        // Top cap rail
        boxr(&mut a.paint, v(x - 0.07, 1.02, z_min), v(x + 0.07, 1.10, z_max), wood);
        // Middle rail
        boxr(&mut a.paint, v(x - 0.04, 0.52, z_min), v(x + 0.04, 0.60, z_max), wood);
        
        // Posts every ~2.2 meters
        let mut z = z_min;
        while z <= z_max + 0.1 {
            boxr(&mut a.paint, v(x - 0.07, 0.0, z - 0.07), v(x + 0.07, 1.12, z + 0.07), wood);
            z += 2.2;
        }
        
        let len = z_max - z_min;
        self.place(a, Vec3::ZERO, 0.0);
        self.collide(v(x, 0.55, (z_min + z_max) / 2.0), v(0.25, 1.1, len));
    }

    /// Upgraded upturned wooden rowboat resting tilted on the sand with a pair of red-bladed oars leaning on the hull.
    pub fn upturned_rowboat(&mut self, pos: Vec3, yaw: f32, hull_color: Color) {
        let mut a = Art::default();
        let wood = c(0.48, 0.38, 0.28);
        let ash_oar = c(0.72, 0.62, 0.46);
        let red_blade = c(0.80, 0.18, 0.14);
        let white_stripe = c(0.92, 0.92, 0.90);

        // Inverted clinker hull resting on beach sand with realistic tilt
        let tilt = Quat::from_rotation_z(0.12) * Quat::from_rotation_x(PI);
        a.paint.wedge(v(0.0, 0.2, -1.2), v(1.3, 0.55, 1.2), tilt * Quat::from_rotation_y(FRAC_PI_2), hull_color);
        boxr(&mut a.paint, v(-0.65, 0.05, -0.6), v(0.65, 0.45, 1.2), hull_color);
        // Keel strip on top of inverted bottom
        boxr(&mut a.paint, v(-0.04, 0.45, -1.1), v(0.04, 0.52, 1.2), wood);
        // Gunwale rub rails
        boxr(&mut a.paint, v(-0.68, 0.02, -0.6), v(-0.62, 0.10, 1.25), wood);
        boxr(&mut a.paint, v(0.62, 0.02, -0.6), v(0.68, 0.10, 1.25), wood);

        // Pair of red-bladed oars leaning on the hull
        for (i, &ox) in [(-0.52f32), (-0.38f32)].iter().enumerate() {
            let oz = 0.4 + (i as f32) * 0.25;
            let oar_rot = Quat::from_rotation_y(0.25) * Quat::from_rotation_z(-0.55);
            // Wooden shaft
            a.paint.cyl_between(v(ox - 0.45, 0.0, oz - 0.2), v(ox + 0.15, 0.85, oz + 0.3), 0.022, ash_oar);
            // Handle grip
            a.paint.cyl(v(ox + 0.18, 0.88, oz + 0.32), 0.018, 0.12, oar_rot, c(0.35, 0.28, 0.20));
            // Spoon blade with red painted tip and white chevron
            let bpos = v(ox - 0.35, 0.12, oz - 0.15);
            a.paint.cuboid_rot(bpos, v(0.14, 0.015, 0.42), oar_rot, red_blade);
            a.paint.cuboid_rot(bpos - v(0.08, 0.0, 0.08), v(0.14, 0.016, 0.08), oar_rot, white_stripe);
        }

        self.place(a, pos, yaw);
        self.collide_local(pos, yaw, v(0.0, 0.25, 0.0), v(1.5, 0.6, 2.6));
    }

    /// Yard rowboat under repair resting on wooden sawhorse trestles.
    pub fn trestle_rowboat(&mut self, pos: Vec3, yaw: f32, hull_color: Color) {
        let mut a = Art::default();
        let wood = c(0.55, 0.45, 0.32);
        let dark_wood = c(0.42, 0.32, 0.22);
        let trestle_wood = c(0.68, 0.60, 0.48);

        // 1. Two sawhorse trestles at z = -0.7 and z = 0.7
        for tz in [-0.7f32, 0.7] {
            // Horizontal top crossbeam
            boxr(&mut a.paint, v(-0.75, 0.68, tz - 0.06), v(0.75, 0.76, tz + 0.06), trestle_wood);
            // Splayed A-frame legs
            for sx in [-0.62f32, 0.62] {
                a.paint.beam(v(sx, 0.0, tz - 0.32), v(sx, 0.70, tz - 0.04), Vec2::new(0.06, 0.06), trestle_wood);
                a.paint.beam(v(sx, 0.0, tz + 0.32), v(sx, 0.70, tz + 0.04), Vec2::new(0.06, 0.06), trestle_wood);
                // Lower horizontal leg spreader
                a.paint.beam(v(sx, 0.25, tz - 0.24), v(sx, 0.25, tz + 0.24), Vec2::new(0.04, 0.04), trestle_wood);
            }
        }

        // 2. Upright rowboat resting atop the trestles (keel at Y = 0.76m)
        let by = 0.76;
        // Bow wedge and main hull
        a.paint.wedge(v(0.0, by + 0.32, -1.3), v(1.3, 0.65, 1.3), Quat::from_rotation_y(FRAC_PI_2) * Quat::from_rotation_z(FRAC_PI_2), hull_color);
        boxr(&mut a.paint, v(-0.65, by, -0.65), v(0.65, by + 0.65, 1.25), hull_color);
        // Interior hollow cutout
        boxr(&mut a.paint, v(-0.54, by + 0.08, -0.6), v(0.54, by + 0.66, 1.15), dark_wood);

        // Keel and stem post
        boxr(&mut a.paint, v(-0.04, by - 0.06, -1.3), v(0.04, by + 0.04, 1.25), dark_wood);
        // Gunwale cap rails
        boxr(&mut a.paint, v(-0.68, by + 0.60, -0.65), v(-0.58, by + 0.68, 1.25), wood);
        boxr(&mut a.paint, v(0.58, by + 0.60, -0.65), v(0.68, by + 0.68, 1.25), wood);

        // 3 thwarts (seats)
        boxr(&mut a.paint, v(-0.52, by + 0.42, -0.3), v(0.52, by + 0.46, -0.05), wood);
        boxr(&mut a.paint, v(-0.52, by + 0.42, 0.35), v(0.52, by + 0.46, 0.60), wood);
        boxr(&mut a.paint, v(-0.48, by + 0.45, 0.95), v(0.48, by + 0.49, 1.20), wood);

        // Interior rib timbers
        for rz in [-0.4, 0.15, 0.7] {
            a.paint.cyl(v(-0.52, by + 0.35, rz), 0.02, 0.5, Quat::IDENTITY, dark_wood);
            a.paint.cyl(v(0.52, by + 0.35, rz), 0.02, 0.5, Quat::IDENTITY, dark_wood);
        }

        self.place(a, pos, yaw);
        self.collide_local(pos, yaw, v(0.0, 0.75, 0.0), v(1.5, 1.5, 3.2));
    }

    /// Beach fire pit: ring of 9 basalt stones, charred embers, glowing red fire heart,
    /// and 2 sturdy driftwood log benches flanking the pit.
    pub fn beach_fire_pit(&mut self, pos: Vec3, yaw: f32) {
        let mut a = Art::default();
        let basalt_dark = c(0.20, 0.20, 0.22);
        let basalt_grey = c(0.28, 0.27, 0.28);
        let ash = c(0.12, 0.12, 0.13);
        let charred_wood = c(0.16, 0.15, 0.14);
        let driftwood = c(0.72, 0.68, 0.62);
        let riser_stone = c(0.35, 0.34, 0.34);

        // 1. Central ash bed & charred firewood
        a.paint.blob(v(0.0, 0.06, 0.0), v(0.75, 0.10, 0.75), ash);
        // Criss-crossed charred firewood logs
        a.paint.cyl(v(0.0, 0.14, 0.0), 0.07, 0.85, Quat::from_rotation_y(0.4) * Quat::from_rotation_z(FRAC_PI_2), charred_wood);
        a.paint.cyl(v(0.0, 0.15, 0.0), 0.065, 0.80, Quat::from_rotation_y(-0.6) * Quat::from_rotation_z(FRAC_PI_2), charred_wood);
        a.paint.cyl(v(0.0, 0.20, 0.0), 0.055, 0.70, Quat::from_rotation_y(1.2) * Quat::from_rotation_z(FRAC_PI_2), charred_wood);

        // Glowing hot embers at the center
        a.glow.blob(v(0.0, 0.12, 0.0), v(0.26, 0.12, 0.26), c(1.0, 0.38, 0.08));
        a.glow.blob(v(0.05, 0.15, -0.04), v(0.16, 0.08, 0.16), c(1.0, 0.70, 0.20));

        // 2. Ring of exactly 9 basalt stones around the fire
        let ring_radius = 0.88;
        for i in 0..9 {
            let ang = (i as f32) * TAU / 9.0;
            let sx = ang.cos() * ring_radius;
            let sz = ang.sin() * ring_radius;
            let col = if i % 2 == 0 { basalt_dark } else { basalt_grey };
            let rad_y = 0.18 + ((i * 3) as f32 * 0.1).sin().abs() * 0.08;
            let rad_xz = 0.22 + ((i * 5) as f32 * 0.1).cos().abs() * 0.08;
            a.paint.blob_rot(
                v(sx, rad_y * 0.8, sz),
                v(rad_xz, rad_y, rad_xz),
                Quat::from_rotation_y(ang + 0.4),
                col,
            );
        }

        // 3. Two sturdy driftwood log benches flanking the fire pit at x = -1.65 and x = +1.65
        for bx in [-1.65f32, 1.65] {
            // Log support risers at ends
            for bz in [-0.75f32, 0.75] {
                a.paint.blob(v(bx, 0.12, bz), v(0.24, 0.20, 0.28), riser_stone);
            }
            // Main peeled driftwood log
            a.paint.cyl(v(bx, 0.32, 0.0), 0.16, 2.1, Quat::from_rotation_x(FRAC_PI_2), driftwood);
            // Flattened seat hewn into top of log
            boxr(&mut a.paint, v(bx - 0.12, 0.34, -0.9), v(bx + 0.12, 0.38, 0.9), c(0.78, 0.74, 0.68));
        }

        self.place(a, pos, yaw);
        // Emissive fire pit light
        self.light(pos + v(0.0, 0.35, 0.0), c(1.0, 0.55, 0.18), 22_000.0);
        // Colliders
        self.collide_local(pos, yaw, v(0.0, 0.2, 0.0), v(1.8, 0.45, 1.8));
        self.collide_local(pos, yaw, v(-1.65, 0.28, 0.0), v(0.5, 0.55, 2.1));
        self.collide_local(pos, yaw, v(1.65, 0.28, 0.0), v(0.5, 0.55, 2.1));
    }

    /// Shoreline shipwreck: half-buried curved keel and 11 exposed rib timbers jutting out of the sand.
    pub fn shipwreck(&mut self, pos: Vec3, yaw: f32) {
        let mut a = Art::default();
        let rotted_oak = c(0.34, 0.28, 0.22);
        let weathered_grey = c(0.48, 0.44, 0.38);
        let plank_wood = c(0.40, 0.34, 0.26);
        let iron_rust = c(0.35, 0.22, 0.16);

        // 1. Half-buried curved keel beam running along Z (-4.5 to +4.5m) with listing tilt
        let tilt = Quat::from_rotation_z(0.22);
        for i in 0..12 {
            let z0 = -4.5 + (i as f32) * 0.75;
            let z1 = z0 + 0.75;
            let y0 = 0.05 + ((i as f32) * 0.25).sin() * 0.25;
            let y1 = 0.05 + (((i + 1) as f32) * 0.25).sin() * 0.25;
            a.paint.beam(tilt * v(0.0, y0, z0), tilt * v(0.0, y1, z1), Vec2::new(0.22, 0.30), rotted_oak);
        }

        // 2. 11 exposed rib timbers jutting out of the sand
        for i in 0..11 {
            let rz = -4.0 + (i as f32) * 0.80;
            let rib_h = 1.4 + ((i as f32 * 0.3).sin() * 0.7).abs();

            // Port ribs jutting high into the air
            let port_base = tilt * v(-0.1, 0.1, rz);
            let port_mid = tilt * v(-0.85, rib_h * 0.6, rz);
            let port_tip = tilt * v(-1.25, rib_h, rz);
            a.paint.cyl_between(port_base, port_mid, 0.07, rotted_oak);
            a.paint.cyl_between(port_mid, port_tip, 0.055, weathered_grey);
            // Splintered ragged broken tip
            a.paint.cone(port_tip, 0.05, 0.18, Quat::from_rotation_z(0.3), weathered_grey);
            // Iron fastener spike
            a.metal.blob(port_base + v(0.0, 0.05, 0.0), v(0.03, 0.03, 0.03), iron_rust);

            // Starboard ribs (broken shorter, half-submerged in sand)
            let stbd_base = tilt * v(0.1, 0.05, rz);
            let stbd_tip = tilt * v(0.65, rib_h * 0.45, rz);
            a.paint.cyl_between(stbd_base, stbd_tip, 0.065, rotted_oak);
        }

        // 3. Splintered hull planks clinging to the outer ribs
        let plank_runs = [
            (-3.8, -0.6, 0.6, 1.8),
            (0.2, 3.4, 0.8, 1.6),
            (-2.5, 1.5, 1.1, 2.2),
        ];
        for &(z_start, z_end, y_lvl, _w) in &plank_runs {
            for step in 0..6 {
                let pz0 = z_start + (step as f32) * (z_end - z_start) / 6.0;
                let pz1 = pz0 + (z_end - z_start) / 6.0;
                a.paint.beam(
                    tilt * v(-0.95, y_lvl, pz0),
                    tilt * v(-1.05, y_lvl + 0.05, pz1),
                    Vec2::new(0.03, 0.16),
                    plank_wood,
                );
            }
        }

        // Half-buried ballast stones in bilge
        for j in 0..6 {
            let bz = -3.2 + (j as f32) * 1.1;
            a.paint.blob(tilt * v(0.2, 0.1, bz), v(0.25, 0.16, 0.25), c(0.3, 0.3, 0.32));
        }

        self.place(a, pos, yaw);
        self.collide_local(pos, yaw, v(-0.5, 0.9, 0.0), v(2.2, 1.8, 8.5));
    }

    /// Pier entrance arch: 2 sturdy piles, cream beam at h=2.75m, diagonal knee braces,
    /// hanging teal signboard with coral fish silhouette on iron chains, dual brass hanging cage lanterns.
    pub fn pier_entrance_arch(&mut self, pos: Vec3, yaw: f32) {
        let mut a = Art::default();
        let pile_timber = c(0.38, 0.32, 0.24);
        let cream_beam = c(0.92, 0.89, 0.80);
        let teal_sign = c(0.16, 0.52, 0.52);
        let coral_fish = c(0.92, 0.45, 0.35);
        let iron = c(0.20, 0.20, 0.22);
        let brass = c(0.78, 0.68, 0.35);

        let span = 3.6f32;
        let beam_h = 2.75f32;

        // 1. Two sturdy wooden round piles
        for sx in [-span * 0.5, span * 0.5] {
            a.paint.cyl(v(sx, 1.75, 0.0), 0.18, 3.5, Quat::IDENTITY, pile_timber);
            // Metal pile cap
            a.metal.cone(v(sx, 3.52, 0.0), 0.19, 0.08, Quat::IDENTITY, iron);
            // Pile rope wraps
            a.paint.torus(v(sx, 2.4, 0.0), 0.02, 0.19, Quat::IDENTITY, c(0.68, 0.58, 0.42));
        }

        // 2. Cream painted timber beam spanning across at h = 2.75m
        boxr(&mut a.paint, v(-span * 0.5 - 0.35, beam_h - 0.14, -0.12), v(span * 0.5 + 0.35, beam_h + 0.14, 0.12), cream_beam);
        // Beveled beam ends
        a.paint.wedge(v(-span * 0.5 - 0.35, beam_h, 0.0), v(0.24, 0.28, 0.24), Quat::from_rotation_z(FRAC_PI_2), cream_beam);
        a.paint.wedge(v(span * 0.5 + 0.35, beam_h, 0.0), v(0.24, 0.28, 0.24), Quat::from_rotation_z(-FRAC_PI_2), cream_beam);

        // 3. Diagonal knee braces reinforcing beam-to-pile joints
        a.paint.beam(v(-span * 0.5 + 0.14, beam_h - 0.7, 0.0), v(-span * 0.5 + 0.7, beam_h - 0.12, 0.0), Vec2::new(0.10, 0.10), cream_beam);
        a.paint.beam(v(span * 0.5 - 0.14, beam_h - 0.7, 0.0), v(span * 0.5 - 0.7, beam_h - 0.12, 0.0), Vec2::new(0.10, 0.10), cream_beam);

        // 4. Hanging teal signboard on iron chains
        let sign_y = beam_h - 0.52;
        // Two iron chain drops
        for cx in [-0.75f32, 0.75] {
            a.metal.cyl(v(cx, beam_h - 0.22, 0.0), 0.012, 0.30, Quat::IDENTITY, iron);
            a.metal.torus(v(cx, beam_h - 0.12, 0.0), 0.01, 0.03, Quat::from_rotation_x(FRAC_PI_2), iron);
        }
        // Teal wooden signboard
        boxr(&mut a.paint, v(-1.0, sign_y - 0.26, -0.04), v(1.0, sign_y + 0.26, 0.04), teal_sign);
        // Signboard trim border
        boxr(&mut a.paint, v(-1.02, sign_y + 0.24, -0.05), v(1.02, sign_y + 0.28, 0.05), cream_beam);
        boxr(&mut a.paint, v(-1.02, sign_y - 0.28, -0.05), v(1.02, sign_y - 0.24, 0.05), cream_beam);
        // Coral painted fish silhouette relief on both faces
        for sz in [-0.045f32, 0.045] {
            a.paint.blob(v(0.0, sign_y, sz), v(0.42, 0.14, 0.02), coral_fish);
            a.paint.wedge(v(0.38, sign_y, sz), v(0.14, 0.18, 0.02), Quat::from_rotation_y(FRAC_PI_2), coral_fish);
            a.paint.wedge(v(-0.05, sign_y + 0.10, sz), v(0.16, 0.08, 0.02), Quat::IDENTITY, coral_fish);
        }

        // 5. Dual brass hanging cage lanterns
        for lx in [-span * 0.5 - 0.28, span * 0.5 + 0.28] {
            let ly = beam_h - 0.35;
            a.metal.beam(v(lx * 0.95, beam_h, 0.0), v(lx, ly + 0.25, 0.0), Vec2::new(0.02, 0.03), iron);
            a.metal.cyl(v(lx, ly + 0.15, 0.0), 0.10, 0.04, Quat::IDENTITY, brass);
            a.glow.blob(v(lx, ly, 0.0), v(0.12, 0.18, 0.12), c(1.0, 0.88, 0.60));
            a.metal.cyl(v(lx, ly - 0.12, 0.0), 0.10, 0.04, Quat::IDENTITY, brass);
            for k in 0..4 {
                let kang = (k as f32) * FRAC_PI_2;
                let krot = Quat::from_rotation_y(kang);
                a.metal.cyl(v(lx, ly, 0.0) + krot * v(0.10, 0.0, 0.0), 0.01, 0.28, Quat::IDENTITY, brass);
            }
        }

        self.place(a, pos, yaw);
        self.light(pos + v(-span * 0.5 - 0.28, beam_h - 0.35, 0.0), c(1.0, 0.88, 0.65), 24_000.0);
        self.light(pos + v(span * 0.5 + 0.28, beam_h - 0.35, 0.0), c(1.0, 0.88, 0.65), 24_000.0);
        self.collide_local(pos, yaw, v(-span * 0.5, 1.75, 0.0), v(0.35, 3.5, 0.35));
        self.collide_local(pos, yaw, v(span * 0.5, 1.75, 0.0), v(0.35, 3.5, 0.35));
    }

    /// Pier rubber tyre fender hung along waterline against berthing pile.
    pub fn tyre_fender(&mut self, pos: Vec3, yaw: f32) {
        let mut a = Art::default();
        let rubber = c(0.14, 0.14, 0.15);
        let rope = c(0.68, 0.60, 0.45);
        let bolt = c(0.40, 0.40, 0.42);

        a.paint.torus(v(0.0, 0.0, 0.0), 0.11, 0.32, Quat::from_rotation_y(FRAC_PI_2), rubber);
        a.paint.torus(v(0.0, 0.0, 0.0), 0.06, 0.24, Quat::from_rotation_y(FRAC_PI_2), c(0.08, 0.08, 0.09));
        a.paint.cyl(v(0.0, 0.35, -0.05), 0.018, 0.50, Quat::IDENTITY, rope);
        a.metal.cyl(v(0.0, 0.60, -0.08), 0.025, 0.12, Quat::from_rotation_x(FRAC_PI_2), bolt);

        self.place(a, pos, yaw);
    }

    /// Pier galvanized steel swim ladder with arched grab handles.
    pub fn pier_ladder(&mut self, pos: Vec3, yaw: f32, height: f32) {
        let mut a = Art::default();
        let steel = c(0.74, 0.77, 0.80);
        let width = 0.52f32;

        for sx in [-width * 0.5, width * 0.5] {
            a.metal.cyl(v(sx, height * 0.5, 0.0), 0.022, height, Quat::IDENTITY, steel);
            a.metal.torus(v(sx, height + 0.25, -0.15), 0.022, 0.18, Quat::from_rotation_z(FRAC_PI_2), steel);
            for k in 0..3 {
                let ky = 0.4 + (k as f32) * (height * 0.4);
                a.metal.cyl(v(sx, ky, -0.08), 0.015, 0.16, Quat::from_rotation_x(FRAC_PI_2), steel);
            }
        }

        let mut ry = 0.25f32;
        while ry < height {
            a.metal.cyl(v(0.0, ry, 0.0), 0.016, width - 0.04, Quat::from_rotation_z(FRAC_PI_2), steel);
            ry += 0.30;
        }

        self.place(a, pos, yaw);
        self.collide_local(pos, yaw, v(0.0, height * 0.5, 0.0), v(0.65, height, 0.25));
    }

    /// Leaning fishing rod with spinning reel and line guides.
    pub fn fishing_rod(&mut self, pos: Vec3, yaw: f32, pitch: f32) {
        let mut a = Art::default();
        let blank_col = c(0.18, 0.22, 0.26);
        let cork = c(0.78, 0.68, 0.50);
        let reel_metal = c(0.65, 0.68, 0.72);
        let gold_spool = c(0.85, 0.72, 0.28);

        let rot = Quat::from_rotation_x(pitch);
        let length = 2.4f32;

        a.paint.cyl(rot * v(0.0, 0.25, 0.0), 0.018, 0.50, rot, cork);
        a.paint.capsule_tapered(rot * v(0.0, 0.50, 0.0), rot * v(0.0, length, 0.0), 0.014, 0.004, blank_col);

        let rpos = rot * v(0.0, 0.42, 0.05);
        a.metal.cuboid(rpos, v(0.02, 0.05, 0.04), reel_metal);
        a.metal.cyl(rpos + v(0.0, -0.04, 0.04), 0.035, 0.06, rot * Quat::from_rotation_z(FRAC_PI_2), reel_metal);
        a.metal.cyl(rpos + v(0.0, -0.04, 0.08), 0.032, 0.04, rot * Quat::from_rotation_z(FRAC_PI_2), gold_spool);
        a.metal.torus(rpos + v(0.0, -0.04, 0.08), 0.004, 0.04, rot * Quat::from_rotation_z(FRAC_PI_2), reel_metal);

        for k in 1..5 {
            let gy = 0.5 + (k as f32) * 0.42;
            let gpos = rot * v(0.0, gy, 0.015);
            a.metal.torus(gpos, 0.003, 0.015 - (k as f32) * 0.002, rot * Quat::from_rotation_x(FRAC_PI_2), reel_metal);
        }

        self.place(a, pos, yaw);
    }

    /// Village laundry line: dual timber poles with catenary sagging rope
    /// and colorful tropical clothes swaying in the sea breeze.
    pub fn laundry_line(&mut self, pos: Vec3, yaw: f32, length: f32) {
        let mut a = Art::default();
        let pole_wood = c(0.60, 0.52, 0.42);
        let rope_hemp = c(0.72, 0.65, 0.48);
        let pin_wood = c(0.82, 0.74, 0.55);

        let colors = [
            c(0.20, 0.65, 0.72), // Turquoise
            c(0.92, 0.48, 0.38), // Coral
            c(0.95, 0.82, 0.25), // Tropical yellow
            c(0.94, 0.94, 0.92), // Crisp white
            c(0.25, 0.42, 0.75), // Sea blue
            c(0.88, 0.35, 0.42), // Hibiscus red
        ];

        for sx in [-length * 0.5, length * 0.5] {
            a.paint.cyl(v(sx, 1.25, 0.0), 0.065, 2.5, Quat::IDENTITY, pole_wood);
            boxr(&mut a.paint, v(sx - 0.05, 2.42, -0.35), v(sx + 0.05, 2.50, 0.35), pole_wood);
            a.paint.beam(v(sx, 1.8, 0.0), v(sx + if sx < 0.0 { -0.7 } else { 0.7 }, 0.0, 0.0), Vec2::new(0.05, 0.05), pole_wood);
        }

        let segments = 12;
        let seg_len = length / (segments as f32);
        for i in 0..segments {
            let x0 = -length * 0.5 + (i as f32) * seg_len;
            let x1 = x0 + seg_len;
            let sag = 0.28;
            let t0 = (i as f32) / (segments as f32) * 2.0 - 1.0;
            let t1 = ((i + 1) as f32) / (segments as f32) * 2.0 - 1.0;
            let y0 = 2.45 - (1.0 - t0 * t0) * sag;
            let y1 = 2.45 - (1.0 - t1 * t1) * sag;
            a.paint.cyl_between(v(x0, y0, 0.0), v(x1, y1, 0.0), 0.012, rope_hemp);
        }

        let num_items = (length / 0.85).floor() as usize;
        for i in 0..num_items {
            let t = (i as f32 + 0.5) / (num_items as f32);
            let cx = -length * 0.5 + 0.6 + t * (length - 1.2);
            let t_norm = t * 2.0 - 1.0;
            let cy = 2.45 - (1.0 - t_norm * t_norm) * 0.28;
            let col = colors[i % colors.len()];

            for px in [-0.12f32, 0.12] {
                a.paint.cyl(v(cx + px, cy + 0.02, 0.0), 0.008, 0.06, Quat::IDENTITY, pin_wood);
            }

            let flutter = Quat::from_rotation_x(0.18 + ((i as f32 * 1.5).sin()) * 0.10);
            let w = 0.38 + ((i * 3) as f32 * 0.1).sin().abs() * 0.15;
            let h = 0.45 + ((i * 2) as f32 * 0.1).cos().abs() * 0.25;
            a.paint.cuboid_rot(v(cx, cy - h * 0.5, 0.0), v(w, h, 0.015), flutter, col);
        }

        self.place(a, pos, yaw);
        self.collide_local(pos, yaw, v(-length * 0.5, 1.25, 0.0), v(0.2, 2.5, 0.2));
        self.collide_local(pos, yaw, v(length * 0.5, 1.25, 0.0), v(0.2, 2.5, 0.2));
    }

    /// Elevated water cistern tank: 4-legged timber tower with corrugated galvanized tank and pipework.
    pub fn elevated_water_tank(&mut self, pos: Vec3, yaw: f32) {
        let mut a = Art::default();
        let heavy_timber = c(0.44, 0.35, 0.25);
        let platform_wood = c(0.55, 0.46, 0.35);
        let tank_steel = c(0.70, 0.74, 0.78);
        let steel_band = c(0.38, 0.40, 0.42);
        let pipe_steel = c(0.48, 0.50, 0.54);
        let valve_red = c(0.85, 0.18, 0.15);

        let tower_h = 4.8f32;
        let base_w = 3.2f32;
        let top_w = 2.5f32;

        let corners = [
            (-1.0f32, -1.0f32),
            (-1.0, 1.0),
            (1.0, -1.0),
            (1.0, 1.0),
        ];
        for &(cx, cz) in &corners {
            let bpos = v(cx * base_w * 0.5, 0.0, cz * base_w * 0.5);
            let tpos = v(cx * top_w * 0.5, tower_h, cz * top_w * 0.5);
            a.paint.cyl_between(bpos, tpos, 0.12, heavy_timber);
            boxr(&mut a.paint, bpos - v(0.2, 0.0, 0.2), bpos + v(0.2, 0.35, 0.2), c(0.65, 0.65, 0.62));
        }

        for tier in 0..3 {
            let y0 = 0.35 + (tier as f32) * (tower_h - 0.35) / 3.0;
            let y1 = 0.35 + ((tier + 1) as f32) * (tower_h - 0.35) / 3.0;
            let t_mid = y1 / tower_h;
            let w_mid = base_w + (top_w - base_w) * t_mid;

            for &(gx, gz) in &[(-1.0f32, 0.0f32), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
                if gx != 0.0 {
                    a.paint.beam(v(gx * w_mid * 0.5, y1, -w_mid * 0.5), v(gx * w_mid * 0.5, y1, w_mid * 0.5), Vec2::new(0.08, 0.08), heavy_timber);
                } else {
                    a.paint.beam(v(-w_mid * 0.5, y1, gz * w_mid * 0.5), v(w_mid * 0.5, y1, gz * w_mid * 0.5), Vec2::new(0.08, 0.08), heavy_timber);
                }
            }

            let w0 = base_w + (top_w - base_w) * (y0 / tower_h);
            let w1 = base_w + (top_w - base_w) * (y1 / tower_h);
            for &(fx, fz) in &[(-1.0f32, 0.0f32), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
                if fx != 0.0 {
                    a.paint.beam(v(fx * w0 * 0.5, y0, -w0 * 0.5), v(fx * w1 * 0.5, y1, w1 * 0.5), Vec2::new(0.05, 0.05), heavy_timber);
                    a.paint.beam(v(fx * w0 * 0.5, y0, w0 * 0.5), v(fx * w1 * 0.5, y1, -w1 * 0.5), Vec2::new(0.05, 0.05), heavy_timber);
                } else {
                    a.paint.beam(v(-w0 * 0.5, y0, fz * w0 * 0.5), v(w1 * 0.5, y1, fz * w1 * 0.5), Vec2::new(0.05, 0.05), heavy_timber);
                    a.paint.beam(v(w0 * 0.5, y0, fz * w0 * 0.5), v(-w1 * 0.5, y1, fz * w1 * 0.5), Vec2::new(0.05, 0.05), heavy_timber);
                }
            }
        }

        let deck_w = top_w + 0.6;
        boxr(&mut a.paint, v(-deck_w * 0.5, tower_h, -deck_w * 0.5), v(deck_w * 0.5, tower_h + 0.15, deck_w * 0.5), platform_wood);
        for &(rx, rz) in &[(-1.0f32, 0.0f32), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
            if rx != 0.0 {
                boxr(&mut a.paint, v(rx * deck_w * 0.5 - 0.04, tower_h + 0.85, -deck_w * 0.5), v(rx * deck_w * 0.5 + 0.04, tower_h + 0.95, deck_w * 0.5), platform_wood);
            } else {
                boxr(&mut a.paint, v(-deck_w * 0.5, tower_h + 0.85, rz * deck_w * 0.5 - 0.04), v(deck_w * 0.5, tower_h + 0.95, rz * deck_w * 0.5 + 0.04), platform_wood);
            }
        }

        let tank_r = 1.25f32;
        let tank_h = 1.9f32;
        let tank_base_y = tower_h + 0.15;
        let tank_mid_y = tank_base_y + tank_h * 0.5;
        a.metal.cyl(v(0.0, tank_mid_y, 0.0), tank_r, tank_h, Quat::IDENTITY, tank_steel);
        for k in 0..4 {
            let by = tank_base_y + 0.2 + (k as f32) * 0.48;
            a.metal.torus(v(0.0, by, 0.0), 0.02, tank_r + 0.01, Quat::IDENTITY, steel_band);
        }
        a.metal.cone(v(0.0, tank_base_y + tank_h + 0.35, 0.0), tank_r + 0.08, 0.70, Quat::IDENTITY, tank_steel);
        a.metal.cyl(v(0.4, tank_base_y + tank_h + 0.55, 0.0), 0.15, 0.25, Quat::IDENTITY, steel_band);

        let px = top_w * 0.5 - 0.15;
        let pz = top_w * 0.5 - 0.15;
        a.metal.cyl(v(px, (tower_h + 0.15) * 0.5, pz), 0.05, tower_h + 0.15, Quat::IDENTITY, pipe_steel);
        a.paint.torus(v(px + 0.08, 1.2, pz), 0.014, 0.11, Quat::from_rotation_z(FRAC_PI_2), valve_red);
        a.metal.cyl(v(px + 0.04, 1.2, pz), 0.015, 0.08, Quat::from_rotation_z(FRAC_PI_2), pipe_steel);

        self.place(a, pos, yaw);
        self.collide_local(pos, yaw, v(0.0, tower_h * 0.5, 0.0), v(base_w, tower_h, base_w));
        self.collide_local(pos, yaw, v(0.0, tower_h + tank_h * 0.5, 0.0), v(tank_r * 2.2, tank_h + 0.7, tank_r * 2.2));
    }

    /// Village T-post net rack with colorful draped mesh and fishing floats.
    pub fn net_drying_rack(&mut self, pos: Vec3, yaw: f32, length: f32) {
        let mut a = Art::default();
        let timber = c(0.48, 0.38, 0.28);
        let pole_wood = c(0.60, 0.50, 0.38);
        let net_teal = c(0.20, 0.52, 0.48);
        let net_tan = c(0.62, 0.55, 0.42);
        let cork = c(0.68, 0.45, 0.24);

        for sx in [-length * 0.5, length * 0.5] {
            a.paint.cyl(v(sx, 1.15, 0.0), 0.08, 2.3, Quat::IDENTITY, timber);
            boxr(&mut a.paint, v(sx - 0.07, 2.22, -0.65), v(sx + 0.07, 2.34, 0.65), timber);
            a.paint.beam(v(sx, 1.7, -0.1), v(sx, 2.22, -0.45), Vec2::new(0.06, 0.06), timber);
            a.paint.beam(v(sx, 1.7, 0.1), v(sx, 2.22, 0.45), Vec2::new(0.06, 0.06), timber);
        }

        for rz in [-0.55f32, 0.0, 0.55] {
            a.paint.cyl(v(0.0, 2.28, rz), 0.04, length + 0.3, Quat::from_rotation_z(FRAC_PI_2), pole_wood);
        }

        for seg in 0..6 {
            let sx = -length * 0.5 + 0.3 + (seg as f32) * (length - 0.6) / 6.0;
            let col = if seg % 2 == 0 { net_teal } else { net_tan };
            a.paint.blob_rot(v(sx + 0.3, 1.45, -0.55), v(0.42, 0.75, 0.08), Quat::from_rotation_y(0.05), col);
            a.paint.cyl(v(sx + 0.2, 2.32, -0.55), 0.035, 0.10, Quat::from_rotation_z(FRAC_PI_2), cork);
        }

        for seg in 0..5 {
            let sx = -length * 0.5 + 0.5 + (seg as f32) * (length - 1.0) / 5.0;
            a.paint.blob(v(sx + 0.3, 1.50, 0.55), v(0.48, 0.70, 0.08), net_tan);
        }

        self.place(a, pos, yaw);
        self.collide_local(pos, yaw, v(0.0, 1.15, 0.0), v(length + 0.4, 2.3, 1.4));
    }

    /// Slat-backed wooden bench with contoured cast-iron arms and wooden slats.
    pub fn park_bench(&mut self, pos: Vec3, yaw: f32, color: Color) {
        let mut a = Art::default();
        let iron = c(0.18, 0.20, 0.20);
        let width = 1.8f32;

        for sx in [-width * 0.5 + 0.08, width * 0.5 - 0.08] {
            a.metal.beam(v(sx, 0.0, -0.24), v(sx, 0.45, -0.20), Vec2::splat(0.045), iron);
            a.metal.beam(v(sx, 0.0, 0.20), v(sx, 0.45, 0.16), Vec2::splat(0.045), iron);
            a.metal.beam(v(sx, 0.45, 0.16), v(sx, 0.90, 0.28), Vec2::splat(0.045), iron);
            a.metal.beam(v(sx, 0.45, -0.24), v(sx, 0.45, 0.20), Vec2::splat(0.045), iron);
            a.metal.beam(v(sx, 0.45, -0.22), v(sx, 0.65, -0.15), Vec2::splat(0.04), iron);
            a.metal.beam(v(sx, 0.65, -0.15), v(sx, 0.65, 0.14), Vec2::splat(0.04), iron);
        }

        for i in 0..5 {
            let sz = -0.20 + (i as f32) * 0.09;
            boxr(&mut a.paint, v(-width * 0.5, 0.45, sz - 0.035), v(width * 0.5, 0.49, sz + 0.035), color);
        }

        for j in 0..4 {
            let by = 0.55 + (j as f32) * 0.09;
            let bz = 0.18 + (j as f32) * 0.03;
            a.paint.cuboid_rot(v(0.0, by, bz), v(width, 0.07, 0.025), Quat::from_rotation_x(0.24), color);
        }

        self.place(a, pos, yaw);
        self.collide_local(pos, yaw, v(0.0, 0.45, 0.0), v(width, 0.9, 0.65));
    }

    /// Neatly stacked split firewood against cottage walls and tool sheds.
    pub fn woodpile(&mut self, pos: Vec3, yaw: f32, width: f32, height: f32) {
        let mut a = Art::default();
        let bark = c(0.32, 0.26, 0.20);
        let split_face = c(0.76, 0.68, 0.52);
        let stake_wood = c(0.48, 0.38, 0.28);

        for sx in [-width * 0.5, width * 0.5] {
            for sz in [-0.25f32, 0.25] {
                a.paint.cyl(v(sx, height * 0.5, sz), 0.035, height + 0.2, Quat::IDENTITY, stake_wood);
            }
        }

        let rows = (height / 0.18).floor() as usize;
        let cols = (width / 0.28).floor() as usize;
        for r in 0..rows {
            let y = 0.10 + (r as f32) * 0.18;
            for c_idx in 0..cols {
                let x = -width * 0.5 + 0.16 + (c_idx as f32) * 0.28 + ((r % 2) as f32) * 0.10;
                if x > width * 0.5 - 0.14 {
                    continue;
                }
                a.paint.cyl(v(x, y, 0.0), 0.09, 0.55, Quat::from_rotation_x(FRAC_PI_2), bark);
                a.paint.blob(v(x, y, 0.27), v(0.14, 0.14, 0.02), split_face);
                a.paint.blob(v(x, y, -0.27), v(0.14, 0.14, 0.02), split_face);
            }
        }

        self.place(a, pos, yaw);
        self.collide_local(pos, yaw, v(0.0, height * 0.5, 0.0), v(width, height, 0.65));
    }

    /// Weathered wooden barrel with dark iron hoops (solid or open with brine/rainwater).
    pub fn nautical_barrel(&mut self, pos: Vec3, yaw: f32, open: bool) {
        let mut a = Art::default();
        let oak = c(0.52, 0.42, 0.30);
        let iron = c(0.22, 0.22, 0.24);

        let h = 0.88f32;
        let r_belly = 0.36f32;
        let r_end = 0.30f32;

        a.paint.frustum(v(0.0, h * 0.25, 0.0), r_belly, r_end, h * 0.5, Quat::IDENTITY, oak);
        a.paint.frustum(v(0.0, h * 0.75, 0.0), r_end, r_belly, h * 0.5, Quat::IDENTITY, oak);

        for hy in [0.08f32, 0.32, 0.56, 0.80] {
            let t = (hy - h * 0.5).abs() / (h * 0.5);
            let hr = r_belly + (r_end - r_belly) * t;
            a.metal.torus(v(0.0, hy, 0.0), 0.015, hr + 0.005, Quat::IDENTITY, iron);
        }

        if open {
            a.glass.blob(v(0.0, h - 0.06, 0.0), v(r_end * 0.9, 0.04, r_end * 0.9), c(0.35, 0.65, 0.70));
        } else {
            a.paint.blob(v(0.0, h - 0.02, 0.0), v(r_end * 0.95, 0.03, r_end * 0.95), oak);
            a.paint.cyl(v(r_belly * 0.95, h * 0.5, 0.0), 0.025, 0.04, Quat::from_rotation_z(FRAC_PI_2), c(0.3, 0.22, 0.15));
        }

        self.place(a, pos, yaw);
        self.collide_local(pos, yaw, v(0.0, h * 0.5, 0.0), v(r_belly * 2.0, h, r_belly * 2.0));
    }

    /// Distant rocky sea stacks and headlands rising from the turquoise ocean (matches screenshot.jpg & screenshot-beach.jpg).
    pub fn sea_stack(&mut self, pos: Vec3, radius: f32, height: f32) {
        let mut a = Art::default();
        let rock_dark = c(0.38, 0.36, 0.35);
        let rock_warm = c(0.48, 0.44, 0.40);
        let layers = 5;
        for i in 0..layers {
            let t = i as f32 / layers as f32;
            let r = radius * (1.0 - t * 0.45);
            let y = pos.y + t * height;
            let h = height / layers as f32 * 1.2;
            let col = if i % 2 == 0 { rock_dark } else { rock_warm };
            a.paint.blob(v(0.0, y + h * 0.5, 0.0), v(r * 2.0, h, r * 1.8), col);
        }
        // Palm cluster on the crown
        let crown_y = pos.y + height;
        for j in 0..3 {
            let ang = (j as f32) * TAU / 3.0;
            let offset = v(ang.cos() * radius * 0.25, 0.0, ang.sin() * radius * 0.25);
            a.paint.cyl(offset + v(0.0, crown_y + 1.8, 0.0), 0.12, 3.6, Quat::IDENTITY, c(0.42, 0.38, 0.32));
            for k in 0..6 {
                let fang = (k as f32) * TAU / 6.0;
                let rot = Quat::from_rotation_y(fang) * Quat::from_rotation_x(0.45);
                a.paint.blob(offset + v(0.0, crown_y + 3.6, 0.0) + rot * v(0.0, 0.0, 1.2), v(0.6, 0.08, 1.6), c(0.24, 0.52, 0.22));
            }
        }
        self.place(a, pos, 0.0);
        self.collide(pos + v(0.0, height * 0.5, 0.0), v(radius * 1.8, height, radius * 1.8));
    }

    /// Perimeter boundaries for Tidewater: walls on North, West, East, leaving South completely open to the vast ocean horizon.
    pub fn coastal_boundary(&mut self) {
        let h = self.half;
        let mut a = Art::default();
        // North, East, West fence runs
        let sides = [
            (v(-h, 0.0, h + 0.5), v(h, 0.0, h + 0.5)),   // North
            (v(-h - 0.5, 0.0, -h), v(-h - 0.5, 0.0, h)), // West
            (v(h + 0.5, 0.0, -h), v(h + 0.5, 0.0, h)),   // East
        ];
        for (p0, p1) in sides {
            self.fence_run(&mut a, p0, p1, 0, 4.0);
        }
        self.place(a, Vec3::ZERO, 0.0);

        // Invisible water barrier collider on the South edge so players don't walk into the void off the map bounds,
        // while the ocean view remains completely open and unobstructed!
        self.collide(v(-36.0, 1.5, -h - 0.5), v(44.0, 5.0, 1.0)); // West ocean edge
        self.collide(v(36.0, 1.5, -h - 0.5), v(44.0, 5.0, 1.0));  // East ocean edge
        self.collide(v(0.0, 1.5, -h - 2.0), v(32.0, 5.0, 1.0));   // Behind pier head
    }

    /// Dramatic green tropical mountain ridge / massif rising behind the village,
    /// with lush green slopes, volcanic plugs, basalt rock cliffs, and dense tropical forest canopies.
    pub fn island_mountain_backdrop(&mut self) {
        let mut a = Art::default();

        // Color palette for lush tropical island terrain
        let deep_jungle = c(0.12, 0.28, 0.13);
        let rich_green = c(0.18, 0.38, 0.18);
        let fern_green = c(0.24, 0.46, 0.22);
        let ridge_green = c(0.30, 0.52, 0.25);
        let sunlit_green = c(0.36, 0.58, 0.28);
        let basalt_dark = c(0.24, 0.23, 0.22);
        let basalt_rock = c(0.35, 0.33, 0.31);
        let basalt_light = c(0.45, 0.42, 0.39);

        // 1. Distant massive mountain range / caldera wall spanning the horizon (Z = 160.0 to 240.0)
        let ridge_segments = [
            (-160.0, 200.0, 90.0, 70.0, 68.0, rich_green),
            (-90.0, 190.0, 80.0, 65.0, 78.0, fern_green),
            (-20.0, 185.0, 85.0, 60.0, 84.0, deep_jungle),
            (50.0, 195.0, 90.0, 65.0, 80.0, rich_green),
            (130.0, 205.0, 95.0, 70.0, 72.0, fern_green),
            (200.0, 215.0, 85.0, 65.0, 62.0, deep_jungle),
        ];
        for &(rx, rz, rw, rd, rh, col) in &ridge_segments {
            a.paint.frustum(v(rx, rh * 0.45, rz), rw * 0.25, rw * 0.5, rh * 0.9, Quat::IDENTITY, col);
            a.paint.wedge(v(rx, rh * 0.5, rz), v(rw * 0.8, rh, rd * 0.7), Quat::IDENTITY, col);
        }

        // 2. Central Dramatic Volcanic Plug (The Great Caldera Peak - rising to Y = 95m)
        // Positioned behind the village plaza at X = 8.0, Z = 135.0
        let plug_x = 8.0;
        let plug_z = 135.0;
        a.paint.frustum(v(plug_x, 22.0, plug_z), 26.0, 48.0, 44.0, Quat::IDENTITY, deep_jungle);
        a.paint.frustum(v(plug_x, 48.0, plug_z), 16.0, 28.0, 42.0, Quat::IDENTITY, rich_green);
        a.paint.frustum(v(plug_x - 1.0, 74.0, plug_z + 2.0), 9.0, 17.0, 36.0, Quat::IDENTITY, basalt_rock);
        a.paint.cone(v(plug_x - 1.0, 90.0, plug_z + 2.0), 9.5, 20.0, Quat::IDENTITY, basalt_dark);
        // Basalt cliff bands and sheer vertical rock faces on the volcanic plug
        a.paint.cuboid(v(plug_x - 8.0, 60.0, plug_z - 10.0), v(14.0, 38.0, 8.0), basalt_rock);
        a.paint.cuboid(v(plug_x + 6.0, 68.0, plug_z - 6.0), v(10.0, 32.0, 12.0), basalt_dark);
        a.paint.cuboid_rot(v(plug_x - 4.0, 78.0, plug_z - 3.0), v(8.0, 24.0, 9.0), Quat::from_rotation_y(0.35), basalt_light);
        // Craggy summit pinnacles
        a.paint.cone(v(plug_x + 3.0, 88.0, plug_z - 2.0), 4.5, 14.0, Quat::IDENTITY, basalt_dark);
        a.paint.cone(v(plug_x - 5.0, 84.0, plug_z + 3.0), 4.0, 12.0, Quat::IDENTITY, basalt_rock);

        // 3. West Volcanic Needle / Twin Peak (at X = -62.0, Z = 142.0, rising to Y = 82m)
        let west_x = -62.0;
        let west_z = 142.0;
        a.paint.frustum(v(west_x, 20.0, west_z), 22.0, 42.0, 40.0, Quat::IDENTITY, rich_green);
        a.paint.frustum(v(west_x, 46.0, west_z), 13.0, 23.0, 38.0, Quat::IDENTITY, fern_green);
        a.paint.frustum(v(west_x, 68.0, west_z), 7.0, 14.0, 30.0, Quat::IDENTITY, basalt_rock);
        a.paint.cone(v(west_x, 80.0, west_z), 7.2, 16.0, Quat::IDENTITY, basalt_dark);
        a.paint.cuboid(v(west_x + 4.0, 56.0, west_z - 8.0), v(10.0, 28.0, 6.0), basalt_rock);

        // 4. East Ridge Massif (at X = 75.0, Z = 148.0, rising to Y = 78m)
        let east_x = 75.0;
        let east_z = 148.0;
        a.paint.frustum(v(east_x, 22.0, east_z), 24.0, 44.0, 44.0, Quat::IDENTITY, deep_jungle);
        a.paint.frustum(v(east_x, 48.0, east_z), 14.0, 25.0, 36.0, Quat::IDENTITY, rich_green);
        a.paint.cone(v(east_x, 68.0, east_z), 14.5, 26.0, Quat::IDENTITY, fern_green);
        a.paint.cuboid(v(east_x - 5.0, 52.0, east_z - 7.0), v(11.0, 26.0, 7.0), basalt_rock);

        // 5. Interlocking Forward Ridge Spurs sloping down towards village (Z = 65.0 to 110.0)
        let spurs = [
            (v(-18.0, 18.0, 92.0), v(32.0, 36.0, 42.0), 0.15, fern_green),
            (v(32.0, 20.0, 98.0), v(34.0, 40.0, 44.0), -0.22, rich_green),
            (v(-85.0, 22.0, 105.0), v(38.0, 44.0, 46.0), 0.30, deep_jungle),
            (v(105.0, 21.0, 110.0), v(36.0, 42.0, 45.0), -0.25, rich_green),
            (v(-40.0, 9.0, 74.0), v(32.0, 18.0, 26.0), 0.08, fern_green),
            (v(0.0, 11.0, 76.0), v(36.0, 22.0, 28.0), 0.0, sunlit_green),
            (v(42.0, 10.0, 75.0), v(30.0, 20.0, 25.0), -0.12, ridge_green),
        ];
        for &(pos, size, rot_y, col) in &spurs {
            a.paint.cuboid_rot(pos, size, Quat::from_rotation_y(rot_y), col);
            a.paint.cuboid_rot(
                pos + v(size.x * 0.35, -size.y * 0.2, -size.z * 0.2),
                v(size.x * 0.4, size.y * 0.5, size.z * 0.4),
                Quat::from_rotation_y(rot_y + 0.2),
                basalt_rock,
            );
        }

        // 6. Coastal Bay Headlands wrapping the West and East flanks
        a.paint.frustum(v(-105.0, 16.0, 45.0), 22.0, 45.0, 32.0, Quat::IDENTITY, deep_jungle);
        a.paint.frustum(v(-120.0, 12.0, 0.0), 18.0, 38.0, 24.0, Quat::IDENTITY, rich_green);
        a.paint.cone(v(-105.0, 32.0, 45.0), 22.0, 24.0, Quat::IDENTITY, fern_green);
        a.paint.frustum(v(105.0, 15.0, 45.0), 22.0, 45.0, 30.0, Quat::IDENTITY, deep_jungle);
        a.paint.frustum(v(120.0, 11.0, 0.0), 18.0, 38.0, 22.0, Quat::IDENTITY, rich_green);
        a.paint.cone(v(105.0, 30.0, 45.0), 22.0, 22.0, Quat::IDENTITY, fern_green);

        // 7. Dense Tropical Forest Canopies & Rainforest Tree Blanket
        let canopy_colors = [deep_jungle, rich_green, fern_green, ridge_green, sunlit_green];
        // Foothill canopy band right behind the village perimeter fence (Z = 60.0 to 85.0, X = -80.0 to 80.0)
        for i in 0..65 {
            let fi = i as f32;
            let cx = -75.0 + (fi / 64.0) * 150.0 + (fi * 3.7).sin() * 5.0;
            let cz = 62.0 + (fi * 2.3).cos().abs() * 20.0;
            let rad_x = 4.5 + (fi * 1.9).sin().abs() * 4.5;
            let rad_y = 3.5 + (fi * 2.7).cos().abs() * 3.5;
            let rad_z = 4.5 + (fi * 1.4).cos().abs() * 4.5;
            let cy = rad_y + (fi * 3.1).sin().abs() * 10.0 + (cz - 60.0) * 0.35;
            let col = canopy_colors[i % canopy_colors.len()];
            a.paint.blob(v(cx, cy, cz), v(rad_x, rad_y, rad_z), col);
        }

        // Mid-slope and ravine rainforest clusters (Z = 85.0 to 130.0)
        for i in 0..45 {
            let fi = i as f32;
            let cx = -90.0 + (fi / 44.0) * 180.0 + (fi * 4.1).sin() * 12.0;
            let cz = 85.0 + (fi * 2.9).cos().abs() * 40.0;
            let cy = 14.0 + (fi * 3.3).sin().abs() * 22.0 + (cz - 85.0) * 0.45;
            let rad = 6.0 + (fi * 1.7).cos().abs() * 6.0;
            let col = canopy_colors[(i + 2) % canopy_colors.len()];
            a.paint.blob(v(cx, cy, cz), v(rad * 1.2, rad * 0.8, rad * 1.2), col);
        }

        // Tall jungle palm / emergent rainforest trees behind the village fence
        for i in 0..24 {
            let fi = i as f32;
            let tx = -55.0 + fi * 4.8 + (fi * 2.1).sin() * 2.0;
            let tz = 59.5 + (fi * 3.4).cos().abs() * 4.0;
            let trunk_h = 7.0 + (fi * 1.3).sin().abs() * 4.0;
            let wood = c(0.40, 0.32, 0.22);
            a.paint.cyl(v(tx, trunk_h * 0.5, tz), 0.22, trunk_h, Quat::IDENTITY, wood);
            let crown_y = trunk_h + 1.2;
            a.paint.blob(v(tx, crown_y, tz), v(3.2, 1.4, 3.2), sunlit_green);
            a.paint.blob(v(tx, crown_y + 0.6, tz), v(2.2, 1.1, 2.2), ridge_green);
        }

        self.place(a, Vec3::ZERO, 0.0);
    }

    /// Warm coral sand beach terrain: gentle foreshore slope, subtle sand dunes,
    /// and coastal beach contours flanking the boardwalk and village.
    pub fn beach_terrain(&mut self) {
        let mut a = Art::default();
        let coral_sand = c(0.88, 0.79, 0.66);
        let dune_highlight = c(0.92, 0.83, 0.70);
        let dry_grass = c(0.70, 0.68, 0.40);
        let sea_oats = c(0.46, 0.60, 0.34);

        // 1. Subtle rolling sand dunes on West Beach (X = -38.0 to -14.0, Z = -16.0 to 2.0)
        let west_dunes = [
            (v(-26.0, 0.35, -12.0), v(14.0, 0.7, 8.0), 0.2),
            (v(-18.0, 0.28, -6.0), v(10.0, 0.55, 7.0), -0.15),
            (v(-34.0, 0.42, -8.0), v(12.0, 0.8, 9.0), 0.1),
            (v(-24.0, 0.30, -2.0), v(11.0, 0.6, 6.5), 0.25),
        ];
        for &(pos, size, rot_y) in &west_dunes {
            a.paint.blob_rot(pos, size * 0.5, Quat::from_rotation_y(rot_y), coral_sand);
            a.paint.blob_rot(pos + v(0.0, 0.08, -0.2), size * 0.42, Quat::from_rotation_y(rot_y), dune_highlight);
        }

        // 2. Subtle rolling sand dunes on East Beach (X = 14.0 to 38.0, Z = -16.0 to 2.0)
        let east_dunes = [
            (v(26.0, 0.35, -12.0), v(14.0, 0.7, 8.0), -0.2),
            (v(18.0, 0.28, -6.0), v(10.0, 0.55, 7.0), 0.15),
            (v(34.0, 0.40, -8.0), v(12.0, 0.8, 9.0), -0.1),
            (v(24.0, 0.30, -2.0), v(11.0, 0.6, 6.5), -0.25),
        ];
        for &(pos, size, rot_y) in &east_dunes {
            a.paint.blob_rot(pos, size * 0.5, Quat::from_rotation_y(rot_y), coral_sand);
            a.paint.blob_rot(pos + v(0.0, 0.08, -0.2), size * 0.42, Quat::from_rotation_y(rot_y), dune_highlight);
        }

        // 3. Gentle beach berm along high tide line (Z = -15.0 to -17.0, X = -48.0 to 48.0)
        for i in 0..16 {
            let fi = i as f32;
            let bx = -44.0 + fi * 5.8;
            if bx.abs() < 5.0 {
                continue; // Keep clear for central pier walkway
            }
            let bz = -15.5 + (fi * 1.8).sin() * 0.8;
            a.paint.blob(v(bx, 0.22, bz), v(4.5, 0.38, 2.2), coral_sand);
        }

        // 4. Coastal sea oats and beach grasses tufting along dune crests
        let grass_spots = [
            v(-28.0, 0.5, -11.0), v(-23.0, 0.4, -13.0), v(-32.0, 0.6, -7.0),
            v(-19.0, 0.4, -5.0), v(-25.0, 0.45, -1.0),
            v(28.0, 0.5, -11.0), v(23.0, 0.4, -13.0), v(32.0, 0.6, -7.0),
            v(19.0, 0.4, -5.0), v(25.0, 0.45, -1.0),
        ];
        for (i, &spot) in grass_spots.iter().enumerate() {
            let col = if i % 2 == 0 { sea_oats } else { dry_grass };
            for j in 0..5 {
                let ang = j as f32 * TAU / 5.0 + (i as f32);
                let tilt = Quat::from_rotation_y(ang) * Quat::from_rotation_z(0.35);
                a.paint.cyl(spot + v(ang.cos() * 0.2, 0.25, ang.sin() * 0.2), 0.03, 0.6, tilt, col);
            }
        }

        self.place(a, Vec3::ZERO, 0.0);
    }
}
