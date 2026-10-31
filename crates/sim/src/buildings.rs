//! Building and belt specifications.

use serde::{Deserialize, Serialize};

use crate::items::Item::{self, *};
use crate::recipes::Category;
use crate::research::Tech;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Dir {
    N,
    E,
    S,
    W,
}

impl Dir {
    pub const ALL: [Dir; 4] = [Dir::N, Dir::E, Dir::S, Dir::W];

    pub fn delta(self) -> (i32, i32) {
        match self {
            Dir::N => (0, 1),
            Dir::E => (1, 0),
            Dir::S => (0, -1),
            Dir::W => (-1, 0),
        }
    }

    pub fn rotate_cw(self) -> Self {
        match self {
            Dir::N => Dir::E,
            Dir::E => Dir::S,
            Dir::S => Dir::W,
            Dir::W => Dir::N,
        }
    }

    pub fn opposite(self) -> Self {
        self.rotate_cw().rotate_cw()
    }

    pub fn code(self) -> u8 {
        self as u8
    }
}

/// Belt speed tiers. `speed` is progress per tick; an item crosses a tile at `BELT_LEN`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum BeltTier {
    Basic,
    Fast,
    Express,
}

pub const BELT_LEN: u8 = 60;

impl BeltTier {
    pub const ALL: [BeltTier; 3] = [BeltTier::Basic, BeltTier::Fast, BeltTier::Express];

    pub fn speed(self) -> u8 {
        match self {
            BeltTier::Basic => 15,
            BeltTier::Fast => 30,
            BeltTier::Express => 60,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            BeltTier::Basic => "Belt",
            BeltTier::Fast => "Fast belt",
            BeltTier::Express => "Express belt",
        }
    }

    pub fn cost(self) -> &'static [(Item, u32)] {
        match self {
            BeltTier::Basic => &[(IronPlate, 1)],
            BeltTier::Fast => &[(IronPlate, 1), (Gear, 1)],
            BeltTier::Express => &[(Steel, 1), (Gear, 2)],
        }
    }

    pub fn tech(self) -> Option<Tech> {
        match self {
            BeltTier::Basic => None,
            BeltTier::Fast => Some(Tech::Logistics2),
            BeltTier::Express => Some(Tech::Logistics3),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum BuildingKind {
    Inserter,
    FastInserter,
    Miner,
    MinerMk2,
    Smelter,
    SmelterMk2,
    Assembler,
    AssemblerMk2,
    Refinery,
    Generator,
    Solar,
    Reactor,
    Depot,
    Provider,
    House,
    Habitat,
    Lab,
    LabMk2,
    Observatory,
    Shipyard,
    Turret,
    Transmitter,
    Beacon,
    Splitter,
    Merger,
}

pub struct BuildingSpec {
    pub name: &'static str,
    pub size: (i32, i32),
    pub cost: &'static [(Item, u32)],
    /// Power drawn while working, in kW.
    pub power: u32,
    /// Power produced while running, in kW.
    pub power_gen: u32,
    /// People required to staff it. Robots run everything with 0 workers.
    pub workers: u32,
    pub tech: Option<Tech>,
    pub key: char,
    pub desc: &'static str,
}

impl BuildingKind {
    pub const ALL: &'static [BuildingKind] = &[
        BuildingKind::Inserter,
        BuildingKind::FastInserter,
        BuildingKind::Miner,
        BuildingKind::MinerMk2,
        BuildingKind::Smelter,
        BuildingKind::SmelterMk2,
        BuildingKind::Assembler,
        BuildingKind::AssemblerMk2,
        BuildingKind::Refinery,
        BuildingKind::Generator,
        BuildingKind::Solar,
        BuildingKind::Reactor,
        BuildingKind::Depot,
        BuildingKind::Provider,
        BuildingKind::House,
        BuildingKind::Habitat,
        BuildingKind::Lab,
        BuildingKind::LabMk2,
        BuildingKind::Observatory,
        BuildingKind::Shipyard,
        BuildingKind::Turret,
        BuildingKind::Transmitter,
        BuildingKind::Beacon,
        BuildingKind::Splitter,
        BuildingKind::Merger,
    ];

    pub fn spec(self) -> BuildingSpec {
        use BuildingKind as K;
        let s = |name, size, cost, power, power_gen, workers, tech, key, desc| BuildingSpec {
            name,
            size,
            cost,
            power,
            power_gen,
            workers,
            tech,
            key,
            desc,
        };
        match self {
            K::Splitter => s(
                "Splitter",
                (1, 1),
                &[(IronPlate, 4), (Gear, 2), (Circuit, 1)],
                0,
                0,
                0,
                None,
                'k',
                "Takes items from behind and deals them out front, left and right in turn. Set a filter to send one item straight ahead and everything else to the sides.",
            ),
            K::Merger => s(
                "Merger",
                (1, 1),
                &[(IronPlate, 4), (Gear, 2), (Circuit, 1)],
                0,
                0,
                0,
                None,
                'j',
                "Takes items from the back and both sides and sends them out the front.",
            ),
            K::Inserter => s(
                "Inserter",
                (1, 1),
                &[(IronPlate, 1), (Gear, 1), (Wire, 1)],
                13,
                0,
                0,
                None,
                'i',
                "Moves items from the tile behind it to the tile in front.",
            ),
            K::FastInserter => s(
                "Fast inserter",
                (1, 1),
                &[(IronPlate, 2), (Gear, 2), (Circuit, 1)],
                40,
                0,
                0,
                Some(Tech::Logistics2),
                'i',
                "A faster inserter.",
            ),
            K::Miner => s(
                "Miner",
                (2, 2),
                &[(IronPlate, 10), (Gear, 5)],
                90,
                0,
                0,
                None,
                'm',
                "Extracts the deposit under it and pushes output forward. Deposits never run out.",
            ),
            K::MinerMk2 => s(
                "Miner Mk2",
                (2, 2),
                &[(Steel, 10), (Gear, 10), (Circuit, 5)],
                180,
                0,
                0,
                Some(Tech::MachineGen2),
                'm',
                "Twice the extraction rate.",
            ),
            K::Smelter => s(
                "Smelter",
                (2, 2),
                &[(Stone, 10), (IronPlate, 4)],
                90,
                0,
                0,
                None,
                'f',
                "Smelts ore. Picks its recipe from the first input.",
            ),
            K::SmelterMk2 => s(
                "Smelter Mk2",
                (2, 2),
                &[(Steel, 10), (Glass, 10), (Circuit, 5)],
                180,
                0,
                0,
                Some(Tech::MachineGen2),
                'f',
                "Smelts twice as fast.",
            ),
            K::Assembler => s(
                "Assembler",
                (3, 3),
                &[(IronPlate, 9), (Gear, 5), (Circuit, 3)],
                75,
                0,
                0,
                None,
                'a',
                "Builds components. Choose a recipe.",
            ),
            K::AssemblerMk2 => s(
                "Assembler Mk2",
                (3, 3),
                &[(Steel, 10), (Gear, 10), (Circuit, 10)],
                150,
                0,
                0,
                Some(Tech::MachineGen2),
                'a',
                "Assembles twice as fast.",
            ),
            K::Refinery => s(
                "Refinery",
                (3, 3),
                &[(Steel, 10), (Pipe, 10), (Circuit, 5)],
                210,
                0,
                0,
                Some(Tech::Chemistry),
                'r',
                "Chemistry: fuel, plastic, melting ice, exotic matter.",
            ),
            K::Generator => s(
                "Generator",
                (2, 2),
                &[(Stone, 5), (IronPlate, 5), (Pipe, 2)],
                0,
                900,
                0,
                None,
                'g',
                "Burns carbon or fuel for 900 kW.",
            ),
            K::Solar => s(
                "Solar panel",
                (2, 2),
                &[(IronPlate, 5), (Glass, 5), (Wire, 5)],
                0,
                60,
                0,
                Some(Tech::SolarPower),
                'p',
                "Free 60 kW.",
            ),
            K::Reactor => s(
                "Fusion reactor",
                (4, 4),
                &[(Steel, 50), (ReactorCore, 1), (TitaniumAlloy, 20)],
                0,
                12000,
                0,
                Some(Tech::NuclearPower),
                'n',
                "Burns fuel cells for 12 MW.",
            ),
            K::Depot => s(
                "Depot",
                (2, 2),
                &[(IronPlate, 20)],
                0,
                0,
                0,
                None,
                'd',
                "Accepts items from belts and inserters into the planet inventory.",
            ),
            K::Provider => s(
                "Provider",
                (1, 1),
                &[(IronPlate, 5), (Gear, 3)],
                10,
                0,
                0,
                None,
                'v',
                "Pushes a chosen item from the planet inventory forward.",
            ),
            K::House => s(
                "House",
                (2, 2),
                &[(Stone, 10), (IronPlate, 5)],
                0,
                0,
                0,
                None,
                'h',
                "Houses 4 people and keeps them fed, even when abandoned.",
            ),
            K::Habitat => s(
                "Habitat",
                (3, 3),
                &[(Steel, 20), (Glass, 20), (Plastic, 10)],
                0,
                0,
                0,
                Some(Tech::Habitats),
                'h',
                "Houses 16 people.",
            ),
            K::Lab => s(
                "Research lab",
                (2, 2),
                &[(IronPlate, 10), (Circuit, 10), (Glass, 5)],
                60,
                0,
                2,
                None,
                'l',
                "Staffed by 2 people: 1 research point per second.",
            ),
            K::LabMk2 => s(
                "Research lab Mk2",
                (3, 3),
                &[(Steel, 20), (Computer, 5), (Glass, 10)],
                240,
                0,
                4,
                Some(Tech::ResearchII),
                'l',
                "Staffed by 4 people: 4 research points per second.",
            ),
            K::Observatory => s(
                "Observatory",
                (2, 2),
                &[(Glass, 20), (Circuit, 10), (IronPlate, 10)],
                200,
                0,
                2,
                Some(Tech::Astronomy),
                'o',
                "Staffed by 2 people: scans nearby star systems.",
            ),
            K::Shipyard => s(
                "Shipyard",
                (4, 4),
                &[(Steel, 50), (Motor, 10), (Circuit, 20)],
                500,
                0,
                2,
                Some(Tech::Rocketry),
                's',
                "Staffed by 2 people: builds ships from planet inventory.",
            ),
            K::Turret => s(
                "Defense turret",
                (1, 1),
                &[(Steel, 10), (Gear, 5), (Circuit, 2)],
                50,
                0,
                0,
                Some(Tech::Defense),
                't',
                "Repels hostile attacks within 7 tiles; can eliminate nests.",
            ),
            K::Transmitter => s(
                "Power transmitter",
                (2, 2),
                &[(Steel, 20), (Computer, 4), (Wire, 50)],
                0,
                0,
                0,
                Some(Tech::InterplanetaryTransmission),
                'x',
                "Shares up to 5 MW of surplus power with linked planets.",
            ),
            K::Beacon => s(
                "Frontier beacon",
                (5, 5),
                &[(BeaconCore, 10), (TitaniumAlloy, 200), (Computer, 50)],
                20000,
                0,
                0,
                Some(Tech::FrontierBeacon),
                'b',
                "Built in another galaxy and powered for 60 s, it completes the game.",
            ),
        }
    }

    pub fn crafting(self) -> Option<(Category, u32)> {
        use BuildingKind as K;
        match self {
            K::Smelter => Some((Category::Smelting, 100)),
            K::SmelterMk2 => Some((Category::Smelting, 200)),
            K::Assembler => Some((Category::Assembly, 100)),
            K::AssemblerMk2 => Some((Category::Assembly, 200)),
            K::Refinery => Some((Category::Chemistry, 100)),
            _ => None,
        }
    }

    pub fn is_inserter(self) -> bool {
        matches!(self, BuildingKind::Inserter | BuildingKind::FastInserter)
    }

    /// Splitters and mergers: pass-through belt routing.
    pub fn is_router(self) -> bool {
        matches!(self, BuildingKind::Splitter | BuildingKind::Merger)
    }

    /// Whether players can cap its item rates.
    pub fn has_rate_limits(self) -> bool {
        self.is_miner()
            || self.is_router()
            || self.crafting().is_some()
            || matches!(
                self,
                BuildingKind::Provider
                    | BuildingKind::Depot
                    | BuildingKind::Generator
                    | BuildingKind::Reactor
            )
    }

    pub fn is_miner(self) -> bool {
        matches!(self, BuildingKind::Miner | BuildingKind::MinerMk2)
    }

    pub fn takes_modules(self) -> bool {
        self.crafting().is_some() || self.is_miner()
    }

    pub fn housing(self) -> u32 {
        match self {
            BuildingKind::House => 4,
            BuildingKind::Habitat => 16,
            _ => 0,
        }
    }

    /// Whether the model and footprint follow `dir` (routers have distinct inlets and outlets).
    pub fn rotatable(self) -> bool {
        self.is_inserter()
            || self.is_miner()
            || self.is_router()
            || matches!(self, BuildingKind::Provider)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Placeable {
    Belt(BeltTier),
    Building(BuildingKind),
}

impl Placeable {
    pub fn name(self) -> &'static str {
        match self {
            Placeable::Belt(t) => t.name(),
            Placeable::Building(k) => k.spec().name,
        }
    }

    pub fn cost(self) -> &'static [(Item, u32)] {
        match self {
            Placeable::Belt(t) => t.cost(),
            Placeable::Building(k) => k.spec().cost,
        }
    }

    pub fn tech(self) -> Option<Tech> {
        match self {
            Placeable::Belt(t) => t.tech(),
            Placeable::Building(k) => k.spec().tech,
        }
    }

    pub fn size(self) -> (i32, i32) {
        match self {
            Placeable::Belt(_) => (1, 1),
            Placeable::Building(k) => k.spec().size,
        }
    }
}
