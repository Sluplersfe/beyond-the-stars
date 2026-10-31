//! Living things and scenery: trees, hostile nests, creatures.
//!
//! Everything here is organic, so instead of the bevelled boxes the buildings use, the
//! models are made from a few private helpers: noisy icosphere blobs (foliage clumps,
//! flesh), lathes (trunks, bodies), swept tubes (branches, roots, legs, veins, horns) and
//! faceted prisms (crystals). Colours are baked per vertex with a top-lit gradient and
//! cavity darkening so the shapes read without relying on the lighting alone.

use bevy::prelude::*;
use std::f32::consts::{PI, TAU};

use crate::mesh_util::*;

// ---------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------

/// Cheap 3D value noise in [0, 1) built from three 2D slices.
fn noise3(p: Vec3, seed: u64) -> f32 {
    (noise(p.x, p.y, seed)
        + noise(p.y + 31.7, p.z, seed ^ 0x33)
        + noise(p.z + 17.3, p.x, seed ^ 0x77))
        / 3.0
}

fn rnd(k: u32, seed: u64) -> f32 {
    hash01(k as i64, 0x5eed, seed)
}

/// Triangle from existing vertices, wound to face along their normals.
fn tri_n(g: &mut Geo, a: u32, b: u32, c: u32) {
    let p = |i: u32| Vec3::from(g.pos[i as usize]);
    let n = |i: u32| Vec3::from(g.nrm[i as usize]);
    let geo = (p(b) - p(a)).cross(p(c) - p(a));
    if geo.dot(n(a) + n(b) + n(c)) < 0.0 {
        g.idx.extend_from_slice(&[a, c, b]);
    } else {
        g.idx.extend_from_slice(&[a, b, c]);
    }
}

/// Flat-shaded triangle facing away from `inside`.
fn facet(g: &mut Geo, a: Vec3, b: Vec3, c: Vec3, inside: Vec3, col: Rgba) {
    let n = (b - a).cross(c - a);
    if n.dot((a + b + c) / 3.0 - inside) < 0.0 {
        g.tri(a, c, b, col);
    } else {
        g.tri(a, b, c, col);
    }
}

/// Recompute smooth normals for the vertices from `start` on (indices from `istart`).
fn smooth_normals(g: &mut Geo, start: usize, istart: usize) {
    let mut acc = vec![Vec3::ZERO; g.pos.len() - start];
    for t in g.idx[istart..].chunks(3) {
        let (a, b, c) = (t[0] as usize, t[1] as usize, t[2] as usize);
        let (pa, pb, pc) = (
            Vec3::from(g.pos[a]),
            Vec3::from(g.pos[b]),
            Vec3::from(g.pos[c]),
        );
        let n = (pb - pa).cross(pc - pa);
        for i in [a, b, c] {
            acc[i - start] += n;
        }
    }
    for (k, n) in acc.into_iter().enumerate() {
        let old = Vec3::from(g.nrm[start + k]);
        g.nrm[start + k] = n.normalize_or(old).into();
    }
}

/// Unit icosphere: 12 vertices / 20 faces, each subdivision quadruples the faces.
fn icosphere(sub: u32) -> (Vec<Vec3>, Vec<[u32; 3]>) {
    let t = (1.0 + 5.0f32.sqrt()) / 2.0;
    let mut v: Vec<Vec3> = [
        (-1.0, t, 0.0),
        (1.0, t, 0.0),
        (-1.0, -t, 0.0),
        (1.0, -t, 0.0),
        (0.0, -1.0, t),
        (0.0, 1.0, t),
        (0.0, -1.0, -t),
        (0.0, 1.0, -t),
        (t, 0.0, -1.0),
        (t, 0.0, 1.0),
        (-t, 0.0, -1.0),
        (-t, 0.0, 1.0),
    ]
    .iter()
    .map(|&(x, y, z)| Vec3::new(x, y, z).normalize())
    .collect();
    let mut f: Vec<[u32; 3]> = vec![
        [0, 11, 5],
        [0, 5, 1],
        [0, 1, 7],
        [0, 7, 10],
        [0, 10, 11],
        [1, 5, 9],
        [5, 11, 4],
        [11, 10, 2],
        [10, 7, 6],
        [7, 1, 8],
        [3, 9, 4],
        [3, 4, 2],
        [3, 2, 6],
        [3, 6, 8],
        [3, 8, 9],
        [4, 9, 5],
        [2, 4, 11],
        [6, 2, 10],
        [8, 6, 7],
        [9, 8, 1],
    ];
    for _ in 0..sub {
        let mut cache = std::collections::HashMap::new();
        let mut mid = |a: u32, b: u32, v: &mut Vec<Vec3>| -> u32 {
            let key = (a.min(b), a.max(b));
            *cache.entry(key).or_insert_with(|| {
                v.push(((v[a as usize] + v[b as usize]) / 2.0).normalize());
                (v.len() - 1) as u32
            })
        };
        let mut nf = Vec::with_capacity(f.len() * 4);
        for [a, b, c] in f {
            let ab = mid(a, b, &mut v);
            let bc = mid(b, c, &mut v);
            let ca = mid(c, a, &mut v);
            nf.extend_from_slice(&[[a, ab, ca], [b, bc, ab], [c, ca, bc], [ab, bc, ca]]);
        }
        f = nf;
    }
    (v, f)
}

/// Lumpy smooth blob (foliage clump, flesh). `bump` is the noise displacement amount;
/// `col` receives the unit direction and the local noise value.
#[allow(clippy::too_many_arguments)]
fn blob(
    g: &mut Geo,
    center: Vec3,
    radius: f32,
    stretch: Vec3,
    sub: u32,
    bump: f32,
    seed: u64,
    col: impl Fn(Vec3, f32) -> Rgba,
) {
    let (v, f) = icosphere(sub);
    let start = g.pos.len();
    let istart = g.idx.len();
    let off = Vec3::new(rnd(1, seed), rnd(2, seed), rnd(3, seed)) * 50.0;
    for d in &v {
        let n = noise3(*d * 2.3 + off, seed) * 0.65 + noise3(*d * 5.1 + off, seed ^ 9) * 0.35;
        let p = center + *d * stretch * radius * (1.0 + bump * (n - 0.5) * 2.0);
        g.vertex(p, (*d / stretch).normalize(), col(*d, n));
    }
    for [a, b, c] in f {
        g.idx
            .extend_from_slice(&[start as u32 + a, start as u32 + c, start as u32 + b]);
    }
    // Icosphere faces above are wound inward-out; fix by checking one.
    let i0 = istart;
    let (a, b, c) = (g.idx[i0], g.idx[i0 + 1], g.idx[i0 + 2]);
    let pa = Vec3::from(g.pos[a as usize]);
    let geo = (Vec3::from(g.pos[b as usize]) - pa).cross(Vec3::from(g.pos[c as usize]) - pa);
    if geo.dot(pa - center) < 0.0 {
        for t in g.idx[istart..].chunks_mut(3) {
            t.swap(1, 2);
        }
    }
    smooth_normals(g, start, istart);
}

/// Small smooth ball (icosphere, 20 or 80 triangles) for eyes, pustules, knobs.
fn ball(g: &mut Geo, center: Vec3, r: f32, stretch: Vec3, sub: u32, col: Rgba) {
    blob(g, center, r, stretch, sub, 0.0, 0, |_, _| col);
}

/// Smooth surface of revolution about `rot * Y` through `base`. `prof` is (radius, height);
/// `col` gets the profile position (0..1) and the angle.
fn lathe(
    g: &mut Geo,
    base: Vec3,
    rot: Quat,
    prof: &[(f32, f32)],
    segs: u32,
    col: impl Fn(f32, f32) -> Rgba,
) {
    let n = prof.len();
    let start = g.pos.len() as u32;
    for (i, &(r, y)) in prof.iter().enumerate() {
        let (r0, y0) = prof[i.saturating_sub(1)];
        let (r1, y1) = prof[(i + 1).min(n - 1)];
        let (dr, dy) = (r1 - r0, y1 - y0);
        for s in 0..=segs {
            let a = s as f32 / segs as f32 * TAU;
            let radial = Vec3::new(a.cos(), 0.0, a.sin());
            let nrm = rot * (radial * dy - Vec3::Y * dr).normalize_or(Vec3::Y);
            let p = base + rot * (radial * r + Vec3::Y * y);
            g.vertex(p, nrm, col(i as f32 / (n - 1) as f32, a));
        }
    }
    let w = segs + 1;
    for i in 0..(n as u32 - 1) {
        for s in 0..segs {
            let a = start + i * w + s;
            let (b, c, d) = (a + 1, a + w + 1, a + w);
            // Degenerate rings (r = 0) still emit; they are harmless slivers.
            tri_n(g, a, b, c);
            tri_n(g, a, c, d);
        }
    }
}

/// Tube swept along a polyline with per-point radius (parallel-transport frames).
fn tube(g: &mut Geo, pts: &[Vec3], radii: &[f32], sides: u32, col: impl Fn(f32, f32) -> Rgba) {
    let n = pts.len();
    let start = g.pos.len() as u32;
    let dir0 = (pts[1] - pts[0]).normalize_or(Vec3::Y);
    let mut side = dir0.any_orthonormal_vector();
    for i in 0..n {
        let t = if i == 0 {
            dir0
        } else if i == n - 1 {
            (pts[i] - pts[i - 1]).normalize_or(dir0)
        } else {
            (pts[i + 1] - pts[i - 1]).normalize_or(dir0)
        };
        side = (side - t * side.dot(t)).normalize_or(t.any_orthonormal_vector());
        let up = t.cross(side);
        for s in 0..=sides {
            let a = s as f32 / sides as f32 * TAU;
            let d = side * a.cos() + up * a.sin();
            g.vertex(pts[i] + d * radii[i], d, col(i as f32 / (n - 1) as f32, a));
        }
    }
    let w = sides + 1;
    for i in 0..(n as u32 - 1) {
        for s in 0..sides {
            let a = start + i * w + s;
            tri_n(g, a, a + 1, a + w + 1);
            tri_n(g, a, a + w + 1, a + w);
        }
    }
}

/// Quadratic Bezier sampled into `n` points.
fn bezier(a: Vec3, b: Vec3, c: Vec3, n: usize) -> Vec<Vec3> {
    (0..n)
        .map(|i| {
            let t = i as f32 / (n - 1) as f32;
            a * (1.0 - t) * (1.0 - t) + b * 2.0 * t * (1.0 - t) + c * t * t
        })
        .collect()
}

/// Gem-like crystal: a faceted hexagonal prism with a pointed tip, along `rot * Y`.
/// The shaft goes to `body`, the tip (top `tip` fraction) to `glow` when given.
#[allow(clippy::too_many_arguments)]
fn crystal(
    body: &mut Geo,
    glow: Option<&mut Geo>,
    base: Vec3,
    rot: Quat,
    r: f32,
    h: f32,
    col: Rgba,
    glow_col: Rgba,
) {
    let sides = 6;
    let shaft = h * 0.72;
    let ring = |y: f32, rr: f32| -> Vec<Vec3> {
        (0..sides)
            .map(|k| {
                let a = k as f32 / sides as f32 * TAU + 0.3;
                base + rot * Vec3::new(a.cos() * rr, y, a.sin() * rr)
            })
            .collect()
    };
    let bot = ring(-0.05 * h, r * 0.85);
    let top = ring(shaft, r);
    let apex = base + rot * Vec3::Y * h;
    let axis_at = |y: f32| base + rot * Vec3::Y * y;
    for k in 0..sides {
        let k1 = (k + 1) % sides;
        // Alternate facet brightness reads as a cut gem.
        let tone = if k % 2 == 0 { 1.0 } else { 0.78 };
        let c0 = scale(col, 0.55 * tone);
        let c1 = scale(col, 1.05 * tone);
        let inside = axis_at(shaft * 0.5);
        let i = body.pos.len() as u32;
        let n = (top[k] - bot[k]).cross(bot[k1] - bot[k]);
        let n = if n.dot(bot[k] - inside) < 0.0 { -n } else { n }.normalize_or(Vec3::Y);
        body.vertex(bot[k], n, c0);
        body.vertex(bot[k1], n, c0);
        body.vertex(top[k1], n, c1);
        body.vertex(top[k], n, c1);
        tri_n(body, i, i + 1, i + 2);
        tri_n(body, i, i + 2, i + 3);
    }
    let tipg = match glow {
        Some(g) => g,
        None => body,
    };
    for k in 0..sides {
        let k1 = (k + 1) % sides;
        let tone = if k % 2 == 0 { 1.0 } else { 0.8 };
        facet(
            tipg,
            top[k],
            top[k1],
            apex,
            axis_at(shaft),
            scale(glow_col, tone),
        );
    }
}

// ---------------------------------------------------------------------------------------
// Trees
// ---------------------------------------------------------------------------------------

/// One drooping, star-shaped pine tier. `y` is where the rim hangs, `rise` how far the
/// apex sits above it.
#[allow(clippy::too_many_arguments)]
fn pine_tier(
    g: &mut Geo,
    y: f32,
    radius: f32,
    rise: f32,
    points: u32,
    twist: f32,
    seed: u64,
    top_col: impl Fn(f32) -> Rgba,
    under: Rgba,
) {
    let apex = Vec3::Y * (y + rise);
    let n2 = points * 2;
    let rim: Vec<Vec3> = (0..n2)
        .map(|k| {
            let a = k as f32 / n2 as f32 * TAU + twist;
            let tip = k % 2 == 0;
            let jitter = 0.88 + 0.24 * rnd(k, seed);
            let rr = if tip { radius * jitter } else { radius * 0.68 };
            let dy = if tip { -rise * 0.18 } else { rise * 0.08 };
            Vec3::new(a.cos() * rr, y + dy, a.sin() * rr)
        })
        .collect();
    let mid: Vec<Vec3> = (0..points)
        .map(|k| {
            let a = (k as f32 * 2.0) / n2 as f32 * TAU + twist;
            let rr = radius * 0.5;
            Vec3::new(a.cos() * rr, y + rise * 0.5, a.sin() * rr)
        })
        .collect();
    let normal_at = |p: Vec3| {
        let radial = Vec3::new(p.x, 0.0, p.z).normalize_or(Vec3::X);
        (radial * rise * 1.1 + Vec3::Y * radius).normalize()
    };
    // Top surface: apex fan to the mid ring, then mid ring to the jagged rim.
    let ia = g.vertex(apex, Vec3::Y, top_col(0.0));
    let im: Vec<u32> = mid
        .iter()
        .map(|p| g.vertex(*p, normal_at(*p), top_col(0.5)))
        .collect();
    let ir: Vec<u32> = rim
        .iter()
        .enumerate()
        .map(|(k, p)| {
            let t = if k % 2 == 0 { 1.0 } else { 0.8 };
            g.vertex(*p, normal_at(*p), top_col(t))
        })
        .collect();
    for k in 0..points as usize {
        let k1 = (k + 1) % points as usize;
        tri_n(g, ia, im[k], im[k1]);
        let (r0, r1, r2) = (ir[2 * k], ir[2 * k + 1], ir[(2 * k + 2) % n2 as usize]);
        tri_n(g, im[k], r0, r1);
        tri_n(g, im[k], r1, im[k1]);
        tri_n(g, im[k1], r1, r2);
    }
    // Shadowed underside back to the trunk.
    let ic = g.vertex(Vec3::Y * (y + rise * 0.1), Vec3::NEG_Y, scale(under, 0.7));
    let iu: Vec<u32> = rim
        .iter()
        .map(|p| {
            let n = (Vec3::new(p.x, 0.0, p.z).normalize_or(Vec3::X) * 0.5 - Vec3::Y).normalize();
            g.vertex(*p, n, under)
        })
        .collect();
    for k in 0..n2 as usize {
        tri_n(g, ic, iu[(k + 1) % n2 as usize], iu[k]);
    }
}

/// Trunk with flared roots.
fn trunk(g: &mut Geo, h: f32, r: f32, col: Rgba, seed: u64) {
    let prof = [
        (r * 1.9, 0.0),
        (r * 1.3, 0.06),
        (r * 1.0, 0.16),
        (r * 0.82, h * 0.6),
        (r * 0.55, h),
    ];
    lathe(g, Vec3::ZERO, Quat::IDENTITY, &prof, 6, |t, a| {
        let bark = 0.85 + 0.25 * noise(a * 3.0, t * 6.0, seed);
        scale(col, bark * (0.75 + 0.35 * t))
    });
    // Root spurs.
    for k in 0..4u32 {
        let a = k as f32 / 4.0 * TAU + rnd(k, seed) * 0.8;
        let d = Vec3::new(a.cos(), 0.0, a.sin());
        let len = r * (2.8 + rnd(k + 7, seed) * 1.6);
        let pts = [
            d * r * 0.6 + Vec3::Y * 0.1,
            d * (r + len * 0.5) + Vec3::Y * 0.03,
            d * (r + len),
        ];
        tube(g, &pts, &[r * 0.55, r * 0.35, 0.004], 4, |t, _| {
            scale(col, 0.7 + 0.15 * t)
        });
    }
}

fn pine(body: &mut Geo, h: f32, snowy: bool) {
    let seed = (h * 977.0) as u64 + if snowy { 51 } else { 7 };
    let bark = lin(if snowy { 0x4a3a30 } else { 0x5b3b25 });
    trunk(body, 0.5 * h, 0.06, bark, seed);
    let (deep, mid, tip) = if snowy {
        (lin(0x1f3a32), lin(0x2c5244), lin(0x3d6a52))
    } else {
        (lin(0x1d4424), lin(0x2f6a2f), lin(0x6a9a3a))
    };
    let snow = lin(0xeef4f8);
    let tiers = 5;
    for t in 0..tiers {
        let f = t as f32 / (tiers - 1) as f32; // 0 bottom .. 1 top
        let y = (0.32 + f * 0.92) * h;
        let radius = 0.56 - f * 0.4;
        let rise = (0.36 - f * 0.08) * h;
        let hue = rnd(t, seed) * 0.25;
        let base = mixc(mid, deep, 0.25 + hue);
        let top = move |p: f32| -> Rgba {
            let c = mixc(base, tip, p * (0.45 + 0.25 * f));
            if snowy {
                // Snow lies on the upper branches; green shows at the tips and edges.
                mixc(snow, scale(c, 1.1), (p * p * 0.9).min(0.85))
            } else {
                c
            }
        };
        pine_tier(
            body,
            y,
            radius,
            rise,
            if t < 2 { 9 } else { 7 },
            rnd(t + 20, seed) * TAU,
            seed + t as u64,
            top,
            scale(deep, 0.55),
        );
    }
    // Leader tip.
    let ty = (0.32 + 0.92) * h + 0.26 * h;
    lathe(
        body,
        Vec3::Y * (ty - 0.05),
        Quat::IDENTITY,
        &[(0.05, 0.0), (0.03, 0.12 * h), (0.0, 0.22 * h)],
        5,
        |t, _| {
            if snowy {
                mixc(snow, tip, t * 0.4)
            } else {
                mixc(mid, tip, t)
            }
        },
    );
}

fn broadleaf(body: &mut Geo, h: f32) {
    let seed = (h * 613.0) as u64 + 3;
    let bark = lin(0x6a4a30);
    trunk(body, 0.62 * h, 0.065, bark, seed);
    let crown = Vec3::Y * 0.95 * h;
    // Branches from the trunk top into the clumps.
    let clumps = [
        (Vec3::new(0.0, 0.32, 0.0), 0.36),
        (Vec3::new(0.3, 0.05, 0.08), 0.28),
        (Vec3::new(-0.24, 0.1, 0.22), 0.27),
        (Vec3::new(-0.16, 0.0, -0.27), 0.27),
        (Vec3::new(0.12, 0.12, -0.3), 0.24),
        (Vec3::new(0.18, 0.3, 0.2), 0.24),
        (Vec3::new(-0.3, 0.28, -0.04), 0.23),
    ];
    for (k, (off, _)) in clumps.iter().enumerate().skip(1).step_by(2) {
        let end = crown + *off * Vec3::new(1.0, h, 1.0) * 0.8;
        let start = Vec3::Y * 0.55 * h;
        let pts = bezier(start, (start + end) / 2.0 + Vec3::Y * 0.12, end, 4);
        tube(body, &pts, &[0.045, 0.035, 0.025, 0.012], 4, |_, _| {
            scale(bark, 0.9 + 0.1 * rnd(k as u32, seed))
        });
    }
    let greens = [lin(0x3f7a2e), lin(0x4e8a32), lin(0x5f9636), lin(0x467f3a)];
    let light = lin(0x9cc456);
    let dark = lin(0x1e3d1c);
    for (k, (off, r)) in clumps.iter().enumerate() {
        let c = greens[(rnd(k as u32 + 3, seed) * 4.0) as usize % 4];
        let center = crown + *off * Vec3::new(1.0, h, 1.0);
        let lift = off.y;
        blob(
            body,
            center,
            *r,
            Vec3::new(1.0, 0.82, 1.0),
            1,
            0.28,
            seed + k as u64 * 13,
            |d, n| {
                // Sunlit top, shadowed underside, speckled with lighter leaves.
                let up = d.y * 0.5 + 0.5;
                let c = mixc(scale(dark, 1.0), c, (up * 1.3 + lift).min(1.0));
                let c = mixc(
                    c,
                    light,
                    ((up - 0.55) * 1.2 + (n - 0.5) * 0.8).clamp(0.0, 0.5),
                );
                scale(c, 0.9 + 0.2 * n)
            },
        );
    }
}

fn spire(body: &mut Geo, glow: &mut Geo, h: f32) {
    let seed = (h * 331.0) as u64 + 11;
    let rock = lin(0x2a2236);
    // Dark host rock.
    body.rock(
        Vec3::new(0.0, 0.06, 0.0),
        0.3,
        Vec3::new(1.2, 0.5, 1.1),
        seed,
        rock,
    );
    body.rock(
        Vec3::new(0.2, 0.03, -0.18),
        0.16,
        Vec3::new(1.0, 0.6, 1.0),
        seed + 1,
        scale(rock, 1.2),
    );
    let shell = lin(0x6a3fb0);
    let core = scale(lin(0xc58cff), 2.2);
    let cyan = scale(lin(0x7fe8ff), 2.0);
    let spikes = [
        (Vec3::new(0.0, 0.05, 0.0), 0.0, 0.0, 0.14, 1.45),
        (Vec3::new(0.14, 0.03, 0.08), 0.42, 0.6, 0.09, 0.85),
        (Vec3::new(-0.14, 0.03, 0.06), 0.38, 2.5, 0.1, 0.95),
        (Vec3::new(0.02, 0.03, -0.16), 0.48, 4.3, 0.08, 0.7),
        (Vec3::new(-0.2, 0.0, -0.1), 0.75, 3.5, 0.06, 0.45),
        (Vec3::new(0.24, 0.0, -0.06), 0.8, 5.6, 0.05, 0.38),
    ];
    for (k, (base, tilt, yaw, r, len)) in spikes.iter().enumerate() {
        let rot = Quat::from_rotation_y(*yaw) * Quat::from_rotation_z(*tilt);
        let c = if k % 3 == 2 { cyan } else { core };
        let sh = mixc(shell, lin(0x4a6ad0), if k % 3 == 2 { 0.5 } else { 0.0 });
        crystal(body, Some(&mut *glow), *base, rot, *r, len * h, sh, c);
    }
    // Glowing seams at the crystal roots.
    for k in 0..5u32 {
        let a = k as f32 / 5.0 * TAU + rnd(k, seed);
        let p = Vec3::new(a.cos() * 0.27, 0.02, a.sin() * 0.24);
        glow.octa(p, 0.03, scale(lin(0xb070ff), 1.6));
    }
}

fn shrub(body: &mut Geo, h: f32) {
    let seed = (h * 211.0) as u64 + 19;
    let stone = lin(0x7a6a58);
    body.rock(
        Vec3::new(0.12, 0.1, 0.05),
        0.26 * h,
        Vec3::new(1.25, 0.72, 1.0),
        seed,
        stone,
    );
    body.rock(
        Vec3::new(-0.16, 0.05, 0.18),
        0.13 * h,
        Vec3::new(1.1, 0.7, 1.2),
        seed + 1,
        scale(stone, 0.9),
    );
    body.rock(
        Vec3::new(0.3, 0.03, -0.2),
        0.07,
        Vec3::new(1.2, 0.6, 1.0),
        seed + 2,
        scale(stone, 1.1),
    );
    // Dry twiggy bush behind the boulder.
    let twig = lin(0x5a4636);
    let root = Vec3::new(-0.16, 0.0, -0.1);
    let leaf = [lin(0x8a8a4a), lin(0x9a8a52), lin(0x6e7440)];
    for k in 0..7u32 {
        let a = k as f32 / 7.0 * TAU + rnd(k, seed) * 0.7;
        let out = Vec3::new(a.cos(), 0.0, a.sin());
        let tip = root
            + out * (0.2 + rnd(k + 9, seed) * 0.14)
            + Vec3::Y * (0.38 + rnd(k + 3, seed) * 0.22) * h;
        let pts = bezier(root, root + Vec3::Y * 0.15 * h + out * 0.04, tip, 3);
        tube(body, &pts, &[0.022, 0.014, 0.004], 3, |t, _| {
            scale(twig, 0.8 + 0.3 * t)
        });
        if k % 2 == 0 {
            let c = leaf[(k / 2) as usize % 3];
            blob(
                body,
                tip - Vec3::Y * 0.03,
                0.09,
                Vec3::new(1.0, 0.75, 1.0),
                0,
                0.35,
                seed + k as u64,
                |d, n| scale(c, 0.75 + 0.25 * (d.y * 0.5 + 0.5) + 0.15 * n),
            );
        }
    }
}

/// Tree (or crystal spire / dry shrub) by variant: 0 lush pine, 1 broadleaf, 2 snowy pine,
/// 3 alien crystal spire, 4 dry shrub with boulders. `h` scales the height and reseeds
/// the details, so a few values give distinct-looking individuals.
pub fn tree(variant: u8, h: f32) -> (Geo, Geo) {
    let mut body = Geo::fine();
    let mut glow = Geo::fine();
    match variant {
        0 => pine(&mut body, h, false),
        1 => broadleaf(&mut body, h),
        2 => pine(&mut body, h, true),
        3 => spire(&mut body, &mut glow, h),
        _ => shrub(&mut body, h),
    }
    body.ao(0.25);
    (body, glow)
}

// ---------------------------------------------------------------------------------------
// Hostile nest
// ---------------------------------------------------------------------------------------

/// Hostile nest: a lumpy organic hive with chitin horns, glowing pustules and veins.
pub fn nest() -> (Geo, Geo) {
    let mut body = Geo::fine();
    let mut glow = Geo::fine();
    let seed = 0x4e57;
    let flesh = lin(0x4a2038);
    let ridge = lin(0x7a3550);
    let dark = lin(0x1c0e1a);
    let hot = lin(0xff3a6a);

    // Main mound: a lat-long shell with lumps and radial folds.
    let (segs, rings) = (30u32, 13u32);
    let start = body.pos.len();
    let istart = body.idx.len();
    let mound = |u: f32, v: f32| -> (Vec3, f32) {
        // v: 0 top .. 1 ground
        let dir = Vec3::new(u.cos(), 0.0, u.sin());
        let fold = (u * 7.0 + noise(u * 2.0, v * 3.0, seed) * 3.0).sin() * 0.5 + 0.5;
        let lump = noise3(dir * 2.0 + Vec3::Y * v * 2.0, seed);
        let r = 1.18 * (v * PI * 0.5).sin().powf(0.8) * (0.86 + 0.18 * lump + 0.06 * fold);
        let y = 0.95 * (v * PI * 0.5).cos() * (0.9 + 0.2 * lump) - 0.04;
        // Crater at the top.
        let crater = (1.0 - v / 0.18).clamp(0.0, 1.0);
        let y = y - crater * 0.22;
        (dir * r.max(0.0) + Vec3::Y * y, fold)
    };
    for ri in 0..=rings {
        let v = 0.12 + 0.88 * ri as f32 / rings as f32;
        for s in 0..=segs {
            let u = s as f32 / segs as f32 * TAU;
            let (p, fold) = mound(u, v);
            let c = mixc(flesh, ridge, fold * 0.7 * (1.0 - v * 0.5));
            let c = scale(c, 0.6 + 0.5 * (1.0 - v));
            body.vertex(p, (p - Vec3::Y * 0.2).normalize_or(Vec3::Y), c);
        }
    }
    let w = segs + 1;
    for ri in 0..rings {
        for s in 0..segs {
            let a = start as u32 + ri * w + s;
            body.idx
                .extend_from_slice(&[a, a + w + 1, a + 1, a, a + w, a + w + 1]);
        }
    }
    // Wind outward (checked on a mid-height triangle), then smooth the normals.
    {
        let k = istart + ((rings / 2 * segs) * 6) as usize;
        let t = &body.idx[k..k + 3];
        let (pa, pb, pc) = (
            Vec3::from(body.pos[t[0] as usize]),
            Vec3::from(body.pos[t[1] as usize]),
            Vec3::from(body.pos[t[2] as usize]),
        );
        let n = (pb - pa).cross(pc - pa);
        if n.dot(pa - Vec3::Y * 0.2) < 0.0 {
            for t in body.idx[istart..].chunks_mut(3) {
                t.swap(1, 2);
            }
        }
    }
    smooth_normals(&mut body, start, istart);
    // Crater lip and the glowing core inside it.
    let lip_y = mound(0.0, 0.12).0.y;
    lathe(
        &mut body,
        Vec3::Y * (lip_y - 0.1),
        Quat::IDENTITY,
        &[
            (0.0, -0.05),
            (0.2, 0.0),
            (0.32, 0.12),
            (0.38, 0.16),
            (0.42, 0.1),
        ],
        14,
        |t, _| mixc(dark, ridge, t),
    );
    blob(
        &mut glow,
        Vec3::Y * (lip_y - 0.05),
        0.2,
        Vec3::new(1.0, 0.6, 1.0),
        1,
        0.2,
        seed + 1,
        |_, n| scale(hot, 1.6 + 1.2 * n),
    );

    // Satellite lumps / egg sacs around the base.
    for k in 0..5u32 {
        let a = k as f32 / 5.0 * TAU + 0.4 + rnd(k, seed) * 0.5;
        let r = 0.95 + rnd(k + 5, seed) * 0.2;
        let c = Vec3::new(a.cos() * r, 0.12, a.sin() * r);
        let size = 0.26 + rnd(k + 9, seed) * 0.1;
        blob(
            &mut body,
            c,
            size,
            Vec3::new(1.0, 0.8, 1.0),
            2,
            0.18,
            seed + 20 + k as u64,
            |d, n| mixc(flesh, ridge, (d.y * 0.5 + 0.3 + n * 0.3).clamp(0.0, 1.0)),
        );
        // Each lump carries a glowing pustule.
        let tip = c + Vec3::new(a.cos() * 0.08, size * 0.7, a.sin() * 0.08);
        ball(
            &mut glow,
            tip,
            0.075,
            Vec3::new(1.0, 0.8, 1.0),
            1,
            scale(hot, 2.4),
        );
    }
    // Pustules on the mound.
    for k in 0..9u32 {
        let u = k as f32 / 9.0 * TAU + 0.2 + rnd(k + 40, seed) * 0.4;
        let v = 0.35 + rnd(k + 50, seed) * 0.35;
        let (p, _) = mound(u, v);
        let r = 0.05 + rnd(k + 60, seed) * 0.05;
        ball(
            &mut glow,
            p,
            r,
            Vec3::new(1.0, 0.85, 1.0),
            1,
            scale(hot, 2.0 + rnd(k, seed)),
        );
    }
    // Veins: glowing tubes from the crater running down the mound.
    for k in 0..7u32 {
        let u0 = k as f32 / 7.0 * TAU + rnd(k + 70, seed) * 0.5;
        let pts: Vec<Vec3> = (0..7)
            .map(|i| {
                let v = 0.16 + i as f32 / 6.0 * 0.8;
                let u = u0 + (i as f32 * 0.9 + k as f32).sin() * 0.12;
                let (p, _) = mound(u, v);
                p * 1.012
            })
            .collect();
        let radii: Vec<f32> = (0..7).map(|i| 0.032 - i as f32 * 0.003).collect();
        tube(&mut glow, &pts, &radii, 4, |t, _| scale(hot, 1.7 - t * 0.6));
    }
    // Chitin horns arching out of the hive.
    let chitin = lin(0x231724);
    let bone = lin(0x9a8478);
    for k in 0..8u32 {
        let u = k as f32 / 8.0 * TAU + rnd(k + 80, seed) * 0.4;
        let v = 0.3 + rnd(k + 90, seed) * 0.35;
        let (p, _) = mound(u, v);
        let out = Vec3::new(u.cos(), 0.0, u.sin());
        let len = 0.55 + rnd(k + 100, seed) * 0.45;
        let pts = bezier(
            p * 0.95,
            p + out * len * 0.25 + Vec3::Y * len * 0.75,
            p + out * len * 0.75 + Vec3::Y * len * 0.85,
            6,
        );
        let radii: Vec<f32> = (0..6).map(|i| 0.075 * (1.0 - i as f32 / 5.5)).collect();
        tube(&mut body, &pts, &radii, 6, |t, a| {
            let sheen = 0.85 + 0.3 * (a * 2.0).cos().max(0.0);
            scale(mixc(chitin, bone, t * t * 0.8), sheen)
        });
    }
    // Root tendrils spreading over the ground.
    for k in 0..7u32 {
        let a = k as f32 / 7.0 * TAU + rnd(k + 110, seed);
        let out = Vec3::new(a.cos(), 0.0, a.sin());
        let side = Vec3::new(-out.z, 0.0, out.x) * (rnd(k + 120, seed) - 0.5) * 0.5;
        let pts = bezier(
            out * 0.9 + Vec3::Y * 0.05,
            out * 1.25 + side + Vec3::Y * 0.04,
            out * 1.45 + side * 2.0,
            5,
        );
        tube(
            &mut body,
            &pts,
            &[0.07, 0.055, 0.04, 0.025, 0.006],
            5,
            |t, _| scale(mixc(flesh, dark, t), 0.9),
        );
    }
    body.ao(0.35);
    (body, glow)
}

// ---------------------------------------------------------------------------------------
// Creature
// ---------------------------------------------------------------------------------------

/// Six-legged alien critter, facing -Z, with glowing eyes and bioluminescent stripes.
pub fn creature() -> (Geo, Geo) {
    let mut body = Geo::fine();
    let mut glow = Geo::fine();
    let shell = lin(0x3a2c5e);
    let edge = lin(0x7a5ab0);
    let belly = lin(0x1e1830);
    let eye = scale(lin(0xff3a2a), 3.0);
    let lume = scale(lin(0xff4a7a), 1.6);
    let along_z = Quat::from_rotation_x(PI / 2.0); // lathe axis +Y -> +Z

    // Abdomen (behind, +Z): segmented ellipsoid.
    let ab: Vec<(f32, f32)> = (0..=9)
        .map(|i| {
            let t = i as f32 / 9.0;
            let r = (t * PI).sin().powf(0.7) * 0.2 * (1.0 - 0.06 * ((i % 2) as f32));
            (r, t * 0.5)
        })
        .collect();
    lathe(
        &mut body,
        Vec3::new(0.0, 0.25, 0.04),
        along_z,
        &ab,
        12,
        |t, a| {
            let top = (-a.sin()).max(0.0); // a = angle; -sin is up after rotation
            let band = if ((t * 9.0) as i32) % 2 == 0 {
                1.0
            } else {
                0.82
            };
            scale(
                mixc(belly, mixc(shell, edge, top * 0.4), 0.3 + top * 0.7),
                band,
            )
        },
    );
    // Thorax and head (front, -Z).
    blob(
        &mut body,
        Vec3::new(0.0, 0.26, -0.05),
        0.13,
        Vec3::new(1.0, 0.85, 1.2),
        1,
        0.08,
        77,
        |d, n| {
            mixc(
                belly,
                mixc(shell, edge, n * 0.5),
                (d.y * 0.6 + 0.6).clamp(0.0, 1.0),
            )
        },
    );
    blob(
        &mut body,
        Vec3::new(0.0, 0.25, -0.2),
        0.095,
        Vec3::new(1.15, 0.85, 1.0),
        1,
        0.06,
        78,
        |d, _| mixc(belly, shell, (d.y * 0.6 + 0.6).clamp(0.0, 1.0)),
    );
    // Carapace ridge plates on the abdomen.
    for k in 0..4 {
        let z = 0.12 + k as f32 * 0.1;
        ball(
            &mut body,
            Vec3::new(0.0, 0.25 + 0.17 - (k as f32 - 1.5).abs() * 0.015, z + 0.04),
            0.05,
            Vec3::new(2.4, 0.55, 1.0),
            1,
            scale(edge, 0.8),
        );
        // Glowing stripes between the plates.
        ball(
            &mut glow,
            Vec3::new(0.0, 0.25 + 0.16, z + 0.09),
            0.02,
            Vec3::new(3.5, 0.6, 0.8),
            0,
            lume,
        );
    }
    // Eyes: two big, two small.
    for (x, y, z, r) in [
        (0.05, 0.29, -0.28, 0.026),
        (-0.05, 0.29, -0.28, 0.026),
        (0.075, 0.27, -0.25, 0.015),
        (-0.075, 0.27, -0.25, 0.015),
    ] {
        ball(&mut glow, Vec3::new(x, y, z), r, Vec3::ONE, 1, eye);
    }
    // Mandibles.
    for side in [-1.0f32, 1.0] {
        let pts = bezier(
            Vec3::new(side * 0.04, 0.21, -0.27),
            Vec3::new(side * 0.07, 0.2, -0.35),
            Vec3::new(side * 0.015, 0.19, -0.39),
            4,
        );
        tube(&mut body, &pts, &[0.018, 0.014, 0.009, 0.002], 4, |t, _| {
            mixc(belly, lin(0xb09a88), t)
        });
    }
    // Antennae.
    for side in [-1.0f32, 1.0] {
        let pts = bezier(
            Vec3::new(side * 0.04, 0.32, -0.25),
            Vec3::new(side * 0.1, 0.45, -0.32),
            Vec3::new(side * 0.16, 0.42, -0.42),
            4,
        );
        tube(&mut body, &pts, &[0.008, 0.006, 0.004, 0.002], 3, |_, _| {
            shell
        });
        glow.octa(Vec3::new(side * 0.16, 0.42, -0.42), 0.012, lume);
    }
    // Six jointed legs.
    for k in 0..3 {
        let z = -0.12 + k as f32 * 0.11;
        for side in [-1.0f32, 1.0] {
            let splay = (k as f32 - 1.0) * 0.35;
            let out = Vec3::new(side * splay.cos(), 0.0, splay.sin() * 0.9);
            let hip = Vec3::new(side * 0.08, 0.24, z);
            let knee = hip + out * 0.2 + Vec3::Y * 0.12;
            let foot = hip + out * 0.38 + Vec3::new(0.0, -0.24, 0.0);
            tube(
                &mut body,
                &[hip, (hip + knee) / 2.0 + Vec3::Y * 0.02, knee],
                &[0.028, 0.024, 0.02],
                5,
                |_, _| shell,
            );
            tube(
                &mut body,
                &[knee, (knee + foot) / 2.0 + out * 0.03, foot],
                &[0.02, 0.014, 0.004],
                5,
                |t, _| mixc(edge, belly, t),
            );
            ball(&mut body, knee, 0.026, Vec3::ONE, 0, scale(edge, 0.9));
        }
    }
    body.ao(0.2);
    (body, glow)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tris(g: &Geo) -> usize {
        g.idx.len() / 3
    }

    #[test]
    fn budgets() {
        for v in 0..5u8 {
            for h in [0.85f32, 1.0, 1.15] {
                let (b, g) = tree(v, h);
                let n = tris(&b) + tris(&g);
                println!("tree {v} h {h}: {n} tris");
                assert!(n <= 800, "tree {v} has {n} tris");
            }
        }
        let (b, g) = nest();
        println!("nest: {} tris", tris(&b) + tris(&g));
        assert!(tris(&b) + tris(&g) <= 6000);
        let (b, g) = creature();
        println!("creature: {} tris", tris(&b) + tris(&g));
        assert!(tris(&b) + tris(&g) <= 2500);
    }
}
