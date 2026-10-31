//! Moving items: inserters, splitters, mergers, storage, providers and conveyor belts.
//!
//! Models face north (-Z), are centred on their footprint and stand on y = 0.
//! Everything here is built from a handful of private helpers (lathes, extrusions,
//! beams, tubes, hazard bands, arrows) so the family shares one visual language:
//! dark steel frames, industrial-yellow paint, bare-metal joints and sparing glow.

use bevy::prelude::*;
use sim::BuildingKind;
use sim::buildings::BeltTier;
use std::f32::consts::{FRAC_PI_2, PI, TAU};

use super::kit::*;
use super::{Anim, Model, Part};
use crate::mesh_util::*;

// ---------------------------------------------------------------------------------------
// Local palette (on top of the shared kit palette).

/// Belt rubber.
const RUBBER: u32 = 0x1d2126;
/// Dark steel used for frames and plinths.
const FRAME: u32 = 0x353c46;
/// Plinth steel, a touch lighter than `FRAME` so it separates from the ground.
const PLATE: u32 = 0x404852;
/// Bright bare metal (shafts, pins, rails).
const BRIGHT: u32 = 0xaab2bc;
/// Darker bare metal (housings, clevises).
const GUNMETAL: u32 = 0x626b77;
/// Hazard-stripe black.
const HAZ: u32 = 0x22262c;

/// Height of the belt's running surface (items ride at ~0.1).
const BELT_TOP: f32 = 0.095;
/// Half the rubber width; the inner faces of the side rails sit here.
const BELT_HALF: f32 = 0.386;
/// Vertex-colour AO the renderer applies to building layers (see `models::building`).
const BUILDING_AO: f32 = 0.45;

/// Builds `kind` into `m`; `s` is the footprint edge length in tiles.
pub fn build(kind: BuildingKind, s: f32, m: &mut Model) {
    use BuildingKind as K;
    match kind {
        K::Inserter => inserter(m, false),
        K::FastInserter => inserter(m, true),
        K::Depot => depot(m, s),
        K::Provider => provider(m),
        K::Splitter => router(m, true),
        K::Merger => router(m, false),
        _ => unreachable!("{kind:?} is not a logistics building"),
    }
}

// ---------------------------------------------------------------------------------------
// Conveyor belt

/// Conveyor segment (one tile) running north.
pub fn belt(tier: BeltTier) -> Geo {
    let (cap, cleat) = belt_colours(tier);
    let mut g = Geo::fine();
    belt_run(&mut g, -0.5, 0.5, cap, cleat, true);
    if tier == BeltTier::Express {
        // Express: a bright pin-stripe along each rail top.
        for sx in [-1.0f32, 1.0] {
            let x = sx * 0.431;
            flat(
                &mut g,
                [
                    Vec3::new(x - 0.008, 0.1335, 0.5),
                    Vec3::new(x + 0.008, 0.1335, 0.5),
                    Vec3::new(x + 0.008, 0.1335, -0.5),
                    Vec3::new(x - 0.008, 0.1335, -0.5),
                ],
                Vec3::Y,
                lin(0xd8ecff),
            );
        }
    } else if tier == BeltTier::Fast {
        // Fast: a dark centre groove on each rail top.
        for sx in [-1.0f32, 1.0] {
            let x = sx * 0.431;
            flat(
                &mut g,
                [
                    Vec3::new(x - 0.006, 0.1335, 0.5),
                    Vec3::new(x + 0.006, 0.1335, 0.5),
                    Vec3::new(x + 0.006, 0.1335, -0.5),
                    Vec3::new(x - 0.006, 0.1335, -0.5),
                ],
                Vec3::Y,
                lin(0x5a1a14),
            );
        }
    }
    g
}

/// Rail cap and cleat colours per tier.
fn belt_colours(tier: BeltTier) -> (Rgba, Rgba) {
    let grey = lin(0x3c434c);
    match tier {
        BeltTier::Basic => (lin(0xe0a526), grey),
        BeltTier::Fast => (lin(0xd8412f), mixc(grey, lin(0x8a3a30), 0.35)),
        BeltTier::Express => (lin(0x3f86e0), mixc(grey, lin(0x3a6aa8), 0.45)),
    }
}

/// A straight run of belt along Z from `z0` to `z1` (z0 < z1): side rails with a
/// coloured cap, ribbed rubber with chevron cleats pointing north, a support leg at the
/// tile middle, and end rollers (visible where a run ends).
fn belt_run(g: &mut Geo, z0: f32, z1: f32, cap: Rgba, cleat: Rgba, rollers: bool) {
    let frame = lin(FRAME);
    // Right rail profile, counter-clockwise (outer bottom → over the top → inner face).
    let rail: [(f32, f32, Rgba); 12] = [
        (0.452, 0.026, scale(frame, 0.5)),
        (0.466, 0.04, scale(frame, 0.85)),
        (0.466, 0.082, frame),
        (0.466, 0.082, scale(cap, 0.4)),
        (0.478, 0.089, scale(cap, 0.55)),
        (0.478, 0.089, scale(cap, 0.85)),
        (0.478, 0.116, scale(cap, 0.95)),
        (0.466, 0.1335, scale(cap, 1.25)),
        (0.4, 0.1335, scale(cap, 1.08)),
        (0.386, 0.121, scale(cap, 1.15)),
        (0.386, 0.095, scale(cap, 0.62)),
        (0.386, 0.05, scale(frame, 0.4)),
    ];
    for sx in [1.0f32, -1.0] {
        let mut prof: Vec<(f32, f32, Rgba)> =
            rail.iter().map(|&(x, y, c)| (x * sx, y, c)).collect();
        if sx < 0.0 {
            prof.reverse();
        }
        extrude_z(g, &prof, z0, z1, true);
    }

    // Rubber: darker toward the rails (contact shadow).
    let rub = lin(RUBBER);
    let xs = [-BELT_HALF, -0.3, 0.3, BELT_HALF];
    let ks = [0.55, 1.0, 1.0, 0.55];
    for i in 0..3 {
        let (xa, xb) = (xs[i], xs[i + 1]);
        let (ca, cb) = (scale(rub, ks[i]), scale(rub, ks[i + 1]));
        let i0 = g.vertex(Vec3::new(xa, BELT_TOP, z1), Vec3::Y, ca);
        g.vertex(Vec3::new(xb, BELT_TOP, z1), Vec3::Y, cb);
        g.vertex(Vec3::new(xb, BELT_TOP, z0), Vec3::Y, cb);
        g.vertex(Vec3::new(xa, BELT_TOP, z0), Vec3::Y, ca);
        g.idx
            .extend_from_slice(&[i0, i0 + 1, i0 + 2, i0, i0 + 2, i0 + 3]);
    }

    // Chevron cleats every quarter tile (phase locked to the tile grid).
    for k in -4..=4 {
        let zc = -0.375 + 0.25 * k as f32;
        if zc - 0.07 < z0 || zc + 0.07 > z1 {
            continue;
        }
        chevron_cleat(g, zc, cleat);
    }

    if rollers {
        for (ze, s) in [(z0, 1.0f32), (z1, -1.0)] {
            // Thin rubber end face, the drum below it and its axle hubs.
            flat(
                g,
                [
                    Vec3::new(-BELT_HALF, BELT_TOP, ze),
                    Vec3::new(BELT_HALF, BELT_TOP, ze),
                    Vec3::new(BELT_HALF, BELT_TOP - 0.012, ze),
                    Vec3::new(-BELT_HALF, BELT_TOP - 0.012, ze),
                ],
                Vec3::Z * -s,
                scale(rub, 0.8),
            );
            let zc = ze + s * 0.034;
            g.cylinder_rot(
                Vec3::new(-BELT_HALF, 0.059, zc),
                0.034,
                2.0 * BELT_HALF,
                10,
                Quat::from_rotation_z(-FRAC_PI_2),
                lin(0x8d96a1),
            );
            for sx in [-1.0f32, 1.0] {
                g.cylinder_rot(
                    Vec3::new(sx * 0.466, 0.06, zc),
                    0.017,
                    0.008,
                    6,
                    Quat::from_rotation_arc(Vec3::Y, Vec3::X * sx),
                    lin(0x8d96a1),
                );
            }
        }
    }

    // Support leg at the tile middle: a foot plate and a bracket on each frame.
    if z0 < -0.08 && z1 > 0.08 {
        for sx in [-1.0f32, 1.0] {
            g.cuboid(
                Vec3::new(sx * 0.452, 0.007, 0.0),
                Vec3::new(0.07, 0.014, 0.15),
                scale(frame, 0.75),
            );
            g.cuboid(
                Vec3::new(sx * 0.471, 0.05, 0.0),
                Vec3::new(0.016, 0.07, 0.05),
                scale(frame, 1.2),
            );
        }
    }

    // Lightening slots along the outer frame faces.
    for sx in [-1.0f32, 1.0] {
        let x = sx * 0.4665;
        for zc in [-0.25f32, 0.25] {
            if zc - 0.09 < z0 || zc + 0.09 > z1 {
                continue;
            }
            flat(
                g,
                [
                    Vec3::new(x, 0.052, zc - 0.08),
                    Vec3::new(x, 0.052, zc + 0.08),
                    Vec3::new(x, 0.07, zc + 0.08),
                    Vec3::new(x, 0.07, zc - 0.08),
                ],
                Vec3::X * sx,
                lin(0x111418),
            );
        }
    }
}

/// One V-shaped cleat on the belt, apex pointing north (-Z), centred at `zc`.
fn chevron_cleat(g: &mut Geo, zc: f32, col: Rgba) {
    let (y0, y1) = (BELT_TOP, BELT_TOP + 0.005);
    let (w, d, t) = (0.315, 0.055, 0.03);
    for sx in [-1.0f32, 1.0] {
        let apex = Vec3::new(0.0, y1, zc - d);
        let end = Vec3::new(sx * w, y1, zc + d);
        let dz = Vec3::Z * t;
        flat(g, [apex, end, end + dz, apex + dz], Vec3::Y, col);
        // Leading (north) face catches light; trailing face is in shadow.
        let down = Vec3::Y * (y0 - y1);
        let lead_n = Vec3::new(sx * 0.4, 0.0, -1.0).normalize();
        flat(
            g,
            [apex, end, end + down, apex + down],
            lead_n,
            scale(col, 1.2),
        );
        flat(
            g,
            [apex + dz, end + dz, end + dz + down, apex + dz + down],
            -lead_n,
            scale(col, 0.6),
        );
    }
}

/// Belt stub for building ports: a belt run whose colours cancel the building AO, so it
/// matches the neighbouring belt tiles exactly.
fn belt_stub(g: &mut Geo, z0: f32, z1: f32, cap: Rgba, rot: Quat) {
    let mut b = Geo::fine();
    belt_run(&mut b, z0, z1, cap, lin(0x3c434c), false);
    un_ao(&mut b, BUILDING_AO);
    g.append(&b, &Transform::from_rotation(rot));
}

/// Undo `Geo::ao(height)` in advance (the renderer darkens building layers near the
/// ground; belt tiles are not darkened).
fn un_ao(g: &mut Geo, height: f32) {
    for (p, c) in g.pos.iter().zip(g.col.iter_mut()) {
        let t = (p[1] / height).clamp(0.0, 1.0);
        let k = 0.66 + 0.34 * (t * t * (3.0 - 2.0 * t));
        c[0] /= k;
        c[1] /= k;
        c[2] /= k;
    }
}

// ---------------------------------------------------------------------------------------
// Inserters

/// Pick-and-place arm on a turntable. The arm is a part pivoting about Y; at angle 0 it
/// holds over the front (-Z) tile, at PI over the back tile. Basic: yellow single boom
/// with a lift ram and counterweight. Fast: blue/white two-segment articulated arm.
fn inserter(m: &mut Model, fast: bool) {
    let accent = lin(if fast { BLUE } else { YELLOW });
    let plate = lin(PLATE);

    // Plinth with a hazard band on the drop side and a direction arrow.
    m.body.cuboid(
        Vec3::new(0.0, 0.03, 0.0),
        Vec3::new(0.86, 0.06, 0.86),
        plate,
    );
    hazard(
        &mut m.body,
        Vec3::new(-0.39, 0.0605, -0.415),
        Vec3::X,
        Vec3::Z,
        Vec3::Y,
        0.78,
        0.065,
        0.075,
        lin(YELLOW),
        lin(HAZ),
    );
    arrow(
        &mut m.glow,
        Vec3::new(0.0, 0.062, -0.37),
        Vec3::NEG_Z,
        0.1,
        scale(lin(if fast { CYAN } else { GREEN }), 0.9),
    );
    // Pickup side: a dark recessed strip with two small intake lights.
    m.body.cuboid(
        Vec3::new(0.0, 0.0615, 0.385),
        Vec3::new(0.5, 0.004, 0.05),
        lin(0x1a1e23),
    );
    for x in [-0.18f32, 0.18] {
        m.glow.cuboid(
            Vec3::new(x, 0.064, 0.385),
            Vec3::new(0.05, 0.004, 0.022),
            scale(lin(if fast { CYAN } else { ORANGE }), 0.7),
        );
    }
    // Corner bolts (the fourth corner holds the status lamp).
    for (x, z) in [(-0.36f32, -0.36f32), (0.36, -0.36), (-0.36, 0.36)] {
        bolt(
            &mut m.metal,
            Vec3::new(x, 0.06, z),
            Vec3::Y,
            0.022,
            lin(BRIGHT),
        );
    }
    m.body
        .cylinder(Vec3::new(0.34, 0.06, 0.34), 0.055, 0.045, 8, lin(0x23282f));
    m.lamp = Vec3::new(0.34, 0.135, 0.34);

    // Static turntable housing: dark skirt, accent band, bolted slewing ring.
    let dark = lin(FRAME);
    let top = if fast { lin(WHITE) } else { accent };
    lathe(
        &mut m.body,
        Vec3::new(0.0, 0.06, 0.0),
        &[
            (0.335, 0.0, scale(dark, 0.7)),
            (0.335, 0.03, dark),
            (0.335, 0.03, scale(accent, 0.8)),
            (0.33, 0.045, accent),
            (0.33, 0.045, scale(top, 0.95)),
            (0.31, 0.066, scale(top, 1.1)),
            (0.275, 0.072, top),
            (0.275, 0.072, scale(dark, 0.6)),
            (0.0, 0.072, scale(dark, 0.6)),
        ],
        28,
    );
    lathe(
        &mut m.metal,
        Vec3::new(0.0, 0.132, 0.0),
        &[
            (0.272, 0.0, lin(GUNMETAL)),
            (0.272, 0.008, lin(BRIGHT)),
            (0.255, 0.012, lin(BRIGHT)),
            (0.25, 0.012, lin(GUNMETAL)),
        ],
        28,
    );
    for k in 0..8 {
        let a = (k as f32 + 0.5) / 8.0 * TAU;
        bolt(
            &mut m.metal,
            Vec3::new(a.cos() * 0.3, 0.123, a.sin() * 0.3),
            Vec3::new(a.cos() * 0.35, 1.0, a.sin() * 0.35).normalize(),
            0.014,
            lin(BRIGHT),
        );
    }
    if fast {
        lathe(
            &mut m.glow,
            Vec3::new(0.0, 0.06, 0.0),
            &[
                (0.337, 0.032, scale(lin(CYAN), 1.3)),
                (0.337, 0.042, scale(lin(CYAN), 1.3)),
            ],
            28,
        );
    }
    // Power feed: a junction box at the back with a cable into the housing.
    m.body.cuboid(
        Vec3::new(-0.3, 0.1, 0.3),
        Vec3::new(0.12, 0.08, 0.1),
        lin(0x2c3239),
    );
    m.glow.cuboid(
        Vec3::new(-0.3, 0.115, 0.3505),
        Vec3::new(0.05, 0.015, 0.002),
        scale(lin(if fast { CYAN } else { GREEN }), 0.8),
    );
    tube(
        &mut m.rough,
        &[
            Vec3::new(-0.3, 0.1, 0.25),
            Vec3::new(-0.27, 0.075, 0.2),
            Vec3::new(-0.23, 0.08, 0.17),
            Vec3::new(-0.21, 0.1, 0.17),
        ],
        0.016,
        6,
        lin(0x1a1d21),
    );

    let pivot = Vec3::new(0.0, 0.144, 0.0);
    let arm = if fast { fast_arm() } else { basic_arm() };
    m.parts.push(Part { pivot, ..arm });
}

/// Basic inserter arm (part-local, pivot at the turntable top).
fn basic_arm() -> Part {
    let mut p = Part::new(Vec3::ZERO, Anim::Arm);
    let yellow = lin(YELLOW);
    let dark = lin(0x2c323a);
    // Rotating turret disc.
    lathe(
        &mut p.body,
        Vec3::ZERO,
        &[
            (0.245, 0.0, scale(dark, 0.7)),
            (0.245, 0.03, dark),
            (0.23, 0.045, scale(dark, 1.25)),
            (0.0, 0.045, scale(dark, 1.25)),
        ],
        24,
    );
    // Shoulder cheeks either side of the boom root.
    for sx in [-1.0f32, 1.0] {
        p.body.cuboid(
            Vec3::new(sx * 0.1, 0.16, 0.02),
            Vec3::new(0.05, 0.23, 0.22),
            yellow,
        );
        hazard(
            &mut p.body,
            Vec3::new(sx * 0.1251, 0.07, -0.08),
            Vec3::Z,
            Vec3::Y,
            Vec3::X * sx,
            0.2,
            0.04,
            0.04,
            lin(YELLOW),
            lin(HAZ),
        );
    }
    // Drive motor on the right cheek, shoulder pin through both.
    p.metal.cylinder_rot(
        Vec3::new(0.125, 0.24, 0.02),
        0.062,
        0.075,
        12,
        Quat::from_rotation_z(-FRAC_PI_2),
        lin(GUNMETAL),
    );
    p.body.cylinder_rot(
        Vec3::new(0.2, 0.24, 0.02),
        0.05,
        0.03,
        12,
        Quat::from_rotation_z(-FRAC_PI_2),
        dark,
    );
    rod(
        &mut p.metal,
        Vec3::new(-0.15, 0.24, 0.02),
        Vec3::new(0.125, 0.24, 0.02),
        0.032,
        10,
        lin(BRIGHT),
    );
    bolt(
        &mut p.metal,
        Vec3::new(-0.15, 0.24, 0.02),
        Vec3::NEG_X,
        0.03,
        lin(BRIGHT),
    );

    // Boom: from a counterweight behind the shoulder up to the wrist over the front tile.
    let shoulder = Vec3::new(0.0, 0.24, 0.02);
    let wrist = Vec3::new(0.0, 0.6, -0.39);
    let dir = (wrist - shoulder).normalize();
    let back = shoulder - dir * 0.17;
    beam(&mut p.body, back, wrist + dir * 0.03, 0.12, 0.09, yellow);
    // Dark top cover and a cable along it.
    let up = Vec3::X.cross(dir).normalize();
    beam(
        &mut p.body,
        shoulder + dir * 0.06 + up * 0.048,
        wrist - dir * 0.07 + up * 0.048,
        0.07,
        0.012,
        lin(0x30363e),
    );
    hazard(
        &mut p.body,
        wrist - dir * 0.2 + Vec3::X * 0.0605 - up * 0.045,
        dir,
        up,
        Vec3::X,
        0.12,
        0.09,
        0.045,
        lin(YELLOW),
        lin(HAZ),
    );
    // Counterweight block.
    let cw_rot = basis(dir, Vec3::X);
    p.body.cuboid_rot(
        back + dir * 0.02 - up * 0.01,
        Vec3::new(0.17, 0.13, 0.11),
        cw_rot,
        lin(0x3a4049),
    );
    p.metal.cuboid_rot(
        back + dir * 0.02 - up * 0.01 - dir * 0.06,
        Vec3::new(0.12, 0.08, 0.012),
        cw_rot,
        lin(GUNMETAL),
    );

    // Lift ram from the turret to the boom underside.
    let anchor = Vec3::new(0.0, 0.05, -0.15);
    let on_boom = shoulder + dir * 0.2 - up * 0.045;
    p.metal
        .cuboid(anchor, Vec3::new(0.08, 0.04, 0.06), lin(GUNMETAL));
    let mid = anchor.lerp(on_boom, 0.55);
    rod(&mut p.body, anchor, mid, 0.034, 10, lin(0x30363e));
    rod(&mut p.metal, mid, on_boom, 0.018, 8, lin(BRIGHT));
    rod(
        &mut p.metal,
        mid - (mid - anchor).normalize() * 0.01,
        mid + (on_boom - mid).normalize() * 0.012,
        0.04,
        10,
        lin(GUNMETAL),
    );

    // Hose along the boom top into the wrist.
    let hose: Vec<Vec3> = (0..=6)
        .map(|k| {
            let t = k as f32 / 6.0;
            shoulder.lerp(wrist, t) + up * (0.062 + 0.02 * (t * PI).sin()) + Vec3::X * 0.035
        })
        .collect();
    tube(&mut p.body, &hose, 0.012, 5, lin(0x1c2025));

    gripper(&mut p, wrist, false);
    p
}

/// Fast inserter arm: sleeker, white and blue, two segments with glowing seams.
fn fast_arm() -> Part {
    let mut p = Part::new(Vec3::ZERO, Anim::Arm);
    let white = lin(WHITE);
    let blue = lin(BLUE);
    let cyan = scale(lin(CYAN), 1.6);
    // Domed turret.
    lathe(
        &mut p.body,
        Vec3::ZERO,
        &[
            (0.245, 0.0, scale(blue, 0.6)),
            (0.245, 0.03, blue),
            (0.245, 0.03, white),
            (0.225, 0.07, white),
            (0.17, 0.105, scale(white, 1.05)),
            (0.08, 0.12, scale(white, 1.1)),
            (0.0, 0.123, scale(white, 1.1)),
        ],
        24,
    );
    lathe(
        &mut p.glow,
        Vec3::ZERO,
        &[(0.2, 0.086, cyan), (0.17, 0.106, cyan)],
        24,
    );
    // Shoulder yoke.
    let shoulder = Vec3::new(0.0, 0.2, 0.05);
    for sx in [-1.0f32, 1.0] {
        p.body.cuboid(
            Vec3::new(sx * 0.08, 0.17, 0.05),
            Vec3::new(0.035, 0.14, 0.13),
            blue,
        );
    }
    rod(
        &mut p.metal,
        shoulder - Vec3::X * 0.12,
        shoulder + Vec3::X * 0.12,
        0.05,
        12,
        lin(BRIGHT),
    );
    for sx in [-1.0f32, 1.0] {
        p.glow.cylinder_rot(
            shoulder + Vec3::X * sx * 0.12,
            0.03,
            0.006,
            10,
            Quat::from_rotation_arc(Vec3::Y, Vec3::X * sx),
            cyan,
        );
    }

    // Upper arm (white) leaning back, forearm (blue) reaching over the front tile.
    let elbow = Vec3::new(0.0, 0.6, 0.13);
    let wrist = Vec3::new(0.0, 0.6, -0.39);
    beam(&mut p.body, shoulder, elbow, 0.085, 0.07, white);
    let d1 = (elbow - shoulder).normalize();
    for sx in [-1.0f32, 1.0] {
        beam(
            &mut p.glow,
            shoulder + d1 * 0.08 + Vec3::X * sx * 0.0435,
            elbow - d1 * 0.08 + Vec3::X * sx * 0.0435,
            0.003,
            0.018,
            cyan,
        );
        beam(
            &mut p.body,
            shoulder + d1 * 0.05 + Vec3::X * sx * 0.045,
            elbow - d1 * 0.05 + Vec3::X * sx * 0.045,
            0.006,
            0.05,
            blue,
        );
    }
    rod(
        &mut p.metal,
        elbow - Vec3::X * 0.07,
        elbow + Vec3::X * 0.07,
        0.048,
        12,
        lin(BRIGHT),
    );
    for sx in [-1.0f32, 1.0] {
        p.body.cylinder_rot(
            elbow + Vec3::X * sx * 0.07,
            0.04,
            0.012,
            10,
            Quat::from_rotation_arc(Vec3::Y, Vec3::X * sx),
            blue,
        );
    }
    let d2 = (wrist - elbow).normalize();
    beam(&mut p.body, elbow + d2 * 0.02, wrist, 0.07, 0.06, blue);
    let up2 = Vec3::X.cross(d2).normalize();
    beam(
        &mut p.body,
        elbow + d2 * 0.07 + up2 * 0.031,
        wrist - d2 * 0.06 + up2 * 0.031,
        0.05,
        0.006,
        white,
    );
    beam(
        &mut p.glow,
        elbow + d2 * 0.1 + up2 * 0.0345,
        wrist - d2 * 0.1 + up2 * 0.0345,
        0.012,
        0.002,
        cyan,
    );
    // Service cable from the turret to the elbow.
    tube(
        &mut p.body,
        &sag(
            Vec3::new(-0.05, 0.11, 0.14),
            elbow + Vec3::new(-0.05, -0.03, 0.03),
            0.04,
            6,
        ),
        0.011,
        5,
        lin(0x1c2025),
    );

    gripper(&mut p, wrist, true);
    p
}

/// Wrist knuckle, rotator and a two-finger claw hanging over the held item.
fn gripper(p: &mut Part, wrist: Vec3, fast: bool) {
    let dark = lin(0x2a3037);
    rod(
        &mut p.metal,
        wrist - Vec3::X * 0.065,
        wrist + Vec3::X * 0.065,
        0.04,
        10,
        lin(BRIGHT),
    );
    let c = Vec3::new(0.0, 0.0, -0.42);
    p.metal.cylinder(
        Vec3::new(c.x, 0.51, c.z),
        0.032,
        wrist.y - 0.51,
        10,
        lin(GUNMETAL),
    );
    // Gripper head.
    p.body.cuboid(
        Vec3::new(c.x, 0.5, c.z),
        Vec3::new(0.27, 0.05, 0.1),
        if fast { lin(WHITE) } else { dark },
    );
    p.metal.cuboid(
        Vec3::new(c.x, 0.468, c.z),
        Vec3::new(0.25, 0.016, 0.06),
        lin(GUNMETAL),
    );
    for sx in [-1.0f32, 1.0] {
        // Finger: a slide block, a straight blade and an inward-bent tip.
        p.metal.cuboid(
            Vec3::new(sx * 0.12, 0.47, c.z),
            Vec3::new(0.035, 0.03, 0.08),
            lin(BRIGHT),
        );
        p.body.cuboid(
            Vec3::new(sx * 0.125, 0.395, c.z),
            Vec3::new(0.022, 0.13, 0.06),
            if fast { lin(BLUE) } else { lin(YELLOW) },
        );
        p.metal.cuboid_rot(
            Vec3::new(sx * 0.112, 0.315, c.z),
            Vec3::new(0.02, 0.05, 0.055),
            Quat::from_rotation_z(sx * 0.45),
            lin(BRIGHT),
        );
        if fast {
            p.glow.cuboid(
                Vec3::new(sx * 0.1135, 0.4, c.z),
                Vec3::new(0.002, 0.06, 0.02),
                scale(lin(CYAN), 1.6),
            );
        }
    }
}

// ---------------------------------------------------------------------------------------
// Depot

/// Storage depot (2x2): a corrugated warehouse with a roll-up door at the back of the
/// lot, a container yard in front of it and a gantry crane over the yard.
fn depot(m: &mut Model, s: f32) {
    let h = s / 2.0 - 0.04;
    foundation(m, h);
    let panel = lin(PANEL);
    let dark = lin(FRAME);
    let yellow = lin(YELLOW);

    // Yard floor markings: painted bay lines.
    for x in [-0.82f32, -0.25, 0.3] {
        m.body.cuboid(
            Vec3::new(x, 0.081, 0.42),
            Vec3::new(0.025, 0.003, 0.84),
            scale(yellow, 0.9),
        );
    }

    // Warehouse.
    let (wz0, wz1) = (-0.9f32, -0.12f32);
    let (wy0, wy1) = (0.08f32, 0.86f32);
    let wc = Vec3::new(0.0, (wy0 + wy1) / 2.0, (wz0 + wz1) / 2.0);
    m.body
        .cuboid(wc, Vec3::new(1.78, wy1 - wy0, wz1 - wz0), scale(panel, 0.9));
    // Corrugation ribs on the yard-facing wall and the east wall.
    for k in 0..17 {
        let x = -0.84 + k as f32 * 0.105;
        if x.abs() < 0.4 {
            continue;
        }
        m.body.cuboid(
            Vec3::new(x, (wy0 + wy1) / 2.0 - 0.02, wz1 + 0.006),
            Vec3::new(0.03, wy1 - wy0 - 0.12, 0.014),
            panel,
        );
    }
    for sx in [-1.0f32, 1.0] {
        for k in 0..7 {
            let z = wz0 + 0.08 + k as f32 * 0.105;
            m.body.cuboid(
                Vec3::new(sx * 0.896, (wy0 + wy1) / 2.0 - 0.02, z),
                Vec3::new(0.014, wy1 - wy0 - 0.12, 0.03),
                panel,
            );
        }
    }
    // Corner columns and a yellow eave band.
    for (x, z) in [(-0.9f32, wz1), (0.9, wz1), (-0.9, wz0), (0.9, wz0)] {
        m.body.cuboid(
            Vec3::new(x.clamp(-0.89, 0.89), (wy0 + wy1) / 2.0, z),
            Vec3::new(0.07, wy1 - wy0 + 0.02, 0.07),
            dark,
        );
    }
    m.body.cuboid(
        Vec3::new(0.0, wy1 - 0.035, wz1 + 0.012),
        Vec3::new(1.74, 0.05, 0.03),
        yellow,
    );
    // Roof: parapet, membrane, skylight, vents and a rooftop beacon.
    let roof_y = wy1;
    m.rough.cuboid(
        Vec3::new(0.0, roof_y + 0.005, wc.z),
        Vec3::new(1.7, 0.012, wz1 - wz0 - 0.08),
        lin(0x5b5e63),
    );
    for (c, sz) in [
        (
            Vec3::new(0.0, roof_y + 0.03, wz1 - 0.02),
            Vec3::new(1.8, 0.06, 0.05),
        ),
        (
            Vec3::new(0.0, roof_y + 0.03, wz0 + 0.02),
            Vec3::new(1.8, 0.06, 0.05),
        ),
        (
            Vec3::new(0.88, roof_y + 0.03, wc.z),
            Vec3::new(0.05, 0.06, 0.74),
        ),
        (
            Vec3::new(-0.88, roof_y + 0.03, wc.z),
            Vec3::new(0.05, 0.06, 0.74),
        ),
    ] {
        m.body.cuboid(c, sz, scale(panel, 0.8));
    }
    // Skylight strip with a steel frame.
    m.metal.cuboid(
        Vec3::new(-0.3, roof_y + 0.03, wc.z),
        Vec3::new(0.86, 0.04, 0.3),
        lin(GUNMETAL),
    );
    m.glass.cuboid(
        Vec3::new(-0.3, roof_y + 0.055, wc.z),
        Vec3::new(0.8, 0.02, 0.24),
        lin(0x9fd6ff),
    );
    for k in 0..5 {
        let x = -0.66 + k as f32 * 0.18;
        m.metal.cuboid(
            Vec3::new(x, roof_y + 0.068, wc.z),
            Vec3::new(0.012, 0.012, 0.25),
            lin(BRIGHT),
        );
    }
    // Rooftop vents.
    for (x, z) in [(0.42f32, -0.66f32), (0.66, -0.66), (0.42, -0.36)] {
        m.metal.cylinder(
            Vec3::new(x, roof_y + 0.01, z),
            0.07,
            0.08,
            10,
            lin(GUNMETAL),
        );
        m.metal
            .cylinder(Vec3::new(x, roof_y + 0.1, z), 0.085, 0.02, 10, lin(BRIGHT));
        m.body.cylinder(
            Vec3::new(x, roof_y + 0.09, z),
            0.06,
            0.012,
            10,
            lin(0x15181c),
        );
    }
    // Painted crate glyph on the roof.
    roof_glyph(m, Vec3::new(0.62, roof_y + 0.0115, -0.36));

    // Roll-up door facing the yard (+Z), partly open with a warm-lit interior.
    let dz = wz1 + 0.012;
    let (dx0, dx1, dy0, dy1) = (-0.34f32, 0.34f32, 0.08f32, 0.64f32);
    m.body.cuboid(
        Vec3::new(0.0, (dy0 + dy1) / 2.0 + 0.03, dz),
        Vec3::new(dx1 - dx0 + 0.14, dy1 - dy0 + 0.06, 0.03),
        dark,
    );
    for sx in [-1.0f32, 1.0] {
        hazard(
            &mut m.body,
            Vec3::new(sx * (dx1 + 0.035), dy0, dz + 0.016),
            Vec3::Y,
            Vec3::X * sx,
            Vec3::Z,
            dy1 - dy0,
            0.03,
            0.06,
            lin(YELLOW),
            lin(HAZ),
        );
    }
    let open_y = dy0 + 0.13;
    // Interior glimpse below the door.
    flat(
        &mut m.body,
        [
            Vec3::new(dx0, dy0, dz + 0.017),
            Vec3::new(dx1, dy0, dz + 0.017),
            Vec3::new(dx1, open_y, dz + 0.017),
            Vec3::new(dx0, open_y, dz + 0.017),
        ],
        Vec3::Z,
        lin(0x0e1013),
    );
    m.glow.cuboid(
        Vec3::new(0.0, open_y - 0.012, dz + 0.016),
        Vec3::new(dx1 - dx0 - 0.04, 0.012, 0.004),
        scale(lin(0xffb860), 0.9),
    );
    // Slats.
    let slats = 9;
    for k in 0..slats {
        let y0 = open_y + (dy1 - open_y) * k as f32 / slats as f32;
        let y1 = open_y + (dy1 - open_y) * (k + 1) as f32 / slats as f32;
        let ym = (y0 + y1) / 2.0;
        let z = dz + 0.018;
        let n_up = Vec3::new(0.0, 0.5, 1.0).normalize();
        let n_dn = Vec3::new(0.0, -0.5, 1.0).normalize();
        let c = lin(0xb9c2cc);
        flat(
            &mut m.metal,
            [
                Vec3::new(dx0, y0, z),
                Vec3::new(dx1, y0, z),
                Vec3::new(dx1, ym, z + 0.008),
                Vec3::new(dx0, ym, z + 0.008),
            ],
            n_dn,
            scale(c, 0.75),
        );
        flat(
            &mut m.metal,
            [
                Vec3::new(dx0, ym, z + 0.008),
                Vec3::new(dx1, ym, z + 0.008),
                Vec3::new(dx1, y1, z),
                Vec3::new(dx0, y1, z),
            ],
            n_up,
            c,
        );
    }
    // Bottom bar, roll housing and a door-status light.
    m.body.cuboid(
        Vec3::new(0.0, open_y + 0.01, dz + 0.03),
        Vec3::new(dx1 - dx0, 0.025, 0.025),
        yellow,
    );
    m.metal.cylinder_rot(
        Vec3::new(dx0 - 0.05, dy1 + 0.06, dz + 0.04),
        0.05,
        dx1 - dx0 + 0.1,
        14,
        Quat::from_rotation_z(-FRAC_PI_2),
        lin(GUNMETAL),
    );
    m.glow.cuboid(
        Vec3::new(dx1 + 0.12, 0.5, dz + 0.02),
        Vec3::new(0.05, 0.05, 0.01),
        scale(lin(GREEN), 1.2),
    );
    // Small lit windows on the east wall.
    for z in [-0.72f32, -0.5, -0.28] {
        m.glow.cuboid(
            Vec3::new(0.905, 0.7, z),
            Vec3::new(0.01, 0.06, 0.12),
            scale(lin(0xbfe6ff), 0.7),
        );
    }
    // Belt intakes on the west and east walls.
    for sx in [-1.0f32, 1.0] {
        let x = sx * 0.9;
        m.body.cuboid(
            Vec3::new(x + sx * 0.03, 0.17, -0.5),
            Vec3::new(0.06, 0.2, 0.34),
            yellow,
        );
        flat(
            &mut m.body,
            [
                Vec3::new(x + sx * 0.0605, 0.1, -0.62),
                Vec3::new(x + sx * 0.0605, 0.1, -0.38),
                Vec3::new(x + sx * 0.0605, 0.22, -0.38),
                Vec3::new(x + sx * 0.0605, 0.22, -0.62),
            ],
            Vec3::X * sx,
            lin(0x0e1013),
        );
        for k in 0..5 {
            let z = -0.6 + k as f32 * 0.05;
            m.rough.cuboid(
                Vec3::new(x + sx * 0.058, 0.17, z),
                Vec3::new(0.006, 0.1, 0.04),
                lin(0x202328),
            );
        }
    }

    // Container yard.
    container(
        m,
        Vec3::new(-0.55, 0.08, 0.4),
        Vec3::new(0.3, 0.27, 0.66),
        lin(0xb4582c),
    );
    container(
        m,
        Vec3::new(-0.55, 0.35, 0.36),
        Vec3::new(0.3, 0.27, 0.66),
        lin(0x2f7f86),
    );
    container(
        m,
        Vec3::new(-0.06, 0.08, 0.52),
        Vec3::new(0.3, 0.27, 0.66),
        lin(0x3d68a8),
    );
    // Palletised crates.
    for (x, z, y, c) in [
        (0.47f32, 0.2f32, 0.0f32, 0xa8763c),
        (0.73, 0.2, 0.0, 0x8c6232),
        (0.47, 0.46, 0.0, 0x8c6232),
        (0.73, 0.46, 0.0, 0xb08042),
        (0.47, 0.2, 0.22, 0x9a6c38),
        (0.6, 0.72, 0.0, 0xa8763c),
    ] {
        crate_box(m, Vec3::new(x, 0.08 + y, z), 0.2, lin(c));
    }

    // Gantry crane: rails, two portal legs and a truss girder with a hoist.
    let gz = 0.42f32;
    for sx in [-1.0f32, 1.0] {
        m.metal.cuboid(
            Vec3::new(sx * 0.86, 0.088, 0.4),
            Vec3::new(0.03, 0.016, 1.0),
            lin(BRIGHT),
        );
        let foot = Vec3::new(sx * 0.86, 0.1, gz);
        m.body.cuboid(foot, Vec3::new(0.07, 0.05, 0.22), dark);
        for dzl in [-0.09f32, 0.09] {
            beam(
                &mut m.body,
                foot + Vec3::new(0.0, 0.02, dzl),
                Vec3::new(sx * 0.86, 1.16, gz + dzl * 0.25),
                0.045,
                0.045,
                yellow,
            );
        }
        beam(
            &mut m.body,
            foot + Vec3::new(0.0, 0.5, -0.07),
            foot + Vec3::new(0.0, 0.5, 0.07),
            0.03,
            0.03,
            yellow,
        );
        // Floodlight on the leg top.
        m.body.cuboid(
            Vec3::new(sx * 0.86, 1.13, gz + 0.07),
            Vec3::new(0.06, 0.04, 0.03),
            dark,
        );
        m.glow.cuboid(
            Vec3::new(sx * 0.86, 1.13, gz + 0.0865),
            Vec3::new(0.045, 0.025, 0.004),
            scale(lin(0xfff1c8), 1.6),
        );
    }
    let gy = 1.18f32;
    for (dy, dzl) in [(0.0f32, -0.05f32), (0.0, 0.05), (-0.1, -0.05), (-0.1, 0.05)] {
        beam(
            &mut m.body,
            Vec3::new(-0.9, gy + dy, gz + dzl),
            Vec3::new(0.9, gy + dy, gz + dzl),
            0.035,
            0.035,
            yellow,
        );
    }
    for k in 0..9 {
        let x0 = -0.84 + k as f32 * 0.21;
        let x1 = x0 + 0.21;
        for dzl in [-0.05f32, 0.05] {
            let (ya, yb) = if k % 2 == 0 {
                (gy - 0.1, gy)
            } else {
                (gy, gy - 0.1)
            };
            rod(
                &mut m.metal,
                Vec3::new(x0, ya, gz + dzl),
                Vec3::new(x1, yb, gz + dzl),
                0.009,
                5,
                lin(0xc9a23a),
            );
        }
    }
    // Hazard stripes on the girder's camera-facing side.
    hazard(
        &mut m.body,
        Vec3::new(-0.3, gy - 0.12, gz + 0.069),
        Vec3::X,
        Vec3::Y,
        Vec3::Z,
        0.6,
        0.14,
        0.06,
        lin(YELLOW),
        lin(HAZ),
    );
    // Trolley, hoist ropes and spreader over the container stack.
    let tx = -0.55f32;
    m.body.cuboid(
        Vec3::new(tx, gy + 0.05, gz),
        Vec3::new(0.2, 0.08, 0.16),
        lin(0x2f353d),
    );
    m.glow.cuboid(
        Vec3::new(tx + 0.07, gy + 0.092, gz),
        Vec3::new(0.03, 0.004, 0.03),
        scale(lin(ORANGE), 1.5),
    );
    let spread_y = 0.74f32;
    for (dx, dzl) in [(-0.06f32, -0.05f32), (0.06, 0.05)] {
        rod(
            &mut m.metal,
            Vec3::new(tx + dx, spread_y + 0.02, gz + dzl),
            Vec3::new(tx + dx, gy - 0.1, gz + dzl),
            0.006,
            5,
            lin(0x2a2e33),
        );
    }
    m.body.cuboid(
        Vec3::new(tx, spread_y, 0.37),
        Vec3::new(0.28, 0.035, 0.6),
        yellow,
    );
    m.body.cuboid(
        Vec3::new(tx, spread_y + 0.03, 0.37),
        Vec3::new(0.12, 0.04, 0.12),
        dark,
    );

    m.lamp = Vec3::new(0.8, roof_y + 0.1, -0.2);
}

/// Concrete foundation with a hazard-striped kerb on the yard side and steel edging.
fn foundation(m: &mut Model, h: f32) {
    m.rough.cuboid(
        Vec3::new(0.0, 0.04, 0.0),
        Vec3::new(h * 2.0, 0.08, h * 2.0),
        lin(CONCRETE),
    );
    // Expansion joints.
    for k in [-0.5f32, 0.0, 0.5] {
        m.rough.cuboid(
            Vec3::new(k * h, 0.0805, 0.0),
            Vec3::new(0.012, 0.002, h * 2.0 - 0.06),
            scale(lin(CONCRETE), 0.6),
        );
    }
    hazard(
        &mut m.body,
        Vec3::new(-h + 0.03, 0.0815, h - 0.075),
        Vec3::X,
        Vec3::Z,
        Vec3::Y,
        h * 2.0 - 0.06,
        0.05,
        0.08,
        lin(YELLOW),
        lin(HAZ),
    );
    // Steel edge angle on the kerb.
    for sz in [-1.0f32, 1.0] {
        m.metal.cuboid(
            Vec3::new(0.0, 0.07, sz * (h - 0.006)),
            Vec3::new(h * 2.0, 0.025, 0.014),
            lin(GUNMETAL),
        );
    }
    for sx in [-1.0f32, 1.0] {
        m.metal.cuboid(
            Vec3::new(sx * (h - 0.006), 0.07, 0.0),
            Vec3::new(0.014, 0.025, h * 2.0),
            lin(GUNMETAL),
        );
    }
}

/// A shipping container: corrugated sides, end doors with lock bars, corner castings.
fn container(m: &mut Model, base: Vec3, size: Vec3, col: Rgba) {
    let c = base + Vec3::Y * size.y / 2.0;
    m.body.cuboid(c, size, col);
    let h = size / 2.0;
    // Corrugation: raised ribs along both long sides.
    let n = (size.z / 0.055) as i32;
    for sx in [-1.0f32, 1.0] {
        for k in 0..n {
            let z = c.z - h.z + 0.05 + k as f32 * (size.z - 0.1) / (n - 1) as f32;
            m.body.cuboid(
                Vec3::new(c.x + sx * (h.x + 0.004), c.y, z),
                Vec3::new(0.008, size.y - 0.05, 0.022),
                scale(col, 1.12),
            );
        }
    }
    // Top ribs.
    for k in 0..5 {
        let z = c.z - h.z + 0.1 + k as f32 * (size.z - 0.2) / 4.0;
        m.body.cuboid(
            Vec3::new(c.x, c.y + h.y + 0.003, z),
            Vec3::new(size.x - 0.05, 0.006, 0.02),
            scale(col, 1.1),
        );
    }
    // Doors on the +Z end.
    let ez = c.z + h.z + 0.003;
    flat(
        &mut m.body,
        [
            Vec3::new(c.x - 0.002, c.y - h.y + 0.02, ez),
            Vec3::new(c.x + 0.002, c.y - h.y + 0.02, ez),
            Vec3::new(c.x + 0.002, c.y + h.y - 0.02, ez),
            Vec3::new(c.x - 0.002, c.y + h.y - 0.02, ez),
        ],
        Vec3::Z,
        scale(col, 0.4),
    );
    for dx in [-0.09f32, -0.035, 0.035, 0.09] {
        rod(
            &mut m.metal,
            Vec3::new(c.x + dx * size.x / 0.3, c.y - h.y + 0.03, ez + 0.006),
            Vec3::new(c.x + dx * size.x / 0.3, c.y + h.y - 0.03, ez + 0.006),
            0.005,
            4,
            lin(BRIGHT),
        );
    }
    // Corner castings.
    for sx in [-1.0f32, 1.0] {
        for sy in [-1.0f32, 1.0] {
            for sz in [-1.0f32, 1.0] {
                m.metal.cuboid(
                    c + Vec3::new(sx * (h.x - 0.012), sy * (h.y - 0.012), sz * (h.z - 0.012)),
                    Vec3::new(0.03, 0.03, 0.03),
                    lin(0x3a3f46),
                );
            }
        }
    }
}

/// A strapped wooden crate on a pallet.
fn crate_box(m: &mut Model, base: Vec3, size: f32, col: Rgba) {
    m.rough.cuboid(
        base + Vec3::Y * 0.015,
        Vec3::new(size + 0.02, 0.03, size + 0.02),
        lin(0x6a5434),
    );
    let c = base + Vec3::Y * (0.03 + size / 2.0);
    m.body.cuboid(c, Vec3::splat(size), col);
    // Two straps over the top.
    for dx in [-0.05f32, 0.05] {
        m.metal.cuboid(
            c + Vec3::new(dx, 0.0, 0.0),
            Vec3::new(0.015, size + 0.006, size + 0.006),
            lin(0x464c54),
        );
    }
}

/// Painted logistics glyph (a box with an arrow into it) on a roof.
fn roof_glyph(m: &mut Model, c: Vec3) {
    let y = c.y;
    let col = scale(lin(YELLOW), 0.85);
    let ring = [(0.12f32, 0.012f32)];
    for &(r, t) in &ring {
        for (a, b) in [
            (Vec2::new(-r, -r), Vec2::new(r, -r + t)),
            (Vec2::new(-r, r - t), Vec2::new(r, r)),
            (Vec2::new(-r, -r), Vec2::new(-r + t, r)),
            (Vec2::new(r - t, -r), Vec2::new(r, r)),
        ] {
            flat(
                &mut m.body,
                [
                    Vec3::new(c.x + a.x, y, c.z + b.y),
                    Vec3::new(c.x + b.x, y, c.z + b.y),
                    Vec3::new(c.x + b.x, y, c.z + a.y),
                    Vec3::new(c.x + a.x, y, c.z + a.y),
                ],
                Vec3::Y,
                col,
            );
        }
    }
    arrow(
        &mut m.body,
        Vec3::new(c.x, y + 0.001, c.z),
        Vec3::Z,
        0.11,
        col,
    );
}

// ---------------------------------------------------------------------------------------
// Provider

/// Provider (1x1): pulls items from the planet network down a glass delivery tube and
/// pushes them out of a lit chute onto the belt in front.
fn provider(m: &mut Model) {
    let yellow = lin(YELLOW);
    let dark = lin(FRAME);
    let green = scale(lin(GREEN), 1.4);

    // Plinth.
    m.body.cuboid(
        Vec3::new(0.0, 0.03, 0.06),
        Vec3::new(0.86, 0.06, 0.74),
        lin(PLATE),
    );
    hazard(
        &mut m.body,
        Vec3::new(-0.39, 0.0605, 0.355),
        Vec3::X,
        Vec3::Z,
        Vec3::Y,
        0.78,
        0.05,
        0.07,
        lin(YELLOW),
        lin(HAZ),
    );
    // Output belt stub running out of the front.
    belt_stub(&mut m.body, -0.5, -0.2, yellow, Quat::IDENTITY);

    // Cabinet.
    let cab_c = Vec3::new(0.0, 0.2, 0.1);
    let cab_s = Vec3::new(0.74, 0.28, 0.5);
    m.body.cuboid(cab_c, cab_s, yellow);
    m.body
        .cuboid(Vec3::new(0.0, 0.36, 0.1), Vec3::new(0.7, 0.04, 0.46), dark);
    // Side grilles.
    for sx in [-1.0f32, 1.0] {
        for k in 0..5 {
            let z = -0.04 + k as f32 * 0.07;
            m.metal.cuboid(
                Vec3::new(sx * 0.373, 0.2, z),
                Vec3::new(0.01, 0.14, 0.022),
                lin(0x3c434c),
            );
        }
    }
    // Front: dispenser mouth over the belt with a lit throat and a hood.
    let mouth_z = cab_c.z - cab_s.z / 2.0;
    flat(
        &mut m.body,
        [
            Vec3::new(-0.28, 0.095, mouth_z - 0.002),
            Vec3::new(0.28, 0.095, mouth_z - 0.002),
            Vec3::new(0.28, 0.28, mouth_z - 0.002),
            Vec3::new(-0.28, 0.28, mouth_z - 0.002),
        ],
        Vec3::NEG_Z,
        lin(0x0c0e10),
    );
    m.glow.cuboid(
        Vec3::new(0.0, 0.27, mouth_z - 0.004),
        Vec3::new(0.5, 0.012, 0.004),
        green,
    );
    // Hood over the mouth with an arrow on top.
    m.body.cuboid(
        Vec3::new(0.0, 0.31, mouth_z - 0.07),
        Vec3::new(0.66, 0.035, 0.14),
        dark,
    );
    for sx in [-1.0f32, 1.0] {
        m.body.cuboid(
            Vec3::new(sx * 0.31, 0.2, mouth_z - 0.07),
            Vec3::new(0.04, 0.22, 0.14),
            dark,
        );
    }
    arrow(
        &mut m.glow,
        Vec3::new(0.0, 0.33, mouth_z - 0.07),
        Vec3::NEG_Z,
        0.11,
        green,
    );
    // Rubber curtain strips hanging in the mouth.
    for k in 0..6 {
        let x = -0.225 + k as f32 * 0.09;
        m.rough.cuboid(
            Vec3::new(x, 0.21, mouth_z - 0.008),
            Vec3::new(0.08, 0.1, 0.006),
            lin(0x2a2f35),
        );
    }

    // Delivery tube: glass cylinder with a glowing core and a floating crate.
    let tc = Vec3::new(0.0, 0.38, 0.12);
    lathe(
        &mut m.metal,
        tc,
        &[
            (0.2, 0.0, lin(GUNMETAL)),
            (0.2, 0.05, lin(GUNMETAL)),
            (0.19, 0.06, lin(BRIGHT)),
            (0.155, 0.06, lin(GUNMETAL)),
        ],
        24,
    );
    lathe(
        &mut m.glass,
        tc,
        &[(0.15, 0.05, lin(0xbfe8d0)), (0.15, 0.36, lin(0xbfe8d0))],
        24,
    );
    m.glow
        .cylinder(tc + Vec3::Y * 0.05, 0.035, 0.31, 10, scale(lin(GREEN), 1.1));
    lathe(
        &mut m.glow,
        tc,
        &[(0.13, 0.065, green), (0.1, 0.075, green)],
        20,
    );
    m.body.cuboid_rot(
        tc + Vec3::Y * 0.22,
        Vec3::splat(0.11),
        Quat::from_euler(EulerRot::YXZ, 0.5, 0.3, 0.2),
        lin(0xb07a3a),
    );
    // Guard bars around the glass.
    for k in 0..4 {
        let a = k as f32 / 4.0 * TAU + PI / 4.0;
        let p = tc + Vec3::new(a.cos() * 0.175, 0.0, a.sin() * 0.175);
        rod(
            &mut m.metal,
            p + Vec3::Y * 0.05,
            p + Vec3::Y * 0.37,
            0.012,
            6,
            lin(BRIGHT),
        );
    }
    // Cap with an uplink antenna.
    lathe(
        &mut m.body,
        tc + Vec3::Y * 0.36,
        &[
            (0.0, 0.0, scale(yellow, 0.5)),
            (0.2, 0.0, scale(yellow, 0.6)),
            (0.2, 0.04, yellow),
            (0.17, 0.07, scale(yellow, 1.1)),
            (0.17, 0.07, dark),
            (0.08, 0.09, dark),
            (0.0, 0.09, dark),
        ],
        24,
    );
    let top = tc + Vec3::Y * 0.45;
    rod(
        &mut m.metal,
        top,
        top + Vec3::new(0.0, 0.16, 0.0),
        0.012,
        6,
        lin(BRIGHT),
    );
    m.glow.sphere(
        top + Vec3::Y * 0.17,
        0.022,
        Vec3::ONE,
        6,
        scale(lin(GREEN), 2.0),
    );
    for k in 0..3 {
        let a = k as f32 / 3.0 * TAU;
        rod(
            &mut m.metal,
            top + Vec3::Y * 0.09,
            top + Vec3::new(a.cos() * 0.06, 0.06, a.sin() * 0.06),
            0.005,
            4,
            lin(BRIGHT),
        );
    }
    // Feed conduit from the tube cap down the back.
    tube(
        &mut m.rough,
        &[
            tc + Vec3::new(0.12, 0.42, 0.1),
            tc + Vec3::new(0.2, 0.42, 0.2),
            tc + Vec3::new(0.27, 0.3, 0.24),
            tc + Vec3::new(0.29, 0.0, 0.24),
            tc + Vec3::new(0.29, -0.1, 0.24),
        ],
        0.022,
        6,
        lin(0x23272c),
    );
    // Status screen on the cabinet's back.
    m.glow.cuboid(
        Vec3::new(-0.15, 0.22, 0.351),
        Vec3::new(0.2, 0.1, 0.004),
        scale(lin(0x2a8f5a), 0.9),
    );
    m.lamp = Vec3::new(-0.27, 0.42, 0.28);
}

// ---------------------------------------------------------------------------------------
// Splitter and merger

/// Belt routers (1x1). Both have belt stubs on all four sides, corner posts and port
/// hoods with glowing arrows. Splitter: white drum with a spinning distributor under a
/// glass dome, cyan arrows pointing out of the front, left and right ports. Merger:
/// yellow box with a funnel hopper, amber arrows pointing into the back, left and right.
fn router(m: &mut Model, splitter: bool) {
    let dark = lin(FRAME);
    let (shell, glow) = if splitter {
        (lin(PANEL), scale(lin(CYAN), 1.5))
    } else {
        (lin(YELLOW), scale(lin(ORANGE), 1.5))
    };
    // Base plate under everything.
    m.body.cuboid(
        Vec3::new(0.0, 0.02, 0.0),
        Vec3::new(0.92, 0.04, 0.92),
        lin(PLATE),
    );
    // Ports: (rotation of the -Z port, flow out of the router?)
    let ports: [(f32, bool); 4] = [
        (0.0, true),
        (PI, false),
        (FRAC_PI_2, splitter),
        (-FRAC_PI_2, splitter),
    ];
    for (a, out) in ports {
        let rot = Quat::from_rotation_y(a);
        belt_stub(&mut m.body, -0.5, -0.22, lin(0x8a929c), rot);
        port_hood(m, rot, out, shell, glow);
    }
    // Corner posts hide where the stub rails meet.
    for (sx, sz) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        let c = Vec3::new(sx * 0.425, 0.0, sz * 0.425);
        m.body
            .cuboid(c + Vec3::Y * 0.09, Vec3::new(0.13, 0.18, 0.13), dark);
        m.body
            .cuboid(c + Vec3::Y * 0.19, Vec3::new(0.11, 0.02, 0.11), shell);
        bolt(&mut m.metal, c + Vec3::Y * 0.2, Vec3::Y, 0.02, lin(BRIGHT));
    }

    if splitter {
        // Housing: square skirt then a round drum.
        m.body.cuboid(
            Vec3::new(0.0, 0.14, 0.0),
            Vec3::new(0.52, 0.2, 0.52),
            scale(shell, 0.92),
        );
        lathe(
            &mut m.body,
            Vec3::new(0.0, 0.24, 0.0),
            &[
                (0.0, 0.0, dark),
                (0.25, 0.0, dark),
                (0.25, 0.03, dark),
                (0.25, 0.03, shell),
                (0.245, 0.1, shell),
                (0.225, 0.13, scale(shell, 1.1)),
                (0.225, 0.13, dark),
                (0.19, 0.135, dark),
            ],
            28,
        );
        lathe(
            &mut m.glow,
            Vec3::new(0.0, 0.24, 0.0),
            &[(0.252, 0.04, glow), (0.252, 0.055, glow)],
            28,
        );
        // Glass dome over the distributor.
        let mut dome = Vec::new();
        for k in 0..=6 {
            let a = k as f32 / 6.0 * FRAC_PI_2;
            dome.push((0.19 * a.cos(), 0.135 + 0.13 * a.sin(), lin(0xcfefff)));
        }
        lathe(&mut m.glass, Vec3::new(0.0, 0.24, 0.0), &dome, 24);
        // Spinning three-vane distributor; vanes point at the three outlets.
        let mut rotor = Part::new(Vec3::new(0.0, 0.375, 0.0), Anim::Spin(3.0));
        rotor
            .metal
            .cylinder(Vec3::new(0.0, 0.0, 0.0), 0.045, 0.08, 10, lin(BRIGHT));
        for k in 0..3 {
            let a = k as f32 / 3.0 * TAU;
            let d = Vec3::new(a.sin(), 0.0, -a.cos());
            let rot = basis(d, Vec3::Y);
            rotor.body.cuboid_rot(
                d * 0.1 + Vec3::Y * 0.03,
                Vec3::new(0.02, 0.06, 0.15),
                rot,
                lin(0x2c3a4a),
            );
            rotor.glow.cuboid_rot(
                d * 0.16 + Vec3::Y * 0.03,
                Vec3::new(0.024, 0.064, 0.02),
                rot,
                glow,
            );
        }
        m.parts.push(rotor);
        m.lamp = Vec3::new(0.425, 0.27, 0.425);
    } else {
        // Housing: yellow box with a dark funnel hopper on top.
        m.body.cuboid(
            Vec3::new(0.0, 0.16, 0.0),
            Vec3::new(0.52, 0.24, 0.52),
            shell,
        );
        let (y0, y1) = (0.28f32, 0.4f32);
        let (r0, r1) = (0.13f32, 0.27f32);
        // Outer sloped walls of the hopper (4 quads), dark inner walls.
        for k in 0..4 {
            let a = k as f32 * FRAC_PI_2;
            let rot = Quat::from_rotation_y(a);
            let p = |x: f32, y: f32, z: f32| rot * Vec3::new(x, y, z);
            let n_out = rot * Vec3::new(0.0, (r1 - r0) / (y1 - y0), -1.0).normalize();
            let n_out = Vec3::new(n_out.x, -n_out.y.abs() * 0.0 + n_out.y, n_out.z);
            flat(
                &mut m.body,
                [
                    p(-r0, y0, -r0),
                    p(r0, y0, -r0),
                    p(r1, y1, -r1),
                    p(-r1, y1, -r1),
                ],
                n_out,
                scale(shell, 0.85),
            );
            // Inner wall (visible from above).
            let n_in = rot * Vec3::new(0.0, 1.0, (r1 - r0) / (y1 - y0)).normalize();
            flat(
                &mut m.body,
                [
                    p(-r0 + 0.02, y0 + 0.02, -r0 + 0.02),
                    p(r0 - 0.02, y0 + 0.02, -r0 + 0.02),
                    p(r1 - 0.025, y1, -r1 + 0.025),
                    p(-r1 + 0.025, y1, -r1 + 0.025),
                ],
                n_in,
                lin(0x22272e),
            );
            // Rim.
            m.metal.cuboid_rot(
                p(0.0, y1 + 0.006, -r1 + 0.012),
                Vec3::new(2.0 * r1, 0.016, 0.026),
                rot,
                lin(BRIGHT),
            );
            // Glowing inflow chevrons on the inner walls of the three inlets.
            if k != 0 {
                let mid = p(0.0, (y0 + y1) / 2.0 + 0.03, -(r0 + r1) / 2.0 + 0.017);
                let fwd = (rot * Vec3::Z).normalize();
                arrow_on(&mut m.glow, mid, fwd, n_in, 0.075, glow);
            }
        }
        // Hopper throat with the merge gate glowing below.
        flat(
            &mut m.glow,
            [
                Vec3::new(-r0 + 0.02, y0 + 0.021, r0 - 0.02),
                Vec3::new(r0 - 0.02, y0 + 0.021, r0 - 0.02),
                Vec3::new(r0 - 0.02, y0 + 0.021, -r0 + 0.02),
                Vec3::new(-r0 + 0.02, y0 + 0.021, -r0 + 0.02),
            ],
            Vec3::Y,
            scale(glow, 0.6),
        );
        arrow(
            &mut m.glow,
            Vec3::new(0.0, y0 + 0.024, 0.0),
            Vec3::NEG_Z,
            0.13,
            scale(glow, 1.4),
        );
        m.lamp = Vec3::new(0.425, 0.27, 0.425);
    }
}

/// Hood over one router port (built for the -Z side, then rotated by `rot`): an arch
/// over the belt with an arrow pointing along the flow.
fn port_hood(m: &mut Model, rot: Quat, out: bool, shell: Rgba, glow: Rgba) {
    let mut body = Geo::fine();
    let mut metal = Geo::fine();
    let mut lit = Geo::fine();
    let dark = lin(FRAME);
    let z = -0.3f32;
    // Arch: two jambs and a lintel standing proud of the housing face.
    for sx in [-1.0f32, 1.0] {
        body.cuboid(
            Vec3::new(sx * 0.21, 0.14, z),
            Vec3::new(0.05, 0.2, 0.1),
            dark,
        );
    }
    body.cuboid(Vec3::new(0.0, 0.255, z), Vec3::new(0.47, 0.05, 0.11), shell);
    hazard(
        &mut body,
        Vec3::new(-0.2, 0.232, z - 0.0555),
        Vec3::X,
        Vec3::Y,
        Vec3::NEG_Z,
        0.4,
        0.045,
        0.05,
        lin(YELLOW),
        lin(HAZ),
    );
    // Dark throat and rubber curtain.
    flat(
        &mut body,
        [
            Vec3::new(-0.185, BELT_TOP, z + 0.03),
            Vec3::new(0.185, BELT_TOP, z + 0.03),
            Vec3::new(0.185, 0.23, z + 0.03),
            Vec3::new(-0.185, 0.23, z + 0.03),
        ],
        Vec3::NEG_Z,
        lin(0x0c0e10),
    );
    for k in 0..4 {
        let x = -0.135 + k as f32 * 0.09;
        metal.cuboid(
            Vec3::new(x, 0.17, z + 0.02),
            Vec3::new(0.08, 0.11, 0.006),
            lin(0x30353c),
        );
    }
    // Flow arrow on the lintel.
    let dir = if out { Vec3::NEG_Z } else { Vec3::Z };
    arrow(&mut lit, Vec3::new(0.0, 0.2805, z), dir, 0.085, glow);
    let tf = Transform::from_rotation(rot);
    m.body.append(&body, &tf);
    m.metal.append(&metal, &tf);
    m.glow.append(&lit, &tf);
}

// ---------------------------------------------------------------------------------------
// Geometry helpers

type Vtx = (Vec3, Vec3, Rgba);

/// A planar quad with one normal; winding is chosen to face along `n`.
fn flat(g: &mut Geo, p: [Vec3; 4], n: Vec3, c: Rgba) {
    let gn = (p[2] - p[0]).cross(p[3] - p[1]);
    let i = g.pos.len() as u32;
    for q in p {
        g.vertex(q, n, c);
    }
    if gn.dot(n) >= 0.0 {
        g.idx.extend_from_slice(&[i, i + 1, i + 2, i, i + 2, i + 3]);
    } else {
        g.idx.extend_from_slice(&[i, i + 2, i + 1, i, i + 3, i + 2]);
    }
}

/// Connects consecutive rows of vertices into quads, each wound to face along its
/// vertex normals. Row pairs flagged in `skip` are left open (creases).
fn grid(g: &mut Geo, rows: &[Vec<Vtx>], skip: &[bool]) {
    let base: Vec<u32> = rows
        .iter()
        .map(|r| {
            let b = g.pos.len() as u32;
            for &(p, n, c) in r {
                g.vertex(p, n, c);
            }
            b
        })
        .collect();
    for r in 0..rows.len().saturating_sub(1) {
        if skip.get(r).copied().unwrap_or(false) {
            continue;
        }
        let w = rows[r].len().min(rows[r + 1].len());
        for k in 0..w - 1 {
            let (a, b) = (base[r] + k as u32, base[r] + k as u32 + 1);
            let (c, d) = (base[r + 1] + k as u32 + 1, base[r + 1] + k as u32);
            let (pa, pb, pc, pd) = (
                rows[r][k].0,
                rows[r][k + 1].0,
                rows[r + 1][k + 1].0,
                rows[r + 1][k].0,
            );
            let gn = (pc - pa).cross(pd - pb);
            let nn = rows[r][k].1 + rows[r][k + 1].1 + rows[r + 1][k + 1].1 + rows[r + 1][k].1;
            if gn.dot(nn) >= 0.0 {
                g.idx.extend_from_slice(&[a, b, c, a, c, d]);
            } else {
                g.idx.extend_from_slice(&[a, c, b, a, d, c]);
            }
        }
    }
}

/// Outward normals of a 2D profile. Traversal convention: the solid is on the left
/// (counter-clockwise for extrusions, bottom→top for lathes). Repeating a point makes a
/// crease (and lets the colour change there).
fn profile_normals(pts: &[(f32, f32)]) -> (Vec<Vec2>, Vec<bool>) {
    let n = pts.len();
    let same = |a: usize, b: usize| {
        (pts[a].0 - pts[b].0).abs() < 1e-6 && (pts[a].1 - pts[b].1).abs() < 1e-6
    };
    let seg = |i: usize| {
        let d = Vec2::new(pts[i + 1].0 - pts[i].0, pts[i + 1].1 - pts[i].1);
        Vec2::new(d.y, -d.x).normalize_or(Vec2::Y)
    };
    let mut nrm = Vec::with_capacity(n);
    for i in 0..n {
        let prev = (i > 0 && !same(i, i - 1)).then(|| seg(i - 1));
        let next = (i + 1 < n && !same(i, i + 1)).then(|| seg(i));
        nrm.push(match (prev, next) {
            (Some(a), Some(b)) => (a + b).normalize_or(a),
            (Some(a), None) => a,
            (None, Some(b)) => b,
            _ => Vec2::Y,
        });
    }
    let skip = (0..n.saturating_sub(1)).map(|i| same(i, i + 1)).collect();
    (nrm, skip)
}

/// Surface of revolution about the vertical axis through `c`; profile = (radius, height,
/// colour) from bottom to top.
fn lathe(g: &mut Geo, c: Vec3, prof: &[(f32, f32, Rgba)], segs: u32) {
    let pts: Vec<(f32, f32)> = prof.iter().map(|q| (q.0, q.1)).collect();
    let (nrm, skip) = profile_normals(&pts);
    let rows: Vec<Vec<Vtx>> = prof
        .iter()
        .zip(&nrm)
        .map(|(&(r, y, col), n)| {
            (0..=segs)
                .map(|k| {
                    let a = k as f32 / segs as f32 * TAU;
                    let d = Vec3::new(a.cos(), 0.0, a.sin());
                    (
                        c + d * r + Vec3::Y * y,
                        (d * n.x + Vec3::Y * n.y).normalize_or(Vec3::Y),
                        col,
                    )
                })
                .collect()
        })
        .collect();
    grid(g, &rows, &skip);
}

/// Extrudes an XY profile (counter-clockwise, x right, y up) along Z from `z0` to `z1`,
/// optionally capping both ends with a fan from the profile centroid.
fn extrude_z(g: &mut Geo, prof: &[(f32, f32, Rgba)], z0: f32, z1: f32, caps: bool) {
    let pts: Vec<(f32, f32)> = prof.iter().map(|q| (q.0, q.1)).collect();
    let (nrm, skip) = profile_normals(&pts);
    let rows: Vec<Vec<Vtx>> = prof
        .iter()
        .zip(&nrm)
        .map(|(&(x, y, col), n)| {
            let nn = Vec3::new(n.x, n.y, 0.0);
            vec![
                (Vec3::new(x, y, z0), nn, col),
                (Vec3::new(x, y, z1), nn, col),
            ]
        })
        .collect();
    grid(g, &rows, &skip);
    if caps {
        let cen = pts.iter().fold(Vec2::ZERO, |a, p| a + Vec2::new(p.0, p.1)) / pts.len() as f32;
        for (z, n) in [(z0, Vec3::NEG_Z), (z1, Vec3::Z)] {
            let mid = prof[prof.len() / 2].2;
            let ci = g.vertex(Vec3::new(cen.x, cen.y, z), n, scale(mid, 0.7));
            let first = g.pos.len() as u32;
            for &(x, y, col) in prof {
                g.vertex(Vec3::new(x, y, z), n, scale(col, 0.7));
            }
            let cnt = prof.len() as u32;
            for k in 0..cnt {
                let (a, b) = (first + k, first + (k + 1) % cnt);
                let (pa, pb) = (Vec3::from(g.pos[a as usize]), Vec3::from(g.pos[b as usize]));
                let pc = Vec3::new(cen.x, cen.y, z);
                if (pb - pa).length_squared() < 1e-10 {
                    continue;
                }
                if (pa - pc).cross(pb - pc).dot(n) >= 0.0 {
                    g.idx.extend_from_slice(&[ci, a, b]);
                } else {
                    g.idx.extend_from_slice(&[ci, b, a]);
                }
            }
        }
    }
}

/// Orientation whose local +Z runs along `fwd` and local +Y is as close to `up` as
/// possible.
fn basis(fwd: Vec3, up: Vec3) -> Quat {
    let z = fwd.normalize();
    let up = if z.cross(up).length_squared() < 1e-6 {
        Vec3::X
    } else {
        up
    };
    let x = up.cross(z).normalize();
    let y = z.cross(x);
    Quat::from_mat3(&Mat3::from_cols(x, y, z))
}

/// Box beam from `a` to `b` with cross-section `w` (sideways) × `h` (up).
fn beam(g: &mut Geo, a: Vec3, b: Vec3, w: f32, h: f32, col: Rgba) {
    let d = b - a;
    g.cuboid_rot(
        (a + b) / 2.0,
        Vec3::new(w, h, d.length()),
        basis(d, Vec3::Y),
        col,
    );
}

/// Cylinder from `a` to `b`.
fn rod(g: &mut Geo, a: Vec3, b: Vec3, r: f32, segs: u32, col: Rgba) {
    let d = b - a;
    g.cylinder_rot(
        a,
        r,
        d.length(),
        segs,
        Quat::from_rotation_arc(Vec3::Y, d.normalize()),
        col,
    );
}

/// A hex bolt head standing on a surface with normal `n`.
fn bolt(g: &mut Geo, p: Vec3, n: Vec3, r: f32, col: Rgba) {
    g.cylinder_rot(
        p - n * 0.002,
        r,
        r * 0.55,
        6,
        Quat::from_rotation_arc(Vec3::Y, n.normalize()),
        col,
    );
}

/// Round tube swept along `pts` (parallel-transport frames, open ends).
fn tube(g: &mut Geo, pts: &[Vec3], r: f32, segs: u32, col: Rgba) {
    let n = pts.len();
    let mut rows = Vec::with_capacity(n);
    let mut prev_u: Option<Vec3> = None;
    for i in 0..n {
        let t = (pts[(i + 1).min(n - 1)] - pts[i.saturating_sub(1)]).normalize_or(Vec3::Y);
        let u = match prev_u {
            Some(u) => (u - t * u.dot(t)).normalize_or(Vec3::X),
            None => {
                let rf = if t.y.abs() < 0.9 { Vec3::Y } else { Vec3::X };
                t.cross(rf).normalize()
            }
        };
        prev_u = Some(u);
        let v = t.cross(u);
        rows.push(
            (0..=segs)
                .map(|k| {
                    let a = k as f32 / segs as f32 * TAU;
                    let d = u * a.cos() + v * a.sin();
                    (pts[i] + d * r, d, col)
                })
                .collect::<Vec<Vtx>>(),
        );
    }
    grid(g, &rows, &[]);
}

/// Points along a sagging cable from `a` to `b`.
fn sag(a: Vec3, b: Vec3, depth: f32, n: usize) -> Vec<Vec3> {
    (0..=n)
        .map(|k| {
            let t = k as f32 / n as f32;
            a.lerp(b, t) - Vec3::Y * depth * 4.0 * t * (1.0 - t)
        })
        .collect()
}

/// Diagonal hazard stripes on the band `o + u*s + v*t` (s in 0..len, t in 0..wid),
/// facing `n`.
#[allow(clippy::too_many_arguments)]
fn hazard(
    g: &mut Geo,
    o: Vec3,
    u: Vec3,
    v: Vec3,
    n: Vec3,
    len: f32,
    wid: f32,
    pitch: f32,
    ca: Rgba,
    cb: Rgba,
) {
    let slant = wid;
    let k0 = (-slant / pitch).floor() as i32 - 1;
    let k1 = (len / pitch).ceil() as i32 + 1;
    let p = |s: f32, t: f32| o + u * s + v * t;
    for k in k0..k1 {
        let s0 = k as f32 * pitch;
        let s1 = s0 + pitch;
        let (a, b) = (s0.clamp(0.0, len), s1.clamp(0.0, len));
        let (c, d) = ((s1 - slant).clamp(0.0, len), (s0 - slant).clamp(0.0, len));
        if (b - a) + (c - d) < 1e-4 {
            continue;
        }
        let col = if k.rem_euclid(2) == 0 { ca } else { cb };
        flat(g, [p(a, 0.0), p(b, 0.0), p(c, wid), p(d, wid)], n, col);
    }
}

/// Flat chevron arrow lying on a horizontal surface, pointing along `fwd`.
fn arrow(g: &mut Geo, c: Vec3, fwd: Vec3, size: f32, col: Rgba) {
    arrow_on(g, c, fwd, Vec3::Y, size, col);
}

/// Chevron arrow on a surface with normal `n`, pointing along `fwd` (projected onto the
/// surface).
fn arrow_on(g: &mut Geo, c: Vec3, fwd: Vec3, n: Vec3, size: f32, col: Rgba) {
    let n = n.normalize();
    let f = (fwd - n * fwd.dot(n)).normalize();
    let side = n.cross(f);
    let tip = c + f * size * 0.5;
    let t = size * 0.32;
    for s in [-1.0f32, 1.0] {
        let tail = c - f * size * 0.5 + side * s * size * 0.55;
        flat(g, [tail, tip, tip - f * t, tail - f * t], n, col);
    }
}
