/*
    Copyright (C) 2020-2023  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
//! Audio device streaming implementation for [cpal](https://crates.io/crates/cpal).
//!
//! This module implements [carousel][crate::carousel] using the **cpal** audio layer.
//!
//! Requires "cpal" feature to be enabled.
use core::convert::TryInto;
use core::time::Duration;

#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

use cpal::{
    Stream,
    PlayStreamError, PauseStreamError, DefaultStreamConfigError, BuildStreamError,
    traits::{DeviceTrait, HostTrait, StreamTrait}
};

pub use cpal::SampleFormat;

use spectrusty_core::audio::AudioSample;
use crate::carousel::*;
pub use super::{AudioHandleError, AudioHandleErrorKind};

/// The struct for producing and controlling the audio playback.
///
/// It embeds the interconnected pair of [carousel][crate::carousel]'s [AudioFrameProducer] with the
/// [AudioFrameConsumer] directly exposing the `producer` to the user. The consumer lives in the
/// **cpal** audio thread and is responsible for filling up the audio output buffer with sample data
/// sent from the `producer`.
///
/// The `T` parameter should be one of the [sample primitives][cpal::Sample].
pub struct AudioHandle<T: cpal::SizedSample + AudioSample> {
    /// The audio sample frequency of the output stream.
    pub sample_rate: u32,
    /// The number of audio channels in the output stream.
    pub channels: u8,
    /// The audio sample producer, interconnected with an audio consumer living in the audio thread.
    pub producer: AudioFrameProducer<T>,
    stream: Stream
}

/// The enum for producing and controlling the audio playback regardless of the sample format used.
#[non_exhaustive]
pub enum AudioHandleAnyFormat {
    I8(AudioHandle<i8>),
    I16(AudioHandle<i16>),
    I32(AudioHandle<i32>),
    I64(AudioHandle<i64>),
    U8(AudioHandle<u8>),
    U16(AudioHandle<u16>),
    U32(AudioHandle<u32>),
    U64(AudioHandle<u64>),
    F32(AudioHandle<f32>),
    F64(AudioHandle<f64>),
}

macro_rules! implement_any {
    ($me:ident, $ha:ident, $ex:expr) => {
        match $me {
            AudioHandleAnyFormat::I8($ha) => $ex,
            AudioHandleAnyFormat::I16($ha) => $ex,
            AudioHandleAnyFormat::I32($ha) => $ex,
            AudioHandleAnyFormat::I64($ha) => $ex,
            AudioHandleAnyFormat::U8($ha) => $ex,
            AudioHandleAnyFormat::U16($ha) => $ex,
            AudioHandleAnyFormat::U32($ha) => $ex,
            AudioHandleAnyFormat::U64($ha) => $ex,
            AudioHandleAnyFormat::F32($ha) => $ex,
            AudioHandleAnyFormat::F64($ha) => $ex,
        }
    };
}

impl AudioHandleAnyFormat {
    /// Returns the sample format of the current variant.
    pub fn sample_format(&self) -> SampleFormat {
        use AudioHandleAnyFormat::*;
        match self {
            I8(..) => SampleFormat::I8,
            I16(..) => SampleFormat::I16,
            I32(..) => SampleFormat::I32,
            I64(..) => SampleFormat::I64,
            U8(..) => SampleFormat::U8,
            U16(..) => SampleFormat::U16,
            U32(..) => SampleFormat::U32,
            U64(..) => SampleFormat::U64,
            F32(..) => SampleFormat::F32,
            F64(..) => SampleFormat::F32,
        }
    }
    /// Returns the audio sample frequency of the output stream.
    pub fn sample_rate(&self) -> u32 {
        implement_any! { self, audio, audio.sample_rate }
    }
    /// Returns the number of audio channels in the output stream.
    pub fn channels(&self) -> u8 {
        implement_any! { self, audio, audio.channels }
    }
    /// Starts playback of the audio device.
    pub fn play(&self) -> Result<(), AudioHandleError> {
        implement_any! { self, audio, audio.play() }
    }
    /// Pauses playback of the audio device.
    pub fn pause(&self) -> Result<(), AudioHandleError> {
        implement_any! { self, audio, audio.pause() }
    }
    /// Closes audio playback and frees underlying resources.
    pub fn close(self) {}
    /// Calls the underlying [AudioFrameProducer::send_frame].
    pub fn send_frame(&mut self) -> AudioFrameResult<()> {
        implement_any! { self, audio, audio.producer.send_frame() }
    }
    /// Creates an instance of the [AudioHandleAnyFormat] from the provided **cpal** `host` with
    /// the default output device and the default audio parameters.
    ///
    /// * `frame_duration_nanos` is the duration in nanoseconds of the standard emulation frame.
    /// * `latency` is the audio latency passed to the [create_carousel].
    pub fn create(
            host: &cpal::Host,
            frame_duration_nanos: u32,
            latency: usize
        ) -> Result<Self, AudioHandleError>
    {
        let device = host.default_output_device()
                     .ok_or_else(|| ("no default output device".to_string(),
                                    AudioHandleErrorKind::AudioSubsystem))?;
        Self::create_with_device(&device, frame_duration_nanos, latency)
    }
    /// Creates an instance of the [AudioHandleAnyFormat] from the provided **cpal** `device`
    /// with the default audio parameters.
    ///
    /// * `frame_duration_nanos` is the duration in nanoseconds of the standard emulation frame.
    /// * `latency` is the audio latency passed to the [create_carousel].
    pub fn create_with_device(
            device: &cpal::Device,
            frame_duration_nanos: u32,
            latency: usize
        ) -> Result<Self, AudioHandleError>
    {
        let config = device.default_output_config()?.config();
        Self::create_with_device_and_config(
            device,
            &config,
            frame_duration_nanos,
            latency,
        )
    }
    /// Creates an instance of the [AudioHandleAnyFormat] from the provided **cpal** `device`
    /// with the desired audio parameters and default sample format.
    ///
    /// * `config` specifies the desired audio parameters.
    /// * `frame_duration_nanos` is the duration in nanoseconds of the standard emulation frame.
    /// * `latency` is the audio latency passed to the [create_carousel].
    pub fn create_with_device_and_config(
            device: &cpal::Device,
            config: &cpal::StreamConfig,
            frame_duration_nanos: u32,
            latency: usize,
        ) -> Result<Self, AudioHandleError>
    {
        let sample_format = device.default_output_config()?.sample_format();
        Self::create_with_device_config_and_sample_format(
            device, config, sample_format, frame_duration_nanos, latency)
    }
    /// Creates an instance of the [AudioHandleAnyFormat] from the provided **cpal** `device`
    /// with the desired audio parameters and sample format.
    ///
    /// * `config` specifies the desired audio parameters.
    /// * `frame_duration_nanos` is the duration in nanoseconds of the standard emulation frame.
    /// * `latency` is the audio latency passed to the [create_carousel].
    pub fn create_with_device_config_and_sample_format(
            device: &cpal::Device,
            config: &cpal::StreamConfig,
            sample_format: cpal::SampleFormat,
            frame_duration_nanos: u32,
            latency: usize,
        ) -> Result<Self, AudioHandleError>
    {
        Ok(match sample_format {
            SampleFormat::I8 => AudioHandleAnyFormat::I8(
                AudioHandle::<i8>::create_with_device_and_config(device, config, frame_duration_nanos, latency)?
            ),
            SampleFormat::I16 => AudioHandleAnyFormat::I16(
                AudioHandle::<i16>::create_with_device_and_config(device, config, frame_duration_nanos, latency)?
            ),
            SampleFormat::I32 => AudioHandleAnyFormat::I32(
                AudioHandle::<i32>::create_with_device_and_config(device, config, frame_duration_nanos, latency)?
            ),
            SampleFormat::I64 => AudioHandleAnyFormat::I64(
                AudioHandle::<i64>::create_with_device_and_config(device, config, frame_duration_nanos, latency)?
            ),
            SampleFormat::U8 => AudioHandleAnyFormat::U8(
                AudioHandle::<u8>::create_with_device_and_config(device, config, frame_duration_nanos, latency)?
            ),
            SampleFormat::U16 => AudioHandleAnyFormat::U16(
                AudioHandle::<u16>::create_with_device_and_config(device, config, frame_duration_nanos, latency)?
            ),
            SampleFormat::U32 => AudioHandleAnyFormat::U32(
                AudioHandle::<u32>::create_with_device_and_config(device, config, frame_duration_nanos, latency)?
            ),
            SampleFormat::U64 => AudioHandleAnyFormat::U64(
                AudioHandle::<u64>::create_with_device_and_config(device, config, frame_duration_nanos, latency)?
            ),
            SampleFormat::F32 => AudioHandleAnyFormat::F32(
                AudioHandle::<f32>::create_with_device_and_config(device, config, frame_duration_nanos, latency)?
            ),
            SampleFormat::F64 => AudioHandleAnyFormat::F64(
                AudioHandle::<f64>::create_with_device_and_config(device, config, frame_duration_nanos, latency)?
            ),
            sf => return Err((format!("unsupported sample format: {sf:?}"), AudioHandleErrorKind::InvalidArguments).into())
        })
    }
}

impl<T: cpal::SizedSample + AudioSample> AudioHandle<T> {
    /// Starts playback of the audio device.
    pub fn play(&self) -> Result<(), AudioHandleError> {
        self.stream.play().map_err(From::from)
    }
    /// Pauses playback of the audio device.
    pub fn pause(&self) -> Result<(), AudioHandleError> {
        self.stream.pause().map_err(From::from)
    }
    /// Closes audio playback and frees underlying resources.
    pub fn close(self) {}
    /// Creates an instance of the [AudioHandle] from the provided **cpal** `device` with the
    /// desired audio parameters.
    ///
    /// * `config` specifies the desired audio parameters.
    /// * `frame_duration_nanos` is the duration in nanoseconds of the standard emulation frame.
    /// * `latency` is the audio latency passed to the [create_carousel].
    pub fn create_with_device_and_config(
            device: &cpal::Device,
            config: &cpal::StreamConfig,
            frame_duration_nanos: u32,
            latency: usize,
        ) -> Result<Self, AudioHandleError>
    {
        let channels: u8 = config.channels.try_into()
                           .map_err(|_| (format!("number of channels: {} exceed the maximum value of 255", config.channels),
                                         AudioHandleErrorKind::InvalidArguments))?;
        let sample_rate = config.sample_rate.0;

        let frame_duration_secs = Duration::from_nanos(frame_duration_nanos.into()).as_secs_f64();
        let audio_frame_samples = (sample_rate as f64 * frame_duration_secs).ceil() as usize;
        debug!("audio specs: {:?}", config);
        debug!("audio frame samples: {} latency: {}", audio_frame_samples, latency);
        let (producer, mut consumer) = create_carousel::<T>(latency, audio_frame_samples, channels);

        let data_fn = move |out: &mut [T], _: &_| match consumer.fill_buffer(out, false) {
            Ok(unfilled) => {
                if !unfilled.is_empty() {
                    for t in unfilled {
                        *t = T::silence()
                    }
                    debug!("missing buffer");
                }
            }
            Err(_) => {
                error!("fatal: producer terminated");
            }
        };

        let err_fn = |err| error!("an error occurred on stream: {}", err);

        let stream = device.build_output_stream(config, data_fn, err_fn, None)?;

        Ok(AudioHandle {
            sample_rate,
            channels,
            producer,
            stream
        })
    }
}

impl From<PlayStreamError> for AudioHandleError {
    fn from(e: PlayStreamError) -> Self {
        let kind = match e {
            PlayStreamError::DeviceNotAvailable => AudioHandleErrorKind::AudioSubsystem,
            _ => AudioHandleErrorKind::AudioStream
        };
        (e.to_string(), kind).into()
    }
}

impl From<PauseStreamError> for AudioHandleError {
    fn from(e: PauseStreamError) -> Self {
        let kind = match e {
            PauseStreamError::DeviceNotAvailable => AudioHandleErrorKind::AudioSubsystem,
            _ => AudioHandleErrorKind::AudioStream
        };
        (e.to_string(), kind).into()
    }
}

impl From<DefaultStreamConfigError> for AudioHandleError {
    fn from(e: DefaultStreamConfigError) -> Self {
        let kind = match e {
            DefaultStreamConfigError::StreamTypeNotSupported => AudioHandleErrorKind::InvalidArguments,
            _ => AudioHandleErrorKind::AudioSubsystem
        };
        (e.to_string(), kind).into()
    }
}

impl From<BuildStreamError> for AudioHandleError {
    fn from(e: BuildStreamError) -> Self {
        let kind = match e {
            BuildStreamError::DeviceNotAvailable => AudioHandleErrorKind::AudioSubsystem,
            BuildStreamError::StreamConfigNotSupported|
            BuildStreamError::InvalidArgument => AudioHandleErrorKind::InvalidArguments,
            _ => AudioHandleErrorKind::AudioStream
        };
        (e.to_string(), kind).into()
    }
}
