/*
    audio_ay_cpal: demonstrates how to render sound directly from the
                   Ay3_891xAudio emulator.
    Copyright (C) 2020-2023  Rafal Michalski

    audio_ay_cpal is free software: you can redistribute it and/or modify
    it under the terms of the GNU General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    audio_ay_cpal is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU General Public License for more details.

    You should have received a copy of the GNU General Public License
    along with this program.  If not, see <https://www.gnu.org/licenses/>.

    Author contact information: see Cargo.toml file, section [package.authors].
*/
use spectrusty::audio::{
    music::*,
    synth::*,
    host::cpal::{AudioHandle, AudioHandleAnyFormat}
};
use spectrusty::audio::*;
use spectrusty::chip::nanos_from_frame_tc_cpu_hz;
use spectrusty::peripherals::ay::{audio::*, AyRegister, AyRegChange};

/****************************************************************************/
/*                                   MAIN                                   */
/****************************************************************************/
const FRAME_TSTATES: i32 = 70908;
const CPU_HZ: u32 = 3_546_900;
const AUDIO_LATENCY: usize = 5;

// A type for the BandLimited sample difference buffer.
type SDT = f32; // i16, i32, f32, f64

fn produce<T>(mut audio: AudioHandle<T>)
    where T: FromSample<SDT> + AudioSample + cpal::SizedSample
{
    // create a band-limited pulse buffer with 3 channels
    let mut bandlim: BlepStereo<BandLimited<SDT>> = BlepStereo::build(SDT::from_sample(0.8))(BandLimited::new(2));
    // ensure BLEP has enough space to fit a single audio frame (no margin - our frames will have constant size)
    bandlim.ensure_frame_time(audio.sample_rate, CPU_HZ as f64, FRAME_TSTATES, 0);
    let channels = audio.channels as usize;

    let mut ay = Ay3_891xAudio::default();
    let mut notes: Vec<u16> = Vec::new();
    let note_scale = equal_tempered_scale_note_freqs(440.0, 0, 12);
    let note_period_iter = Ay3_891xAudio::tone_periods(CPU_HZ as f32/2.0, 0, 7, note_scale);
    notes.extend(note_period_iter);

    let mut changes = vec![
        AyRegChange::new(0, AyRegister::MixerControl, 0b00111101),
        AyRegChange::new(0, AyRegister::NoisePeriod, 14),
        AyRegChange::new(0, AyRegister::AmpLevelB, 16),
        AyRegChange::new(0, AyRegister::ToneFineB, notes[6*12] as u8), 
        AyRegChange::new(0, AyRegister::ToneCoarseB, (notes[6*12] >> 8) as u8),
        AyRegChange::new(0, AyRegister::EnvPerFine, 116),
        AyRegChange::new(0, AyRegister::EnvPerCoarse, 10),
        AyRegChange::new(0, AyRegister::EnvShape, ENV_SHAPE_CONT_MASK),
    ];
    // render frames
    loop {
        ay.render_audio::<AyAmps<SDT>,_,_>(changes.drain(..),
                                        &mut bandlim, FRAME_TSTATES, FRAME_TSTATES, [0, 1, 2]);
        // close current frame
        let frame_sample_count = Blep::end_frame(&mut bandlim, FRAME_TSTATES);
        // render BLEP frame into the sample buffer
        audio.producer.render_frame(|ref mut vec| {
            // set sample buffer size so to the size of the BLEP frame
            vec.resize(frame_sample_count * channels, T::silence());
            // render each stereo sample into the first two audio channels
            for chan in 0..=1 {
                for (p, sample) in vec[chan..].iter_mut()
                                   .step_by(channels)
                                   .zip(bandlim.sum_iter::<T>(chan))
                {
                    *p = sample;
                }
            }
        });
        // send sample buffer to the consumer
        audio.producer.send_frame().unwrap();
        // prepare BLEP for the next frame
        bandlim.next_frame();
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!(r#"audio_ay_cpal  Copyright (C) 2020-2023  Rafal Michalski
This program comes with ABSOLUTELY NO WARRANTY.
This is free software, and you are welcome to redistribute it under certain conditions."#);

    let frame_duration_nanos = nanos_from_frame_tc_cpu_hz(FRAME_TSTATES as u32, CPU_HZ) as u32;
    let audio = AudioHandleAnyFormat::create(&cpal::default_host(),
                                             frame_duration_nanos,
                                             AUDIO_LATENCY)?;
    audio.play()?;

    match audio {
        AudioHandleAnyFormat::I8(audio) => produce::<i8>(audio),
        AudioHandleAnyFormat::U8(audio) => produce::<u8>(audio),
        AudioHandleAnyFormat::I16(audio) => produce::<i16>(audio),
        AudioHandleAnyFormat::U16(audio) => produce::<u16>(audio),
        AudioHandleAnyFormat::I32(audio) => produce::<i32>(audio),
        AudioHandleAnyFormat::U32(audio) => produce::<u32>(audio),
        AudioHandleAnyFormat::I64(audio) => produce::<i64>(audio),
        AudioHandleAnyFormat::U64(audio) => produce::<u64>(audio),
        AudioHandleAnyFormat::F32(audio) => produce::<f32>(audio),
        AudioHandleAnyFormat::F64(audio) => produce::<f64>(audio),
        _ => Err("Unsupported audio format!")?,
    }

    Ok(())
}
