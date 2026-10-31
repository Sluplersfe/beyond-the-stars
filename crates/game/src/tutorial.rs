//! The guided tutorial: a checklist of steps shown in a card at the top of the screen, with
//! pulsing highlights on the UI buttons and world tiles the player needs next.
//!
//! Steps finish when their goals are true in the sim or the UI, so the guide follows what the
//! player actually does. It can be started on a new game or restarted from the pause menu.

use std::collections::{BTreeMap, BTreeSet};
use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;
use sim::surface::Terrain;
use sim::{BuildingKind, HOME, Item, Sim};

use crate::camera::CameraRig;
use crate::menu::{MenuAction, MenuState, Screen, Session};
use crate::save::TutorialSave;
use crate::state::*;
use crate::ui::{ACCENT, Btn, BtnLabel, DIM, HEAD, PanelId, UiAction, UiNode, WindowRow};

pub struct TutorialPlugin;

impl Plugin for TutorialPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Tutorial>()
            .add_systems(Startup, spawn_glows)
            .add_systems(
                Update,
                (tick, highlight_ui, shift_windows, draw_marks).chain(),
            );
    }
}

/// Things a step can point at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    /// A UI button, by label (exact, or followed by a hotkey like `Miner [N]`).
    Ui(&'static str),
    /// A whole HUD panel.
    Panel(PanelId),
    /// The nearest iron patch.
    Iron,
    /// The home depot.
    Depot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Goal {
    /// Press the Continue button.
    Continue,
    Pan,
    Zoom,
    Rotate,
    PauseResume,
    Built(BuildingKind),
    RecipeSet(BuildingKind),
    IronOre(u32),
    IronPlates(u32),
    SelectBuilding,
    ResearchPicked,
    WindowOpen(Popup),
    ViewMap,
    ViewPlanet,
    Saved,
}

impl Goal {
    pub fn label(self) -> String {
        match self {
            Goal::Continue => "Read this, then press Continue".into(),
            Goal::Pan => "Move the camera (W A S D)".into(),
            Goal::Zoom => "Zoom in or out (mouse wheel)".into(),
            Goal::Rotate => "Rotate the view (middle-drag or , .)".into(),
            Goal::PauseResume => "Pause the game, then resume".into(),
            Goal::Built(k) => format!("Build a {}", k.spec().name),
            Goal::RecipeSet(k) => format!("Choose a recipe for the {}", k.spec().name),
            Goal::IronOre(n) => format!("Deliver {n} iron ore to the Depot"),
            Goal::IronPlates(n) => format!("Deliver {n} iron plates to the Depot"),
            Goal::SelectBuilding => "Click a building to inspect it".into(),
            Goal::ResearchPicked => "Pick a research project".into(),
            Goal::WindowOpen(Popup::TechTree) => "Open the Time Control tree (U)".into(),
            Goal::WindowOpen(_) => "Open the window".into(),
            Goal::ViewMap => "Open the star map (M)".into(),
            Goal::ViewPlanet => "Return to your colony (M)".into(),
            Goal::Saved => "Save the game (F5)".into(),
        }
    }
}

pub struct Step {
    pub title: &'static str,
    pub body: &'static str,
    pub goals: &'static [Goal],
    pub marks: &'static [Mark],
    /// Point the camera at this when the step begins.
    pub focus: Option<Mark>,
    /// Shown if the player seems stuck.
    pub hint: &'static str,
}

const fn step(
    title: &'static str,
    body: &'static str,
    goals: &'static [Goal],
    marks: &'static [Mark],
) -> Step {
    Step {
        title,
        body,
        goals,
        marks,
        focus: None,
        hint: "",
    }
}

use BuildingKind as K;

pub const STEPS: &[Step] = &[
    step(
        "Welcome, Overseer",
        "You oversee a young civilization on Terra Nova. Automate mining and production here, then build ships to settle other worlds and galaxies. This guide takes about ten minutes. Leave any time and restart it from the pause menu (Esc).",
        &[Goal::Continue],
        &[],
    ),
    step(
        "Look around",
        "Move the camera with W A S D (hold Shift to go faster) and zoom with the mouse wheel. Rotate by dragging with the middle mouse button, or press the comma and period keys.",
        &[Goal::Pan, Goal::Zoom, Goal::Rotate],
        &[],
    ),
    step(
        "Read your status",
        "The top bar shows your stockpiles with their change per minute, plus population and power. The left card tracks power supply against demand, population, research and threats. Machines stop when power runs out.",
        &[Goal::Continue],
        &[Mark::Panel(PanelId::TopBar), Mark::Panel(PanelId::Left)],
    ),
    step(
        "Control time",
        "Space pauses and resumes. Number keys and plus / minus change the game speed; faster speeds are unlocked by research. Pause the game now, then resume it.",
        &[Goal::PauseResume],
        &[Mark::Ui("Pause")],
    ),
    Step {
        focus: Some(Mark::Iron),
        ..step(
            "Find iron",
            "Coloured patches on the ground are ore deposits. The marked patch is iron, the base of everything you will build. Deposits never run out.",
            &[Goal::Continue],
            &[Mark::Iron],
        )
    },
    Step {
        hint: "A miner must sit on the deposit and costs iron plates and gears from your stockpile.",
        ..step(
            "Place a miner",
            "Open Mining in the bottom bar, choose Miner and click on the iron patch. Press R before placing to turn it: a miner pushes ore out of the side it faces, so face it towards the Depot in the middle of the base.",
            &[Goal::Built(K::Miner)],
            &[Mark::Ui("Mining"), Mark::Ui("Miner"), Mark::Iron],
        )
    },
    Step {
        hint: "Select the miner to see its status. It must face the first belt, and the belt line must run all the way into the Depot.",
        ..step(
            "Connect a conveyor",
            "Open Logistics, choose Belt and click-drag from the tile in front of the miner to the Depot (the 2x2 box near the middle). Belts carry one item per tile and right-click removes one. The Depot stores everything that reaches it.",
            &[Goal::IronOre(5)],
            &[Mark::Ui("Logistics"), Mark::Ui("Belt"), Mark::Depot],
        )
    },
    step(
        "Inspect a building",
        "Press Q to stop building, then click the miner. Its card shows status, items in and out per minute, power draw and efficiency. Lamps on buildings: green working, yellow waiting for something, red no power, grey idle.",
        &[Goal::SelectBuilding],
        &[],
    ),
    step(
        "Smelt the ore",
        "Ore must be smelted into plates. Open Smelting and place a Smelter so that a belt runs straight into it: belts feed any machine in front of them, but machines never push products out by themselves.",
        &[Goal::Built(K::Smelter)],
        &[Mark::Ui("Smelting"), Mark::Ui("Smelter")],
    ),
    Step {
        hint: "Check the Power bar on the left: if demand is above supply, build a Generator. Make sure the inserter faces away from the smelter.",
        ..step(
            "Take the plates out",
            "Place an Inserter beside the smelter, facing away from it (R rotates), then a belt from the inserter to the Depot. Inserters move items from the tile behind them to the tile in front. Wait for plates to arrive.",
            &[Goal::IronPlates(5)],
            &[Mark::Ui("Logistics"), Mark::Ui("Inserter"), Mark::Depot],
        )
    },
    step(
        "Keep the power on",
        "Everything draws from a shared grid. Your Generator burns carbon fed by a Provider. If the left card shows demand above supply, build more in Power. Solar panels and better reactors come with research.",
        &[Goal::Continue],
        &[Mark::Panel(PanelId::Left), Mark::Ui("Power")],
    ),
    step(
        "Make components",
        "Assemblers craft parts from plates. Place one in Production, then click it and choose a recipe (try Gear) on its card. Feed it with belts and empty it with an inserter, just like the smelter.",
        &[Goal::Built(K::Assembler), Goal::RecipeSet(K::Assembler)],
        &[Mark::Ui("Production"), Mark::Ui("Assembler")],
    ),
    step(
        "Houses and people",
        "People staff labs, observatories and shipyards. Each House holds 4 and the population grows while there is room. Build a House in Housing.",
        &[Goal::Built(K::House)],
        &[Mark::Ui("Housing"), Mark::Ui("House")],
    ),
    step(
        "Build a research lab",
        "Research unlocks new buildings and faster time. A Research Lab needs 2 people and power and makes 1 point per second. Build one in Research.",
        &[Goal::Built(K::Lab)],
        &[Mark::Ui("Research"), Mark::Ui("Research lab")],
    ),
    step(
        "Choose a project",
        "Open the Research window (T) and pick a technology. Time control techs speed up the game, so Basic Time Acceleration is a good first choice. Progress is kept when you switch projects.",
        &[Goal::ResearchPicked],
        &[Mark::Ui("Flask")],
    ),
    step(
        "The time control tree",
        "The Time Control tree (U) has four tiers, from 2x up to 32768x, ending with Instant-Warp, which skips ahead until a ship arrives. Open it and have a look.",
        &[Goal::WindowOpen(Popup::TechTree)],
        &[Mark::Ui("Atom")],
    ),
    step(
        "Reach for the stars",
        "Research Rocketry, then build a Shipyard in Production. Ships carry cargo and people. A Scout surveys other worlds and a colony kit founds a colony; colonies only start working after you visit them in person.",
        &[Goal::Continue],
        &[],
    ),
    step(
        "Open the star map",
        "Press M, or click the emblem at the top left, to open the star map. Systems, planets and ships live there. Click a planet to inspect it and click it again to zoom in.",
        &[Goal::ViewMap],
        &[],
    ),
    step(
        "Back home",
        "Press M again to return to your colony. On the map, selecting a destination and choosing Plan Journey sends your ship there.",
        &[Goal::ViewPlanet],
        &[],
    ),
    step(
        "Save your game",
        "Press F5, or open the pause menu with Esc and choose Save Game. The game also autosaves every few minutes (see Settings).",
        &[Goal::Saved],
        &[],
    ),
    step(
        "You are ready",
        "Open Help (F1) for the goals checklist. The long-term aim is to light the Frontier Beacon in another galaxy. Restart this guide any time from the pause menu. Good luck, Overseer.",
        &[Goal::Continue],
        &[],
    ),
];

// ---------------------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------------------

/// What the sim looks like, for deciding whether goals are met.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Facts {
    pub kinds: BTreeMap<BuildingKind, u32>,
    pub with_recipe: BTreeSet<BuildingKind>,
    pub ore: u32,
    pub plates: u32,
}

pub fn facts(sim: &Sim) -> Facts {
    let mut f = Facts::default();
    for p in sim.colonies() {
        if let Some(s) = &p.surface {
            for b in s.buildings.values() {
                *f.kinds.entry(b.kind).or_insert(0) += 1;
                if b.recipe.is_some() {
                    f.with_recipe.insert(b.kind);
                }
            }
        }
        if let Some(c) = &p.colony {
            f.ore += c.inventory.get(Item::IronOre);
            f.plates += c.inventory.get(Item::IronPlate);
        }
    }
    f
}

#[derive(Debug, Clone, Copy)]
pub struct Camera {
    pub focus: Vec3,
    pub dist: f32,
    pub yaw: f32,
}

/// Live state besides the sim that goals look at.
pub struct Live {
    pub camera: Camera,
    pub view: View,
    pub selected: Selection,
    pub window: Popup,
    pub saw_pause: bool,
    pub paused: bool,
    pub research_picked: bool,
    pub manual_saves: u32,
}

#[derive(Debug, Clone)]
pub struct Baseline {
    facts: Facts,
    camera: Camera,
    saves: u32,
}

impl Goal {
    pub fn done(self, now: &Facts, live: &Live, base: &Baseline) -> bool {
        let more = |k: BuildingKind| {
            now.kinds.get(&k).copied().unwrap_or(0) > base.facts.kinds.get(&k).copied().unwrap_or(0)
        };
        match self {
            // Handled by the button.
            Goal::Continue => false,
            Goal::Pan => live.camera.focus.distance(base.camera.focus) > 3.0,
            Goal::Zoom => (live.camera.dist - base.camera.dist).abs() > 2.0,
            Goal::Rotate => (live.camera.yaw - base.camera.yaw).abs() > 0.25,
            Goal::PauseResume => live.saw_pause && !live.paused,
            Goal::Built(k) => more(k),
            Goal::RecipeSet(k) => now.with_recipe.contains(&k),
            Goal::IronOre(n) => now.ore >= base.facts.ore + n,
            Goal::IronPlates(n) => now.plates >= base.facts.plates + n,
            Goal::SelectBuilding => matches!(live.selected, Selection::Building(..)),
            Goal::ResearchPicked => live.research_picked,
            Goal::WindowOpen(w) => live.window == w,
            Goal::ViewMap => matches!(live.view, View::Map(_)),
            Goal::ViewPlanet => matches!(live.view, View::Planet(_)),
            Goal::Saved => live.manual_saves > base.saves,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuideAction {
    Next,
    Prev,
    /// Collapse or expand the card.
    Toggle,
    Exit,
}

#[derive(Resource, Default)]
pub struct Tutorial {
    pub active: bool,
    pub step: usize,
    /// Completed or dismissed at least once.
    pub finished: bool,
    pub minimized: bool,
    base: Option<Baseline>,
    saw_pause: bool,
    /// Seconds the current step's goals have all been true.
    complete_for: f32,
    /// Seconds spent on the step.
    stuck_for: f32,
    /// A step needs a camera move when it begins.
    focus_pending: bool,
}

impl Tutorial {
    pub fn start(&mut self) {
        *self = Tutorial {
            active: true,
            ..Tutorial::default()
        };
        self.focus_pending = true;
    }

    pub fn dismiss(&mut self) {
        self.active = false;
        self.finished = true;
        self.base = None;
    }

    pub fn restore(&mut self, s: TutorialSave) {
        *self = Tutorial::default();
        self.finished = s.finished;
        if let Some(i) = s.step {
            self.active = true;
            self.step = (i as usize).min(STEPS.len() - 1);
            self.focus_pending = true;
        }
    }

    pub fn snapshot(&self) -> TutorialSave {
        TutorialSave {
            step: self.active.then_some(self.step as u32),
            finished: self.finished,
        }
    }

    fn goto(&mut self, step: usize) {
        self.step = step;
        self.base = None;
        self.saw_pause = false;
        self.complete_for = 0.0;
        self.stuck_for = 0.0;
        self.focus_pending = true;
    }

    pub fn apply(&mut self, a: GuideAction, log: &mut MessageLog) {
        match a {
            GuideAction::Next => {
                if self.step + 1 >= STEPS.len() {
                    self.dismiss();
                    log.push(MsgKind::Good, "Tutorial complete. Good luck, Overseer!");
                } else {
                    self.goto(self.step + 1);
                }
            }
            GuideAction::Prev => {
                if self.step > 0 {
                    self.goto(self.step - 1);
                }
            }
            GuideAction::Toggle => self.minimized = !self.minimized,
            GuideAction::Exit => {
                self.dismiss();
                log.push(
                    MsgKind::Info,
                    "Tutorial closed. Restart it any time from the pause menu (Esc).",
                );
            }
        }
    }

    pub fn current(&self) -> &'static Step {
        &STEPS[self.step.min(STEPS.len() - 1)]
    }

    fn goal_states(&self, now: &Facts, live: &Live) -> Vec<bool> {
        let Some(base) = &self.base else {
            return vec![false; self.current().goals.len()];
        };
        self.current()
            .goals
            .iter()
            .map(|g| g.done(now, live, base))
            .collect()
    }
}

// ---------------------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn tick(
    mut t: ResMut<Tutorial>,
    sim: Res<SimState>,
    view: Res<View>,
    selected: Res<Selected>,
    window: Res<OpenWindow>,
    virt: Res<Time<Virtual>>,
    time: Res<Time<Real>>,
    mut rig: ResMut<CameraRig>,
    session: Res<Session>,
    screen: Res<Screen>,
    menu: Res<MenuState>,
    mut log: ResMut<MessageLog>,
) {
    if !t.active || *screen != Screen::Playing || menu.open {
        return;
    }
    let dt = time.delta_secs();
    let cam = Camera {
        focus: rig.goal_focus,
        dist: rig.goal_dist,
        yaw: rig.yaw,
    };
    let now = facts(&sim.0);
    if t.base.is_none() {
        t.base = Some(Baseline {
            facts: now.clone(),
            camera: cam,
            saves: session.manual_saves,
        });
    }
    if t.focus_pending {
        t.focus_pending = false;
        if let Some(m) = t.current().focus
            && let Some(p) = mark_world(&sim.0, m)
            && matches!(*view, View::Planet(_))
        {
            rig.goal_focus = p;
            rig.goal_dist = 32.0;
            // The camera moved on purpose; measure camera goals from the new framing.
            if let Some(b) = t.base.as_mut() {
                b.camera = Camera {
                    focus: p,
                    dist: 32.0,
                    yaw: rig.yaw,
                };
            }
        }
    }
    if virt.is_paused() {
        t.saw_pause = true;
    }
    let live = Live {
        camera: cam,
        view: *view,
        selected: selected.0,
        window: window.0,
        saw_pause: t.saw_pause,
        paused: virt.is_paused(),
        research_picked: sim.0.research.current.is_some() || !sim.0.research.unlocked.is_empty(),
        manual_saves: session.manual_saves,
    };
    let states = t.goal_states(&now, &live);
    let step = t.current();
    let auto: Vec<bool> = step
        .goals
        .iter()
        .zip(&states)
        .filter(|(g, _)| **g != Goal::Continue)
        .map(|(_, d)| *d)
        .collect();
    let continue_only = step.goals.contains(&Goal::Continue);
    t.stuck_for += dt;
    if !continue_only && !auto.is_empty() && auto.iter().all(|d| *d) {
        t.complete_for += dt;
        if t.complete_for > 1.4 {
            t.apply(GuideAction::Next, &mut log);
        }
    } else {
        t.complete_for = 0.0;
    }
}

/// Content of the tutorial card.
pub fn panel(ctx: &crate::panels::PanelCtx) -> Vec<UiNode> {
    let t = &*ctx.tutorial;
    if !t.active || *ctx.screen != Screen::Playing || ctx.menu.open {
        return vec![];
    }
    let n = STEPS.len();
    let step = t.current();
    let tag = format!("TUTORIAL {}/{}", t.step + 1, n);
    if t.minimized {
        return vec![UiNode::Row(vec![
            UiNode::sized(tag, ACCENT, 12.0),
            UiNode::dim(step.title),
            UiNode::btn(
                "Show",
                UiAction::Menu(MenuAction::Guide(GuideAction::Toggle)),
            ),
        ])];
    }
    let now = facts(&ctx.sim.0);
    let live = Live {
        camera: Camera {
            focus: ctx.rig.goal_focus,
            dist: ctx.rig.goal_dist,
            yaw: ctx.rig.yaw,
        },
        view: *ctx.view,
        selected: ctx.selected.0,
        window: ctx.window.0,
        saw_pause: t.saw_pause,
        paused: ctx.virt.is_paused(),
        research_picked: ctx.sim.0.research.current.is_some()
            || !ctx.sim.0.research.unlocked.is_empty(),
        manual_saves: ctx.session.manual_saves,
    };
    let states = t.goal_states(&now, &live);
    let guide = |a| UiAction::Menu(MenuAction::Guide(a));
    let mut out = vec![
        UiNode::Spread(vec![
            UiNode::sized(tag, ACCENT, 12.0),
            UiNode::Row(vec![
                UiNode::btn("Hide", guide(GuideAction::Toggle)),
                UiNode::btn("Exit tutorial", guide(GuideAction::Exit)),
            ]),
        ]),
        UiNode::sized(step.title, HEAD, 20.0),
        UiNode::text(step.body),
    ];
    let good = Color::srgb(0.45, 0.95, 0.5);
    for (g, done) in step.goals.iter().zip(&states) {
        if *g == Goal::Continue {
            continue;
        }
        out.push(UiNode::Row(vec![
            UiNode::Dot(if *done { good } else { DIM }, 9.0),
            UiNode::colored(g.label(), if *done { good } else { Color::WHITE }),
        ]));
    }
    if !step.hint.is_empty() && t.stuck_for > 75.0 {
        out.push(UiNode::colored(
            format!("Hint: {}", step.hint),
            Color::srgb(1.0, 0.8, 0.3),
        ));
    }
    let last = t.step + 1 == n;
    let all_done = !states.is_empty()
        && step
            .goals
            .iter()
            .zip(&states)
            .filter(|(g, _)| **g != Goal::Continue)
            .all(|(_, d)| *d);
    out.push(UiNode::Bar((t.step as f32 + 1.0) / n as f32, ACCENT));
    out.push(UiNode::Row(vec![
        UiNode::Button(
            "Back".into(),
            guide(GuideAction::Prev),
            if t.step == 0 {
                Btn::Disabled
            } else {
                Btn::Normal
            },
        ),
        UiNode::Spacer,
        UiNode::Button(
            if last {
                "Finish"
            } else if step.goals.contains(&Goal::Continue) {
                "Continue"
            } else {
                "Skip step"
            }
            .into(),
            guide(GuideAction::Next),
            if step.goals.contains(&Goal::Continue) || all_done {
                Btn::Primary
            } else {
                Btn::Normal
            },
        ),
    ]));
    out
}

// ---------------------------------------------------------------------------------------
// Highlights
// ---------------------------------------------------------------------------------------

#[derive(Component)]
struct Glow(usize);

const GLOWS: usize = 6;

fn spawn_glows(mut commands: Commands) {
    for i in 0..GLOWS {
        commands.spawn((
            Glow(i),
            Node {
                position_type: PositionType::Absolute,
                display: Display::None,
                border: UiRect::all(px(3)),
                border_radius: BorderRadius::all(px(8)),
                ..default()
            },
            BorderColor::all(Color::srgb(1.0, 0.85, 0.2)),
            GlobalZIndex(150),
            Pickable::IGNORE,
        ));
    }
}

fn label_matches(label: &str, target: &str) -> bool {
    label == target
        || label
            .strip_prefix(target)
            .is_some_and(|rest| rest.starts_with(" ["))
}

#[allow(clippy::too_many_arguments)]
fn highlight_ui(
    t: Res<Tutorial>,
    screen: Res<Screen>,
    menu: Res<MenuState>,
    time: Res<Time<Real>>,
    buttons: Query<(&BtnLabel, &ComputedNode, &UiGlobalTransform)>,
    panels: Query<(&PanelId, &ComputedNode, &UiGlobalTransform)>,
    mut glows: Query<(&Glow, &mut Node, &mut BorderColor), Without<BtnLabel>>,
) {
    let mut rects: Vec<(Vec2, Vec2)> = Vec::new();
    if t.active && *screen == Screen::Playing && !menu.open && !t.minimized {
        for m in t.current().marks {
            match m {
                Mark::Ui(label) => {
                    for (l, node, tf) in &buttons {
                        if label_matches(&l.0, label) && node.size().x > 0.0 {
                            rects.push((tf.translation, node.size() * node.inverse_scale_factor));
                            // `translation` is physical; convert the centre below.
                            let last = rects.len() - 1;
                            rects[last].0 *= node.inverse_scale_factor;
                        }
                    }
                }
                Mark::Panel(id) => {
                    for (p, node, tf) in &panels {
                        if p == id && node.size().x > 0.0 {
                            rects.push((
                                tf.translation * node.inverse_scale_factor,
                                node.size() * node.inverse_scale_factor,
                            ));
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let pulse = 0.55 + 0.45 * (time.elapsed_secs() * 4.0).sin().abs();
    for (g, mut node, mut border) in &mut glows {
        match rects.get(g.0) {
            Some((center, size)) => {
                node.display = Display::Flex;
                node.left = px(center.x - size.x / 2.0 - 4.0);
                node.top = px(center.y - size.y / 2.0 - 4.0);
                node.width = px(size.x + 8.0);
                node.height = px(size.y + 8.0);
                *border = BorderColor::all(Color::srgba(1.0, 0.85, 0.2, pulse));
            }
            None => node.display = Display::None,
        }
    }
}

/// While the tutorial card is up, windows start below it instead of under it.
fn shift_windows(
    t: Res<Tutorial>,
    screen: Res<Screen>,
    menu: Res<MenuState>,
    card: Query<(&PanelId, &ComputedNode)>,
    mut rows: Query<&mut Node, With<WindowRow>>,
) {
    let visible = t.active && *screen == Screen::Playing && !menu.open;
    let h = card
        .iter()
        .find(|(p, _)| **p == PanelId::Tutorial)
        .map(|(_, n)| n.size().y * n.inverse_scale_factor)
        .unwrap_or(0.0);
    let top = if visible && h > 0.0 {
        74.0 + h + 6.0
    } else {
        74.0
    };
    for mut n in &mut rows {
        if n.top != px(top) {
            n.top = px(top);
        }
    }
}

/// World position of a mark on the home planet, at ground level.
pub fn mark_world(sim: &Sim, m: Mark) -> Option<Vec3> {
    let s = sim.surface(HOME)?;
    match m {
        Mark::Iron => {
            let c = Vec2::new(s.w as f32 / 2.0, s.h as f32 / 2.0);
            let mut best: Option<(f32, Vec2)> = None;
            let mut sum = Vec2::ZERO;
            let mut n = 0.0;
            // Centroid of the iron patch closest to the landing zone.
            let mut tiles: Vec<Vec2> = Vec::new();
            for y in 0..s.h {
                for x in 0..s.w {
                    if let Some(t) = s.tile(x, y)
                        && t.terrain == Terrain::Deposit(Item::IronOre)
                    {
                        tiles.push(tile_center(x, y));
                    }
                }
            }
            for p in &tiles {
                let d = p.distance(c);
                if best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, *p));
                }
            }
            let (_, seed) = best?;
            for p in &tiles {
                if p.distance(seed) < 6.0 {
                    sum += *p;
                    n += 1.0;
                }
            }
            Some(to_world(sum / n, 0.2))
        }
        Mark::Depot => s
            .buildings
            .values()
            .find(|b| b.kind == BuildingKind::Depot)
            .map(|b| {
                let (w, h) = b.spec_size();
                to_world(
                    Vec2::new(b.x as f32 + w as f32 / 2.0, b.y as f32 + h as f32 / 2.0),
                    0.4,
                )
            }),
        _ => None,
    }
}

fn draw_marks(
    t: Res<Tutorial>,
    screen: Res<Screen>,
    menu: Res<MenuState>,
    view: Res<View>,
    sim: Res<SimState>,
    time: Res<Time<Real>>,
    mut gizmos: Gizmos,
) {
    if !t.active || *screen != Screen::Playing || menu.open || t.minimized {
        return;
    }
    if !matches!(*view, View::Planet(p) if p == HOME) {
        return;
    }
    let pulse = (time.elapsed_secs() * 3.0).sin() * 0.5 + 0.5;
    for m in t.current().marks {
        let Some(p) = mark_world(&sim.0, *m) else {
            continue;
        };
        let r = match m {
            Mark::Iron => 4.5,
            _ => 2.0,
        };
        let color = Color::srgba(1.0, 0.85, 0.2, 0.6 + 0.4 * pulse);
        for k in 0..2 {
            gizmos.circle(
                Isometry3d::new(p, Quat::from_rotation_x(FRAC_PI_2)),
                r + pulse * 0.8 + k as f32 * 0.6,
                color,
            );
        }
        gizmos.line(p, p + Vec3::Y * 5.0, color);
        gizmos.line(p + Vec3::Y * 5.0, p + Vec3::new(0.5, 4.0, 0.0), color);
        gizmos.line(p + Vec3::Y * 5.0, p + Vec3::new(-0.5, 4.0, 0.0), color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::{Command, Dir, Placeable};

    fn live() -> Live {
        Live {
            camera: Camera {
                focus: Vec3::ZERO,
                dist: 30.0,
                yaw: 0.0,
            },
            view: View::Planet(HOME),
            selected: Selection::None,
            window: Popup::None,
            saw_pause: false,
            paused: false,
            research_picked: false,
            manual_saves: 0,
        }
    }

    fn base(sim: &Sim) -> Baseline {
        Baseline {
            facts: facts(sim),
            camera: live().camera,
            saves: 0,
        }
    }

    #[test]
    fn every_step_has_text_and_a_goal() {
        assert!(STEPS.len() >= 15);
        for s in STEPS {
            assert!(!s.title.is_empty() && s.body.len() > 40, "{}", s.title);
            assert!(!s.goals.is_empty(), "{}", s.title);
            assert!(s.body.len() < 330, "{} is too long for the card", s.title);
        }
    }

    #[test]
    fn building_a_miner_completes_the_goal() {
        let mut sim = Sim::new(1);
        let b = base(&sim);
        assert!(!Goal::Built(K::Miner).done(&facts(&sim), &live(), &b));
        let c = sim.surface(HOME).expect("home").w / 2;
        sim.apply(Command::Place {
            planet: HOME,
            x: c - 13,
            y: c + 8,
            what: Placeable::Building(K::Miner),
            dir: Dir::E,
        })
        .expect("place miner");
        assert!(Goal::Built(K::Miner).done(&facts(&sim), &live(), &b));
        // A goal is measured from the step start: a second step starting now would not count it.
        let b2 = base(&sim);
        assert!(!Goal::Built(K::Miner).done(&facts(&sim), &live(), &b2));
    }

    #[test]
    fn iron_ore_goal_needs_new_ore() {
        let mut sim = Sim::new(1);
        let b = base(&sim);
        let goal = Goal::IronOre(5);
        assert!(!goal.done(&facts(&sim), &live(), &b));
        sim.universe
            .planet_mut(HOME)
            .and_then(|p| p.colony.as_mut())
            .expect("colony")
            .inventory
            .add(Item::IronOre, 5);
        assert!(goal.done(&facts(&sim), &live(), &b));
    }

    #[test]
    fn camera_goals_measure_from_baseline() {
        let sim = Sim::new(1);
        let b = base(&sim);
        let mut l = live();
        assert!(!Goal::Pan.done(&facts(&sim), &l, &b));
        l.camera.focus = Vec3::new(10.0, 0.0, 0.0);
        l.camera.dist = 12.0;
        l.camera.yaw = 1.0;
        for g in [Goal::Pan, Goal::Zoom, Goal::Rotate] {
            assert!(g.done(&facts(&sim), &l, &b));
        }
    }

    #[test]
    fn pause_goal_needs_pause_then_resume() {
        let sim = Sim::new(1);
        let b = base(&sim);
        let mut l = live();
        l.paused = true;
        l.saw_pause = true;
        assert!(!Goal::PauseResume.done(&facts(&sim), &l, &b));
        l.paused = false;
        assert!(Goal::PauseResume.done(&facts(&sim), &l, &b));
    }

    #[test]
    fn marks_exist_on_the_home_planet() {
        let sim = Sim::new(1);
        assert!(mark_world(&sim, Mark::Iron).is_some());
        assert!(mark_world(&sim, Mark::Depot).is_some());
    }

    #[test]
    fn stepping_and_saving_progress() {
        let mut log = MessageLog::default();
        let mut t = Tutorial::default();
        t.start();
        assert_eq!(t.snapshot().step, Some(0));
        t.apply(GuideAction::Next, &mut log);
        t.apply(GuideAction::Next, &mut log);
        assert_eq!(t.snapshot().step, Some(2));
        let mut t2 = Tutorial::default();
        t2.restore(t.snapshot());
        assert!(t2.active && t2.step == 2);
        t2.apply(GuideAction::Exit, &mut log);
        assert!(!t2.active && t2.finished);
        assert_eq!(t2.snapshot().step, None);
        // Running off the end finishes the guide.
        let mut t3 = Tutorial::default();
        t3.restore(TutorialSave {
            step: Some(STEPS.len() as u32 - 1),
            finished: false,
        });
        t3.apply(GuideAction::Next, &mut log);
        assert!(!t3.active && t3.finished);
    }

    #[test]
    fn hotkey_labels_match_targets() {
        assert!(label_matches("Miner [N]", "Miner"));
        assert!(label_matches("Research lab", "Research lab"));
        assert!(!label_matches("Miner Mk2", "Miner"));
        assert!(!label_matches("Mining", "Miner"));
    }
}
