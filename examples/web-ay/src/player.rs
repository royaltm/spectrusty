use core::any::Any;
use core::num::NonZeroU32;
use core::time::Duration;
use std::io::{Cursor, Read};

use serde_json::{self, json};

use z80emu::{Cpu, Z80NMOS};
use zxspecemu::memory::{ZxMemory, SinglePageMemory};
use zxspecemu::audio::sample::*;
use zxspecemu::audio::*;
use zxspecemu::audio::synth::*;
use zxspecemu::audio::ay::*;
use zxspecemu::formats::{
    ay::*,
    sna::*
};
use zxspecemu::chip::*;
use zxspecemu::chip::ay_player::*;
pub use zxspecemu::audio::synth::{BandLimHiFi, BandLimLowTreb, BandLimLowBass, BandLimNarrow};

pub type Ay128kPlayer = AyPlayer<Ay128kPortDecode>;

macro_rules! resource {
    ($file:expr) => {
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../resources/", $file)
    };
}

const ROM48: &[u8] = include_bytes!(resource!("48k.rom"));

pub struct AyFilePlayer<F=BandLimHiFi> {
    time_rate: f64,
    cpu: Z80NMOS,
    player: Ay128kPlayer,
    // bandlim: BlepStereo<BandLimited<f32, F>>
    bandlim: BlepAmpFilter<BlepStereo<BandLimited<f32, F>>>,
    ay_file: Option<PinAyFile>,
}

impl<F: BandLimOpt> AyFilePlayer<F> {
    pub fn new(sample_rate: u32) -> Self {
        let mut bandlim = BlepAmpFilter::new(0.777, BlepStereo::new(0.5, BandLimited::<f32,F>::new(2)));
        // let mut bandlim = BlepStereo::new(0.5, BandLimited::<f32,F>::new(2));
        let time_rate = Ay128kPlayer::ensure_audio_frame_time(&mut bandlim, sample_rate);
        let mut cpu = Z80NMOS::default();
        let mut player = Ay128kPlayer::default();
        player.reset(&mut cpu, true);
        player.reset_frames();
        AyFilePlayer {
            time_rate, cpu, player, bandlim,
            ay_file: None
        }
    }

    pub fn load_file<B: Into<Box<[u8]>>>(
                &mut self,
                data: B
            ) -> Result<serde_json::Value, String>
    {
        self.player.reset(&mut self.cpu, true);
        self.player.reset_frames();
        self.ay_file = None;
        let ay_file = match parse_ay(data) {
            Ok(af) => af,
            Err(ay_err) if ay_err.data.len() == SNA_LENGTH => {
                self.player.memory.rom_page_mut(0).unwrap()
                    .copy_from_slice(&ROM48);
                read_sna(Cursor::new(&ay_err.data),
                        &mut self.cpu,
                        &mut self.player.memory)
                    .map_err(|e| e.to_string())?;
                return Ok(json!({"type": ".sna", "author":"", "misc":"", "songs":[]}))
            }
            Err(err) => return Err(err.to_string())
        };
        ay_file.initialize_player(&mut self.cpu,
                                &mut self.player.memory, 0);
        let res = json!({
            "type": ".ay",
            "author": ay_file.meta.author.to_str_lossy(),
            "misc": ay_file.meta.misc.to_str_lossy(),
            "songs": ay_file.songs.iter().map(|song| {
                json!({
                    "name": song.name.to_str_lossy(),
                })
            }).collect::<Vec<_>>()
        });
        self.ay_file = Some(ay_file);
        Ok(res)
    }

    pub fn set_song(&mut self, song_index: usize) -> Option<serde_json::Value> {
        if let Some(ay_file) = self.ay_file.as_ref() {
            if let Some(song) = ay_file.songs.get(song_index) {
                ay_file.initialize_player(&mut self.cpu, &mut self.player.memory, song_index);
                return Some(json!({
                    "name": song.name.to_str_lossy(),
                    "duration": song.song_duration,
                    "fade_duration": song.fade_duration,
                }))
            }
        }
        None
    }

    pub fn run_frame(&mut self) -> usize {
        let time_rate = self.time_rate;
        self.player.execute_next_frame(&mut self.cpu);
        self.player.render_ay_audio_frame::<AyAmps<f32>>(&mut self.bandlim, time_rate, [0, 1, 2]);
        self.player.render_earmic_out_audio_frame::<EarOutAmps4<f32>>(&mut self.bandlim, time_rate, 2);
        let frame_sample_count = self.player.end_audio_frame(&mut self.bandlim, time_rate);
        frame_sample_count
    }

    pub fn render_audio_channel(&self, channel: usize, target: &mut [f32]) {
        for (sample, p) in self.bandlim.sum_iter(channel).zip(target.iter_mut()) {
            *p = sample;
        }
    }

    pub fn next_frame(&mut self) {
        self.bandlim.next_frame();
    }
}
