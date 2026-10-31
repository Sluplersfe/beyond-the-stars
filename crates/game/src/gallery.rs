//! Model gallery for inspecting art in isolation (debug only).
//!
//! `BTS_GALLERY=<set>` lays models out on a neutral checker floor far from the planet and
//! points the camera at them. `<set>` is `buildings`, `items`, `nature`, `belts`, or a
//! comma list of building / item names as spelled in code (`Miner,Smelter,Gear`).
//! Optional: `BTS_GALLERY_YAW` / `BTS_GALLERY_PITCH` (radians, default 0.6 / 0.75),
//! `BTS_GALLERY_ZOOM` (distance multiplier, default 1). The layout order (left→right,
//! back→front rows) is logged at startup. Combine with `BTS_SHOT` for screenshots.

use bevy::prelude::*;
use bevy::transform::TransformSystems;
use sim::{BeltTier, BuildingKind, Item};

use crate::camera::MainCamera;
use crate::mesh_util::*;
use crate::render::{Lib, spawn_static_model};

/// Far from any planet surface, so the scene never overlaps terrain.
const ORIGIN: Vec3 = Vec3::new(4000.0, 0.0, 4000.0);

pub struct GalleryPlugin;

impl Plugin for GalleryPlugin {
    fn build(&self, app: &mut App) {
        if std::env::var("BTS_GALLERY").is_err() {
            return;
        }
        app.init_resource::<Framing>()
            .add_systems(PostStartup, spawn_gallery)
            .add_systems(PostUpdate, aim_camera.before(TransformSystems::Propagate))
            .add_systems(PostUpdate, hide_ui.before(TransformSystems::Propagate));
    }
}

#[derive(Resource, Default)]
struct Framing {
    center: Vec3,
    radius: f32,
}

enum Entry {
    Building(BuildingKind),
    Item(Item),
    Belt(BeltTier),
    Tree(usize),
    Nest,
    Creature,
    Rocket,
}

impl Entry {
    fn footprint(&self) -> f32 {
        match self {
            Entry::Building(k) => k.spec().size.0 as f32,
            Entry::Item(_) => 0.5,
            Entry::Belt(_) => 1.0,
            Entry::Tree(_) | Entry::Creature => 1.0,
            Entry::Nest => 2.5,
            Entry::Rocket => 1.0,
        }
    }

    fn name(&self) -> String {
        match self {
            Entry::Building(k) => format!("{k:?}"),
            Entry::Item(i) => format!("{i:?}"),
            Entry::Belt(t) => format!("Belt{t:?}"),
            Entry::Tree(v) => format!("Tree{v}"),
            Entry::Nest => "Nest".into(),
            Entry::Creature => "Creature".into(),
            Entry::Rocket => "Rocket".into(),
        }
    }
}

fn entries(spec: &str) -> Vec<Entry> {
    match spec {
        "buildings" => BuildingKind::ALL
            .iter()
            .map(|k| Entry::Building(*k))
            .collect(),
        "items" => Item::ALL.iter().map(|i| Entry::Item(*i)).collect(),
        "belts" => BeltTier::ALL.iter().map(|t| Entry::Belt(*t)).collect(),
        "nature" => {
            let mut v: Vec<Entry> = (0..5).map(Entry::Tree).collect();
            v.extend([Entry::Nest, Entry::Creature, Entry::Rocket]);
            v
        }
        list => list
            .split(',')
            .filter_map(|name| {
                let name = name.trim();
                BuildingKind::ALL
                    .iter()
                    .find(|k| format!("{k:?}") == name)
                    .map(|k| Entry::Building(*k))
                    .or_else(|| {
                        Item::ALL
                            .iter()
                            .find(|i| format!("{i:?}") == name)
                            .map(|i| Entry::Item(*i))
                    })
                    .or_else(|| match name {
                        "Nest" => Some(Entry::Nest),
                        "Creature" => Some(Entry::Creature),
                        "Rocket" => Some(Entry::Rocket),
                        n if n.starts_with("Tree") => n[4..].parse().ok().map(Entry::Tree),
                        n if n.starts_with("Belt") => BeltTier::ALL
                            .iter()
                            .find(|t| format!("{t:?}") == n[4..])
                            .map(|t| Entry::Belt(*t)),
                        _ => {
                            warn!("gallery: unknown model {name:?}");
                            None
                        }
                    })
            })
            .collect(),
    }
}

fn spawn_gallery(
    mut commands: Commands,
    mut lib: ResMut<Lib>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut framing: ResMut<Framing>,
) {
    let spec = std::env::var("BTS_GALLERY").unwrap_or_default();
    let list = entries(&spec);
    if list.is_empty() {
        return;
    }
    let cols = (list.len() as f32).sqrt().ceil() as usize;
    let gap = if list.iter().all(|e| matches!(e, Entry::Item(_))) {
        0.35
    } else {
        1.2
    };
    let cell = list.iter().map(|e| e.footprint()).fold(0.0, f32::max) + gap;
    let rows = list.len().div_ceil(cols);
    let mut names: Vec<Vec<String>> = vec![Vec::new(); rows];
    for (i, e) in list.iter().enumerate() {
        let (c, r) = (i % cols, i / cols);
        names[r].push(e.name());
        let p = ORIGIN
            + Vec3::new(
                (c as f32 - (cols as f32 - 1.0) / 2.0) * cell,
                0.0,
                (r as f32 - (rows as f32 - 1.0) / 2.0) * cell,
            );
        let tf = Transform::from_translation(p);
        match e {
            Entry::Building(k) => {
                spawn_static_model(&mut commands, &mut lib, &mut meshes, *k, tf);
            }
            Entry::Item(it) => {
                let h = lib.item_model(&mut meshes, *it);
                commands.spawn((Mesh3d(h), MeshMaterial3d(lib.vc.clone()), tf));
            }
            Entry::Belt(t) => {
                for k in -1..=1 {
                    commands.spawn((
                        Mesh3d(lib.belt_mesh(*t)),
                        MeshMaterial3d(lib.vc.clone()),
                        tf.with_translation(p + Vec3::Z * k as f32),
                    ));
                }
            }
            Entry::Tree(v) => spawn_pair(&mut commands, &lib, lib.tree_meshes(*v), tf),
            Entry::Nest => spawn_pair(&mut commands, &lib, lib.creature_meshes()[0].clone(), tf),
            Entry::Creature => {
                spawn_pair(&mut commands, &lib, lib.creature_meshes()[1].clone(), tf)
            }
            Entry::Rocket => spawn_pair(&mut commands, &lib, lib.rocket_meshes(), tf),
        }
    }
    for (r, row) in names.iter().enumerate() {
        info!(
            "gallery row {r} (back to front), left to right: {}",
            row.join(", ")
        );
    }
    let half = Vec2::new(cols as f32, rows as f32) * cell / 2.0;
    framing.center = ORIGIN + Vec3::Y * 0.6;
    framing.radius = half.length().max(1.5);

    // Checker floor, one square per tile, so scale reads at a glance.
    let mut floor = Geo::default();
    let n = (half.max_element() + 4.0).ceil() as i32;
    for x in -n..n {
        for z in -n..n {
            let c = if (x + z).rem_euclid(2) == 0 {
                0x3a3f46
            } else {
                0x33373d
            };
            let (x0, z0) = (ORIGIN.x + x as f32, ORIGIN.z + z as f32);
            floor.quad(
                Vec3::new(x0, 0.0, z0 + 1.0),
                Vec3::new(x0 + 1.0, 0.0, z0 + 1.0),
                Vec3::new(x0 + 1.0, 0.0, z0),
                Vec3::new(x0, 0.0, z0),
                lin(c),
            );
        }
    }
    commands.spawn((
        Mesh3d(meshes.add(floor.build())),
        MeshMaterial3d(lib.rough.clone()),
    ));
}

fn spawn_pair(
    commands: &mut Commands,
    lib: &Lib,
    (b, g): (Handle<Mesh>, Handle<Mesh>),
    tf: Transform,
) {
    commands.spawn((
        Mesh3d(b),
        MeshMaterial3d(lib.vc.clone()),
        tf,
        children![(Mesh3d(g), MeshMaterial3d(lib.glow.clone()))],
    ));
}

fn env_f32(key: &str, default: f32) -> f32 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn aim_camera(framing: Res<Framing>, mut cam: Single<&mut Transform, With<MainCamera>>) {
    if framing.radius <= 0.0 {
        return;
    }
    let yaw = env_f32("BTS_GALLERY_YAW", 0.6);
    let pitch = env_f32("BTS_GALLERY_PITCH", 0.75);
    let dist = framing.radius * 2.1 * env_f32("BTS_GALLERY_ZOOM", 1.0);
    let dir = Vec3::new(
        yaw.sin() * pitch.cos(),
        pitch.sin(),
        yaw.cos() * pitch.cos(),
    );
    **cam = Transform::from_translation(framing.center + dir * dist)
        .looking_at(framing.center, Vec3::Y);
}

/// The HUD would cover the models; hide every UI root.
fn hide_ui(mut q: Query<&mut Visibility, (With<Node>, Without<ChildOf>)>) {
    for mut v in &mut q {
        if *v != Visibility::Hidden {
            *v = Visibility::Hidden;
        }
    }
}
