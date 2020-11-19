/*
    web-zxspectrum: ZX Spectrum emulator example as a Web application.
    Copyright (C) 2020  Rafal Michalski

    For the full copyright notice, see the lib.rs file.
*/
use core::ops::Range;
use std::io::{self, Write};

use spectrusty::z80emu::{Cpu, Z80NMOS, disasm};
use spectrusty::audio::{Blep, UlaAudioFrame};
use spectrusty::clock::FTs;
use spectrusty::chip::{
    UlaCommon,
    AnimationFrameSyncTimer,
    ReadEarMode,
};
use spectrusty::memory::ZxMemory;
use spectrusty::formats::scr::{LoadScr, ScreenDataProvider};
use spectrusty::peripherals::{
    ZXKeyboardMap,
    mouse::MouseButtons
};
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
    fn get_key_state(&self) -> ZXKeyboardMap;
    fn change_key_state(&mut self, key: u8, pressed: bool);
    fn send_mouse_move(&mut self, mouse_move: (i16, i16));
    fn update_mouse_button(&mut self, button: i16, pressed: bool);
    fn read_ear_mode(&self) -> ReadEarMode;
    fn set_read_ear_mode(&mut self, mode: ReadEarMode);
    fn load_scr(&mut self, scr_data: &[u8]) -> io::Result<()>;
    fn save_scr(&self, dst: &mut Vec<u8>) -> io::Result<()>;
    fn poke_memory(&mut self, address: u16, value: u8);
    fn peek_memory(&self, address: u16) -> u8;
    fn dump_memory(&self, range: Range<u16>) -> io::Result<Vec<u8>>;
    fn disassemble_memory(&self, range: Range<u16>) -> io::Result<String>;
}

impl<C: Cpu, U, B> SpectrumControl<B> for ZxSpectrum<C, U, MemTap>
    where U: UlaCommon + DeviceAccess + UlaAudioFrame<B> + ScreenDataProvider,
          B: Blep<SampleDelta=f32>,
          Self: JoystickAccess,
          Self: MouseAccess
{
    fn run_frames_accelerated(&mut self, time_sync: &mut AnimationFrameSyncTimer) -> Result<(FTs, bool)> {
        self.run_frames_accelerated(time_sync, utils::now)
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

    fn get_key_state(&self) -> ZXKeyboardMap {
        self.ula.get_key_state()
    }

    fn change_key_state(&mut self, key: u8, pressed: bool) {
        let keymap = self.ula.get_key_state().change_key_state(key, pressed);
        self.ula.set_key_state(keymap);
    }

    fn send_mouse_move(&mut self, mouse_move: (i16, i16)) {
        if let Some(mouse) = self.mouse_interface() {
            mouse.move_mouse(mouse_move.into())
        }
    }

    fn update_mouse_button(&mut self, button: i16, pressed: bool) {
        if let Some(mouse) = self.mouse_interface() {
            let button_mask = match button {
                0 => MouseButtons::LEFT,
                1 => MouseButtons::MIDDLE,
                2 => MouseButtons::RIGHT,
                _ => return
            };
            let buttons = mouse.get_buttons();
            mouse.set_buttons(if pressed {
                buttons|button_mask
            }
            else {
                buttons&!button_mask
            })
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

    fn save_scr(&self, dst: &mut Vec<u8>) -> io::Result<()> {
        self.ula.save_scr(dst)
    }

    fn poke_memory(&mut self, address: u16, value: u8) {
        self.ula.memory_mut().write(address, value)
    }

    fn peek_memory(&self, address: u16) -> u8 {
        self.ula.memory_ref().read(address)
    }

    fn dump_memory(&self, range: Range<u16>) -> io::Result<Vec<u8>> {
        let mut mem = Vec::with_capacity(range.len());
        for page in self.ula.memory_ref().iter_pages(range)? {
            mem.write_all(page)?;
        }
        Ok(mem)
    }

    fn disassemble_memory(&self, range: Range<u16>) -> io::Result<String> {
        let pc = range.start;
        let temp = self.dump_memory(range)?;
        let mut output = String::new();
        disasm::disasm_memory_write_text::<Z80NMOS, _>(pc, &temp, &mut output).unwrap();
        Ok(output)
    }
}
