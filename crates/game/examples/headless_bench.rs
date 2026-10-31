//! Headless benchmark: runs the simulation schedule with no window or renderer.
//! The home surface is replaced by a 100x100 grid of express belts forming one loop,
//! half filled with items, so every tick moves 5,000 items.
//! Usage: cargo run -p game --example headless_bench --release [ticks]

use std::time::Instant;

use bevy::prelude::*;
use sim::{BeltTier, Dir, HOME, Item, Sim, Surface};

#[derive(Resource)]
struct SimState(Sim);

const N: i32 = 100;

fn belt_loop(seed: u64) -> Surface {
    let mut s = Surface::new(N, N, seed);
    for y in 0..N {
        for x in 1..N {
            let dir = if y % 2 == 0 {
                if x == N - 1 { Dir::N } else { Dir::E }
            } else if x == 1 && y != N - 1 {
                Dir::N
            } else {
                Dir::W
            };
            s.place_belt(x, y, dir, BeltTier::Express);
        }
        s.place_belt(
            0,
            y,
            if y == 0 { Dir::E } else { Dir::S },
            BeltTier::Express,
        );
    }
    for y in 0..N {
        for x in 0..N {
            if (x + y) % 2 == 0 {
                s.add_item(x, y, Item::IronOre);
            }
        }
    }
    s
}

fn main() {
    let ticks: u32 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(1000);

    let mut sim = Sim::new(1);
    let home = sim.universe.planet_mut(HOME).expect("home");
    home.surface = Some(belt_loop(1));
    let belts = home.surface.as_ref().map(|s| s.belt_count()).unwrap_or(0);

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(SimState(sim))
        .add_systems(FixedUpdate, step_sim);
    app.finish();
    app.cleanup();

    let start = Instant::now();
    for _ in 0..ticks {
        app.world_mut().run_schedule(FixedUpdate);
    }
    let elapsed = start.elapsed().as_secs_f64();

    let state = app.world().resource::<SimState>();
    println!(
        "ticks={} belts={} elapsed={:.3}s ticks_per_sec={:.1} hash={:#018x}",
        ticks,
        belts,
        elapsed,
        ticks as f64 / elapsed,
        state.0.state_hash()
    );
}

fn step_sim(mut state: ResMut<SimState>) {
    state.0.step();
}
