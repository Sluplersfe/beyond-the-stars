//! Small declarative UI layer: each panel describes its content as `UiNode`s every frame;
//! the panel's entities are rebuilt only when that description changes.

use std::collections::HashMap;

use bevy::picking::events::PointerPress;
use bevy::picking::hover::{HoverMap, Hovered};
use bevy::picking::pointer::PointerId;
use bevy::prelude::*;
use bevy::text::LetterSpacing;
use bevy::ui::{Pressed, ScrollPosition};
use bevy::ui_widgets::{Activate, ActivateOnPress, Button, ScrollArea, observe};
use bevy::window::PrimaryWindow;
use sim::{Command, PlanetId};

use crate::camera::CameraRig;
use crate::icons::{Icon, Icons};
use crate::panels;
use crate::state::*;
use crate::time_control::Speed;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PointerOverUi>()
            .init_resource::<UiActions>()
            .init_resource::<PanelCache>()
            .init_resource::<BuildCategory>()
            .init_resource::<TechTab>()
            .init_resource::<CodexTab>()
            .init_resource::<ScrollOffsets>()
            .init_resource::<UiViewport>()
            .add_systems(Startup, spawn_panels)
            .add_systems(
                PreUpdate,
                (
                    detect_pointer_over_ui.after(bevy::picking::PickingSystems::Hover),
                    save_scroll_offsets.after(bevy::picking::PickingSystems::Last),
                ),
            )
            .add_systems(
                Update,
                (
                    track_viewport,
                    handle_actions,
                    rebuild_panels,
                    button_hover,
                    update_tooltip,
                    age_messages,
                )
                    .chain(),
            );
    }
}

#[derive(Resource, Default)]
pub struct PointerOverUi(pub bool);

#[derive(Resource, Default)]
pub struct UiActions(pub Vec<UiAction>);

/// Last scroll offset of each named scroll area, so rebuilds keep the view in place.
#[derive(Resource, Default)]
pub struct ScrollOffsets(pub HashMap<&'static str, f32>);

#[derive(Component)]
struct UiScroll(&'static str);

/// Size of the screen in UI units (logical pixels divided by the UI scale), so panel
/// layouts can drop optional content on small screens.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct UiViewport(pub Vec2);

impl Default for UiViewport {
    fn default() -> Self {
        UiViewport(Vec2::new(1600.0, 900.0))
    }
}

fn track_viewport(
    windows: Query<&Window, With<PrimaryWindow>>,
    scale: Res<UiScale>,
    mut vp: ResMut<UiViewport>,
) {
    if let Ok(w) = windows.single() {
        let v = Vec2::new(w.width(), w.height()) / scale.0.max(0.1);
        if (v - vp.0).length_squared() > 0.25 {
            vp.0 = v;
        }
    }
}

/// Tech-tree tab: false = technologies, true = time control.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TechTab(pub bool);

/// Codex tab: recipes, structures or items.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodexTab {
    #[default]
    Recipes,
    Buildings,
    Items,
}

/// Build-toolbar category whose flyout is open.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildCategory(pub Option<panels::Cat>);

#[derive(Debug, Clone, PartialEq)]
pub enum UiAction {
    TechTab(bool),
    CodexTab(CodexTab),
    SetTool(Tool),
    Cmd(Command),
    Cmds(Vec<Command>),
    OpenWindow(Popup),
    SetSpeed(u32),
    SpeedStep(bool),
    TogglePause,
    Warp(u32),
    ViewPlanet(PlanetId),
    ViewMap(MapLevel),
    Select(Selection),
    Save,
    Load,
    Menu(crate::menu::MenuAction),
    Category(panels::Cat),
    /// Multiply the camera distance.
    Zoom(f32),
    /// Normalised (0..1, y down) click on the minimap.
    Minimap(Vec2),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Btn {
    Normal,
    Active,
    Disabled,
    Danger,
    /// Borderless list row.
    Flat,
    /// Large call-to-action.
    Primary,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UiNode {
    Text(String, Color, f32),
    Button(String, UiAction, Btn),
    Row(Vec<UiNode>),
    Wrap(Vec<UiNode>),
    /// Equal-width columns.
    Columns(Vec<Vec<UiNode>>),
    Gap,
    Icon(Icon, Color, f32),
    /// Progress bar (fraction, colour).
    Bar(f32, Color),
    /// Row with children pushed to both ends.
    Spread(Vec<UiNode>),
    /// Bordered sub-card with an accent colour.
    Card(Vec<UiNode>, Color),
    Dot(Color, f32),
    /// Toolbar tile: icon above a label.
    Tile(Icon, String, UiAction, Btn),
    IconBtn(Icon, UiAction, Btn, f32),
    /// Full-width row button: icon, label, right-aligned text.
    ListBtn(Icon, Color, String, String, UiAction, Btn),
    /// Vertical stack.
    Col(Vec<UiNode>),
    Minimap(Handle<Image>, f32),
    Divider,
    /// Takes the remaining row space.
    Spacer,
    /// Vertically scrollable area; `key` keeps the offset across rebuilds.
    Scroll {
        key: &'static str,
        height: Val,
        items: Vec<UiNode>,
    },
    /// Compact tech-tree node: icon ring, name, status and an optional progress bar.
    TechNode {
        icon: Icon,
        title: String,
        status: String,
        frac: Option<f32>,
        accent: Color,
        action: UiAction,
        style: Btn,
    },
    /// Fixed-width column.
    Fixed(f32, Vec<UiNode>),
    /// Full-screen layer for menus: content on the left or centred, over a dimmed scene.
    Overlay {
        left: bool,
        items: Vec<UiNode>,
    },
    /// Framed card of a fixed width.
    Panel {
        width: f32,
        items: Vec<UiNode>,
    },
    /// Text input; clicking focuses it.
    Field {
        id: crate::menu::FieldId,
        value: String,
        hint: String,
        focused: bool,
    },
    /// Label with an on/off switch.
    Toggle(String, bool, UiAction),
    /// Label with a value, a bar and -/+ buttons.
    Stepper {
        label: String,
        value: String,
        frac: Option<f32>,
        dec: UiAction,
        inc: UiAction,
    },
    /// Large menu button with a smaller note on the right.
    BigButton {
        label: String,
        note: String,
        action: UiAction,
        style: Btn,
    },
    /// Section header: accent tick, letter-spaced caps label and a hairline rule.
    Section(String, Color),
    /// Letter-spaced display text (window titles, big numbers).
    Spaced(String, Color, f32),
    /// Small rounded tag.
    Chip(String, Color),
    /// Icon on a tinted rounded plate of the given size.
    Badge(Icon, Color, f32),
    /// Inset group of items laid out in a row (top bar clusters).
    Well(Vec<UiNode>),
    /// Segmented control: (label, action, selected).
    Tabs(Vec<(String, UiAction, bool)>),
    /// Two-line choice button (build menu entries): title, subline, subline colour.
    Choice {
        title: String,
        label: String,
        sub: String,
        sub_color: Color,
        action: UiAction,
        style: Btn,
    },
    /// Wraps a node so hovering it shows a tooltip.
    Tip(String, Box<UiNode>),
    /// Key/value line: label left (dim), value right (coloured).
    Kv(String, String, Color),
}

// ---------------------------------------------------------------------------------------
// Design tokens. One cool glass palette with an industrial-amber secondary accent.
// ---------------------------------------------------------------------------------------

pub const TEXT: Color = Color::srgb(0.85, 0.9, 0.96);
pub const DIM: Color = Color::srgb(0.55, 0.62, 0.72);
pub const ACCENT: Color = Color::srgb(0.4, 0.82, 1.0);
pub const HEAD: Color = Color::srgb(0.97, 0.985, 1.0);
/// Industrial-yellow secondary accent (matches the machines' paint).
pub const AMBER: Color = Color::srgb(0.96, 0.72, 0.24);
/// Barely-there text for captions and disabled labels.
pub const FAINT: Color = Color::srgb(0.38, 0.43, 0.51);

/// Glass surface of the HUD panels.
pub const PANEL_BG: Color = Color::srgba(0.025, 0.045, 0.08, 0.84);
pub const PANEL_BORDER: Color = Color::srgba(0.5, 0.75, 1.0, 0.16);
/// Hairlines and inset frames.
const LINE: Color = Color::srgba(0.55, 0.78, 1.0, 0.13);
/// Inset wells (top-bar clusters, tracks, fields).
const WELL: Color = Color::srgba(0.0, 0.01, 0.03, 0.42);

impl UiNode {
    pub fn text(s: impl Into<String>) -> Self {
        UiNode::Text(s.into(), TEXT, 13.0)
    }
    pub fn dim(s: impl Into<String>) -> Self {
        UiNode::Text(s.into(), DIM, 12.0)
    }
    pub fn title(s: impl Into<String>) -> Self {
        UiNode::Text(s.into(), HEAD, 18.0)
    }
    /// Section heading (accent tick, caps label, hairline rule).
    pub fn head(s: impl Into<String>) -> Self {
        UiNode::Section(s.into(), ACCENT)
    }
    pub fn colored(s: impl Into<String>, c: Color) -> Self {
        UiNode::Text(s.into(), c, 13.0)
    }
    pub fn sized(s: impl Into<String>, c: Color, size: f32) -> Self {
        UiNode::Text(s.into(), c, size)
    }
    pub fn btn(s: impl Into<String>, a: UiAction) -> Self {
        UiNode::Button(s.into(), a, Btn::Normal)
    }
    pub fn btn_if(s: impl Into<String>, a: UiAction, enabled: bool) -> Self {
        UiNode::Button(
            s.into(),
            a,
            if enabled { Btn::Normal } else { Btn::Disabled },
        )
    }
    pub fn tip(s: impl Into<String>, n: UiNode) -> Self {
        UiNode::Tip(s.into(), Box::new(n))
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelId {
    TopBar,
    Left,
    Right,
    BottomLeft,
    Toolbar,
    Flyout,
    BottomRight,
    Toasts,
    Hint,
    Window,
    Overlay,
    Tutorial,
    Menu,
}

#[derive(Resource, Default)]
struct PanelCache {
    content: HashMap<PanelId, Vec<UiNode>>,
}

/// Resting look of a button, restored after hover/press.
#[derive(Component)]
struct BtnBase {
    bg: Color,
    border: BorderColor,
    style: Btn,
}

/// Text of a button, for scripted input.
#[derive(Component)]
pub struct BtnLabel(pub String);

/// Hover text of a node.
#[derive(Component)]
struct Tip(String);

/// The single floating tooltip box.
#[derive(Component)]
struct TooltipBox;

// ---------------------------------------------------------------------------------------
// Surface helpers
// ---------------------------------------------------------------------------------------

fn vgrad(stops: Vec<ColorStop>) -> BackgroundGradient {
    BackgroundGradient::from(LinearGradient::to_bottom(stops))
}

/// Faint cool light falling on the top of a panel.
fn panel_sheen() -> BackgroundGradient {
    vgrad(vec![
        ColorStop::px(Color::srgba(0.45, 0.7, 1.0, 0.035), 0.0),
        ColorStop::px(Color::srgba(0.45, 0.7, 1.0, 0.0), 90.0),
    ])
}

/// Rim light: bright top edge fading down the sides.
fn rim(c: Color, top: f32, bottom: f32) -> BorderGradient {
    BorderGradient::from(LinearGradient::to_bottom(vec![
        ColorStop::percent(c.with_alpha(top), 0.0),
        ColorStop::percent(c.with_alpha(bottom), 100.0),
    ]))
}

fn drop_shadow(alpha: f32, y: f32, blur: f32) -> BoxShadow {
    BoxShadow::new(
        Color::srgba(0.0, 0.0, 0.02, alpha),
        px(0),
        px(y),
        px(0),
        px(blur),
    )
}

fn glow(c: Color, alpha: f32, blur: f32) -> BoxShadow {
    BoxShadow::new(c.with_alpha(alpha), px(0), px(0), px(0), px(blur))
}

/// Glass panel look: translucent fill, top sheen, rim-lit border and a soft drop shadow.
fn glass() -> impl Bundle {
    (
        BackgroundColor(PANEL_BG),
        panel_sheen(),
        BorderColor::all(PANEL_BORDER),
        rim(Color::srgb(0.6, 0.85, 1.0), 0.42, 0.04),
        drop_shadow(0.5, 6.0, 22.0),
    )
}

fn spawn_panels(mut commands: Commands) {
    let panel = |id: PanelId, node: Node| (id, node, glass());
    let card = || Node {
        flex_direction: FlexDirection::Column,
        padding: UiRect::all(px(12)),
        border: UiRect::all(px(1)),
        border_radius: BorderRadius::all(px(10)),
        row_gap: px(4),
        ..default()
    };
    let abs = Node {
        position_type: PositionType::Absolute,
        ..card()
    };
    // Top bar: a full-width strip with a cyan light line along its lower edge.
    commands.spawn((
        PanelId::TopBar,
        Node {
            top: px(0),
            left: px(0),
            right: px(0),
            height: px(62),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: px(12),
            padding: UiRect::axes(px(12), px(6)),
            border: UiRect::bottom(px(1)),
            border_radius: BorderRadius::ZERO,
            ..abs.clone()
        },
        BackgroundColor(Color::srgba(0.02, 0.035, 0.065, 0.9)),
        vgrad(vec![
            ColorStop::percent(Color::srgba(0.4, 0.65, 1.0, 0.07), 0.0),
            ColorStop::percent(Color::srgba(0.0, 0.0, 0.0, 0.18), 100.0),
        ]),
        BorderColor::all(Color::srgba(0.4, 0.7, 1.0, 0.12)),
        BorderGradient::from(LinearGradient::to_right(vec![
            ColorStop::percent(Color::srgba(0.4, 0.82, 1.0, 0.0), 0.0),
            ColorStop::percent(Color::srgba(0.4, 0.82, 1.0, 0.55), 30.0),
            ColorStop::percent(Color::srgba(0.4, 0.82, 1.0, 0.55), 70.0),
            ColorStop::percent(Color::srgba(0.4, 0.82, 1.0, 0.0), 100.0),
        ])),
        drop_shadow(0.45, 2.0, 18.0),
    ));
    commands.spawn(panel(
        PanelId::Left,
        Node {
            top: px(74),
            left: px(10),
            width: px(264),
            ..abs.clone()
        },
    ));
    commands.spawn((
        panel(
            PanelId::Right,
            Node {
                top: px(74),
                right: px(10),
                width: px(330),
                max_height: percent(66),
                overflow: Overflow::scroll_y(),
                ..abs.clone()
            },
        ),
        ScrollArea,
    ));
    commands.spawn(panel(
        PanelId::BottomLeft,
        Node {
            bottom: px(10),
            left: px(10),
            flex_direction: FlexDirection::Row,
            column_gap: px(12),
            padding: UiRect::all(px(10)),
            ..abs.clone()
        },
    ));
    commands.spawn(panel(
        PanelId::BottomRight,
        Node {
            bottom: px(10),
            right: px(10),
            width: px(330),
            ..abs.clone()
        },
    ));
    // Centred panels sit in full-width transparent rows.
    let centred = |commands: &mut Commands, id: PanelId, pos: Node, inner: Node, bg: bool| {
        let mut wrapper = commands.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                right: px(0),
                top: pos.top,
                bottom: pos.bottom,
                justify_content: JustifyContent::Center,
                ..default()
            },
            Pickable::IGNORE,
        ));
        wrapper.with_children(|c| {
            if bg {
                let mut e = c.spawn(panel(id, inner));
                if id == PanelId::Window {
                    e.insert((
                        BackgroundColor(Color::srgba(0.016, 0.028, 0.052, 0.985)),
                        vgrad(vec![
                            ColorStop::px(Color::srgba(0.4, 0.7, 1.0, 0.04), 0.0),
                            ColorStop::px(Color::srgba(0.4, 0.7, 1.0, 0.0), 140.0),
                        ]),
                        rim(Color::srgb(0.6, 0.85, 1.0), 0.6, 0.08),
                        drop_shadow(0.7, 14.0, 48.0),
                    ));
                }
            } else {
                c.spawn((id, inner));
            }
        });
        wrapper.id()
    };
    centred(
        &mut commands,
        PanelId::Toolbar,
        Node {
            bottom: px(10),
            ..abs.clone()
        },
        Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            padding: UiRect::all(px(6)),
            column_gap: px(4),
            border_radius: BorderRadius::all(px(14)),
            ..card()
        },
        true,
    );
    centred(
        &mut commands,
        PanelId::Flyout,
        Node {
            bottom: px(100),
            ..abs.clone()
        },
        Node {
            max_width: px(840),
            row_gap: px(8),
            ..card()
        },
        true,
    );
    centred(
        &mut commands,
        PanelId::Hint,
        Node {
            bottom: px(100),
            ..abs.clone()
        },
        Node {
            max_width: px(760),
            padding: UiRect::axes(px(14), px(7)),
            border_radius: BorderRadius::all(px(18)),
            ..card()
        },
        true,
    );
    // Tutorial card with the toasts stacked beneath it.
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                right: px(0),
                top: px(72),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(6),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|c| {
            c.spawn(panel(
                PanelId::Tutorial,
                Node {
                    width: px(620),
                    max_width: percent(94),
                    padding: UiRect::all(px(14)),
                    row_gap: px(6),
                    ..card()
                },
            ))
            .insert((
                BorderColor::all(AMBER.with_alpha(0.3)),
                rim(AMBER, 0.85, 0.15),
                drop_shadow(0.55, 8.0, 28.0),
            ));
            c.spawn((
                PanelId::Toasts,
                Node {
                    padding: UiRect::all(px(0)),
                    align_items: AlignItems::Center,
                    ..card()
                },
                Pickable::IGNORE,
            ));
        });
    let window_row = centred(
        &mut commands,
        PanelId::Window,
        Node {
            top: px(74),
            ..abs.clone()
        },
        Node {
            width: px(940),
            max_width: percent(70),
            max_height: px(760),
            overflow: Overflow::clip_y(),
            padding: UiRect::all(px(18)),
            border_radius: BorderRadius::all(px(14)),
            row_gap: px(6),
            ..card()
        },
        true,
    );
    // Windows draw above the HUD panels they may overlap.
    commands
        .entity(window_row)
        .insert((WindowRow, GlobalZIndex(20)));
    centred(
        &mut commands,
        PanelId::Overlay,
        Node {
            top: px(140),
            ..abs
        },
        Node {
            max_width: px(720),
            align_items: AlignItems::Center,
            padding: UiRect::axes(px(22), px(14)),
            ..card()
        },
        true,
    );
    // Full-screen menu layer above everything else.
    commands.spawn((
        PanelId::Menu,
        Node {
            position_type: PositionType::Absolute,
            left: px(0),
            top: px(0),
            width: percent(100),
            height: percent(100),
            ..default()
        },
        GlobalZIndex(100),
    ));
    // Floating tooltip, positioned by `update_tooltip`.
    commands
        .spawn((
            TooltipBox,
            Node {
                position_type: PositionType::Absolute,
                display: Display::None,
                max_width: px(320),
                padding: UiRect::axes(px(10), px(6)),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(7)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.03, 0.06, 0.11, 0.97)),
            BorderColor::all(ACCENT.with_alpha(0.45)),
            drop_shadow(0.6, 4.0, 14.0),
            GlobalZIndex(300),
            Pickable::IGNORE,
        ))
        .with_child((text_bundle("", TEXT, 12.0), Pickable::IGNORE));
}

/// The row that holds the window panel, moved down while the tutorial card is showing.
#[derive(Component)]
pub struct WindowRow;

fn detect_pointer_over_ui(
    hover: Res<HoverMap>,
    nodes: Query<(), With<Node>>,
    mut over: ResMut<PointerOverUi>,
) {
    over.0 = hover
        .get(&PointerId::Mouse)
        .is_some_and(|hits| hits.keys().any(|e| nodes.contains(*e)));
}

fn save_scroll_offsets(
    scrolls: Query<(&UiScroll, &ScrollPosition)>,
    mut offsets: ResMut<ScrollOffsets>,
) {
    for (key, pos) in &scrolls {
        offsets.0.insert(key.0, pos.y);
    }
}

/// (background, text, border) of a button style at rest.
fn btn_colors(style: Btn) -> (Color, Color, Color) {
    match style {
        Btn::Normal => (
            Color::srgba(0.085, 0.14, 0.22, 0.92),
            TEXT,
            Color::srgba(0.5, 0.72, 1.0, 0.24),
        ),
        Btn::Active => (
            Color::srgba(0.07, 0.33, 0.52, 0.96),
            Color::WHITE,
            Color::srgba(0.45, 0.86, 1.0, 0.95),
        ),
        Btn::Disabled => (
            Color::srgba(0.05, 0.065, 0.09, 0.7),
            Color::srgb(0.4, 0.44, 0.5),
            Color::srgba(1.0, 1.0, 1.0, 0.06),
        ),
        Btn::Danger => (
            Color::srgba(0.36, 0.06, 0.07, 0.94),
            Color::srgb(1.0, 0.9, 0.88),
            Color::srgba(1.0, 0.42, 0.38, 0.75),
        ),
        Btn::Flat => (Color::NONE, TEXT, Color::NONE),
        Btn::Primary => (
            Color::srgba(0.11, 0.42, 0.74, 0.97),
            Color::WHITE,
            Color::srgba(0.6, 0.9, 1.0, 0.95),
        ),
    }
}

/// Glossy top-light over a button's fill.
fn btn_sheen(style: Btn) -> Option<BackgroundGradient> {
    let a = match style {
        Btn::Flat => return None,
        Btn::Disabled => 0.012,
        Btn::Primary | Btn::Active => 0.08,
        _ => 0.04,
    };
    Some(vgrad(vec![
        ColorStop::percent(Color::srgba(0.8, 0.92, 1.0, a), 0.0),
        ColorStop::percent(Color::srgba(0.8, 0.92, 1.0, 0.0), 55.0),
        ColorStop::percent(Color::srgba(0.0, 0.0, 0.0, a * 0.8), 100.0),
    ]))
}

fn btn_glow(style: Btn, bg: Color) -> Option<BoxShadow> {
    match style {
        Btn::Active => Some(glow(ACCENT, 0.28, 10.0)),
        Btn::Primary => Some(glow(Color::srgb(0.3, 0.7, 1.0), 0.4, 14.0)),
        Btn::Danger => Some(glow(bg, 0.35, 10.0)),
        _ => None,
    }
}

fn text_bundle(s: &str, c: Color, size: f32) -> impl Bundle {
    (
        Text::new(s.to_string()),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(c),
    )
}

/// Letter-spaced text for headings.
fn spaced_bundle(s: &str, c: Color, size: f32, spacing: f32) -> impl Bundle {
    (text_bundle(s, c, size), LetterSpacing::Px(spacing))
}

fn icon_bundle(icons: &Icons, i: Icon, c: Color, size: f32) -> impl Bundle {
    (
        Node {
            width: px(size),
            height: px(size),
            flex_shrink: 0.0,
            ..default()
        },
        ImageNode {
            image: icons.get(i),
            color: c,
            ..default()
        },
    )
}

/// Icon centred on a tinted, rim-lit rounded plate.
fn badge_bundle(c: Color, size: f32) -> impl Bundle {
    (
        Node {
            width: px(size),
            height: px(size),
            flex_shrink: 0.0,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px((size * 0.28).round())),
            ..default()
        },
        BackgroundColor(c.with_alpha(0.08)),
        vgrad(vec![
            ColorStop::percent(c.with_alpha(0.1), 0.0),
            ColorStop::percent(c.with_alpha(0.0), 100.0),
        ]),
        BorderColor::all(c.with_alpha(0.32)),
    )
}

/// Tooltip text for icon-only buttons, derived from what they do.
fn action_tip(a: &UiAction) -> Option<String> {
    let s = match a {
        UiAction::OpenWindow(p) => match p {
            Popup::Research => "Research  [T]",
            Popup::TechTree => "Technology tree  [U]",
            Popup::Codex => "Codex  [O]",
            Popup::Ships => "Ships  [B]",
            Popup::Colonies => "Colonies  [C]",
            Popup::Logistics => "Logistics  [L]",
            Popup::Resources => "Resources",
            Popup::Help => "Help & settings  [F1]",
            Popup::None => return None,
        },
        UiAction::TogglePause => "Pause / resume  [Space]",
        UiAction::SetSpeed(1) => "Normal speed  [1]",
        UiAction::SpeedStep(true) => "Faster  [+]",
        UiAction::SpeedStep(false) => "Slower  [-]",
        UiAction::Zoom(k) if *k > 1.0 => "Zoom out",
        UiAction::Zoom(_) => "Zoom in",
        UiAction::ViewMap(_) => "Star map  [M]",
        _ => return None,
    };
    Some(s.to_string())
}

/// Make an entity a clickable button that queues `action`.
fn make_button(
    e: &mut EntityCommands,
    action: &UiAction,
    style: Btn,
    label: &str,
    bg: Color,
    border: Color,
) {
    e.insert((
        BtnBase {
            bg,
            border: BorderColor::all(border),
            style,
        },
        BtnLabel(label.to_string()),
    ));
    if let Some(g) = btn_sheen(style) {
        e.insert(g);
    }
    if let Some(s) = btn_glow(style, bg) {
        e.insert(s);
    }
    if style != Btn::Disabled {
        let action = action.clone();
        e.insert((
            Button,
            ActivateOnPress,
            Hovered::default(),
            observe(move |_: On<Activate>, mut acts: ResMut<UiActions>| {
                acts.0.push(action.clone());
            }),
        ));
    }
}

fn spawn_all(
    parent: &mut ChildSpawnerCommands,
    items: &[UiNode],
    icons: &Icons,
    s: &ScrollOffsets,
) {
    for n in items {
        spawn_ui_node(parent, n, icons, s);
    }
}

/// Rounded progress track with a gradient fill and a soft glow.
fn spawn_bar(parent: &mut ChildSpawnerCommands, frac: f32, c: Color, h: f32) {
    let f = frac.clamp(0.0, 1.0);
    let r = BorderRadius::all(px(h / 2.0));
    parent
        .spawn((
            Node {
                width: percent(100),
                height: px(h),
                flex_shrink: 0.0,
                border_radius: r,
                ..default()
            },
            BackgroundColor(Color::srgba(0.6, 0.75, 1.0, 0.06)),
        ))
        .with_children(|t| {
            if f <= 0.0 {
                return;
            }
            t.spawn((
                Node {
                    width: percent(f * 100.0),
                    min_width: px(h),
                    height: percent(100),
                    border_radius: r,
                    ..default()
                },
                BackgroundColor(c),
                BackgroundGradient::from(LinearGradient::to_right(vec![
                    ColorStop::percent(Color::srgba(0.0, 0.0, 0.0, 0.35), 0.0),
                    ColorStop::percent(Color::srgba(0.0, 0.0, 0.0, 0.0), 70.0),
                    ColorStop::percent(Color::srgba(1.0, 1.0, 1.0, 0.3), 100.0),
                ])),
                glow(c, 0.45, 6.0),
            ));
        });
}

/// Small L-shaped bracket for framing corners (`corner`: 0 TL, 1 TR, 2 BL, 3 BR).
fn bracket(corner: u8, c: Color, len: f32, inset: f32) -> impl Bundle {
    let (top, bottom) = if corner < 2 {
        (px(inset), Val::Auto)
    } else {
        (Val::Auto, px(inset))
    };
    let (left, right) = if corner.is_multiple_of(2) {
        (px(inset), Val::Auto)
    } else {
        (Val::Auto, px(inset))
    };
    let w = px(2);
    let border = UiRect {
        left: if corner.is_multiple_of(2) { w } else { px(0) },
        right: if corner % 2 == 1 { w } else { px(0) },
        top: if corner < 2 { w } else { px(0) },
        bottom: if corner >= 2 { w } else { px(0) },
    };
    (
        Node {
            position_type: PositionType::Absolute,
            top,
            bottom,
            left,
            right,
            width: px(len),
            height: px(len),
            border,
            ..default()
        },
        BorderColor::all(c),
        Pickable::IGNORE,
    )
}

fn spawn_ui_node(
    parent: &mut ChildSpawnerCommands,
    node: &UiNode,
    icons: &Icons,
    scroll: &ScrollOffsets,
) {
    match node {
        UiNode::Text(s, c, size) => {
            parent.spawn(text_bundle(s, *c, *size));
        }
        UiNode::Spaced(s, c, size) => {
            parent.spawn((
                spaced_bundle(s, *c, *size, (size * 0.14).max(1.0)),
                TextShadow {
                    offset: Vec2::new(0.0, 2.0),
                    color: Color::srgba(0.0, 0.0, 0.0, 0.6),
                },
            ));
        }
        UiNode::Icon(i, c, size) => {
            parent.spawn(icon_bundle(icons, *i, *c, *size));
        }
        UiNode::Badge(i, c, size) => {
            parent
                .spawn(badge_bundle(*c, *size))
                .with_child(icon_bundle(
                    icons,
                    *i,
                    c.mix(&Color::WHITE, 0.15),
                    (size * 0.58).round(),
                ));
        }
        UiNode::Dot(c, size) => {
            let mut e = parent.spawn((
                Node {
                    width: px(*size),
                    height: px(*size),
                    flex_shrink: 0.0,
                    border_radius: BorderRadius::all(percent(50)),
                    ..default()
                },
                BackgroundColor(*c),
            ));
            if *size <= 14.0 {
                e.insert(glow(*c, 0.55, size * 0.7));
            } else {
                // Planet-sized dots get a lit-sphere look.
                e.insert(BackgroundGradient::from(RadialGradient::new(
                    UiPosition::anchor(Vec2::new(-0.16, -0.2)),
                    RadialGradientShape::FarthestCorner,
                    vec![
                        ColorStop::percent(Color::srgba(1.0, 1.0, 1.0, 0.35), 0.0),
                        ColorStop::percent(Color::srgba(1.0, 1.0, 1.0, 0.0), 45.0),
                        ColorStop::percent(Color::srgba(0.0, 0.0, 0.0, 0.55), 100.0),
                    ],
                )));
            }
        }
        UiNode::Chip(s, c) => {
            parent.spawn((
                Node {
                    padding: UiRect::axes(px(7), px(2)),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(9)),
                    flex_shrink: 0.0,
                    ..default()
                },
                BackgroundColor(c.with_alpha(0.07)),
                BorderColor::all(c.with_alpha(0.45)),
                children![text_bundle(s, c.mix(&Color::WHITE, 0.2), 11.0)],
            ));
        }
        UiNode::Bar(frac, c) => spawn_bar(parent, *frac, *c, 6.0),
        UiNode::Divider => {
            parent.spawn((
                Node {
                    width: percent(100),
                    height: px(1),
                    flex_shrink: 0.0,
                    margin: UiRect::vertical(px(5)),
                    ..default()
                },
                BackgroundGradient::from(LinearGradient::to_right(vec![
                    ColorStop::percent(Color::srgba(0.5, 0.75, 1.0, 0.0), 0.0),
                    ColorStop::percent(Color::srgba(0.5, 0.75, 1.0, 0.3), 25.0),
                    ColorStop::percent(Color::srgba(0.5, 0.75, 1.0, 0.3), 75.0),
                    ColorStop::percent(Color::srgba(0.5, 0.75, 1.0, 0.0), 100.0),
                ])),
            ));
        }
        UiNode::Section(s, c) => {
            parent
                .spawn(Node {
                    width: percent(100),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: px(7),
                    margin: UiRect::new(px(0), px(0), px(6), px(2)),
                    ..default()
                })
                .with_children(|r| {
                    r.spawn((
                        Node {
                            width: px(3),
                            height: px(11),
                            border_radius: BorderRadius::all(px(2)),
                            ..default()
                        },
                        BackgroundColor(*c),
                        glow(*c, 0.6, 5.0),
                    ));
                    r.spawn(spaced_bundle(
                        &s.to_uppercase(),
                        c.mix(&Color::WHITE, 0.15),
                        11.0,
                        1.6,
                    ));
                    r.spawn((
                        Node {
                            flex_grow: 1.0,
                            height: px(1),
                            ..default()
                        },
                        BackgroundGradient::from(LinearGradient::to_right(vec![
                            ColorStop::percent(c.with_alpha(0.35), 0.0),
                            ColorStop::percent(c.with_alpha(0.0), 100.0),
                        ])),
                    ));
                });
        }
        UiNode::Kv(k, v, c) => {
            parent.spawn((
                Node {
                    width: percent(100),
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    column_gap: px(8),
                    ..default()
                },
                children![text_bundle(k, DIM, 12.0), text_bundle(v, *c, 12.5)],
            ));
        }
        UiNode::Button(label, action, style) => {
            let (bg, fg, border) = btn_colors(*style);
            let primary = *style == Btn::Primary;
            let mut e = parent.spawn((
                Node {
                    padding: if primary {
                        UiRect::axes(px(16), px(9))
                    } else {
                        UiRect::axes(px(10), px(4))
                    },
                    margin: UiRect::all(px(1)),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(if primary { 8 } else { 6 })),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    width: if primary { percent(100) } else { Val::Auto },
                    ..default()
                },
                BackgroundColor(bg),
                BorderColor::all(border),
            ));
            if primary {
                e.with_child(spaced_bundle(label, fg, 13.0, 0.8));
            } else {
                e.with_child(text_bundle(label, fg, 12.5));
            }
            make_button(&mut e, action, *style, label, bg, border);
        }
        UiNode::Tabs(tabs) => {
            parent
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Row,
                        padding: UiRect::all(px(3)),
                        column_gap: px(2),
                        border: UiRect::all(px(1)),
                        border_radius: BorderRadius::all(px(9)),
                        ..default()
                    },
                    BackgroundColor(WELL),
                    BorderColor::all(LINE),
                ))
                .with_children(|row| {
                    for (label, action, on) in tabs {
                        let style = if *on { Btn::Active } else { Btn::Flat };
                        let (bg, _, border) = btn_colors(style);
                        let fg = if *on { Color::WHITE } else { DIM };
                        let mut e = row.spawn((
                            Node {
                                padding: UiRect::axes(px(14), px(5)),
                                border: UiRect::all(px(1)),
                                border_radius: BorderRadius::all(px(7)),
                                ..default()
                            },
                            BackgroundColor(bg),
                            BorderColor::all(border),
                            children![text_bundle(label, fg, 12.5)],
                        ));
                        make_button(&mut e, action, style, label, bg, border);
                    }
                });
        }
        UiNode::Tile(icon, label, action, style) => {
            let active = *style == Btn::Active;
            let disabled = *style == Btn::Disabled;
            let (bg, border) = (Color::NONE, Color::NONE);
            let fg = if active {
                Color::WHITE
            } else if disabled {
                FAINT
            } else {
                TEXT
            };
            let plate = if active {
                ACCENT
            } else {
                Color::srgb(0.55, 0.7, 0.9)
            };
            let mut e = parent.spawn((
                Node {
                    width: px(84),
                    height: px(74),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    row_gap: px(5),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(10)),
                    ..default()
                },
                BackgroundColor(bg),
                BorderColor::all(border),
            ));
            e.with_children(|c| {
                let mut p = c.spawn((
                    Node {
                        width: px(40),
                        height: px(40),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        border: UiRect::all(px(1)),
                        border_radius: BorderRadius::all(px(11)),
                        ..default()
                    },
                    BackgroundColor(plate.with_alpha(if active { 0.24 } else { 0.045 })),
                    vgrad(vec![
                        ColorStop::percent(Color::srgba(1.0, 1.0, 1.0, 0.1), 0.0),
                        ColorStop::percent(Color::srgba(1.0, 1.0, 1.0, 0.0), 60.0),
                    ]),
                    BorderColor::all(plate.with_alpha(if active { 0.9 } else { 0.18 })),
                ));
                if active {
                    p.insert(glow(ACCENT, 0.45, 12.0));
                }
                p.with_child(icon_bundle(icons, *icon, fg, 24.0));
                c.spawn(text_bundle(label, fg, 11.5));
                if active {
                    c.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            bottom: px(-1),
                            width: px(28),
                            height: px(3),
                            border_radius: BorderRadius::all(px(2)),
                            ..default()
                        },
                        BackgroundColor(ACCENT),
                        glow(ACCENT, 0.8, 6.0),
                    ));
                }
            });
            make_button(&mut e, action, *style, label, bg, border);
            // Tiles keep a transparent rest state; hover adds a soft plate.
            e.insert(BtnBase {
                bg,
                border: BorderColor::all(border),
                style: Btn::Flat,
            });
            e.remove::<BackgroundGradient>();
            e.remove::<BoxShadow>();
        }
        UiNode::IconBtn(icon, action, style, size) => {
            let (bg, fg, border) = btn_colors(*style);
            let fg = if *style == Btn::Flat { DIM } else { fg };
            let mut e = parent.spawn((
                Node {
                    width: px(size + 14.0),
                    height: px(size + 14.0),
                    flex_shrink: 0.0,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(((size + 14.0) * 0.26).round())),
                    ..default()
                },
                BackgroundColor(bg),
                BorderColor::all(border),
            ));
            e.with_child(icon_bundle(icons, *icon, fg, *size));
            make_button(&mut e, action, *style, &format!("{icon:?}"), bg, border);
            if let Some(t) = action_tip(action) {
                e.insert(Tip(t));
            }
        }
        UiNode::ListBtn(icon, ic, label, right, action, style) => {
            let (bg, fg, border) = btn_colors(*style);
            let mut e = parent.spawn((
                Node {
                    width: percent(100),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: px(10),
                    padding: UiRect::axes(px(6), px(5)),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(8)),
                    ..default()
                },
                BackgroundColor(bg),
                BorderColor::all(border),
            ));
            e.with_children(|c| {
                c.spawn(badge_bundle(*ic, 28.0))
                    .with_child(icon_bundle(icons, *icon, *ic, 16.0));
                c.spawn((
                    Node {
                        flex_grow: 1.0,
                        ..default()
                    },
                    children![text_bundle(label, fg, 13.0)],
                ));
                if !right.is_empty() {
                    c.spawn(text_bundle(right, DIM, 12.0));
                }
            });
            make_button(&mut e, action, *style, label, bg, border);
        }
        UiNode::Choice {
            title,
            label,
            sub,
            sub_color,
            action,
            style,
        } => {
            let (bg, fg, border) = btn_colors(*style);
            let mut e = parent.spawn((
                Node {
                    min_width: px(150),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(2),
                    padding: UiRect::axes(px(11), px(6)),
                    margin: UiRect::all(px(1)),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(8)),
                    ..default()
                },
                BackgroundColor(bg),
                BorderColor::all(border),
                children![
                    text_bundle(title, fg, 13.0),
                    text_bundle(sub, *sub_color, 10.5)
                ],
            ));
            make_button(&mut e, action, *style, label, bg, border);
        }
        UiNode::Minimap(image, size) => {
            parent
                .spawn((
                    Node {
                        width: px(*size),
                        height: px(*size),
                        flex_shrink: 0.0,
                        border: UiRect::all(px(1)),
                        border_radius: BorderRadius::all(px(8)),
                        ..default()
                    },
                    BorderColor::all(ACCENT.with_alpha(0.3)),
                    ImageNode::new(image.clone()),
                    drop_shadow(0.5, 0.0, 8.0),
                ))
                .with_children(|m| {
                    // Inner vignette and HUD corner brackets.
                    m.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: px(0),
                            right: px(0),
                            top: px(0),
                            bottom: px(0),
                            border_radius: BorderRadius::all(px(7)),
                            ..default()
                        },
                        BackgroundGradient::from(RadialGradient::new(
                            UiPosition::CENTER,
                            RadialGradientShape::FarthestCorner,
                            vec![
                                ColorStop::percent(Color::srgba(0.0, 0.0, 0.0, 0.0), 55.0),
                                ColorStop::percent(Color::srgba(0.0, 0.02, 0.06, 0.55), 100.0),
                            ],
                        )),
                        Pickable::IGNORE,
                    ));
                    for k in 0..4 {
                        m.spawn(bracket(k, ACCENT.with_alpha(0.85), 14.0, 4.0));
                    }
                })
                .observe(
                    |ev: On<PointerPress>,
                     q: Query<(&ComputedNode, &UiGlobalTransform)>,
                     mut acts: ResMut<UiActions>| {
                        let Ok((node, tf)) = q.get(ev.entity) else {
                            return;
                        };
                        let scale = 1.0 / node.inverse_scale_factor;
                        let size = node.size();
                        let top_left = tf.translation - size / 2.0;
                        let rel = (ev.pointer.position * scale - top_left) / size;
                        acts.0
                            .push(UiAction::Minimap(rel.clamp(Vec2::ZERO, Vec2::ONE)));
                    },
                );
        }
        UiNode::Row(items) | UiNode::Wrap(items) | UiNode::Spread(items) => {
            let wrap = if matches!(node, UiNode::Wrap(_)) {
                FlexWrap::Wrap
            } else {
                FlexWrap::NoWrap
            };
            let justify = if matches!(node, UiNode::Spread(_)) {
                JustifyContent::SpaceBetween
            } else {
                JustifyContent::Start
            };
            parent
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    flex_wrap: wrap,
                    align_items: AlignItems::Center,
                    justify_content: justify,
                    column_gap: px(8),
                    row_gap: px(4),
                    width: if matches!(node, UiNode::Spread(_)) {
                        percent(100)
                    } else {
                        Val::Auto
                    },
                    ..default()
                })
                .with_children(|p| spawn_all(p, items, icons, scroll));
        }
        UiNode::Well(items) => {
            parent
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        column_gap: px(14),
                        padding: UiRect::axes(px(10), px(4)),
                        border: UiRect::all(px(1)),
                        border_radius: BorderRadius::all(px(10)),
                        height: px(48),
                        ..default()
                    },
                    BackgroundColor(WELL),
                    BorderColor::all(LINE),
                    BorderGradient::from(LinearGradient::to_bottom(vec![
                        ColorStop::percent(Color::srgba(0.0, 0.0, 0.0, 0.35), 0.0),
                        ColorStop::percent(Color::srgba(0.55, 0.78, 1.0, 0.12), 100.0),
                    ])),
                ))
                .with_children(|p| spawn_all(p, items, icons, scroll));
        }
        UiNode::Col(items) => {
            parent
                .spawn(Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(1),
                    ..default()
                })
                .with_children(|p| spawn_all(p, items, icons, scroll));
        }
        UiNode::Card(items, accent) => {
            parent
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: px(3),
                        padding: UiRect::new(px(10), px(8), px(7), px(7)),
                        border: UiRect::new(px(3), px(1), px(1), px(1)),
                        border_radius: BorderRadius::all(px(8)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.03, 0.05, 0.085, 0.8)),
                    BackgroundGradient::from(LinearGradient::to_right(vec![
                        ColorStop::percent(accent.with_alpha(0.07), 0.0),
                        ColorStop::percent(accent.with_alpha(0.0), 60.0),
                    ])),
                    BorderColor {
                        left: *accent,
                        top: accent.with_alpha(0.22),
                        right: accent.with_alpha(0.12),
                        bottom: accent.with_alpha(0.12),
                    },
                ))
                .with_children(|p| spawn_all(p, items, icons, scroll));
        }
        UiNode::Columns(cols) => {
            parent
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: px(12),
                    ..default()
                })
                .with_children(|p| {
                    for col in cols {
                        p.spawn(Node {
                            flex_direction: FlexDirection::Column,
                            flex_grow: 1.0,
                            flex_basis: px(0),
                            row_gap: px(4),
                            ..default()
                        })
                        .with_children(|c| spawn_all(c, col, icons, scroll));
                    }
                });
        }
        UiNode::Gap => {
            parent.spawn(Node {
                height: px(6),
                flex_shrink: 0.0,
                ..default()
            });
        }
        UiNode::Spacer => {
            parent.spawn(Node {
                flex_grow: 1.0,
                ..default()
            });
        }
        UiNode::Scroll { key, height, items } => {
            let y = scroll.0.get(key).copied().unwrap_or(0.0);
            parent
                .spawn((
                    Node {
                        width: percent(100),
                        max_height: *height,
                        flex_direction: FlexDirection::Column,
                        row_gap: px(6),
                        padding: UiRect::right(px(4)),
                        overflow: Overflow::scroll_y(),
                        ..default()
                    },
                    ScrollArea,
                    UiScroll(key),
                    ScrollPosition(Vec2::new(0.0, y)),
                ))
                .with_children(|p| spawn_all(p, items, icons, scroll));
        }
        UiNode::Fixed(w, items) => {
            parent
                .spawn(Node {
                    width: px(*w),
                    flex_shrink: 0.0,
                    flex_direction: FlexDirection::Column,
                    row_gap: px(3),
                    ..default()
                })
                .with_children(|p| spawn_all(p, items, icons, scroll));
        }
        UiNode::Tip(text, inner) => {
            parent
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        flex_shrink: 0.0,
                        ..default()
                    },
                    Tip(text.clone()),
                    Hovered::default(),
                ))
                .with_children(|p| spawn_ui_node(p, inner, icons, scroll));
        }
        UiNode::Overlay { left, items } => {
            let mut e = parent.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0),
                    top: px(0),
                    width: percent(100),
                    height: percent(100),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    justify_content: if *left {
                        JustifyContent::Start
                    } else {
                        JustifyContent::Center
                    },
                    padding: if *left {
                        UiRect::left(percent(7))
                    } else {
                        UiRect::ZERO
                    },
                    ..default()
                },
                BackgroundColor(Color::srgba(
                    0.01,
                    0.02,
                    0.05,
                    if *left { 0.0 } else { 0.62 },
                )),
            ));
            if *left {
                e.insert(BackgroundGradient::from(LinearGradient::to_right(vec![
                    ColorStop::percent(Color::srgba(0.0, 0.01, 0.04, 0.75), 0.0),
                    ColorStop::percent(Color::srgba(0.0, 0.01, 0.04, 0.0), 55.0),
                ])));
            } else {
                e.insert(BackgroundGradient::from(RadialGradient::new(
                    UiPosition::CENTER,
                    RadialGradientShape::FarthestCorner,
                    vec![
                        ColorStop::percent(Color::srgba(0.0, 0.0, 0.0, 0.0), 30.0),
                        ColorStop::percent(Color::srgba(0.0, 0.0, 0.02, 0.5), 100.0),
                    ],
                )));
            }
            e.with_children(|p| {
                if *left {
                    p.spawn((
                        Node {
                            width: px(430),
                            flex_direction: FlexDirection::Column,
                            row_gap: px(9),
                            padding: UiRect::all(px(30)),
                            border: UiRect::all(px(1)),
                            border_radius: BorderRadius::all(px(16)),
                            ..default()
                        },
                        glass(),
                    ))
                    .insert(BackgroundColor(Color::srgba(0.015, 0.03, 0.065, 0.66)))
                    .with_children(|c| spawn_all(c, items, icons, scroll));
                } else {
                    spawn_all(p, items, icons, scroll);
                }
            });
        }
        UiNode::Panel { width, items } => {
            parent
                .spawn((
                    Node {
                        width: px(*width),
                        max_width: percent(94),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(8),
                        padding: UiRect::all(px(24)),
                        border: UiRect::all(px(1)),
                        border_radius: BorderRadius::all(px(16)),
                        ..default()
                    },
                    glass(),
                ))
                .insert((
                    BackgroundColor(Color::srgba(0.018, 0.032, 0.06, 0.97)),
                    drop_shadow(0.7, 14.0, 48.0),
                ))
                .with_children(|c| spawn_all(c, items, icons, scroll));
        }
        UiNode::Field {
            id,
            value,
            hint,
            focused,
        } => {
            let (bg, border) = if *focused {
                (Color::srgba(0.04, 0.1, 0.18, 1.0), ACCENT)
            } else {
                (Color::srgba(0.01, 0.025, 0.05, 0.9), LINE.with_alpha(0.3))
            };
            let (shown, color) = if value.is_empty() && !*focused {
                (hint.clone(), FAINT)
            } else if *focused {
                (format!("{value}|"), Color::WHITE)
            } else {
                (value.clone(), TEXT)
            };
            let mut e = parent.spawn((
                Node {
                    width: percent(100),
                    padding: UiRect::axes(px(12), px(9)),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(8)),
                    ..default()
                },
                BackgroundColor(bg),
                BorderColor::all(border),
                children![text_bundle(&shown, color, 15.0)],
            ));
            make_button(
                &mut e,
                &UiAction::Menu(crate::menu::MenuAction::Focus(*id)),
                Btn::Normal,
                "field",
                bg,
                border,
            );
            e.remove::<BackgroundGradient>();
            if *focused {
                e.insert(glow(ACCENT, 0.3, 10.0));
            }
        }
        UiNode::Toggle(label, on, action) => {
            // Label with a pill switch.
            parent
                .spawn(Node {
                    width: percent(100),
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|r| {
                    r.spawn(text_bundle(label, TEXT, 14.0));
                    let (bg, border) = if *on {
                        (Color::srgba(0.08, 0.4, 0.62, 0.95), ACCENT)
                    } else {
                        (Color::srgba(0.05, 0.08, 0.13, 0.95), LINE.with_alpha(0.35))
                    };
                    let mut e = r.spawn((
                        Node {
                            width: px(64),
                            height: px(24),
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            justify_content: if *on {
                                JustifyContent::End
                            } else {
                                JustifyContent::Start
                            },
                            padding: UiRect::all(px(3)),
                            column_gap: px(5),
                            border: UiRect::all(px(1)),
                            border_radius: BorderRadius::all(px(12)),
                            ..default()
                        },
                        BackgroundColor(bg),
                        BorderColor::all(border),
                    ));
                    e.with_children(|s| {
                        let knob = (
                            Node {
                                width: px(16),
                                height: px(16),
                                border_radius: BorderRadius::all(percent(50)),
                                ..default()
                            },
                            BackgroundColor(if *on { Color::WHITE } else { DIM }),
                        );
                        if *on {
                            s.spawn(text_bundle("ON", Color::WHITE, 10.5));
                            s.spawn(knob);
                        } else {
                            s.spawn(knob);
                            s.spawn(text_bundle("OFF", DIM, 10.5));
                        }
                    });
                    make_button(
                        &mut e,
                        action,
                        Btn::Normal,
                        if *on { "ON" } else { "OFF" },
                        bg,
                        border,
                    );
                    if *on {
                        e.insert(glow(ACCENT, 0.3, 8.0));
                    }
                });
        }
        UiNode::Stepper {
            label,
            value,
            frac,
            dec,
            inc,
        } => {
            let mut mid = vec![UiNode::IconBtn(Icon::Minus, dec.clone(), Btn::Normal, 10.0)];
            if let Some(f) = frac {
                mid.push(UiNode::Fixed(96.0, vec![UiNode::Bar(*f, ACCENT)]));
            }
            mid.push(UiNode::Fixed(
                62.0,
                vec![UiNode::sized(value.clone(), Color::WHITE, 13.0)],
            ));
            mid.push(UiNode::IconBtn(Icon::Plus, inc.clone(), Btn::Normal, 10.0));
            let n = UiNode::Spread(vec![
                UiNode::sized(label.clone(), TEXT, 13.0),
                UiNode::Row(mid),
            ]);
            spawn_ui_node(parent, &n, icons, scroll);
        }
        UiNode::BigButton {
            label,
            note,
            action,
            style,
        } => {
            let (bg, fg, border) = btn_colors(*style);
            let mut e = parent.spawn((
                Node {
                    width: percent(100),
                    padding: UiRect::axes(px(18), px(12)),
                    border: UiRect::new(px(3), px(1), px(1), px(1)),
                    border_radius: BorderRadius::all(px(10)),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::SpaceBetween,
                    column_gap: px(12),
                    ..default()
                },
                BackgroundColor(bg),
                BorderColor::all(border),
            ));
            e.with_children(|c| {
                c.spawn(spaced_bundle(label, fg, 19.0, 1.0));
                if !note.is_empty() {
                    c.spawn(text_bundle(note, DIM, 12.0));
                }
            });
            make_button(&mut e, action, *style, label, bg, border);
        }
        UiNode::TechNode {
            icon,
            title,
            status,
            frac,
            accent,
            action,
            style,
        } => {
            let (bg, border, fg, ring) = match style {
                Btn::Active => (
                    accent
                        .mix(&Color::srgb(0.02, 0.04, 0.07), 0.78)
                        .with_alpha(0.96),
                    accent.with_alpha(0.55),
                    Color::WHITE,
                    *accent,
                ),
                Btn::Primary => (
                    Color::srgba(0.05, 0.2, 0.34, 0.98),
                    ACCENT,
                    Color::WHITE,
                    ACCENT,
                ),
                Btn::Disabled => (
                    Color::srgba(0.03, 0.04, 0.06, 0.88),
                    Color::srgba(0.4, 0.45, 0.55, 0.16),
                    Color::srgb(0.5, 0.55, 0.63),
                    accent.with_alpha(0.32),
                ),
                _ => (
                    Color::srgba(0.05, 0.085, 0.14, 0.97),
                    accent.with_alpha(0.6),
                    TEXT,
                    *accent,
                ),
            };
            let mut e = parent.spawn((
                Node {
                    width: px(174),
                    min_height: px(46),
                    flex_shrink: 0.0,
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: px(7),
                    padding: UiRect::new(px(6), px(7), px(4), px(4)),
                    border: UiRect::new(px(3), px(1), px(1), px(1)),
                    border_radius: BorderRadius::all(px(8)),
                    ..default()
                },
                BackgroundColor(bg),
                BorderColor {
                    left: ring,
                    ..BorderColor::all(border)
                },
            ));
            e.with_children(|c| {
                let done = *style == Btn::Active;
                let mut ring_e = c.spawn((
                    Node {
                        width: px(28),
                        height: px(28),
                        flex_shrink: 0.0,
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        border: UiRect::all(px(if done { 0 } else { 2 })),
                        border_radius: BorderRadius::all(percent(50)),
                        ..default()
                    },
                    BorderColor::all(ring),
                    BackgroundColor(if done { ring } else { ring.with_alpha(0.12) }),
                ));
                if done {
                    ring_e.insert(glow(ring, 0.5, 8.0));
                }
                ring_e.with_child(icon_bundle(
                    icons,
                    *icon,
                    if done {
                        Color::srgb(0.03, 0.05, 0.08)
                    } else {
                        ring
                    },
                    15.0,
                ));
                c.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    flex_grow: 1.0,
                    flex_shrink: 1.0,
                    row_gap: px(2),
                    ..default()
                })
                .with_children(|t| {
                    t.spawn(text_bundle(title, fg, 11.5));
                    let sc = if *style == Btn::Disabled {
                        FAINT
                    } else {
                        ring.mix(&Color::WHITE, 0.3)
                    };
                    t.spawn(text_bundle(status, sc, 10.0));
                    if let Some(f) = frac {
                        spawn_bar(t, *f, ACCENT, 3.0);
                    }
                });
            });
            make_button(&mut e, action, *style, title, bg, border);
            e.insert(BtnBase {
                bg,
                border: BorderColor {
                    left: ring,
                    ..BorderColor::all(border)
                },
                style: *style,
            });
            if *style == Btn::Primary {
                e.insert(glow(ACCENT, 0.4, 14.0));
            } else {
                e.remove::<BoxShadow>();
            }
            e.remove::<BackgroundGradient>();
            e.insert(vgrad(vec![
                ColorStop::percent(Color::srgba(1.0, 1.0, 1.0, 0.05), 0.0),
                ColorStop::percent(Color::srgba(1.0, 1.0, 1.0, 0.0), 60.0),
            ]));
        }
    }
}

fn rebuild_panels(
    mut commands: Commands,
    mut cache: ResMut<PanelCache>,
    mut panels_q: Query<(Entity, &PanelId, &mut Node)>,
    ctx: panels::PanelCtx,
    icons: Res<Icons>,
    scroll: Res<ScrollOffsets>,
) {
    for (entity, id, mut node) in &mut panels_q {
        if *id == PanelId::Window {
            let wide = matches!(
                ctx.window.0,
                crate::state::Popup::TechTree | crate::state::Popup::Codex
            );
            let (w, mw) = if wide { (1440.0, 96.0) } else { (940.0, 70.0) };
            if node.width != px(w) {
                node.width = px(w);
                node.max_width = percent(mw);
                node.max_height = if wide { Val::Vh(90.0) } else { px(760) };
            }
        }
        let content = panels::build(*id, &ctx);
        if cache.content.get(id) == Some(&content) {
            continue;
        }
        node.display = if content.is_empty() {
            Display::None
        } else {
            Display::Flex
        };
        commands.entity(entity).despawn_children();
        commands.entity(entity).with_children(|p| {
            for n in &content {
                spawn_ui_node(p, n, &icons, &scroll);
            }
        });
        cache.content.insert(*id, content);
    }
}

fn btn_state(base: &BtnBase, hovered: bool, down: bool) -> (Color, BorderColor) {
    if !hovered && !down {
        return (base.bg, base.border);
    }
    let flat = base.bg.alpha() < 0.01;
    let bg = if flat {
        Color::srgba(0.45, 0.72, 1.0, if down { 0.18 } else { 0.1 })
    } else if down {
        base.bg.darker(0.05)
    } else {
        base.bg.lighter(0.07)
    };
    let border = match base.style {
        Btn::Danger => Color::srgb(1.0, 0.55, 0.5),
        _ if flat => ACCENT.with_alpha(0.35),
        _ => ACCENT.with_alpha(0.85),
    };
    (bg, BorderColor::all(border))
}

/// Button feedback: hover lifts and rim-lights, press sinks.
#[allow(clippy::type_complexity)]
fn button_hover(
    mut q: Query<(
        &Hovered,
        Has<Pressed>,
        &BtnBase,
        &mut BackgroundColor,
        &mut BorderColor,
    )>,
    changed: Query<Entity, (With<BtnBase>, Or<(Changed<Hovered>, Added<Pressed>)>)>,
    mut released: RemovedComponents<Pressed>,
) {
    let mut todo: Vec<Entity> = changed.iter().collect();
    todo.extend(released.read());
    for e in todo {
        let Ok((h, down, base, mut bg, mut border)) = q.get_mut(e) else {
            continue;
        };
        let (want_bg, want_border) = btn_state(base, h.get(), down);
        bg.0 = want_bg;
        *border = want_border;
    }
}

/// Shows the hovered node's `Tip` in the floating box after a short delay.
#[allow(clippy::type_complexity)]
fn update_tooltip(
    time: Res<Time<Real>>,
    tips: Query<(Entity, &Tip, &Hovered, &ComputedNode, &UiGlobalTransform)>,
    mut boxes: Query<(&mut Node, &ComputedNode, &Children), With<TooltipBox>>,
    mut texts: Query<&mut Text>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut state: Local<(Option<Entity>, f32)>,
) {
    let Ok((mut node, box_size, children)) = boxes.single_mut() else {
        return;
    };
    let hit = tips
        .iter()
        .filter(|(_, _, h, n, _)| h.get() && n.size().x > 0.0)
        .min_by(|a, b| {
            let (sa, sb) = (a.3.size(), b.3.size());
            (sa.x * sa.y).total_cmp(&(sb.x * sb.y))
        });
    let Some((e, tip, _, cn, tf)) = hit else {
        *state = (None, 0.0);
        if node.display != Display::None {
            node.display = Display::None;
        }
        return;
    };
    if state.0 != Some(e) {
        *state = (Some(e), 0.0);
        if let Some(&child) = children.first()
            && let Ok(mut t) = texts.get_mut(child)
            && t.0 != tip.0
        {
            t.0 = tip.0.clone();
        }
    }
    state.1 += time.delta_secs();
    if state.1 < 0.3 {
        node.display = Display::None;
        return;
    }
    let inv = cn.inverse_scale_factor;
    let centre = tf.translation * inv;
    let size = cn.size() * inv;
    let bsize = box_size.size() * box_size.inverse_scale_factor;
    let screen = windows
        .single()
        .map(|w| Vec2::new(w.physical_width() as f32, w.physical_height() as f32) * inv)
        .unwrap_or(Vec2::new(1600.0, 900.0));
    let x = (centre.x - bsize.x / 2.0).clamp(6.0, (screen.x - bsize.x - 6.0).max(6.0));
    let mut y = centre.y + size.y / 2.0 + 8.0;
    if y + bsize.y > screen.y - 6.0 {
        y = centre.y - size.y / 2.0 - bsize.y - 8.0;
    }
    node.display = Display::Flex;
    node.left = px(x);
    node.top = px(y);
}

fn age_messages(
    time: Res<Time<Real>>,
    mut log: ResMut<MessageLog>,
    mut overlay: ResMut<GenerationOverlay>,
) {
    let dt = time.delta_secs();
    for m in &mut log.entries {
        m.age += dt;
    }
    if overlay.remaining > 0.0 {
        overlay.remaining -= dt;
        if overlay.remaining <= 0.0 {
            overlay.lines.clear();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_actions(
    mut acts: ResMut<UiActions>,
    mut tool: ResMut<Tool>,
    mut queue: ResMut<CommandQueue>,
    mut window: ResMut<OpenWindow>,
    mut speed: ResMut<Speed>,
    mut virt: ResMut<Time<Virtual>>,
    mut warp: ResMut<Warp>,
    mut view: ResMut<View>,
    mut selected: ResMut<Selected>,
    sim: ResMut<SimState>,
    mut log: ResMut<MessageLog>,
    mut cat: ResMut<BuildCategory>,
    mut rig: ResMut<CameraRig>,
    mut menu: ResMut<crate::menu::MenuQueue>,
    (mut tech_tab, mut codex_tab): (ResMut<TechTab>, ResMut<CodexTab>),
    net: Res<crate::net::Net>,
) {
    for a in std::mem::take(&mut acts.0) {
        match a {
            UiAction::TechTab(t) => tech_tab.0 = t,
            UiAction::CodexTab(t) => *codex_tab = t,
            UiAction::SetTool(t) => *tool = t,
            UiAction::Cmd(c) => queue.push(c),
            UiAction::Cmds(cs) => queue.0.extend(cs),
            UiAction::OpenWindow(w) => {
                window.0 = if window.0 == w { Popup::None } else { w };
            }
            UiAction::SetSpeed(s) => {
                let s = s.min(sim.0.research.max_speed());
                if net.in_game() {
                    net.request_speed(s.min(net.max_speed));
                } else {
                    speed.0 = s;
                    virt.unpause();
                }
            }
            UiAction::SpeedStep(up) => {
                let s = crate::time_control::next_speed(speed.0, sim.0.research.max_speed(), up);
                if net.in_game() {
                    net.request_speed(s.min(net.max_speed));
                } else {
                    speed.0 = s;
                    virt.unpause();
                }
            }
            UiAction::TogglePause => {
                if net.in_game() {
                    net.request_pause(!net.paused);
                } else {
                    virt.toggle();
                }
            }
            UiAction::Warp(_) if net.in_game() => {
                log.push(MsgKind::Warn, "Instant-Warp is not available in multiplayer");
            }
            UiAction::Warp(ship) => {
                if sim.0.research.has(sim::Tech::InstantWarp) {
                    warp.0 = Some(ship);
                } else {
                    log.push(MsgKind::Warn, "Instant-Warp is not researched yet");
                }
            }
            UiAction::ViewPlanet(p) => {
                if sim.0.surface(p).is_some() {
                    *view = View::Planet(p);
                    selected.0 = Selection::None;
                    window.0 = Popup::None;
                } else {
                    log.push(MsgKind::Warn, "No surface data: survey the planet first");
                }
            }
            UiAction::ViewMap(level) => {
                *view = View::Map(level);
                *tool = Tool::Select;
                cat.0 = None;
                if matches!(selected.0, Selection::Building(..)) {
                    selected.0 = Selection::None;
                }
            }
            UiAction::Select(s) => selected.0 = s,
            UiAction::Save => menu.0.push(crate::menu::MenuAction::SaveNow),
            UiAction::Load => menu.0.push(crate::menu::MenuAction::Reload),
            UiAction::Menu(m) => menu.0.push(m),
            UiAction::Category(c) => {
                cat.0 = if cat.0 == Some(c) { None } else { Some(c) };
            }
            UiAction::Zoom(k) => {
                let (lo, hi) = rig.dist_range;
                rig.goal_dist = (rig.goal_dist * k).clamp(lo, hi);
            }
            UiAction::Minimap(rel) => match *view {
                View::Planet(p) => {
                    if let Some(s) = sim.0.surface(p) {
                        let plane = Vec2::new(rel.x * s.w as f32, (1.0 - rel.y) * s.h as f32);
                        rig.goal_focus = to_world(plane, 0.0);
                    }
                }
                View::Map(MapLevel::Galaxy(_)) => {
                    let r = crate::panels::GALAXY_MINIMAP_RADIUS;
                    let plane = Vec2::new(rel.x * 2.0 - 1.0, 1.0 - rel.y * 2.0) * r;
                    rig.goal_focus = to_world(plane, 0.0);
                }
                View::Map(_) => {}
            },
        }
    }
}
