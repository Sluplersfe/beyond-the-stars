//! Debug hooks driven by environment variables, for automated visual checks:
//! `BTS_SHOT=out.png` saves a screenshot after `BTS_SHOT_FRAME` frames (default 90) and exits.
//! `BTS_VIEW=system|galaxy|universe|research|techtree|codex|logistics|colonies|help` opens a view.
//! `BTS_DEMO=1` builds a small sample factory and runs the sim forward.
//! `BTS_SCRIPT="btn:Research [T];key:KeyE;tile:-10,9;..."` plays inputs, one step every
//! 12 frames: UI buttons get real pointer events at their on-screen centre.

use crate::ui::BtnLabel;
use bevy::input::InputSystems;
use bevy::picking::pointer::{Location, PointerAction, PointerButton, PointerId, PointerInput};
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use bevy::ui::UiGlobalTransform;
use bevy::ui_widgets::Button;
use bevy::window::{PrimaryWindow, WindowRef};
use sim::{BeltTier, BuildingKind, Command, Dir, HOME, Placeable, Recipe};

use crate::state::*;

pub struct DebugPlugin;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        if std::env::var("BTS_DEMO").is_ok() {
            app.add_systems(Startup, demo_factory);
        }
        if let Ok(v) = std::env::var("BTS_VIEW") {
            app.add_systems(
                Update,
                move |mut view: ResMut<View>,
                      mut win: ResMut<OpenWindow>,
                      mut done: Local<bool>| {
                    if *done {
                        return;
                    }
                    *done = true;
                    match v.as_str() {
                        "system" => *view = View::Map(MapLevel::System(HOME.system())),
                        "galaxy" => *view = View::Map(MapLevel::Galaxy(0)),
                        "universe" => *view = View::Map(MapLevel::Universe),
                        "research" => win.0 = Popup::Research,
                        "techtree" => win.0 = Popup::TechTree,
                        "codex" => win.0 = Popup::Codex,
                        "logistics" => win.0 = Popup::Logistics,
                        "colonies" => win.0 = Popup::Colonies,
                        "help" => win.0 = Popup::Help,
                        _ => {}
                    }
                },
            );
        }
        if let Ok(page) = std::env::var("BTS_MENU") {
            // Open a menu page (main, new, load, settings, pause) for screenshots.
            app.add_systems(
                Update,
                move |mut queue: ResMut<crate::menu::MenuQueue>, mut n: Local<u32>| {
                    use crate::menu::{MenuAction as A, Page};
                    *n += 1;
                    if *n != 3 {
                        return;
                    }
                    match page.as_str() {
                        "new" => queue.0.push(A::Open(Page::NewGame)),
                        "load" => queue.0.push(A::Open(Page::Load)),
                        "settings" => queue.0.push(A::Open(Page::Settings)),
                        "pause" => queue.0.push(A::Pause),
                        _ => {}
                    }
                },
            );
        }
        if let Ok(step) = std::env::var("BTS_TUTORIAL") {
            let step: usize = step.parse().unwrap_or(0);
            app.add_systems(
                Update,
                move |mut t: ResMut<crate::tutorial::Tutorial>, mut n: Local<u32>| {
                    *n += 1;
                    if *n == 3 {
                        t.restore(crate::save::TutorialSave {
                            step: Some(step as u32),
                            finished: false,
                        });
                    }
                },
            );
        }
        if let Ok(script) = std::env::var("BTS_SCRIPT") {
            let steps: Vec<String> = script.split(';').map(|s| s.trim().to_string()).collect();
            app.insert_resource(Script { steps, frame: 0 })
                .init_resource::<DebugTile>()
                .add_systems(PreUpdate, run_script.after(InputSystems));
        }
        if let Ok(z) = std::env::var("BTS_ZOOM") {
            let z: f32 = z.parse().unwrap_or(0.5);
            app.add_systems(
                Update,
                move |mut rig: ResMut<crate::camera::CameraRig>,
                      sim: Res<SimState>,
                      mut n: Local<u32>| {
                    *n += 1;
                    if *n != 5 {
                        return;
                    }
                    let c = sim.0.surface(HOME).map(|s| s.w / 2).unwrap_or(32);
                    let f = to_world(tile_center(c - 2, c + 2), 0.0);
                    rig.snap(f, 46.0 * z);
                },
            );
        }
        if let Ok(path) = std::env::var("BTS_SHOT") {
            let frame: u32 = std::env::var("BTS_SHOT_FRAME")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(90);
            // Exit once the file is written (pipelines may still be compiling at `frame`,
            // so a slow first frame never cuts the capture short).
            let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            app.add_systems(
                Update,
                move |mut commands: Commands,
                      mut n: Local<u32>,
                      mut exit: MessageWriter<AppExit>| {
                    use std::sync::atomic::Ordering;
                    *n += 1;
                    if *n == frame {
                        let flag = done.clone();
                        commands
                            .spawn(Screenshot::primary_window())
                            .observe(save_to_disk(path.clone()))
                            .observe(move |_: On<ScreenshotCaptured>| {
                                flag.store(true, Ordering::SeqCst);
                            });
                    }
                    // Give the save observer a few frames, then leave.
                    if (*n > frame + 5 && done.load(Ordering::SeqCst)) || *n == frame + 1200 {
                        exit.write(AppExit::Success);
                    }
                },
            );
        }
    }
}

fn demo_factory(mut sim: ResMut<SimState>, mut selected: ResMut<Selected>) {
    let sim = &mut sim.0;
    let c = sim.surface(HOME).map(|s| s.w / 2).unwrap_or(32);
    let place = |sim: &mut sim::Sim, what, x, y, dir| {
        let r = sim.apply(Command::Place {
            planet: HOME,
            x,
            y,
            what,
            dir,
        });
        if let Err(e) = r {
            warn!("demo place failed at {x},{y}: {e}");
        }
    };
    let b = |k| Placeable::Building(k);
    // Iron: miner on the iron patch → belt → inserter → smelter → inserter → belt → depot.
    let (ix, iy) = (c - 13, c + 8);
    place(sim, b(BuildingKind::Miner), ix, iy, Dir::E);
    for x in ix + 2..c - 5 {
        place(sim, Placeable::Belt(BeltTier::Basic), x, iy, Dir::E);
    }
    place(sim, b(BuildingKind::Inserter), c - 5, iy, Dir::E);
    place(sim, b(BuildingKind::Smelter), c - 4, iy, Dir::E);
    place(sim, b(BuildingKind::Inserter), c - 4, iy - 1, Dir::S);
    for y in c + 1..iy - 1 {
        let d = if y == c + 1 { Dir::E } else { Dir::S };
        place(sim, Placeable::Belt(BeltTier::Basic), c - 4, y, d);
    }
    place(sim, Placeable::Belt(BeltTier::Basic), c - 3, c + 1, Dir::E);
    place(sim, Placeable::Belt(BeltTier::Basic), c - 2, c + 1, Dir::E);
    place(sim, Placeable::Belt(BeltTier::Basic), c - 1, c + 1, Dir::S);
    place(sim, b(BuildingKind::Lab), c - 8, c + 3, Dir::N);
    place(sim, b(BuildingKind::Assembler), c + 6, c + 6, Dir::N);
    let _ = sim.apply(Command::SelectResearch(sim::Tech::Time2));
    if let Some(id) = sim
        .surface(HOME)
        .and_then(|s| s.building_at(c + 6, c + 6))
        .map(|b| b.id)
    {
        let _ = sim.apply(Command::SetRecipe {
            planet: HOME,
            building: id,
            recipe: Some(Recipe::MakeGear),
        });
        selected.0 = Selection::Building(HOME, id);
    }
    for _ in 0..600 {
        sim.step();
    }
    if std::env::var("BTS_DEMO").as_deref() == Ok("2") {
        // A scout carrying the overseer to Glacia, mid-flight.
        if let Some(c) = sim
            .universe
            .planet_mut(HOME)
            .and_then(|p| p.colony.as_mut())
        {
            c.inventory.add(sim::Item::Fuel, 50);
        }
        let ship = sim.spawn_ship(HOME, sim::Hull::Scout, sim::Drive::Chemical);
        let glacia = sim::PlanetId { index: 2, ..HOME };
        let _ = sim.apply(Command::Ride { ship: Some(ship) });
        let _ = sim.apply(Command::Launch { ship, to: glacia });
        let other = sim.spawn_ship(HOME, sim::Hull::Freighter, sim::Drive::Chemical);
        let _ = sim.apply(Command::Launch {
            ship: other,
            to: sim::PlanetId { index: 1, ..HOME },
        });
        for _ in 0..150 {
            sim.step();
        }
    }
}

#[derive(Resource)]
struct Script {
    steps: Vec<String>,
    frame: u32,
}

/// Tile the scripted "mouse" is over (overrides the real cursor in planet view).
#[derive(Resource, Default)]
pub struct DebugTile(pub Option<(i32, i32)>);

fn key_code(name: &str) -> Option<KeyCode> {
    Some(match name {
        "Escape" => KeyCode::Escape,
        "Space" => KeyCode::Space,
        "F1" => KeyCode::F1,
        "Digit1" => KeyCode::Digit1,
        "Digit2" => KeyCode::Digit2,
        "KeyE" => KeyCode::KeyE,
        "KeyI" => KeyCode::KeyI,
        "KeyN" => KeyCode::KeyN,
        "KeyF" => KeyCode::KeyF,
        "KeyQ" => KeyCode::KeyQ,
        "KeyR" => KeyCode::KeyR,
        "KeyM" => KeyCode::KeyM,
        "KeyT" => KeyCode::KeyT,
        "KeyX" => KeyCode::KeyX,
        _ => return None,
    })
}

#[allow(clippy::too_many_arguments)]
fn run_script(
    mut script: ResMut<Script>,
    buttons: Query<(&BtnLabel, &UiGlobalTransform), With<Button>>,
    window: Single<(Entity, &bevy::window::Window), With<PrimaryWindow>>,
    mut pointer: MessageWriter<PointerInput>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut tile: ResMut<DebugTile>,
    sim: Res<SimState>,
) {
    script.frame += 1;
    let f = script.frame;
    // Release whatever the previous step pressed.
    if f % 12 == 2 {
        keys.release_all();
        mouse.release_all();
    }
    if !f.is_multiple_of(12) {
        return;
    }
    let idx = (f / 12) as usize - 1;
    let Some(step) = script.steps.get(idx).cloned() else {
        return;
    };
    let (kind, arg) = step.split_once(':').unwrap_or((step.as_str(), ""));
    let c = sim.0.surface(HOME).map(|s| s.w / 2).unwrap_or(32);
    match kind {
        "btn" => {
            let hit = buttons.iter().find(|(label, _)| label.0.starts_with(arg));
            match hit {
                Some((_, tf)) => {
                    let pos = tf.translation / window.1.scale_factor();
                    let Some(target) = WindowRef::Primary.normalize(Some(window.0)) else {
                        return;
                    };
                    let location = Location {
                        target: bevy::camera::NormalizedRenderTarget::Window(target),
                        position: pos,
                    };
                    for action in [
                        PointerAction::Move { delta: Vec2::ZERO },
                        PointerAction::Press(PointerButton::Primary),
                        PointerAction::Release(PointerButton::Primary),
                    ] {
                        pointer.write(PointerInput::new(
                            PointerId::Mouse,
                            location.clone(),
                            action,
                        ));
                    }
                    info!("script: clicked button '{arg}' at {pos}");
                }
                None => warn!("script: no button '{arg}'"),
            }
        }
        "key" => match key_code(arg) {
            Some(k) => keys.press(k),
            None => warn!("script: unknown key {arg}"),
        },
        "tile" | "rtile" => {
            let (x, y) = arg.split_once(',').unwrap_or(("0", "0"));
            let (x, y): (i32, i32) = (x.parse().unwrap_or(0), y.parse().unwrap_or(0));
            tile.0 = Some((c + x, c + y));
            mouse.press(if kind == "tile" {
                MouseButton::Left
            } else {
                MouseButton::Right
            });
        }
        "report" => {
            let s = &sim.0;
            info!(
                "script report: research={:?} buildings={} belts={} tick={}",
                s.research.current,
                s.surface(HOME).map(|x| x.buildings.len()).unwrap_or(0),
                s.surface(HOME).map(|x| x.belt_count()).unwrap_or(0),
                s.tick()
            );
        }
        _ => {}
    }
}
