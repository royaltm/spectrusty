use core::time::Duration;
use std::io::{Read, Write, Seek, Cursor};

use serde::{Serialize, Deserialize};

#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

use spectrusty::audio::{
    UlaAudioFrame,
    AudioFrame, EarMicAmps4, EarOutAmps4, EarInAmps2, AmpLevels, Blep,
};
use spectrusty::z80emu::{z80::Flavour, Z80, Cpu};
use spectrusty::clock::FTs;
use spectrusty::chip::{
    HostConfig,
    UlaCommon,
};

#[cfg(not(target_arch = "wasm32"))]
use spectrusty::chip::ThreadSyncTimer;

#[cfg(target_arch = "wasm32")]
use spectrusty::chip::AnimationFrameSyncTimer;

use spectrusty::video::{
    VideoFrame, BorderSize
};
use spectrusty::peripherals::{
    ZXKeyboardMap,
    serial::KeypadKeys,
    ay::audio::{AyFuseAmps, AyAmps}
};

use spectrusty::formats::tap::TapChunkRead;
use spectrusty_utils::{
    tap::{Tape, romload::try_instant_rom_tape_load_or_verify}
};
pub use spectrusty_utils::tap::TapState;

use super::config::*;
use super::devices::DeviceAccess;

pub type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

pub trait SpectrumUla {
    type Chipset;
}

/// # Note
///
/// After deserialization, a device index should be rebuild with [DynamicDevices::rebuild_device_index].
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ZxSpectrum<C: Cpu, U, F> {
    pub cpu: C,
    pub ula: U,
    pub nmi_request: bool,
    pub reset_request: Option<bool>,
    #[serde(bound = "")] // so we won't have F: Serialize + Deserialize<'de> requirement
    pub state: EmulatorState<F>
}

impl<C: Cpu, U: Default, F> Default for ZxSpectrum<C, U, F> {
    fn default() -> Self {
        ZxSpectrum {
            cpu: C::default(),
            ula: U::default(),
            nmi_request: false,
            reset_request: None,
            state: EmulatorState::default()
        }
    }
}

pub type MemTap = Cursor<Vec<u8>>;

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
#[serde(bound = "")] // so we won't have F: Serialize + Deserialize<'de> requirement
pub struct EmulatorState<F=MemTap> {
    /// the TAPE recorder, maybe a tape is inside?
    #[serde(skip)]
    pub tape: Tape<F>,
    /// a record of a previous frame EAR IN counter
    pub prev_ear_in_counter: u32,
    /// a counter of frames where EAR IN counter is zero
    pub ear_in_zero_counter: u32,
    /// is the emulation paused?
    pub paused: bool,
    /// do we want to run as fast as possible?
    pub turbo: bool,
    /// emulation speed: 1.0 - original rate
    pub clock_rate_factor: f32,
    /// do we want to auto accelerate and enable auto load?
    pub flash_tape: bool,
    /// do we want to hear the tape signal?
    pub audible_tape: bool,
    /// AY blep channels
    pub ay_channels: AyChannelsMode,
    /// AY AmpLevels
    pub ay_amps: AyAmpSelect,
    /// EAR/MIC blep channel
    pub earmic_channel: usize,
    /// sub joystick index of the selected joystick device
    pub sub_joy: usize,
    /// rendering area border size
    pub border_size: BorderSize,
    /// frame buffer interlace mode
    #[serde(default)]
    pub interlace: InterlaceMode,
    /// instant tape loading using ROM loading routines
    #[serde(default = "default_instant_tape")]
    pub instant_tape: bool,
    /// dynamic device indices
    #[serde(skip)]
    pub devices: DeviceIndex
}

fn default_instant_tape() -> bool { true }

impl<C: Cpu, U, F> SpectrumUla for ZxSpectrum<C, U, F> {
    type Chipset = U;
}

impl<F> Default for EmulatorState<F> {
    fn default() -> Self {
        EmulatorState {
            tape: Default::default(),
            prev_ear_in_counter: 0,
            ear_in_zero_counter: 0,
            paused: false,
            turbo: false,
            clock_rate_factor: 1.0,
            flash_tape: true,
            audible_tape: true,
            ay_channels: AyChannelsMode::default(),
            ay_amps: AyAmpSelect::default(),
            earmic_channel: 2,
            sub_joy: 0,
            border_size: BorderSize::Full,
            interlace: InterlaceMode::default(),
            instant_tape: default_instant_tape(),
            devices: DeviceIndex::default()
        }
    }
}

impl<L: Flavour, U, F> ZxSpectrum<Z80<L>, U, F>
{
    pub fn into_cpu_flavour<T: Flavour>(self) -> ZxSpectrum<Z80<T>, U, F>
        where T: From<L>
    {
        let ZxSpectrum { cpu, ula, nmi_request, reset_request, state } = self;
        ZxSpectrum {
            cpu: cpu.into_flavour(),
            ula, nmi_request, reset_request, state
        }
    }
}

impl<C: Cpu, U, F> ZxSpectrum<C, U, F>
    where U: HostConfig
{
    pub fn effective_frame_duration_nanos(&self) -> u32 {
        U::effective_frame_duration_nanos(self.state.clock_rate_factor as f64)
    }

    pub fn effective_frame_duration(&self) -> Duration {
        U::effective_frame_duration(self.state.clock_rate_factor as f64)
    }

    pub fn effective_cpu_rate(&self) -> f64 {
        U::effective_cpu_rate(self.state.clock_rate_factor as f64)
    }

    pub fn ensure_audio_frame_time<B: Blep>(&self, blep: &mut B, sample_rate: u32)
        where U: AudioFrame<B>
    {
        self.ula.ensure_audio_frame_time(blep, sample_rate, self.effective_cpu_rate())
    }
}

impl<C: Cpu, U, F> ZxSpectrum<C, U, F>
    where U: UlaCommon,
          F: Read + Write + Seek
{
    pub fn update_keyboard<FN: FnOnce(ZXKeyboardMap) -> ZXKeyboardMap>(
            &mut self,
            update_keys: FN)
        where U: DeviceAccess
    {
        let keymap = update_keys( self.ula.get_key_state() );
        self.ula.set_key_state(keymap);
    }

    pub fn update_keypad128_keys<FN: FnOnce(KeypadKeys) -> KeypadKeys>(
            &mut self,
            update_keys: FN
        )
        where U: DeviceAccess
    {
        if let Some(keypad) = self.ula.keypad128_mut() {
            let padmap = update_keys( keypad.get_key_state() );
            keypad.set_key_state(padmap);
        }
    }

    /// returns `Ok(Some(chunk_saved))` when recording or `Ok(None)` if not
    fn record_tape_from_mic_out(&mut self) -> Result<Option<bool>> {
        // get the writer if the tape is inserted and is being recorded
        if let Some(ref mut writer) = self.state.tape.recording_writer_mut() {
            // extract the MIC OUT state changes as a pulse iterator
            let pulses_iter = self.ula.mic_out_pulse_iter();
            // decode the pulses as TAPE data and write it as a TAP chunk fragment
            let chunks = writer.write_pulses_as_tap_chunks(pulses_iter)?;
            if self.state.flash_tape && !self.state.turbo || self.state.turbo {
                // is the state of the pulse decoder idle?
                self.state.turbo = !writer.get_ref().is_idle();
            }
            return Ok(Some(chunks != 0))
        }
        Ok(None)
    }

    /// Very simple heuristics for detecting if spectrum needs some TAPE data.
    /// Returns `Ok(true)` if instant load attempt was made.
    fn auto_detect_load_from_tape(&mut self) -> Result<bool> {
        const ZERO_FRAMES_THRESHOLD_TS: u32 = 3_600_000;
        let count = self.ula.read_ear_in_count();
        let state = &mut self.state;
        if count == 0 {
            if state.turbo && state.tape.is_playing() {
                state.ear_in_zero_counter += 1;
                if state.ear_in_zero_counter > ZERO_FRAMES_THRESHOLD_TS /
                                               U::VideoFrame::FRAME_TSTATES_COUNT as u32 {
                    state.tape.stop();
                    state.turbo = false;
                    state.prev_ear_in_counter = 0;
                    state.ear_in_zero_counter = 0
                }
            }
            else {
                state.ear_in_zero_counter = 0;
            }
        }
        else {
            // if turbo is on and the tape is playing
            if state.turbo && state.tape.is_playing() {
                const IDLE_THRESHOLD: u32 = 20;
                // stop the tape and slow down
                // if the EAR IN probing falls below the threshold
                if state.prev_ear_in_counter + count < IDLE_THRESHOLD {
                    state.tape.stop();
                    state.turbo = false;
                }
            }
            // if flash loading is enabled and a tape isn't running
            else if state.tape.is_inserted() && !state.tape.running {
                if state.instant_tape {
                    let mut chunk_no = 0;
                    // try to instantly load/verify if ROM loader is being used
                    if let Some(read_len) = try_instant_rom_tape_load_or_verify(
                        &mut self.cpu,
                        self.ula.memory_mut(),
                        || {
                            state.tape.make_reader()?;
                            let pulse_iter = state.tape.reader_mut().unwrap().get_mut();
                            let is_lead = pulse_iter.state().is_lead();
                            let chunk_reader = pulse_iter.get_mut();
                            chunk_no = chunk_reader.chunk_no();
                            if is_lead {
                                chunk_reader.rewind_chunk()?;
                            }
                            else {
                                chunk_reader.forward_chunk()?;
                            }
                            Ok(chunk_reader.get_mut())
                        })?
                    {
                        let pulse_iter = state.tape.reader_mut().unwrap().get_mut();
                        pulse_iter.data_from_next();
                        state.prev_ear_in_counter = 0;
                        state.ear_in_zero_counter = 0;
                        return Ok(read_len != 0 || chunk_no != pulse_iter.get_ref().chunk_no());
                    }
                }
                const PROBE_THRESHOLD: u32 = 69888/1000;
                // play the tape and speed up
                // if the EAR IN probing exceeds the threshold
                if state.flash_tape && count > U::VideoFrame::FRAME_TSTATES_COUNT as u32 / PROBE_THRESHOLD {
                    state.tape.play()?;
                    state.turbo = true;
                }
            }
            state.prev_ear_in_counter = count;
            state.ear_in_zero_counter = 0;
        }
        Ok(false)
    }

    /// Returns `Ok(end_of_tape)`
    fn feed_ear_in_or_stop_tape(&mut self) -> Result<bool> {
        // get the reader if the tape is inserted and is being played
        if let Some(ref mut feeder) = self.state.tape.playing_reader_mut() {
            // check if any pulse is still left in the feeder
            let mut feeder = feeder.peekable();
            if feeder.peek().is_some() {
                // feed EAR IN line with pulses from our pulse iterator
                // only up to the end of a single frame
                self.ula.feed_ear_in(&mut feeder, Some(1));
            }
            else {
                // end of tape
                self.state.tape.stop();
                // always end turbo mode when the tape stops
                self.state.turbo = false;
                return Ok(true)
            }
        }
        Ok(false)
    }
    /// Returns (T-states diff, state_changed)
    pub fn run_frame(&mut self) -> Result<(FTs, bool)> {
        let (turbo, running) = (self.state.turbo, self.state.tape.running);

        let chunk_saved_or_instaload = match self.record_tape_from_mic_out()? {
            Some(saved) => saved,
            None => {
                if self.state.flash_tape || self.state.instant_tape || self.state.turbo {
                    self.auto_detect_load_from_tape()?
                }
                else {
                    false
                }
            }
        };
        // clean up the internal buffers of ULA so we won't append the EAR IN data
        // to the previous frame's data
        self.ula.ensure_next_frame();
        // and we also need the timestamp of the beginning of a frame
        let fts_start = self.ula.current_tstate();

        if self.feed_ear_in_or_stop_tape()? && running {
            // only report it when the tape was running before
            info!("Auto STOP: End of TAPE");
        }

        if self.nmi_request {
            if self.ula.nmi(&mut self.cpu) {
                self.nmi_request = false;
            }
        }
        if let Some(hard) = self.reset_request.take() {
            self.ula.reset(&mut self.cpu, hard);
        }
        self.ula.execute_next_frame(&mut self.cpu);

        let fts_delta = self.ula.current_tstate() - fts_start;
        let state_changed = chunk_saved_or_instaload ||
                            running != self.state.tape.running ||
                            turbo   != self.state.turbo;
        Ok((fts_delta, state_changed))
    }
    /// Returns (T-states diff, state_changed)
    ///
    /// Runs frames as fast as possible until a single frame duration passes in real-time
    /// or if turbo state ends automatically.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn run_frames_accelerated(&mut self, time_sync: &mut ThreadSyncTimer) -> Result<(FTs, bool)> {
        let mut sum: FTs = 0;
        let mut state_changed = false;
        while time_sync.check_frame_elapsed().is_none() {
            let (cycles, schg) = self.run_frame()?;
            sum += cycles;
            if schg {
                state_changed = true;
                if !self.state.turbo {
                    break;
                }
            }
        }
        Ok((sum, state_changed))
    }
    #[cfg(target_arch = "wasm32")]
    pub fn run_frames_accelerated<FN: Fn() -> f64>(
            &mut self,
            time_sync: &mut AnimationFrameSyncTimer,
            timer: FN
        ) -> Result<(FTs, bool)>
    {
        let mut sum: FTs = 0;
        let mut state_changed = false;
        while time_sync.check_frame_elapsed(timer()).is_none() {
            let (cycles, schg) = self.run_frame()?;
            sum += cycles;
            if schg {
                state_changed = true;
                if !self.state.turbo {
                    break;
                }
            }
        }
        Ok((sum, state_changed))
    }
    /// Runs frames inserting a keymap each frame from the keymap iterator.
    pub fn run_with_auto_type<'a, I: IntoIterator<Item=&'a (ZXKeyboardMap, u32)>>(
            &mut self,
            pretype_frames: u32,
            keypresses: I
        ) -> Result<(FTs, bool)>
    {
        let mut sum: FTs = 0;
        let mut state_changed = false;
        let mut runner = |keymap| -> Result<()> {
            self.ula.set_key_state(keymap);
            let (cycles, schg) = self.run_frame()?;
            sum += cycles;
            if schg {
                state_changed = true;
            }
            Ok(())
        };
        for _ in 0..pretype_frames {
            runner(ZXKeyboardMap::empty())?;
        }
        for &(keymap, repeat) in keypresses.into_iter() {
            for _ in 0..repeat {
                runner(keymap)?;
            }
        }
        runner(ZXKeyboardMap::empty())?;
        Ok((sum, state_changed))
    }
    /// Adds pulse steps to the `blep` and returns the number of samples ready to be produced.
    pub fn render_audio<B>(&mut self, blep: &mut B) -> usize
        where U: UlaAudioFrame<B>,
              B: Blep,
              AyAmps<B::SampleDelta>: AmpLevels<B::SampleDelta>,
              AyFuseAmps<B::SampleDelta>: AmpLevels<B::SampleDelta>,
              EarMicAmps4<B::SampleDelta>: AmpLevels<B::SampleDelta>,
              EarInAmps2<B::SampleDelta>: AmpLevels<B::SampleDelta>,
              EarOutAmps4<B::SampleDelta>: AmpLevels<B::SampleDelta>
    {
        let ay_channels = self.state.ay_channels.into();
        match self.state.ay_amps {
            AyAmpSelect::Spec => {
                self.ula.render_ay_audio_frame::<AyAmps<B::SampleDelta>>(blep, ay_channels);
            }
            AyAmpSelect::Fuse => {
                self.ula.render_ay_audio_frame::<AyFuseAmps<B::SampleDelta>>(blep, ay_channels);
            }
        }

        let channel = self.state.earmic_channel;

        if self.state.audible_tape {
            self.ula.render_earmic_out_audio_frame::<EarMicAmps4<B::SampleDelta>>(blep, channel);
            self.ula.render_ear_in_audio_frame::<EarInAmps2<B::SampleDelta>>(blep, channel);
        }
        else {
            self.ula.render_earmic_out_audio_frame::<EarOutAmps4<B::SampleDelta>>(blep, channel);
        }
        self.ula.end_audio_frame(blep)
    }

    pub fn reset(&mut self, hard: bool) {
        self.reset_request = Some(hard);
    }

    pub fn trigger_nmi(&mut self) {
        self.nmi_request = true;
    }
}
