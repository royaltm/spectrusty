/*
    audio_tap_cpal: ZX Spectrum TAPE sound demo!
    Copyright (C) 2020-2022  Rafal Michalski

    audio_tap_cpal is free software: you can redistribute it and/or modify
    it under the terms of the GNU General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    audio_tap_cpal is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU General Public License for more details.

    You should have received a copy of the GNU General Public License
    along with this program.  If not, see <https://www.gnu.org/licenses/>.

    Author contact information: see Cargo.toml file, section [package.authors].
*/
use std::io::{Cursor, Read, Seek};
use spectrusty::audio::{
    synth::*,
    host::cpal::{AudioHandle, AudioHandleAnyFormat}
};
use spectrusty::audio::*;
use spectrusty::chip::nanos_from_frame_tc_cpu_hz;
use spectrusty::formats::tap::{*, pulse::*};
/****************************************************************************/
/*                                   MAIN                                   */
/****************************************************************************/
type WavWriter = hound::WavWriter<std::io::BufWriter<std::fs::File>>;

const FRAME_TSTATES: i32 = 69888;
const CPU_HZ: u32 = 3_500_000;
const AMPLITUDE: f32 = 0.1;
const AUDIO_LATENCY: usize = 1;

fn produce<T: 'static + FromSample<i16> + AudioSample + cpal::Sample + Send, R: Read + Seek>(
        mut audio: AudioHandle<T>,
        read: R,
        mut writer: WavWriter
    )
    where i16: IntoSample<T>
{
    // create a band-limited pulse buffer with 1 channel
    let mut bandlim: BandLimited<i16> = BandLimited::new(1);
    // ensure BLEP has enough space to fit a single audio frame (no margin - our frames will have constant size)
    bandlim.ensure_frame_time(audio.sample_rate, CPU_HZ as f64, FRAME_TSTATES, 0);
    let channels = audio.channels as usize;
    let mut tstamp: i32 = 0;
    let mut delta: i16 = i16::from_sample(1.0f32);
    // let mut delta: f32 = 1.0;
    // create TapChunkReader
    let mut tap_reader = read_tap(read);
    // create ReadEncPulseIter from a cursor from a vector
    let mut tap_pulse_iter = ReadEncPulseIter::new(Cursor::new(Vec::new()));
    let mut done = false;
    // render frames
    while !done {
        // add pulses that fits the currently rendered frame
        while tstamp < FRAME_TSTATES {
            bandlim.add_step(0, tstamp, delta);
            delta = -delta;
            // ReadEncPulseIter yields delta timestamps (in T-states) between pulses
            match tap_pulse_iter.next() {
                Some(delta) => {
                    tstamp += delta.get() as i32;
                    if tap_pulse_iter.state().is_sync1() {
                        // print chunk info when rendering a sync pulse
                        let chunk = TapChunk::from(tap_pulse_iter.get_ref().get_ref()).validated().unwrap();
                        println!("{}", chunk);
                        if chunk.is_head() {
                            println!("{:?}", chunk);
                            if let Some(BlockType::NumberArray)|Some(BlockType::CharArray) = chunk.block_type() {
                                println!("array name: {}", chunk.array_name().unwrap());
                            }
                        }
                    }
                }
                None => {
                    // forward reader to the next tap chunk
                    if tap_reader.next_chunk().unwrap().is_some() {
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
                        tstamp += consts::PAUSE_PULSE_LENGTH.get() as i32;
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
        Blep::end_frame(&mut bandlim, FRAME_TSTATES);
        // render BLEP frame into the sample buffer
        audio.producer.render_frame(|ref mut vec| {
            let sample_iter = bandlim.sum_iter::<i16>(0); // channel 0
            // set sample buffer size so to the size of the BLEP frame
            vec.resize(sample_iter.len() * channels, T::silence());
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
        // send sample buffer to the consumer
        audio.producer.send_frame().unwrap();
        // prepare BLEP for the next frame
        bandlim.next_frame();
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
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!(r#"audio_tap_cpal  Copyright (C) 2020-2022  Rafal Michalski
This program comes with ABSOLUTELY NO WARRANTY.
This is free software, and you are welcome to redistribute it under certain conditions."#);

    let frame_duration_nanos = nanos_from_frame_tc_cpu_hz(FRAME_TSTATES as u32, CPU_HZ) as u32;
    let audio = AudioHandleAnyFormat::create(&cpal::default_host(),
                                             frame_duration_nanos,
                                             AUDIO_LATENCY)?;
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: audio.sample_rate(),
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let tap_name = std::env::args().nth(1).unwrap_or_else(|| "../../resources/read_tap_test.tap".into());
    println!("Loading TAP: {}", tap_name);
    let file = std::fs::File::open(&tap_name)?;

    let wav_name = "tap_output.wav";
    println!("Creating WAV: {:?}", wav_name);
    let writer = WavWriter::create(wav_name, spec)?;

    audio.play()?;

    match audio {
        AudioHandleAnyFormat::I16(audio) => produce::<i16,_>(audio, file, writer),
        AudioHandleAnyFormat::U16(audio) => produce::<u16,_>(audio, file, writer),
        AudioHandleAnyFormat::F32(audio) => produce::<f32,_>(audio, file, writer),
    }

    Ok(())
}
