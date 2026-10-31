//! Deterministic simulation core. No rendering or Bevy dependencies.
//!
//! Rules: integer state only, seeded RNG/hashes only, ordered collections only.
//! The game talks to the sim only through [`Command`]s and reads it through accessors.

pub mod buildings;
pub mod colony;
pub mod items;
pub mod recipes;
pub mod research;
pub mod ships;
pub mod surface;
pub mod universe;

use std::collections::BTreeMap;

use rand_core::SeedableRng;
use rand_pcg::Pcg64Mcg;
use serde::{Deserialize, Serialize};

pub use buildings::{BeltTier, BuildingKind, Dir, Placeable};
pub use colony::{Colony, ColonyEvent, Policy};
pub use items::{Inventory, Item};
pub use recipes::Recipe;
pub use research::{Research, Tech};
pub use ships::{Drive, Hull, Route, Ship, ShipLoc, ShipOrder};
pub use surface::{BuildingId, Status, Surface, tile_id};
pub use universe::{HOME, Planet, PlanetId, SystemId, SystemKnowledge, Universe};

/// Simulation ticks per second of game time at 1x speed.
pub const TICKS_PER_SEC: u64 = 20;

/// Fixed-point scalar: 16.16 format, for any fractional sim value.
pub type Fixed = i32;
pub const FIXED_ONE: Fixed = 1 << 16;

/// Everything the player can do. Input code builds these; the sim validates them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    Place {
        planet: PlanetId,
        x: i32,
        y: i32,
        what: Placeable,
        dir: Dir,
    },
    Remove {
        planet: PlanetId,
        x: i32,
        y: i32,
    },
    SetRecipe {
        planet: PlanetId,
        building: BuildingId,
        recipe: Option<Recipe>,
    },
    SetFilter {
        planet: PlanetId,
        building: BuildingId,
        item: Option<Item>,
    },
    /// Cap items per minute in / out; `None` removes the cap.
    SetRateLimits {
        planet: PlanetId,
        building: BuildingId,
        input: Option<u32>,
        output: Option<u32>,
    },
    InsertModule {
        planet: PlanetId,
        building: BuildingId,
        module: Item,
    },
    SelectResearch(Tech),
    OrderShip {
        planet: PlanetId,
        building: BuildingId,
        order: ShipOrder,
    },
    Load {
        ship: u32,
        item: Item,
        qty: u32,
    },
    Unload {
        ship: u32,
        item: Item,
        qty: u32,
    },
    Board {
        ship: u32,
        people: u32,
    },
    Disembark {
        ship: u32,
        people: u32,
    },
    Launch {
        ship: u32,
        to: PlanetId,
    },
    SetSurvey {
        ship: u32,
        on: bool,
    },
    FoundColony {
        ship: u32,
    },
    /// Only valid while the player is visiting the planet in person.
    ActivateColony {
        planet: PlanetId,
    },
    SetRoute {
        ship: u32,
        route: Option<Route>,
    },
    SetPolicy {
        planet: PlanetId,
        policy: Policy,
    },
    /// The overseer boards a docked ship at their location (None: stay on the planet).
    Ride {
        ship: Option<u32>,
    },
}

/// Where the invulnerable overseer is. Colonies must be visited in person to activate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Player {
    pub at: Option<PlanetId>,
    pub riding: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    ResearchComplete(Tech),
    ShipBuilt {
        ship: u32,
        planet: PlanetId,
    },
    ShipArrived {
        ship: u32,
        planet: PlanetId,
    },
    SurveyComplete(PlanetId),
    SystemsDetected(u32),
    /// A new location was procedurally generated (never on revisits).
    Generated {
        what: &'static str,
        name: String,
    },
    ColonyFounded(PlanetId),
    ColonyActivated(PlanetId),
    ColonyAbandoned(PlanetId),
    PopulationGrew(PlanetId),
    PowerShortage(PlanetId),
    Attack {
        planet: PlanetId,
        repelled: bool,
        killed: bool,
    },
    NestDestroyed(PlanetId),
    Victory,
}

/// A colony in a power network: (planet, net surplus kW, transmitter limit kW).
type Link = (PlanetId, i64, i64);

#[derive(Debug, Serialize, Deserialize)]
pub struct Sim {
    seed: u64,
    tick: u64,
    rng: Pcg64Mcg,
    pub universe: Universe,
    pub research: Research,
    pub ships: BTreeMap<u32, Ship>,
    next_ship: u32,
    pub victory: Option<u64>,
    pub player: Player,
    #[serde(skip)]
    events: Vec<Event>,
}

fn starting_inventory() -> Inventory {
    use Item::*;
    Inventory::with(&[
        (IronPlate, 400),
        (Gear, 100),
        (Wire, 120),
        (Circuit, 60),
        (Stone, 200),
        (Carbon, 100),
        (Glass, 30),
        (Pipe, 30),
        (CopperPlate, 50),
    ])
}

impl Sim {
    pub fn new(seed: u64) -> Self {
        let mut sim = Self {
            seed,
            tick: 0,
            rng: Pcg64Mcg::seed_from_u64(seed),
            universe: Universe::new(seed),
            research: Research::default(),
            ships: BTreeMap::new(),
            next_ship: 1,
            victory: None,
            player: Player {
                at: Some(HOME),
                riding: None,
            },
            events: Vec::new(),
        };
        let home = sim.universe.planet_mut(HOME).expect("home planet");
        home.survey = 100;
        home.scan_level = 4;
        let mut surface = Surface::generate(home, seed, true);
        let c = surface.w / 2;
        let _ = surface.place_building(BuildingKind::Depot, c - 1, c - 1, Dir::N);
        let _ = surface.place_building(BuildingKind::House, c + 3, c + 2, Dir::N);
        let _ = surface.place_building(BuildingKind::House, c + 3, c - 1, Dir::N);
        let _ = surface.place_building(BuildingKind::House, c + 3, c - 4, Dir::N);
        if let Ok(g) = surface.place_building(BuildingKind::Generator, c - 5, c - 1, Dir::N) {
            surface
                .buildings
                .get_mut(&g)
                .expect("generator")
                .input
                .add(Item::Carbon, 5);
        }
        if let Ok(p) = surface.place_building(BuildingKind::Provider, c - 3, c - 1, Dir::W) {
            surface.buildings.get_mut(&p).expect("provider").filter = Some(Item::Carbon);
        }
        home.surface = Some(surface);
        home.colony = Some(Colony::new(0, starting_inventory(), 8, true));
        sim
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    pub fn tick(&self) -> u64 {
        self.tick
    }

    pub fn planet(&self, id: PlanetId) -> Option<&Planet> {
        self.universe.planet(id)
    }

    pub fn surface(&self, id: PlanetId) -> Option<&Surface> {
        self.universe.planet(id)?.surface.as_ref()
    }

    pub fn colony(&self, id: PlanetId) -> Option<&Colony> {
        self.universe.planet(id)?.colony.as_ref()
    }

    pub fn colonies(&self) -> impl Iterator<Item = &Planet> {
        self.universe.planets().filter(|p| p.colony.is_some())
    }

    /// Drain events produced since the last call.
    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    /// Ticks until the given ship arrives, for Instant-Warp.
    pub fn ticks_until_arrival(&self, ship: u32) -> Option<u64> {
        match self.ships.get(&ship)?.loc {
            ShipLoc::Transit { arrive, .. } => Some(arrive.saturating_sub(self.tick)),
            ShipLoc::Docked(_) => None,
        }
    }

    /// Convenience for tests and benchmarks: place a basic belt on the home planet.
    pub fn place_belt(&mut self, x: i32, y: i32, dir: Dir) -> bool {
        let s = self
            .universe
            .planet_mut(HOME)
            .and_then(|p| p.surface.as_mut())
            .expect("home surface");
        s.place_belt(x, y, dir, BeltTier::Basic)
    }

    fn surface_colony_mut(&mut self, id: PlanetId) -> Result<(&mut Surface, &mut Colony), String> {
        let p = self
            .universe
            .planet_mut(id)
            .ok_or_else(|| "Unknown planet".to_string())?;
        match (p.surface.as_mut(), p.colony.as_mut()) {
            (Some(s), Some(c)) => Ok((s, c)),
            _ => Err("No colony on this planet".into()),
        }
    }

    fn unlocked(&self, t: Option<Tech>) -> bool {
        t.is_none_or(|t| self.research.has(t))
    }

    /// Validate and apply a command.
    pub fn apply(&mut self, cmd: Command) -> Result<(), String> {
        match cmd {
            Command::Place {
                planet,
                x,
                y,
                what,
                dir,
            } => {
                if !self.unlocked(what.tech()) {
                    return Err(format!("{} is not researched yet", what.name()));
                }
                if what == Placeable::Building(BuildingKind::Beacon) && planet.galaxy == 0 {
                    return Err("The frontier beacon must be built in another galaxy".into());
                }
                let (surface, colony) = self.surface_colony_mut(planet)?;
                surface.can_place(what, x, y).map_err(str::to_string)?;
                if !colony.inventory.pay(what.cost()) {
                    return Err(format!("Not enough materials for {}", what.name()));
                }
                surface.place(what, x, y, dir).map_err(str::to_string)
            }
            Command::Remove { planet, x, y } => {
                let (surface, colony) = self.surface_colony_mut(planet)?;
                if let Some(b) = surface.building_at(x, y)
                    && b.kind == BuildingKind::Shipyard
                    && let Some(order) = b.order
                {
                    colony.inventory.merge(order.cost());
                }
                let (what, contents) = surface.remove(x, y).ok_or("Nothing to remove")?;
                colony.inventory.merge(Inventory::with(what.cost()));
                colony.inventory.merge(contents);
                Ok(())
            }
            Command::SetRecipe {
                planet,
                building,
                recipe,
            } => {
                if let Some(r) = recipe
                    && !self.unlocked(r.info().tech)
                {
                    return Err("Recipe not researched".into());
                }
                let (surface, colony) = self.surface_colony_mut(planet)?;
                let b = surface
                    .buildings
                    .get_mut(&building)
                    .ok_or("No such building")?;
                let cat = b.kind.crafting().ok_or("Not a crafting machine")?.0;
                if let Some(r) = recipe
                    && r.info().category != cat
                {
                    return Err("This machine cannot make that".into());
                }
                if b.recipe != recipe {
                    // Return inputs and any in-progress materials to the planet.
                    if b.progress > 0
                        && let Some(old) = b.recipe
                    {
                        colony.inventory.merge(Inventory::with(old.info().inputs));
                    }
                    colony.inventory.merge(std::mem::take(&mut b.input));
                    b.progress = 0;
                    b.recipe = recipe;
                }
                Ok(())
            }
            Command::SetFilter {
                planet,
                building,
                item,
            } => {
                let (surface, _) = self.surface_colony_mut(planet)?;
                let b = surface
                    .buildings
                    .get_mut(&building)
                    .ok_or("No such building")?;
                if !matches!(b.kind, BuildingKind::Provider | BuildingKind::Splitter) {
                    return Err("Only providers and splitters take a filter".into());
                }
                b.filter = item;
                Ok(())
            }
            Command::SetRateLimits {
                planet,
                building,
                input,
                output,
            } => {
                let (surface, _) = self.surface_colony_mut(planet)?;
                let b = surface
                    .buildings
                    .get_mut(&building)
                    .ok_or("No such building")?;
                if !b.kind.has_rate_limits() {
                    return Err("This building has no rate controls".into());
                }
                b.set_limits(input, output);
                Ok(())
            }
            Command::InsertModule {
                planet,
                building,
                module,
            } => {
                if !matches!(module, Item::SpeedModule | Item::EfficiencyModule) {
                    return Err("Not a module".into());
                }
                let (surface, colony) = self.surface_colony_mut(planet)?;
                let b = surface
                    .buildings
                    .get_mut(&building)
                    .ok_or("No such building")?;
                if !b.kind.takes_modules() {
                    return Err("This building has no module slots".into());
                }
                if b.modules.len() >= 2 {
                    return Err("Module slots are full".into());
                }
                if colony.inventory.take(module, 1) == 0 {
                    return Err("No module in the planet inventory".into());
                }
                b.modules.push(module);
                Ok(())
            }
            Command::SelectResearch(t) => {
                if self.research.select(t) {
                    Ok(())
                } else {
                    Err("Prerequisites not met".into())
                }
            }
            Command::OrderShip {
                planet,
                building,
                order,
            } => {
                if !self.research.has(order.hull.tech()) || !self.unlocked(order.drive.tech()) {
                    return Err("Ship design not researched".into());
                }
                let (surface, colony) = self.surface_colony_mut(planet)?;
                let b = surface
                    .buildings
                    .get_mut(&building)
                    .ok_or("No such building")?;
                if b.kind != BuildingKind::Shipyard {
                    return Err("Not a shipyard".into());
                }
                if b.order.is_some() {
                    return Err("Shipyard is busy".into());
                }
                let cost: Vec<(Item, u32)> = order.cost().iter().collect();
                if !colony.inventory.pay(&cost) {
                    return Err("Not enough materials for this ship".into());
                }
                b.order = Some(order);
                b.progress = 0;
                Ok(())
            }
            Command::Load { ship, item, qty } => {
                let s = self.ships.get(&ship).ok_or("No such ship")?;
                let at = s.docked_at().ok_or("Ship is in transit")?;
                let qty = qty.min(s.free_cargo());
                let colony = self
                    .universe
                    .planet_mut(at)
                    .and_then(|p| p.colony.as_mut())
                    .ok_or("No inventory here")?;
                let n = colony.inventory.take(item, qty);
                if n == 0 {
                    return Err("Nothing to load".into());
                }
                self.ships.get_mut(&ship).expect("ship").cargo.add(item, n);
                Ok(())
            }
            Command::Unload { ship, item, qty } => {
                let s = self.ships.get_mut(&ship).ok_or("No such ship")?;
                let at = s.docked_at().ok_or("Ship is in transit")?;
                let colony = self
                    .universe
                    .planet_mut(at)
                    .and_then(|p| p.colony.as_mut())
                    .ok_or("No colony here to receive cargo")?;
                let n = s.cargo.take(item, qty);
                colony.inventory.add(item, n);
                Ok(())
            }
            Command::Board { ship, people } => {
                let s = self.ships.get(&ship).ok_or("No such ship")?;
                let at = s.docked_at().ok_or("Ship is in transit")?;
                let seats = s.hull.seats() - s.passengers;
                let colony = self
                    .universe
                    .planet_mut(at)
                    .and_then(|p| p.colony.as_mut())
                    .ok_or("No people here")?;
                let n = people.min(seats).min(colony.population);
                if n == 0 {
                    return Err("No free seats or no people".into());
                }
                colony.population -= n;
                self.ships.get_mut(&ship).expect("ship").passengers += n;
                Ok(())
            }
            Command::Disembark { ship, people } => {
                let s = self.ships.get_mut(&ship).ok_or("No such ship")?;
                let at = s.docked_at().ok_or("Ship is in transit")?;
                let colony = self
                    .universe
                    .planet_mut(at)
                    .and_then(|p| p.colony.as_mut())
                    .ok_or("No colony here")?;
                let n = people.min(s.passengers);
                s.passengers -= n;
                colony.population += n;
                Ok(())
            }
            Command::Launch { ship, to } => self.launch(ship, to),
            Command::SetSurvey { ship, on } => {
                let s = self.ships.get_mut(&ship).ok_or("No such ship")?;
                if on && s.hull != Hull::Scout {
                    return Err("Only scouts carry survey modules".into());
                }
                s.surveying = on;
                Ok(())
            }
            Command::FoundColony { ship } => self.found_colony(ship),
            Command::ActivateColony { planet } => {
                if self.player.at != Some(planet) {
                    return Err(
                        "You must visit the colony in person to establish work systems".into(),
                    );
                }
                let p = self.universe.planet_mut(planet).ok_or("Unknown planet")?;
                let c = p.colony.as_mut().ok_or("No colony here")?;
                if c.population == 0 {
                    return Err("Nobody lives here: bring people first".into());
                }
                if !c.active {
                    c.active = true;
                    self.events.push(Event::ColonyActivated(planet));
                }
                Ok(())
            }
            Command::SetRoute { ship, route } => {
                if route.is_some() && !self.research.has(Tech::LogisticsAutomation) {
                    return Err("Automated routes need Logistics automation".into());
                }
                let s = self.ships.get_mut(&ship).ok_or("No such ship")?;
                s.route = route;
                s.wait = 0;
                Ok(())
            }
            Command::SetPolicy { planet, policy } => {
                let (_, c) = self.surface_colony_mut(planet)?;
                c.policy = policy;
                Ok(())
            }
            Command::Ride { ship } => {
                let here = self.player.at.ok_or("You are in transit")?;
                if let Some(id) = ship {
                    let s = self.ships.get(&id).ok_or("No such ship")?;
                    if s.docked_at() != Some(here) {
                        return Err("That ship is not docked where you are".into());
                    }
                }
                self.player.riding = ship;
                Ok(())
            }
        }
    }

    fn launch(&mut self, ship: u32, to: PlanetId) -> Result<(), String> {
        let s = self.ships.get(&ship).ok_or("No such ship")?;
        let from = s.docked_at().ok_or("Ship is already in transit")?;
        let dest_sys = self
            .universe
            .system(to.system())
            .ok_or("Unknown destination")?;
        if dest_sys.knowledge == SystemKnowledge::Unknown {
            return Err("Destination has not been discovered".into());
        }
        let plan = s.plan_trip(&self.universe, from, to)?;
        let (fuel, need) = plan.fuel;
        // Fuel comes from the ship's own cargo first, then the local planet inventory.
        let in_cargo = s.cargo.get(fuel);
        let local = self
            .universe
            .planet(from)
            .and_then(|p| p.colony.as_ref())
            .map(|c| c.inventory.get(fuel))
            .unwrap_or(0);
        if in_cargo + local < need {
            return Err(format!(
                "Needs {} {} for this trip ({} available)",
                need,
                fuel.name(),
                in_cargo + local
            ));
        }
        let from_cargo = in_cargo.min(need);
        let s = self.ships.get_mut(&ship).expect("ship");
        s.cargo.take(fuel, from_cargo);
        if need > from_cargo {
            self.universe
                .planet_mut(from)
                .and_then(|p| p.colony.as_mut())
                .expect("checked")
                .inventory
                .take(fuel, need - from_cargo);
        }
        let s = self.ships.get_mut(&ship).expect("ship");
        s.loc = ShipLoc::Transit {
            from,
            to,
            depart: self.tick,
            arrive: self.tick + plan.ticks,
        };
        Ok(())
    }

    fn found_colony(&mut self, ship: u32) -> Result<(), String> {
        let s = self.ships.get(&ship).ok_or("No such ship")?;
        let at = s.docked_at().ok_or("Ship is in transit")?;
        if s.cargo.get(Item::ColonyKit) == 0 {
            return Err("Needs a colony kit in the ship's cargo".into());
        }
        let p = self.universe.planet(at).ok_or("Unknown planet")?;
        if p.colony.is_some() {
            return Err("Already colonized".into());
        }
        if !p.surveyed() {
            return Err("The planet must be fully surveyed first".into());
        }
        let seed = self.seed;
        let s = self.ships.get_mut(&ship).expect("ship");
        s.cargo.take(Item::ColonyKit, 1);
        let cargo = std::mem::take(&mut s.cargo);
        let people = std::mem::take(&mut s.passengers);
        let p = self.universe.planet_mut(at).expect("planet");
        if p.surface.is_none() {
            p.surface = Some(Surface::generate(p, seed, false));
            self.events.push(Event::Generated {
                what: "planet surface",
                name: p.name.clone(),
            });
        }
        Colony::deploy_kit(p.surface.as_mut().expect("surface"));
        p.colony = Some(Colony::new(self.tick, cargo, people, false));
        self.events.push(Event::ColonyFounded(at));
        Ok(())
    }

    /// Mark a system detected, generating its planets on first detection.
    fn detect_system(&mut self, id: SystemId, level: u8) -> bool {
        let seed = self.seed;
        let Some(sys) = self.universe.system_mut(id) else {
            return false;
        };
        let newly = sys.knowledge == SystemKnowledge::Unknown;
        if newly {
            sys.knowledge = SystemKnowledge::Detected;
        }
        if sys.generate_planets(seed) {
            self.events.push(Event::Generated {
                what: "star system",
                name: sys.name.clone(),
            });
        }
        let sys = self.universe.system_mut(id).expect("system");
        for p in &mut sys.planets {
            p.scan_level = p.scan_level.max(level);
        }
        newly
    }

    fn scan_level(&self) -> (u8, i64) {
        let r = &self.research;
        if r.has(Tech::ScanIV) {
            (4, 2000)
        } else if r.has(Tech::ScanIII) {
            (3, 450)
        } else if r.has(Tech::ScanII) {
            (2, 250)
        } else {
            (1, 130)
        }
    }

    fn scan_from(&mut self, from: PlanetId) {
        let (level, radius) = self.scan_level();
        let g = from.galaxy as usize;
        let origin = self
            .universe
            .system(from.system())
            .map(|s| s.pos)
            .unwrap_or((0, 0));
        let targets: Vec<SystemId> = self.universe.galaxies[g]
            .systems
            .iter()
            .filter(|s| {
                let d2 = (s.pos.0 - origin.0) as i64 * (s.pos.0 - origin.0) as i64
                    + (s.pos.1 - origin.1) as i64 * (s.pos.1 - origin.1) as i64;
                d2 <= radius * radius
            })
            .map(|s| s.id)
            .collect();
        let mut found = 0;
        for id in targets {
            if self.detect_system(id, level) {
                found += 1;
            }
        }
        if found > 0 {
            self.events.push(Event::SystemsDetected(found));
        }
    }

    fn arrive(&mut self, ship: u32, at: PlanetId) {
        let seed = self.seed;
        if let Some(sys) = self.universe.system_mut(at.system()) {
            let generated = sys.generate_planets(seed);
            if generated {
                let name = sys.name.clone();
                self.events.push(Event::Generated {
                    what: "star system",
                    name,
                });
            }
            let sys = self.universe.system_mut(at.system()).expect("system");
            sys.knowledge = SystemKnowledge::Visited;
            for p in &mut sys.planets {
                p.scan_level = p.scan_level.max(2);
            }
        }
        self.ships.get_mut(&ship).expect("ship").loc = ShipLoc::Docked(at);
        self.events.push(Event::ShipArrived { ship, planet: at });
    }

    fn step_ships(&mut self) {
        let ids: Vec<u32> = self.ships.keys().copied().collect();
        for id in ids {
            let loc = self.ships[&id].loc;
            match loc {
                ShipLoc::Transit { to, arrive, .. } if arrive <= self.tick => self.arrive(id, to),
                ShipLoc::Docked(at) => {
                    let s = &self.ships[&id];
                    if s.surveying && self.tick.is_multiple_of(12) {
                        let seed = self.seed;
                        if let Some(p) = self.universe.planet_mut(at)
                            && p.survey < 100
                        {
                            p.survey += 1;
                            if p.survey == 100 {
                                self.events.push(Event::SurveyComplete(at));
                                if p.surface.is_none() {
                                    p.surface = Some(Surface::generate(p, seed, false));
                                    self.events.push(Event::Generated {
                                        what: "planet surface",
                                        name: p.name.clone(),
                                    });
                                }
                            }
                        }
                    }
                    if self.ships[&id].route.is_some() {
                        self.step_route(id, at);
                    }
                }
                _ => {}
            }
        }
    }

    /// Automated freighter logic: unload everything, load the list, go.
    fn step_route(&mut self, id: u32, at: PlanetId) {
        let s = self.ships.get_mut(&id).expect("ship");
        let route = s.route.clone().expect("route");
        let (load, dest) = if at == route.a {
            (route.load_a.clone(), route.b)
        } else if at == route.b {
            (route.load_b.clone(), route.a)
        } else {
            // Off-route: head to the start.
            let _ = self.launch(id, route.a);
            return;
        };
        let Some(colony) = self.universe.planet_mut(at).and_then(|p| p.colony.as_mut()) else {
            let _ = self.launch(id, dest);
            return;
        };
        if s.wait == 0 {
            // Fresh arrival: unload all cargo that is not wanted for the next leg.
            let cargo: Vec<(Item, u32)> = s.cargo.iter().collect();
            for (item, n) in cargo {
                let keep = load.iter().find(|l| l.0 == item).map(|l| l.1).unwrap_or(0);
                if n > keep {
                    s.cargo.take(item, n - keep);
                    colony.inventory.add(item, n - keep);
                }
            }
        }
        let mut complete = true;
        for &(item, want) in &load {
            let have = s.cargo.get(item);
            if have < want {
                let n = colony
                    .inventory
                    .take(item, (want - have).min(s.free_cargo()));
                s.cargo.add(item, n);
                if s.cargo.get(item) < want {
                    complete = false;
                }
            }
        }
        s.wait += 1;
        let loaded = load.iter().any(|&(i, _)| s.cargo.get(i) > 0);
        let go = load.is_empty() || complete || (loaded && s.wait > 600);
        if go && self.launch(id, dest).is_ok() {
            self.ships.get_mut(&id).expect("ship").wait = 0;
        }
    }

    /// Plan interplanetary/interstellar power sharing between linked colonies.
    fn plan_transmission(&self) -> BTreeMap<PlanetId, i32> {
        let mut out = BTreeMap::new();
        if !self.research.has(Tech::InterplanetaryTransmission) {
            return out;
        }
        let interstellar = self.research.has(Tech::InterstellarTransmission);
        // Group linked colonies: by system, or by galaxy with interstellar tech.
        let mut groups: BTreeMap<(u16, u16), Vec<Link>> = BTreeMap::new();
        for p in self.colonies() {
            let Some(s) = &p.surface else { continue };
            let (cap, demand, tx) = Colony::power_balance(s);
            if tx == 0 {
                continue;
            }
            let key = if interstellar {
                (p.id.galaxy, 0)
            } else {
                (p.id.galaxy, p.id.system)
            };
            let limit = 5000 * tx as i64;
            let net = (cap as i64 - demand as i64).clamp(-limit, limit);
            groups.entry(key).or_default().push((p.id, net, limit));
        }
        for members in groups.values() {
            let mut surplus: i64 = members.iter().filter(|m| m.1 > 0).map(|m| m.1).sum();
            for &(id, net, _) in members {
                if net < 0 {
                    let take = (-net).min(surplus);
                    surplus -= take;
                    *out.entry(id).or_insert(0) += take as i32;
                }
            }
            let mut given: i64 = members
                .iter()
                .filter(|m| m.1 < 0)
                .map(|m| out.get(&m.0).copied().unwrap_or(0) as i64)
                .sum();
            for &(id, net, _) in members {
                if net > 0 && given > 0 {
                    let send = net.min(given);
                    given -= send;
                    *out.entry(id).or_insert(0) -= send as i32;
                }
            }
        }
        out
    }

    /// Advance one fixed step. Deterministic for a given seed and command history.
    pub fn step(&mut self) {
        self.step_ships();
        if let Some(id) = self.player.riding {
            match self.ships.get(&id) {
                Some(s) => self.player.at = s.docked_at(),
                None => self.player.riding = None,
            }
        }

        let transfers = self.plan_transmission();
        let colony_ids: Vec<PlanetId> = self.colonies().map(|p| p.id).collect();
        let mut research_points = 0;
        let mut scans = Vec::new();
        let mut built = Vec::new();
        for id in colony_ids {
            let tick = self.tick;
            let seed = self.seed;
            let transfer = transfers.get(&id).copied().unwrap_or(0);
            let p = self.universe.planet_mut(id).expect("colony planet");
            let (Some(surface), Some(colony)) = (p.surface.as_mut(), p.colony.as_mut()) else {
                continue;
            };
            let out = colony.step(surface, tick, seed, &self.research, transfer);
            research_points += out.research;
            for e in out.events {
                match e {
                    ColonyEvent::PopulationGrew => self.events.push(Event::PopulationGrew(id)),
                    ColonyEvent::Abandoned => self.events.push(Event::ColonyAbandoned(id)),
                    ColonyEvent::PowerShortage => self.events.push(Event::PowerShortage(id)),
                    ColonyEvent::Attack { repelled, killed } => self.events.push(Event::Attack {
                        planet: id,
                        repelled,
                        killed,
                    }),
                    ColonyEvent::NestDestroyed => self.events.push(Event::NestDestroyed(id)),
                    ColonyEvent::ShipBuilt(order) => built.push((id, order)),
                    ColonyEvent::Scan(_) => scans.push(id),
                    ColonyEvent::BeaconCharged => {
                        if self.victory.is_none() && id.galaxy != 0 {
                            self.victory = Some(self.tick);
                            self.events.push(Event::Victory);
                        }
                    }
                }
            }
        }
        for (planet, order) in built {
            let id = self.next_ship;
            self.next_ship += 1;
            let name = format!(
                "{} {}",
                order.hull.name(),
                universe::gen_name(universe::mix(self.seed, &[0x5417, id as u64]))
            );
            self.ships.insert(
                id,
                Ship {
                    id,
                    name,
                    hull: order.hull,
                    drive: order.drive,
                    loc: ShipLoc::Docked(planet),
                    cargo: Inventory::new(),
                    passengers: 0,
                    crew: order.hull.crew(),
                    surveying: false,
                    route: None,
                    wait: 0,
                },
            );
            self.events.push(Event::ShipBuilt { ship: id, planet });
        }
        for from in scans {
            self.scan_from(from);
        }

        self.research.pool += research_points;
        self.research.generated += research_points as u64;
        if let Some(t) = self.research.advance() {
            self.on_research(t);
        }
        self.tick += 1;
    }

    fn on_research(&mut self, t: Tech) {
        self.events.push(Event::ResearchComplete(t));
        if t == Tech::VoidPropulsion {
            // Deep-space telescopes chart the neighbouring galaxies' cores.
            let seed = self.seed;
            for g in 1..self.universe.galaxies.len() {
                self.universe.galaxies[g].chart(seed);
                self.events.push(Event::Generated {
                    what: "galaxy",
                    name: self.universe.galaxies[g].name.clone(),
                });
                self.detect_system(
                    SystemId {
                        galaxy: g as u16,
                        index: 0,
                    },
                    2,
                );
            }
        }
    }

    /// Spawn a ship directly (scenario setup, tests).
    pub fn spawn_ship(&mut self, at: PlanetId, hull: Hull, drive: Drive) -> u32 {
        let id = self.next_ship;
        self.next_ship += 1;
        self.ships.insert(
            id,
            Ship {
                id,
                name: format!("{} {}", hull.name(), id),
                hull,
                drive,
                loc: ShipLoc::Docked(at),
                cargo: Inventory::new(),
                passengers: 0,
                crew: hull.crew(),
                surveying: false,
                route: None,
                wait: 0,
            },
        );
        id
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
        if let Some(p) = self.player.at {
            feed(&p.galaxy.to_le_bytes());
            feed(&p.system.to_le_bytes());
            feed(&[p.index]);
        }
        feed(&self.player.riding.unwrap_or(0).to_le_bytes());
        self.research.hash_into(&mut feed);
        for g in &self.universe.galaxies {
            for s in &g.systems {
                feed(&[s.knowledge as u8]);
                for p in &s.planets {
                    feed(&[p.scan_level, p.survey]);
                    if let Some(surface) = &p.surface {
                        surface.hash_into(&mut feed);
                    }
                    if let Some(c) = &p.colony {
                        c.hash_into(&mut feed);
                    }
                }
            }
        }
        for s in self.ships.values() {
            s.hash_into(&mut feed);
        }
        h
    }

    pub fn save(&self) -> Result<Vec<u8>, String> {
        rmp_serde::to_vec(self).map_err(|e| e.to_string())
    }

    pub fn load(bytes: &[u8]) -> Result<Sim, String> {
        rmp_serde::from_slice(bytes).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(seed: u64, ticks: u32) -> u64 {
        let mut sim = Sim::new(seed);
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

    #[test]
    fn home_generator_powers_on() {
        let mut sim = Sim::new(3);
        for _ in 0..40 {
            sim.step();
        }
        let c = sim.colony(HOME).unwrap();
        assert!(c.power.produced > 0, "{:?}", c.power);
    }

    #[test]
    fn place_and_remove_refunds() {
        let mut sim = Sim::new(5);
        let before = sim.colony(HOME).unwrap().inventory.get(Item::IronPlate);
        let s = sim.surface(HOME).unwrap();
        let (x, y) = (s.w / 2, s.w / 2 + 4);
        sim.apply(Command::Place {
            planet: HOME,
            x,
            y,
            what: Placeable::Belt(BeltTier::Basic),
            dir: Dir::E,
        })
        .unwrap();
        assert_eq!(
            sim.colony(HOME).unwrap().inventory.get(Item::IronPlate),
            before - 1
        );
        assert!(
            sim.apply(Command::Place {
                planet: HOME,
                x,
                y,
                what: Placeable::Belt(BeltTier::Basic),
                dir: Dir::E,
            })
            .is_err()
        );
        sim.apply(Command::Remove { planet: HOME, x, y }).unwrap();
        assert_eq!(
            sim.colony(HOME).unwrap().inventory.get(Item::IronPlate),
            before
        );
    }

    #[test]
    fn belt_determinism_with_commands() {
        let run = || {
            let mut sim = Sim::new(9);
            let c = sim.surface(HOME).unwrap().w / 2;
            for x in 0..8 {
                sim.place_belt(c - 4 + x, c + 5, Dir::E);
            }
            for _ in 0..50 {
                sim.step();
            }
            sim.state_hash()
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn ship_travel_survey_and_colonize() {
        let mut sim = Sim::new(11);
        sim.research.unlocked.insert(Tech::Rocketry);
        let ship = sim.spawn_ship(HOME, Hull::Scout, Drive::Chemical);
        sim.universe
            .planet_mut(HOME)
            .unwrap()
            .colony
            .as_mut()
            .unwrap()
            .inventory
            .add(Item::Fuel, 100);
        let target = PlanetId { index: 2, ..HOME };
        sim.apply(Command::Load {
            ship,
            item: Item::ColonyKit,
            qty: 1,
        })
        .unwrap_err();
        sim.universe
            .planet_mut(HOME)
            .unwrap()
            .colony
            .as_mut()
            .unwrap()
            .inventory
            .add(Item::ColonyKit, 1);
        sim.apply(Command::Load {
            ship,
            item: Item::ColonyKit,
            qty: 1,
        })
        .unwrap();
        sim.apply(Command::Board { ship, people: 2 }).unwrap();
        sim.apply(Command::Launch { ship, to: target }).unwrap();
        let eta = sim.ticks_until_arrival(ship).unwrap();
        for _ in 0..=eta {
            sim.step();
        }
        assert_eq!(sim.ships[&ship].docked_at(), Some(target));
        assert!(
            sim.apply(Command::FoundColony { ship }).is_err(),
            "needs survey"
        );
        sim.apply(Command::SetSurvey { ship, on: true }).unwrap();
        for _ in 0..1300 {
            sim.step();
        }
        assert!(sim.planet(target).unwrap().surveyed());
        sim.apply(Command::FoundColony { ship }).unwrap();
        let c = sim.colony(target).unwrap();
        assert!(!c.active);
        assert_eq!(c.population, 2);
        assert!(
            sim.apply(Command::ActivateColony { planet: target })
                .is_err(),
            "not present"
        );
        // The overseer travels there in person.
        let ferry = sim.spawn_ship(HOME, Hull::Scout, Drive::Chemical);
        sim.apply(Command::Ride { ship: Some(ferry) }).unwrap();
        sim.apply(Command::Launch {
            ship: ferry,
            to: target,
        })
        .unwrap();
        sim.step();
        assert_eq!(sim.player.at, None);
        for _ in 0..=sim.ticks_until_arrival(ferry).unwrap() {
            sim.step();
        }
        assert_eq!(sim.player.at, Some(target));
        sim.apply(Command::ActivateColony { planet: target })
            .unwrap();
        assert!(sim.colony(target).unwrap().active);
    }

    #[test]
    fn research_from_labs() {
        let mut sim = Sim::new(4);
        let c = sim.surface(HOME).unwrap().w / 2;
        sim.apply(Command::Place {
            planet: HOME,
            x: c - 2,
            y: c + 4,
            what: Placeable::Building(BuildingKind::Lab),
            dir: Dir::N,
        })
        .unwrap();
        sim.apply(Command::SelectResearch(Tech::Time2)).unwrap();
        for _ in 0..(20 * 25) {
            sim.step();
        }
        assert!(sim.research.has(Tech::Time2));
        assert_eq!(sim.research.max_speed(), 2);
    }

    #[test]
    fn save_load_roundtrip_keeps_hash() {
        let mut sim = Sim::new(8);
        for _ in 0..100 {
            sim.step();
        }
        let bytes = sim.save().unwrap();
        let mut loaded = Sim::load(&bytes).unwrap();
        assert_eq!(loaded.state_hash(), sim.state_hash());
        for _ in 0..100 {
            sim.step();
            loaded.step();
        }
        assert_eq!(loaded.state_hash(), sim.state_hash());
    }
}
