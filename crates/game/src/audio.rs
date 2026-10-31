//! Procedural sound effects: short synthesized tones, no audio assets needed.

use core::time::Duration;

use bevy::audio::{AddAudioSource, ChannelCount, SampleRate, Source, Volume};
use bevy::prelude::*;
use bevy::reflect::TypePath;

use crate::settings::Settings;

pub struct SfxPlugin;

impl Plugin for SfxPlugin {
    fn build(&self, app: &mut App) {
        app.add_audio_source::<Tone>()
            .init_resource::<SfxQueue>()
            .add_systems(Update, (toggle_sound, play_sfx).chain());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sfx {
    Build,
    Error,
    Research,
    Ship,
    Alert,
    Attack,
    Generate,
    Victory,
}

#[derive(Resource, Default)]
pub struct SfxQueue(pub Vec<Sfx>);

/// A sequence of (frequency Hz, seconds, square-ness 0..1) notes.
#[derive(Asset, TypePath, Clone)]
pub struct Tone {
    notes: Vec<(f32, f32, f32)>,
}

pub struct ToneDecoder {
    notes: Vec<(f32, f32, f32)>,
    note: usize,
    sample: u32,
    phase: f32,
}

const RATE: u32 = 44_100;

impl Iterator for ToneDecoder {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let &(freq, secs, square) = self.notes.get(self.note)?;
        let len = (secs * RATE as f32) as u32;
        if self.sample >= len {
            self.note += 1;
            self.sample = 0;
            return self.next();
        }
        let t = self.sample as f32 / len as f32;
        // Short attack, exponential-ish decay.
        let env = (t * 40.0).min(1.0) * (1.0 - t).powf(1.6);
        self.phase = (self.phase + freq / RATE as f32) % 1.0;
        let sine = (self.phase * std::f32::consts::TAU).sin();
        let sq = if self.phase < 0.5 { 1.0 } else { -1.0 };
        self.sample += 1;
        Some((sine * (1.0 - square) + sq * square * 0.5) * env * 0.5)
    }
}

impl Source for ToneDecoder {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> ChannelCount {
        ChannelCount::new(1).expect("mono")
    }
    fn sample_rate(&self) -> SampleRate {
        SampleRate::new(RATE).expect("rate")
    }
    fn total_duration(&self) -> Option<Duration> {
        Some(Duration::from_secs_f32(
            self.notes.iter().map(|n| n.1).sum(),
        ))
    }
}

impl Decodable for Tone {
    type Decoder = ToneDecoder;

    fn decoder(&self) -> ToneDecoder {
        ToneDecoder {
            notes: self.notes.clone(),
            note: 0,
            sample: 0,
            phase: 0.0,
        }
    }
}

fn tone_for(s: Sfx) -> (Tone, f32) {
    let n = |v: &[(f32, f32, f32)]| Tone { notes: v.to_vec() };
    match s {
        Sfx::Build => (n(&[(1400.0, 0.04, 0.2)]), 0.25),
        Sfx::Error => (n(&[(180.0, 0.09, 0.6), (150.0, 0.09, 0.6)]), 0.35),
        Sfx::Research => (
            n(&[(523.0, 0.12, 0.0), (659.0, 0.12, 0.0), (784.0, 0.25, 0.0)]),
            0.4,
        ),
        Sfx::Ship => (n(&[(440.0, 0.15, 0.1), (587.0, 0.3, 0.1)]), 0.35),
        Sfx::Alert => (n(&[(110.0, 0.18, 0.8), (110.0, 0.18, 0.8)]), 0.4),
        Sfx::Attack => (
            n(&[(300.0, 0.08, 0.9), (220.0, 0.08, 0.9), (160.0, 0.2, 0.9)]),
            0.4,
        ),
        Sfx::Generate => (
            n(&[(1046.0, 0.1, 0.0), (1318.0, 0.1, 0.0), (1568.0, 0.3, 0.0)]),
            0.15,
        ),
        Sfx::Victory => (
            n(&[
                (523.0, 0.2, 0.1),
                (659.0, 0.2, 0.1),
                (784.0, 0.2, 0.1),
                (1046.0, 0.6, 0.1),
            ]),
            0.5,
        ),
    }
}

fn toggle_sound(keys: Res<ButtonInput<KeyCode>>, mut settings: ResMut<Settings>) {
    if keys.just_pressed(KeyCode::F2) {
        settings.muted = !settings.muted;
    }
}

fn play_sfx(
    mut commands: Commands,
    mut queue: ResMut<SfxQueue>,
    settings: Res<Settings>,
    mut tones: ResMut<Assets<Tone>>,
) {
    let mut played: Vec<Sfx> = Vec::new();
    for s in queue.0.drain(..) {
        if settings.sfx_gain() <= 0.0 || played.contains(&s) {
            continue;
        }
        played.push(s);
        let (tone, vol) = tone_for(s);
        commands.spawn((
            AudioPlayer(tones.add(tone)),
            PlaybackSettings::DESPAWN.with_volume(Volume::Linear(vol * settings.sfx_gain())),
        ));
    }
}
