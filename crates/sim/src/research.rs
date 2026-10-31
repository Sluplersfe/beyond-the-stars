//! Branching technology tree and the civilization-wide research pool.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Branch {
    Industrial,
    Energy,
    Rocketry,
    Automation,
    Planetary,
    Astrophysics,
    Time,
}

impl Branch {
    pub const ALL: [Branch; 7] = [
        Branch::Industrial,
        Branch::Energy,
        Branch::Rocketry,
        Branch::Automation,
        Branch::Planetary,
        Branch::Astrophysics,
        Branch::Time,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Branch::Industrial => "Industrial engineering",
            Branch::Energy => "Energy",
            Branch::Rocketry => "Rocketry & propulsion",
            Branch::Automation => "Automation",
            Branch::Planetary => "Planetary engineering",
            Branch::Astrophysics => "Astrophysics & exotic science",
            Branch::Time => "Time control",
        }
    }
}

impl Tech {
    /// Game-speed multiplier unlocked by a time-control tech.
    pub fn speed(self) -> Option<u32> {
        use Tech::*;
        Some(match self {
            Time2 => 2,
            Time4 => 4,
            Time8 => 8,
            Time16 => 16,
            Time32 => 32,
            Time64 => 64,
            Time128 => 128,
            Time256 => 256,
            Time512 => 512,
            Time1024 => 1024,
            Time2048 => 2048,
            Time4096 => 4096,
            Time8192 => 8192,
            Time16384 => 16384,
            Time32768 => 32768,
            _ => return None,
        })
    }

    /// Tier (1-4) of a time-control tech, for the tech-tree layout.
    pub fn time_tier(self) -> Option<u8> {
        match self.speed() {
            Some(s) if s <= 16 => Some(1),
            Some(s) if s <= 256 => Some(2),
            Some(s) if s <= 4096 => Some(3),
            Some(_) => Some(4),
            None if self == Tech::InstantWarp => Some(4),
            None => None,
        }
    }
}

pub struct TechInfo {
    pub name: &'static str,
    pub branch: Branch,
    pub cost: u32,
    pub prereqs: &'static [Tech],
    pub desc: &'static str,
}

macro_rules! techs {
    ($($id:ident: $name:expr, $branch:ident, $cost:expr, [$($pre:ident),*], $desc:expr;)*) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        pub enum Tech {
            $($id,)*
        }

        impl Tech {
            pub const ALL: &'static [Tech] = &[$(Tech::$id,)*];

            pub fn info(self) -> TechInfo {
                match self {
                    $(Tech::$id => TechInfo {
                        name: $name,
                        branch: Branch::$branch,
                        cost: $cost,
                        prereqs: &[$(Tech::$pre),*],
                        desc: $desc,
                    },)*
                }
            }
        }
    };
}

techs! {
    // Industrial engineering
    Metallurgy: "Metallurgy", Industrial, 60, [], "Steel smelting.";
    Logistics2: "Fast logistics", Industrial, 200, [Metallurgy], "Fast belts and fast inserters.";
    MachineGen2: "Machine generation II", Industrial, 700, [Metallurgy, AdvancedElectronics], "Mk2 miners, smelters and assemblers: double speed.";
    TitaniumProcessing: "Titanium processing", Industrial, 600, [Metallurgy, Chemistry], "Titanium alloy from off-world titanium ore.";
    Modules: "Machine modules", Industrial, 1500, [MachineGen2], "Speed and efficiency modules for machines.";
    Logistics3: "Express logistics", Industrial, 2500, [Logistics2, MachineGen2], "Express belts: one tile per tick.";
    // Energy
    SolarPower: "Solar power", Energy, 80, [], "Solar panels: free, weak power.";
    Chemistry: "Chemistry", Energy, 100, [], "Refinery: fuel, plastic, melting ice.";
    NuclearPower: "Fusion reactors", Energy, 3000, [TitaniumProcessing, AdvancedElectronics], "Reactors burning fuel cells.";
    InterplanetaryTransmission: "Interplanetary transmission", Energy, 2500, [NuclearPower, Rocketry], "Power transmitters link planets within a star system.";
    InterstellarTransmission: "Interstellar transmission", Energy, 12000, [InterplanetaryTransmission, WarpTheory], "Transmitters link planets across the galaxy.";
    // Rocketry and propulsion
    Rocketry: "Rocketry", Rocketry, 300, [Metallurgy, Chemistry], "Shipyard, rocket engines, scout hulls.";
    CargoShips: "Cargo ships", Rocketry, 700, [Rocketry], "Freighter hulls (cargo 400, 10 seats).";
    WarpTheory: "Warp theory", Rocketry, 5000, [Rocketry, TitaniumProcessing, AdvancedElectronics], "Warp drives for interstellar travel.";
    DeepSpaceCarriers: "Deep-space carriers", Rocketry, 9000, [WarpTheory, CargoShips], "Carrier hulls (cargo 2000, 40 seats).";
    VoidPropulsion: "Void propulsion", Rocketry, 40000, [WarpTheory, ExoticPhysics], "Void drives: intergalactic travel.";
    // Automation
    AdvancedElectronics: "Advanced electronics", Automation, 300, [Chemistry], "Computers and navigation systems.";
    Robotics: "Robotics", Automation, 500, [AdvancedElectronics], "Robotic parts.";
    ResearchII: "Research automation", Automation, 1000, [AdvancedElectronics], "Mk2 research labs: 4x research output.";
    LogisticsAutomation: "Logistics automation", Automation, 1500, [CargoShips], "Automated freighter routes.";
    // Planetary engineering
    Defense: "Planetary defense", Planetary, 250, [Metallurgy], "Defense turrets against hostile life.";
    Colonization: "Colonization", Planetary, 900, [Robotics, Rocketry], "Colony kits to found new colonies.";
    Habitats: "Advanced habitats", Planetary, 1200, [Colonization], "Habitat blocks housing 16 people.";
    // Astrophysics
    Astronomy: "Astronomy", Astrophysics, 150, [], "Observatory and basic scans.";
    ScanII: "Intermediate scanning", Astrophysics, 800, [Astronomy, AdvancedElectronics], "Wider scans; planet categories.";
    ScanIII: "Advanced scanning", Astrophysics, 3000, [ScanII], "Much wider scans; resource indicators.";
    ScanIV: "Specialized scanning", Astrophysics, 8000, [ScanIII], "Detect rare deposits and hostile life.";
    ExoticPhysics: "Exotic physics", Astrophysics, 8000, [WarpTheory, ScanIII], "Refine exotic crystals into exotic matter.";
    FrontierBeacon: "Frontier beacon", Astrophysics, 60000, [VoidPropulsion], "The intergalactic beacon megastructure.";
    // Time control (tiers 1-4)
    Time2: "Basic Time Acceleration", Time, 20, [], "Unlocks Fast Mode 2x.";
    Time4: "Fast Mode 4x", Time, 100, [Time2], "Game speed x4.";
    Time8: "Fast Mode 8x", Time, 200, [Time4], "Game speed x8.";
    Time16: "Fast Mode 16x", Time, 400, [Time8], "Game speed x16.";
    Time32: "Fast Mode 32x", Time, 800, [Time16], "Game speed x32.";
    Time64: "Fast Mode 64x", Time, 1600, [Time32], "Game speed x64.";
    Time128: "Fast Mode 128x", Time, 3200, [Time64], "Game speed x128.";
    Time256: "Fast Mode 256x", Time, 6400, [Time128], "Game speed x256.";
    Time512: "Ultra Fast Mode", Time, 12800, [Time256], "Game speed x512.";
    Time1024: "Hyper Fast Mode", Time, 25600, [Time512], "Game speed x1,024.";
    Time2048: "Time Compression", Time, 51200, [Time1024], "Game speed x2,048.";
    Time4096: "Temporal Overdrive", Time, 102400, [Time2048], "Game speed x4,096.";
    Time8192: "Beyond Time", Time, 204800, [Time4096], "Game speed x8,192.";
    Time16384: "Ultra Beyond Time", Time, 409600, [Time8192], "Game speed x16,384.";
    Time32768: "Cosmic Acceleration", Time, 819200, [Time16384], "Game speed x32,768.";
    InstantWarp: "Instant-Warp", Time, 1638400, [Time32768], "Skip time until a scout ship or other vessel reaches its destination.";
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Research {
    pub unlocked: BTreeSet<Tech>,
    pub progress: BTreeMap<Tech, u32>,
    pub current: Option<Tech>,
    /// Shared civilization-wide pool, spent on the current project.
    pub pool: u32,
    /// Lifetime points generated (stats only).
    pub generated: u64,
}

impl Research {
    pub fn has(&self, t: Tech) -> bool {
        self.unlocked.contains(&t)
    }

    pub fn available(&self, t: Tech) -> bool {
        !self.has(t) && t.info().prereqs.iter().all(|p| self.has(*p))
    }

    pub fn progress_of(&self, t: Tech) -> u32 {
        self.progress.get(&t).copied().unwrap_or(0)
    }

    /// Switch projects freely; accumulated progress is preserved.
    pub fn select(&mut self, t: Tech) -> bool {
        if self.available(t) {
            self.current = Some(t);
            true
        } else {
            false
        }
    }

    /// Spend the pool on the current project. Returns a newly finished tech.
    pub(crate) fn advance(&mut self) -> Option<Tech> {
        let t = self.current?;
        let cost = t.info().cost;
        let p = self.progress.entry(t).or_insert(0);
        let spend = self.pool.min(cost - *p);
        *p += spend;
        self.pool -= spend;
        if *p >= cost {
            self.progress.remove(&t);
            self.unlocked.insert(t);
            self.current = None;
            return Some(t);
        }
        None
    }

    /// Highest unlocked time multiplier.
    pub fn max_speed(&self) -> u32 {
        self.unlocked
            .iter()
            .filter_map(|t| t.speed())
            .max()
            .unwrap_or(1)
    }

    pub(crate) fn hash_into(&self, feed: &mut impl FnMut(&[u8])) {
        for t in &self.unlocked {
            feed(&[*t as u8]);
        }
        for (t, p) in &self.progress {
            feed(&[*t as u8]);
            feed(&p.to_le_bytes());
        }
        feed(&[self.current.map(|t| t as u8 + 1).unwrap_or(0)]);
        feed(&self.pool.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prereqs_exist_and_are_acyclic() {
        // Every tech must be reachable by unlocking available techs repeatedly.
        let mut r = Research::default();
        loop {
            let next: Vec<Tech> = Tech::ALL
                .iter()
                .copied()
                .filter(|t| r.available(*t))
                .collect();
            if next.is_empty() {
                break;
            }
            r.unlocked.extend(next);
        }
        assert_eq!(r.unlocked.len(), Tech::ALL.len());
    }

    #[test]
    fn switching_preserves_progress() {
        let mut r = Research::default();
        assert!(r.select(Tech::Time2));
        r.pool = 10;
        r.advance();
        assert!(r.select(Tech::Metallurgy));
        assert_eq!(r.progress_of(Tech::Time2), 10);
        r.select(Tech::Time2);
        r.pool = 10;
        assert_eq!(r.advance(), Some(Tech::Time2));
        assert_eq!(r.max_speed(), 2);
    }

    #[test]
    fn time_chain_doubles_to_32768() {
        let mut r = Research::default();
        let mut expect = 1;
        for t in Tech::ALL.iter().filter(|t| t.speed().is_some()) {
            assert!(r.available(*t), "{t:?} not next in chain");
            r.unlocked.insert(*t);
            expect *= 2;
            assert_eq!(r.max_speed(), expect);
        }
        assert_eq!(r.max_speed(), 32768);
        assert!(r.available(Tech::InstantWarp));
        assert_eq!(Tech::InstantWarp.info().cost, 1_638_400);
    }
}
