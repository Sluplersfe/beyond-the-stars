//! Mouse and keyboard → commands. Input never touches the sim directly; it queues commands.

use bevy::prelude::*;
use sim::buildings::BeltTier;
use sim::{BuildingKind, Command, Dir, Placeable};

use crate::camera::MainCamera;
use crate::menu::{MenuAction, MenuQueue, MenuState, gameplay};
use crate::state::*;
use crate::ui::{PointerOverUi, UiAction, UiActions};

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Tool::Select)
            .insert_resource(BuildDir(Dir::E))
            .insert_resource(Selected(Selection::None))
            .insert_resource(OpenWindow(Popup::None))
            .init_resource::<HoverTile>()
            .init_resource::<FocusShip>()
            .init_resource::<DragState>()
            .add_systems(
                Update,
                (
                    global_keys.in_set(GlobalKeys).run_if(gameplay),
                    cursor_tile,
                    planet_mouse.run_if(gameplay),
                    track_focus,
                )
                    .chain(),
            );
    }
}

/// The system that turns hotkeys into actions; menu systems run after it.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct GlobalKeys;

#[derive(Resource, Default)]
struct DragState {
    last: Option<(i32, i32)>,
}

/// Cursor ray hit on the ground plane, in surface-plane coordinates.
pub fn cursor_ground(
    window: &bevy::window::Window,
    cam: (&Camera, &GlobalTransform),
) -> Option<Vec2> {
    let pos = window.cursor_position()?;
    let ray = cam.0.viewport_to_world(cam.1, pos).ok()?;
    let hit = ray.plane_intersection_point(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y))?;
    Some(to_plane(hit))
}

fn cursor_tile(
    view: Res<View>,
    window: Single<&bevy::window::Window>,
    cam: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut hover: ResMut<HoverTile>,
    over_ui: Res<PointerOverUi>,
    menu: Res<MenuState>,
    debug: Option<Res<crate::debug::DebugTile>>,
) {
    if menu.open {
        hover.0 = None;
        return;
    }
    if let Some(t) = debug.and_then(|d| d.0)
        && matches!(*view, View::Planet(_))
    {
        hover.0 = Some(t);
        return;
    }
    hover.0 = match *view {
        View::Planet(_) if !over_ui.0 => cursor_ground(&window, (cam.0, cam.1)).map(world_to_tile),
        _ => None,
    };
}

#[allow(clippy::too_many_arguments)]
fn global_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut tool: ResMut<Tool>,
    mut dir: ResMut<BuildDir>,
    mut window: ResMut<OpenWindow>,
    mut selected: ResMut<Selected>,
    view: Res<View>,
    sim: Res<SimState>,
    mut acts: ResMut<UiActions>,
    mut menu: ResMut<MenuQueue>,
    mut sfx: ResMut<crate::audio::SfxQueue>,
) {
    let r = &sim.0.research;
    if keys.just_pressed(KeyCode::Escape) {
        if window.0 != Popup::None {
            window.0 = Popup::None;
        } else if *tool != Tool::Select {
            *tool = Tool::Select;
        } else if selected.0 != Selection::None {
            selected.0 = Selection::None;
        } else {
            menu.0.push(MenuAction::Pause);
        }
    }
    if keys.just_pressed(KeyCode::KeyQ) {
        *tool = Tool::Select;
    }
    if keys.just_pressed(KeyCode::KeyX) {
        *tool = Tool::Remove;
    }
    if keys.just_pressed(KeyCode::KeyR) {
        dir.0 = dir.0.rotate_cw();
        sfx.0.push(crate::audio::Sfx::Rotate);
    }
    for (k, w) in [
        (KeyCode::KeyT, Popup::Research),
        (KeyCode::KeyU, Popup::TechTree),
        (KeyCode::KeyO, Popup::Codex),
        (KeyCode::KeyB, Popup::Ships),
        (KeyCode::KeyL, Popup::Logistics),
        (KeyCode::KeyC, Popup::Colonies),
        (KeyCode::F1, Popup::Help),
    ] {
        if keys.just_pressed(k) {
            window.0 = if window.0 == w { Popup::None } else { w };
        }
    }
    if keys.just_pressed(KeyCode::F5) {
        acts.0.push(UiAction::Save);
    }
    if keys.just_pressed(KeyCode::F9) {
        acts.0.push(UiAction::Load);
    }
    if keys.just_pressed(KeyCode::KeyM) {
        match *view {
            View::Planet(p) => acts.0.push(UiAction::ViewMap(MapLevel::System(p.system()))),
            View::Map(_) => {
                let target = match selected.0 {
                    Selection::Planet(p) if sim.0.surface(p).is_some() => p,
                    _ => sim.0.player.at.unwrap_or(sim::HOME),
                };
                acts.0.push(UiAction::ViewPlanet(target));
            }
        }
    }
    if !matches!(*view, View::Planet(_)) {
        return;
    }
    // Build hotkeys. Pressing E again cycles through unlocked belt tiers.
    if keys.just_pressed(KeyCode::KeyE) {
        let tiers: Vec<BeltTier> = BeltTier::ALL
            .iter()
            .copied()
            .filter(|t| t.tech().is_none_or(|x| r.has(x)))
            .collect();
        let next = match *tool {
            Tool::Build(Placeable::Belt(t)) => {
                let i = tiers.iter().position(|x| *x == t).unwrap_or(0);
                tiers[(i + 1) % tiers.len()]
            }
            _ => tiers[tiers.len() - 1],
        };
        *tool = Tool::Build(Placeable::Belt(next));
    }
    let best = |a: BuildingKind, b: BuildingKind| {
        if b.spec().tech.is_none_or(|t| r.has(t)) {
            b
        } else {
            a
        }
    };
    for (k, kind) in [
        (
            KeyCode::KeyI,
            best(BuildingKind::Inserter, BuildingKind::FastInserter),
        ),
        (
            KeyCode::KeyN,
            best(BuildingKind::Miner, BuildingKind::MinerMk2),
        ),
        (
            KeyCode::KeyF,
            best(BuildingKind::Smelter, BuildingKind::SmelterMk2),
        ),
        (
            KeyCode::KeyY,
            best(BuildingKind::Assembler, BuildingKind::AssemblerMk2),
        ),
        (KeyCode::KeyG, BuildingKind::Generator),
        (KeyCode::KeyH, BuildingKind::House),
    ] {
        if keys.just_pressed(k) {
            *tool = Tool::Build(Placeable::Building(kind));
        }
    }
}

/// Anchor (bottom-left) so the footprint is centred on the cursor tile.
pub fn anchor_for(what: Placeable, tile: (i32, i32)) -> (i32, i32) {
    let (w, h) = what.size();
    (tile.0 - (w - 1) / 2, tile.1 - (h - 1) / 2)
}

#[allow(clippy::too_many_arguments)]
fn planet_mouse(
    view: Res<View>,
    mouse: Res<ButtonInput<MouseButton>>,
    hover: Res<HoverTile>,
    tool: Res<Tool>,
    mut dir: ResMut<BuildDir>,
    mut queue: ResMut<CommandQueue>,
    mut selected: ResMut<Selected>,
    mut drag: ResMut<DragState>,
    sim: Res<SimState>,
) {
    let View::Planet(planet) = *view else {
        return;
    };
    let Some(tile) = hover.0 else {
        if !mouse.pressed(MouseButton::Left) {
            drag.last = None;
        }
        return;
    };
    if !mouse.pressed(MouseButton::Left) && !mouse.pressed(MouseButton::Right) {
        drag.last = None;
        return;
    }
    let fresh = mouse.just_pressed(MouseButton::Left) || mouse.just_pressed(MouseButton::Right);
    if !fresh && drag.last == Some(tile) {
        return;
    }
    let prev = drag.last;
    drag.last = Some(tile);

    if mouse.pressed(MouseButton::Right) {
        queue.push(Command::Remove {
            planet,
            x: tile.0,
            y: tile.1,
        });
        return;
    }
    match *tool {
        Tool::Select => {
            if !fresh {
                return;
            }
            selected.0 = match sim
                .0
                .surface(planet)
                .and_then(|s| s.building_at(tile.0, tile.1))
            {
                Some(b) => Selection::Building(planet, b.id),
                None => Selection::None,
            };
        }
        Tool::Remove => queue.push(Command::Remove {
            planet,
            x: tile.0,
            y: tile.1,
        }),
        Tool::Build(what) => {
            if let (Placeable::Belt(_), Some(p)) = (what, prev) {
                // Dragging belts: face the drag direction, and turn the previous tile too.
                let d = (tile.0 - p.0, tile.1 - p.1);
                let nd = match d {
                    (1, 0) => Some(Dir::E),
                    (-1, 0) => Some(Dir::W),
                    (0, 1) => Some(Dir::N),
                    (0, -1) => Some(Dir::S),
                    _ => None,
                };
                if let Some(nd) = nd {
                    if nd != dir.0
                        && let Some(s) = sim.0.surface(planet)
                        && s.tile(p.0, p.1).and_then(|t| t.belt).is_some()
                    {
                        queue.push(Command::Remove {
                            planet,
                            x: p.0,
                            y: p.1,
                        });
                        queue.push(Command::Place {
                            planet,
                            x: p.0,
                            y: p.1,
                            what,
                            dir: nd,
                        });
                    }
                    dir.0 = nd;
                }
            } else if !fresh {
                return;
            }
            if fresh
                && matches!(what, Placeable::Belt(_))
                && sim
                    .0
                    .surface(planet)
                    .and_then(|s| s.tile(tile.0, tile.1))
                    .and_then(|t| t.belt)
                    .is_some()
            {
                // Clicking an existing belt with the belt tool removes it.
                queue.push(Command::Remove {
                    planet,
                    x: tile.0,
                    y: tile.1,
                });
                return;
            }
            if fresh
                && let Some(b) = sim
                    .0
                    .surface(planet)
                    .and_then(|s| s.building_at(tile.0, tile.1))
            {
                // Clicking an existing building with a build tool inspects it instead.
                selected.0 = Selection::Building(planet, b.id);
                return;
            }
            let (x, y) = anchor_for(what, tile);
            queue.push(Command::Place {
                planet,
                x,
                y,
                what,
                dir: dir.0,
            });
        }
    }
}

fn track_focus(selected: Res<Selected>, mut focus: ResMut<FocusShip>) {
    if let Selection::Ship(id) = selected.0
        && focus.0 != Some(id)
    {
        focus.0 = Some(id);
    }
}
