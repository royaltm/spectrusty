/*
    ay_pleyer_cpal: Command-line ZX Spectrum AY file player demo.
    Copyright (C) 2020  Rafal Michalski

    ay_pleyer_cpal is free software: you can redistribute it and/or modify
    it under the terms of the GNU General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    ay_pleyer_cpal is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU General Public License for more details.

    You should have received a copy of the GNU General Public License
    along with this program.  If not, see <https://www.gnu.org/licenses/>.

    Author contact information: see Cargo.toml file, section [package.authors].
*/
use core::num::NonZeroU32;
use std::io::Read;

#[allow(unused_imports)]
use log::{error, warn, info, debug, trace, Level};

use spectrusty::z80emu::{self, Cpu, Z80NMOS};
// use spectrusty::cpu_debug::print_debug_memory;
use spectrusty::audio::{synth::*, host::cpal::{AudioHandle, AudioHandleAnyFormat}};
use spectrusty::audio::*;
use spectrusty::peripherals::ay::{*, audio::*};
use spectrusty::formats::{
    ay::*,
    // sna::*
};
use spectrusty::chip::*;
use spectrusty::chip::ay_player::*;
/****************************************************************************/
/*                                   MAIN                                   */
/****************************************************************************/
type Ay128kPlayer = AyPlayer<Ay128kPortDecode>;
type WavWriter = hound::WavWriter<std::io::BufWriter<std::fs::File>>;
const AUDIO_LATENCY: usize = 5;

fn produce<T, R: Read>(
        mut audio: AudioHandle<T>,
        rd: R,
        song_index: u16,
        mut first_length: Option<NonZeroU32>,
        mut writer: Option<WavWriter>
)
    where T: 'static + FromSample<f32> + AudioSample + cpal::Sample + Send,
        i16: IntoSample<T>, f32: FromSample<T>,
{
    let output_channels = audio.channels as usize;
    let frame_duration = ZxSpectrum128Config::frame_duration();
    // BandLimHiFi
    // BandLimLowTreb
    // BandLimLowBass
    // BandLimNarrow
    // let mut bandlim = BlepStereo::new(BandLimited::<f32>::new(3), 0.5);
    // let mut bandlim = BlepAmpFilter::new(0.777, BlepStereo::new(0.5,BandLimited::<f32>::new(2)));
    let mut bandlim = BlepAmpFilter::build(0.777)(BlepStereo::build(0.8)(BandLimited::<f32>::new(2)));
    // let mut bandlim = BlepAmpFilter::new(BandLimited::<f32>::new(3), 0.777);
    // BandLimited::<f32>::new(3).wrap_with(BlepStereo::build(0.5)).wrap_with(BlepAmpFilter::build(0.777));
    // BlepAmpFilter::build(0.777).wrap(BlepStereo::build(0.5)).wrap(BandLimited::<f32>::new(3));
    let mut cpu = Z80NMOS::default();
    let mut player = Ay128kPlayer::default();
    player.ensure_audio_frame_time(&mut bandlim, audio.sample_rate);
    player.reset(&mut cpu, true);
    // let rom_file = std::fs::File::open("resources/roms/48.rom").unwrap();
    // player.memory.load_into_rom_page(0, rom_file).unwrap();
    // read_sna(rd, &mut cpu, &mut player.memory).unwrap();
    let ay_file = match read_ay(rd) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("data: {:?}", e.ay_parse_ref().unwrap().data.len());
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
                let length = ay_file.songs[song_index].song_duration as u32;
                if length == 0 { DEFAULT_SONG_LENGTH } else { length }
            }
        };
        first_length = None;

        println!("^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^");
        println!("Song name[{}]: {}", song_index, ay_file.songs[song_index].name);
        println!("Song length: {} frames", ay_file.songs[song_index].song_duration);
        println!("Song fade: {} frames", ay_file.songs[song_index].fade_duration);
        println!("Playing: {:?}", song_length * frame_duration);

        player.reset(&mut cpu, true);
        player.reset_frames();
        ay_file.initialize_player(&mut cpu, &mut player.memory, song_index);
        debug!("CPU: {:?}", cpu);
        let mut tsync = ThreadSyncTimer::new(player.frame_duration_nanos());
        let start_time = tsync.time;
        // render frames
        // debug!("CURRENT FRAME: {} song_length: {}", player.current_frame(), song_length);
        while player.current_frame() <= song_length.into() {
        // loop {
            // debug!("frame_tstates: {}", player.frame_tstate());
            player.execute_next_frame(&mut cpu);
            player.render_ay_audio_frame::<AyAmps<f32>>(&mut bandlim, [0, 1, 2]);
            // player.render_ay_audio_frame::<AyFuseAmps<f32>>(&mut bandlim, [0, 1, 2]);
            player.render_earmic_out_audio_frame::<EarOutAmps4<f32>>(&mut bandlim, 2);
            // close current frame
            let frame_sample_count = player.end_audio_frame(&mut bandlim);
            // render BLEP frame into the sample buffer
            audio.producer.render_frame(|ref mut vec| {
                let sample_iter = bandlim.sum_iter::<T>(0)
                                         .zip(bandlim.sum_iter::<T>(1))
                                         .map(|(a,b)| [a,b]);
                // set sample buffer size to the size of the BLEP frame
                vec.resize(frame_sample_count * output_channels, T::silence());
                // render each sample
                for (chans, samples) in vec.chunks_mut(output_channels).zip(sample_iter) {
                    // write sample to first 2 channels
                    for (p, sample) in chans.iter_mut().zip(samples.iter()) {
                        *p = *sample;
                    }
                }
            });
            if let Some(ref mut writer) = writer {
                // write to the wav file
                for (l, r) in bandlim.sum_iter::<f32>(0)
                                     .zip(bandlim.sum_iter::<f32>(1)) {
                    writer.write_sample(l).unwrap();
                    writer.write_sample(r).unwrap();
                }
            }
            // prepare BLEP for the next frame
            bandlim.next_frame();
            // send sample buffer to the consumer
            audio.producer.send_frame().unwrap();
            trace!("{:04x} {:04x} {:04x} {:04x} {:04x} {:02x?} ",
                    cpu.get_sp(),
                    cpu.get_reg16(z80emu::StkReg16::HL),
                    cpu.get_reg16(z80emu::StkReg16::BC),
                    cpu.get_reg16(z80emu::StkReg16::DE),
                    cpu.get_reg16(z80emu::StkReg16::AF),
                    &player.ay_io.registers()[0..14]);
            if let Err(missed) = tsync.synchronize_thread_to_frame() {
                warn!("missed frames: {}", missed);
            }
        }
        println!("Played: {:?}", start_time.elapsed());
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!(r#"ay_pleyer_cpal  Copyright (C) 2020  Rafal Michalski
This program comes with ABSOLUTELY NO WARRANTY.
This is free software, and you are welcome to redistribute it under certain conditions."#);

    let log_level = if cfg!(debug_assertions) {
        log::LevelFilter::Trace
    }
    else {
        log::LevelFilter::Info
    };
    simple_logger::SimpleLogger::new().with_level(log_level).init()
                   .map_err(|_| "simple logger initialization failed")?;

    let frame_duration_nanos = ZxSpectrum128Config::frame_duration_nanos() as u32;
    let audio = AudioHandleAnyFormat::create(&cpal::default_host(),
                                             frame_duration_nanos,
                                             AUDIO_LATENCY)?;
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: audio.sample_rate(),
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };

    let file_name = std::env::args().nth(1).unwrap_or_else(|| "../../resources/nodes_of_yesod.ay".into());
    println!("Loading: {}", file_name);
    let song_index = std::env::args().nth(2).unwrap_or_else(|| "0".into());
    let song_index: u16 = song_index.parse().unwrap();

    let first_length = std::env::args().nth(3).unwrap_or_else(|| "0".into());
    let first_length = NonZeroU32::new(first_length.parse().unwrap());

    let file = std::fs::File::open(&file_name)?;
    let wav_name = "tap_output.wav";
    println!("Creating WAV: {:?}", wav_name);
    let writer = Some(WavWriter::create(wav_name, spec)?);

    let frame_duration = ZxSpectrum128Config::frame_duration();
    debug!("frame duration: {:?} rate: {}", frame_duration, 1.0 / frame_duration.as_secs_f64());
    audio.play()?;

    match audio {
        AudioHandleAnyFormat::I16(audio) => produce::<i16,_>(audio, file, song_index, first_length, writer),
        AudioHandleAnyFormat::U16(audio) => produce::<u16,_>(audio, file, song_index, first_length, writer),
        AudioHandleAnyFormat::F32(audio) => produce::<f32,_>(audio, file, song_index, first_length, writer),
    }

    Ok(())
}
