//! 3D orbit camera: WASD pan, wheel zoom, middle-drag or ,/. to rotate, per-view framing.
//!
//! The camera also carries the post-processing stack: HDR + bloom (toggled by the
//! settings), TonyMcMapface tonemapping with a light colour grade, SSAO for contact
//! occlusion, SMAA (SSAO needs MSAA off) and distance fog that follows the zoom so the
//! focus area stays crisp while the horizon fades into the planet's haze.

use bevy::anti_alias::smaa::{Smaa, SmaaPreset};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::input::mouse::{
    AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollPixelsPerLine,
};
use bevy::pbr::{
    DistanceFog, FogFalloff, ScreenSpaceAmbientOcclusion, ScreenSpaceAmbientOcclusionQualityLevel,
};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::render::view::{ColorGrading, ColorGradingGlobal, ColorGradingSection};

use crate::menu::gameplay;
use crate::settings::Settings;
use crate::state::*;
use crate::ui::PointerOverUi;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraRig>()
            .add_systems(Startup, spawn_camera)
            .add_systems(
                Update,
                (
                    frame_on_view_change,
                    pan.run_if(gameplay),
                    zoom.run_if(gameplay),
                    rotate.run_if(gameplay),
                    apply_rig,
                    follow_fog,
                )
                    .chain(),
            );
    }
}

#[derive(Component)]
pub struct MainCamera;

/// Where the camera looks and from how far. `goal` is eased towards smoothly.
#[derive(Resource, Clone, Copy, Debug)]
pub struct CameraRig {
    pub focus: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub dist: f32,
    pub goal_focus: Vec3,
    pub goal_dist: f32,
    pub bounds: (Vec3, Vec3),
    pub dist_range: (f32, f32),
}

impl Default for CameraRig {
    fn default() -> Self {
        Self {
            focus: Vec3::ZERO,
            yaw: 0.6,
            pitch: 0.85,
            dist: 40.0,
            goal_focus: Vec3::ZERO,
            goal_dist: 40.0,
            bounds: (Vec3::splat(-1e6), Vec3::splat(1e6)),
            dist_range: (5.0, 200.0),
        }
    }
}

impl CameraRig {
    pub fn offset(&self) -> Vec3 {
        Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.cos() * self.pitch.cos(),
        ) * self.dist
    }

    /// Jump to a framing immediately (no easing).
    pub fn snap(&mut self, focus: Vec3, dist: f32) {
        self.focus = focus;
        self.goal_focus = focus;
        self.dist = dist;
        self.goal_dist = dist;
    }
}

fn spawn_camera(mut commands: Commands) {
    let off = std::env::var("BTS_PP_OFF").unwrap_or_default();
    let mut e = commands.spawn((
        Camera3d::default(),
        Camera {
            clear_color: ClearColorConfig::Custom(Color::srgb(0.01, 0.015, 0.035)),
            ..default()
        },
        Tonemapping::TonyMcMapface,
        Bloom::NATURAL,
        DistanceFog {
            color: Color::srgba(0.35, 0.5, 0.65, 1.0),
            falloff: FogFalloff::Linear {
                start: 80.0,
                end: 260.0,
            },
            ..default()
        },
        Transform::from_xyz(0.0, 30.0, 30.0).looking_at(Vec3::ZERO, Vec3::Y),
        MainCamera,
    ));
    if !off.contains("grade") {
        e.insert(grade());
    }
    if !off.contains("ssao") {
        // SSAO needs MSAA off; SMAA smooths edges instead (and is cheaper).
        e.insert((
            Msaa::Off,
            ScreenSpaceAmbientOcclusion {
                quality_level: ScreenSpaceAmbientOcclusionQualityLevel::Medium,
                radius: 0.9,
                constant_object_thickness: 0.3,
            },
        ));
    }
    if !off.contains("smaa") {
        e.insert(Smaa {
            preset: SmaaPreset::High,
        });
    }
}

/// A gentle filmic grade on top of TonyMcMapface (which is neutral but a little flat):
/// richer colour, a touch of contrast, cool shadows and warm highlights.
fn grade() -> ColorGrading {
    ColorGrading {
        global: ColorGradingGlobal {
            post_saturation: 1.08,
            ..default()
        },
        shadows: ColorGradingSection {
            saturation: 1.05,
            contrast: 1.04,
            ..default()
        },
        midtones: ColorGradingSection {
            saturation: 1.1,
            contrast: 1.06,
            ..default()
        },
        highlights: ColorGradingSection {
            saturation: 1.0,
            ..default()
        },
    }
}

/// Planet view: fog starts beyond the focus and thickens toward the horizon, scaled with
/// the zoom so close-ups stay clear and wide shots get aerial perspective.
fn follow_fog(
    view: Res<View>,
    rig: Res<CameraRig>,
    mut fog: Single<&mut DistanceFog, With<MainCamera>>,
) {
    if !matches!(*view, View::Planet(_)) {
        return;
    }
    let falloff = FogFalloff::Linear {
        start: rig.dist * 1.1 + 30.0,
        end: rig.dist * 3.2 + 170.0,
    };
    if let FogFalloff::Linear { start, end } = fog.falloff
        && (start - rig.dist * 1.1 - 30.0).abs() < 0.01
        && (end - rig.dist * 3.2 - 170.0).abs() < 0.01
    {
        return;
    }
    fog.falloff = falloff;
}

/// Framing for each view: focus, distance, pitch, distance limits.
fn frame_on_view_change(
    view: Res<View>,
    sim: Res<SimState>,
    mut rig: ResMut<CameraRig>,
    mut fog: Single<&mut DistanceFog, With<MainCamera>>,
) {
    if !view.is_changed() {
        return;
    }
    match *view {
        View::Planet(p) => {
            let (w, h) = sim
                .0
                .surface(p)
                .map(|s| (s.w as f32, s.h as f32))
                .unwrap_or((64.0, 64.0));
            let c = to_world(Vec2::new(w / 2.0, h / 2.0), 0.0);
            rig.snap(c, 34.0);
            rig.pitch = 0.82;
            rig.yaw = 0.6;
            rig.bounds = (Vec3::new(-8.0, 0.0, -h - 8.0), Vec3::new(w + 8.0, 0.0, 8.0));
            rig.dist_range = (8.0, 140.0);
            fog.falloff = FogFalloff::Linear {
                start: 90.0,
                end: 300.0,
            };
        }
        View::Map(level) => {
            let (dist, pitch, range) = match level {
                MapLevel::System(_) => (95.0, 0.9, (25.0, 260.0)),
                MapLevel::Galaxy(_) => (125.0, 1.0, (25.0, 320.0)),
                MapLevel::Universe => (420.0, 1.05, (80.0, 1400.0)),
            };
            let focus = match level {
                MapLevel::Universe => crate::map::universe_center(&sim.0),
                _ => Vec3::ZERO,
            };
            rig.snap(focus, dist);
            rig.pitch = pitch;
            rig.yaw = 0.0;
            rig.bounds = (Vec3::splat(-2000.0), Vec3::splat(2000.0));
            rig.dist_range = range;
            fog.falloff = FogFalloff::Linear {
                start: 5000.0,
                end: 9000.0,
            };
            // No sun glow in space; the planet scene sets its own when it is rebuilt.
            fog.directional_light_color = Color::NONE;
        }
    }
}

fn pan(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time<Real>>,
    settings: Res<Settings>,
    mut rig: ResMut<CameraRig>,
) {
    let mut d = Vec2::ZERO;
    if keys.any_pressed([KeyCode::KeyW, KeyCode::ArrowUp]) {
        d.y += 1.0;
    }
    if keys.any_pressed([KeyCode::KeyS, KeyCode::ArrowDown]) {
        d.y -= 1.0;
    }
    if keys.any_pressed([KeyCode::KeyA, KeyCode::ArrowLeft]) {
        d.x -= 1.0;
    }
    if keys.any_pressed([KeyCode::KeyD, KeyCode::ArrowRight]) {
        d.x += 1.0;
    }
    if d == Vec2::ZERO {
        return;
    }
    let speed = if keys.pressed(KeyCode::ShiftLeft) {
        2.4
    } else {
        1.0
    } * rig.dist
        * 0.9
        * settings.camera_speed;
    let forward = Vec3::new(-rig.yaw.sin(), 0.0, -rig.yaw.cos());
    let right = Vec3::new(rig.yaw.cos(), 0.0, -rig.yaw.sin());
    let delta = (forward * d.y + right * d.x).normalize() * speed * time.delta_secs();
    let (lo, hi) = rig.bounds;
    rig.goal_focus = (rig.goal_focus + delta).clamp(lo, hi);
}

fn zoom(
    scroll: Res<AccumulatedMouseScroll>,
    conv: Res<MouseScrollPixelsPerLine>,
    over_ui: Res<PointerOverUi>,
    settings: Res<Settings>,
    mut rig: ResMut<CameraRig>,
) {
    if over_ui.0 {
        return;
    }
    let lines = scroll.to_lines(&conv).delta.y * if settings.invert_zoom { -1.0 } else { 1.0 };
    if lines == 0.0 {
        return;
    }
    let (lo, hi) = rig.dist_range;
    rig.goal_dist = (rig.goal_dist * (1.0 - lines * 0.12).clamp(0.5, 1.5)).clamp(lo, hi);
}

fn rotate(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    time: Res<Time<Real>>,
    mut rig: ResMut<CameraRig>,
) {
    let dt = time.delta_secs();
    if keys.pressed(KeyCode::Comma) {
        rig.yaw -= 1.5 * dt;
    }
    if keys.pressed(KeyCode::Period) {
        rig.yaw += 1.5 * dt;
    }
    if mouse.pressed(MouseButton::Middle) {
        rig.yaw -= motion.delta.x * 0.005;
        rig.pitch = (rig.pitch + motion.delta.y * 0.004).clamp(0.25, 1.5);
    }
}

fn apply_rig(
    time: Res<Time<Real>>,
    mut rig: ResMut<CameraRig>,
    mut cam: Single<&mut Transform, With<MainCamera>>,
) {
    let k = 1.0 - (-12.0 * time.delta_secs()).exp();
    rig.focus = rig.focus.lerp(rig.goal_focus, k);
    rig.dist += (rig.goal_dist - rig.dist) * k;
    let eye = rig.focus + rig.offset();
    **cam = Transform::from_translation(eye).looking_at(rig.focus, Vec3::Y);
}
