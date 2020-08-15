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
    BlepAmpFilter, BlepStereo, AudioSample,
    synth::BandLimited
};

const AUDIO_CHANNELS: u32 = 2;
// The maximum number of audio buffers in use.
const QUEUE_MAX_LEN: usize = 8;
// The minimum sound start delay.
//
// Serves as a synchronization margin between audio and emulated frames.
const SOUND_START_MIN_DELAY: f64 = 0.01;
// The maximum sound start delay.
//
// If the audio frame would be scheduled to play with a longer delay than this constant,
// rendering audio frame will be skipped. This prevents audio to desynchronize with the emulation.
const SOUND_START_MAX_DELAY: f64 = 0.1;

type AudioQueue = VecDeque<AudioBuffer>;

pub type BandLim = BlepAmpFilter<BlepStereo<BandLimited<f32>>>;

pub fn create_blep() -> BandLim {
    BlepAmpFilter::build(0.5)(BlepStereo::build(0.86)(BandLimited::new(2)))
}

pub struct AudioStream {
    buffers: [Vec<f32>;AUDIO_CHANNELS as usize],
    audio_queue: AudioQueue,
    sample_rate: f32,
    buffer_length: u32,
    buffer_duration: f64,
    ctx: AudioContext,
    gain: GainNode,
    started_at: f64
}

impl Drop for AudioStream {
    fn drop(&mut self) {
        let _ = self.ctx.close();
    }
}

impl AudioStream {
    /// Provide the maximum duration (in seconds) of a single audio frame.
    ///
    /// Any values between `0.02` and `1.0` are being accepted, otherwise returns an error.
    pub fn new(max_buffer_duration: f32) -> Result<Self, JsValue> {
        if max_buffer_duration < 0.02 || max_buffer_duration > 1.0 {
            Err("requested buffer duration should be between 0.02 and 1.0")?;
        }

        let ctx = AudioContext::new()?;
        let sample_rate = ctx.sample_rate();
        let buffer_length = (sample_rate * max_buffer_duration).trunc() as u32;

        console_log!("sample_rate: {} buffer_length: {}", sample_rate, buffer_length);

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
            started_at: 0.0
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate as u32
    }
    /// Plays the next audio frame from the given `BandLim` and prepares it for the next frame.
    ///
    /// The duration of the rendered audio frame must not exceed the value given to [AudioStream::new],
    /// otherwise an error is being returned.
    pub fn play_next_audio_frame(&mut self, bandlim: &mut BandLim) -> Result<(), JsValue> {
        let nsamples = self.render_audio_frame(bandlim);
        let duration = nsamples as f64 / self.sample_rate as f64;
        if duration > self.buffer_duration {
            Err("frame duration exceeds the maximum buffer duration")?;
        }

        let current_time = self.ctx.current_time();
        let time = self.started_at.max(current_time + SOUND_START_MIN_DELAY);
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
        self.started_at = time + duration;
        Ok(())
    }

    fn render_audio_frame(&mut self, bandlim: &mut BandLim) -> usize {
        let mut nsamples = 0;
        for (channel, target) in self.buffers.iter_mut().enumerate() {
            let sample_iter = bandlim.sum_iter(channel);
            nsamples = sample_iter.len();
            target.resize(nsamples, f32::silence());
            for (sample, dst) in sample_iter.zip(target.iter_mut()) {
                *dst = sample;
            }
        }
        bandlim.next_frame();
        nsamples
    }
    /// Sets the gain for this oscillator, between 0.0 and 1.0.
    pub fn gain(&self) -> f32 {
        self.gain.gain().value()
    }
    /// Sets the gain for this oscillator, between 0.0 and 1.0.
    pub fn set_gain(&self, gain: f32) {
        self.gain.gain().set_value(gain.min(1.0).max(0.0));
    }

    pub fn pause(&self) -> Result<Promise, JsValue> {
        let ctx = &self.ctx;
        match ctx.state() {
            AudioContextState::Suspended => {}
            AudioContextState::Running => {
                return ctx.suspend()
            }
            _ => Err("Audio context closed")?
        }
        Ok(Promise::resolve(&JsValue::UNDEFINED))
    }

    pub fn resume(&self) -> Result<Promise, JsValue> {
        let ctx = &self.ctx;
        match ctx.state() {
            AudioContextState::Suspended => {
                return ctx.resume()
            }
            AudioContextState::Running => {}
            _ => Err("Audio context closed")?
        }
        Ok(Promise::resolve(&JsValue::UNDEFINED))
    }
}
