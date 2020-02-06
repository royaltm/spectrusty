//! ZX Spectrum tape sound PORN!!!!
#[path = "../tests/audio_cpal.rs"]
mod audio_cpal;
use std::io::{Seek, Read, Cursor};

use audio_cpal::*;

use zxspecemu::audio::carousel::*;
use zxspecemu::audio::sample::*;
use zxspecemu::audio::*;
use zxspecemu::audio::ay::*;
use zxspecemu::audio::music::*;
use zxspecemu::formats::read_ear::*;
use zxspecemu::formats::tap::*;
/****************************************************************************/
/*                                   MAIN                                   */
/****************************************************************************/
type WavWriter = hound::WavWriter<std::io::BufWriter<std::fs::File>>;

const FRAME_TSTATES: i32 = 69888;
const CPU_HZ: u32 = 3_500_000;
const AMPLITUDE: f32 = 0.1;

fn produce<T: 'static + FromSample<i16> + AudioSample + Send,
           R: Read + Seek>(mut audio: Audio<T>, read: R, mut writer: WavWriter)
where i16: IntoSample<T>
{
    // create a band-limited pulse buffer with 1 channel
    let mut bandlim: BandLimited<i16> = BandLimited::new(1);
    let time_rate = <f64 as SampleTime>::time_rate(audio.sample_rate, CPU_HZ);
    // ensure BLEP has enough space to fit a single audio frame (no margin - our frames will have constant size)
    bandlim.ensure_frame_time(time_rate.at_timestamp(FRAME_TSTATES), time_rate.at_timestamp(0));
    let channels = audio.channels as usize;
    let mut tstamp: i32 = 0;
    let mut delta: i16 = i16::from_sample(1.0f32);
    // let mut delta: f32 = 1.0;
    // create TapChunkReader
    let mut tap_reader = read_tap(read);
    // create EarPulseIter from a cursor from a vector
    let mut tap_pulse_iter = EarPulseIter::new(Cursor::new(Vec::new()));
    let mut done = false;
    // render frames
    while !done {
        // add pulses that fits the currently rendered frame
        while tstamp < FRAME_TSTATES {
            bandlim.add_step(0, time_rate.at_timestamp(tstamp), delta);
            delta = -delta;
            // EarPulseIter yields delta timestamps (in T-states) between pulses
            match tap_pulse_iter.next() {
                Some(delta) => {
                    tstamp += delta.get() as i32;
                    if tap_pulse_iter.state().is_sync1() {
                        // print chunk info when rendering a sync pulse
                        let chunk = TapChunk::from(tap_pulse_iter.get_ref().get_ref()).validated().unwrap();
                        println!("{}", chunk);
                    }
                }
                None => {
                    // forward reader to the next tap chunk
                    if tap_reader.next_chunk().unwrap() != 0 {
                        let cursor = tap_pulse_iter.get_mut();
                        // reset cursor
                        cursor.set_position(0);
                        let tap_buf = cursor.get_mut();
                        // load tap data into the buffer
                        tap_buf.clear();
                        tap_reader.read_to_end(tap_buf).unwrap();
                        // reset pulse iterator
                        tap_pulse_iter.reset();
                        if let Some(err) = tap_pulse_iter.err() {
                            panic!("{:?}", err)
                        }
                        // pause between chunks
                        tstamp += PAUSE_PULSE_LENGTH.get() as i32;
                    }
                    else {
                        // no more chunks
                        done = true;
                        break;
                    }
                }
            }
        }
        tstamp -= FRAME_TSTATES;
        // close current frame
        bandlim.end_frame(time_rate.at_timestamp(FRAME_TSTATES));
        // render BLEP frame into the sample buffer
        audio.producer.render_frame(|ref mut vec| {
            let sample_iter = bandlim.sum_iter::<i16>(0); // channel 0
            // set sample buffer size so to the size of the BLEP frame
            vec.resize(sample_iter.len() * channels, T::zero());
            // render each sample
            for (chans, sample) in vec.chunks_mut(channels).zip(sample_iter) {
                // write to the wav file
                writer.write_sample(sample).unwrap();
                // convert sample type
                let sample = T::from_sample(sample.mul_norm(i16::from_sample(AMPLITUDE)));
                // write sample to each channel
                for p in chans {
                    *p = sample;
                }
            }
        });
        // prepare BLEP for the next frame
        bandlim.next_frame();
        // send sample buffer to the consumer
        audio.producer.send_frame().unwrap();
    }
    // render 3 silent buffers before stopping
    for _ in 0..3 {
        audio.producer.render_frame(|ref mut vec| {
            let sample: T = 0i16.into_sample();
            for p in vec.iter_mut() {
                *p = sample;
            }
        });
        audio.producer.send_frame().unwrap();
    }

    let Audio { join_handle, producer, ..} = audio;
    // drop producer so the consumer should quit
    drop(producer);
    // drop writer so the file will close
    drop(writer);
    // wait for the consumer thread to quit
    join_handle.join().unwrap();
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let adf = AudioDeviceFormat::find_default()?;
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: adf.format.sample_rate.0,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let tap_name = std::env::args().nth(1).unwrap_or_else(|| "tests/read_tap_test.tap".into());
    println!("Loading TAP: {}", tap_name);
    let file = std::fs::File::open(&tap_name)?;

    let wav_name = "tap_output.wav";
    println!("Creating WAV: {:?}", wav_name);
    let writer = WavWriter::create(wav_name, spec)?;

    let frame_duration: f64 = FRAME_TSTATES as f64 / CPU_HZ as f64;
    let audio = adf.play(1, frame_duration)?;

    match audio {
        AudioUnknownDataType::I16(audio) => produce::<i16,_>(audio, file, writer),
        AudioUnknownDataType::U16(audio) => produce::<u16,_>(audio, file, writer),
        AudioUnknownDataType::F32(audio) => produce::<f32,_>(audio, file, writer),
    }

    Ok(())
}
