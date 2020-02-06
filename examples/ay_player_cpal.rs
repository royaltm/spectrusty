//! ZX Spectrum tape sound PORN!!!!
#[path = "../tests/audio_cpal.rs"]
mod audio_cpal;
use std::io::{Read};
use core::time::Duration;

use audio_cpal::*;

use z80emu::{Cpu, Z80NMOS};
use zxspecemu::memory::ZxMemory;
use zxspecemu::audio::carousel::*;
use zxspecemu::audio::sample::*;
use zxspecemu::audio::*;
use zxspecemu::audio::ay::*;
use zxspecemu::formats::sna::*;
use zxspecemu::chip::*;
use zxspecemu::chip::ay_player::*;
/****************************************************************************/
/*                                   MAIN                                   */
/****************************************************************************/
type Ay128kPlayer = AyPlayer<Ay128kPortDecode>;
type WavWriter = hound::WavWriter<std::io::BufWriter<std::fs::File>>;

fn produce<T: 'static + FromSample<f32> + AudioSample + Send, R: Read>(mut audio: Audio<T>, rd: R, mut writer: WavWriter)
where i16: IntoSample<T>
{
    let output_channels = audio.channels as usize;
    let mut bandlim: BandLimited<f32> = BandLimited::new(3);
    let time_rate = Ay128kPlayer::ensure_audio_frame_time(&mut bandlim, audio.sample_rate);
    let mut cpu = Z80NMOS::default();
    let mut player = Ay128kPlayer::default();
    player.reset(&mut cpu, true);
    let rom_file = std::fs::File::open("resources/48k.rom").unwrap();
    player.memory.load_into_rom_page(0, rom_file).unwrap();
    read_sna(rd, &mut cpu, &mut player.memory).unwrap();
    let mut tsync = ThreadSyncTimer::new();
    println!("CPU: {:?}", cpu);
    // render frames
    loop {
        player.execute_next_frame(&mut cpu);
        player.render_ay_audio_frame::<AyAmpLevels<f32>>(&mut bandlim, time_rate, [0, 1, 2]);
        // close current frame
        let frame_sample_count = player.end_audio_frame(&mut bandlim, time_rate);
        // render BLEP frame into the sample buffer
        audio.producer.render_frame(|ref mut vec| {
            let sample_iter = bandlim.sum_iter::<T>(0)
                            .zip(bandlim.sum_iter::<T>(1)
                                .zip(bandlim.sum_iter::<T>(2)))
                            .map(|(a,(b,c))| [a,b,c]);
            // set sample buffer size so to the size of the BLEP frame
            vec.resize(frame_sample_count * output_channels, T::zero());
            // render each sample
            for (chans, samples) in vec.chunks_mut(output_channels).zip(sample_iter) {
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
        if let Err(_) = tsync.synchronize_thread_to_frame::<Ay128kPlayer>() {
            println!("frame too long");
        }
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

    let file_name = std::env::args().nth(1).unwrap_or_else(|| "tests/DeathWish3.sna".into());
    println!("Loading SNA: {}", file_name);
    let file = std::fs::File::open(&file_name)?;

    let wav_name = "tap_output.wav";
    println!("Creating WAV: {:?}", wav_name);
    let writer = WavWriter::create(wav_name, spec)?;

    let frame_duration = Duration::from_nanos(Ay128kPlayer::FRAME_TIME_NANOS.into());
    let latency = if cfg!(debug_assertions) { 3 } else { 1 };
    let audio = adf.play(latency, frame_duration.as_secs_f64())?;

    match audio {
        AudioUnknownDataType::I16(audio) => produce::<i16,_>(audio, file, writer),
        AudioUnknownDataType::U16(audio) => produce::<u16,_>(audio, file, writer),
        AudioUnknownDataType::F32(audio) => produce::<f32,_>(audio, file, writer),
    }

    Ok(())
}
