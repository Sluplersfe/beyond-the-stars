//! Procedural, vertex-coloured meshes: everything 3D in the game is built from these
//! primitives and merged so a whole model is one draw.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use std::f32::consts::{PI, TAU};

pub type Rgba = [f32; 4];

/// sRGB hex → linear RGBA.
pub fn lin(hex: u32) -> Rgba {
    let c = Color::srgb_u8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8).to_linear();
    [c.red, c.green, c.blue, 1.0]
}

pub fn scale(c: Rgba, k: f32) -> Rgba {
    [c[0] * k, c[1] * k, c[2] * k, c[3]]
}

pub fn mixc(a: Rgba, b: Rgba, t: f32) -> Rgba {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        a[3] + (b[3] - a[3]) * t,
    ]
}

/// Deterministic hash → [0, 1).
pub fn hash01(a: i64, b: i64, c: u64) -> f32 {
    let mut h = (a as u64)
        .wrapping_mul(0x9e37_79b9_7f4a_7c15)
        .wrapping_add((b as u64).wrapping_mul(0xc2b2_ae3d_27d4_eb4f))
        .wrapping_add(c.wrapping_mul(0x1656_67b1_9e37_79f9));
    h ^= h >> 29;
    h = h.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    h ^= h >> 32;
    (h >> 40) as f32 / (1u64 << 24) as f32
}

/// Smooth value noise in [0, 1).
pub fn noise(x: f32, y: f32, seed: u64) -> f32 {
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let (i, j) = (x0 as i64, y0 as i64);
    let a = hash01(i, j, seed);
    let b = hash01(i + 1, j, seed);
    let c = hash01(i, j + 1, seed);
    let d = hash01(i + 1, j + 1, seed);
    let top = a + (b - a) * sx;
    let bot = c + (d - c) * sx;
    top + (bot - top) * sy
}

/// Three-octave fractal noise in roughly [0, 1).
pub fn fbm(x: f32, y: f32, seed: u64) -> f32 {
    noise(x, y, seed) * 0.55
        + noise(x * 2.1, y * 2.1, seed ^ 0x51) * 0.3
        + noise(x * 4.3, y * 4.3, seed ^ 0xa7) * 0.15
}

#[derive(Default, Clone)]
pub struct Geo {
    /// Detailed mode: bevelled and subdivided boxes, chamfered cylinders, finer spheres.
    pub fine: bool,
    pub pos: Vec<[f32; 3]>,
    pub nrm: Vec<[f32; 3]>,
    pub col: Vec<Rgba>,
    pub idx: Vec<u32>,
}

impl Geo {
    /// A builder whose primitives are tessellated for close-up models.
    pub fn fine() -> Self {
        Self {
            fine: true,
            ..default()
        }
    }

    pub fn is_empty(&self) -> bool {
        self.idx.is_empty()
    }

    pub fn vertex(&mut self, p: Vec3, n: Vec3, c: Rgba) -> u32 {
        self.pos.push(p.into());
        self.nrm.push(n.into());
        self.col.push(c);
        (self.pos.len() - 1) as u32
    }

    /// Flat-shaded quad (counter-clockwise when seen from the front).
    pub fn quad(&mut self, a: Vec3, b: Vec3, c: Vec3, d: Vec3, col: Rgba) {
        let n = (b - a).cross(d - a).normalize_or(Vec3::Y);
        let i = self.vertex(a, n, col);
        self.vertex(b, n, col);
        self.vertex(c, n, col);
        self.vertex(d, n, col);
        self.idx
            .extend_from_slice(&[i, i + 1, i + 2, i, i + 2, i + 3]);
    }

    pub fn tri(&mut self, a: Vec3, b: Vec3, c: Vec3, col: Rgba) {
        let n = (b - a).cross(c - a).normalize_or(Vec3::Y);
        let i = self.vertex(a, n, col);
        self.vertex(b, n, col);
        self.vertex(c, n, col);
        self.idx.extend_from_slice(&[i, i + 1, i + 2]);
    }

    /// Append another builder's geometry, transformed.
    pub fn append(&mut self, other: &Geo, tf: &Transform) {
        let base = self.pos.len() as u32;
        let inv_scale = 1.0 / tf.scale;
        for ((p, n), c) in other.pos.iter().zip(&other.nrm).zip(&other.col) {
            self.pos.push(tf.transform_point(Vec3::from(*p)).into());
            self.nrm.push(
                (tf.rotation * (Vec3::from(*n) * inv_scale))
                    .normalize_or(Vec3::Y)
                    .into(),
            );
            self.col.push(*c);
        }
        self.idx.extend(other.idx.iter().map(|i| i + base));
    }

    /// Axis-aligned box from its centre and full size, optionally rotated about its centre.
    pub fn cuboid(&mut self, center: Vec3, size: Vec3, col: Rgba) {
        self.cuboid_rot(center, size, Quat::IDENTITY, col);
    }

    pub fn cuboid_rot(&mut self, center: Vec3, size: Vec3, rot: Quat, col: Rgba) {
        if self.fine && size.min_element() >= 0.05 {
            return self.cuboid_fine(center, size, rot, col);
        }
        let h = size / 2.0;
        let p = |x: f32, y: f32, z: f32| center + rot * Vec3::new(x * h.x, y * h.y, z * h.z);
        // Top slightly brighter, bottom darker: reads better from the RTS angle.
        let top = scale(col, 1.08);
        self.quad(
            p(-1., 1., 1.),
            p(1., 1., 1.),
            p(1., 1., -1.),
            p(-1., 1., -1.),
            top,
        );
        self.quad(
            p(-1., -1., -1.),
            p(1., -1., -1.),
            p(1., -1., 1.),
            p(-1., -1., 1.),
            scale(col, 0.6),
        );
        self.quad(
            p(-1., -1., 1.),
            p(1., -1., 1.),
            p(1., 1., 1.),
            p(-1., 1., 1.),
            col,
        );
        self.quad(
            p(1., -1., -1.),
            p(-1., -1., -1.),
            p(-1., 1., -1.),
            p(1., 1., -1.),
            col,
        );
        self.quad(
            p(1., -1., 1.),
            p(1., -1., -1.),
            p(1., 1., -1.),
            p(1., 1., 1.),
            col,
        );
        self.quad(
            p(-1., -1., -1.),
            p(-1., -1., 1.),
            p(-1., 1., 1.),
            p(-1., 1., -1.),
            col,
        );
    }

    /// Vertical cylinder standing on `base`.
    pub fn cylinder(&mut self, base: Vec3, radius: f32, height: f32, segs: u32, col: Rgba) {
        self.cylinder_rot(base, radius, height, segs, Quat::IDENTITY, col);
    }

    /// Cylinder along the rotated Y axis, starting at `base`.
    pub fn cylinder_rot(
        &mut self,
        base: Vec3,
        radius: f32,
        height: f32,
        segs: u32,
        rot: Quat,
        col: Rgba,
    ) {
        if self.fine && radius >= 0.05 && height >= 0.05 {
            return self.cylinder_fine(base, radius, height, segs, rot, col);
        }
        let up = rot * Vec3::Y;
        let ring = |k: u32| {
            let a = k as f32 / segs as f32 * TAU;
            rot * Vec3::new(a.cos(), 0.0, a.sin())
        };
        let start = self.pos.len() as u32;
        for k in 0..=segs {
            let d = ring(k);
            self.vertex(base + d * radius, d, col);
            self.vertex(base + d * radius + up * height, d, col);
        }
        for k in 0..segs {
            let i = start + k * 2;
            self.idx
                .extend_from_slice(&[i, i + 1, i + 3, i, i + 3, i + 2]);
        }
        // Caps
        let top = scale(col, 1.08);
        let ct = self.vertex(base + up * height, up, top);
        let cb = self.vertex(base, -up, scale(col, 0.6));
        let rim = self.pos.len() as u32;
        for k in 0..=segs {
            let d = ring(k);
            self.vertex(base + d * radius + up * height, up, top);
            self.vertex(base + d * radius, -up, scale(col, 0.6));
        }
        for k in 0..segs {
            let i = rim + k * 2;
            self.idx
                .extend_from_slice(&[ct, i + 2, i, cb, i + 1, i + 3]);
        }
    }

    /// Cone standing on `base` (4 segments, rotated, make a pyramid roof).
    pub fn cone(&mut self, base: Vec3, radius: f32, height: f32, segs: u32, angle: f32, col: Rgba) {
        let apex = base + Vec3::Y * height;
        for k in 0..segs {
            let a0 = angle + k as f32 / segs as f32 * TAU;
            let a1 = angle + (k + 1) as f32 / segs as f32 * TAU;
            let p0 = base + Vec3::new(a0.cos(), 0.0, a0.sin()) * radius;
            let p1 = base + Vec3::new(a1.cos(), 0.0, a1.sin()) * radius;
            self.tri(p1, p0, apex, col);
            self.tri(p0, p1, base, scale(col, 0.6));
        }
    }

    /// Smooth UV sphere, optionally squashed (`stretch` scales each axis).
    pub fn sphere(&mut self, center: Vec3, radius: f32, stretch: Vec3, segs: u32, col: Rgba) {
        let segs = if self.fine {
            (segs * 2).clamp(12, 40)
        } else {
            segs
        };
        let rings = (segs / 2).max(3);
        let start = self.pos.len() as u32;
        for r in 0..=rings {
            let v = r as f32 / rings as f32 * PI;
            for s in 0..=segs {
                let u = s as f32 / segs as f32 * TAU;
                let n = Vec3::new(v.sin() * u.cos(), v.cos(), v.sin() * u.sin());
                self.vertex(
                    center + n * stretch * radius,
                    (n / stretch).normalize(),
                    col,
                );
            }
        }
        let w = segs + 1;
        for r in 0..rings {
            for s in 0..segs {
                let i = start + r * w + s;
                self.idx
                    .extend_from_slice(&[i, i + 1, i + w + 1, i, i + w + 1, i + w]);
            }
        }
    }

    /// Torus lying flat (around Y).
    pub fn torus(&mut self, center: Vec3, major: f32, minor: f32, segs: u32, col: Rgba) {
        let sides = if self.fine { 14 } else { 8 };
        let segs = if self.fine {
            (segs * 2).clamp(16, 48)
        } else {
            segs
        };
        let start = self.pos.len() as u32;
        for s in 0..=segs {
            let u = s as f32 / segs as f32 * TAU;
            let dir = Vec3::new(u.cos(), 0.0, u.sin());
            for k in 0..=sides {
                let v = k as f32 / sides as f32 * TAU;
                let n = dir * v.cos() + Vec3::Y * v.sin();
                self.vertex(center + dir * major + n * minor, n, col);
            }
        }
        let w = sides + 1;
        for s in 0..segs {
            for k in 0..sides {
                let i = start + s * w + k;
                self.idx
                    .extend_from_slice(&[i, i + 1, i + w + 1, i, i + w + 1, i + w]);
            }
        }
    }

    /// Small octahedron (cheap "point" for particle clouds).
    pub fn octa(&mut self, c: Vec3, r: f32, col: Rgba) {
        let px = c + Vec3::X * r;
        let nx = c - Vec3::X * r;
        let py = c + Vec3::Y * r;
        let ny = c - Vec3::Y * r;
        let pz = c + Vec3::Z * r;
        let nz = c - Vec3::Z * r;
        for (a, b) in [(px, pz), (pz, nx), (nx, nz), (nz, px)] {
            self.tri(a, b, ny, col);
            self.tri(b, a, py, col);
        }
    }

    /// Ground contact shading: darkens vertices near y = 0 over `height`.
    pub fn ao(&mut self, height: f32) {
        for (p, c) in self.pos.iter().zip(self.col.iter_mut()) {
            let t = (p[1] / height).clamp(0.0, 1.0);
            let k = 0.66 + 0.34 * (t * t * (3.0 - 2.0 * t));
            c[0] *= k;
            c[1] *= k;
            c[2] *= k;
        }
    }

    pub fn build(self) -> Mesh {
        let (uv, tan) = box_uvs(&self.pos, &self.nrm);
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.nrm)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
        .with_inserted_attribute(Mesh::ATTRIBUTE_TANGENT, tan)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.col)
        .with_inserted_indices(Indices::U32(self.idx))
    }
}

/// Texture repeats per world unit (one tile = one unit) for the tiling surface textures.
pub const UV_PER_UNIT: f32 = 0.5;

/// Box-projected UVs (the axis the normal points along picks the plane) and the matching
/// analytic tangents, so tiling detail/normal textures work on every procedural mesh
/// without unwrapping. Seams fall where the dominant axis changes, which noise-like
/// surface textures hide.
pub fn box_uvs(pos: &[[f32; 3]], nrm: &[[f32; 3]]) -> (Vec<[f32; 2]>, Vec<[f32; 4]>) {
    let mut uv = Vec::with_capacity(pos.len());
    let mut tan = Vec::with_capacity(pos.len());
    for (p, n) in pos.iter().zip(nrm) {
        let (p, n) = (Vec3::from(*p), Vec3::from(*n));
        let a = n.abs();
        // (u, v, direction of +u, direction of +v)
        let (u, v, tu, tv) = if a.x >= a.y && a.x >= a.z {
            (p.z, -p.y, Vec3::Z, Vec3::NEG_Y)
        } else if a.y >= a.z {
            (p.x, p.z, Vec3::X, Vec3::Z)
        } else {
            (p.x, -p.y, Vec3::X, Vec3::NEG_Y)
        };
        uv.push([u * UV_PER_UNIT, v * UV_PER_UNIT]);
        let t = (tu - n * n.dot(tu)).normalize_or(tu);
        let w = if n.cross(t).dot(tv) >= 0.0 { 1.0 } else { -1.0 };
        tan.push([t.x, t.y, t.z, w]);
    }
    (uv, tan)
}

/// Soft radial glow texture (white, alpha falls off), used for billboards.
pub fn glow_image(size: u32, falloff: f32) -> Image {
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    let mut data = Vec::with_capacity((size * size * 4) as usize);
    let c = (size as f32 - 1.0) / 2.0;
    for y in 0..size {
        for x in 0..size {
            let d = (Vec2::new(x as f32 - c, y as f32 - c).length() / c).min(1.0);
            let a = ((1.0 - d).max(0.0)).powf(falloff);
            data.extend_from_slice(&[255, 255, 255, (a * 255.0) as u8]);
        }
    }
    Image::new(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
}

fn grain(p: Vec3) -> f32 {
    0.94 + 0.12 * noise(p.x * 7.0 + p.y * 3.1, p.z * 7.0 + p.y * 5.3, 7)
}

fn smooth_tint(n: Vec3) -> f32 {
    if n.y > 0.5 {
        1.08
    } else if n.y < -0.5 {
        0.6
    } else {
        1.0
    }
}

impl Geo {
    /// Triangle with per-vertex normals; winding is fixed to face along the normals.
    fn emit_tri(&mut self, p: [Vec3; 3], n: [Vec3; 3], c: [Rgba; 3]) {
        let geo = (p[1] - p[0]).cross(p[2] - p[0]);
        let flip = geo.dot(n[0] + n[1] + n[2]) < 0.0;
        let i = self.pos.len() as u32;
        for k in 0..3 {
            self.vertex(p[k], n[k], c[k]);
        }
        if flip {
            self.idx.extend_from_slice(&[i, i + 2, i + 1]);
        } else {
            self.idx.extend_from_slice(&[i, i + 1, i + 2]);
        }
    }

    fn emit_quad(&mut self, p: [Vec3; 4], n: [Vec3; 4], c: [Rgba; 4]) {
        self.emit_tri([p[0], p[1], p[2]], [n[0], n[1], n[2]], [c[0], c[1], c[2]]);
        self.emit_tri([p[0], p[2], p[3]], [n[0], n[2], n[3]], [c[0], c[2], c[3]]);
    }

    /// Bevelled box: subdivided faces with grain, bright rounded edges, seams and rivets.
    fn cuboid_fine(&mut self, center: Vec3, size: Vec3, rot: Quat, col: Rgba) {
        let h = size / 2.0;
        let b = (size.min_element() * 0.2).min(0.03);
        let axes = [Vec3::X, Vec3::Y, Vec3::Z];
        let world = |l: Vec3| center + rot * l;
        let cells = |half: f32| ((2.0 * half / 0.35).ceil() as usize).clamp(1, 6);
        let at = |a: usize, sa: f32, u: usize, fu: f32, v: usize, fv: f32| {
            let mut l = Vec3::ZERO;
            l[a] = sa;
            l[u] = fu;
            l[v] = fv;
            l
        };

        for a in 0..3 {
            for s in [-1.0f32, 1.0] {
                let (u, v) = ((a + 1) % 3, (a + 2) % 3);
                let n_l = axes[a] * s;
                let nw = rot * n_l;
                let base_col = scale(col, smooth_tint(n_l));
                let (hu, hv) = (h[u] - b, h[v] - b);
                let (nu, nv) = (cells(hu), cells(hv));
                let start = self.pos.len() as u32;
                for j in 0..=nv {
                    for i in 0..=nu {
                        let fu = -hu + 2.0 * hu * i as f32 / nu as f32;
                        let fv = -hv + 2.0 * hv * j as f32 / nv as f32;
                        let wp = world(at(a, s * h[a], u, fu, v, fv));
                        self.vertex(wp, nw, scale(base_col, grain(wp)));
                    }
                }
                let w = nu as u32 + 1;
                for j in 0..nv as u32 {
                    for i in 0..nu as u32 {
                        let q = start + j * w + i;
                        if s > 0.0 {
                            self.idx
                                .extend_from_slice(&[q, q + 1, q + w + 1, q, q + w + 1, q + w]);
                        } else {
                            self.idx
                                .extend_from_slice(&[q, q + w + 1, q + 1, q, q + w, q + w + 1]);
                        }
                    }
                }
                // A groove across the long axis of big panels, with rivets at the corners.
                if hu.min(hv) > 0.2 && hu.max(hv) > 0.45 {
                    let lift = 0.004;
                    let groove = scale(col, 0.42);
                    let (along, across) = if hu >= hv { (u, v) } else { (v, u) };
                    let (ha, hc) = (h[along] - b, h[across] - b);
                    let g = 0.012;
                    let p = |t: f32, o: f32| world(at(a, s * (h[a] + lift), along, t, across, o));
                    self.emit_quad(
                        [p(-ha, -g), p(ha, -g), p(ha, g), p(-ha, g)],
                        [nw; 4],
                        [groove; 4],
                    );
                    let rivet = scale(col, 1.25);
                    let m = 0.06;
                    let (t1, t2) = (rot * axes[along], rot * axes[across]);
                    for (ta, tc) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                        let c0 =
                            world(at(a, s * h[a], along, ta * (ha - m), across, tc * (hc - m)));
                        let r = 0.022;
                        let apex = c0 + nw * 0.012;
                        let corners = [
                            c0 + (t1 + t2) * r,
                            c0 + (-t1 + t2) * r,
                            c0 + (-t1 - t2) * r,
                            c0 + (t1 - t2) * r,
                        ];
                        for k in 0..4 {
                            let (q0, q1) = (corners[k], corners[(k + 1) % 4]);
                            let fn_ = ((q0 - apex).cross(q1 - apex)).normalize_or(nw);
                            let fn_ = if fn_.dot(nw) < 0.0 { -fn_ } else { fn_ };
                            self.emit_tri([q0, q1, apex], [fn_; 3], [rivet; 3]);
                        }
                    }
                }
            }
        }

        // Rounded edges: one strip per edge, normals blend from one face to the other.
        for a in 0..3 {
            for bx in (a + 1)..3 {
                let c_ax = 3 - a - bx;
                for sa in [-1.0f32, 1.0] {
                    for sb in [-1.0f32, 1.0] {
                        let (na, nb) = (axes[a] * sa, axes[bx] * sb);
                        let n_a = rot * na;
                        let n_b = rot * nb;
                        let edge_col = scale(col, (smooth_tint(na) + smooth_tint(nb)) / 2.0 * 1.14);
                        let hc = h[c_ax] - b;
                        let n = cells(hc);
                        for k in 0..n {
                            let t0 = -hc + 2.0 * hc * k as f32 / n as f32;
                            let t1 = -hc + 2.0 * hc * (k + 1) as f32 / n as f32;
                            let pt = |on_a: bool, t: f32| {
                                let mut l = Vec3::ZERO;
                                if on_a {
                                    l[a] = sa * h[a];
                                    l[bx] = sb * (h[bx] - b);
                                } else {
                                    l[a] = sa * (h[a] - b);
                                    l[bx] = sb * h[bx];
                                }
                                l[c_ax] = t;
                                world(l)
                            };
                            let (p0, p1, p2, p3) =
                                (pt(true, t0), pt(true, t1), pt(false, t1), pt(false, t0));
                            self.emit_quad(
                                [p0, p1, p2, p3],
                                [n_a, n_a, n_b, n_b],
                                [
                                    scale(edge_col, grain(p0)),
                                    scale(edge_col, grain(p1)),
                                    scale(edge_col, grain(p2)),
                                    scale(edge_col, grain(p3)),
                                ],
                            );
                        }
                    }
                }
            }
        }

        // Corner triangles close the gaps between three strips.
        for sx in [-1.0f32, 1.0] {
            for sy in [-1.0f32, 1.0] {
                for sz in [-1.0f32, 1.0] {
                    let a = world(Vec3::new(sx * h.x, sy * (h.y - b), sz * (h.z - b)));
                    let bb = world(Vec3::new(sx * (h.x - b), sy * h.y, sz * (h.z - b)));
                    let c = world(Vec3::new(sx * (h.x - b), sy * (h.y - b), sz * h.z));
                    let ec = scale(col, 1.1);
                    self.emit_tri(
                        [a, bb, c],
                        [
                            rot * (Vec3::X * sx),
                            rot * (Vec3::Y * sy),
                            rot * (Vec3::Z * sz),
                        ],
                        [ec; 3],
                    );
                }
            }
        }
    }

    /// Cylinder with chamfered rims and smooth sides.
    fn cylinder_fine(
        &mut self,
        base: Vec3,
        radius: f32,
        height: f32,
        segs: u32,
        rot: Quat,
        col: Rgba,
    ) {
        let segs = (segs * 2).clamp(16, 40);
        let up = rot * Vec3::Y;
        let b = (radius * 0.18).min(height * 0.25).min(0.03);
        let ring = |k: u32| {
            let a = k as f32 / segs as f32 * TAU;
            rot * Vec3::new(a.cos(), 0.0, a.sin())
        };
        // (radius, y, normal radial part, normal up part, tint)
        let profile: [(f32, f32, f32, f32, f32); 5] = [
            (radius - b, 0.0, 0.0, -1.0, 0.6),
            (radius, b, 1.0, 0.0, 0.9),
            (radius, height - b, 1.0, 0.0, 1.0),
            (radius - b, height, 0.0, 1.0, 1.08),
            (0.0, height, 0.0, 1.0, 1.08),
        ];
        let first = self.pos.len() as u32;
        for &(r, y, nr, ny, tint) in &profile[..4] {
            for k in 0..=segs {
                let d = ring(k);
                let n = (d * nr + up * ny).normalize_or(up);
                let p = base + d * r + up * y;
                self.vertex(p, n, scale(col, tint * grain(p)));
            }
        }
        let w = segs + 1;
        for band in 0..3u32 {
            for k in 0..segs {
                let lo = first + band * w + k;
                let hi = lo + w;
                self.idx
                    .extend_from_slice(&[lo, hi, hi + 1, lo, hi + 1, lo + 1]);
            }
        }
        // Cap discs.
        let top = scale(col, 1.08);
        let ct = self.vertex(base + up * height, up, top);
        let cb = self.vertex(base, -up, scale(col, 0.6));
        let top_ring = first + 3 * w;
        let bot_ring = first;
        for k in 0..segs {
            self.idx
                .extend_from_slice(&[ct, top_ring + k + 1, top_ring + k]);
            self.idx
                .extend_from_slice(&[cb, bot_ring + k, bot_ring + k + 1]);
        }
    }

    /// Faceted boulder: a displaced sphere with per-facet colour variation.
    pub fn rock(&mut self, center: Vec3, radius: f32, stretch: Vec3, seed: u64, col: Rgba) {
        let (rings, segs) = (5u32, 9u32);
        let point = |r: u32, s: u32| {
            let v = r as f32 / rings as f32 * PI;
            let u = s.rem_euclid(segs) as f32 / segs as f32 * TAU;
            let n = Vec3::new(v.sin() * u.cos(), v.cos(), v.sin() * u.sin());
            let bump = 0.74 + 0.5 * hash01(r as i64, s.rem_euclid(segs) as i64, seed);
            let bump = if r == 0 || r == rings { 1.0 } else { bump };
            center + n * stretch * radius * bump
        };
        for r in 0..rings {
            for s in 0..segs {
                let (a, b, c, d) = (
                    point(r, s),
                    point(r, s + 1),
                    point(r + 1, s + 1),
                    point(r + 1, s),
                );
                let tone = 0.82 + 0.36 * hash01(r as i64 + 40, s as i64, seed ^ 0x77);
                let tone = tone
                    * (0.85 + 0.2 * ((a.y - center.y) / (radius * stretch.y)).clamp(-1.0, 1.0));
                let cc = scale(col, tone);
                if r == 0 {
                    self.tri(a, c, d, cc);
                } else if r == rings - 1 {
                    self.tri(a, b, c, cc);
                } else {
                    self.tri(a, b, c, cc);
                    self.tri(a, c, d, cc);
                }
            }
        }
    }
}
