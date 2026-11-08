//! Main menu, pause menu, new game / load / settings pages, and the game lifecycle behind them
//! (starting, loading, saving, autosaving, leaving to the menu).
//!
//! Page content is described with the same `UiNode`s as the HUD, so it can be drawn natively
//! by `ui.rs` or serialised for the HTML overlay in `menu_web.rs`.

use bevy::ecs::system::SystemParam;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use sim::{HOME, Sim};

use crate::camera::CameraRig;
use crate::net::{self, MpHandoff, Net, Phase};
use crate::save::{self, SaveEntry};
use crate::settings::{AUTOSAVE_CHOICES, Settings};
use crate::state::*;
use crate::time_control::Speed;
use crate::tutorial::Tutorial;
use crate::ui::{ACCENT, Btn, DIM, HEAD, UiAction, UiNode};

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MenuQueue>()
            .init_resource::<Session>()
            .add_systems(
                Update,
                (
                    menu_keys,
                    field_input,
                    handle_menu_actions,
                    backdrop,
                    autosave,
                )
                    .chain()
                    .in_set(MenuSystems)
                    .after(crate::input::GlobalKeys),
            );
    }
}

/// The menu systems, so multiplayer can order itself after them.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct MenuSystems;

/// Which top-level screen the player is on. The sim keeps running behind `Playing` only.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    MainMenu,
    Playing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Page {
    Main,
    NewGame,
    Load,
    Settings,
    Pause,
    Multiplayer,
    CreateRoom,
    HostWorld,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FieldId {
    Name,
    Seed,
    Server,
    PlayerName,
    RoomName,
    RoomPass,
    Chat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    Master(i8),
    Sfx(i8),
    Music(i8),
    Muted,
    Fullscreen,
    Vsync,
    UiScale(i8),
    Bloom,
    Shadows,
    CameraSpeed(i8),
    InvertZoom,
    Autosave(i8),
    OfferTutorial,
    ShowFps,
    Reset,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MenuAction {
    Open(Page),
    Back,
    Resume,
    /// Open the pause menu from the game.
    Pause,
    Focus(FieldId),
    /// Text edited in the HTML overlay.
    SetField(FieldId, String),
    RandomSeed,
    ToggleTutorial,
    StartNew,
    Continue,
    Load(String),
    /// Reload the current game's slot (quick load).
    Reload,
    AskDelete(String),
    Delete(String),
    Cancel,
    LoadPage(i32),
    SaveNow,
    /// New tutorial game from the main menu, or restart the guide in the current game.
    Tutorial,
    ToMain,
    Quit,
    MpConnect,
    MpDisconnect,
    MpRefresh,
    MpCreate,
    MpMax(i8),
    /// Open the running single-player game as a room.
    MpHostCurrent,
    /// Open a saved game as a room.
    MpHost(String),
    MpJoin(String),
    MpDelete(String),
    /// A joined room's sim is ready in `MpHandoff`.
    MpBegin,
    /// The connection dropped mid-game.
    MpLost(String),
    ChatOpen,
    ChatSend,
    Set(Setting),
    /// Tutorial card buttons.
    Guide(crate::tutorial::GuideAction),
}

#[derive(Resource, Default)]
pub struct MenuQueue(pub Vec<MenuAction>);

/// The game currently being played and where it saves to.
#[derive(Resource)]
pub struct Session {
    pub slot: String,
    pub name: String,
    /// Manual saves this session (the tutorial watches it).
    pub manual_saves: u32,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            slot: "colony".into(),
            name: "Colony".into(),
            manual_saves: 0,
        }
    }
}

#[derive(Resource)]
pub struct MenuState {
    /// A menu is covering the game (always true on the main menu).
    pub open: bool,
    pub page: Page,
    stack: Vec<Page>,
    pub name: String,
    pub seed: String,
    pub tutorial: bool,
    pub focus: Option<FieldId>,
    pub saves: Vec<SaveEntry>,
    pub load_page: usize,
    /// Slot waiting for delete confirmation.
    pub confirm: Option<String>,
    pub notice: Option<String>,
    pub mp_server: String,
    pub mp_name: String,
    pub room_name: String,
    pub room_pass: String,
    pub room_max: u32,
    pub chat: String,
    was_paused: bool,
    /// The HTML overlay is drawing the menu instead of the native UI.
    pub web: bool,
}

impl MenuState {
    pub fn new(start_open: bool) -> Self {
        Self {
            open: start_open,
            page: if start_open { Page::Main } else { Page::Pause },
            stack: vec![],
            name: "Colony".into(),
            seed: String::new(),
            tutorial: true,
            focus: None,
            saves: save::list(),
            load_page: 0,
            confirm: None,
            notice: None,
            mp_server: String::new(),
            mp_name: String::new(),
            room_name: String::new(),
            room_pass: String::new(),
            room_max: 4,
            chat: String::new(),
            was_paused: false,
            web: false,
        }
    }

    fn root(screen: Screen) -> Page {
        match screen {
            Screen::MainMenu => Page::Main,
            Screen::Playing => Page::Pause,
        }
    }
}

/// Run condition: the game world accepts input (no menu covering it).
pub fn gameplay(menu: Res<MenuState>) -> bool {
    !menu.open && menu.focus != Some(FieldId::Chat)
}

pub const TUTORIAL_SEED: u64 = 1;
const SAVES_PER_PAGE: usize = 5;

/// Turn a typed seed into a number: digits as-is, words hashed, empty random.
pub fn seed_from_text(s: &str) -> u64 {
    let t = s.trim();
    if t.is_empty() {
        return random_seed();
    }
    t.parse::<u64>().unwrap_or_else(|_| {
        t.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
            (h ^ b as u64).wrapping_mul(0x0100_0000_01b3)
        })
    })
}

/// Full-range random number for identifiers (not gameplay).
fn random_seed_u64() -> u64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1);
    let mut x = nanos ^ ((std::process::id() as u64) << 32) ^ 0x9e37_79b9_7f4a_7c15;
    for _ in 0..3 {
        x ^= x >> 33;
        x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    }
    x ^ (x >> 29)
}

pub fn random_seed() -> u64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1);
    let mut x = nanos ^ 0x9e37_79b9_7f4a_7c15;
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    x % 1_000_000
}

// ---------------------------------------------------------------------------------------
// Page content
// ---------------------------------------------------------------------------------------

fn big(label: &str, note: impl Into<String>, action: MenuAction) -> UiNode {
    UiNode::BigButton {
        label: label.into(),
        note: note.into(),
        action: UiAction::Menu(action),
        style: Btn::Normal,
    }
}

fn primary(label: &str, note: impl Into<String>, action: MenuAction) -> UiNode {
    UiNode::BigButton {
        label: label.into(),
        note: note.into(),
        action: UiAction::Menu(action),
        style: Btn::Primary,
    }
}

fn small(label: &str, action: MenuAction) -> UiNode {
    UiNode::Button(label.into(), UiAction::Menu(action), Btn::Normal)
}

pub struct MenuView<'a> {
    pub menu: &'a MenuState,
    pub settings: &'a Settings,
    pub session: &'a Session,
    pub sim: &'a Sim,
    pub tutorial_active: bool,
    pub net: &'a Net,
    /// A single-player game is running behind the menu.
    pub playing: bool,
}

/// What the menu layer should show. Empty when no menu is open.
pub fn content(v: &MenuView) -> Vec<UiNode> {
    if !v.menu.open {
        return vec![];
    }
    match v.menu.page {
        Page::Main => main_page(v),
        Page::NewGame => new_game_page(v),
        Page::Load => load_page(v),
        Page::Settings => settings_page(v),
        Page::Pause => pause_page(v),
        Page::Multiplayer => multiplayer_page(v),
        Page::CreateRoom => create_room_page(v),
        Page::HostWorld => host_world_page(v),
    }
}

fn title(text: &str) -> UiNode {
    UiNode::sized(text, HEAD, 30.0)
}

fn notice(v: &MenuView) -> Vec<UiNode> {
    v.menu
        .notice
        .as_ref()
        .map(|n| vec![UiNode::colored(n.clone(), Color::srgb(1.0, 0.8, 0.4))])
        .unwrap_or_default()
}

fn main_page(v: &MenuView) -> Vec<UiNode> {
    let latest = v.menu.saves.first();
    let mut items = vec![
        UiNode::sized("BEYOND THE STARS", HEAD, 34.0),
        UiNode::sized(
            "Automation within locations. Logistics between locations.",
            DIM,
            13.5,
        ),
        UiNode::Gap,
        UiNode::Gap,
    ];
    if let Some(s) = latest {
        items.push(primary(
            "Continue",
            format!("{} - {}", s.meta.name, save::ago(s.meta.saved_unix)),
            MenuAction::Continue,
        ));
    }
    items.extend([
        big("New Game", "", MenuAction::Open(Page::NewGame)),
        big(
            "Load Game",
            if v.menu.saves.is_empty() {
                "no saves yet".to_string()
            } else {
                format!("{} saved", v.menu.saves.len())
            },
            MenuAction::Open(Page::Load),
        ),
        big("Tutorial", "a guided first colony", MenuAction::Tutorial),
        big("Multiplayer", "play with friends", MenuAction::Open(Page::Multiplayer)),
        big("Settings", "", MenuAction::Open(Page::Settings)),
        big("Quit", "", MenuAction::Quit),
    ]);
    items.extend(notice(v));
    items.push(UiNode::Gap);
    items.push(UiNode::dim(format!("v{}", env!("CARGO_PKG_VERSION"))));
    vec![UiNode::Overlay { left: true, items }]
}

fn pause_page(v: &MenuView) -> Vec<UiNode> {
    if v.net.in_game() {
        return mp_pause_page(v);
    }
    let mut items = vec![
        title("Paused"),
        UiNode::dim(format!("{} - {}", v.session.name, fmt_ticks(v.sim.tick()))),
        UiNode::Gap,
        primary("Resume", "Esc", MenuAction::Resume),
        big("Save Game", "F5", MenuAction::SaveNow),
        big("Load Game", "", MenuAction::Open(Page::Load)),
        big("Multiplayer", "host this world or join one", MenuAction::Open(Page::Multiplayer)),
        big("Settings", "", MenuAction::Open(Page::Settings)),
        big(
            "Tutorial",
            if v.tutorial_active {
                "restart the guide"
            } else {
                "start the guide"
            },
            MenuAction::Tutorial,
        ),
        big("Main Menu", "saves first", MenuAction::ToMain),
        big("Quit Game", "", MenuAction::Quit),
    ];
    items.extend(notice(v));
    vec![UiNode::Overlay {
        left: false,
        items: vec![UiNode::Panel {
            width: 460.0,
            items,
        }],
    }]
}

fn field(id: FieldId, value: &str, hint: &str, v: &MenuView) -> UiNode {
    UiNode::Field {
        id,
        value: value.to_string(),
        hint: hint.into(),
        focused: v.menu.focus == Some(id),
    }
}

fn mp_pause_page(v: &MenuView) -> Vec<UiNode> {
    let room = v.net.room.as_ref().map(|r| r.name.as_str()).unwrap_or("Room");
    let mut items = vec![
        title("Multiplayer"),
        UiNode::dim(format!(
            "{room} - {} - the game keeps running",
            fmt_ticks(v.sim.tick())
        )),
        UiNode::Gap,
        primary("Resume", "Esc", MenuAction::Resume),
        UiNode::dim(format!(
            "Online: {}",
            v.net
                .players
                .iter()
                .map(|(id, n)| if *id == v.net.you { format!("{n} (you)") } else { n.clone() })
                .collect::<Vec<_>>()
                .join(", ")
        )),
        big("Settings", "", MenuAction::Open(Page::Settings)),
        big("Leave Server", "back to the main menu", MenuAction::ToMain),
        big("Quit Game", "", MenuAction::Quit),
    ];
    items.extend(notice(v));
    vec![UiNode::Overlay {
        left: false,
        items: vec![UiNode::Panel {
            width: 460.0,
            items,
        }],
    }]
}

fn multiplayer_page(v: &MenuView) -> Vec<UiNode> {
    let m = v.menu;
    let n = v.net;
    let mut items = vec![title("Multiplayer")];
    match n.phase {
        Phase::Offline | Phase::Connecting => {
            items.extend([
                UiNode::dim("Server address"),
                field(FieldId::Server, &m.mp_server, net::DEFAULT_SERVER, v),
                UiNode::dim("Your name"),
                field(FieldId::PlayerName, &m.mp_name, "Player", v),
                UiNode::Gap,
            ]);
            if n.phase == Phase::Connecting {
                items.push(UiNode::dim("Connecting..."));
                items.push(big("Cancel", "", MenuAction::MpDisconnect));
            } else {
                items.push(primary("Connect", "", MenuAction::MpConnect));
            }
        }
        _ => {
            items.push(UiNode::dim(format!(
                "Connected as {} - {} room(s)",
                n.name,
                n.rooms.len()
            )));
            items.push(UiNode::dim("Password for locked rooms"));
            items.push(field(FieldId::RoomPass, &m.room_pass, "none", v));
            if n.rooms.is_empty() {
                items.push(UiNode::dim("No rooms yet. Create the first one."));
            }
            let mut cards = vec![];
            for r in &n.rooms {
                let full = r.players >= r.max;
                let lock = if r.locked { "locked | " } else { "" };
                let speed = if r.paused {
                    "paused".to_string()
                } else {
                    format!("{}x", r.speed)
                };
                let mut buttons = vec![UiNode::Button(
                    if full { "Full".into() } else { "Join".into() },
                    UiAction::Menu(MenuAction::MpJoin(r.id.clone())),
                    if full { Btn::Disabled } else { Btn::Active },
                )];
                buttons.push(small("Delete", MenuAction::MpDelete(r.id.clone())));
                cards.push(UiNode::Card(
                    vec![
                        UiNode::Spread(vec![
                            UiNode::sized(r.name.clone(), HEAD, 16.0),
                            UiNode::Row(buttons),
                        ]),
                        UiNode::dim(format!(
                            "{lock}{}/{} players | {} played | {speed} | seed {}",
                            r.players,
                            r.max,
                            fmt_ticks(r.tick),
                            r.seed
                        )),
                    ],
                    ACCENT,
                ));
            }
            if !cards.is_empty() {
                items.push(UiNode::Scroll {
                    key: "mp-rooms",
                    height: px(260),
                    items: cards,
                });
            }
            items.push(UiNode::Gap);
            items.push(primary("Create Room", "a new world", MenuAction::Open(Page::CreateRoom)));
            if v.playing {
                items.push(big(
                    "Host Current World",
                    v.session.name.clone(),
                    MenuAction::MpHostCurrent,
                ));
            }
            items.push(big(
                "Host a Saved World",
                format!("{} saved", m.saves.len()),
                MenuAction::Open(Page::HostWorld),
            ));
            items.push(UiNode::Row(vec![
                small("Refresh", MenuAction::MpRefresh),
                small("Disconnect", MenuAction::MpDisconnect),
            ]));
        }
    }
    items.push(big("Back", "Esc", MenuAction::Back));
    items.extend(notice(v));
    vec![UiNode::Overlay {
        left: false,
        items: vec![UiNode::Panel {
            width: 620.0,
            items,
        }],
    }]
}

fn create_room_page(v: &MenuView) -> Vec<UiNode> {
    let m = v.menu;
    let mut items = vec![
        title("Create Room"),
        UiNode::dim("Room name"),
        field(FieldId::RoomName, &m.room_name, "Our colony", v),
        UiNode::dim("World seed (a number or any word; empty = random)"),
        UiNode::Row(vec![
            UiNode::Fixed(300.0, vec![field(FieldId::Seed, &m.seed, "random", v)]),
            small("Randomize", MenuAction::RandomSeed),
        ]),
        UiNode::dim("Password (optional)"),
        field(FieldId::RoomPass, &m.room_pass, "none", v),
        UiNode::Row(vec![
            UiNode::dim(format!("Max players: {}", m.room_max)),
            small("-", MenuAction::MpMax(-1)),
            small("+", MenuAction::MpMax(1)),
        ]),
        UiNode::Gap,
        primary("Create", "", MenuAction::MpCreate),
        big("Back", "Esc", MenuAction::Back),
    ];
    items.extend(notice(v));
    vec![UiNode::Overlay {
        left: false,
        items: vec![UiNode::Panel {
            width: 520.0,
            items,
        }],
    }]
}

fn host_world_page(v: &MenuView) -> Vec<UiNode> {
    let m = v.menu;
    let mut items = vec![
        title("Host a Saved World"),
        UiNode::dim("Players join your world as it is now. The server keeps it afterwards."),
        UiNode::dim("Password (optional)"),
        field(FieldId::RoomPass, &m.room_pass, "none", v),
        UiNode::Row(vec![
            UiNode::dim(format!("Max players: {}", m.room_max)),
            small("-", MenuAction::MpMax(-1)),
            small("+", MenuAction::MpMax(1)),
        ]),
    ];
    if m.saves.is_empty() {
        items.push(UiNode::dim("No saved games yet."));
    }
    let pages = m.saves.len().div_ceil(SAVES_PER_PAGE).max(1);
    let page = m.load_page.min(pages - 1);
    for s in m.saves.iter().skip(page * SAVES_PER_PAGE).take(SAVES_PER_PAGE) {
        let label = if s.is_autosave() {
            format!("{} (autosave)", s.meta.name)
        } else {
            s.meta.name.clone()
        };
        items.push(UiNode::Card(
            vec![
                UiNode::Spread(vec![
                    UiNode::sized(label, HEAD, 16.0),
                    UiNode::Button(
                        "Host".into(),
                        UiAction::Menu(MenuAction::MpHost(s.slot.clone())),
                        Btn::Active,
                    ),
                ]),
                UiNode::dim(format!(
                    "{} played | {} colonies | {} people | {}",
                    fmt_ticks(s.meta.tick),
                    s.meta.colonies,
                    s.meta.population,
                    save::ago(s.meta.saved_unix)
                )),
            ],
            ACCENT,
        ));
    }
    if pages > 1 {
        items.push(UiNode::Row(vec![
            small("< Prev", MenuAction::LoadPage(-1)),
            UiNode::dim(format!("page {} / {pages}", page + 1)),
            small("Next >", MenuAction::LoadPage(1)),
        ]));
    }
    items.push(big("Back", "Esc", MenuAction::Back));
    items.extend(notice(v));
    vec![UiNode::Overlay {
        left: false,
        items: vec![UiNode::Panel {
            width: 640.0,
            items,
        }],
    }]
}

fn new_game_page(v: &MenuView) -> Vec<UiNode> {
    let m = v.menu;
    let mut items = vec![
        title("New Game"),
        UiNode::dim("Colony name"),
        UiNode::Field {
            id: FieldId::Name,
            value: m.name.clone(),
            hint: "Colony".into(),
            focused: m.focus == Some(FieldId::Name),
        },
        UiNode::dim("World seed (a number or any word; empty = random)"),
        UiNode::Row(vec![
            UiNode::Fixed(
                300.0,
                vec![UiNode::Field {
                    id: FieldId::Seed,
                    value: m.seed.clone(),
                    hint: "random".into(),
                    focused: m.focus == Some(FieldId::Seed),
                }],
            ),
            small("Randomize", MenuAction::RandomSeed),
        ]),
        UiNode::Gap,
        UiNode::Toggle(
            "Guided tutorial".into(),
            m.tutorial,
            UiAction::Menu(MenuAction::ToggleTutorial),
        ),
        UiNode::dim(
            "A short guide that walks you through your first factory. You can restart it later from the pause menu.",
        ),
        UiNode::Gap,
        primary("Start", "", MenuAction::StartNew),
        big("Back", "Esc", MenuAction::Back),
    ];
    items.extend(notice(v));
    vec![UiNode::Overlay {
        left: false,
        items: vec![UiNode::Panel {
            width: 520.0,
            items,
        }],
    }]
}

fn load_page(v: &MenuView) -> Vec<UiNode> {
    let m = v.menu;
    let mut items = vec![title("Load Game")];
    if m.saves.is_empty() {
        items.push(UiNode::dim("No saved games yet."));
    }
    let pages = m.saves.len().div_ceil(SAVES_PER_PAGE).max(1);
    let page = m.load_page.min(pages - 1);
    for s in m
        .saves
        .iter()
        .skip(page * SAVES_PER_PAGE)
        .take(SAVES_PER_PAGE)
    {
        let label = if s.is_autosave() {
            format!("{} (autosave)", s.meta.name)
        } else {
            s.meta.name.clone()
        };
        let detail = format!(
            "{} played | {} colonies | {} people | seed {} | {}",
            fmt_ticks(s.meta.tick),
            s.meta.colonies,
            s.meta.population,
            s.meta.seed,
            save::ago(s.meta.saved_unix)
        );
        let confirming = m.confirm.as_deref() == Some(s.slot.as_str());
        let buttons = if confirming {
            UiNode::Row(vec![
                UiNode::Button(
                    "Delete for good".into(),
                    UiAction::Menu(MenuAction::Delete(s.slot.clone())),
                    Btn::Danger,
                ),
                small("Keep", MenuAction::Cancel),
            ])
        } else {
            UiNode::Row(vec![
                UiNode::Button(
                    "Load".into(),
                    UiAction::Menu(MenuAction::Load(s.slot.clone())),
                    Btn::Active,
                ),
                small("Delete", MenuAction::AskDelete(s.slot.clone())),
            ])
        };
        items.push(UiNode::Card(
            vec![
                UiNode::Spread(vec![UiNode::sized(label, HEAD, 16.0), buttons]),
                UiNode::dim(detail),
            ],
            ACCENT,
        ));
    }
    if pages > 1 {
        items.push(UiNode::Row(vec![
            small("< Prev", MenuAction::LoadPage(-1)),
            UiNode::dim(format!("page {} / {pages}", page + 1)),
            small("Next >", MenuAction::LoadPage(1)),
        ]));
    }
    items.push(big("Back", "Esc", MenuAction::Back));
    items.extend(notice(v));
    vec![UiNode::Overlay {
        left: false,
        items: vec![UiNode::Panel {
            width: 640.0,
            items,
        }],
    }]
}

fn pct(v: f32) -> String {
    format!("{:.0}%", v * 100.0)
}

fn settings_page(v: &MenuView) -> Vec<UiNode> {
    let s = v.settings;
    let set = |x: Setting| UiAction::Menu(MenuAction::Set(x));
    let step =
        |label: &str, value: String, frac: Option<f32>, mk: fn(i8) -> Setting| UiNode::Stepper {
            label: label.into(),
            value,
            frac,
            dec: set(mk(-1)),
            inc: set(mk(1)),
        };
    let toggle = |label: &str, on: bool, x: Setting| UiNode::Toggle(label.into(), on, set(x));
    let autosave = if s.autosave_minutes == 0 {
        "Off".to_string()
    } else {
        format!("{} min", s.autosave_minutes)
    };
    let items = vec![
        title("Settings"),
        UiNode::head("Audio"),
        step(
            "Master volume",
            pct(s.master_volume),
            Some(s.master_volume),
            Setting::Master,
        ),
        step(
            "Effects volume",
            pct(s.sfx_volume),
            Some(s.sfx_volume),
            Setting::Sfx,
        ),
        step(
            "Music volume",
            pct(s.music_volume),
            Some(s.music_volume),
            Setting::Music,
        ),
        toggle("Mute all sound [F2]", s.muted, Setting::Muted),
        UiNode::Divider,
        UiNode::head("Display"),
        toggle("Fullscreen", s.fullscreen, Setting::Fullscreen),
        toggle("VSync", s.vsync, Setting::Vsync),
        step(
            "Interface scale",
            pct(s.ui_scale),
            Some((s.ui_scale - 0.7) / 0.8),
            Setting::UiScale,
        ),
        toggle("Show FPS", s.show_fps, Setting::ShowFps),
        UiNode::Divider,
        UiNode::head("Graphics"),
        toggle("Bloom glow", s.bloom, Setting::Bloom),
        toggle("Shadows", s.shadows, Setting::Shadows),
        UiNode::Divider,
        UiNode::head("Gameplay"),
        step(
            "Camera speed",
            format!("{:.2}x", s.camera_speed),
            Some((s.camera_speed - 0.5) / 1.5),
            Setting::CameraSpeed,
        ),
        toggle("Invert scroll zoom", s.invert_zoom, Setting::InvertZoom),
        step("Autosave every", autosave, None, Setting::Autosave),
        toggle(
            "Offer the tutorial on new games",
            s.offer_tutorial,
            Setting::OfferTutorial,
        ),
        UiNode::Gap,
        UiNode::Row(vec![
            small("Reset", MenuAction::Set(Setting::Reset)),
            UiNode::Spacer,
            UiNode::Button(
                "Back".into(),
                UiAction::Menu(MenuAction::Back),
                Btn::Primary,
            ),
        ]),
    ];
    vec![UiNode::Overlay {
        left: false,
        items: vec![UiNode::Panel {
            width: 580.0,
            items,
        }],
    }]
}

// ---------------------------------------------------------------------------------------
// Settings changes
// ---------------------------------------------------------------------------------------

fn nudge(v: f32, d: i8, step: f32, lo: f32, hi: f32) -> f32 {
    let n = ((v + d as f32 * step) / step).round() * step;
    n.clamp(lo, hi)
}

pub fn apply_setting(s: &mut Settings, c: Setting) {
    match c {
        Setting::Master(d) => s.master_volume = nudge(s.master_volume, d, 0.1, 0.0, 1.0),
        Setting::Sfx(d) => s.sfx_volume = nudge(s.sfx_volume, d, 0.1, 0.0, 1.0),
        Setting::Music(d) => s.music_volume = nudge(s.music_volume, d, 0.1, 0.0, 1.0),
        Setting::Muted => s.muted = !s.muted,
        Setting::Fullscreen => s.fullscreen = !s.fullscreen,
        Setting::Vsync => s.vsync = !s.vsync,
        Setting::UiScale(d) => s.ui_scale = nudge(s.ui_scale, d, 0.1, 0.7, 1.5),
        Setting::Bloom => s.bloom = !s.bloom,
        Setting::Shadows => s.shadows = !s.shadows,
        Setting::CameraSpeed(d) => s.camera_speed = nudge(s.camera_speed, d, 0.25, 0.5, 2.0),
        Setting::InvertZoom => s.invert_zoom = !s.invert_zoom,
        Setting::Autosave(d) => {
            let i = AUTOSAVE_CHOICES
                .iter()
                .position(|m| *m == s.autosave_minutes)
                .unwrap_or(2) as i32;
            let i = (i + d as i32).clamp(0, AUTOSAVE_CHOICES.len() as i32 - 1);
            s.autosave_minutes = AUTOSAVE_CHOICES[i as usize];
        }
        Setting::OfferTutorial => s.offer_tutorial = !s.offer_tutorial,
        Setting::ShowFps => s.show_fps = !s.show_fps,
        Setting::Reset => *s = Settings::default(),
    }
}

// ---------------------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------------------

fn menu_keys(
    keys: Res<ButtonInput<KeyCode>>,
    menu: Res<MenuState>,
    net: Res<Net>,
    mut queue: ResMut<MenuQueue>,
) {
    if keys.just_pressed(KeyCode::Escape) && menu.focus == Some(FieldId::Chat) {
        queue.0.push(MenuAction::Cancel);
        return;
    }
    if keys.just_pressed(KeyCode::Enter) && net.in_game() && !menu.open && menu.focus.is_none() {
        queue.0.push(MenuAction::ChatOpen);
        return;
    }
    if keys.just_pressed(KeyCode::Escape) && menu.open {
        queue
            .0
            .push(if menu.focus.is_some() || menu.confirm.is_some() {
                MenuAction::Cancel
            } else {
                MenuAction::Back
            });
    }
}

/// The text a field edits and its length limit.
fn field_buf(menu: &mut MenuState, f: FieldId) -> (&mut String, usize) {
    match f {
        FieldId::Name => (&mut menu.name, 24),
        FieldId::Seed => (&mut menu.seed, 20),
        FieldId::Server => (&mut menu.mp_server, 64),
        FieldId::PlayerName => (&mut menu.mp_name, 20),
        FieldId::RoomName => (&mut menu.room_name, 32),
        FieldId::RoomPass => (&mut menu.room_pass, 32),
        FieldId::Chat => (&mut menu.chat, 200),
    }
}

/// The field Tab moves to from `f` on the current page.
fn next_field(menu: &MenuState, net: &Net, f: FieldId) -> FieldId {
    let ring: &[FieldId] = match menu.page {
        Page::NewGame => &[FieldId::Name, FieldId::Seed],
        Page::Multiplayer | Page::HostWorld if net.connected() => &[FieldId::RoomPass],
        Page::Multiplayer => &[FieldId::Server, FieldId::PlayerName],
        Page::CreateRoom => &[FieldId::RoomName, FieldId::Seed, FieldId::RoomPass],
        _ => &[],
    };
    match ring.iter().position(|x| *x == f) {
        Some(i) => ring[(i + 1) % ring.len()],
        None => f,
    }
}

/// Edits the focused text field from keyboard events.
fn field_input(
    mut events: MessageReader<KeyboardInput>,
    mut menu: ResMut<MenuState>,
    net: Res<Net>,
    mut queue: ResMut<MenuQueue>,
) {
    let Some(field) = menu.focus else {
        events.clear();
        return;
    };
    for e in events.read() {
        if !e.state.is_pressed() {
            continue;
        }
        match &e.logical_key {
            Key::Backspace => {
                field_buf(&mut menu, field).0.pop();
            }
            Key::Enter => {
                queue.0.push(match field {
                    FieldId::Chat => MenuAction::ChatSend,
                    FieldId::Server | FieldId::PlayerName if !net.connected() => {
                        MenuAction::MpConnect
                    }
                    _ => MenuAction::Cancel,
                });
            }
            Key::Tab => {
                menu.focus = Some(next_field(&menu, &net, field));
                return;
            }
            _ => {
                if let Some(t) = &e.text {
                    let (buf, max) = field_buf(&mut menu, field);
                    for c in t.chars() {
                        if !c.is_control() && buf.chars().count() < max {
                            buf.push(c);
                        }
                    }
                }
            }
        }
    }
}

/// Everything a new or loaded game has to reset.
#[derive(SystemParam)]
struct Game<'w> {
    sim: ResMut<'w, SimState>,
    view: ResMut<'w, View>,
    tool: ResMut<'w, Tool>,
    selected: ResMut<'w, Selected>,
    window: ResMut<'w, OpenWindow>,
    speed: ResMut<'w, Speed>,
    virt: ResMut<'w, Time<Virtual>>,
    warp: ResMut<'w, Warp>,
    log: ResMut<'w, MessageLog>,
    dirty: ResMut<'w, SceneDirty>,
    cat: ResMut<'w, crate::ui::BuildCategory>,
    queue: ResMut<'w, CommandQueue>,
    focus: ResMut<'w, FocusShip>,
    overlay: ResMut<'w, GenerationOverlay>,
}

impl Game<'_> {
    fn begin(&mut self, sim: Sim) {
        let at = sim
            .player
            .at
            .filter(|p| sim.surface(*p).is_some())
            .unwrap_or(HOME);
        self.sim.0 = sim;
        *self.view = View::Planet(at);
        *self.tool = Tool::Select;
        self.selected.0 = Selection::None;
        self.window.0 = Popup::None;
        self.speed.0 = 1;
        self.warp.0 = None;
        self.cat.0 = None;
        self.queue.0.clear();
        self.focus.0 = None;
        self.log.entries.clear();
        self.overlay.lines.clear();
        self.overlay.remaining = 0.0;
        self.dirty.0 = true;
        self.virt.unpause();
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_menu_actions(
    mut queue: ResMut<MenuQueue>,
    mut menu: ResMut<MenuState>,
    mut screen: ResMut<Screen>,
    mut session: ResMut<Session>,
    mut settings: ResMut<Settings>,
    mut tutorial: ResMut<Tutorial>,
    mut game: Game,
    mut net: ResMut<Net>,
    mut hand: ResMut<MpHandoff>,
    mut exit: MessageWriter<AppExit>,
) {
    for a in std::mem::take(&mut queue.0) {
        menu.notice = None;
        match a {
            MenuAction::Pause => {
                if *screen == Screen::Playing && !menu.open {
                    menu.open = true;
                    menu.page = Page::Pause;
                    menu.stack.clear();
                    menu.was_paused = game.virt.is_paused();
                    game.virt.pause();
                }
            }
            MenuAction::Resume => close(&mut menu, &mut game),
            MenuAction::Open(p) => {
                if *screen == Screen::Playing && !menu.open {
                    menu.open = true;
                    menu.page = Page::Pause;
                    menu.stack.clear();
                    menu.was_paused = game.virt.is_paused();
                    game.virt.pause();
                }
                let from = menu.page;
                menu.stack.push(from);
                menu.page = p;
                menu.focus = None;
                menu.confirm = None;
                match p {
                    Page::Load => {
                        menu.saves = save::list();
                        menu.load_page = 0;
                    }
                    Page::NewGame => {
                        menu.name = default_name(&menu.saves);
                        menu.seed.clear();
                        menu.tutorial = settings.offer_tutorial;
                    }
                    Page::Multiplayer => {
                        menu.mp_server = if settings.mp_server.is_empty() {
                            net::DEFAULT_SERVER.to_string()
                        } else {
                            settings.mp_server.clone()
                        };
                        menu.mp_name = settings.mp_name.clone();
                        menu.room_pass.clear();
                        if net.phase == Phase::Lobby {
                            net.refresh();
                        }
                    }
                    Page::HostWorld => {
                        menu.saves = save::list();
                        menu.load_page = 0;
                    }
                    Page::CreateRoom => {
                        menu.room_name = if net.name.is_empty() {
                            "Our colony".into()
                        } else {
                            format!("{}'s colony", net.name)
                        };
                        menu.seed.clear();
                        menu.room_pass.clear();
                    }
                    _ => {}
                }
            }
            MenuAction::Back => {
                menu.focus = None;
                menu.confirm = None;
                if menu.page == Page::Multiplayer && !net.in_game() {
                    net.disconnect();
                }
                match menu.stack.pop() {
                    Some(p) => menu.page = p,
                    None if *screen == Screen::Playing => close(&mut menu, &mut game),
                    None => menu.page = MenuState::root(*screen),
                }
            }
            MenuAction::Cancel => {
                if menu.focus == Some(FieldId::Chat) {
                    menu.chat.clear();
                }
                menu.focus = None;
                menu.confirm = None;
            }
            MenuAction::Focus(f) => menu.focus = Some(f),
            MenuAction::SetField(f, v) => {
                let (buf, max) = field_buf(&mut menu, f);
                *buf = v.chars().take(max).collect();
            }
            MenuAction::RandomSeed => menu.seed = random_seed().to_string(),
            MenuAction::ToggleTutorial => menu.tutorial = !menu.tutorial,
            MenuAction::StartNew => {
                let seed = seed_from_text(&menu.seed);
                let name = menu.name.trim().to_string();
                let name = if name.is_empty() {
                    "Colony".into()
                } else {
                    name
                };
                let with_tutorial = menu.tutorial;
                start_new(
                    seed,
                    name,
                    with_tutorial,
                    &mut menu,
                    &mut screen,
                    &mut session,
                    &mut tutorial,
                    &mut game,
                );
            }
            MenuAction::Continue => {
                menu.saves = save::list();
                if let Some(s) = menu.saves.first().map(|s| s.slot.clone()) {
                    load_slot(
                        &s,
                        &mut menu,
                        &mut screen,
                        &mut session,
                        &mut tutorial,
                        &mut game,
                    );
                }
            }
            MenuAction::Load(slot) => load_slot(
                &slot,
                &mut menu,
                &mut screen,
                &mut session,
                &mut tutorial,
                &mut game,
            ),
            MenuAction::Reload => {
                if *screen == Screen::Playing && !net.in_game() {
                    let slot = session.slot.clone();
                    load_slot(
                        &slot,
                        &mut menu,
                        &mut screen,
                        &mut session,
                        &mut tutorial,
                        &mut game,
                    );
                }
            }
            MenuAction::AskDelete(slot) => menu.confirm = Some(slot),
            MenuAction::Delete(slot) => {
                match save::delete(&slot) {
                    Ok(()) => {}
                    Err(e) => menu.notice = Some(format!("Could not delete: {e}")),
                }
                menu.confirm = None;
                menu.saves = save::list();
                let pages = menu.saves.len().div_ceil(SAVES_PER_PAGE).max(1);
                menu.load_page = menu.load_page.min(pages - 1);
            }
            MenuAction::LoadPage(d) => {
                let pages = menu.saves.len().div_ceil(SAVES_PER_PAGE).max(1) as i32;
                menu.load_page = (menu.load_page as i32 + d).clamp(0, pages - 1) as usize;
                menu.confirm = None;
            }
            MenuAction::SaveNow if net.in_game() => {
                game.log
                    .push(MsgKind::Info, "The server keeps the world: no local saves in multiplayer");
            }
            MenuAction::SaveNow => {
                if *screen == Screen::Playing {
                    let slot = session.slot.clone();
                    write_save(&slot, &session, &tutorial, &mut game, true);
                    session.manual_saves += 1;
                    menu.saves = save::list();
                }
            }
            MenuAction::Tutorial if net.in_game() => {}
            MenuAction::Tutorial => {
                if *screen == Screen::Playing {
                    tutorial.start();
                    close(&mut menu, &mut game);
                } else {
                    start_new(
                        TUTORIAL_SEED,
                        "Tutorial".into(),
                        true,
                        &mut menu,
                        &mut screen,
                        &mut session,
                        &mut tutorial,
                        &mut game,
                    );
                }
            }
            MenuAction::ToMain => {
                if *screen == Screen::Playing && !net.in_game() {
                    let slot = session.slot.clone();
                    write_save(&slot, &session, &tutorial, &mut game, false);
                }
                net.disconnect();
                *screen = Screen::MainMenu;
                menu.open = true;
                menu.page = Page::Main;
                menu.stack.clear();
                menu.saves = save::list();
                menu.focus = None;
                game.virt.unpause();
            }
            MenuAction::Quit => {
                if *screen == Screen::Playing && !net.in_game() {
                    let slot = session.slot.clone();
                    write_save(&slot, &session, &tutorial, &mut game, false);
                }
                net.disconnect();
                exit.write(AppExit::Success);
            }
            MenuAction::MpConnect => {
                let mut addr = menu.mp_server.trim().to_string();
                if addr.is_empty() {
                    addr = net::DEFAULT_SERVER.into();
                }
                if !addr.contains(':') {
                    addr.push_str(":4001");
                }
                let name = menu.mp_name.trim().to_string();
                let name = if name.is_empty() { "Player".to_string() } else { name };
                if settings.mp_key.is_empty() {
                    settings.mp_key = format!("{:016x}{:016x}", random_seed_u64(), random_seed_u64());
                }
                settings.mp_server = addr.clone();
                settings.mp_name = name.clone();
                net.error = None;
                net.connect(&addr, &name, &settings.mp_key);
                menu.focus = None;
            }
            MenuAction::MpDisconnect => {
                net.disconnect();
                menu.focus = None;
            }
            MenuAction::MpRefresh => net.refresh(),
            MenuAction::MpMax(d) => {
                menu.room_max = (menu.room_max as i32 + d as i32).clamp(2, 16) as u32;
            }
            MenuAction::MpCreate => {
                let name = menu.room_name.trim().to_string();
                if name.is_empty() {
                    menu.notice = Some("Give the room a name".into());
                } else {
                    let seed = seed_from_text(&menu.seed);
                    net.create(&name, seed, &menu.room_pass, menu.room_max);
                }
            }
            MenuAction::MpHostCurrent => {
                if *screen == Screen::Playing && !net.in_game() && net.phase == Phase::Lobby {
                    let slot = session.slot.clone();
                    write_save(&slot, &session, &tutorial, &mut game, false);
                    match game.sim.0.save().and_then(|b| Sim::load(&b)) {
                        Ok(sim) => net.host(sim, &session.name, &menu.room_pass, menu.room_max),
                        Err(e) => menu.notice = Some(format!("Could not open this world: {e}")),
                    }
                }
            }
            MenuAction::MpHost(slot) => {
                if net.phase == Phase::Lobby {
                    match save::read(&slot) {
                        Ok((meta, sim)) => {
                            net.host(sim, &meta.name, &menu.room_pass, menu.room_max)
                        }
                        Err(e) => menu.notice = Some(format!("Could not load '{slot}': {e}")),
                    }
                }
            }
            MenuAction::MpJoin(id) => net.join(&id, &menu.room_pass),
            MenuAction::MpDelete(id) => net.delete(&id),
            MenuAction::MpBegin => {
                if let Some(sim) = hand.0.take() {
                    game.begin(sim);
                    game.speed.0 = net.speed;
                    let name = net
                        .room
                        .as_ref()
                        .map(|r| r.name.clone())
                        .unwrap_or_else(|| "Multiplayer".into());
                    *session = Session {
                        slot: String::new(),
                        name: name.clone(),
                        manual_saves: 0,
                    };
                    tutorial.dismiss();
                    *screen = Screen::Playing;
                    menu.open = false;
                    menu.stack.clear();
                    menu.focus = None;
                    menu.was_paused = false;
                    game.log.push(
                        MsgKind::Good,
                        format!("Joined {name}. Press Enter to chat"),
                    );
                }
            }
            MenuAction::MpLost(why) => {
                net.disconnect();
                *screen = Screen::MainMenu;
                menu.open = true;
                menu.page = Page::Main;
                menu.stack.clear();
                menu.focus = None;
                menu.notice = Some(if why.is_empty() {
                    "Disconnected from the server".into()
                } else {
                    why
                });
                game.virt.unpause();
            }
            MenuAction::ChatOpen => {
                if net.in_game() && !menu.open {
                    menu.chat.clear();
                    menu.focus = Some(FieldId::Chat);
                }
            }
            MenuAction::ChatSend => {
                let text = menu.chat.trim().to_string();
                if !text.is_empty() {
                    net.chat(&text);
                }
                menu.chat.clear();
                menu.focus = None;
            }
            MenuAction::Set(c) => apply_setting(&mut settings, c),
            MenuAction::Guide(g) => tutorial.apply(g, &mut game.log),
        }
    }
}

fn default_name(saves: &[SaveEntry]) -> String {
    let mut n = saves.len() + 1;
    loop {
        let name = format!("Colony {n}");
        if !saves.iter().any(|s| s.meta.name == name) {
            return name;
        }
        n += 1;
    }
}

fn close(menu: &mut MenuState, game: &mut Game) {
    if !menu.open {
        return;
    }
    menu.open = false;
    menu.focus = None;
    menu.stack.clear();
    if !menu.was_paused {
        game.virt.unpause();
    }
}

#[allow(clippy::too_many_arguments)]
fn start_new(
    seed: u64,
    name: String,
    with_tutorial: bool,
    menu: &mut MenuState,
    screen: &mut Screen,
    session: &mut Session,
    tutorial: &mut Tutorial,
    game: &mut Game,
) {
    game.begin(Sim::new(seed));
    *session = Session {
        slot: save::unique(&name),
        name,
        manual_saves: 0,
    };
    if with_tutorial {
        tutorial.start();
    } else {
        tutorial.dismiss();
    }
    *screen = Screen::Playing;
    menu.open = false;
    menu.stack.clear();
    menu.focus = None;
    menu.was_paused = false;
    let slot = session.slot.clone();
    write_save(&slot, session, tutorial, game, false);
    menu.saves = save::list();
    game.log
        .push(MsgKind::Info, format!("Welcome to {}", session.name));
}

fn load_slot(
    slot: &str,
    menu: &mut MenuState,
    screen: &mut Screen,
    session: &mut Session,
    tutorial: &mut Tutorial,
    game: &mut Game,
) {
    match save::read(slot) {
        Ok((meta, sim)) => {
            game.begin(sim);
            // Loading the autosave must not make later manual saves overwrite it.
            let target = if slot == save::AUTOSAVE {
                save::unique(&meta.name)
            } else {
                slot.to_string()
            };
            *session = Session {
                slot: target,
                name: meta.name.clone(),
                manual_saves: 0,
            };
            tutorial.restore(meta.tutorial);
            *screen = Screen::Playing;
            menu.open = false;
            menu.stack.clear();
            menu.focus = None;
            menu.was_paused = false;
            game.log
                .push(MsgKind::Good, format!("Loaded {}", meta.name));
        }
        Err(e) => menu.notice = Some(format!("Could not load '{slot}': {e}")),
    }
}

fn write_save(slot: &str, session: &Session, tutorial: &Tutorial, game: &mut Game, announce: bool) {
    let meta = save::meta_for(&game.sim.0, &session.name, tutorial.snapshot());
    match save::write(slot, &game.sim.0, &meta) {
        Ok(()) if announce => game
            .log
            .push(MsgKind::Good, format!("Saved '{}'", session.name)),
        Ok(()) => {}
        Err(e) => game.log.push(MsgKind::Error, format!("Save failed: {e}")),
    }
}

/// On the main menu the camera drifts slowly around the galaxy behind the menu.
fn backdrop(
    screen: Res<Screen>,
    time: Res<Time<Real>>,
    mut view: ResMut<View>,
    mut rig: ResMut<CameraRig>,
) {
    if *screen != Screen::MainMenu {
        return;
    }
    if !matches!(*view, View::Map(MapLevel::Galaxy(_))) {
        *view = View::Map(MapLevel::Galaxy(0));
    }
    rig.yaw += time.delta_secs() * 0.035;
    rig.pitch = 0.62;
    rig.goal_dist = 105.0;
    rig.goal_focus = Vec3::ZERO;
}

#[allow(clippy::too_many_arguments)]
fn autosave(
    time: Res<Time<Real>>,
    screen: Res<Screen>,
    menu: Res<MenuState>,
    settings: Res<Settings>,
    session: Res<Session>,
    tutorial: Res<Tutorial>,
    sim: Res<SimState>,
    mut log: ResMut<MessageLog>,
    net: Res<Net>,
    mut since: Local<f32>,
) {
    if *screen != Screen::Playing || menu.open || settings.autosave_minutes == 0 || net.in_game() {
        return;
    }
    *since += time.delta_secs();
    if *since < settings.autosave_minutes as f32 * 60.0 {
        return;
    }
    *since = 0.0;
    let meta = save::meta_for(&sim.0, &session.name, tutorial.snapshot());
    match save::write(save::AUTOSAVE, &sim.0, &meta) {
        Ok(()) => log.push(MsgKind::Info, "Autosaved"),
        Err(e) => log.push(MsgKind::Error, format!("Autosave failed: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeds_parse_numbers_and_hash_words() {
        assert_eq!(seed_from_text("42"), 42);
        assert_eq!(seed_from_text(" 7 "), 7);
        assert_eq!(seed_from_text("hello"), seed_from_text("hello"));
        assert_ne!(seed_from_text("hello"), seed_from_text("world"));
    }

    #[test]
    fn setting_steps_stay_in_range() {
        let mut s = Settings::default();
        for _ in 0..20 {
            apply_setting(&mut s, Setting::Master(1));
        }
        assert_eq!(s.master_volume, 1.0);
        for _ in 0..20 {
            apply_setting(&mut s, Setting::Master(-1));
        }
        assert_eq!(s.master_volume, 0.0);
        for _ in 0..20 {
            apply_setting(&mut s, Setting::UiScale(1));
        }
        assert_eq!(s.ui_scale, 1.5);
        for _ in 0..20 {
            apply_setting(&mut s, Setting::CameraSpeed(-1));
        }
        assert_eq!(s.camera_speed, 0.5);
    }

    #[test]
    fn autosave_choices_cycle_and_clamp() {
        let mut s = Settings::default();
        for _ in 0..10 {
            apply_setting(&mut s, Setting::Autosave(1));
        }
        assert_eq!(s.autosave_minutes, 20);
        for _ in 0..10 {
            apply_setting(&mut s, Setting::Autosave(-1));
        }
        assert_eq!(s.autosave_minutes, 0);
    }

    #[test]
    fn default_names_skip_taken_ones() {
        let meta = |n: &str| SaveEntry {
            slot: n.into(),
            meta: crate::save::SaveMeta {
                name: n.into(),
                seed: 1,
                tick: 0,
                colonies: 1,
                population: 1,
                saved_unix: 0,
                tutorial: crate::save::TutorialSave::default(),
            },
        };
        let saves = vec![meta("Colony 2")];
        assert_eq!(default_name(&saves), "Colony 3");
    }
}
