//! Procedural universe: galaxies → star systems → planets. Every generated value comes
//! from a stable hash of the world seed and the location's ids, so a location always
//! regenerates identically; changes are persisted on top of it.

use serde::{Deserialize, Serialize};

use crate::colony::Colony;
use crate::items::Item;
use crate::surface::Surface;

/// splitmix64-style hash of several words.
pub fn mix(seed: u64, words: &[u64]) -> u64 {
    let mut h = seed ^ 0x9E37_79B9_7F4A_7C15;
    for &w in words {
        h = h.wrapping_add(w).wrapping_add(0x9E37_79B9_7F4A_7C15);
        h ^= h >> 30;
        h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        h ^= h >> 27;
        h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
        h ^= h >> 31;
    }
    h
}

/// Uniform-ish integer in `0..n` from a hash.
pub fn pick(h: u64, n: u64) -> u64 {
    if n == 0 { 0 } else { h % n }
}

pub fn isqrt(v: i64) -> i64 {
    if v <= 0 {
        return 0;
    }
    // Integer Newton iteration; no floats in sim state.
    let mut x = v;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + v / x) / 2;
    }
    x
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SystemId {
    pub galaxy: u16,
    pub index: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PlanetId {
    pub galaxy: u16,
    pub system: u16,
    pub index: u8,
}

impl PlanetId {
    pub fn system(self) -> SystemId {
        SystemId {
            galaxy: self.galaxy,
            index: self.system,
        }
    }

    pub fn words(self) -> [u64; 3] {
        [self.galaxy as u64, self.system as u64, self.index as u64]
    }
}

pub const HOME: PlanetId = PlanetId {
    galaxy: 0,
    system: 0,
    index: 0,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum PlanetCategory {
    Rocky,
    Ice,
    Volcanic,
    Living,
    MetalRich,
    Exotic,
}

impl PlanetCategory {
    pub fn name(self) -> &'static str {
        match self {
            PlanetCategory::Rocky => "Rocky",
            PlanetCategory::Ice => "Ice",
            PlanetCategory::Volcanic => "Volcanic",
            PlanetCategory::Living => "Living",
            PlanetCategory::MetalRich => "Metal-rich",
            PlanetCategory::Exotic => "Exotic",
        }
    }

    pub fn rgb(self) -> u32 {
        match self {
            PlanetCategory::Rocky => 0xb08a68,
            PlanetCategory::Ice => 0xb8e8ff,
            PlanetCategory::Volcanic => 0xe05020,
            PlanetCategory::Living => 0x40c060,
            PlanetCategory::MetalRich => 0x9098b0,
            PlanetCategory::Exotic => 0xd050ff,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum StarType {
    RedDwarf,
    Yellow,
    BlueGiant,
    WhiteDwarf,
    Neutron,
}

impl StarType {
    pub fn name(self) -> &'static str {
        match self {
            StarType::RedDwarf => "Red dwarf",
            StarType::Yellow => "Yellow star",
            StarType::BlueGiant => "Blue giant",
            StarType::WhiteDwarf => "White dwarf",
            StarType::Neutron => "Neutron star",
        }
    }

    pub fn rgb(self) -> u32 {
        match self {
            StarType::RedDwarf => 0xff7050,
            StarType::Yellow => 0xffe070,
            StarType::BlueGiant => 0x80b0ff,
            StarType::WhiteDwarf => 0xf0f0ff,
            StarType::Neutron => 0xd090ff,
        }
    }
}

/// What the player knows about a star system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum SystemKnowledge {
    Unknown,
    /// Revealed by a scan: star type and planet count.
    Detected,
    /// A ship has been there. Destinations in it are selectable.
    Visited,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Planet {
    pub id: PlanetId,
    pub name: String,
    pub category: PlanetCategory,
    /// 0 small, 1 medium, 2 large.
    pub size: u8,
    pub orbit: u8,
    /// True deposit profile: (resource, richness 1-3). Hidden until surveyed.
    pub profile: Vec<(Item, u8)>,
    pub hostile: bool,
    /// Level of scan information: 0 none, 1 size, 2 category, 3 resource hints, 4 rare/hostile.
    pub scan_level: u8,
    /// Survey progress 0-100. At 100 the true characteristics are known.
    pub survey: u8,
    /// Generated on demand (survey completion or colonization), then persisted.
    pub surface: Option<Surface>,
    pub colony: Option<Colony>,
}

impl Planet {
    pub fn surveyed(&self) -> bool {
        self.survey >= 100
    }

    pub fn category_known(&self) -> bool {
        self.scan_level >= 2 || self.survey >= 25
    }

    /// Resources the player can see: all of them once surveyed, common ones from hints.
    pub fn known_resources(&self) -> Vec<(Item, bool)> {
        if self.surveyed() {
            return self.profile.iter().map(|&(i, _)| (i, true)).collect();
        }
        if self.scan_level >= 3 || self.survey >= 50 {
            let rare_visible = self.scan_level >= 4;
            return self
                .profile
                .iter()
                .filter(|(i, _)| rare_visible || !is_rare(*i))
                .map(|&(i, _)| (i, false))
                .collect();
        }
        Vec::new()
    }

    pub fn hostile_known(&self) -> bool {
        self.surveyed() || self.scan_level >= 4 || self.colony.is_some()
    }

    pub fn surface_size(&self) -> i32 {
        48 + 16 * self.size as i32
    }
}

pub fn is_rare(i: Item) -> bool {
    matches!(i, Item::TitaniumOre | Item::ExoticCrystal | Item::VoidShard)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StarSystem {
    pub id: SystemId,
    pub name: String,
    /// Position inside the galaxy, in galactic units.
    pub pos: (i32, i32),
    pub star: StarType,
    pub knowledge: SystemKnowledge,
    /// Generated when the system is first detected.
    pub planets: Vec<Planet>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Galaxy {
    pub id: u16,
    pub name: String,
    /// Position in intergalactic space.
    pub pos: (i32, i32),
    pub radius: i32,
    /// Star systems; empty until the galaxy is first charted.
    pub systems: Vec<StarSystem>,
}

pub const GALAXY_DEFS: [(&str, (i32, i32), u16); 3] = [
    ("Lumen Galaxy", (0, 0), 40),
    ("Andara Galaxy", (14000, 4000), 30),
    ("Kessel Drift", (-9000, 12000), 30),
];

const SYL: [&str; 24] = [
    "ka", "ro", "ve", "lis", "tor", "an", "mi", "zen", "sha", "dul", "or", "qua", "nex", "vi",
    "ta", "lo", "xen", "pra", "um", "bel", "cor", "dra", "phi", "sol",
];

pub fn gen_name(h: u64) -> String {
    let n = 2 + pick(h, 2) as usize;
    let mut s = String::new();
    for i in 0..n {
        s.push_str(SYL[pick(mix(h, &[i as u64]), SYL.len() as u64) as usize]);
    }
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => s,
    }
}

const ROMAN: [&str; 8] = ["I", "II", "III", "IV", "V", "VI", "VII", "VIII"];

pub fn profile_for(cat: PlanetCategory, h: u64, galaxy: u16) -> Vec<(Item, u8)> {
    use Item::*;
    let r = |k: u64| 1 + pick(mix(h, &[k]), 3) as u8;
    let mut p = match cat {
        PlanetCategory::Rocky => vec![
            (IronOre, r(1)),
            (CopperOre, r(2)),
            (Stone, r(3)),
            (Carbon, r(4)),
        ],
        PlanetCategory::Ice => vec![(Ice, r(1)), (Gas, r(2)), (Stone, r(3)), (Carbon, 1)],
        PlanetCategory::Volcanic => vec![
            (Carbon, r(1)),
            (Stone, r(2)),
            (IronOre, r(3)),
            (TitaniumOre, 1),
        ],
        PlanetCategory::Living => vec![(Biomass, r(1)), (Water, r(2)), (Carbon, r(3)), (Stone, 1)],
        PlanetCategory::MetalRich => vec![
            (IronOre, r(1)),
            (CopperOre, r(2)),
            (TitaniumOre, r(3)),
            (Stone, 1),
        ],
        PlanetCategory::Exotic => vec![(ExoticCrystal, r(1)), (Stone, r(2)), (Gas, 1)],
    };
    if cat == PlanetCategory::Rocky && pick(mix(h, &[9]), 100) < 20 {
        p.push((TitaniumOre, 1));
    }
    if cat == PlanetCategory::Exotic && galaxy != 0 {
        p.push((VoidShard, r(5)));
    }
    p
}

impl Galaxy {
    pub fn new(seed: u64, id: u16) -> Self {
        let (name, pos, _) = GALAXY_DEFS[id as usize];
        let _ = seed;
        Galaxy {
            id,
            name: name.to_string(),
            pos,
            radius: 650,
            systems: Vec::new(),
        }
    }

    /// Chart the galaxy's star positions. Planets are generated per system on detection.
    pub fn chart(&mut self, seed: u64) {
        if !self.systems.is_empty() {
            return;
        }
        let count = GALAXY_DEFS[self.id as usize].2;
        let mut positions: Vec<(i32, i32)> = vec![(0, 0)];
        let mut attempt = 0u64;
        while positions.len() < count as usize && attempt < 10_000 {
            attempt += 1;
            let h = mix(seed, &[0x5157, self.id as u64, attempt]);
            let x = pick(h, 1200) as i32 - 600;
            let y = pick(h >> 20, 1200) as i32 - 600;
            if x * x + y * y > 600 * 600 {
                continue;
            }
            if positions
                .iter()
                .any(|&(px, py)| (px - x).pow(2) + (py - y).pow(2) < 70 * 70)
            {
                continue;
            }
            positions.push((x, y));
        }
        // Nearest first, so low indices are close to the core and ids stay stable.
        positions[1..].sort_by_key(|&(x, y)| (x * x + y * y, x, y));
        for (i, pos) in positions.into_iter().enumerate() {
            let h = mix(seed, &[0x5157, self.id as u64, 0x1000 + i as u64]);
            let star = match pick(h, 10) {
                0..=3 => StarType::RedDwarf,
                4..=6 => StarType::Yellow,
                7 => StarType::BlueGiant,
                8 => StarType::WhiteDwarf,
                _ => StarType::Neutron,
            };
            let name = if self.id == 0 && i == 0 {
                "Helios".to_string()
            } else {
                gen_name(h)
            };
            self.systems.push(StarSystem {
                id: SystemId {
                    galaxy: self.id,
                    index: i as u16,
                },
                name,
                pos,
                star,
                knowledge: SystemKnowledge::Unknown,
                planets: Vec::new(),
            });
        }
    }
}

impl StarSystem {
    /// Generate the planets of this system if not done yet. Returns true if generated now.
    pub fn generate_planets(&mut self, seed: u64) -> bool {
        if !self.planets.is_empty() {
            return false;
        }
        let g = self.id.galaxy;
        if g == 0 && self.id.index == 0 {
            self.planets = home_system(self.id);
            return true;
        }
        let h = mix(seed, &[0x9a7e, g as u64, self.id.index as u64]);
        let count = 2 + pick(h, 5) as u8;
        // Progression guarantee: a near-core system in every galaxy holds an exotic world.
        let force_exotic = matches!(self.id.index, 0 | 2);
        for i in 0..count {
            let ph = mix(h, &[i as u64]);
            let roll = pick(ph, 12);
            let mut cat = match (self.star, roll) {
                (StarType::Neutron, 0..=4) => PlanetCategory::Exotic,
                (StarType::BlueGiant, 0..=3) => PlanetCategory::Volcanic,
                (StarType::RedDwarf, 0..=3) => PlanetCategory::Ice,
                _ => match roll {
                    0..=2 => PlanetCategory::Rocky,
                    3..=4 => PlanetCategory::Ice,
                    5..=6 => PlanetCategory::Volcanic,
                    7..=8 => PlanetCategory::Living,
                    9..=10 => PlanetCategory::MetalRich,
                    _ => PlanetCategory::Exotic,
                },
            };
            if force_exotic && i == count / 2 {
                cat = PlanetCategory::Exotic;
            }
            let hostile = match cat {
                PlanetCategory::Living => true,
                PlanetCategory::Volcanic | PlanetCategory::Exotic => pick(ph >> 8, 100) < 30,
                _ => false,
            };
            self.planets.push(Planet {
                id: PlanetId {
                    galaxy: g,
                    system: self.id.index,
                    index: i,
                },
                name: format!("{} {}", self.name, ROMAN[i as usize]),
                category: cat,
                size: pick(ph >> 16, 3) as u8,
                orbit: i,
                profile: profile_for(cat, ph, g),
                hostile,
                scan_level: 0,
                survey: 0,
                surface: None,
                colony: None,
            });
        }
        true
    }
}

/// The handcrafted starting system.
fn home_system(id: SystemId) -> Vec<Planet> {
    use Item::*;
    let p = |index: u8, name: &str, category, size, profile: Vec<(Item, u8)>, hostile| Planet {
        id: PlanetId {
            galaxy: id.galaxy,
            system: id.index,
            index,
        },
        name: name.to_string(),
        category,
        size,
        orbit: index,
        profile,
        hostile,
        scan_level: 2,
        survey: 0,
        surface: None,
        colony: None,
    };
    vec![
        p(
            0,
            "Terra Nova",
            PlanetCategory::Living,
            1,
            vec![
                (IronOre, 2),
                (CopperOre, 2),
                (Stone, 2),
                (Carbon, 2),
                (Water, 1),
                (Gas, 1),
            ],
            false,
        ),
        p(
            1,
            "Ferros",
            PlanetCategory::MetalRich,
            0,
            vec![(IronOre, 3), (TitaniumOre, 2), (CopperOre, 1), (Stone, 1)],
            false,
        ),
        p(
            2,
            "Glacia",
            PlanetCategory::Ice,
            1,
            vec![(Ice, 3), (Gas, 2), (Stone, 1), (Carbon, 1)],
            false,
        ),
        p(
            3,
            "Verdance",
            PlanetCategory::Living,
            2,
            vec![(Biomass, 3), (Water, 2), (Carbon, 2), (Stone, 1)],
            true,
        ),
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Universe {
    pub galaxies: Vec<Galaxy>,
}

impl Universe {
    pub fn new(seed: u64) -> Self {
        let mut galaxies: Vec<Galaxy> = (0..GALAXY_DEFS.len() as u16)
            .map(|g| Galaxy::new(seed, g))
            .collect();
        galaxies[0].chart(seed);
        let home = &mut galaxies[0].systems[0];
        home.knowledge = SystemKnowledge::Visited;
        home.generate_planets(seed);
        Universe { galaxies }
    }

    pub fn system(&self, id: SystemId) -> Option<&StarSystem> {
        self.galaxies
            .get(id.galaxy as usize)?
            .systems
            .get(id.index as usize)
    }

    pub fn system_mut(&mut self, id: SystemId) -> Option<&mut StarSystem> {
        self.galaxies
            .get_mut(id.galaxy as usize)?
            .systems
            .get_mut(id.index as usize)
    }

    pub fn planet(&self, id: PlanetId) -> Option<&Planet> {
        self.system(id.system())?.planets.get(id.index as usize)
    }

    pub fn planet_mut(&mut self, id: PlanetId) -> Option<&mut Planet> {
        self.system_mut(id.system())?
            .planets
            .get_mut(id.index as usize)
    }

    pub fn planets(&self) -> impl Iterator<Item = &Planet> {
        self.galaxies
            .iter()
            .flat_map(|g| g.systems.iter())
            .flat_map(|s| s.planets.iter())
    }

    pub fn planets_mut(&mut self) -> impl Iterator<Item = &mut Planet> {
        self.galaxies
            .iter_mut()
            .flat_map(|g| g.systems.iter_mut())
            .flat_map(|s| s.planets.iter_mut())
    }

    /// Distance between two systems in galactic units (intergalactic gaps included).
    pub fn system_distance(&self, a: SystemId, b: SystemId) -> i64 {
        let (Some(sa), Some(sb)) = (self.system(a), self.system(b)) else {
            return i64::MAX / 4;
        };
        let ga = &self.galaxies[a.galaxy as usize];
        let gb = &self.galaxies[b.galaxy as usize];
        let ax = ga.pos.0 as i64 + sa.pos.0 as i64;
        let ay = ga.pos.1 as i64 + sa.pos.1 as i64;
        let bx = gb.pos.0 as i64 + sb.pos.0 as i64;
        let by = gb.pos.1 as i64 + sb.pos.1 as i64;
        isqrt((ax - bx).pow(2) + (ay - by).pow(2))
    }
}
