//! Item models carried on belts and by inserters.
//!
//! Each item is ~0.3 units across, rests on y = 0 and stays under ~500 triangles (hundreds
//! are on screen). Items are drawn with the painted material only, so "glowing" parts are
//! vertex colours moderately above 1.0 — enough for bloom, not enough to blow out.

use bevy::prelude::*;
use std::f32::consts::{FRAC_PI_2, PI, TAU};

use super::kit::{beam, extrude_z, flat, lathe, rod};
use crate::mesh_util::*;

const DARK: u32 = 0x1c2026;
const STEEL: u32 = 0x8a939e;
const GOLD: u32 = 0xe0b040;

/// Gentle emissive boost for small lit details.
fn hot(c: Rgba) -> Rgba {
    scale(c, 1.8)
}

/// Faceted ore chunk cluster with vein flecks in `vein`.
fn ore(g: &mut Geo, seed: u64, body: Rgba, vein: Option<Rgba>) {
    g.rock(Vec3::new(-0.03, 0.085, 0.01), 0.115, Vec3::new(1.25, 0.8, 1.05), seed, body);
    g.rock(
        Vec3::new(0.095, 0.06, 0.075),
        0.08,
        Vec3::new(1.05, 0.85, 1.1),
        seed + 1,
        scale(body, 0.88),
    );
    g.rock(
        Vec3::new(0.07, 0.055, -0.095),
        0.065,
        Vec3::new(1.1, 0.85, 1.0),
        seed + 2,
        scale(body, 1.1),
    );
    if let Some(v) = vein {
        for (i, (x, y, z)) in [
            (-0.11f32, 0.15f32, 0.03f32),
            (0.0, 0.17, -0.04),
            (0.1, 0.11, 0.1),
            (0.09, 0.1, -0.1),
            (-0.06, 0.12, 0.09),
        ]
        .into_iter()
        .enumerate()
        {
            g.rock(Vec3::new(x, y, z), 0.022 + 0.006 * (i % 2) as f32, Vec3::ONE, seed + 9 + i as u64, v);
        }
    }
}

/// Cluster of hexagonal crystal prisms with pointed tips.
fn crystals(g: &mut Geo, base: Rgba, tip: Rgba) {
    lathe(
        g,
        Vec3::ZERO,
        &[(0.14, 0.0, scale(base, 0.45)), (0.12, 0.04, scale(base, 0.45)), (0.0, 0.05, scale(base, 0.45))],
        7,
    );
    for (x, z, r, hgt, lean) in [
        (0.0f32, 0.0f32, 0.055f32, 0.3f32, 0.0f32),
        (-0.08, 0.04, 0.04, 0.2, -0.35),
        (0.075, 0.05, 0.036, 0.17, 0.4),
        (0.03, -0.08, 0.034, 0.16, 0.3),
        (-0.05, -0.06, 0.028, 0.12, -0.3),
    ] {
        let rot = Quat::from_rotation_z(lean) * Quat::from_rotation_x(lean * 0.6);
        let mut c = Geo::fine();
        lathe(
            &mut c,
            Vec3::ZERO,
            &[
                (r, 0.0, scale(base, 0.7)),
                (r, hgt * 0.72, base),
                (r, hgt * 0.72, tip),
                (0.0, hgt, tip),
            ],
            6,
        );
        g.append(&c, &Transform::from_translation(Vec3::new(x, 0.03, z)).with_rotation(rot));
    }
}

/// Steel drum with rolled hoops and a bung; `band` is the label colour.
fn drum(g: &mut Geo, body: Rgba, band: Rgba) {
    lathe(
        g,
        Vec3::ZERO,
        &[
            (0.1, 0.0, scale(body, 0.6)),
            (0.112, 0.012, body),
            (0.112, 0.07, body),
            (0.118, 0.08, scale(body, 1.1)),
            (0.112, 0.09, body),
            (0.112, 0.09, band),
            (0.112, 0.17, band),
            (0.112, 0.17, body),
            (0.118, 0.185, scale(body, 1.1)),
            (0.112, 0.2, body),
            (0.112, 0.255, body),
            (0.104, 0.27, scale(body, 1.1)),
            (0.0, 0.27, scale(body, 0.9)),
        ],
        18,
    );
    g.cylinder(Vec3::new(0.05, 0.27, 0.0), 0.022, 0.014, 8, lin(DARK));
    g.cylinder(Vec3::new(-0.05, 0.27, 0.02), 0.014, 0.01, 8, lin(DARK));
}

/// Stack of `n` bevelled plates with stamped bolt holes.
fn plates(g: &mut Geo, c: Rgba, n: usize, w: f32, d: f32, t: f32) {
    for k in 0..n {
        let y = t / 2.0 + k as f32 * (t + 0.006);
        let rot = Quat::from_rotation_y(k as f32 * 0.09 - 0.09);
        let tone = scale(c, 0.88 + 0.07 * k as f32);
        g.cuboid_rot(Vec3::Y * y, Vec3::new(w, t, d), rot, tone);
        if k == n - 1 {
            for (sx, sz) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                let p = Vec3::Y * (y + t / 2.0 + 0.001) + rot * Vec3::new(sx * (w / 2.0 - 0.035), 0.0, sz * (d / 2.0 - 0.035));
                g.cylinder(p, 0.012, 0.002, 8, scale(c, 0.45));
            }
            // Stamp mark.
            g.cuboid_rot(
                Vec3::Y * (y + t / 2.0 + 0.001),
                Vec3::new(w * 0.4, 0.002, 0.03),
                rot,
                scale(c, 0.7),
            );
        }
    }
}

/// Trapezoidal ingot, long along X.
fn ingot(g: &mut Geo, at: Vec3, col: Rgba) {
    let mut b = Geo::fine();
    extrude_z(
        &mut b,
        &[
            (-0.06, 0.0, scale(col, 0.85)),
            (0.06, 0.0, scale(col, 0.85)),
            (0.045, 0.06, scale(col, 1.15)),
            (-0.045, 0.06, scale(col, 1.15)),
        ],
        -0.13,
        0.13,
        true,
    );
    g.append(&b, &Transform::from_translation(at).with_rotation(Quat::from_rotation_y(FRAC_PI_2)));
}

/// Small equipment box with chamfered edges.
fn case(g: &mut Geo, c: Vec3, size: Vec3, col: Rgba) {
    g.cuboid(c, size, col);
}

/// Small carried/belt model for an item, about 0.3 units across, resting on y = 0.
pub fn item(item: sim::Item) -> Geo {
    use sim::Item as I;
    let mut g = Geo::fine();
    // Round parts at item scale don't need the fine tessellation.
    let mut lo = Geo::default();
    let c = lin(item.rgb());
    let dark = lin(DARK);
    let steel = lin(STEEL);
    let seed = item as u64 * 17 + 3;
    match item {
        // ---- Raw materials ----
        I::IronOre => ore(&mut g, seed, lin(0x6b5446), Some(lin(0xb0623a))),
        I::CopperOre => ore(&mut g, seed, lin(0x5e5248), Some(lin(0x3fb39a))),
        I::TitaniumOre => ore(&mut g, seed, lin(0x5a6470), Some(lin(0xc8d8e8))),
        I::Stone => ore(&mut g, seed, lin(0x9a9488), None),
        I::Carbon => {
            ore(&mut g, seed, lin(0x26262a), Some(lin(0x4a4a52)));
        }
        I::Ice => crystals(&mut g, lin(0x9fd2f0), lin(0xe6f6ff)),
        I::ExoticCrystal => crystals(&mut g, scale(c, 0.8), hot(c)),
        I::VoidShard => crystals(&mut g, lin(0x2a1450), hot(lin(0x9a50ff))),
        I::Water => drum(&mut g, lin(0x3a7ad8), lin(0xe9edf1)),
        I::Fuel => {
            drum(&mut g, lin(0xc8a020), lin(0x22262c));
            // Flammable diamond on the band.
            g.cuboid_rot(
                Vec3::new(0.0, 0.13, -0.113),
                Vec3::new(0.05, 0.05, 0.006),
                Quat::from_rotation_z(PI / 4.0),
                lin(0xff4a3a),
            );
        }
        I::Gas => {
            // Pressure bottle with a valve and a coloured shoulder.
            lathe(
                &mut g,
                Vec3::ZERO,
                &[
                    (0.08, 0.0, scale(steel, 0.7)),
                    (0.09, 0.015, steel),
                    (0.09, 0.2, steel),
                    (0.09, 0.2, c),
                    (0.08, 0.24, c),
                    (0.05, 0.27, c),
                    (0.025, 0.28, c),
                    (0.0, 0.28, c),
                ],
                16,
            );
            g.cylinder(Vec3::Y * 0.28, 0.018, 0.035, 8, lin(GOLD));
            g.cuboid(Vec3::new(0.0, 0.305, 0.0), Vec3::new(0.07, 0.015, 0.015), dark);
        }
        I::Biomass => {
            // Bundle of leafy matter in a crate tray.
            g.cuboid(Vec3::Y * 0.03, Vec3::new(0.26, 0.06, 0.2), lin(0x6a5040));
            for (k, (x, z)) in [(-0.07f32, -0.04f32), (0.05, -0.05), (-0.03, 0.05), (0.08, 0.05), (0.0, 0.0)]
                .into_iter()
                .enumerate()
            {
                let col = scale(c, 0.8 + 0.1 * (k % 3) as f32);
                lo.sphere(Vec3::new(x, 0.1, z), 0.06, Vec3::new(1.1, 0.8, 1.0), 8, col);
            }
            for (x, z, a) in [(-0.05f32, 0.02f32, 0.4f32), (0.06, -0.02, -0.5)] {
                g.cuboid_rot(
                    Vec3::new(x, 0.17, z),
                    Vec3::new(0.12, 0.012, 0.05),
                    Quat::from_rotation_y(a) * Quat::from_rotation_z(0.5),
                    scale(c, 1.2),
                );
            }
        }

        // ---- Refined ----
        I::IronPlate => plates(&mut g, lin(0xb8c0c8), 3, 0.27, 0.2, 0.026),
        I::CopperPlate => plates(&mut g, lin(0xe08a58), 3, 0.27, 0.2, 0.026),
        I::Plastic => {
            // Stack of moulded sheets in bright polymer.
            plates(&mut g, lin(0xf0eee0), 2, 0.25, 0.2, 0.03);
            g.cuboid(Vec3::new(0.0, 0.08, 0.0), Vec3::new(0.18, 0.02, 0.12), lin(0x4fb0e0));
        }
        I::Glass => {
            // Panes in a little rack, edges catching light.
            g.cuboid(Vec3::new(0.0, 0.015, 0.0), Vec3::new(0.27, 0.03, 0.12), lin(0x6a5040));
            for k in 0..3 {
                let z = -0.035 + k as f32 * 0.035;
                g.cuboid_rot(
                    Vec3::new(0.0, 0.12, z),
                    Vec3::new(0.24, 0.18, 0.01),
                    Quat::from_rotation_x(-0.12),
                    lin(0xa8d8ec),
                );
                g.cuboid_rot(
                    Vec3::new(0.0, 0.21, z - 0.01),
                    Vec3::new(0.24, 0.008, 0.012),
                    Quat::from_rotation_x(-0.12),
                    lin(0xe6f6ff),
                );
            }
        }
        I::Steel => {
            ingot(&mut g, Vec3::new(-0.065, 0.0, 0.0), lin(0x8890a0));
            ingot(&mut g, Vec3::new(0.065, 0.0, 0.0), lin(0x8890a0));
            ingot(&mut g, Vec3::new(0.0, 0.06, 0.0), lin(0x9aa2b0));
        }
        I::TitaniumAlloy => {
            ingot(&mut g, Vec3::new(-0.065, 0.0, 0.0), lin(0x9cc8e4));
            ingot(&mut g, Vec3::new(0.065, 0.0, 0.0), lin(0x9cc8e4));
            ingot(&mut g, Vec3::new(0.0, 0.06, 0.0), lin(0xb4dcf4));
        }
        I::Wire => {
            // Spool with flanges and coiled copper.
            g.cylinder(Vec3::ZERO, 0.12, 0.025, 18, dark);
            g.cylinder(Vec3::Y * 0.2, 0.12, 0.025, 18, dark);
            g.cylinder(Vec3::Y * 0.025, 0.04, 0.175, 10, steel);
            // Coiled copper: a ridged winding, one ridge per turn.
            let cu = lin(0xd8843a);
            let prof: Vec<(f32, f32, Rgba)> = (0..=18)
                .map(|k| {
                    let ridge = k % 2 == 1;
                    let r = if ridge { 0.104 } else { 0.094 };
                    (r, 0.028 + k as f32 * 0.0083, scale(cu, if ridge { 1.05 } else { 0.8 }))
                })
                .collect();
            lathe(&mut g, Vec3::ZERO, &prof, 14);
        }

        // ---- Components ----
        I::Gear => {
            let n = 12;
            let (y, t) = (0.0, 0.07);
            lathe(
                &mut g,
                Vec3::Y * y,
                &[(0.1, 0.0, scale(steel, 0.8)), (0.1, t, steel), (0.04, t, steel), (0.04, t, dark), (0.0, t, dark)],
                24,
            );
            for k in 0..n {
                let a = k as f32 / n as f32 * TAU;
                g.cuboid_rot(
                    Vec3::new(a.cos() * 0.118, t / 2.0, a.sin() * 0.118),
                    Vec3::new(0.045, t, 0.042),
                    Quat::from_rotation_y(-a),
                    scale(steel, 0.92),
                );
            }
            // Lightening holes.
            for k in 0..5 {
                let a = k as f32 / 5.0 * TAU;
                g.cylinder(Vec3::new(a.cos() * 0.07, t, a.sin() * 0.07), 0.016, 0.002, 8, scale(steel, 0.4));
            }
            // A smaller meshing gear leaning on it.
            g.cylinder(Vec3::new(0.09, t, 0.08), 0.05, 0.03, 12, lin(GOLD));
        }
        I::Pipe => {
            let along = Quat::from_rotation_z(FRAC_PI_2);
            g.cylinder_rot(Vec3::new(0.13, 0.09, 0.0), 0.075, 0.26, 16, along, c);
            for x in [0.13f32, -0.09] {
                g.cylinder_rot(Vec3::new(x, 0.09, 0.0), 0.095, 0.04, 16, along, scale(c, 0.7));
                for k in 0..6 {
                    let a = k as f32 / 6.0 * TAU;
                    g.cylinder_rot(
                        Vec3::new(x - 0.042, 0.09 + a.sin() * 0.075, a.cos() * 0.075),
                        0.008,
                        0.008,
                        6,
                        along,
                        lin(0xc0c8d0),
                    );
                }
            }
            g.cylinder_rot(Vec3::new(-0.131, 0.09, 0.0), 0.055, 0.002, 12, along, dark);
        }
        I::Motor => {
            let along = Quat::from_rotation_z(FRAC_PI_2);
            g.cylinder_rot(Vec3::new(0.1, 0.1, 0.0), 0.095, 0.19, 18, along, c);
            for x in [0.07f32, 0.03, -0.01, -0.05] {
                g.cylinder_rot(Vec3::new(x, 0.1, 0.0), 0.1, 0.012, 18, along, scale(c, 0.65));
            }
            g.cylinder_rot(Vec3::new(-0.09, 0.1, 0.0), 0.07, 0.03, 14, along, steel);
            g.cylinder_rot(Vec3::new(-0.12, 0.1, 0.0), 0.016, 0.05, 8, along, lin(0xd0d8e0));
            g.cuboid(Vec3::new(0.02, 0.012, 0.0), Vec3::new(0.18, 0.024, 0.16), dark);
            g.cuboid(Vec3::new(0.04, 0.205, 0.0), Vec3::new(0.06, 0.03, 0.06), dark);
        }
        I::Circuit => {
            g.cuboid(Vec3::new(0.0, 0.014, 0.0), Vec3::new(0.27, 0.028, 0.21), lin(0x1f8a3a));
            case(&mut g, Vec3::new(-0.05, 0.045, 0.0), Vec3::new(0.09, 0.034, 0.09), dark);
            g.cuboid(Vec3::new(-0.05, 0.063, 0.0), Vec3::new(0.05, 0.004, 0.05), lin(0x3a3f48));
            case(&mut g, Vec3::new(0.075, 0.04, -0.055), Vec3::new(0.06, 0.024, 0.045), dark);
            g.cylinder(Vec3::new(0.08, 0.028, 0.05), 0.02, 0.04, 10, lin(0x3060c0));
            for z in [-0.06f32, -0.03, 0.0, 0.03, 0.06] {
                g.cuboid(Vec3::new(-0.05, 0.031, z), Vec3::new(0.13, 0.004, 0.008), lin(GOLD));
            }
            g.cuboid(Vec3::new(0.02, 0.031, 0.09), Vec3::new(0.22, 0.004, 0.008), lin(GOLD));
            g.cuboid(Vec3::new(0.125, 0.014, 0.0), Vec3::new(0.02, 0.02, 0.18), lin(GOLD));
            g.cuboid(Vec3::new(0.11, 0.03, 0.03), Vec3::new(0.012, 0.006, 0.012), hot(lin(0x60ff80)));
        }
        I::RobotParts => {
            // Articulated arm segment: two links, a joint and a gripper.
            g.cylinder(Vec3::ZERO, 0.09, 0.04, 16, dark);
            lo.sphere(Vec3::Y * 0.07, 0.05, Vec3::ONE, 12, steel);
            beam(&mut g, Vec3::new(0.0, 0.07, 0.0), Vec3::new(0.08, 0.2, 0.0), 0.05, 0.05, c);
            lo.sphere(Vec3::new(0.08, 0.2, 0.0), 0.04, Vec3::ONE, 10, steel);
            beam(&mut g, Vec3::new(0.08, 0.2, 0.0), Vec3::new(-0.04, 0.24, 0.0), 0.04, 0.04, c);
            for s in [-1.0f32, 1.0] {
                rod(&mut g, Vec3::new(-0.04, 0.24, 0.0), Vec3::new(-0.1, 0.22, s * 0.03), 0.01, 6, steel);
            }
        }

        // ---- Advanced tech ----
        I::Computer => {
            case(&mut g, Vec3::new(0.0, 0.08, 0.0), Vec3::new(0.26, 0.16, 0.2), lin(0x2a2f38));
            g.cuboid(Vec3::new(0.0, 0.09, -0.1005), Vec3::new(0.2, 0.1, 0.003), scale(lin(0x20a0a0), 1.4));
            for k in 0..3 {
                g.cuboid(
                    Vec3::new(-0.06 + k as f32 * 0.04, 0.09, -0.102),
                    Vec3::new(0.02, 0.06 - k as f32 * 0.012, 0.002),
                    hot(lin(0x40e0d0)),
                );
            }
            for k in 0..5 {
                g.cuboid(
                    Vec3::new(-0.1 + k as f32 * 0.05, 0.161, 0.04),
                    Vec3::new(0.03, 0.004, 0.08),
                    lin(0x1a1e24),
                );
            }
        }
        I::NavSystem => {
            g.cylinder(Vec3::ZERO, 0.09, 0.05, 16, dark);
            lo.sphere(Vec3::Y * 0.14, 0.085, Vec3::ONE, 10, lin(0xd0d8e0));
            lo.torus(Vec3::Y * 0.14, 0.12, 0.01, 12, hot(c));
            let mut ring = Geo::default();
            ring.torus(Vec3::ZERO, 0.12, 0.008, 12, lin(0xa0a8b0));
            g.append(&ring, &Transform::from_translation(Vec3::Y * 0.14).with_rotation(Quat::from_rotation_x(1.1)));
            g.cylinder(Vec3::Y * 0.22, 0.008, 0.08, 6, lin(0xd0d8e0));
            lo.sphere(Vec3::Y * 0.3, 0.014, Vec3::ONE, 6, hot(lin(0xff4a3a)));
        }
        I::ReactorCore => {
            g.cylinder(Vec3::ZERO, 0.1, 0.04, 16, dark);
            g.cylinder(Vec3::Y * 0.24, 0.1, 0.04, 16, dark);
            lo.sphere(Vec3::Y * 0.14, 0.075, Vec3::ONE, 14, hot(c));
            for k in 0..4 {
                let a = k as f32 / 4.0 * TAU + PI / 4.0;
                rod(
                    &mut g,
                    Vec3::new(a.cos() * 0.09, 0.04, a.sin() * 0.09),
                    Vec3::new(a.cos() * 0.09, 0.24, a.sin() * 0.09),
                    0.012,
                    6,
                    steel,
                );
            }
            lo.torus(Vec3::Y * 0.14, 0.1, 0.012, 20, lin(0x6a7480));
        }
        I::SpeedModule | I::EfficiencyModule => {
            case(&mut g, Vec3::new(0.0, 0.045, 0.0), Vec3::new(0.2, 0.09, 0.26), lin(0x2a2f38));
            g.cuboid(Vec3::new(0.0, 0.0915, 0.0), Vec3::new(0.15, 0.004, 0.2), scale(c, 1.3));
            // Chevron (speed) or leaf-ish diamond (efficiency) emblem.
            if item == I::SpeedModule {
                for k in 0..2 {
                    let z = -0.03 + k as f32 * 0.05;
                    for s in [-1.0f32, 1.0] {
                        g.cuboid_rot(
                            Vec3::new(s * 0.025, 0.095, z),
                            Vec3::new(0.06, 0.004, 0.014),
                            Quat::from_rotation_y(s * 0.6),
                            hot(lin(0xffffff)),
                        );
                    }
                }
            } else {
                g.cuboid_rot(
                    Vec3::new(0.0, 0.095, 0.0),
                    Vec3::new(0.06, 0.004, 0.06),
                    Quat::from_rotation_y(PI / 4.0),
                    hot(lin(0xffffff)),
                );
            }
            for z in [-0.1f32, -0.05, 0.0, 0.05, 0.1] {
                for s in [-1.0f32, 1.0] {
                    g.cuboid(Vec3::new(s * 0.104, 0.04, z), Vec3::new(0.01, 0.03, 0.02), lin(GOLD));
                }
            }
        }
        I::RocketEngine => {
            // Combustion chamber, turbopump and a flared bell.
            lathe(
                &mut g,
                Vec3::ZERO,
                &[
                    (0.13, 0.0, scale(c, 0.6)),
                    (0.12, 0.02, c),
                    (0.08, 0.1, c),
                    (0.05, 0.16, scale(c, 0.8)),
                    (0.05, 0.16, steel),
                    (0.065, 0.2, steel),
                    (0.065, 0.26, steel),
                    (0.0, 0.27, steel),
                ],
                18,
            );
            g.cylinder(Vec3::new(0.08, 0.19, 0.0), 0.03, 0.06, 10, dark);
            rod(&mut g, Vec3::new(0.08, 0.2, 0.0), Vec3::new(0.05, 0.12, 0.0), 0.01, 6, lin(GOLD));
            g.cylinder(Vec3::Y * 0.001, 0.1, 0.002, 16, hot(lin(0xff8030)));
        }
        I::ColonyKit => {
            case(&mut g, Vec3::new(0.0, 0.1, 0.0), Vec3::new(0.27, 0.2, 0.21), lin(0xd8dde2));
            g.cuboid(Vec3::new(0.0, 0.1, 0.0), Vec3::new(0.05, 0.205, 0.215), c);
            g.cuboid(Vec3::new(0.0, 0.1, 0.0), Vec3::new(0.275, 0.205, 0.05), c);
            for sx in [-1.0f32, 1.0] {
                g.cuboid(Vec3::new(sx * 0.137, 0.12, 0.0), Vec3::new(0.012, 0.03, 0.09), dark);
            }
            g.cuboid(Vec3::new(0.08, 0.203, 0.06), Vec3::new(0.06, 0.004, 0.04), hot(lin(0x40d070)));
        }
        I::WarpDrive => {
            g.cylinder(Vec3::ZERO, 0.07, 0.05, 14, dark);
            let mut ring = Geo::default();
            ring.torus(Vec3::ZERO, 0.11, 0.022, 12, lin(0x6a7480));
            for tilt in [0.0f32, FRAC_PI_2] {
                g.append(
                    &ring,
                    &Transform::from_translation(Vec3::Y * 0.15)
                        .with_rotation(Quat::from_rotation_y(tilt) * Quat::from_rotation_x(FRAC_PI_2)),
                );
            }
            lo.sphere(Vec3::Y * 0.15, 0.05, Vec3::ONE, 10, hot(c));
            lo.torus(Vec3::Y * 0.15, 0.085, 0.008, 12, hot(c));
        }
        I::ExoticMatter => {
            // Containment flask: glass-like shell with a glowing core.
            g.cylinder(Vec3::ZERO, 0.08, 0.03, 14, dark);
            g.cylinder(Vec3::Y * 0.24, 0.07, 0.03, 14, dark);
            lo.sphere(Vec3::Y * 0.14, 0.085, Vec3::ONE, 14, scale(c, 0.5));
            lo.sphere(Vec3::Y * 0.14, 0.05, Vec3::ONE, 10, hot(c));
            for k in 0..3 {
                let a = k as f32 / 3.0 * TAU;
                rod(
                    &mut g,
                    Vec3::new(a.cos() * 0.075, 0.03, a.sin() * 0.075),
                    Vec3::new(a.cos() * 0.065, 0.24, a.sin() * 0.065),
                    0.008,
                    5,
                    steel,
                );
            }
        }
        I::FuelCell => {
            lathe(
                &mut g,
                Vec3::ZERO,
                &[
                    (0.085, 0.0, dark),
                    (0.09, 0.01, lin(0x2e353e)),
                    (0.09, 0.08, lin(0x2e353e)),
                    (0.092, 0.08, hot(c)),
                    (0.092, 0.17, hot(c)),
                    (0.09, 0.17, lin(0x2e353e)),
                    (0.09, 0.24, lin(0x2e353e)),
                    (0.06, 0.255, lin(0x2e353e)),
                    (0.0, 0.255, lin(0x2e353e)),
                ],
                16,
            );
            g.cylinder(Vec3::Y * 0.255, 0.03, 0.025, 10, lin(GOLD));
        }
        I::VoidDrive => {
            g.cylinder(Vec3::ZERO, 0.08, 0.04, 14, dark);
            lo.sphere(Vec3::Y * 0.15, 0.085, Vec3::ONE, 14, lin(0x0a0612));
            lo.torus(Vec3::Y * 0.15, 0.12, 0.016, 16, scale(c, 0.6));
            lo.torus(Vec3::Y * 0.15, 0.125, 0.007, 16, hot(lin(0x8a50ff)));
            for k in 0..4 {
                let a = k as f32 / 4.0 * TAU;
                beam(
                    &mut g,
                    Vec3::new(a.cos() * 0.06, 0.04, a.sin() * 0.06),
                    Vec3::new(a.cos() * 0.12, 0.15, a.sin() * 0.12),
                    0.016,
                    0.016,
                    lin(0x3a3f48),
                );
            }
        }
        I::BeaconCore => {
            g.cylinder(Vec3::ZERO, 0.07, 0.035, 12, dark);
            g.octa(Vec3::Y * 0.16, 0.11, hot(lin(0xdfefff)));
            let mut ring = Geo::default();
            ring.torus(Vec3::ZERO, 0.13, 0.008, 24, lin(0xc8d0d8));
            g.append(&ring, &Transform::from_translation(Vec3::Y * 0.16).with_rotation(Quat::from_rotation_x(0.4)));
            for k in 0..3 {
                let a = k as f32 / 3.0 * TAU;
                flat(
                    &mut g,
                    [
                        Vec3::new(a.cos() * 0.06, 0.035, a.sin() * 0.06),
                        Vec3::new(a.cos() * 0.09, 0.035, a.sin() * 0.09),
                        Vec3::new(a.cos() * 0.05, 0.09, a.sin() * 0.05),
                        Vec3::new(a.cos() * 0.04, 0.08, a.sin() * 0.04),
                    ],
                    Vec3::new(-a.sin(), 0.0, a.cos()),
                    lin(0xc8d0d8),
                );
            }
        }
    }
    g.append(&lo, &Transform::IDENTITY);
    g
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_stay_within_triangle_budget() {
        let over: Vec<String> = sim::Item::ALL
            .iter()
            .map(|&it| (it, item(it).idx.len() / 3))
            .filter(|&(_, t)| t == 0 || t > 900)
            .map(|(it, t)| format!("{it:?}: {t}"))
            .collect();
        assert!(over.is_empty(), "over budget: {over:?}");
    }
}
