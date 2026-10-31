//! End-to-end check that the late game is reachable: intergalactic travel, survey,
//! colonization, in-person activation, reactors, and the Frontier Beacon victory.

use sim::universe::SystemKnowledge;
use sim::*;

fn home_inv(sim: &mut Sim) -> &mut Inventory {
    &mut sim
        .universe
        .planet_mut(HOME)
        .unwrap()
        .colony
        .as_mut()
        .unwrap()
        .inventory
}

fn run_until(sim: &mut Sim, max: u64, mut done: impl FnMut(&Sim) -> bool) {
    for _ in 0..max {
        if done(sim) {
            return;
        }
        sim.step();
    }
    assert!(done(sim), "condition not reached within {max} ticks");
}

/// Find an anchor where `what` fits (and `also` fits at the given offset), scanning
/// outward from the centre.
fn find_spot(
    sim: &Sim,
    p: PlanetId,
    what: Placeable,
    also: Option<(Placeable, i32, i32)>,
) -> (i32, i32) {
    let s = sim.surface(p).unwrap();
    let c = s.w / 2;
    for r in 0..s.w {
        for y in c - r..=c + r {
            for x in c - r..=c + r {
                let extra = also.is_none_or(|(a, dx, dy)| s.can_place(a, x + dx, y + dy).is_ok());
                if s.can_place(what, x, y).is_ok() && extra {
                    return (x, y);
                }
            }
        }
    }
    panic!("no room for {what:?}");
}

#[test]
fn late_game_path_reaches_victory() {
    let mut sim = Sim::new(7);
    for t in Tech::ALL {
        if *t != Tech::VoidPropulsion {
            sim.research.unlocked.insert(*t);
        }
    }
    // Researching void propulsion charts the neighbouring galaxies.
    sim.apply(Command::SelectResearch(Tech::VoidPropulsion))
        .unwrap();
    sim.research.pool = 1_000_000;
    sim.step();
    assert!(sim.research.has(Tech::VoidPropulsion));
    let gateway = SystemId {
        galaxy: 1,
        index: 0,
    };
    let sys = sim.universe.system(gateway).unwrap();
    assert_eq!(sys.knowledge, SystemKnowledge::Detected);
    let target = sys
        .planets
        .iter()
        .find(|p| p.profile.iter().any(|(i, _)| *i == Item::VoidShard))
        .expect("gateway system guarantees void shards")
        .id;

    let inv = home_inv(&mut sim);
    inv.add(Item::ExoticMatter, 200);
    inv.add(Item::ColonyKit, 1);
    inv.add(Item::BeaconCore, 10);
    inv.add(Item::TitaniumAlloy, 300);
    inv.add(Item::Computer, 60);
    inv.add(Item::Steel, 200);
    inv.add(Item::ReactorCore, 2);
    inv.add(Item::FuelCell, 40);
    inv.add(Item::Gear, 50);

    let scout = sim.spawn_ship(HOME, Hull::Scout, Drive::Void);
    let carrier = sim.spawn_ship(HOME, Hull::Carrier, Drive::Void);
    for (item, n) in [
        (Item::ColonyKit, 1),
        (Item::BeaconCore, 10),
        (Item::TitaniumAlloy, 300),
        (Item::Computer, 60),
        (Item::Steel, 200),
        (Item::ReactorCore, 2),
        (Item::FuelCell, 40),
        (Item::IronPlate, 50),
        (Item::Gear, 50),
        (Item::ExoticMatter, 60),
    ] {
        sim.apply(Command::Load {
            ship: carrier,
            item,
            qty: n,
        })
        .unwrap();
    }
    sim.apply(Command::Board {
        ship: carrier,
        people: 6,
    })
    .unwrap();
    sim.apply(Command::Ride {
        ship: Some(carrier),
    })
    .unwrap();

    sim.apply(Command::Launch {
        ship: scout,
        to: target,
    })
    .unwrap();
    sim.apply(Command::Launch {
        ship: carrier,
        to: target,
    })
    .unwrap();
    run_until(&mut sim, 100_000, |s| {
        s.ships[&scout].docked_at() == Some(target)
    });
    assert_eq!(
        sim.universe.system(gateway).unwrap().knowledge,
        SystemKnowledge::Visited
    );
    sim.apply(Command::SetSurvey {
        ship: scout,
        on: true,
    })
    .unwrap();
    run_until(&mut sim, 5_000, |s| s.planet(target).unwrap().surveyed());
    run_until(&mut sim, 100_000, |s| {
        s.ships[&carrier].docked_at() == Some(target)
    });
    assert_eq!(sim.player.at, Some(target));

    sim.apply(Command::FoundColony { ship: carrier }).unwrap();
    sim.apply(Command::ActivateColony { planet: target })
        .unwrap();
    let c = sim.colony(target).unwrap();
    assert_eq!(c.population, 6);
    assert_eq!(c.inventory.get(Item::BeaconCore), 10);

    // Two reactors fed by providers, then the beacon.
    for _ in 0..2 {
        let provider = Placeable::Building(BuildingKind::Provider);
        let (x, y) = find_spot(
            &sim,
            target,
            Placeable::Building(BuildingKind::Reactor),
            Some((provider, -1, 0)),
        );
        sim.apply(Command::Place {
            planet: target,
            x,
            y,
            what: Placeable::Building(BuildingKind::Reactor),
            dir: Dir::N,
        })
        .unwrap();
        // Provider directly left of the reactor, pushing east.
        let px = x - 1;
        let py = y;
        sim.apply(Command::Place {
            planet: target,
            x: px,
            y: py,
            what: Placeable::Building(BuildingKind::Provider),
            dir: Dir::E,
        })
        .unwrap();
        let id = sim.surface(target).unwrap().building_at(px, py).unwrap().id;
        sim.apply(Command::SetFilter {
            planet: target,
            building: id,
            item: Some(Item::FuelCell),
        })
        .unwrap();
    }
    let (bx, by) = find_spot(
        &sim,
        target,
        Placeable::Building(BuildingKind::Beacon),
        None,
    );
    sim.apply(Command::Place {
        planet: target,
        x: bx,
        y: by,
        what: Placeable::Building(BuildingKind::Beacon),
        dir: Dir::N,
    })
    .unwrap();
    let mut saw_victory = false;
    for _ in 0..3000 {
        sim.step();
        saw_victory |= sim.take_events().contains(&Event::Victory);
        if sim.victory.is_some() {
            break;
        }
    }
    let power = sim.colony(target).unwrap().power;
    assert!(sim.victory.is_some(), "no victory; power {power:?}");
    assert!(saw_victory);
}

#[test]
fn home_planet_has_every_early_resource() {
    // Plastic (computers, colony kits) must be makeable before any colony exists.
    let sim = Sim::new(1);
    let s = sim.surface(HOME).unwrap();
    for item in [
        Item::IronOre,
        Item::CopperOre,
        Item::Stone,
        Item::Carbon,
        Item::Water,
        Item::Gas,
    ] {
        let mut found = false;
        for y in 0..s.h {
            for x in 0..s.w {
                if s.tile(x, y).unwrap().terrain == surface::Terrain::Deposit(item) {
                    found = true;
                }
            }
        }
        assert!(found, "home lacks {item:?}");
    }
}

#[test]
fn scanning_detects_nearby_systems() {
    let mut sim = Sim::new(2);
    sim.research.unlocked.insert(Tech::Astronomy);
    sim.research.unlocked.insert(Tech::ScanII);
    let c = sim.surface(HOME).unwrap().w / 2;
    let (x, y) = (c - 8, c + 3);
    sim.apply(Command::Place {
        planet: HOME,
        x,
        y,
        what: Placeable::Building(BuildingKind::Observatory),
        dir: Dir::N,
    })
    .unwrap();
    for _ in 0..700 {
        sim.step();
    }
    let detected = sim.universe.galaxies[0]
        .systems
        .iter()
        .filter(|s| s.knowledge != SystemKnowledge::Unknown)
        .count();
    assert!(detected > 1, "observatory detected nothing");
}
