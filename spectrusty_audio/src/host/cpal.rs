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

/// The struct for producing and controling the audio playback.
///
/// It embeds the interconnected pair of [carousel]'s [AudioFrameProducer] with the [AudioFrameConsumer]
/// directly exposing the `producer` to the user. The consumer lives in the **cpal** audio thread and is
/// responsible for filling up the audio output buffer with sample data sent by the `producer`.
///
/// The `T` parameter should be one of the [sample primitives][cpal::Sample].
pub struct AudioHandle<T: cpal::Sample + AudioSample> {
    /// The audio sample frequency of the output stream.
    pub sample_rate: u32,
    /// The number of audio channels in the output stream.
    pub channels: u8,
    /// The audio sample producer interconnected with an audio consumer living in the audio thread.
    pub producer: AudioFrameProducer<T>,
    stream: Stream
}

/// The enum for producing and controling the audio playback regardless of the sample format used.
pub enum AudioHandleAnyFormat {
    I16(AudioHandle<i16>),
    U16(AudioHandle<u16>),
    F32(AudioHandle<f32>),
}

impl AudioHandleAnyFormat {
    /// Returns the sample format of the current variant.
    pub fn sample_format(&self) -> SampleFormat {
        use AudioHandleAnyFormat::*;
        match self {
            I16(..) => SampleFormat::I16,
            U16(..) => SampleFormat::U16,
            F32(..) => SampleFormat::F32,
        }
    }
    /// Returns the audio sample frequency of the output stream.
    pub fn sample_rate(&self) -> u32 {
        use AudioHandleAnyFormat::*;
        match self {
            I16(audio) => audio.sample_rate,
            U16(audio) => audio.sample_rate,
            F32(audio) => audio.sample_rate,
        }
    }
    /// Returns the number of audio channels in the output stream.
    pub fn channels(&self) -> u8 {
        use AudioHandleAnyFormat::*;
        match self {
            I16(audio) => audio.channels,
            U16(audio) => audio.channels,
            F32(audio) => audio.channels,
        }
    }
    /// Starts playback of the audio device.
    pub fn play(&self) -> Result<(), AudioHandleError> {
        use AudioHandleAnyFormat::*;
        match self {
            I16(audio) => audio.play(),
            U16(audio) => audio.play(),
            F32(audio) => audio.play(),
        }
    }
    /// Pauses playback of the audio device.
    pub fn pause(&self) -> Result<(), AudioHandleError> {
        use AudioHandleAnyFormat::*;
        match self {
            I16(audio) => audio.pause(),
            U16(audio) => audio.pause(),
            F32(audio) => audio.pause(),
        }
    }
    /// Closes audio playback and frees underlying resources.
    pub fn close(self) {}
    /// Calls the underlying [AudioFrameProducer::send_frame].
    pub fn send_frame(&mut self) -> AudioFrameResult<()> {
        use AudioHandleAnyFormat::*;
        match self {
            I16(audio) => audio.producer.send_frame(),
            U16(audio) => audio.producer.send_frame(),
            F32(audio) => audio.producer.send_frame(),
        }
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
                     .ok_or_else(|| (format!("no default output device"),
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
    /// with the desired audio parameters and sample format.
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
        Ok(match device.default_output_config()?.sample_format() {
            SampleFormat::I16 => AudioHandleAnyFormat::I16(
                AudioHandle::<i16>::create_with_device_and_config(device, config, frame_duration_nanos, latency)?
            ),
            SampleFormat::U16 => AudioHandleAnyFormat::U16(
                AudioHandle::<u16>::create_with_device_and_config(device, config, frame_duration_nanos, latency)?
            ),
            SampleFormat::F32 => AudioHandleAnyFormat::F32(
                AudioHandle::<f32>::create_with_device_and_config(device, config, frame_duration_nanos, latency)?
            )
        })
    }
}

impl<T: cpal::Sample + AudioSample> AudioHandle<T> {
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
                if unfilled.len() != 0 {
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

        let stream = device.build_output_stream(config, data_fn, err_fn)?;

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
