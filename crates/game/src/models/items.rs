//! Item models carried on belts and by inserters.

use bevy::prelude::*;
use std::f32::consts::{FRAC_PI_2, PI};

use crate::mesh_util::*;

/// Small carried/belt model for an item, about 0.3 across, resting on y = 0.
/// Glowing parts use colours above 1.0 so bloom picks them up.
pub fn item(item: sim::Item) -> Geo {
    use sim::Item as I;
    let mut g = Geo::fine();
    let c = lin(item.rgb());
    let hot = |c: Rgba| scale(c, 3.0);
    let dark = lin(0x1c2026);
    let rot_y = |a: f32| Quat::from_rotation_y(a);
    match item {
        I::IronOre | I::CopperOre | I::Stone | I::TitaniumOre | I::Carbon => {
            let seed = item as u64 * 17 + 3;
            g.rock(
                Vec3::new(-0.04, 0.09, 0.0),
                0.12,
                Vec3::new(1.2, 0.8, 1.0),
                seed,
                scale(c, 0.9),
            );
            g.rock(
                Vec3::new(0.09, 0.07, 0.07),
                0.09,
                Vec3::new(1.0, 0.8, 1.1),
                seed + 1,
                c,
            );
            g.rock(
                Vec3::new(0.05, 0.06, -0.1),
                0.07,
                Vec3::new(1.1, 0.8, 1.0),
                seed + 2,
                scale(c, 1.1),
            );
            // Ore flecks catch the light.
            if item != I::Stone && item != I::Carbon {
                let fleck = scale(c, 1.9);
                for (x, y, z) in [(-0.1, 0.17, 0.02), (0.08, 0.14, 0.1), (0.02, 0.1, -0.12)] {
                    g.rock(Vec3::new(x, y, z), 0.025, Vec3::ONE, seed + 9, fleck);
                }
            }
        }
        I::Ice | I::ExoticCrystal | I::VoidShard => {
            let glowing = item != I::Ice;
            for (x, z, r, hgt, a) in [
                (0.0f32, 0.0f32, 0.07f32, 0.3f32, 0.0f32),
                (-0.09, 0.05, 0.05, 0.2, 0.4),
                (0.08, 0.06, 0.045, 0.17, -0.3),
                (0.03, -0.09, 0.04, 0.15, 0.8),
            ] {
                let tip = Vec3::new(x, 0.0, z);
                g.cylinder(tip, r, hgt * 0.7, 6, scale(c, 0.8));
                if glowing {
                    g.cone(tip + Vec3::Y * hgt * 0.7, r, hgt * 0.3, 6, a, hot(c));
                } else {
                    g.cone(
                        tip + Vec3::Y * hgt * 0.7,
                        r,
                        hgt * 0.3,
                        6,
                        a,
                        scale(c, 1.15),
                    );
                }
            }
        }
        I::Water | I::Fuel | I::Gas => {
            // Drum with rolled rims, or a pressure bottle for gas.
            if item == I::Gas {
                g.cylinder(Vec3::ZERO, 0.09, 0.2, 16, lin(0x9aa4ae));
                g.sphere(
                    Vec3::Y * 0.2,
                    0.09,
                    Vec3::new(1.0, 0.8, 1.0),
                    10,
                    lin(0x9aa4ae),
                );
                g.cylinder(Vec3::Y * 0.27, 0.025, 0.05, 8, dark);
                g.cylinder(Vec3::Y * 0.08, 0.092, 0.05, 16, hot(scale(c, 0.8)));
            } else {
                g.cylinder(Vec3::ZERO, 0.11, 0.27, 16, scale(c, 0.9));
                for y in [0.04, 0.12, 0.2] {
                    g.cylinder(Vec3::Y * y, 0.117, 0.025, 16, scale(c, 0.55));
                }
                g.cylinder(Vec3::Y * 0.27, 0.07, 0.025, 12, dark);
                if item == I::Fuel {
                    g.cuboid(
                        Vec3::new(0.0, 0.16, 0.112),
                        Vec3::new(0.09, 0.07, 0.01),
                        hot(lin(0xffe060)),
                    );
                }
            }
        }
        I::Biomass => {
            g.rock(
                Vec3::new(0.0, 0.08, 0.0),
                0.13,
                Vec3::new(1.2, 0.8, 1.1),
                41,
                c,
            );
            for (x, z, a) in [
                (-0.07f32, 0.03f32, 0.4f32),
                (0.06, -0.02, -0.5),
                (0.0, 0.08, 1.2),
            ] {
                g.cuboid_rot(
                    Vec3::new(x, 0.2, z),
                    Vec3::new(0.15, 0.02, 0.07),
                    Quat::from_rotation_y(a) * Quat::from_rotation_z(0.5),
                    scale(c, 1.3),
                );
            }
        }
        I::IronPlate | I::CopperPlate | I::Glass | I::Plastic => {
            let (w, d, t, n) = if item == I::Glass {
                (0.26, 0.2, 0.02, 3)
            } else {
                (0.27, 0.18, 0.03, 3)
            };
            for k in 0..n {
                let off = (k as f32 - 1.0) * 0.012;
                g.cuboid_rot(
                    Vec3::new(off, t / 2.0 + k as f32 * (t + 0.004), -off),
                    Vec3::new(w, t, d),
                    rot_y(k as f32 * 0.12 - 0.1),
                    if item == I::Glass {
                        scale(c, 1.1)
                    } else {
                        scale(c, 0.9 + 0.08 * k as f32)
                    },
                );
            }
        }
        I::Steel | I::TitaniumAlloy => {
            // Stacked ingots.
            for (x, y, z) in [
                (-0.07f32, 0.0f32, 0.0f32),
                (0.07, 0.0, 0.0),
                (0.0, 0.07, 0.0),
            ] {
                g.cuboid(
                    Vec3::new(x, y + 0.035, z),
                    Vec3::new(0.12, 0.07, 0.26),
                    scale(c, 0.85),
                );
                g.cuboid(
                    Vec3::new(x, y + 0.075, z),
                    Vec3::new(0.09, 0.015, 0.22),
                    scale(c, 1.2),
                );
            }
        }
        I::Wire => {
            g.torus(Vec3::Y * 0.07, 0.085, 0.045, 14, c);
            g.torus(Vec3::Y * 0.13, 0.075, 0.04, 14, scale(c, 1.1));
            g.cylinder(Vec3::ZERO, 0.04, 0.2, 10, dark);
        }
        I::Pipe => {
            let along = Quat::from_rotation_z(FRAC_PI_2);
            g.cylinder_rot(Vec3::new(0.13, 0.1, 0.0), 0.09, 0.26, 16, along, c);
            for x in [-0.11f32, 0.08] {
                g.cylinder_rot(
                    Vec3::new(x + 0.05, 0.1, 0.0),
                    0.105,
                    0.04,
                    16,
                    along,
                    scale(c, 0.7),
                );
            }
            g.cylinder_rot(Vec3::new(0.0, 0.1, 0.0), 0.06, 0.01, 12, along, dark);
        }
        I::Gear => {
            let n = 12;
            g.cylinder(Vec3::ZERO, 0.1, 0.06, 16, scale(c, 1.05));
            for k in 0..n {
                let a = k as f32 / n as f32 * PI * 2.0;
                g.cuboid_rot(
                    Vec3::new(a.cos() * 0.115, 0.03, a.sin() * 0.115),
                    Vec3::new(0.05, 0.06, 0.04),
                    Quat::from_rotation_y(-a),
                    scale(c, 0.9),
                );
            }
            g.cylinder(Vec3::Y * 0.058, 0.035, 0.012, 10, dark);
        }
        I::Motor => {
            let along = Quat::from_rotation_z(FRAC_PI_2);
            g.cylinder_rot(Vec3::new(0.1, 0.1, 0.0), 0.1, 0.2, 16, along, c);
            g.cylinder_rot(
                Vec3::new(-0.1, 0.1, 0.0),
                0.06,
                0.04,
                12,
                along,
                scale(lin(0xc0c8d0), 1.0),
            );
            g.cylinder_rot(
                Vec3::new(-0.14, 0.1, 0.0),
                0.02,
                0.06,
                8,
                along,
                lin(0xc0c8d0),
            );
            for x in [0.04f32, 0.1, 0.16] {
                g.cylinder_rot(
                    Vec3::new(x, 0.1, 0.0),
                    0.106,
                    0.015,
                    16,
                    along,
                    scale(c, 0.6),
                );
            }
        }
        I::Circuit => {
            g.cuboid(
                Vec3::new(0.0, 0.015, 0.0),
                Vec3::new(0.27, 0.03, 0.22),
                lin(0x1f8a3a),
            );
            for (x, z, w, d) in [
                (-0.05f32, 0.0f32, 0.09f32, 0.09f32),
                (0.07, -0.05, 0.06, 0.05),
                (0.07, 0.05, 0.05, 0.05),
            ] {
                g.cuboid(Vec3::new(x, 0.05, z), Vec3::new(w, 0.04, d), dark);
            }
            let gold = lin(0xe0b040);
            for z in [-0.07f32, -0.035, 0.0, 0.035, 0.07] {
                g.cuboid(
                    Vec3::new(-0.05, 0.04, z),
                    Vec3::new(0.115, 0.012, 0.01),
                    gold,
                );
            }
            g.cuboid(
                Vec3::new(-0.05, 0.032, -0.09),
                Vec3::new(0.22, 0.004, 0.01),
                gold,
            );
            g.cuboid(
                Vec3::new(0.08, 0.034, 0.0),
                Vec3::new(0.01, 0.004, 0.12),
                gold,
            );
        }
        I::RobotParts => {
            g.sphere(Vec3::new(0.0, 0.1, 0.0), 0.09, Vec3::ONE, 10, c);
            g.cuboid_rot(
                Vec3::new(0.1, 0.1, 0.0),
                Vec3::new(0.15, 0.05, 0.05),
                Quat::from_rotation_z(0.3),
                scale(c, 0.8),
            );
            g.cuboid(
                Vec3::new(-0.08, 0.13, 0.0),
                Vec3::new(0.06, 0.12, 0.05),
                scale(c, 0.8),
            );
            g.cylinder_rot(
                Vec3::new(0.0, 0.1, -0.12),
                0.035,
                0.24,
                10,
                Quat::from_rotation_x(-FRAC_PI_2),
                dark,
            );
        }
        I::Computer => {
            g.cuboid(
                Vec3::new(0.0, 0.1, 0.0),
                Vec3::new(0.26, 0.2, 0.2),
                lin(0x2a2f38),
            );
            g.cuboid(
                Vec3::new(0.0, 0.11, 0.101),
                Vec3::new(0.2, 0.12, 0.006),
                hot(c),
            );
            g.cuboid(
                Vec3::new(0.08, 0.03, 0.101),
                Vec3::new(0.05, 0.015, 0.006),
                hot(lin(0x60ff80)),
            );
        }
        I::NavSystem => {
            g.cylinder(Vec3::ZERO, 0.08, 0.05, 12, dark);
            g.sphere(Vec3::Y * 0.12, 0.1, Vec3::ONE, 12, lin(0xd0d8e0));
            g.torus(Vec3::Y * 0.12, 0.13, 0.012, 24, hot(c));
            g.cylinder_rot(
                Vec3::new(0.0, 0.12, 0.0),
                0.13,
                0.01,
                24,
                Quat::from_rotation_x(0.9),
                scale(c, 0.8),
            );
            g.cylinder(Vec3::Y * 0.2, 0.01, 0.1, 6, lin(0xd0d8e0));
        }
        I::ReactorCore => {
            g.sphere(Vec3::Y * 0.15, 0.09, Vec3::ONE, 12, hot(c));
            for r in [0.13f32, 0.115] {
                g.torus(Vec3::Y * 0.15, r, 0.012, 20, lin(0x6a7480));
            }
            g.cylinder(Vec3::ZERO, 0.1, 0.04, 12, dark);
            g.cylinder(Vec3::Y * 0.26, 0.1, 0.04, 12, dark);
        }
        I::SpeedModule | I::EfficiencyModule => {
            g.cuboid(
                Vec3::new(0.0, 0.05, 0.0),
                Vec3::new(0.2, 0.1, 0.26),
                lin(0x2a2f38),
            );
            g.cuboid(
                Vec3::new(0.0, 0.108, 0.0),
                Vec3::new(0.16, 0.014, 0.2),
                hot(c),
            );
            for z in [-0.1f32, -0.05, 0.0, 0.05, 0.1] {
                g.cuboid(
                    Vec3::new(0.108, 0.05, z),
                    Vec3::new(0.016, 0.04, 0.02),
                    lin(0xe0b040),
                );
                g.cuboid(
                    Vec3::new(-0.108, 0.05, z),
                    Vec3::new(0.016, 0.04, 0.02),
                    lin(0xe0b040),
                );
            }
        }
        I::RocketEngine => {
            g.cylinder(Vec3::Y * 0.2, 0.07, 0.07, 12, lin(0x8a929c));
            g.cone(Vec3::new(0.0, 0.2, 0.0), 0.12, -0.2, 14, 0.0, c);
            g.cylinder(Vec3::ZERO, 0.11, 0.025, 14, hot(scale(c, 0.9)));
            g.cylinder(Vec3::Y * 0.02, 0.05, 0.18, 10, lin(0x4a525c));
        }
        I::ColonyKit => {
            g.cuboid(
                Vec3::new(0.0, 0.1, 0.0),
                Vec3::new(0.26, 0.2, 0.22),
                lin(0xd8dde2),
            );
            g.cuboid(Vec3::new(0.0, 0.1, 0.0), Vec3::new(0.06, 0.205, 0.225), c);
            g.cuboid(Vec3::new(0.0, 0.1, 0.0), Vec3::new(0.265, 0.205, 0.06), c);
            g.cuboid(
                Vec3::new(0.0, 0.215, 0.0),
                Vec3::new(0.12, 0.03, 0.12),
                hot(lin(0xffd0e8)),
            );
        }
        I::WarpDrive => {
            g.torus(Vec3::Y * 0.14, 0.11, 0.025, 24, lin(0x6a7480));
            g.torus(Vec3::Y * 0.14, 0.11, 0.012, 24, hot(c));
            g.sphere(Vec3::Y * 0.14, 0.06, Vec3::ONE, 12, hot(c));
            g.cylinder(Vec3::ZERO, 0.06, 0.06, 10, dark);
        }
        I::ExoticMatter => {
            g.sphere(Vec3::Y * 0.15, 0.08, Vec3::ONE, 14, hot(c));
            for k in 0..6 {
                let a = k as f32 * 1.05;
                g.octa(
                    Vec3::new(
                        a.cos() * 0.12,
                        0.15 + (a * 2.0).sin() * 0.06,
                        a.sin() * 0.12,
                    ),
                    0.025,
                    hot(scale(c, 0.8)),
                );
            }
        }
        I::FuelCell => {
            g.cylinder(Vec3::ZERO, 0.09, 0.26, 16, lin(0x2e353e));
            g.cylinder(Vec3::Y * 0.09, 0.095, 0.08, 16, hot(c));
            g.cylinder(Vec3::Y * 0.26, 0.04, 0.03, 10, lin(0xe0b040));
        }
        I::VoidDrive => {
            g.sphere(Vec3::Y * 0.14, 0.09, Vec3::ONE, 14, lin(0x07040f));
            g.torus(Vec3::Y * 0.14, 0.12, 0.014, 24, hot(c));
            g.torus(Vec3::Y * 0.14, 0.12, 0.02, 24, scale(c, 0.3));
            g.cylinder(Vec3::ZERO, 0.07, 0.04, 10, dark);
        }
        I::BeaconCore => {
            g.octa(Vec3::Y * 0.15, 0.13, hot(c));
            g.torus(Vec3::Y * 0.15, 0.15, 0.008, 24, lin(0xc8d0d8));
            g.cylinder(Vec3::ZERO, 0.06, 0.03, 10, dark);
        }
    }
    g
}
