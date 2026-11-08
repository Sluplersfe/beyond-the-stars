//! The only place that mutates the sim: queued commands and fixed steps.

use bevy::prelude::*;
use sim::{Event, PlanetId, Sim};

use crate::audio::{Sfx, SfxQueue};
use crate::state::*;

pub struct SimBridgePlugin;

impl Plugin for SimBridgePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CommandQueue>()
            .init_resource::<MessageLog>()
            .init_resource::<GenerationOverlay>()
            .add_systems(
                Update,
                drain_events
                    .after(crate::time_control::SimStep)
                    .after(crate::net::MpStep),
            );
    }
}

pub fn apply_all(
    sim: &mut Sim,
    queue: &mut CommandQueue,
    log: &mut MessageLog,
    sfx: &mut SfxQueue,
) {
    for cmd in queue.0.drain(..) {
        let sound = command_sound(&cmd);
        match sim.apply(cmd) {
            Ok(()) => sfx.0.extend(sound),
            // Dragging the remove tool over empty ground is not worth a message.
            Err(e) if e == "Nothing to remove" => {}
            Err(e) => {
                log.push(MsgKind::Error, e);
                sfx.0.push(Sfx::Error);
            }
        }
    }
}

/// The sound a successfully applied command makes.
pub(crate) fn command_sound(cmd: &sim::Command) -> Option<Sfx> {
    use sim::Command as C;
    Some(match cmd {
        C::Place {
            what: sim::Placeable::Belt(_),
            ..
        } => Sfx::Belt,
        C::Place { .. } => Sfx::Build,
        C::Remove { .. } => Sfx::Demolish,
        C::SelectResearch(_) => Sfx::ResearchStart,
        C::Launch { .. } => Sfx::Launch,
        C::SetRecipe { .. }
        | C::SetFilter { .. }
        | C::SetRateLimits { .. }
        | C::InsertModule { .. }
        | C::OrderShip { .. }
        | C::Load { .. }
        | C::Unload { .. }
        | C::Board { .. }
        | C::Disembark { .. }
        | C::SetSurvey { .. }
        | C::FoundColony { .. }
        | C::ActivateColony { .. }
        | C::SetRoute { .. }
        | C::SetPolicy { .. }
        | C::Ride { .. } => Sfx::Confirm,
    })
}

const GEN_JOKES: [&str; 12] = [
    "Reticulating orbital splines...",
    "Convincing gravity to cooperate...",
    "Polishing freshly minted asteroids...",
    "Negotiating with the local vacuum...",
    "Assigning planets their personalities...",
    "Sprinkling ore deposits, artisanally...",
    "Teaching photons where to go...",
    "Rolling dice. Einstein disapproves...",
    "Untangling a minor spacetime knot...",
    "Inventing a few new shades of rock...",
    "Persuading the star to keep burning...",
    "Checking for pre-existing landlords...",
];

const QUOTES: [&str; 10] = [
    "\"The universe is under no obligation to be convenient.\" - Dr. Ilsa Varn, theoretical physicist",
    "\"Every vacuum has a price, and it is usually fuel.\" - Prof. Otto Kreml",
    "\"I measured the cosmos twice. It was bigger the second time.\" - Dr. Mina Solberg",
    "\"Entropy is just the universe tidying up wrong.\" - Prof. Hal Quistrom",
    "\"Somewhere out there, a planet is waiting to be a factory.\" - Dr. Aurelio Pask",
    "\"Light is fast. Paperwork is faster.\" - Prof. Dana Ilyich",
    "\"There is no such thing as a free photon.\" - Dr. Renske Mol",
    "\"Gravity: the original logistics constraint.\" - Prof. Tomas Ekwueme",
    "\"If it isn't on the manifest, it isn't on the ship.\" - Dr. Sefa Lindqvist",
    "\"Time is relative. Deadlines are absolute.\" - Prof. Kai Brenner",
];

pub fn planet_name(sim: &Sim, id: PlanetId) -> String {
    sim.planet(id)
        .map(|p| p.name.clone())
        .unwrap_or_else(|| "?".into())
}

fn drain_events(
    mut sim: ResMut<SimState>,
    mut log: ResMut<MessageLog>,
    mut overlay: ResMut<GenerationOverlay>,
    mut warp: ResMut<Warp>,
    time: Res<Time<Real>>,
    mut sfx: ResMut<SfxQueue>,
    mut launches: ResMut<crate::render::Launches>,
) {
    let events = sim.0.take_events();
    let sim = &sim.0;
    for e in events {
        sfx.0.extend(match &e {
            Event::ResearchComplete(_) | Event::SurveyComplete(_) | Event::ColonyActivated(_) => {
                Some(Sfx::Research)
            }
            Event::ShipBuilt { .. } | Event::ShipArrived { .. } | Event::ColonyFounded(_) => {
                Some(Sfx::Ship)
            }
            Event::PowerShortage(_) | Event::ColonyAbandoned(_) => Some(Sfx::Alert),
            Event::Attack {
                repelled: false, ..
            } => Some(Sfx::Attack),
            Event::Generated { .. } => Some(Sfx::Generate),
            Event::Victory => Some(Sfx::Victory),
            _ => None,
        });
        match e {
            Event::ResearchComplete(t) => {
                log.push(MsgKind::Good, format!("Research complete: {}", t.info().name))
            }
            Event::ShipBuilt { ship, planet } => {
                launches.0.push(planet);
                log.push(
                MsgKind::Good,
                format!(
                    "{} launched from the shipyard at {}",
                    sim.ships.get(&ship).map(|s| s.name.as_str()).unwrap_or("Ship"),
                    planet_name(sim, planet)
                ),
            )
            }
            Event::ShipArrived { ship, planet } => {
                if warp.0 == Some(ship) {
                    warp.0 = None;
                }
                log.push(
                    MsgKind::Info,
                    format!(
                        "{} arrived at {}",
                        sim.ships.get(&ship).map(|s| s.name.as_str()).unwrap_or("Ship"),
                        planet_name(sim, planet)
                    ),
                )
            }
            Event::SurveyComplete(p) => {
                log.push(MsgKind::Good, format!("Survey of {} complete", planet_name(sim, p)))
            }
            Event::SystemsDetected(n) => {
                log.push(MsgKind::Info, format!("Scan detected {n} new star system(s)"))
            }
            Event::Generated { what, name } => {
                let h = name.bytes().fold(time.elapsed().as_millis() as usize, |a, b| {
                    a.wrapping_mul(31).wrapping_add(b as usize)
                });
                if overlay.remaining <= 0.0 {
                    overlay.lines.clear();
                }
                overlay.lines.push(format!("Generating {what}: {name}"));
                overlay.lines.push(GEN_JOKES[h % GEN_JOKES.len()].to_string());
                if overlay.lines.len() <= 2 {
                    overlay.lines.push(QUOTES[(h / 7) % QUOTES.len()].to_string());
                }
                overlay.lines.truncate(9);
                overlay.remaining = 4.5;
            }
            Event::ColonyFounded(p) => log.push(
                MsgKind::Good,
                format!(
                    "Colony founded on {}. Visit it in person to establish work systems.",
                    planet_name(sim, p)
                ),
            ),
            Event::ColonyActivated(p) => log.push(
                MsgKind::Good,
                format!("Work systems established on {}", planet_name(sim, p)),
            ),
            Event::ColonyAbandoned(p) => log.push(
                MsgKind::Warn,
                format!("{} has been abandoned: human work stopped", planet_name(sim, p)),
            ),
            Event::PopulationGrew(_) => {}
            Event::PowerShortage(p) => {
                log.push(MsgKind::Warn, format!("Power shortage on {}", planet_name(sim, p)))
            }
            Event::Attack {
                planet,
                repelled,
                killed,
            } => {
                let name = planet_name(sim, planet);
                if repelled {
                    log.push(MsgKind::Info, format!("Defenses repelled an attack on {name}"));
                } else if killed {
                    log.push(MsgKind::Error, format!("Hostiles killed a colonist on {name}"));
                } else {
                    log.push(MsgKind::Warn, format!("Hostiles damaged a building on {name}"));
                }
            }
            Event::NestDestroyed(p) => {
                log.push(MsgKind::Good, format!("Hostile nest destroyed on {}", planet_name(sim, p)))
            }
            Event::Victory => log.push(
                MsgKind::Good,
                "The Frontier Beacon is lit! Your civilization spans the galaxies. (You can keep playing.)",
            ),
        }
    }
}
