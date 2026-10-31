//! Procedural models. Buildings face north (-Z), are centred on their footprint and stand
//! on y = 0; one tile is one world unit.
//!
//! Each model is split into material layers so one mesh never needs two materials:
//! `body` (painted panels), `metal` (bare/brushed metal), `rough` (concrete, rubber,
//! soil, matte plastic), `glass` (transparent) and `glow` (unlit emissive; colours above
//! 1.0 bloom). Vertex colours tint every layer.

use bevy::prelude::*;
use sim::BuildingKind;

use crate::mesh_util::*;

mod colony;
mod items;
pub mod kit;
mod logistics;
mod nature;
mod power;
mod production;

pub use colony::rocket;
pub use items::item;
pub use logistics::belt;
pub use nature::{creature, nest, tree};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Anim {
    /// Spins about Y while the building works.
    Spin(f32),
    /// Inserter arm: swings between back (empty) and front (holding).
    Arm,
    /// Turret head: tracks the nearest nest.
    Aim,
    /// Always spins slowly.
    Idle(f32),
}

/// A moving piece, built around its own pivot (the origin of its geometry).
pub struct Part {
    pub body: Geo,
    pub metal: Geo,
    pub glow: Geo,
    pub pivot: Vec3,
    pub anim: Anim,
}

impl Part {
    #[allow(dead_code)]
    pub fn new(pivot: Vec3, anim: Anim) -> Self {
        Self {
            body: Geo::fine(),
            metal: Geo::fine(),
            glow: Geo::fine(),
            pivot,
            anim,
        }
    }
}

pub struct Model {
    pub body: Geo,
    pub metal: Geo,
    pub rough: Geo,
    pub glass: Geo,
    pub glow: Geo,
    pub parts: Vec<Part>,
    /// Where the status lamp sits.
    pub lamp: Vec3,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            body: Geo::fine(),
            metal: Geo::fine(),
            rough: Geo::fine(),
            glass: Geo::fine(),
            glow: Geo::fine(),
            parts: Vec::new(),
            lamp: Vec3::new(0.25, 0.9, 0.25),
        }
    }
}

impl Model {
    /// Every layer and part in its rest pose as one mesh (used for the placement hologram).
    pub fn merged(&self) -> Geo {
        let mut g = self.body.clone();
        for l in [&self.metal, &self.rough, &self.glass, &self.glow] {
            g.append(l, &Transform::IDENTITY);
        }
        for p in &self.parts {
            let tf = Transform::from_translation(p.pivot);
            g.append(&p.body, &tf);
            g.append(&p.metal, &tf);
            g.append(&p.glow, &tf);
        }
        g
    }
}

pub fn building(kind: BuildingKind) -> Model {
    use BuildingKind as K;
    let s = kind.spec().size.0 as f32;
    let mut m = Model {
        lamp: Vec3::new(s / 2.0 - 0.25, 0.9, s / 2.0 - 0.25),
        ..default()
    };
    match kind {
        K::Miner
        | K::MinerMk2
        | K::Smelter
        | K::SmelterMk2
        | K::Assembler
        | K::AssemblerMk2
        | K::Refinery => production::build(kind, s, &mut m),
        K::Inserter | K::FastInserter | K::Depot | K::Provider | K::Splitter | K::Merger => {
            logistics::build(kind, s, &mut m)
        }
        K::Generator | K::Solar | K::Reactor | K::Transmitter | K::Beacon => {
            power::build(kind, s, &mut m)
        }
        K::House | K::Habitat | K::Lab | K::LabMk2 | K::Observatory | K::Shipyard | K::Turret => {
            colony::build(kind, s, &mut m)
        }
    }
    for g in [&mut m.body, &mut m.metal, &mut m.rough] {
        g.ao(0.45);
    }
    m
}
