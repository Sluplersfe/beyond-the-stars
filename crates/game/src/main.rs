use bevy::prelude::*;
use sim::Sim;

#[derive(Resource)]
struct SimState(Sim);

fn main() {
    let mut sim = Sim::new(1);
    sim.spawn_grid(64, 64);

    App::new()
        .add_plugins(DefaultPlugins)
        .insert_resource(SimState(sim))
        // Simulation only advances in FixedUpdate. Pause = no fixed steps.
        .add_systems(FixedUpdate, step_sim)
        .run();
}

fn step_sim(mut state: ResMut<SimState>) {
    state.0.step();
}
