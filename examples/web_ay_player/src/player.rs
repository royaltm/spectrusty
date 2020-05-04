use std::io::Cursor;

use serde::{Serialize, Deserialize};
use serde_json::{self, json};

use spectrusty::z80emu::Z80NMOS;
use spectrusty::memory::ZxMemory;
use spectrusty::audio::{*, synth::{BandLimited, BandLimOpt}};
use spectrusty::peripherals::ay::{Ay128kPortDecode, audio::AyAudioFrame};
use spectrusty::formats::{
    ay::*,
    sna::*
};
use spectrusty::chip::{ControlUnit, ay_player::AyPlayer};
pub use spectrusty::audio::synth::{BandLimWide, BandLimLowTreb, BandLimLowBass, BandLimNarrow};
pub use spectrusty::audio::AmpLevels;
pub use spectrusty::peripherals::ay::audio::{AyAmps, AyFuseAmps};
pub use spectrusty::chip::{HostConfig, ZxSpectrumPALConfig, ZxSpectrum128Config};

pub type Ay128kPlayer = AyPlayer<Ay128kPortDecode>;

macro_rules! resource {
    ($file:expr) => {
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../resources/", $file)
    };
}

const ROM48: &[u8] = include_bytes!(resource!("48.rom"));

#[derive(Clone, Copy, Serialize, Deserialize, Debug)]
pub enum Clocking {
    ZXSpectrum128k,
    ZXSpectrum48k,
}

#[derive(Clone, Copy, Serialize, Deserialize, Debug)]
pub enum AyAmpSelect {
    Spec,
    Fuse,
}

#[derive(Clone, Copy, Serialize, Deserialize, Debug)]
pub enum AyChannelsMode {
    ABC,
    ACB,
    BAC,
    BCA,
    CAB,
    CBA,
    Mono
}

impl Default for AyChannelsMode {
    fn default() -> Self {
        AyChannelsMode::ACB
    }
}

impl AyChannelsMode {
    fn is_mono(self) -> bool {
        if let AyChannelsMode::Mono = self {
            true
        }
        else {
            false
        }
    }
}

impl From<AyChannelsMode> for [usize;3] {
    fn from(channels: AyChannelsMode) -> Self {
        use AyChannelsMode::*;
        match channels {
                  // A, B, C -> 0: left, 1: right, 2: center
            ABC  => [0, 2, 1],
            ACB  => [0, 1, 2],
            BAC  => [2, 0, 1],
            BCA  => [1, 0, 2],
            CAB  => [2, 1, 0],
            CBA  => [1, 2, 0],
            Mono => [2, 2, 2]
        }
    }
}

/// A whole "circuit board" bundled together.
pub struct AyFilePlayer<F=BandLimWide> {
    cpu: Z80NMOS,
    player: Ay128kPlayer,
    bandlim: BlepAmpFilter<BlepStereo<BandLimited<f32, F>>>,
    sample_rate: u32,
    ay_file: Option<PinAyFile>,
    channels: [usize; 3]
}

const MONO_AMP_FILTER: f32 = 2.0/3.0;
const STEREO_AMP_FILTER: f32 = 0.777;

impl<F: BandLimOpt> AyFilePlayer<F> {
    pub fn new(sample_rate: u32) -> Self {
        let mut bandlim = BlepAmpFilter::new(STEREO_AMP_FILTER,
                                BlepStereo::new(0.5,
                                    BandLimited::<f32,F>::new(2)
                        ));
        let mut cpu = Z80NMOS::default();
        let mut player = Ay128kPlayer::default();
        player.ensure_audio_frame_time(&mut bandlim, sample_rate);
        let channels = AyChannelsMode::default().into();
        player.reset(&mut cpu, true);
        player.reset_frames();
        AyFilePlayer {
            cpu, player, bandlim, channels, sample_rate,
            ay_file: None
        }
    }
    // sets cpu clocking
    pub fn set_clocking(&mut self, clocking: Clocking) {
        match clocking {
            Clocking::ZXSpectrum48k => self.player.set_host_config::<ZxSpectrumPALConfig>(),
            Clocking::ZXSpectrum128k => self.player.set_host_config::<ZxSpectrum128Config>(),
        }
        self.player.ensure_audio_frame_time(&mut self.bandlim, self.sample_rate);
    }
    // tries the .AY parser and if it fails and the size is right reads as .SNA
    pub fn load_file<B: Into<Box<[u8]>>>(
                &mut self,
                data: B
            ) -> Result<serde_json::Value, String>
    {
        self.player.reset(&mut self.cpu, true);
        self.player.reset_frames();
        self.bandlim.reset();
        self.ay_file = None;
        let ay_file = match parse_ay(data) {
            Ok(af) => af,
            Err(ay_err) if ay_err.data.len() == SNA_LENGTH => {
                self.player.memory.rom_mut()
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
    /// Returns a serde json object with information about the selected song.
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
    /// Sets channel mode and adjusts amplifier filter.
    pub fn set_channels_mode(&mut self, config: AyChannelsMode) {
        if config.is_mono() {
            self.bandlim.filter = MONO_AMP_FILTER;
        }
        else {
            self.bandlim.filter = STEREO_AMP_FILTER;
        }
        self.channels = config.into();
    }
    /// Runs a single frame, returns a number of samples to be rendered
    pub fn run_frame<V: AmpLevels<f32>>(&mut self) -> usize {
        self.player.execute_next_frame(&mut self.cpu);
        self.player.render_ay_audio_frame::<V>(&mut self.bandlim, self.channels);
        self.player.render_earmic_out_audio_frame::<EarOutAmps4<f32>>(&mut self.bandlim, 2);
        let frame_sample_count = self.player.end_audio_frame(&mut self.bandlim);
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
