//! Extraction and processing: miners, smelters, assemblers, refinery.
//!
//! Models face north (-Z), are centred on their footprint and stand on y = 0.

use bevy::prelude::*;
use sim::BuildingKind;
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI, TAU};

use super::kit::*;
use super::{Anim, Model, Part};
use crate::mesh_util::*;

/// Builds `kind` into `m`; `s` is the footprint edge length in tiles.
pub fn build(kind: BuildingKind, s: f32, m: &mut Model) {
    use BuildingKind as K;
    match kind {
        K::Miner | K::MinerMk2 => miner(m, s, kind == K::MinerMk2),
        K::Smelter | K::SmelterMk2 => smelter(m, s, kind == K::SmelterMk2),
        K::Assembler | K::AssemblerMk2 => old_assembler(m, s, kind == K::AssemblerMk2),
        K::Refinery => old_refinery(m, s),
        _ => unreachable!("{kind:?} is not a production building"),
    }
}

// ---------------------------------------------------------------------------------------
// Palette additions (kit colours are the shared base).

/// Bare steel for frames, pipes and bolts (the metal layer is tinted by this).
const IRON: u32 = 0x8d96a1;
/// Darker gunmetal for heavy machinery parts.
const GUNMETAL: u32 = 0x3c434d;
/// Black rubber for cables and hoses.
const RUBBER: u32 = 0x1d2025;
/// Foundation concrete.
const SLAB: u32 = 0x77736c;
/// Raw ore in hoppers.
const ORE: u32 = 0x5b4a3e;

// ---------------------------------------------------------------------------------------
// Geometry helpers.

/// Rotation taking +Y onto `d`.
fn up_to(d: Vec3) -> Quat {
    Quat::from_rotation_arc(Vec3::Y, d.normalize_or(Vec3::Y))
}

/// Cheap unbevelled box (12 triangles) for small details.
fn blk(g: &mut Geo, c: Vec3, size: Vec3, col: Rgba) {
    blk_rot(g, c, size, Quat::IDENTITY, col);
}

fn blk_rot(g: &mut Geo, c: Vec3, size: Vec3, rot: Quat, col: Rgba) {
    let mut t = Geo::default();
    t.cuboid_rot(c, size, rot, col);
    g.append(&t, &Transform::IDENTITY);
}

/// Square bar of width `w` from `a` to `b`.
fn bar(g: &mut Geo, a: Vec3, b: Vec3, w: f32, col: Rgba) {
    let d = b - a;
    blk_rot(
        g,
        (a + b) / 2.0,
        Vec3::new(w, d.length(), w),
        level_to(d),
        col,
    );
}

/// Rotation taking +Y onto `d` while keeping local X horizontal (square sections stay
/// square to the ground instead of turning into diamonds).
fn level_to(d: Vec3) -> Quat {
    let y = d.normalize_or(Vec3::Y);
    let x = if y.y.abs() > 0.995 {
        Vec3::X
    } else {
        Vec3::Y.cross(y).normalize()
    };
    Quat::from_mat3(&Mat3::from_cols(x, y, x.cross(y)))
}

/// Cheap (unchamfered) cylinder from `a` to `b`.
fn rod(g: &mut Geo, a: Vec3, b: Vec3, r: f32, segs: u32, col: Rgba) {
    let d = b - a;
    let mut t = Geo::default();
    t.cylinder_rot(a, r, d.length(), segs, up_to(d), col);
    g.append(&t, &Transform::IDENTITY);
}

/// Triangle from existing vertices, wound to face along their normals.
fn tri_auto(g: &mut Geo, a: u32, b: u32, c: u32) {
    let p = |i: u32| Vec3::from(g.pos[i as usize]);
    let n = |i: u32| Vec3::from(g.nrm[i as usize]);
    let geo = (p(b) - p(a)).cross(p(c) - p(a));
    if geo.dot(n(a) + n(b) + n(c)) < 0.0 {
        g.idx.extend_from_slice(&[a, c, b]);
    } else {
        g.idx.extend_from_slice(&[a, b, c]);
    }
}

/// Flat quad facing `n` whatever the corner order (corners go around the rim).
fn face(g: &mut Geo, p: [Vec3; 4], n: Vec3, col: Rgba) {
    let i = g.pos.len() as u32;
    for q in p {
        g.vertex(q, n, col);
    }
    tri_auto(g, i, i + 1, i + 2);
    tri_auto(g, i, i + 2, i + 3);
}

/// Subtle per-vertex value noise so large smooth surfaces are not flat colour.
fn grain(p: Vec3) -> f32 {
    0.94 + 0.12 * noise(p.x * 6.0 + p.y * 2.9, p.z * 6.0 + p.y * 4.7, 11)
}

/// Smooth surface of revolution about the vertical axis through `base`. `prof` lists
/// (radius, height) bottom to top; bends sharper than ~40° stay crisp. `ecol(i)` colours
/// the edge from point `i` to `i + 1`.
fn lathe_e(g: &mut Geo, base: Vec3, prof: &[(f32, f32)], segs: u32, ecol: impl Fn(usize) -> Rgba) {
    let n = prof.len();
    if n < 2 {
        return;
    }
    let en: Vec<Vec2> = (0..n - 1)
        .map(|i| {
            let t = Vec2::new(prof[i + 1].0 - prof[i].0, prof[i + 1].1 - prof[i].1)
                .normalize_or(Vec2::Y);
            Vec2::new(t.y, -t.x)
        })
        .collect();
    let w = segs + 1;
    for i in 0..n - 1 {
        let blend = |j: usize| {
            if en[i].dot(en[j]) > 0.76 {
                (en[i] + en[j]).normalize()
            } else {
                en[i]
            }
        };
        let n0 = if i > 0 { blend(i - 1) } else { en[i] };
        let n1 = if i + 2 < n { blend(i + 1) } else { en[i] };
        let c = ecol(i);
        let start = g.pos.len() as u32;
        for (pt, nn) in [(prof[i], n0), (prof[i + 1], n1)] {
            for k in 0..=segs {
                let a = k as f32 / segs as f32 * TAU;
                let d = Vec3::new(a.cos(), 0.0, a.sin());
                let nrm = (d * nn.x + Vec3::Y * nn.y).normalize_or(Vec3::Y);
                let p = base + d * pt.0 + Vec3::Y * pt.1;
                let tint = 1.0 + 0.08 * nrm.y.max(0.0) - 0.4 * (-nrm.y).max(0.0);
                g.vertex(p, nrm, scale(c, tint * grain(p)));
            }
        }
        for k in 0..segs {
            let (a, b) = (start + k, start + k + 1);
            let (c2, d) = (a + w, b + w);
            g.idx.extend_from_slice(&[a, c2, d, a, d, b]);
        }
    }
}

fn lathe(g: &mut Geo, base: Vec3, prof: &[(f32, f32)], segs: u32, col: Rgba) {
    lathe_e(g, base, prof, segs, |_| col);
}

/// Thin flat band (a short open tube) around a vertical axis.
fn band(g: &mut Geo, base: Vec3, r: f32, h: f32, t: f32, segs: u32, col: Rgba) {
    lathe(
        g,
        base,
        &[(r - 0.002, 0.0), (r + t, 0.0), (r + t, h), (r - 0.002, h)],
        segs,
        col,
    );
}

/// Tube swept along a polyline, with corners rounded to radius `bend`.
fn pipe(g: &mut Geo, pts: &[Vec3], r: f32, bend: f32, segs: u32, col: Rgba) {
    if pts.len() < 2 {
        return;
    }
    let mut path = vec![pts[0]];
    for i in 1..pts.len() - 1 {
        let (a, p, b) = (pts[i - 1], pts[i], pts[i + 1]);
        let (d0, d1) = (p - a, b - p);
        let cut = bend.min(d0.length() * 0.5).min(d1.length() * 0.5);
        let s = p - d0.normalize_or_zero() * cut;
        let e = p + d1.normalize_or_zero() * cut;
        for k in 0..=4 {
            let t = k as f32 / 4.0;
            path.push(s * (1.0 - t) * (1.0 - t) + p * 2.0 * t * (1.0 - t) + e * t * t);
        }
    }
    path.push(pts[pts.len() - 1]);
    path.dedup_by(|a, b| a.distance(*b) < 1e-4);
    let n = path.len();
    let tangent =
        |i: usize| (path[(i + 1).min(n - 1)] - path[i.saturating_sub(1)]).normalize_or(Vec3::Y);
    let t0 = tangent(0);
    let mut refv = if t0.y.abs() < 0.9 { Vec3::Y } else { Vec3::X };
    let w = segs + 1;
    let start = g.pos.len() as u32;
    for (i, &p) in path.iter().enumerate() {
        let t = tangent(i);
        refv = (refv - t * refv.dot(t)).normalize_or(Vec3::X);
        let bin = t.cross(refv);
        for k in 0..=segs {
            let a = k as f32 / segs as f32 * TAU;
            let d = refv * a.cos() + bin * a.sin();
            let q = p + d * r;
            g.vertex(
                q,
                d,
                scale(
                    col,
                    grain(q) * (1.0 + 0.08 * d.y.max(0.0) - 0.35 * (-d.y).max(0.0)),
                ),
            );
        }
    }
    for i in 0..n as u32 - 1 {
        for k in 0..segs {
            let a = start + i * w + k;
            let (b, c, d) = (a + 1, a + w, a + w + 1);
            tri_auto(g, a, c, d);
            tri_auto(g, a, d, b);
        }
    }
}

/// Low-poly torus around an arbitrary axis (rot * Y).
fn ring(g: &mut Geo, c: Vec3, major: f32, minor: f32, segs: u32, sides: u32, rot: Quat, col: Rgba) {
    let start = g.pos.len() as u32;
    for s in 0..=segs {
        let u = s as f32 / segs as f32 * TAU;
        let dir = Vec3::new(u.cos(), 0.0, u.sin());
        for k in 0..=sides {
            let v = k as f32 / sides as f32 * TAU;
            let n = dir * v.cos() + Vec3::Y * v.sin();
            let p = dir * major + n * minor;
            g.vertex(c + rot * p, rot * n, col);
        }
    }
    let w = sides + 1;
    for s in 0..segs {
        for k in 0..sides {
            let i = start + s * w + k;
            tri_auto(g, i, i + 1, i + w + 1);
            tri_auto(g, i, i + w + 1, i + w);
        }
    }
}

/// Diagonal hazard stripes on the rectangle `o + u*[0,len] + v*[0,wid]`, facing `n`.
/// Draws only the yellow stripes: put them just above a dark surface.
fn stripes(g: &mut Geo, o: Vec3, u: Vec3, v: Vec3, len: f32, wid: f32, n: Vec3, col: Rgba) {
    let pitch = 0.14;
    let sw = 0.065;
    let slant = wid * 0.7;
    let mut x = 0.0;
    while x + sw + slant <= len + 1e-4 {
        face(
            g,
            [
                o + u * x,
                o + u * (x + sw),
                o + u * (x + sw + slant) + v * wid,
                o + u * (x + slant) + v * wid,
            ],
            n,
            col,
        );
        x += pitch;
    }
}

/// Right-handed (u, up) axes on a face with normal `n`, `up` pointing skywards.
fn face_axes(u: Vec3, n: Vec3) -> (Vec3, Vec3) {
    let up = n.cross(u).normalize_or(Vec3::Y);
    if up.y < 0.0 { (-u, -up) } else { (u, up) }
}

/// Chamfered (fine) cylinder from `a` to `b`.
fn cyl(g: &mut Geo, a: Vec3, b: Vec3, r: f32, segs: u32, col: Rgba) {
    let d = b - a;
    g.cylinder_rot(a, r, d.length(), segs, up_to(d), col);
}

/// Grille: a dark recess with horizontal slats, on a face with outward normal `n`.
/// `c` is the centre on the face, `u` the horizontal direction along the face.
fn grille(g: &mut Geo, c: Vec3, u: Vec3, n: Vec3, w: f32, h: f32, slats: u32, frame: Rgba) {
    let (u, up) = face_axes(u, n);
    face(
        g,
        [
            c - u * w / 2.0 - up * h / 2.0 + n * 0.003,
            c + u * w / 2.0 - up * h / 2.0 + n * 0.003,
            c + u * w / 2.0 + up * h / 2.0 + n * 0.003,
            c - u * w / 2.0 + up * h / 2.0 + n * 0.003,
        ],
        n,
        lin(0x15181c),
    );
    let rot = Quat::from_mat3(&Mat3::from_cols(u, up, n));
    for k in 0..slats {
        let y = -h / 2.0 + h * (k as f32 + 0.5) / slats as f32;
        blk_rot(
            g,
            c + up * y + n * 0.012,
            Vec3::new(w, h / slats as f32 * 0.45, 0.02),
            rot * Quat::from_rotation_x(-0.5),
            frame,
        );
    }
}

/// Ladder from `a` (bottom centre) to `b` (top centre); `side` is the rail spacing axis.
fn ladder(g: &mut Geo, a: Vec3, b: Vec3, side: Vec3, col: Rgba) {
    let hw = 0.065;
    bar(g, a - side * hw, b - side * hw, 0.018, col);
    bar(g, a + side * hw, b + side * hw, 0.018, col);
    let len = a.distance(b);
    let n = (len / 0.11).floor() as i32;
    for k in 1..=n {
        let p = a + (b - a) * (k as f32 / (n + 1) as f32);
        bar(g, p - side * hw, p + side * hw, 0.012, col);
    }
}

/// Guard rail along `a`→`b` (points at deck level).
fn railing(g: &mut Geo, a: Vec3, b: Vec3, h: f32, col: Rgba) {
    let len = a.distance(b);
    let n = ((len / 0.32).ceil() as i32).max(1);
    for k in 0..=n {
        let p = a + (b - a) * (k as f32 / n as f32);
        bar(g, p, p + Vec3::Y * h, 0.02, col);
    }
    bar(g, a + Vec3::Y * h, b + Vec3::Y * h, 0.022, col);
    bar(g, a + Vec3::Y * h * 0.5, b + Vec3::Y * h * 0.5, 0.016, col);
}

/// Hanging cable between two points (a sagging catenary-like curve).
#[allow(dead_code)]
fn cable(g: &mut Geo, a: Vec3, b: Vec3, sag: f32, r: f32, col: Rgba) {
    let pts: Vec<Vec3> = (0..=8)
        .map(|k| {
            let t = k as f32 / 8.0;
            a.lerp(b, t) - Vec3::Y * sag * 4.0 * t * (1.0 - t)
        })
        .collect();
    pipe(g, &pts, r, 0.0, 5, col);
}

/// Bolt heads in a circle (looking along +Y).
fn bolt_ring(g: &mut Geo, c: Vec3, r: f32, n: u32, size: f32, col: Rgba) {
    for k in 0..n {
        let a = k as f32 / n as f32 * TAU;
        blk_rot(
            g,
            c + Vec3::new(a.cos(), 0.0, a.sin()) * r,
            Vec3::new(size, size * 0.6, size),
            Quat::from_rotation_y(-a),
            col,
        );
    }
}

/// Glowing screen with a few UI lines, on a face with normal `n` (horizontal `u`).
fn screen(g: &mut Geo, c: Vec3, u: Vec3, n: Vec3, w: f32, h: f32, tint: u32, seed: u64) {
    let (u, up) = face_axes(u, n);
    let p = |x: f32, y: f32, z: f32| c + u * x + up * y + n * z;
    face(
        g,
        [
            p(-w / 2.0, -h / 2.0, 0.004),
            p(w / 2.0, -h / 2.0, 0.004),
            p(w / 2.0, h / 2.0, 0.004),
            p(-w / 2.0, h / 2.0, 0.004),
        ],
        n,
        scale(lin(tint), 0.16),
    );
    let lines = 3;
    for k in 0..lines {
        let y = h / 2.0 - h * (k as f32 + 0.8) / (lines as f32 + 0.6);
        let len = w * (0.35 + 0.5 * hash01(k as i64, 3, seed));
        face(
            g,
            [
                p(-w * 0.4, y - h * 0.05, 0.007),
                p(-w * 0.4 + len, y - h * 0.05, 0.007),
                p(-w * 0.4 + len, y + h * 0.05, 0.007),
                p(-w * 0.4, y + h * 0.05, 0.007),
            ],
            n,
            scale(lin(tint), 0.75),
        );
    }
}

/// Concrete foundation slab with steel edge trim, a hazard kerb on the output (front)
/// edge and anchor plates in the corners. Returns the slab top height.
fn foundation(m: &mut Model, s: f32) -> f32 {
    let e = s / 2.0 - 0.04;
    let top = 0.1;
    m.rough.cuboid(
        Vec3::new(0.0, top / 2.0, 0.0),
        Vec3::new(2.0 * e, top, 2.0 * e),
        lin(SLAB),
    );
    let trim = lin(IRON);
    for sgn in [-1.0f32, 1.0] {
        blk(
            &mut m.metal,
            Vec3::new(0.0, top - 0.012, sgn * (e - 0.012)),
            Vec3::new(2.0 * e + 0.004, 0.028, 0.028),
            trim,
        );
        blk(
            &mut m.metal,
            Vec3::new(sgn * (e - 0.012), top - 0.012, 0.0),
            Vec3::new(0.028, 0.028, 2.0 * e + 0.004),
            trim,
        );
    }
    // Hazard kerb on the output edge.
    let kz = -e + 0.07;
    blk(
        &mut m.body,
        Vec3::new(0.0, top + 0.015, kz),
        Vec3::new(2.0 * e - 0.1, 0.03, 0.1),
        lin(0x24272c),
    );
    stripes(
        &mut m.body,
        Vec3::new(-e + 0.05, top + 0.0315, kz + 0.05),
        Vec3::X,
        Vec3::NEG_Z,
        2.0 * e - 0.1,
        0.1,
        Vec3::Y,
        lin(YELLOW),
    );
    for (sx, sz) in [(-1.0f32, 1.0f32), (1.0, 1.0)] {
        let c = Vec3::new(sx * (e - 0.1), top + 0.006, sz * (e - 0.1));
        blk(&mut m.metal, c, Vec3::new(0.12, 0.012, 0.12), lin(0x6c747e));
        for (bx, bz) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            blk(
                &mut m.metal,
                c + Vec3::new(bx * 0.04, 0.01, bz * 0.04),
                Vec3::splat(0.018),
                lin(IRON),
            );
        }
    }
    top
}

/// A spinning part with body, metal and glow geometry.
fn part(pivot: Vec3, anim: Anim) -> Part {
    Part::new(pivot, anim)
}

/// Fan rotor (blades + hub) lying flat, centred on the origin.
fn fan_rotor(p: &mut Part, r: f32, blades: u32, col: Rgba) {
    for k in 0..blades {
        let a = k as f32 / blades as f32 * TAU;
        let rot = Quat::from_rotation_y(a) * Quat::from_rotation_x(0.35);
        blk_rot(
            &mut p.metal,
            Quat::from_rotation_y(a) * Vec3::new(0.0, 0.0, r * 0.55),
            Vec3::new(r * 0.32, 0.012, r * 0.85),
            rot,
            col,
        );
    }
    p.body
        .cylinder(Vec3::new(0.0, -0.02, 0.0), r * 0.22, 0.05, 6, lin(DARK));
}

// ---------------------------------------------------------------------------------------
// Miner: drilling rig on outriggers. Auger in an open derrick, ore chute to a hopper,
// output chute at the front, power pack with radiator fan and exhaust at the back.

fn miner(m: &mut Model, s: f32, mk2: bool) {
    let accent = lin(if mk2 { BLUE } else { YELLOW });
    let accent_dk = scale(accent, 0.55);
    let iron = lin(IRON);
    let gun = lin(GUNMETAL);
    let top = foundation(m, s);

    // Chassis: dark frame, painted deck rim, tread-plate deck.
    let deck = 0.4;
    let (cx0, cx1, cz0, cz1) = (-0.66, 0.66, -0.55, 0.72);
    let cc = Vec3::new((cx0 + cx1) / 2.0, 0.0, (cz0 + cz1) / 2.0);
    let (cw, cd) = (cx1 - cx0, cz1 - cz0);
    m.body.cuboid(
        cc.with_y(top + 0.09),
        Vec3::new(cw - 0.1, 0.18, cd - 0.1),
        gun,
    );
    m.body
        .cuboid(cc.with_y(deck - 0.06), Vec3::new(cw, 0.12, cd), accent);
    m.body.cuboid(
        cc.with_y(deck + 0.005),
        Vec3::new(cw - 0.08, 0.02, cd - 0.08),
        lin(0x3d434b),
    );
    // Hazard bands on the deck rim (front face).
    stripes(
        &mut m.body,
        Vec3::new(cx0 + 0.04, deck - 0.11, cz0 - 0.002),
        Vec3::X,
        Vec3::Y,
        cw - 0.08,
        0.09,
        Vec3::NEG_Z,
        lin(DARK),
    );

    // Outriggers with hydraulic jacks and foot pads.
    for (sx, sz) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        let foot = Vec3::new(sx * 0.8, top, sz * 0.8);
        let root = Vec3::new(sx * 0.5, deck - 0.08, sz * 0.5 + 0.08);
        let tip = foot.with_y(deck - 0.02);
        bar(&mut m.body, root, tip, 0.11, accent);
        blk(
            &mut m.body,
            tip + Vec3::Y * 0.01,
            Vec3::new(0.15, 0.1, 0.15),
            accent_dk,
        );
        m.metal.cylinder(foot, 0.12, 0.035, 10, iron);
        rod(
            &mut m.metal,
            foot + Vec3::Y * 0.03,
            tip - Vec3::Y * 0.12,
            0.035,
            8,
            lin(0xc4ccd6),
        );
        m.body.cylinder(tip - Vec3::Y * 0.2, 0.055, 0.16, 8, gun);
        // Hydraulic hose from the jack back into the chassis.
        pipe(
            &mut m.rough,
            &[
                tip - Vec3::Y * 0.1 + Vec3::new(-sx * 0.05, 0.0, 0.0),
                root + Vec3::new(sx * 0.05, -0.08, 0.0),
            ],
            0.012,
            0.0,
            4,
            lin(RUBBER),
        );
    }

    // ---- Derrick over the auger.
    let t = Vec3::new(-0.24, deck, 0.06);
    let head_y = deck + 1.38;
    let (b0, b1) = (0.33, 0.2);
    let corners = |half: f32, y: f32| {
        [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
            .map(|(x, z)| Vec3::new(t.x + x * half, y, t.z + z * half))
    };
    let lo = corners(b0, deck);
    let hi = corners(b1, head_y);
    for k in 0..4 {
        bar(&mut m.body, lo[k], hi[k], 0.065, accent);
        blk(
            &mut m.metal,
            lo[k] + Vec3::Y * 0.02,
            Vec3::new(0.12, 0.04, 0.12),
            iron,
        );
    }
    let levels = [deck + 0.02, deck + 0.42, deck + 0.85, head_y - 0.05];
    for w in levels.windows(2) {
        let h0 = (w[0] - deck) / (head_y - deck);
        let h1 = (w[1] - deck) / (head_y - deck);
        let c0 = corners(b0 + (b1 - b0) * h0, w[0]);
        let c1 = corners(b0 + (b1 - b0) * h1, w[1]);
        for k in 0..4 {
            let j = (k + 1) % 4;
            bar(&mut m.metal, c1[k], c1[j], 0.032, gun);
            bar(&mut m.metal, c0[k], c1[j], 0.018, gun);
            bar(&mut m.metal, c0[j], c1[k], 0.018, gun);
        }
    }
    // Ladder up the back face.
    ladder(
        &mut m.metal,
        Vec3::new(t.x, deck, t.z + b0 + 0.05),
        Vec3::new(t.x, head_y - 0.05, t.z + b1 + 0.05),
        Vec3::X,
        iron,
    );
    // Bore collar on the deck.
    lathe(
        &mut m.metal,
        t,
        &[
            (0.27, 0.0),
            (0.27, 0.05),
            (0.22, 0.07),
            (0.21, 0.03),
            (0.0, 0.03),
        ],
        20,
        iron,
    );
    m.body
        .cylinder(t + Vec3::Y * 0.01, 0.2, 0.03, 12, lin(0x111317));
    // Drive head: gearbox, motors and crown with a sheave.
    let hc = Vec3::new(t.x, head_y + 0.13, t.z);
    m.body.cuboid(hc, Vec3::new(0.5, 0.26, 0.46), accent);
    blk(
        &mut m.body,
        hc + Vec3::new(0.0, 0.14, 0.0),
        Vec3::new(0.42, 0.04, 0.38),
        accent_dk,
    );
    for sx in [-1.0f32, 1.0] {
        let mc = hc + Vec3::new(sx * 0.25, -0.02, 0.0);
        cyl(&mut m.metal, mc, mc + Vec3::X * sx * 0.16, 0.1, 10, gun);
        for k in 0..4 {
            let x = sx * (0.29 + k as f32 * 0.025);
            blk(
                &mut m.metal,
                Vec3::new(t.x + x, mc.y, t.z),
                Vec3::new(0.01, 0.22, 0.22),
                iron,
            );
        }
    }
    grille(
        &mut m.body,
        hc + Vec3::new(0.0, 0.0, 0.232),
        Vec3::X,
        Vec3::Z,
        0.3,
        0.14,
        3,
        gun,
    );
    // Crown: A-frame mast with sheave wheel.
    let crown = hc + Vec3::Y * 0.16;
    for sz in [-1.0f32, 1.0] {
        bar(
            &mut m.metal,
            crown + Vec3::new(-0.15, 0.0, sz * 0.12),
            crown + Vec3::new(0.0, 0.32, sz * 0.05),
            0.03,
            iron,
        );
        bar(
            &mut m.metal,
            crown + Vec3::new(0.15, 0.0, sz * 0.12),
            crown + Vec3::new(0.0, 0.32, sz * 0.05),
            0.03,
            iron,
        );
    }
    let sheave = crown + Vec3::Y * 0.3;
    m.metal.cylinder_rot(
        sheave - Vec3::Z * 0.035,
        0.1,
        0.07,
        10,
        Quat::from_rotation_x(FRAC_PI_2),
        gun,
    );
    ring(
        &mut m.body,
        sheave,
        0.1,
        0.02,
        14,
        5,
        Quat::from_rotation_x(FRAC_PI_2),
        accent,
    );
    m.lamp = sheave + Vec3::new(0.0, 0.16, 0.0);
    // Hoist cable from the sheave down to the deck winch.
    let winch = Vec3::new(t.x + 0.42, deck + 0.1, t.z + 0.42);
    rod(
        &mut m.rough,
        sheave + Vec3::new(0.1, 0.0, 0.0),
        winch + Vec3::Y * 0.05,
        0.008,
        4,
        lin(RUBBER),
    );
    cyl(
        &mut m.metal,
        winch - Vec3::X * 0.1,
        winch + Vec3::X * 0.1,
        0.07,
        10,
        gun,
    );
    ring(
        &mut m.body,
        winch - Vec3::X * 0.1,
        0.075,
        0.015,
        10,
        4,
        Quat::from_rotation_z(FRAC_PI_2),
        accent,
    );
    ring(
        &mut m.body,
        winch + Vec3::X * 0.1,
        0.075,
        0.015,
        10,
        4,
        Quat::from_rotation_z(FRAC_PI_2),
        accent,
    );

    // ---- Auger (spins while working).
    let mut auger = part(t, Anim::Spin(if mk2 { 7.5 } else { 4.5 }));
    let len = head_y - deck;
    rod(
        &mut auger.metal,
        Vec3::Y * 0.02,
        Vec3::Y * (len - 0.02),
        0.055,
        8,
        lin(0xb8c0ca),
    );
    helix(
        &mut auger.metal,
        0.05,
        0.2,
        0.06,
        len - 0.32,
        3.6,
        0.012,
        lin(0xa9b2bd),
    );
    // Drive collar with bolts and a painted index mark (shows the rotation).
    let cy = len - 0.24;
    auger.body.cylinder(Vec3::Y * cy, 0.11, 0.2, 10, accent);
    blk(
        &mut auger.body,
        Vec3::new(0.0, cy + 0.1, 0.112),
        Vec3::new(0.05, 0.18, 0.012),
        lin(DARK),
    );
    bolt_ring(
        &mut auger.metal,
        Vec3::Y * (cy + 0.205),
        0.08,
        6,
        0.025,
        iron,
    );
    if mk2 {
        ring(
            &mut auger.glow,
            Vec3::Y * (cy - 0.02),
            0.12,
            0.012,
            16,
            4,
            Quat::IDENTITY,
            scale(lin(CYAN), 1.6),
        );
    }
    m.parts.push(auger);

    // ---- Ore chute from the derrick to the hopper.
    let hop = Vec3::new(0.42, 0.0, -0.32);
    let hop_top = deck + 0.72;
    let chute_a = Vec3::new(t.x + 0.18, deck + 1.0, t.z - 0.08);
    let chute_b = Vec3::new(hop.x - 0.12, hop_top + 0.06, hop.z + 0.04);
    trough(m, chute_a, chute_b, 0.16, 0.07, iron, lin(ORE));

    // ---- Hopper (square frustum) full of ore, on legs.
    let y0 = deck + 0.22;
    hopper(m, hop, y0, hop_top, 0.09, 0.27, accent, deck, lin(ORE), 3);
    // Spout and output chute to the front edge.
    blk(
        &mut m.metal,
        Vec3::new(hop.x, y0 - 0.05, hop.z),
        Vec3::new(0.16, 0.1, 0.16),
        gun,
    );
    trough(
        m,
        Vec3::new(hop.x, y0 - 0.08, hop.z - 0.05),
        Vec3::new(hop.x, top + 0.12, -0.92),
        0.2,
        0.07,
        iron,
        lin(ORE),
    );
    m.glow.cuboid(
        Vec3::new(hop.x, top + 0.13, -0.93),
        Vec3::new(0.2, 0.025, 0.02),
        scale(lin(GREEN), 1.4),
    );

    // ---- Power pack (back right) with radiator fan and exhaust.
    let pp = Vec3::new(0.4, deck, 0.4);
    let (pw, ph, pd) = (0.48, 0.44, 0.54);
    let pc = pp + Vec3::Y * (ph / 2.0);
    m.body.cuboid(pc, Vec3::new(pw, ph, pd), accent);
    blk(
        &mut m.body,
        pc + Vec3::Y * (ph / 2.0 + 0.02),
        Vec3::new(pw - 0.04, 0.04, pd - 0.04),
        gun,
    );
    grille(
        &mut m.body,
        pc + Vec3::new(pw / 2.0 + 0.001, 0.02, -0.08),
        Vec3::Z,
        Vec3::X,
        0.26,
        0.24,
        4,
        gun,
    );
    grille(
        &mut m.body,
        pc + Vec3::new(0.0, 0.02, pd / 2.0 + 0.001),
        Vec3::X,
        Vec3::Z,
        0.3,
        0.2,
        3,
        gun,
    );
    stripes(
        &mut m.body,
        pc + Vec3::new(-pw / 2.0, -ph / 2.0 + 0.02, pd / 2.0 + 0.003),
        Vec3::X,
        Vec3::Y,
        pw,
        0.06,
        Vec3::Z,
        lin(DARK),
    );
    // Radiator fan in a shroud on the roof.
    let fan_c = pc + Vec3::new(-0.02, ph / 2.0 + 0.05, -0.06);
    lathe(
        &mut m.metal,
        fan_c - Vec3::Y * 0.02,
        &[(0.19, 0.0), (0.19, 0.06), (0.16, 0.06), (0.16, 0.0)],
        18,
        gun,
    );
    m.body
        .cylinder(fan_c - Vec3::Y * 0.02, 0.16, 0.01, 12, lin(0x111317));
    let mut fan = part(
        fan_c + Vec3::Y * 0.015,
        Anim::Spin(if mk2 { 14.0 } else { 10.0 }),
    );
    fan_rotor(&mut fan, 0.15, 5, lin(0xa0a8b2));
    m.parts.push(fan);
    // Exhaust stack with a soot-black tip.
    let ex = pc + Vec3::new(0.15, ph / 2.0, 0.19);
    pipe(
        &mut m.metal,
        &[ex, ex + Vec3::Y * 0.4, ex + Vec3::new(0.0, 0.52, -0.08)],
        0.04,
        0.08,
        8,
        iron,
    );
    rod(
        &mut m.metal,
        ex + Vec3::new(0.0, 0.45, -0.07),
        ex + Vec3::new(0.0, 0.535, -0.17),
        0.045,
        8,
        lin(0x22252a),
    );
    m.metal.cylinder(ex + Vec3::Y * 0.08, 0.06, 0.18, 8, gun);

    // ---- Hydraulic tank (back left) on saddles.
    let tk = Vec3::new(-0.42, deck + 0.13, 0.52);
    cyl(
        &mut m.body,
        tk - Vec3::X * 0.18,
        tk + Vec3::X * 0.18,
        0.12,
        10,
        if mk2 { lin(PANEL) } else { lin(DARK) },
    );
    for dx in [-0.1f32, 0.1] {
        blk(
            &mut m.metal,
            tk + Vec3::new(dx, -0.08, 0.0),
            Vec3::new(0.04, 0.1, 0.22),
            gun,
        );
        ring(
            &mut m.metal,
            tk + Vec3::X * dx,
            0.123,
            0.01,
            12,
            3,
            Quat::from_rotation_z(FRAC_PI_2),
            iron,
        );
    }
    // Power and hydraulic lines clipped along the back-right derrick leg.
    let leg_lo = lo[2] + Vec3::new(0.05, 0.0, 0.05);
    let leg_hi = hi[2] + Vec3::new(0.05, 0.0, 0.05);
    pipe(
        &mut m.rough,
        &[
            pc + Vec3::new(-pw / 2.0, ph / 2.0 - 0.1, -0.1),
            leg_lo + Vec3::Y * 0.3,
            leg_hi - Vec3::Y * 0.02,
            hc + Vec3::new(0.2, -0.05, 0.2),
        ],
        0.02,
        0.06,
        5,
        lin(RUBBER),
    );
    pipe(
        &mut m.rough,
        &[
            tk + Vec3::new(0.18, 0.04, 0.03),
            leg_lo + Vec3::new(-0.04, 0.2, 0.0),
            leg_hi + Vec3::new(-0.04, -0.02, 0.0),
            hc + Vec3::new(0.12, -0.05, 0.22),
        ],
        0.015,
        0.06,
        5,
        lin(0x8a2f22),
    );
    for k in 1..4 {
        let p = leg_lo.lerp(leg_hi, k as f32 / 4.0);
        blk(
            &mut m.metal,
            p - Vec3::new(0.02, 0.0, 0.0),
            Vec3::new(0.09, 0.025, 0.05),
            iron,
        );
    }

    // ---- Control cabinet (front left) with a screen.
    let cab = Vec3::new(-0.5, deck, -0.42);
    m.body
        .cuboid(cab + Vec3::Y * 0.2, Vec3::new(0.24, 0.4, 0.18), lin(PANEL));
    blk(
        &mut m.body,
        cab + Vec3::Y * 0.415,
        Vec3::new(0.28, 0.03, 0.22),
        gun,
    );
    screen(
        &mut m.glow,
        cab + Vec3::new(0.0, 0.26, -0.091),
        Vec3::NEG_X,
        Vec3::NEG_Z,
        0.16,
        0.1,
        if mk2 { CYAN } else { GREEN },
        5,
    );
    screen(
        &mut m.glow,
        cab + Vec3::new(0.121, 0.26, 0.0),
        Vec3::Z,
        Vec3::X,
        0.12,
        0.1,
        if mk2 { CYAN } else { GREEN },
        9,
    );
    rod(
        &mut m.metal,
        cab + Vec3::new(-0.08, 0.43, 0.05),
        cab + Vec3::new(-0.08, 0.75, 0.05),
        0.008,
        4,
        iron,
    );
    m.glow.sphere(
        cab + Vec3::new(-0.08, 0.76, 0.05),
        0.016,
        Vec3::ONE,
        4,
        scale(lin(RED), 1.5),
    );

    // Deck railing along the back and right edges.
    railing(
        &mut m.metal,
        Vec3::new(cx0 + 0.04, deck, cz1 - 0.03),
        Vec3::new(-0.62, deck, cz1 - 0.03),
        0.18,
        accent,
    );
    railing(
        &mut m.metal,
        Vec3::new(cx1 - 0.03, deck, -0.5),
        Vec3::new(cx1 - 0.03, deck, 0.1),
        0.18,
        accent,
    );

    if mk2 {
        // Twin hydraulic rams beside the auger and a coolant loop.
        for sx in [-1.0f32, 1.0] {
            let base = Vec3::new(t.x + sx * 0.27, deck, t.z - 0.27);
            m.body.cylinder(base, 0.05, 0.6, 8, accent);
            rod(
                &mut m.metal,
                base + Vec3::Y * 0.6,
                Vec3::new(t.x + sx * 0.17, head_y, t.z - 0.17),
                0.025,
                8,
                lin(0xd0d8e2),
            );
        }
        for k in 0..3 {
            m.glow.cuboid(
                Vec3::new(
                    t.x + b0 * 0.62 + 0.01,
                    deck + 0.3 + k as f32 * 0.35,
                    t.z + 0.0,
                ),
                Vec3::new(0.012, 0.12, 0.05),
                scale(lin(CYAN), 1.4),
            );
        }
        pipe(
            &mut m.metal,
            &[
                tk + Vec3::new(-0.18, 0.0, 0.0),
                tk + Vec3::new(-0.24, 0.0, 0.0),
                Vec3::new(-0.66, deck + 0.13, t.z),
                Vec3::new(t.x - b0, deck + 0.13, t.z),
            ],
            0.025,
            0.08,
            6,
            lin(0xd0d8e2),
        );
        m.glow.cuboid(
            tk + Vec3::new(0.0, 0.0, -0.121),
            Vec3::new(0.18, 0.04, 0.006),
            scale(lin(CYAN), 1.2),
        );
    }
}

/// Square hopper (an inverted frustum) heaped with ore, on four legs down to `floor`.
#[allow(clippy::too_many_arguments)]
fn hopper(
    m: &mut Model,
    c: Vec3,
    y0: f32,
    y1: f32,
    hb: f32,
    ht: f32,
    paint: Rgba,
    floor: f32,
    ore: Rgba,
    seed: u64,
) {
    let hp = |hx: f32, y: f32, sx: f32, sz: f32| Vec3::new(c.x + sx * hx, y, c.z + sz * hx);
    let sides = [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
    for k in 0..4 {
        let (a, b) = (sides[k], sides[(k + 1) % 4]);
        let mid = Vec2::new(a.0 + b.0, a.1 + b.1).normalize();
        let n = Vec3::new(mid.x, (ht - hb) / (y1 - y0), mid.y).normalize();
        face(
            &mut m.body,
            [
                hp(hb, y0, a.0, a.1),
                hp(hb, y0, b.0, b.1),
                hp(ht, y1, b.0, b.1),
                hp(ht, y1, a.0, a.1),
            ],
            n,
            paint,
        );
        // Inner wall (seen over the rim), darker.
        face(
            &mut m.body,
            [
                hp(ht - 0.015, y1, a.0, a.1),
                hp(ht - 0.015, y1, b.0, b.1),
                hp(hb, y0 + 0.02, b.0, b.1),
                hp(hb, y0 + 0.02, a.0, a.1),
            ],
            -n,
            scale(paint, 0.45),
        );
        bar(
            &mut m.metal,
            hp(ht + 0.012, y1, a.0, a.1),
            hp(ht + 0.012, y1, b.0, b.1),
            0.035,
            lin(IRON),
        );
        bar(
            &mut m.metal,
            hp(ht - 0.02, y1 - 0.02, a.0, a.1),
            hp(ht - 0.02, floor, a.0, a.1),
            0.04,
            lin(GUNMETAL),
        );
        // Stiffener rib down the middle of each face.
        let m0 = (hp(hb, y0, a.0, a.1) + hp(hb, y0, b.0, b.1)) / 2.0;
        let m1 = (hp(ht, y1, a.0, a.1) + hp(ht, y1, b.0, b.1)) / 2.0;
        bar(
            &mut m.metal,
            m0 + n * 0.012,
            m1 + n * 0.012,
            0.025,
            scale(paint, 0.7),
        );
    }
    blk(
        &mut m.metal,
        Vec3::new(c.x, y0 - 0.04, c.z),
        Vec3::new(hb * 1.8, 0.08, hb * 1.8),
        lin(GUNMETAL),
    );
    let k = ht / 0.27;
    face(
        &mut m.rough,
        [
            hp(ht - 0.02, y1 - 0.05, -1.0, -1.0),
            hp(ht - 0.02, y1 - 0.05, 1.0, -1.0),
            hp(ht - 0.02, y1 - 0.05, 1.0, 1.0),
            hp(ht - 0.02, y1 - 0.05, -1.0, 1.0),
        ],
        Vec3::Y,
        scale(ore, 0.7),
    );
    for (i, (dx, dz, r)) in [
        (-0.1f32, -0.08f32, 0.09f32),
        (0.08, -0.1, 0.08),
        (0.06, 0.09, 0.1),
        (-0.09, 0.1, 0.075),
        (0.0, 0.0, 0.11),
    ]
    .into_iter()
    .enumerate()
    {
        m.rough.rock(
            Vec3::new(c.x + dx * k, y1 - 0.03, c.z + dz * k),
            r * k,
            Vec3::new(1.0, 0.6, 1.0),
            i as u64 + seed,
            mixc(ore, scale(ore, 1.5), i as f32 * 0.15),
        );
    }
}

/// Inclined open trough (U-section chute) from `a` to `b` with ore in it.
fn trough(m: &mut Model, a: Vec3, b: Vec3, w: f32, h: f32, col: Rgba, ore: Rgba) {
    let d = b - a;
    let len = d.length();
    let fwd = d / len;
    let side = Vec3::Y.cross(fwd).normalize_or(Vec3::X);
    let up = fwd.cross(side).normalize();
    let rot = Quat::from_mat3(&Mat3::from_cols(side, up, fwd));
    let c = (a + b) / 2.0;
    blk_rot(&mut m.metal, c, Vec3::new(w, 0.02, len), rot, col);
    for s in [-1.0f32, 1.0] {
        blk_rot(
            &mut m.metal,
            c + side * s * (w / 2.0) + up * (h / 2.0),
            Vec3::new(0.02, h, len),
            rot,
            scale(col, 0.85),
        );
    }
    blk_rot(
        &mut m.rough,
        c + up * 0.02,
        Vec3::new(w - 0.04, 0.02, len * 0.9),
        rot,
        scale(ore, 0.8),
    );
}

/// Helicoid flight (auger blade) around the Y axis from `y0` to `y1`.
fn helix(g: &mut Geo, r_in: f32, r_out: f32, y0: f32, y1: f32, turns: f32, thick: f32, col: Rgba) {
    let steps = (turns * 18.0) as u32;
    let pitch = (y1 - y0) / turns;
    let start = g.pos.len() as u32;
    // Per step: top-inner, top-outer, bottom-outer, bottom-inner (+ rim with radial normal).
    for k in 0..=steps {
        let f = k as f32 / steps as f32;
        let a = f * turns * TAU;
        let y = y0 + f * (y1 - y0);
        let d = Vec3::new(a.cos(), 0.0, a.sin());
        let tan = Vec3::new(-a.sin(), 0.0, a.cos());
        let slope = pitch / TAU;
        let n_out = (tan * r_out + Vec3::Y * slope).cross(d).normalize();
        let n_out = if n_out.y < 0.0 { -n_out } else { n_out };
        let n_in = (tan * r_in + Vec3::Y * slope).cross(d).normalize();
        let n_in = if n_in.y < 0.0 { -n_in } else { n_in };
        let ti = d * r_in + Vec3::Y * (y + thick / 2.0);
        let to = d * r_out + Vec3::Y * (y + thick / 2.0);
        let bo = d * r_out + Vec3::Y * (y - thick / 2.0);
        let bi = d * r_in + Vec3::Y * (y - thick / 2.0);
        g.vertex(ti, n_in, col);
        g.vertex(to, n_out, scale(col, 1.1));
        g.vertex(bo, -n_out, scale(col, 0.55));
        g.vertex(bi, -n_in, scale(col, 0.55));
        g.vertex(to, d, scale(col, 1.15));
        g.vertex(bo, d, scale(col, 0.8));
    }
    let w = 6;
    for k in 0..steps {
        let i = start + k * w;
        let j = i + w;
        tri_auto(g, i, i + 1, j + 1);
        tri_auto(g, i, j + 1, j);
        tri_auto(g, i + 3, i + 2, j + 2);
        tri_auto(g, i + 3, j + 2, j + 3);
        tri_auto(g, i + 4, i + 5, j + 5);
        tri_auto(g, i + 4, j + 5, j + 4);
    }
}

// ---------------------------------------------------------------------------------------
// Smelter: a compact blast furnace. Brick plinth, banded shaft with a ring of glowing
// tuyeres, tap hole pouring into a ladle at the front, hot ingots leaving on rollers,
// skip bridge from the ore hopper at the back, hot-blast stove and a glowing stack.

/// Radius of a lathe profile at height `y` (linear between points).
fn prof_r(prof: &[(f32, f32)], y: f32) -> f32 {
    for w in prof.windows(2) {
        let ((r0, y0), (r1, y1)) = (w[0], w[1]);
        if y >= y0 && y <= y1 && y1 > y0 {
            return r0 + (r1 - r0) * (y - y0) / (y1 - y0);
        }
    }
    prof.last().map_or(0.0, |p| p.0)
}

/// Running-bond brickwork on the rectangle `o + u*[0,len] + v*[0,hgt]` facing `n`.
#[allow(clippy::too_many_arguments)]
fn bricks(
    g: &mut Geo,
    o: Vec3,
    u: Vec3,
    v: Vec3,
    n: Vec3,
    len: f32,
    hgt: f32,
    seed: u64,
    col: Rgba,
) {
    let (bl, gap) = (0.15, 0.011);
    let rows = ((hgt / 0.066).round() as i32).max(1);
    let bh = hgt / rows as f32;
    for r in 0..rows {
        let mut x = if r % 2 == 0 { 0.0 } else { -bl * 0.5 };
        let mut k = 0;
        while x < len {
            let x0 = x.max(0.0) + gap / 2.0;
            let x1 = (x + bl).min(len) - gap / 2.0;
            if x1 > x0 + 0.01 {
                let y0 = r as f32 * bh + gap / 2.0;
                let y1 = (r + 1) as f32 * bh - gap / 2.0;
                let t = hash01(r as i64, k, seed);
                // Soot creeps up from below, a few bricks are burnt darker.
                let burn = if hash01(k, r as i64, seed ^ 0x55) > 0.85 {
                    0.6
                } else {
                    1.0
                };
                let c = scale(col, (0.78 + 0.4 * t) * burn);
                let off = n * 0.004;
                face(
                    g,
                    [
                        o + u * x0 + v * y0 + off,
                        o + u * x1 + v * y0 + off,
                        o + u * x1 + v * y1 + off,
                        o + u * x0 + v * y1 + off,
                    ],
                    n,
                    c,
                );
            }
            x += bl;
            k += 1;
        }
    }
}

fn smelter(m: &mut Model, s: f32, mk2: bool) {
    let top = foundation(m, s);
    let iron = lin(IRON);
    let gun = lin(GUNMETAL);
    let accent = lin(if mk2 { BLUE } else { YELLOW });
    let hot = if mk2 {
        scale(lin(0xffc870), 1.5)
    } else {
        scale(lin(0xff7a22), 1.4)
    };
    let molten = if mk2 {
        scale(lin(0xfff1c0), 2.0)
    } else {
        scale(lin(0xffa040), 1.8)
    };

    // ---- Plinth: refractory brick (Mk2: clad steel) with a steel cap.
    let pc = Vec3::new(-0.06, 0.0, 0.1);
    let (pw, pd, ph) = (1.42, 1.3, 0.34);
    let p_top = top + ph;
    let (x0, x1, z0, z1) = (
        pc.x - pw / 2.0,
        pc.x + pw / 2.0,
        pc.z - pd / 2.0,
        pc.z + pd / 2.0,
    );
    if mk2 {
        m.body.cuboid(
            pc.with_y(top + ph / 2.0),
            Vec3::new(pw, ph, pd),
            lin(0x4f5965),
        );
        for (o, u, n, len) in [
            (Vec3::new(x0, top, z0), Vec3::X, Vec3::NEG_Z, pw),
            (Vec3::new(x1, top, z0), Vec3::Z, Vec3::X, pd),
            (Vec3::new(x0, top, z1), Vec3::X, Vec3::Z, pw),
            (Vec3::new(x0, top, z0), Vec3::Z, Vec3::NEG_X, pd),
        ] {
            let ribs = (len / 0.2) as i32;
            for k in 1..ribs {
                let p = o + u * (len * k as f32 / ribs as f32) + n * 0.01;
                blk(
                    &mut m.metal,
                    p + Vec3::Y * (ph / 2.0),
                    Vec3::new(0.03, ph - 0.04, 0.03),
                    iron,
                );
            }
            face(
                &mut m.body,
                [
                    o + n * 0.003 + Vec3::Y * (ph - 0.1),
                    o + u * len + n * 0.003 + Vec3::Y * (ph - 0.1),
                    o + u * len + n * 0.003 + Vec3::Y * (ph - 0.05),
                    o + n * 0.003 + Vec3::Y * (ph - 0.05),
                ],
                n,
                accent,
            );
        }
    } else {
        let brick = lin(0x9a4a32);
        blk(
            &mut m.rough,
            pc.with_y(top + ph / 2.0),
            Vec3::new(pw, ph, pd),
            lin(0x6a5f58),
        );
        bricks(
            &mut m.rough,
            Vec3::new(x0, top, z0),
            Vec3::X,
            Vec3::Y,
            Vec3::NEG_Z,
            pw,
            ph,
            1,
            brick,
        );
        bricks(
            &mut m.rough,
            Vec3::new(x1, top, z0),
            Vec3::Z,
            Vec3::Y,
            Vec3::X,
            pd,
            ph,
            2,
            brick,
        );
        bricks(
            &mut m.rough,
            Vec3::new(x0, top, z1),
            Vec3::X,
            Vec3::Y,
            Vec3::Z,
            pw,
            ph,
            3,
            brick,
        );
        bricks(
            &mut m.rough,
            Vec3::new(x0, top, z0),
            Vec3::Z,
            Vec3::Y,
            Vec3::NEG_X,
            pd,
            ph,
            4,
            brick,
        );
    }
    m.metal.cuboid(
        pc.with_y(p_top + 0.02),
        Vec3::new(pw + 0.04, 0.04, pd + 0.04),
        gun,
    );
    // Glowing vents in the plinth's right side.
    for k in 0..3 {
        let c = Vec3::new(x1 + 0.002, top + 0.17, z0 + 0.35 + k as f32 * 0.3);
        face(
            &mut m.glow,
            [
                c + Vec3::new(0.0, -0.06, -0.1),
                c + Vec3::new(0.0, -0.06, 0.1),
                c + Vec3::new(0.0, 0.06, 0.1),
                c + Vec3::new(0.0, 0.06, -0.1),
            ],
            Vec3::X,
            hot,
        );
        for j in 0..3 {
            blk_rot(
                &mut m.metal,
                c + Vec3::new(0.015, -0.04 + j as f32 * 0.04, 0.0),
                Vec3::new(0.02, 0.022, 0.22),
                Quat::from_rotation_z(0.6),
                gun,
            );
        }
        blk(
            &mut m.metal,
            c + Vec3::new(0.006, 0.0, 0.0),
            Vec3::new(0.012, 0.15, 0.24),
            scale(gun, 0.7),
        );
    }

    // ---- Furnace shaft.
    let f = Vec3::new(-0.1, p_top + 0.04, 0.12);
    let prof = [
        (0.6, 0.0),
        (0.6, 0.07),
        (0.56, 0.09),
        (0.58, 0.38),
        (0.45, 0.98),
        (0.45, 1.06),
        (0.31, 1.15),
        (0.2, 1.19),
    ];
    let (shell, heat) = if mk2 {
        (lin(PANEL), lin(0x9aa3ad))
    } else {
        (lin(0x4a4542), lin(0x7c4630))
    };
    lathe_e(&mut m.body, f, &prof, 26, |i| {
        mixc(heat, shell, (i as f32 / 4.0).min(1.0))
    });
    // Banding (Mk2: blue).
    for (y, h) in [
        (0.0, 0.07),
        (0.2, 0.04),
        (0.6, 0.04),
        (0.86, 0.04),
        (1.0, 0.06),
    ] {
        let r = prof_r(&prof, y + h / 2.0);
        if mk2 {
            band(&mut m.body, f + Vec3::Y * y, r, h, 0.018, 26, accent);
        } else {
            band(&mut m.metal, f + Vec3::Y * y, r, h, 0.015, 26, gun);
        }
    }
    if mk2 {
        // Induction coils glowing through slots in the cladding.
        for y in [0.3f32, 0.45] {
            let r = prof_r(&prof, y);
            band(
                &mut m.glow,
                f + Vec3::Y * y,
                r,
                0.03,
                0.006,
                26,
                scale(lin(0xffb84a), 1.6),
            );
        }
    }

    // Bustle pipe with downcomers feeding glowing tuyeres.
    let bust_y = 0.52;
    let bust_r = prof_r(&prof, bust_y) + 0.1;
    ring(
        &mut m.metal,
        f + Vec3::Y * bust_y,
        bust_r,
        0.045,
        24,
        6,
        Quat::IDENTITY,
        gun,
    );
    for k in 0..8 {
        let a = (k as f32 + 0.5) / 8.0 * TAU;
        let d = Vec3::new(a.cos(), 0.0, a.sin());
        let tuy = f + Vec3::Y * 0.2 + d * (prof_r(&prof, 0.2) + 0.005);
        rod(
            &mut m.metal,
            f + Vec3::Y * bust_y + d * bust_r,
            tuy + d * 0.07 + Vec3::Y * 0.02,
            0.02,
            6,
            iron,
        );
        rod(&mut m.metal, tuy + d * 0.08, tuy - d * 0.02, 0.028, 6, gun);
        blk_rot(
            &mut m.glow,
            tuy + d * 0.085,
            Vec3::new(0.035, 0.035, 0.01),
            Quat::from_rotation_y(-a + FRAC_PI_2),
            molten,
        );
    }

    // ---- Tap hole and runner into the ladle at the front.
    let tap = f + Vec3::new(0.0, 0.14, -prof_r(&prof, 0.14));
    blk(
        &mut m.metal,
        tap + Vec3::new(0.0, 0.0, -0.015),
        Vec3::new(0.22, 0.18, 0.05),
        gun,
    );
    face(
        &mut m.glow,
        [
            tap + Vec3::new(-0.06, -0.07, -0.042),
            tap + Vec3::new(0.06, -0.07, -0.042),
            tap + Vec3::new(0.06, 0.05, -0.042),
            tap + Vec3::new(-0.06, 0.05, -0.042),
        ],
        Vec3::NEG_Z,
        molten,
    );
    let lad = Vec3::new(tap.x, top, -0.64);
    let run_a = tap + Vec3::new(0.0, -0.07, -0.04);
    let run_b = Vec3::new(lad.x, top + 0.36, lad.z + 0.08);
    trough(m, run_a, run_b, 0.12, 0.05, gun, gun);
    let dir = (run_b - run_a).normalize();
    let side = Vec3::Y.cross(dir).normalize();
    let up = dir.cross(side);
    blk_rot(
        &mut m.glow,
        (run_a + run_b) / 2.0 + up * 0.022,
        Vec3::new(0.07, 0.01, run_a.distance(run_b) * 0.95),
        Quat::from_mat3(&Mat3::from_cols(side, up, dir)),
        molten,
    );
    // Falling stream.
    blk(
        &mut m.glow,
        run_b + Vec3::new(0.0, -0.04, -0.035),
        Vec3::new(0.035, 0.09, 0.03),
        molten,
    );
    // Ladle with trunnions and a molten surface.
    lathe(
        &mut m.metal,
        lad,
        &[
            (0.0, 0.0),
            (0.12, 0.0),
            (0.16, 0.05),
            (0.175, 0.28),
            (0.195, 0.29),
            (0.195, 0.32),
            (0.165, 0.32),
            (0.15, 0.28),
        ],
        18,
        scale(gun, 0.9),
    );
    m.glow
        .cylinder(lad + Vec3::Y * 0.275, 0.152, 0.01, 10, molten);
    for sx in [-1.0f32, 1.0] {
        cyl(
            &mut m.metal,
            lad + Vec3::new(sx * 0.17, 0.2, 0.0),
            lad + Vec3::new(sx * 0.23, 0.2, 0.0),
            0.04,
            8,
            iron,
        );
    }
    band(
        &mut m.body,
        lad + Vec3::Y * 0.12,
        0.172,
        0.05,
        0.01,
        18,
        accent,
    );

    // ---- Hot ingots leaving on a roller conveyor (front right).
    let cx = 0.45;
    let (cz0, cz1) = (z0 - 0.02, -0.93);
    let cy = top + 0.2;
    let len = cz0 - cz1;
    let ccz = (cz0 + cz1) / 2.0;
    for sx in [-1.0f32, 1.0] {
        blk(
            &mut m.metal,
            Vec3::new(cx + sx * 0.13, cy, ccz),
            Vec3::new(0.03, 0.06, len),
            gun,
        );
        for z in [cz0 - 0.06, cz1 + 0.06] {
            blk(
                &mut m.metal,
                Vec3::new(cx + sx * 0.13, (top + cy) / 2.0, z),
                Vec3::new(0.03, cy - top, 0.03),
                gun,
            );
        }
    }
    let rollers = 6;
    for k in 0..rollers {
        let z = cz1 + 0.04 + (len - 0.08) * k as f32 / (rollers - 1) as f32;
        rod(
            &mut m.metal,
            Vec3::new(cx - 0.12, cy + 0.01, z),
            Vec3::new(cx + 0.12, cy + 0.01, z),
            0.022,
            6,
            iron,
        );
    }
    for (k, z) in [cz1 + 0.07, cz1 + 0.22].into_iter().enumerate() {
        let c = Vec3::new(cx, cy + 0.06, z);
        let glow_k = 0.55 + 0.3 * k as f32;
        m.glow
            .cuboid(c, Vec3::new(0.15, 0.05, 0.08), scale(hot, glow_k));
    }
    // Ingot exit in the plinth front, glowing.
    face(
        &mut m.glow,
        [
            Vec3::new(cx - 0.11, top + 0.18, z0 - 0.003),
            Vec3::new(cx + 0.11, top + 0.18, z0 - 0.003),
            Vec3::new(cx + 0.11, top + 0.3, z0 - 0.003),
            Vec3::new(cx - 0.11, top + 0.3, z0 - 0.003),
        ],
        Vec3::NEG_Z,
        hot,
    );
    blk(
        &mut m.metal,
        Vec3::new(cx, top + 0.32, z0 - 0.02),
        Vec3::new(0.28, 0.04, 0.05),
        gun,
    );
    stripes(
        &mut m.body,
        Vec3::new(cx - 0.14, top + 0.3, z0 - 0.046),
        Vec3::X,
        Vec3::Y,
        0.28,
        0.04,
        Vec3::NEG_Z,
        accent,
    );

    // ---- Stack on the furnace top with a glowing rim.
    let st = f + Vec3::Y * 1.17;
    let stack = [
        (0.2, 0.0),
        (0.13, 0.07),
        (0.13, 0.62),
        (0.12, 0.8),
        (0.15, 0.81),
        (0.15, 0.86),
        (0.11, 0.86),
        (0.1, 0.78),
    ];
    let soot = lin(0x1f1d1c);
    lathe_e(&mut m.body, st, &stack, 18, |i| {
        if i >= 2 {
            mixc(shell, soot, 0.6 + 0.2 * (i as f32 - 2.0).min(2.0))
        } else {
            shell
        }
    });
    band(&mut m.metal, st + Vec3::Y * 0.3, 0.13, 0.04, 0.012, 18, gun);
    m.glow.cylinder(st + Vec3::Y * 0.79, 0.1, 0.01, 10, hot);
    ring(
        &mut m.glow,
        st + Vec3::Y * 0.865,
        0.13,
        0.012,
        18,
        4,
        Quat::IDENTITY,
        scale(hot, 0.8),
    );
    if mk2 {
        // Second, slimmer stack.
        let st2 = f + Vec3::new(0.22, 1.03, -0.12);
        lathe_e(
            &mut m.body,
            st2,
            &[
                (0.09, 0.0),
                (0.075, 0.05),
                (0.075, 0.6),
                (0.09, 0.61),
                (0.09, 0.65),
                (0.06, 0.65),
                (0.055, 0.6),
            ],
            14,
            |i| {
                if i >= 2 {
                    mixc(shell, soot, 0.7)
                } else {
                    shell
                }
            },
        );
        m.glow.cylinder(st2 + Vec3::Y * 0.6, 0.055, 0.01, 8, hot);
        band(
            &mut m.body,
            st2 + Vec3::Y * 0.25,
            0.075,
            0.04,
            0.012,
            14,
            accent,
        );
    }

    // ---- Hot-blast stove (back left) feeding the bustle pipe.
    let hs = Vec3::new(-0.76, top, 0.76);
    lathe_e(
        &mut m.body,
        hs,
        &[
            (0.0, 0.0),
            (0.17, 0.0),
            (0.17, 0.05),
            (0.155, 0.07),
            (0.155, 1.28),
            (0.12, 1.4),
            (0.06, 1.45),
            (0.0, 1.46),
        ],
        16,
        |i| if i < 2 { gun } else { shell },
    );
    for y in [0.35f32, 0.75, 1.1] {
        band(&mut m.metal, hs + Vec3::Y * y, 0.155, 0.035, 0.012, 16, gun);
    }
    if mk2 {
        band(
            &mut m.body,
            hs + Vec3::Y * 0.55,
            0.155,
            0.08,
            0.012,
            16,
            accent,
        );
    }
    ladder(
        &mut m.metal,
        hs + Vec3::new(0.0, 0.0, 0.18),
        hs + Vec3::new(0.0, 1.25, 0.18),
        Vec3::X,
        iron,
    );
    // Hot-blast main to the bustle pipe and the gas line from the furnace top.
    let bust_pt = f + Vec3::Y * bust_y + Vec3::new(-1.0, 0.0, 1.0).normalize() * bust_r;
    pipe(
        &mut m.metal,
        &[
            hs + Vec3::new(0.11, 0.62, -0.11),
            bust_pt + Vec3::new(-0.04, 0.0, 0.04),
        ],
        0.045,
        0.0,
        8,
        gun,
    );
    pipe(
        &mut m.metal,
        &[
            f + Vec3::new(-0.18, 1.1, 0.18),
            f + Vec3::new(-0.24, 1.38, 0.24),
            hs + Vec3::new(0.0, 1.52, 0.0),
            hs + Vec3::Y * 1.4,
        ],
        0.05,
        0.14,
        8,
        scale(shell, 0.8),
    );

    // ---- Ore hopper (back right) and skip bridge up to the furnace throat.
    let hop = Vec3::new(0.6, 0.0, 0.63);
    let hop_top = top + 0.75;
    hopper(
        m,
        hop,
        top + 0.42,
        hop_top,
        0.08,
        0.25,
        accent,
        top,
        lin(ORE),
        7,
    );
    let sa = Vec3::new(hop.x - 0.06, hop_top + 0.02, hop.z - 0.06);
    let sb = f + Vec3::new(0.12, 1.2, 0.08);
    skip_bridge(m, sa, sb, 0.2, accent);
    // Trestle under the bridge.
    let mid = sa.lerp(sb, 0.35);
    for sx in [-1.0f32, 1.0] {
        let side = Vec3::Y.cross(sb - sa).normalize() * sx * 0.1;
        bar(
            &mut m.metal,
            mid + side,
            Vec3::new(mid.x, p_top, mid.z) + side,
            0.035,
            gun,
        );
    }

    // ---- Control desk with a temperature display (front left) carries the lamp.
    let cd = Vec3::new(-0.68, top, -0.66);
    m.body
        .cuboid(cd + Vec3::Y * 0.22, Vec3::new(0.3, 0.44, 0.24), lin(PANEL));
    blk(
        &mut m.body,
        cd + Vec3::Y * 0.455,
        Vec3::new(0.34, 0.03, 0.28),
        gun,
    );
    screen(
        &mut m.glow,
        cd + Vec3::new(0.0, 0.3, -0.121),
        Vec3::NEG_X,
        Vec3::NEG_Z,
        0.2,
        0.12,
        if mk2 { CYAN } else { ORANGE },
        4,
    );
    screen(
        &mut m.glow,
        cd + Vec3::new(0.151, 0.3, 0.0),
        Vec3::Z,
        Vec3::X,
        0.16,
        0.12,
        if mk2 { CYAN } else { ORANGE },
        8,
    );
    grille(
        &mut m.body,
        cd + Vec3::new(0.0, 0.12, 0.121),
        Vec3::X,
        Vec3::Z,
        0.2,
        0.1,
        3,
        gun,
    );
    m.lamp = cd + Vec3::new(0.0, 0.53, 0.0);
    // Cable from the desk to the furnace.
    pipe(
        &mut m.rough,
        &[
            cd + Vec3::new(0.1, 0.44, 0.08),
            cd + Vec3::new(0.2, 0.5, 0.2),
            f + Vec3::new(-0.35, 0.3, -0.35),
        ],
        0.018,
        0.08,
        5,
        lin(RUBBER),
    );
    if mk2 {
        // Transformer feeding the induction coils.
        let tr = Vec3::new(0.66, top, -0.05);
        m.body
            .cuboid(tr + Vec3::Y * 0.2, Vec3::new(0.2, 0.4, 0.3), lin(0x4f5965));
        for k in 0..4 {
            blk(
                &mut m.metal,
                tr + Vec3::new(0.11, 0.2, -0.11 + k as f32 * 0.075),
                Vec3::new(0.03, 0.34, 0.02),
                iron,
            );
        }
        for k in 0..2 {
            m.glow.cuboid(
                tr + Vec3::new(-0.05 + k as f32 * 0.1, 0.42, 0.0),
                Vec3::new(0.03, 0.04, 0.2),
                scale(lin(CYAN), 1.3),
            );
        }
    }
}

/// Inclined skip bridge: two side girders with a belt between them.
fn skip_bridge(m: &mut Model, a: Vec3, b: Vec3, w: f32, paint: Rgba) {
    let d = b - a;
    let len = d.length();
    let fwd = d / len;
    let side = Vec3::Y.cross(fwd).normalize_or(Vec3::X);
    let up = fwd.cross(side).normalize();
    let rot = Quat::from_mat3(&Mat3::from_cols(side, up, fwd));
    let c = (a + b) / 2.0;
    for s in [-1.0f32, 1.0] {
        blk_rot(
            &mut m.body,
            c + side * s * (w / 2.0),
            Vec3::new(0.035, 0.09, len),
            rot,
            paint,
        );
        // Lattice web under the girder.
        let n = (len / 0.16) as i32;
        for k in 0..n {
            let p0 = a + fwd * (len * k as f32 / n as f32) + side * s * (w / 2.0) - up * 0.04;
            let p1 =
                a + fwd * (len * (k as f32 + 0.5) / n as f32) + side * s * (w / 2.0) - up * 0.13;
            let p2 =
                a + fwd * (len * (k as f32 + 1.0) / n as f32) + side * s * (w / 2.0) - up * 0.04;
            bar(&mut m.metal, p0, p1, 0.015, lin(GUNMETAL));
            bar(&mut m.metal, p1, p2, 0.015, lin(GUNMETAL));
        }
        blk_rot(
            &mut m.metal,
            c + side * s * (w / 2.0) - up * 0.13,
            Vec3::new(0.02, 0.02, len * 0.95),
            rot,
            lin(GUNMETAL),
        );
    }
    blk_rot(
        &mut m.rough,
        c + up * 0.01,
        Vec3::new(w - 0.04, 0.02, len),
        rot,
        lin(RUBBER),
    );
    // Ore lumps riding up.
    for k in 0..4 {
        let p = a + fwd * (len * (k as f32 + 0.5) / 4.0) + up * 0.045;
        m.rough
            .rock(p, 0.035, Vec3::new(1.0, 0.7, 1.0), k + 21, lin(ORE));
    }
    // Head pulley housing.
    blk_rot(
        &mut m.body,
        b + up * 0.03,
        Vec3::new(w + 0.06, 0.12, 0.14),
        rot,
        scale(paint, 0.8),
    );
}

// ---------------------------------------------------------------------------------------
// Previous models (being replaced).

fn old_assembler(m: &mut Model, s: f32, mk2: bool) {
    let (wall, roof) = if !mk2 {
        (0x5d6b7c, YELLOW)
    } else {
        (0x3a4a63, 0x4a90e0)
    };
    pad(m, s);
    m.body.cuboid(
        Vec3::new(0.0, 0.6, 0.0),
        Vec3::new(2.5, 1.1, 2.5),
        lin(wall),
    );
    m.body.cuboid(
        Vec3::new(0.0, 1.22, 0.0),
        Vec3::new(2.2, 0.15, 2.2),
        lin(roof),
    );
    windows(m, 0.85, 1.26, -1.0, 1.0, 5, CYAN);
    let mut fan = Geo::fine();
    fan.cuboid(
        Vec3::new(0.0, 0.07, 0.0),
        Vec3::new(0.75, 0.04, 0.12),
        lin(PANEL),
    );
    m.parts.push(spinner(
        fan,
        Geo::fine(),
        Vec3::new(0.55, 1.33, -0.45),
        Anim::Spin(6.0),
    ));
    m.lamp = Vec3::new(1.0, 1.45, 1.0);
}

fn old_refinery(m: &mut Model, s: f32) {
    pad(m, s);
    for (x, z, r, h, c) in [
        (-0.65, -0.6, 0.5, 1.6, WHITE),
        (0.6, -0.55, 0.42, 1.2, PANEL),
    ] {
        m.body.cylinder(Vec3::new(x, 0.35, z), r, h, 14, lin(c));
    }
    m.body
        .cylinder(Vec3::new(0.75, 0.35, 0.75), 0.08, 2.0, 8, lin(STEEL));
    m.glow.sphere(
        Vec3::new(0.75, 2.42, 0.75),
        0.12,
        Vec3::new(1.0, 1.6, 1.0),
        8,
        scale(lin(ORANGE), 2.5),
    );
    m.lamp = Vec3::new(1.1, 0.7, -1.1);
    let _ = (PI, FRAC_PI_4, PURPLE);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tris(m: &Model) -> usize {
        let mut n = 0;
        for g in [&m.body, &m.metal, &m.rough, &m.glass, &m.glow] {
            n += g.idx.len() / 3;
        }
        for p in &m.parts {
            n += (p.body.idx.len() + p.metal.idx.len() + p.glow.idx.len()) / 3;
        }
        n
    }

    #[test]
    fn production_triangle_budgets() {
        use BuildingKind as K;
        for k in [
            K::Miner,
            K::MinerMk2,
            K::Smelter,
            K::SmelterMk2,
            K::Assembler,
            K::AssemblerMk2,
            K::Refinery,
        ] {
            let m = super::super::building(k);
            let size = k.spec().size.0;
            let n = tris(&m);
            let budget = if size <= 2 { 15_000 } else { 30_000 };
            println!("{k:?}: {n} triangles (budget {budget})");
            assert!(n <= budget, "{k:?} uses {n} triangles");
        }
    }
}
