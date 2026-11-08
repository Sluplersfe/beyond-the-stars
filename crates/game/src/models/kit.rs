//! Shared palette and helpers for the building models.
#![allow(dead_code)]

use bevy::prelude::*;
use std::f32::consts::TAU;

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

// ---------------------------------------------------------------------------------------
// Shared geometry helpers (same as the ones the production/logistics models use).

/// Concrete foundation slab of half-size `h` with a hazard-striped front kerb and steel
/// edge angles.
pub fn foundation(m: &mut Model, h: f32) {
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
        lin(0x22262c),
    );
    // Steel edge angle on the kerb.
    for sz in [-1.0f32, 1.0] {
        m.metal.cuboid(
            Vec3::new(0.0, 0.07, sz * (h - 0.006)),
            Vec3::new(h * 2.0, 0.025, 0.014),
            lin(0x626b77),
        );
    }
    for sx in [-1.0f32, 1.0] {
        m.metal.cuboid(
            Vec3::new(sx * (h - 0.006), 0.07, 0.0),
            Vec3::new(0.014, 0.025, h * 2.0),
            lin(0x626b77),
        );
    }
}


pub type Vtx = (Vec3, Vec3, Rgba);

/// A planar quad with one normal; winding is chosen to face along `n`.
pub fn flat(g: &mut Geo, p: [Vec3; 4], n: Vec3, c: Rgba) {
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
pub fn grid(g: &mut Geo, rows: &[Vec<Vtx>], skip: &[bool]) {
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
pub fn profile_normals(pts: &[(f32, f32)]) -> (Vec<Vec2>, Vec<bool>) {
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
pub fn lathe(g: &mut Geo, c: Vec3, prof: &[(f32, f32, Rgba)], segs: u32) {
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
pub fn extrude_z(g: &mut Geo, prof: &[(f32, f32, Rgba)], z0: f32, z1: f32, caps: bool) {
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
pub fn basis(fwd: Vec3, up: Vec3) -> Quat {
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
pub fn beam(g: &mut Geo, a: Vec3, b: Vec3, w: f32, h: f32, col: Rgba) {
    let d = b - a;
    g.cuboid_rot(
        (a + b) / 2.0,
        Vec3::new(w, h, d.length()),
        basis(d, Vec3::Y),
        col,
    );
}

/// Cylinder from `a` to `b`.
pub fn rod(g: &mut Geo, a: Vec3, b: Vec3, r: f32, segs: u32, col: Rgba) {
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
pub fn bolt(g: &mut Geo, p: Vec3, n: Vec3, r: f32, col: Rgba) {
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
pub fn tube(g: &mut Geo, pts: &[Vec3], r: f32, segs: u32, col: Rgba) {
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
pub fn sag(a: Vec3, b: Vec3, depth: f32, n: usize) -> Vec<Vec3> {
    (0..=n)
        .map(|k| {
            let t = k as f32 / n as f32;
            a.lerp(b, t) - Vec3::Y * depth * 4.0 * t * (1.0 - t)
        })
        .collect()
}

/// Diagonal hazard stripes on the band `o + u*s + v*t` (s in 0..len, t in 0..wid),
/// facing `n`.
pub fn hazard(
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
pub fn arrow(g: &mut Geo, c: Vec3, fwd: Vec3, size: f32, col: Rgba) {
    arrow_on(g, c, fwd, Vec3::Y, size, col);
}

/// Chevron arrow on a surface with normal `n`, pointing along `fwd` (projected onto the
/// surface).
pub fn arrow_on(g: &mut Geo, c: Vec3, fwd: Vec3, n: Vec3, size: f32, col: Rgba) {
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
