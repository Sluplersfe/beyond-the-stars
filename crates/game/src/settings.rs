//! Player settings: persisted as JSON and applied to the window, audio, camera and renderer.

use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::window::{MonitorSelection, PresentMode, PrimaryWindow, WindowMode};
use serde::{Deserialize, Serialize};

use crate::camera::MainCamera;
use crate::paths;

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FrameTimeDiagnosticsPlugin::default())
            .init_resource::<SaveTimer>()
            .add_systems(Startup, spawn_fps)
            .add_systems(
                Update,
                (
                    apply_window,
                    apply_ui_scale,
                    apply_graphics,
                    update_fps,
                    persist,
                ),
            );
    }
}

#[derive(Resource, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// 0..=1
    pub master_volume: f32,
    /// 0..=1
    pub sfx_volume: f32,
    /// 0..=1, the ambient score.
    pub music_volume: f32,
    pub muted: bool,
    pub fullscreen: bool,
    pub vsync: bool,
    /// 0.7..=1.5
    pub ui_scale: f32,
    pub bloom: bool,
    pub shadows: bool,
    /// Pan speed multiplier, 0.5..=2.
    pub camera_speed: f32,
    pub invert_zoom: bool,
    /// Minutes between autosaves; 0 turns them off.
    pub autosave_minutes: u32,
    /// Offer the tutorial when starting a new game.
    pub offer_tutorial: bool,
    pub show_fps: bool,
    /// Last multiplayer server address, player name and the private key that marks rooms as yours.
    pub mp_server: String,
    pub mp_name: String,
    pub mp_key: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            master_volume: 0.8,
            sfx_volume: 0.8,
            music_volume: 0.6,
            muted: false,
            fullscreen: false,
            vsync: true,
            ui_scale: 1.0,
            bloom: true,
            shadows: true,
            camera_speed: 1.0,
            invert_zoom: false,
            autosave_minutes: 5,
            offer_tutorial: true,
            show_fps: false,
            mp_server: String::new(),
            mp_name: String::new(),
            mp_key: String::new(),
        }
    }
}

pub const AUTOSAVE_CHOICES: [u32; 5] = [0, 2, 5, 10, 20];

impl Settings {
    /// Pull out-of-range values (hand-edited files) back into range.
    pub fn normalized(mut self) -> Self {
        let d = Settings::default();
        let fix = |v: f32, lo: f32, hi: f32, dv: f32| {
            if v.is_finite() { v.clamp(lo, hi) } else { dv }
        };
        self.master_volume = fix(self.master_volume, 0.0, 1.0, d.master_volume);
        self.sfx_volume = fix(self.sfx_volume, 0.0, 1.0, d.sfx_volume);
        self.music_volume = fix(self.music_volume, 0.0, 1.0, d.music_volume);
        self.ui_scale = fix(self.ui_scale, 0.7, 1.5, 1.0);
        self.camera_speed = fix(self.camera_speed, 0.5, 2.0, 1.0);
        self.autosave_minutes = self.autosave_minutes.min(60);
        self
    }

    /// Volume multiplier for sound effects.
    pub fn sfx_gain(&self) -> f32 {
        if self.muted {
            0.0
        } else {
            self.master_volume * self.sfx_volume
        }
    }

    /// Volume multiplier for the ambient music.
    pub fn music_gain(&self) -> f32 {
        if self.muted {
            0.0
        } else {
            self.master_volume * self.music_volume
        }
    }

    pub fn load() -> Self {
        std::fs::read(paths::settings_file())
            .ok()
            .and_then(|b| serde_json::from_slice::<Settings>(&b).ok())
            .map(Settings::normalized)
            .unwrap_or_default()
    }

    pub fn store(&self) {
        let path = paths::settings_file();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match serde_json::to_vec_pretty(self) {
            Ok(b) => {
                if let Err(e) = std::fs::write(&path, b) {
                    warn!("could not write settings: {e}");
                }
            }
            Err(e) => warn!("could not encode settings: {e}"),
        }
    }
}

/// Debounces writes while a value is being stepped quickly.
#[derive(Resource, Default)]
struct SaveTimer(Option<f32>);

fn persist(
    settings: Res<Settings>,
    time: Res<Time<Real>>,
    mut timer: ResMut<SaveTimer>,
    mut first: Local<bool>,
) {
    if !*first {
        // The initial insertion counts as "changed"; nothing to write yet.
        *first = true;
        return;
    }
    if settings.is_changed() {
        timer.0 = Some(0.6);
    }
    if let Some(t) = timer.0.as_mut() {
        *t -= time.delta_secs();
        if *t <= 0.0 {
            settings.store();
            timer.0 = None;
        }
    }
}

fn apply_window(settings: Res<Settings>, mut window: Single<&mut Window, With<PrimaryWindow>>) {
    if !settings.is_changed() {
        return;
    }
    let mode = if settings.fullscreen {
        WindowMode::BorderlessFullscreen(MonitorSelection::Current)
    } else {
        WindowMode::Windowed
    };
    if window.mode != mode {
        window.mode = mode;
    }
    let present = if settings.vsync {
        PresentMode::AutoVsync
    } else {
        PresentMode::AutoNoVsync
    };
    if window.present_mode != present {
        window.present_mode = present;
    }
}

fn apply_ui_scale(settings: Res<Settings>, mut scale: ResMut<UiScale>) {
    if settings.is_changed() && (scale.0 - settings.ui_scale).abs() > 1e-4 {
        scale.0 = settings.ui_scale;
    }
}

fn apply_graphics(
    settings: Res<Settings>,
    mut commands: Commands,
    cam: Single<(Entity, Has<Bloom>), With<MainCamera>>,
    mut lights: Query<&mut DirectionalLight>,
) {
    let (e, has_bloom) = *cam;
    if settings.bloom && !has_bloom {
        commands.entity(e).insert(Bloom::NATURAL);
    } else if !settings.bloom && has_bloom {
        commands.entity(e).remove::<Bloom>();
    }
    // Scenes respawn their sun on every rebuild, so enforce this continuously.
    for mut l in &mut lights {
        if l.shadow_maps_enabled != settings.shadows {
            l.shadow_maps_enabled = settings.shadows;
        }
    }
}

#[derive(Component)]
struct FpsText;

fn spawn_fps(mut commands: Commands) {
    commands.spawn((
        FpsText,
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(12.0),
            ..default()
        },
        TextColor(Color::srgba(0.8, 0.9, 1.0, 0.8)),
        Node {
            position_type: PositionType::Absolute,
            right: px(8),
            bottom: px(2),
            ..default()
        },
        GlobalZIndex(200),
        Pickable::IGNORE,
    ));
}

fn update_fps(
    settings: Res<Settings>,
    diag: Res<DiagnosticsStore>,
    mut q: Single<(&mut Text, &mut Visibility), With<FpsText>>,
) {
    let (text, vis) = &mut *q;
    if !settings.show_fps {
        **vis = Visibility::Hidden;
        return;
    }
    **vis = Visibility::Inherited;
    if let Some(fps) = diag
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed())
    {
        let s = format!("{fps:.0} fps");
        if text.0 != s {
            text.0 = s;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_files_fill_in_defaults() {
        let s: Settings = serde_json::from_str(r#"{"fullscreen": true}"#).expect("parse");
        assert!(s.fullscreen);
        assert_eq!(s.master_volume, Settings::default().master_volume);
    }

    #[test]
    fn out_of_range_values_are_clamped() {
        let s = Settings {
            master_volume: 9.0,
            ui_scale: 0.0,
            camera_speed: f32::NAN,
            autosave_minutes: 9999,
            ..Settings::default()
        }
        .normalized();
        assert_eq!(s.master_volume, 1.0);
        assert_eq!(s.ui_scale, 0.7);
        assert_eq!(s.camera_speed, 1.0);
        assert_eq!(s.autosave_minutes, 60);
    }

    #[test]
    fn muting_silences_effects() {
        let mut s = Settings::default();
        assert!(s.sfx_gain() > 0.0);
        assert!(s.music_gain() > 0.0);
        s.muted = true;
        assert_eq!(s.sfx_gain(), 0.0);
        assert_eq!(s.music_gain(), 0.0);
    }

    #[test]
    fn old_files_get_a_music_volume() {
        let s: Settings = serde_json::from_str(r#"{"sfx_volume": 0.5}"#).expect("parse");
        assert_eq!(s.music_volume, Settings::default().music_volume);
    }
}
