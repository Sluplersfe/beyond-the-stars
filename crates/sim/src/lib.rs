//! Deterministic simulation core. No rendering or Bevy dependencies.
//!
//! Rules: fixed-point state only, seeded RNG only, ordered collections only.

use std::collections::BTreeMap;

use rand_core::{RngCore, SeedableRng};
use rand_pcg::Pcg64Mcg;

/// Fixed-point scalar: 16.16 format. Use this for all simulation state.
pub type Fixed = i32;
pub const FIXED_ONE: Fixed = 1 << 16;

/// Stable entity id, derived from seed and coordinates, never from allocation order.
pub type TileId = u64;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Tile {
    pub x: i32,
    pub y: i32,
    /// Simple per-tile value that the tick advances.
    pub value: Fixed,
}

#[derive(Debug)]
pub struct Sim {
    seed: u64,
    tick: u64,
    rng: Pcg64Mcg,
    /// Ordered map so iteration order is stable.
    tiles: BTreeMap<TileId, Tile>,
}

/// Stable id from seed and grid coordinates (splitmix-style mix).
pub fn tile_id(seed: u64, x: i32, y: i32) -> TileId {
    let mut h = seed ^ 0x9E37_79B9_7F4A_7C15;
    h = h.wrapping_add((x as u32 as u64) << 32 | (y as u32 as u64));
    h ^= h >> 30;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^ (h >> 31)
}

impl Sim {
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            tick: 0,
            rng: Pcg64Mcg::seed_from_u64(seed),
            tiles: BTreeMap::new(),
        }
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    pub fn tick(&self) -> u64 {
        self.tick
    }

    pub fn tiles(&self) -> impl Iterator<Item = &Tile> {
        self.tiles.values()
    }

    /// Spawn a `width` x `height` grid of tiles with stable ids.
    pub fn spawn_grid(&mut self, width: i32, height: i32) {
        for y in 0..height {
            for x in 0..width {
                let id = tile_id(self.seed, x, y);
                self.tiles.insert(id, Tile { x, y, value: 0 });
            }
        }
    }

    /// Advance one fixed step. Deterministic for a given seed and history.
    pub fn step(&mut self) {
        for tile in self.tiles.values_mut() {
            let r = self.rng.next_u32() as i32 & 0xFF;
            tile.value = tile.value.wrapping_add(r);
        }
        self.tick += 1;
    }

    /// FNV-1a over the full state. Stable across runs and Rust versions.
    pub fn state_hash(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut feed = |bytes: &[u8]| {
            for b in bytes {
                h ^= *b as u64;
                h = h.wrapping_mul(0x0100_0000_01b3);
            }
        };
        feed(&self.tick.to_le_bytes());
        for (id, tile) in &self.tiles {
            feed(&id.to_le_bytes());
            feed(&tile.x.to_le_bytes());
            feed(&tile.y.to_le_bytes());
            feed(&tile.value.to_le_bytes());
        }
        h
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(seed: u64, ticks: u32) -> u64 {
        let mut sim = Sim::new(seed);
        sim.spawn_grid(16, 16);
        for _ in 0..ticks {
            sim.step();
        }
        sim.state_hash()
    }

    #[test]
    fn determinism_same_seed_same_hash() {
        assert_eq!(run(42, 500), run(42, 500));
    }

    #[test]
    fn different_seed_different_hash() {
        assert_ne!(run(1, 100), run(2, 100));
    }

    #[test]
    fn tile_ids_are_stable_and_unique() {
        let a = tile_id(7, 3, 4);
        assert_eq!(a, tile_id(7, 3, 4));
        assert_ne!(a, tile_id(7, 4, 3));
        assert_ne!(a, tile_id(8, 3, 4));
    }

    #[test]
    fn tick_counter_advances() {
        let mut sim = Sim::new(0);
        sim.step();
        sim.step();
        assert_eq!(sim.tick(), 2);
    }
}
