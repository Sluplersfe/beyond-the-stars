//! Small declarative UI layer: each panel describes its content as `UiNode`s every frame;
//! the panel's entities are rebuilt only when that description changes.

use std::collections::HashMap;

use bevy::picking::events::PointerPress;
use bevy::picking::hover::{HoverMap, Hovered};
use bevy::picking::pointer::PointerId;
use bevy::prelude::*;
use bevy::ui::ScrollPosition;
use bevy::ui_widgets::{Activate, ActivateOnPress, Button, ScrollArea, observe};
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
                (handle_actions, rebuild_panels, button_hover, age_messages).chain(),
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
}

pub const TEXT: Color = Color::srgb(0.86, 0.9, 0.96);
pub const DIM: Color = Color::srgb(0.55, 0.62, 0.72);
pub const ACCENT: Color = Color::srgb(0.45, 0.78, 1.0);
pub const HEAD: Color = Color::srgb(0.98, 0.99, 1.0);

impl UiNode {
    pub fn text(s: impl Into<String>) -> Self {
        UiNode::Text(s.into(), TEXT, 13.0)
    }
    pub fn dim(s: impl Into<String>) -> Self {
        UiNode::Text(s.into(), DIM, 12.0)
    }
    pub fn title(s: impl Into<String>) -> Self {
        UiNode::Text(s.into(), HEAD, 17.0)
    }
    pub fn head(s: impl Into<String>) -> Self {
        UiNode::Text(s.into(), ACCENT, 14.0)
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

#[derive(Component)]
struct BtnBase(Color, Color);

/// Text of a button, for scripted input.
#[derive(Component)]
pub struct BtnLabel(pub String);

pub const PANEL_BG: Color = Color::srgba(0.03, 0.07, 0.13, 0.88);
pub const PANEL_BORDER: Color = Color::srgba(0.25, 0.5, 0.85, 0.55);

fn spawn_panels(mut commands: Commands) {
    let panel = |id: PanelId, node: Node| {
        (
            id,
            node,
            BackgroundColor(PANEL_BG),
            BorderColor::all(PANEL_BORDER),
        )
    };
    let card = || Node {
        flex_direction: FlexDirection::Column,
        padding: UiRect::all(px(10)),
        border: UiRect::all(px(1)),
        border_radius: BorderRadius::all(px(8)),
        row_gap: px(4),
        ..default()
    };
    let abs = Node {
        position_type: PositionType::Absolute,
        ..card()
    };
    commands
        .spawn(panel(
            PanelId::TopBar,
            Node {
                top: px(0),
                left: px(0),
                right: px(0),
                height: px(62),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: px(18),
                padding: UiRect::axes(px(14), px(6)),
                border: UiRect::bottom(px(1)),
                border_radius: BorderRadius::ZERO,
                ..abs.clone()
            },
        ))
        .insert(BackgroundColor(Color::srgba(0.02, 0.05, 0.1, 0.92)));
    commands.spawn(panel(
        PanelId::Left,
        Node {
            top: px(74),
            left: px(10),
            width: px(260),
            ..abs.clone()
        },
    ));
    commands.spawn(panel(
        PanelId::Right,
        Node {
            top: px(74),
            right: px(10),
            width: px(320),
            max_height: percent(66),
            overflow: Overflow::clip_y(),
            ..abs.clone()
        },
    ));
    commands.spawn(panel(
        PanelId::BottomLeft,
        Node {
            bottom: px(10),
            left: px(10),
            flex_direction: FlexDirection::Row,
            column_gap: px(10),
            ..abs.clone()
        },
    ));
    commands.spawn(panel(
        PanelId::BottomRight,
        Node {
            bottom: px(10),
            right: px(10),
            width: px(320),
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
            let mut e = c.spawn(panel(id, inner));
            if !bg {
                e.insert((BackgroundColor(Color::NONE), BorderColor::all(Color::NONE)));
            } else if id == PanelId::Window {
                e.insert(BackgroundColor(Color::srgba(0.02, 0.045, 0.09, 0.97)));
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
            padding: UiRect::all(px(6)),
            column_gap: px(2),
            ..card()
        },
        true,
    );
    centred(
        &mut commands,
        PanelId::Flyout,
        Node {
            bottom: px(98),
            ..abs.clone()
        },
        Node {
            max_width: px(820),
            ..card()
        },
        true,
    );
    centred(
        &mut commands,
        PanelId::Hint,
        Node {
            bottom: px(98),
            ..abs.clone()
        },
        Node {
            max_width: px(760),
            padding: UiRect::axes(px(12), px(6)),
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
                top: px(70),
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
                    border: UiRect::all(px(2)),
                    ..card()
                },
            ))
            .insert(BorderColor::all(Color::srgba(1.0, 0.85, 0.2, 0.7)));
            c.spawn(panel(
                PanelId::Toasts,
                Node {
                    padding: UiRect::all(px(0)),
                    align_items: AlignItems::Center,
                    ..card()
                },
            ))
            .insert((BackgroundColor(Color::NONE), BorderColor::all(Color::NONE)));
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
            padding: UiRect::all(px(16)),
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
            ..card()
        },
        true,
    );
    // Full-screen menu layer above everything else.
    commands
        .spawn(panel(
            PanelId::Menu,
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                top: px(0),
                width: percent(100),
                height: percent(100),
                padding: UiRect::ZERO,
                border: UiRect::ZERO,
                border_radius: BorderRadius::ZERO,
                ..default()
            },
        ))
        .insert((
            BackgroundColor(Color::NONE),
            BorderColor::all(Color::NONE),
            GlobalZIndex(100),
        ));
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

fn btn_colors(style: Btn) -> (Color, Color, Color) {
    match style {
        Btn::Normal => (
            Color::srgba(0.07, 0.14, 0.25, 0.95),
            TEXT,
            Color::srgba(0.25, 0.45, 0.75, 0.6),
        ),
        Btn::Active => (
            Color::srgba(0.1, 0.33, 0.62, 0.95),
            Color::WHITE,
            Color::srgb(0.45, 0.78, 1.0),
        ),
        Btn::Disabled => (
            Color::srgba(0.05, 0.07, 0.1, 0.8),
            Color::srgb(0.4, 0.43, 0.48),
            Color::srgba(0.2, 0.22, 0.26, 0.6),
        ),
        Btn::Danger => (
            Color::srgba(0.35, 0.08, 0.08, 0.95),
            Color::WHITE,
            Color::srgb(0.85, 0.3, 0.3),
        ),
        Btn::Flat => (Color::NONE, TEXT, Color::NONE),
        Btn::Primary => (
            Color::srgba(0.06, 0.2, 0.4, 0.95),
            Color::WHITE,
            Color::srgb(0.4, 0.7, 1.0),
        ),
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

/// Make an entity a clickable button that queues `action`.
fn make_button(
    e: &mut EntityCommands,
    action: &UiAction,
    style: Btn,
    label: &str,
    bg: Color,
    border: Color,
) {
    e.insert((BtnBase(bg, border), BtnLabel(label.to_string())));
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
        UiNode::Icon(i, c, size) => {
            parent.spawn(icon_bundle(icons, *i, *c, *size));
        }
        UiNode::Dot(c, size) => {
            parent.spawn((
                Node {
                    width: px(*size),
                    height: px(*size),
                    flex_shrink: 0.0,
                    border_radius: BorderRadius::all(percent(50)),
                    ..default()
                },
                BackgroundColor(*c),
            ));
        }
        UiNode::Bar(frac, c) => {
            parent
                .spawn((
                    Node {
                        width: percent(100),
                        height: px(6),
                        border_radius: BorderRadius::all(px(3)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.1)),
                ))
                .with_child((
                    Node {
                        width: percent(frac.clamp(0.0, 1.0) * 100.0),
                        height: percent(100),
                        border_radius: BorderRadius::all(px(3)),
                        ..default()
                    },
                    BackgroundColor(*c),
                ));
        }
        UiNode::Divider => {
            parent.spawn((
                Node {
                    width: percent(100),
                    height: px(1),
                    margin: UiRect::vertical(px(3)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.4, 0.6, 0.9, 0.25)),
            ));
        }
        UiNode::Button(label, action, style) => {
            let (bg, fg, border) = btn_colors(*style);
            let mut e = parent.spawn((
                Node {
                    padding: UiRect::axes(px(8), px(3)),
                    margin: UiRect::all(px(1)),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(5)),
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                BackgroundColor(bg),
                BorderColor::all(border),
                children![text_bundle(label, fg, 12.5)],
            ));
            make_button(&mut e, action, *style, label, bg, border);
            if *style == Btn::Primary {
                e.insert(Node {
                    padding: UiRect::axes(px(16), px(10)),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(6)),
                    justify_content: JustifyContent::Center,
                    width: percent(100),
                    ..default()
                });
            }
        }
        UiNode::Tile(icon, label, action, style) => {
            let (bg, fg, border) = btn_colors(*style);
            let bg = if *style == Btn::Normal {
                Color::NONE
            } else {
                bg
            };
            let border = if *style == Btn::Normal {
                Color::NONE
            } else {
                border
            };
            let mut e = parent.spawn((
                Node {
                    width: px(86),
                    height: px(70),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    row_gap: px(6),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(6)),
                    ..default()
                },
                BackgroundColor(bg),
                BorderColor::all(border),
            ));
            e.with_children(|c| {
                c.spawn(icon_bundle(icons, *icon, fg, 28.0));
                c.spawn(text_bundle(label, fg, 12.5));
            });
            make_button(&mut e, action, *style, label, bg, border);
        }
        UiNode::IconBtn(icon, action, style, size) => {
            let (bg, fg, border) = btn_colors(*style);
            let mut e = parent.spawn((
                Node {
                    width: px(size + 14.0),
                    height: px(size + 14.0),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(6)),
                    ..default()
                },
                BackgroundColor(bg),
                BorderColor::all(border),
            ));
            e.with_child(icon_bundle(icons, *icon, fg, *size));
            make_button(&mut e, action, *style, &format!("{icon:?}"), bg, border);
        }
        UiNode::ListBtn(icon, ic, label, right, action, style) => {
            let (bg, fg, border) = btn_colors(*style);
            let mut e = parent.spawn((
                Node {
                    width: percent(100),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: px(10),
                    padding: UiRect::axes(px(10), px(8)),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(6)),
                    ..default()
                },
                BackgroundColor(bg),
                BorderColor::all(border),
            ));
            e.with_children(|c| {
                c.spawn(icon_bundle(icons, *icon, *ic, 20.0));
                c.spawn((
                    Node {
                        flex_grow: 1.0,
                        ..default()
                    },
                    children![text_bundle(label, fg, 13.5)],
                ));
                c.spawn(text_bundle(right, DIM, 12.5));
            });
            make_button(&mut e, action, *style, label, bg, border);
        }
        UiNode::Minimap(image, size) => {
            parent
                .spawn((
                    Node {
                        width: px(*size),
                        height: px(*size),
                        border: UiRect::all(px(1)),
                        border_radius: BorderRadius::all(px(6)),
                        ..default()
                    },
                    BorderColor::all(PANEL_BORDER),
                    ImageNode::new(image.clone()),
                ))
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
                    column_gap: px(6),
                    row_gap: px(2),
                    width: if matches!(node, UiNode::Spread(_)) {
                        percent(100)
                    } else {
                        Val::Auto
                    },
                    ..default()
                })
                .with_children(|p| {
                    for n in items {
                        spawn_ui_node(p, n, icons, scroll);
                    }
                });
        }
        UiNode::Col(items) => {
            parent
                .spawn(Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(1),
                    ..default()
                })
                .with_children(|p| {
                    for n in items {
                        spawn_ui_node(p, n, icons, scroll);
                    }
                });
        }
        UiNode::Card(items, accent) => {
            parent
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: px(2),
                        padding: UiRect::all(px(7)),
                        border: UiRect::all(px(1)),
                        border_radius: BorderRadius::all(px(6)),
                        ..default()
                    },
                    BackgroundColor(accent.with_alpha(0.08)),
                    BorderColor::all(accent.with_alpha(0.6)),
                ))
                .with_children(|p| {
                    for n in items {
                        spawn_ui_node(p, n, icons, scroll);
                    }
                });
        }
        UiNode::Columns(cols) => {
            parent
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: px(10),
                    ..default()
                })
                .with_children(|p| {
                    for col in cols {
                        p.spawn(Node {
                            flex_direction: FlexDirection::Column,
                            flex_grow: 1.0,
                            flex_basis: px(0),
                            row_gap: px(2),
                            ..default()
                        })
                        .with_children(|c| {
                            for n in col {
                                spawn_ui_node(c, n, icons, scroll);
                            }
                        });
                    }
                });
        }
        UiNode::Gap => {
            parent.spawn(Node {
                height: px(6),
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
                        row_gap: px(4),
                        overflow: Overflow::scroll_y(),
                        ..default()
                    },
                    ScrollArea,
                    UiScroll(key),
                    ScrollPosition(Vec2::new(0.0, y)),
                ))
                .with_children(|p| {
                    for n in items {
                        spawn_ui_node(p, n, icons, scroll);
                    }
                });
        }
        UiNode::Fixed(w, items) => {
            parent
                .spawn(Node {
                    width: px(*w),
                    flex_shrink: 0.0,
                    flex_direction: FlexDirection::Column,
                    row_gap: px(2),
                    ..default()
                })
                .with_children(|p| {
                    for n in items {
                        spawn_ui_node(p, n, icons, scroll);
                    }
                });
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
                    if *left { 0.0 } else { 0.64 },
                )),
            ));
            e.with_children(|p| {
                if *left {
                    p.spawn((
                        Node {
                            width: px(430),
                            flex_direction: FlexDirection::Column,
                            row_gap: px(9),
                            padding: UiRect::all(px(30)),
                            border_radius: BorderRadius::all(px(14)),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.01, 0.03, 0.07, 0.6)),
                    ))
                    .with_children(|c| {
                        for n in items {
                            spawn_ui_node(c, n, icons, scroll);
                        }
                    });
                } else {
                    for n in items {
                        spawn_ui_node(p, n, icons, scroll);
                    }
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
                        row_gap: px(7),
                        padding: UiRect::all(px(24)),
                        border: UiRect::all(px(1)),
                        border_radius: BorderRadius::all(px(14)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.02, 0.045, 0.09, 0.97)),
                    BorderColor::all(PANEL_BORDER),
                ))
                .with_children(|c| {
                    for n in items {
                        spawn_ui_node(c, n, icons, scroll);
                    }
                });
        }
        UiNode::Field {
            id,
            value,
            hint,
            focused,
        } => {
            let (bg, border) = if *focused {
                (Color::srgba(0.05, 0.12, 0.22, 1.0), ACCENT)
            } else {
                (
                    Color::srgba(0.03, 0.07, 0.13, 1.0),
                    Color::srgba(0.25, 0.45, 0.75, 0.6),
                )
            };
            let (shown, color) = if value.is_empty() && !*focused {
                (hint.clone(), DIM)
            } else if *focused {
                (format!("{value}|"), Color::WHITE)
            } else {
                (value.clone(), TEXT)
            };
            let mut e = parent.spawn((
                Node {
                    width: percent(100),
                    padding: UiRect::axes(px(10), px(8)),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(6)),
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
        }
        UiNode::Toggle(label, on, action) => {
            let n = UiNode::Spread(vec![
                UiNode::sized(label.clone(), TEXT, 14.0),
                UiNode::Button(
                    if *on { "ON".into() } else { "OFF".into() },
                    action.clone(),
                    if *on { Btn::Active } else { Btn::Normal },
                ),
            ]);
            spawn_ui_node(parent, &n, icons, scroll);
        }
        UiNode::Stepper {
            label,
            value,
            frac,
            dec,
            inc,
        } => {
            let mut mid = vec![UiNode::Button("-".into(), dec.clone(), Btn::Normal)];
            if let Some(f) = frac {
                mid.push(UiNode::Fixed(110.0, vec![UiNode::Bar(*f, ACCENT)]));
            }
            mid.push(UiNode::Fixed(
                64.0,
                vec![UiNode::sized(value.clone(), Color::WHITE, 14.0)],
            ));
            mid.push(UiNode::Button("+".into(), inc.clone(), Btn::Normal));
            let n = UiNode::Spread(vec![
                UiNode::sized(label.clone(), TEXT, 14.0),
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
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(8)),
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
                c.spawn(text_bundle(label, fg, 20.0));
                if !note.is_empty() {
                    c.spawn(text_bundle(note, DIM, 12.5));
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
                    accent.with_alpha(0.22),
                    accent.with_alpha(0.9),
                    Color::WHITE,
                    *accent,
                ),
                Btn::Primary => (
                    Color::srgba(0.08, 0.2, 0.36, 0.98),
                    ACCENT,
                    Color::WHITE,
                    ACCENT,
                ),
                Btn::Disabled => (
                    Color::srgba(0.03, 0.045, 0.07, 0.92),
                    Color::srgba(0.3, 0.35, 0.45, 0.35),
                    DIM,
                    accent.with_alpha(0.35),
                ),
                _ => (
                    Color::srgba(0.05, 0.09, 0.16, 0.97),
                    accent.with_alpha(0.75),
                    TEXT,
                    *accent,
                ),
            };
            let mut e = parent.spawn((
                Node {
                    width: px(174),
                    min_height: px(42),
                    flex_shrink: 0.0,
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: px(6),
                    padding: UiRect::axes(px(7), px(3)),
                    border: UiRect::all(px(if *style == Btn::Primary { 2 } else { 1 })),
                    border_radius: BorderRadius::all(px(7)),
                    ..default()
                },
                BackgroundColor(bg),
                BorderColor::all(border),
            ));
            e.with_children(|c| {
                c.spawn((
                    Node {
                        width: px(28),
                        height: px(28),
                        flex_shrink: 0.0,
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        border: UiRect::all(px(2)),
                        border_radius: BorderRadius::all(percent(50)),
                        ..default()
                    },
                    BorderColor::all(ring),
                    BackgroundColor(ring.with_alpha(0.12)),
                ))
                .with_child(icon_bundle(icons, *icon, ring, 15.0));
                c.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    flex_grow: 1.0,
                    row_gap: px(1),
                    ..default()
                })
                .with_children(|t| {
                    t.spawn(text_bundle(title, fg, 11.5));
                    let sc = if *style == Btn::Disabled {
                        DIM
                    } else {
                        ring.mix(&Color::WHITE, 0.25)
                    };
                    t.spawn(text_bundle(status, sc, 10.0));
                    if let Some(f) = frac {
                        t.spawn((
                            Node {
                                width: percent(100),
                                height: px(3),
                                border_radius: BorderRadius::all(px(2)),
                                ..default()
                            },
                            BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.1)),
                        ))
                        .with_child((
                            Node {
                                width: percent(f.clamp(0.0, 1.0) * 100.0),
                                height: percent(100),
                                border_radius: BorderRadius::all(px(2)),
                                ..default()
                            },
                            BackgroundColor(ACCENT),
                        ));
                    }
                });
            });
            make_button(&mut e, action, *style, title, bg, border);
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

fn button_hover(
    mut q: Query<(&Hovered, &BtnBase, &mut BackgroundColor, &mut BorderColor), Changed<Hovered>>,
) {
    for (h, base, mut bg, mut border) in &mut q {
        if h.get() {
            bg.0 = if base.0 == Color::NONE {
                Color::srgba(0.3, 0.55, 0.9, 0.18)
            } else {
                base.0.lighter(0.08)
            };
            *border = BorderColor::all(ACCENT.with_alpha(0.8));
        } else {
            bg.0 = base.0;
            *border = BorderColor::all(base.1);
        }
    }
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
    mut tech_tab: ResMut<TechTab>,
    mut codex_tab: ResMut<CodexTab>,
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
                speed.0 = s.min(sim.0.research.max_speed());
                virt.unpause();
            }
            UiAction::SpeedStep(up) => {
                speed.0 = crate::time_control::next_speed(speed.0, sim.0.research.max_speed(), up);
                virt.unpause();
            }
            UiAction::TogglePause => virt.toggle(),
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
