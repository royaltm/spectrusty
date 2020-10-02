/*
    web-zxspectrum: ZX Spectrum emulator example as a Web application.
    Copyright (C) 2020  Rafal Michalski

    For the full copyright notice, see the lib.rs file.
*/
use std::collections::VecDeque;
use wasm_bindgen::prelude::*;
use js_sys::Promise;
use web_sys::{
    AudioBuffer,
    AudioContext,
    AudioContextState,
    GainNode,
};
use spectrusty::audio::{
    BlepAmpFilter, BlepStereo,
    synth::BandLimited
};
use crate::utils::Result;

const AUDIO_CHANNELS: u32 = 2;
/// The maximum number of audio buffers in use.
const QUEUE_MAX_LEN: usize = 8;
/// The minimum sound start delay.
///
/// Serves as a synchronization margin between audio and emulated frames.
const SOUND_START_MIN_DELAY: f64 = 0.01;
/// The maximum sound start delay.
///
/// If the audio frame would be scheduled to play with a longer delay than this constant,
/// rendering audio frame will be skipped. This prevents audio lag.
const SOUND_START_MAX_DELAY: f64 = 0.1;

type AudioQueue = VecDeque<AudioBuffer>;

/// The type used for the `Blep` implementation.
pub type BandLim = BlepAmpFilter<BlepStereo<BandLimited<f32>>>;

/// `Blep` factory.
pub fn create_blep() -> BandLim {
    BlepAmpFilter::build(0.5)(BlepStereo::build(0.86)(BandLimited::new(2)))
}

/// This type implements audio playback via Web Audio API.
pub struct AudioStream {
    /// Intermediary audio buffers.
    buffers: [Vec<f32>;AUDIO_CHANNELS as usize],
    /// Web AudioBuffer queue.
    audio_queue: AudioQueue,
    /// Cached AudioContext's sample rate.
    sample_rate: f32,
    /// The number of samples of the [AudioBuffer]s added to the [AudioQueue].
    buffer_length: u32,
    /// The maximum duration of a single audio frame in seconds.
    buffer_duration: f64,
    /// The audio context used for audio playback.
    ctx: AudioContext,
    /// The gain node to control audio volume.
    gain: GainNode,
    /// The next frame's playback should start at this timestamp or later.
    next_at: f64
}

impl Drop for AudioStream {
    fn drop(&mut self) {
        let _ = self.ctx.close();
    }
}

impl AudioStream {
    /// Creates an instance of `AudioStream`.
    ///
    /// Provide the maximum duration (in seconds) of a single audio frame.
    ///
    /// Any values between `0.02` and `1.0` are being accepted, otherwise returns an error.
    pub fn new(max_buffer_duration: f32) -> Result<Self> {
        if max_buffer_duration < 0.02 || max_buffer_duration > 1.0 {
            return Err("requested buffer duration should be between 0.02 and 1.0".into());
        }

        let ctx = AudioContext::new()?;
        let sample_rate = ctx.sample_rate();
        let buffer_length = (sample_rate * max_buffer_duration).trunc() as u32;

        // log::debug!("sample_rate: {} buffer_length: {}", sample_rate, buffer_length);

        let mut audio_queue = AudioQueue::new();
        let audio_buffer = ctx.create_buffer(AUDIO_CHANNELS, buffer_length, sample_rate)?;
        let buffer_duration = audio_buffer.duration();
        audio_queue.push_back(audio_buffer);

        let gain = ctx.create_gain()?;
        gain.gain().set_value(1.0);
        gain.connect_with_audio_node(&ctx.destination())?;

        Ok(AudioStream {
            buffer_length,
            buffer_duration,
            audio_queue,
            ctx,
            buffers: Default::default(),
            sample_rate,
            gain,
            next_at: 0.0
        })
    }
    /// Returns the sample rate of the audio playback.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate as u32
    }
    /// Renders the next audio frame and schedules it to be played from the given `BandLim`.
    ///
    /// Prepares the `BandLim` for the next frame.
    ///
    /// # Errors
    ///
    /// The duration of the rendered audio frame must not exceed the value given to [AudioStream::new],
    /// otherwise an error is being returned.
    ///
    /// # Panics
    /// The `bandlim` should be in a finalized state, otherwise this function will panic.
    pub fn play_next_audio_frame(&mut self, bandlim: &mut BandLim) -> Result<()> {
        let nsamples = self.render_audio_frame(bandlim);
        let duration = nsamples as f64 / self.sample_rate as f64;
        if duration > self.buffer_duration {
            return Err("frame duration exceeds the maximum buffer duration".into());
        }

        let current_time = self.ctx.current_time();
        let time = self.next_at.max(current_time + SOUND_START_MIN_DELAY);
        if time > current_time + SOUND_START_MAX_DELAY {
            return Ok(());
        }

        if self.audio_queue.len() < QUEUE_MAX_LEN {
            let audio_buffer = self.ctx.create_buffer(AUDIO_CHANNELS, self.buffer_length, self.sample_rate)?;
            self.audio_queue.push_back(audio_buffer);
        }
        else {
            self.audio_queue.rotate_left(1);
        }

        let audio_buffer = self.audio_queue.back().unwrap();

        for (buffer, channel) in self.buffers.iter_mut().zip(0..AUDIO_CHANNELS as i32) {
            assert_eq!(nsamples, buffer.len());
            audio_buffer.copy_to_channel(buffer, channel)?;
        }

        let source = self.ctx.create_buffer_source()?;
        source.set_buffer(Some(&audio_buffer));
        source.connect_with_audio_node(&self.gain)?;
        source.start_with_when_and_grain_offset_and_grain_duration(time, 0.0, duration)?;
        self.next_at = time + duration;
        Ok(())
    }

    fn render_audio_frame(&mut self, bandlim: &mut BandLim) -> usize {
        let mut nsamples = 0;
        for (channel, target) in self.buffers.iter_mut().enumerate() {
            let sample_iter = bandlim.sum_iter::<f32>(channel);
            nsamples = sample_iter.len();
            target.clear();
            target.extend(sample_iter);
        }
        bandlim.next_frame();
        nsamples
    }
    /// Returns the volume gain for audio playback.
    pub fn gain(&self) -> f32 {
        self.gain.gain().value()
    }
    /// Sets the volume gain for audio playback.
    ///
    /// Provided values are being capped between 0.0 and 1.0.
    pub fn set_gain(&self, gain: f32) {
        self.gain.gain().set_value(gain.min(1.0).max(0.0));
    }
    /// Pauses audio playback by suspending audio context.
    ///
    /// Returns a [Promise] which resolves when playback has been paused or is rejected
    /// when the audio context is already closed.
    pub fn pause(&self) -> Result<Promise> {
        let ctx = &self.ctx;
        match ctx.state() {
            AudioContextState::Running => ctx.suspend(),
            AudioContextState::Suspended => Ok(Promise::resolve(&JsValue::UNDEFINED)),
            _ => Ok(Promise::reject(&JsValue::from_str("Audio context closed")))
        }
    }
    /// Resumes audio playback by resuming audio context.
    ///
    /// Returns a [Promise] which resolves when playback has been resumed or is rejected
    /// when the audio context is already closed.
    pub fn resume(&self) -> Result<Promise> {
        let ctx = &self.ctx;
        match ctx.state() {
            AudioContextState::Suspended => ctx.resume(),
            AudioContextState::Running => Ok(Promise::resolve(&JsValue::UNDEFINED)),
            _ => Ok(Promise::reject(&JsValue::from_str("Audio context closed")))
        }
    }
}
