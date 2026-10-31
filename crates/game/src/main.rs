mod audio;
mod camera;
mod debug;
mod gallery;
mod hud;
mod icons;
mod input;
mod map;
mod menu;
mod menu_web;
mod mesh_util;
mod models;
mod panels;
mod paths;
mod render;
mod save;
mod settings;
mod sim_bridge;
mod state;
mod textures;
mod time_control;
mod tutorial;
mod ui;
#[cfg(feature = "html-menu")]
mod webview;

use bevy::prelude::*;
use bevy::window::WindowResolution;
use sim::{HOME, Sim};

use menu::{MenuState, Screen};
use settings::Settings;
use state::{SimState, View};

/// Debug hooks that need the game world on screen skip the menu.
fn wants_menu() -> bool {
    let skip = std::env::args().nth(1).is_some()
        || std::env::var_os("BTS_NOMENU").is_some()
        || ["BTS_DEMO", "BTS_ZOOM", "BTS_TUTORIAL", "BTS_GALLERY"]
            .iter()
            .any(|k| std::env::var_os(k).is_some())
        || std::env::var("BTS_VIEW").is_ok_and(|v| v != "menu")
        || std::env::var("BTS_MENU").is_ok_and(|v| v == "pause");
    !skip
}

fn main() {
    let seed = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let menu_first = wants_menu();
    let settings = Settings::load();

    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Beyond the Stars".into(),
            resolution: WindowResolution::new(1600, 900),
            ..default()
        }),
        ..default()
    }))
    .insert_resource(settings)
    .insert_resource(SimState(Sim::new(seed)))
    .insert_resource(if menu_first {
        View::Map(state::MapLevel::Galaxy(0))
    } else {
        View::Planet(HOME)
    })
    .insert_resource(if menu_first {
        Screen::MainMenu
    } else {
        Screen::Playing
    })
    .insert_resource(MenuState::new(menu_first))
    .add_plugins((
        settings::SettingsPlugin,
        audio::SfxPlugin,
        sim_bridge::SimBridgePlugin,
        time_control::TimeControlPlugin,
        camera::CameraPlugin,
        input::InputPlugin,
        render::RenderPlugin,
        map::MapPlugin,
        icons::IconPlugin,
        hud::HudPlugin,
        ui::UiPlugin,
        menu::MenuPlugin,
        tutorial::TutorialPlugin,
        debug::DebugPlugin,
    ));
    #[cfg(feature = "html-menu")]
    app.add_plugins(webview::WebviewPlugin);
    app.add_plugins(gallery::GalleryPlugin);
    app.run();
}
