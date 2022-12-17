/*
    web-zxspectrum: ZX Spectrum emulator example as a Web application.
    Copyright (C) 2020-2022  Rafal Michalski

    web-zxspectrum is free software: you can redistribute it and/or modify
    it under the terms of the GNU General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    web-zxspectrum is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU General Public License for more details.

    You should have received a copy of the GNU General Public License
    along with this program.  If not, see <https://www.gnu.org/licenses/>.

    Author contact information: see Cargo.toml file, section [package.authors].
*/
mod utils;
mod control;
mod audio;
mod serde;
mod snapshot;

use core::convert::TryInto;
use core::str::FromStr;
use std::io::{Cursor, Write, Seek, SeekFrom};

use wasm_bindgen::{Clamped, prelude::*};
use js_sys::{Array, Promise, Uint8Array, Int32Array};

use web_sys::{
    KeyboardEvent,
    ImageData,
};
use spectrusty::z80emu::Z80NMOS;

use spectrusty::chip::{AnimationFrameSyncTimer, ReadEarMode};
use spectrusty::memory::NoMemoryExtension;
use spectrusty::formats::{
    snapshot::SnapshotResult,
    z80::{load_z80, save_z80v1, save_z80v2, save_z80v3},
    sna::{load_sna, save_sna},
    tap::{TapChunkRead, TapReadInfoIter}
};
use spectrusty::video::BorderSize;
use zxspectrum_common::{
    JoystickAccess,
    ZxSpectrumModel, ModelRequest,
    MemTap, TapState,
    spectrum_model_dispatch
};

use audio::{BandLim, AudioStream, create_blep};
use control::{SpectrumControl};
use utils::{Result, JsErr};
use self::serde::{SerdeDynDevice, DeviceType, recreate_model_dynamic_devices};

#[wasm_bindgen]
extern "C" {
    type Object;
    #[wasm_bindgen(constructor)]
    fn new() -> Object;
    #[wasm_bindgen(method, indexing_setter)]
    fn set(this: &Object, key: &JsValue, value: &JsValue);

    fn alert(s: &str);
    // #[wasm_bindgen(js_namespace = console)]
    // fn log(s: &str);
    // #[wasm_bindgen(js_namespace = console, js_name = log)]
    // fn log_u32(a: u32);
    // #[wasm_bindgen(js_namespace = console, js_name = log)]
    // fn log_f32(a: f32);
    // #[wasm_bindgen(js_namespace = console, js_name = log)]
    // fn log_f64(a: f64);
}

type ZxSpectrumEmuModel = ZxSpectrumModel<Z80NMOS,
                                          SerdeDynDevice,
                                          NoMemoryExtension>;

/// This is the main class being instantiated in javascript.
#[wasm_bindgen]
pub struct ZxSpectrumEmu {
    model: ZxSpectrumEmuModel,
    audio_stream: AudioStream,
    animation_sync: AnimationFrameSyncTimer,
    bandlim: BandLim,
    pixel_data: Vec<u8>,
    mouse_move: (i16, i16)
}

#[wasm_bindgen]
impl ZxSpectrumEmu {
    /// Creates an instance of `ZxSpectrumEmu`.
    ///
    /// Provide the maximum duration (in seconds) of a single audio frame and the initial `model` name.
    ///
    /// # Errors
    /// Any duration values between `0.02` and `1.0` are being accepted, otherwise returns an error.
    /// Another error is returned if the provided `model` string is not recognized.
    #[wasm_bindgen(constructor)]
    pub fn new(audio_buffer_max_duration: f32, model: &str) -> Result<ZxSpectrumEmu> {
        let mut bandlim = create_blep();
        let audio_stream = AudioStream::new(audio_buffer_max_duration)?;
        let model_request = ModelRequest::from_str(model).unwrap_or(ModelRequest::SpectrumPlus2B);
        let model = ZxSpectrumModel::new(model_request);
        model.ensure_audio_frame_time(&mut bandlim, audio_stream.sample_rate());
        let animation_sync = AnimationFrameSyncTimer::new(utils::now(), model.effective_frame_duration_nanos());
        Ok(ZxSpectrumEmu {
            audio_stream,
            model,
            animation_sync,
            bandlim,
            pixel_data: Vec::new(),
            mouse_move: (0, 0)
        })
    }
    /// Returns the required target canvas dimensions.
    #[wasm_bindgen(getter = canvasSize)]
    pub fn canvas_size(&self) -> Box<[u32]> {
        let (w, h) = self.spectrum_control_ref().target_size_pixels();
        Box::new([w, h])
    }
    /// Runs emulator frames. Renders and plays audio frames if applicable.
    ///
    /// Returns:
    /// * `true` if emulator's state has changed (the tape has stopped or turbo state has changed).
    /// * `false` if emulator's state hasn't change but at least one frame was run at this iteration.
    /// * `undefined` if no frame was run at this iteration.
    ///
    /// The `time` argument should be provided from callback argument of `window.requestAnimationFrame` or 
    /// from `performance.now()`.
    #[wasm_bindgen(js_name = runFramesWithAudio)]
    pub fn run_frames_with_audio(&mut self, time: f64) -> Result<Option<bool>> {
        let mut state_changed = false;
        let state = self.model.emulator_state_ref();
        if state.turbo {
            let clock_rate_factor = state.clock_rate_factor;
            if clock_rate_factor != 1.0 {
                self.animation_sync.set_frame_duration(self.model.frame_duration_nanos());
            }
            let model = spectrum_control_from_model_mut(&mut self.model);
            state_changed = model.run_frames_accelerated(&mut self.animation_sync)
                                 .js_err()?.1;
            if clock_rate_factor != 1.0 {
                self.animation_sync.set_frame_duration(self.model.effective_frame_duration_nanos());
            }
        }
        else {
            let model = spectrum_control_from_model_mut(&mut self.model);
            if self.mouse_move != (0, 0) {
                model.send_mouse_move(self.mouse_move);
                self.mouse_move = (0, 0);
            }
            let num_frames = match self.animation_sync.num_frames_to_synchronize(time) {
                Ok(num) => num,
                Err(_time) => {
                    // crate::log_f64(time);
                    1
                }
            };
            if num_frames == 0 {
                return Ok(None)
            }
            else {
                for _ in 0..num_frames {
                    let(_, stchg) = model.run_frame().js_err()?;
                    if stchg {
                        state_changed = true;
                    }
                    model.render_audio(&mut self.bandlim);
                    self.audio_stream.play_next_audio_frame(&mut self.bandlim)?;
                }
            }
        }
        Ok(Some(state_changed))
    }
    /// Returns an `ImageData` object on success with pixels rendered from the last run frame's video data.
    ///
    /// # NOTE
    /// The returned image's `data` property points to `wasm` memory. In order to use the image in an
    /// asynchronous context you need to copy its data first.
    #[wasm_bindgen(js_name = renderVideo)]
    pub fn render_video(&mut self) -> Result<ImageData> {
        let model = spectrum_control_from_model_mut(&mut self.model);
        let pixel_data = &mut self.pixel_data;
        let (width, height) = model.render_video_frame(pixel_data);
        ImageData::new_with_u8_clamped_array_and_sh(Clamped(pixel_data), width, height)
    }
    /// Updates emulator input from `KeyboardEvent` and `pressed` boolean.
    #[wasm_bindgen(js_name = updateStateFromKeyEvent)]
    pub fn update_state_from_key_event(&mut self, event: &KeyboardEvent, pressed: bool) {
        event.prevent_default();
        let shift_down = event.shift_key();
        let ctrl_down = event.ctrl_key();
        let num_lock = event.get_modifier_state("NumLock");
        let key = event.code();
        self.spectrum_control_mut()
            .process_keyboard_event(&key, pressed, shift_down, ctrl_down, num_lock);
    }
    /// Returns the state of Spectrum keyboard as a map encoded into 40-bit bitmap.
    ///
    /// Because JavaScript only allows to perform bitwise operations on the lower 32-bits of
    /// a numeric value, to access the highest 8 bits divide the result of this function by
    /// `0x1_0000_0000` first.
    ///
    /// See [ZXKeyboardMap][spectrusty::peripherals::ZXKeyboardMap] for the mapping of specific keys.
    #[wasm_bindgen(getter)]
    pub fn keyboard(&self) -> f64 {
        self.spectrum_control_ref().get_key_state().bits() as f64
    }
    /// Alters the state of the single Spectrum keyboard key indicated as a key map index.
    ///
    /// See [ZXKeyboardMap][spectrusty::peripherals::ZXKeyboardMap] for the mapping of specific keys.
    #[wasm_bindgen(js_name = setKeyState)]
    pub fn set_key_state(&mut self, key: u8, pressed: bool) {
        self.spectrum_control_mut().change_key_state(key, pressed)
    }
    /// Update relative mouse position.
    #[wasm_bindgen(js_name = moveMouse)]
    pub fn move_mouse(&mut self, movement_x: i32, movement_y: i32) {
        fn clamped_i16(v: i32) -> i16 {
            v.try_into().unwrap_or_else(|_|
                if v < 0 {
                    i16::min_value()
                } else {
                    i16::max_value()
                }
            )
        }
        let dx = clamped_i16(movement_x);
        let dy = clamped_i16(movement_y);
        let (x, y) = self.mouse_move;
        self.mouse_move = (x.saturating_add(dx), y.saturating_add(dy));
    }
    /// Update state of mouse buttons.
    ///
    /// * `button` should be `MouseEvent.button` from `mousedown` or `mouseup` events.
    /// * `pressed` should be `true` for the `mousedown` and `false` for the `mouseup`.
    #[wasm_bindgen(js_name = updateMouseButton)]
    pub fn update_mouse_button(&mut self, button: i16, pressed: bool) {
        self.spectrum_control_mut()
            .update_mouse_button(button, pressed);
    }
    /// Hot swaps the emulated Spectrum model to the `model` name given.
    ///
    /// # Errors
    /// An error is returned if the provided `model` string is not recognized.
    #[wasm_bindgen(js_name = selectModel)]
    pub fn select_model(&mut self, model: &str) -> Result<()> {
        let model_request = ModelRequest::from_str(model)?;
        self.model.change_model(model_request);
        self.update_on_frame_duration_changed();
        Ok(())
    }
    /// Returns the currently emulated Spectrum model as a `String`.
    #[wasm_bindgen(getter)]
    pub fn model(&mut self) -> String {
        ModelRequest::from(&self.model).to_string()
    }
    /// Resets the emulated Spectrum model.
    ///
    /// The `hard` argument should be:
    /// * `false` for soft reset.
    /// * `true` for hard reset.
    #[wasm_bindgen]
    pub fn reset(&mut self, hard: bool) {
        self.spectrum_control_mut().reset(hard)
    }
    /// Emulates the power off/on cycle of the emulated Spectrum model.
    ///
    /// This method re-initializes peripheral devices state and randomizes memory content.
    #[wasm_bindgen(js_name = powerCycle)]
    pub fn power_cycle(&mut self) -> Result<()> {
        let mut model = ZxSpectrumModel::new((&self.model).into());
        core::mem::swap(&mut self.model, &mut model);
        recreate_model_dynamic_devices(&model, &mut self.model)?;
        let (_, state) = model.into_cpu_and_state();
        self.model.set_emulator_state(state);
        Ok(())
    }
    /// Initializes NMI trigger. The NMI will be triggered at the earliest possible moment
    /// during the next frame run.
    #[wasm_bindgen(js_name = triggerNmi)]
    pub fn trigger_nmi(&mut self) {
        self.spectrum_control_mut().trigger_nmi()
    }
    /// Resets the current model and initializes loading from the tape.
    #[wasm_bindgen(js_name = resetAndLoad)]
    pub fn reset_and_load(&mut self) -> Result<bool> {
        let (_, state_changed) = self.model.reset_and_load().js_err()?;
        Ok(state_changed)
    }
    /// Resets and halts the CPU.
    #[wasm_bindgen(js_name = resetAndHalt)]
    pub fn reset_and_halt(&mut self)  {
        self.spectrum_control_mut().reset_and_halt();
    }
    /// Changes next rendered frame's border size from the given border size name.
    ///
    /// # Errors
    /// An error is returned if the given border size name is not recognized.
    #[wasm_bindgen(js_name = selectBorderSize)]
    pub fn select_border_size(&mut self, border_size: &str) -> Result<()> {
        self.model.emulator_state_mut().border_size = BorderSize::from_str(border_size)
                                                            .js_err()?;
        Ok(())
    }
    /// Returns the current border size name as a `String`.
    #[wasm_bindgen(getter = borderSize)]
    pub fn border_size(&self) -> String {
        self.model.emulator_state_ref().border_size.to_string()
    }
    /// Sets the (de)interlace mode for the next rendered frame.
    ///
    /// The `value` argument should be:
    /// * `0` to disable de-interlacing.
    /// * `1` for enabling de-interlacing with the odd frame lines being rendered above even frame lines.
    /// * `2` for enabling de-interlacing with the even frame lines being rendered above odd frame lines.
    ///
    /// # Errors
    /// An error is returned if the given `value` is not one of the above.
    #[wasm_bindgen(setter)]
    pub fn set_interlace(&mut self, value: u8) -> Result<()> {
        self.model.emulator_state_mut().interlace = value.try_into().js_err()?;
        Ok(())
    }
    /// Returns the (de)interlace mode.
    #[wasm_bindgen(getter)]
    pub fn interlace(&self) -> u8 {
        self.model.emulator_state_ref().interlace.into()
    }
    /// Sets the CPU rate factor.
    ///
    /// `1.0` is the natural emulation speed.
    ///
    /// The `rate` will be capped between `0.2` and `5.0`.
    #[wasm_bindgen(js_name = setCpuRateFactor)]
    pub fn set_cpu_rate_factor(&mut self, rate: f32) {
        let rate = rate.max(0.2).min(5.0);
        self.model.emulator_state_mut().clock_rate_factor = rate;
        self.update_on_frame_duration_changed();
    }
    /// Returns the current CPU rate factor.
    #[wasm_bindgen(getter = cpuRateFactor)]
    pub fn cpu_rate_factor(&self) -> f32 {
        self.model.emulator_state_ref().clock_rate_factor
    }
    /// Sets turbo mode.
    ///
    /// When turbo mode is enabled frames are run as fast as possible and audio is not being played.
    #[wasm_bindgen(setter)]
    pub fn set_turbo(&mut self, is_turbo: bool) {
        self.model.emulator_state_mut().turbo = is_turbo;
    }
    /// Returns the state of the turbo mode.
    #[wasm_bindgen(getter)]
    pub fn turbo(&mut self) -> bool {
        self.model.emulator_state_ref().turbo
    }
    /// Sets the volume percent gain for audio playback.
    ///
    /// Provided values above 100 are being capped.
    #[wasm_bindgen(setter)]
    pub fn set_gain(&self, gain: u32) {
        self.audio_stream.set_gain(gain as f32 / 100.0);
    }
    /// Returns the volume percent gain for audio playback.
    #[wasm_bindgen(getter)]
    pub fn gain(&self) -> u32 {
        (self.audio_stream.gain() * 100.0) as u32
    }
    /// Sets the state of the audible tape flag.
    ///
    /// If audible tape flag is enabled, the tape playback and recording sound will be played.
    #[wasm_bindgen(setter = audibleTape)]
    pub fn set_audible_tape(&mut self, is_audible: bool) {
        self.model.emulator_state_mut().audible_tape = is_audible;
    }
    /// Returns the state of the audible tape flag.
    #[wasm_bindgen(getter = audibleTape)]
    pub fn audible_tape(&self) -> bool {
        self.model.emulator_state_ref().audible_tape
    }
    /// Sets the state of the auto-play and accelerate tape flag.
    #[wasm_bindgen(setter = fastTape)]
    pub fn set_fast_tape(&mut self, is_fast: bool) {
        self.model.emulator_state_mut().flash_tape = is_fast;
    }
    /// Returns the state of the auto-play and accelerate tape flag.
    #[wasm_bindgen(getter = fastTape)]
    pub fn fast_tape(&self) -> bool {
        self.model.emulator_state_ref().flash_tape
    }
    /// Sets the state of the instant tape load flag.
    #[wasm_bindgen(setter = instantTape)]
    pub fn set_instant_tape(&mut self, is_instant: bool) {
        self.model.emulator_state_mut().instant_tape = is_instant;
    }
    /// Returns the state of the instant tape load flag.
    #[wasm_bindgen(getter = instantTape)]
    pub fn instant_tape(&self) -> bool {
        self.model.emulator_state_ref().instant_tape
    }
    /// Pauses audio playback by suspending audio context.
    ///
    /// Returns a [Promise] which resolves when playback has been paused or is rejected
    /// when the audio context is already closed.
    #[wasm_bindgen(js_name = pauseAudio)]
    pub fn pause_audio(&self) -> Result<Promise> {
        self.audio_stream.pause()
    }
    /// Resumes audio playback by resuming audio context.
    ///
    /// Returns a [Promise] which resolves when playback has been resumed or is rejected
    /// when the audio context is already closed.
    #[wasm_bindgen(js_name = resumeAudio)]
    pub fn resume_audio(&self) -> Result<Promise> {
        self.audio_stream.resume()
    }
    /// Attempts to load a `.SCR` file into the video memory setting the appropriate video mode on success.
    ///
    /// # Errors
    /// Returns an error if the file format was not recognized or if the current Spectrum model does not support
    /// required screen mode.
    #[wasm_bindgen(js_name = showScr)]
    pub fn show_scr(&mut self, scr_data: Vec<u8>) -> Result<()> {
        self.spectrum_control_mut().load_scr(scr_data.as_slice()).js_err()
    }
    /// Attempts to load a `.SNA` snapshot file.
    ///
    /// # Errors
    /// Returns an error if the file format is wrong or malformed.
    #[wasm_bindgen(js_name = loadSna)]
    pub fn load_sna(&mut self, sna_data: Vec<u8>) -> Result<()> {
        load_sna(Cursor::new(sna_data), self).js_err()?;
        self.update_on_frame_duration_changed();
        Ok(())
    }
    /// Attempts to load a `.Z80` snapshot file.
    ///
    /// # Errors
    /// Returns an error if the file format is wrong or malformed.
    #[wasm_bindgen(js_name = loadZ80)]
    pub fn load_z80(&mut self, z80_data: Vec<u8>) -> Result<()> {
        load_z80(&z80_data[..], self).js_err()?;
        self.update_on_frame_duration_changed();
        Ok(())
    }
    /// Returns an array with information about the content of the inserted TAPE.
    ///
    /// Each array item represents a TAPE chunk as an object with properties `info`
    /// as a string and `size` as a number of bytes in a chunk.
    #[wasm_bindgen(js_name = tapeInfo)]
    pub fn tape_info(&mut self) -> Result<Array> {
        let array = js_sys::Array::new();
        if let Some(iter) = self.model.emulator_state_mut().tape.try_reader_mut().unwrap()
        .map(|mut reader| {
            reader.rewind();
            TapReadInfoIter::from(reader)
        }) {
            let info_name = JsValue::from_str("info");
            let size_name = JsValue::from_str("size");
            for info in iter {
                let info = info.js_err()?;
                let text = info.to_string();
                let size = info.tap_chunk_size();
                let obj = Object::new();
                obj.set(&info_name, &JsValue::from_str(&text));
                obj.set(&size_name, &JsValue::from_f64(size as f64));
                array.push(&obj);
            }
        }
        Ok(array)
    }
    /// Ejects the inserted TAPE if any.
    #[wasm_bindgen(js_name = ejectTape)]
    pub fn eject_tape(&mut self) {
        self.model.emulator_state_mut().tape.eject();
    }
    /// Appends the provided TAPE data to the already inserted TAPE.
    #[wasm_bindgen(js_name = appendTape)]
    pub fn append_tape(&mut self, tape_data: Vec<u8>) -> Result<Array> {
        let tape = &mut self.model.emulator_state_mut().tape;
        let mut old_pos = 0;
        let file = tape.eject()
                       .map(|tap| tap.try_into_file()
                           .and_then(|mut crs| {
                                old_pos = crs.seek(SeekFrom::End(0))?;
                                crs.write_all(&tape_data)?;
                                Ok(crs)
                            })
                       ).transpose().js_err()?
                       .unwrap_or_else(|| Cursor::new(tape_data));
        tape.stop();
        tape.insert_as_reader(file);
        loop {
            let res = self.model.emulator_state_mut().tape
                      .rewind_nth_chunk(1)
                      .map_err(|e| JsValue::from_str(&e.to_string()))
                      .and_then(|_| self.tape_info());
            if res.is_err() {
                let tape = &mut self.model.emulator_state_mut().tape;
                if old_pos == 0 {
                    tape.eject();
                }
                else {
                    let crs = tape.reader_mut().unwrap().get_mut().get_mut().get_mut().get_mut();
                    crs.get_mut().truncate(old_pos as usize);
                    crs.set_position(0);
                    old_pos = 0;
                    continue
                }
            }
            return res;
        }
    }
    /// Inserts a new TAPE, replacing the previous TAPE if any.
    #[wasm_bindgen(js_name = insertTape)]
    pub fn insert_tape(&mut self, tape_data: Vec<u8>) -> Result<Array> {
        let new_tap = Cursor::new(tape_data);
        let tape = &mut self.model.emulator_state_mut().tape;
        tape.stop();
        let mb_old_tap = tape.insert_as_reader(new_tap);
        let res = tape.rewind_nth_chunk(1)
                      .map_err(|e| JsValue::from_str(&e.to_string()))
                      .and_then(|_| self.tape_info());
        if res.is_err() {
            self.model.emulator_state_mut().tape.tap = mb_old_tap;
        }
        res
    }
    /// Returns the content of the inserted TAPE in a `.TAP` binary format.
    ///
    /// Returns `undefined` if no TAPE was inserted.
    #[wasm_bindgen(js_name = tapeData)]
    pub fn tape_data(&mut self) -> Option<Uint8Array> {
        self.model.emulator_state_mut().tape.try_reader_mut().unwrap()
        .map(|reader| {
            Uint8Array::from(&**reader.get_ref().get_ref().get_ref())
        })
    }
    /// Returns a screen snapshot data of the emulated Spectrum in an '.SCR' format on success.
    ///
    /// # Errors
    /// Returns an error if the snapshot could not be created in this format.
    #[wasm_bindgen(js_name = snapScr)]
    pub fn snap_scr(&self) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        self.spectrum_control_ref().save_scr(&mut buf).js_err()?;
        Ok(buf)
    }
    /// Returns a snapshot data of the emulated Spectrum in a '.SNA' format on success.
    ///
    /// # Errors
    /// Returns an error if the snapshot could not be created in this format.
    #[wasm_bindgen(js_name = saveSNA)]
    pub fn save_sna(&mut self) -> Result<Vec<u8>> {
        self.model.ensure_cpu_is_safe_for_snapshot();
        let mut buf = Vec::new();
        let result = save_sna(self, &mut buf).js_err()?;
        report_result(result);
        Ok(buf)
    }
    /// Returns a snapshot data of the emulated Spectrum in a '.Z80' format on success.
    ///
    /// Provide format version as: `1`, `2` or `3`.
    ///
    /// # Errors
    /// Returns an error if the snapshot could not be created in this format.
    #[wasm_bindgen(js_name = saveZ80)]
    pub fn save_z80(&mut self, ver: u32) -> Result<Vec<u8>> {
        self.model.ensure_cpu_is_safe_for_snapshot();
        let mut buf = Vec::new();
        let result = match ver {
            1 => save_z80v1(self, &mut buf).js_err()?,
            2 => save_z80v2(self, &mut buf).js_err()?,
            3 => save_z80v3(self, &mut buf).js_err()?,
            _ => return Err("Z80 version should be: 1,2 or 3".into())
        };
        report_result(result);
        Ok(buf)
    }
    /// Serializes the current state of the emulated Spectrum model to a JSON string.
    #[wasm_bindgen(js_name = toJSON)]
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(&self.model).js_err()
    }
    /// Attempts to deserialize a Spectrum model with the serialized state from a JSON string.
    ///
    /// # Errors
    /// Returns an error if the JSON format is mangled or incomplete.
    #[wasm_bindgen(js_name = parseJSON)]
    pub fn parse_json(&mut self, json: &str) -> Result<()> {
        self.model = serde_json::from_str(json).js_err()?;
        self.model.rebuild_device_index();
        self.update_on_frame_duration_changed();
        Ok(())
    }
    /// Returns the current TAPE head position as a tuple of `[chunk_index, bytes_left]`.
    ///
    /// * `chunk_index` being an index of the TAPE chunk, starting from 0.
    /// * `bytes_left` being a number of bytes between the current head position and the chunk's end.
    ///
    /// If no TAPE has been inserted returns `[-1, 0]`.
    #[wasm_bindgen(js_name = tapeProgress)]
    pub fn tape_progress(&self) -> Int32Array {
        if let Some(reader) = self.model.emulator_state_ref().tape.reader_ref() {
            let chunk_index = reader.chunk_no() as i32 - 1;
            let chunk_limit = reader.chunk_limit() as i32 + 1;
            return Int32Array::from([chunk_index, chunk_limit].as_ref())
        }
        Int32Array::from([-1, 0].as_ref())
    }
    /// Moves the TAPE head position to the beginning of the indicated chunk.
    #[wasm_bindgen(js_name = selectTapeChunk)]
    pub fn select_tape_chunk(&mut self, chunk_index: u32) -> Result<()> {
        self.model.emulator_state_mut().tape.rewind_nth_chunk(chunk_index + 1)
            .js_err()?;
        Ok(())
    }
    /// Returns the current status of the TAPE player/recorder.
    ///
    /// * `0` - the TAPE is idle or absent.
    /// * `1` - the TAPE is being played.
    /// * `2` - the TAPE is being recorded.
    #[wasm_bindgen(js_name = tapeStatus)]
    pub fn tape_status(&mut self) -> u32 {
        match self.model.emulator_state_ref().tape.tap_state() {
            TapState::Idle => 0,
            TapState::Playing => 1,
            TapState::Recording => 2,
        }
    }
    /// Starts or stops TAPE playback depending on its previous state.
    ///
    /// Returns the TAPE status on success. See [ZxSpectrumEmu::tape_status].
    #[wasm_bindgen(js_name = togglePlayTape)]
    pub fn toggle_play_tape(&mut self) -> Result<u32> {
        let tape = &mut self.model.emulator_state_mut().tape;
        if tape.is_ejected() || tape.is_running() {
            tape.stop();
        }
        else {
            tape.play().js_err()?;
        }
        Ok(self.tape_status())
    }
    /// Starts or stops TAPE recording depending on its previous state.
    ///
    /// Returns the TAPE status on success. See [ZxSpectrumEmu::tape_status].
    #[wasm_bindgen(js_name = toggleRecordTape)]
    pub fn toggle_record_tape(&mut self) -> Result<u32> {
        let tape = &mut self.model.emulator_state_mut().tape;
        if tape.is_running() {
            tape.stop();
            tape.make_reader().and_then(|_| tape.rewind_nth_chunk(1))
                              .js_err()?;
        }
        else {
            if tape.is_ejected() {
                tape.try_insert_as_writer(MemTap::default()).js_err()?;
            }
            tape.record().js_err()?;
        }
        Ok(self.tape_status())
    }
    /// Sets the `AY-3-891x` PSG digital level to amplitude conversion scheme.
    ///
    /// # Errors
    /// `amps` can be either "Spec" or "Fuse". Otherwise an error is returned.
    #[wasm_bindgen(setter = ayAmps)]
    pub fn set_ay_amps(&mut self, amps: &str) -> Result<()> {
        self.model.emulator_state_mut().ay_amps = amps.parse()?;
        Ok(())
    }
    /// Returns the current `AY-3-891x` PSG digital level to amplitude conversion scheme.
    #[wasm_bindgen(getter = ayAmps)]
    pub fn ay_amps(&self) -> String {
        self.model.emulator_state_ref().ay_amps.to_string()
    }
    /// Sets the `AY-3-891x` PSG channel mixing scheme.
    ///
    /// # Errors
    /// `channels` can be either a permutation of "ABC" characters or "mono". Otherwise an error is returned.
    #[wasm_bindgen(setter = ayChannels)]
    pub fn set_ay_channels(&mut self, channels: &str) -> Result<()> {
        self.model.emulator_state_mut().ay_channels = channels.parse()?;
        Ok(())
    }
    /// Returns the current `AY-3-891x` PSG channel mixing scheme.
    #[wasm_bindgen(getter = ayChannels)]
    pub fn ay_channels(&self) -> String {
        self.model.emulator_state_ref().ay_channels.to_string()
    }
    /// Selects the emulated joystick.
    ///
    /// `joy` can be one of:
    /// * `-1` for no joystick.
    /// * `0` for Kempston.
    /// * `1` for Fuller.
    /// * `2` for Sinclair Right.
    /// * `3` for Sinclair Left.
    /// * `4` for Cursor.
    #[wasm_bindgen(js_name = selectJoystick)]
    pub fn select_joystick(&mut self, joy: usize) {
        self.model.select_joystick(joy);
    }
    /// Returns the selected joystick name or "None".
    #[wasm_bindgen(getter)]
    pub fn joystick(&self) -> String {
        let name = self.model.current_joystick().unwrap_or("None");
        let sub_joy = self.model.emulator_state_ref().sub_joy;
        if name == "Sinclair" {
            format!("{} {}", name, if sub_joy == 0 { "Right" } else { "Left" })
        }
        else {
            name.to_string()
        }
    }
    /// Attempts to attach a device to the dynamic bus.
    #[wasm_bindgen(js_name = attachDevice)]
    pub fn attach_device(&mut self, device_name: &str) -> Result<bool> {
        device_name.parse().map(|dt: DeviceType|
            dt.attach_device_to_model(&mut self.model)
        ).js_err()
    }
    /// Attempts to detach a device from the dynamic bus.
    #[wasm_bindgen(js_name = detachDevice)]
    pub fn detach_device(&mut self, device_name: &str) -> Result<()> {
        device_name.parse().map(|dt: DeviceType|
            dt.detach_device_from_model(&mut self.model)
        ).js_err()
    }
    /// Returns `true` if an indicated device is present in the dynamic bus. Otherwise returns `false`.
    #[wasm_bindgen(js_name = hasDevice)]
    pub fn has_device(&self, device_name: &str) -> Result<bool> {
        device_name.parse().map(|dt: DeviceType|
            dt.has_device_in_model(&self.model)
        ).js_err()
    }
    /// Sets the emulated keyboard issue.
    ///
    /// # Errors
    /// The `mode` should be "Issue 2" or "Issue 3". Otherwise an error is returned.
    #[wasm_bindgen(setter = keyboardIssue)]
    pub fn set_keyboard_issue(&mut self, mode: &str) -> Result<()> {
        let mode = ReadEarMode::from_str(mode).js_err()?;
        self.spectrum_control_mut().set_read_ear_mode(mode);
        Ok(())
    }
    /// Returns the emulated keyboard issue.
    #[wasm_bindgen(getter = keyboardIssue)]
    pub fn keyboard_issue(&mut self) -> String {
        self.spectrum_control_mut().read_ear_mode().to_string()
    }

    pub fn poke(&mut self, address: u16, value: u8) {
        self.spectrum_control_mut().poke_memory(address, value)
    }

    pub fn peek(&self, address: u16) -> u8 {
        self.spectrum_control_ref().peek_memory(address)
    }

    pub fn dump(&self, start: u16, end: u16) -> Result<Vec<u8>> {
        self.spectrum_control_ref().dump_memory(start..end).js_err()
    }

    pub fn disassemble(&self, start: u16, end: u16) -> Result<String> {
        self.spectrum_control_ref().disassemble_memory(start..end).js_err()
    }

    fn update_on_frame_duration_changed(&mut self) {
        self.model.ensure_audio_frame_time(&mut self.bandlim, self.audio_stream.sample_rate());
        self.animation_sync.set_frame_duration(self.model.effective_frame_duration_nanos());
    }

    fn spectrum_control_ref(&self) -> &dyn SpectrumControl<BandLim> {
        spectrum_control_from_model_ref(&self.model)
    }

    fn spectrum_control_mut(&mut self) -> &mut dyn SpectrumControl<BandLim> {
        spectrum_control_from_model_mut(&mut self.model)
    }
}

fn spectrum_control_from_model_mut(model: &mut ZxSpectrumEmuModel) -> &mut dyn SpectrumControl<BandLim> {
    spectrum_model_dispatch!(model(spec) => spec)
}

fn spectrum_control_from_model_ref(model: &ZxSpectrumEmuModel) -> &dyn SpectrumControl<BandLim> {
    spectrum_model_dispatch!(model(spec) => spec)
}

fn report_result(result: SnapshotResult) {
    if !result.is_empty() {
        alert!("The substantial amount of information has been lost in the selected snapshot format.\n\n {:?}",
            result);
    }
}
