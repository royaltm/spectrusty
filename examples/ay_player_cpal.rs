//! ZX Spectrum tape sound PORN!!!!
#[path = "../tests/audio_cpal.rs"]
mod audio_cpal;

use zxspecemu::memory::SinglePageMemory;
use core::num::NonZeroU32;
use std::io::{Read};
use core::time::Duration;

#[allow(unused_imports)]
use log::{error, warn, info, debug, trace, Level};

use audio_cpal::*;

use z80emu::Z80NMOS;
// use zxspecemu::cpu_debug::print_debug_memory;
use zxspecemu::memory::ZxMemory;
use zxspecemu::audio::carousel::*;
use zxspecemu::audio::sample::*;
use zxspecemu::audio::*;
use zxspecemu::audio::synth::*;
use zxspecemu::audio::ay::*;
use zxspecemu::formats::{ay::*, sna::*};
use zxspecemu::chip::*;
use zxspecemu::chip::ay_player::*;
/****************************************************************************/
/*                                   MAIN                                   */
/****************************************************************************/
type Ay128kPlayer = AyPlayer<Ay128kPortDecode>;
type WavWriter = hound::WavWriter<std::io::BufWriter<std::fs::File>>;

fn produce<T, R: Read>(mut audio: Audio<T>, rd: R, song_index: u16, mut first_length: Option<NonZeroU32>, _writer: Option<WavWriter>)
where T: 'static + FromSample<f32> + AudioSample + Send,
      i16: IntoSample<T>, f32: FromSample<T>,
{
    let output_channels = audio.channels as usize;
    let frame_time = Duration::from_nanos(Ay128kPlayer::FRAME_TIME_NANOS.into());
    // BandLimHiFi
    // BandLimLowTreb
    // BandLimLowBass
    // BandLimNarrow
    let mut bandlim: BlepAmpFilter<BandLimited<f32, BandLimHiFi>,f32> = BlepAmpFilter::new(BandLimited::new(3), 0.777);
    let time_rate = Ay128kPlayer::ensure_audio_frame_time(&mut bandlim, audio.sample_rate);
    let mut cpu = Z80NMOS::default();
    let mut player = Ay128kPlayer::default();
    player.reset(&mut cpu, true);
    let rom_file = std::fs::File::open("resources/48k.rom").unwrap();
    player.memory.load_into_rom_page(0, rom_file).unwrap();
    // read_sna(rd, &mut cpu, &mut player.memory).unwrap();
    let ay_file = match read_ay(rd) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("{}", e);
            panic!("ay loading error")
        }
    };
    debug!("{:?}", ay_file);
    println!("Author: {}", ay_file.meta.author);
    println!("Misc: {}", ay_file.meta.misc);
    println!("File version: {}", ay_file.meta.file_version);
    println!("Player version: {}", ay_file.meta.player_version);
    println!("Songs: {}", ay_file.songs.len());
    let mut song_index_iter = (0..ay_file.songs.len()).cycle();
    for _ in 0..song_index { song_index_iter.next(); }
    const DEFAULT_SONG_LENGTH: u32 = 5*60*50;
    for song_index in song_index_iter {
        let song_length: u32 = match first_length {
            Some(len) => len.get(),
            None => {
                let length = ay_file.songs[song_index].song_length as u32;
                if length == 0 { DEFAULT_SONG_LENGTH } else { length }
            }
        };
        first_length = None;
        println!("^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^");
        println!("Song name[{}]: {}", song_index, ay_file.songs[song_index].name);
        println!("Song length: {} frames", ay_file.songs[song_index].song_length);
        println!("Song fade: {} frames", ay_file.songs[song_index].fade_length);
        println!("Playing: {:?}", song_length * frame_time);
        player.reset(&mut cpu, true);
        player.reset_frames();
        ay_file.initialize_player(&mut cpu, &mut player.memory, song_index);
        debug!("CPU: {:?}", cpu);
        let mut tsync = ThreadSyncTimer::new();
        let start_time = tsync.time;
        // render frames
        // debug!("CURRENT FRAME: {} song_length: {}", player.current_frame(), song_length);
        while player.current_frame() <= song_length.into() {
        // loop {
            // debug!("frame_tstates: {}", player.frame_tstate());
            player.execute_next_frame(&mut cpu);
            player.render_ay_audio_frame::<AyAmps<f32>>(&mut bandlim, time_rate, [0, 1, 2]);
            // player.render_ay_audio_frame::<AyFuseAmps<f32>>(&mut bandlim, time_rate, [0, 1, 2]);
            player.render_earmic_out_audio_frame::<EarOutAmps4<f32>>(&mut bandlim, time_rate, 2);
            // close current frame
            let frame_sample_count = player.end_audio_frame(&mut bandlim, time_rate);
            // render BLEP frame into the sample buffer
            audio.producer.render_frame(|ref mut vec| {
                let sample_iter = bandlim.sum_iter::<T>(0)
                                .zip(bandlim.sum_iter::<T>(1)
                                    .zip(bandlim.sum_iter::<T>(2)))
                                .map(|(a,(b,c))| [a,b,c]);
                // set sample buffer size so to the size of the BLEP frame
                vec.resize(frame_sample_count * output_channels, T::center());
                // render each sample
                for (chans, samples) in vec.chunks_mut(output_channels).zip(sample_iter) {
                    // write to the wav file
                    // writer.write_sample(f32::from_sample(sample)).unwrap();
                    // convert sample type
                    // let sample = T::from_sample(samples);
                    // write sample to each channel
                    // for p in chans.iter_mut() {
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
                warn!("frame too long");
            }
        }
        println!("Played: {:?}", start_time.elapsed());
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let log_level = if cfg!(debug_assertions) {
        Level::Trace
    }
    else {
        Level::Info
    };
    simple_logger::init_with_level(log_level).map_err(|_| "simple logger initialization failed")?;
    let adf = AudioDeviceFormat::find_default()?;
    let _spec = hound::WavSpec {
        channels: 1,
        sample_rate: adf.format.sample_rate.0,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };

    // let file_name = std::env::args().nth(1).unwrap_or_else(|| "tests/DeathWish3.sna".into());
    // println!("Loading SNA: {}", file_name);
    let file_name = std::env::args().nth(1).unwrap_or_else(|| "tests/NodesOfYesod.AY".into());
    println!("Loading: {}", file_name);
    let song_index = std::env::args().nth(2).unwrap_or_else(|| "0".into());
    let song_index: u16 = song_index.parse().unwrap();

    let first_length = std::env::args().nth(3).unwrap_or_else(|| "0".into());
    let first_length = NonZeroU32::new(first_length.parse().unwrap());

    let file = std::fs::File::open(&file_name)?;
    // let wav_name = "tap_output.wav";
    // println!("Creating WAV: {:?}", wav_name);
    let writer = None; //WavWriter::create(wav_name, spec)?;

    let frame_duration = Duration::from_nanos(Ay128kPlayer::FRAME_TIME_NANOS.into());
    debug!("frame duration: {:?} rate: {}", frame_duration, 1.0 / frame_duration.as_secs_f64());
    let latency = 5;
    let audio = adf.play(latency, frame_duration.as_secs_f64())?;

    match audio {
        AudioUnknownDataType::I16(audio) => produce::<i16,_>(audio, file, song_index, first_length, writer),
        AudioUnknownDataType::U16(audio) => produce::<u16,_>(audio, file, song_index, first_length, writer),
        AudioUnknownDataType::F32(audio) => produce::<f32,_>(audio, file, song_index, first_length, writer),
    }

    Ok(())
}
