use std::convert::TryInto;
use std::thread::JoinHandle;
use std::thread;

#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

use cpal::traits::{DeviceTrait, EventLoopTrait, HostTrait};
use cpal::{UnknownTypeOutputBuffer, Format, SampleFormat, Sample, SampleRate};

use zxspecemu::audio::carousel::*;

pub enum AudioUnknownDataType {
    I16(Audio<i16>),
    U16(Audio<u16>),
    F32(Audio<f32>),
}

pub struct Audio<T> {
    pub sample_rate: u32,
    pub channels: u8,
    pub producer: AudioFrameProducer<T>,
    pub join_handle: JoinHandle<()>
}

pub struct AudioDeviceFormat {
    pub host: cpal::Host,
    pub device: cpal::Device,
    pub format: Format,
}

trait UnwrapUnknownTypeOutputBuffer: Sized {
    fn unwrap_buffer<'a>(buffer: &'a mut UnknownTypeOutputBuffer<'_>) -> &'a mut [Self];
}

macro_rules! impl_unwrap_unknown_type_output_buffer {
    ($ty:ty, $det:ident) => {
        impl UnwrapUnknownTypeOutputBuffer for $ty {
            fn unwrap_buffer<'a>(buffer: &'a mut UnknownTypeOutputBuffer<'_>) -> &'a mut [$ty] {
                match buffer {
                    UnknownTypeOutputBuffer::$det(ref mut buf) => buf,
                    _ => panic!("unexpected sample format")
                }
            }
        }
    };
}

impl_unwrap_unknown_type_output_buffer!(i16, I16);
impl_unwrap_unknown_type_output_buffer!(u16, U16);
impl_unwrap_unknown_type_output_buffer!(f32, F32);

impl AudioDeviceFormat {
    pub fn find_default() -> Result<Self, Box<dyn std::error::Error>> {
        debug!("{:?}", cpal::available_hosts());
        let host = cpal::default_host();
        for device in host.output_devices()? {
            debug!("device: {}", device.name()?);
            let supported_formats_range = device.supported_output_formats()?;
            for format in supported_formats_range {
                debug!("  output: {:?}", format);
            }
            let supported_formats_range = device.supported_input_formats()?;
            for format in supported_formats_range {
                debug!("  input: {:?}", format);
            }
        }
        let device = host.default_output_device().ok_or("failed to find a default output device")?;
        debug!("Default device: {}", device.name()?);

        let format = device.default_output_format()?;
        debug!("Default format: {:?}", format);
        Ok(AudioDeviceFormat { host, device, format })
    }

    pub fn play(self, latency: usize, frame_duration: f64) -> Result<AudioUnknownDataType, Box<dyn std::error::Error>> {
        let AudioDeviceFormat { host, device, format: Format { sample_rate, channels, data_type} } = self;
        match data_type {
            SampleFormat::I16 => {
                play::<i16>(latency, frame_duration, host, device, sample_rate.0, channels.try_into()? )
                .map(|audio| AudioUnknownDataType::I16(audio))
            }
            SampleFormat::U16 => {
                play::<u16>(latency, frame_duration, host, device, sample_rate.0, channels.try_into()? )
                .map(|audio| AudioUnknownDataType::U16(audio))
            }
            SampleFormat::F32 => {
                play::<f32>(latency, frame_duration, host, device, sample_rate.0, channels.try_into()? )
                .map(|audio| AudioUnknownDataType::F32(audio))
            }
        }
    }
}

fn play<T>(latency: usize, frame_duration: f64,
           host: cpal::Host, device: cpal::Device, sample_rate: u32, channels: u8) -> Result<Audio<T>, Box<dyn std::error::Error>>
where T: 'static + Sample + AudioSample + Send + UnwrapUnknownTypeOutputBuffer
{
    let format = Format {
        channels: channels.into(),
        sample_rate: SampleRate(sample_rate),
        data_type: T::get_format()
    };

    let event_loop = host.event_loop();
    let stream_id = event_loop.build_output_stream(&device, &format)?;

    let sample_rate = format.sample_rate.0;
    let channels: u8 = format.channels.try_into()?;
    let frame_samples = (sample_rate as f64 * frame_duration).ceil() as usize;

    let (producer, mut consumer) = create_carousel::<T>(latency, frame_samples, channels);

    event_loop.play_stream(stream_id)?;

    let join_handle = thread::spawn(move || {
        event_loop.run(|stream_id, result| {
            let mut data = match result {
                Ok(data) => data,
                Err(err) => {
                    debug!("an error occurred on stream {:?}: {}", stream_id, err);
                    return;
                }
            };

            let res = match data {
                cpal::StreamData::Output { ref mut buffer } => {
                    consumer.fill_buffer(T::unwrap_buffer(buffer), false)
                }
                _ => panic!("unexpected input data")
            };
            match res {
                Ok(unfilled) => {
                    if unfilled.len() != 0 {
                        debug!("missing buffer");
                    }
                }
                Err(_) => {
                    debug!("no remote, terminating stream");
                    event_loop.destroy_stream(stream_id);
                }
            }

        });
    });
    Ok(Audio {
        sample_rate,
        channels,
        producer,
        join_handle
    })
}
