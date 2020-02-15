//! ZX Spectrum tape sound PORN!!!!
#[path = "../tests/audio_cpal.rs"]
mod audio_cpal;

use audio_cpal::*;

use zxspecemu::audio::carousel::*;
use zxspecemu::audio::sample::*;
use zxspecemu::audio::*;
use zxspecemu::audio::ay::*;
use zxspecemu::audio::music::*;
use zxspecemu::audio::synth::*;
use zxspecemu::io::ay::{AyRegister, AyRegChange};

/****************************************************************************/
/*                                   MAIN                                   */
/****************************************************************************/
type WavWriter = hound::WavWriter<std::io::BufWriter<std::fs::File>>;

const FRAME_TSTATES: i32 = 70908;
const CPU_HZ: u32 = 3_546_900;

fn produce<T: 'static + FromSample<f32> + AudioSample + Send>(mut audio: Audio<T>, _writer: WavWriter)
where i16: IntoSample<T>
{
    // create a band-limited pulse buffer with 1 channel
    let mut bandlim: BandLimited<f32> = BandLimited::new(3);
    // ensure BLEP has enough space to fit a single audio frame (no margin - our frames will have constant size)
    bandlim.ensure_frame_time(audio.sample_rate, CPU_HZ, FRAME_TSTATES, 0);
    let channels = audio.channels as usize;

    let mut ay = Ay3_891xAudio::default();
    let mut notes: Vec<u16> = Vec::new();
    notes.extend(Ay3_891xAudio::tone_periods(CPU_HZ as f32/2.0, 0, 7, equal_tempered_scale_note_freqs(440.0, 0, 12)));

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
        ay.render_audio::<AyAmps<f32>,_,_,_>(changes.drain(..),
                                                    &mut bandlim, FRAME_TSTATES, [0, 1, 2]);
        // close current frame
        let frame_sample_count = Blep::end_frame(&mut bandlim, FRAME_TSTATES);
        // render BLEP frame into the sample buffer
        audio.producer.render_frame(|ref mut vec| {
            let sample_iter = bandlim.sum_iter::<T>(0)
                            .zip(bandlim.sum_iter::<T>(1)
                                .zip(bandlim.sum_iter::<T>(2)))
                            .map(|(a,(b,c))| [a,b,c]);
            // set sample buffer size so to the size of the BLEP frame
            vec.resize(frame_sample_count * channels, T::center());
            // render each sample
            for (chans, samples) in vec.chunks_mut(channels).zip(sample_iter) {
                // write to the wav file
                // writer.write_sample(sample).unwrap();
                // convert sample type
                // let sample = T::from_sample(sample);
                // write sample to each channel
                for (p, sample) in chans.iter_mut().zip(samples.iter()) {
                    *p = *sample;
                }
            }
        });
        // prepare BLEP for the next frame
        bandlim.next_frame();
        // send sample buffer to the consumer
        audio.producer.send_frame().unwrap();
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let adf = AudioDeviceFormat::find_default()?;
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: adf.format.sample_rate.0,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    // let tap_name = std::env::args().nth(1).unwrap_or_else(|| "tests/read_tap_test.tap".into());
    // println!("Loading TAP: {}", tap_name);
    // let file = std::fs::File::open(&tap_name)?;

    let wav_name = "tap_output.wav";
    println!("Creating WAV: {:?}", wav_name);
    let writer = WavWriter::create(wav_name, spec)?;

    let frame_duration: f64 = FRAME_TSTATES as f64 / CPU_HZ as f64;
    let audio = adf.play(1, frame_duration)?;

    match audio {
        AudioUnknownDataType::I16(audio) => produce::<i16>(audio, writer),
        AudioUnknownDataType::U16(audio) => produce::<u16>(audio, writer),
        AudioUnknownDataType::F32(audio) => produce::<f32>(audio, writer),
    }

    Ok(())
}
