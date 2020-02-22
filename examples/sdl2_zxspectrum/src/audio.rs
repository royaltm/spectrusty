use core::ops::{Deref, DerefMut};
use core::convert::TryInto;
use core::time::Duration;
use std::error::Error;

#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

use sdl2::{Sdl, audio::{AudioCallback, AudioSpecDesired}};
pub use sdl2::audio::AudioDevice;

use zxspecemu::audio::carousel::*;

impl AudioCallback for AudioCb<f32> {
    type Channel = f32;

    fn callback(&mut self, out: &mut [f32]) {
        // let now = std::time::Instant::now();
        // self.2 += 1;
        // let since = now.duration_since(self.1) / self.2;
        // println!("buff since: {:?} cnt {}", since, self.2);
        // self.1 = now;
        match self.fill_buffer(out, false) {
            Ok(unfilled) => {
                // println!("buff since: {:?} dur {:?} {} cnt {}", since, now.elapsed(), unfilled.len(), self.2);
                if unfilled.len() != 0 {
                    debug!("missing buffer");
                }
            }
            Err(_) => {
                error!("fatal: producer terminated");
            }
        }
    }
}

#[derive(Debug)]
struct AudioCb<T>(AudioFrameConsumer<T>, std::time::Instant, u32);

impl<T> Deref for AudioCb<T> {
    type Target = AudioFrameConsumer<T>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for AudioCb<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub struct Audio {
    pub sample_rate: u32,
    pub channels: u8,
    pub producer: AudioFrameProducer<f32>,
    device: AudioDevice<AudioCb<f32>>,
}

const SAMPLE_RATE: i32 = 44100;
const CHANNELS: u8 = 2;

impl Audio {
    pub fn resume(&mut self) {
        self.device.resume()
    }
    pub fn pause(&mut self) {
        self.device.pause()
    }
    pub fn close(self) -> AudioFrameConsumer<f32> {
        debug!("closing audio");
        self.device.close_and_get_callback().0
    }
    pub fn create(
                sdl_context: &Sdl,
                latency: usize,
                frame_duration_nanos: u32,
            ) -> Result<Audio, Box<dyn Error>>
    {
        let frame_duration_secs = Duration::from_nanos(frame_duration_nanos.into()).as_secs_f64();
        let audio_subsystem = sdl_context.audio()?;
        let desired_sample_frames = (SAMPLE_RATE as f64 * frame_duration_secs).ceil() as usize;
        let samples: u16 = (desired_sample_frames * latency).checked_next_power_of_two().unwrap().try_into().unwrap();
        let latency = (samples as usize / desired_sample_frames) + 1;
        let desired_spec = AudioSpecDesired {
            freq: Some(SAMPLE_RATE),
            channels: Some(CHANNELS),
            samples: Some(samples),
        };

        let mut producer: Option<AudioFrameProducer<f32>> = None;
        let device = audio_subsystem.open_playback(None, &desired_spec, |spec| {
            // Show obtained AudioSpec
            debug!("{:?}", spec);
            let sample_frames = (spec.freq as f64 * frame_duration_secs).ceil() as usize;
            debug!("Sample frames: {} latency: {}", sample_frames, latency);
            let (prod, consumer) = create_carousel::<f32>(latency, sample_frames, spec.channels);
            producer = Some(prod);
            AudioCb(consumer, std::time::Instant::now(), 0)
        })?;

        // let (sample_rate, channels) = {
        //     let lock = device.lock();
        //     ((*lock).freq as u32, (*lock).channels)
        // };
        let spec = device.spec();

        Ok(Audio{
            sample_rate: spec.freq as u32,
            channels: spec.channels,
            producer: producer.unwrap(),
            device
        })
    }
}
