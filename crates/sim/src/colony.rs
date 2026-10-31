//! Colonies: local inventory, population, power and the human-dependent work systems.

use serde::{Deserialize, Serialize};

use crate::buildings::{BuildingKind, Dir};
use crate::items::{Inventory, Item};
use crate::research::Research;
use crate::ships::ShipOrder;
use crate::surface::{Status, Surface};
use crate::universe::{mix, pick};

/// Ticks of full output one fuel item gives a generator or reactor.
fn fuel_ticks(kind: BuildingKind, item: Item) -> u32 {
    match (kind, item) {
        (BuildingKind::Generator, Item::Carbon) => 400,
        (BuildingKind::Generator, Item::Fuel) => 1000,
        (BuildingKind::Reactor, Item::FuelCell) => 4000,
        _ => 0,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Policy {
    /// Turrets attack nests; nests attack anywhere in reach.
    Eliminate,
    /// Turrets only defend; nests only raid buildings close to them, and less often.
    Coexist,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PowerStats {
    /// kW actually produced this tick.
    pub produced: u32,
    /// kW that could be produced with current fuel.
    pub capacity: u32,
    /// kW requested by consumers.
    pub demand: u32,
    /// Positive: received from other planets. Negative: sent.
    pub transfer: i32,
    pub shortage: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Colony {
    pub inventory: Inventory,
    pub population: u32,
    /// Work systems established in person by the player.
    pub active: bool,
    pub policy: Policy,
    pub growth: u32,
    pub power: PowerStats,
    pub founded: u64,
    /// Lifetime research generated here.
    pub research_total: u64,
    /// Production counters for the last minute window (stats).
    pub delivered: Inventory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColonyEvent {
    PopulationGrew,
    Abandoned,
    PowerShortage,
    Attack { repelled: bool, killed: bool },
    NestDestroyed,
    ShipBuilt(ShipOrder),
    Scan(u8),
    BeaconCharged,
}

#[derive(Debug, Default)]
pub struct ColonyOutput {
    pub research: u32,
    pub events: Vec<ColonyEvent>,
}

impl Colony {
    pub fn new(tick: u64, inventory: Inventory, population: u32, active: bool) -> Self {
        Colony {
            inventory,
            population,
            active,
            policy: Policy::Coexist,
            growth: 0,
            power: PowerStats::default(),
            founded: tick,
            research_total: 0,
            delivered: Inventory::new(),
        }
    }

    pub fn housing(surface: &Surface) -> u32 {
        surface.buildings.values().map(|b| b.kind.housing()).sum()
    }

    pub fn workers_needed(surface: &Surface) -> u32 {
        surface
            .buildings
            .values()
            .map(|b| b.kind.spec().workers)
            .sum()
    }

    /// Generation available (kW) and demand (kW), used to plan transmission.
    pub fn power_balance(surface: &Surface) -> (u32, u32, u32) {
        let mut cap = 0;
        let mut demand = 0;
        let mut transmitters = 0;
        for b in surface.buildings.values() {
            match b.kind {
                BuildingKind::Solar => cap += b.kind.spec().power_gen,
                BuildingKind::Generator | BuildingKind::Reactor => {
                    if b.timer > 0 || !b.input.is_empty() {
                        cap += b.kind.spec().power_gen;
                    }
                }
                BuildingKind::Transmitter => transmitters += 1,
                _ => {}
            }
            if b.wants_power() {
                demand += b.power_draw();
            }
        }
        (cap, demand, transmitters)
    }

    /// Place the prefab landing hub that a colony kit deploys.
    pub fn deploy_kit(surface: &mut Surface) {
        let c = surface.w / 2;
        let _ = surface.place_building(BuildingKind::Depot, c - 1, c - 1, Dir::N);
        let _ = surface.place_building(BuildingKind::House, c + 2, c - 1, Dir::N);
        let _ = surface.place_building(BuildingKind::Solar, c - 4, c - 1, Dir::N);
        let _ = surface.place_building(BuildingKind::Solar, c - 4, c + 2, Dir::N);
    }

    /// One tick of colony life. `transfer` is power received (+) or sent (-) in kW.
    pub fn step(
        &mut self,
        surface: &mut Surface,
        tick: u64,
        seed: u64,
        research: &Research,
        transfer: i32,
    ) -> ColonyOutput {
        let mut out = ColonyOutput::default();
        self.step_power(surface, tick, transfer, &mut out);
        self.step_staff(surface);

        let f = surface.step_factory(tick, research, &mut self.inventory);
        for (i, n) in f.delivered.iter() {
            self.delivered.add(i, n);
        }
        self.inventory.merge(f.delivered);

        self.step_work(surface, tick, &mut out);
        self.step_people(surface, &mut out);
        if !surface.nests.is_empty() {
            self.step_hostiles(surface, tick, seed, &mut out);
        }
        self.research_total += out.research as u64;
        out
    }

    fn step_power(
        &mut self,
        surface: &mut Surface,
        tick: u64,
        transfer: i32,
        out: &mut ColonyOutput,
    ) {
        let mut demand: u32 = 0;
        for b in surface.buildings.values() {
            if b.wants_power() && b.disabled_until <= tick {
                demand += b.power_draw();
            }
        }
        let mut available: i64 = transfer as i64;
        let mut capacity: u32 = 0;
        for b in surface.buildings.values_mut() {
            if b.kind == BuildingKind::Solar {
                available += b.kind.spec().power_gen as i64;
                capacity += b.kind.spec().power_gen;
                b.status = Status::Working;
            }
        }
        // Burn fuel only while it is needed.
        let need = demand as i64;
        for b in surface.buildings.values_mut() {
            if !matches!(b.kind, BuildingKind::Generator | BuildingKind::Reactor) {
                continue;
            }
            let output_kw = b.kind.spec().power_gen;
            let first = b.input.iter().next().map(|(i, _)| i);
            if b.timer == 0
                && let Some(item) = first
            {
                b.input.take(item, 1);
                b.timer = fuel_ticks(b.kind, item);
            }
            if b.timer == 0 {
                b.status = Status::NoFuel;
                continue;
            }
            capacity += output_kw;
            if available < need {
                available += output_kw as i64;
                b.timer -= 1;
                b.status = Status::Working;
            } else {
                b.status = Status::Idle;
            }
        }
        let produced = available.max(0) as u32;
        let mut remaining = available;
        let mut short = false;
        for b in surface.buildings.values_mut() {
            if b.kind.spec().power == 0 {
                b.powered = true;
                continue;
            }
            if !b.wants_power() {
                b.powered = remaining > 0;
                continue;
            }
            let draw = b.power_draw() as i64;
            if draw <= remaining {
                remaining -= draw;
                b.powered = true;
            } else {
                b.powered = false;
                short = true;
            }
        }
        if short && !self.power.shortage {
            out.events.push(ColonyEvent::PowerShortage);
        }
        self.power = PowerStats {
            produced,
            capacity,
            demand,
            transfer,
            shortage: short,
        };
    }

    fn step_staff(&mut self, surface: &mut Surface) {
        let mut free = if self.active { self.population } else { 0 };
        for b in surface.buildings.values_mut() {
            let w = b.kind.spec().workers;
            if w == 0 {
                continue;
            }
            b.staffed = self.active && free >= w;
            if b.staffed {
                free -= w;
            }
        }
    }

    fn step_work(&mut self, surface: &mut Surface, tick: u64, out: &mut ColonyOutput) {
        let active = self.active;
        let mut pop = self.population;
        for b in surface.buildings.values_mut() {
            if b.kind.spec().workers == 0
                && !matches!(b.kind, BuildingKind::Turret | BuildingKind::Beacon)
            {
                continue;
            }
            if b.disabled_until > tick {
                b.status = Status::Damaged;
                continue;
            }
            let ready = |b: &crate::surface::Building| -> Option<Status> {
                if b.kind.spec().workers > 0 && !active {
                    Some(Status::Inactive)
                } else if b.kind.spec().workers > 0 && !b.staffed {
                    Some(Status::NoWorkers)
                } else if !b.powered {
                    Some(Status::NoPower)
                } else {
                    None
                }
            };
            match b.kind {
                BuildingKind::Lab | BuildingKind::LabMk2 => {
                    if let Some(s) = ready(b) {
                        b.status = s;
                        continue;
                    }
                    b.status = Status::Working;
                    b.timer += 1;
                    if b.timer >= 20 {
                        b.timer = 0;
                        out.research += if b.kind == BuildingKind::LabMk2 { 4 } else { 1 };
                    }
                }
                BuildingKind::Observatory => {
                    if let Some(s) = ready(b) {
                        b.status = s;
                        continue;
                    }
                    b.status = Status::Working;
                    b.timer += 1;
                    if b.timer >= 600 {
                        b.timer = 0;
                        out.events.push(ColonyEvent::Scan(0));
                    }
                }
                BuildingKind::Shipyard => {
                    let Some(order) = b.order else {
                        b.status = Status::Idle;
                        continue;
                    };
                    if let Some(s) = ready(b) {
                        b.status = s;
                        continue;
                    }
                    if b.progress < order.hull.build_ticks() {
                        b.progress += 1;
                        b.status = Status::Working;
                    } else if pop >= order.hull.crew() {
                        pop -= order.hull.crew();
                        b.progress = 0;
                        b.order = None;
                        b.status = Status::Idle;
                        out.events.push(ColonyEvent::ShipBuilt(order));
                    } else {
                        b.status = Status::NoWorkers;
                    }
                }
                BuildingKind::Turret => {
                    b.status = if b.powered {
                        Status::Working
                    } else {
                        Status::NoPower
                    };
                }
                BuildingKind::Beacon => {
                    if b.powered {
                        b.status = Status::Working;
                        b.timer += 1;
                        if b.timer == 1200 {
                            out.events.push(ColonyEvent::BeaconCharged);
                        }
                    } else {
                        b.status = Status::NoPower;
                    }
                }
                _ => {}
            }
        }
        self.population = pop;
    }

    fn step_people(&mut self, surface: &Surface, out: &mut ColonyOutput) {
        let housing = Self::housing(surface);
        if self.population >= 2 && self.population < housing {
            self.growth += 1;
            if self.growth >= 900 {
                self.growth = 0;
                self.population += 1;
                out.events.push(ColonyEvent::PopulationGrew);
            }
        }
        if self.population == 0 && self.active {
            self.active = false;
            out.events.push(ColonyEvent::Abandoned);
        }
    }

    fn step_hostiles(
        &mut self,
        surface: &mut Surface,
        tick: u64,
        seed: u64,
        out: &mut ColonyOutput,
    ) {
        let turrets: Vec<(i32, i32)> = surface
            .buildings
            .values()
            .filter(|b| b.kind == BuildingKind::Turret && b.powered && b.disabled_until <= tick)
            .map(|b| (b.x, b.y))
            .collect();
        let in_range = |(ax, ay): (i32, i32), (bx, by): (i32, i32), r: i32| {
            (ax - bx).pow(2) + (ay - by).pow(2) <= r * r
        };

        if self.policy == Policy::Eliminate {
            for t in &turrets {
                if let Some(n) = surface
                    .nests
                    .iter_mut()
                    .filter(|n| in_range(*t, (n.x, n.y), 7))
                    .min_by_key(|n| ((n.x - t.0).pow(2) + (n.y - t.1).pow(2), n.x, n.y))
                {
                    n.hp = n.hp.saturating_sub(3);
                }
            }
            let before = surface.nests.len();
            surface.nests.retain(|n| n.hp > 0);
            for _ in surface.nests.len()..before {
                out.events.push(ColonyEvent::NestDestroyed);
            }
        }
        for n in &mut surface.nests {
            n.hp = (n.hp + 1).min(crate::surface::NEST_HP);
        }

        let (reach, chance) = match self.policy {
            Policy::Eliminate => (20, 100),
            Policy::Coexist => (10, 35),
        };
        for (k, n) in surface.nests.iter().enumerate() {
            if !(tick + 300 * k as u64).is_multiple_of(1500) || tick == 0 {
                continue;
            }
            if pick(mix(seed, &[tick, k as u64, 0xa77a]), 100) >= chance {
                continue;
            }
            let target = surface
                .buildings
                .values()
                .filter(|b| in_range((b.x, b.y), (n.x, n.y), reach))
                .min_by_key(|b| ((b.x - n.x).pow(2) + (b.y - n.y).pow(2), b.id))
                .map(|b| b.id);
            let Some(tid) = target else {
                continue;
            };
            let tb = &surface.buildings[&tid];
            let repelled = turrets.iter().any(|t| in_range(*t, (tb.x, tb.y), 7));
            let mut killed = false;
            if !repelled {
                if tb.kind.housing() > 0 && self.population > 0 {
                    self.population -= 1;
                    killed = true;
                }
                surface
                    .buildings
                    .get_mut(&tid)
                    .expect("target")
                    .disabled_until = tick + 900;
            }
            out.events.push(ColonyEvent::Attack { repelled, killed });
        }
    }

    pub(crate) fn hash_into(&self, feed: &mut impl FnMut(&[u8])) {
        self.inventory.hash_into(feed);
        feed(&self.population.to_le_bytes());
        feed(&[self.active as u8, self.policy as u8]);
        feed(&self.growth.to_le_bytes());
    }
}
