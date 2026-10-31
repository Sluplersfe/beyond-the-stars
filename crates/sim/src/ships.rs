//! Ships, cargo and physical travel. Nothing moves between locations except on a ship.

use serde::{Deserialize, Serialize};

use crate::items::{Inventory, Item, Item::*};
use crate::research::Tech;
use crate::universe::{PlanetId, Universe};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Hull {
    Scout,
    Freighter,
    Carrier,
}

impl Hull {
    pub const ALL: [Hull; 3] = [Hull::Scout, Hull::Freighter, Hull::Carrier];

    pub fn name(self) -> &'static str {
        match self {
            Hull::Scout => "Scout",
            Hull::Freighter => "Freighter",
            Hull::Carrier => "Deep-space carrier",
        }
    }

    pub fn cost(self) -> &'static [(Item, u32)] {
        match self {
            Hull::Scout => &[(Steel, 20), (RocketEngine, 1), (Circuit, 10)],
            Hull::Freighter => &[(Steel, 100), (RocketEngine, 4), (Circuit, 20)],
            Hull::Carrier => &[(TitaniumAlloy, 150), (RocketEngine, 10), (Computer, 20)],
        }
    }

    pub fn tech(self) -> Tech {
        match self {
            Hull::Scout => Tech::Rocketry,
            Hull::Freighter => Tech::CargoShips,
            Hull::Carrier => Tech::DeepSpaceCarriers,
        }
    }

    /// Cargo capacity in item units.
    pub fn cargo(self) -> u32 {
        match self {
            Hull::Scout => 60,
            Hull::Freighter => 400,
            Hull::Carrier => 2000,
        }
    }

    /// Passenger seats (people need proper transport conditions).
    pub fn seats(self) -> u32 {
        match self {
            Hull::Scout => 2,
            Hull::Freighter => 10,
            Hull::Carrier => 40,
        }
    }

    /// Crew taken from the shipyard planet's population.
    pub fn crew(self) -> u32 {
        match self {
            Hull::Scout => 1,
            Hull::Freighter => 2,
            Hull::Carrier => 4,
        }
    }

    pub fn build_ticks(self) -> u32 {
        match self {
            Hull::Scout => 1200,
            Hull::Freighter => 2400,
            Hull::Carrier => 6000,
        }
    }

    /// Travel speed factor in percent (scouts are light and fast).
    pub fn speed_pct(self) -> i64 {
        match self {
            Hull::Scout => 150,
            Hull::Freighter => 100,
            Hull::Carrier => 120,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Drive {
    /// Within one star system.
    Chemical,
    /// Between star systems of one galaxy.
    Warp,
    /// Between galaxies.
    Void,
}

impl Drive {
    pub const ALL: [Drive; 3] = [Drive::Chemical, Drive::Warp, Drive::Void];

    pub fn name(self) -> &'static str {
        match self {
            Drive::Chemical => "Chemical",
            Drive::Warp => "Warp",
            Drive::Void => "Void",
        }
    }

    pub fn cost(self) -> &'static [(Item, u32)] {
        match self {
            Drive::Chemical => &[],
            Drive::Warp => &[(WarpDrive, 1)],
            Drive::Void => &[(VoidDrive, 1)],
        }
    }

    pub fn tech(self) -> Option<Tech> {
        match self {
            Drive::Chemical => None,
            Drive::Warp => Some(Tech::WarpTheory),
            Drive::Void => Some(Tech::VoidPropulsion),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShipOrder {
    pub hull: Hull,
    pub drive: Drive,
}

impl ShipOrder {
    pub fn cost(self) -> Inventory {
        let mut inv = Inventory::with(self.hull.cost());
        for &(i, n) in self.drive.cost() {
            inv.add(i, n);
        }
        inv
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShipLoc {
    Docked(PlanetId),
    Transit {
        from: PlanetId,
        to: PlanetId,
        depart: u64,
        arrive: u64,
    },
}

/// A recurring automated shipment between two planets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Route {
    pub a: PlanetId,
    pub b: PlanetId,
    /// Items to load at `a` for delivery to `b`.
    pub load_a: Vec<(Item, u32)>,
    /// Items to load at `b` for the return trip.
    pub load_b: Vec<(Item, u32)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ship {
    pub id: u32,
    pub name: String,
    pub hull: Hull,
    pub drive: Drive,
    pub loc: ShipLoc,
    pub cargo: Inventory,
    pub passengers: u32,
    pub crew: u32,
    pub surveying: bool,
    pub route: Option<Route>,
    /// Ticks spent waiting for cargo on an automated route.
    pub wait: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TripPlan {
    pub ticks: u64,
    pub fuel: (Item, u32),
}

impl Ship {
    pub fn free_cargo(&self) -> u32 {
        self.hull.cargo().saturating_sub(self.cargo.total())
    }

    pub fn docked_at(&self) -> Option<PlanetId> {
        match self.loc {
            ShipLoc::Docked(p) => Some(p),
            ShipLoc::Transit { .. } => None,
        }
    }

    /// Travel time and fuel for a trip, or why it is impossible.
    pub fn plan_trip(
        &self,
        uni: &Universe,
        from: PlanetId,
        to: PlanetId,
    ) -> Result<TripPlan, String> {
        if from == to {
            return Err("Already there".into());
        }
        let (Some(pa), Some(pb)) = (uni.planet(from), uni.planet(to)) else {
            return Err("Unknown destination".into());
        };
        let speed = self.hull.speed_pct();
        let plan = if from.system() == to.system() {
            let d = (pa.orbit as i64 - pb.orbit as i64).abs();
            TripPlan {
                ticks: ((300 + 150 * d) * 100 / speed) as u64,
                fuel: (Fuel, 5 + 3 * d as u32),
            }
        } else if from.galaxy == to.galaxy {
            if self.drive == Drive::Chemical {
                return Err("Needs a warp drive for interstellar travel".into());
            }
            let d = uni.system_distance(from.system(), to.system());
            TripPlan {
                ticks: ((1200 + 12 * d) * 100 / speed) as u64,
                fuel: (Fuel, 20 + (d / 10) as u32),
            }
        } else {
            if self.drive != Drive::Void {
                return Err("Needs a void drive for intergalactic travel".into());
            }
            let d = uni.system_distance(from.system(), to.system());
            TripPlan {
                ticks: ((6000 + d) * 100 / speed) as u64,
                fuel: (ExoticMatter, 10 + (d / 1000) as u32),
            }
        };
        Ok(plan)
    }

    pub(crate) fn hash_into(&self, feed: &mut impl FnMut(&[u8])) {
        feed(&self.id.to_le_bytes());
        feed(&[self.hull as u8, self.drive as u8, self.surveying as u8]);
        match self.loc {
            ShipLoc::Docked(p) => {
                feed(&[0]);
                feed(&p.galaxy.to_le_bytes());
                feed(&p.system.to_le_bytes());
                feed(&[p.index]);
            }
            ShipLoc::Transit {
                from,
                to,
                depart,
                arrive,
            } => {
                feed(&[1]);
                for p in [from, to] {
                    feed(&p.galaxy.to_le_bytes());
                    feed(&p.system.to_le_bytes());
                    feed(&[p.index]);
                }
                feed(&depart.to_le_bytes());
                feed(&arrive.to_le_bytes());
            }
        }
        self.cargo.hash_into(feed);
        feed(&self.passengers.to_le_bytes());
        feed(&self.crew.to_le_bytes());
        feed(&self.wait.to_le_bytes());
    }
}
