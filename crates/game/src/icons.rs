//! Procedural UI icons, drawn once at startup from signed-distance shapes.
//!
//! Every icon is a small vector painting: layers of SDF shapes with grey "tones"
//! (two-tone faces, facets, cylinder shading), dark ink details, a light bevel on
//! top-left edges, and a soft dark outline + drop shadow so the glyph reads on any
//! panel. Icons are greyscale (white = full tint); UI nodes tint them, so the
//! shading survives any accent colour and the shadow stays dark.
//!
//! Rendering: 128 px base, 3x3 supersampling with analytic SDF coverage per
//! sub-sample, premultiplied-linear accumulation, and a full premultiplied mip
//! chain with trilinear sampling so the icons stay crisp at 10–40 px.

use std::collections::HashMap;
use std::f32::consts::{FRAC_1_SQRT_2, PI, TAU};

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    Galaxy,
    Star,
    Planet,
    Colony,
    Rocket,
    Flask,
    Box,
    Atom,
    Gear,
    Pick,
    Flame,
    Factory,
    Bolt,
    House,
    Shield,
    Pause,
    Play,
    Fast,
    Person,
    Warning,
    Cube,
    Gem,
    Belt,
    Arm,
    Trash,
    Close,
    Chevrons,
    Clock,
    Infinity,
    Swirl,
    Target,
    Cursor,
    Rotate,
    Disk,
    Plus,
    Minus,
    Eye,
    Book,
}

impl Icon {
    pub const ALL: [Icon; 38] = [
        Icon::Galaxy,
        Icon::Star,
        Icon::Planet,
        Icon::Colony,
        Icon::Rocket,
        Icon::Flask,
        Icon::Box,
        Icon::Atom,
        Icon::Gear,
        Icon::Pick,
        Icon::Flame,
        Icon::Factory,
        Icon::Bolt,
        Icon::House,
        Icon::Shield,
        Icon::Pause,
        Icon::Play,
        Icon::Fast,
        Icon::Person,
        Icon::Warning,
        Icon::Cube,
        Icon::Gem,
        Icon::Belt,
        Icon::Arm,
        Icon::Trash,
        Icon::Close,
        Icon::Chevrons,
        Icon::Clock,
        Icon::Infinity,
        Icon::Swirl,
        Icon::Target,
        Icon::Cursor,
        Icon::Rotate,
        Icon::Disk,
        Icon::Plus,
        Icon::Minus,
        Icon::Eye,
        Icon::Book,
    ];
}

#[derive(Resource)]
pub struct Icons(pub HashMap<Icon, Handle<Image>>);

impl Icons {
    pub fn get(&self, i: Icon) -> Handle<Image> {
        self.0.get(&i).cloned().unwrap_or_default()
    }
}

pub struct IconPlugin;

impl Plugin for IconPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, make_icons);
    }
}

/// Base resolution of every icon (display sizes are ~10–50 px).
const SIZE: usize = 128;

fn make_icons(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let chains = render_all(SIZE);
    let map = Icon::ALL
        .iter()
        .zip(chains)
        .map(|(i, chain)| (*i, images.add(to_image(SIZE, &chain))))
        .collect();
    commands.insert_resource(Icons(map));
}

/// Render every icon (in parallel) into premultiplied-linear mip chains.
fn render_all(size: usize) -> Vec<Vec<Vec<[f32; 2]>>> {
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(1, Icon::ALL.len());
    let mut out: Vec<Vec<Vec<[f32; 2]>>> = vec![Vec::new(); Icon::ALL.len()];
    let per = Icon::ALL.len().div_ceil(threads);
    std::thread::scope(|s| {
        for (chunk, icons) in out.chunks_mut(per).zip(Icon::ALL.chunks(per)) {
            s.spawn(move || {
                for (slot, icon) in chunk.iter_mut().zip(icons) {
                    *slot = mip_chain(rasterize(*icon, size), size);
                }
            });
        }
    });
    out
}

// ------------------------------------------------------------------ tones & style

/// Brightest face (catches the light).
const HI: f32 = 1.0;
/// Default face.
const BASE: f32 = 0.9;
/// Secondary face / part.
const MID: f32 = 0.76;
/// Shaded face.
const LOW: f32 = 0.6;
/// Dark recesses / glass.
const DEEP: f32 = 0.42;
/// Ink details (windows, slots, lines) — stays dark under any tint.
const INK: f32 = 0.13;

/// Canvas half-extent in shape units (shapes live in about [-0.95, 0.95]).
const EXTENT: f32 = 1.13;

// ------------------------------------------------------------------ canvas

/// One sample of a layered SDF painting. Colours are grey tones (sRGB-ish
/// 0..1); `sil` is the signed distance of the overall silhouette.
struct Cv {
    col: f32,
    a: f32,
    sil: f32,
    aa: f32,
}

impl Cv {
    fn new(aa: f32) -> Self {
        Cv {
            col: 0.0,
            a: 0.0,
            sil: f32::MAX,
            aa,
        }
    }

    fn cov(&self, d: f32) -> f32 {
        (0.5 - d / self.aa).clamp(0.0, 1.0)
    }

    /// Paint an opaque shape with a tone.
    fn fill(&mut self, d: f32, tone: f32) {
        self.paint(d, tone, 1.0);
    }

    /// Paint a shape with tone and opacity (glass, smoke).
    fn paint(&mut self, d: f32, tone: f32, op: f32) {
        let k = self.cov(d) * op;
        self.col = self.col * (1.0 - k) + tone * k;
        self.a = self.a * (1.0 - k) + k;
        self.sil = self.sil.min(d);
    }

    /// Translucent haze that does not take part in the silhouette (no
    /// outline or drop shadow around it).
    fn haze(&mut self, d: f32, tone: f32, op: f32) {
        let k = self.cov(d) * op;
        self.col = self.col * (1.0 - k) + tone * k;
        self.a = self.a * (1.0 - k) + k;
    }

    /// Recolour what is already painted (clipped to the painting).
    fn tint(&mut self, d: f32, tone: f32) {
        let k = self.cov(d);
        self.col = self.col * (1.0 - k) + tone * self.a * k;
    }

    /// Dark detail line / opening, clipped to the painting.
    fn ink(&mut self, d: f32) {
        self.tint(d, INK);
    }

    /// Erase (transparent gap).
    fn cut(&mut self, d: f32) {
        let k = 1.0 - self.cov(d);
        self.col *= k;
        self.a *= k;
        self.sil = self.sil.max(-d);
    }
}

// ------------------------------------------------------------------ rasterizer

fn srgb_to_lin(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn lin_to_srgb(c: f32) -> f32 {
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Silhouette distance at `p` (cheap re-evaluation of the painting).
fn sil(icon: Icon, p: Vec2) -> f32 {
    let mut c = Cv::new(1e-3);
    draw(icon, p, &mut c);
    c.sil
}

/// Rasterize one icon into premultiplied linear [grey, alpha] pixels.
fn rasterize(icon: Icon, size: usize) -> Vec<[f32; 2]> {
    const SS: usize = 3;
    let px = 2.0 * EXTENT / size as f32;
    let sub = px / SS as f32;
    let mut out = Vec::with_capacity(size * size);
    for y in 0..size {
        for x in 0..size {
            let centre = v(
                (x as f32 + 0.5) * px - EXTENT,
                EXTENT - (y as f32 + 0.5) * px,
            );
            // Supersampled painting.
            let (mut c, mut a) = (0.0, 0.0);
            for sy in 0..SS {
                for sx in 0..SS {
                    let p = v(
                        (x as f32 + (sx as f32 + 0.5) / SS as f32) * px - EXTENT,
                        EXTENT - (y as f32 + (sy as f32 + 0.5) / SS as f32) * px,
                    );
                    let mut cv = Cv::new(sub);
                    draw(icon, p, &mut cv);
                    if cv.a > 0.0 {
                        // Soft top-lit gradient across the whole glyph.
                        let lit = 0.9 + 0.1 * p.y - 0.03 * p.x;
                        let tone = (cv.col / cv.a * lit).clamp(0.0, 1.0);
                        c += srgb_to_lin(tone) * cv.a;
                        a += cv.a;
                    }
                }
            }
            let n = (SS * SS) as f32;
            let (mut c, a) = (c / n, a / n);

            // Silhouette distance, renormalised by its local gradient so that
            // inexact fields (spirals, clipped unions, lemniscates) still get
            // an outline / shadow of uniform width.
            let d0 = sil(icon, centre);
            let gx = sil(icon, centre + v(px, 0.0)) - d0;
            let gy = sil(icon, centre + v(0.0, px)) - d0;
            let k = px / v(gx, gy).length().clamp(0.35 * px, 3.0 * px);
            let sil_n = |q: Vec2| sil(icon, q) * k;
            let d = d0 * k;
            if a > 0.0 {
                // Bevel: lit rim where the shape ends just up-left, shade down-right.
                let up = sil_n(centre + v(-0.03, 0.05));
                let dn = sil_n(centre + v(0.03, -0.05));
                let rim = smoothstep(-0.5 * px, 0.5 * px, up);
                let shade = smoothstep(-0.5 * px, 0.5 * px, dn);
                c += (a - c) * 0.55 * rim;
                c *= 1.0 - 0.35 * shade;
            }
            // Dark outline hugging the silhouette + soft drop shadow.
            let outline = 0.8 * (1.0 - smoothstep(0.045, 0.045 + px, d));
            let ds = sil_n(centre + v(-0.035, 0.075));
            let shadow = 0.42 * (1.0 - smoothstep(-0.04, 0.13, ds));
            let back = outline.max(shadow);
            let a = a + back * (1.0 - a);
            c = c.min(a);
            out.push([c, a]);
        }
    }
    out
}

/// Box-filtered premultiplied mip chain (level 0 first).
fn mip_chain(base: Vec<[f32; 2]>, size: usize) -> Vec<Vec<[f32; 2]>> {
    let mut levels = vec![base];
    let mut n = size;
    while n > 1 {
        let h = n / 2;
        let src = levels.last().unwrap();
        let mut dst = Vec::with_capacity(h * h);
        for y in 0..h {
            for x in 0..h {
                let q = [
                    src[2 * y * n + 2 * x],
                    src[2 * y * n + 2 * x + 1],
                    src[(2 * y + 1) * n + 2 * x],
                    src[(2 * y + 1) * n + 2 * x + 1],
                ];
                dst.push([
                    (q[0][0] + q[1][0] + q[2][0] + q[3][0]) * 0.25,
                    (q[0][1] + q[1][1] + q[2][1] + q[3][1]) * 0.25,
                ]);
            }
        }
        levels.push(dst);
        n = h;
    }
    levels
}

/// Straight-alpha sRGB bytes for one premultiplied-linear pixel.
fn encode(p: [f32; 2]) -> [u8; 4] {
    let [c, a] = p;
    let g = if a > 1e-5 { (c / a).min(1.0) } else { 0.0 };
    let g = (lin_to_srgb(g) * 255.0 + 0.5) as u8;
    [g, g, g, (a.clamp(0.0, 1.0) * 255.0 + 0.5) as u8]
}

fn to_image(size: usize, chain: &[Vec<[f32; 2]>]) -> Image {
    let data: Vec<u8> = chain.iter().flatten().flat_map(|p| encode(*p)).collect();
    let mut img = Image::new_uninit(
        Extent3d {
            width: size as u32,
            height: size as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    img.data = Some(data);
    img.texture_descriptor.mip_level_count = chain.len() as u32;
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        ..default()
    });
    img
}

// ------------------------------------------------------------------ SDF helpers
// Signed distances (negative inside), coordinates in about [-1, 1], y up.

fn v(x: f32, y: f32) -> Vec2 {
    Vec2::new(x, y)
}

fn circle(p: Vec2, c: Vec2, r: f32) -> f32 {
    p.distance(c) - r
}

fn ring(p: Vec2, c: Vec2, r: f32, w: f32) -> f32 {
    (p.distance(c) - r).abs() - w
}

fn rect(p: Vec2, c: Vec2, half: Vec2) -> f32 {
    let d = (p - c).abs() - half;
    d.max(Vec2::ZERO).length() + d.x.max(d.y).min(0.0)
}

fn rrect(p: Vec2, c: Vec2, half: Vec2, r: f32) -> f32 {
    rect(p, c, half - Vec2::splat(r)) - r
}

/// Capsule from `a` to `b` with radius `w`.
fn seg(p: Vec2, a: Vec2, b: Vec2, w: f32) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let h = (pa.dot(ba) / ba.dot(ba)).clamp(0.0, 1.0);
    (pa - ba * h).length() - w
}

/// Polyline stroke through `pts` with radius `w`.
fn path(p: Vec2, pts: &[Vec2], w: f32) -> f32 {
    pts.windows(2)
        .map(|s| seg(p, s[0], s[1], w))
        .fold(f32::MAX, f32::min)
}

/// Exact distance to an arbitrary simple polygon (any winding, may be concave).
fn polygon(p: Vec2, pts: &[Vec2]) -> f32 {
    let n = pts.len();
    let mut d = (p - pts[0]).length_squared();
    let mut s = 1.0;
    let mut j = n - 1;
    for i in 0..n {
        let e = pts[j] - pts[i];
        let w = p - pts[i];
        let b = w - e * (w.dot(e) / e.dot(e)).clamp(0.0, 1.0);
        d = d.min(b.length_squared());
        let c0 = p.y >= pts[i].y;
        let c1 = p.y < pts[j].y;
        let c2 = e.x * w.y > e.y * w.x;
        if (c0 && c1 && c2) || (!c0 && !c1 && !c2) {
            s = -s;
        }
        j = i;
    }
    s * d.sqrt()
}

/// Tapered capsule from `a` (radius `ra`) to `b` (radius `rb`).
fn taper(p: Vec2, a: Vec2, b: Vec2, ra: f32, rb: f32) -> f32 {
    let axis = b - a;
    let h = axis.length();
    let y = axis / h;
    let x = v(y.y, -y.x);
    let q = p - a;
    let q = v(q.dot(x).abs(), q.dot(y));
    let bb = (ra - rb) / h;
    let aa = (1.0 - bb * bb).sqrt();
    let k = q.dot(v(-bb, aa));
    if k < 0.0 {
        q.length() - ra
    } else if k > aa * h {
        (q - v(0.0, h)).length() - rb
    } else {
        q.dot(v(aa, bb)) - ra
    }
}

/// Arc of radius `r`, stroke radius `w`, centred on direction `dir` (radians)
/// spanning ±`half` radians.
fn arc(p: Vec2, c: Vec2, r: f32, w: f32, dir: f32, half: f32) -> f32 {
    let q = rot(p - c, dir - PI / 2.0);
    let q = v(q.x.abs(), q.y);
    let sc = v(half.sin(), half.cos());
    if sc.y * q.x > sc.x * q.y {
        (q - sc * r).length() - w
    } else {
        (q.length() - r).abs() - w
    }
}

/// Approximate distance to an ellipse outline (semi-axes `ab`).
fn ellipse(p: Vec2, ab: Vec2) -> f32 {
    let k0 = (p / ab).length();
    let k1 = (p / (ab * ab)).length();
    if k1 < 1e-6 {
        -ab.min_element()
    } else {
        k0 * (k0 - 1.0) / k1
    }
}

/// Rotate a point by `-a` (i.e. into a frame rotated by `a`).
fn rot(p: Vec2, a: f32) -> Vec2 {
    let (s, c) = a.sin_cos();
    Vec2::new(c * p.x + s * p.y, -s * p.x + c * p.y)
}

fn smin(a: f32, b: f32, k: f32) -> f32 {
    let h = (0.5 + 0.5 * (b - a) / k).clamp(0.0, 1.0);
    b + (a - b) * h - k * h * (1.0 - h)
}

/// Shade of a sphere lit from the upper left.
fn sphere_tone(p: Vec2, c: Vec2, r: f32) -> f32 {
    let q = (p - c) / r;
    let z = (1.0 - q.length_squared()).max(0.0).sqrt();
    let n = Vec3::new(q.x, q.y, z);
    let l = Vec3::new(-0.45, 0.6, 0.66).normalize();
    let diff = n.dot(l).max(0.0);
    let spec = n.dot((l + Vec3::Z).normalize()).max(0.0).powf(24.0);
    (0.5 + 0.48 * diff + 0.25 * spec).min(1.0)
}

/// Tone of a flat facet given its outward direction in the image plane.
fn facet(dir: Vec2) -> f32 {
    let l = v(-0.55, 0.83);
    LOW + (HI - LOW) * (dir.normalize_or_zero().dot(l) * 0.5 + 0.5)
}

/// Point on a circle.
fn polar(r: f32, a: f32) -> Vec2 {
    v(a.cos() * r, a.sin() * r)
}

// ------------------------------------------------------------------ icons

/// Draw `f` uniformly scaled by `s` about the origin (keeps AA width and the
/// silhouette distance in canvas units).
fn scaled(p: Vec2, c: &mut Cv, s: f32, f: fn(Vec2, &mut Cv)) {
    let (aa, sil) = (c.aa, c.sil);
    c.aa = aa / s;
    c.sil = sil / s;
    f(p / s, c);
    c.aa = aa;
    c.sil *= s;
}

fn draw(icon: Icon, p: Vec2, c: &mut Cv) {
    match icon {
        Icon::Galaxy => galaxy(p, c),
        Icon::Swirl => swirl(p, c),
        Icon::Star => star(p, c),
        Icon::Planet => planet(p, c),
        Icon::Colony => colony(p, c),
        Icon::Rocket => rocket(p, c),
        Icon::Flask => flask(p, c),
        Icon::Box => crate_box(p, c),
        Icon::Cube => cube(p, c),
        Icon::Gem => gem(p, c),
        Icon::Atom => atom(p, c),
        Icon::Gear => gear(p, c),
        Icon::Pick => scaled(p, c, 0.9, pick),
        Icon::Flame => flame(p, c),
        Icon::Factory => factory(p, c),
        Icon::Bolt => scaled(p, c, 0.9, bolt),
        Icon::House => house(p, c),
        Icon::Shield => scaled(p, c, 0.93, shield),
        Icon::Pause => {
            c.fill(rrect(p, v(-0.3, 0.0), v(0.17, 0.66), 0.07), BASE);
            c.fill(rrect(p, v(0.3, 0.0), v(0.17, 0.66), 0.07), BASE);
        }
        Icon::Play => {
            let d = polygon(p, &[v(-0.4, 0.6), v(-0.4, -0.6), v(0.62, 0.0)]) - 0.08;
            c.fill(d, BASE);
        }
        Icon::Fast => {
            let a = polygon(p, &[v(-0.78, 0.5), v(-0.78, -0.5), v(-0.04, 0.0)]) - 0.07;
            let b = polygon(p, &[v(0.04, 0.5), v(0.04, -0.5), v(0.78, 0.0)]) - 0.07;
            c.fill(a, MID);
            c.fill(b, BASE);
        }
        Icon::Person => person(p, c),
        Icon::Warning => warning(p, c),
        Icon::Belt => belt(p, c),
        Icon::Arm => arm(p, c),
        Icon::Trash => trash(p, c),
        Icon::Close => {
            let d = seg(p, v(-0.56, -0.56), v(0.56, 0.56), 0.13).min(seg(
                p,
                v(-0.56, 0.56),
                v(0.56, -0.56),
                0.13,
            ));
            c.fill(d, BASE);
        }
        Icon::Chevrons => {
            for (k, tone) in [(0, DEEP + 0.08), (1, MID), (2, HI)] {
                let x = -0.48 + k as f32 * 0.42;
                let d = path(
                    p,
                    &[v(x - 0.2, 0.5), v(x + 0.2, 0.0), v(x - 0.2, -0.5)],
                    0.11,
                );
                c.fill(d, tone);
            }
        }
        Icon::Clock => clock(p, c),
        Icon::Infinity => infinity(p, c),
        Icon::Target => scaled(p, c, 0.92, target),
        Icon::Cursor => cursor(p, c),
        Icon::Rotate => rotate(p, c),
        Icon::Disk => disk(p, c),
        Icon::Plus => {
            let d = rrect(p, Vec2::ZERO, v(0.66, 0.13), 0.06).min(rrect(
                p,
                Vec2::ZERO,
                v(0.13, 0.66),
                0.06,
            ));
            c.fill(d, BASE);
        }
        Icon::Minus => c.fill(rrect(p, Vec2::ZERO, v(0.66, 0.13), 0.06), BASE),
        Icon::Eye => eye(p, c),
        Icon::Book => book(p, c),
    }
}

/// Distance to a logarithmic-ish spiral arm family plus the tone fall-off.
fn spiral(p: Vec2, arms: f32, twist: f32, phase: f32) -> (f32, f32) {
    let r = p.length().max(1e-4);
    let a = p.y.atan2(p.x);
    let sector = TAU / arms;
    let diff = (a - r * twist - phase).rem_euclid(sector) - sector * 0.5;
    let diff = sector * 0.5 - diff.abs();
    let slope = (1.0 + (twist * r).powi(2)).sqrt();
    (diff * r / slope, r)
}

fn galaxy(p: Vec2, c: &mut Cv) {
    // An inclined spiral disc: work in the disc plane (un-squashed).
    let squash = 0.56;
    let q = rot(p, 0.42);
    let s = v(q.x, q.y / squash);
    // Faint dusty disc so the silhouette reads as an ellipse when tiny.
    let disc = ellipse(q, v(0.94, 0.94 * squash));
    c.haze(disc, MID, 0.16);
    c.haze(ellipse(q, v(0.62, 0.62 * squash)), MID, 0.14);
    let (d, r) = spiral(s, 2.0, -3.0, 0.9);
    let w = 0.04 + 0.2 * (1.0 - r / 0.95).max(0.0).powf(0.7);
    let arm = ((d - w).max(r - 0.95).max(0.12 - r)) * squash;
    c.fill(arm, (1.04 - 0.3 * r).clamp(MID, HI));
    // Dust lane along the inner edge of each arm.
    c.tint(
        (((d - w * 0.45).abs() - 0.025).max(r - 0.75).max(0.3 - r)) * squash,
        LOW,
    );
    // Bright bulge with a hot centre.
    let bulge = ellipse(q, v(0.32, 0.32 * squash + 0.04));
    c.fill(
        bulge,
        sphere_tone(v(q.x, q.y / 0.7), Vec2::ZERO, 0.32).max(0.82),
    );
    c.fill(circle(p, Vec2::ZERO, 0.075), HI);
    for (pt, rr) in [(v(0.66, 0.66), 0.15), (v(-0.72, -0.58), 0.12)] {
        c.fill(sparkle(p, pt, rr), HI);
    }
}

/// Four-point sparkle.
fn sparkle(p: Vec2, c: Vec2, r: f32) -> f32 {
    let q = (p - c).abs();
    let pts = [v(0.0, r), v(r * 0.22, r * 0.22), v(r, 0.0), v(0.0, 0.0)];
    polygon(q, &pts) - r * 0.06
}

fn swirl(p: Vec2, c: &mut Cv) {
    let (d, r) = spiral(p, 3.0, -4.2, 0.0);
    let w = 0.02 + 0.13 * (PI * ((r - 0.12) / 0.8).clamp(0.0, 1.0)).sin();
    let arm = (d - w).max(r - 0.92).max(0.2 - r);
    c.fill(arm, (1.05 - 0.45 * r).clamp(LOW, HI));
    c.fill(ring(p, Vec2::ZERO, 0.2, 0.055), HI);
    c.fill(circle(p, Vec2::ZERO, 0.07), HI);
}

fn star(p: Vec2, c: &mut Cv) {
    let o = v(0.0, -0.06);
    let pt = |k: i32| {
        let a = PI / 2.0 + k as f32 * PI / 5.0;
        let r = if k.rem_euclid(2) == 0 { 0.92 } else { 0.4 };
        o + polar(r, a)
    };
    let pts: Vec<Vec2> = (0..10).map(pt).collect();
    let d = polygon(p, &pts) - 0.04;
    // Bevelled facets: each spike splits into two halves with their own tone.
    let q = p - o;
    let ang = (q.y.atan2(q.x) - PI / 2.0).rem_euclid(TAU);
    let k = (ang / (PI / 5.0)).floor() as i32;
    let mid = PI / 2.0 + (k as f32 + 0.5) * PI / 5.0;
    let tone = facet(polar(1.0, mid));
    c.fill(d, tone);
    c.fill(circle(p, o, 0.12).max(d), (tone + 0.12).min(HI));
}

fn planet(p: Vec2, c: &mut Cv) {
    let tilt = 0.38;
    let q = rot(p, tilt);
    let ab = v(0.93, 0.28);
    let band = ellipse(q, ab).abs() - 0.065;
    let sphere = circle(p, Vec2::ZERO, 0.56);
    // Back half of the ring, then the planet, then a gap and the front half.
    c.fill(band.max(-q.y), MID);
    c.fill(sphere, sphere_tone(p, Vec2::ZERO, 0.56));
    // Cloud bands.
    let qb = rot(p, tilt);
    c.tint(
        (ring(qb, v(0.0, 1.25), 1.15, 0.05)).max(sphere),
        sphere_tone(p, Vec2::ZERO, 0.56) * 0.8,
    );
    c.cut((ellipse(q, ab).abs() - 0.13).max(q.y).max(sphere - 0.06));
    c.fill(band.max(q.y), BASE);
    c.ink((ellipse(q, ab * 0.995).abs() - 0.012).max(band + 0.02));
}

fn colony(p: Vec2, c: &mut Cv) {
    let base_y = -0.42;
    // Side module and antenna mast.
    c.fill(rrect(p, v(0.62, -0.18), v(0.2, 0.24), 0.05), MID);
    c.ink(rrect(p, v(0.62, -0.13), v(0.1, 0.06), 0.02));
    c.fill(seg(p, v(-0.0, 0.2), v(-0.0, 0.66), 0.045), MID);
    c.fill(circle(p, v(0.0, 0.72), 0.09), HI);
    // Dome with glazing frame.
    let dome = circle(p, v(0.0, base_y), 0.62).max(base_y - p.y);
    c.cut(
        circle(p, v(0.0, base_y), 0.7)
            .max(base_y - p.y)
            .max(p.x - 0.4),
    );
    c.fill(dome, sphere_tone(p, v(0.0, base_y), 0.62) * 0.98);
    let frame = (p.y - (base_y + 0.24))
        .abs()
        .min((p.y - (base_y + 0.45)).abs())
        - 0.03;
    let frame = frame
        .min(ellipse(p - v(0.0, base_y), v(0.3, 0.62)).abs() - 0.03)
        .min(p.x.abs() - 0.03)
        .max(dome + 0.06);
    c.ink(frame);
    // Foundation platform.
    c.fill(rrect(p, v(0.0, base_y - 0.17), v(0.9, 0.15), 0.06), LOW);
    c.ink(rect(p, v(0.0, base_y - 0.17), v(0.78, 0.025)));
    c.fill(rrect(p, v(-0.6, base_y - 0.38), v(0.07, 0.1), 0.02), DEEP);
    c.fill(rrect(p, v(0.6, base_y - 0.38), v(0.07, 0.1), 0.02), DEEP);
}

fn rocket(p: Vec2, c: &mut Cv) {
    let q = rot(p, -PI / 4.0);
    let q = q + v(0.0, 0.04);
    // Ogive nose + cylindrical body.
    let (rr, cx) = (1.292, 1.052);
    let nose = circle(q, v(-cx, 0.2), rr).max(circle(q, v(cx, 0.2), rr));
    let body = nose
        .max(-0.5 - q.y)
        .min(rect(q, v(0.0, -0.15), v(0.24, 0.35)));
    let cyl = |x: f32| 1.0 - 0.45 * ((x / 0.24 + 1.0) * 0.5).clamp(0.0, 1.0).powf(1.5);
    // Fins and nozzle behind the body.
    let fin_l = polygon(
        q,
        &[
            v(-0.2, 0.02),
            v(-0.52, -0.42),
            v(-0.52, -0.72),
            v(-0.2, -0.46),
        ],
    ) - 0.03;
    let fin_r = polygon(
        q,
        &[v(0.2, 0.02), v(0.2, -0.46), v(0.52, -0.72), v(0.52, -0.42)],
    ) - 0.03;
    c.fill(fin_l, MID);
    c.fill(fin_r, LOW);
    c.fill(
        polygon(
            q,
            &[
                v(-0.15, -0.46),
                v(0.15, -0.46),
                v(0.21, -0.64),
                v(-0.21, -0.64),
            ],
        ) - 0.02,
        DEEP + 0.06,
    );
    // Flame.
    c.fill(taper(q, v(0.0, -0.76), v(0.0, -1.03), 0.15, 0.02), HI);
    c.cut(taper(q, v(0.0, -0.75), v(0.0, -0.93), 0.06, 0.02));
    c.fill(taper(q, v(0.0, -0.76), v(0.0, -0.9), 0.05, 0.015), MID);
    c.fill(body, cyl(q.x));
    // Nose cap band and porthole.
    c.ink(rect(q, v(0.0, 0.47), v(0.3, 0.022)).max(body + 0.02));
    c.tint(body.max(0.47 - q.y), cyl(q.x) * 0.84);
    c.ink(rect(q, v(0.0, -0.38), v(0.3, 0.02)).max(body + 0.02));
    c.fill(circle(q, v(0.0, 0.14), 0.15), LOW);
    c.fill(circle(q, v(0.0, 0.14), 0.1), DEEP);
    c.fill(circle(q, v(-0.035, 0.18), 0.035), HI);
    // Centre fin edge-on.
    c.fill(rrect(q, v(0.0, -0.56), v(0.04, 0.17), 0.02), DEEP + 0.1);
}

fn flask(p: Vec2, c: &mut Cv) {
    let body = (polygon(
        p,
        &[
            v(-0.12, 0.34),
            v(-0.64, -0.58),
            v(0.64, -0.58),
            v(0.12, 0.34),
        ],
    ) - 0.1)
        .min(rect(p, v(0.0, 0.52), v(0.15, 0.3)));
    let rim = rrect(p, v(0.0, 0.8), v(0.27, 0.075), 0.035);
    // Glass: translucent fill and solid wall.
    c.paint(body, HI, 0.32);
    let inner = body + 0.08;
    // Liquid with a wavy surface and bubbles.
    let surf = -0.14 + 0.03 * (p.x * 9.0).sin();
    let liquid = inner.max(p.y - surf);
    c.fill(liquid, BASE);
    c.tint(liquid.max(surf - 0.1 - p.y), HI);
    for (o, r) in [
        (v(-0.18, -0.35), 0.075),
        (v(0.12, -0.2), 0.05),
        (v(0.2, -0.45), 0.04),
    ] {
        c.cut(circle(p, o, r));
        c.paint(circle(p, o, r), HI, 0.32);
    }
    c.fill(body.abs() - 0.055, HI);
    c.fill(rim, HI);
    // Glint on the glass.
    c.fill(
        seg(p, v(-0.32, -0.32), v(-0.12, 0.08), 0.03).max(inner + 0.02),
        HI,
    );
    c.fill(seg(p, v(0.0, 0.22), v(0.0, 0.62), 0.02).max(inner), MID);
}

/// Isometric box corners: (top, left, centre, right, bottom-left, bottom, bottom-right).
const ISO: [Vec2; 7] = [
    Vec2::new(0.0, 0.86),
    Vec2::new(-0.8, 0.43),
    Vec2::new(0.0, 0.0),
    Vec2::new(0.8, 0.43),
    Vec2::new(-0.8, -0.43),
    Vec2::new(0.0, -0.86),
    Vec2::new(0.8, -0.43),
];

fn iso_faces(p: Vec2, c: &mut Cv, s: f32) -> (f32, f32, f32) {
    let [t, l, m, r, bl, b, br] = ISO.map(|x| x * s);
    let top = polygon(p, &[t, l, m, r]);
    let left = polygon(p, &[l, bl, b, m]);
    let right = polygon(p, &[m, b, br, r]);
    let all = polygon(p, &[t, l, bl, b, br, r]) - 0.03;
    c.fill(all, MID);
    c.tint(top, HI);
    c.tint(left, MID);
    c.tint(right, LOW);
    (top, left, right)
}

fn cube(p: Vec2, c: &mut Cv) {
    iso_faces(p, c, 0.97);
    let [t, l, m, r, ..] = ISO.map(|x| x * 0.97);
    // Inset panel on each face.
    let k = 0.72;
    let ctop = (t + l + m + r) * 0.25;
    let inset = |a: Vec2, cc: Vec2| cc + (a - cc) * k;
    let [_, _, _, _, bl, b, br] = ISO.map(|x| x * 0.97);
    let cl = (l + bl + b + m) * 0.25;
    let cr = (m + b + br + r) * 0.25;
    let edge = |pts: [Vec2; 4], cc: Vec2| polygon(p, &pts.map(|a| inset(a, cc))).abs() - 0.022;
    c.tint(edge([t, l, m, r], ctop), BASE);
    c.tint(edge([l, bl, b, m], cl), LOW + 0.04);
    c.tint(edge([m, b, br, r], cr), DEEP);
}

fn crate_box(p: Vec2, c: &mut Cv) {
    iso_faces(p, c, 0.97);
    let [t, l, m, r, bl, b, _] = ISO.map(|x| x * 0.97);
    // Tape strip running across the top and down the front-left face.
    let a0 = (t + l) * 0.5;
    let a1 = (m + r) * 0.5;
    let tape_top = polygon(
        p,
        &[
            a0 + (t - l) * 0.09,
            a0 - (t - l) * 0.09,
            a1 - (r - m) * 0.09,
            a1 + (r - m) * 0.09,
        ],
    );
    c.tint(tape_top, MID);
    let f0 = (l + m) * 0.5;
    let f1 = (bl + b) * 0.5;
    let w = (m - l) * 0.09;
    c.tint(polygon(p, &[f0 - w, f1 - w, f1 + w, f0 + w]), LOW - 0.06);
    // Seams on the edges between faces.
    c.ink(seg(p, m, b, 0.018));
    c.ink(seg(p, l, m, 0.018).min(seg(p, m, r, 0.018)));
    // Label on the right face.
    let lc = (m + b + r + (b + (r - m))) * 0.25;
    let ax = (r - m) * 0.22;
    let ay = (b - m) * 0.14;
    c.ink(polygon(
        p,
        &[lc - ax - ay, lc + ax - ay, lc + ax + ay, lc - ax + ay],
    ));
}

fn gem(p: Vec2, c: &mut Cv) {
    let y_g = 0.22;
    let (a, b) = (v(-0.4, 0.7), v(0.4, 0.7));
    let g = [v(-0.9, y_g), v(-0.32, y_g), v(0.32, y_g), v(0.9, y_g)];
    let cul = v(0.0, -0.88);
    let all = polygon(p, &[g[0], a, b, g[3], cul]) - 0.025;
    c.fill(all, MID);
    let facets: [(&[Vec2], f32); 7] = [
        (&[g[0], a, g[1]], 0.86),
        (&[a, g[2], g[1]], HI),
        (&[a, b, g[2]], 0.94),
        (&[b, g[3], g[2]], 0.7),
        (&[g[0], g[1], cul], 0.74),
        (&[g[1], g[2], cul], 0.58),
        (&[g[2], g[3], cul], DEEP),
    ];
    for (pts, t) in facets {
        c.tint(polygon(p, pts) + 0.012, t);
    }
    c.fill(sparkle(p, v(-0.52, 0.6), 0.22), HI);
}

fn atom(p: Vec2, c: &mut Cv) {
    let ab = v(0.87, 0.31);
    for k in 0..3 {
        let ang = k as f32 * PI / 3.0 + PI / 2.0;
        let q = rot(p, ang);
        c.fill(
            ellipse(q, ab).abs() - 0.052,
            if k == 1 { MID } else { BASE },
        );
    }
    // Electrons with a dark gap so they pop off the orbit.
    for k in 0..3 {
        let ang = k as f32 * PI / 3.0 + PI / 2.0;
        let t: f32 = [0.3, 2.4, 4.3][k];
        let e = rot(v(t.cos() * ab.x, t.sin() * ab.y), -ang);
        c.cut(circle(p, e, 0.15));
        c.fill(circle(p, e, 0.1), sphere_tone(p, e, 0.1));
    }
    c.cut(circle(p, Vec2::ZERO, 0.26));
    c.fill(
        circle(p, Vec2::ZERO, 0.19),
        sphere_tone(p, Vec2::ZERO, 0.19),
    );
}

fn gear(p: Vec2, c: &mut Cv) {
    let n = 8.0;
    let sector = TAU / n;
    let a = p.y.atan2(p.x) - PI / 2.0;
    let k = (a / sector).round();
    let q = rot(p, k * sector);
    let tooth = polygon(
        q,
        &[v(-0.15, 0.5), v(0.15, 0.5), v(0.095, 0.86), v(-0.095, 0.86)],
    ) - 0.035;
    let body = smin(circle(p, Vec2::ZERO, 0.64), tooth, 0.06);
    c.fill(body, BASE);
    // Recessed web with lightening holes, raised hub.
    c.tint(circle(p, Vec2::ZERO, 0.46), MID);
    c.ink(ring(p, Vec2::ZERO, 0.47, 0.022));
    for i in 0..6 {
        let h = polar(0.33, i as f32 * TAU / 6.0 + 0.26);
        c.cut(circle(p, h, 0.065));
    }
    c.fill(circle(p, Vec2::ZERO, 0.22), HI);
    c.cut(circle(p, Vec2::ZERO, 0.1));
    c.ink(rect(p, v(0.0, 0.12), v(0.035, 0.035)).max(circle(p, Vec2::ZERO, 0.2)));
}

fn pick(p: Vec2, c: &mut Cv) {
    // Frame: u along the handle (towards the head), w across it.
    let u = (p.x + p.y) * FRAC_1_SQRT_2;
    let w = (p.y - p.x) * FRAC_1_SQRT_2;
    let q = v(w, u);
    // Handle with a grip wrap.
    let handle = taper(q, v(0.0, -0.98), v(0.0, 0.42), 0.1, 0.085);
    c.fill(handle, MID);
    c.tint(handle.max(q.y + 0.44), LOW);
    for y in [-0.88, -0.74, -0.6] {
        c.ink(rect(q, v(0.0, y), v(0.2, 0.018)).max(handle));
    }
    // Curved head: a crescent built from two quadratic curves meeting at
    // sharp tips, thick over the socket.
    let quad = |a: Vec2, k: Vec2, b: Vec2, t: f32| {
        a * (1.0 - t).powi(2) + k * 2.0 * t * (1.0 - t) + b * t * t
    };
    let (tl, tr) = (v(-0.9, 0.0), v(0.9, 0.0));
    let mut pts: Vec<Vec2> = (0..=16)
        .map(|i| quad(tl, v(0.0, 1.02), tr, i as f32 / 16.0))
        .collect();
    pts.extend(
        (1..16)
            .rev()
            .map(|i| quad(tl, v(0.0, 0.52), tr, i as f32 / 16.0)),
    );
    let head = polygon(q, &pts) - 0.02;
    c.cut(head - 0.05);
    c.fill(head, HI);
    // Lower (inner) bevel and darker tips for a forged, two-tone look.
    let mut inner: Vec<Vec2> = (0..=16)
        .map(|i| quad(tl, v(0.0, 0.8), tr, i as f32 / 16.0))
        .collect();
    inner.extend(
        (1..16)
            .rev()
            .map(|i| quad(tl, v(0.0, 0.52), tr, i as f32 / 16.0)),
    );
    c.tint(polygon(q, &inner).max(head), LOW + 0.04);
    c.tint(head.max(0.6 - q.x.abs()), MID);
    c.tint(
        polygon(q, &inner).max(head).max(0.6 - q.x.abs()),
        DEEP + 0.04,
    );
    // Socket collar.
    c.fill(rrect(q, v(0.0, 0.4), v(0.15, 0.15), 0.04), LOW);
    c.ink(rect(q, v(0.0, 0.4), v(0.16, 0.018)).max(rrect(q, v(0.0, 0.4), v(0.15, 0.15), 0.04)));
}

fn flame(p: Vec2, c: &mut Cv) {
    let outer = smin(
        taper(p, v(0.02, -0.42), v(0.12, 0.95), 0.44, 0.02),
        taper(p, v(-0.28, -0.3), v(-0.52, 0.3), 0.2, 0.02),
        0.12,
    );
    let outer = smin(
        outer,
        taper(p, v(0.3, -0.25), v(0.5, 0.12), 0.16, 0.02),
        0.1,
    );
    // Outer tongues darken towards their tips; layered hotter cores inside.
    c.fill(outer, (LOW + 0.12 - 0.1 * p.y).clamp(LOW - 0.05, MID));
    let mid = smin(
        taper(p, v(0.02, -0.5), v(0.04, 0.52), 0.31, 0.02),
        taper(p, v(-0.14, -0.45), v(-0.3, 0.0), 0.13, 0.02),
        0.08,
    );
    c.ink((mid.abs() - 0.012).max(outer + 0.03));
    c.fill(mid, (0.88 - 0.08 * p.y).min(HI));
    let core = taper(p, v(0.02, -0.6), v(0.0, 0.1), 0.17, 0.02);
    c.fill(core, HI);
    c.fill(taper(p, v(-0.04, -0.56), v(-0.04, -0.2), 0.05, 0.01), 1.0);
    // Embers.
    c.fill(circle(p, v(-0.5, 0.58), 0.045), MID);
    c.fill(circle(p, v(0.5, 0.42), 0.035), MID);
}

fn factory(p: Vec2, c: &mut Cv) {
    let floor = -0.84;
    // Chimney with smoke.
    c.fill(rrect(p, v(0.62, -0.12), v(0.13, 0.62), 0.03), LOW);
    c.ink(rect(p, v(0.62, 0.32), v(0.13, 0.025)));
    c.fill(rrect(p, v(0.62, 0.52), v(0.17, 0.05), 0.02), MID);
    c.paint(circle(p, v(0.7, 0.75), 0.11), MID, 0.85);
    c.paint(circle(p, v(0.47, 0.86), 0.08), MID, 0.7);
    // Sawtooth roof hall.
    let hall = rect(
        p,
        v(-0.2, (floor - 0.12) * 0.5),
        v(0.66, (-0.12 - floor) * 0.5),
    );
    c.fill(hall, BASE);
    for k in 0..3 {
        let x0 = -0.86 + k as f32 * 0.44;
        let tooth = polygon(p, &[v(x0, -0.13), v(x0 + 0.44, -0.13), v(x0, 0.34)]);
        c.fill(tooth, HI);
        c.ink(seg(p, v(x0 + 0.04, -0.1), v(x0 + 0.04, 0.2), 0.03));
    }
    c.ink(rect(p, v(-0.2, -0.17), v(0.66, 0.022)));
    // Windows and door.
    for k in 0..3 {
        let x = -0.68 + k as f32 * 0.32;
        c.ink(rrect(p, v(x, -0.42), v(0.1, 0.1), 0.02));
    }
    c.ink(rrect(p, v(0.3, -0.62), v(0.12, 0.22), 0.02));
    c.fill(rrect(p, v(-0.05, floor - 0.04), v(0.94, 0.05), 0.02), LOW);
}

fn bolt(p: Vec2, c: &mut Cv) {
    let pts = [
        v(0.24, 0.95),
        v(-0.5, -0.08),
        v(-0.05, -0.08),
        v(-0.26, -0.95),
        v(0.52, 0.12),
        v(0.06, 0.12),
    ];
    let d = polygon(p, &pts) - 0.03;
    // Split along the bolt's spine: lit left half, shaded right half.
    let a = v(0.24, 0.95);
    let b = v(-0.26, -0.95);
    let side = (b - a).perp_dot(p - a);
    c.fill(d, if side < 0.0 { HI } else { MID });
}

fn house(p: Vec2, c: &mut Cv) {
    // Chimney.
    c.fill(rect(p, v(0.42, 0.5), v(0.09, 0.22)), LOW);
    // Walls.
    let walls = polygon(
        p,
        &[
            v(-0.62, -0.86),
            v(0.62, -0.86),
            v(0.62, 0.1),
            v(0.0, 0.64),
            v(-0.62, 0.1),
        ],
    );
    c.fill(walls, BASE);
    c.tint(walls.max(-p.x), MID);
    // Roof with overhang, separated by a gap.
    let roof = path(p, &[v(-0.84, -0.02), v(0.0, 0.76), v(0.84, -0.02)], 0.11);
    c.cut(roof - 0.06);
    c.fill(roof, HI);
    // Door and window.
    c.ink(rrect(p, v(-0.24, -0.6), v(0.16, 0.27), 0.03));
    c.fill(circle(p, v(-0.14, -0.6), 0.03), MID);
    c.ink(rrect(p, v(0.28, -0.3), v(0.17, 0.15), 0.02));
    c.fill(
        rect(p, v(0.28, -0.3), v(0.02, 0.15)).min(rect(p, v(0.28, -0.3), v(0.17, 0.02))),
        MID,
    );
}

fn shield(p: Vec2, c: &mut Cv) {
    // Curved lower edge (quadratic bezier to the point), mirrored.
    let (a, ctrl, b) = (v(0.72, 0.12), v(0.7, -0.55), v(0.0, -0.95));
    let curve: Vec<Vec2> = (1..=10)
        .map(|i| {
            let t = i as f32 / 10.0;
            a * (1.0 - t).powi(2) + ctrl * 2.0 * t * (1.0 - t) + b * t * t
        })
        .collect();
    let mut pts = vec![v(-0.72, 0.8), v(0.0, 0.92), v(0.72, 0.8), a];
    pts.extend(&curve);
    pts.extend(curve.iter().rev().skip(1).map(|q| v(-q.x, q.y)));
    pts.push(v(-0.72, 0.12));
    let d = polygon(p, &pts) - 0.03;
    c.fill(d, if p.x < 0.0 { HI } else { MID });
    c.ink((d + 0.15).abs() - 0.025);
    // Emblem: upward chevron.
    let em = path(p, &[v(-0.3, -0.22), v(0.0, 0.12), v(0.3, -0.22)], 0.075);
    c.tint(em, if p.x < 0.0 { LOW } else { DEEP });
}

fn person(p: Vec2, c: &mut Cv) {
    let body = circle(p, v(0.0, -0.98), 0.8).max(-0.86 - p.y);
    let body = smin(body, rrect(p, v(0.0, -0.62), v(0.62, 0.26), 0.2), 0.05);
    c.fill(body, MID);
    c.cut(circle(p, v(0.0, 0.36), 0.42));
    c.fill(
        circle(p, v(0.0, 0.36), 0.32),
        sphere_tone(p, v(0.0, 0.36), 0.32).max(0.75),
    );
}

fn warning(p: Vec2, c: &mut Cv) {
    let tri = polygon(p, &[v(0.0, 0.74), v(-0.8, -0.68), v(0.8, -0.68)]) - 0.11;
    c.fill(tri, HI);
    c.tint(tri.max(-p.x), BASE);
    c.ink(taper(p, v(0.0, -0.18), v(0.0, 0.38), 0.065, 0.095));
    c.ink(circle(p, v(0.0, -0.46), 0.1));
}

fn belt(p: Vec2, c: &mut Cv) {
    let cy = -0.36;
    let loop_ = rrect(p, v(0.0, cy), v(0.92, 0.3), 0.3);
    // Rollers inside the loop.
    c.fill(loop_ + 0.1, DEEP);
    for x in [-0.62, 0.0, 0.62] {
        c.fill(circle(p, v(x, cy), 0.15), MID);
        c.ink(circle(p, v(x, cy), 0.045));
    }
    c.fill(loop_.abs() - 0.065, BASE);
    c.tint((loop_.abs() - 0.065).max(cy - p.y), HI);
    // Belt treads.
    for k in 0..7 {
        let x = -0.6 + k as f32 * 0.2;
        c.ink(rect(p, v(x, cy + 0.3), v(0.018, 0.05)));
    }
    // Cargo riding on top.
    c.fill(rrect(p, v(-0.32, 0.25), v(0.24, 0.22), 0.04), HI);
    c.ink(rect(p, v(-0.32, 0.27), v(0.24, 0.03)));
    c.fill(rrect(p, v(0.34, 0.2), v(0.18, 0.17), 0.04), MID);
    c.ink(rect(p, v(0.34, 0.2), v(0.03, 0.17)));
}

fn arm(p: Vec2, c: &mut Cv) {
    let j0 = v(-0.44, -0.56);
    let j1 = v(-0.16, 0.38);
    let j2 = v(0.5, 0.2);
    // Base turret.
    c.fill(rrect(p, v(-0.44, -0.84), v(0.42, 0.09), 0.04), LOW);
    c.fill(circle(p, v(-0.44, -0.7), 0.22).max(-0.72 - p.y), MID);
    // Links.
    c.fill(taper(p, j0, j1, 0.15, 0.12), BASE);
    c.fill(taper(p, j1, j2, 0.12, 0.09), HI);
    // Gripper.
    let claw = path(p, &[j2, v(0.72, 0.38), v(0.86, 0.26)], 0.065).min(path(
        p,
        &[j2, v(0.7, 0.0), v(0.86, 0.08)],
        0.065,
    ));
    c.fill(claw, MID);
    // Joints.
    for (j, r) in [(j0, 0.16), (j1, 0.15), (j2, 0.11)] {
        c.cut(ring(p, j, r + 0.025, 0.025));
        c.fill(circle(p, j, r), LOW);
        c.ink(circle(p, j, r * 0.35));
    }
}

fn trash(p: Vec2, c: &mut Cv) {
    let can = polygon(
        p,
        &[
            v(-0.56, 0.38),
            v(0.56, 0.38),
            v(0.45, -0.84),
            v(-0.45, -0.84),
        ],
    ) - 0.04;
    c.fill(can, BASE);
    c.tint(can.max(p.x - 0.0), MID);
    for x in [-0.24f32, 0.0, 0.24] {
        c.ink(seg(p, v(x, 0.2), v(x * 0.8, -0.66), 0.035));
    }
    c.fill(rrect(p, v(0.0, 0.54), v(0.74, 0.08), 0.04), HI);
    let handle = (rrect(p, v(0.0, 0.66), v(0.22, 0.12), 0.06).abs() - 0.045).max(0.6 - p.y);
    c.fill(handle, MID);
}

fn clock(p: Vec2, c: &mut Cv) {
    c.fill(circle(p, Vec2::ZERO, 0.88), BASE);
    c.tint(circle(p, Vec2::ZERO, 0.74), DEEP - 0.08);
    c.tint(ring(p, Vec2::ZERO, 0.85, 0.035), HI);
    for k in 0..12 {
        let a = k as f32 * TAU / 12.0;
        let (r0, w) = if k % 3 == 0 {
            (0.5, 0.045)
        } else {
            (0.58, 0.028)
        };
        c.tint(
            seg(p, polar(r0, a), polar(0.64, a), w),
            if k % 3 == 0 { HI } else { MID },
        );
    }
    c.tint(seg(p, Vec2::ZERO, v(0.0, 0.54), 0.055), HI);
    c.tint(seg(p, Vec2::ZERO, v(0.34, -0.16), 0.065), HI);
    c.tint(circle(p, Vec2::ZERO, 0.1), HI);
    c.ink(circle(p, Vec2::ZERO, 0.035));
}

fn infinity(p: Vec2, c: &mut Cv) {
    let a = 0.84;
    let r2 = p.length_squared();
    let f = r2 * r2 - a * a * (p.x * p.x - p.y * p.y);
    let g = v(
        4.0 * p.x * r2 - 2.0 * a * a * p.x,
        4.0 * p.y * r2 + 2.0 * a * a * p.y,
    );
    let d = f / g.length().max(1e-3);
    // Stroke swells on the lobes, thins at the crossing.
    let w = 0.07 + 0.045 * (p.x.abs() / 0.6).min(1.0);
    let lem = d.abs() - w;
    c.fill(lem, if p.x < 0.0 { HI } else { MID });
    // Over/under crossing.
    let over = |e: f32| seg(p, v(-0.26, -0.26), v(0.26, 0.26), e).max(p.length() - 0.3);
    c.cut(over(w + 0.06).max(-lem - 0.2));
    c.fill(lem.max(over(w + 0.03)), BASE);
}

fn target(p: Vec2, c: &mut Cv) {
    c.fill(ring(p, Vec2::ZERO, 0.68, 0.075), BASE);
    c.fill(ring(p, Vec2::ZERO, 0.38, 0.07), MID);
    c.fill(circle(p, Vec2::ZERO, 0.13), HI);
    for k in 0..4 {
        let a = k as f32 * PI / 2.0;
        c.cut(seg(p, polar(0.52, a), polar(0.92, a), 0.1));
        c.fill(seg(p, polar(0.5, a), polar(0.94, a), 0.05), HI);
    }
}

fn cursor(p: Vec2, c: &mut Cv) {
    let pts = [
        v(-0.46, 0.88),
        v(-0.46, -0.44),
        v(-0.18, -0.18),
        v(0.02, -0.7),
        v(0.22, -0.62),
        v(0.02, -0.1),
        v(0.4, -0.1),
    ];
    let d = polygon(p, &pts) - 0.04;
    let a = v(-0.46, 0.88);
    let b = v(0.12, -0.66);
    c.fill(
        d,
        if (b - a).perp_dot(p - a) > 0.0 {
            HI
        } else {
            MID
        },
    );
}

fn rotate(p: Vec2, c: &mut Cv) {
    let gap = 0.95; // half-aperture of the opening, radians
    let centre_dir = PI / 4.0 + PI;
    let arcd = arc(p, Vec2::ZERO, 0.6, 0.1, centre_dir, PI - gap);
    c.fill(arcd, BASE);
    // Arrowhead at the clockwise end (top).
    let end = centre_dir - (PI - gap);
    let e = polar(0.6, end);
    let t = v(end.sin(), -end.cos());
    let n = polar(1.0, end);
    let pts = [
        e + t * 0.3,
        e + n * 0.27 - t * 0.04,
        e - n * 0.27 - t * 0.04,
    ];
    c.fill(polygon(p, &pts) - 0.03, HI);
}

fn disk(p: Vec2, c: &mut Cv) {
    let body = polygon(
        p,
        &[
            v(-0.76, 0.76),
            v(0.5, 0.76),
            v(0.76, 0.5),
            v(0.76, -0.76),
            v(-0.76, -0.76),
        ],
    ) - 0.06;
    c.fill(body, MID);
    // Metal shutter with slot.
    c.tint(rect(p, v(-0.1, 0.56), v(0.42, 0.3)).max(body), HI);
    c.ink(rrect(p, v(0.12, 0.54), v(0.09, 0.18), 0.02));
    // Label.
    c.tint(rrect(p, v(0.0, -0.4), v(0.56, 0.34), 0.05), BASE + 0.05);
    for y in [-0.26, -0.44, -0.62] {
        c.ink(rect(p, v(0.0, y), v(0.4, 0.022)));
    }
}

fn eye(p: Vec2, c: &mut Cv) {
    let lens = circle(p, v(0.0, -0.62), 1.05).max(circle(p, v(0.0, 0.62), 1.05));
    c.fill(lens, BASE);
    c.cut(circle(p, Vec2::ZERO, 0.46).max(lens + 0.05));
    c.fill(
        circle(p, Vec2::ZERO, 0.4).max(lens),
        sphere_tone(p, Vec2::ZERO, 0.4) * 0.8,
    );
    c.ink(circle(p, Vec2::ZERO, 0.17));
    c.fill(circle(p, v(-0.12, 0.13), 0.065), HI);
}

fn book(p: Vec2, c: &mut Cv) {
    let page = |p: Vec2, sgn: f32| {
        let x = p.x * sgn;
        let t = ((x - 0.05) / 0.82).clamp(0.0, 1.0);
        let top = 0.5 + 0.2 * t.sqrt();
        let bot = -0.7 + 0.12 * t.sqrt();
        (0.05 - x).max(x - 0.87).max(p.y - top).max(bot - p.y)
    };
    let left = page(p, -1.0);
    let right = page(p, 1.0);
    // Cover peeking out below.
    let cover = (page(p + v(0.0, 0.1), -1.0) - 0.05).min(page(p + v(0.0, 0.1), 1.0) - 0.05);
    c.fill(cover.max(-0.86 - p.y), LOW);
    c.fill(left, HI);
    c.fill(right, MID + 0.06);
    // Text lines following the page curve.
    for k in 0..4 {
        let o = k as f32 * 0.22;
        for sgn in [-1.0, 1.0] {
            let x = p.x * sgn;
            let t = ((x - 0.05) / 0.82).clamp(0.0, 1.0);
            let y = 0.34 + 0.2 * t.sqrt() - o;
            let line = ((p.y - y).abs() - 0.025).max((x - 0.5).abs() - 0.3);
            c.ink(line.max(if sgn < 0.0 { left } else { right }));
        }
    }
    c.ink(rect(p, v(0.0, -0.08), v(0.018, 0.6)));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Dump all icons as one raw RGBA sheet for inspection:
    /// `BTS_ICON_DUMP=/path/sheet.rgba cargo test -p game icon_sheet`.
    #[test]
    fn icon_sheet() {
        let size = 128;
        let chains = render_all(size);
        if let Ok(path) = std::env::var("BTS_ICON_DUMP") {
            let cols = 8;
            let rows = Icon::ALL.len().div_ceil(cols);
            let (w, h) = (cols * size, rows * size);
            let mut out = vec![0u8; w * h * 4];
            for (i, chain) in chains.iter().enumerate() {
                let (cx, cy) = ((i % cols) * size, (i / cols) * size);
                for y in 0..size {
                    for x in 0..size {
                        let px = encode(chain[0][y * size + x]);
                        let o = ((cy + y) * w + cx + x) * 4;
                        out[o..o + 4].copy_from_slice(&px);
                    }
                }
            }
            std::fs::write(&path, out).unwrap();
            println!("{w}x{h}");
        }
        let mut bad = Vec::new();
        for (icon, chain) in Icon::ALL.iter().zip(&chains) {
            let base = &chain[0];
            let covered = base.iter().filter(|p| p[1] > 0.5).count();
            if covered <= size * size / 40 {
                bad.push(format!("{icon:?} is nearly empty"));
            }
            // Nothing may touch the border (shadow/outline included).
            let edge = (0..size)
                .flat_map(|i| [(i, 0), (i, size - 1), (0, i), (size - 1, i)])
                .map(|(x, y)| base[y * size + x][1])
                .fold(0.0f32, f32::max);
            if edge >= 0.05 {
                bad.push(format!("{icon:?} touches the edge ({edge:.2})"));
            }
        }
        assert!(bad.is_empty(), "{bad:?}");
    }
}
