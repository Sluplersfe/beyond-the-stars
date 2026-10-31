//! 3D star map: system → galaxy → intergalactic views, with screen-space labels.

use std::collections::HashMap;
use std::f32::consts::{FRAC_PI_2, TAU};

use bevy::light::GlobalAmbientLight;
use bevy::pbr::DistanceFog;
use bevy::prelude::*;
use sim::universe::{PlanetCategory, SystemKnowledge, mix};
use sim::{PlanetId, Ship, ShipLoc, Sim, SystemId};

use crate::camera::MainCamera;
use crate::mesh_util::*;
use crate::state::*;
use crate::ui::{PointerOverUi, UiAction, UiActions};

pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MapScene>()
            .init_resource::<LabelPool>()
            .add_systems(Startup, init_map_lib)
            .add_systems(
                Update,
                (follow_journey, map_input)
                    .chain()
                    .run_if(crate::menu::gameplay),
            )
            .add_systems(
                PostUpdate,
                (sync_map_scene, update_ships, face_camera, spin_bodies)
                    .chain()
                    .before(TransformSystems::Propagate),
            )
            .add_systems(
                PostUpdate,
                (draw_map_gizmos, update_labels).after(TransformSystems::Propagate),
            );
    }
}

/// Galaxy units → world units in the galaxy view.
const GALAXY_SCALE: f32 = 0.1;
/// Intergalactic units → world units in the universe view.
const UNIVERSE_SCALE: f32 = 0.025;

#[derive(Component)]
struct MapEntity;

#[derive(Component)]
struct Billboard;

#[derive(Component)]
struct Spin(f32);

#[derive(Component)]
struct ShipMarker;

#[derive(Resource)]
struct MapLib {
    glow_tex: Handle<Image>,
    glow: Handle<StandardMaterial>,
    ship: Handle<Mesh>,
    ship_glow: Handle<Mesh>,
    quad: Handle<Mesh>,
    starfield: Handle<Mesh>,
    star_mats: HashMap<u32, Handle<StandardMaterial>>,
    sprite_mats: HashMap<(u32, u8), Handle<StandardMaterial>>,
    galaxy_clouds: HashMap<(u16, bool), Handle<Mesh>>,
    planet_meshes: HashMap<(PlanetId, bool), Handle<Mesh>>,
    vc: Handle<StandardMaterial>,
}

#[derive(Resource, Default)]
struct MapScene {
    key: Option<(MapLevel, u64)>,
    ships: HashMap<u32, Entity>,
}

fn init_map_lib(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let glow_tex = images.add(glow_image(64, 2.2));
    let glow = mats.add(StandardMaterial {
        base_color: Color::LinearRgba(LinearRgba::rgb(1.6, 1.6, 1.6)),
        unlit: true,
        fog_enabled: false,
        ..default()
    });
    let vc = mats.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.8,
        fog_enabled: false,
        ..default()
    });
    let mut ship = Geo::default();
    ship.cone(Vec3::ZERO, 0.22, 0.9, 6, 0.0, lin(0xdfe6ee));
    ship.cuboid(
        Vec3::new(0.0, 0.1, 0.0),
        Vec3::new(0.9, 0.05, 0.25),
        lin(0x8a96a6),
    );
    let mut tf = Transform::from_rotation(Quat::from_rotation_x(-FRAC_PI_2));
    tf.translation = Vec3::new(0.0, 0.0, 0.2);
    let mut ship_rot = Geo::default();
    ship_rot.append(&ship, &tf);
    let mut ship_glow = Geo::default();
    ship_glow.sphere(
        Vec3::new(0.0, 0.0, 0.25),
        0.16,
        Vec3::new(1.0, 1.0, 1.6),
        8,
        scale(lin(0x60d0ff), 3.0),
    );
    let mut stars = Geo::default();
    for k in 0..2500u64 {
        let u = hash01(k as i64, 1, 77) * 2.0 - 1.0;
        let a = hash01(k as i64, 2, 77) * TAU;
        let r = (1.0 - u * u).sqrt();
        let dir = Vec3::new(r * a.cos(), u, r * a.sin());
        let b = 0.3 + hash01(k as i64, 3, 77).powi(3) * 2.5;
        let tint = mixc(lin(0xb0c8ff), lin(0xffe0c0), hash01(k as i64, 4, 77));
        stars.octa(dir * 700.0, 0.9 + b * 0.6, scale(tint, b));
    }
    commands.insert_resource(MapLib {
        glow_tex,
        glow,
        ship: meshes.add(ship_rot.build()),
        ship_glow: meshes.add(ship_glow.build()),
        quad: meshes.add(Rectangle::new(1.0, 1.0)),
        starfield: meshes.add(stars.build()),
        star_mats: HashMap::new(),
        sprite_mats: HashMap::new(),
        galaxy_clouds: HashMap::new(),
        planet_meshes: HashMap::new(),
        vc,
    });
}

impl MapLib {
    /// Emissive (bloom) material for a star colour.
    fn star_mat(
        &mut self,
        mats: &mut Assets<StandardMaterial>,
        hex: u32,
        k: f32,
    ) -> Handle<StandardMaterial> {
        self.star_mats
            .entry(hex ^ ((k * 100.0) as u32) << 24)
            .or_insert_with(|| {
                let c = rgb(hex).to_linear() * k;
                mats.add(StandardMaterial {
                    base_color: Color::LinearRgba(c),
                    unlit: true,
                    fog_enabled: false,
                    ..default()
                })
            })
            .clone()
    }

    /// Additive soft-glow sprite material.
    fn sprite(
        &mut self,
        mats: &mut Assets<StandardMaterial>,
        hex: u32,
        alpha: u8,
    ) -> Handle<StandardMaterial> {
        let tex = self.glow_tex.clone();
        self.sprite_mats
            .entry((hex, alpha))
            .or_insert_with(|| {
                let c = rgb(hex).to_linear();
                mats.add(StandardMaterial {
                    base_color: Color::LinearRgba(c.with_alpha(alpha as f32 / 255.0)),
                    base_color_texture: Some(tex),
                    unlit: true,
                    alpha_mode: AlphaMode::Add,
                    fog_enabled: false,
                    double_sided: true,
                    cull_mode: None,
                    ..default()
                })
            })
            .clone()
    }
}

pub fn planet_angle(id: PlanetId) -> f32 {
    (mix(
        0xa11e,
        &[id.galaxy as u64, id.system as u64, id.index as u64],
    ) % 6283) as f32
        / 1000.0
}

pub fn orbit_radius(orbit: u8) -> f32 {
    9.0 + 6.5 * orbit as f32
}

pub fn planet_pos(id: PlanetId, orbit: u8) -> Vec3 {
    let a = planet_angle(id);
    Vec3::new(a.cos(), 0.0, -a.sin()) * orbit_radius(orbit)
}

pub fn planet_radius(size: u8) -> f32 {
    0.8 + 0.3 * size as f32
}

fn system_pos(sim: &Sim, id: SystemId) -> Vec3 {
    sim.universe
        .system(id)
        .map(|s| Vec3::new(s.pos.0 as f32, 0.0, -(s.pos.1 as f32)) * GALAXY_SCALE)
        .unwrap_or_default()
}

fn galaxy_pos(sim: &Sim, g: u16) -> Vec3 {
    let p = sim.universe.galaxies[g as usize].pos;
    Vec3::new(p.0 as f32, 0.0, -(p.1 as f32)) * UNIVERSE_SCALE
}

pub fn universe_center(sim: &Sim) -> Vec3 {
    let n = sim.universe.galaxies.len().max(1) as f32;
    sim.universe
        .galaxies
        .iter()
        .map(|g| galaxy_pos(sim, g.id))
        .sum::<Vec3>()
        / n
}

fn orbit_of(sim: &Sim, p: PlanetId) -> u8 {
    sim.planet(p).map(|p| p.orbit).unwrap_or(0)
}

fn progress(sim: &Sim, depart: u64, arrive: u64) -> f32 {
    if arrive <= depart {
        return 1.0;
    }
    ((sim.tick().saturating_sub(depart)) as f32 / (arrive - depart) as f32).clamp(0.0, 1.0)
}

/// Where a ship appears at a map level (and which way it points), if at all.
fn ship_pos(sim: &Sim, s: &Ship, level: MapLevel) -> Option<(Vec3, Vec3)> {
    let k = (s.id % 5) as f32;
    let fwd = Vec3::NEG_Z;
    match (s.loc, level) {
        (ShipLoc::Docked(p), MapLevel::System(sys)) if p.system() == sys => {
            let base = planet_pos(p, orbit_of(sim, p));
            let a = k * 1.25;
            Some((base + Vec3::new(a.cos() * 2.4, 0.6, a.sin() * 2.4), fwd))
        }
        (ShipLoc::Docked(p), MapLevel::Galaxy(g)) if p.galaxy == g => Some((
            system_pos(sim, p.system()) + Vec3::new(1.2 + k * 0.5, 0.8, 0.8),
            fwd,
        )),
        (ShipLoc::Docked(p), MapLevel::Universe) => Some((
            galaxy_pos(sim, p.galaxy) + Vec3::new(4.0 + k, 2.0, 0.0),
            fwd,
        )),
        (
            ShipLoc::Transit {
                from,
                to,
                depart,
                arrive,
            },
            level,
        ) => {
            let t = progress(sim, depart, arrive);
            let lift = Vec3::Y * 0.6;
            let lerp = |a: Vec3, b: Vec3| {
                let d = (b - a).normalize_or(fwd);
                Some((
                    a.lerp(b, t) + lift + Vec3::Y * (t * std::f32::consts::PI).sin() * 1.5,
                    d,
                ))
            };
            match level {
                MapLevel::System(sys) if from.system() == sys && to.system() == sys => lerp(
                    planet_pos(from, orbit_of(sim, from)),
                    planet_pos(to, orbit_of(sim, to)),
                ),
                MapLevel::System(sys) if from.system() == sys || to.system() == sys => {
                    // Leaving or entering the system: travel to or from its edge.
                    let local = if from.system() == sys { from } else { to };
                    let p = planet_pos(local, orbit_of(sim, local));
                    let edge = p.normalize_or(Vec3::X) * 80.0;
                    if from.system() == sys {
                        lerp(p, edge)
                    } else {
                        lerp(edge, p)
                    }
                }
                MapLevel::Galaxy(g) if from.galaxy == g && to.galaxy == g => {
                    lerp(system_pos(sim, from.system()), system_pos(sim, to.system()))
                }
                MapLevel::Universe if from.galaxy != to.galaxy => {
                    lerp(galaxy_pos(sim, from.galaxy), galaxy_pos(sim, to.galaxy))
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// Travel is an animated journey: when the overseer departs aboard a ship, show it on
/// the map; on arrival, land on the destination's surface.
fn follow_journey(
    sim: Res<SimState>,
    mut last: Local<Option<Option<PlanetId>>>,
    mut view: ResMut<View>,
    mut selected: ResMut<Selected>,
) {
    let sim = &sim.0;
    let now = sim.player.at;
    let prev = last.replace(now).unwrap_or(Some(sim::HOME));
    if prev == now {
        return;
    }
    match (
        prev,
        now,
        sim.player.riding.and_then(|id| sim.ships.get(&id)),
    ) {
        (Some(_), None, Some(ship)) => {
            if let ShipLoc::Transit { from, to, .. } = ship.loc {
                *view = View::Map(if from.system() == to.system() {
                    MapLevel::System(from.system())
                } else if from.galaxy == to.galaxy {
                    MapLevel::Galaxy(from.galaxy)
                } else {
                    MapLevel::Universe
                });
                selected.0 = Selection::Ship(ship.id);
            }
        }
        (None, Some(p), _) => {
            *view = if sim.surface(p).is_some() {
                View::Planet(p)
            } else {
                View::Map(MapLevel::System(p.system()))
            };
            selected.0 = Selection::Planet(p);
        }
        _ => {}
    }
}

/// Things that can be clicked on the map, with their world positions.
enum Pick {
    Ship(u32),
    Planet(PlanetId),
    Star(SystemId),
    System(SystemId),
    Galaxy(u16),
}

fn pickables(sim: &Sim, level: MapLevel) -> Vec<(Vec3, f32, Pick)> {
    let mut out = Vec::new();
    for s in sim.ships.values() {
        if let Some((p, _)) = ship_pos(sim, s, level) {
            out.push((p, 14.0, Pick::Ship(s.id)));
        }
    }
    match level {
        MapLevel::System(sid) => {
            if let Some(sys) = sim.universe.system(sid) {
                for p in &sys.planets {
                    out.push((planet_pos(p.id, p.orbit), 26.0, Pick::Planet(p.id)));
                }
                out.push((Vec3::ZERO, 40.0, Pick::Star(sid)));
            }
        }
        MapLevel::Galaxy(g) => {
            for s in &sim.universe.galaxies[g as usize].systems {
                if s.knowledge != SystemKnowledge::Unknown {
                    out.push((system_pos(sim, s.id), 18.0, Pick::System(s.id)));
                }
            }
        }
        MapLevel::Universe => {
            for gal in &sim.universe.galaxies {
                out.push((galaxy_pos(sim, gal.id), 60.0, Pick::Galaxy(gal.id)));
            }
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn map_input(
    view: Res<View>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    over_ui: Res<PointerOverUi>,
    window: Single<&bevy::window::Window>,
    cam: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    sim: Res<SimState>,
    mut selected: ResMut<Selected>,
    mut acts: ResMut<UiActions>,
    mut last_click: Local<f32>,
    time: Res<Time<Real>>,
) {
    let View::Map(level) = *view else {
        return;
    };
    let sim = &sim.0;
    let up = keys.just_pressed(KeyCode::Backspace)
        || (mouse.just_pressed(MouseButton::Right) && !over_ui.0);
    if up {
        let next = match level {
            MapLevel::System(s) => MapLevel::Galaxy(s.galaxy),
            MapLevel::Galaxy(_) => MapLevel::Universe,
            MapLevel::Universe => MapLevel::Universe,
        };
        acts.0.push(UiAction::ViewMap(next));
        return;
    }
    if !mouse.just_pressed(MouseButton::Left) || over_ui.0 {
        return;
    }
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let now = time.elapsed_secs();
    let double = now - *last_click < 0.35;
    *last_click = now;
    let best = pickables(sim, level)
        .into_iter()
        .filter_map(|(p, r, what)| {
            let sp = cam.0.world_to_viewport(cam.1, p).ok()?;
            let d = sp.distance(cursor);
            (d <= r).then_some((d, what))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0));
    match best.map(|b| b.1) {
        Some(Pick::Ship(id)) => selected.0 = Selection::Ship(id),
        Some(Pick::Planet(p)) => {
            selected.0 = Selection::Planet(p);
            if double && sim.surface(p).is_some() {
                acts.0.push(UiAction::ViewPlanet(p));
            }
        }
        Some(Pick::Star(s)) => selected.0 = Selection::System(s),
        Some(Pick::System(s)) => {
            if double || selected.0 == Selection::System(s) {
                acts.0.push(UiAction::ViewMap(MapLevel::System(s)));
            }
            selected.0 = Selection::System(s);
        }
        Some(Pick::Galaxy(g)) if !sim.universe.galaxies[g as usize].systems.is_empty() => {
            acts.0.push(UiAction::ViewMap(MapLevel::Galaxy(g)));
        }
        Some(Pick::Galaxy(_)) | None => {}
    }
}

/// Cheap fingerprint of what the static map scene depends on.
fn scene_key(sim: &Sim, level: MapLevel) -> u64 {
    let mut h: u64 = 1469598103934665603;
    let mut feed = |v: u64| {
        h ^= v;
        h = h.wrapping_mul(1099511628211);
    };
    match level {
        MapLevel::System(sid) => {
            if let Some(sys) = sim.universe.system(sid) {
                for p in &sys.planets {
                    feed(p.category_known() as u64);
                    feed(p.colony.is_some() as u64);
                }
                feed(sys.planets.len() as u64);
            }
        }
        MapLevel::Galaxy(g) => {
            for s in &sim.universe.galaxies[g as usize].systems {
                feed(s.knowledge as u64);
            }
        }
        MapLevel::Universe => {
            for g in &sim.universe.galaxies {
                feed(g.systems.len() as u64);
            }
        }
    }
    h
}

fn category_colors(cat: Option<PlanetCategory>) -> [Rgba; 4] {
    match cat {
        Some(PlanetCategory::Living) => {
            [lin(0x1f4f8a), lin(0x3f8a3a), lin(0x8a7a4a), lin(0xf0f4f8)]
        }
        Some(PlanetCategory::Rocky) => [lin(0x6a4a34), lin(0xa07a52), lin(0xc8a878), lin(0xe0d0b8)],
        Some(PlanetCategory::Ice) => [lin(0x8ab0d0), lin(0xc8e0f0), lin(0xe8f4ff), lin(0xffffff)],
        Some(PlanetCategory::Volcanic) => [
            lin(0x1a1210),
            lin(0x3a2a24),
            lin(0x241a16),
            scale(lin(0xff6a20), 3.0),
        ],
        Some(PlanetCategory::MetalRich) => {
            [lin(0x4a5260), lin(0x7a8494), lin(0x9aa4b4), lin(0xc8d0dc)]
        }
        Some(PlanetCategory::Exotic) => [
            lin(0x3a1a6a),
            lin(0x8a3ac0),
            lin(0xd060ff),
            scale(lin(0x60f0ff), 2.0),
        ],
        None => [lin(0x50545c), lin(0x6a6e78), lin(0x80848c), lin(0x9a9ea8)],
    }
}

/// Noise-coloured sphere for a planet.
fn planet_mesh(id: PlanetId, cat: Option<PlanetCategory>) -> Mesh {
    let mut m = Geo::default();
    m.sphere(Vec3::ZERO, 1.0, Vec3::ONE, 40, [1.0; 4]);
    let pal = category_colors(cat);
    let seed = mix(
        0xb10b,
        &[id.galaxy as u64, id.system as u64, id.index as u64],
    );
    for (p, c) in m.pos.iter().zip(m.col.iter_mut()) {
        let v = Vec3::from(*p);
        let n3 = |k: f32, s: u64| {
            (fbm(v.x * k + 7.0, v.y * k + 3.0, seed ^ s)
                + fbm(v.y * k + 11.0, v.z * k + 5.0, seed ^ s ^ 1)
                + fbm(v.z * k + 13.0, v.x * k + 2.0, seed ^ s ^ 2))
                / 3.0
        };
        let n = n3(2.2, 1);
        let lat = v.y.abs();
        *c = match cat {
            Some(PlanetCategory::Living) => {
                if lat > 0.85 {
                    pal[3]
                } else if n < 0.5 {
                    pal[0]
                } else if n < 0.6 {
                    pal[1]
                } else {
                    mixc(pal[1], pal[2], ((n - 0.6) * 4.0).min(1.0))
                }
            }
            Some(PlanetCategory::Volcanic) => {
                let crack = n3(5.0, 9);
                if (crack - 0.5).abs() < 0.025 {
                    pal[3]
                } else {
                    mixc(pal[0], pal[1], n)
                }
            }
            Some(PlanetCategory::Exotic) => {
                let band = (v.y * 6.0 + n * 4.0).sin() * 0.5 + 0.5;
                if n > 0.66 {
                    pal[3]
                } else {
                    mixc(pal[0], pal[2], band)
                }
            }
            _ => {
                let t = n3(3.0, 4);
                let base = mixc(pal[0], pal[2], t);
                if lat > 0.9 {
                    mixc(base, pal[3], 0.7)
                } else {
                    base
                }
            }
        };
    }
    m.build()
}

/// Spiral-galaxy particle cloud (radius ~65 world units at scale 1).
fn galaxy_cloud(gid: u16, bright: bool, count: u32) -> Mesh {
    let mut m = Geo::default();
    let arms = 2 + (gid % 3) as u32;
    let seed = 0x6a1a + gid as u64;
    let r_max = 65.0;
    let core = lin(0xffd7a0);
    let inner = lin(0xff9ec0);
    let outer = lin(0x6a8cff);
    let pink = lin(0xd070ff);
    let dim = if bright { 1.0 } else { 0.35 };
    for k in 0..count {
        let h = |c: u64| hash01(k as i64, gid as i64, seed ^ c);
        let gauss = |a: u64| (h(a) + h(a + 1) + h(a + 2) + h(a + 3) - 2.0) * 0.9;
        let (pos, col, size);
        if h(9) < 0.18 {
            // Bulge
            let r = h(1).powf(1.5) * 9.0;
            let a = h(2) * TAU;
            pos = Vec3::new(
                a.cos() * r,
                gauss(20) * (2.5 - r * 0.2).max(0.3),
                a.sin() * r,
            );
            col = scale(mixc(core, inner, r / 9.0), 1.4 + (1.0 - r / 9.0) * 2.0);
            size = 0.08 + h(5) * 0.12;
        } else {
            let arm = (h(3) * arms as f32).floor();
            let r = 6.0 + h(1).powf(0.8) * (r_max - 6.0);
            let wind = r * 0.085;
            let spread = gauss(30) * (0.25 + r / r_max * 0.35);
            let a = arm / arms as f32 * TAU + wind + spread;
            let jitter = Vec3::new(gauss(40), gauss(50) * 0.25, gauss(60)) * (1.0 + r * 0.04);
            pos = Vec3::new(a.cos() * r, 0.0, a.sin() * r) + jitter;
            let t = r / r_max;
            let base = if h(7) < 0.15 {
                pink
            } else {
                mixc(inner, outer, t)
            };
            col = scale(base, 0.6 + h(8) * 1.2);
            size = 0.05 + h(5).powi(3) * 0.18;
        }
        m.octa(pos, size, scale(col, dim));
    }
    m.build()
}

#[allow(clippy::too_many_arguments)]
fn sync_map_scene(
    mut commands: Commands,
    view: Res<View>,
    sim: Res<SimState>,
    mut scene: ResMut<MapScene>,
    mut lib: ResMut<MapLib>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    existing: Query<Entity, With<MapEntity>>,
    mut cam: Single<(&mut DistanceFog, &mut Camera), With<MainCamera>>,
    mut ambient: ResMut<GlobalAmbientLight>,
) {
    let sim = &sim.0;
    let key = match *view {
        View::Map(level) => Some((level, scene_key(sim, level))),
        View::Planet(_) => None,
    };
    if scene.key == key {
        return;
    }
    let level_changed = scene.key.map(|k| k.0) != key.map(|k| k.0);
    scene.key = key;
    for e in &existing {
        commands.entity(e).try_despawn();
    }
    scene.ships.clear();
    let Some((level, _)) = key else {
        return;
    };
    if level_changed {
        let space = Color::srgb(0.008, 0.01, 0.03);
        cam.1.clear_color = ClearColorConfig::Custom(space);
        cam.0.color = space;
        ambient.brightness = 80.0;
        ambient.color = Color::srgb(0.6, 0.7, 1.0);
    }
    let lib = &mut *lib;
    commands.spawn((
        MapEntity,
        Mesh3d(lib.starfield.clone()),
        MeshMaterial3d(lib.glow.clone()),
    ));
    let spawn_sprite = |commands: &mut Commands,
                        lib: &mut MapLib,
                        mats: &mut Assets<StandardMaterial>,
                        pos: Vec3,
                        size: f32,
                        hex: u32,
                        alpha: u8| {
        let m = lib.sprite(mats, hex, alpha);
        commands.spawn((
            MapEntity,
            Billboard,
            Mesh3d(lib.quad.clone()),
            MeshMaterial3d(m),
            Transform::from_translation(pos).with_scale(Vec3::splat(size)),
        ));
    };
    match level {
        MapLevel::Galaxy(g) => {
            let cloud = lib
                .galaxy_clouds
                .entry((g, true))
                .or_insert_with(|| meshes.add(galaxy_cloud(g, true, 26000)))
                .clone();
            commands.spawn((
                MapEntity,
                Mesh3d(cloud),
                MeshMaterial3d(lib.glow.clone()),
                Spin(0.004),
            ));
            spawn_sprite(
                &mut commands,
                lib,
                &mut mats,
                Vec3::ZERO,
                55.0,
                0xffb070,
                120,
            );
            spawn_sprite(
                &mut commands,
                lib,
                &mut mats,
                Vec3::ZERO,
                22.0,
                0xffe0c0,
                200,
            );
            // Nebulae along the arms.
            for k in 0..36u64 {
                let r = 12.0 + hash01(k as i64, 1, 0x4e) * 50.0;
                let a = hash01(k as i64, 2, 0x4e) * TAU;
                let hex = [0xd060ff, 0x6080ff, 0xff70a0, 0x40c0ff][(k % 4) as usize];
                spawn_sprite(
                    &mut commands,
                    lib,
                    &mut mats,
                    Vec3::new(a.cos() * r, 0.0, a.sin() * r),
                    10.0 + hash01(k as i64, 3, 0x4e) * 18.0,
                    hex,
                    40,
                );
            }
            for s in &sim.universe.galaxies[g as usize].systems {
                let pos = system_pos(sim, s.id);
                let known = s.knowledge != SystemKnowledge::Unknown;
                let hex = s.star.rgb();
                let k = if known { 6.0 } else { 3.0 };
                let mat = lib.star_mat(&mut mats, hex, k);
                commands.spawn((
                    MapEntity,
                    Mesh3d(
                        meshes.add(
                            Sphere::new(if known { 0.7 } else { 0.35 })
                                .mesh()
                                .ico(2)
                                .expect("ico"),
                        ),
                    ),
                    MeshMaterial3d(mat),
                    Transform::from_translation(pos),
                ));
                if known {
                    spawn_sprite(&mut commands, lib, &mut mats, pos, 6.5, hex, 150);
                } else {
                    spawn_sprite(&mut commands, lib, &mut mats, pos, 2.6, hex, 90);
                }
            }
        }
        MapLevel::System(sid) => {
            let Some(sys) = sim.universe.system(sid) else {
                return;
            };
            let hex = sys.star.rgb();
            let mat = lib.star_mat(&mut mats, hex, 8.0);
            commands.spawn((
                MapEntity,
                Mesh3d(meshes.add(Sphere::new(3.2).mesh().ico(4).expect("ico"))),
                MeshMaterial3d(mat),
            ));
            spawn_sprite(&mut commands, lib, &mut mats, Vec3::ZERO, 30.0, hex, 170);
            spawn_sprite(&mut commands, lib, &mut mats, Vec3::ZERO, 70.0, hex, 50);
            commands.spawn((
                MapEntity,
                PointLight {
                    intensity: 1.0e8,
                    range: 400.0,
                    radius: 3.0,
                    color: rgb(hex).mix(&Color::WHITE, 0.6),
                    shadow_maps_enabled: false,
                    ..default()
                },
            ));
            for p in &sys.planets {
                let cat = p.category_known().then_some(p.category);
                let mesh = lib
                    .planet_meshes
                    .entry((p.id, cat.is_some()))
                    .or_insert_with(|| meshes.add(planet_mesh(p.id, cat)))
                    .clone();
                let pos = planet_pos(p.id, p.orbit);
                let r = planet_radius(p.size);
                commands.spawn((
                    MapEntity,
                    Mesh3d(mesh),
                    MeshMaterial3d(lib.vc.clone()),
                    Transform::from_translation(pos)
                        .with_scale(Vec3::splat(r))
                        .with_rotation(Quat::from_rotation_z(0.35)),
                    Spin(0.15 + p.id.index as f32 * 0.03),
                ));
                let atmo = match cat {
                    Some(PlanetCategory::Living) => Some(0x60a8ff),
                    Some(PlanetCategory::Ice) => Some(0xb0e0ff),
                    Some(PlanetCategory::Exotic) => Some(0xd070ff),
                    Some(PlanetCategory::Volcanic) => Some(0xff7030),
                    _ => None,
                };
                if let Some(c) = atmo {
                    spawn_sprite(&mut commands, lib, &mut mats, pos, r * 3.4, c, 70);
                }
                if p.colony.is_some() {
                    let mut ring = Geo::default();
                    let active = p.colony.as_ref().is_some_and(|c| c.active);
                    let c = if active { 0x50ff90 } else { 0xffc040 };
                    ring.torus(Vec3::ZERO, r * 1.55, 0.05, 48, scale(lin(c), 1.5));
                    commands.spawn((
                        MapEntity,
                        Mesh3d(meshes.add(ring.build())),
                        MeshMaterial3d(lib.glow.clone()),
                        Transform::from_translation(pos).with_rotation(Quat::from_rotation_x(0.3)),
                    ));
                }
            }
        }
        MapLevel::Universe => {
            for gal in &sim.universe.galaxies {
                let charted = !gal.systems.is_empty();
                let cloud = lib
                    .galaxy_clouds
                    .entry((gal.id, charted))
                    .or_insert_with(|| meshes.add(galaxy_cloud(gal.id, charted, 9000)))
                    .clone();
                let pos = galaxy_pos(sim, gal.id);
                let s = gal.radius as f32 * UNIVERSE_SCALE / 65.0 * 3.0;
                commands.spawn((
                    MapEntity,
                    Mesh3d(cloud),
                    MeshMaterial3d(lib.glow.clone()),
                    Transform::from_translation(pos)
                        .with_scale(Vec3::splat(s))
                        .with_rotation(Quat::from_rotation_x(0.2 * gal.id as f32 - 0.2)),
                    Spin(0.01),
                ));
                spawn_sprite(
                    &mut commands,
                    lib,
                    &mut mats,
                    pos,
                    65.0 * s,
                    0xffc090,
                    if charted { 110 } else { 50 },
                );
            }
        }
    }
}

fn update_ships(
    mut commands: Commands,
    view: Res<View>,
    sim: Res<SimState>,
    mut scene: ResMut<MapScene>,
    lib: Res<MapLib>,
    mut q: Query<(&mut Transform, &mut Visibility), With<ShipMarker>>,
) {
    let View::Map(level) = *view else {
        return;
    };
    let sim = &sim.0;
    let size = match level {
        MapLevel::System(_) => 1.0,
        MapLevel::Galaxy(_) => 1.6,
        MapLevel::Universe => 6.0,
    };
    for s in sim.ships.values() {
        let e = *scene.ships.entry(s.id).or_insert_with(|| {
            commands
                .spawn((
                    MapEntity,
                    ShipMarker,
                    Mesh3d(lib.ship.clone()),
                    MeshMaterial3d(lib.vc.clone()),
                    Transform::default(),
                    Visibility::Hidden,
                    children![(
                        Mesh3d(lib.ship_glow.clone()),
                        MeshMaterial3d(lib.glow.clone())
                    )],
                ))
                .id()
        });
        let Ok((mut tf, mut vis)) = q.get_mut(e) else {
            continue;
        };
        match ship_pos(sim, s, level) {
            Some((p, d)) => {
                *tf = Transform::from_translation(p)
                    .with_rotation(Quat::from_rotation_arc(
                        Vec3::NEG_Z,
                        d.normalize_or(Vec3::NEG_Z),
                    ))
                    .with_scale(Vec3::splat(size));
                *vis = Visibility::Visible;
            }
            None => *vis = Visibility::Hidden,
        }
    }
}

fn face_camera(
    cam: Single<&Transform, (With<MainCamera>, Without<Billboard>)>,
    mut q: Query<&mut Transform, With<Billboard>>,
) {
    for mut tf in &mut q {
        tf.rotation = cam.rotation;
    }
}

fn spin_bodies(time: Res<Time<Real>>, mut q: Query<(&Spin, &mut Transform)>) {
    let dt = time.delta_secs();
    for (s, mut tf) in &mut q {
        tf.rotate_local_y(s.0 * dt);
    }
}

fn flat(pos: Vec3) -> Isometry3d {
    Isometry3d::new(pos, Quat::from_rotation_x(FRAC_PI_2))
}

fn dashed(g: &mut Gizmos, a: Vec3, b: Vec3, dash: f32, color: Color, offset: f32) {
    let len = a.distance(b);
    let dir = (b - a).normalize_or_zero();
    let mut t = offset % (dash * 2.0);
    while t < len {
        let e = (t + dash).min(len);
        g.line(a + dir * t, a + dir * e, color);
        t += dash * 2.0;
    }
}

fn draw_map_gizmos(
    mut g: Gizmos,
    view: Res<View>,
    sim: Res<SimState>,
    selected: Res<Selected>,
    focus: Res<FocusShip>,
    time: Res<Time<Real>>,
) {
    let View::Map(level) = *view else {
        return;
    };
    let sim = &sim.0;
    let t = time.elapsed_secs();
    let pulse = 0.5 + 0.5 * (t * 3.0).sin();
    let sel = Color::srgb(1.0, 0.85, 0.3);
    let cyan = Color::srgb(0.3, 0.85, 1.0);
    let trade = Color::srgba(1.0, 0.75, 0.25, 0.85);
    match level {
        MapLevel::System(sid) => {
            let Some(sys) = sim.universe.system(sid) else {
                return;
            };
            for p in &sys.planets {
                g.circle(
                    flat(Vec3::ZERO),
                    orbit_radius(p.orbit),
                    Color::srgba(0.45, 0.6, 0.9, 0.22),
                )
                .resolution(128);
                let pos = planet_pos(p.id, p.orbit);
                let r = planet_radius(p.size);
                if selected.0 == Selection::Planet(p.id) {
                    g.circle(flat(pos), r * 2.0 + pulse * 0.2, sel)
                        .resolution(48);
                }
                if sim.player.at == Some(p.id) {
                    g.circle(flat(pos), r * 2.4 + pulse * 0.4, cyan)
                        .resolution(48);
                }
                if p.hostile && p.hostile_known() {
                    g.circle(flat(pos), r * 2.8, Color::srgba(1.0, 0.25, 0.2, 0.6))
                        .resolution(48);
                }
                if p.survey > 0 && p.survey < 100 {
                    let arc = p.survey as f32 / 100.0 * TAU;
                    g.arc_3d(arc, r * 1.8, flat(pos), Color::srgb(0.4, 0.8, 1.0));
                }
            }
            if selected.0 == Selection::System(sid) {
                g.circle(flat(Vec3::ZERO), 5.0 + pulse * 0.4, sel)
                    .resolution(48);
            }
        }
        MapLevel::Galaxy(gid) => {
            let gal = &sim.universe.galaxies[gid as usize];
            let known: Vec<(SystemId, Vec3)> = gal
                .systems
                .iter()
                .filter(|s| s.knowledge != SystemKnowledge::Unknown)
                .map(|s| (s.id, system_pos(sim, s.id)))
                .collect();
            // Faint links to the nearest known neighbours.
            for (i, (_, a)) in known.iter().enumerate() {
                let mut near: Vec<(f32, Vec3)> = known
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| *j != i)
                    .map(|(_, (_, b))| (a.distance(*b), *b))
                    .collect();
                near.sort_by(|x, y| x.0.total_cmp(&y.0));
                for (d, b) in near.into_iter().take(2) {
                    if d < 28.0 {
                        g.line(*a, b, Color::srgba(0.5, 0.65, 1.0, 0.18));
                    }
                }
            }
            for s in &gal.systems {
                if s.knowledge == SystemKnowledge::Unknown {
                    continue;
                }
                let pos = system_pos(sim, s.id);
                let colonized = s.planets.iter().any(|p| p.colony.is_some());
                if colonized {
                    g.circle(flat(pos), 1.4, Color::srgb(0.3, 1.0, 0.5))
                        .resolution(32);
                }
                if selected.0 == Selection::System(s.id) {
                    g.circle(flat(pos), 2.0 + pulse * 0.3, sel).resolution(32);
                }
                if sim.player.at.map(|p| p.system()) == Some(s.id) {
                    g.circle(flat(pos), 2.6 + pulse * 0.6, cyan).resolution(32);
                    g.circle(flat(pos), 3.6 + pulse, cyan.with_alpha(0.4))
                        .resolution(32);
                }
            }
        }
        MapLevel::Universe => {}
    }
    // Routes: travel (solid, cyan) and automated trade (dashed, gold).
    for s in sim.ships.values() {
        if let Some(r) = &s.route {
            let ends = match level {
                MapLevel::System(sid) if r.a.system() == sid && r.b.system() == sid => Some((
                    planet_pos(r.a, orbit_of(sim, r.a)),
                    planet_pos(r.b, orbit_of(sim, r.b)),
                )),
                MapLevel::Galaxy(gid) if r.a.galaxy == gid && r.b.galaxy == gid => {
                    Some((system_pos(sim, r.a.system()), system_pos(sim, r.b.system())))
                }
                _ => None,
            };
            if let Some((a, b)) = ends {
                dashed(&mut g, a, b, 0.8, trade, t * 1.5);
            }
        }
        if let ShipLoc::Transit { to, .. } = s.loc
            && let Some((p, _)) = ship_pos(sim, s, level)
        {
            let dest = match level {
                MapLevel::System(sid) if to.system() == sid => {
                    Some(planet_pos(to, orbit_of(sim, to)))
                }
                MapLevel::Galaxy(gid) if to.galaxy == gid => Some(system_pos(sim, to.system())),
                MapLevel::Universe => Some(galaxy_pos(sim, to.galaxy)),
                _ => None,
            };
            if let Some(d) = dest {
                let c = if sim.player.riding == Some(s.id) {
                    cyan
                } else {
                    cyan.with_alpha(0.5)
                };
                g.line(p, d, c);
            }
        }
        let highlight = selected.0 == Selection::Ship(s.id) || focus.0 == Some(s.id);
        if highlight && let Some((p, _)) = ship_pos(sim, s, level) {
            let r = match level {
                MapLevel::System(_) => 1.2,
                MapLevel::Galaxy(_) => 1.8,
                MapLevel::Universe => 7.0,
            };
            g.circle(flat(p), r + pulse * 0.2, sel).resolution(24);
        }
    }
}

// ---------------------------------------------------------------------------
// Screen-space labels

struct LabelSpec {
    pos: Vec3,
    title: String,
    color: Color,
    marker: Option<Color>,
    dots: Vec<Color>,
    big: bool,
}

#[derive(Component)]
struct MapLabel;

#[derive(Resource, Default)]
struct LabelPool {
    entries: Vec<LabelEntry>,
}

struct LabelEntry {
    root: Entity,
    text: Entity,
    marker: Entity,
    dots_row: Entity,
    title: String,
    dots: Vec<Color>,
    marker_color: Option<Color>,
}

fn label_specs(sim: &Sim, level: MapLevel) -> Vec<LabelSpec> {
    let mut out = Vec::new();
    let you = sim.player.at;
    let dot_of = |p: &sim::Planet| {
        if p.category_known() {
            rgb(p.category.rgb())
        } else {
            Color::srgb(0.45, 0.48, 0.55)
        }
    };
    match level {
        MapLevel::Galaxy(g) => {
            for s in &sim.universe.galaxies[g as usize].systems {
                if s.knowledge == SystemKnowledge::Unknown {
                    continue;
                }
                let colony = s
                    .planets
                    .iter()
                    .find_map(|p| p.colony.as_ref().map(|c| c.active));
                let hostile = s.planets.iter().any(|p| p.hostile && p.hostile_known());
                let marker = if you.map(|p| p.system()) == Some(s.id) {
                    Color::srgb(0.3, 0.85, 1.0)
                } else if hostile {
                    Color::srgb(1.0, 0.3, 0.25)
                } else {
                    match colony {
                        Some(true) => Color::srgb(0.35, 1.0, 0.5),
                        Some(false) => Color::srgb(1.0, 0.75, 0.25),
                        None if s.knowledge == SystemKnowledge::Visited => {
                            Color::srgb(0.5, 0.75, 1.0)
                        }
                        None => Color::srgb(0.55, 0.58, 0.65),
                    }
                };
                out.push(LabelSpec {
                    pos: system_pos(sim, s.id),
                    title: s.name.clone(),
                    color: Color::srgb(0.92, 0.95, 1.0),
                    marker: Some(marker),
                    dots: s.planets.iter().take(7).map(dot_of).collect(),
                    big: false,
                });
            }
        }
        MapLevel::System(sid) => {
            if let Some(sys) = sim.universe.system(sid) {
                out.push(LabelSpec {
                    pos: Vec3::new(0.0, -4.5, 0.0),
                    title: format!("{} - {}", sys.name, sys.star.name()),
                    color: rgb(sys.star.rgb()).mix(&Color::WHITE, 0.5),
                    marker: None,
                    dots: vec![],
                    big: true,
                });
                for p in &sys.planets {
                    let r = planet_radius(p.size);
                    let mut title = p.name.clone();
                    if p.category_known() {
                        title.push_str(&format!("  {}", p.category.name()));
                    }
                    let marker = if you == Some(p.id) {
                        Some(Color::srgb(0.3, 0.85, 1.0))
                    } else if p.hostile && p.hostile_known() {
                        Some(Color::srgb(1.0, 0.3, 0.25))
                    } else {
                        p.colony.as_ref().map(|c| {
                            if c.active {
                                Color::srgb(0.35, 1.0, 0.5)
                            } else {
                                Color::srgb(1.0, 0.75, 0.25)
                            }
                        })
                    };
                    out.push(LabelSpec {
                        pos: planet_pos(p.id, p.orbit) - Vec3::Y * (r + 0.8),
                        title,
                        color: Color::srgb(0.9, 0.93, 0.98),
                        marker,
                        dots: vec![],
                        big: false,
                    });
                }
            }
        }
        MapLevel::Universe => {
            for gal in &sim.universe.galaxies {
                let charted = !gal.systems.is_empty();
                out.push(LabelSpec {
                    pos: galaxy_pos(sim, gal.id) - Vec3::Y * 8.0,
                    title: if charted {
                        gal.name.clone()
                    } else {
                        format!("{} (uncharted)", gal.name)
                    },
                    color: if charted {
                        Color::srgb(0.85, 0.8, 1.0)
                    } else {
                        Color::srgb(0.55, 0.55, 0.65)
                    },
                    marker: you
                        .filter(|p| p.galaxy == gal.id)
                        .map(|_| Color::srgb(0.3, 0.85, 1.0)),
                    dots: vec![],
                    big: true,
                });
            }
        }
    }
    out
}

fn dot(c: Color, size: f32) -> impl Bundle {
    (
        Node {
            width: px(size),
            height: px(size),
            border_radius: BorderRadius::all(percent(50)),
            border: UiRect::all(px(1)),
            ..default()
        },
        BackgroundColor(c),
        BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.35)),
    )
}

#[allow(clippy::too_many_arguments)]
fn update_labels(
    mut commands: Commands,
    screen: Res<crate::menu::Screen>,
    view: Res<View>,
    sim: Res<SimState>,
    mut pool: ResMut<LabelPool>,
    cam: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut nodes: Query<(&mut Node, &mut Visibility), With<MapLabel>>,
    mut texts: Query<(&mut Text, &mut TextColor, &mut TextFont)>,
    mut markers: Query<(&mut Node, &mut BackgroundColor), Without<MapLabel>>,
) {
    let specs = match *view {
        View::Map(level) if *screen == crate::menu::Screen::Playing => label_specs(&sim.0, level),
        _ => vec![],
    };
    while pool.entries.len() < specs.len() {
        let (mut text, mut marker, mut dots_row) = (
            Entity::PLACEHOLDER,
            Entity::PLACEHOLDER,
            Entity::PLACEHOLDER,
        );
        let root = commands
            .spawn((
                MapLabel,
                Node {
                    position_type: PositionType::Absolute,
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Start,
                    row_gap: px(2),
                    ..default()
                },
                Visibility::Hidden,
                GlobalZIndex(-1),
                Pickable::IGNORE,
            ))
            .with_children(|c| {
                c.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: px(5),
                    ..default()
                })
                .with_children(|row| {
                    marker = row.spawn(dot(Color::NONE, 9.0)).id();
                    text = row
                        .spawn((
                            Text::new(""),
                            TextFont {
                                font_size: FontSize::Px(13.0),
                                ..default()
                            },
                            TextColor(Color::WHITE),
                            TextShadow::default(),
                        ))
                        .id();
                });
                dots_row = c
                    .spawn(Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: px(3),
                        margin: UiRect::left(px(16)),
                        ..default()
                    })
                    .id();
            })
            .id();
        pool.entries.push(LabelEntry {
            root,
            text,
            marker,
            dots_row,
            title: String::new(),
            dots: Vec::new(),
            marker_color: Some(Color::NONE),
        });
    }
    let (camera, cam_tf) = *cam;
    for (k, entry) in pool.entries.iter_mut().enumerate() {
        let Ok((mut node, mut vis)) = nodes.get_mut(entry.root) else {
            continue;
        };
        let Some(spec) = specs.get(k) else {
            *vis = Visibility::Hidden;
            continue;
        };
        let Ok(sp) = camera.world_to_viewport(cam_tf, spec.pos) else {
            *vis = Visibility::Hidden;
            continue;
        };
        *vis = Visibility::Visible;
        node.left = px(sp.x + if spec.big { -60.0 } else { 10.0 });
        node.top = px(sp.y + if spec.big { 6.0 } else { -10.0 });
        if let Ok((mut t, mut c, mut f)) = texts.get_mut(entry.text) {
            if entry.title != spec.title {
                entry.title = spec.title.clone();
                t.0 = spec.title.clone();
            }
            if c.0 != spec.color {
                c.0 = spec.color;
            }
            let size = FontSize::Px(if spec.big { 17.0 } else { 13.0 });
            if f.font_size != size {
                f.font_size = size;
            }
        }
        if entry.marker_color != spec.marker {
            entry.marker_color = spec.marker;
            if let Ok((mut n, mut bg)) = markers.get_mut(entry.marker) {
                n.display = if spec.marker.is_some() {
                    Display::Flex
                } else {
                    Display::None
                };
                bg.0 = spec.marker.unwrap_or(Color::NONE);
            }
        }
        if entry.dots != spec.dots {
            entry.dots = spec.dots.clone();
            commands.entity(entry.dots_row).despawn_children();
            for c in &spec.dots {
                let d = commands.spawn(dot(*c, 9.0)).id();
                commands.entity(entry.dots_row).add_child(d);
            }
        }
    }
}
