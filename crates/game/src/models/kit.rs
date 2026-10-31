//! Shared palette and helpers for the building models.
#![allow(dead_code)]

use bevy::prelude::*;

use super::{Anim, Model, Part};
use crate::mesh_util::*;

// Palette shared by the factory: industrial yellow, steel, panels, accents.
pub const YELLOW: u32 = 0xe0a526;
pub const STEEL: u32 = 0x4a5361;
pub const DARK: u32 = 0x2b313b;
pub const PANEL: u32 = 0xc9d1da;
pub const WHITE: u32 = 0xe9edf1;
pub const CONCRETE: u32 = 0x6d6a66;
pub const BLUE: u32 = 0x3f7fd0;
pub const CYAN: u32 = 0x5fd8ff;
pub const ORANGE: u32 = 0xff8a2a;
pub const GREEN: u32 = 0x5cf08a;
pub const PURPLE: u32 = 0xb070ff;
pub const RED: u32 = 0xff4a3a;

pub fn pad(m: &mut Model, s: f32) {
    m.body.cuboid(
        Vec3::new(0.0, 0.04, 0.0),
        Vec3::new(s - 0.08, 0.08, s - 0.08),
        lin(CONCRETE),
    );
    // Hazard-stripe kerb along the front edge.
    for k in 0..((s * 3.0) as i32) {
        let x = -s / 2.0 + 0.2 + k as f32 / 3.0;
        if x > s / 2.0 - 0.2 {
            break;
        }
        let c = if k % 2 == 0 { YELLOW } else { DARK };
        m.body.cuboid(
            Vec3::new(x, 0.09, -s / 2.0 + 0.1),
            Vec3::new(0.3, 0.03, 0.08),
            lin(c),
        );
    }
}

/// Windows: a row of small glowing panes on the +Z (south) face.
pub fn windows(m: &mut Model, y: f32, z: f32, from: f32, to: f32, n: i32, color: u32) {
    for k in 0..n {
        let t = (k as f32 + 0.5) / n as f32;
        let x = from + (to - from) * t;
        m.glow.cuboid(
            Vec3::new(x, y, z),
            Vec3::new(0.12, 0.1, 0.02),
            scale(lin(color), 1.0),
        );
    }
}

pub fn spinner(body: Geo, glow: Geo, pivot: Vec3, anim: Anim) -> Part {
    Part {
        body,
        metal: Geo::fine(),
        glow,
        pivot,
        anim,
    }
}
