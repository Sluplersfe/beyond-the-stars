//! Panel contents. Pure functions of game state → `UiNode` trees.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use sim::buildings::BeltTier;
use sim::research::Branch;
use sim::surface::{Building, Terrain};
use sim::universe::{PlanetCategory, SystemKnowledge};
use sim::{
    BuildingKind, Command, Drive, Hull, Item, Placeable, Planet, PlanetId, Policy, Recipe, Route,
    Ship, ShipLoc, ShipOrder, Sim, Status, Tech,
};

use crate::camera::CameraRig;
use crate::hud::{Minimap, Rates, scope_of, snapshot};
use crate::icons::Icon;
use crate::sim_bridge::planet_name;
use crate::state::*;
use crate::time_control::{SimRate, Speed};
use crate::ui::{ACCENT, Btn, BuildCategory, CodexTab, DIM, PanelId, TEXT, UiAction, UiNode};

#[derive(SystemParam)]
pub struct PanelCtx<'w> {
    pub sim: Res<'w, SimState>,
    pub view: Res<'w, View>,
    pub tool: Res<'w, Tool>,
    pub dir: Res<'w, BuildDir>,
    pub selected: Res<'w, Selected>,
    pub window: Res<'w, OpenWindow>,
    pub speed: Res<'w, Speed>,
    pub virt: Res<'w, Time<Virtual>>,
    pub warp: Res<'w, Warp>,
    pub log: Res<'w, MessageLog>,
    pub overlay: Res<'w, GenerationOverlay>,
    pub hover: Res<'w, HoverTile>,
    pub focus: Res<'w, FocusShip>,
    pub cat: Res<'w, BuildCategory>,
    pub rates: Res<'w, Rates>,
    pub mini: Res<'w, Minimap>,
    pub rate: Res<'w, SimRate>,
    pub rig: Res<'w, CameraRig>,
    pub screen: Res<'w, crate::menu::Screen>,
    pub menu: Res<'w, crate::menu::MenuState>,
    pub settings: Res<'w, crate::settings::Settings>,
    pub session: Res<'w, crate::menu::Session>,
    pub tutorial: Res<'w, crate::tutorial::Tutorial>,
    pub tech_tab: Res<'w, crate::ui::TechTab>,
    pub codex_tab: Res<'w, crate::ui::CodexTab>,
}

const GOOD: Color = Color::srgb(0.45, 0.95, 0.5);
const WARN: Color = Color::srgb(1.0, 0.8, 0.3);
const BAD: Color = Color::srgb(1.0, 0.4, 0.35);
const INFO: Color = Color::srgb(0.6, 0.8, 1.0);
const PURPLE: Color = Color::srgb(0.68, 0.48, 1.0);
const POWER: Color = Color::srgb(0.35, 0.95, 0.55);
const YELLOW: Color = Color::srgb(1.0, 0.82, 0.25);

/// Galaxy minimap half-width, in galaxy-view world units.
pub const GALAXY_MINIMAP_RADIUS: f32 = 70.0;

/// Build-toolbar categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Cat {
    Mining,
    Smelting,
    Production,
    Power,
    Logistics,
    Housing,
    Research,
    Defense,
}

impl Cat {
    pub const ALL: [Cat; 8] = [
        Cat::Mining,
        Cat::Smelting,
        Cat::Production,
        Cat::Power,
        Cat::Logistics,
        Cat::Housing,
        Cat::Research,
        Cat::Defense,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Cat::Mining => "Mining",
            Cat::Smelting => "Smelting",
            Cat::Production => "Production",
            Cat::Power => "Power",
            Cat::Logistics => "Logistics",
            Cat::Housing => "Housing",
            Cat::Research => "Research",
            Cat::Defense => "Defense",
        }
    }

    pub fn icon(self) -> Icon {
        match self {
            Cat::Mining => Icon::Pick,
            Cat::Smelting => Icon::Flame,
            Cat::Production => Icon::Factory,
            Cat::Power => Icon::Bolt,
            Cat::Logistics => Icon::Belt,
            Cat::Housing => Icon::House,
            Cat::Research => Icon::Flask,
            Cat::Defense => Icon::Shield,
        }
    }

    pub fn items(self) -> Vec<Placeable> {
        use BuildingKind as K;
        let b = |ks: &[K]| {
            ks.iter()
                .map(|k| Placeable::Building(*k))
                .collect::<Vec<_>>()
        };
        match self {
            Cat::Mining => b(&[K::Miner, K::MinerMk2]),
            Cat::Smelting => b(&[K::Smelter, K::SmelterMk2, K::Refinery]),
            Cat::Production => b(&[K::Assembler, K::AssemblerMk2, K::Shipyard]),
            Cat::Power => b(&[K::Generator, K::Solar, K::Reactor, K::Transmitter]),
            Cat::Logistics => BeltTier::ALL
                .iter()
                .map(|t| Placeable::Belt(*t))
                .chain(b(&[
                    K::Inserter,
                    K::FastInserter,
                    K::Splitter,
                    K::Merger,
                    K::Provider,
                    K::Depot,
                ]))
                .collect(),
            Cat::Housing => b(&[K::House, K::Habitat]),
            Cat::Research => b(&[K::Lab, K::LabMk2, K::Observatory, K::Beacon]),
            Cat::Defense => b(&[K::Turret]),
        }
    }

    pub fn of(kind: BuildingKind) -> Cat {
        Cat::ALL
            .into_iter()
            .find(|c| c.items().contains(&Placeable::Building(kind)))
            .unwrap_or(Cat::Production)
    }
}

pub fn build(id: PanelId, ctx: &PanelCtx) -> Vec<UiNode> {
    if id == PanelId::Menu {
        // The HTML overlay draws the menu itself when it is active.
        if ctx.menu.web {
            return vec![];
        }
        return crate::menu::content(&crate::menu::MenuView {
            menu: &ctx.menu,
            settings: &ctx.settings,
            session: &ctx.session,
            sim: &ctx.sim.0,
            tutorial_active: ctx.tutorial.active,
        });
    }
    if *ctx.screen != crate::menu::Screen::Playing {
        return vec![];
    }
    let planet = matches!(*ctx.view, View::Planet(_));
    match id {
        PanelId::Menu => vec![],
        PanelId::Tutorial => crate::tutorial::panel(ctx),
        PanelId::TopBar => top_bar(ctx),
        PanelId::Left if planet => planet_overview(ctx),
        PanelId::Left => map_nav(ctx),
        PanelId::Right => info(ctx),
        PanelId::BottomLeft if planet => planet_minimap(ctx),
        PanelId::BottomLeft => galaxy_minimap(ctx),
        PanelId::Toolbar if planet => build_toolbar(ctx),
        PanelId::Toolbar => nav_toolbar(ctx),
        PanelId::Flyout => flyout(ctx),
        PanelId::BottomRight if planet => research_card(ctx),
        PanelId::BottomRight => destination_card(ctx),
        PanelId::Toasts => log(ctx),
        PanelId::Hint => hint(ctx),
        PanelId::Window => window(ctx),
        PanelId::Overlay => overlay(ctx),
    }
}

fn item_icon(i: Item) -> Icon {
    if i.tier() == 1 || i.tier() == 6 && matches!(i, Item::ExoticCrystal | Item::VoidShard) {
        Icon::Gem
    } else {
        Icon::Cube
    }
}

fn item_color(i: Item) -> Color {
    rgb(i.rgb()).lighter(0.12)
}

fn rate_text(r: f32) -> (String, Color) {
    if r.abs() < 0.5 {
        ("+0/min".into(), DIM)
    } else if r > 0.0 {
        (format!("+{}/min", fmt_num(r as f64)), GOOD)
    } else {
        (format!("{}/min", fmt_num(r as f64)), BAD)
    }
}

fn stat(icon: Icon, color: Color, value: String, sub: String, sub_color: Color) -> UiNode {
    UiNode::Row(vec![
        UiNode::Icon(icon, color, 26.0),
        UiNode::Col(vec![
            UiNode::sized(value, Color::WHITE, 16.0),
            UiNode::sized(sub, sub_color, 11.0),
        ]),
    ])
}

fn game_clock(tick: u64) -> String {
    let s = tick / sim::TICKS_PER_SEC;
    format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
}

fn top_bar(ctx: &PanelCtx) -> Vec<UiNode> {
    let sim = &ctx.sim.0;
    let scope = scope_of(&ctx.view, sim);
    let (emblem, title, subtitle) = match *ctx.view {
        View::Planet(p) => {
            let planet = sim.planet(p);
            let name = planet.map(|p| p.name.clone()).unwrap_or_default();
            let cat = planet.map(|p| p.category.name()).unwrap_or("");
            let here = if sim.player.at == Some(p) {
                "you are here"
            } else {
                "remote view"
            };
            (Icon::Planet, name, format!("{cat} - {here}"))
        }
        View::Map(level) => {
            let sub = match level {
                MapLevel::Universe => "Intergalactic View".to_string(),
                MapLevel::Galaxy(g) => {
                    format!("{} - Galaxy View", sim.universe.galaxies[g as usize].name)
                }
                MapLevel::System(s) => format!(
                    "{} - System View",
                    sim.universe
                        .system(s)
                        .map(|s| s.name.as_str())
                        .unwrap_or("")
                ),
            };
            (Icon::Galaxy, "Star Map".to_string(), sub)
        }
    };
    let map_level = match *ctx.view {
        View::Planet(p) => MapLevel::Galaxy(p.galaxy),
        View::Map(_) => MapLevel::Galaxy(sim.player.at.map(|p| p.galaxy).unwrap_or(0)),
    };
    let mut out = vec![UiNode::Row(vec![
        UiNode::IconBtn(emblem, UiAction::ViewMap(map_level), Btn::Active, 30.0),
        UiNode::Col(vec![
            UiNode::sized(title, Color::srgb(0.55, 0.85, 1.0), 19.0),
            UiNode::sized(subtitle, DIM, 12.0),
        ]),
    ])];

    // Resources with net rate per minute.
    let (items, pop) = snapshot(sim, scope);
    let mut ranked: Vec<(Item, u32)> = items.into_iter().filter(|(_, n)| *n > 0).collect();
    ranked.sort_by_key(|(i, n)| (std::cmp::Reverse(*n), *i));
    let mut res: Vec<UiNode> = Vec::new();
    for (i, n) in ranked.into_iter().take(6) {
        let (r, c) = rate_text(ctx.rates.item_per_min(i));
        res.push(stat(item_icon(i), item_color(i), fmt_num(n as f64), r, c));
    }
    out.push(UiNode::Row(res));
    out.push(UiNode::Spacer);

    // Population and power.
    let (r, c) = rate_text(ctx.rates.pop_per_min());
    out.push(stat(Icon::Person, INFO, pop.to_string(), r, c));
    let (produced, demand, capacity, shortage) = sim
        .colonies()
        .filter(|p| scope == crate::hud::Scope::Civ || Some(p.id) == ctx_planet(ctx))
        .filter_map(|p| p.colony.as_ref())
        .fold((0i64, 0i64, 0i64, false), |a, c| {
            (
                a.0 + c.power.produced as i64,
                a.1 + c.power.demand as i64,
                a.2 + c.power.capacity as i64,
                a.3 || c.power.shortage,
            )
        });
    let _ = produced;
    let surplus = capacity - demand;
    out.push(stat(
        Icon::Bolt,
        YELLOW,
        fmt_kw(demand),
        format!(
            "{}{} spare",
            if surplus >= 0 { "+" } else { "" },
            fmt_kw(surplus)
        ),
        if shortage || surplus < 0 { BAD } else { GOOD },
    ));

    // Quick navigation (planet view has no bottom nav bar).
    if matches!(*ctx.view, View::Planet(_)) {
        let w = ctx.window.0;
        let nav = |i: Icon, p: Popup| {
            UiNode::IconBtn(
                i,
                UiAction::OpenWindow(p),
                if w == p { Btn::Active } else { Btn::Normal },
                18.0,
            )
        };
        out.push(UiNode::Row(vec![
            nav(Icon::Flask, Popup::Research),
            nav(Icon::Atom, Popup::TechTree),
            nav(Icon::Book, Popup::Codex),
            nav(Icon::Rocket, Popup::Ships),
            nav(Icon::Colony, Popup::Colonies),
        ]));
    }

    // Time controls.
    let paused = ctx.virt.is_paused();
    let max = sim.research.max_speed();
    let speed = ctx.speed.0;
    let label = if ctx.warp.0.is_some() {
        "Instant-Warp".to_string()
    } else if paused {
        "Paused".to_string()
    } else if speed == 1 {
        "Normal".to_string()
    } else {
        let eff = ctx.rate.effective();
        if eff > 1.0 && eff < speed as f32 * 0.75 {
            format!("Fast {speed}x (~{:.0}x)", eff)
        } else {
            format!("Fast {speed}x")
        }
    };
    out.push(UiNode::Row(vec![
        UiNode::IconBtn(
            Icon::Pause,
            UiAction::TogglePause,
            if paused { Btn::Danger } else { Btn::Normal },
            18.0,
        ),
        UiNode::IconBtn(
            Icon::Play,
            UiAction::SetSpeed(1),
            if !paused && speed == 1 {
                Btn::Active
            } else {
                Btn::Normal
            },
            18.0,
        ),
        UiNode::IconBtn(
            Icon::Fast,
            UiAction::SpeedStep(true),
            if max <= speed {
                Btn::Disabled
            } else if !paused && speed > 1 {
                Btn::Active
            } else {
                Btn::Normal
            },
            18.0,
        ),
        UiNode::Col(vec![
            UiNode::sized(game_clock(sim.tick()), Color::WHITE, 15.0),
            UiNode::sized(label, if paused { WARN } else { ACCENT }, 11.0),
        ]),
        UiNode::IconBtn(
            Icon::Gear,
            UiAction::OpenWindow(Popup::Help),
            Btn::Normal,
            20.0,
        ),
    ]));
    out
}

fn ctx_planet(ctx: &PanelCtx) -> Option<PlanetId> {
    match *ctx.view {
        View::Planet(p) => Some(p),
        View::Map(_) => None,
    }
}

fn labeled_bar(icon: Icon, color: Color, label: &str, value: String, frac: f32) -> Vec<UiNode> {
    vec![
        UiNode::Spread(vec![
            UiNode::Row(vec![UiNode::Icon(icon, color, 20.0), UiNode::text(label)]),
            UiNode::sized(value, TEXT, 12.5),
        ]),
        UiNode::Bar(frac, color),
        UiNode::Gap,
    ]
}

fn planet_overview(ctx: &PanelCtx) -> Vec<UiNode> {
    let View::Planet(p) = *ctx.view else {
        return vec![];
    };
    let sim = &ctx.sim.0;
    let mut out = vec![
        UiNode::sized("Planet Overview", Color::WHITE, 16.0),
        UiNode::Gap,
    ];
    let (Some(c), Some(s)) = (sim.colony(p), sim.surface(p)) else {
        out.push(UiNode::colored("Uncolonized", WARN));
        out.push(UiNode::dim("Deliver a colony kit by ship to build here."));
        return out;
    };
    out.push(UiNode::ListBtn(
        Icon::Cube,
        INFO,
        "Resources".into(),
        format!("{} items  >", fmt_num(c.inventory.total() as f64)),
        UiAction::OpenWindow(Popup::Resources),
        Btn::Flat,
    ));
    let pw = c.power;
    let cap = pw.capacity.max(1) as f32;
    out.extend(labeled_bar(
        Icon::Bolt,
        if pw.shortage { BAD } else { POWER },
        "Power",
        format!(
            "{} / {}",
            fmt_kw(pw.demand as i64),
            fmt_kw(pw.capacity as i64)
        ),
        pw.demand as f32 / cap,
    ));
    let housing = sim::Colony::housing(s);
    out.extend(labeled_bar(
        Icon::Person,
        INFO,
        "Population",
        format!("{} / {}", c.population, housing),
        c.population as f32 / housing.max(1) as f32,
    ));
    let r = &sim.research;
    let (rv, rf) = match r.current {
        Some(t) => {
            let cost = t.info().cost;
            (
                format!(
                    "{} / {}",
                    fmt_num(r.progress_of(t) as f64),
                    fmt_num(cost as f64)
                ),
                r.progress_of(t) as f32 / cost as f32,
            )
        }
        None => (format!("+{}/s", research_rate(sim)), 0.0),
    };
    out.extend(labeled_bar(Icon::Flask, PURPLE, "Research", rv, rf));
    let nests = s.nests.len();
    let (threat, tc) = match nests {
        0 => ("None", GOOD),
        1 => ("Low", YELLOW),
        2..=3 => ("Medium", WARN),
        _ => ("High", BAD),
    };
    out.push(UiNode::Spread(vec![
        UiNode::Row(vec![
            UiNode::Icon(Icon::Warning, BAD, 20.0),
            UiNode::text("Threats"),
        ]),
        UiNode::colored(threat, tc),
    ]));
    if !c.active {
        let here = sim.player.at == Some(p);
        out.push(UiNode::Gap);
        out.push(UiNode::colored("Work systems not established", WARN));
        out.push(UiNode::btn_if(
            if here {
                "Establish work systems"
            } else {
                "Visit in person to establish"
            },
            UiAction::Cmd(Command::ActivateColony { planet: p }),
            here,
        ));
    }
    if nests > 0 {
        let policy = |label: &str, pol: Policy| {
            UiNode::Button(
                label.into(),
                UiAction::Cmd(Command::SetPolicy {
                    planet: p,
                    policy: pol,
                }),
                if c.policy == pol {
                    Btn::Active
                } else {
                    Btn::Normal
                },
            )
        };
        out.push(UiNode::Row(vec![
            UiNode::dim("Policy"),
            policy("Eliminate", Policy::Eliminate),
            policy("Coexist", Policy::Coexist),
        ]));
    }
    out
}

fn planet_minimap(ctx: &PanelCtx) -> Vec<UiNode> {
    let View::Planet(p) = *ctx.view else {
        return vec![];
    };
    let sim = &ctx.sim.0;
    let r = &sim.research;
    let has = |k: BuildingKind| k.spec().tech.is_none_or(|t| r.has(t));
    let belt = BeltTier::ALL
        .iter()
        .copied()
        .rfind(|t| t.tech().is_none_or(|x| r.has(x)))
        .unwrap_or(BeltTier::Basic);
    let inserter = if has(BuildingKind::FastInserter) {
        BuildingKind::FastInserter
    } else {
        BuildingKind::Inserter
    };
    let tool = *ctx.tool;
    let style = |active: bool| if active { Btn::Active } else { Btn::Flat };
    let mut menu = vec![
        UiNode::ListBtn(
            Icon::Colony,
            INFO,
            "Buildings".into(),
            String::new(),
            UiAction::Category(Cat::Production),
            style(ctx.cat.0.is_some() && ctx.cat.0 != Some(Cat::Power)),
        ),
        UiNode::ListBtn(
            Icon::Belt,
            INFO,
            "Conveyors".into(),
            "E".into(),
            UiAction::SetTool(Tool::Build(Placeable::Belt(belt))),
            style(matches!(tool, Tool::Build(Placeable::Belt(_)))),
        ),
        UiNode::ListBtn(
            Icon::Arm,
            INFO,
            "Inserters".into(),
            "I".into(),
            UiAction::SetTool(Tool::Build(Placeable::Building(inserter))),
            style(matches!(tool, Tool::Build(Placeable::Building(k)) if k.is_inserter())),
        ),
        UiNode::ListBtn(
            Icon::Bolt,
            YELLOW,
            "Power".into(),
            String::new(),
            UiAction::Category(Cat::Power),
            style(ctx.cat.0 == Some(Cat::Power)),
        ),
        UiNode::ListBtn(
            Icon::Trash,
            BAD,
            "Demolish".into(),
            "X".into(),
            UiAction::SetTool(Tool::Remove),
            if tool == Tool::Remove {
                Btn::Danger
            } else {
                Btn::Flat
            },
        ),
    ];
    if tool != Tool::Select {
        menu.push(UiNode::ListBtn(
            Icon::Cursor,
            TEXT,
            "Select".into(),
            "Q".into(),
            UiAction::SetTool(Tool::Select),
            Btn::Flat,
        ));
    }
    let _ = p;
    vec![
        UiNode::Minimap(ctx.mini.planet.clone(), 190.0),
        UiNode::Fixed(170.0, menu),
    ]
}

fn galaxy_minimap(ctx: &PanelCtx) -> Vec<UiNode> {
    match *ctx.view {
        View::Map(MapLevel::Galaxy(_)) | View::Map(MapLevel::System(_)) => {
            vec![UiNode::Minimap(ctx.mini.galaxy.clone(), 190.0)]
        }
        _ => vec![],
    }
}

fn build_toolbar(ctx: &PanelCtx) -> Vec<UiNode> {
    let View::Planet(p) = *ctx.view else {
        return vec![];
    };
    if ctx.sim.0.colony(p).is_none() {
        return vec![];
    }
    Cat::ALL
        .iter()
        .map(|c| {
            let in_cat = matches!(*ctx.tool, Tool::Build(pl) if c.items().contains(&pl));
            UiNode::Tile(
                c.icon(),
                c.name().into(),
                UiAction::Category(*c),
                if ctx.cat.0 == Some(*c) || in_cat {
                    Btn::Active
                } else {
                    Btn::Normal
                },
            )
        })
        .collect()
}

fn nav_toolbar(ctx: &PanelCtx) -> Vec<UiNode> {
    let sim = &ctx.sim.0;
    let g = sim.player.at.map(|p| p.galaxy).unwrap_or(0);
    let w = ctx.window.0;
    let win = |i: Icon, label: &str, p: Popup| {
        UiNode::Tile(
            i,
            label.into(),
            UiAction::OpenWindow(p),
            if w == p { Btn::Active } else { Btn::Normal },
        )
    };
    let on_galaxy = matches!(*ctx.view, View::Map(MapLevel::Galaxy(_)));
    vec![
        UiNode::Tile(
            Icon::Planet,
            "Map".into(),
            UiAction::ViewMap(MapLevel::Galaxy(g)),
            if on_galaxy && w == Popup::None {
                Btn::Active
            } else {
                Btn::Normal
            },
        ),
        win(Icon::Flask, "Research", Popup::Research),
        win(Icon::Rocket, "Ships", Popup::Ships),
        win(Icon::Colony, "Colonies", Popup::Colonies),
        win(Icon::Box, "Logistics", Popup::Logistics),
        win(Icon::Atom, "Technology", Popup::TechTree),
        win(Icon::Book, "Codex", Popup::Codex),
        win(Icon::Gear, "Settings", Popup::Help),
    ]
}

fn placeable_key(pl: Placeable) -> &'static str {
    match pl {
        Placeable::Belt(BeltTier::Basic) => " [E]",
        Placeable::Building(k) => match k {
            BuildingKind::Inserter => " [I]",
            BuildingKind::Miner => " [N]",
            BuildingKind::Smelter => " [F]",
            BuildingKind::Assembler => " [Y]",
            BuildingKind::Generator => " [G]",
            BuildingKind::House => " [H]",
            _ => "",
        },
        _ => "",
    }
}

fn placing_details(ctx: &PanelCtx, pl: Placeable) -> Vec<UiNode> {
    let View::Planet(p) = *ctx.view else {
        return vec![];
    };
    let sim = &ctx.sim.0;
    let (w, h) = pl.size();
    let mut cost = vec![UiNode::dim("Cost:")];
    cost.extend(cost_text(pl.cost(), sim.colony(p).map(|c| &c.inventory)));
    let desc = match pl {
        Placeable::Building(k) => k.spec().desc,
        Placeable::Belt(_) => {
            "Drag to lay belts. Belts feed depots directly; machines need inserters."
        }
    };
    vec![
        UiNode::Row(vec![
            UiNode::sized(pl.name(), Color::WHITE, 14.0),
            UiNode::dim(format!(
                "{w}x{h}  facing {:?}  -  R rotate, right-click remove, Q done",
                ctx.dir.0
            )),
        ]),
        UiNode::Wrap(cost),
        UiNode::dim(desc),
    ]
}

fn flyout(ctx: &PanelCtx) -> Vec<UiNode> {
    let (View::Planet(p), Some(cat)) = (*ctx.view, ctx.cat.0) else {
        return vec![];
    };
    let sim = &ctx.sim.0;
    if sim.colony(p).is_none() {
        return vec![];
    }
    let r = &sim.research;
    let inv = sim.colony(p).map(|c| &c.inventory);
    let buttons: Vec<UiNode> = cat
        .items()
        .into_iter()
        .map(|pl| {
            let unlocked = pl.tech().is_none_or(|t| r.has(t));
            let affordable = inv.is_some_and(|i| i.has_all(pl.cost()));
            let label = if unlocked {
                format!("{}{}", pl.name(), placeable_key(pl))
            } else {
                format!("{} (locked)", pl.name())
            };
            UiNode::Button(
                label,
                UiAction::SetTool(Tool::Build(pl)),
                if !unlocked {
                    Btn::Disabled
                } else if *ctx.tool == Tool::Build(pl) {
                    Btn::Active
                } else if affordable {
                    Btn::Normal
                } else {
                    Btn::Flat
                },
            )
        })
        .collect();
    let mut out = vec![
        UiNode::Spread(vec![
            UiNode::Row(vec![
                UiNode::Icon(cat.icon(), ACCENT, 20.0),
                UiNode::sized(cat.name(), Color::WHITE, 15.0),
            ]),
            UiNode::IconBtn(Icon::Close, UiAction::Category(cat), Btn::Flat, 12.0),
        ]),
        UiNode::Wrap(buttons),
    ];
    if let Tool::Build(pl) = *ctx.tool
        && cat.items().contains(&pl)
    {
        out.push(UiNode::Divider);
        out.extend(placing_details(ctx, pl));
    }
    out
}

fn hint(ctx: &PanelCtx) -> Vec<UiNode> {
    let sim = &ctx.sim.0;
    match *ctx.view {
        View::Planet(p) => {
            if ctx.cat.0.is_some() && ctx.window.0 == Popup::None {
                return vec![];
            }
            match *ctx.tool {
                Tool::Build(pl) => placing_details(ctx, pl),
                Tool::Remove => vec![UiNode::colored(
                    "Demolish: click or drag. Materials and contents are refunded. Q to stop.",
                    WARN,
                )],
                Tool::Select => {
                    let Some(((x, y), s)) = ctx.hover.0.zip(sim.surface(p)) else {
                        return vec![];
                    };
                    let Some(t) = s.tile(x, y) else {
                        return vec![];
                    };
                    let terrain = match t.terrain {
                        Terrain::Ground => "Ground".to_string(),
                        Terrain::Rock => "Rock (unbuildable)".to_string(),
                        Terrain::Lava => "Lava (unbuildable)".to_string(),
                        Terrain::Deposit(i) => {
                            format!("{} deposit, richness {}", i.name(), s.richness_of(i))
                        }
                    };
                    let what = s
                        .building_at(x, y)
                        .map(|b| format!(" - {} ({})", b.kind.spec().name, b.status.label()))
                        .or_else(|| t.belt.map(|b| format!(" - {}", b.tier.name())))
                        .unwrap_or_default();
                    vec![UiNode::dim(format!("({x}, {y}) {terrain}{what}"))]
                }
            }
        }
        View::Map(_) => vec![UiNode::dim(
            "Click to select, click again (or double-click) to open. Right-click / Backspace: zoom out. Middle-drag or , . rotate.",
        )],
    }
}

fn research_card(ctx: &PanelCtx) -> Vec<UiNode> {
    let sim = &ctx.sim.0;
    let r = &sim.research;
    let mut out = vec![UiNode::Spread(vec![
        UiNode::sized("Current Research", Color::WHITE, 15.0),
        UiNode::IconBtn(
            Icon::Atom,
            UiAction::OpenWindow(Popup::Research),
            Btn::Flat,
            14.0,
        ),
    ])];
    match r.current {
        Some(t) => {
            let i = t.info();
            let prog = r.progress_of(t);
            let frac = prog as f32 / i.cost as f32;
            out.push(UiNode::Row(vec![
                UiNode::IconBtn(
                    Icon::Flask,
                    UiAction::OpenWindow(Popup::Research),
                    Btn::Primary,
                    28.0,
                ),
                UiNode::Fixed(
                    220.0,
                    vec![
                        UiNode::sized(i.name, Color::WHITE, 14.0),
                        UiNode::colored(format!("{}%", (frac * 100.0) as u32), ACCENT),
                        UiNode::Bar(frac, PURPLE),
                        UiNode::Spread(vec![
                            UiNode::dim(format!("+{}/s", research_rate(sim))),
                            UiNode::dim(format!(
                                "{} / {}",
                                fmt_num(prog as f64),
                                fmt_num(i.cost as f64)
                            )),
                        ]),
                    ],
                ),
            ]));
        }
        None => {
            out.push(UiNode::colored("No research project selected", WARN));
            out.push(UiNode::btn(
                "Choose research [T]",
                UiAction::OpenWindow(Popup::Research),
            ));
        }
    }
    out
}

fn richness_word(n: u8) -> (&'static str, Color) {
    match n {
        0 | 1 => ("Low", DIM),
        2 => ("Moderate", INFO),
        _ => ("Rich", GOOD),
    }
}

fn category_color(p: &Planet) -> Color {
    if p.category_known() {
        rgb(p.category.rgb())
    } else {
        Color::srgb(0.45, 0.48, 0.55)
    }
}

fn category_label(p: &Planet) -> String {
    if !p.category_known() {
        return "Uncharacterized".into();
    }
    match p.category {
        PlanetCategory::Living => "Terrestrial (Temperate)",
        PlanetCategory::Rocky => "Terrestrial (Rocky)",
        PlanetCategory::Ice => "Ice World",
        PlanetCategory::Volcanic => "Volcanic",
        PlanetCategory::MetalRich => "Metal-rich",
        PlanetCategory::Exotic => "Exotic",
    }
    .into()
}

fn system_card(ctx: &PanelCtx, sid: sim::SystemId) -> Vec<UiNode> {
    let sim = &ctx.sim.0;
    let Some(s) = sim.universe.system(sid) else {
        return vec![];
    };
    let star = rgb(s.star.rgb());
    let mut out = vec![
        UiNode::Row(vec![
            UiNode::Icon(Icon::Star, star, 34.0),
            UiNode::Col(vec![
                UiNode::sized(s.name.clone(), Color::WHITE, 19.0),
                UiNode::sized(s.star.name(), DIM, 12.0),
            ]),
        ]),
        UiNode::Row(
            s.planets
                .iter()
                .map(|p| UiNode::Dot(category_color(p), 24.0))
                .collect(),
        ),
        UiNode::dim(format!(
            "{} planets  -  {}",
            s.planets.len(),
            match s.knowledge {
                SystemKnowledge::Unknown => "unknown",
                SystemKnowledge::Detected => "scanned",
                SystemKnowledge::Visited => "visited",
            }
        )),
        UiNode::Divider,
        UiNode::head("Planets"),
    ];
    if s.planets.is_empty() {
        out.push(UiNode::dim("No planetary data. Send a ship or scan."));
    }
    for p in &s.planets {
        let (status, color) = planet_status(p);
        let _ = status;
        out.push(UiNode::ListBtn(
            Icon::Planet,
            category_color(p),
            p.name.clone(),
            category_label(p),
            UiAction::Select(Selection::Planet(p.id)),
            if ctx.selected.0 == Selection::Planet(p.id) {
                Btn::Active
            } else {
                Btn::Flat
            },
        ));
        let _ = color;
    }
    // Scanned resources across the system, best richness per item.
    let mut res: Vec<(Item, u8)> = Vec::new();
    for p in &s.planets {
        for (i, _) in p.known_resources() {
            let amount = p
                .profile
                .iter()
                .find(|(x, _)| *x == i)
                .map(|(_, n)| *n)
                .unwrap_or(1);
            match res.iter_mut().find(|(x, _)| *x == i) {
                Some(e) => e.1 = e.1.max(amount),
                None => res.push((i, amount)),
            }
        }
    }
    if !res.is_empty() {
        out.push(UiNode::Divider);
        out.push(UiNode::Row(vec![
            UiNode::head("Resources"),
            UiNode::dim("(scanned)"),
        ]));
        for (i, n) in res.into_iter().take(8) {
            let (word, c) = richness_word(n);
            out.push(UiNode::Row(vec![
                UiNode::Icon(item_icon(i), item_color(i), 22.0),
                UiNode::Col(vec![UiNode::text(i.name()), UiNode::sized(word, c, 11.0)]),
            ]));
        }
    }
    out
}

/// Ship the player would travel on from where they are, if any.
fn journey_ship(sim: &Sim, focus: Option<u32>) -> Option<&Ship> {
    let at = sim.player.at?;
    let docked = |s: &&Ship| s.docked_at() == Some(at);
    focus
        .and_then(|id| sim.ships.get(&id))
        .filter(docked)
        .or_else(|| {
            sim.ships
                .values()
                .filter(docked)
                .max_by_key(|s| (s.drive, s.hull.seats()))
        })
}

fn destination_card(ctx: &PanelCtx) -> Vec<UiNode> {
    let sim = &ctx.sim.0;
    let Selection::Planet(pid) = ctx.selected.0 else {
        return vec![];
    };
    let Some(p) = sim.planet(pid) else {
        return vec![];
    };
    let res: Vec<UiNode> = p
        .known_resources()
        .into_iter()
        .take(6)
        .map(|(i, _)| UiNode::Icon(item_icon(i), item_color(i), 20.0))
        .collect();
    let mut out = vec![
        UiNode::head("Selected Destination"),
        UiNode::Row(vec![
            UiNode::Dot(category_color(p), 52.0),
            UiNode::Col(vec![
                UiNode::sized(p.name.clone(), Color::WHITE, 17.0),
                UiNode::sized(category_label(p), DIM, 12.0),
                UiNode::Row(res),
            ]),
        ]),
    ];
    if sim.player.at == Some(pid) {
        out.push(UiNode::colored("You are here", INFO));
        if p.surface.is_some() {
            out.push(UiNode::Button(
                "View surface".into(),
                UiAction::ViewPlanet(pid),
                Btn::Primary,
            ));
        }
        return out;
    }
    match (sim.player.at, journey_ship(sim, ctx.focus.0)) {
        (Some(at), Some(ship)) => match ship.plan_trip(&sim.universe, at, pid) {
            Ok(plan) => {
                out.push(UiNode::dim(format!(
                    "Aboard {}: {} travel, {} {}",
                    ship.name,
                    fmt_ticks(plan.ticks),
                    plan.fuel.1,
                    plan.fuel.0.name()
                )));
                let mut cmds = Vec::new();
                if sim.player.riding != Some(ship.id) {
                    cmds.push(Command::Ride {
                        ship: Some(ship.id),
                    });
                }
                cmds.push(Command::Launch {
                    ship: ship.id,
                    to: pid,
                });
                out.push(UiNode::Button(
                    "Plan Journey".into(),
                    UiAction::Cmds(cmds),
                    Btn::Primary,
                ));
            }
            Err(e) => {
                out.push(UiNode::colored(format!("{}: {e}", ship.name), WARN));
                out.push(UiNode::Button(
                    "Plan Journey".into(),
                    UiAction::Cmds(vec![]),
                    Btn::Disabled,
                ));
            }
        },
        (None, _) => out.push(UiNode::dim("You are travelling.")),
        (Some(_), None) => {
            out.push(UiNode::dim("No ship docked at your location."));
            out.push(UiNode::Button(
                "Plan Journey".into(),
                UiAction::Cmds(vec![]),
                Btn::Disabled,
            ));
        }
    }
    out
}

fn map_nav(ctx: &PanelCtx) -> Vec<UiNode> {
    let sim = &ctx.sim.0;
    let View::Map(level) = *ctx.view else {
        return vec![];
    };
    let g = match level {
        MapLevel::Galaxy(g) => g,
        MapLevel::System(s) => s.galaxy,
        MapLevel::Universe => sim.player.at.map(|p| p.galaxy).unwrap_or(0),
    };
    let systems = &sim.universe.galaxies[g as usize].systems;
    let known = systems
        .iter()
        .filter(|s| s.knowledge != SystemKnowledge::Unknown)
        .count();
    let planets: Vec<&Planet> = systems.iter().flat_map(|s| s.planets.iter()).collect();
    let surveyed = planets.iter().filter(|p| p.surveyed()).count();
    let colonies = sim.colonies().count();
    let scouts = sim.ships.values().filter(|s| s.hull == Hull::Scout).count();
    let home_sys = sim
        .player
        .at
        .map(|p| p.system())
        .unwrap_or(sim::HOME.system());
    let nav = |i: Icon, label: &str, right: String, a: UiAction, active: bool| {
        UiNode::ListBtn(
            i,
            ACCENT,
            label.into(),
            right,
            a,
            if active { Btn::Active } else { Btn::Flat },
        )
    };
    let legend =
        |c: Color, label: &str| UiNode::Row(vec![UiNode::Dot(c, 11.0), UiNode::text(label)]);
    let zoom_frac = {
        let (lo, hi) = ctx.rig.dist_range;
        1.0 - ((ctx.rig.goal_dist - lo) / (hi - lo)).clamp(0.0, 1.0)
    };
    vec![
        nav(
            Icon::Galaxy,
            "Galaxy",
            String::new(),
            UiAction::ViewMap(MapLevel::Galaxy(g)),
            matches!(level, MapLevel::Galaxy(_)),
        ),
        nav(
            Icon::Star,
            "Star Systems",
            format!("{known} / {}", systems.len()),
            UiAction::ViewMap(MapLevel::System(home_sys)),
            matches!(level, MapLevel::System(_)),
        ),
        nav(
            Icon::Planet,
            "Planets",
            format!("{surveyed} / {}", planets.len()),
            UiAction::OpenWindow(Popup::Colonies),
            false,
        ),
        nav(
            Icon::Colony,
            "Colonies",
            format!("{colonies}"),
            UiAction::OpenWindow(Popup::Colonies),
            false,
        ),
        nav(
            Icon::Rocket,
            "Scouts",
            format!("{scouts} / {}", sim.ships.len()),
            UiAction::OpenWindow(Popup::Ships),
            false,
        ),
        nav(
            Icon::Swirl,
            "Intergalactic",
            format!("{}", sim.universe.galaxies.len()),
            UiAction::ViewMap(MapLevel::Universe),
            level == MapLevel::Universe,
        ),
        UiNode::Divider,
        legend(Color::srgb(0.3, 0.85, 1.0), "Your location"),
        legend(Color::srgb(0.5, 0.75, 1.0), "Known (visited) system"),
        legend(Color::srgb(0.55, 0.58, 0.65), "Scanned system"),
        legend(Color::srgb(0.3, 0.32, 0.38), "Unscanned system"),
        legend(Color::srgb(0.35, 1.0, 0.5), "Colonized planet"),
        legend(Color::srgb(1.0, 0.75, 0.25), "Outpost (inactive colony)"),
        UiNode::Row(vec![
            UiNode::Icon(Icon::Warning, BAD, 12.0),
            UiNode::text("Hostile life"),
        ]),
        UiNode::Row(vec![
            UiNode::colored("- - -", YELLOW),
            UiNode::text("Trade route"),
        ]),
        UiNode::Row(vec![
            UiNode::colored("------", ACCENT),
            UiNode::text("Travel route"),
        ]),
        UiNode::Divider,
        UiNode::dim("Zoom"),
        UiNode::Row(vec![
            UiNode::IconBtn(Icon::Minus, UiAction::Zoom(1.25), Btn::Flat, 12.0),
            UiNode::Fixed(
                150.0,
                vec![UiNode::Bar((zoom_frac * 20.0).round() / 20.0, ACCENT)],
            ),
            UiNode::IconBtn(Icon::Plus, UiAction::Zoom(0.8), Btn::Flat, 12.0),
        ]),
    ]
}

fn resources_window(ctx: &PanelCtx) -> Vec<UiNode> {
    let sim = &ctx.sim.0;
    let scope = scope_of(&ctx.view, sim);
    let (items, _) = snapshot(sim, scope);
    let title = match scope {
        crate::hud::Scope::Colony(p) => format!("Resources - {}", planet_name(sim, p)),
        crate::hud::Scope::Civ => "Resources - all colonies".into(),
    };
    let mut out = vec![window_header(&title, Popup::Resources)];
    if items.is_empty() {
        out.push(UiNode::dim(
            "Nothing stored yet. Belts into a Depot fill the inventory.",
        ));
    }
    let mut cols: Vec<Vec<UiNode>> = vec![vec![], vec![], vec![]];
    let mut list: Vec<(Item, u32)> = items.into_iter().collect();
    list.sort_by_key(|(i, _)| (i.tier(), *i));
    for (k, (i, n)) in list.into_iter().enumerate() {
        let (r, c) = rate_text(ctx.rates.item_per_min(i));
        cols[k % 3].push(UiNode::Spread(vec![
            UiNode::Row(vec![
                UiNode::Icon(item_icon(i), item_color(i), 20.0),
                UiNode::text(i.name()),
            ]),
            UiNode::Row(vec![
                UiNode::sized(fmt_num(n as f64), Color::WHITE, 13.0),
                UiNode::sized(r, c, 11.0),
            ]),
        ]));
    }
    out.push(UiNode::Columns(cols));
    out
}

fn window_header(title: &str, p: Popup) -> UiNode {
    UiNode::Spread(vec![
        UiNode::sized(title, Color::srgb(0.55, 0.85, 1.0), 22.0),
        UiNode::IconBtn(Icon::Close, UiAction::OpenWindow(p), Btn::Flat, 14.0),
    ])
}

const TIER_COLORS: [u32; 4] = [0x3ea0ff, 0x3fd07a, 0xa070ff, 0xff8a3a];

fn tech_tree_window(ctx: &PanelCtx) -> Vec<UiNode> {
    let r = &ctx.sim.0.research;
    let time = ctx.tech_tab.0;
    let tab = |label: &str, on: bool| {
        UiNode::Button(
            label.into(),
            UiAction::TechTab(on),
            if time == on { Btn::Active } else { Btn::Normal },
        )
    };
    let done = Tech::ALL.iter().filter(|t| r.has(**t)).count();
    let mut out = vec![
        UiNode::Row(vec![
            UiNode::Icon(Icon::Atom, ACCENT, 30.0),
            UiNode::Fixed(
                420.0,
                vec![
                    UiNode::sized("TECHNOLOGY", Color::srgb(0.6, 0.86, 1.0), 26.0),
                    UiNode::dim(format!(
                        "{done} of {} researched  -  {}/s research",
                        Tech::ALL.len(),
                        research_rate(&ctx.sim.0)
                    )),
                    UiNode::Row(vec![
                        legend(GOOD, "Done"),
                        legend(ACCENT, "Researching"),
                        legend(Color::WHITE, "Available"),
                        legend(Color::srgb(0.35, 0.4, 0.5), "Locked"),
                    ]),
                ],
            ),
            UiNode::Col(vec![current_research_card(ctx)]),
            UiNode::Spacer,
            tab("Technologies", false),
            tab("Time control", true),
            UiNode::IconBtn(
                Icon::Close,
                UiAction::OpenWindow(Popup::TechTree),
                Btn::Flat,
                14.0,
            ),
        ]),
        UiNode::Gap,
    ];
    if time {
        out.push(UiNode::Scroll {
            key: "techtree-time",
            height: Val::Vh(74.0),
            items: time_lanes(r),
        });
        out.push(UiNode::dim(
            "Mouse wheel over the tree scrolls. Each level multiplies game speed; Instant-Warp skips time until a vessel reaches its destination.",
        ));
    } else {
        out.push(UiNode::Scroll {
            key: "techtree-branches",
            height: Val::Vh(74.0),
            items: branch_lanes(r),
        });
        out.push(UiNode::dim(
            "Mouse wheel over the tree scrolls. Columns follow prerequisites: a technology sits one tier right of its deepest requirement, which may be in another branch.",
        ));
    }
    out
}

fn legend(c: Color, label: &str) -> UiNode {
    UiNode::Row(vec![UiNode::Dot(c, 11.0), UiNode::text(label)])
}

fn branch_style(b: Branch) -> (Color, Icon) {
    match b {
        Branch::Industrial => (rgb(0xff9a3c), Icon::Gear),
        Branch::Energy => (rgb(0xffd23f), Icon::Bolt),
        Branch::Rocketry => (rgb(0xff5d6c), Icon::Rocket),
        Branch::Automation => (rgb(0x3fd0e0), Icon::Arm),
        Branch::Planetary => (rgb(0x4fdc7a), Icon::Planet),
        Branch::Astrophysics => (rgb(0xb07cff), Icon::Star),
        Branch::Time => (rgb(0x3ea0ff), Icon::Clock),
    }
}

/// Longest prerequisite chain below a tech: its column in the tree.
fn tech_depth(t: Tech) -> usize {
    t.info()
        .prereqs
        .iter()
        .map(|p| tech_depth(*p) + 1)
        .max()
        .unwrap_or(0)
}

fn tech_node(r: &sim::Research, t: Tech, icon: Icon, accent: Color) -> UiNode {
    let i = t.info();
    let pct = r.progress_of(t) as f32 / i.cost.max(1) as f32;
    let (status, frac, style) = if r.has(t) {
        ("Researched".to_string(), None, Btn::Active)
    } else if r.current == Some(t) {
        (
            format!("Researching {:.0}%", pct * 100.0),
            Some(pct),
            Btn::Primary,
        )
    } else if r.available(t) {
        let s = if pct > 0.0 {
            format!("{} pts  ({:.0}% done)", fmt_num(i.cost as f64), pct * 100.0)
        } else {
            format!("{} pts", fmt_num(i.cost as f64))
        };
        (s, (pct > 0.0).then_some(pct), Btn::Normal)
    } else {
        let missing: Vec<&str> = i
            .prereqs
            .iter()
            .filter(|p| !r.has(**p))
            .map(|p| p.info().name)
            .collect();
        let status = match missing.as_slice() {
            [one] => format!("Needs {one}"),
            [first, rest @ ..] => format!("Needs {first} +{}", rest.len()),
            [] => "Locked".into(),
        };
        (status, None, Btn::Disabled)
    };
    UiNode::TechNode {
        icon,
        title: i.name.to_string(),
        status,
        frac,
        accent,
        action: UiAction::Cmd(Command::SelectResearch(t)),
        style,
    }
}

fn lane_label(b: Branch, techs: &[Tech], r: &sim::Research, accent: Color, icon: Icon) -> UiNode {
    let have = techs.iter().filter(|t| r.has(**t)).count();
    UiNode::Fixed(
        140.0,
        vec![
            UiNode::Row(vec![
                UiNode::Icon(icon, accent, 18.0),
                UiNode::sized(b.name(), accent, 13.0),
            ]),
            UiNode::dim(format!("{have}/{} researched", techs.len())),
            UiNode::Bar(have as f32 / techs.len().max(1) as f32, accent),
        ],
    )
}

fn branch_lanes(r: &sim::Research) -> Vec<UiNode> {
    let cols = Tech::ALL
        .iter()
        .filter(|t| t.info().branch != Branch::Time)
        .map(|t| tech_depth(*t))
        .max()
        .unwrap_or(0)
        + 1;
    let mut out = vec![UiNode::Row(
        std::iter::once(UiNode::Fixed(140.0, vec![UiNode::dim("Branch")]))
            .chain(
                (0..cols)
                    .map(|c| UiNode::Fixed(174.0, vec![UiNode::dim(format!("Tier {}", c + 1))])),
            )
            .collect(),
    )];
    for b in Branch::ALL.into_iter().filter(|b| *b != Branch::Time) {
        let (accent, icon) = branch_style(b);
        let techs: Vec<Tech> = Tech::ALL
            .iter()
            .copied()
            .filter(|t| t.info().branch == b)
            .collect();
        let mut row = vec![lane_label(b, &techs, r, accent, icon)];
        for c in 0..cols {
            let cell: Vec<UiNode> = techs
                .iter()
                .filter(|t| tech_depth(**t) == c)
                .map(|t| tech_node(r, *t, icon, accent))
                .collect();
            row.push(UiNode::Fixed(174.0, cell));
        }
        out.push(UiNode::Card(vec![UiNode::Row(row)], accent));
    }
    out
}

fn time_lanes(r: &sim::Research) -> Vec<UiNode> {
    let tiers = [
        ("Tier 1 - Basic", "Speed up time in small steps."),
        ("Tier 2 - Advanced", "Much larger speed multipliers."),
        ("Tier 3 - High tech", "Extreme time acceleration."),
        (
            "Tier 4 - Ultimate",
            "Near-instant travel; skip to arrivals.",
        ),
    ];
    let mut out = vec![];
    for (k, (name, desc)) in tiers.iter().enumerate() {
        let accent = rgb(TIER_COLORS[k]);
        let techs: Vec<Tech> = Tech::ALL
            .iter()
            .copied()
            .filter(|t| t.time_tier() == Some(k as u8 + 1))
            .collect();
        let have = techs.iter().filter(|t| r.has(**t)).count();
        let mut row = vec![UiNode::Fixed(
            190.0,
            vec![
                UiNode::sized(*name, accent, 15.0),
                UiNode::dim(*desc),
                UiNode::Bar(have as f32 / techs.len().max(1) as f32, accent),
            ],
        )];
        for (n, t) in techs.iter().enumerate() {
            if n > 0 {
                row.push(UiNode::Icon(Icon::Chevrons, accent.with_alpha(0.6), 14.0));
            }
            let icon = match t.speed() {
                Some(s) if s <= 256 => Icon::Fast,
                Some(s) if s <= 4096 => Icon::Swirl,
                Some(_) => Icon::Infinity,
                None => Icon::Target,
            };
            row.push(tech_node(r, *t, icon, accent));
        }
        out.push(UiNode::Card(vec![UiNode::Row(row)], accent));
    }
    out
}

fn current_research_card(ctx: &PanelCtx) -> UiNode {
    let r = &ctx.sim.0.research;
    let rate = research_rate(&ctx.sim.0);
    match r.current {
        Some(t) => {
            let i = t.info();
            let (accent, icon) = branch_style(i.branch);
            let got = r.progress_of(t);
            let left = i.cost.saturating_sub(got);
            let eta = match left.checked_div(rate) {
                Some(s) => format!("about {}m {:02}s at {rate}/s", s / 60, s % 60),
                None => "no labs working".into(),
            };
            UiNode::Card(
                vec![UiNode::Row(vec![
                    UiNode::Icon(icon, accent, 26.0),
                    UiNode::Col(vec![
                        UiNode::Row(vec![
                            UiNode::dim("Researching"),
                            UiNode::sized(i.name, Color::WHITE, 15.0),
                        ]),
                        UiNode::dim(i.desc),
                    ]),
                    UiNode::Spacer,
                    UiNode::Fixed(
                        230.0,
                        vec![
                            UiNode::Bar(got as f32 / i.cost.max(1) as f32, ACCENT),
                            UiNode::dim(format!(
                                "{} / {} pts - {eta}",
                                fmt_num(got as f64),
                                fmt_num(i.cost as f64)
                            )),
                        ],
                    ),
                ])],
                accent,
            )
        }
        None => UiNode::Card(
            vec![UiNode::Row(vec![
                UiNode::Icon(Icon::Warning, WARN, 20.0),
                UiNode::colored(
                    "No project selected - click an available technology below.",
                    WARN,
                ),
            ])],
            WARN,
        ),
    }
}

fn cost_text(cost: &[(Item, u32)], have: Option<&sim::Inventory>) -> Vec<UiNode> {
    cost.iter()
        .map(|&(i, n)| {
            let ok = have.is_none_or(|inv| inv.get(i) >= n);
            let s = match have {
                Some(inv) => format!("{n} {} ({})", i.name(), inv.get(i)),
                None => format!("{n} {}", i.name()),
            };
            UiNode::colored(s, if ok { INFO } else { BAD })
        })
        .collect()
}

fn cost_string(cost: &[(Item, u32)]) -> String {
    cost.iter()
        .map(|(i, n)| format!("{n} {}", i.name()))
        .collect::<Vec<_>>()
        .join(", ")
}

fn research_rate(sim: &Sim) -> u32 {
    sim.colonies()
        .filter_map(|p| p.surface.as_ref())
        .flat_map(|s| s.buildings.values())
        .filter(|b| b.status == Status::Working)
        .map(|b| match b.kind {
            BuildingKind::Lab => 1,
            BuildingKind::LabMk2 => 4,
            _ => 0,
        })
        .sum()
}

fn log(ctx: &PanelCtx) -> Vec<UiNode> {
    ctx.log
        .entries
        .iter()
        .filter(|m| m.age < 9.0)
        .take(4)
        .map(|m| {
            let c = match m.kind {
                MsgKind::Info => INFO,
                MsgKind::Good => GOOD,
                MsgKind::Warn => WARN,
                MsgKind::Error => BAD,
            };
            UiNode::Card(
                vec![UiNode::Row(vec![
                    UiNode::Dot(c, 8.0),
                    UiNode::colored(m.text.clone(), Color::WHITE),
                ])],
                c,
            )
        })
        .collect()
}

fn overlay(ctx: &PanelCtx) -> Vec<UiNode> {
    let mut out: Vec<UiNode> = ctx
        .overlay
        .lines
        .iter()
        .enumerate()
        .map(|(i, l)| {
            if i % 3 == 0 {
                UiNode::colored(l.clone(), Color::srgb(0.95, 0.8, 1.0))
            } else {
                UiNode::Text(l.clone(), Color::srgb(0.75, 0.7, 0.9), 12.0)
            }
        })
        .collect();
    if let Some(t) = ctx.sim.0.victory
        && ctx.sim.0.tick() < t + 20 * 30
    {
        out.insert(0, UiNode::title("THE FRONTIER BEACON IS LIT - VICTORY"));
    }
    out
}

fn info(ctx: &PanelCtx) -> Vec<UiNode> {
    let sim = &ctx.sim.0;
    let planet_view = matches!(*ctx.view, View::Planet(_));
    match ctx.selected.0 {
        Selection::Building(p, id) => match sim.surface(p).and_then(|s| s.buildings.get(&id)) {
            Some(b) => building_info(ctx, p, b),
            None => vec![],
        },
        Selection::Planet(p) => planet_info(ctx, p),
        Selection::System(s) => system_card(ctx, s),
        Selection::Ship(id) => match sim.ships.get(&id) {
            Some(s) => ship_info(ctx, s),
            None => vec![],
        },
        Selection::None if planet_view => vec![],
        Selection::None => match *ctx.view {
            View::Map(MapLevel::System(s)) => system_card(ctx, s),
            View::Map(MapLevel::Galaxy(_)) => match sim.player.at {
                Some(p) => system_card(ctx, p.system()),
                None => vec![],
            },
            _ => vec![],
        },
    }
}

fn status_color(s: Status) -> Color {
    match s {
        Status::Working => GOOD,
        Status::Idle => Color::srgb(0.7, 0.75, 0.8),
        Status::NoPower | Status::Damaged | Status::NoWorkers | Status::Inactive => BAD,
        _ => WARN,
    }
}

fn inv_rows(inv: &sim::Inventory) -> Vec<UiNode> {
    if inv.is_empty() {
        return vec![UiNode::dim("  (empty)")];
    }
    vec![UiNode::Wrap(
        inv.iter()
            .map(|(i, n)| {
                UiNode::colored(format!("{} x{}", i.name(), n), rgb(i.rgb()).lighter(0.15))
            })
            .collect(),
    )]
}

fn building_info(ctx: &PanelCtx, p: PlanetId, b: &Building) -> Vec<UiNode> {
    let sim = &ctx.sim.0;
    let spec = b.kind.spec();
    let inv = sim.colony(p).map(|c| &c.inventory);
    let mut out = vec![
        UiNode::Spread(vec![
            UiNode::sized("Selected Building", Color::WHITE, 15.0),
            UiNode::IconBtn(
                Icon::Close,
                UiAction::Select(Selection::None),
                Btn::Flat,
                12.0,
            ),
        ]),
        UiNode::Row(vec![
            UiNode::IconBtn(
                Cat::of(b.kind).icon(),
                UiAction::Select(Selection::Building(p, b.id)),
                Btn::Primary,
                30.0,
            ),
            UiNode::Col(vec![
                UiNode::sized(spec.name, Color::WHITE, 17.0),
                UiNode::Row(vec![
                    UiNode::Dot(status_color(b.status), 9.0),
                    UiNode::colored(b.status.label(), status_color(b.status)),
                ]),
            ]),
        ]),
    ];
    if let Some(rec) = b.recipe
        && b.kind.crafting().is_some()
    {
        let i = rec.info();
        let secs = i.ticks as f32 * 100.0 / b.speed() as f32 / sim::TICKS_PER_SEC as f32;
        let frac = (b.progress as f32 / (i.ticks as f32 * 100.0)).min(1.0);
        out.push(UiNode::Row(vec![
            UiNode::dim("Recipe:"),
            UiNode::colored(i.name, ACCENT),
        ]));
        out.push(UiNode::Bar(frac, ACCENT));
        out.push(UiNode::Spread(vec![
            UiNode::dim(format!("{secs:.1}s per cycle")),
            UiNode::dim(format!("{}%", (frac * 100.0) as u32)),
        ]));
        let per_min = |n: u32| format!("{:.1} /min", n as f32 * 60.0 / secs);
        let rows = |list: &[(Item, u32)]| {
            UiNode::Wrap(
                list.iter()
                    .map(|&(it, n)| {
                        UiNode::Row(vec![
                            UiNode::Icon(item_icon(it), item_color(it), 18.0),
                            UiNode::text(per_min(n)),
                        ])
                    })
                    .collect(),
            )
        };
        out.push(UiNode::dim("Input"));
        out.push(rows(i.inputs));
        out.push(UiNode::dim("Output"));
        out.push(rows(i.outputs));
        out.push(UiNode::Divider);
    }
    if spec.power > 0 {
        out.push(UiNode::Spread(vec![
            UiNode::Row(vec![
                UiNode::Icon(Icon::Bolt, YELLOW, 16.0),
                UiNode::text("Power"),
            ]),
            UiNode::colored(
                fmt_kw(b.power_draw() as i64),
                if b.powered { TEXT } else { BAD },
            ),
        ]));
        out.push(UiNode::Spread(vec![
            UiNode::Row(vec![
                UiNode::Icon(Icon::Gear, INFO, 16.0),
                UiNode::text("Efficiency"),
            ]),
            UiNode::text(format!("{}%", b.speed())),
        ]));
    }
    if spec.power_gen > 0 {
        out.push(UiNode::text(format!(
            "Generates up to {}",
            fmt_kw(spec.power_gen as i64)
        )));
    }
    if spec.workers > 0 {
        out.push(UiNode::text(format!(
            "Workers: {} ({})",
            spec.workers,
            if b.staffed { "staffed" } else { "unstaffed" }
        )));
    }
    let r = &sim.research;
    if let Some((cat, _)) = b.kind.crafting() {
        if b.recipe.is_none() {
            out.push(UiNode::colored("No recipe selected", WARN));
        }
        let recipes: Vec<UiNode> = Recipe::ALL
            .iter()
            .copied()
            .filter(|rc| rc.info().category == cat && rc.info().tech.is_none_or(|t| r.has(t)))
            .map(|rc| {
                UiNode::Button(
                    rc.info().name.to_string(),
                    UiAction::Cmd(Command::SetRecipe {
                        planet: p,
                        building: b.id,
                        recipe: Some(rc),
                    }),
                    if b.recipe == Some(rc) {
                        Btn::Active
                    } else {
                        Btn::Normal
                    },
                )
            })
            .collect();
        out.push(UiNode::dim("Choose recipe:"));
        out.push(UiNode::Wrap(recipes));
        out.push(UiNode::dim("Buffers (in / out):"));
        out.extend(inv_rows(&b.input));
        out.extend(inv_rows(&b.output));
    }
    if b.kind.is_miner() {
        match b.filter {
            Some(item) => {
                let s = sim.surface(p).expect("surface");
                let per_min =
                    b.speed() as u64 * s.richness_of(item) as u64 * 60 * sim::TICKS_PER_SEC / 6000;
                out.push(UiNode::text(format!(
                    "Extracting {} - {} per minute (deposits never deplete)",
                    item.name(),
                    per_min
                )));
            }
            None => out.push(UiNode::colored("No deposit under this miner", BAD)),
        }
        out.extend(inv_rows(&b.output));
    }
    if b.kind.takes_modules() {
        out.push(UiNode::text(format!(
            "Modules ({}/2): {}",
            b.modules.len(),
            b.modules
                .iter()
                .map(|m| m.name())
                .collect::<Vec<_>>()
                .join(", ")
        )));
        if r.has(Tech::Modules) && b.modules.len() < 2 {
            let mut row = Vec::new();
            for m in [Item::SpeedModule, Item::EfficiencyModule] {
                let have = inv.map(|i| i.get(m)).unwrap_or(0);
                row.push(UiNode::btn_if(
                    format!("Insert {} ({have})", m.name()),
                    UiAction::Cmd(Command::InsertModule {
                        planet: p,
                        building: b.id,
                        module: m,
                    }),
                    have > 0,
                ));
            }
            out.push(UiNode::Wrap(row));
        }
    }
    if b.kind.has_rate_limits() {
        out.extend(rate_controls(p, b));
    }
    match b.kind {
        BuildingKind::Provider | BuildingKind::Splitter => {
            let what = if b.kind == BuildingKind::Splitter {
                "Straight ahead"
            } else {
                "Pushing"
            };
            out.push(UiNode::text(format!(
                "{what}: {}",
                b.filter
                    .map(|i| i.name())
                    .unwrap_or(if b.kind == BuildingKind::Splitter {
                        "everything (no filter)"
                    } else {
                        "nothing"
                    })
            )));
            if let Some(inv) = inv {
                let mut items: Vec<(Item, u32)> = inv.iter().collect();
                items.sort_by_key(|(i, n)| (std::cmp::Reverse(*n), *i));
                let mut row: Vec<UiNode> = items
                    .into_iter()
                    .take(30)
                    .map(|(i, _)| {
                        UiNode::Button(
                            i.name().to_string(),
                            UiAction::Cmd(Command::SetFilter {
                                planet: p,
                                building: b.id,
                                item: Some(i),
                            }),
                            if b.filter == Some(i) {
                                Btn::Active
                            } else {
                                Btn::Normal
                            },
                        )
                    })
                    .collect();
                row.push(UiNode::btn(
                    "None",
                    UiAction::Cmd(Command::SetFilter {
                        planet: p,
                        building: b.id,
                        item: None,
                    }),
                ));
                out.push(UiNode::Wrap(row));
            }
        }
        BuildingKind::Generator | BuildingKind::Reactor => {
            out.push(UiNode::text(format!(
                "Fuel buffer: {}  burn left: {}",
                b.input.total(),
                fmt_ticks(b.timer as u64)
            )));
            out.push(UiNode::dim(if b.kind == BuildingKind::Generator {
                "Feed carbon or fuel with an inserter or provider."
            } else {
                "Feed fuel cells with an inserter or provider."
            }));
        }
        BuildingKind::Shipyard => {
            match b.order {
                Some(o) => out.push(UiNode::text(format!(
                    "Building {} ({} drive): {}%",
                    o.hull.name(),
                    o.drive.name(),
                    b.progress * 100 / o.hull.build_ticks()
                ))),
                None => out.push(UiNode::dim("Idle. Order a ship:")),
            }
            if b.order.is_none() {
                for hull in Hull::ALL {
                    if !r.has(hull.tech()) {
                        continue;
                    }
                    for drive in Drive::ALL {
                        if !drive.tech().is_none_or(|t| r.has(t)) {
                            continue;
                        }
                        let order = ShipOrder { hull, drive };
                        let cost: Vec<(Item, u32)> = order.cost().iter().collect();
                        let ok = inv.is_some_and(|i| i.has_all(&cost));
                        out.push(UiNode::btn_if(
                            format!("{} / {} drive", hull.name(), drive.name()),
                            UiAction::Cmd(Command::OrderShip {
                                planet: p,
                                building: b.id,
                                order,
                            }),
                            ok,
                        ));
                        out.push(UiNode::dim(format!(
                            "  {} | cargo {} | seats {} | crew {}",
                            cost_string(&cost),
                            hull.cargo(),
                            hull.seats(),
                            hull.crew()
                        )));
                    }
                }
            }
        }
        BuildingKind::Observatory => {
            out.push(UiNode::dim(
                "Scans every 30 s. Better scanning research widens range and detail.",
            ));
        }
        BuildingKind::Lab | BuildingKind::LabMk2 => {
            out.push(UiNode::dim(
                "Research points enter the civilization-wide pool.",
            ));
        }
        BuildingKind::Beacon => {
            out.push(UiNode::text(format!(
                "Charge: {}/1200 ticks",
                b.timer.min(1200)
            )));
        }
        BuildingKind::House | BuildingKind::Habitat => {
            out.push(UiNode::dim(format!("Houses {} people.", b.kind.housing())));
        }
        _ => {}
    }
    out.push(UiNode::dim(spec.desc));
    out.push(UiNode::Gap);
    out.push(UiNode::Button(
        "Demolish (refund)".into(),
        UiAction::Cmd(Command::Remove {
            planet: p,
            x: b.x,
            y: b.y,
        }),
        Btn::Danger,
    ));
    out
}

fn planet_status(p: &Planet) -> (&'static str, Color) {
    match &p.colony {
        Some(c) if c.active => ("Operational colony", GOOD),
        Some(_) => ("Colonized (work systems inactive)", WARN),
        None if p.surveyed() => ("Surveyed", INFO),
        None if p.survey > 0 => ("Survey in progress", INFO),
        None if p.scan_level > 0 => ("Scanned", Color::srgb(0.7, 0.75, 0.8)),
        None => ("Unknown", Color::srgb(0.5, 0.5, 0.55)),
    }
}

fn planet_info(ctx: &PanelCtx, pid: PlanetId) -> Vec<UiNode> {
    let sim = &ctx.sim.0;
    let Some(p) = sim.planet(pid) else {
        return vec![];
    };
    let (status, color) = planet_status(p);
    let size = ["Small", "Medium", "Large"][p.size as usize];
    let mut out = vec![
        UiNode::title(p.name.clone()),
        UiNode::colored(status, color),
        UiNode::text(format!(
            "{} {} world | orbit {}",
            size,
            if p.category_known() {
                p.category.name()
            } else {
                "uncharacterized"
            },
            p.orbit + 1
        )),
    ];
    if !p.surveyed() {
        out.push(UiNode::dim(format!(
            "Survey {}% | scan detail level {}",
            p.survey, p.scan_level
        )));
    }
    let res = p.known_resources();
    if res.is_empty() {
        out.push(UiNode::dim("Resources: unknown - survey with a scout"));
    } else {
        let exact = res.iter().all(|r| r.1);
        out.push(UiNode::dim(if exact {
            "Resources (surveyed):"
        } else {
            "Resource indicators (uncertain):"
        }));
        out.push(UiNode::Wrap(
            res.iter()
                .map(|(i, _)| UiNode::colored(i.name(), rgb(i.rgb()).lighter(0.2)))
                .collect(),
        ));
    }
    if p.hostile_known() {
        if p.hostile {
            let nests = p.surface.as_ref().map(|s| s.nests.len()).unwrap_or(0);
            out.push(UiNode::colored(
                if p.surface.is_some() {
                    format!("Hostile life: {nests} nest(s)")
                } else {
                    "Hostile life detected".into()
                },
                BAD,
            ));
        } else {
            out.push(UiNode::dim("No hostile life"));
        }
    }
    let here = sim.player.at == Some(pid);
    if let Some(c) = &p.colony {
        let s = p.surface.as_ref().expect("colony surface");
        out.push(UiNode::text(format!(
            "Population {} / housing {} | founded {} ago",
            c.population,
            sim::Colony::housing(s),
            fmt_ticks(sim.tick().saturating_sub(c.founded))
        )));
        out.push(UiNode::text(format!(
            "Power {} / {} demand | research generated {}",
            fmt_kw(c.power.produced as i64),
            fmt_kw(c.power.demand as i64),
            c.research_total
        )));
        if !c.active {
            out.push(UiNode::btn_if(
                if here {
                    "Establish work systems".to_string()
                } else {
                    "Establish work systems (visit in person first)".to_string()
                },
                UiAction::Cmd(Command::ActivateColony { planet: pid }),
                here,
            ));
        }
        if !s.nests.is_empty() {
            out.push(UiNode::Row(vec![
                UiNode::dim("Hostile policy:"),
                UiNode::Button(
                    "Eliminate".into(),
                    UiAction::Cmd(Command::SetPolicy {
                        planet: pid,
                        policy: Policy::Eliminate,
                    }),
                    if c.policy == Policy::Eliminate {
                        Btn::Active
                    } else {
                        Btn::Normal
                    },
                ),
                UiNode::Button(
                    "Coexist".into(),
                    UiAction::Cmd(Command::SetPolicy {
                        planet: pid,
                        policy: Policy::Coexist,
                    }),
                    if c.policy == Policy::Coexist {
                        Btn::Active
                    } else {
                        Btn::Normal
                    },
                ),
            ]));
            out.push(UiNode::dim(
                "Eliminate: turrets attack nests, nests raid widely. Coexist: rarer raids near nests only.",
            ));
        }
    }
    if p.surface.is_some() && *ctx.view != View::Planet(pid) {
        out.push(UiNode::btn("View surface", UiAction::ViewPlanet(pid)));
    }
    let docked: Vec<&Ship> = sim
        .ships
        .values()
        .filter(|s| s.docked_at() == Some(pid))
        .collect();
    if !docked.is_empty() {
        out.push(UiNode::dim("Ships here:"));
        out.push(UiNode::Wrap(
            docked
                .iter()
                .map(|s| UiNode::btn(s.name.clone(), UiAction::Select(Selection::Ship(s.id))))
                .collect(),
        ));
    }
    if let Some(sid) = ctx.focus.0
        && let Some(ship) = sim.ships.get(&sid)
        && let Some(from) = ship.docked_at()
        && from != pid
    {
        out.push(UiNode::Gap);
        match ship.plan_trip(&sim.universe, from, pid) {
            Ok(plan) => {
                out.push(UiNode::btn(
                    format!(
                        "Send {} here ({}, {} {})",
                        ship.name,
                        fmt_ticks(plan.ticks),
                        plan.fuel.1,
                        plan.fuel.0.name()
                    ),
                    UiAction::Cmd(Command::Launch { ship: sid, to: pid }),
                ));
                if sim.research.has(Tech::LogisticsAutomation) {
                    let template: Vec<(Item, u32)> = ship.cargo.iter().collect();
                    out.push(UiNode::btn(
                        format!("Automate route: {} <-> here", planet_name(sim, from)),
                        UiAction::Cmd(Command::SetRoute {
                            ship: sid,
                            route: Some(Route {
                                a: from,
                                b: pid,
                                load_a: template,
                                load_b: vec![],
                            }),
                        }),
                    ));
                    out.push(UiNode::dim(
                        "  (the ship's current cargo becomes the per-trip load)",
                    ));
                }
            }
            Err(e) => out.push(UiNode::colored(format!("{}: {e}", ship.name), WARN)),
        }
    }
    out
}

fn ship_info(ctx: &PanelCtx, s: &Ship) -> Vec<UiNode> {
    let sim = &ctx.sim.0;
    let mut out = vec![
        UiNode::title(s.name.clone()),
        UiNode::text(format!(
            "{} hull | {} drive | crew {}",
            s.hull.name(),
            s.drive.name(),
            s.crew
        )),
    ];
    let riding = sim.player.riding == Some(s.id);
    match s.loc {
        ShipLoc::Docked(p) => {
            out.push(UiNode::colored(
                format!("Docked at {}", planet_name(sim, p)),
                INFO,
            ));
        }
        ShipLoc::Transit {
            from, to, arrive, ..
        } => {
            out.push(UiNode::colored(
                format!(
                    "In transit {} -> {} | arrives in {}",
                    planet_name(sim, from),
                    planet_name(sim, to),
                    fmt_ticks(arrive.saturating_sub(sim.tick()))
                ),
                INFO,
            ));
            out.push(UiNode::btn_if(
                "Instant-Warp to arrival",
                UiAction::Warp(s.id),
                sim.research.has(Tech::InstantWarp),
            ));
        }
    }
    out.push(UiNode::text(format!(
        "Cargo {}/{} | passengers {}/{}",
        s.cargo.total(),
        s.hull.cargo(),
        s.passengers,
        s.hull.seats()
    )));
    let docked = s.docked_at();
    for (item, n) in s.cargo.iter() {
        let mut row = vec![UiNode::colored(
            format!("{} x{n}", item.name()),
            rgb(item.rgb()).lighter(0.15),
        )];
        if docked.is_some() {
            row.push(UiNode::btn(
                "-10",
                UiAction::Cmd(Command::Unload {
                    ship: s.id,
                    item,
                    qty: 10,
                }),
            ));
            row.push(UiNode::btn(
                "-all",
                UiAction::Cmd(Command::Unload {
                    ship: s.id,
                    item,
                    qty: n,
                }),
            ));
        }
        out.push(UiNode::Row(row));
    }
    if let Some(r) = &s.route {
        out.push(UiNode::colored(
            format!(
                "Automated route: {} <-> {} carrying {}",
                planet_name(sim, r.a),
                planet_name(sim, r.b),
                cost_string(&r.load_a)
            ),
            GOOD,
        ));
        out.push(UiNode::btn(
            "Stop route",
            UiAction::Cmd(Command::SetRoute {
                ship: s.id,
                route: None,
            }),
        ));
    }
    let Some(at) = docked else {
        if riding {
            out.push(UiNode::colored("You are aboard.", INFO));
        }
        return out;
    };
    let mut actions = Vec::new();
    if s.hull == Hull::Scout {
        actions.push(UiNode::Button(
            format!("Survey: {}", if s.surveying { "ON" } else { "OFF" }),
            UiAction::Cmd(Command::SetSurvey {
                ship: s.id,
                on: !s.surveying,
            }),
            if s.surveying {
                Btn::Active
            } else {
                Btn::Normal
            },
        ));
    }
    if s.cargo.get(Item::ColonyKit) > 0 && sim.colony(at).is_none() {
        actions.push(UiNode::btn(
            "Found colony here",
            UiAction::Cmd(Command::FoundColony { ship: s.id }),
        ));
    }
    if sim.player.at == Some(at) {
        actions.push(if riding {
            UiNode::btn(
                "Disembark (you)",
                UiAction::Cmd(Command::Ride { ship: None }),
            )
        } else {
            UiNode::btn(
                "Board (you ride along)",
                UiAction::Cmd(Command::Ride { ship: Some(s.id) }),
            )
        });
    }
    if !actions.is_empty() {
        out.push(UiNode::Wrap(actions));
    }
    if let Some(c) = sim.colony(at) {
        out.push(UiNode::Wrap(vec![
            UiNode::btn(
                "Board 1 person",
                UiAction::Cmd(Command::Board {
                    ship: s.id,
                    people: 1,
                }),
            ),
            UiNode::btn(
                "Board 5",
                UiAction::Cmd(Command::Board {
                    ship: s.id,
                    people: 5,
                }),
            ),
            UiNode::btn_if(
                "Disembark all",
                UiAction::Cmd(Command::Disembark {
                    ship: s.id,
                    people: s.passengers,
                }),
                s.passengers > 0,
            ),
        ]));
        out.push(UiNode::dim(format!(
            "Load from {} (have):",
            planet_name(sim, at)
        )));
        let mut items: Vec<(Item, u32)> = c.inventory.iter().collect();
        let pinned = [Item::Fuel, Item::ColonyKit, Item::ExoticMatter];
        items.sort_by_key(|(i, n)| (!pinned.contains(i), std::cmp::Reverse(*n), *i));
        for (item, n) in items.into_iter().take(14) {
            out.push(UiNode::Row(vec![
                UiNode::colored(
                    format!("{} ({n})", item.name()),
                    rgb(item.rgb()).lighter(0.15),
                ),
                UiNode::btn(
                    "+10",
                    UiAction::Cmd(Command::Load {
                        ship: s.id,
                        item,
                        qty: 10,
                    }),
                ),
                UiNode::btn(
                    "+50",
                    UiAction::Cmd(Command::Load {
                        ship: s.id,
                        item,
                        qty: 50,
                    }),
                ),
                UiNode::btn(
                    "+all",
                    UiAction::Cmd(Command::Load {
                        ship: s.id,
                        item,
                        qty: n,
                    }),
                ),
            ]));
        }
    }
    out.push(UiNode::dim("Launch to (in this system):"));
    let mut dests = Vec::new();
    if let Some(sys) = sim.universe.system(at.system()) {
        for p in &sys.planets {
            if p.id == at {
                continue;
            }
            if let Ok(plan) = s.plan_trip(&sim.universe, at, p.id) {
                dests.push(UiNode::btn(
                    format!(
                        "{} ({}, {} fuel)",
                        p.name,
                        fmt_ticks(plan.ticks),
                        plan.fuel.1
                    ),
                    UiAction::Cmd(Command::Launch {
                        ship: s.id,
                        to: p.id,
                    }),
                ));
            }
        }
    }
    out.push(UiNode::Wrap(dests));
    out.push(UiNode::dim(
        "Other destinations: select a planet on the star map.",
    ));
    out.push(UiNode::btn(
        "Open star map",
        UiAction::ViewMap(MapLevel::Galaxy(at.galaxy)),
    ));
    out
}

fn window(ctx: &PanelCtx) -> Vec<UiNode> {
    match ctx.window.0 {
        Popup::None => vec![],
        Popup::Research => research_window(ctx),
        Popup::TechTree => tech_tree_window(ctx),
        Popup::Ships => ships_window(ctx),
        Popup::Logistics => logistics_window(ctx),
        Popup::Colonies => colonies_window(ctx),
        Popup::Resources => resources_window(ctx),
        Popup::Codex => codex_window(ctx),
        Popup::Help => help_window(ctx),
    }
}

fn research_window(ctx: &PanelCtx) -> Vec<UiNode> {
    let sim = &ctx.sim.0;
    let r = &sim.research;
    let mut out = vec![
        window_header("Research", Popup::Research),
        UiNode::Row(vec![
            UiNode::text(format!(
                "+{} points/s | pool {} | lifetime {}",
                research_rate(sim),
                r.pool,
                r.generated
            )),
            UiNode::Spacer,
            UiNode::btn(
                "Time control tree [U]",
                UiAction::OpenWindow(Popup::TechTree),
            ),
        ]),
    ];
    match r.current {
        Some(t) => {
            let i = t.info();
            out.push(UiNode::colored(
                format!("Current: {} - {}/{} - {}", i.name, r.progress_of(t), i.cost, i.desc),
                INFO,
            ));
        }
        None => out.push(UiNode::colored(
            "Pick a project. Switching keeps accumulated progress. Labs need staff (people) and power.",
            WARN,
        )),
    }
    let cols = Branch::ALL
        .iter()
        .filter(|b| **b != Branch::Time)
        .map(|br| {
            let mut col = vec![UiNode::head(br.name())];
            for t in Tech::ALL.iter().copied().filter(|t| t.info().branch == *br) {
                let i = t.info();
                if r.has(t) {
                    col.push(UiNode::Text(format!("[x] {}", i.name), GOOD, 12.0));
                    continue;
                }
                let prog = r.progress_of(t);
                let label = if prog > 0 {
                    format!("{} {}%", i.name, prog * 100 / i.cost)
                } else {
                    format!("{} ({})", i.name, i.cost)
                };
                let style = if r.current == Some(t) {
                    Btn::Active
                } else if r.available(t) {
                    Btn::Normal
                } else {
                    Btn::Disabled
                };
                col.push(UiNode::Button(
                    label,
                    UiAction::Cmd(Command::SelectResearch(t)),
                    style,
                ));
                if style == Btn::Disabled {
                    let missing: Vec<&str> = i
                        .prereqs
                        .iter()
                        .filter(|p| !r.has(**p))
                        .map(|p| p.info().name)
                        .collect();
                    col.push(UiNode::Text(
                        format!("  needs {}", missing.join(", ")),
                        Color::srgb(0.45, 0.47, 0.52),
                        10.5,
                    ));
                }
            }
            col
        })
        .collect();
    out.push(UiNode::Columns(cols));
    out
}

fn ships_window(ctx: &PanelCtx) -> Vec<UiNode> {
    let sim = &ctx.sim.0;
    let mut out = vec![window_header("Ships", Popup::Ships)];
    if sim.ships.is_empty() {
        out.push(UiNode::dim(
            "No ships yet. Research Rocketry, build a Shipyard, and order a Scout.",
        ));
    }
    for s in sim.ships.values() {
        let status = match s.loc {
            ShipLoc::Docked(p) => format!("docked at {}", planet_name(sim, p)),
            ShipLoc::Transit {
                from, to, arrive, ..
            } => format!(
                "{} -> {}, {} left",
                planet_name(sim, from),
                planet_name(sim, to),
                fmt_ticks(arrive.saturating_sub(sim.tick()))
            ),
        };
        let cargo = if s.cargo.is_empty() {
            "empty".to_string()
        } else {
            s.cargo
                .iter()
                .map(|(i, n)| format!("{n} {}", i.name()))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mut row = vec![
            UiNode::btn(s.name.clone(), UiAction::Select(Selection::Ship(s.id))),
            UiNode::text(status),
        ];
        if s.route.is_some() {
            row.push(UiNode::colored("[auto]", GOOD));
        }
        if matches!(s.loc, ShipLoc::Transit { .. }) && sim.research.has(Tech::InstantWarp) {
            row.push(UiNode::btn("Warp", UiAction::Warp(s.id)));
        }
        out.push(UiNode::Row(row));
        out.push(UiNode::dim(format!(
            "    {} / {} drive | cargo: {} | passengers {}",
            s.hull.name(),
            s.drive.name(),
            cargo,
            s.passengers
        )));
    }
    out
}

fn logistics_window(ctx: &PanelCtx) -> Vec<UiNode> {
    let sim = &ctx.sim.0;
    let mut out = vec![window_header("Logistics", Popup::Logistics)];
    let routed: Vec<&Ship> = sim.ships.values().filter(|s| s.route.is_some()).collect();
    out.push(UiNode::head("Automated routes"));
    if routed.is_empty() {
        out.push(UiNode::dim(if sim.research.has(Tech::LogisticsAutomation) {
            "None yet. Select a docked freighter, load a cargo template, then pick a destination planet and choose Automate route."
        } else {
            "Research Logistics automation to run recurring freighter routes."
        }));
    }
    for s in routed {
        let r = s.route.as_ref().expect("route");
        out.push(UiNode::Row(vec![
            UiNode::btn(s.name.clone(), UiAction::Select(Selection::Ship(s.id))),
            UiNode::colored(
                format!("{} <-> {}", planet_name(sim, r.a), planet_name(sim, r.b)),
                YELLOW,
            ),
            UiNode::dim(cost_string(&r.load_a)),
        ]));
    }
    out.push(UiNode::Divider);
    out.push(UiNode::head("In transit"));
    let mut any = false;
    for s in sim.ships.values() {
        if let ShipLoc::Transit {
            from, to, arrive, ..
        } = s.loc
        {
            any = true;
            out.push(UiNode::Row(vec![
                UiNode::Icon(Icon::Rocket, ACCENT, 16.0),
                UiNode::text(format!(
                    "{}: {} -> {} ({} left, {} cargo)",
                    s.name,
                    planet_name(sim, from),
                    planet_name(sim, to),
                    fmt_ticks(arrive.saturating_sub(sim.tick())),
                    s.cargo.total()
                )),
            ]));
        }
    }
    if !any {
        out.push(UiNode::dim("No ships in flight."));
    }
    out
}

fn colonies_window(ctx: &PanelCtx) -> Vec<UiNode> {
    let sim = &ctx.sim.0;
    let mut out = vec![window_header("Colonies", Popup::Colonies)];
    for p in sim.colonies() {
        let c = p.colony.as_ref().expect("colony");
        let s = p.surface.as_ref().expect("surface");
        let mut row = vec![
            UiNode::btn(p.name.clone(), UiAction::ViewPlanet(p.id)),
            UiNode::colored(
                if c.active {
                    "operational"
                } else {
                    "work systems inactive"
                },
                if c.active { GOOD } else { WARN },
            ),
            UiNode::text(format!(
                "pop {}/{} | power {}/{} | {} items | {} research",
                c.population,
                sim::Colony::housing(s),
                fmt_kw(c.power.produced as i64),
                fmt_kw(c.power.demand as i64),
                c.inventory.total(),
                c.research_total
            )),
        ];
        if c.power.shortage {
            row.push(UiNode::colored("POWER SHORTAGE", BAD));
        }
        if sim.player.at == Some(p.id) {
            row.push(UiNode::colored("(you are here)", INFO));
        }
        out.push(UiNode::Row(row));
    }
    out
}

fn codex_window(ctx: &PanelCtx) -> Vec<UiNode> {
    let r = &ctx.sim.0.research;
    let tab = *ctx.codex_tab;
    let recipes_known = Recipe::ALL
        .iter()
        .filter(|rc| rc.info().tech.is_none_or(|t| r.has(t)))
        .count();
    let places: Vec<Placeable> = Cat::ALL.iter().flat_map(|c| c.items()).collect();
    let places_known = places
        .iter()
        .filter(|pl| pl.tech().is_none_or(|t| r.has(t)))
        .count();
    let summary = match tab {
        CodexTab::Recipes => format!("{recipes_known} of {} recipes known", Recipe::ALL.len()),
        CodexTab::Buildings => format!("{places_known} of {} structures unlocked", places.len()),
        CodexTab::Items => format!("{} item types across six tiers", Item::ALL.len()),
    };
    let tab_btn = |label: &str, t: CodexTab| {
        UiNode::Button(
            label.into(),
            UiAction::CodexTab(t),
            if tab == t { Btn::Active } else { Btn::Normal },
        )
    };
    let mut out = vec![
        UiNode::Row(vec![
            UiNode::Icon(Icon::Book, ACCENT, 30.0),
            UiNode::Fixed(
                430.0,
                vec![
                    UiNode::sized("CODEX", Color::srgb(0.6, 0.86, 1.0), 26.0),
                    UiNode::dim(summary),
                ],
            ),
            UiNode::Spacer,
            tab_btn("Recipes", CodexTab::Recipes),
            tab_btn("Structures", CodexTab::Buildings),
            tab_btn("Items", CodexTab::Items),
            UiNode::IconBtn(
                Icon::Close,
                UiAction::OpenWindow(Popup::Codex),
                Btn::Flat,
                14.0,
            ),
        ]),
        UiNode::Gap,
    ];
    let (key, items, hint) = match tab {
        CodexTab::Recipes => (
            "codex-recipes",
            codex_recipes(r),
            "Locked recipes unlock through research. Times are one cycle at 100% machine speed; Mk2 machines run at 200%.",
        ),
        CodexTab::Buildings => (
            "codex-buildings",
            codex_buildings(r),
            "Costs are paid when placing. Locked structures unlock through research and are listed with their technology.",
        ),
        CodexTab::Items => (
            "codex-items",
            codex_items(),
            "Items are stored in the local planet inventory. Made by lists every recipe that produces the item.",
        ),
    };
    out.push(UiNode::Scroll {
        key,
        height: Val::Vh(72.0),
        items,
    });
    out.push(UiNode::dim(hint));
    out
}

fn codex_recipes(r: &sim::Research) -> Vec<UiNode> {
    use sim::recipes::Category;
    let by_cat = |cat| {
        Recipe::ALL
            .iter()
            .copied()
            .filter(|rc| rc.info().category == cat)
            .collect::<Vec<_>>()
    };
    let smelting = by_cat(Category::Smelting);
    let chemistry = by_cat(Category::Chemistry);
    let mut assembly = by_cat(Category::Assembly);
    let second = assembly.len().div_ceil(2);
    let assembly_b = assembly.split_off(second);
    vec![UiNode::Columns(vec![
        codex_recipe_column(Category::Smelting, &smelting, r, ""),
        codex_recipe_column(Category::Chemistry, &chemistry, r, ""),
        codex_recipe_column(Category::Assembly, &assembly, r, ""),
        codex_recipe_column(Category::Assembly, &assembly_b, r, " (cont.)"),
    ])]
}

fn codex_recipe_column(
    cat: sim::recipes::Category,
    list: &[Recipe],
    r: &sim::Research,
    suffix: &str,
) -> Vec<UiNode> {
    let (icon, color) = recipe_cat_style(cat);
    let known = list
        .iter()
        .filter(|rc| rc.info().tech.is_none_or(|t| r.has(t)))
        .count();
    let mut col = vec![codex_group(
        icon,
        &format!("{}{suffix}", recipe_cat_name(cat)),
        color,
        known,
        list.len(),
    )];
    col.extend(list.iter().map(|rc| codex_recipe_card(*rc, r, color)));
    col
}

fn codex_recipe_card(rc: Recipe, r: &sim::Research, accent: Color) -> UiNode {
    let i = rc.info();
    let known = i.tech.is_none_or(|t| r.has(t));
    let alpha = if known { 1.0 } else { 0.4 };
    let machine = match i.category {
        sim::recipes::Category::Smelting => "Smelter",
        sim::recipes::Category::Chemistry => "Refinery",
        sim::recipes::Category::Assembly => "Assembler",
    };
    let (out, n) = i.outputs.first().copied().unwrap_or((Item::IronPlate, 1));
    let name = if n > 1 {
        format!("{} x{n}", out.name())
    } else {
        out.name().to_string()
    };
    let secs = i.ticks as f32 / sim::TICKS_PER_SEC as f32;
    let mut inputs = Vec::new();
    for (k, &(item, qty)) in i.inputs.iter().enumerate() {
        if k > 0 {
            inputs.push(UiNode::dim("+"));
        }
        inputs.push(UiNode::Row(vec![
            UiNode::Icon(item_icon(item), item_color(item).with_alpha(alpha), 14.0),
            UiNode::sized(format!("{} x{qty}", item.name()), DIM, 10.5),
        ]));
    }
    let status = if known {
        UiNode::colored(format!("{machine} - known"), GOOD)
    } else {
        match i.tech {
            Some(t) => UiNode::colored(format!("{machine} - locked: {}", t.info().name), WARN),
            None => UiNode::colored(format!("{machine} - known"), GOOD),
        }
    };
    UiNode::Card(
        vec![
            UiNode::Spread(vec![
                UiNode::Row(vec![
                    UiNode::Icon(item_icon(out), item_color(out).with_alpha(alpha), 18.0),
                    UiNode::sized(name, if known { Color::WHITE } else { DIM }, 12.5),
                ]),
                UiNode::dim(format!("{secs:.1}s")),
            ]),
            UiNode::Wrap(inputs),
            status,
        ],
        if known { accent } else { DIM },
    )
}

fn codex_buildings(r: &sim::Research) -> Vec<UiNode> {
    let groups = [
        vec![Cat::Mining, Cat::Housing, Cat::Defense],
        vec![Cat::Smelting, Cat::Power],
        vec![Cat::Production, Cat::Research],
        vec![Cat::Logistics],
    ];
    let mut cols = Vec::new();
    for group in groups {
        let mut col = Vec::new();
        for cat in group {
            let items = cat.items();
            let known = items
                .iter()
                .filter(|pl| pl.tech().is_none_or(|t| r.has(t)))
                .count();
            col.push(codex_group(
                cat.icon(),
                cat.name(),
                codex_cat_color(cat),
                known,
                items.len(),
            ));
            col.extend(items.into_iter().map(|pl| codex_placeable_card(pl, r)));
        }
        cols.push(col);
    }
    vec![UiNode::Columns(cols)]
}

fn codex_placeable_card(pl: Placeable, r: &sim::Research) -> UiNode {
    let known = pl.tech().is_none_or(|t| r.has(t));
    let alpha = if known { 1.0 } else { 0.4 };
    let (w, h) = pl.size();
    let cat = cat_for(pl);
    let mut rows = vec![
        UiNode::Spread(vec![
            UiNode::Row(vec![
                UiNode::Icon(cat.icon(), codex_cat_color(cat), 18.0),
                UiNode::sized(pl.name(), if known { Color::WHITE } else { DIM }, 13.0),
            ]),
            UiNode::dim(format!("{w}x{h}")),
        ]),
        codex_cost(pl.cost(), alpha),
    ];
    let desc = match pl {
        Placeable::Building(k) => k.spec().desc,
        Placeable::Belt(_) => {
            "Drag to lay belts. Belts feed depots directly; machines need inserters."
        }
    };
    rows.push(UiNode::dim(desc));
    let mut facts = Vec::new();
    match pl {
        Placeable::Belt(t) => facts.push(format!("{} tiles/s", t.speed())),
        Placeable::Building(k) => {
            let spec = k.spec();
            if spec.power > 0 {
                facts.push(format!("{} kW draw", spec.power));
            }
            if spec.power_gen > 0 {
                facts.push(format!("{} kW output", spec.power_gen));
            }
            if spec.workers > 0 {
                facts.push(format!("{} workers", spec.workers));
            }
        }
    }
    if !facts.is_empty() {
        rows.push(UiNode::dim(facts.join(" - ")));
    }
    rows.push(if known {
        UiNode::colored("Unlocked", GOOD)
    } else {
        match pl.tech() {
            Some(t) => UiNode::colored(format!("Locked: {}", t.info().name), WARN),
            None => UiNode::colored("Unlocked", GOOD),
        }
    });
    UiNode::Card(rows, if known { codex_cat_color(cat) } else { DIM })
}

fn codex_items() -> Vec<UiNode> {
    let mut list = Item::ALL.to_vec();
    list.sort_by_key(|i| (i.tier(), *i));
    let mut cols: Vec<Vec<UiNode>> = (0..4).map(|_| Vec::new()).collect();
    let n = list.len().max(1);
    for (k, item) in list.into_iter().enumerate() {
        cols[k * 4 / n].push(codex_item_card(item));
    }
    vec![UiNode::Columns(cols)]
}

fn codex_item_card(item: Item) -> UiNode {
    let sources = Recipe::ALL
        .iter()
        .copied()
        .filter(|rc| rc.info().outputs.iter().any(|(o, _)| *o == item))
        .collect::<Vec<_>>();
    let used = Recipe::ALL
        .iter()
        .filter(|rc| rc.info().inputs.iter().any(|(i, _)| *i == item))
        .count();
    let source = if sources.is_empty() {
        "Raw resource".to_string()
    } else {
        format!(
            "Made by {}",
            sources
                .iter()
                .map(|rc| rc.info().name)
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    UiNode::Card(
        vec![
            UiNode::Spread(vec![
                UiNode::Row(vec![
                    UiNode::Icon(item_icon(item), item_color(item), 18.0),
                    UiNode::sized(item.name(), Color::WHITE, 12.5),
                ]),
                UiNode::dim(format!("T{}", item.tier())),
            ]),
            UiNode::dim(source),
            UiNode::dim(format!(
                "Used in {used} recipe{}",
                if used == 1 { "" } else { "s" }
            )),
        ],
        item_color(item),
    )
}

fn codex_group(icon: Icon, title: &str, color: Color, known: usize, total: usize) -> UiNode {
    UiNode::Spread(vec![
        UiNode::Row(vec![
            UiNode::Icon(icon, color, 18.0),
            UiNode::sized(title, color, 14.0),
        ]),
        UiNode::dim(format!("{known}/{total}")),
    ])
}

fn codex_cost(cost: &[(Item, u32)], alpha: f32) -> UiNode {
    UiNode::Wrap(
        cost.iter()
            .map(|&(item, n)| {
                UiNode::Row(vec![
                    UiNode::Icon(item_icon(item), item_color(item).with_alpha(alpha), 15.0),
                    UiNode::sized(
                        format!("{} x{n}", item.name()),
                        DIM.with_alpha(alpha.max(0.8)),
                        11.0,
                    ),
                ])
            })
            .collect(),
    )
}

fn recipe_cat_name(cat: sim::recipes::Category) -> &'static str {
    match cat {
        sim::recipes::Category::Smelting => "Smelting",
        sim::recipes::Category::Chemistry => "Chemistry",
        sim::recipes::Category::Assembly => "Assembly",
    }
}

fn recipe_cat_style(cat: sim::recipes::Category) -> (Icon, Color) {
    match cat {
        sim::recipes::Category::Smelting => (Icon::Flame, rgb(0xff9a3c)),
        sim::recipes::Category::Chemistry => (Icon::Flask, rgb(0x4fdc7a)),
        sim::recipes::Category::Assembly => (Icon::Factory, rgb(0x5aa0ff)),
    }
}

fn codex_cat_color(cat: Cat) -> Color {
    match cat {
        Cat::Mining => rgb(0xd08a5a),
        Cat::Smelting => rgb(0xff9a3c),
        Cat::Production => rgb(0x5aa0ff),
        Cat::Power => YELLOW,
        Cat::Logistics => ACCENT,
        Cat::Housing => GOOD,
        Cat::Research => PURPLE,
        Cat::Defense => BAD,
    }
}

fn cat_for(pl: Placeable) -> Cat {
    match pl {
        Placeable::Belt(_) => Cat::Logistics,
        Placeable::Building(k) => Cat::of(k),
    }
}

fn goals(sim: &Sim) -> Vec<(bool, &'static str)> {
    let colonies = sim.colonies().count();
    let has_kind = |k: BuildingKind| {
        sim.colonies()
            .filter_map(|p| p.surface.as_ref())
            .any(|s| s.buildings.values().any(|b| b.kind == k))
    };
    let titanium = sim.colonies().any(|p| {
        p.colony.as_ref().is_some_and(|c| {
            c.inventory.get(Item::TitaniumOre) + c.inventory.get(Item::TitaniumAlloy) > 0
        })
    });
    vec![
        (
            has_kind(BuildingKind::Lab) && !sim.research.unlocked.is_empty(),
            "Build a staffed research lab and finish a technology",
        ),
        (
            has_kind(BuildingKind::Shipyard),
            "Research Rocketry and build a shipyard",
        ),
        (!sim.ships.is_empty(), "Build your first scout ship"),
        (
            sim.universe
                .planets()
                .any(|p| p.surveyed() && p.id != sim::HOME),
            "Survey another world",
        ),
        (colonies >= 2, "Deliver a colony kit and found a colony"),
        (
            sim.colonies()
                .filter(|p| p.colony.as_ref().is_some_and(|c| c.active))
                .count()
                >= 2,
            "Visit the colony in person and establish work systems",
        ),
        (titanium, "Bring off-world titanium into your industry"),
        (
            sim.ships.values().any(|s| s.drive != Drive::Chemical),
            "Build a warp-capable ship",
        ),
        (
            sim.universe.galaxies[0]
                .systems
                .iter()
                .filter(|s| s.knowledge == SystemKnowledge::Visited)
                .count()
                >= 3,
            "Visit three star systems",
        ),
        (
            sim.colonies().any(|p| p.id.galaxy != 0),
            "Establish a colony in another galaxy",
        ),
        (sim.victory.is_some(), "Light the Frontier Beacon"),
    ]
}

fn help_window(ctx: &PanelCtx) -> Vec<UiNode> {
    let mut out = vec![
        window_header("Help", Popup::Help),
        UiNode::Row(vec![
            UiNode::btn("Save game [F5]", UiAction::Save),
            UiNode::btn("Reload last save [F9]", UiAction::Load),
            UiNode::btn(
                "Tutorial",
                UiAction::Menu(crate::menu::MenuAction::Tutorial),
            ),
            UiNode::btn(
                "Settings",
                UiAction::Menu(crate::menu::MenuAction::Open(crate::menu::Page::Settings)),
            ),
            UiNode::btn("Main menu", UiAction::Menu(crate::menu::MenuAction::ToMain)),
        ]),
        UiNode::Divider,
        UiNode::head("How to play"),
        UiNode::text(
            "Automation within locations. Logistics between locations. Research that expands what your civilization can do.",
        ),
        UiNode::Gap,
        UiNode::dim(
            "Camera: WASD/arrows pan (Shift faster), wheel zoom, middle-drag or , . to rotate. M: star map. Backspace/right-click on map: zoom out.",
        ),
        UiNode::dim(
            "Building: pick a category in the bottom toolbar or press E/I/N/F/Y/G/H. R rotates. Left-click places (drag for belts), right-click removes. Q/Esc: select tool.",
        ),
        UiNode::dim(
            "Time: Space pause, 1-5 = 1x/4x/16x/64x/1024x, +/- step. Faster speeds (up to 32768x) unlock in the Time Control tree. Instant-Warp skips to a ship's arrival.",
        ),
        UiNode::dim(
            "Windows: T research, U time-control tree, O codex, B ships, L logistics, C colonies, F1 help. F5 save, F9 load.",
        ),
        UiNode::Gap,
        UiNode::dim(
            "Miners push ore onto belts or straight into machines in front of them. Inserters move items from behind to in front. Belts flowing into a Depot store items in the planet inventory; Providers push items out of it.",
        ),
        UiNode::dim(
            "Machines need power. Generators burn carbon or fuel (feed them with a provider or inserter). Unpowered machines pause and resume automatically.",
        ),
        UiNode::dim(
            "Labs, observatories and shipyards need people. Houses keep people alive; colonies work only after you visit them in person (ride a ship there) and establish work systems.",
        ),
        UiNode::dim(
            "Ships carry everything between planets: load cargo and people, then pick a destination. Fuel comes from ship cargo first, then the local inventory.",
        ),
        UiNode::Gap,
        UiNode::head("Goals"),
    ];
    for (done, g) in goals(&ctx.sim.0) {
        out.push(UiNode::colored(
            format!("{} {g}", if done { "[x]" } else { "[ ]" }),
            if done {
                GOOD
            } else {
                Color::srgb(0.75, 0.78, 0.82)
            },
        ));
    }
    out
}

/// Item rate caps the player can step through, per minute; past the last is "no cap".
const RATE_STEPS: [u32; 9] = [6, 12, 30, 60, 120, 240, 480, 900, 1200];

fn step_rate(cur: Option<u32>, up: bool) -> Option<u32> {
    let n = RATE_STEPS.len();
    // Index on the ladder; `n` means uncapped.
    let i = cur.map_or(n, |v| {
        RATE_STEPS.iter().position(|&s| s >= v).unwrap_or(n - 1)
    });
    let j = if up {
        (i + 1).min(n)
    } else {
        i.saturating_sub(1)
    };
    RATE_STEPS.get(j).copied()
}

/// Throughput caps: steppers for items in and out per minute.
fn rate_controls(p: PlanetId, b: &sim::surface::Building) -> Vec<UiNode> {
    let k = b.kind;
    let takes_in = !k.is_miner() && k != BuildingKind::Provider;
    let gives_out = !matches!(
        k,
        BuildingKind::Depot | BuildingKind::Generator | BuildingKind::Reactor
    );
    let label = |v: Option<u32>| match v {
        Some(n) => format!("{n}/min"),
        None => "no cap".to_string(),
    };
    let frac = |v: Option<u32>| v.map(|n| n as f32 / 1200.0);
    let set = |i: Option<u32>, o: Option<u32>| {
        UiAction::Cmd(Command::SetRateLimits {
            planet: p,
            building: b.id,
            input: i,
            output: o,
        })
    };
    let mut out = vec![
        UiNode::Divider,
        UiNode::colored("Throughput limits", ACCENT),
    ];
    if takes_in {
        out.push(UiNode::Stepper {
            label: "Max input".into(),
            value: label(b.in_limit),
            frac: frac(b.in_limit),
            dec: set(step_rate(b.in_limit, false), b.out_limit),
            inc: set(step_rate(b.in_limit, true), b.out_limit),
        });
    }
    if gives_out {
        out.push(UiNode::Stepper {
            label: "Max output".into(),
            value: label(b.out_limit),
            frac: frac(b.out_limit),
            dec: set(b.in_limit, step_rate(b.out_limit, false)),
            inc: set(b.in_limit, step_rate(b.out_limit, true)),
        });
    }
    if b.in_limit.is_some() || b.out_limit.is_some() {
        out.push(UiNode::btn("Remove limits", set(None, None)));
    }
    out
}
