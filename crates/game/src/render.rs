//! 3D planet view. Reads sim state only; never writes to it.
//!
//! Surface tile (x, y) occupies world x..x+1, z = -y-1..-y (north is -Z); ground at y = 0.

use std::collections::HashMap;
use std::f32::consts::{FRAC_PI_2, PI, TAU};

use bevy::light::{CascadeShadowConfigBuilder, GlobalAmbientLight};
use bevy::pbr::DistanceFog;
use bevy::prelude::*;
use sim::buildings::{BELT_LEN, BeltTier};
use sim::surface::{Status, Surface, Terrain};
use sim::universe::PlanetCategory;
use sim::{BuildingId, BuildingKind, Dir, Item, Placeable, PlanetId};

use crate::camera::MainCamera;
use crate::input::anchor_for;
use crate::mesh_util::*;
use crate::models::{self, Anim};
use crate::state::*;

pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(bevy::light::DirectionalLightShadowMap { size: 4096 })
            .init_resource::<SceneMaps>()
            .init_resource::<SceneDirty>()
            .init_resource::<Launches>()
            .add_systems(Startup, init_lib)
            .add_systems(
                PostUpdate,
                (
                    sync_planet_scene,
                    animate_parts,
                    animate_world,
                    update_ghost,
                    spawn_launches,
                )
                    .chain()
                    .before(TransformSystems::Propagate),
            )
            .add_systems(Update, draw_planet_gizmos);
    }
}

/// Rocket launches to animate (shipyard planet), queued by the event drain.
#[derive(Resource, Default)]
pub struct Launches(pub Vec<PlanetId>);

pub const WATER_LEVEL: f32 = -0.55;
const BORDER: i32 = 40;

#[derive(Component)]
pub struct PlanetScene;

#[derive(Component)]
struct PartAnim {
    building: BuildingId,
    anim: Anim,
    angle: f32,
}

#[derive(Component)]
struct Grow(f32);

#[derive(Component)]
struct Creature {
    center: Vec3,
    phase: f32,
    radius: f32,
    speed: f32,
}

#[derive(Component)]
struct Pulse {
    base: f32,
    speed: f32,
}

#[derive(Component)]
struct Ghost;

#[derive(Component)]
struct Rocket {
    t: f32,
    start: Vec3,
}

struct PartHandles {
    body: Handle<Mesh>,
    metal: Option<Handle<Mesh>>,
    glow: Option<Handle<Mesh>>,
    pivot: Vec3,
    anim: Anim,
}

pub struct ModelHandles {
    body: Handle<Mesh>,
    metal: Option<Handle<Mesh>>,
    rough: Option<Handle<Mesh>>,
    glow: Option<Handle<Mesh>>,
    glass: Option<Handle<Mesh>>,
    parts: Vec<PartHandles>,
    lamp: Vec3,
}

/// Shared meshes and materials.
#[derive(Resource)]
pub struct Lib {
    /// Painted surfaces (the default vertex-coloured material).
    pub vc: Handle<StandardMaterial>,
    pub metal: Handle<StandardMaterial>,
    pub rough: Handle<StandardMaterial>,
    pub glow: Handle<StandardMaterial>,
    pub glass: Handle<StandardMaterial>,
    terrain: Handle<StandardMaterial>,
    lava: Handle<StandardMaterial>,
    ghost_ok: Handle<StandardMaterial>,
    ghost_bad: Handle<StandardMaterial>,
    lamps: [Handle<StandardMaterial>; 4],
    lamp_mesh: Handle<Mesh>,
    item_meshes: HashMap<Item, Handle<Mesh>>,
    models: HashMap<BuildingKind, ModelHandles>,
    ghost_meshes: HashMap<BuildingKind, Handle<Mesh>>,
    belts: HashMap<BeltTier, Handle<Mesh>>,
    nest: (Handle<Mesh>, Handle<Mesh>),
    creature: (Handle<Mesh>, Handle<Mesh>),
    trees: Vec<(Handle<Mesh>, Handle<Mesh>)>,
    chunks: HashMap<Item, (Handle<Mesh>, Handle<Mesh>)>,
    rocket: (Handle<Mesh>, Handle<Mesh>),
}

/// A degenerate mesh, for models without a glowing part.
fn empty_mesh() -> Geo {
    let mut m = Geo::default();
    m.octa(Vec3::new(0.0, -2.0, 0.0), 0.0001, [0.0; 4]);
    m
}

impl Lib {
    fn model(&mut self, meshes: &mut Assets<Mesh>, kind: BuildingKind) -> &ModelHandles {
        self.models.entry(kind).or_insert_with(|| {
            let m = models::building(kind);
            let opt =
                |b: Geo, meshes: &mut Assets<Mesh>| (!b.is_empty()).then(|| meshes.add(b.build()));
            ModelHandles {
                body: meshes.add(m.body.build()),
                metal: opt(m.metal, meshes),
                rough: opt(m.rough, meshes),
                glow: opt(m.glow, meshes),
                glass: opt(m.glass, meshes),
                parts: m
                    .parts
                    .into_iter()
                    .map(|p| PartHandles {
                        body: meshes.add(p.body.build()),
                        metal: opt(p.metal, meshes),
                        glow: opt(p.glow, meshes),
                        pivot: p.pivot,
                        anim: p.anim,
                    })
                    .collect(),
                lamp: m.lamp,
            }
        })
    }

    fn ghost_mesh(&mut self, meshes: &mut Assets<Mesh>, kind: BuildingKind) -> Handle<Mesh> {
        self.ghost_meshes
            .entry(kind)
            .or_insert_with(|| meshes.add(models::building(kind).merged().build()))
            .clone()
    }

    fn item_mesh(&mut self, meshes: &mut Assets<Mesh>, item: Item) -> Handle<Mesh> {
        self.item_meshes
            .entry(item)
            .or_insert_with(|| meshes.add(models::item(item).build()))
            .clone()
    }

    fn chunk(&mut self, meshes: &mut Assets<Mesh>, item: Item) -> (Handle<Mesh>, Handle<Mesh>) {
        self.chunks
            .entry(item)
            .or_insert_with(|| {
                let mut body = Geo::default();
                let mut glow = Geo::default();
                let c = lin(item.rgb());
                let crystal = matches!(
                    item,
                    Item::ExoticCrystal | Item::VoidShard | Item::Ice | Item::TitaniumOre
                );
                for (x, z, r) in [
                    (-0.18f32, -0.1f32, 0.16f32),
                    (0.15, 0.12, 0.13),
                    (0.12, -0.2, 0.1),
                ] {
                    if crystal {
                        body.cone(Vec3::new(x, 0.0, z), r, r * 3.0, 5, x, scale(c, 0.8));
                        if matches!(item, Item::ExoticCrystal | Item::VoidShard) {
                            glow.cone(Vec3::new(x, 0.0, z), r * 0.5, r * 3.3, 5, x, scale(c, 1.2));
                        }
                    } else {
                        body.rock(
                            Vec3::new(x, r * 0.3, z),
                            r,
                            Vec3::new(1.2, 0.8, 1.0),
                            (x * 100.0) as u64 + 7,
                            c,
                        );
                    }
                }
                if item == Item::Gas {
                    glow.sphere(Vec3::Y * 0.1, 0.12, Vec3::ONE, 6, scale(c, 0.8));
                }
                if glow.is_empty() {
                    glow = empty_mesh();
                }
                (meshes.add(body.build()), meshes.add(glow.build()))
            })
            .clone()
    }
}

fn init_lib(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let m = crate::textures::create(&mut images, &mut mats);
    let (vc, metal, rough, glow, glass, terrain, lava) = (
        m.paint, m.metal, m.rough, m.glow, m.glass, m.terrain, m.lava,
    );
    // Holograms keep the model's own colours, shifted toward blue (or red when blocked).
    let ghost = |tint: Color, glow: LinearRgba, mats: &mut Assets<StandardMaterial>| {
        mats.add(StandardMaterial {
            base_color: tint,
            emissive: glow,
            alpha_mode: AlphaMode::Blend,
            perceptual_roughness: 0.35,
            metallic: 0.0,
            ..default()
        })
    };
    let lamp = |c: LinearRgba, mats: &mut Assets<StandardMaterial>| {
        mats.add(StandardMaterial {
            base_color: Color::LinearRgba(c),
            unlit: true,
            ..default()
        })
    };
    let tree_meshes = (0..5u8)
        .map(|v| {
            let (b, mut g) = models::tree(v, 1.0);
            if g.is_empty() {
                g = empty_mesh();
            }
            (meshes.add(b.build()), meshes.add(g.build()))
        })
        .collect();
    let (nb, ng) = models::nest();
    let (cb, cg) = models::creature();
    let (rocket, flame) = models::rocket();
    commands.insert_resource(Lib {
        vc,
        metal,
        rough,
        glow,
        glass,
        terrain,
        lava,
        ghost_ok: ghost(
            Color::srgba(0.42, 0.68, 1.0, 0.6),
            LinearRgba::rgb(0.02, 0.09, 0.26),
            &mut mats,
        ),
        ghost_bad: ghost(
            Color::srgba(1.0, 0.4, 0.34, 0.6),
            LinearRgba::rgb(0.3, 0.03, 0.02),
            &mut mats,
        ),
        lamps: [
            lamp(LinearRgba::rgb(0.3, 4.0, 0.8), &mut mats),
            lamp(LinearRgba::rgb(4.0, 3.0, 0.3), &mut mats),
            lamp(LinearRgba::rgb(5.0, 0.4, 0.3), &mut mats),
            lamp(LinearRgba::rgb(0.4, 0.4, 0.5), &mut mats),
        ],
        lamp_mesh: meshes.add(Sphere::new(0.07).mesh().ico(1).expect("ico")),
        item_meshes: HashMap::new(),
        models: HashMap::new(),
        ghost_meshes: HashMap::new(),
        belts: BeltTier::ALL
            .iter()
            .map(|t| (*t, meshes.add(models::belt(*t).build())))
            .collect(),
        nest: (meshes.add(nb.build()), meshes.add(ng.build())),
        creature: (meshes.add(cb.build()), meshes.add(cg.build())),
        trees: tree_meshes,
        chunks: HashMap::new(),
        rocket: (meshes.add(rocket.build()), meshes.add(flame.build())),
    });
    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.75, 0.82, 1.0),
        brightness: 350.0,
        ..default()
    });
}

struct BuildingVis {
    entity: Entity,
    lamp: Entity,
    status: Status,
    kind: BuildingKind,
    dir: Dir,
}

#[derive(Resource, Default)]
struct SceneMaps {
    planet: Option<PlanetId>,
    buildings: HashMap<BuildingId, BuildingVis>,
    belts: HashMap<(i32, i32), (Entity, BeltTier, Dir)>,
    items: Vec<Entity>,
    /// Decorations bound to a tile; hidden while something is built on it.
    decor: Vec<((i32, i32), Entity, bool)>,
    nests: HashMap<(i32, i32), Entity>,
    frame: u32,
}

pub fn dir_angle(d: Dir) -> f32 {
    match d {
        Dir::N => 0.0,
        Dir::E => -FRAC_PI_2,
        Dir::S => PI,
        Dir::W => FRAC_PI_2,
    }
}

pub fn footprint_center(x: i32, y: i32, w: i32, h: i32) -> Vec3 {
    to_world(
        Vec2::new(x as f32 + w as f32 / 2.0, y as f32 + h as f32 / 2.0),
        0.0,
    )
}

pub fn tile_world(x: i32, y: i32, h: f32) -> Vec3 {
    to_world(tile_center(x, y), h)
}

pub struct Palette {
    ground: [Rgba; 2],
    rock: [Rgba; 2],
    shore: Rgba,
    seabed: Rgba,
    peak: Rgba,
    pub fog: Color,
    water: Color,
    tree: Option<u8>,
    sea_glow: bool,
}

pub fn palette(cat: PlanetCategory) -> Palette {
    #[allow(clippy::too_many_arguments)]
    fn p(
        g0: u32,
        g1: u32,
        r0: u32,
        r1: u32,
        shore: u32,
        seabed: u32,
        peak: u32,
        fog: u32,
        water: Color,
        tree: Option<u8>,
        glow: bool,
    ) -> Palette {
        Palette {
            ground: [lin(g0), lin(g1)],
            rock: [lin(r0), lin(r1)],
            shore: lin(shore),
            seabed: lin(seabed),
            peak: lin(peak),
            fog: rgb(fog),
            water,
            tree,
            sea_glow: glow,
        }
    }
    match cat {
        PlanetCategory::Living => p(
            0x4e8a35,
            0x6f9a3c,
            0x6e6052,
            0x9a8b78,
            0xd9c48c,
            0x2a6a7a,
            0xb8b0a4,
            0x9cc4dc,
            Color::srgba(0.05, 0.38, 0.62, 0.82),
            Some(0),
            false,
        ),
        PlanetCategory::Rocky => p(
            0x9c7a56,
            0xb08c62,
            0x5e4a3c,
            0x86705c,
            0xc8aa7a,
            0x4a5a5a,
            0xa89a88,
            0xc8b49a,
            Color::srgba(0.12, 0.3, 0.42, 0.85),
            Some(4),
            false,
        ),
        PlanetCategory::Ice => p(
            0xd6e4ee,
            0xc4d8e6,
            0x7d93a8,
            0xa6bacb,
            0xb6cede,
            0x3a5a7a,
            0xffffff,
            0xcfe0ee,
            Color::srgba(0.55, 0.75, 0.9, 0.9),
            Some(2),
            false,
        ),
        PlanetCategory::Volcanic => p(
            0x3a302c,
            0x4a3a32,
            0x241e1c,
            0x3a2e2a,
            0x2a2220,
            0x1a1210,
            0x5a4a44,
            0x3a2420,
            Color::LinearRgba(LinearRgba::new(3.2, 0.8, 0.12, 1.0)),
            None,
            true,
        ),
        PlanetCategory::MetalRich => p(
            0x707684,
            0x5e6470,
            0x464c58,
            0x6a7280,
            0x8a8a84,
            0x2a3440,
            0xb0b8c4,
            0x8a96a6,
            Color::srgba(0.1, 0.25, 0.35, 0.85),
            Some(4),
            false,
        ),
        PlanetCategory::Exotic => p(
            0x5a3a78,
            0x6c4a8a,
            0x3a2650,
            0x55407a,
            0x8a6ab0,
            0x2a1a4a,
            0xd0b0ff,
            0x4a3068,
            Color::LinearRgba(LinearRgba::new(0.6, 0.2, 1.4, 0.8)),
            Some(3),
            true,
        ),
    }
}

/// Distance (in tiles) from each rock tile to the nearest non-rock tile.
fn rock_depth(s: &Surface) -> Vec<u8> {
    let (w, h) = (s.w, s.h);
    let mut d = vec![u8::MAX; (w * h) as usize];
    let mut queue = std::collections::VecDeque::new();
    for y in 0..h {
        for x in 0..w {
            if s.tile(x, y).map(|t| t.terrain) != Some(Terrain::Rock) {
                d[(y * w + x) as usize] = 0;
                queue.push_back((x, y));
            }
        }
    }
    while let Some((x, y)) = queue.pop_front() {
        let v = d[(y * w + x) as usize];
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (nx, ny) = (x + dx, y + dy);
            if nx < 0 || ny < 0 || nx >= w || ny >= h {
                continue;
            }
            let i = (ny * w + nx) as usize;
            if d[i] == u8::MAX {
                d[i] = v.saturating_add(1);
                queue.push_back((nx, ny));
            }
        }
    }
    d
}

struct TerrainGen<'a> {
    s: &'a Surface,
    seed: u64,
    depth: Vec<u8>,
}

fn smooth(x: f32, a: f32, b: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl TerrainGen<'_> {
    fn terrain(&self, x: i32, y: i32) -> Option<Terrain> {
        self.s.tile(x, y).map(|t| t.terrain)
    }

    /// Distance from a point to the map rectangle (0 inside).
    fn outside(&self, x: f32, y: f32) -> f32 {
        let dx = (-x).max(x - self.s.w as f32).max(0.0);
        let dy = (-y).max(y - self.s.h as f32).max(0.0);
        (dx * dx + dy * dy).sqrt()
    }

    fn corner_height(&self, cx: i32, cy: i32) -> f32 {
        let (fx, fy) = (cx as f32, cy as f32);
        let d = self.outside(fx, fy);
        if d <= 0.0 {
            let mut rock = 0;
            let mut lava = 0;
            let mut n = 0;
            let mut min_depth = u8::MAX;
            for (tx, ty) in [(cx - 1, cy - 1), (cx, cy - 1), (cx - 1, cy), (cx, cy)] {
                if let Some(t) = self.terrain(tx, ty) {
                    n += 1;
                    match t {
                        Terrain::Rock => {
                            rock += 1;
                            min_depth = min_depth.min(self.depth[(ty * self.s.w + tx) as usize]);
                        }
                        Terrain::Lava => lava += 1,
                        _ => {}
                    }
                }
            }
            if n == 4 && rock == 4 {
                let k = min_depth.min(7) as f32;
                return 0.45 + k * 0.55 + fbm(fx * 0.2, fy * 0.2, self.seed) * (0.6 + k * 0.4);
            }
            if n == 4 && lava == 4 {
                return -0.22;
            }
            return 0.0;
        }
        let shore = 4.0 + fbm(fx * 0.03, fy * 0.03, self.seed ^ 0x5407) * 34.0;
        let ramp = smooth(d, 1.0, 7.0) * (1.0 - smooth(d, shore - 6.0, shore));
        let hills = fbm(fx * 0.07, fy * 0.07, self.seed ^ 0x4111) * 3.2 * ramp;
        let dropoff = (d - shore).max(0.0) * 0.45;
        hills - dropoff - smooth(d, shore - 3.0, shore + 1.0) * 0.8
    }
}

struct TerrainOut {
    land: Geo,
    lava: Geo,
    border_props: Geo,
    border_glow: Geo,
    tile_trees: Vec<(i32, i32, u8, f32, f32)>,
}

fn build_terrain(cat: PlanetCategory, s: &Surface, seed: u64) -> TerrainOut {
    let pal = palette(cat);
    let tg = TerrainGen {
        s,
        seed,
        depth: rock_depth(s),
    };
    let (x0, y0, x1, y1) = (-BORDER, -BORDER, s.w + BORDER, s.h + BORDER);
    let cw = (x1 - x0 + 1) as usize;
    let mut heights = vec![0.0f32; cw * (y1 - y0 + 1) as usize];
    for cy in y0..=y1 {
        for cx in x0..=x1 {
            heights[(cy - y0) as usize * cw + (cx - x0) as usize] = tg.corner_height(cx, cy);
        }
    }
    let hgt = |cx: i32, cy: i32| heights[(cy - y0) as usize * cw + (cx - x0) as usize];
    let mut out = TerrainOut {
        land: Geo::default(),
        lava: Geo::default(),
        border_props: Geo::default(),
        border_glow: Geo::default(),
        tile_trees: Vec::new(),
    };
    let tree_shapes: Vec<(Geo, Geo)> = (0..5u8).map(|v| models::tree(v, 1.0)).collect();
    for y in y0..y1 {
        for x in x0..x1 {
            let hs = [hgt(x, y), hgt(x + 1, y), hgt(x + 1, y + 1), hgt(x, y + 1)];
            let avg = hs.iter().sum::<f32>() / 4.0;
            let (fx, fy) = (x as f32, y as f32);
            let jitter = 0.94 + hash01(x as i64, y as i64, seed) * 0.12;
            let patch = fbm(fx * 0.09, fy * 0.09, seed ^ 0x77);
            let ground = mixc(pal.ground[0], pal.ground[1], patch);
            let terrain = tg.terrain(x, y);
            let col = match terrain {
                Some(Terrain::Ground) => ground,
                Some(Terrain::Rock) => {
                    let t = (avg / 4.0).clamp(0.0, 1.0);
                    let c = mixc(pal.rock[0], pal.rock[1], t);
                    if avg > 3.2 { mixc(c, pal.peak, 0.6) } else { c }
                }
                Some(Terrain::Lava) => scale(pal.rock[0], 0.6),
                Some(Terrain::Deposit(i)) => {
                    let r = s.richness_of(i) as f32;
                    mixc(ground, lin(i.rgb()), 0.45 + r * 0.06)
                }
                None => {
                    if avg < WATER_LEVEL - 0.7 {
                        pal.seabed
                    } else if avg < WATER_LEVEL + 0.25 {
                        pal.shore
                    } else if avg > 2.2 {
                        mixc(pal.rock[1], pal.peak, ((avg - 2.2) / 1.5).clamp(0.0, 1.0))
                    } else if avg > 1.4 {
                        mixc(ground, pal.rock[1], (avg - 1.4) / 0.8)
                    } else {
                        ground
                    }
                }
            };
            let col = scale(col, jitter);
            let p = |cx: i32, cy: i32, h: f32| to_world(Vec2::new(cx as f32, cy as f32), h);
            let corners = [
                p(x, y, hs[0]),
                p(x + 1, y, hs[1]),
                p(x + 1, y + 1, hs[2]),
                p(x, y + 1, hs[3]),
            ];
            if terrain == Some(Terrain::Lava) {
                let glow = noise(fx * 0.4, fy * 0.4, seed ^ 0x1a5a);
                let c = mixc(lin(0xff5a10), lin(0xffc040), glow);
                out.lava
                    .quad(corners[0], corners[1], corners[2], corners[3], c);
                continue;
            }
            out.land
                .quad(corners[0], corners[1], corners[2], corners[3], col);

            // Decorations
            let r = hash01(x as i64, y as i64, seed ^ 0x7ee);
            let dens = fbm(fx * 0.11, fy * 0.11, seed ^ 0x3e3);
            match (terrain, pal.tree) {
                (Some(Terrain::Ground), Some(v)) => {
                    let (want, variant) = match v {
                        0 => (dens > 0.6 && r < 0.45, if r < 0.12 { 1 } else { 0 }),
                        2 => (dens > 0.62 && r < 0.3, 2),
                        3 => (dens > 0.66 && r < 0.2, 3),
                        _ => (r < 0.025, 4),
                    };
                    if want {
                        out.tile_trees
                            .push((x, y, variant, 0.7 + r * 1.2, r * 40.0));
                    }
                }
                (None, Some(v)) if avg > WATER_LEVEL + 0.35 && avg < 2.4 => {
                    let rich = if v == 0 { 0.5 } else { 0.62 };
                    if dens > rich && r < 0.55 {
                        let variant = if v == 0 && r < 0.15 { 1 } else { v };
                        let h = 0.8 + r * 1.3;
                        let jx = hash01(x as i64, y as i64, 91) - 0.5;
                        let jz = hash01(x as i64, y as i64, 92) - 0.5;
                        let base =
                            to_world(Vec2::new(fx + 0.5 + jx * 0.6, fy + 0.5 + jz * 0.6), avg);
                        let tf = Transform::from_translation(base)
                            .with_rotation(Quat::from_rotation_y(r * 40.0))
                            .with_scale(Vec3::splat(h));
                        let (b, g) = &tree_shapes[variant as usize];
                        out.border_props.append(b, &tf);
                        out.border_glow.append(g, &tf);
                    }
                }
                _ => {}
            }
        }
    }
    out
}

/// Ensures the scene matches the viewed planet, then syncs buildings, belts and items.
#[allow(clippy::too_many_arguments)]
fn sync_planet_scene(
    mut commands: Commands,
    view: Res<View>,
    sim: Res<SimState>,
    mut maps: ResMut<SceneMaps>,
    mut dirty: ResMut<SceneDirty>,
    mut lib: ResMut<Lib>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    scene: Query<Entity, With<PlanetScene>>,
    mut tfs: Query<(&mut Transform, &mut Visibility)>,
    mut mat_q: Query<&mut MeshMaterial3d<StandardMaterial>>,
    mut mesh_q: Query<&mut Mesh3d>,
    mut cam: Single<(&mut DistanceFog, &mut Camera), With<MainCamera>>,
    mut ambient: ResMut<GlobalAmbientLight>,
) {
    let target = match *view {
        View::Planet(p) if sim.0.surface(p).is_some() => Some(p),
        _ => None,
    };
    let lib = &mut *lib;
    if maps.planet != target || dirty.0 {
        for e in &scene {
            commands.entity(e).try_despawn();
        }
        *maps = SceneMaps::default();
        maps.planet = target;
        dirty.0 = false;
        let Some(p) = target else {
            return;
        };
        let planet = sim.0.planet(p).expect("planet");
        let s = planet.surface.as_ref().expect("surface");
        let seed = sim::universe::mix(sim.0.seed(), &p.words());
        let pal = palette(planet.category);
        cam.0.color = pal.fog;
        cam.1.clear_color = ClearColorConfig::Custom(pal.fog);
        ambient.brightness = 160.0;
        ambient.color = pal.fog.mix(&Color::WHITE, 0.5);

        let t = build_terrain(planet.category, s, seed);
        commands.spawn((
            PlanetScene,
            Mesh3d(meshes.add(t.land.build())),
            MeshMaterial3d(lib.terrain.clone()),
        ));
        for (mb, mat) in [
            (t.lava, lib.lava.clone()),
            (t.border_props, lib.vc.clone()),
            (t.border_glow, lib.glow.clone()),
        ] {
            if !mb.is_empty() {
                commands.spawn((
                    PlanetScene,
                    Mesh3d(meshes.add(mb.build())),
                    MeshMaterial3d(mat),
                ));
            }
        }
        // Sea and a deep floor beneath it.
        let center = to_world(Vec2::new(s.w as f32 / 2.0, s.h as f32 / 2.0), 0.0);
        let water = mats.add(if pal.sea_glow {
            StandardMaterial {
                base_color: pal.water,
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                ..default()
            }
        } else {
            StandardMaterial {
                base_color: pal.water,
                perceptual_roughness: 0.06,
                reflectance: 0.7,
                alpha_mode: AlphaMode::Blend,
                ..default()
            }
        });
        let plane = meshes.add(Plane3d::default().mesh().size(1600.0, 1600.0));
        commands.spawn((
            PlanetScene,
            Mesh3d(plane.clone()),
            MeshMaterial3d(water),
            Transform::from_translation(center.with_y(WATER_LEVEL)),
            Pulse {
                base: WATER_LEVEL,
                speed: 0.6,
            },
        ));
        commands.spawn((
            PlanetScene,
            Mesh3d(plane),
            MeshMaterial3d(mats.add(StandardMaterial {
                base_color: Color::srgb(0.03, 0.08, 0.12),
                unlit: true,
                ..default()
            })),
            Transform::from_translation(center.with_y(-6.0)),
        ));
        // Sun
        commands.spawn((
            PlanetScene,
            DirectionalLight {
                illuminance: 9_000.0,
                shadow_maps_enabled: true,
                color: Color::srgb(1.0, 0.96, 0.9),
                ..default()
            },
            Transform::default().looking_to(Vec3::new(-0.45, -0.75, -0.5), Vec3::Y),
            CascadeShadowConfigBuilder {
                first_cascade_far_bound: 40.0,
                maximum_distance: 260.0,
                ..default()
            }
            .build(),
        ));
        // Tile-bound decorations: trees and ore outcrops.
        for (x, y, v, h, rot) in t.tile_trees {
            let (b, g) = lib.trees[v as usize].clone();
            let jx = hash01(x as i64, y as i64, 91) - 0.5;
            let jz = hash01(x as i64, y as i64, 92) - 0.5;
            let e = commands
                .spawn((
                    PlanetScene,
                    Mesh3d(b),
                    MeshMaterial3d(lib.vc.clone()),
                    Transform::from_translation(to_world(
                        Vec2::new(x as f32 + 0.5 + jx * 0.4, y as f32 + 0.5 + jz * 0.4),
                        0.0,
                    ))
                    .with_rotation(Quat::from_rotation_y(rot))
                    .with_scale(Vec3::splat(h)),
                    Visibility::Visible,
                    children![(Mesh3d(g), MeshMaterial3d(lib.glow.clone()))],
                ))
                .id();
            maps.decor.push(((x, y), e, true));
        }
        for y in 0..s.h {
            for x in 0..s.w {
                if let Some(Terrain::Deposit(i)) = s.tile(x, y).map(|t| t.terrain) {
                    if hash01(x as i64, y as i64, seed ^ 0xc4) > 0.55 {
                        continue;
                    }
                    let (b, g) = lib.chunk(&mut meshes, i);
                    let e = commands
                        .spawn((
                            PlanetScene,
                            Mesh3d(b),
                            MeshMaterial3d(lib.vc.clone()),
                            Transform::from_translation(tile_world(x, y, 0.0)).with_rotation(
                                Quat::from_rotation_y(hash01(x as i64, y as i64, 5) * 6.0),
                            ),
                            Visibility::Visible,
                            children![(Mesh3d(g), MeshMaterial3d(lib.glow.clone()))],
                        ))
                        .id();
                    maps.decor.push(((x, y), e, true));
                }
            }
        }
        // Ghost for build previews.
        commands.spawn((
            PlanetScene,
            Ghost,
            bevy::light::NotShadowCaster,
            Mesh3d(lib.belts[&BeltTier::Basic].clone()),
            MeshMaterial3d(lib.ghost_ok.clone()),
            Transform::default(),
            Visibility::Hidden,
        ));
        // Spawn models next frame, once the scene entities exist.
        return;
    }
    let Some(p) = target else {
        return;
    };
    let s = sim.0.surface(p).expect("surface");
    maps.frame = maps.frame.wrapping_add(1);

    // Buildings
    let mut seen = Vec::with_capacity(s.buildings.len());
    for b in s.buildings.values() {
        seen.push(b.id);
        let dir = if b.kind.rotatable() { b.dir } else { Dir::N };
        if let Some(v) = maps.buildings.get(&b.id)
            && (v.kind != b.kind || v.dir != dir)
        {
            commands.entity(v.entity).despawn();
            maps.buildings.remove(&b.id);
        }
        match maps.buildings.get_mut(&b.id) {
            Some(v) => {
                if v.status != b.status {
                    v.status = b.status;
                    if let Ok(mut m) = mat_q.get_mut(v.lamp) {
                        m.0 = lib.lamps[lamp_index(b.status)].clone();
                    }
                }
            }
            None => {
                let (w, h) = b.spec_size();
                let center = footprint_center(b.x, b.y, w, h);
                let lamps = lib.lamps.clone();
                let lamp_mesh = lib.lamp_mesh.clone();
                let (vc, glow, glass) = (lib.vc.clone(), lib.glow.clone(), lib.glass.clone());
                let (metal, rough) = (lib.metal.clone(), lib.rough.clone());
                let model = lib.model(&mut meshes, b.kind);
                let mut ec = commands.spawn((
                    PlanetScene,
                    Mesh3d(model.body.clone()),
                    MeshMaterial3d(vc.clone()),
                    Transform::from_translation(center)
                        .with_rotation(Quat::from_rotation_y(dir_angle(dir)))
                        .with_scale(Vec3::new(1.0, 0.05, 1.0)),
                    Grow(0.0),
                ));
                let mut lamp = Entity::PLACEHOLDER;
                ec.with_children(|c| {
                    if let Some(g) = &model.glow {
                        c.spawn((Mesh3d(g.clone()), MeshMaterial3d(glow.clone())));
                    }
                    if let Some(g) = &model.glass {
                        c.spawn((Mesh3d(g.clone()), MeshMaterial3d(glass.clone())));
                    }
                    if let Some(g) = &model.metal {
                        c.spawn((Mesh3d(g.clone()), MeshMaterial3d(metal.clone())));
                    }
                    if let Some(g) = &model.rough {
                        c.spawn((Mesh3d(g.clone()), MeshMaterial3d(rough.clone())));
                    }
                    for part in &model.parts {
                        let mut e = c.spawn((
                            Mesh3d(part.body.clone()),
                            MeshMaterial3d(vc.clone()),
                            Transform::from_translation(part.pivot),
                            PartAnim {
                                building: b.id,
                                anim: part.anim,
                                angle: 0.0,
                            },
                        ));
                        if let Some(g) = &part.glow {
                            e.with_child((Mesh3d(g.clone()), MeshMaterial3d(glow.clone())));
                        }
                        if let Some(g) = &part.metal {
                            e.with_child((Mesh3d(g.clone()), MeshMaterial3d(metal.clone())));
                        }
                    }
                    lamp = c
                        .spawn((
                            Mesh3d(lamp_mesh),
                            MeshMaterial3d(lamps[lamp_index(b.status)].clone()),
                            Transform::from_translation(model.lamp),
                        ))
                        .id();
                });
                let entity = ec.id();
                maps.buildings.insert(
                    b.id,
                    BuildingVis {
                        entity,
                        lamp,
                        status: b.status,
                        kind: b.kind,
                        dir,
                    },
                );
            }
        }
    }
    if maps.buildings.len() != seen.len() {
        seen.sort_unstable();
        let gone: Vec<BuildingId> = maps
            .buildings
            .keys()
            .filter(|id| seen.binary_search(id).is_err())
            .copied()
            .collect();
        for id in gone {
            if let Some(v) = maps.buildings.remove(&id) {
                commands.entity(v.entity).despawn();
            }
        }
    }

    // Belts
    let mut belt_seen = 0;
    for (x, y, belt) in s.belts() {
        belt_seen += 1;
        match maps.belts.get(&(x, y)).copied() {
            Some((_, tier, dir)) if tier == belt.tier && dir == belt.dir => {}
            other => {
                if let Some((e, _, _)) = other {
                    commands.entity(e).despawn();
                }
                let e = commands
                    .spawn((
                        PlanetScene,
                        Mesh3d(lib.belts[&belt.tier].clone()),
                        MeshMaterial3d(lib.vc.clone()),
                        Transform::from_translation(tile_world(x, y, 0.0))
                            .with_rotation(Quat::from_rotation_y(dir_angle(belt.dir))),
                    ))
                    .id();
                maps.belts.insert((x, y), (e, belt.tier, belt.dir));
            }
        }
    }
    if maps.belts.len() != belt_seen {
        let gone: Vec<(i32, i32)> = maps
            .belts
            .keys()
            .filter(|(x, y)| s.tile(*x, *y).and_then(|t| t.belt).is_none())
            .copied()
            .collect();
        for k in gone {
            if let Some((e, _, _)) = maps.belts.remove(&k) {
                commands.entity(e).despawn();
            }
        }
    }

    // Decorations hide under anything built on their tile.
    if maps.frame % 10 == 1 {
        for (tile, e, shown) in &mut maps.decor {
            let occupied = s
                .tile(tile.0, tile.1)
                .is_some_and(|t| t.building.is_some() || t.belt.is_some());
            if occupied == *shown {
                *shown = !occupied;
                if let Ok((_, mut vis)) = tfs.get_mut(*e) {
                    *vis = if occupied {
                        Visibility::Hidden
                    } else {
                        Visibility::Visible
                    };
                }
            }
        }
    }

    // Nests and their critters.
    let alive: Vec<(i32, i32)> = s.nests.iter().map(|n| (n.x, n.y)).collect();
    for &(x, y) in &alive {
        if maps.nests.contains_key(&(x, y)) {
            continue;
        }
        let c = tile_world(x, y, 0.0);
        let (nb, ng) = lib.nest.clone();
        let (cb, cg) = lib.creature.clone();
        let e = commands
            .spawn((
                PlanetScene,
                Mesh3d(nb),
                MeshMaterial3d(lib.vc.clone()),
                Transform::from_translation(c),
                Visibility::Visible,
                children![(Mesh3d(ng), MeshMaterial3d(lib.glow.clone()))],
            ))
            .with_children(|n| {
                for k in 0..4 {
                    n.spawn((
                        Mesh3d(cb.clone()),
                        MeshMaterial3d(lib.vc.clone()),
                        Transform::default(),
                        Creature {
                            phase: k as f32 * 1.7 + x as f32,
                            radius: 2.0 + k as f32 * 0.7,
                            speed: 0.25 + k as f32 * 0.07,
                            center: Vec3::ZERO,
                        },
                        children![(Mesh3d(cg.clone()), MeshMaterial3d(lib.glow.clone()))],
                    ));
                }
            })
            .id();
        maps.nests.insert((x, y), e);
    }
    if maps.nests.len() != alive.len() {
        let gone: Vec<(i32, i32)> = maps
            .nests
            .keys()
            .filter(|k| !alive.contains(k))
            .copied()
            .collect();
        for k in gone {
            if let Some(e) = maps.nests.remove(&k) {
                commands.entity(e).despawn();
            }
        }
    }

    // Items on belts and in inserter hands, from a reusable pool.
    let mut items: Vec<(Vec3, Item)> = Vec::new();
    for (x, y, belt) in s.belts() {
        if let Some(i) = belt.item {
            let t = belt.progress as f32 / BELT_LEN as f32 - 0.5;
            let p = tile_center(x, y) + dir_vec(belt.dir) * t * TILE;
            items.push((to_world(p, 0.1), i));
        }
    }
    for b in s.buildings.values() {
        if let Some(i) = b.held {
            let p = tile_center(b.x, b.y) + dir_vec(b.dir) * 0.42;
            items.push((to_world(p, 0.36), i));
        }
    }
    while maps.items.len() < items.len() {
        let e = commands
            .spawn((
                PlanetScene,
                Mesh3d(lib.item_meshes.values().next().cloned().unwrap_or_default()),
                MeshMaterial3d(lib.vc.clone()),
                Transform::default(),
                Visibility::Hidden,
            ))
            .id();
        maps.items.push(e);
    }
    for (k, e) in maps.items.iter().enumerate() {
        let Ok((mut tf, mut vis)) = tfs.get_mut(*e) else {
            continue;
        };
        match items.get(k) {
            Some((pos, item)) => {
                tf.translation = *pos;
                if let Ok(mut m) = mesh_q.get_mut(*e) {
                    let h = lib.item_mesh(&mut meshes, *item);
                    if m.0 != h {
                        m.0 = h;
                    }
                }
                *vis = Visibility::Visible;
            }
            None => *vis = Visibility::Hidden,
        }
    }
}

fn lamp_index(s: Status) -> usize {
    match s {
        Status::Working => 0,
        Status::NoPower | Status::Damaged | Status::NoFuel => 2,
        Status::Idle | Status::Inactive => 3,
        _ => 1,
    }
}

fn animate_parts(
    time: Res<Time<Real>>,
    view: Res<View>,
    sim: Res<SimState>,
    virt: Res<Time<Virtual>>,
    mut q: Query<(&mut PartAnim, &mut Transform)>,
) {
    let View::Planet(p) = *view else {
        return;
    };
    let Some(s) = sim.0.surface(p) else {
        return;
    };
    let dt = if virt.is_paused() {
        0.0
    } else {
        time.delta_secs()
    };
    for (mut a, mut tf) in &mut q {
        let Some(b) = s.buildings.get(&a.building) else {
            continue;
        };
        let working = b.status == Status::Working;
        match a.anim {
            Anim::Spin(speed) => {
                if working {
                    a.angle += speed * dt;
                }
            }
            Anim::Idle(speed) => a.angle += speed * dt,
            Anim::Arm => {
                let goal = if b.held.is_some() { 0.0 } else { PI };
                a.angle += (goal - a.angle) * (1.0 - (-10.0 * dt).exp());
            }
            Anim::Aim => {
                let me = tile_center(b.x, b.y);
                let target = s
                    .nests
                    .iter()
                    .map(|n| tile_center(n.x, n.y))
                    .min_by(|a, c| a.distance(me).total_cmp(&c.distance(me)));
                let goal = match target {
                    Some(t) if t.distance(me) < 12.0 => {
                        let d = t - me;
                        // Model faces north (+y on the plane).
                        (-d.x).atan2(d.y)
                    }
                    _ => a.angle + 0.4 * dt,
                };
                let mut diff = (goal - a.angle) % TAU;
                if diff > PI {
                    diff -= TAU;
                } else if diff < -PI {
                    diff += TAU;
                }
                a.angle += diff * (1.0 - (-4.0 * dt).exp());
            }
        }
        tf.rotation = Quat::from_rotation_y(a.angle);
    }
}

#[allow(clippy::type_complexity)]
fn animate_world(
    time: Res<Time<Real>>,
    mut grow: Query<(Entity, &mut Grow, &mut Transform), Without<Creature>>,
    mut critters: Query<(&Creature, &mut Transform), Without<Grow>>,
    mut pulses: Query<(&Pulse, &mut Transform), (Without<Creature>, Without<Grow>)>,
    mut commands: Commands,
) {
    let dt = time.delta_secs();
    let t = time.elapsed_secs();
    for (e, mut g, mut tf) in &mut grow {
        g.0 = (g.0 + dt * 4.0).min(1.0);
        let k = 1.0 - (1.0 - g.0).powi(3);
        tf.scale = Vec3::new(1.0, k.max(0.05), 1.0);
        if g.0 >= 1.0 {
            commands.entity(e).remove::<Grow>();
        }
    }
    for (c, mut tf) in &mut critters {
        let a = c.phase + t * c.speed;
        let r = c.radius + (t * 1.3 + c.phase).sin() * 0.5;
        let p = c.center + Vec3::new(a.cos() * r, 0.0, a.sin() * r);
        let tangent = Vec3::new(-a.sin(), 0.0, a.cos());
        tf.translation = p + Vec3::Y * ((t * 9.0 + c.phase).sin().abs() * 0.05);
        tf.rotation = Quat::from_rotation_arc(Vec3::NEG_Z, tangent);
    }
    for (p, mut tf) in &mut pulses {
        tf.translation.y = p.base + (t * p.speed).sin() * 0.04;
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn update_ghost(
    view: Res<View>,
    tool: Res<Tool>,
    hover: Res<HoverTile>,
    dir: Res<BuildDir>,
    sim: Res<SimState>,
    mut lib: ResMut<Lib>,
    mut meshes: ResMut<Assets<Mesh>>,
    ghost: Option<
        Single<
            (
                &mut Mesh3d,
                &mut MeshMaterial3d<StandardMaterial>,
                &mut Transform,
                &mut Visibility,
            ),
            With<Ghost>,
        >,
    >,
) {
    let Some(ghost) = ghost else {
        return;
    };
    let (mut mesh, mut mat, mut tf, mut vis) = ghost.into_inner();
    let (View::Planet(p), Tool::Build(what), Some(tile)) = (*view, *tool, hover.0) else {
        *vis = Visibility::Hidden;
        return;
    };
    let Some(s) = sim.0.surface(p) else {
        return;
    };
    let (ax, ay) = anchor_for(what, tile);
    let (w, h) = what.size();
    let ok = s.can_place(what, ax, ay).is_ok()
        && sim
            .0
            .colony(p)
            .is_some_and(|c| c.inventory.has_all(what.cost()));
    let (handle, rotatable) = match what {
        Placeable::Belt(t) => (lib.belts[&t].clone(), true),
        Placeable::Building(k) => (lib.ghost_mesh(&mut meshes, k), k.rotatable()),
    };
    if mesh.0 != handle {
        mesh.0 = handle;
    }
    let m = if ok {
        lib.ghost_ok.clone()
    } else {
        lib.ghost_bad.clone()
    };
    if mat.0 != m {
        mat.0 = m;
    }
    let d = if rotatable { dir.0 } else { Dir::N };
    *tf = Transform::from_translation(footprint_center(ax, ay, w, h) + Vec3::Y * 0.02)
        .with_rotation(Quat::from_rotation_y(dir_angle(d)));
    *vis = Visibility::Visible;
}

fn spawn_launches(
    mut commands: Commands,
    mut launches: ResMut<Launches>,
    view: Res<View>,
    sim: Res<SimState>,
    lib: Res<Lib>,
    time: Res<Time<Real>>,
    mut rockets: Query<(Entity, &mut Rocket, &mut Transform)>,
) {
    for p in launches.0.drain(..) {
        if *view != View::Planet(p) {
            continue;
        }
        let Some(s) = sim.0.surface(p) else {
            continue;
        };
        if let Some(b) = s
            .buildings
            .values()
            .find(|b| b.kind == BuildingKind::Shipyard)
        {
            let start = footprint_center(b.x, b.y, 4, 4) + Vec3::new(0.3, 0.24, 0.3);
            commands.spawn((
                PlanetScene,
                Mesh3d(lib.rocket.0.clone()),
                MeshMaterial3d(lib.vc.clone()),
                Transform::from_translation(start),
                Rocket { t: 0.0, start },
                children![(
                    Mesh3d(lib.rocket.1.clone()),
                    MeshMaterial3d(lib.glow.clone())
                )],
            ));
        }
    }
    for (e, mut r, mut tf) in &mut rockets {
        r.t += time.delta_secs();
        let h = 3.0 * r.t * r.t;
        tf.translation = r.start + Vec3::new(r.t * r.t * 0.3, h, 0.0);
        tf.rotation = Quat::from_rotation_z(-(r.t * 0.08).min(0.4));
        if r.t > 6.0 {
            commands.entity(e).despawn();
        }
    }
}

pub fn flat(pos: Vec3) -> Isometry3d {
    Isometry3d::new(pos, Quat::from_rotation_x(FRAC_PI_2))
}

fn draw_planet_gizmos(
    mut g: Gizmos,
    view: Res<View>,
    sim: Res<SimState>,
    hover: Res<HoverTile>,
    tool: Res<Tool>,
    selected: Res<Selected>,
    time: Res<Time<Real>>,
) {
    let View::Planet(p) = *view else {
        return;
    };
    let Some(s) = sim.0.surface(p) else {
        return;
    };
    let size = Vec2::new(s.w as f32, s.h as f32) * TILE;
    g.rect(
        flat(to_world(size / 2.0, 0.03)),
        size,
        Color::srgba(0.4, 0.7, 1.0, 0.25),
    );
    let t = time.elapsed_secs();
    let pulse = 0.6 + 0.4 * (t * 4.0).sin();

    for b in s.buildings.values() {
        if b.kind.crafting().is_some()
            && b.progress > 0
            && let Some(r) = b.recipe
        {
            let (w, h) = b.spec_size();
            let frac = (b.progress as f32 / (r.info().ticks as f32 * 100.0)).min(1.0);
            let c = footprint_center(b.x, b.y, w, h);
            let left = c + Vec3::new(-(w as f32) / 2.0 + 0.2, 1.9, h as f32 / 2.0);
            g.line(
                left,
                left + Vec3::X * (w as f32 - 0.4) * frac,
                Color::srgb(0.3, 1.0, 0.5),
            );
        }
    }
    let red = Color::srgb(1.0, 0.2, 0.15);
    for n in &s.nests {
        let c = tile_world(n.x, n.y, 0.05);
        g.circle(flat(c), 3.2, Color::srgba(1.0, 0.15, 0.2, 0.5 * pulse))
            .resolution(48);
        let frac = n.hp as f32 / sim::surface::NEST_HP as f32;
        let left = c + Vec3::new(-1.0, 2.2, 0.0);
        g.line(
            left,
            left + Vec3::X * 2.0 * frac,
            Color::srgb(1.0, 0.3, 0.3),
        );
        // Warning marker
        let top = c + Vec3::Y * (3.2 + (t * 2.0).sin() * 0.15);
        let (l, r) = (
            top + Vec3::new(-0.35, -0.6, 0.0),
            top + Vec3::new(0.35, -0.6, 0.0),
        );
        g.linestrip([top, l, r, top], red);
        g.line(top - Vec3::Y * 0.18, top - Vec3::Y * 0.4, red);
    }

    if let Selection::Building(sp, id) = selected.0
        && sp == p
        && let Some(b) = s.buildings.get(&id)
    {
        let (w, h) = b.spec_size();
        let c = footprint_center(b.x, b.y, w, h) + Vec3::Y * 0.05;
        let sz = Vec2::new(w as f32, h as f32) * TILE;
        let col = Color::srgba(0.4, 0.9, 1.0, pulse);
        g.rect(flat(c), sz + 0.2, col);
        g.rect(
            flat(c + Vec3::Y * 0.05),
            sz + 0.35,
            col.with_alpha(0.4 * pulse),
        );
        if b.kind == BuildingKind::Turret {
            g.circle(flat(c), 7.0 * TILE, Color::srgba(1.0, 0.3, 0.3, 0.6))
                .resolution(64);
        }
    }

    let Some(tile) = hover.0 else {
        return;
    };
    match *tool {
        Tool::Build(what) => {
            let (ax, ay) = anchor_for(what, tile);
            let (w, h) = what.size();
            let ok = s.can_place(what, ax, ay).is_ok();
            let c = footprint_center(ax, ay, w, h) + Vec3::Y * 0.04;
            let color = if ok {
                Color::srgb(0.3, 1.0, 0.5)
            } else {
                Color::srgb(1.0, 0.3, 0.3)
            };
            g.rect(flat(c), Vec2::new(w as f32, h as f32), color);
            if what == Placeable::Building(BuildingKind::Turret) {
                g.circle(flat(c), 7.0 * TILE, Color::srgba(1.0, 0.3, 0.3, 0.5))
                    .resolution(64);
            }
        }
        Tool::Remove => {
            g.rect(
                flat(tile_world(tile.0, tile.1, 0.05)),
                Vec2::splat(TILE),
                Color::srgb(1.0, 0.4, 0.2),
            );
        }
        Tool::Select => {
            g.rect(
                flat(tile_world(tile.0, tile.1, 0.05)),
                Vec2::splat(TILE),
                Color::srgba(1.0, 1.0, 1.0, 0.5),
            );
        }
    }
}

/// Spawns a building model in its rest pose (no animation, no status lamp). Used by the
/// model gallery; the live scene spawns through `sync_planet_scene`.
pub fn spawn_static_model(
    commands: &mut Commands,
    lib: &mut Lib,
    meshes: &mut Assets<Mesh>,
    kind: BuildingKind,
    tf: Transform,
) -> Entity {
    let (vc, metal, rough, glow, glass) = (
        lib.vc.clone(),
        lib.metal.clone(),
        lib.rough.clone(),
        lib.glow.clone(),
        lib.glass.clone(),
    );
    let model = lib.model(meshes, kind);
    let mut ec = commands.spawn((Mesh3d(model.body.clone()), MeshMaterial3d(vc.clone()), tf));
    ec.with_children(|c| {
        for (mesh, mat) in [
            (&model.metal, &metal),
            (&model.rough, &rough),
            (&model.glow, &glow),
            (&model.glass, &glass),
        ] {
            if let Some(g) = mesh {
                c.spawn((Mesh3d(g.clone()), MeshMaterial3d(mat.clone())));
            }
        }
        for part in &model.parts {
            let mut e = c.spawn((
                Mesh3d(part.body.clone()),
                MeshMaterial3d(vc.clone()),
                Transform::from_translation(part.pivot),
            ));
            if let Some(g) = &part.glow {
                e.with_child((Mesh3d(g.clone()), MeshMaterial3d(glow.clone())));
            }
            if let Some(g) = &part.metal {
                e.with_child((Mesh3d(g.clone()), MeshMaterial3d(metal.clone())));
            }
        }
    });
    ec.id()
}

impl Lib {
    /// Mesh for an item model (paint material), cached.
    pub fn item_model(&mut self, meshes: &mut Assets<Mesh>, item: Item) -> Handle<Mesh> {
        self.item_mesh(meshes, item)
    }

    /// (body, glow) meshes of a tree variant.
    pub fn tree_meshes(&self, variant: usize) -> (Handle<Mesh>, Handle<Mesh>) {
        self.trees[variant % self.trees.len()].clone()
    }

    /// (body, glow) meshes of the hostile nest and the creature.
    pub fn creature_meshes(&self) -> [(Handle<Mesh>, Handle<Mesh>); 2] {
        [self.nest.clone(), self.creature.clone()]
    }

    /// (body, glow) meshes of the shipyard rocket.
    pub fn rocket_meshes(&self) -> (Handle<Mesh>, Handle<Mesh>) {
        self.rocket.clone()
    }

    pub fn belt_mesh(&self, tier: BeltTier) -> Handle<Mesh> {
        self.belts[&tier].clone()
    }
}
