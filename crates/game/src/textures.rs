//! Procedural surface textures and the shared PBR materials built from them.
//!
//! Every procedural mesh carries box-projected UVs and tangents (`mesh_util::box_uvs`,
//! `UV_PER_UNIT` repeats per world unit), so the tiling texture sets generated here apply to
//! all models and the terrain:
//!
//! * **albedo** — near-white greyscale detail (mottling, grime, rain streaks, aggregate,
//!   pebbles). The vertex colour carries the hue; each material's base colour is scaled so
//!   its detail averages to 1.0 and only adds variation, never an overall darkening.
//! * **normal** — tangent space, derived from a height field. `+Y` points along `+V`, which
//!   is the bitangent `box_uvs` emits, so relief lights correctly on every face.
//! * **ORM** — R = ambient occlusion, G = perceptual roughness, B = metallic (glTF layout),
//!   bound as both the metallic-roughness and the occlusion texture.
//!
//! Everything is periodic (wrapping noise lattices, wrap-around stamps), carries a full
//! CPU-built mip chain (normals are averaged then renormalised and the normal variance each
//! mip loses is folded into that mip's roughness, Toksvig style, so distant relief turns
//! into broader highlights instead of sparkling), and is sampled with repeat addressing,
//! trilinear and 16x anisotropic filtering. Generation is spread over all cores and keeps
//! per-pixel work to a handful of lattice lookups so it stays fast in debug builds.

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::math::Affine2;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use std::f32::consts::{PI, TAU};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// The shared materials every scene draws with.
#[derive(Clone)]
pub struct Materials {
    /// Painted panels (most building bodies, items).
    pub paint: Handle<StandardMaterial>,
    /// Bare / brushed metal: pipes, frames, bolts, rails.
    pub metal: Handle<StandardMaterial>,
    /// Concrete, rubber, soil, matte plastic.
    pub rough: Handle<StandardMaterial>,
    /// Windows and canopies.
    pub glass: Handle<StandardMaterial>,
    /// Unlit emissive (lights, screens, plasma). Vertex colours above 1.0 bloom.
    pub glow: Handle<StandardMaterial>,
    /// Planet ground (vertex-coloured per tile).
    pub terrain: Handle<StandardMaterial>,
    /// Molten / glowing ground.
    pub lava: Handle<StandardMaterial>,
}

/// Texture size of the paint, metal, rough and lava sets.
const MAP: usize = 1024;
/// The terrain set is larger: one repeat spans 8×8 tiles of open ground.
const TERRAIN_MAP: usize = 2048;

/// Texture repeats per `box_uvs` UV unit (`UV_PER_UNIT` = 0.5 per world unit, so 0.5 here
/// means one repeat every 4 world units).
const PAINT_TILING: f32 = 0.5;
const METAL_TILING: f32 = 1.0;
const ROUGH_TILING: f32 = 0.5;
const TERRAIN_TILING: f32 = 0.25;
const LAVA_TILING: f32 = 0.5;

pub fn create(images: &mut Assets<Image>, mats: &mut Assets<StandardMaterial>) -> Materials {
    let start = Instant::now();
    let [paint, metal, rough, terrain, lava] = std::thread::scope(|s| {
        Kind::ALL
            .map(|k| s.spawn(move || build(k, k.size())))
            .map(|job| job.join().expect("texture generation panicked"))
    });
    info!(
        "generated procedural surface textures in {} ms",
        start.elapsed().as_millis()
    );

    let paint = pbr(images, paint, PAINT_TILING);
    let metal = pbr(images, metal, METAL_TILING);
    let rough = pbr(images, rough, ROUGH_TILING);
    let terrain = pbr(images, terrain, TERRAIN_TILING);
    let lava = StandardMaterial {
        base_color: grey(3.2),
        base_color_texture: Some(images.add(image(
            lava.n,
            lava.albedo,
            TextureFormat::Rgba8UnormSrgb,
        ))),
        uv_transform: Affine2::from_scale(Vec2::splat(LAVA_TILING)),
        unlit: true,
        ..default()
    };

    Materials {
        paint: mats.add(paint),
        metal: mats.add(metal),
        rough: mats.add(rough),
        terrain: mats.add(terrain),
        lava: mats.add(lava),
        // Premultiplied: the vertex alpha sets how much of the background shows through,
        // while reflections and the sun's highlight stay at full strength like real glass.
        glass: mats.add(StandardMaterial {
            base_color: grey(0.32),
            perceptual_roughness: 0.06,
            reflectance: 0.6,
            alpha_mode: AlphaMode::Premultiplied,
            ..default()
        }),
        glow: mats.add(StandardMaterial {
            base_color: Color::LinearRgba(LinearRgba::rgb(2.5, 2.5, 2.5)),
            unlit: true,
            ..default()
        }),
    }
}

/// A lit material from a full texture set: detail albedo, normal and ORM.
fn pbr(images: &mut Assets<Image>, m: Maps, tiling: f32) -> StandardMaterial {
    let orm = images.add(image(m.n, m.orm, TextureFormat::Rgba8Unorm));
    StandardMaterial {
        base_color: grey(1.0 / m.mean),
        base_color_texture: Some(images.add(image(m.n, m.albedo, TextureFormat::Rgba8UnormSrgb))),
        normal_map_texture: Some(images.add(image(m.n, m.normal, TextureFormat::Rgba8Unorm))),
        metallic_roughness_texture: Some(orm.clone()),
        occlusion_texture: Some(orm),
        // The ORM texture holds absolute values; the factors just pass them through.
        perceptual_roughness: 1.0,
        metallic: 1.0,
        uv_transform: Affine2::from_scale(Vec2::splat(tiling)),
        ..default()
    }
}

fn grey(k: f32) -> Color {
    Color::LinearRgba(LinearRgba::rgb(k, k, k))
}

/// A GPU image holding a complete mip chain, sampled with repeat + trilinear + 16x aniso.
fn image(n: usize, data: Vec<u8>, format: TextureFormat) -> Image {
    let mut img = Image::new_uninit(
        Extent3d {
            width: n as u32,
            height: n as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        format,
        RenderAssetUsages::RENDER_WORLD,
    );
    debug_assert_eq!(data.len(), chain_texels(n) * 4);
    img.data = Some(data);
    img.texture_descriptor.mip_level_count = mip_levels(n);
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        address_mode_w: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 16,
        ..default()
    });
    img
}

fn mip_levels(n: usize) -> u32 {
    n.max(1).ilog2() + 1
}

/// Texels in a full square mip chain starting at `n`×`n`.
fn chain_texels(n: usize) -> usize {
    (0..mip_levels(n)).map(|l| (n >> l).max(1).pow(2)).sum()
}

// ------------------------------------------------------------------------ texture sets

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Paint,
    Metal,
    Rough,
    Terrain,
    Lava,
}

impl Kind {
    const ALL: [Kind; 5] = [
        Kind::Paint,
        Kind::Metal,
        Kind::Rough,
        Kind::Terrain,
        Kind::Lava,
    ];

    fn size(self) -> usize {
        if self == Kind::Terrain {
            TERRAIN_MAP
        } else {
            MAP
        }
    }

    /// Height → slope scale for the normal map.
    fn normal_strength(self) -> f32 {
        match self {
            Kind::Paint => 1.0,
            Kind::Metal => 1.0,
            Kind::Rough => 1.0,
            Kind::Terrain => 1.0,
            Kind::Lava => 0.0,
        }
    }

    /// How much of the normal variance a mip loses is added to its roughness.
    fn toksvig(self) -> f32 {
        match self {
            Kind::Metal => 0.6,
            _ => 1.0,
        }
    }
}

/// One generated pixel before encoding.
#[derive(Clone, Copy, Default)]
struct Px {
    /// Linear albedo multiplier.
    alb: f32,
    /// Height in texels (at 1024²) for the normal map.
    h: f32,
    /// Perceptual roughness.
    r: f32,
    /// Metallic.
    m: f32,
    /// Ambient occlusion.
    ao: f32,
}

/// One texel of a mip level: everything averaged in the space it filters correctly in.
#[derive(Clone, Copy, Default)]
struct Texel {
    alb: f32,
    /// GGX alpha² (perceptual roughness⁴).
    a2: f32,
    m: f32,
    ao: f32,
    /// Unnormalised average of the unit normals below this texel.
    n: [f32; 3],
}

/// Encoded maps of one set: each `Vec` is a whole mip chain, level 0 first.
struct Maps {
    n: usize,
    albedo: Vec<u8>,
    normal: Vec<u8>,
    orm: Vec<u8>,
    /// Average albedo (the 1×1 mip), used to normalise the base colour.
    mean: f32,
}

fn build(kind: Kind, n: usize) -> Maps {
    let px = match kind {
        Kind::Paint => paint(n),
        Kind::Metal => metal(n),
        Kind::Rough => rough(n),
        Kind::Terrain => terrain(n),
        Kind::Lava => lava(n),
    };
    encode(&px, n, kind)
}

/// Fills a pixel grid in parallel; `f(u, v, index)` gets texel-centre UVs in [0, 1).
fn fill(n: usize, f: impl Fn(f32, f32, usize) -> Px + Sync) -> Vec<Px> {
    let mut px = vec![Px::default(); n * n];
    let inv = 1.0 / n as f32;
    par_rows(&mut px, n, |y, row| {
        let v = (y as f32 + 0.5) * inv;
        for (x, p) in row.iter_mut().enumerate() {
            *p = f((x as f32 + 0.5) * inv, v, y * n + x);
        }
    });
    px
}

/// Painted steel: orange-peel relief, soft dents, fine scratches, dusty patches, rain
/// streaks and chipped wear spots that read shinier.
fn paint(n: usize) -> Vec<Px> {
    let mott = Fbm::new(4, 4, 0.6, 11);
    let grime = Fbm::new(3, 5, 0.6, 12);
    let streak_a = Grad::new(96, 4, 13);
    let streak_b = Grad::new(190, 7, 14);
    let streak_c = Grad::new(40, 2, 15);
    let wear = Fbm::new(4, 3, 0.55, 16);
    let chip = Fbm::new(40, 3, 0.5, 17);
    let peel_a = Grad::new(150, 150, 18);
    let peel_b = Grad::new(300, 300, 19);
    let mut dents = vec![0.0; n * n];
    stamp_dents(&mut dents, n, 26, (0.012, 0.045), 0.08, 20);
    let mut scratches = vec![0.0; n * n];
    stamp_scratches(&mut scratches, n, 130, (0.015, 0.11), 0.0, 21);
    fill(n, |u, v, i| {
        let m = mott.at(u, v);
        let g = grime.at(u, v);
        let patch = smooth(0.02, 0.38, g);
        let s = streak_a.at(u, v) + 0.6 * streak_b.at(u, v) + 0.5 * streak_c.at(u, v);
        let streak = smooth(0.45, 1.0, s) * smooth(-0.25, 0.15, g + 0.3 * m);
        let worn = smooth(0.05, 0.4, wear.at(u, v));
        let c = chip.at(u, v);
        let chipped = smooth(0.5 - 0.3 * worn, 0.54 - 0.3 * worn, c);
        let peel = 0.6 * peel_a.at(u, v) + 0.4 * peel_b.at(u, v);
        let sc = scratches[i];
        Px {
            alb: 0.95 + 0.035 * m - 0.1 * patch - 0.07 * streak - 0.08 * chipped
                + 0.04 * sc
                + 0.012 * peel,
            h: 0.22 * peel + dents[i] - 0.9 * chipped - 0.7 * sc,
            r: 0.5 + 0.05 * m + 0.17 * patch + 0.07 * streak - 0.16 * chipped - 0.12 * sc,
            m: 0.12 + 0.3 * chipped + 0.12 * sc,
            ao: 1.0 - 0.1 * chipped - 0.25 * sc,
        }
    })
}

/// Brushed steel: fine grain along U, broad polishing bands, long scratches with the grain,
/// oily smudges and a few oxidised spots that are darker, rougher and less metallic.
fn metal(n: usize) -> Vec<Px> {
    let brush_a = Grad::new(3, 380, 31);
    let brush_b = Grad::new(6, 760, 32);
    let brush_c = Grad::new(2, 160, 33);
    let band = Grad::new(4, 48, 34);
    let smudge = Fbm::new(4, 3, 0.55, 35);
    let oxide = Fbm::new(6, 4, 0.55, 36);
    let pits = Grad::new(220, 220, 37);
    let mut scratches = vec![0.0; n * n];
    stamp_scratches(&mut scratches, n, 170, (0.02, 0.16), 0.75, 38);
    fill(n, |u, v, i| {
        let br = 0.5 * brush_a.at(u, v) + 0.3 * brush_b.at(u, v) + 0.45 * brush_c.at(u, v);
        let bd = band.at(u, v);
        let sm = smooth(0.05, 0.4, smudge.at(u, v));
        let ox = smooth(0.22, 0.5, oxide.at(u, v));
        let sc = scratches[i];
        Px {
            alb: 0.93 + 0.035 * bd + 0.02 * br - 0.2 * ox - 0.04 * sm + 0.03 * sc,
            h: 0.25 * br - 0.8 * sc + 0.35 * ox * pits.at(u, v),
            r: 0.27 + 0.05 * bd + 0.12 * sm + 0.3 * ox + 0.05 * sc,
            m: 0.95 - 0.35 * ox,
            ao: 1.0 - 0.2 * sc,
        }
    })
}

/// Cast concrete / rubber: exposed aggregate, sand specks, air pores, water stains and a
/// few hairline cracks.
fn rough(n: usize) -> Vec<Px> {
    let mott = Fbm::new(4, 4, 0.6, 51);
    let stain = Fbm::new(2, 4, 0.55, 52);
    let grain_a = Grad::new(380, 380, 53);
    let grain_b = Grad::new(190, 190, 54);
    let aggregate = Scatter::new(52, 55, (0.18, 0.42), 1.6, |_, _| 0.6);
    let sand = Scatter::new(140, 56, (0.2, 0.4), 1.3, |_, _| 0.55);
    let pores = Scatter::new(100, 57, (0.12, 0.3), 1.2, |_, _| 0.22);
    let cracks = Cells::new(5, 58);
    let warp_u = Fbm::new(10, 3, 0.5, 59);
    let warp_v = Fbm::new(10, 3, 0.5, 60);
    let crack_mask = Fbm::new(2, 3, 0.5, 61);
    fill(n, |u, v, _| {
        let m = mott.at(u, v);
        let st = smooth(0.1, 0.45, stain.at(u, v));
        let grain = 0.6 * grain_a.at(u, v) + 0.4 * grain_b.at(u, v);
        let a = aggregate.at(u, v);
        let s = sand.at(u, v);
        let p = pores.at(u, v);
        let hit = cracks.at(u + 0.012 * warp_u.at(u, v), v + 0.012 * warp_v.at(u, v));
        let cr =
            (1.0 - smooth(0.0, 0.03, hit.f2 - hit.f1)) * smooth(0.02, 0.25, crack_mask.at(u, v));
        Px {
            alb: 0.9 + 0.05 * m - 0.12 * st + 0.1 * a.tone * a.cov + 0.08 * s.tone * s.cov
                - 0.3 * p.cov
                - 0.35 * cr
                + 0.03 * grain
                - 0.05 * a.ring,
            h: 1.4 * a.dome + 0.5 * s.dome - 1.6 * p.dome - 2.0 * cr + 0.25 * grain,
            r: 0.86 + 0.06 * m + 0.05 * st - 0.1 * a.cov,
            m: 0.0,
            ao: 1.0 - 0.5 * p.cov - 0.5 * cr - 0.15 * a.ring,
        }
    })
}

/// Open ground: soil grain and clumps, pebble beds, scattered stones with contact shadows
/// and patches of dried-mud cracks.
fn terrain(n: usize) -> Vec<Px> {
    let tone = Fbm::new(8, 3, 0.6, 71);
    let clump = Fbm::new(24, 3, 0.55, 72);
    let grain_a = Grad::new(640, 640, 73);
    let grain_b = Grad::new(320, 320, 74);
    let bed = Fbm::new(3, 3, 0.55, 75);
    let pebbles = Scatter::new(176, 76, (0.22, 0.42), 1.7, |u, v| {
        0.12 + 0.6 * smooth(-0.1, 0.3, bed.at(u, v))
    });
    let stones = Scatter::new(46, 77, (0.22, 0.4), 1.6, |u, v| {
        0.04 + 0.22 * smooth(0.0, 0.35, bed.at(u, v))
    });
    let cracks = Cells::new(13, 78);
    let fine_cracks = Cells::new(34, 79);
    let warp_u = Fbm::new(12, 2, 0.5, 80);
    let warp_v = Fbm::new(12, 2, 0.5, 81);
    let crack_mask = Fbm::new(3, 3, 0.55, 82);
    fill(n, |u, v, _| {
        let t = tone.at(u, v);
        let grain = 0.5 * grain_a.at(u, v) + 0.5 * grain_b.at(u, v);
        let pb = pebbles.at(u, v);
        let st = stones.at(u, v);
        let (wu, wv) = (u + 0.006 * warp_u.at(u, v), v + 0.006 * warp_v.at(u, v));
        let c1 = cracks.at(wu, wv);
        let c2 = fine_cracks.at(wu, wv);
        let mask = smooth(0.05, 0.3, crack_mask.at(u, v));
        let cr = mask
            * ((1.0 - smooth(0.0, 0.025, c1.f2 - c1.f1))
                .max(0.6 * (1.0 - smooth(0.0, 0.04, c2.f2 - c2.f1))));
        Px {
            alb: 0.86 + 0.07 * t + 0.03 * grain + 0.14 * pb.tone * pb.cov + 0.16 * st.tone * st.cov
                - 0.1 * pb.ring
                - 0.14 * st.ring
                - 0.32 * cr,
            h: 0.9 * clump.at(u, v) + 0.25 * grain + 1.6 * pb.dome + 3.5 * st.dome - 2.5 * cr,
            r: 0.93 - 0.1 * pb.cov - 0.16 * st.cov + 0.03 * t,
            m: 0.0,
            ao: 1.0 - 0.45 * cr - 0.25 * pb.ring - 0.35 * st.ring,
        }
    })
}

/// Lava (unlit): dark cooling crust plates split by glowing cracks, with hotter molten zones.
fn lava(n: usize) -> Vec<Px> {
    let plates = Cells::new(9, 91);
    let fine = Cells::new(26, 92);
    let warp_u = Fbm::new(8, 3, 0.5, 93);
    let warp_v = Fbm::new(8, 3, 0.5, 94);
    let crust = Fbm::new(16, 3, 0.55, 95);
    let hot = Fbm::new(3, 2, 0.5, 96);
    fill(n, |u, v, _| {
        let (wu, wv) = (u + 0.02 * warp_u.at(u, v), v + 0.02 * warp_v.at(u, v));
        let e1 = {
            let h = plates.at(wu, wv);
            h.f2 - h.f1
        };
        let e2 = {
            let h = fine.at(wu, wv);
            h.f2 - h.f1
        };
        let heat = smooth(-0.1, 0.35, hot.at(u, v));
        let core = 1.0 - smooth(0.0, 0.05 + 0.05 * heat, e1);
        let halo = (1.0 - smooth(0.0, 0.25, e1)).powi(2) * 0.35;
        let veins = (1.0 - smooth(0.0, 0.05, e2)) * (0.15 + 0.5 * heat);
        let k = crust.at(u, v);
        Px {
            alb: (0.06 + 0.06 * (k + 0.5) + 0.18 * heat + core + halo + 0.6 * veins)
                .clamp(0.0, 1.0),
            ..Px::default()
        }
    })
}

// ------------------------------------------------------------------------- encoding

/// Normals, mip chain and byte packing for one set.
fn encode(px: &[Px], n: usize, kind: Kind) -> Maps {
    let k = kind.normal_strength() * n as f32 / 1024.0;
    let mut level = vec![Texel::default(); n * n];
    par_rows(&mut level, n, |y, row| {
        let (up, mid, down) = ((y + n - 1) % n * n, y * n, (y + 1) % n * n);
        for (x, t) in row.iter_mut().enumerate() {
            let (left, right) = ((x + n - 1) % n, (x + 1) % n);
            let p = px[mid + x];
            // Central differences with wrap-around: x → tangent (+U), y → bitangent (+V).
            let dx = (px[mid + right].h - px[mid + left].h) * 0.5 * k;
            let dy = (px[down + x].h - px[up + x].h) * 0.5 * k;
            let inv = 1.0 / (dx * dx + dy * dy + 1.0).sqrt();
            let r = p.r.clamp(0.04, 1.0);
            *t = Texel {
                alb: p.alb.clamp(0.0, 1.0),
                a2: r * r * r * r,
                m: p.m.clamp(0.0, 1.0),
                ao: p.ao.clamp(0.0, 1.0),
                n: [-dx * inv, -dy * inv, inv],
            };
        }
    });
    let mut levels = vec![level];
    let mut size = n;
    while size > 1 {
        let next = downsample(levels.last().expect("level"), size);
        levels.push(next);
        size /= 2;
    }
    let mean = levels.last().expect("1x1 level")[0].alb.max(0.05);

    let lut = srgb_lut();
    let albedo = pack(&levels, n, |t| {
        let s = lut[(t.alb.clamp(0.0, 1.0) * 4095.0 + 0.5) as usize];
        [s, s, s, 255]
    });
    if kind == Kind::Lava {
        return Maps {
            n,
            albedo,
            normal: Vec::new(),
            orm: Vec::new(),
            mean,
        };
    }
    let normal = pack(&levels, n, |t| {
        let inv = 1.0 / length(t.n).max(1e-6);
        [
            snorm(t.n[0] * inv),
            snorm(t.n[1] * inv),
            snorm(t.n[2] * inv),
            255,
        ]
    });
    let toksvig = kind.toksvig();
    let orm = pack(&levels, n, |t| {
        let len = length(t.n).clamp(1e-3, 1.0);
        let a2 = t.a2 + toksvig * 2.0 * (1.0 - len) / len;
        [unorm(t.ao), unorm(a2.sqrt().sqrt()), unorm(t.m), 255]
    });
    Maps {
        n,
        albedo,
        normal,
        orm,
        mean,
    }
}

/// 2×2 box filter to the next mip level (`n` = size of `src`).
fn downsample(src: &[Texel], n: usize) -> Vec<Texel> {
    let h = (n / 2).max(1);
    let mut out = vec![Texel::default(); h * h];
    par_rows(&mut out, h, |y, row| {
        let (r0, r1) = (2 * y * n, (2 * y + 1) * n);
        for (x, t) in row.iter_mut().enumerate() {
            let q = [
                &src[r0 + 2 * x],
                &src[r0 + 2 * x + 1],
                &src[r1 + 2 * x],
                &src[r1 + 2 * x + 1],
            ];
            *t = Texel {
                alb: (q[0].alb + q[1].alb + q[2].alb + q[3].alb) * 0.25,
                a2: (q[0].a2 + q[1].a2 + q[2].a2 + q[3].a2) * 0.25,
                m: (q[0].m + q[1].m + q[2].m + q[3].m) * 0.25,
                ao: (q[0].ao + q[1].ao + q[2].ao + q[3].ao) * 0.25,
                n: [
                    (q[0].n[0] + q[1].n[0] + q[2].n[0] + q[3].n[0]) * 0.25,
                    (q[0].n[1] + q[1].n[1] + q[2].n[1] + q[3].n[1]) * 0.25,
                    (q[0].n[2] + q[1].n[2] + q[2].n[2] + q[3].n[2]) * 0.25,
                ],
            };
        }
    });
    out
}

/// Packs every mip level into one RGBA8 buffer, level 0 first.
fn pack(levels: &[Vec<Texel>], n: usize, f: impl Fn(&Texel) -> [u8; 4] + Sync) -> Vec<u8> {
    let mut out = vec![0u8; chain_texels(n) * 4];
    let mut rest = out.as_mut_slice();
    let mut size = n;
    for level in levels {
        let (head, tail) = std::mem::take(&mut rest).split_at_mut(level.len() * 4);
        par_rows(head, size * 4, |y, row| {
            for (x, out) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                out.copy_from_slice(&f(&level[y * size + x]));
            }
        });
        rest = tail;
        size = (size / 2).max(1);
    }
    out
}

fn length(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

fn unorm(x: f32) -> u8 {
    (x.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

fn snorm(x: f32) -> u8 {
    unorm(x * 0.5 + 0.5)
}

/// Linear [0, 1] (4096 steps) → sRGB byte.
fn srgb_lut() -> Vec<u8> {
    (0..4096)
        .map(|i| {
            let l = i as f32 / 4095.0;
            let s = if l <= 0.003_130_8 {
                l * 12.92
            } else {
                1.055 * l.powf(1.0 / 2.4) - 0.055
            };
            unorm(s)
        })
        .collect()
}

// ----------------------------------------------------------------------- parallelism

fn threads() -> usize {
    static N: OnceLock<usize> = OnceLock::new();
    *N.get_or_init(|| {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .clamp(1, 32)
    })
}

/// Runs `f(row_index, row)` over every `width`-long row of `data`, spread over all cores.
fn par_rows<T: Send>(data: &mut [T], width: usize, f: impl Fn(usize, &mut [T]) + Sync) {
    let rows = data.len() / width;
    if data.len() < 1 << 14 || threads() == 1 {
        for (y, row) in data.chunks_mut(width).enumerate() {
            f(y, row);
        }
        return;
    }
    let band = (rows / (threads() * 4)).clamp(1, 16);
    let jobs = Mutex::new(data.chunks_mut(width * band).enumerate());
    let f = &f;
    std::thread::scope(|s| {
        for _ in 0..threads().min(rows.div_ceil(band)) {
            s.spawn(|| {
                loop {
                    let Some((b, chunk)) = jobs.lock().expect("row jobs").next() else {
                        break;
                    };
                    for (i, row) in chunk.chunks_mut(width).enumerate() {
                        f(b * band + i, row);
                    }
                }
            });
        }
    });
}

// ----------------------------------------------------------------------------- noise

fn hash(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

/// Deterministic random value in [0, 1).
fn rnd(a: u32, b: u32) -> f32 {
    (hash(a ^ hash(b.wrapping_add(0x9e37_79b9))) >> 8) as f32 / 16_777_216.0
}

fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Periodic 2-D gradient (Perlin) noise with `px`×`py` lattice cells across the texture,
/// so it tiles exactly and can be stretched (e.g. brushed grain, rain streaks). Each
/// instance gets a random lattice offset so octaves never line up. Output ≈ [-1, 1].
struct Grad {
    px: usize,
    py: usize,
    fx: f32,
    fy: f32,
    ox: f32,
    oy: f32,
    g: Vec<[f32; 2]>,
}

impl Grad {
    fn new(px: usize, py: usize, seed: u32) -> Self {
        let seed = hash(seed.wrapping_mul(0x2545_f491));
        let g = (0..px * py)
            .map(|i| {
                let a = rnd(i as u32, seed) * TAU;
                [a.cos(), a.sin()]
            })
            .collect();
        Self {
            px,
            py,
            fx: px as f32,
            fy: py as f32,
            // In [p, 2p): keeps lattice coordinates positive for UVs warped down to -1.
            ox: (1.0 + rnd(seed, 1)) * px as f32,
            oy: (1.0 + rnd(seed, 2)) * py as f32,
            g,
        }
    }

    fn at(&self, u: f32, v: f32) -> f32 {
        let x = u * self.fx + self.ox;
        let y = v * self.fy + self.oy;
        let (xi, yi) = (x as usize, y as usize);
        let (tx, ty) = (x - xi as f32, y - yi as f32);
        let x0 = xi % self.px;
        let x1 = if x0 + 1 == self.px { 0 } else { x0 + 1 };
        let y0 = yi % self.py;
        let y1 = if y0 + 1 == self.py { 0 } else { y0 + 1 };
        let (r0, r1) = (y0 * self.px, y1 * self.px);
        let (a, b, c, d) = (
            self.g[r0 + x0],
            self.g[r0 + x1],
            self.g[r1 + x0],
            self.g[r1 + x1],
        );
        let n00 = a[0] * tx + a[1] * ty;
        let n10 = b[0] * (tx - 1.0) + b[1] * ty;
        let n01 = c[0] * tx + c[1] * (ty - 1.0);
        let n11 = d[0] * (tx - 1.0) + d[1] * (ty - 1.0);
        let wx = tx * tx * tx * (tx * (tx * 6.0 - 15.0) + 10.0);
        let wy = ty * ty * ty * (ty * (ty * 6.0 - 15.0) + 10.0);
        let top = n00 + (n10 - n00) * wx;
        let bottom = n01 + (n11 - n01) * wx;
        (top + (bottom - top) * wy) * 1.4
    }
}

/// Fractal sum of periodic gradient noise: periods `base`, `2·base`, … with amplitudes
/// falling by `gain`, normalised to sum to 1 (output roughly ±0.5, σ ≈ 0.2).
struct Fbm(Vec<(Grad, f32)>);

impl Fbm {
    fn new(base: usize, octaves: u32, gain: f32, seed: u32) -> Self {
        let total: f32 = (0..octaves).map(|o| gain.powi(o as i32)).sum();
        Self(
            (0..octaves)
                .map(|o| {
                    let p = base << o;
                    (
                        Grad::new(p, p, seed.wrapping_mul(31).wrapping_add(o)),
                        gain.powi(o as i32) / total,
                    )
                })
                .collect(),
        )
    }

    fn at(&self, u: f32, v: f32) -> f32 {
        let mut s = 0.0;
        for (g, a) in &self.0 {
            s += g.at(u, v) * a;
        }
        s
    }
}

/// Periodic cellular (Worley) noise: one jittered feature point per cell of a `p`×`p` grid.
struct Cells {
    p: usize,
    fp: f32,
    pt: Vec<[f32; 2]>,
}

/// Distances (in cells) to the nearest and second-nearest feature points.
struct Hit {
    f1: f32,
    f2: f32,
}

impl Cells {
    fn new(p: usize, seed: u32) -> Self {
        let seed = hash(seed.wrapping_mul(0x9e37_79b9));
        Self {
            p,
            fp: p as f32,
            pt: (0..p * p)
                .map(|k| [rnd(k as u32, seed), rnd(k as u32, seed ^ 0x5bd1_e995)])
                .collect(),
        }
    }

    /// Grid coordinates of `(u, v)` (any value in (-1, 2)): cell column, row and the
    /// position inside the cell.
    fn locate(&self, u: f32, v: f32) -> (usize, usize, f32, f32) {
        let x = u * self.fp + self.fp;
        let y = v * self.fp + self.fp;
        let (xi, yi) = (x as usize, y as usize);
        (xi % self.p, yi % self.p, x - xi as f32, y - yi as f32)
    }

    fn at(&self, u: f32, v: f32) -> Hit {
        let p = self.p;
        let (cx, cy, tx, ty) = self.locate(u, v);
        let (mut f1, mut f2) = (9.0f32, 9.0f32);
        for j in 0..3 {
            let row = (cy + p + j - 1) % p * p;
            let oy = j as f32 - 1.0 - ty;
            for i in 0..3 {
                let q = self.pt[row + (cx + p + i - 1) % p];
                let dx = i as f32 - 1.0 + q[0] - tx;
                let dy = oy + q[1];
                let d = dx * dx + dy * dy;
                if d < f1 {
                    f2 = f1;
                    f1 = d;
                } else if d < f2 {
                    f2 = d;
                }
            }
        }
        Hit {
            f1: f1.sqrt(),
            f2: f2.sqrt(),
        }
    }
}

/// Scattered elliptical discs (pebbles, aggregate, pores): at most one per cell, with a
/// random radius, orientation, aspect and tone; presence follows a density field sampled
/// at the disc centre, so discs are never cut in half.
struct Scatter {
    cells: Cells,
    /// 1 / radius² (0 = no disc in this cell).
    inv_r2: Vec<f32>,
    /// (cos, sin, aspect) of the ellipse.
    shape: Vec<[f32; 3]>,
    tone: Vec<f32>,
}

/// What a `Scatter` contributes at one point.
struct Disc {
    /// Paraboloid height of the covering disc, 0..1.
    dome: f32,
    /// Anti-aliased coverage, 0..1.
    cov: f32,
    /// Tone of the covering disc, -1..1.
    tone: f32,
    /// Contact-shadow weight just outside a disc, 0..1.
    ring: f32,
}

/// Squared normalised distance where a disc's contact shadow ends.
const RING: f32 = 1.7;

impl Scatter {
    fn new(
        p: usize,
        seed: u32,
        radius: (f32, f32),
        max_aspect: f32,
        density: impl Fn(f32, f32) -> f32,
    ) -> Self {
        let cells = Cells::new(p, seed);
        let seed = hash(seed ^ 0x68e3_1da4);
        let mut inv_r2 = Vec::with_capacity(p * p);
        let mut shape = Vec::with_capacity(p * p);
        let mut tone = Vec::with_capacity(p * p);
        for (k, q) in cells.pt.iter().enumerate() {
            let id = k as u32;
            let (u, v) = (
                ((k % p) as f32 + q[0]) / p as f32,
                ((k / p) as f32 + q[1]) / p as f32,
            );
            let present = rnd(id, seed) < density(u.fract(), v.fract());
            let r = radius.0 + (radius.1 - radius.0) * rnd(id, seed ^ 1);
            inv_r2.push(if present { 1.0 / (r * r) } else { 0.0 });
            let a = rnd(id, seed ^ 2) * PI;
            shape.push([
                a.cos(),
                a.sin(),
                1.0 + (max_aspect - 1.0) * rnd(id, seed ^ 3),
            ]);
            tone.push(rnd(id, seed ^ 4) * 2.0 - 1.0);
        }
        Self {
            cells,
            inv_r2,
            shape,
            tone,
        }
    }

    fn at(&self, u: f32, v: f32) -> Disc {
        let c = &self.cells;
        let p = c.p;
        let (cx, cy, tx, ty) = c.locate(u, v);
        let (mut best, mut tone, mut ring) = (0.0f32, 0.0f32, 0.0f32);
        for j in 0..3 {
            let row = (cy + p + j - 1) % p * p;
            let oy = j as f32 - 1.0 - ty;
            for i in 0..3 {
                let k = row + (cx + p + i - 1) % p;
                let inv = self.inv_r2[k];
                if inv <= 0.0 {
                    continue;
                }
                let q = c.pt[k];
                let dx = i as f32 - 1.0 + q[0] - tx;
                let dy = oy + q[1];
                let s = self.shape[k];
                let ex = dx * s[0] + dy * s[1];
                let ey = (dy * s[0] - dx * s[1]) * s[2];
                let dd = (ex * ex + ey * ey) * inv;
                if dd < 1.0 {
                    if 1.0 - dd > best {
                        best = 1.0 - dd;
                        tone = self.tone[k];
                    }
                } else if dd < RING {
                    ring = ring.max((RING - dd) / (RING - 1.0));
                }
            }
        }
        Disc {
            dome: best,
            cov: smooth(0.0, 0.12, best),
            tone,
            ring: if best > 0.0 { 0.0 } else { ring * ring },
        }
    }
}

// ---------------------------------------------------------------------------- stamps

/// Wraps a pixel coordinate onto the `n`-periodic grid.
fn wrap(i: i32, n: usize) -> usize {
    i.rem_euclid(n as i32) as usize
}

/// Soft round dents (negative height, depth ∝ radius) with wrap-around.
fn stamp_dents(buf: &mut [f32], n: usize, count: u32, radius: (f32, f32), depth: f32, seed: u32) {
    let nf = n as f32;
    for k in 0..count {
        let (cx, cy) = (rnd(k, seed) * nf, rnd(k, seed ^ 1) * nf);
        let r = (radius.0 + (radius.1 - radius.0) * rnd(k, seed ^ 2)) * nf;
        let d = depth * r * (0.4 + 0.6 * rnd(k, seed ^ 3));
        let ri = r.ceil() as i32;
        let (ix, iy) = (cx as i32, cy as i32);
        for oy in -ri..=ri {
            let fy = (iy + oy) as f32 + 0.5 - cy;
            let row = wrap(iy + oy, n) * n;
            for ox in -ri..=ri {
                let fx = (ix + ox) as f32 + 0.5 - cx;
                let t = (fx * fx + fy * fy) / (r * r);
                if t < 1.0 {
                    buf[row + wrap(ix + ox, n)] -= d * (1.0 - t) * (1.0 - t);
                }
            }
        }
    }
}

/// Thin tapered scratches as a 0..1 mask (max-blended) with wrap-around. `align` = 0 gives
/// random directions; towards 1 they follow the U axis (brushing direction).
fn stamp_scratches(
    buf: &mut [f32],
    n: usize,
    count: u32,
    length: (f32, f32),
    align: f32,
    seed: u32,
) {
    let nf = n as f32;
    let width = (nf / 1024.0).max(0.5);
    for k in 0..count {
        let (x0, y0) = (rnd(k, seed) * nf, rnd(k, seed ^ 1) * nf);
        let along = rnd(k, seed ^ 5) < align;
        let angle = if along {
            (rnd(k, seed ^ 2) - 0.5) * 0.12
        } else {
            rnd(k, seed ^ 2) * TAU
        };
        let len = (length.0 + (length.1 - length.0) * rnd(k, seed ^ 3)) * nf;
        let w = width * (0.7 + 0.7 * rnd(k, seed ^ 4));
        let strength = 0.45 + 0.55 * rnd(k, seed ^ 6);
        let bend = (rnd(k, seed ^ 7) - 0.5) * 0.5;
        let steps = (len * 2.0) as u32 + 1;
        let reach = (w + 1.0).ceil() as i32;
        for s in 0..=steps {
            let t = s as f32 / steps as f32;
            let a = angle + bend * (t - 0.5);
            let (x, y) = (x0 + a.cos() * len * t, y0 + a.sin() * len * t);
            let v = strength * (PI * t).sin().sqrt();
            let (ix, iy) = (x.floor() as i32, y.floor() as i32);
            for oy in -reach..=reach {
                let fy = (iy + oy) as f32 + 0.5 - y;
                let row = wrap(iy + oy, n) * n;
                for ox in -reach..=reach {
                    let fx = (ix + ox) as f32 + 0.5 - x;
                    let d = (fx * fx + fy * fy).sqrt();
                    if d < w {
                        let i = row + wrap(ix + ox, n);
                        buf[i] = buf[i].max(v * (1.0 - d / w));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const N: usize = 128;

    /// Mean absolute difference of one channel of mip 0 across the wrap-around seam and
    /// between interior neighbours: `[(seam, interior) horizontally, (…) vertically]`.
    /// (step across the wrap-around seam, worst step across any interior line), per axis.
    /// Comparing against the worst interior line keeps sparse features (a lone scratch
    /// running along a line) from failing a texture that does tile.
    fn seams(data: &[u8], n: usize, ch: usize) -> [(f32, f32); 2] {
        let at = |x: usize, y: usize| data[(y * n + x) * 4 + ch] as f32;
        let line = |f: &dyn Fn(usize) -> f32| (0..n).map(f).sum::<f32>() / n as f32;
        let hs = line(&|a| (at(n - 1, a) - at(0, a)).abs());
        let vs = line(&|a| (at(a, n - 1) - at(a, 0)).abs());
        let (mut hi, mut vi) = (0.0f32, 0.0f32);
        for b in 0..n - 1 {
            hi = hi.max(line(&|a| (at(b, a) - at(b + 1, a)).abs()));
            vi = vi.max(line(&|a| (at(a, b) - at(a, b + 1)).abs()));
        }
        [(hs, hi), (vs, vi)]
    }

    #[test]
    fn textures_tile_seamlessly() {
        for kind in Kind::ALL {
            let m = build(kind, N);
            let mut maps = vec![("albedo", &m.albedo)];
            if kind != Kind::Lava {
                maps.push(("normal", &m.normal));
                maps.push(("orm", &m.orm));
            }
            for (name, data) in maps {
                assert_eq!(
                    data.len(),
                    chain_texels(N) * 4,
                    "{kind:?} {name} chain size"
                );
                for ch in 0..3 {
                    for (dir, (seam, inner)) in
                        ["horizontal", "vertical"].iter().zip(seams(data, N, ch))
                    {
                        assert!(
                            seam <= inner * 1.25 + 1.5,
                            "{kind:?} {name} channel {ch}: {dir} seam step {seam:.2} vs worst interior line {inner:.2}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn noise_primitives_are_periodic() {
        let g = Grad::new(7, 3, 1);
        let f = Fbm::new(3, 4, 0.5, 2);
        let c = Cells::new(5, 3);
        for i in 0..50 {
            let t = i as f32 / 50.0;
            assert!((g.at(0.0, t) - g.at(1.0, t)).abs() < 1e-3);
            assert!((g.at(t, 0.0) - g.at(t, 1.0)).abs() < 1e-3);
            assert!((f.at(0.0, t) - f.at(1.0, t)).abs() < 1e-3);
            let (a, b) = (c.at(1e-4, t), c.at(1.0 + 1e-4, t));
            assert!((a.f1 - b.f1).abs() < 1e-3 && (a.f2 - b.f2).abs() < 1e-3);
        }
    }

    #[test]
    fn normal_maps_are_normalised() {
        for kind in [Kind::Paint, Kind::Metal, Kind::Rough, Kind::Terrain] {
            let m = build(kind, N);
            for px in m.normal.as_chunks::<4>().0 {
                let v = [px[0], px[1], px[2]].map(|c| c as f32 / 255.0 * 2.0 - 1.0);
                let len = length(v);
                assert!((len - 1.0).abs() < 0.02, "{kind:?}: normal length {len}");
                assert!(v[2] > 0.2, "{kind:?}: normal faces away ({v:?})");
            }
        }
    }

    #[test]
    fn material_values_stay_in_range() {
        for kind in [Kind::Paint, Kind::Metal, Kind::Rough, Kind::Terrain] {
            let m = build(kind, N);
            let level0 = &m.orm[..N * N * 4];
            let avg = |ch: usize| {
                level0
                    .as_chunks::<4>().0.iter()
                    .map(|p| p[ch] as f32 / 255.0)
                    .sum::<f32>()
                    / (N * N) as f32
            };
            let (rough, metal) = (avg(1), avg(2));
            let (lo, hi) = match kind {
                Kind::Paint => ((0.45, 0.7), (0.05, 0.25)),
                Kind::Metal => ((0.2, 0.45), (0.8, 1.0)),
                _ => ((0.75, 1.0), (0.0, 0.01)),
            };
            assert!(
                (lo.0..=lo.1).contains(&rough),
                "{kind:?}: mean roughness {rough}"
            );
            assert!(
                (hi.0..=hi.1).contains(&metal),
                "{kind:?}: mean metallic {metal}"
            );
            assert!(
                m.mean > 0.6 && m.mean <= 1.0,
                "{kind:?}: mean albedo {}",
                m.mean
            );
        }
    }
}
