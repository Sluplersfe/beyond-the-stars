//! Shared game-side resources. The sim lives here; everything else only reads it or
//! queues commands for it.

use std::collections::VecDeque;

use bevy::prelude::*;
use sim::{BuildingId, Command, Dir, Placeable, PlanetId, Sim, SystemId};

/// World units per surface tile.
pub const TILE: f32 = 1.0;

#[derive(Resource)]
pub struct SimState(pub Sim);

/// Commands from input and UI, applied to the sim in `FixedUpdate`.
#[derive(Resource, Default)]
pub struct CommandQueue(pub Vec<Command>);

impl CommandQueue {
    pub fn push(&mut self, c: Command) {
        self.0.push(c);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapLevel {
    Universe,
    Galaxy(u16),
    System(SystemId),
}

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Planet(PlanetId),
    Map(MapLevel),
}

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Select,
    Build(Placeable),
    Remove,
}

#[derive(Resource, Debug, Clone, Copy)]
pub struct BuildDir(pub Dir);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    None,
    Building(PlanetId, BuildingId),
    Planet(PlanetId),
    System(SystemId),
    Ship(u32),
}

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selected(pub Selection);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Popup {
    None,
    Research,
    TechTree,
    Ships,
    Logistics,
    Colonies,
    Resources,
    Codex,
    Help,
}

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenWindow(pub Popup);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MsgKind {
    Info,
    Good,
    Warn,
    Error,
}

#[derive(Debug, Clone)]
pub struct Message {
    pub text: String,
    pub kind: MsgKind,
    pub age: f32,
}

#[derive(Resource, Default)]
pub struct MessageLog {
    pub entries: VecDeque<Message>,
}

impl MessageLog {
    pub fn push(&mut self, kind: MsgKind, text: impl Into<String>) {
        self.entries.push_front(Message {
            text: text.into(),
            kind,
            age: 0.0,
        });
        self.entries.truncate(8);
    }
}

/// Temporary overlay shown only during genuine procedural generation.
#[derive(Resource, Default)]
pub struct GenerationOverlay {
    pub lines: Vec<String>,
    pub remaining: f32,
}

/// Ship selected for Instant-Warp, if any.
#[derive(Resource, Default)]
pub struct Warp(pub Option<u32>);

/// Set when the visual scene for the current view must be rebuilt.
#[derive(Resource, Default)]
pub struct SceneDirty(pub bool);

pub fn rgb(hex: u32) -> Color {
    Color::srgb_u8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

pub fn tile_center(x: i32, y: i32) -> Vec2 {
    Vec2::new((x as f32 + 0.5) * TILE, (y as f32 + 0.5) * TILE)
}

/// Surface plane (tile units, y = north) to 3D world (y up, north = -z).
pub fn to_world(p: Vec2, height: f32) -> Vec3 {
    Vec3::new(p.x, height, -p.y)
}

/// 3D world point to surface plane coordinates.
pub fn to_plane(w: Vec3) -> Vec2 {
    Vec2::new(w.x, -w.z)
}

pub fn world_to_tile(p: Vec2) -> (i32, i32) {
    ((p.x / TILE).floor() as i32, (p.y / TILE).floor() as i32)
}

/// Compact number formatting: 2.4k, 1.2M.
pub fn fmt_num(n: f64) -> String {
    let a = n.abs();
    if a >= 1e6 {
        format!("{:.1}M", n / 1e6)
    } else if a >= 1e4 {
        format!("{:.0}k", n / 1e3)
    } else if a >= 1e3 {
        format!("{:.1}k", n / 1e3)
    } else {
        format!("{n:.0}")
    }
}

pub fn dir_vec(d: Dir) -> Vec2 {
    let (x, y) = d.delta();
    Vec2::new(x as f32, y as f32)
}

pub fn fmt_ticks(ticks: u64) -> String {
    let s = ticks / sim::TICKS_PER_SEC;
    if s >= 3600 {
        format!("{}h{:02}m", s / 3600, (s / 60) % 60)
    } else if s >= 60 {
        format!("{}m{:02}s", s / 60, s % 60)
    } else {
        format!("{s}s")
    }
}

pub fn fmt_kw(kw: i64) -> String {
    if kw.abs() >= 1000 {
        format!("{:.1} MW", kw as f64 / 1000.0)
    } else {
        format!("{kw} kW")
    }
}

/// The ship most recently selected; planet panels offer to send it there.
#[derive(Resource, Default)]
pub struct FocusShip(pub Option<u32>);

/// Surface tile under the cursor in planet view.
#[derive(Resource, Default)]
pub struct HoverTile(pub Option<(i32, i32)>);
