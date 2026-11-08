//! Shows the user's `assets/menu/background.html` behind the main menu in a native child
//! webview (wry) and draws the menu on top of it from inside the page.
//!
//! A webview is a separate OS surface, so Bevy UI cannot be drawn over it; the menu is
//! therefore serialised (`menu_web`) and rendered by an injected script. When the webview
//! cannot be created (Wayland, missing WebKitGTK, no background file) or the page never
//! answers, the native menu is used instead.

use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use bevy::window::{PrimaryWindow, RawHandleWrapper};
use wry::dpi::{PhysicalPosition, PhysicalSize};
use wry::raw_window_handle::RawWindowHandle;
use wry::{Rect, WebView, WebViewBuilder};

use crate::camera::MainCamera;
use crate::menu::{self, MenuState, MenuView, Screen, Session};
use crate::menu_web::{self, Frame, Incoming};
use crate::settings::Settings;
use crate::state::SimState;
use crate::tutorial::Tutorial;
use crate::ui::UiActions;

/// Seconds to wait for the page's `ready` message before using the native menu.
const READY_TIMEOUT: f32 = 5.0;

pub struct WebviewPlugin;

impl Plugin for WebviewPlugin {
    fn build(&self, app: &mut App) {
        app.insert_non_send(WebMenu::default())
            .add_systems(Update, sync);
    }
}

#[derive(Default)]
struct WebMenu {
    view: Option<WebView>,
    inbox: Arc<Mutex<Vec<String>>>,
    gtk_ready: bool,
    /// Creation failed or timed out; stay on the native menu for this run.
    failed: bool,
    ready: bool,
    age: f32,
    frame: Frame,
    version: u64,
    last_json: String,
    size: (u32, u32),
    shown: bool,
}

impl WebMenu {
    fn init_gtk(&mut self) -> bool {
        if !self.gtk_ready {
            self.gtk_ready = gtk::init().is_ok();
        }
        self.gtk_ready
    }

    /// Hide instead of destroying: tearing the child X window down races GDK's own
    /// property queries and GDK exits the process on the resulting BadWindow error.
    fn hide(&mut self) {
        if let Some(v) = &self.view {
            let _ = v.set_visible(false);
            let _ = v.focus_parent();
        }
        self.shown = false;
    }

    fn show(&mut self) {
        if let Some(v) = &self.view {
            let _ = v.set_visible(true);
        }
        self.shown = true;
        self.last_json.clear();
    }
}

fn create(
    web: &mut WebMenu,
    handle: &RawHandleWrapper,
    (w, h): (u32, u32),
) -> Result<WebView, String> {
    let path = crate::paths::menu_background().ok_or("assets/menu/background.html not found")?;
    let html = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !matches!(handle.get_window_handle(), RawWindowHandle::Xlib(_)) {
        return Err("the window is not an X11 window (embedded webviews need X11)".into());
    }
    if !web.init_gtk() {
        return Err("GTK could not be initialised".into());
    }
    let inbox = web.inbox.clone();
    let builder = WebViewBuilder::new()
        .with_bounds(Rect {
            position: PhysicalPosition::new(0, 0).into(),
            size: PhysicalSize::new(w, h).into(),
        })
        .with_background_color((3, 5, 13, 255))
        .with_initialization_script_for_main_only(menu_web::OVERLAY_JS, true)
        .with_ipc_handler(move |req| {
            if let Ok(mut q) = inbox.lock() {
                q.push(req.body().clone());
            }
        })
        // The page is local and self-contained; never navigate away from it.
        .with_navigation_handler(|url| {
            ["about:", "data:", "blob:", "file:"]
                .iter()
                .any(|p| url.starts_with(p))
        })
        .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
        .with_html(html);
    // SAFETY: called from a main-thread (non-send) system.
    let parent = unsafe { handle.get_handle() };
    builder.build_as_child(&parent).map_err(|e| e.to_string())
}

#[allow(clippy::too_many_arguments)]
fn sync(
    mut web: NonSendMut<WebMenu>,
    screen: Res<Screen>,
    mut menu: ResMut<MenuState>,
    settings: Res<Settings>,
    session: Res<Session>,
    sim: Res<SimState>,
    tutorial: Res<Tutorial>,
    net: Res<crate::net::Net>,
    time: Res<Time<Real>>,
    window: Single<(&Window, &RawHandleWrapper), With<PrimaryWindow>>,
    mut camera: Single<&mut Camera, With<MainCamera>>,
    mut acts: ResMut<UiActions>,
) {
    let (win, handle) = *window;
    let size = (win.physical_width(), win.physical_height());
    let want = *screen == Screen::MainMenu && !web.failed && size.0 > 0 && size.1 > 0;

    if want && web.view.is_none() {
        match create(&mut web, handle, size) {
            Ok(v) => {
                info!("main menu: showing assets/menu/background.html");
                web.view = Some(v);
                web.size = size;
                web.shown = true;
            }
            Err(e) => {
                warn!("html main menu unavailable ({e}); using the built-in menu");
                web.failed = true;
            }
        }
    } else if want && !web.shown {
        web.show();
    } else if !want && web.shown {
        web.hide();
    }

    let live = web.view.is_some() && web.shown;
    if web.view.is_some() {
        // Pump GTK so the embedded WebKit view runs (and hides cleanly).
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }
    }
    if menu.web != live {
        menu.web = live;
    }
    let want_active = !live;
    if camera.is_active != want_active {
        camera.is_active = want_active;
    }
    if !live {
        return;
    }

    web.age += time.delta_secs();
    if !web.ready && web.age > READY_TIMEOUT {
        warn!("background page never became ready; using the built-in menu");
        web.failed = true;
        web.hide();
        menu.web = false;
        camera.is_active = true;
        return;
    }

    if web.size != size {
        web.size = size;
        if let Some(v) = &web.view {
            let _ = v.set_bounds(Rect {
                position: PhysicalPosition::new(0, 0).into(),
                size: PhysicalSize::new(size.0, size.1).into(),
            });
        }
    }

    let msgs: Vec<String> = web
        .inbox
        .lock()
        .map(|mut q| std::mem::take(&mut *q))
        .unwrap_or_default();
    for m in msgs {
        match menu_web::decode(&m) {
            Some(Incoming::Ready) => {
                web.ready = true;
                web.last_json.clear();
            }
            Some(msg) => {
                if let Some(a) = msg.to_action(&web.frame, web.version) {
                    acts.0.push(a);
                }
            }
            None => {}
        }
    }
    if !web.ready {
        return;
    }

    let nodes = menu::content(&MenuView {
        menu: &menu,
        settings: &settings,
        session: &session,
        sim: &sim.0,
        tutorial_active: tutorial.active,
        net: &net,
        playing: false,
    });
    let page = menu_web::page_name(menu.page);
    let mut frame = menu_web::encode(page, web.version, &nodes);
    if frame.json != web.last_json {
        web.version += 1;
        frame = menu_web::encode(page, web.version, &nodes);
        let script = format!("window.__bts_set && window.__bts_set({})", frame.json);
        if let Some(v) = &web.view {
            let _ = v.evaluate_script(&script);
        }
        web.last_json = frame.json.clone();
        web.frame = frame;
    }
}
