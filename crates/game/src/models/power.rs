//! Power and long-range infrastructure: generators, solar, reactor, transmitter, beacon.
//!
//! Models face north (-Z), are centred on their footprint and stand on y = 0.

use bevy::prelude::*;
use sim::BuildingKind;
use std::f32::consts::{FRAC_PI_2, PI, TAU};

use super::kit::*;
use super::{Anim, Model, Part};
use crate::mesh_util::*;

/// Bare steel for frames, pipes and fittings.
const GUNMETAL: u32 = 0x5c6570;
/// Bright machined metal.
const BRIGHT: u32 = 0xa9b1bb;
/// Near-black housings.
const SHADOW: u32 = 0x23282f;
/// Copper windings and busbars.
const COPPER: u32 = 0xc7773a;
/// Photovoltaic cell blue.
const CELL: u32 = 0x0e1f42;
/// Reactor / beacon ceramic white.
const CERAMIC: u32 = 0xdfe4ea;

/// Builds `kind` into `m`; `s` is the footprint edge length in tiles.
pub fn build(kind: BuildingKind, s: f32, m: &mut Model) {
    use BuildingKind as K;
    let h = s / 2.0;
    match kind {
        K::Generator => generator(m, h),
        K::Solar => solar(m, h),
        K::Reactor => reactor(m, h),
        K::Transmitter => transmitter(m, h),
        K::Beacon => beacon(m, h),
        _ => unreachable!("{kind:?} is not a power building"),
    }
}

// ---------------------------------------------------------------------------------------
// Generator: diesel genset on a skid — engine, alternator, radiator with a spinning fan,
// exhaust stacks and a fuel tank.

fn generator(m: &mut Model, h: f32) {
    foundation(m, h);
    let y0 = 0.08;
    // Skid rails.
    for x in [-0.55f32, 0.55] {
        m.metal
            .cuboid(Vec3::new(x, y0 + 0.04, 0.05), Vec3::new(0.1, 0.08, 1.7), lin(GUNMETAL));
    }
    let deck = y0 + 0.08;
    // Engine block (painted) with a ribbed sump and two valve covers.
    let eng = Vec3::new(-0.05, deck, 0.25);
    m.body
        .cuboid(eng + Vec3::new(0.0, 0.08, 0.0), Vec3::new(0.9, 0.16, 0.7), lin(SHADOW));
    m.body
        .cuboid(eng + Vec3::new(0.0, 0.36, 0.0), Vec3::new(0.82, 0.4, 0.62), lin(YELLOW));
    for x in [-0.2f32, 0.2] {
        m.body.cuboid(
            eng + Vec3::new(x, 0.6, 0.0),
            Vec3::new(0.3, 0.09, 0.52),
            scale(lin(YELLOW), 0.85),
        );
        for k in 0..4 {
            let z = -0.18 + k as f32 * 0.12;
            bolt(&mut m.metal, eng + Vec3::new(x, 0.645, z), Vec3::Y, 0.018, lin(BRIGHT));
        }
    }
    // Side ribs on the block.
    for k in 0..5 {
        let z = -0.24 + k as f32 * 0.12;
        for sx in [-1.0f32, 1.0] {
            m.body.cuboid(
                eng + Vec3::new(sx * 0.415, 0.36, z),
                Vec3::new(0.03, 0.3, 0.04),
                scale(lin(YELLOW), 0.8),
            );
        }
    }
    // Alternator behind the engine: a ribbed drum with a copper-glimpse vent.
    let alt = Vec3::new(-0.05, deck + 0.34, 0.78);
    let along_z = Quat::from_rotation_x(FRAC_PI_2);
    m.body
        .cylinder_rot(alt - Vec3::Z * 0.16, 0.27, 0.34, 20, along_z, lin(BLUE));
    for k in 0..4 {
        m.metal.cylinder_rot(
            alt + Vec3::Z * (-0.12 + k as f32 * 0.08),
            0.285,
            0.025,
            20,
            along_z,
            lin(GUNMETAL),
        );
    }
    m.body.cuboid(
        alt + Vec3::new(0.0, -0.22, 0.0),
        Vec3::new(0.46, 0.14, 0.3),
        lin(SHADOW),
    );
    // Terminal box with copper busbars.
    let tb = Vec3::new(0.3, deck + 0.42, 0.78);
    m.body.cuboid(tb, Vec3::new(0.16, 0.2, 0.2), lin(SHADOW));
    for k in 0..3 {
        m.metal.cuboid(
            tb + Vec3::new(0.09, 0.05 - k as f32 * 0.05, 0.0),
            Vec3::new(0.03, 0.02, 0.14),
            lin(COPPER),
        );
    }

    // Radiator at the front (output side): a tall box with grille slats and a fan on top.
    let rad = Vec3::new(-0.05, deck, -0.48);
    m.body
        .cuboid(rad + Vec3::Y * 0.36, Vec3::new(0.9, 0.72, 0.36), lin(YELLOW));
    for k in 0..9 {
        let x = -0.36 + k as f32 * 0.09;
        m.metal.cuboid(
            rad + Vec3::new(x, 0.36, -0.185),
            Vec3::new(0.025, 0.56, 0.02),
            lin(SHADOW),
        );
    }
    m.metal.cuboid(
        rad + Vec3::new(0.0, 0.36, -0.18),
        Vec3::new(0.78, 0.58, 0.01),
        scale(lin(GUNMETAL), 0.6),
    );
    // Fan shroud.
    lathe(
        &mut m.metal,
        rad + Vec3::Y * 0.72,
        &[
            (0.3, 0.0, lin(GUNMETAL)),
            (0.32, 0.03, lin(GUNMETAL)),
            (0.32, 0.03, lin(GUNMETAL)),
            (0.32, 0.09, lin(BRIGHT)),
            (0.29, 0.09, lin(BRIGHT)),
            (0.29, 0.09, scale(lin(SHADOW), 0.7)),
            (0.29, 0.02, scale(lin(SHADOW), 0.7)),
        ],
        24,
    );
    let mut fan = Part::new(rad + Vec3::Y * 0.78, Anim::Spin(9.0));
    fan.metal
        .cylinder(Vec3::Y * -0.01, 0.05, 0.05, 12, lin(BRIGHT));
    for k in 0..5 {
        let a = k as f32 / 5.0 * TAU;
        fan.body.cuboid_rot(
            Vec3::new(a.cos() * 0.15, 0.015, a.sin() * 0.15),
            Vec3::new(0.21, 0.012, 0.075),
            Quat::from_rotation_y(-a) * Quat::from_rotation_x(0.35),
            lin(SHADOW),
        );
    }
    m.parts.push(fan);

    // Exhaust: manifold pipe off the engine into two stacks with rain caps.
    for (k, x) in [0.42f32, 0.62].into_iter().enumerate() {
        let top = deck + 1.25 + k as f32 * 0.08;
        tube(
            &mut m.metal,
            &[
                eng + Vec3::new(0.38, 0.5, 0.0),
                Vec3::new(x - 0.05, deck + 0.55, 0.25),
                Vec3::new(x, deck + 0.7, 0.3),
                Vec3::new(x, top, 0.3),
            ],
            0.045,
            10,
            lin(GUNMETAL),
        );
        let cap = Vec3::new(x, top, 0.3);
        m.metal.cylinder(cap - Vec3::Y * 0.08, 0.06, 0.08, 12, lin(SHADOW));
        m.glow
            .cylinder(cap - Vec3::Y * 0.005, 0.042, 0.004, 10, lin(0xff7a2a));
        m.metal.cuboid_rot(
            cap + Vec3::new(0.0, 0.06, 0.0),
            Vec3::new(0.13, 0.012, 0.13),
            Quat::from_rotation_x(0.35),
            lin(SHADOW),
        );
    }
    // Fuel tank along the right edge on saddles.
    let tank = Vec3::new(0.72, deck + 0.22, 0.05);
    for z in [-0.35f32, 0.35] {
        m.metal.cuboid(
            Vec3::new(tank.x, deck + 0.06, tank.z + z),
            Vec3::new(0.32, 0.12, 0.08),
            lin(GUNMETAL),
        );
    }
    let tank_rot = Quat::from_rotation_x(FRAC_PI_2);
    m.body
        .cylinder_rot(tank + Vec3::Z * -0.5, 0.16, 1.0, 20, tank_rot, lin(RED));
    for z in [-0.5f32, 0.5] {
        m.body
            .sphere(tank + Vec3::Z * z, 0.16, Vec3::new(1.0, 1.0, 0.45), 16, lin(RED));
    }
    m.metal
        .cylinder(tank + Vec3::new(0.0, 0.15, -0.2), 0.04, 0.05, 10, lin(BRIGHT));
    hazard(
        &mut m.body,
        tank + Vec3::new(0.161, -0.04, -0.3),
        Vec3::Z,
        Vec3::Y,
        Vec3::X,
        0.6,
        0.08,
        0.06,
        lin(YELLOW),
        lin(0x22262c),
    );
    // Control cabinet with a status screen.
    let cab = Vec3::new(-0.72, deck, 0.2);
    m.body
        .cuboid(cab + Vec3::Y * 0.3, Vec3::new(0.3, 0.6, 0.42), lin(PANEL));
    m.glow.cuboid(
        cab + Vec3::new(-0.152, 0.4, 0.0),
        Vec3::new(0.005, 0.12, 0.2),
        lin(0x3aa0c0),
    );
    for k in 0..3 {
        m.glow.cuboid(
            cab + Vec3::new(-0.152, 0.22, -0.1 + k as f32 * 0.1),
            Vec3::new(0.005, 0.025, 0.025),
            lin([0x40c060, 0xc0a030, 0x40c060][k]),
        );
    }
    // Power cable from the terminal box to a corner pylon.
    let post = Vec3::new(0.82, 0.08, 0.85);
    m.metal
        .cylinder(post, 0.03, 0.75, 8, lin(GUNMETAL));
    for k in 0..3 {
        m.rough.cylinder(
            post + Vec3::new(0.0, 0.62 + k as f32 * 0.04, 0.0),
            0.045,
            0.025,
            10,
            lin(0x6a4a3a),
        );
    }
    tube(
        &mut m.rough,
        &sag(tb + Vec3::new(0.09, 0.0, 0.0), post + Vec3::Y * 0.7, 0.12, 8),
        0.018,
        6,
        lin(SHADOW),
    );
    m.lamp = Vec3::new(-0.72, deck + 0.66, 0.2);
}

// ---------------------------------------------------------------------------------------
// Solar: a single-axis tracker carrying four PV modules, with an inverter box.

fn solar(m: &mut Model, h: f32) {
    foundation(m, h);
    let y0 = 0.08;
    // Tracker posts and torque tube along X.
    let axis_y = 0.62;
    for x in [-0.7f32, 0.0, 0.7] {
        m.metal
            .cuboid(Vec3::new(x, y0 + 0.03, 0.0), Vec3::new(0.18, 0.06, 0.18), lin(GUNMETAL));
        m.metal.cuboid(
            Vec3::new(x, (y0 + axis_y) / 2.0, 0.0),
            Vec3::new(0.07, axis_y - y0, 0.07),
            lin(BRIGHT),
        );
    }
    let along_x = Quat::from_rotation_z(FRAC_PI_2);
    m.metal.cylinder_rot(
        Vec3::new(0.9, axis_y, 0.0),
        0.045,
        1.8,
        12,
        along_x,
        lin(GUNMETAL),
    );
    // Slew drive on the middle post.
    m.body.cylinder_rot(
        Vec3::new(0.09, axis_y, 0.0),
        0.1,
        0.18,
        16,
        along_x,
        lin(YELLOW),
    );

    // The panel tilts about X toward the south-facing sun side; everything on it is laid
    // out in the panel's own frame so cell lines lie exactly on the surface.
    let tilt = Quat::from_rotation_x(-0.42);
    let pc = Vec3::new(0.0, axis_y + 0.08, 0.0);
    let on = |x: f32, y: f32, z: f32| pc + tilt * Vec3::new(x, y, z);
    let (pw, pd) = (1.86, 1.62);
    // Aluminium frame under the modules.
    m.metal
        .cuboid_rot(on(0.0, -0.02, 0.0), Vec3::new(pw, 0.03, pd), tilt, lin(BRIGHT));
    for z in [-0.5f32, 0.5] {
        m.metal.cuboid_rot(
            on(0.0, -0.06, z),
            Vec3::new(pw - 0.1, 0.05, 0.05),
            tilt,
            lin(GUNMETAL),
        );
    }
    // Four modules (2 × 2), each a dark cell field with silver busbars and a white frame.
    let (mw, md) = (pw / 2.0 - 0.04, pd / 2.0 - 0.04);
    for (cx, cz) in [(-0.5f32, -0.5f32), (0.5, -0.5), (-0.5, 0.5), (0.5, 0.5)] {
        let (ox, oz) = (cx * (mw + 0.04), cz * (md + 0.04));
        m.body.cuboid_rot(
            on(ox, 0.012, oz),
            Vec3::new(mw, 0.022, md),
            tilt,
            lin(0xd0d6dc),
        );
        m.body.cuboid_rot(
            on(ox, 0.016, oz),
            Vec3::new(mw - 0.05, 0.022, md - 0.05),
            tilt,
            lin(CELL),
        );
        let top = 0.0275;
        // Cell grid: lines across both directions, flat on the module.
        let (cols, rows) = (6, 5);
        for i in 1..cols {
            let x = ox - (mw - 0.05) / 2.0 + (mw - 0.05) * i as f32 / cols as f32;
            m.metal.cuboid_rot(
                on(x, top, oz),
                Vec3::new(0.012, 0.004, md - 0.06),
                tilt,
                lin(0xb8c4d0),
            );
        }
        for j in 1..rows {
            let z = oz - (md - 0.05) / 2.0 + (md - 0.05) * j as f32 / rows as f32;
            m.metal.cuboid_rot(
                on(ox, top, z),
                Vec3::new(mw - 0.06, 0.004, 0.01),
                tilt,
                lin(0x9aa6b4),
            );
        }
    }
    // Inverter box with cooling fins and a status light, conduit to the slew drive.
    let inv = Vec3::new(0.62, y0, 0.72);
    m.body
        .cuboid(inv + Vec3::Y * 0.17, Vec3::new(0.42, 0.34, 0.2), lin(PANEL));
    for k in 0..6 {
        m.metal.cuboid(
            inv + Vec3::new(-0.15 + k as f32 * 0.06, 0.17, 0.105),
            Vec3::new(0.015, 0.26, 0.02),
            lin(GUNMETAL),
        );
    }
    m.glow.cuboid(
        inv + Vec3::new(0.0, 0.28, -0.101),
        Vec3::new(0.16, 0.04, 0.004),
        lin(0x40d070),
    );
    tube(
        &mut m.metal,
        &[
            inv + Vec3::new(-0.21, 0.1, 0.0),
            Vec3::new(0.2, y0 + 0.1, 0.72),
            Vec3::new(0.1, y0 + 0.1, 0.3),
            Vec3::new(0.0, y0 + 0.2, 0.05),
            Vec3::new(0.0, axis_y - 0.1, 0.0),
        ],
        0.02,
        6,
        lin(SHADOW),
    );
    m.lamp = inv + Vec3::new(0.15, 0.4, 0.0);
}

// ---------------------------------------------------------------------------------------
// Reactor: fusion tokamak hall — ceramic containment dome with a spinning magnet ring,
// glowing plasma slits, twin cooling towers and coolant loops.

fn reactor(m: &mut Model, h: f32) {
    foundation(m, h);
    let y0 = 0.08;
    // Raised circular plinth with a hazard ring.
    lathe(
        &mut m.rough,
        Vec3::new(0.0, y0, 0.3),
        &[
            (1.45, 0.0, lin(CONCRETE)),
            (1.45, 0.16, lin(CONCRETE)),
            (1.45, 0.16, scale(lin(CONCRETE), 1.1)),
            (1.3, 0.2, scale(lin(CONCRETE), 1.1)),
            (0.0, 0.2, scale(lin(CONCRETE), 1.1)),
        ],
        40,
    );
    for k in 0..24 {
        let a0 = k as f32 / 24.0 * TAU;
        let a1 = (k as f32 + 0.5) / 24.0 * TAU;
        let col = lin(YELLOW);
        let c = Vec3::new(0.0, y0 + 0.161, 0.3);
        let r0 = 1.452;
        let p = |a: f32, y: f32| c + Vec3::new(a.cos() * r0, y, a.sin() * r0);
        let n = Vec3::new((a0 + a1).cos() * 0.5, 0.0, (a0 + a1).sin() * 0.5).normalize();
        flat(
            &mut m.body,
            [p(a0, -0.06), p(a1, -0.06), p(a1, -0.01), p(a0, -0.01)],
            n,
            col,
        );
    }
    let dome_c = Vec3::new(0.0, y0 + 0.2, 0.3);
    // Containment dome: drum wall, then a ribbed ceramic cap.
    let dome: Vec<(f32, f32, Rgba)> = (0..=10)
        .map(|k| {
            let t = k as f32 / 10.0;
            let a = t * FRAC_PI_2;
            let (r, y) = if k == 0 {
                (1.12, 0.0)
            } else {
                (1.12 * a.cos().max(0.0), 0.55 + 0.95 * a.sin())
            };
            (r, y, scale(lin(CERAMIC), 0.92 + 0.08 * t))
        })
        .collect();
    let mut prof = vec![(1.12, 0.0, scale(lin(CERAMIC), 0.85)), (1.12, 0.55, lin(CERAMIC))];
    prof.extend(dome.into_iter().skip(1));
    lathe(&mut m.body, dome_c, &prof, 40);
    // Meridian ribs.
    for k in 0..12 {
        let a = k as f32 / 12.0 * TAU;
        let d = Vec3::new(a.cos(), 0.0, a.sin());
        let pts: Vec<Vec3> = (0..=8)
            .map(|i| {
                let t = i as f32 / 8.0 * FRAC_PI_2 * 0.92;
                dome_c + d * (1.13 * t.cos()) + Vec3::Y * (0.55 + 0.96 * t.sin())
            })
            .collect();
        let mut all = vec![dome_c + d * 1.13];
        all.extend(pts);
        tube(&mut m.metal, &all, 0.022, 6, lin(GUNMETAL));
    }
    // Plasma viewing slits around the drum (sparing glow).
    for k in 0..8 {
        let a = (k as f32 + 0.5) / 8.0 * TAU;
        let d = Vec3::new(a.cos(), 0.0, a.sin());
        m.glow.cuboid_rot(
            dome_c + d * 1.125 + Vec3::Y * 0.3,
            Vec3::new(0.04, 0.26, 0.012),
            Quat::from_rotation_y(-a + FRAC_PI_2),
            lin(0x5fd8ff),
        );
        m.metal.cuboid_rot(
            dome_c + d * 1.13 + Vec3::Y * 0.3,
            Vec3::new(0.09, 0.32, 0.012),
            Quat::from_rotation_y(-a + FRAC_PI_2),
            lin(SHADOW),
        );
    }
    // Crown: a vent ring with the plasma glow inside.
    let crown = dome_c + Vec3::Y * 1.48;
    lathe(
        &mut m.metal,
        crown,
        &[
            (0.32, 0.0, lin(GUNMETAL)),
            (0.32, 0.12, lin(GUNMETAL)),
            (0.32, 0.12, lin(BRIGHT)),
            (0.26, 0.14, lin(BRIGHT)),
            (0.26, 0.14, lin(SHADOW)),
            (0.26, 0.05, lin(SHADOW)),
        ],
        24,
    );
    m.glow
        .cylinder(crown + Vec3::Y * 0.05, 0.24, 0.02, 20, lin(0x6fe0ff));

    // Spinning magnet ring (toroidal field coils) around the dome.
    let mut ring = Part::new(dome_c + Vec3::Y * 0.35, Anim::Spin(1.5));
    ring.metal.torus(Vec3::ZERO, 1.3, 0.05, 48, lin(BRIGHT));
    for k in 0..12 {
        let a = k as f32 / 12.0 * TAU;
        let d = Vec3::new(a.cos(), 0.0, a.sin());
        ring.body.cuboid_rot(
            d * 1.3,
            Vec3::new(0.12, 0.2, 0.12),
            Quat::from_rotation_y(-a),
            lin(BLUE),
        );
        ring.metal.cuboid_rot(
            d * 1.3 + Vec3::Y * 0.105,
            Vec3::new(0.13, 0.02, 0.13),
            Quat::from_rotation_y(-a),
            lin(COPPER),
        );
        if k % 3 == 0 {
            ring.glow.cuboid_rot(
                d * 1.365,
                Vec3::new(0.015, 0.08, 0.05),
                Quat::from_rotation_y(-a),
                lin(0x4fc8ff),
            );
        }
    }
    m.parts.push(ring);
    // Ring support pylons.
    for k in 0..4 {
        let a = (k as f32 + 0.5) / 4.0 * TAU;
        let d = Vec3::new(a.cos(), 0.0, a.sin());
        beam(
            &mut m.metal,
            dome_c + d * 1.5,
            dome_c + d * 1.42 + Vec3::Y * 0.22,
            0.08,
            0.08,
            lin(GUNMETAL),
        );
    }

    // Twin hyperboloid cooling towers at the back corners, with steam-glow rims.
    for x in [-1.35f32, 1.35] {
        let c = Vec3::new(x, y0, -1.35);
        let prof: Vec<(f32, f32, Rgba)> = (0..=8)
            .map(|k| {
                let t = k as f32 / 8.0;
                let r = 0.38 - 0.42 * t + 0.4 * t * t;
                (r + 0.02, t * 1.3, scale(lin(0xb9b4ac), 0.9 + 0.12 * t))
            })
            .collect();
        let mut p = prof.clone();
        let top_r = p.last().map(|q| q.0).unwrap_or(0.3);
        p.push((top_r, 1.3, lin(0x8f8a82)));
        p.push((top_r - 0.04, 1.3, lin(0x8f8a82)));
        p.push((top_r - 0.04, 1.0, scale(lin(SHADOW), 0.8)));
        lathe(&mut m.rough, c, &p, 28);
        // Support legs ring at the base.
        for k in 0..10 {
            let a = k as f32 / 10.0 * TAU;
            let d = Vec3::new(a.cos(), 0.0, a.sin());
            beam(
                &mut m.metal,
                c + d * 0.42,
                c + d * 0.38 + Vec3::Y * 0.14,
                0.03,
                0.03,
                lin(GUNMETAL),
            );
        }
        // Warm vapour glow just inside the lip.
        m.glow
            .torus(c + Vec3::Y * 1.26, top_r - 0.06, 0.01, 24, lin(0x8fb8c8));
        // Coolant loop from the dome to the tower.
        tube(
            &mut m.metal,
            &[
                dome_c + Vec3::new(x.signum() * 0.85, 0.15, -0.75),
                Vec3::new(x * 0.9, y0 + 0.35, -1.1),
                c + Vec3::new(-x.signum() * 0.25, 0.35, 0.25),
            ],
            0.06,
            12,
            lin(BLUE),
        );
        for t in [0.3f32, 0.7] {
            let a = dome_c + Vec3::new(x.signum() * 0.85, 0.15, -0.75);
            let b = c + Vec3::new(-x.signum() * 0.25, 0.35, 0.25);
            m.metal.torus(a.lerp(b, t), 0.075, 0.015, 12, lin(GUNMETAL));
        }
    }

    // Control block at the front-right with lit windows and an antenna.
    let cb = Vec3::new(1.35, y0, 1.35);
    m.body
        .cuboid(cb + Vec3::Y * 0.3, Vec3::new(0.9, 0.6, 0.8), lin(PANEL));
    m.body
        .cuboid(cb + Vec3::Y * 0.63, Vec3::new(0.95, 0.06, 0.85), lin(SHADOW));
    for k in 0..4 {
        m.glow.cuboid(
            cb + Vec3::new(-0.3 + k as f32 * 0.2, 0.38, 0.401),
            Vec3::new(0.12, 0.14, 0.004),
            lin(0xffd890),
        );
    }
    m.metal
        .cylinder(cb + Vec3::new(0.3, 0.66, -0.25), 0.015, 0.5, 6, lin(BRIGHT));
    // Transformer yard at the front-left: two units with insulators.
    for (k, z) in [0.95f32, 1.55].into_iter().enumerate() {
        let t = Vec3::new(-1.4, y0, z);
        m.body
            .cuboid(t + Vec3::Y * 0.22, Vec3::new(0.5, 0.44, 0.4), lin(0x5b6b5b));
        for f in 0..5 {
            m.metal.cuboid(
                t + Vec3::new(-0.2 + f as f32 * 0.1, 0.22, 0.205),
                Vec3::new(0.02, 0.36, 0.02),
                lin(GUNMETAL),
            );
        }
        for x in [-0.12f32, 0.0, 0.12] {
            for i in 0..3 {
                m.rough.cylinder(
                    t + Vec3::new(x, 0.44 + i as f32 * 0.04, 0.0),
                    0.03 - i as f32 * 0.004,
                    0.03,
                    8,
                    lin(0x8a5a40),
                );
            }
        }
        let _ = k;
    }
    // Warning stripes on the front kerb of the plinth are part of the foundation; add
    // radiation trefoil plates on the drum.
    for a in [PI * 0.5 + 0.35, PI * 0.5 - 0.35] {
        let d = Vec3::new(a.cos(), 0.0, a.sin());
        let c = dome_c + d * 1.125 + Vec3::Y * 0.3;
        m.body.cuboid_rot(
            c,
            Vec3::new(0.2, 0.2, 0.01),
            Quat::from_rotation_y(-a + FRAC_PI_2),
            lin(YELLOW),
        );
    }
    m.lamp = cb + Vec3::new(-0.3, 0.72, -0.2);
}

// ---------------------------------------------------------------------------------------
// Transmitter: lattice mast with a big steerable dish and an emitter feed.

fn transmitter(m: &mut Model, h: f32) {
    foundation(m, h);
    let y0 = 0.08;
    let top = 2.05;
    // Tapered four-leg lattice tower.
    let leg = |sx: f32, sz: f32, y: f32| {
        let w = 0.55 - 0.35 * ((y - y0) / (top - y0));
        Vec3::new(sx * w, y, sz * w)
    };
    let corners = [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
    for (sx, sz) in corners {
        beam(
            &mut m.metal,
            leg(sx, sz, y0),
            leg(sx, sz, top),
            0.06,
            0.06,
            lin(GUNMETAL),
        );
        m.rough.cuboid(
            leg(sx, sz, y0) + Vec3::Y * 0.04,
            Vec3::new(0.18, 0.08, 0.18),
            lin(CONCRETE),
        );
    }
    let levels = 5;
    for l in 0..=levels {
        let y = y0 + 0.12 + (top - y0 - 0.12) * l as f32 / levels as f32;
        for i in 0..4 {
            let (ax, az) = corners[i];
            let (bx, bz) = corners[(i + 1) % 4];
            beam(
                &mut m.metal,
                leg(ax, az, y),
                leg(bx, bz, y),
                0.03,
                0.03,
                lin(GUNMETAL),
            );
            if l < levels {
                let y2 = y0 + 0.12 + (top - y0 - 0.12) * (l + 1) as f32 / levels as f32;
                let (from, to) = if l % 2 == 0 {
                    (leg(ax, az, y), leg(bx, bz, y2))
                } else {
                    (leg(bx, bz, y), leg(ax, az, y2))
                };
                beam(&mut m.metal, from, to, 0.022, 0.022, lin(BRIGHT));
            }
        }
    }
    // Ladder up one face with a cage.
    for k in 0..14 {
        let y = y0 + 0.15 + k as f32 * 0.14;
        let w = leg(1.0, 1.0, y);
        m.metal.cuboid(
            Vec3::new(0.0, y, w.z + 0.03),
            Vec3::new(0.18, 0.012, 0.012),
            lin(YELLOW),
        );
    }
    // Head platform with railing.
    m.metal
        .cuboid(Vec3::new(0.0, top, 0.0), Vec3::new(0.7, 0.05, 0.7), lin(SHADOW));
    for (sx, sz) in corners {
        m.metal.cylinder(
            Vec3::new(sx * 0.33, top, sz * 0.33),
            0.012,
            0.22,
            6,
            lin(YELLOW),
        );
    }
    for i in 0..4 {
        let (ax, az) = corners[i];
        let (bx, bz) = corners[(i + 1) % 4];
        rod(
            &mut m.metal,
            Vec3::new(ax * 0.33, top + 0.22, az * 0.33),
            Vec3::new(bx * 0.33, top + 0.22, bz * 0.33),
            0.01,
            6,
            lin(YELLOW),
        );
    }
    // Azimuth bearing.
    m.metal
        .cylinder(Vec3::new(0.0, top + 0.02, 0.0), 0.18, 0.12, 20, lin(BRIGHT));

    // Dish assembly spins in azimuth above the platform.
    let mut dish = Part::new(Vec3::new(0.0, top + 0.15, 0.0), Anim::Spin(1.0));
    dish.body.cuboid(Vec3::Y * 0.12, Vec3::new(0.22, 0.24, 0.3), lin(PANEL));
    // Parabolic reflector tilted up toward -Z, built as a lathe in its own frame.
    let tilt = Quat::from_rotation_x(-0.75);
    let dc = Vec3::new(0.0, 0.42, 0.05);
    let mut refl = Geo::fine();
    let prof: Vec<(f32, f32, Rgba)> = (0..=8)
        .map(|k| {
            let r = 0.85 * k as f32 / 8.0;
            (r, 0.35 * (r / 0.85).powi(2), scale(lin(0xe6eaee), 0.92 + 0.08 * r))
        })
        .collect();
    // Inner (concave, facing up) and outer (back) surfaces.
    let mut inner = prof.clone();
    inner.reverse();
    lathe(&mut refl, Vec3::ZERO, &inner, 36);
    let back: Vec<(f32, f32, Rgba)> = prof
        .iter()
        .map(|&(r, y, _)| (r, y - 0.03, lin(GUNMETAL)))
        .collect();
    lathe(&mut refl, Vec3::ZERO, &back, 36);
    refl.torus(Vec3::Y * 0.34, 0.85, 0.02, 40, lin(BRIGHT));
    let tf = Transform::from_translation(dc).with_rotation(tilt);
    dish.body.append(&refl, &tf);
    // Feed horn on three struts, with the emitter glow.
    let feed = dc + tilt * Vec3::Y * 0.85;
    for k in 0..3 {
        let a = k as f32 / 3.0 * TAU;
        let rim = dc + tilt * Vec3::new(a.cos() * 0.8, 0.33, a.sin() * 0.8);
        rod(&mut dish.metal, rim, feed, 0.012, 6, lin(BRIGHT));
    }
    dish.metal.sphere(feed, 0.07, Vec3::ONE, 12, lin(SHADOW));
    dish.glow
        .sphere(feed - tilt * Vec3::Y * 0.05, 0.045, Vec3::ONE, 10, lin(0x8fe6ff));
    // Counterweight.
    dish.metal
        .cuboid(Vec3::new(0.0, 0.2, 0.28), Vec3::new(0.2, 0.16, 0.14), lin(SHADOW));
    m.parts.push(dish);

    // Equipment shed at the base with a lit door and cable runs up the tower.
    let shed = Vec3::new(0.6, y0, -0.62);
    m.body
        .cuboid(shed + Vec3::Y * 0.2, Vec3::new(0.62, 0.4, 0.5), lin(PANEL));
    m.body
        .cuboid(shed + Vec3::Y * 0.43, Vec3::new(0.68, 0.05, 0.56), lin(SHADOW));
    m.glow.cuboid(
        shed + Vec3::new(-0.1, 0.16, -0.251),
        Vec3::new(0.14, 0.24, 0.004),
        lin(0xffd890),
    );
    for (k, x) in [0.2f32, 0.4].into_iter().enumerate() {
        m.body.cuboid(
            shed + Vec3::new(x - 0.2, 0.3, -0.252),
            Vec3::new(0.1, 0.06, 0.004),
            lin([0x40c060, 0x3aa0c0][k]),
        );
    }
    tube(
        &mut m.rough,
        &[
            shed + Vec3::new(-0.31, 0.3, 0.0),
            leg(1.0, -1.0, 0.5) + Vec3::new(0.02, 0.0, 0.02),
            leg(1.0, -1.0, top - 0.05) + Vec3::new(0.02, 0.0, 0.02),
        ],
        0.02,
        6,
        lin(SHADOW),
    );
    // Aircraft warning light on the mast tip.
    m.metal
        .cylinder(Vec3::new(-0.3, top, -0.3), 0.012, 0.35, 6, lin(BRIGHT));
    m.glow
        .sphere(Vec3::new(-0.3, top + 0.37, -0.3), 0.03, Vec3::ONE, 8, lin(RED));
    m.lamp = shed + Vec3::new(0.2, 0.5, 0.1);
}

// ---------------------------------------------------------------------------------------
// Beacon: landmark spire on a stepped octagonal plinth, with buttress fins, rotating halo
// rings and a glowing crown.

fn beacon(m: &mut Model, h: f32) {
    foundation(m, h);
    let y0 = 0.08;
    // Stepped octagonal plinth (8-sided lathe).
    lathe(
        &mut m.rough,
        Vec3::Y * y0,
        &[
            (2.2, 0.0, lin(CONCRETE)),
            (2.2, 0.18, lin(CONCRETE)),
            (2.2, 0.18, scale(lin(CONCRETE), 1.12)),
            (1.75, 0.18, scale(lin(CONCRETE), 1.12)),
            (1.75, 0.18, lin(CONCRETE)),
            (1.75, 0.36, lin(CONCRETE)),
            (1.75, 0.36, scale(lin(CONCRETE), 1.12)),
            (1.3, 0.36, scale(lin(CONCRETE), 1.12)),
            (1.3, 0.36, lin(SHADOW)),
            (1.3, 0.5, lin(SHADOW)),
            (1.3, 0.5, scale(lin(CERAMIC), 0.8)),
            (0.0, 0.5, scale(lin(CERAMIC), 0.8)),
        ],
        8,
    );
    // Light strips on the step risers.
    for k in 0..8 {
        let a0 = (k as f32 + 0.5) / 8.0 * TAU - TAU / 16.0;
        let d = Vec3::new(a0.cos(), 0.0, a0.sin());
        let r = 1.75 * (PI / 8.0).cos();
        m.glow.cuboid_rot(
            Vec3::Y * (y0 + 0.27) + d * (r + 0.004),
            Vec3::new(0.5, 0.025, 0.008),
            Quat::from_rotation_y(-a0 + FRAC_PI_2),
            lin(0x4fb8e8),
        );
    }
    let base = Vec3::Y * (y0 + 0.5);
    // The spire: a slender lathe with collar bands.
    let spire_h = 5.6;
    let prof: Vec<(f32, f32, Rgba)> = (0..=12)
        .map(|k| {
            let t = k as f32 / 12.0;
            let r = 0.62 * (1.0 - t).powf(1.4) + 0.08;
            (r, t * spire_h, scale(lin(CERAMIC), 0.86 + 0.14 * t))
        })
        .collect();
    lathe(&mut m.body, base, &prof, 24);
    for t in [0.18f32, 0.42, 0.66, 0.85] {
        let r = 0.62 * (1.0 - t).powf(1.4) + 0.08;
        m.metal
            .torus(base + Vec3::Y * t * spire_h, r + 0.01, 0.025, 28, lin(GUNMETAL));
    }
    // Four buttress fins, each with a light seam.
    for k in 0..4 {
        let a = k as f32 / 4.0 * TAU + PI / 4.0;
        let d = Vec3::new(a.cos(), 0.0, a.sin());
        let side = Vec3::new(-d.z, 0.0, d.x);
        let pts = [
            (1.25f32, 0.0f32),
            (0.6, 0.0),
            (0.45, 2.4),
            (0.55, 1.2),
        ];
        let p = |r: f32, y: f32, s: f32| base + d * r + Vec3::Y * y + side * s;
        let w = 0.05;
        // Two triangular faces and edge caps.
        for s in [-w, w] {
            flat(
                &mut m.body,
                [p(pts[0].0, pts[0].1, s), p(pts[1].0, pts[1].1, s), p(pts[2].0, pts[2].1, s), p(pts[3].0, pts[3].1, s)],
                side * s.signum(),
                lin(CERAMIC),
            );
        }
        let edge_a = p(pts[0].0, pts[0].1, 0.0);
        let edge_b = p(pts[3].0, pts[3].1, 0.0);
        let edge_c = p(pts[2].0, pts[2].1, 0.0);
        beam(&mut m.metal, edge_a, edge_b, 2.0 * w + 0.01, 0.03, lin(GUNMETAL));
        beam(&mut m.metal, edge_b, edge_c, 2.0 * w + 0.01, 0.03, lin(GUNMETAL));
        m.glow.cuboid_rot(
            (edge_b + edge_c) / 2.0 + d * 0.02,
            Vec3::new(0.012, (edge_c - edge_b).length() * 0.8, 0.012),
            Quat::from_rotation_arc(Vec3::Y, (edge_c - edge_b).normalize()),
            lin(0x4fb8e8),
        );
    }
    // Crown: cradle + glowing orb at the top.
    let tip = base + Vec3::Y * spire_h;
    for k in 0..4 {
        let a = k as f32 / 4.0 * TAU;
        let d = Vec3::new(a.cos(), 0.0, a.sin());
        tube(
            &mut m.metal,
            &[
                tip - Vec3::Y * 0.3,
                tip + d * 0.22 + Vec3::Y * 0.1,
                tip + d * 0.18 + Vec3::Y * 0.5,
            ],
            0.02,
            6,
            lin(BRIGHT),
        );
    }
    let orb = tip + Vec3::Y * 0.42;
    m.glow.sphere(orb, 0.26, Vec3::ONE, 16, lin(0xbfeeff));
    m.glass.sphere(orb, 0.34, Vec3::ONE, 16, [0.5, 0.8, 1.0, 0.2]);

    // Halo rings orbit the spire mid-height.
    let mut rings = Part::new(base + Vec3::Y * 2.7, Anim::Idle(0.6));
    rings.metal.torus(Vec3::ZERO, 1.4, 0.05, 56, lin(BRIGHT));
    rings.metal.torus(Vec3::Y * 1.1, 1.0, 0.04, 48, lin(BRIGHT));
    for (r, y, n) in [(1.4f32, 0.0f32, 8), (1.0, 1.1, 6)] {
        for k in 0..n {
            let a = k as f32 / n as f32 * TAU;
            let d = Vec3::new(a.cos(), 0.0, a.sin());
            rings.body.cuboid_rot(
                d * r + Vec3::Y * y,
                Vec3::new(0.1, 0.12, 0.16),
                Quat::from_rotation_y(-a),
                lin(SHADOW),
            );
            rings.glow.cuboid_rot(
                d * (r + 0.055) + Vec3::Y * y,
                Vec3::new(0.01, 0.06, 0.1),
                Quat::from_rotation_y(-a),
                lin(0x5fd8ff),
            );
        }
    }
    // Spokes from the lower ring to a hub collar on the spire.
    for k in 0..4 {
        let a = k as f32 / 4.0 * TAU + PI / 8.0;
        let d = Vec3::new(a.cos(), 0.0, a.sin());
        rod(&mut rings.metal, d * 0.48, d * 1.36, 0.015, 6, lin(GUNMETAL));
    }
    rings.metal.torus(Vec3::ZERO, 0.46, 0.04, 24, lin(GUNMETAL));
    m.parts.push(rings);

    // Corner floodlight posts on the foundation.
    for (sx, sz) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        let p = Vec3::new(sx * 2.15, y0, sz * 2.15);
        m.metal.cylinder(p, 0.03, 0.6, 8, lin(GUNMETAL));
        let aim = Quat::from_rotation_arc(Vec3::Y, (-Vec3::new(sx, -1.2, sz)).normalize());
        m.body
            .cylinder_rot(p + Vec3::Y * 0.6, 0.07, 0.1, 12, aim, lin(SHADOW));
        m.glow.cylinder_rot(
            p + Vec3::Y * 0.6 + aim * Vec3::Y * 0.1,
            0.055,
            0.005,
            12,
            aim,
            lin(0xfff0c8),
        );
    }
    m.lamp = Vec3::new(1.95, y0 + 0.6, 1.95);
}
