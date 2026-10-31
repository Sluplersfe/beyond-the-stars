//! Items and local inventories.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

macro_rules! items {
    ($($name:ident => $label:expr, $tier:expr, $rgb:expr;)*) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        pub enum Item {
            $($name,)*
        }

        impl Item {
            pub const ALL: &'static [Item] = &[$(Item::$name,)*];

            pub fn name(self) -> &'static str {
                match self {
                    $(Item::$name => $label,)*
                }
            }

            /// Economy tier 1-6 (raw → exotic).
            pub fn tier(self) -> u8 {
                match self {
                    $(Item::$name => $tier,)*
                }
            }

            /// Display colour as 0xRRGGBB. Used only by rendering.
            pub fn rgb(self) -> u32 {
                match self {
                    $(Item::$name => $rgb,)*
                }
            }
        }
    };
}

items! {
    // Tier 1: raw materials
    IronOre => "Iron ore", 1, 0x8a6d5a;
    CopperOre => "Copper ore", 1, 0xc0703a;
    Stone => "Stone", 1, 0x9a9488;
    Water => "Water", 1, 0x3a7ad8;
    Carbon => "Carbon", 1, 0x2a2a2a;
    Ice => "Ice", 1, 0xbfe6ff;
    Gas => "Atmospheric gas", 1, 0x9fe0c0;
    TitaniumOre => "Titanium ore", 1, 0x7fa0b8;
    Biomass => "Biomass", 1, 0x4fa040;
    ExoticCrystal => "Exotic crystal", 6, 0xd040f0;
    VoidShard => "Void shard", 6, 0x6020a0;
    // Tier 2: refined
    IronPlate => "Iron plate", 2, 0xb8c0c8;
    CopperPlate => "Copper plate", 2, 0xe89060;
    Wire => "Copper wire", 2, 0xf0a040;
    Glass => "Glass", 2, 0xd0f0f8;
    Steel => "Steel", 2, 0x8890a0;
    TitaniumAlloy => "Titanium alloy", 2, 0xa0d0f0;
    Fuel => "Fuel", 2, 0xf0d030;
    Plastic => "Plastic", 2, 0xf0f0e0;
    // Tier 3: industrial components
    Gear => "Machine parts", 3, 0x909090;
    Pipe => "Pipe", 3, 0x707880;
    Motor => "Motor", 3, 0x5070a0;
    Circuit => "Circuit", 3, 0x30c050;
    RobotParts => "Robotic parts", 3, 0xe0e060;
    // Tier 4: advanced technology
    Computer => "Computer", 4, 0x20a0a0;
    NavSystem => "Navigation system", 4, 0x2060e0;
    ReactorCore => "Reactor core", 4, 0x40f0a0;
    SpeedModule => "Speed module", 4, 0x40a0ff;
    EfficiencyModule => "Efficiency module", 4, 0x60ff60;
    // Tier 5: spacefaring
    RocketEngine => "Rocket engine", 5, 0xff6030;
    ColonyKit => "Colony kit", 5, 0xffa0d0;
    WarpDrive => "Warp drive", 5, 0x8080ff;
    // Tier 6: exotic
    ExoticMatter => "Exotic matter", 6, 0xff40ff;
    FuelCell => "Fuel cell", 6, 0x80ff40;
    VoidDrive => "Void drive", 6, 0x4000ff;
    BeaconCore => "Beacon core", 6, 0xffffff;
}

/// A local inventory. Resources always belong to a planet, ship, or building.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Inventory {
    items: BTreeMap<Item, u32>,
}

impl Inventory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(items: &[(Item, u32)]) -> Self {
        let mut inv = Self::new();
        for &(i, n) in items {
            inv.add(i, n);
        }
        inv
    }

    pub fn get(&self, item: Item) -> u32 {
        self.items.get(&item).copied().unwrap_or(0)
    }

    pub fn add(&mut self, item: Item, n: u32) {
        if n > 0 {
            *self.items.entry(item).or_insert(0) += n;
        }
    }

    /// Remove up to `n`; returns how many were removed.
    pub fn take(&mut self, item: Item, n: u32) -> u32 {
        let Some(have) = self.items.get_mut(&item) else {
            return 0;
        };
        let t = (*have).min(n);
        *have -= t;
        if *have == 0 {
            self.items.remove(&item);
        }
        t
    }

    pub fn has_all(&self, cost: &[(Item, u32)]) -> bool {
        cost.iter().all(|&(i, n)| self.get(i) >= n)
    }

    /// Remove the whole cost atomically. Returns false (and changes nothing) if short.
    pub fn pay(&mut self, cost: &[(Item, u32)]) -> bool {
        if !self.has_all(cost) {
            return false;
        }
        for &(i, n) in cost {
            self.take(i, n);
        }
        true
    }

    pub fn total(&self) -> u32 {
        self.items.values().sum()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (Item, u32)> + '_ {
        self.items.iter().map(|(&i, &n)| (i, n))
    }

    pub fn merge(&mut self, other: Inventory) {
        for (i, n) in other.items {
            self.add(i, n);
        }
    }

    pub(crate) fn hash_into(&self, feed: &mut impl FnMut(&[u8])) {
        for (i, n) in &self.items {
            feed(&[*i as u8]);
            feed(&n.to_le_bytes());
        }
        feed(&[0xff]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pay_is_atomic() {
        let mut inv = Inventory::with(&[(Item::IronPlate, 5)]);
        assert!(!inv.pay(&[(Item::IronPlate, 3), (Item::Gear, 1)]));
        assert_eq!(inv.get(Item::IronPlate), 5);
        assert!(inv.pay(&[(Item::IronPlate, 3)]));
        assert_eq!(inv.get(Item::IronPlate), 2);
    }
}
