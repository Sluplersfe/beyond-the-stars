//! Colony, science and defence: houses, habitats, labs, observatory, shipyard, turret,
//! and the rocket the shipyard launches.
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
/// Warm interior light.
const WARM: u32 = 0xffd08a;
/// Research purple.
const LAB: u32 = 0x8a5cd8;
/// Armour olive for the turret.
const ARMOUR: u32 = 0x5a6248;
/// Window glass tint (rgba; the glass material is alpha-blended).
const GLASS: Rgba = [0.55, 0.75, 0.9, 0.28];

/// Builds `kind` into `m`; `s` is the footprint edge length in tiles.
pub fn build(kind: BuildingKind, s: f32, m: &mut Model) {
    use BuildingKind as K;
    let h = s / 2.0;
    match kind {
        K::House => house(m, h),
        K::Habitat => habitat(m, h),
        K::Lab => lab(m, h, false),
        K::LabMk2 => lab(m, h, true),
        K::Observatory => observatory(m, h),
        K::Shipyard => shipyard(m, h),
        K::Turret => turret(m),
        _ => unreachable!("{kind:?} is not a colony building"),
    }
}

/// A row of lit windows with frames on a wall facing `n` (horizontal normal), centred at
/// `c`, spread along `along`.
fn window_row(m: &mut Model, c: Vec3, along: Vec3, n: Vec3, count: usize, gap: f32, size: Vec2) {
    let rot = Quat::from_rotation_arc(Vec3::Z, n);
    for k in 0..count {
        let p = c + along * ((k as f32 - (count as f32 - 1.0) / 2.0) * gap);
        m.metal.cuboid_rot(
            p + n * 0.004,
            Vec3::new(size.x + 0.03, size.y + 0.03, 0.012),
            rot,
            lin(SHADOW),
        );
        m.glow
            .cuboid_rot(p + n * 0.011, Vec3::new(size.x, size.y, 0.004), rot, lin(WARM));
        // Mullion.
        m.metal.cuboid_rot(
            p + n * 0.014,
            Vec3::new(0.012, size.y, 0.004),
            rot,
            lin(BRIGHT),
        );
    }
}

// ---------------------------------------------------------------------------------------
// House: a prefab colonist home — barrel-roofed module, entry porch, solar roof panels,
// side airlock pod, garden planter and an antenna.

fn house(m: &mut Model, h: f32) {
    foundation(m, h);
    let y0 = 0.08;
    // Raised floor plinth.
    m.metal
        .cuboid(Vec3::new(-0.1, y0 + 0.04, 0.1), Vec3::new(1.4, 0.08, 1.1), lin(GUNMETAL));
    let fy = y0 + 0.08;
    let (w, d, wall_h) = (1.3f32, 1.0f32, 0.5f32);
    let c = Vec3::new(-0.1, fy, 0.1);
    m.body
        .cuboid(c + Vec3::Y * wall_h / 2.0, Vec3::new(w, wall_h, d), lin(WHITE));
    // Trim band and corner posts.
    m.body.cuboid(
        c + Vec3::Y * 0.06,
        Vec3::new(w + 0.02, 0.12, d + 0.02),
        lin(BLUE),
    );
    for (sx, sz) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        m.metal.cuboid(
            c + Vec3::new(sx * w / 2.0, wall_h / 2.0, sz * d / 2.0),
            Vec3::new(0.05, wall_h + 0.02, 0.05),
            lin(GUNMETAL),
        );
    }
    // Barrel roof running along X (profile in ZY, extruded along X via a rotated build).
    let mut roof = Geo::fine();
    let segs = 12;
    let r = d / 2.0 + 0.06;
    let mut prof: Vec<(f32, f32, Rgba)> = Vec::new();
    // Outer arc, counter-clockwise in (x=z-axis, y) seen from +X... build in XY then rotate.
    for k in 0..=segs {
        let a = k as f32 / segs as f32 * PI;
        prof.push((a.cos() * r, a.sin() * r * 0.55, scale(lin(0x9aa4b0), 0.9 + 0.1 * a.sin())));
    }
    prof.push((-r, 0.0, lin(SHADOW)));
    prof.push((-r, -0.02, lin(SHADOW)));
    prof.push((r, -0.02, lin(SHADOW)));
    prof.push((r, 0.0, lin(SHADOW)));
    extrude_z(&mut roof, &prof, -w / 2.0 - 0.06, w / 2.0 + 0.06, true);
    m.body.append(
        &roof,
        &Transform::from_translation(c + Vec3::Y * wall_h).with_rotation(Quat::from_rotation_y(FRAC_PI_2)),
    );
    // Roof ribs.
    for k in 0..5 {
        let x = c.x - w / 2.0 + w * k as f32 / 4.0;
        let pts: Vec<Vec3> = (0..=10)
            .map(|i| {
                let a = i as f32 / 10.0 * PI;
                Vec3::new(x, c.y + wall_h + a.sin() * (r + 0.012) * 0.55, c.z + a.cos() * (r + 0.012))
            })
            .collect();
        tube(&mut m.metal, &pts, 0.012, 6, lin(GUNMETAL));
    }
    // Solar panels on the south slope of the roof.
    for k in 0..3 {
        let x = c.x - 0.4 + k as f32 * 0.4;
        let a: f32 = 0.95;
        let p = c + Vec3::new(x - c.x, wall_h + a.sin() * (r + 0.02) * 0.55, a.cos() * (r + 0.02));
        let n = Vec3::new(0.0, a.sin() * 1.0, a.cos() * 0.55 * 1.0).normalize();
        let rot = Quat::from_rotation_arc(Vec3::Y, n);
        m.body
            .cuboid_rot(p, Vec3::new(0.34, 0.012, 0.3), rot, lin(0x0e1f42));
        m.metal.cuboid_rot(
            p + n * 0.007,
            Vec3::new(0.008, 0.002, 0.28),
            rot,
            lin(0xa0acb8),
        );
    }
    // Windows: front (-Z) and the east side.
    window_row(
        m,
        c + Vec3::new(0.25, 0.3, -d / 2.0),
        Vec3::X,
        Vec3::NEG_Z,
        2,
        0.32,
        Vec2::new(0.2, 0.18),
    );
    window_row(
        m,
        c + Vec3::new(w / 2.0, 0.3, 0.0),
        Vec3::Z,
        Vec3::X,
        2,
        0.4,
        Vec2::new(0.22, 0.18),
    );
    // Entry: door, porch roof on posts, steps.
    let door = c + Vec3::new(-0.35, 0.0, -d / 2.0);
    m.body.cuboid(
        door + Vec3::new(0.0, 0.2, -0.006),
        Vec3::new(0.2, 0.38, 0.012),
        lin(BLUE),
    );
    m.glow.cuboid(
        door + Vec3::new(0.0, 0.33, -0.014),
        Vec3::new(0.12, 0.06, 0.004),
        lin(WARM),
    );
    m.body.cuboid(
        door + Vec3::new(0.0, 0.47, -0.16),
        Vec3::new(0.42, 0.03, 0.32),
        lin(YELLOW),
    );
    for sx in [-1.0f32, 1.0] {
        m.metal.cylinder(
            door + Vec3::new(sx * 0.18, 0.0, -0.3),
            0.015,
            0.46,
            6,
            lin(GUNMETAL),
        );
    }
    for k in 0..2 {
        m.rough.cuboid(
            door + Vec3::new(0.0, -0.02 - k as f32 * 0.03, -0.12 - k as f32 * 0.1),
            Vec3::new(0.3, 0.03, 0.12),
            lin(CONCRETE),
        );
    }
    // Airlock pod on the west end.
    let pod = c + Vec3::new(-w / 2.0 - 0.18, 0.0, 0.2);
    m.body.cylinder(pod, 0.2, 0.42, 20, lin(WHITE));
    m.body
        .sphere(pod + Vec3::Y * 0.42, 0.2, Vec3::new(1.0, 0.5, 1.0), 16, lin(WHITE));
    m.metal.torus(pod + Vec3::Y * 0.12, 0.205, 0.015, 24, lin(BLUE));
    m.glow
        .cuboid(pod + Vec3::new(-0.2, 0.28, 0.0), Vec3::new(0.004, 0.06, 0.1), lin(0x40d070));
    // Garden planter with shrubs at the front-right.
    let g = Vec3::new(0.62, y0, -0.68);
    m.rough
        .cuboid(g + Vec3::Y * 0.06, Vec3::new(0.62, 0.12, 0.36), lin(0x6a5040));
    m.rough
        .cuboid(g + Vec3::Y * 0.12, Vec3::new(0.56, 0.01, 0.3), lin(0x3a2a1e));
    for (k, (x, z)) in [(-0.2f32, -0.05f32), (0.0, 0.07), (0.18, -0.06)].into_iter().enumerate() {
        let col = lin([0x4a8a36, 0x3f7a30, 0x5a9a3a][k]);
        m.rough.sphere(
            g + Vec3::new(x, 0.2, z),
            0.09,
            Vec3::new(1.0, 0.9, 1.0),
            10,
            col,
        );
    }
    // Antenna mast with a little dish and a red tip light.
    let ant = c + Vec3::new(0.5, wall_h + 0.2, 0.35);
    m.metal.cylinder(ant, 0.012, 0.45, 6, lin(BRIGHT));
    m.metal.cuboid(
        ant + Vec3::Y * 0.3,
        Vec3::new(0.18, 0.01, 0.01),
        lin(BRIGHT),
    );
    m.body.sphere(
        ant + Vec3::new(0.0, 0.2, 0.05),
        0.07,
        Vec3::new(1.0, 1.0, 0.35),
        12,
        lin(WHITE),
    );
    m.glow
        .sphere(ant + Vec3::Y * 0.46, 0.018, Vec3::ONE, 8, lin(RED));
    m.lamp = Vec3::new(0.8, y0 + 0.2, 0.8);
}

// ---------------------------------------------------------------------------------------
// Habitat: pressurised greenhouse dome with an interior park, ribbed frame and airlocks.

fn habitat(m: &mut Model, h: f32) {
    foundation(m, h);
    let y0 = 0.08;
    let c = Vec3::new(0.0, y0, 0.0);
    let rr = 1.22;
    // Ring wall with a band of windows.
    lathe(
        &mut m.body,
        c,
        &[
            (rr + 0.08, 0.0, lin(GUNMETAL)),
            (rr + 0.08, 0.06, lin(GUNMETAL)),
            (rr + 0.08, 0.06, lin(WHITE)),
            (rr + 0.04, 0.3, lin(WHITE)),
            (rr + 0.04, 0.3, lin(BLUE)),
            (rr + 0.04, 0.34, lin(BLUE)),
            (rr, 0.34, lin(BLUE)),
        ],
        48,
    );
    for k in 0..16 {
        let a = (k as f32 + 0.5) / 16.0 * TAU;
        let d = Vec3::new(a.cos(), 0.0, a.sin());
        m.glow.cuboid_rot(
            c + d * (rr + 0.065) + Vec3::Y * 0.18,
            Vec3::new(0.18, 0.08, 0.004),
            Quat::from_rotation_y(-a + FRAC_PI_2),
            lin(WARM),
        );
    }
    // Interior floor: lawn with paths, a pond, trees and two small lit buildings.
    let floor_y = c.y + 0.3;
    m.rough
        .cylinder(c + Vec3::Y * 0.28, rr - 0.02, 0.02, 40, lin(0x4a8a36));
    for a in [0.0f32, FRAC_PI_2] {
        m.rough.cuboid_rot(
            Vec3::new(c.x, floor_y + 0.002, c.z),
            Vec3::new(2.0 * rr - 0.2, 0.004, 0.14),
            Quat::from_rotation_y(a),
            lin(0xc8bba0),
        );
    }
    m.glass
        .cylinder(Vec3::new(0.5, floor_y, 0.45), 0.25, 0.006, 20, [0.25, 0.5, 0.75, 0.75]);
    for (k, (x, z, s)) in [
        (-0.6f32, -0.5f32, 1.0f32),
        (-0.75, 0.15, 0.8),
        (-0.3, 0.7, 0.9),
        (0.55, -0.55, 1.1),
        (0.2, -0.85, 0.75),
        (0.85, 0.0, 0.85),
    ]
    .into_iter()
    .enumerate()
    {
        let p = Vec3::new(x, floor_y, z);
        m.rough.cylinder(p, 0.025, 0.18 * s, 6, lin(0x5a3a22));
        let col = lin([0x3f7a30, 0x4a8a36, 0x5a9a3a][k % 3]);
        m.rough.sphere(
            p + Vec3::Y * 0.26 * s,
            0.14 * s,
            Vec3::new(1.0, 0.9, 1.0),
            10,
            col,
        );
    }
    for (x, z) in [(0.1f32, 0.3f32), (-0.2, -0.25)] {
        let p = Vec3::new(x, floor_y, z);
        m.body
            .cuboid(p + Vec3::Y * 0.12, Vec3::new(0.28, 0.24, 0.22), lin(WHITE));
        m.glow.cuboid(
            p + Vec3::new(0.0, 0.13, -0.111),
            Vec3::new(0.18, 0.06, 0.004),
            lin(WARM),
        );
        m.body
            .cuboid(p + Vec3::Y * 0.25, Vec3::new(0.32, 0.03, 0.26), lin(BLUE));
    }
    // Central light mast.
    m.metal
        .cylinder(Vec3::new(0.0, floor_y, 0.0), 0.03, 0.85, 8, lin(BRIGHT));
    m.glow
        .sphere(Vec3::new(0.0, floor_y + 0.88, 0.0), 0.06, Vec3::ONE, 10, lin(0xfff0c8));

    // Glass dome with meridian ribs and two latitude rings.
    let dome_c = c + Vec3::Y * 0.34;
    let dh = 1.05;
    let dome: Vec<(f32, f32, Rgba)> = (0..=12)
        .map(|k| {
            let a = k as f32 / 12.0 * FRAC_PI_2;
            (rr * a.cos(), dh * a.sin(), GLASS)
        })
        .collect();
    lathe(&mut m.glass, dome_c, &dome, 40);
    for k in 0..12 {
        let a = k as f32 / 12.0 * TAU;
        let d = Vec3::new(a.cos(), 0.0, a.sin());
        let pts: Vec<Vec3> = (0..=10)
            .map(|i| {
                let t = i as f32 / 10.0 * FRAC_PI_2;
                dome_c + d * (rr + 0.01) * t.cos() + Vec3::Y * (dh + 0.01) * t.sin()
            })
            .collect();
        tube(&mut m.metal, &pts, 0.018, 6, lin(BRIGHT));
    }
    for t in [0.35f32, 0.75] {
        let a = t * FRAC_PI_2;
        m.metal.torus(
            dome_c + Vec3::Y * (dh + 0.01) * a.sin(),
            (rr + 0.01) * a.cos(),
            0.016,
            40,
            lin(BRIGHT),
        );
    }
    m.metal
        .cylinder(dome_c + Vec3::Y * (dh - 0.02), 0.14, 0.06, 16, lin(GUNMETAL));
    m.glow
        .sphere(dome_c + Vec3::Y * (dh + 0.06), 0.035, Vec3::ONE, 8, lin(RED));

    // Airlock tunnels to the south and east with doors.
    for (dir, len) in [(Vec3::NEG_Z, 0.32f32), (Vec3::X, 0.3)] {
        let rot = Quat::from_rotation_arc(Vec3::Y, dir);
        let start = c + Vec3::Y * 0.2 + dir * (rr - 0.02);
        m.body.cylinder_rot(start, 0.17, len, 20, rot, lin(WHITE));
        for t in [0.1f32, 0.24] {
            m.metal
                .cylinder_rot(start + dir * t, 0.18, 0.03, 20, rot, lin(BLUE));
        }
        let end = start + dir * len;
        m.metal.cylinder_rot(end, 0.18, 0.02, 20, rot, lin(GUNMETAL));
        m.body
            .cylinder_rot(end + dir * 0.02, 0.12, 0.006, 16, rot, lin(SHADOW));
        m.glow.cuboid_rot(
            end + dir * 0.03 + Vec3::Y * 0.14,
            Vec3::new(0.1, 0.025, 0.004),
            Quat::from_rotation_arc(Vec3::Z, dir),
            lin(0x40d070),
        );
    }
    // Life-support units at the back corners.
    for x in [-1.1f32, 1.1] {
        let p = Vec3::new(x, y0, 1.15);
        m.body
            .cuboid(p + Vec3::Y * 0.16, Vec3::new(0.4, 0.32, 0.36), lin(PANEL));
        m.metal
            .cylinder(p + Vec3::Y * 0.32, 0.12, 0.04, 16, lin(SHADOW));
        m.metal.torus(p + Vec3::Y * 0.36, 0.12, 0.012, 16, lin(BRIGHT));
    }
    m.lamp = Vec3::new(1.25, y0 + 0.45, -1.25);
}

// ---------------------------------------------------------------------------------------
// Lab: clean research building with a glass dome over a spinning hologram, sensor dishes
// and sample tanks. Mk2 is larger with an extra wing.

fn lab(m: &mut Model, h: f32, mk2: bool) {
    foundation(m, h);
    let y0 = 0.08;
    let k = if mk2 { 1.45 } else { 1.0 };
    let accent = if mk2 { lin(BLUE) } else { lin(LAB) };
    let c = Vec3::new(0.0, y0, 0.08 * k);
    let (w, d, wh) = (1.35 * k, 1.1 * k, 0.55 * k);
    // Main block with chamfered upper band.
    m.body
        .cuboid(c + Vec3::Y * wh / 2.0, Vec3::new(w, wh, d), lin(WHITE));
    m.body.cuboid(
        c + Vec3::Y * (wh + 0.03),
        Vec3::new(w + 0.04, 0.06, d + 0.04),
        accent,
    );
    m.body.cuboid(
        c + Vec3::Y * 0.05,
        Vec3::new(w + 0.03, 0.1, d + 0.03),
        lin(GUNMETAL),
    );
    // Window band wrapping the front and sides.
    window_row(
        m,
        c + Vec3::new(0.0, wh * 0.55, -d / 2.0),
        Vec3::X,
        Vec3::NEG_Z,
        if mk2 { 5 } else { 3 },
        0.27 * k,
        Vec2::new(0.2, 0.14 * k),
    );
    for sx in [-1.0f32, 1.0] {
        window_row(
            m,
            c + Vec3::new(sx * w / 2.0, wh * 0.55, 0.0),
            Vec3::Z,
            Vec3::X * sx,
            if mk2 { 3 } else { 2 },
            0.3 * k,
            Vec2::new(0.2, 0.14 * k),
        );
    }
    // Entrance with a canopy.
    let door = c + Vec3::new(0.0, 0.0, -d / 2.0);
    m.body.cuboid(
        door + Vec3::new(-w * 0.32, 0.17, -0.006),
        Vec3::new(0.2, 0.3, 0.012),
        accent,
    );
    m.body.cuboid(
        door + Vec3::new(-w * 0.32, 0.34, -0.1),
        Vec3::new(0.32, 0.025, 0.2),
        lin(WHITE),
    );
    // Roof deck: dome over a hologram.
    let roof = c + Vec3::Y * (wh + 0.06);
    let dr = 0.42 * k;
    lathe(
        &mut m.metal,
        roof,
        &[
            (dr + 0.06, 0.0, lin(GUNMETAL)),
            (dr + 0.06, 0.06, lin(GUNMETAL)),
            (dr + 0.06, 0.06, lin(BRIGHT)),
            (dr, 0.08, lin(BRIGHT)),
        ],
        32,
    );
    let dome: Vec<(f32, f32, Rgba)> = (0..=10)
        .map(|i| {
            let a = i as f32 / 10.0 * FRAC_PI_2;
            (dr * a.cos(), 0.08 + dr * 0.9 * a.sin(), [0.75, 0.6, 0.95, 0.22])
        })
        .collect();
    lathe(&mut m.glass, roof, &dome, 32);
    for i in 0..8 {
        let a = i as f32 / 8.0 * TAU;
        let dd = Vec3::new(a.cos(), 0.0, a.sin());
        let pts: Vec<Vec3> = (0..=8)
            .map(|j| {
                let t = j as f32 / 8.0 * FRAC_PI_2;
                roof + Vec3::Y * 0.08 + dd * (dr + 0.008) * t.cos() + Vec3::Y * (dr * 0.9 + 0.008) * t.sin()
            })
            .collect();
        tube(&mut m.metal, &pts, 0.01, 5, lin(BRIGHT));
    }
    // Hologram: projector pedestal and spinning glowing rings/atom.
    m.metal
        .cylinder(roof + Vec3::Y * 0.08, 0.1 * k, 0.08, 16, lin(SHADOW));
    let mut holo = Part::new(roof + Vec3::Y * (0.16 + dr * 0.35), Anim::Spin(2.0));
    let hc = if mk2 { lin(0x6fc8ff) } else { lin(0xc49aff) };
    holo.glow.sphere(Vec3::ZERO, 0.06 * k, Vec3::ONE, 12, hc);
    for (i, tilt) in [0.0f32, 1.05, -1.05].into_iter().enumerate() {
        let mut ring = Geo::fine();
        ring.torus(Vec3::ZERO, 0.2 * k, 0.008, 32, scale(hc, 0.8 - i as f32 * 0.1));
        holo.glow.append(
            &ring,
            &Transform::from_rotation(Quat::from_rotation_y(tilt) * Quat::from_rotation_x(1.2)),
        );
    }
    m.parts.push(holo);
    // Sensor dishes on the roof corners.
    for (sx, sz) in [(-1.0f32, 1.0f32), (1.0, 1.0)] {
        let p = roof + Vec3::new(sx * (w / 2.0 - 0.15), 0.0, sz * (d / 2.0 - 0.15));
        m.metal.cylinder(p, 0.015, 0.16, 6, lin(BRIGHT));
        let aim = Quat::from_rotation_arc(Vec3::Y, Vec3::new(sx * 0.4, 1.0, -0.3).normalize());
        let mut dish = Geo::fine();
        lathe(
            &mut dish,
            Vec3::ZERO,
            &[
                (0.13, 0.05, lin(WHITE)),
                (0.09, 0.022, lin(WHITE)),
                (0.0, 0.0, lin(WHITE)),
            ],
            20,
        );
        lathe(
            &mut dish,
            Vec3::ZERO,
            &[
                (0.0, -0.01, lin(PANEL)),
                (0.09, 0.012, lin(PANEL)),
                (0.13, 0.04, lin(PANEL)),
            ],
            20,
        );
        m.body
            .append(&dish, &Transform::from_translation(p + Vec3::Y * 0.17).with_rotation(aim));
    }
    // Sample tanks with glowing fluid on the east side.
    let tanks = if mk2 { 3 } else { 2 };
    for i in 0..tanks {
        let p = Vec3::new(w / 2.0 + 0.2, y0, c.z - 0.25 * k + i as f32 * 0.28);
        if p.x > h - 0.1 {
            continue;
        }
        m.metal.cylinder(p, 0.1, 0.05, 16, lin(GUNMETAL));
        m.glass.cylinder(p + Vec3::Y * 0.05, 0.09, 0.32, 16, [0.7, 0.75, 0.85, 0.3]);
        m.glow
            .cylinder(p + Vec3::Y * 0.06, 0.075, 0.18 + 0.05 * i as f32, 14, scale(accent, 0.8));
        m.metal
            .cylinder(p + Vec3::Y * 0.37, 0.1, 0.04, 16, lin(GUNMETAL));
    }
    if mk2 {
        // Extra wing: a lower annex with a particle-accelerator ring on its roof.
        let a = Vec3::new(-w / 2.0 + 0.1, y0, d / 2.0 + 0.32);
        m.body
            .cuboid(a + Vec3::Y * 0.2, Vec3::new(0.9, 0.4, 0.5), lin(PANEL));
        m.metal
            .torus(a + Vec3::Y * 0.43, 0.2, 0.035, 32, lin(BRIGHT));
        m.glow.torus(a + Vec3::Y * 0.43, 0.2, 0.012, 32, lin(0x6fc8ff));
        m.metal.cylinder(a + Vec3::Y * 0.4, 0.06, 0.06, 12, lin(SHADOW));
    }
    m.lamp = Vec3::new(w / 2.0 - 0.1, wh + 0.2, c.z - d / 2.0 + 0.1);
}

// ---------------------------------------------------------------------------------------
// Observatory: drum base with a rotating dome, open slit and telescope tube.

fn observatory(m: &mut Model, h: f32) {
    foundation(m, h);
    let y0 = 0.08;
    let c = Vec3::new(0.0, y0, 0.05);
    let r = 0.68;
    lathe(
        &mut m.body,
        c,
        &[
            (r + 0.06, 0.0, lin(GUNMETAL)),
            (r + 0.06, 0.08, lin(GUNMETAL)),
            (r + 0.06, 0.08, lin(WHITE)),
            (r, 0.12, lin(WHITE)),
            (r, 0.55, lin(WHITE)),
            (r, 0.55, lin(BLUE)),
            (r + 0.03, 0.58, lin(BLUE)),
            (r + 0.03, 0.62, lin(BLUE)),
        ],
        40,
    );
    // Door and steps on the front.
    m.body.cuboid(
        c + Vec3::new(0.0, 0.22, -r - 0.005),
        Vec3::new(0.2, 0.3, 0.02),
        lin(BLUE),
    );
    m.glow.cuboid(
        c + Vec3::new(0.0, 0.42, -r - 0.012),
        Vec3::new(0.1, 0.04, 0.004),
        lin(WARM),
    );
    for k in 0..2 {
        m.rough.cuboid(
            c + Vec3::new(0.0, 0.025 - k as f32 * 0.0, -r - 0.08 - k as f32 * 0.08),
            Vec3::new(0.3, 0.05 - k as f32 * 0.02, 0.1),
            lin(CONCRETE),
        );
    }
    // Small round windows.
    for i in 0..6 {
        let a = i as f32 / 6.0 * TAU + 0.6;
        let d = Vec3::new(a.cos(), 0.0, a.sin());
        m.glow.cylinder_rot(
            c + d * r + Vec3::Y * 0.36,
            0.05,
            0.006,
            12,
            Quat::from_rotation_arc(Vec3::Y, d),
            lin(WARM),
        );
    }
    // Rotating dome with the slit and telescope.
    let mut dome = Part::new(c + Vec3::Y * 0.62, Anim::Idle(0.3));
    let dr = r - 0.02;
    // Dome shell built from slices, leaving a slit open around +X... the slit faces -Z.
    let slit = 0.16;
    let rows = 12;
    let cols = 36;
    for i in 0..rows {
        for j in 0..cols {
            let (t0, t1) = (i as f32 / rows as f32 * FRAC_PI_2, (i + 1) as f32 / rows as f32 * FRAC_PI_2);
            let (a0, a1) = (j as f32 / cols as f32 * TAU, (j + 1) as f32 / cols as f32 * TAU);
            let mid = (a0 + a1) / 2.0;
            // Slit: around angle -PI/2 (pointing -Z), skip panels within the slit width.
            let p = |t: f32, a: f32| Vec3::new(a.cos() * t.cos(), t.sin(), a.sin() * t.cos()) * dr;
            let cen = p((t0 + t1) / 2.0, mid);
            if cen.x.abs() < slit && cen.z < 0.0 {
                continue;
            }
            let ps = [p(t0, a0), p(t0, a1), p(t1, a1), p(t1, a0)];
            let n = cen.normalize();
            let shade = 0.9 + 0.1 * (t0 / FRAC_PI_2);
            flat(&mut dome.body, ps, n, scale(lin(WHITE), shade));
        }
    }
    // Slit edges and shutter rails.
    for sx in [-1.0f32, 1.0] {
        let pts: Vec<Vec3> = (0..=10)
            .map(|i| {
                let t = i as f32 / 10.0 * FRAC_PI_2;
                let zz = -((dr * t.cos()).powi(2) - slit * slit).max(0.0).sqrt();
                Vec3::new(sx * slit, dr * t.sin(), zz)
            })
            .collect();
        tube(&mut dome.metal, &pts, 0.018, 6, lin(GUNMETAL));
    }
    dome.metal.torus(Vec3::ZERO, dr + 0.01, 0.025, 40, lin(GUNMETAL));
    // Telescope: tube on a fork mount, pointing out of the slit.
    dome.metal.cylinder(Vec3::ZERO, 0.08, 0.18, 12, lin(SHADOW));
    let tilt = Quat::from_rotation_x(-0.7);
    let tb = Vec3::new(0.0, 0.25, 0.05);
    for sx in [-1.0f32, 1.0] {
        dome.metal.cuboid(
            Vec3::new(sx * 0.11, 0.2, 0.05),
            Vec3::new(0.03, 0.14, 0.06),
            lin(GUNMETAL),
        );
    }
    dome.body.cylinder_rot(
        tb - tilt * Vec3::Y * 0.2,
        0.085,
        0.72,
        20,
        tilt,
        lin(PANEL),
    );
    for t in [-0.15f32, 0.3] {
        dome.metal
            .cylinder_rot(tb + tilt * Vec3::Y * t, 0.093, 0.04, 20, tilt, lin(BLUE));
    }
    dome.body.cylinder_rot(
        tb + tilt * Vec3::Y * 0.52,
        0.07,
        0.006,
        16,
        tilt,
        lin(SHADOW),
    );
    m.parts.push(dome);
    // Weather mast.
    let mast = Vec3::new(0.82, y0, 0.82);
    m.metal.cylinder(mast, 0.015, 0.7, 6, lin(BRIGHT));
    for k in 0..3 {
        let a = k as f32 / 3.0 * TAU;
        rod(
            &mut m.metal,
            mast + Vec3::Y * 0.7,
            mast + Vec3::new(a.cos() * 0.08, 0.72, a.sin() * 0.08),
            0.006,
            4,
            lin(BRIGHT),
        );
        m.body.sphere(
            mast + Vec3::new(a.cos() * 0.08, 0.72, a.sin() * 0.08),
            0.018,
            Vec3::ONE,
            8,
            lin(WHITE),
        );
    }
    m.lamp = Vec3::new(-0.8, y0 + 0.2, -0.8);
}

// ---------------------------------------------------------------------------------------
// Shipyard: launch pad with flame trench and blast ring, gantry tower with swing arms,
// fuel tank farm and a parked rocket. The launched rocket spawns at the pad centre
// (+0.3, 0.24, +0.3) — keep the pad there.

fn shipyard(m: &mut Model, h: f32) {
    foundation(m, h);
    let y0 = 0.08;
    let pad = Vec3::new(0.3, y0, 0.3);
    // Pad: concrete disc with steel deck, hazard ring and flame deflector slot.
    lathe(
        &mut m.rough,
        pad,
        &[
            (1.45, 0.0, lin(CONCRETE)),
            (1.45, 0.1, lin(CONCRETE)),
            (1.45, 0.1, scale(lin(CONCRETE), 1.1)),
            (1.3, 0.14, scale(lin(CONCRETE), 1.1)),
            (1.3, 0.14, lin(0x55606c)),
            (0.0, 0.14, lin(0x55606c)),
        ],
        48,
    );
    m.metal.torus(pad + Vec3::Y * 0.16, 1.1, 0.04, 48, lin(YELLOW));
    for k in 0..16 {
        let a = k as f32 / 16.0 * TAU;
        let d = Vec3::new(a.cos(), 0.0, a.sin());
        m.body.cuboid_rot(
            pad + d * 1.37 + Vec3::Y * 0.12,
            Vec3::new(0.12, 0.012, 0.06),
            Quat::from_rotation_y(-a),
            if k % 2 == 0 { lin(YELLOW) } else { lin(0x22262c) },
        );
    }
    // Flame trench running east-west through the pad.
    m.rough.cuboid(
        pad + Vec3::new(0.0, 0.142, 0.0),
        Vec3::new(2.5, 0.004, 0.38),
        scale(lin(SHADOW), 0.6),
    );
    for sz in [-1.0f32, 1.0] {
        m.metal.cuboid(
            pad + Vec3::new(0.0, 0.16, sz * 0.2),
            Vec3::new(2.5, 0.04, 0.03),
            lin(GUNMETAL),
        );
    }
    // Launch mount: four hold-down posts around the rocket.
    for k in 0..4 {
        let a = k as f32 / 4.0 * TAU + PI / 4.0;
        let d = Vec3::new(a.cos(), 0.0, a.sin());
        m.metal
            .cuboid(pad + d * 0.42 + Vec3::Y * 0.24, Vec3::new(0.1, 0.2, 0.1), lin(GUNMETAL));
        m.glow.cuboid(
            pad + d * 0.47 + Vec3::Y * 0.3,
            Vec3::new(0.02, 0.02, 0.02),
            lin(0xff9a3a),
        );
    }
    // Parked rocket on the pad.
    let (rb, rf) = rocket();
    let _ = rf;
    m.body.append(&rb, &Transform::from_translation(pad + Vec3::Y * 0.24).with_scale(Vec3::splat(0.9)));

    // Gantry tower at the north-west with service arms reaching the rocket.
    let gt = Vec3::new(-1.35, y0, -1.15);
    let top = 3.1;
    let corners = [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
    let hw = 0.28;
    for (sx, sz) in corners {
        beam(
            &mut m.metal,
            gt + Vec3::new(sx * hw, 0.0, sz * hw),
            gt + Vec3::new(sx * hw, top, sz * hw),
            0.06,
            0.06,
            lin(RED),
        );
    }
    let levels = 8;
    for l in 0..=levels {
        let y = 0.1 + (top - 0.1) * l as f32 / levels as f32;
        for i in 0..4 {
            let (ax, az) = corners[i];
            let (bx, bz) = corners[(i + 1) % 4];
            let a = gt + Vec3::new(ax * hw, y, az * hw);
            let b = gt + Vec3::new(bx * hw, y, bz * hw);
            beam(&mut m.metal, a, b, 0.03, 0.03, lin(RED));
            if l < levels {
                let y2 = 0.1 + (top - 0.1) * (l + 1) as f32 / levels as f32;
                let b2 = gt + Vec3::new(bx * hw, y2, bz * hw);
                beam(&mut m.metal, a, b2, 0.02, 0.02, lin(GUNMETAL));
            }
        }
        if l % 2 == 0 {
            m.metal.cuboid(
                gt + Vec3::Y * y,
                Vec3::new(2.0 * hw, 0.02, 2.0 * hw),
                lin(SHADOW),
            );
        }
    }
    // Service arms toward the rocket (pad centre).
    for (y, len) in [(1.2f32, 1.0f32), (2.2, 1.0)] {
        let from = gt + Vec3::new(hw, y, hw);
        let to_dir = (pad - gt).with_y(0.0).normalize();
        let to = from + to_dir * len;
        beam(&mut m.metal, from, to, 0.12, 0.06, lin(RED));
        beam(&mut m.metal, from + Vec3::Y * 0.12, to + Vec3::Y * 0.04, 0.02, 0.02, lin(GUNMETAL));
        m.glow
            .sphere(to + Vec3::Y * 0.05, 0.02, Vec3::ONE, 6, lin(0xffb040));
    }
    // Crane boom and warning beacon on top.
    let ct = gt + Vec3::Y * top;
    m.metal.cylinder(ct, 0.08, 0.1, 12, lin(SHADOW));
    beam(&mut m.metal, ct + Vec3::new(-0.3, 0.12, 0.0), ct + Vec3::new(0.9, 0.12, 0.6), 0.06, 0.08, lin(YELLOW));
    m.glow
        .sphere(ct + Vec3::Y * 0.25, 0.04, Vec3::ONE, 8, lin(RED));
    m.metal.cylinder(ct + Vec3::Y * 0.1, 0.012, 0.14, 6, lin(BRIGHT));

    // Fuel tank farm at the north-east: two spheres and a horizontal tank, piped to pad.
    for (k, p) in [Vec3::new(1.3, y0, -1.3), Vec3::new(0.65, y0, -1.45)].into_iter().enumerate() {
        let r = 0.3 - k as f32 * 0.05;
        for i in 0..4 {
            let a = i as f32 / 4.0 * TAU + PI / 4.0;
            let leg = p + Vec3::new(a.cos() * r * 0.75, 0.0, a.sin() * r * 0.75);
            m.metal.cylinder(leg, 0.025, r + 0.12, 6, lin(GUNMETAL));
        }
        m.body
            .sphere(p + Vec3::Y * (r + 0.2), r, Vec3::ONE, 20, lin(WHITE));
        m.metal
            .torus(p + Vec3::Y * (r + 0.2), r + 0.005, 0.015, 32, lin(BLUE));
        tube(
            &mut m.metal,
            &[
                p + Vec3::Y * 0.12,
                Vec3::new(p.x, y0 + 0.1, -0.7),
                pad + Vec3::new(0.3, 0.12, -0.5),
            ],
            0.025,
            8,
            lin(BLUE),
        );
    }
    // Assembly hangar at the south-west with a lit roller door.
    let hg = Vec3::new(-1.3, y0, 1.25);
    m.body
        .cuboid(hg + Vec3::Y * 0.35, Vec3::new(1.2, 0.7, 1.2), lin(PANEL));
    let mut roof = Geo::fine();
    let prof: Vec<(f32, f32, Rgba)> = (0..=8)
        .map(|k| {
            let a = k as f32 / 8.0 * PI;
            (a.cos() * 0.6, a.sin() * 0.22, lin(0x8a96a2))
        })
        .collect();
    extrude_z(&mut roof, &prof, -0.6, 0.6, true);
    m.body.append(&roof, &Transform::from_translation(hg + Vec3::Y * 0.7));
    m.body.cuboid(
        hg + Vec3::new(0.0, 0.3, -0.605),
        Vec3::new(0.7, 0.55, 0.012),
        lin(GUNMETAL),
    );
    for k in 0..8 {
        m.metal.cuboid(
            hg + Vec3::new(0.0, 0.06 + k as f32 * 0.07, -0.614),
            Vec3::new(0.68, 0.01, 0.004),
            lin(SHADOW),
        );
    }
    m.glow.cuboid(
        hg + Vec3::new(0.0, 0.62, -0.612),
        Vec3::new(0.6, 0.03, 0.004),
        lin(WARM),
    );
    hazard(
        &mut m.body,
        hg + Vec3::new(-0.4, 0.002, -0.85),
        Vec3::X,
        Vec3::Z,
        Vec3::Y,
        0.8,
        0.18,
        0.1,
        lin(YELLOW),
        lin(0x22262c),
    );
    m.lamp = Vec3::new(1.75, y0 + 0.3, 1.75);
}

// ---------------------------------------------------------------------------------------
// Turret: armoured octagonal base with a rotating twin-barrel head (Anim::Aim, barrels
// toward -Z from the pivot).

fn turret(m: &mut Model) {
    let y0 = 0.0;
    // Octagonal armoured base.
    lathe(
        &mut m.body,
        Vec3::Y * y0,
        &[
            (0.46, 0.0, scale(lin(ARMOUR), 0.8)),
            (0.46, 0.12, scale(lin(ARMOUR), 0.9)),
            (0.46, 0.12, lin(ARMOUR)),
            (0.36, 0.34, lin(ARMOUR)),
            (0.36, 0.34, lin(GUNMETAL)),
            (0.3, 0.4, lin(GUNMETAL)),
        ],
        8,
    );
    // Hazard band and bolts on the skirt.
    for k in 0..8 {
        let a = (k as f32 + 0.5) / 8.0 * TAU;
        let d = Vec3::new(a.cos(), 0.0, a.sin());
        let r = 0.46 * (PI / 8.0).cos();
        if k % 2 == 0 {
            m.body.cuboid_rot(
                d * (r + 0.003) + Vec3::Y * 0.06,
                Vec3::new(0.15, 0.05, 0.006),
                Quat::from_rotation_y(-a + FRAC_PI_2),
                lin(YELLOW),
            );
        }
        bolt(&mut m.metal, d * 0.4 + Vec3::Y * 0.2, (d + Vec3::Y * 0.9).normalize(), 0.015, lin(BRIGHT));
    }
    // Ring bearing.
    m.metal.cylinder(Vec3::Y * 0.4, 0.26, 0.08, 24, lin(BRIGHT));
    m.metal.torus(Vec3::Y * 0.46, 0.26, 0.015, 24, lin(SHADOW));

    let mut head = Part::new(Vec3::new(0.0, 0.58, 0.0), Anim::Aim);
    // Angular housing: wedge front, flat back.
    let mut shell = Geo::fine();
    extrude_z(
        &mut shell,
        &[
            (-0.2, -0.1, lin(ARMOUR)),
            (0.2, -0.1, lin(ARMOUR)),
            (0.24, 0.0, lin(ARMOUR)),
            (0.18, 0.13, scale(lin(ARMOUR), 1.12)),
            (-0.18, 0.13, scale(lin(ARMOUR), 1.12)),
            (-0.24, 0.0, lin(ARMOUR)),
        ],
        -0.18,
        0.24,
        true,
    );
    head.body.append(&shell, &Transform::IDENTITY);
    // Sloped front armour plate.
    head.body.cuboid_rot(
        Vec3::new(0.0, 0.03, -0.2),
        Vec3::new(0.42, 0.2, 0.05),
        Quat::from_rotation_x(0.5),
        scale(lin(ARMOUR), 1.05),
    );
    // Twin barrels with muzzle brakes and recoil sleeves.
    let fwd = Quat::from_rotation_x(-FRAC_PI_2);
    for sx in [-1.0f32, 1.0] {
        let b = Vec3::new(sx * 0.09, 0.02, -0.18);
        head.metal.cylinder_rot(b, 0.045, 0.1, 14, fwd, lin(SHADOW));
        head.metal
            .cylinder_rot(b + Vec3::NEG_Z * 0.1, 0.026, 0.38, 12, fwd, lin(GUNMETAL));
        head.metal.cylinder_rot(
            b + Vec3::NEG_Z * 0.46,
            0.036,
            0.07,
            12,
            fwd,
            lin(SHADOW),
        );
        head.glow.cylinder_rot(
            b + Vec3::NEG_Z * 0.531,
            0.016,
            0.003,
            10,
            fwd,
            lin(0xff7a3a),
        );
    }
    // Sensor eye, ammo drum, antenna.
    head.body.cuboid(Vec3::new(0.0, 0.17, -0.02), Vec3::new(0.14, 0.08, 0.14), lin(SHADOW));
    head.glow.cuboid(Vec3::new(0.0, 0.17, -0.092), Vec3::new(0.08, 0.025, 0.004), lin(RED));
    head.body.cylinder_rot(
        Vec3::new(-0.24, 0.0, 0.08),
        0.08,
        0.08,
        16,
        Quat::from_rotation_z(FRAC_PI_2),
        lin(YELLOW),
    );
    head.metal.cylinder(Vec3::new(0.12, 0.13, 0.18), 0.008, 0.22, 4, lin(BRIGHT));
    hazard(
        &mut head.body,
        Vec3::new(-0.18, 0.131, 0.16),
        Vec3::X,
        Vec3::Z,
        Vec3::Y,
        0.36,
        0.06,
        0.05,
        lin(YELLOW),
        lin(0x22262c),
    );
    m.parts.push(head);
    m.lamp = Vec3::new(0.32, 0.2, 0.32);
}

// ---------------------------------------------------------------------------------------
// Rocket

/// Rocket launched from the shipyard: (body, exhaust flame). Stands on y = 0, nose up;
/// the flame hangs below y = 0.
pub fn rocket() -> (Geo, Geo) {
    let mut g = Geo::fine();
    let white = lin(0xe9edf1);
    let dark = lin(0x30363e);
    // Engine bell cluster under a skirt.
    for i in 0..4 {
        let a = i as f32 / 4.0 * TAU + PI / 4.0;
        let p = Vec3::new(a.cos() * 0.13, 0.0, a.sin() * 0.13);
        lathe(
            &mut g,
            p,
            &[
                (0.09, -0.12, lin(GUNMETAL)),
                (0.06, 0.0, lin(GUNMETAL)),
                (0.04, 0.08, dark),
            ],
            14,
        );
    }
    // Body: first stage, interstage, second stage, payload fairing.
    let prof = [
        (0.3, 0.05, dark),
        (0.33, 0.12, dark),
        (0.33, 0.12, white),
        (0.33, 1.45, white),
        (0.33, 1.45, dark),
        (0.31, 1.58, dark),
        (0.31, 1.58, white),
        (0.31, 2.35, white),
        (0.31, 2.35, lin(BLUE)),
        (0.315, 2.42, lin(BLUE)),
        (0.315, 2.42, white),
        (0.3, 2.6, white),
        (0.26, 2.8, white),
        (0.19, 2.98, white),
        (0.1, 3.1, white),
        (0.0, 3.16, white),
    ];
    lathe(&mut g, Vec3::ZERO, &prof, 28);
    // Black roll pattern bands and a flag stripe.
    for (y, hgt) in [(0.95f32, 0.12f32), (2.05, 0.1)] {
        lathe(
            &mut g,
            Vec3::ZERO,
            &[(0.332, y, dark), (0.332, y + hgt, dark)],
            28,
        );
    }
    g.cuboid(
        Vec3::new(0.0, 0.7, -0.33),
        Vec3::new(0.08, 0.5, 0.012),
        lin(RED),
    );
    // Four fins.
    for i in 0..4 {
        let a = i as f32 / 4.0 * TAU;
        let d = Vec3::new(a.cos(), 0.0, a.sin());
        let side = Vec3::new(-d.z, 0.0, d.x);
        let p = |r: f32, y: f32, s: f32| d * r + Vec3::Y * y + side * s;
        for s in [-0.015f32, 0.015] {
            flat(
                &mut g,
                [p(0.3, 0.1, s), p(0.62, 0.0, s), p(0.55, 0.32, s), p(0.3, 0.7, s)],
                side * s.signum(),
                lin(RED),
            );
        }
        beam(&mut g, p(0.62, 0.0, 0.0), p(0.55, 0.32, 0.0), 0.03, 0.02, dark);
        beam(&mut g, p(0.55, 0.32, 0.0), p(0.3, 0.7, 0.0), 0.03, 0.02, lin(RED));
    }
    // Capsule windows.
    for i in 0..3 {
        let a = -FRAC_PI_2 + (i as f32 - 1.0) * 0.35;
        let d = Vec3::new(a.cos(), 0.0, a.sin());
        g.cylinder_rot(
            d * 0.29 + Vec3::Y * 2.7,
            0.035,
            0.03,
            10,
            Quat::from_rotation_arc(Vec3::Y, d),
            dark,
        );
    }

    let mut flame = Geo::fine();
    flame.cone(Vec3::Y * -0.08, 0.26, -1.4, 16, 0.0, scale(lin(0xffa040), 1.4));
    flame.cone(Vec3::Y * -0.08, 0.16, -0.9, 14, 0.3, scale(lin(0xfff0c0), 1.6));
    flame.sphere(
        Vec3::Y * -0.1,
        0.3,
        Vec3::new(1.0, 0.5, 1.0),
        14,
        scale(lin(0xffd080), 1.4),
    );
    (g, flame)
}
