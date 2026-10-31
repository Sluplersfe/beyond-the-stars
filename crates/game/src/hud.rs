//! HUD data that needs history or images: resource rates (+N/min) and minimaps.

use std::collections::{BTreeMap, VecDeque};

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use sim::surface::Terrain;
use sim::universe::SystemKnowledge;
use sim::{Item, PlanetId, Sim, TICKS_PER_SEC};

use crate::camera::CameraRig;
use crate::state::*;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Rates>()
            .add_systems(Startup, init_minimap)
            .add_systems(Update, (track_rates, update_minimap));
    }
}

/// Which inventory the top bar shows: one colony, or the whole civilization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Colony(PlanetId),
    Civ,
}

pub fn scope_of(view: &View, sim: &Sim) -> Scope {
    match *view {
        View::Planet(p) if sim.colony(p).is_some() => Scope::Colony(p),
        _ => Scope::Civ,
    }
}

pub struct Snapshot {
    tick: u64,
    items: BTreeMap<Item, u32>,
    pop: u32,
}

pub fn snapshot(sim: &Sim, scope: Scope) -> (BTreeMap<Item, u32>, u32) {
    let mut items = BTreeMap::new();
    let mut pop = 0;
    for p in sim.colonies() {
        if scope != Scope::Civ && scope != Scope::Colony(p.id) {
            continue;
        }
        let c = p.colony.as_ref().expect("colony");
        pop += c.population;
        for (i, n) in c.inventory.iter() {
            *items.entry(i).or_insert(0) += n;
        }
    }
    (items, pop)
}

#[derive(Resource, Default)]
pub struct Rates {
    scope: Option<Scope>,
    samples: VecDeque<Snapshot>,
}

impl Rates {
    fn span(&self) -> Option<(&Snapshot, &Snapshot, f32)> {
        let (a, b) = (self.samples.front()?, self.samples.back()?);
        let mins = (b.tick - a.tick) as f32 / TICKS_PER_SEC as f32 / 60.0;
        (mins > 0.0).then_some((a, b, mins))
    }

    /// Net change per in-game minute.
    pub fn item_per_min(&self, item: Item) -> f32 {
        self.span()
            .map(|(a, b, m)| {
                (*b.items.get(&item).unwrap_or(&0) as f32
                    - *a.items.get(&item).unwrap_or(&0) as f32)
                    / m
            })
            .unwrap_or(0.0)
    }

    pub fn pop_per_min(&self) -> f32 {
        self.span()
            .map(|(a, b, m)| (b.pop as f32 - a.pop as f32) / m)
            .unwrap_or(0.0)
    }
}

fn track_rates(sim: Res<SimState>, view: Res<View>, mut rates: ResMut<Rates>) {
    let sim = &sim.0;
    let scope = scope_of(&view, sim);
    if rates.scope != Some(scope) {
        rates.scope = Some(scope);
        rates.samples.clear();
    }
    let due = rates
        .samples
        .back()
        .is_none_or(|s| sim.tick() >= s.tick + TICKS_PER_SEC || sim.tick() < s.tick);
    if !due {
        return;
    }
    if rates.samples.back().is_some_and(|s| sim.tick() < s.tick) {
        rates.samples.clear();
    }
    let (items, pop) = snapshot(sim, scope);
    rates.samples.push_back(Snapshot {
        tick: sim.tick(),
        items,
        pop,
    });
    // One in-game minute of history.
    while rates
        .samples
        .front()
        .is_some_and(|s| sim.tick() - s.tick > 60 * TICKS_PER_SEC)
    {
        rates.samples.pop_front();
    }
}

#[derive(Resource)]
pub struct Minimap {
    pub planet: Handle<Image>,
    pub galaxy: Handle<Image>,
}

const GALAXY_PX: u32 = 160;

fn blank(w: u32, h: u32) -> Image {
    Image::new_fill(
        Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[6, 12, 24, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
}

fn init_minimap(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    commands.insert_resource(Minimap {
        planet: images.add(blank(64, 64)),
        galaxy: images.add(blank(GALAXY_PX, GALAXY_PX)),
    });
}

fn put(img: &mut Image, x: i32, y: i32, c: [u8; 3]) {
    let (w, h) = (img.width() as i32, img.height() as i32);
    if x < 0 || y < 0 || x >= w || y >= h {
        return;
    }
    if let Ok(px) = img.pixel_bytes_mut(UVec3::new(x as u32, y as u32, 0)) {
        px[..3].copy_from_slice(&c);
        px[3] = 255;
    }
}

fn hex3(h: u32) -> [u8; 3] {
    [(h >> 16) as u8, (h >> 8) as u8, h as u8]
}

fn update_minimap(
    sim: Res<SimState>,
    view: Res<View>,
    rig: Res<CameraRig>,
    mini: Res<Minimap>,
    mut images: ResMut<Assets<Image>>,
    mut frame: Local<u32>,
) {
    *frame += 1;
    if !frame.is_multiple_of(10) {
        return;
    }
    let sim = &sim.0;
    match *view {
        View::Planet(p) => {
            let Some(planet) = sim.planet(p) else {
                return;
            };
            let Some(s) = planet.surface.as_ref() else {
                return;
            };
            let Some(mut img) = images.get_mut(&mini.planet) else {
                return;
            };
            if img.width() != s.w as u32 || img.height() != s.h as u32 {
                *img = blank(s.w as u32, s.h as u32);
            }
            let ground = hex3(planet.category.rgb());
            let ground = ground.map(|c| (c as u16 * 2 / 3) as u8);
            for y in 0..s.h {
                for x in 0..s.w {
                    let t = s.tile(x, y).expect("tile");
                    let c = if t.building.is_some() {
                        [235, 235, 225]
                    } else if t.belt.is_some() {
                        [150, 150, 150]
                    } else {
                        match t.terrain {
                            Terrain::Ground => ground,
                            Terrain::Rock => ground.map(|c| c / 2),
                            Terrain::Lava => [230, 90, 20],
                            Terrain::Deposit(i) => hex3(i.rgb()),
                        }
                    };
                    put(&mut img, x, s.h - 1 - y, c);
                }
            }
            for n in &s.nests {
                for d in -1..=1 {
                    put(&mut img, n.x + d, s.h - 1 - n.y, [255, 40, 40]);
                    put(&mut img, n.x, s.h - 1 - n.y + d, [255, 40, 40]);
                }
            }
            // Camera focus marker.
            let f = to_plane(rig.focus);
            let r = (rig.dist * 0.45) as i32;
            let (cx, cy) = (f.x as i32, s.h - 1 - f.y as i32);
            for k in -r..=r {
                for (x, y) in [
                    (cx + k, cy - r),
                    (cx + k, cy + r),
                    (cx - r, cy + k),
                    (cx + r, cy + k),
                ] {
                    put(&mut img, x, y, [120, 220, 255]);
                }
            }
        }
        View::Map(_) => {
            let Some(mut img) = images.get_mut(&mini.galaxy) else {
                return;
            };
            let g = sim.player.at.map(|p| p.galaxy).unwrap_or(0);
            let n = GALAXY_PX as i32;
            for y in 0..n {
                for x in 0..n {
                    let d = Vec2::new(x as f32 - n as f32 / 2.0, y as f32 - n as f32 / 2.0)
                        .length()
                        / (n as f32 / 2.0);
                    let glow = ((1.0 - d).max(0.0) * 40.0) as u8;
                    put(&mut img, x, y, [8 + glow, 12 + glow / 2, 26 + glow]);
                }
            }
            let to_px = |pos: (i32, i32)| {
                let k = n as f32 / 2.0 / (crate::panels::GALAXY_MINIMAP_RADIUS * 10.0);
                (
                    (n as f32 / 2.0 + pos.0 as f32 * k) as i32,
                    (n as f32 / 2.0 - pos.1 as f32 * k) as i32,
                )
            };
            for s in &sim.universe.galaxies[g as usize].systems {
                let (x, y) = to_px(s.pos);
                match s.knowledge {
                    SystemKnowledge::Unknown => put(&mut img, x, y, [70, 76, 100]),
                    _ => {
                        let c = hex3(s.star.rgb());
                        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                            put(&mut img, x + dx, y + dy, c);
                        }
                    }
                }
            }
            if let Some(p) = sim.player.at
                && let Some(s) = sim.universe.system(p.system())
            {
                let (x, y) = to_px(s.pos);
                for k in -3..=3 {
                    put(&mut img, x + k, y - 3, [80, 220, 255]);
                    put(&mut img, x + k, y + 3, [80, 220, 255]);
                    put(&mut img, x - 3, y + k, [80, 220, 255]);
                    put(&mut img, x + 3, y + k, [80, 220, 255]);
                }
            }
            let f = to_plane(rig.focus) * 10.0;
            let (cx, cy) = to_px((f.x as i32, f.y as i32));
            let r = (rig.dist * 0.35) as i32;
            for k in -r..=r {
                for (x, y) in [
                    (cx + k, cy - r),
                    (cx + k, cy + r),
                    (cx - r, cy + k),
                    (cx + r, cy + k),
                ] {
                    put(&mut img, x, y, [230, 230, 240]);
                }
            }
        }
    }
}
