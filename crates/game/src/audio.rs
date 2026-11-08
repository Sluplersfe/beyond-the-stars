//! Procedural audio. A small offline synthesizer (band-limited oscillators, FM, state-variable
//! filters, envelopes, a Freeverb-style reverb and a ping-pong delay) renders every sound effect
//! and every music layer exactly once, on a background thread, into cached 16-bit stereo buffers.
//! Playback then only streams those buffers through Bevy, with a little per-play pitch variation.
//!
//! On top of the effects there is a generative ambient score (a seamless drone bed, overlapping
//! pad chords chosen by a random walk, occasional FM bell motifs) and a factory hum whose level
//! follows the number of working machines on the planet being viewed.
//!
//! `BTS_SFX_DUMP=<dir>` writes every rendered buffer to `<dir>/<name>.wav` for inspection.

use core::time::Duration;
use std::collections::HashMap;
use std::f32::consts::{PI, TAU};
use std::sync::{Arc, Mutex, mpsc};

use bevy::audio::{AddAudioSource, AudioSinkPlayback, ChannelCount, SampleRate, Source, Volume};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::ui::UiGlobalTransform;
use bevy::ui_widgets::{Activate, Button as WidgetButton};

use crate::settings::Settings;

pub struct SfxPlugin;

impl Plugin for SfxPlugin {
    fn build(&self, app: &mut App) {
        app.add_audio_source::<Pcm>()
            .init_resource::<SfxQueue>()
            .init_resource::<Bank>()
            .init_resource::<Director>()
            .add_systems(Startup, start_rendering)
            .add_systems(Update, toggle_sound)
            .add_systems(
                PostUpdate,
                (
                    receive_sounds,
                    ui_hover,
                    watch_state,
                    play_sfx,
                    music_director,
                    factory_hum,
                    apply_volumes,
                )
                    .chain(),
            )
            .add_observer(on_activate);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Sfx {
    /// A building was placed.
    Build,
    /// A belt tile was placed (softer than `Build`).
    Belt,
    /// Something was removed.
    Demolish,
    /// The build direction turned.
    Rotate,
    Error,
    /// Any UI button press.
    Click,
    /// The pointer moved onto a UI button.
    Hover,
    /// The active tool changed.
    Tool,
    /// A building, ship or planet was selected.
    Select,
    /// A setting, recipe, filter or order was applied.
    Confirm,
    WindowOpen,
    WindowClose,
    /// The menu flipped to another page.
    Page,
    /// Camera moved between the planet and the star maps (or to another planet).
    ViewChange,
    ResearchStart,
    Research,
    Ship,
    /// A ship was ordered to depart.
    Launch,
    Alert,
    Attack,
    Generate,
    Victory,
    Pause,
    Resume,
}

impl Sfx {
    pub const ALL: [Sfx; 24] = [
        Sfx::Build,
        Sfx::Belt,
        Sfx::Demolish,
        Sfx::Rotate,
        Sfx::Error,
        Sfx::Click,
        Sfx::Hover,
        Sfx::Tool,
        Sfx::Select,
        Sfx::Confirm,
        Sfx::WindowOpen,
        Sfx::WindowClose,
        Sfx::Page,
        Sfx::ViewChange,
        Sfx::ResearchStart,
        Sfx::Research,
        Sfx::Ship,
        Sfx::Launch,
        Sfx::Alert,
        Sfx::Attack,
        Sfx::Generate,
        Sfx::Victory,
        Sfx::Pause,
        Sfx::Resume,
    ];

    fn name(self) -> &'static str {
        match self {
            Sfx::Build => "build",
            Sfx::Belt => "belt",
            Sfx::Demolish => "demolish",
            Sfx::Rotate => "rotate",
            Sfx::Error => "error",
            Sfx::Click => "click",
            Sfx::Hover => "hover",
            Sfx::Tool => "tool",
            Sfx::Select => "select",
            Sfx::Confirm => "confirm",
            Sfx::WindowOpen => "window_open",
            Sfx::WindowClose => "window_close",
            Sfx::Page => "page",
            Sfx::ViewChange => "view_change",
            Sfx::ResearchStart => "research_start",
            Sfx::Research => "research",
            Sfx::Ship => "ship",
            Sfx::Launch => "launch",
            Sfx::Alert => "alert",
            Sfx::Attack => "attack",
            Sfx::Generate => "generate",
            Sfx::Victory => "victory",
            Sfx::Pause => "pause",
            Sfx::Resume => "resume",
        }
    }

    /// Round-robin takes rendered for high-traffic sounds, so repeats never sound machine-gunned.
    fn variants(self) -> u8 {
        match self {
            Sfx::Click | Sfx::Belt | Sfx::Build => 3,
            Sfx::Hover | Sfx::Rotate | Sfx::Demolish | Sfx::Select => 2,
            _ => 1,
        }
    }

    /// Mix level relative to the effects volume.
    fn volume(self) -> f32 {
        match self {
            Sfx::Build => 0.42,
            Sfx::Belt => 0.22,
            Sfx::Demolish => 0.34,
            Sfx::Rotate => 0.22,
            Sfx::Error => 0.30,
            Sfx::Click => 0.17,
            Sfx::Hover => 0.045,
            Sfx::Tool => 0.16,
            Sfx::Select => 0.17,
            Sfx::Confirm => 0.18,
            Sfx::WindowOpen | Sfx::WindowClose => 0.16,
            Sfx::Page => 0.13,
            Sfx::ViewChange => 0.26,
            Sfx::ResearchStart => 0.22,
            Sfx::Research => 0.36,
            Sfx::Ship => 0.38,
            Sfx::Launch => 0.34,
            Sfx::Alert => 0.30,
            Sfx::Attack => 0.38,
            Sfx::Generate => 0.17,
            Sfx::Victory => 0.48,
            Sfx::Pause | Sfx::Resume => 0.18,
        }
    }

    /// Minimum seconds between two plays.
    fn min_gap(self) -> f64 {
        match self {
            Sfx::Hover => 0.045,
            Sfx::Click | Sfx::Belt => 0.035,
            Sfx::Build | Sfx::Demolish | Sfx::Rotate => 0.06,
            Sfx::Error => 0.3,
            Sfx::Research | Sfx::Generate | Sfx::Attack => 0.5,
            Sfx::Ship | Sfx::Launch => 0.9,
            Sfx::Alert => 2.5,
            Sfx::Victory => 4.0,
            _ => 0.07,
        }
    }

    /// Per-play pitch spread (fraction).
    fn spread(self) -> f32 {
        match self {
            Sfx::Victory | Sfx::Research | Sfx::Generate | Sfx::ResearchStart => 0.0,
            Sfx::Click | Sfx::Hover | Sfx::Belt | Sfx::Rotate => 0.05,
            _ => 0.03,
        }
    }

    /// Sounds that make this one redundant when they fire in the same frame.
    fn yields_to(self, other: Sfx) -> bool {
        use Sfx::*;
        match self {
            Hover => other != Hover,
            Click => matches!(
                other,
                Tool | Select
                    | Confirm
                    | WindowOpen
                    | WindowClose
                    | Page
                    | ResearchStart
                    | ViewChange
                    | Pause
                    | Resume
                    | Rotate
                    | Launch
                    | Error
            ),
            Demolish => matches!(other, Build | Belt),
            Belt => other == Build,
            Pause | Resume => matches!(other, WindowOpen | WindowClose | ViewChange),
            Tool | Select | WindowClose | WindowOpen | Page => other == ViewChange,
            _ => false,
        }
    }
}

#[derive(Resource, Default)]
pub struct SfxQueue(pub Vec<Sfx>);

// =======================================================================================
// Playback: cached PCM buffers as a Bevy audio source
// =======================================================================================

pub const RATE: u32 = 44_100;
const SR: f32 = RATE as f32;

/// Interleaved 16-bit stereo, rendered once and shared by every play.
#[derive(Asset, TypePath, Clone)]
pub struct Pcm {
    data: Arc<[i16]>,
}

pub struct PcmDecoder {
    data: Arc<[i16]>,
    pos: usize,
}

impl Iterator for PcmDecoder {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let v = *self.data.get(self.pos)?;
        self.pos += 1;
        Some(v as f32 / 32768.0)
    }
}

impl Source for PcmDecoder {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> ChannelCount {
        ChannelCount::new(2).expect("stereo")
    }
    fn sample_rate(&self) -> SampleRate {
        SampleRate::new(RATE).expect("rate")
    }
    fn total_duration(&self) -> Option<Duration> {
        Some(Duration::from_secs_f32(self.data.len() as f32 / 2.0 / SR))
    }
}

impl Decodable for Pcm {
    type Decoder = PcmDecoder;

    fn decoder(&self) -> PcmDecoder {
        PcmDecoder {
            data: self.data.clone(),
            pos: 0,
        }
    }
}

/// Everything the renderer produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Key {
    Sfx(Sfx, u8),
    Drone,
    Hum,
    Pad(u8),
    Bells(u8),
}

const PAD_COUNT: u8 = 6;
const BELL_COUNT: u8 = 6;

impl Key {
    fn name(self) -> String {
        match self {
            Key::Sfx(s, v) => format!("sfx_{}_{v}", s.name()),
            Key::Drone => "music_drone".into(),
            Key::Hum => "ambient_hum".into(),
            Key::Pad(i) => format!("music_pad_{i}"),
            Key::Bells(i) => format!("music_bells_{i}"),
        }
    }

    /// Effects first so they are ready almost immediately; the long music layers follow.
    fn all() -> Vec<Key> {
        let mut v = Vec::new();
        for s in Sfx::ALL {
            for i in 0..s.variants() {
                v.push(Key::Sfx(s, i));
            }
        }
        v.push(Key::Drone);
        v.push(Key::Hum);
        v.push(Key::Pad(0));
        v.extend((0..BELL_COUNT).map(Key::Bells));
        v.extend((1..PAD_COUNT).map(Key::Pad));
        v
    }

    fn render(self) -> Vec<f32> {
        match self {
            Key::Sfx(s, v) => render_sfx(s, v),
            Key::Drone => render_drone(),
            Key::Hum => render_hum(),
            Key::Pad(i) => render_pad(i),
            Key::Bells(i) => render_bells(i),
        }
    }
}

#[derive(Resource, Default)]
struct Bank {
    map: HashMap<Key, Handle<Pcm>>,
}

#[derive(Resource)]
struct Incoming(Mutex<mpsc::Receiver<(Key, Vec<f32>)>>);

fn start_rendering(mut commands: Commands) {
    let (tx, rx) = mpsc::channel();
    let dump = std::env::var("BTS_SFX_DUMP").ok();
    let spawned = std::thread::Builder::new()
        .name("sfx-render".into())
        .spawn(move || {
            for key in Key::all() {
                let pcm = key.render();
                if let Some(dir) = &dump {
                    let path = std::path::Path::new(dir).join(format!("{}.wav", key.name()));
                    if let Err(e) = write_wav(&path, &pcm) {
                        eprintln!("could not write {}: {e}", path.display());
                    }
                }
                if tx.send((key, pcm)).is_err() {
                    return;
                }
            }
        });
    if let Err(e) = spawned {
        warn!("audio renderer did not start: {e}");
    }
    commands.insert_resource(Incoming(Mutex::new(rx)));
}

fn receive_sounds(
    incoming: Option<Res<Incoming>>,
    mut bank: ResMut<Bank>,
    mut assets: ResMut<Assets<Pcm>>,
) {
    let Some(incoming) = incoming else {
        return;
    };
    let Ok(rx) = incoming.0.lock() else {
        return;
    };
    while let Ok((key, pcm)) = rx.try_recv() {
        let data: Arc<[i16]> = pcm
            .iter()
            .map(|v| (v.clamp(-1.0, 1.0) * 32767.0).round() as i16)
            .collect();
        bank.map.insert(key, assets.add(Pcm { data }));
    }
}

/// Write interleaved stereo f32 samples as a 16-bit WAV file.
pub fn write_wav(path: &std::path::Path, pcm: &[f32]) -> std::io::Result<()> {
    let data_len = (pcm.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 4).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for v in pcm {
        out.extend_from_slice(&((v.clamp(-1.0, 1.0) * 32767.0).round() as i16).to_le_bytes());
    }
    std::fs::write(path, out)
}

// =======================================================================================
// Game-side systems
// =======================================================================================

/// A one-shot effect currently playing.
#[derive(Component)]
struct SfxVoice;

/// A music layer; its sink volume follows the music volume setting.
#[derive(Component)]
struct MusicVoice(f32);

/// The looping factory hum.
#[derive(Component)]
struct HumVoice;

#[derive(Resource)]
struct Director {
    rng: Rng,
    last: HashMap<Sfx, f64>,
    next_variant: HashMap<Sfx, u8>,
    drone: bool,
    hum: bool,
    pad_timer: f32,
    chord: u8,
    bell_timer: f32,
    last_bells: u8,
    hum_level: f32,
    hum_target: f32,
    hum_poll: f32,
    hover_pos: Option<(i32, i32)>,
}

impl Default for Director {
    fn default() -> Self {
        Self {
            rng: Rng::new(0x5EED_A0D1),
            last: HashMap::new(),
            next_variant: HashMap::new(),
            drone: false,
            hum: false,
            pad_timer: 1.5,
            chord: 0,
            bell_timer: 14.0,
            last_bells: BELL_COUNT,
            hum_level: 0.0,
            hum_target: 0.0,
            hum_poll: 0.0,
            hover_pos: None,
        }
    }
}

fn toggle_sound(keys: Res<ButtonInput<KeyCode>>, mut settings: ResMut<Settings>) {
    if keys.just_pressed(KeyCode::F2) {
        settings.muted = !settings.muted;
    }
}

fn on_activate(_: On<Activate>, mut queue: ResMut<SfxQueue>) {
    queue.0.push(Sfx::Click);
}

/// A soft tick when the pointer enters a button. UI panels are rebuilt often, so a button is
/// identified by where it sits on screen, and freshly spawned buttons never tick.
fn ui_hover(
    q: Query<(Ref<Hovered>, &UiGlobalTransform), With<WidgetButton>>,
    mut dir: ResMut<Director>,
    mut queue: ResMut<SfxQueue>,
) {
    for (h, t) in &q {
        if !h.is_changed() || h.is_added() {
            continue;
        }
        let p = t.translation;
        let pos = ((p.x / 4.0).round() as i32, (p.y / 4.0).round() as i32);
        if h.get() {
            if dir.hover_pos != Some(pos) {
                dir.hover_pos = Some(pos);
                queue.0.push(Sfx::Hover);
            }
        } else if dir.hover_pos == Some(pos) {
            dir.hover_pos = None;
        }
    }
}

#[derive(Default)]
struct Prev {
    init: bool,
    window: Option<crate::state::Popup>,
    tool: Option<crate::state::Tool>,
    selected: Option<crate::state::Selection>,
    view: Option<crate::state::View>,
    menu: Option<(bool, crate::menu::Page)>,
    screen: Option<crate::menu::Screen>,
    paused: bool,
}

/// Turns state transitions made anywhere in the game into interface sounds.
#[allow(clippy::too_many_arguments)]
fn watch_state(
    window: Option<Res<crate::state::OpenWindow>>,
    tool: Option<Res<crate::state::Tool>>,
    selected: Option<Res<crate::state::Selected>>,
    view: Option<Res<crate::state::View>>,
    menu: Option<Res<crate::menu::MenuState>>,
    screen: Option<Res<crate::menu::Screen>>,
    virt: Res<Time<Virtual>>,
    mut prev: Local<Prev>,
    mut queue: ResMut<SfxQueue>,
) {
    use crate::state::{Popup, Selection};
    let now = Prev {
        init: true,
        window: window.map(|w| w.0),
        tool: tool.map(|t| *t),
        selected: selected.map(|s| s.0),
        view: view.map(|v| *v),
        menu: menu.map(|m| (m.open, m.page)),
        screen: screen.map(|s| *s),
        paused: virt.is_paused(),
    };
    if !prev.init {
        *prev = now;
        return;
    }
    let q = &mut queue.0;
    if now.window != prev.window
        && let (Some(a), Some(b)) = (prev.window, now.window)
    {
        q.push(if b == Popup::None && a != Popup::None {
            Sfx::WindowClose
        } else {
            Sfx::WindowOpen
        });
    }
    if now.tool != prev.tool && prev.tool.is_some() {
        q.push(Sfx::Tool);
    }
    if now.selected != prev.selected
        && prev.selected.is_some()
        && !matches!(now.selected, Some(Selection::None) | None)
    {
        q.push(Sfx::Select);
    }
    if now.view != prev.view && prev.view.is_some()
        || now.screen != prev.screen && prev.screen.is_some()
    {
        q.push(Sfx::ViewChange);
    }
    if let (Some((was_open, was_page)), Some((open, page))) = (prev.menu, now.menu) {
        if open != was_open {
            q.push(if open {
                Sfx::WindowOpen
            } else {
                Sfx::WindowClose
            });
        } else if open && page != was_page {
            q.push(Sfx::Page);
        }
    }
    if now.paused != prev.paused {
        q.push(if now.paused { Sfx::Pause } else { Sfx::Resume });
    }
    *prev = now;
}

fn sfx_volume(base: f32, gain: f32) -> Volume {
    Volume::Linear(base * gain)
}

fn play_sfx(
    mut commands: Commands,
    mut queue: ResMut<SfxQueue>,
    settings: Res<Settings>,
    bank: Res<Bank>,
    mut dir: ResMut<Director>,
    time: Res<Time<Real>>,
    voices: Query<(), With<SfxVoice>>,
) {
    if queue.0.is_empty() {
        return;
    }
    let mut frame: Vec<Sfx> = Vec::new();
    for s in queue.0.drain(..) {
        if !frame.contains(&s) {
            frame.push(s);
        }
    }
    let gain = settings.sfx_gain();
    if gain <= 0.0 {
        return;
    }
    let now = time.elapsed_secs_f64();
    let mut active = voices.iter().count();
    let keep: Vec<Sfx> = frame
        .iter()
        .copied()
        .filter(|s| !frame.iter().any(|o| o != s && s.yields_to(*o)))
        .collect();
    for s in keep {
        let last = dir.last.get(&s).copied();
        if last.is_some_and(|t| now - t < s.min_gap()) {
            continue;
        }
        // Dense UI chatter is cheap to drop; the rest always gets through.
        let minor = matches!(s, Sfx::Hover | Sfx::Click | Sfx::Belt | Sfx::Rotate);
        if active >= 28 || (minor && active >= 16) {
            continue;
        }
        let v = dir.next_variant.get(&s).copied().unwrap_or(0) % s.variants();
        let Some(handle) = bank.map.get(&Key::Sfx(s, v)) else {
            continue;
        };
        let nv = (v + 1 + (dir.rng.f() * 1.99) as u8 % s.variants().max(1)) % s.variants();
        dir.next_variant
            .insert(s, if nv == v { (v + 1) % s.variants() } else { nv });
        let mut vol = s.volume();
        // Rapid repeats (belt drags, demolish sweeps) settle down instead of stacking up.
        if last.is_some_and(|t| now - t < 0.25) {
            vol *= 0.75;
        }
        let speed = 1.0 + dir.rng.bi() * s.spread();
        dir.last.insert(s, now);
        active += 1;
        commands.spawn((
            SfxVoice,
            AudioPlayer(handle.clone()),
            PlaybackSettings::DESPAWN
                .with_volume(sfx_volume(vol, gain))
                .with_speed(speed),
        ));
    }
}

/// Pad chords for the score; each row moves smoothly to the ones listed in `NEXT`.
const CHORDS: [[f32; 5]; PAD_COUNT as usize] = [
    // Am9
    [110.00, 164.81, 196.00, 246.94, 261.63],
    // Fmaj7#11
    [87.31, 130.81, 164.81, 220.00, 246.94],
    // Cmaj9
    [130.81, 196.00, 246.94, 293.66, 329.63],
    // G6/9
    [98.00, 146.83, 220.00, 246.94, 329.63],
    // Dm9
    [146.83, 220.00, 261.63, 329.63, 349.23],
    // Em7
    [82.41, 123.47, 146.83, 196.00, 246.94],
];
const NEXT: [[u8; 3]; PAD_COUNT as usize] = [
    [1, 4, 5],
    [2, 0, 3],
    [3, 1, 5],
    [0, 5, 2],
    [5, 1, 0],
    [0, 1, 4],
];
/// Seconds between pad entries; pads last ~20 s, so neighbours cross-fade.
const PAD_EVERY: f32 = 13.0;

fn music_director(
    mut commands: Commands,
    mut dir: ResMut<Director>,
    bank: Res<Bank>,
    settings: Res<Settings>,
    time: Res<Time<Real>>,
) {
    let gain = settings.music_gain();
    if !dir.drone
        && let Some(h) = bank.map.get(&Key::Drone)
    {
        dir.drone = true;
        commands.spawn((
            MusicVoice(0.34),
            AudioPlayer(h.clone()),
            PlaybackSettings::LOOP.with_volume(sfx_volume(0.34, gain)),
        ));
    }
    let dt = time.delta_secs().min(0.25);
    dir.pad_timer -= dt;
    if dir.pad_timer <= 0.0 {
        let options = NEXT[dir.chord as usize];
        let pick = options[(dir.rng.f() * 2.999) as usize];
        if let Some(h) = bank.map.get(&Key::Pad(pick)) {
            dir.chord = pick;
            commands.spawn((
                MusicVoice(0.26),
                AudioPlayer(h.clone()),
                PlaybackSettings::DESPAWN.with_volume(sfx_volume(0.26, gain)),
            ));
            dir.pad_timer = PAD_EVERY + dir.rng.bi() * 1.0;
        } else if let Some(h) = bank.map.get(&Key::Pad(0)) {
            // Early on only the first pad is ready.
            dir.chord = 0;
            commands.spawn((
                MusicVoice(0.26),
                AudioPlayer(h.clone()),
                PlaybackSettings::DESPAWN.with_volume(sfx_volume(0.26, gain)),
            ));
            dir.pad_timer = PAD_EVERY;
        } else {
            dir.pad_timer = 0.5;
        }
    }
    dir.bell_timer -= dt;
    if dir.bell_timer <= 0.0 {
        let mut i = (dir.rng.f() * BELL_COUNT as f32) as u8 % BELL_COUNT;
        if i == dir.last_bells {
            i = (i + 1) % BELL_COUNT;
        }
        if let Some(h) = bank.map.get(&Key::Bells(i)) {
            dir.last_bells = i;
            commands.spawn((
                MusicVoice(0.15),
                AudioPlayer(h.clone()),
                PlaybackSettings::DESPAWN.with_volume(sfx_volume(0.15, gain)),
            ));
        }
        dir.bell_timer = 22.0 + dir.rng.f() * 26.0;
    }
}

/// Count working machines on the viewed planet a few times a second and steer the hum toward it.
#[allow(clippy::too_many_arguments)]
fn factory_hum(
    mut commands: Commands,
    mut dir: ResMut<Director>,
    bank: Res<Bank>,
    time: Res<Time<Real>>,
    virt: Res<Time<Virtual>>,
    view: Option<Res<crate::state::View>>,
    sim: Option<Res<crate::state::SimState>>,
    screen: Option<Res<crate::menu::Screen>>,
) {
    if !dir.hum
        && let Some(h) = bank.map.get(&Key::Hum)
    {
        dir.hum = true;
        commands.spawn((
            HumVoice,
            AudioPlayer(h.clone()),
            PlaybackSettings::LOOP.with_volume(Volume::Linear(0.0)),
        ));
    }
    let dt = time.delta_secs().min(0.25);
    dir.hum_poll -= dt;
    if dir.hum_poll <= 0.0 {
        dir.hum_poll = 0.3;
        let playing = screen.is_none_or(|s| *s == crate::menu::Screen::Playing);
        let working = match (view.as_deref(), sim.as_deref()) {
            (Some(crate::state::View::Planet(p)), Some(sim)) if playing && !virt.is_paused() => sim
                .0
                .surface(*p)
                .map(|s| {
                    s.buildings
                        .values()
                        .filter(|b| b.status == sim::Status::Working)
                        .count()
                })
                .unwrap_or(0),
            _ => 0,
        };
        dir.hum_target = hum_level(working);
    }
    let k = 1.0 - (-dt / 1.2).exp();
    dir.hum_level += (dir.hum_target - dir.hum_level) * k;
}

/// Hum loudness for `n` working machines: audible with a handful, saturating for big bases.
fn hum_level(n: usize) -> f32 {
    if n == 0 {
        0.0
    } else {
        0.3 + 0.7 * (1.0 - (-(n as f32) / 40.0).exp())
    }
}

fn apply_volumes(
    settings: Res<Settings>,
    dir: Res<Director>,
    mut music: Query<(&MusicVoice, &mut AudioSink), Without<HumVoice>>,
    mut hum: Query<&mut AudioSink, With<HumVoice>>,
) {
    let mg = settings.music_gain();
    for (m, mut sink) in &mut music {
        let v = m.0 * mg;
        if (sink.volume().to_linear() - v).abs() > 1e-4 {
            sink.set_volume(Volume::Linear(v));
        }
    }
    let hv = 0.2 * dir.hum_level * settings.sfx_gain();
    for mut sink in &mut hum {
        if (sink.volume().to_linear() - hv).abs() > 1e-4 {
            sink.set_volume(Volume::Linear(hv));
        }
    }
}

// =======================================================================================
// Synthesis toolkit
// =======================================================================================

/// Deterministic xorshift64* generator.
#[derive(Clone)]
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        let mut r = Rng((seed ^ 0x9E37_79B9_7F4A_7C15).wrapping_mul(0xBF58_476D_1CE4_E5B9) | 1);
        for _ in 0..4 {
            r.u64();
        }
        r
    }
    fn u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// Uniform in [0, 1).
    fn f(&mut self) -> f32 {
        (self.u64() >> 40) as f32 / (1u64 << 24) as f32
    }
    /// Uniform in [-1, 1).
    fn bi(&mut self) -> f32 {
        self.f() * 2.0 - 1.0
    }
    fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.f()
    }
    fn fork(&mut self) -> Rng {
        Rng::new(self.u64())
    }
}

/// Stereo working buffer.
struct Buf {
    l: Vec<f32>,
    r: Vec<f32>,
}

impl Buf {
    fn new(secs: f32) -> Self {
        let n = (secs * SR) as usize;
        Buf {
            l: vec![0.0; n],
            r: vec![0.0; n],
        }
    }
    fn len(&self) -> usize {
        self.l.len()
    }
    fn ensure(&mut self, n: usize) {
        if self.l.len() < n {
            self.l.resize(n, 0.0);
            self.r.resize(n, 0.0);
        }
    }
    fn extend(&mut self, secs: f32) {
        let n = self.len() + (secs * SR) as usize;
        self.ensure(n);
    }
}

/// Equal-power pan gains, unity in the centre.
fn pan_gains(pan: f32) -> (f32, f32) {
    let a = (pan.clamp(-1.0, 1.0) + 1.0) * PI / 4.0;
    (
        a.cos() * std::f32::consts::SQRT_2,
        a.sin() * std::f32::consts::SQRT_2,
    )
}

/// Run `f(t)` for `dur` seconds starting at `at`, panned, mixed into `b`.
fn voice(b: &mut Buf, at: f32, dur: f32, pan: f32, mut f: impl FnMut(f32) -> f32) {
    let s0 = (at.max(0.0) * SR) as usize;
    let n = (dur * SR) as usize;
    b.ensure(s0 + n);
    let (gl, gr) = pan_gains(pan);
    for i in 0..n {
        let v = f(i as f32 / SR) * tail_gain(i, n);
        b.l[s0 + i] += v * gl;
        b.r[s0 + i] += v * gr;
    }
}

/// Like `voice` but `f` returns a stereo pair.
fn voice2(b: &mut Buf, at: f32, dur: f32, mut f: impl FnMut(f32) -> (f32, f32)) {
    let s0 = (at.max(0.0) * SR) as usize;
    let n = (dur * SR) as usize;
    b.ensure(s0 + n);
    for i in 0..n {
        let (l, r) = f(i as f32 / SR);
        let g = tail_gain(i, n);
        b.l[s0 + i] += l * g;
        b.r[s0 + i] += r * g;
    }
}

/// A 4 ms fade at the end of every voice, so envelopes cut off at a finite length never click.
fn tail_gain(i: usize, n: usize) -> f32 {
    const FADE: usize = (0.004 * SR) as usize;
    let left = n - i;
    if left >= FADE {
        1.0
    } else {
        left as f32 / FADE as f32
    }
}

fn blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let x = t / dt;
        x + x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}

/// Phase-accumulator oscillator with band-limited (polyBLEP) saw and pulse.
#[derive(Default, Clone, Copy)]
struct Osc {
    p: f32,
}

impl Osc {
    fn at(p: f32) -> Self {
        Osc {
            p: p.rem_euclid(1.0),
        }
    }
    fn tick(&mut self, f: f32) -> f32 {
        let p = self.p;
        self.p = (self.p + f.max(0.0) / SR).fract();
        p
    }
    fn sine(&mut self, f: f32) -> f32 {
        (self.tick(f) * TAU).sin()
    }
    fn saw(&mut self, f: f32) -> f32 {
        let dt = (f / SR).clamp(1e-6, 0.5);
        let p = self.tick(f);
        2.0 * p - 1.0 - blep(p, dt)
    }
    fn pulse(&mut self, f: f32, pw: f32) -> f32 {
        let dt = (f / SR).clamp(1e-6, 0.5);
        let p = self.tick(f);
        let mut v = if p < pw { 1.0 } else { -1.0 };
        v += blep(p, dt);
        v -= blep((p - pw + 1.0).fract(), dt);
        v
    }
    fn tri(&mut self, f: f32) -> f32 {
        let p = self.tick(f);
        1.0 - 4.0 * (p - 0.5).abs()
    }
    /// Phase-modulation operator pair: carrier at `f`, modulator at `f * ratio`.
    fn fm(&mut self, m: &mut Osc, f: f32, ratio: f32, index: f32) -> f32 {
        let mv = m.sine(f * ratio) * index;
        (self.tick(f) * TAU + mv).sin()
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Lp,
    Bp,
    Hp,
}

/// Topology-preserving state-variable filter (Simper). Coefficients are cached while the
/// cutoff holds still, which keeps sweeps cheap.
#[derive(Clone, Copy)]
struct Svf {
    s1: f32,
    s2: f32,
    fc: f32,
    q: f32,
    a1: f32,
    a2: f32,
    a3: f32,
    k: f32,
}

impl Default for Svf {
    fn default() -> Self {
        Svf {
            s1: 0.0,
            s2: 0.0,
            fc: -1.0,
            q: -1.0,
            a1: 0.0,
            a2: 0.0,
            a3: 0.0,
            k: 1.0,
        }
    }
}

impl Svf {
    fn run(&mut self, x: f32, fc: f32, q: f32) -> (f32, f32, f32) {
        let fc = fc.clamp(10.0, SR * 0.45);
        if (fc - self.fc).abs() > self.fc * 0.002 || q != self.q {
            self.fc = fc;
            self.q = q;
            let g = (PI * fc / SR).tan();
            self.k = 1.0 / q.max(0.05);
            self.a1 = 1.0 / (1.0 + g * (g + self.k));
            self.a2 = g * self.a1;
            self.a3 = g * self.a2;
        }
        let v3 = x - self.s2;
        let v1 = self.a1 * self.s1 + self.a2 * v3;
        let v2 = self.s2 + self.a2 * self.s1 + self.a3 * v3;
        self.s1 = 2.0 * v1 - self.s1;
        self.s2 = 2.0 * v2 - self.s2;
        (v2, v1 * self.k, x - self.k * v1 - v2)
    }
    fn tick(&mut self, mode: Mode, x: f32, fc: f32, q: f32) -> f32 {
        let (lp, bp, hp) = self.run(x, fc, q);
        match mode {
            Mode::Lp => lp,
            Mode::Bp => bp,
            Mode::Hp => hp,
        }
    }
    fn lp(&mut self, x: f32, fc: f32, q: f32) -> f32 {
        self.run(x, fc, q).0
    }
}

/// Linear attack, exponential decay.
fn perc(t: f32, a: f32, tau: f32) -> f32 {
    if t < 0.0 {
        0.0
    } else if t < a {
        t / a
    } else {
        (-(t - a) / tau).exp()
    }
}

/// Attack / exponential decay to sustain / squared release after `gate` seconds.
fn adsr(t: f32, a: f32, d: f32, s: f32, r: f32, gate: f32) -> f32 {
    let level = |t: f32| {
        if t < a {
            (t / a).sqrt()
        } else {
            s + (1.0 - s) * (-(t - a) / d).exp()
        }
    };
    if t < gate {
        level(t)
    } else {
        let x = (1.0 - (t - gate) / r).max(0.0);
        level(gate) * x * x
    }
}

/// Smooth 0→1 over [0, 1].
fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Exponential glide from `f0` toward `f1`.
fn glide(t: f32, f0: f32, f1: f32, tau: f32) -> f32 {
    f1 + (f0 - f1) * (-t / tau).exp()
}

/// Exponential sweep from `f0` to `f1` over `dur`.
fn sweep(t: f32, dur: f32, f0: f32, f1: f32) -> f32 {
    f0 * (f1 / f0).powf((t / dur).clamp(0.0, 1.0))
}

// ---- instruments --------------------------------------------------------------------

/// Sine kick with a pitch drop: the body of every impact.
fn thump(b: &mut Buf, at: f32, f0: f32, f1: f32, ptau: f32, atau: f32, gain: f32) {
    let mut o = Osc::default();
    voice(b, at, atau * 7.0 + 0.01, 0.0, |t| {
        o.sine(glide(t, f0, f1, ptau)) * perc(t, 0.0012, atau) * gain
    });
}

/// Filtered noise burst.
#[allow(clippy::too_many_arguments)]
fn noise_hit(
    b: &mut Buf,
    at: f32,
    mode: Mode,
    fc: f32,
    q: f32,
    tau: f32,
    gain: f32,
    pan: f32,
    rng: &mut Rng,
) {
    let mut r = rng.fork();
    let mut f = Svf::default();
    voice(b, at, tau * 7.0 + 0.005, pan, |t| {
        f.tick(mode, r.bi(), fc, q) * perc(t, 0.0008, tau) * gain
    });
}

/// Modal resonator bank: decaying sine partials (metal plates, struts, debris).
fn modes(b: &mut Buf, at: f32, partials: &[(f32, f32, f32)], pan: f32) {
    for &(f, g, tau) in partials {
        let mut o = Osc::default();
        voice(b, at, tau * 7.0, pan, |t| {
            o.sine(f) * perc(t, 0.0015, tau) * g
        });
    }
}

/// Two-operator FM bell.
#[allow(clippy::too_many_arguments)]
fn bell(b: &mut Buf, at: f32, f: f32, ratio: f32, index: f32, tau: f32, gain: f32, pan: f32) {
    let (mut c, mut m) = (Osc::default(), Osc::default());
    voice(b, at, tau * 6.5, pan, |t| {
        let idx = index * (0.2 + 0.8 * (-t / (tau * 0.35)).exp());
        c.fm(&mut m, f, ratio, idx) * perc(t, 0.0025, tau) * gain
    });
}

#[derive(Clone, Copy)]
enum Wave {
    Sine,
    Tri,
    Saw,
}

/// Short pitched blip sweeping `f0`→`f1`.
#[allow(clippy::too_many_arguments)]
fn blip(b: &mut Buf, at: f32, dur: f32, f0: f32, f1: f32, wave: Wave, gain: f32, pan: f32) {
    let mut o = Osc::default();
    let mut lp = Svf::default();
    voice(b, at, dur, pan, |t| {
        let f = sweep(t, dur, f0, f1);
        let s = match wave {
            Wave::Sine => o.sine(f),
            Wave::Tri => o.tri(f),
            Wave::Saw => lp.lp(o.saw(f), f * 3.0, 0.7),
        };
        let env = smooth(t / 0.003) * (1.0 - t / dur).powi(2);
        s * env * gain
    });
}

/// Band-passed noise sweep with a moving pan: whooshes, swishes, wind.
#[allow(clippy::too_many_arguments)]
fn whoosh(
    b: &mut Buf,
    at: f32,
    dur: f32,
    f0: f32,
    f1: f32,
    q: f32,
    gain: f32,
    pans: (f32, f32),
    rng: &mut Rng,
) {
    let (mut rl, mut rr) = (rng.fork(), rng.fork());
    let (mut fl, mut fr) = (Svf::default(), Svf::default());
    voice2(b, at, dur, |t| {
        let x = t / dur;
        let fc = sweep(t, dur, f0, f1);
        let env = (PI * x).sin().powf(1.5) * gain;
        let (gl, gr) = pan_gains(pans.0 + (pans.1 - pans.0) * x);
        // Mostly-correlated noise keeps the image focused while it travels.
        let n = rl.bi();
        let nl = n * 0.8 + rr.bi() * 0.2;
        let nr = n * 0.8 + rr.bi() * 0.2;
        (
            fl.tick(Mode::Bp, nl, fc, q) * env * gl,
            fr.tick(Mode::Bp, nr, fc, q) * env * gr,
        )
    });
}

/// Servo motor: buzzy saw through a resonant band, with gear flutter.
fn servo(b: &mut Buf, at: f32, dur: f32, f0: f32, f1: f32, gain: f32, pan: f32) {
    let (mut o, mut o2) = (Osc::default(), Osc::default());
    let mut bp = Svf::default();
    let mut lp = Svf::default();
    voice(b, at, dur, pan, |t| {
        let x = t / dur;
        let f = f0 + (f1 - f0) * smooth(x);
        let flutter = 1.0 + 0.35 * (TAU * 42.0 * t).sin();
        let s = o.saw(f) * 0.7 + o2.pulse(f * 2.0, 0.3) * 0.3;
        let y = bp.tick(Mode::Bp, s, f * 3.0, 2.5) * 0.7 + lp.lp(s, f * 1.5, 0.7) * 0.3;
        let env = smooth(x / 0.2) * smooth((1.0 - x) / 0.35);
        y * env * flutter * gain
    });
}

/// Random micro-impacts: crunch, crackle, debris.
#[allow(clippy::too_many_arguments)]
fn crackle(
    b: &mut Buf,
    at: f32,
    dur: f32,
    count: usize,
    fc: (f32, f32),
    gain: f32,
    decay: f32,
    rng: &mut Rng,
) {
    for _ in 0..count {
        let t = rng.f().powf(1.6) * dur;
        let g = gain * (-t / decay).exp() * rng.range(0.3, 1.0);
        let f = rng.range(fc.0, fc.1);
        let tau = rng.range(0.0015, 0.006);
        let pan = rng.bi() * 0.6;
        noise_hit(b, at + t, Mode::Bp, f, 1.4, tau, g, pan, rng);
    }
}

/// Brass-like voice: detuned saws through an envelope-driven low-pass, with a pitch scoop
/// and delayed vibrato.
fn brass(b: &mut Buf, at: f32, f: f32, gate: f32, gain: f32, pan: f32) {
    let mut o = [Osc::at(0.0), Osc::at(0.31), Osc::at(0.67)];
    let mut sub = Osc::default();
    let det = [-0.0045, 0.0, 0.0052];
    let mut lp = Svf::default();
    let mut lp2 = Svf::default();
    voice(b, at, gate + 0.6, pan, |t| {
        let scoop = 1.0 - 0.025 * (-t / 0.035).exp();
        let vib_amt = smooth((t - 0.25) / 0.5) * 0.0045;
        let ff = f * scoop * (1.0 + vib_amt * (TAU * 5.3 * t).sin());
        let mut s = 0.0;
        for (osc, d) in o.iter_mut().zip(det) {
            s += osc.saw(ff * (1.0 + d));
        }
        s = s / 3.0 + sub.pulse(ff * 0.5, 0.5) * 0.18;
        let bite = smooth(t / 0.035) * (0.45 + 0.55 * (-t / 0.22).exp());
        let fc = f * (1.3 + 6.5 * bite);
        let y = lp2.lp(lp.lp(s, fc, 0.85), fc * 1.6, 0.6);
        y * adsr(t, 0.03, 0.3, 0.72, 0.45, gate) * gain
    });
}

// ---- effects --------------------------------------------------------------------------

struct Comb {
    buf: Vec<f32>,
    i: usize,
    store: f32,
}

impl Comb {
    fn new(n: usize) -> Self {
        Comb {
            buf: vec![0.0; n],
            i: 0,
            store: 0.0,
        }
    }
    fn run(&mut self, x: f32, fb: f32, damp: f32) -> f32 {
        let y = self.buf[self.i];
        self.store = y * (1.0 - damp) + self.store * damp;
        self.buf[self.i] = x + self.store * fb;
        self.i = (self.i + 1) % self.buf.len();
        y
    }
}

struct Allpass {
    buf: Vec<f32>,
    i: usize,
}

impl Allpass {
    fn new(n: usize) -> Self {
        Allpass {
            buf: vec![0.0; n],
            i: 0,
        }
    }
    fn run(&mut self, x: f32) -> f32 {
        let b = self.buf[self.i];
        self.buf[self.i] = x + b * 0.5;
        self.i = (self.i + 1) % self.buf.len();
        b - x
    }
}

/// Stereo Freeverb-style reverb. `room` is comb feedback (0.7 small … 0.95 cavernous),
/// `damp` darkens the tail, `wet` is the return level, `tail` extends the buffer.
fn reverb(b: &mut Buf, room: f32, damp: f32, wet: f32, tail: f32) {
    const COMBS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
    const APS: [usize; 4] = [556, 441, 341, 225];
    const SPREAD: usize = 23;
    b.extend(tail);
    let mut cl: Vec<Comb> = COMBS.iter().map(|&n| Comb::new(n)).collect();
    let mut cr: Vec<Comb> = COMBS.iter().map(|&n| Comb::new(n + SPREAD)).collect();
    let mut al: Vec<Allpass> = APS.iter().map(|&n| Allpass::new(n)).collect();
    let mut ar: Vec<Allpass> = APS.iter().map(|&n| Allpass::new(n + SPREAD)).collect();
    // Pre-delay separates the dry hit from the room.
    let pre = (0.012 * SR) as usize;
    let mut line = vec![0.0f32; pre];
    let mut li = 0;
    let room = room.clamp(0.0, 0.97);
    let damp = damp.clamp(0.0, 0.95);
    // Gentle high cut on the input so tails stay soft.
    let mut tone = Svf::default();
    for i in 0..b.len() {
        let x_now = (b.l[i] + b.r[i]) * 0.015;
        let x = line[li];
        line[li] = tone.lp(x_now, 6500.0, 0.6);
        li = (li + 1) % pre;
        let mut ol = 0.0;
        let mut or = 0.0;
        for c in &mut cl {
            ol += c.run(x, room, damp);
        }
        for c in &mut cr {
            or += c.run(x, room, damp);
        }
        for a in &mut al {
            ol = a.run(ol);
        }
        for a in &mut ar {
            or = a.run(or);
        }
        b.l[i] += ol * wet * 3.0;
        b.r[i] += or * wet * 3.0;
    }
}

/// Ping-pong delay with a darkening feedback path.
fn echo(b: &mut Buf, tl: f32, tr: f32, fb: f32, wet: f32, tail: f32) {
    b.extend(tail);
    let nl = ((tl * SR) as usize).max(1);
    let nr = ((tr * SR) as usize).max(1);
    let mut dl = vec![0.0f32; nl];
    let mut dr = vec![0.0f32; nr];
    let (mut il, mut ir) = (0, 0);
    let (mut fl, mut fr) = (Svf::default(), Svf::default());
    for i in 0..b.len() {
        let yl = dl[il];
        let yr = dr[ir];
        let xl = b.l[i];
        let xr = b.r[i];
        dl[il] = (xl + xr) * 0.5 + fr.lp(yr, 3200.0, 0.6) * fb;
        dr[ir] = fl.lp(yl, 3200.0, 0.6) * fb;
        il = (il + 1) % nl;
        ir = (ir + 1) % nr;
        b.l[i] += yl * wet;
        b.r[i] += yr * wet;
    }
}

/// Fade everything out between `from` and `to` seconds and cut the rest, so long ringing
/// tails end gracefully instead of hanging around.
fn fade_tail(b: &mut Buf, from: f32, to: f32) {
    let (a, z) = ((from * SR) as usize, (to * SR) as usize);
    for i in a..b.len().min(z) {
        let g = 1.0 - smooth((i - a) as f32 / (z - a) as f32);
        b.l[i] *= g;
        b.r[i] *= g;
    }
    b.l.truncate(z);
    b.r.truncate(z);
}

/// One-pole DC blocker (~17 Hz).
fn dc_block(x: &mut [f32]) {
    let (mut x1, mut y1) = (0.0f32, 0.0f32);
    for s in x.iter_mut() {
        let y = *s - x1 + 0.9975 * y1;
        x1 = *s;
        y1 = y;
        *s = y;
    }
}

/// Soft saturation curve normalised so `sat(1) == 1`.
fn sat(x: f32, drive: f32) -> f32 {
    (x * drive).tanh() / drive.tanh()
}

/// Master a one-shot: DC block, optional soft saturation (the limiter), trim silence, fades,
/// peak-normalise to `peak`, interleave.
fn finish(mut b: Buf, peak: f32, drive: f32) -> Vec<f32> {
    dc_block(&mut b.l);
    dc_block(&mut b.r);
    let m = peak_of(&b);
    if m < 1e-9 {
        return vec![0.0; 2];
    }
    for ch in [&mut b.l, &mut b.r] {
        for s in ch.iter_mut() {
            *s = if drive > 0.0 {
                sat(*s / m, drive)
            } else {
                *s / m
            };
        }
    }
    // Trim the inaudible tail (below -76 dB) and close with a short fade.
    let thresh = 1.6e-4;
    let last = (0..b.len())
        .rev()
        .find(|&i| b.l[i].abs() > thresh || b.r[i].abs() > thresh)
        .unwrap_or(0);
    let end = (last + (0.01 * SR) as usize).min(b.len());
    b.l.truncate(end);
    b.r.truncate(end);
    let fade_in = 24.min(end);
    let fade_out = ((0.015 * SR) as usize).min(end);
    for i in 0..fade_in {
        let g = i as f32 / fade_in as f32;
        b.l[i] *= g;
        b.r[i] *= g;
    }
    for i in 0..fade_out {
        let g = i as f32 / fade_out as f32;
        b.l[end - 1 - i] *= g;
        b.r[end - 1 - i] *= g;
    }
    let m = peak_of(&b).max(1e-9);
    interleave(&b, peak / m)
}

/// Master a loop rendered `loop_s + xfade_s` long: cross-fade the overrun onto the start so
/// the end flows into the beginning, DC-block in steady state, normalise.
fn finish_loop(mut b: Buf, loop_s: f32, xfade_s: f32, peak: f32) -> Vec<f32> {
    let n = (loop_s * SR) as usize;
    let x = (xfade_s * SR) as usize;
    b.ensure(n + x);
    for ch in [&mut b.l, &mut b.r] {
        for i in 0..x {
            let w = i as f32 / x as f32;
            ch[i] = ch[i] * w + ch[n + i] * (1.0 - w);
        }
        ch.truncate(n);
        // Run the blocker over two laps and keep the second so its state wraps cleanly.
        let mut two = ch.clone();
        two.extend_from_slice(ch);
        dc_block(&mut two);
        ch.copy_from_slice(&two[n..]);
    }
    let m = peak_of(&b).max(1e-9);
    interleave(&b, peak / m)
}

fn peak_of(b: &Buf) -> f32 {
    b.l.iter()
        .chain(b.r.iter())
        .fold(0.0f32, |m, v| m.max(v.abs()))
}

fn interleave(b: &Buf, g: f32) -> Vec<f32> {
    let mut out = Vec::with_capacity(b.len() * 2);
    for i in 0..b.len() {
        out.push(b.l[i] * g);
        out.push(b.r[i] * g);
    }
    out
}

/// Frequency with a whole number of cycles in `secs`, so loops wrap without a seam.
fn cyc(f: f32, secs: f32) -> f32 {
    (f * secs).round() / secs
}

// =======================================================================================
// Sound design
// =======================================================================================

fn render_sfx(s: Sfx, v: u8) -> Vec<f32> {
    let mut rng = Rng::new(s as u64 * 7919 + v as u64 * 104_729 + 17);
    let rng = &mut rng;
    // Small per-take differences for round-robin variants.
    let k = 1.0 + (v as f32 - 1.0) * 0.045;
    let mut b = Buf::new(0.05);
    let b = &mut b;
    let (peak, drive) = match s {
        Sfx::Build => {
            // Heavy clunk as the module seats…
            thump(b, 0.0, 185.0 * k, 50.0, 0.022, 0.11, 1.0);
            noise_hit(b, 0.0, Mode::Lp, 1300.0 * k, 0.8, 0.02, 0.75, 0.0, rng);
            noise_hit(b, 0.0, Mode::Bp, 4200.0, 1.3, 0.0035, 0.45, 0.0, rng);
            modes(
                b,
                0.003,
                &[
                    (431.0 * k, 0.16, 0.09),
                    (1187.0 * k, 0.11, 0.07),
                    (1873.0 * k, 0.07, 0.05),
                    (2961.0 * k, 0.04, 0.035),
                ],
                0.0,
            );
            // …servos settle it…
            servo(b, 0.07, 0.24, 360.0 * k, 690.0 * k, 0.16, -0.25);
            // …a latch snaps shut…
            noise_hit(b, 0.29, Mode::Bp, 3400.0, 2.0, 0.003, 0.32, 0.15, rng);
            thump(b, 0.29, 320.0, 170.0, 0.01, 0.02, 0.22);
            // …and it chimes ready.
            bell(b, 0.31, 1567.98 * k, 1.41, 1.1, 0.22, 0.11, 0.25);
            bell(b, 0.31, 3135.96 * k, 1.0, 0.3, 0.08, 0.04, 0.25);
            reverb(b, 0.78, 0.5, 0.11, 0.6);
            (0.92, 1.8)
        }
        Sfx::Belt => {
            thump(b, 0.0, 150.0 * k, 75.0, 0.015, 0.035, 0.7);
            noise_hit(b, 0.0, Mode::Lp, 1900.0 * k, 0.9, 0.012, 0.6, 0.0, rng);
            noise_hit(b, 0.0, Mode::Bp, 3600.0 * k, 1.5, 0.0025, 0.4, 0.0, rng);
            modes(
                b,
                0.002,
                &[(880.0 * k, 0.07, 0.04), (2110.0 * k, 0.04, 0.03)],
                0.0,
            );
            // Roller settle.
            noise_hit(b, 0.045, Mode::Bp, 2600.0 * k, 1.8, 0.003, 0.22, 0.1, rng);
            reverb(b, 0.7, 0.5, 0.06, 0.3);
            (0.9, 1.6)
        }
        Sfx::Demolish => {
            servo(b, 0.0, 0.22, 680.0 * k, 260.0 * k, 0.18, 0.2);
            thump(b, 0.07, 130.0 * k, 38.0, 0.03, 0.13, 1.0);
            noise_hit(b, 0.07, Mode::Lp, 700.0, 0.8, 0.06, 0.6, 0.0, rng);
            crackle(b, 0.07, 0.32, 26, (700.0, 4200.0), 0.55, 0.16, rng);
            modes(
                b,
                0.075,
                &[
                    (317.0 * k, 0.1, 0.07),
                    (743.0, 0.07, 0.06),
                    (1291.0, 0.05, 0.05),
                ],
                -0.1,
            );
            // Digital de-rez sweep.
            blip(b, 0.02, 0.28, 1300.0 * k, 260.0, Wave::Saw, 0.12, -0.3);
            reverb(b, 0.8, 0.5, 0.14, 0.6);
            (0.92, 1.8)
        }
        Sfx::Rotate => {
            for (i, g) in [0.55f32, 0.42, 0.6].iter().enumerate() {
                let t = i as f32 * 0.038;
                noise_hit(
                    b,
                    t,
                    Mode::Bp,
                    (3000.0 + 300.0 * i as f32) * k,
                    2.2,
                    0.0028,
                    *g,
                    0.0,
                    rng,
                );
                thump(b, t, 520.0 * k, 300.0, 0.006, 0.012, g * 0.35);
            }
            blip(b, 0.0, 0.11, 620.0 * k, 940.0 * k, Wave::Tri, 0.16, 0.0);
            reverb(b, 0.7, 0.5, 0.06, 0.25);
            (0.85, 1.4)
        }
        Sfx::Error => {
            for (i, f) in [196.0f32, 164.81].iter().enumerate() {
                let at = i as f32 * 0.17;
                let (mut a, mut c, mut sub) = (Osc::default(), Osc::at(0.25), Osc::default());
                let mut lp = Svf::default();
                let f = *f;
                voice(b, at, 0.2, 0.0, |t| {
                    let buzz = 0.75 + 0.25 * (TAU * 32.0 * t).sin();
                    let s = a.pulse(f, 0.5) * 0.55 + c.saw(f * 1.003) * 0.45;
                    let y = lp.lp(s, 950.0, 0.9) * buzz + sub.sine(f * 0.5) * 0.35;
                    y * adsr(t, 0.008, 0.1, 0.7, 0.05, 0.14)
                });
            }
            reverb(b, 0.7, 0.6, 0.05, 0.25);
            (0.85, 1.5)
        }
        Sfx::Click => {
            let mut o = Osc::default();
            let mut body = Osc::default();
            voice(b, 0.0, 0.06, 0.0, |t| {
                o.sine(glide(t, 2100.0 * k, 1250.0 * k, 0.004)) * perc(t, 0.0004, 0.011)
                    + body.sine(720.0 * k) * perc(t, 0.0008, 0.009) * 0.3
            });
            noise_hit(b, 0.0, Mode::Hp, 5200.0, 0.7, 0.0015, 0.35, 0.0, rng);
            (0.85, 1.2)
        }
        Sfx::Hover => {
            let mut o = Osc::default();
            voice(b, 0.0, 0.04, 0.0, |t| {
                o.sine(3100.0 * k) * perc(t, 0.0008, 0.0055)
            });
            (0.6, 0.0)
        }
        Sfx::Tool => {
            blip(b, 0.0, 0.06, 1046.5, 1046.5, Wave::Tri, 0.7, -0.1);
            blip(b, 0.05, 0.1, 1568.0, 1568.0, Wave::Tri, 0.6, 0.1);
            bell(b, 0.05, 1568.0, 2.0, 0.4, 0.05, 0.25, 0.1);
            reverb(b, 0.75, 0.5, 0.12, 0.3);
            (0.8, 0.0)
        }
        Sfx::Select => {
            bell(b, 0.0, 1318.5 * k, 2.0, 0.9, 0.08, 0.8, 0.0);
            bell(b, 0.0, 2637.0 * k, 1.0, 0.2, 0.035, 0.25, 0.0);
            noise_hit(b, 0.0, Mode::Hp, 6000.0, 0.7, 0.002, 0.15, 0.0, rng);
            reverb(b, 0.78, 0.45, 0.12, 0.35);
            (0.8, 0.0)
        }
        Sfx::Confirm => {
            bell(b, 0.0, 1046.5, 2.0, 0.7, 0.07, 0.7, -0.15);
            bell(b, 0.075, 1568.0, 2.0, 0.7, 0.1, 0.7, 0.15);
            reverb(b, 0.78, 0.45, 0.13, 0.4);
            (0.8, 0.0)
        }
        Sfx::WindowOpen => {
            whoosh(b, 0.0, 0.22, 450.0, 3400.0, 1.4, 0.55, (-0.3, 0.3), rng);
            blip(b, 0.0, 0.16, 520.0, 900.0, Wave::Sine, 0.45, 0.0);
            noise_hit(b, 0.17, Mode::Bp, 4200.0, 1.6, 0.002, 0.35, 0.0, rng);
            bell(b, 0.17, 1760.0, 2.0, 0.3, 0.035, 0.18, 0.0);
            reverb(b, 0.75, 0.5, 0.1, 0.3);
            (0.8, 0.0)
        }
        Sfx::WindowClose => {
            whoosh(b, 0.0, 0.2, 2800.0, 520.0, 1.4, 0.5, (0.3, -0.3), rng);
            blip(b, 0.0, 0.15, 820.0, 450.0, Wave::Sine, 0.45, 0.0);
            thump(b, 0.14, 260.0, 160.0, 0.012, 0.016, 0.4);
            noise_hit(b, 0.14, Mode::Bp, 2400.0, 1.6, 0.002, 0.25, 0.0, rng);
            reverb(b, 0.75, 0.5, 0.1, 0.3);
            (0.8, 0.0)
        }
        Sfx::Page => {
            whoosh(b, 0.0, 0.16, 1200.0, 4200.0, 1.0, 0.6, (-0.2, 0.2), rng);
            noise_hit(b, 0.12, Mode::Bp, 3000.0, 1.4, 0.003, 0.25, 0.0, rng);
            reverb(b, 0.72, 0.5, 0.08, 0.25);
            (0.75, 0.0)
        }
        Sfx::ViewChange => {
            let (mut rl, mut rr) = (rng.fork(), rng.fork());
            let (mut fl, mut fr) = (Svf::default(), Svf::default());
            let dur = 1.15;
            voice2(b, 0.0, dur, |t| {
                let x = t / dur;
                let fc = if x < 0.4 {
                    sweep(x, 0.4, 180.0, 2600.0)
                } else {
                    sweep(x - 0.4, 0.6, 2600.0, 260.0)
                };
                let env = smooth(x / 0.35) * smooth((1.0 - x) / 0.6) * 0.8;
                let (gl, gr) = pan_gains(-0.6 + 1.2 * smooth(x));
                let n = rl.bi();
                (
                    fl.lp(n * 0.85 + rr.bi() * 0.15, fc, 1.1) * env * gl,
                    fr.lp(n * 0.85 + rr.bi() * 0.15, fc, 1.1) * env * gr,
                )
            });
            let (mut sub, mut warp) = (Osc::default(), Osc::default());
            let mut lp = Svf::default();
            voice(b, 0.0, 1.1, 0.0, |t| {
                let x = t / 1.1;
                let env = smooth(x / 0.3) * smooth((1.0 - x) / 0.5);
                (sub.sine(sweep(t, 1.1, 95.0, 42.0)) * 0.5
                    + lp.lp(warp.saw(sweep(t, 1.1, 110.0, 55.0)), 420.0, 0.8) * 0.18)
                    * env
            });
            bell(b, 0.38, 2093.0, 1.5, 0.5, 0.18, 0.05, 0.4);
            reverb(b, 0.88, 0.5, 0.24, 1.2);
            (0.85, 1.3)
        }
        Sfx::ResearchStart => {
            for (i, f) in [880.0f32, 1108.73, 1318.51, 1760.0].iter().enumerate() {
                let at = i as f32 * 0.058;
                let pan = -0.4 + 0.27 * i as f32;
                bell(b, at, *f, 1.0, 0.8, 0.05, 0.55, pan);
                bell(b, at, f * 2.0, 3.0, 0.3, 0.02, 0.15, pan);
            }
            echo(b, 0.115, 0.172, 0.35, 0.3, 0.6);
            reverb(b, 0.8, 0.45, 0.14, 0.6);
            (0.8, 0.0)
        }
        Sfx::Research => {
            // A6add9 strum of FM bells, doubled a few cents apart for shimmer.
            let notes = [440.0f32, 659.26, 880.0, 1108.73, 987.77, 1318.51];
            for (i, f) in notes.iter().enumerate() {
                let at = i as f32 * 0.04;
                let pan = (i as f32 / 5.0 - 0.5) * 1.2;
                bell(b, at, *f, 3.5, 1.5, 1.3, 0.3, pan);
                bell(b, at + 0.004, f * 1.0035, 1.0, 0.6, 1.6, 0.22, -pan);
                bell(b, at, f * 2.0, 2.0, 0.4, 0.4, 0.06, pan * 0.5);
            }
            // Warm root under the chord.
            let mut o = Osc::default();
            voice(b, 0.0, 3.0, 0.0, |t| {
                o.sine(220.0) * adsr(t, 0.04, 1.0, 0.5, 1.4, 1.4) * 0.35
            });
            echo(b, 0.21, 0.32, 0.3, 0.18, 1.0);
            reverb(b, 0.9, 0.35, 0.42, 2.6);
            fade_tail(b, 3.8, 6.0);
            (0.85, 0.0)
        }
        Sfx::Ship => {
            rocket(b, 0.0, 3.0, 1.1, rng);
            whoosh(b, 1.4, 1.3, 380.0, 2600.0, 1.2, 0.35, (-0.6, 0.7), rng);
            reverb(b, 0.86, 0.5, 0.24, 1.6);
            (0.88, 1.6)
        }
        Sfx::Launch => {
            thump(b, 0.0, 150.0, 45.0, 0.03, 0.16, 1.0);
            noise_hit(b, 0.0, Mode::Lp, 2600.0, 0.8, 0.05, 0.7, 0.0, rng);
            rocket(b, 0.05, 1.8, 0.6, rng);
            whoosh(b, 0.6, 1.0, 500.0, 3200.0, 1.3, 0.3, (0.0, 0.6), rng);
            reverb(b, 0.85, 0.5, 0.22, 1.4);
            (0.88, 1.6)
        }
        Sfx::Alert => {
            for i in 0..2 {
                let at = i as f32 * 0.62;
                let (mut a, mut c) = (Osc::default(), Osc::at(0.5));
                let (mut bp, mut lp) = (Svf::default(), Svf::default());
                voice(b, at, 0.5, 0.0, |t| {
                    let f = 300.0 + 75.0 * smooth(t / 0.09);
                    let s = a.saw(f) * 0.6 + c.pulse(f * 1.005, 0.4) * 0.4;
                    let y = bp.tick(Mode::Bp, s, 950.0, 1.3) * 0.7 + lp.lp(s, 2300.0, 0.7) * 0.35;
                    y * adsr(t, 0.018, 0.2, 0.85, 0.07, 0.4)
                });
            }
            reverb(b, 0.74, 0.5, 0.16, 0.6);
            (0.85, 1.4)
        }
        Sfx::Attack => {
            // Laser: a fast falling FM chirp.
            let (mut c, mut m, mut s2) = (Osc::default(), Osc::default(), Osc::default());
            let mut lp = Svf::default();
            voice(b, 0.0, 0.3, -0.3, |t| {
                let f = glide(t, 3200.0, 170.0, 0.05);
                let idx = 2.6 * (-t / 0.08).exp();
                let y =
                    c.fm(&mut m, f, 0.5, idx) * 0.7 + lp.lp(s2.saw(f * 1.01), 3200.0, 0.7) * 0.3;
                y * perc(t, 0.002, 0.08)
            });
            // Impact.
            thump(b, 0.11, 115.0, 36.0, 0.035, 0.17, 1.0);
            let mut f = Svf::default();
            let mut r = rng.fork();
            voice(b, 0.11, 0.6, 0.15, |t| {
                f.lp(r.bi(), glide(t, 3200.0, 380.0, 0.06), 0.8) * perc(t, 0.001, 0.09) * 0.8
            });
            crackle(b, 0.12, 0.45, 18, (900.0, 5000.0), 0.4, 0.2, rng);
            modes(
                b,
                0.11,
                &[
                    (297.0, 0.08, 0.1),
                    (741.0, 0.05, 0.08),
                    (1307.0, 0.04, 0.06),
                ],
                0.1,
            );
            reverb(b, 0.83, 0.5, 0.2, 0.9);
            (0.9, 2.0)
        }
        Sfx::Generate => {
            let notes = [
                659.26f32, 739.99, 830.61, 987.77, 1108.73, 1318.51, 1479.98, 1661.22, 1975.53,
            ];
            for (i, f) in notes.iter().enumerate() {
                let at = i as f32 * 0.052;
                let pan = -0.7 + 1.4 * i as f32 / 8.0;
                bell(b, at, *f, 2.0, 1.0, 0.22, 0.5, pan);
                bell(b, at, f * 3.0, 1.0, 0.2, 0.05, 0.12, pan);
            }
            echo(b, 0.18, 0.27, 0.42, 0.32, 1.0);
            reverb(b, 0.88, 0.4, 0.3, 1.4);
            (0.82, 0.0)
        }
        Sfx::Victory => {
            // Pick-up G–C–E, then a held C major chord over a timpani hit and cymbal swell.
            for (i, f) in [392.0f32, 523.25, 659.26].iter().enumerate() {
                let at = i as f32 * 0.17;
                brass(b, at, *f, 0.13, 0.45, (i as f32 - 1.0) * 0.2);
            }
            let chord = [
                (130.81f32, 0.0f32),
                (261.63, -0.35),
                (392.0, 0.35),
                (523.25, -0.15),
                (659.26, 0.2),
                (783.99, 0.0),
            ];
            for (f, pan) in chord {
                brass(b, 0.54, f, 2.5, 0.3, pan);
            }
            thump(b, 0.54, 100.0, 82.0, 0.25, 0.6, 0.9);
            noise_hit(b, 0.54, Mode::Lp, 420.0, 0.8, 0.05, 0.5, 0.0, rng);
            let mut hp = Svf::default();
            let mut r = rng.fork();
            voice(b, 0.0, 3.4, 0.0, |t| {
                let env = if t < 0.54 {
                    smooth(t / 0.54).powi(2)
                } else {
                    (-(t - 0.54) / 1.0).exp()
                };
                hp.tick(Mode::Hp, r.bi(), 6500.0, 0.7) * env * 0.14
            });
            reverb(b, 0.9, 0.45, 0.3, 2.8);
            (0.9, 1.3)
        }
        Sfx::Pause => {
            power_sweep(b, 640.0, 210.0);
            (0.8, 0.0)
        }
        Sfx::Resume => {
            power_sweep(b, 230.0, 690.0);
            (0.8, 0.0)
        }
    };
    let mut out = Buf::new(0.0);
    std::mem::swap(&mut out, b);
    finish(out, peak, drive)
}

/// Rocket engine: swelling low rumble, rising engine tone, crackle.
fn rocket(b: &mut Buf, at: f32, dur: f32, rise: f32, rng: &mut Rng) {
    let (mut rl, mut rr) = (rng.fork(), rng.fork());
    let (mut fl, mut fr) = (Svf::default(), Svf::default());
    let (mut e1, mut e2, mut sub) = (Osc::default(), Osc::at(0.3), Osc::default());
    let mut lp = Svf::default();
    let env = |t: f32| smooth(t / rise) * smooth((dur - t) / (dur - rise).max(0.1));
    voice2(b, at, dur, |t| {
        let e = env(t);
        let fc = 90.0 + 900.0 * e;
        let n = rl.bi();
        let jitter = 0.8 + 0.2 * rr.bi();
        let tone = lp.lp(
            e1.saw(sweep(t, dur, 55.0, 120.0)) + e2.saw(sweep(t, dur, 55.4, 121.0)),
            420.0 + 300.0 * e,
            0.9,
        ) * 0.22;
        let s = sub.sine(sweep(t, dur, 36.0, 52.0)) * 0.5 * jitter;
        (
            (fl.lp(n * 0.8 + rr.bi() * 0.2, fc, 0.9) + tone + s) * e,
            (fr.lp(n * 0.8 + rr.bi() * 0.2, fc, 0.9) + tone + s) * e,
        )
    });
    let count = (dur * 30.0) as usize;
    for _ in 0..count {
        let t = rng.f() * dur;
        let g = 0.25 * env(t);
        let f = rng.range(900.0, 2600.0);
        let pan = rng.bi() * 0.5;
        noise_hit(b, at + t, Mode::Bp, f, 1.2, 0.002, g, pan, rng);
    }
}

/// Power-down / power-up tone for pausing and resuming time.
fn power_sweep(b: &mut Buf, f0: f32, f1: f32) {
    let (mut a, mut c) = (Osc::default(), Osc::default());
    let mut lp = Svf::default();
    let dur = 0.32;
    voice(b, 0.0, dur, 0.0, |t| {
        let x = t / dur;
        let f = sweep(t, dur, f0, f1);
        let s = a.tri(f) * 0.7 + lp.lp(c.saw(f * 0.5), f * 2.0, 0.8) * 0.3;
        s * smooth(x / 0.06) * smooth((1.0 - x) / 0.5)
    });
    bell(
        b,
        if f1 > f0 { 0.26 } else { 0.0 },
        f0.max(f1) * 2.0,
        2.0,
        0.4,
        0.06,
        0.15,
        0.0,
    );
    reverb(b, 0.78, 0.5, 0.12, 0.4);
}

// ---- music ----------------------------------------------------------------------------

const DRONE_LOOP: f32 = 32.0;
const DRONE_XFADE: f32 = 6.0;

/// Seamless bed: low A/E drones with slow beating, and filtered "solar wind".
fn render_drone() -> Vec<f32> {
    let total = DRONE_LOOP + DRONE_XFADE;
    let mut b = Buf::new(total);
    let mut rng = Rng::new(0xD20E);
    let l = DRONE_LOOP;
    let lfo = |t: f32, k: f32, ph: f32| 0.5 + 0.5 * (TAU * k * t / l + ph).sin();
    let tones = [
        (cyc(55.0, l), 0.42, 1.0, 0.0),
        (cyc(55.0, l) + 2.0 / l, 0.2, 2.0, 1.3),
        (cyc(82.41, l), 0.22, 3.0, 2.1),
        (cyc(110.0, l) + 3.0 / l, 0.12, 2.0, 0.4),
        (cyc(164.81, l), 0.06, 5.0, 4.0),
    ];
    let mut osc: Vec<Osc> = tones.iter().map(|_| Osc::default()).collect();
    let mut warm = Osc::default();
    let mut warm_lp = Svf::default();
    let (mut nl, mut nr) = (rng.fork(), rng.fork());
    let (mut wl, mut wr) = (Svf::default(), Svf::default());
    let (mut al, mut ar) = (Svf::default(), Svf::default());
    voice2(&mut b, 0.0, total, |t| {
        let mut c = 0.0;
        for (o, &(f, g, k, ph)) in osc.iter_mut().zip(&tones) {
            c += o.sine(f) * g * (0.55 + 0.45 * lfo(t, k, ph));
        }
        c += warm_lp.lp(warm.saw(cyc(55.0, l)), 160.0 + 90.0 * lfo(t, 1.0, 0.7), 0.8) * 0.18;
        let wind_f = 420.0 * (1.0 + 0.7 * lfo(t, 2.0, 0.0));
        let wind_g = 0.3 * (0.4 + 0.6 * lfo(t, 3.0, 1.0));
        let air_f = 2400.0 * (1.0 + 0.35 * lfo(t, 3.0, 2.0));
        let air_g = 0.06 * lfo(t, 5.0, 0.5);
        let (a, bb) = (nl.bi(), nr.bi());
        let l_out = c
            + wl.tick(Mode::Bp, a, wind_f, 1.6) * wind_g
            + al.tick(Mode::Bp, a, air_f * 1.03, 5.0) * air_g;
        let r_out = c
            + wr.tick(Mode::Bp, bb, wind_f * 1.07, 1.6) * wind_g
            + ar.tick(Mode::Bp, bb, air_f, 5.0) * air_g;
        (l_out, r_out)
    });
    finish_loop(b, DRONE_LOOP, DRONE_XFADE, 0.85)
}

const HUM_LOOP: f32 = 6.0;
const HUM_XFADE: f32 = 1.0;

/// Seamless factory ambience: mains hum, rhythmic machinery, distant clanks and a servo whine.
fn render_hum() -> Vec<f32> {
    let total = HUM_LOOP + HUM_XFADE;
    let l = HUM_LOOP;
    let mut b = Buf::new(total);
    let mut rng = Rng::new(0x4B11);
    let hum = [
        (cyc(50.0, l), 0.45),
        (cyc(100.0, l), 0.3),
        (cyc(150.0, l), 0.12),
        (cyc(200.0, l), 0.06),
    ];
    let mut osc = [Osc::default(); 4];
    let mut whine = Osc::default();
    let (mut nl, mut nr) = (rng.fork(), rng.fork());
    let (mut fl, mut fr) = (Svf::default(), Svf::default());
    let (mut gl, mut gr) = (Svf::default(), Svf::default());
    voice2(&mut b, 0.0, total, |t| {
        let mut c = 0.0;
        for (o, &(f, g)) in osc.iter_mut().zip(&hum) {
            c += o.sine(f) * g;
        }
        // Conveyor rhythm at 2 Hz, with a lighter off-beat.
        let beat = (TAU * 2.0 * t).sin().max(0.0).powi(3)
            + 0.4 * (TAU * 4.0 * t + 1.0).sin().max(0.0).powi(4);
        let mach = 0.25 + 0.5 * beat;
        let w = whine.sine(cyc(587.0, l) * (1.0 + 0.003 * (TAU * t / l).sin())) * 0.025;
        let (a, bb) = (nl.bi(), nr.bi());
        let l_out =
            c + fl.lp(a, 260.0, 0.8) * mach + gl.tick(Mode::Bp, a, 1400.0, 3.0) * 0.05 * beat + w;
        let r_out =
            c + fr.lp(bb, 280.0, 0.8) * mach + gr.tick(Mode::Bp, bb, 1500.0, 3.0) * 0.05 * beat + w;
        (l_out, r_out)
    });
    // Distant clanks, kept clear of the cross-fade region.
    for _ in 0..6 {
        let t = rng.range(HUM_XFADE + 0.05, l - 0.4);
        let f = rng.range(500.0, 1400.0);
        let pan = rng.bi() * 0.8;
        noise_hit(&mut b, t, Mode::Bp, f * 2.0, 2.0, 0.01, 0.12, pan, &mut rng);
        modes(&mut b, t, &[(f, 0.05, 0.05), (f * 2.76, 0.03, 0.03)], pan);
    }
    finish_loop(b, HUM_LOOP, HUM_XFADE, 0.85)
}

/// Pad layer: five-note chord of detuned saws, dark breathing low-pass, long swell, big hall.
fn render_pad(i: u8) -> Vec<f32> {
    let chord = CHORDS[i as usize % CHORDS.len()];
    let mut rng = Rng::new(0x9AD0 + i as u64);
    let len = 20.0;
    let (attack, gate, release) = (6.0, 13.0, 7.0);
    let mut b = Buf::new(len);
    struct NoteV {
        f: f32,
        saws: [Osc; 4],
        det: [f32; 4],
        tri: Osc,
        rate: f32,
        ph: f32,
    }
    let mut notes: Vec<NoteV> = chord
        .iter()
        .map(|&f| NoteV {
            f,
            saws: [
                Osc::at(rng.f()),
                Osc::at(rng.f()),
                Osc::at(rng.f()),
                Osc::at(rng.f()),
            ],
            det: [
                -0.0052 + rng.bi() * 0.001,
                0.0023 + rng.bi() * 0.001,
                -0.0021 + rng.bi() * 0.001,
                0.0055 + rng.bi() * 0.001,
            ],
            tri: Osc::at(rng.f()),
            rate: rng.range(0.05, 0.13),
            ph: rng.f() * TAU,
        })
        .collect();
    let norm = 1.0 / (chord.len() as f32).sqrt();
    let mut sub = Osc::default();
    let (mut fl, mut fr) = (Svf::default(), Svf::default());
    let lfo_ph = rng.f() * TAU;
    voice2(&mut b, 0.0, len, |t| {
        let env = if t < attack {
            smooth(t / attack)
        } else if t < gate {
            1.0
        } else {
            smooth(1.0 - (t - gate) / release)
        };
        let (mut l, mut r, mut c) = (0.0, 0.0, 0.0);
        for n in notes.iter_mut() {
            let breathe = 0.75 + 0.25 * (TAU * n.rate * t + n.ph).sin();
            let g = norm * breathe;
            l += (n.saws[0].saw(n.f * (1.0 + n.det[0]))
                + n.saws[1].saw(n.f * (1.0 + n.det[1])) * 0.8)
                * g;
            r += (n.saws[2].saw(n.f * (1.0 + n.det[2]))
                + n.saws[3].saw(n.f * (1.0 + n.det[3])) * 0.8)
                * g;
            c += n.tri.tri(n.f) * g * 0.6;
        }
        let swell = (PI * (t / len)).sin();
        let fc = 320.0 + 820.0 * swell * (0.75 + 0.25 * (TAU * 0.09 * t + lfo_ph).sin());
        let s = sub.sine(chord[0]) * 0.45;
        (
            (fl.lp(l, fc, 0.75) + c + s) * env,
            (fr.lp(r, fc * 1.04, 0.75) + c + s) * env,
        )
    });
    reverb(&mut b, 0.92, 0.55, 0.5, 4.0);
    finish(b, 0.85, 0.0)
}

/// A short bell motif from A minor pentatonic with echoes and a long hall.
fn render_bells(i: u8) -> Vec<f32> {
    const SCALE: [f32; 9] = [
        440.0, 523.25, 587.33, 659.26, 783.99, 880.0, 1046.5, 1174.66, 1318.51,
    ];
    let mut rng = Rng::new(0xBE11 + i as u64 * 31);
    let mut b = Buf::new(1.0);
    let count = 3 + (rng.f() * 2.99) as usize;
    let mut idx = (rng.f() * 6.0) as i32;
    let mut t = 0.0;
    let (ratio, index) = match i % 3 {
        0 => (3.5, 1.0),
        1 => (1.4, 0.8),
        _ => (2.0, 1.2),
    };
    for _ in 0..count {
        let f = SCALE[idx.clamp(0, SCALE.len() as i32 - 1) as usize];
        let pan = rng.bi() * 0.6;
        let g = rng.range(0.6, 1.0);
        bell(&mut b, t, f, ratio, index, 1.5, 0.5 * g, pan);
        bell(&mut b, t, f * 0.5, 1.0, 0.3, 1.8, 0.18 * g, pan * 0.5);
        t += rng.range(0.32, 0.8);
        let step = if rng.f() < 0.6 { 1 } else { 2 };
        idx += if rng.f() < 0.55 { step } else { -step };
        idx = idx.clamp(0, SCALE.len() as i32 - 1);
    }
    echo(&mut b, 0.42, 0.63, 0.42, 0.32, 3.0);
    reverb(&mut b, 0.93, 0.4, 0.55, 5.0);
    finish(b, 0.85, 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(name: &str, pcm: &[f32], min_s: f32, max_s: f32) {
        assert!(pcm.len().is_multiple_of(2), "{name}: not stereo");
        let secs = pcm.len() as f32 / 2.0 / SR;
        assert!(
            secs >= min_s && secs <= max_s,
            "{name}: duration {secs:.2}s"
        );
        assert!(pcm.iter().all(|v| v.is_finite()), "{name}: NaN/inf");
        let peak = pcm.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak <= 0.99, "{name}: peak {peak}");
        let rms = (pcm.iter().map(|v| v * v).sum::<f32>() / pcm.len() as f32).sqrt();
        assert!(rms > 0.004, "{name}: silent (rms {rms})");
        for ch in 0..2 {
            let mean = pcm.iter().skip(ch).step_by(2).sum::<f32>() / (pcm.len() / 2) as f32;
            assert!(mean.abs() < 0.01, "{name}: DC offset {mean}");
        }
        if let Ok(dir) = std::env::var("BTS_SFX_DUMP") {
            let _ = std::fs::create_dir_all(&dir);
            write_wav(&std::path::Path::new(&dir).join(format!("{name}.wav")), pcm).expect("wav");
        }
    }

    #[test]
    fn every_effect_renders_cleanly() {
        for s in Sfx::ALL {
            for v in 0..s.variants() {
                let key = Key::Sfx(s, v);
                check(&key.name(), &key.render(), 0.02, 8.0);
            }
        }
    }

    #[test]
    fn every_music_layer_renders_cleanly() {
        for key in [
            Key::Drone,
            Key::Hum,
            Key::Pad(0),
            Key::Pad(3),
            Key::Bells(0),
            Key::Bells(4),
        ] {
            check(&key.name(), &key.render(), 3.0, 40.0);
        }
    }

    #[test]
    fn rendering_is_deterministic() {
        assert_eq!(render_sfx(Sfx::Build, 1), render_sfx(Sfx::Build, 1));
    }

    #[test]
    fn loops_wrap_without_a_click() {
        for (name, pcm) in [("drone", render_drone()), ("hum", render_hum())] {
            let n = pcm.len();
            // Compare the jump across the seam with typical sample-to-sample motion.
            let seam = (pcm[0] - pcm[n - 2]).abs().max((pcm[1] - pcm[n - 1]).abs());
            let typical = pcm
                .windows(3)
                .step_by(2)
                .map(|w| (w[2] - w[0]).abs())
                .fold(0.0f32, f32::max);
            assert!(seam <= typical, "{name}: seam {seam} vs max step {typical}");
            assert_eq!(
                n,
                ((if name == "drone" {
                    DRONE_LOOP
                } else {
                    HUM_LOOP
                } * SR) as usize)
                    * 2
            );
        }
    }

    #[test]
    fn same_frame_rules_drop_redundant_sounds() {
        assert!(Sfx::Click.yields_to(Sfx::WindowOpen));
        assert!(Sfx::Demolish.yields_to(Sfx::Belt));
        assert!(!Sfx::Build.yields_to(Sfx::Click));
        assert!(Sfx::Hover.yields_to(Sfx::Click));
    }

    #[test]
    fn hum_follows_machine_count() {
        assert_eq!(hum_level(0), 0.0);
        assert!(hum_level(5) < hum_level(50));
        assert!(hum_level(10_000) <= 1.0);
    }
}
