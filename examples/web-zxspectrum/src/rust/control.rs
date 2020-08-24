use std::io;

use spectrusty::z80emu::Cpu;
use spectrusty::audio::{Blep, UlaAudioFrame};
use spectrusty::clock::FTs;
use spectrusty::chip::{
    UlaCommon,
    AnimationFrameSyncTimer,
    ReadEarMode,
};
use spectrusty::formats::scr::{LoadScr, ScreenDataProvider};

use spectrusty_utils::{
    keyboard::web_sys::{
        update_keymap, update_keypad_keys,
        update_joystick_from_key_event
    }
};

use spectrusty::video::pixel::{
    PixelBufA32,
    SpectrumPalRGBA32
    //, GrayscalePalRGBA32
};

use super::utils;

pub type SpectrumPal = SpectrumPalRGBA32; // GrayscalePalRGBA32;

const FIRE_KEY: &str = "ControlRight";
// const SCREEN_DATA_LEN: usize = 6912;

use zxspectrum_common::*;

pub trait SpectrumControl<B: Blep>: VideoControl +
                                    for<'a> VideoBuffer<'a, PixelBufA32<'a>, SpectrumPal>
{
    fn run_frames_accelerated(&mut self, time_sync: &mut AnimationFrameSyncTimer) -> Result<(FTs, bool)>;
    fn run_frame(&mut self) -> Result<(FTs, bool)>;
    fn render_audio(&mut self, blep: &mut B) -> usize;
    fn reset(&mut self, hard: bool);
    fn trigger_nmi(&mut self);
    fn emulator_state_ref(&self) -> &EmulatorState;
    fn emulator_state_mut(&mut self) -> &mut EmulatorState;
    fn process_keyboard_event(&mut self, key: &str, pressed: bool, shift_down: bool, ctrl_down: bool, num_lock: bool);
    fn read_ear_mode(&self) -> ReadEarMode;
    fn set_read_ear_mode(&mut self, mode: ReadEarMode);
    fn load_scr(&mut self, scr_data: &[u8]) -> io::Result<()>;
}

impl<C: Cpu, U, B> SpectrumControl<B> for ZxSpectrum<C, U, MemTap>
    where U: UlaCommon + DeviceAccess + UlaAudioFrame<B> + ScreenDataProvider,
          B: Blep<SampleDelta=f32>,
          Self: JoystickAccess
{
    fn run_frames_accelerated(&mut self, time_sync: &mut AnimationFrameSyncTimer) -> Result<(FTs, bool)> {
        self.run_frames_accelerated(time_sync, || utils::now())
    }

    fn run_frame(&mut self) -> Result<(FTs, bool)> {
        self.run_frame()
    }

    fn render_audio(&mut self, blep: &mut B) -> usize {
        self.render_audio(blep)
    }

    fn reset(&mut self, hard: bool) {
        self.reset(hard)
    }

    fn trigger_nmi(&mut self) {
        self.trigger_nmi()
    }

    fn emulator_state_ref(&self) -> &EmulatorState {
        &self.state
    }

    fn emulator_state_mut(&mut self) -> &mut EmulatorState {
        &mut self.state
    }

    fn process_keyboard_event(&mut self, key: &str, pressed: bool, shift_down: bool, ctrl_down: bool, num_lock: bool) {
        if !update_joystick_from_key_event(key, pressed, FIRE_KEY,
                                            || self.joystick_interface()) {
            self.update_keyboard(|keymap|
                update_keymap(keymap, key, pressed, shift_down, ctrl_down)
            );
            self.update_keypad128_keys(|padmap|
                update_keypad_keys(padmap, key, pressed, num_lock)
            );
        }
    }

    fn read_ear_mode(&self) -> ReadEarMode {
        self.ula.read_ear_mode()
    }

    fn set_read_ear_mode(&mut self, mode: ReadEarMode) {
        self.ula.set_read_ear_mode(mode)
    }

    fn load_scr(&mut self, scr_data: &[u8]) -> io::Result<()> {
        self.ula.load_scr(io::Cursor::new(scr_data))
    }
}
