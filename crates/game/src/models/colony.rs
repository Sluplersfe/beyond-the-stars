//! Colony, science and defence: houses, habitats, labs, observatory, shipyard, turret.
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
        K::House => {
            m.body.cuboid(
                Vec3::new(0.0, 0.04, 0.0),
                Vec3::new(1.85, 0.08, 1.85),
                lin(0x8a8a7a),
            );
            m.body.cuboid(
                Vec3::new(-0.15, 0.45, 0.05),
                Vec3::new(1.3, 0.75, 1.1),
                lin(WHITE),
            );
            m.body.cone(
                Vec3::new(-0.15, 0.82, 0.05),
                0.95,
                0.5,
                4,
                FRAC_PI_4,
                lin(0x3d6e8a),
            );
            m.body.cuboid(
                Vec3::new(0.6, 0.3, -0.5),
                Vec3::new(0.4, 0.5, 0.4),
                lin(PANEL),
            );
            windows(m, 0.5, 0.61, -0.65, 0.35, 3, 0xffd890);
            // Garden
            for (x, z) in [(0.7f32, 0.55f32), (0.55, 0.75)] {
                m.body
                    .sphere(Vec3::new(x, 0.22, z), 0.17, Vec3::ONE, 8, lin(0x4a8a3a));
            }
            m.lamp = Vec3::new(0.75, 0.3, -0.75);
        }
        K::Habitat => {
            m.body.cylinder(Vec3::ZERO, 1.4, 0.25, 32, lin(PANEL));
            m.body
                .cylinder(Vec3::new(0.0, 0.25, 0.0), 1.25, 0.06, 32, lin(0x5a8a40));
            for k in 0..7 {
                let a = k as f32 * 0.9;
                let r = 0.3 + (k % 3) as f32 * 0.3;
                m.body.sphere(
                    Vec3::new(a.cos() * r, 0.5, a.sin() * r),
                    0.22,
                    Vec3::new(1.0, 1.3, 1.0),
                    8,
                    lin(0x3f8a3a),
                );
            }
            m.body.cuboid(
                Vec3::new(0.0, 0.6, 0.0),
                Vec3::new(0.4, 0.7, 0.4),
                lin(WHITE),
            );
            m.glass.sphere(
                Vec3::new(0.0, 0.3, 0.0),
                1.3,
                Vec3::new(1.0, 0.85, 1.0),
                28,
                [0.6, 0.85, 1.0, 0.25],
            );
            m.glow
                .torus(Vec3::new(0.0, 0.26, 0.0), 1.32, 0.03, 40, lin(CYAN));
            m.lamp = Vec3::new(1.2, 0.4, 1.2);
        }
        K::Lab | K::LabMk2 => {
            pad(m, s);
            let k = s / 2.0;
            m.body.cuboid(
                Vec3::new(0.0, 0.45, 0.0),
                Vec3::new(1.5 * k, 0.8, 1.5 * k),
                lin(WHITE),
            );
            m.body.cuboid(
                Vec3::new(0.0, 0.9, 0.0),
                Vec3::new(1.6 * k, 0.1, 1.6 * k),
                lin(0x6a4aa0),
            );
            m.glow.cuboid(
                Vec3::new(0.0, 0.6, 0.0),
                Vec3::new(1.52 * k, 0.06, 1.52 * k),
                scale(lin(PURPLE), 1.5),
            );
            m.glass.sphere(
                Vec3::new(0.0, 1.25, 0.0),
                0.4 * k,
                Vec3::ONE,
                16,
                [0.7, 0.6, 1.0, 0.35],
            );
            m.glow.sphere(
                Vec3::new(0.0, 1.25, 0.0),
                0.15 * k,
                Vec3::ONE,
                10,
                scale(lin(PURPLE), 4.0),
            );
            let mut ring = Geo::fine();
            ring.torus(Vec3::ZERO, 0.55 * k, 0.03, 32, lin(PANEL));
            m.parts.push(spinner(
                ring,
                Geo::fine(),
                Vec3::new(0.0, 1.25, 0.0),
                Anim::Spin(2.0),
            ));
            m.lamp = Vec3::new(0.7 * k, 1.0, 0.7 * k);
        }
        K::Observatory => {
            pad(m, s);
            m.body
                .cylinder(Vec3::new(0.0, 0.08, 0.0), 0.8, 0.6, 24, lin(PANEL));
            m.body.sphere(
                Vec3::new(0.0, 0.68, 0.0),
                0.75,
                Vec3::new(1.0, 0.8, 1.0),
                20,
                lin(WHITE),
            );
            let mut scope = Geo::fine();
            scope.cylinder_rot(
                Vec3::ZERO,
                0.14,
                0.95,
                10,
                Quat::from_rotation_x(-0.7),
                lin(DARK),
            );
            let mut lens = Geo::fine();
            lens.sphere(
                Quat::from_rotation_x(-0.7) * Vec3::Y * 0.95,
                0.12,
                Vec3::ONE,
                8,
                scale(lin(CYAN), 2.0),
            );
            m.parts.push(spinner(
                scope,
                lens,
                Vec3::new(0.0, 1.0, 0.0),
                Anim::Idle(0.3),
            ));
            m.lamp = Vec3::new(0.75, 0.4, 0.75);
        }
        K::Shipyard => {
            m.body.cuboid(
                Vec3::new(0.0, 0.05, 0.0),
                Vec3::new(3.9, 0.1, 3.9),
                lin(0x3a3f47),
            );
            m.body
                .cylinder(Vec3::new(0.3, 0.1, 0.3), 1.45, 0.12, 40, lin(0x55606c));
            m.body
                .torus(Vec3::new(0.3, 0.24, 0.3), 1.1, 0.05, 40, lin(YELLOW));
            m.glow.torus(
                Vec3::new(0.3, 0.23, 0.3),
                1.42,
                0.04,
                48,
                scale(lin(CYAN), 1.5),
            );
            // Gantry tower
            m.body.cuboid(
                Vec3::new(-1.45, 1.6, -1.2),
                Vec3::new(0.5, 3.2, 0.5),
                lin(0x7a8592),
            );
            for k in 0..6 {
                m.body.cuboid(
                    Vec3::new(-1.45, 0.4 + k as f32 * 0.55, -1.2),
                    Vec3::new(0.56, 0.06, 0.56),
                    lin(YELLOW),
                );
            }
            m.body.cuboid(
                Vec3::new(-0.75, 2.6, -0.6),
                Vec3::new(1.1, 0.12, 0.12),
                lin(0x7a8592),
            );
            // Rocket
            let r = Vec3::new(0.3, 0.24, 0.3);
            m.body.cylinder(r, 0.32, 2.4, 16, lin(WHITE));
            m.body
                .cone(r + Vec3::Y * 2.4, 0.32, 0.7, 16, 0.0, lin(WHITE));
            m.body
                .cylinder(r + Vec3::Y * 1.5, 0.33, 0.12, 16, lin(0x30363e));
            for k in 0..4 {
                let a = k as f32 * FRAC_PI_2;
                let d = Vec3::new(a.cos(), 0.0, a.sin());
                m.body.cuboid_rot(
                    r + d * 0.38 + Vec3::Y * 0.35,
                    Vec3::new(0.25, 0.6, 0.04),
                    Quat::from_rotation_y(-a),
                    lin(0xc04030),
                );
            }
            m.glow
                .cylinder(r - Vec3::Y * 0.02, 0.2, 0.04, 12, scale(lin(ORANGE), 2.0));
            m.lamp = Vec3::new(1.7, 0.4, 1.7);
        }
        K::Turret => {
            m.body.cylinder(Vec3::ZERO, 0.42, 0.25, 12, lin(DARK));
            m.body
                .cylinder(Vec3::new(0.0, 0.25, 0.0), 0.25, 0.2, 12, lin(STEEL));
            let mut head = Geo::fine();
            head.cuboid(Vec3::ZERO, Vec3::new(0.42, 0.25, 0.45), lin(0x7a3030));
            for x in [-0.09f32, 0.09] {
                head.cylinder_rot(
                    Vec3::new(x, 0.02, -0.15),
                    0.05,
                    0.5,
                    8,
                    Quat::from_rotation_x(-FRAC_PI_2),
                    lin(DARK),
                );
            }
            let mut tip = Geo::fine();
            tip.cuboid(
                Vec3::new(0.0, 0.14, 0.1),
                Vec3::new(0.2, 0.03, 0.1),
                scale(lin(RED), 2.5),
            );
            m.parts
                .push(spinner(head, tip, Vec3::new(0.0, 0.58, 0.0), Anim::Aim));
            m.lamp = Vec3::new(0.35, 0.3, 0.35);
        }
        _ => unreachable!("{kind:?} is not a colony building"),
    }
}

/// Rocket launched from the shipyard: (body, exhaust flame). Stands on y = 0, nose up.
pub fn rocket() -> (Geo, Geo) {
    let mut rocket = Geo::fine();
    rocket.cylinder(Vec3::ZERO, 0.32, 2.4, 16, lin(0xe9edf1));
    rocket.cone(Vec3::Y * 2.4, 0.32, 0.7, 16, 0.0, lin(0xe9edf1));
    rocket.cylinder(Vec3::Y * 1.5, 0.33, 0.12, 16, lin(0x30363e));
    let mut flame = Geo::fine();
    flame.cone(Vec3::ZERO, 0.28, -1.6, 10, 0.0, scale(lin(0xffa040), 3.0));
    flame.sphere(
        Vec3::ZERO,
        0.3,
        Vec3::new(1.0, 0.6, 1.0),
        8,
        scale(lin(0xffe0a0), 4.0),
    );
    (rocket, flame)
}
