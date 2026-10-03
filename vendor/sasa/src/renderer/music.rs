use crate::{buffer_is_full, AudioClip, Frame, Renderer};
#[path = "block_low_pass.rs"]
mod block_low_pass;
use anyhow::{Context, Result};
use block_low_pass::BlockLowPass;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc, Arc, Weak,
};

#[derive(Debug, Clone)]
pub struct MusicParams {
    pub loop_mix_time: f64,
    pub amplifier: f32,
    pub playback_rate: f64,
    pub command_buffer_size: usize,
}

impl Default for MusicParams {
    fn default() -> Self {
        Self {
            loop_mix_time: -1.,
            amplifier: 1.,
            playback_rate: 1.,
            command_buffer_size: 16,
        }
    }
}

struct SharedState {
    position: AtomicU64, // float in bits
    paused: AtomicBool,
    block_low_pass: AtomicBool,
}
impl Default for SharedState {
    fn default() -> Self {
        Self {
            position: AtomicU64::default(),
            paused: AtomicBool::new(true),
            block_low_pass: AtomicBool::new(false),
        }
    }
}

enum MusicCommand {
    Pause,
    Resume,
    SetAmplifier(f32),
    SeekTo(f64),
    SetLowPass(f32),
    FadeIn(f64),
    FadeOut(f64),
}
pub(crate) struct MusicRenderer {
    clip: AudioClip,
    settings: MusicParams,
    state: Weak<SharedState>,
    rx: mpsc::Receiver<MusicCommand>,
    paused: bool,
    index: usize,
    last_sample_rate: u32,
    low_pass: f32,
    last_output: Frame,
    block_low_pass: BlockLowPass,

    fade_time: i32,
    fade_current: i32,
}
impl MusicRenderer {
    fn prepare(&mut self, sample_rate: u32) {
        if self.last_sample_rate != sample_rate {
            let factor = sample_rate as f32 / self.last_sample_rate as f32;
            self.index = (self.index as f32 * factor).round() as _;
            self.last_sample_rate = sample_rate;
            self.fade_time = (self.fade_time as f32 * factor).round() as _;
            self.fade_current = (self.fade_current as f32 * factor).round() as _;
        }
        while let Ok(cmd) = self.rx.try_recv() {
            match cmd {
                MusicCommand::Pause => {
                    self.paused = true;
                    self.block_low_pass.reset();
                    if let Some(state) = self.state.upgrade() {
                        state.paused.store(true, Ordering::Relaxed);
                    }
                }
                MusicCommand::Resume => {
                    self.paused = false;
                    if let Some(state) = self.state.upgrade() {
                        state.paused.store(false, Ordering::Relaxed);
                    }
                }
                MusicCommand::SetAmplifier(amp) => {
                    self.settings.amplifier = amp;
                }
                MusicCommand::SeekTo(position) => {
                    self.block_low_pass.reset();
                    self.index = (position * sample_rate as f64 / self.settings.playback_rate).round() as usize;
                }
                MusicCommand::SetLowPass(low_pass) => {
                    self.low_pass = low_pass;
                }
                MusicCommand::FadeIn(time) => {
                    if self.paused {
                        self.paused = false;
                        if let Some(state) = self.state.upgrade() {
                            state.paused.store(false, Ordering::Relaxed);
                        }
                    }
                    self.fade_time = (time * sample_rate as f64).round() as _;
                    self.fade_current = 0;
                }
                MusicCommand::FadeOut(time) => {
                    self.fade_time = (-time * sample_rate as f64).round() as _;
                    self.fade_current = 0;
                }
            }
        }
        if let Some(state) = self.state.upgrade() {
            self.block_low_pass
                .set_active(!self.paused && state.block_low_pass.load(Ordering::Relaxed));
        }
    }

    #[inline]
    fn frame(&mut self, position: f64, delta: f64) -> Option<Frame> {
        let s = &self.settings;
        if let Some(mut frame) = self.clip.sample(position) {
            if s.loop_mix_time >= 0. {
                let pos = position + s.loop_mix_time - self.clip.length();
                if pos >= 0. {
                    if let Some(new_frame) = self.clip.sample(pos) {
                        frame = frame + new_frame;
                    }
                }
            }
            self.index += 1;
            let mut amp = s.amplifier;
            if self.fade_time != 0 {
                if self.fade_time > 0 {
                    self.fade_current += 1;
                    if self.fade_current >= self.fade_time {
                        self.fade_time = 0;
                    } else {
                        amp *= self.fade_current as f32 / self.fade_time as f32;
                    }
                } else {
                    self.fade_current -= 1;
                    if self.fade_current <= self.fade_time {
                        self.fade_time = 0;
                        self.paused = true;
                        if let Some(state) = self.state.upgrade() {
                            state.paused.store(true, Ordering::Relaxed);
                        }
                        return None;
                    } else {
                        amp *= 1. - self.fade_current as f32 / self.fade_time as f32;
                    }
                }
            }
            Some(frame * amp)
        } else if s.loop_mix_time >= 0. {
            let position = position - self.clip.length() + s.loop_mix_time;
            self.index = (position / delta).round() as _;
            Some(if let Some(frame) = self.clip.sample(position) {
                frame * s.amplifier
            } else {
                Frame::default()
            })
        } else {
            self.paused = true;
            None
        }
    }

    #[inline]
    fn position(&self, delta: f64) -> f64 {
        self.index as f64 * delta
    }

    #[inline(always)]
    fn update_and_get(&mut self, frame: Frame) -> Frame {
        self.last_output = self.last_output * self.low_pass + frame * (1. - self.low_pass);
        self.block_low_pass.process(self.last_output, self.last_sample_rate)
    }
}

impl Renderer for MusicRenderer {
    fn alive(&self) -> bool {
        self.state.strong_count() != 0
    }

    fn render_mono(&mut self, sample_rate: u32, data: &mut [f32]) {
        self.prepare(sample_rate);
        if !self.paused {
            let delta = 1. / sample_rate as f64 * self.settings.playback_rate;
            let mut position = self.index as f64 * delta;
            for sample in data.iter_mut() {
                if let Some(frame) = self.frame(position, delta) {
                    *sample += self.update_and_get(frame).avg();
                } else {
                    break;
                }
                position += delta;
            }
            if let Some(state) = self.state.upgrade() {
                state.position.store(self.position(delta).to_bits(), Ordering::Relaxed);
            }
        }
    }

    fn render_stereo(&mut self, sample_rate: u32, data: &mut [f32]) {
        self.prepare(sample_rate);
        if !self.paused {
            let delta = 1. / sample_rate as f64 * self.settings.playback_rate;
            let mut position = self.index as f64 * delta;
            for sample in data.chunks_exact_mut(2) {
                if let Some(frame) = self.frame(position, delta) {
                    let frame = self.update_and_get(frame);
                    sample[0] += frame.0;
                    sample[1] += frame.1;
                } else {
                    break;
                }
                position += delta;
            }
            if let Some(state) = self.state.upgrade() {
                state.position.store(self.position(delta).to_bits(), Ordering::Relaxed);
            }
        }
    }
}

pub struct Music {
    shared: Arc<SharedState>,
    tx: mpsc::SyncSender<MusicCommand>,
}
impl Music {
    pub(crate) fn new(clip: AudioClip, settings: MusicParams) -> (Music, MusicRenderer) {
        let (tx, rx) = mpsc::sync_channel(settings.command_buffer_size);
        let arc = Arc::default();
        let renderer = MusicRenderer {
            clip,
            settings,
            state: Arc::downgrade(&arc),
            rx,
            paused: true,
            index: 0,
            last_sample_rate: 1,
            low_pass: 0.,
            last_output: Frame(0., 0.),
            block_low_pass: BlockLowPass::default(),

            fade_time: 0,
            fade_current: 0,
        };
        (Self { shared: arc, tx }, renderer)
    }

    pub fn play(&mut self) -> Result<()> {
        self.tx.send(MusicCommand::Resume).map_err(buffer_is_full).context("play music")
    }

    pub fn pause(&mut self) -> Result<()> {
        self.set_block_low_pass(false);
        self.tx.send(MusicCommand::Pause).map_err(buffer_is_full).context("pause")
    }

    pub fn paused(&mut self) -> bool {
        self.shared.paused.load(Ordering::Relaxed)
    }

    pub fn set_amplifier(&mut self, amp: f32) -> Result<()> {
        self.tx
            .send(MusicCommand::SetAmplifier(amp))
            .map_err(buffer_is_full)
            .context("set amplifier")
    }

    pub fn seek_to(&mut self, position: f64) -> Result<()> {
        self.set_block_low_pass(false);
        self.tx.send(MusicCommand::SeekTo(position)).map_err(buffer_is_full).context("seek to")
    }

    pub fn set_low_pass(&mut self, low_pass: f32) -> Result<()> {
        self.tx
            .send(MusicCommand::SetLowPass(low_pass))
            .map_err(buffer_is_full)
            .context("set low pass")
    }

    /// Enable the Phigros noise-field music filter. Safe to call every frame:
    /// this does not enqueue or block, and the sweep restarts only on a change.
    pub fn set_block_low_pass(&self, active: bool) {
        self.shared.block_low_pass.store(active, Ordering::Relaxed);
    }

    pub fn fade_in(&mut self, time: f64) -> Result<()> {
        self.tx.send(MusicCommand::FadeIn(time)).map_err(buffer_is_full).context("fade in")
    }

    pub fn fade_out(&mut self, time: f64) -> Result<()> {
        self.tx.send(MusicCommand::FadeOut(time)).map_err(buffer_is_full).context("fade out")
    }

    pub fn position(&self) -> f64 {
        f64::from_bits(self.shared.position.load(Ordering::Relaxed))
    }
}

#[cfg(test)]
mod block_filter_tests {
    use super::*;

    fn tone(rate: u32, hz: f32) -> AudioClip {
        AudioClip::from_raw(
            (0..rate)
                .map(|i| Frame((std::f32::consts::TAU * hz * i as f32 / rate as f32).sin() * 0.1, 0.))
                .collect(),
            rate,
        )
    }

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|x| x * x).sum::<f32>() / samples.len() as f32).sqrt()
    }

    #[test]
    fn attenuates_high_frequencies_at_each_output_rate_without_stereo_crosstalk() {
        for rate in [32000, 44100, 48000, 96000] {
            for (hz, min_gain, max_gain) in [(200., 0.95, 1.1), (6000., 0., 0.07)] {
                let (mut music, mut renderer) = Music::new(tone(rate, hz), MusicParams::default());
                music.set_block_low_pass(true);
                music.play().unwrap();
                let mut data = vec![0.; rate as usize];
                renderer.render_stereo(rate, &mut data);
                let left: Vec<_> = data.chunks_exact(2).skip(1000).map(|f| f[0]).collect();
                let gain = rms(&left) / (0.1 / 2_f32.sqrt());
                assert!((min_gain..=max_gain).contains(&gain), "rate={rate} hz={hz} gain={gain}");
                assert!(data.chunks_exact(2).all(|f| f[1] == 0.));
            }
        }
    }

    #[test]
    fn default_and_released_music_bypass_and_keep_playback_position() {
        let rate = 48000;
        let (mut dry, mut dry_renderer) = Music::new(tone(rate, 6000.), MusicParams::default());
        let (mut wet, mut wet_renderer) = Music::new(tone(rate, 6000.), MusicParams::default());
        dry.play().unwrap();
        wet.play().unwrap();
        let mut a = vec![0.; 2400];
        let mut b = a.clone();
        dry_renderer.render_mono(rate, &mut a);
        wet_renderer.render_mono(rate, &mut b);
        assert_eq!(a, b);
        wet.set_block_low_pass(true);
        a.fill(0.);
        b.fill(0.);
        dry_renderer.render_mono(rate, &mut a);
        wet_renderer.render_mono(rate, &mut b);
        assert!(rms(&b) < rms(&a) * 0.1);
        wet.set_block_low_pass(false);
        let mut a = vec![0.; 6000];
        let mut b = a.clone();
        dry_renderer.render_mono(rate, &mut a);
        wet_renderer.render_mono(rate, &mut b);
        assert_eq!(&a[4900..], &b[4900..]);
        assert_eq!(dry.position(), wet.position());
    }

    #[test]
    fn pause_and_seek_clear_filter_without_leaking_tail() {
        let rate = 48000;
        let (mut music, mut renderer) = Music::new(tone(rate, 6000.), MusicParams::default());
        music.play().unwrap();
        music.set_block_low_pass(true);
        renderer.render_mono(rate, &mut [0.; 128]);
        music.pause().unwrap();
        let mut silent = [0.; 128];
        renderer.render_mono(rate, &mut silent);
        assert_eq!(silent, [0.; 128]);
        music.play().unwrap();
        music.set_block_low_pass(true);
        renderer.render_mono(rate, &mut [0.; 128]);
        music.seek_to(0.).unwrap();
        let mut actual = [0.; 128];
        renderer.render_mono(rate, &mut actual);
        let (mut dry, mut dry_renderer) = Music::new(tone(rate, 6000.), MusicParams::default());
        dry.play().unwrap();
        let mut expected = [0.; 128];
        dry_renderer.render_mono(rate, &mut expected);
        assert_eq!(actual, expected);
    }
}
