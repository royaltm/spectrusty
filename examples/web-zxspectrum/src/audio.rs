#![allow(unused_imports)]
use core::mem;
use core::ops::Range;
use core::cell::RefCell;
use std::rc::Rc;
use std::collections::VecDeque;
use wasm_bindgen::{JsCast, prelude::*};
use js_sys::Promise;
use web_sys::{
    AudioBuffer,
    AudioContext,
    AudioContextState,
    GainNode,
};
use wasm_bindgen_futures::JsFuture;
use spectrusty::audio::{
    BlepAmpFilter, BlepStereo, AudioSample,
    synth::BandLimited
};

use super::log_f64;

const AUDIO_CHANNELS: u32 = 2;
const SOUND_START_MIN_DELAY: f64 = 0.01;
const SOUND_START_MAX_DELAY: f64 = 0.1;

type EndClosure = Closure<dyn FnMut()>;

type AudioQueue = VecDeque<AudioQueueItem>;

pub type BandLim = BlepAmpFilter<BlepStereo<BandLimited<f32>>>;

pub fn create_blep() -> BandLim {
    BlepAmpFilter::build(0.5)(BlepStereo::build(0.86)(BandLimited::new(2)))
}

struct AudioQueueItem {
    audio_buffer: AudioBuffer,
    end_closure: Rc<RefCell<Option<EndClosure>>>,
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
        let end_closure = Rc::new(RefCell::new(None));
        let buffer_duration = audio_buffer.duration();
        audio_queue.push_back(AudioQueueItem { audio_buffer, end_closure });

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

    pub fn play_next_audio_frame(&mut self, bandlim: &mut BandLim) -> Result<(), JsValue> {
        let nsamples = self.render_audio_frame(bandlim);

        if self.audio_queue.front().unwrap().end_closure.borrow().is_some() {
            let audio_buffer = self.ctx.create_buffer(AUDIO_CHANNELS, self.buffer_length, self.sample_rate)?;
            let end_closure = Rc::new(RefCell::new(None));
            self.audio_queue.push_back(AudioQueueItem { audio_buffer, end_closure });
        }
        else {
            self.audio_queue.rotate_left(1);
        }
        let audio_item = self.audio_queue.back_mut().unwrap();

        for (buffer, channel) in self.buffers.iter_mut().zip(0..AUDIO_CHANNELS as i32) {
            assert_eq!(nsamples, buffer.len());
            audio_item.audio_buffer.copy_to_channel(buffer, channel)?;
        }

        let weakref = Rc::downgrade(&audio_item.end_closure);
        let closure = Closure::once(move || {
            if let Some(rc) = weakref.upgrade() {
                *rc.borrow_mut() = None;
            }
        });

        let source = self.ctx.create_buffer_source()?;
        source.set_onended(Some( closure.as_ref().unchecked_ref() ));
        *audio_item.end_closure.borrow_mut() = Some(closure);

        let duration = nsamples as f64 / self.sample_rate as f64;
        if duration > self.buffer_duration {
            Err("frame duration exceeds the maximum")?;
        }
        source.set_buffer(Some(&audio_item.audio_buffer));
        source.connect_with_audio_node(&self.gain)?;
        let current_time = self.ctx.current_time();
        let time = self.started_at.max(current_time + SOUND_START_MIN_DELAY);
        if time > current_time + SOUND_START_MAX_DELAY {
            *audio_item.end_closure.borrow_mut() = None;
        }
        else {
            source.start_with_when_and_grain_offset_and_grain_duration(time, 0.0, duration)?;
            self.started_at = time + duration;
        }
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
