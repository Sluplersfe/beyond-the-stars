//! Power and long-range infrastructure: generators, solar, reactor, transmitter, beacon.
//!
//! Models face north (-Z), are centred on their footprint and stand on y = 0.

use bevy::prelude::*;
use sim::BuildingKind;
#[allow(unused_imports)]
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI, TAU};

use super::kit::*;
use super::{Anim, Model};
#[allow(unused_imports)]
use crate::mesh_util::*;

/// Builds `kind` into `m`; `s` is the footprint edge length in tiles.
#[allow(unused_variables)]
pub fn build(kind: BuildingKind, s: f32, m: &mut Model) {
    use BuildingKind as K;
    match kind {
        K::Generator => {
            pad(m, s);
            m.body.cuboid(
                Vec3::new(0.0, 0.45, 0.1),
                Vec3::new(1.5, 0.75, 1.2),
                lin(0x55595f),
            );
            m.body.cuboid(
                Vec3::new(0.0, 0.86, 0.1),
                Vec3::new(1.2, 0.08, 0.9),
                lin(YELLOW),
            );
            for x in [-0.45f32, 0.45] {
                m.body
                    .cylinder(Vec3::new(x, 0.85, 0.5), 0.13, 0.75, 10, lin(DARK));
            }
            for k in 0..3 {
                m.glow.cuboid(
                    Vec3::new(-0.35 + 0.35 * k as f32, 0.45, -0.51),
                    Vec3::new(0.22, 0.06, 0.02),
                    scale(lin(ORANGE), 1.6),
                );
            }
            let mut fan = Geo::fine();
            fan.cuboid(Vec3::ZERO, Vec3::new(0.6, 0.04, 0.1), lin(PANEL));
            fan.cuboid(Vec3::ZERO, Vec3::new(0.1, 0.04, 0.6), lin(PANEL));
            m.parts.push(spinner(
                fan,
                Geo::fine(),
                Vec3::new(0.0, 0.93, -0.1),
                Anim::Spin(8.0),
            ));
            m.lamp = Vec3::new(0.65, 0.9, -0.55);
        }
        K::Solar => {
            m.body.cuboid(
                Vec3::new(0.0, 0.03, 0.0),
                Vec3::new(1.8, 0.06, 1.8),
                lin(CONCRETE),
            );
            for (x, z) in [(-0.6f32, -0.6f32), (0.6, -0.6), (-0.6, 0.6), (0.6, 0.6)] {
                m.body.cuboid(
                    Vec3::new(x, 0.25, z),
                    Vec3::new(0.06, 0.45, 0.06),
                    lin(STEEL),
                );
            }
            let tilt = Quat::from_rotation_x(-0.45);
            m.body.cuboid_rot(
                Vec3::new(0.0, 0.5, 0.0),
                Vec3::new(1.85, 0.05, 1.7),
                tilt,
                lin(0x1d3460),
            );
            for k in 0..4 {
                let z = -0.6 + 0.4 * k as f32;
                // Lie on the tilted panel: offset in the panel's own frame.
                m.glow.cuboid_rot(
                    Vec3::new(0.0, 0.5, 0.0) + tilt * Vec3::new(0.0, 0.03, z),
                    Vec3::new(1.75, 0.01, 0.02),
                    tilt,
                    scale(lin(0x4a80ff), 0.5),
                );
            }
            m.lamp = Vec3::new(0.8, 0.3, 0.8);
        }
        K::Reactor => {
            pad(m, s);
            m.body
                .cylinder(Vec3::new(0.0, 0.08, 0.0), 1.75, 0.4, 32, lin(STEEL));
            m.body
                .cylinder(Vec3::new(0.0, 0.48, 0.0), 1.4, 0.5, 32, lin(PANEL));
            m.body.sphere(
                Vec3::new(0.0, 0.95, 0.0),
                1.15,
                Vec3::new(1.0, 0.75, 1.0),
                24,
                lin(WHITE),
            );
            m.glow.torus(
                Vec3::new(0.0, 0.62, 0.0),
                1.45,
                0.07,
                40,
                scale(lin(GREEN), 2.0),
            );
            for k in 0..4 {
                let a = k as f32 * FRAC_PI_2 + FRAC_PI_4;
                let p = Vec3::new(a.cos(), 0.0, a.sin()) * 1.55;
                m.body.cylinder(p + Vec3::Y * 0.08, 0.2, 1.4, 10, lin(DARK));
                m.glow
                    .cylinder(p + Vec3::Y * 1.48, 0.14, 0.05, 10, lin(GREEN));
            }
            let mut ring = Geo::fine();
            ring.torus(Vec3::ZERO, 1.3, 0.05, 40, lin(PANEL));
            let mut ring_glow = Geo::fine();
            ring_glow.sphere(
                Vec3::new(1.3, 0.0, 0.0),
                0.1,
                Vec3::ONE,
                8,
                scale(lin(GREEN), 3.0),
            );
            ring_glow.sphere(
                Vec3::new(-1.3, 0.0, 0.0),
                0.1,
                Vec3::ONE,
                8,
                scale(lin(GREEN), 3.0),
            );
            m.parts.push(spinner(
                ring,
                ring_glow,
                Vec3::new(0.0, 1.95, 0.0),
                Anim::Spin(1.5),
            ));
            m.lamp = Vec3::new(1.7, 0.6, 1.7);
        }
        K::Transmitter => {
            pad(m, s);
            for (x, z) in [
                (-0.35f32, -0.35f32),
                (0.35, -0.35),
                (-0.35, 0.35),
                (0.35, 0.35),
            ] {
                m.body.cuboid_rot(
                    Vec3::new(x * 0.6, 1.3, z * 0.6),
                    Vec3::new(0.07, 2.6, 0.07),
                    Quat::from_rotation_z(-x * 0.12) * Quat::from_rotation_x(z * 0.12),
                    lin(STEEL),
                );
            }
            for k in 0..5 {
                m.body.cuboid(
                    Vec3::new(0.0, 0.3 + k as f32 * 0.5, 0.0),
                    Vec3::new(0.6 - k as f32 * 0.06, 0.04, 0.6 - k as f32 * 0.06),
                    lin(YELLOW),
                );
            }
            m.glow.sphere(
                Vec3::new(0.0, 2.75, 0.0),
                0.2,
                Vec3::ONE,
                12,
                scale(lin(0xfff070), 4.0),
            );
            let mut dish = Geo::fine();
            dish.cone(Vec3::ZERO, 0.35, 0.18, 12, 0.0, lin(PANEL));
            m.parts.push(spinner(
                dish,
                Geo::fine(),
                Vec3::new(0.0, 2.2, 0.0),
                Anim::Spin(1.0),
            ));
            m.lamp = Vec3::new(0.75, 0.3, 0.75);
        }
        K::Beacon => {
            m.body.cylinder(Vec3::ZERO, 2.4, 0.3, 6, lin(STEEL));
            m.body
                .cylinder(Vec3::new(0.0, 0.3, 0.0), 1.8, 0.3, 6, lin(PANEL));
            m.body
                .cylinder(Vec3::new(0.0, 0.6, 0.0), 1.2, 0.3, 6, lin(WHITE));
            m.body
                .cone(Vec3::new(0.0, 0.9, 0.0), 0.7, 5.5, 6, 0.0, lin(WHITE));
            m.glow.cone(
                Vec3::new(0.0, 0.92, 0.0),
                0.72,
                3.0,
                6,
                PI / 6.0,
                scale(lin(CYAN), 0.6),
            );
            m.glow.sphere(
                Vec3::new(0.0, 6.6, 0.0),
                0.45,
                Vec3::ONE,
                16,
                scale(lin(0xd0f8ff), 6.0),
            );
            let mut rings = Geo::fine();
            rings.torus(Vec3::ZERO, 1.4, 0.06, 48, lin(PANEL));
            rings.torus(Vec3::Y * 1.3, 1.0, 0.05, 40, lin(PANEL));
            let mut rings_glow = Geo::fine();
            for k in 0..6 {
                let a = k as f32 * PI / 3.0;
                rings_glow.sphere(
                    Vec3::new(a.cos() * 1.4, 0.0, a.sin() * 1.4),
                    0.09,
                    Vec3::ONE,
                    6,
                    scale(lin(CYAN), 3.0),
                );
            }
            m.parts.push(spinner(
                rings,
                rings_glow,
                Vec3::new(0.0, 3.2, 0.0),
                Anim::Idle(0.6),
            ));
            m.lamp = Vec3::new(2.0, 0.4, 2.0);
        }
        _ => unreachable!("{kind:?} is not a power building"),
    }
}
