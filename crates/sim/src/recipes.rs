//! Production recipes. Times are in ticks at machine speed 1.

use serde::{Deserialize, Serialize};

use crate::items::Item::{self, *};
use crate::research::Tech;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Category {
    Smelting,
    Assembly,
    Chemistry,
}

pub struct RecipeInfo {
    pub name: &'static str,
    pub category: Category,
    pub inputs: &'static [(Item, u32)],
    pub outputs: &'static [(Item, u32)],
    pub ticks: u32,
    pub tech: Option<Tech>,
}

macro_rules! recipes {
    ($($id:ident: $name:expr, $cat:ident, [$($ii:ident * $in:expr),*] => [$($oi:ident * $on:expr),*], $t:expr, $tech:expr;)*) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        pub enum Recipe {
            $($id,)*
        }

        impl Recipe {
            pub const ALL: &'static [Recipe] = &[$(Recipe::$id,)*];

            pub fn info(self) -> RecipeInfo {
                match self {
                    $(Recipe::$id => RecipeInfo {
                        name: $name,
                        category: Category::$cat,
                        inputs: &[$(($ii, $in)),*],
                        outputs: &[$(($oi, $on)),*],
                        ticks: $t,
                        tech: $tech,
                    },)*
                }
            }
        }
    };
}

recipes! {
    // Smelting
    SmeltIron: "Iron plate", Smelting, [IronOre * 1] => [IronPlate * 1], 64, None;
    SmeltCopper: "Copper plate", Smelting, [CopperOre * 1] => [CopperPlate * 1], 64, None;
    SmeltGlass: "Glass", Smelting, [Stone * 2] => [Glass * 1], 64, None;
    SmeltSteel: "Steel", Smelting, [IronPlate * 5, Carbon * 1] => [Steel * 1], 256, Some(Tech::Metallurgy);
    SmeltTitanium: "Titanium alloy", Smelting, [TitaniumOre * 2, Steel * 1] => [TitaniumAlloy * 1], 320, Some(Tech::TitaniumProcessing);
    // Chemistry
    MeltIce: "Melt ice", Chemistry, [Ice * 1] => [Water * 2], 40, Some(Tech::Chemistry);
    RefineFuel: "Fuel", Chemistry, [Water * 1, Carbon * 2] => [Fuel * 2], 100, Some(Tech::Chemistry);
    BioFuel: "Bio-fuel", Chemistry, [Biomass * 2] => [Fuel * 3], 100, Some(Tech::Chemistry);
    MakePlastic: "Plastic", Chemistry, [Gas * 2, Carbon * 1] => [Plastic * 2], 100, Some(Tech::Chemistry);
    BioPlastic: "Bio-plastic", Chemistry, [Biomass * 2, Water * 1] => [Plastic * 2], 120, Some(Tech::Chemistry);
    RefineExotic: "Exotic matter", Chemistry, [ExoticCrystal * 2, Fuel * 2] => [ExoticMatter * 1], 400, Some(Tech::ExoticPhysics);
    // Assembly
    MakeWire: "Copper wire", Assembly, [CopperPlate * 1] => [Wire * 2], 20, None;
    MakeGear: "Machine parts", Assembly, [IronPlate * 2] => [Gear * 1], 20, None;
    MakePipe: "Pipe", Assembly, [IronPlate * 1] => [Pipe * 1], 20, None;
    MakeCircuit: "Circuit", Assembly, [Wire * 3, IronPlate * 1] => [Circuit * 1], 40, None;
    MakeMotor: "Motor", Assembly, [Gear * 1, IronPlate * 1, Wire * 2] => [Motor * 1], 60, None;
    MakeComputer: "Computer", Assembly, [Circuit * 4, Glass * 1, Plastic * 2] => [Computer * 1], 160, Some(Tech::AdvancedElectronics);
    MakeNav: "Navigation system", Assembly, [Computer * 1, Circuit * 2, Glass * 2] => [NavSystem * 1], 200, Some(Tech::AdvancedElectronics);
    MakeRobot: "Robotic parts", Assembly, [Motor * 1, Circuit * 2, Steel * 1] => [RobotParts * 1], 120, Some(Tech::Robotics);
    MakeEngine: "Rocket engine", Assembly, [Motor * 2, Pipe * 4, Steel * 4] => [RocketEngine * 1], 240, Some(Tech::Rocketry);
    MakeColonyKit: "Colony kit", Assembly, [Steel * 20, RobotParts * 4, Computer * 2] => [ColonyKit * 1], 600, Some(Tech::Colonization);
    MakeSpeedModule: "Speed module", Assembly, [Computer * 2, Circuit * 5] => [SpeedModule * 1], 300, Some(Tech::Modules);
    MakeEffModule: "Efficiency module", Assembly, [Computer * 2, Circuit * 5] => [EfficiencyModule * 1], 300, Some(Tech::Modules);
    MakeReactorCore: "Reactor core", Assembly, [TitaniumAlloy * 5, Computer * 2, Pipe * 10] => [ReactorCore * 1], 400, Some(Tech::NuclearPower);
    MakeFuelCell: "Fuel cell", Assembly, [TitaniumAlloy * 1, Fuel * 10] => [FuelCell * 1], 200, Some(Tech::NuclearPower);
    MakeWarpDrive: "Warp drive", Assembly, [TitaniumAlloy * 10, Computer * 2, NavSystem * 1] => [WarpDrive * 1], 800, Some(Tech::WarpTheory);
    MakeVoidDrive: "Void drive", Assembly, [ExoticMatter * 10, WarpDrive * 1, ReactorCore * 1] => [VoidDrive * 1], 1200, Some(Tech::VoidPropulsion);
    MakeBeaconCore: "Beacon core", Assembly, [VoidShard * 20, ExoticMatter * 10, Computer * 10] => [BeaconCore * 1], 1600, Some(Tech::FrontierBeacon);
}

impl Recipe {
    /// First smelting recipe that uses `item` as an input (smelters auto-select).
    pub fn smelting_for(item: Item) -> Option<Recipe> {
        Recipe::ALL.iter().copied().find(|r| {
            let i = r.info();
            i.category == Category::Smelting && i.inputs.first().map(|x| x.0) == Some(item)
        })
    }
}
