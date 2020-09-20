/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
//! Audio device streaming implementation for [SDL2](https://crates.io/crates/sdl2).
//!
//! This module implements [carousel][crate::carousel] using the **SDL2** audio layer.
//!
//! Requires "sdl2" feature to be enabled.
use core::convert::TryFrom;
use core::time::Duration;

#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

use sdl2::{
    Sdl,
    audio::{AudioDevice, AudioCallback, AudioSpecDesired, AudioFormatNum}
};

pub use sdl2::audio::AudioStatus;

use spectrusty_core::audio::AudioSample;
use crate::carousel::*;
pub use super::{AudioHandleError, AudioHandleErrorKind};

struct AudioCb<T>(AudioFrameConsumer<T>);

impl<T: AudioFormatNum + AudioSample> AudioCallback for AudioCb<T> {
    type Channel = T;

    fn callback(&mut self, out: &mut [T]) {
        match self.0.fill_buffer(out, false) {
            Ok(unfilled) => {
                if unfilled.len() != 0 {
                    for t in unfilled {
                        *t = T::SILENCE
                    }
                    debug!("missing buffer");
                }
            }
            Err(_) => {
                error!("fatal: producer terminated");
            }
        }
    }
}

/// The struct for producing and controlling the audio playback.
///
/// It embeds the interconnected pair of [carousel][crate::carousel]'s [AudioFrameProducer] with the
/// [AudioFrameConsumer] directly exposing the `producer` to the user. The consumer lives in the
/// **SDL2** audio thread and is responsible for filling up the audio output buffer with sample data
/// sent from the `producer`.
///
/// The `T` parameter should be one of the [sample primitives][AudioSample].
pub struct AudioHandle<T: AudioFormatNum + AudioSample> {
    /// The audio sample frequency of the output stream.
    pub sample_rate: u32,
    /// The number of audio channels in the output stream.
    pub channels: u8,
    /// The number of samples in the output audio buffer.
    pub samples: u16,
    /// The audio sample producer, interconnected with an audio consumer living in the audio thread.
    pub producer: AudioFrameProducer<T>,
    device: AudioDevice<AudioCb<T>>,
}

const DEFAULT_SAMPLE_RATE: i32 = 44100;
const DEFAULT_CHANNELS: u8 = 2;

impl<T: AudioFormatNum + AudioSample> AudioHandle<T> {
    /// Returns the status of the audio device playback.
    pub fn status(&self) -> AudioStatus {
        self.device.status()
    }
    /// Starts playback of the audio device.
    pub fn play(&self) {
        self.device.resume()
    }
    /// Pauses playback of the audio device.
    pub fn pause(&self) {
        self.device.pause()
    }
    /// Closes audio playback and frees underlying resources.
    /// Returns the unwrapped frame consumer.
    pub fn close(self) -> AudioFrameConsumer<T> {
        self.device.close_and_get_callback().0
    }
    /// Creates an instance of the [AudioHandle] from the provided **SDL2** context.
    ///
    /// The audio parameters used by default are the sample rate of 44100, 2 channels, and the
    /// output buffer size adjusted to the requested latency.
    ///
    /// * `frame_duration_nanos` is the duration in nanoseconds of the standard emulation frame.
    /// * `latency` is the requested audio latency; the actual `latency` argument passed to the
    ///   [create_carousel] may be larger but never smaller than the provided one.
    pub fn create(
                sdl_context: &Sdl,
                frame_duration_nanos: u32,
                latency: usize
            ) -> Result<Self, AudioHandleError>
    {
        Self::create_with_specs(
                sdl_context,
                AudioSpecDesired { freq: None, channels: None, samples: None },
                frame_duration_nanos,
                latency
        )
    }
    /// Creates an instance of the [AudioHandle] from the provided **SDL2** context with the
    /// desired audio parameters.
    ///
    /// The audio parameters used by default are the sample rate of 44100, 2 channels, and the
    /// output buffer size adjusted to the requested latency.
    ///
    /// * `frame_duration_nanos` is the duration in nanoseconds of the standard emulation frame.
    /// * `latency` is the requested audio latency; the actual `latency` argument passed to the
    ///   [create_carousel] may be larger but never smaller than the provided one.
    /// * `desired_spec` can be populated with the desired audio parameters.
    pub fn create_with_specs(
                sdl_context: &Sdl,
                mut desired_spec: AudioSpecDesired,
                frame_duration_nanos: u32,
                latency: usize
            ) -> Result<Self, AudioHandleError>
    {
        let audio_subsystem = sdl_context.audio().map_err(|e| (e, AudioHandleErrorKind::AudioSubsystem))?;
        let frame_duration_secs = Duration::from_nanos(frame_duration_nanos.into()).as_secs_f64();

        if desired_spec.freq.is_none() {
            desired_spec.freq = Some(DEFAULT_SAMPLE_RATE);
        }
        if desired_spec.channels.is_none() {
            desired_spec.channels = Some(DEFAULT_CHANNELS);
        }
        if desired_spec.samples.is_none() {
            let audio_frame_samples = (desired_spec.freq.unwrap() as f64 * frame_duration_secs).ceil() as usize;
            let samples: u16 = (audio_frame_samples * latency.max(1)).checked_next_power_of_two()
                .and_then(|samples| u16::try_from(samples).ok())
                .unwrap_or(0x8000);
            desired_spec.samples = Some(samples);
        }

        let mut producer: Option<AudioFrameProducer<T>> = None;

        let device = audio_subsystem.open_playback(None, &desired_spec, |spec| {
            let audio_frame_samples = (spec.freq as f64 * frame_duration_secs).ceil() as usize;
            let min_latency = (spec.samples as usize / audio_frame_samples).max(1);
            let latency = latency.max(min_latency);
            debug!("audio specs: {:?}", spec);
            debug!("audio frame samples: {} latency: {}", audio_frame_samples, latency);
            let (prd, consumer) = create_carousel::<T>(latency, audio_frame_samples, spec.channels);
            producer = Some(prd);
            AudioCb(consumer)
        }).map_err(|e| (e, AudioHandleErrorKind::AudioStream))?;

        let spec = device.spec();

        Ok(AudioHandle {
            sample_rate: spec.freq as u32,
            channels: spec.channels,
            samples: spec.samples,
            producer: producer.unwrap(),
            device
        })
    }
}
