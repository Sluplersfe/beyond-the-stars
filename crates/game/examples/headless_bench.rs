//! Headless benchmark: runs the simulation schedule with no window or renderer.
//! Usage: cargo run -p game --example headless_bench --release [ticks]

use std::time::Instant;

use bevy::prelude::*;
use sim::Sim;

#[derive(Resource)]
struct SimState(Sim);

fn main() {
    let ticks: u32 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(1000);

    let mut sim = Sim::new(1);
    sim.spawn_grid(100, 100); // 10,000 tiles

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
        "ticks={} tiles=10000 elapsed={:.3}s ticks_per_sec={:.1} hash={:#018x}",
        ticks,
        elapsed,
        ticks as f64 / elapsed,
        state.0.state_hash()
    );
}

fn step_sim(mut state: ResMut<SimState>) {
    state.0.step();
}
