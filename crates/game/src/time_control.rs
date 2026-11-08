//! Pause, speed multipliers and Instant-Warp. The sim is stepped from `Update` with a
//! per-frame time budget, so very high multipliers degrade gracefully to "as fast as the
//! CPU allows" instead of stalling the frame.

use std::time::{Duration, Instant};

use bevy::prelude::*;
use sim::TICKS_PER_SEC;

use crate::state::*;

pub struct TimeControlPlugin;

pub const SPEEDS: [u32; 16] = [
    1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192, 16384, 32768,
];

/// Frame time the sim may use before the remaining backlog is dropped.
const STEP_BUDGET: Duration = Duration::from_millis(12);

#[derive(Resource)]
pub struct Speed(pub u32);

/// Measured simulation rate, for the time display.
#[derive(Resource, Default)]
pub struct SimRate {
    pub ticks_per_sec: f32,
    owed: f64,
}

impl SimRate {
    /// Achieved multiplier over real time.
    pub fn effective(&self) -> f32 {
        self.ticks_per_sec / TICKS_PER_SEC as f32
    }
}

impl Plugin for TimeControlPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Speed(1))
            .init_resource::<Warp>()
            .init_resource::<SimRate>()
            .configure_sets(Update, SimStep.run_if(crate::menu::gameplay))
            .add_systems(
                Update,
                (
                    time_keys,
                    clamp_speed,
                    step_sim.run_if(crate::net::offline),
                    instant_warp.run_if(crate::net::offline),
                )
                    .chain()
                    .in_set(SimStep),
            );
    }
}

/// Systems that advance the simulation; readers of sim state run after this.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct SimStep;

pub fn next_speed(cur: u32, max: u32, up: bool) -> u32 {
    let idx = SPEEDS.iter().position(|s| *s == cur).unwrap_or(0);
    let i = if up {
        (idx + 1).min(SPEEDS.len() - 1)
    } else {
        idx.saturating_sub(1)
    };
    SPEEDS[i].min(max)
}

fn time_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut speed: ResMut<Speed>,
    mut virt: ResMut<Time<Virtual>>,
    sim: Res<SimState>,
    mut log: ResMut<MessageLog>,
    net: Res<crate::net::Net>,
) {
    if keys.just_pressed(KeyCode::Space) {
        if net.in_game() {
            net.request_pause(!net.paused);
        } else {
            virt.toggle();
        }
    }
    let max = sim.0.research.max_speed();
    let mut want = None;
    for (k, s) in [
        (KeyCode::Digit1, 1),
        (KeyCode::Digit2, 4),
        (KeyCode::Digit3, 16),
        (KeyCode::Digit4, 64),
        (KeyCode::Digit5, 1024),
    ] {
        if keys.just_pressed(k) {
            want = Some(s);
        }
    }
    let idx = SPEEDS.iter().position(|s| *s == speed.0).unwrap_or(0);
    if keys.just_pressed(KeyCode::Equal) || keys.just_pressed(KeyCode::NumpadAdd) {
        want = Some(SPEEDS[(idx + 1).min(SPEEDS.len() - 1)]);
    }
    if keys.just_pressed(KeyCode::Minus) || keys.just_pressed(KeyCode::NumpadSubtract) {
        want = Some(SPEEDS[idx.saturating_sub(1)]);
    }
    if let Some(s) = want {
        if s > max {
            log.push(
                MsgKind::Warn,
                format!("{s}x is not unlocked yet (max {max}x). Research time acceleration."),
            );
        }
        if net.in_game() {
            net.request_speed(s.min(max).min(net.max_speed));
        } else {
            speed.0 = s.min(max);
            virt.unpause();
        }
    }
}

fn clamp_speed(mut speed: ResMut<Speed>, sim: Res<SimState>) {
    let max = sim.0.research.max_speed();
    if speed.0 > max {
        speed.0 = max;
    }
}

#[allow(clippy::too_many_arguments)]
fn step_sim(
    time: Res<Time<Real>>,
    virt: Res<Time<Virtual>>,
    speed: Res<Speed>,
    warp: Res<Warp>,
    mut rate: ResMut<SimRate>,
    mut sim: ResMut<SimState>,
    mut queue: ResMut<CommandQueue>,
    mut log: ResMut<MessageLog>,
    mut sfx: ResMut<crate::audio::SfxQueue>,
) {
    crate::sim_bridge::apply_all(&mut sim.0, &mut queue, &mut log, &mut sfx);
    let dt = time.delta_secs_f64().min(0.1);
    if virt.is_paused() || warp.0.is_some() {
        rate.owed = 0.0;
        rate.ticks_per_sec = 0.0;
        return;
    }
    rate.owed += dt * TICKS_PER_SEC as f64 * speed.0 as f64;
    let start = Instant::now();
    let mut steps = 0u32;
    while rate.owed >= 1.0 {
        sim.0.step();
        rate.owed -= 1.0;
        steps += 1;
        if start.elapsed() > STEP_BUDGET {
            // Behind: drop the backlog rather than spiral.
            rate.owed = 0.0;
            break;
        }
    }
    if dt > 0.0 {
        let tps = steps as f32 / dt as f32;
        rate.ticks_per_sec += (tps - rate.ticks_per_sec) * 0.1;
    }
}

/// Advance simulation time until the selected ship arrives, within a per-frame budget.
fn instant_warp(
    mut warp: ResMut<Warp>,
    mut sim: ResMut<SimState>,
    mut queue: ResMut<CommandQueue>,
    mut log: ResMut<MessageLog>,
    mut sfx: ResMut<crate::audio::SfxQueue>,
) {
    let Some(ship) = warp.0 else {
        return;
    };
    crate::sim_bridge::apply_all(&mut sim.0, &mut queue, &mut log, &mut sfx);
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(14) {
        match sim.0.ticks_until_arrival(ship) {
            Some(_) => sim.0.step(),
            None => {
                warp.0 = None;
                break;
            }
        }
    }
}
