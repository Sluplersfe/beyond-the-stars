//! A planet surface: terrain grid, belts, buildings and the local factory simulation.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::buildings::{BELT_LEN, BeltTier, BuildingKind, Dir, Placeable};
use crate::items::{Inventory, Item};
use crate::recipes::{Category, Recipe};
use crate::research::Research;
use crate::ships::ShipOrder;
use crate::universe::{Planet, PlanetCategory, mix, pick};

/// Stable entity id, derived from a seed and grid coordinates, never from allocation order.
pub type BuildingId = u64;

/// Stable id from seed and grid coordinates (splitmix-style mix).
pub fn tile_id(seed: u64, x: i32, y: i32) -> BuildingId {
    let mut h = seed ^ 0x9E37_79B9_7F4A_7C15;
    h = h.wrapping_add(((x as u32 as u64) << 32) | (y as u32 as u64));
    h ^= h >> 30;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^ (h >> 31)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Terrain {
    Ground,
    Rock,
    Lava,
    Deposit(Item),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Belt {
    pub dir: Dir,
    pub tier: BeltTier,
    pub item: Option<Item>,
    /// 0..=BELT_LEN; the item is ready to leave the tile at BELT_LEN.
    pub progress: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tile {
    pub terrain: Terrain,
    pub building: Option<BuildingId>,
    pub belt: Option<Belt>,
}

/// What a building is doing. Rendering uses this to show bottlenecks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    Idle,
    Working,
    NoRecipe,
    Locked,
    MissingInputs,
    OutputFull,
    NoPower,
    NoFuel,
    NoWorkers,
    Inactive,
    Damaged,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Idle => "Idle",
            Status::Working => "Working",
            Status::NoRecipe => "No recipe",
            Status::Locked => "Recipe not researched",
            Status::MissingInputs => "Missing inputs",
            Status::OutputFull => "Output full",
            Status::NoPower => "NO POWER",
            Status::NoFuel => "No fuel",
            Status::NoWorkers => "Not enough workers",
            Status::Inactive => "Work systems not established",
            Status::Damaged => "Damaged by hostiles",
        }
    }

    pub fn is_problem(self) -> bool {
        !matches!(self, Status::Idle | Status::Working)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Building {
    pub id: BuildingId,
    pub kind: BuildingKind,
    pub x: i32,
    pub y: i32,
    pub dir: Dir,
    pub recipe: Option<Recipe>,
    /// Item a miner extracts or a provider pushes.
    pub filter: Option<Item>,
    pub input: Inventory,
    pub output: Inventory,
    /// Work units done on the current craft (100 per tick at speed 1).
    pub progress: u32,
    pub timer: u32,
    pub held: Option<Item>,
    pub modules: Vec<Item>,
    pub status: Status,
    pub powered: bool,
    pub staffed: bool,
    pub order: Option<ShipOrder>,
    pub disabled_until: u64,
    /// Player cap on items taken in, per minute (`None` = unlimited).
    #[serde(default)]
    pub in_limit: Option<u32>,
    /// Player cap on items sent out, per minute.
    #[serde(default)]
    pub out_limit: Option<u32>,
    /// Token buckets for the limits, in 1/`RATE_UNIT` items.
    #[serde(default)]
    pub in_budget: u32,
    #[serde(default)]
    pub out_budget: u32,
}

/// Ticks per minute: a limit of N items/min adds N units per tick, an item costs this many.
pub const RATE_UNIT: u32 = crate::TICKS_PER_SEC as u32 * 60;
/// Highest settable limit, items per minute.
pub const RATE_MAX: u32 = 1200;

fn bucket_ok(limit: Option<u32>, budget: u32) -> bool {
    limit.is_none() || budget >= RATE_UNIT
}

fn bucket_refill(limit: Option<u32>, budget: &mut u32) {
    if let Some(l) = limit {
        // Small burst allowance absorbs machines that only act every few ticks.
        *budget = (*budget + l).min(2 * RATE_UNIT);
    }
}

fn bucket_spend(limit: Option<u32>, budget: &mut u32) {
    if limit.is_some() {
        *budget = budget.saturating_sub(RATE_UNIT);
    }
}

impl Building {
    pub fn can_take_in(&self) -> bool {
        bucket_ok(self.in_limit, self.in_budget)
    }

    pub fn can_send_out(&self) -> bool {
        bucket_ok(self.out_limit, self.out_budget)
    }

    pub fn spent_out(&mut self) {
        bucket_spend(self.out_limit, &mut self.out_budget);
    }

    pub fn set_limits(&mut self, input: Option<u32>, output: Option<u32>) {
        let clamp = |v: Option<u32>| v.map(|v| v.min(RATE_MAX));
        self.in_limit = clamp(input);
        self.out_limit = clamp(output);
        self.in_budget = 0;
        self.out_budget = 0;
    }

    pub fn spec_size(&self) -> (i32, i32) {
        self.kind.spec().size
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        let (w, h) = self.spec_size();
        x >= self.x && y >= self.y && x < self.x + w && y < self.y + h
    }

    /// Effective power draw in kW, after modules.
    pub fn power_draw(&self) -> u32 {
        let base = self.kind.spec().power as i64;
        let mut pct: i64 = 100;
        for m in &self.modules {
            match m {
                Item::SpeedModule => pct += 50,
                Item::EfficiencyModule => pct -= 30,
                _ => {}
            }
        }
        (base * pct.max(20) / 100) as u32
    }

    /// Work units per tick for crafting and mining.
    pub fn speed(&self) -> u32 {
        let base = match self.kind {
            BuildingKind::MinerMk2 => 200,
            _ => self.kind.crafting().map(|c| c.1).unwrap_or(100),
        };
        let speed_mods = self
            .modules
            .iter()
            .filter(|m| **m == Item::SpeedModule)
            .count() as u32;
        base + base * speed_mods / 2
    }

    /// Wants power this tick (based on last tick's status).
    pub fn wants_power(&self) -> bool {
        if self.kind.spec().power == 0 {
            return false;
        }
        match self.kind {
            BuildingKind::Inserter
            | BuildingKind::FastInserter
            | BuildingKind::Provider
            | BuildingKind::Turret
            | BuildingKind::Beacon => true,
            _ => matches!(self.status, Status::Working | Status::NoPower),
        }
    }

    /// Tile in front of a building, where it pushes output.
    pub fn front(&self) -> (i32, i32) {
        let (w, h) = self.spec_size();
        match self.dir {
            Dir::N => (self.x, self.y + h),
            Dir::E => (self.x + w, self.y),
            Dir::S => (self.x, self.y - 1),
            Dir::W => (self.x - 1, self.y),
        }
    }

    /// Tile behind a 1x1 building, where an inserter picks up.
    pub fn back(&self) -> (i32, i32) {
        let (dx, dy) = self.dir.delta();
        (self.x - dx, self.y - dy)
    }

    /// Whether this building takes `item` as input right now.
    pub fn accepts(&self, item: Item, research: &Research) -> bool {
        use BuildingKind as K;
        if !self.can_take_in() {
            return false;
        }
        match self.kind {
            K::Depot => true,
            K::Splitter | K::Merger => self.input.total() < 2,
            K::Generator => matches!(item, Item::Carbon | Item::Fuel) && self.input.total() < 5,
            K::Reactor => item == Item::FuelCell && self.input.total() < 2,
            k if k.crafting().is_some() => {
                let recipe = match self.recipe {
                    Some(r) => r,
                    None if k.crafting().map(|c| c.0) == Some(Category::Smelting) => {
                        match Recipe::smelting_for(item) {
                            Some(r) => r,
                            None => return false,
                        }
                    }
                    None => return false,
                };
                if let Some(t) = recipe.info().tech
                    && !research.has(t)
                {
                    return false;
                }
                recipe
                    .info()
                    .inputs
                    .iter()
                    .any(|&(i, n)| i == item && self.input.get(i) < (n * 3).max(4))
            }
            _ => false,
        }
    }

    /// Insert one item (caller checked `accepts`). Depots are handled by the caller.
    fn insert(&mut self, item: Item) {
        bucket_spend(self.in_limit, &mut self.in_budget);
        if self.recipe.is_none() && self.kind.crafting().map(|c| c.0) == Some(Category::Smelting) {
            self.recipe = Recipe::smelting_for(item);
        }
        self.input.add(item, 1);
    }

    /// Everything inside the building, returned on removal.
    pub fn contents(&self) -> Inventory {
        let mut inv = self.input.clone();
        inv.merge(self.output.clone());
        if let Some(h) = self.held {
            inv.add(h, 1);
        }
        for m in &self.modules {
            inv.add(*m, 1);
        }
        inv
    }

    fn hash_into(&self, feed: &mut impl FnMut(&[u8])) {
        feed(&self.id.to_le_bytes());
        feed(&[
            self.kind as u8,
            self.dir.code(),
            self.recipe.map(|r| r as u8 + 1).unwrap_or(0),
            self.filter.map(|r| r as u8 + 1).unwrap_or(0),
            self.held.map(|r| r as u8 + 1).unwrap_or(0),
            self.status as u8,
            self.powered as u8,
            self.staffed as u8,
        ]);
        feed(&self.x.to_le_bytes());
        feed(&self.y.to_le_bytes());
        self.input.hash_into(feed);
        self.output.hash_into(feed);
        feed(&self.progress.to_le_bytes());
        feed(&self.timer.to_le_bytes());
        feed(&self.disabled_until.to_le_bytes());
        for v in [self.in_limit, self.out_limit] {
            feed(&v.map(|v| v + 1).unwrap_or(0).to_le_bytes());
        }
        feed(&self.in_budget.to_le_bytes());
        feed(&self.out_budget.to_le_bytes());
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Nest {
    pub x: i32,
    pub y: i32,
    pub hp: u32,
}

pub const NEST_HP: u32 = 3000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Surface {
    pub w: i32,
    pub h: i32,
    seed: u64,
    tiles: Vec<Tile>,
    pub buildings: BTreeMap<BuildingId, Building>,
    /// Indices of tiles holding belts, in ascending order.
    belts: BTreeSet<u32>,
    pub nests: Vec<Nest>,
    /// Deposit richness per resource (1-3).
    pub richness: Vec<(Item, u8)>,
    #[serde(skip)]
    scratch: Vec<u8>,
}

/// Result of a factory step that the colony needs.
#[derive(Debug, Default)]
pub struct FactoryOutput {
    pub delivered: Inventory,
}

impl Surface {
    pub fn new(w: i32, h: i32, seed: u64) -> Self {
        Self {
            w,
            h,
            seed,
            tiles: vec![
                Tile {
                    terrain: Terrain::Ground,
                    building: None,
                    belt: None,
                };
                (w * h) as usize
            ],
            buildings: BTreeMap::new(),
            belts: BTreeSet::new(),
            nests: Vec::new(),
            richness: Vec::new(),
            scratch: Vec::new(),
        }
    }

    pub fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.w && y < self.h
    }

    fn idx(&self, x: i32, y: i32) -> Option<usize> {
        self.in_bounds(x, y).then(|| (y * self.w + x) as usize)
    }

    pub fn tile(&self, x: i32, y: i32) -> Option<&Tile> {
        self.idx(x, y).map(|i| &self.tiles[i])
    }

    pub fn set_terrain(&mut self, x: i32, y: i32, t: Terrain) {
        if let Some(i) = self.idx(x, y) {
            self.tiles[i].terrain = t;
        }
    }

    pub fn belt_count(&self) -> usize {
        self.belts.len()
    }

    pub fn belts(&self) -> impl Iterator<Item = (i32, i32, &Belt)> + '_ {
        self.belts.iter().filter_map(|&i| {
            let t = &self.tiles[i as usize];
            t.belt
                .as_ref()
                .map(|b| (i as i32 % self.w, i as i32 / self.w, b))
        })
    }

    pub fn building_at(&self, x: i32, y: i32) -> Option<&Building> {
        let id = self.tile(x, y)?.building?;
        self.buildings.get(&id)
    }

    pub fn richness_of(&self, item: Item) -> u8 {
        self.richness
            .iter()
            .find(|(i, _)| *i == item)
            .map(|r| r.1)
            .unwrap_or(1)
    }

    /// Check a placement without changing anything.
    pub fn can_place(&self, what: Placeable, x: i32, y: i32) -> Result<(), &'static str> {
        let (w, h) = what.size();
        let mut deposit = false;
        for ty in y..y + h {
            for tx in x..x + w {
                let Some(t) = self.tile(tx, ty) else {
                    return Err("Out of bounds");
                };
                match t.terrain {
                    Terrain::Rock => return Err("Blocked by rock"),
                    Terrain::Lava => return Err("Cannot build on lava"),
                    Terrain::Deposit(_) => deposit = true,
                    Terrain::Ground => {}
                }
                if t.building.is_some() || t.belt.is_some() {
                    return Err("Tile is occupied");
                }
                if self
                    .nests
                    .iter()
                    .any(|n| (n.x - tx).abs() <= 1 && (n.y - ty).abs() <= 1)
                {
                    return Err("Hostile nest in the way");
                }
            }
        }
        if let Placeable::Building(k) = what
            && k.is_miner()
            && !deposit
        {
            return Err("Miners must cover a deposit");
        }
        Ok(())
    }

    /// Place a belt. Fails if the tile already holds something.
    pub fn place_belt(&mut self, x: i32, y: i32, dir: Dir, tier: BeltTier) -> bool {
        if self.can_place(Placeable::Belt(tier), x, y).is_err() {
            return false;
        }
        let i = self.idx(x, y).expect("checked");
        self.tiles[i].belt = Some(Belt {
            dir,
            tier,
            item: None,
            progress: 0,
        });
        self.belts.insert(i as u32);
        true
    }

    /// Place a building with its anchor (bottom-left) at x, y.
    pub fn place_building(
        &mut self,
        kind: BuildingKind,
        x: i32,
        y: i32,
        dir: Dir,
    ) -> Result<BuildingId, &'static str> {
        self.can_place(Placeable::Building(kind), x, y)?;
        let (w, h) = kind.spec().size;
        let id = tile_id(self.seed, x, y);
        let mut filter = None;
        if kind.is_miner() {
            // Mine the most common deposit under the footprint (ties: first in item order).
            let mut counts: BTreeMap<Item, u32> = BTreeMap::new();
            for ty in y..y + h {
                for tx in x..x + w {
                    if let Some(Tile {
                        terrain: Terrain::Deposit(item),
                        ..
                    }) = self.tile(tx, ty)
                    {
                        *counts.entry(*item).or_insert(0) += 1;
                    }
                }
            }
            filter = counts
                .iter()
                .max_by_key(|(i, n)| (**n, std::cmp::Reverse(**i)))
                .map(|(i, _)| *i);
        }
        for ty in y..y + h {
            for tx in x..x + w {
                let i = self.idx(tx, ty).expect("checked");
                self.tiles[i].building = Some(id);
            }
        }
        self.buildings.insert(
            id,
            Building {
                id,
                kind,
                x,
                y,
                dir,
                recipe: None,
                filter,
                input: Inventory::new(),
                output: Inventory::new(),
                progress: 0,
                timer: 0,
                held: None,
                modules: Vec::new(),
                status: Status::Idle,
                powered: false,
                staffed: false,
                order: None,
                disabled_until: 0,
                in_limit: None,
                out_limit: None,
                in_budget: 0,
                out_budget: 0,
            },
        );
        Ok(id)
    }

    pub fn place(&mut self, what: Placeable, x: i32, y: i32, dir: Dir) -> Result<(), &'static str> {
        match what {
            Placeable::Belt(t) => {
                self.can_place(what, x, y)?;
                self.place_belt(x, y, dir, t);
                Ok(())
            }
            Placeable::Building(k) => self.place_building(k, x, y, dir).map(|_| ()),
        }
    }

    /// Remove whatever is on a tile. Returns what was removed and its contents.
    pub fn remove(&mut self, x: i32, y: i32) -> Option<(Placeable, Inventory)> {
        let i = self.idx(x, y)?;
        if let Some(belt) = self.tiles[i].belt.take() {
            self.belts.remove(&(i as u32));
            let mut inv = Inventory::new();
            if let Some(item) = belt.item {
                inv.add(item, 1);
            }
            return Some((Placeable::Belt(belt.tier), inv));
        }
        let id = self.tiles[i].building?;
        let b = self.buildings.remove(&id)?;
        let (w, h) = b.spec_size();
        for ty in b.y..b.y + h {
            for tx in b.x..b.x + w {
                if let Some(j) = self.idx(tx, ty) {
                    self.tiles[j].building = None;
                }
            }
        }
        Some((Placeable::Building(b.kind), b.contents()))
    }

    /// Put one item on a belt. Returns false if there is no free belt there.
    pub fn add_item(&mut self, x: i32, y: i32, item: Item) -> bool {
        let Some(i) = self.idx(x, y) else {
            return false;
        };
        match &mut self.tiles[i].belt {
            Some(b) if b.item.is_none() => {
                b.item = Some(item);
                b.progress = 0;
                true
            }
            _ => false,
        }
    }

    /// Can `item` be dropped at (x, y) right now?
    fn can_drop(&self, x: i32, y: i32, item: Item, research: &Research) -> bool {
        let Some(t) = self.tile(x, y) else {
            return false;
        };
        if let Some(b) = &t.belt {
            return b.item.is_none();
        }
        match t.building.and_then(|id| self.buildings.get(&id)) {
            Some(b) => !b.kind.is_inserter() && b.accepts(item, research),
            None => false,
        }
    }

    /// Drop `item` at (x, y). Items dropped into depots go to the planet inventory.
    fn drop_item(
        &mut self,
        x: i32,
        y: i32,
        item: Item,
        research: &Research,
        out: &mut FactoryOutput,
    ) -> bool {
        if !self.can_drop(x, y, item, research) {
            return false;
        }
        let i = self.idx(x, y).expect("checked");
        if let Some(b) = &mut self.tiles[i].belt {
            b.item = Some(item);
            b.progress = 0;
            return true;
        }
        let id = self.tiles[i].building.expect("checked");
        let b = self.buildings.get_mut(&id).expect("checked");
        if b.kind == BuildingKind::Depot {
            bucket_spend(b.in_limit, &mut b.in_budget);
            out.delivered.add(item, 1);
        } else {
            b.insert(item);
        }
        true
    }

    /// Advance machines, inserters and belts by one tick. Power and staffing flags must
    /// already be set by the colony. Items dropped in depots are returned in `out`.
    pub fn step_factory(
        &mut self,
        tick: u64,
        research: &Research,
        inventory: &mut Inventory,
    ) -> FactoryOutput {
        let mut out = FactoryOutput::default();
        let ids: Vec<BuildingId> = self.buildings.keys().copied().collect();

        for b in self.buildings.values_mut() {
            bucket_refill(b.in_limit, &mut b.in_budget);
            bucket_refill(b.out_limit, &mut b.out_budget);
        }
        // Miners and crafting machines.
        for &id in &ids {
            let b = self.buildings.get_mut(&id).expect("id from map");
            if b.disabled_until > tick {
                b.status = Status::Damaged;
                continue;
            }
            if b.kind.is_miner() {
                step_miner(b, &self.richness);
            } else if b.kind.crafting().is_some() {
                step_crafter(b, research);
            }
        }
        // Miners push output forward.
        for &id in &ids {
            let b = &self.buildings[&id];
            if !b.kind.is_miner() {
                continue;
            }
            if !b.can_send_out() {
                continue;
            }
            let Some((item, _)) = b.output.iter().next() else {
                continue;
            };
            let (fx, fy) = b.front();
            if self.drop_item(fx, fy, item, research, &mut out) {
                let b = self.buildings.get_mut(&id).expect("id");
                b.output.take(item, 1);
                b.spent_out();
            }
        }
        // Splitters and mergers.
        for &id in &ids {
            if self.buildings[&id].kind.is_router() {
                self.step_router(id, tick, research, &mut out);
            }
        }
        // Inserters and providers.
        for &id in &ids {
            let kind = self.buildings[&id].kind;
            if kind.is_inserter() {
                self.step_inserter(id, tick, research, &mut out);
            } else if kind == BuildingKind::Provider {
                self.step_provider(id, research, inventory, &mut out);
            }
        }
        self.step_belts(research, &mut out);
        out
    }

    fn step_inserter(
        &mut self,
        id: BuildingId,
        tick: u64,
        research: &Research,
        out: &mut FactoryOutput,
    ) {
        let b = &self.buildings[&id];
        if b.disabled_until > tick {
            self.buildings.get_mut(&id).expect("id").status = Status::Damaged;
            return;
        }
        if !b.powered {
            self.buildings.get_mut(&id).expect("id").status = Status::NoPower;
            return;
        }
        let period = if b.kind == BuildingKind::FastInserter {
            10
        } else {
            26
        };
        if b.timer > 0 {
            let b = self.buildings.get_mut(&id).expect("id");
            b.timer -= 1;
            b.status = Status::Working;
            return;
        }
        let (fx, fy) = b.front();
        let (bx, by) = b.back();
        if let Some(item) = b.held {
            let dropped = self.drop_item(fx, fy, item, research, out);
            let b = self.buildings.get_mut(&id).expect("id");
            if dropped {
                b.held = None;
                b.timer = period / 2;
                b.status = Status::Working;
            } else {
                b.status = Status::OutputFull;
            }
            return;
        }
        // Pick up only what the target can take, so inserters never jam on wrong items.
        let mut picked = None;
        if let Some(si) = self.idx(bx, by) {
            if let Some(belt) = self.tiles[si].belt {
                if let Some(item) = belt.item
                    && self.can_drop(fx, fy, item, research)
                {
                    self.tiles[si].belt.as_mut().expect("belt").item = None;
                    picked = Some(item);
                }
            } else if let Some(src_id) = self.tiles[si].building {
                let src = &self.buildings[&src_id];
                if !src.kind.is_inserter()
                    && !src.kind.is_router()
                    && src_id != id
                    && src.can_send_out()
                {
                    let candidate = src
                        .output
                        .iter()
                        .map(|(i, _)| i)
                        .find(|&i| self.can_drop(fx, fy, i, research));
                    if let Some(item) = candidate {
                        let src = self.buildings.get_mut(&src_id).expect("src");
                        src.output.take(item, 1);
                        src.spent_out();
                        picked = Some(item);
                    }
                }
            }
        }
        let b = self.buildings.get_mut(&id).expect("id");
        match picked {
            Some(item) => {
                b.held = Some(item);
                b.timer = period / 2;
                b.status = Status::Working;
            }
            None => b.status = Status::Idle,
        }
    }

    /// Splitter: back in; front, left, right out in turn (filtered item only ahead).
    /// Merger: back and sides in; front out.
    fn step_router(
        &mut self,
        id: BuildingId,
        tick: u64,
        research: &Research,
        out: &mut FactoryOutput,
    ) {
        let b = &self.buildings[&id];
        if b.disabled_until > tick {
            return;
        }
        let Some((item, _)) = b.input.iter().next() else {
            self.buildings.get_mut(&id).expect("id").status = Status::Idle;
            return;
        };
        if !b.can_send_out() {
            self.buildings.get_mut(&id).expect("id").status = Status::Working;
            return;
        }
        let at = |d: Dir| {
            let (dx, dy) = d.delta();
            (b.x + dx, b.y + dy)
        };
        let fwd = b.dir;
        let mut exits: Vec<(i32, i32)> = Vec::with_capacity(3);
        if b.kind == BuildingKind::Merger {
            exits.push(at(fwd));
        } else {
            let sides = [at(fwd.rotate_cw().opposite()), at(fwd.rotate_cw())];
            match b.filter {
                Some(f) if f == item => exits.push(at(fwd)),
                Some(_) => exits.extend(sides),
                None => {
                    exits.push(at(fwd));
                    exits.extend(sides);
                }
            }
        }
        let start = b.timer as usize;
        let n = exits.len();
        for k in 0..n {
            let (x, y) = exits[(start + k) % n];
            if self.drop_item(x, y, item, research, out) {
                let b = self.buildings.get_mut(&id).expect("id");
                b.input.take(item, 1);
                b.spent_out();
                b.timer = ((start + k + 1) % n) as u32;
                b.status = Status::Working;
                return;
            }
        }
        self.buildings.get_mut(&id).expect("id").status = Status::OutputFull;
    }

    fn step_provider(
        &mut self,
        id: BuildingId,
        research: &Research,
        inventory: &mut Inventory,
        out: &mut FactoryOutput,
    ) {
        let b = &self.buildings[&id];
        if !b.powered {
            self.buildings.get_mut(&id).expect("id").status = Status::NoPower;
            return;
        }
        if b.timer > 0 {
            self.buildings.get_mut(&id).expect("id").timer -= 1;
            return;
        }
        let Some(item) = b.filter else {
            self.buildings.get_mut(&id).expect("id").status = Status::NoRecipe;
            return;
        };
        let (fx, fy) = b.front();
        let status = if inventory.get(item) == 0 {
            Status::MissingInputs
        } else if !b.can_send_out() {
            Status::Idle
        } else if self.drop_item(fx, fy, item, research, out) {
            inventory.take(item, 1);
            self.buildings.get_mut(&id).expect("id").spent_out();
            Status::Working
        } else {
            Status::OutputFull
        };
        let b = self.buildings.get_mut(&id).expect("id");
        b.status = status;
        b.timer = 6;
    }

    /// Move belt items. A full chain advances together; side-loads merge in index order.
    fn step_belts(&mut self, research: &Research, out: &mut FactoryOutput) {
        const UNSEEN: u8 = 0;
        const VISITING: u8 = 1;
        const MOVED: u8 = 2;
        const STAYED: u8 = 3;
        const CLAIMED: u8 = 4; // bit flag: this tile already received an item this tick

        let n = self.tiles.len();
        self.scratch.clear();
        self.scratch.resize(n, UNSEEN);

        for &i in &self.belts {
            let b = self.tiles[i as usize].belt.as_mut().expect("indexed belt");
            if b.item.is_some() && b.progress < BELT_LEN {
                b.progress = (b.progress + b.tier.speed()).min(BELT_LEN);
            }
        }

        let order: Vec<u32> = self.belts.iter().copied().collect();
        let mut stack: Vec<usize> = Vec::new();
        for start in order {
            stack.push(start as usize);
            while let Some(&i) = stack.last() {
                let state = self.scratch[i] & 3;
                if state == MOVED || state == STAYED {
                    stack.pop();
                    continue;
                }
                let belt = self.tiles[i].belt.expect("belt on stack");
                let Some(item) = belt.item.filter(|_| belt.progress >= BELT_LEN) else {
                    self.scratch[i] = (self.scratch[i] & CLAIMED) | STAYED;
                    stack.pop();
                    continue;
                };
                let (dx, dy) = belt.dir.delta();
                let (x, y) = (i as i32 % self.w + dx, i as i32 / self.w + dy);
                let Some(t) = self.idx(x, y) else {
                    self.scratch[i] = (self.scratch[i] & CLAIMED) | STAYED;
                    stack.pop();
                    continue;
                };
                let mut moved = false;
                if let Some(next) = self.tiles[t].belt {
                    let tstate = self.scratch[t] & 3;
                    if next.item.is_some() && tstate == UNSEEN && state == UNSEEN {
                        // Resolve the downstream tile first.
                        self.scratch[i] = (self.scratch[i] & CLAIMED) | VISITING;
                        stack.push(t);
                        continue;
                    }
                    let free = next.item.is_none() && self.scratch[t] & CLAIMED == 0;
                    if free {
                        self.tiles[t].belt.as_mut().expect("belt").item = Some(item);
                        self.tiles[t].belt.as_mut().expect("belt").progress = 0;
                        self.scratch[t] |= CLAIMED;
                        moved = true;
                    }
                } else if let Some(bid) = self.tiles[t].building {
                    let b = &self.buildings[&bid];
                    let from = (i as i32 % self.w, i as i32 / self.w);
                    // Routers only take items through their inlets.
                    let inlet = match b.kind {
                        BuildingKind::Splitter => from == b.back(),
                        BuildingKind::Merger => from != b.front(),
                        _ => true,
                    };
                    if inlet && !b.kind.is_inserter() && b.accepts(item, research) {
                        let b = self.buildings.get_mut(&bid).expect("building");
                        if b.kind == BuildingKind::Depot {
                            bucket_spend(b.in_limit, &mut b.in_budget);
                            out.delivered.add(item, 1);
                        } else {
                            b.insert(item);
                        }
                        moved = true;
                    }
                }
                if moved {
                    let b = self.tiles[i].belt.as_mut().expect("belt");
                    b.item = None;
                    b.progress = 0;
                }
                self.scratch[i] = (self.scratch[i] & CLAIMED) | if moved { MOVED } else { STAYED };
                stack.pop();
            }
        }
    }

    pub(crate) fn hash_into(&self, feed: &mut impl FnMut(&[u8])) {
        feed(&self.w.to_le_bytes());
        feed(&self.h.to_le_bytes());
        for &i in &self.belts {
            let b = self.tiles[i as usize].belt.expect("belt");
            feed(&i.to_le_bytes());
            feed(&[
                b.dir.code(),
                b.tier as u8,
                b.item.map(|x| x as u8 + 1).unwrap_or(0),
                b.progress,
            ]);
        }
        for b in self.buildings.values() {
            b.hash_into(feed);
        }
        for n in &self.nests {
            feed(&n.x.to_le_bytes());
            feed(&n.y.to_le_bytes());
            feed(&n.hp.to_le_bytes());
        }
    }

    /// Generate a surface for a planet. Deterministic from seed and planet id.
    pub fn generate(planet: &Planet, seed: u64, handcrafted_home: bool) -> Surface {
        let size = planet.surface_size();
        let h0 = mix(
            seed,
            &[
                0x5afe,
                planet.id.words()[0],
                planet.id.words()[1],
                planet.id.words()[2],
            ],
        );
        let mut s = Surface::new(size, size, h0);
        s.richness = planet.profile.clone();
        let c = size / 2;

        // Obstacles from coarse value noise, kept away from the landing zone.
        let cell = 6;
        let noise = |x: i32, y: i32| -> u64 {
            let (cx, cy) = (x.div_euclid(cell), y.div_euclid(cell));
            let (fx, fy) = (x.rem_euclid(cell) as u64, y.rem_euclid(cell) as u64);
            let v = |a: i32, b: i32| pick(mix(h0, &[a as u64, b as u64, 7]), 100);
            let top = v(cx, cy) * (cell as u64 - fx) + v(cx + 1, cy) * fx;
            let bot = v(cx, cy + 1) * (cell as u64 - fx) + v(cx + 1, cy + 1) * fx;
            (top * (cell as u64 - fy) + bot * fy) / (cell as u64 * cell as u64)
        };
        let obstacle = match planet.category {
            PlanetCategory::Volcanic => Terrain::Lava,
            _ => Terrain::Rock,
        };
        for y in 0..size {
            for x in 0..size {
                let near_center = (x - c).abs() < 9 && (y - c).abs() < 9;
                let edge = x == 0 || y == 0 || x == size - 1 || y == size - 1;
                if !near_center && (noise(x, y) > 74 || edge) {
                    s.set_terrain(x, y, obstacle);
                }
            }
        }

        let patch = |s: &mut Surface, item: Item, cx: i32, cy: i32, r: i32, salt: u64| {
            for y in cy - r..=cy + r {
                for x in cx - r..=cx + r {
                    let d2 = (x - cx).pow(2) + (y - cy).pow(2);
                    let jitter = pick(mix(h0, &[x as u64, y as u64, salt]), (r as u64) + 1) as i32;
                    if d2 <= r * r + jitter
                        && s.in_bounds(x, y)
                        && x > 0
                        && y > 0
                        && x < size - 1
                        && y < size - 1
                    {
                        s.set_terrain(x, y, Terrain::Deposit(item));
                    }
                }
            }
        };

        if handcrafted_home {
            let layout = [
                (Item::IronOre, -12, 9, 4),
                (Item::CopperOre, 12, 9, 4),
                (Item::Stone, -12, -10, 3),
                (Item::Carbon, 12, -10, 3),
                (Item::Water, 0, 17, 2),
                (Item::Gas, 2, -19, 2),
                (Item::IronOre, -22, 20, 3),
                (Item::CopperOre, 22, -22, 3),
            ];
            for (k, (item, dx, dy, r)) in layout.into_iter().enumerate() {
                patch(&mut s, item, c + dx, c + dy, r, k as u64);
            }
        } else {
            for (k, &(item, rich)) in planet.profile.iter().enumerate() {
                let patches = 1 + rich as u64;
                for p in 0..patches {
                    let ph = mix(h0, &[0x9a7c, k as u64, p]);
                    // The first patch of each resource is guaranteed near the landing zone.
                    let spread = if p == 0 { 14 } else { size as u64 - 12 };
                    let off = if p == 0 { 9 } else { 6 };
                    let ang = pick(ph, 8) as i32;
                    let (dx, dy) = [
                        (1, 0),
                        (1, 1),
                        (0, 1),
                        (-1, 1),
                        (-1, 0),
                        (-1, -1),
                        (0, -1),
                        (1, -1),
                    ][ang as usize];
                    let dist = off + pick(ph >> 8, spread / 2 + 1) as i32;
                    let (px, py) = if p == 0 {
                        (c + dx * dist, c + dy * dist)
                    } else {
                        (
                            6 + pick(ph >> 16, spread) as i32,
                            6 + pick(ph >> 32, spread) as i32,
                        )
                    };
                    patch(&mut s, item, px, py, 2 + rich as i32, ph);
                }
            }
            if planet.hostile {
                let nests = 2 + pick(h0 >> 3, 3) as i32;
                for k in 0..nests {
                    let nh = mix(h0, &[0x4e57, k as u64]);
                    let corner = [(1, 1), (-1, 1), (1, -1), (-1, -1)][(k % 4) as usize];
                    let d = 14 + pick(nh, (c - 16).max(1) as u64) as i32;
                    let (nx, ny) = (
                        c + corner.0 * d,
                        c + corner.1 * d / 2 + pick(nh >> 9, 6) as i32,
                    );
                    let (nx, ny) = (nx.clamp(3, size - 4), ny.clamp(3, size - 4));
                    for y in ny - 1..=ny + 1 {
                        for x in nx - 1..=nx + 1 {
                            s.set_terrain(x, y, Terrain::Ground);
                        }
                    }
                    s.nests.push(Nest {
                        x: nx,
                        y: ny,
                        hp: NEST_HP,
                    });
                }
            }
        }
        s
    }
}

fn step_miner(b: &mut Building, richness: &[(Item, u8)]) {
    let Some(item) = b.filter else {
        b.status = Status::Idle;
        return;
    };
    if b.output.total() >= 5 {
        b.status = Status::OutputFull;
        return;
    }
    if !b.powered {
        b.status = Status::NoPower;
        return;
    }
    let rich = richness
        .iter()
        .find(|(i, _)| *i == item)
        .map(|r| r.1)
        .unwrap_or(1) as u32;
    b.progress += b.speed() * rich;
    b.status = Status::Working;
    if b.progress >= 6000 {
        b.progress -= 6000;
        b.output.add(item, 1);
    }
}

fn step_crafter(b: &mut Building, research: &Research) {
    let Some(recipe) = b.recipe else {
        b.status = Status::NoRecipe;
        return;
    };
    let info = recipe.info();
    if let Some(t) = info.tech
        && !research.has(t)
    {
        b.status = Status::Locked;
        return;
    }
    if b.progress == 0 {
        let blocked = info
            .outputs
            .iter()
            .any(|&(i, n)| b.output.get(i) + n > (2 * n).max(10));
        if blocked {
            b.status = Status::OutputFull;
            return;
        }
        if !b.input.has_all(info.inputs) {
            b.status = Status::MissingInputs;
            return;
        }
        if !b.powered {
            b.status = Status::NoPower;
            return;
        }
        b.input.pay(info.inputs);
        b.progress = 1;
    } else if !b.powered {
        // Progress is preserved; resumes when power returns.
        b.status = Status::NoPower;
        return;
    }
    b.status = Status::Working;
    b.progress += b.speed();
    if b.progress > info.ticks * 100 {
        for &(i, n) in info.outputs {
            b.output.add(i, n);
        }
        b.progress = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(w: i32, h: i32) -> Surface {
        Surface::new(w, h, 1)
    }

    #[test]
    fn place_on_occupied_fails() {
        let mut s = flat(4, 4);
        assert!(s.place_belt(1, 1, Dir::E, BeltTier::Basic));
        assert!(!s.place_belt(1, 1, Dir::N, BeltTier::Basic));
        s.remove(1, 1);
        assert!(s.place_belt(1, 1, Dir::N, BeltTier::Basic));
        assert!(s.place_building(BuildingKind::Depot, 0, 0, Dir::N).is_err());
    }

    #[test]
    fn express_belt_moves_item_one_tile_per_tick() {
        let mut s = flat(4, 1);
        let r = Research::default();
        let mut inv = Inventory::new();
        for x in 0..3 {
            s.place_belt(x, 0, Dir::E, BeltTier::Express);
        }
        assert!(s.add_item(0, 0, Item::IronOre));
        s.step_factory(0, &r, &mut inv);
        assert!(s.tile(1, 0).unwrap().belt.unwrap().item.is_some());
        s.step_factory(1, &r, &mut inv);
        assert!(s.tile(2, 0).unwrap().belt.unwrap().item.is_some());
        s.step_factory(2, &r, &mut inv);
        // End of the line: the item stays.
        assert!(s.tile(2, 0).unwrap().belt.unwrap().item.is_some());
    }

    #[test]
    fn full_belt_advances_together() {
        let mut s = flat(6, 1);
        let r = Research::default();
        let mut inv = Inventory::new();
        for x in 0..5 {
            s.place_belt(x, 0, Dir::E, BeltTier::Express);
        }
        s.place_building(BuildingKind::Depot, 5, 0, Dir::N).ok();
        for x in 0..5 {
            s.add_item(x, 0, Item::Stone);
        }
        // Depot is 2x2 and doesn't fit in a 6x1 grid, so the chain is blocked at the end.
        s.step_factory(0, &r, &mut inv);
        assert_eq!(s.belts().filter(|b| b.2.item.is_some()).count(), 5);

        let mut s = flat(8, 2);
        for x in 0..5 {
            s.place_belt(x, 0, Dir::E, BeltTier::Express);
            s.add_item(x, 0, Item::Stone);
        }
        s.place_building(BuildingKind::Depot, 5, 0, Dir::N).unwrap();
        let out = s.step_factory(0, &r, &mut inv);
        assert_eq!(out.delivered.get(Item::Stone), 1);
        // Everything shifted one tile; the first tile is now empty.
        assert!(s.tile(0, 0).unwrap().belt.unwrap().item.is_none());
        assert!(s.tile(4, 0).unwrap().belt.unwrap().item.is_some());
    }

    #[test]
    fn miner_smelter_chain_produces_plates() {
        let mut s = flat(12, 6);
        s.richness = vec![(Item::IronOre, 3)];
        for y in 0..2 {
            for x in 0..2 {
                s.set_terrain(x, y, Terrain::Deposit(Item::IronOre));
            }
        }
        let mut r = Research::default();
        r.unlocked.clear();
        let miner = s.place_building(BuildingKind::Miner, 0, 0, Dir::E).unwrap();
        let smelter = s
            .place_building(BuildingKind::Smelter, 2, 0, Dir::N)
            .unwrap();
        let ins = s
            .place_building(BuildingKind::Inserter, 4, 0, Dir::E)
            .unwrap();
        s.place_building(BuildingKind::Depot, 5, 0, Dir::N).unwrap();
        let mut inv = Inventory::new();
        let mut delivered = 0;
        for t in 0..2000 {
            for id in [miner, smelter, ins] {
                s.buildings.get_mut(&id).unwrap().powered = true;
            }
            delivered += s
                .step_factory(t, &r, &mut inv)
                .delivered
                .get(Item::IronPlate);
        }
        assert!(delivered > 10, "delivered {delivered}");
    }

    #[test]
    fn unpowered_machine_keeps_progress() {
        let mut b = Building {
            id: 1,
            kind: BuildingKind::Assembler,
            x: 0,
            y: 0,
            dir: Dir::N,
            recipe: Some(Recipe::MakeGear),
            filter: None,
            input: Inventory::with(&[(Item::IronPlate, 2)]),
            output: Inventory::new(),
            progress: 0,
            timer: 0,
            held: None,
            modules: vec![],
            status: Status::Idle,
            powered: true,
            staffed: false,
            order: None,
            disabled_until: 0,
            in_limit: None,
            out_limit: None,
            in_budget: 0,
            out_budget: 0,
        };
        let r = Research::default();
        step_crafter(&mut b, &r);
        let p = b.progress;
        b.powered = false;
        step_crafter(&mut b, &r);
        assert_eq!(b.status, Status::NoPower);
        assert_eq!(b.progress, p);
        b.powered = true;
        for _ in 0..30 {
            step_crafter(&mut b, &r);
        }
        assert_eq!(b.output.get(Item::Gear), 1);
    }

    /// Splitter at (1,1) facing E, fed by a belt from the west.
    fn splitter_rig() -> (Surface, BuildingId) {
        let mut s = flat(4, 3);
        s.place_belt(0, 1, Dir::E, BeltTier::Express);
        let id = s
            .place_building(BuildingKind::Splitter, 1, 1, Dir::E)
            .unwrap();
        s.place_belt(2, 1, Dir::E, BeltTier::Basic);
        s.place_belt(1, 2, Dir::N, BeltTier::Basic);
        s.place_belt(1, 0, Dir::S, BeltTier::Basic);
        (s, id)
    }

    #[test]
    fn splitter_deals_items_to_all_three_exits() {
        let (mut s, _) = splitter_rig();
        let r = Research::default();
        let mut inv = Inventory::new();
        for t in 0..3 {
            s.add_item(0, 1, Item::IronOre);
            for k in 0..4 {
                s.step_factory(t * 4 + k, &r, &mut inv);
            }
        }
        for (x, y) in [(2, 1), (1, 2), (1, 0)] {
            assert!(
                s.tile(x, y).unwrap().belt.unwrap().item.is_some(),
                "exit {x},{y}"
            );
        }
    }

    #[test]
    fn splitter_filter_sends_match_ahead() {
        let (mut s, id) = splitter_rig();
        s.buildings.get_mut(&id).unwrap().filter = Some(Item::CopperOre);
        let r = Research::default();
        let mut inv = Inventory::new();
        s.add_item(0, 1, Item::IronOre);
        for t in 0..4 {
            s.step_factory(t, &r, &mut inv);
        }
        assert!(s.tile(2, 1).unwrap().belt.unwrap().item.is_none());
    }

    #[test]
    fn merger_joins_side_inputs() {
        let mut s = flat(3, 3);
        s.place_building(BuildingKind::Merger, 1, 1, Dir::E)
            .unwrap();
        s.place_belt(1, 0, Dir::N, BeltTier::Express);
        s.place_belt(1, 2, Dir::S, BeltTier::Express);
        s.place_belt(2, 1, Dir::E, BeltTier::Basic);
        s.add_item(1, 0, Item::IronOre);
        s.add_item(1, 2, Item::IronOre);
        let r = Research::default();
        let mut inv = Inventory::new();
        let mut through = 0;
        for t in 0..60 {
            s.step_factory(t, &r, &mut inv);
            let i = s.idx(2, 1).unwrap();
            if let Some(b) = s.tiles[i].belt.as_mut()
                && b.item.take().is_some()
            {
                through += 1;
            }
        }
        assert_eq!(through, 2);
    }

    #[test]
    fn output_limit_caps_items_per_minute() {
        let mut s = flat(6, 6);
        let id = s
            .place_building(BuildingKind::Provider, 0, 0, Dir::E)
            .unwrap();
        s.place_building(BuildingKind::Depot, 1, 0, Dir::N).unwrap();
        let b = s.buildings.get_mut(&id).unwrap();
        b.filter = Some(Item::IronPlate);
        b.set_limits(None, Some(30));
        let r = Research::default();
        let mut inv = Inventory::new();
        inv.add(Item::IronPlate, 1000);
        let mut delivered = 0;
        for t in 0..RATE_UNIT as u64 {
            s.buildings.get_mut(&id).unwrap().powered = true;
            delivered += s
                .step_factory(t, &r, &mut inv)
                .delivered
                .get(Item::IronPlate);
        }
        assert!((29..=31).contains(&delivered), "delivered {delivered}");
    }
}
