mod utils;
mod control;
mod audio;
mod serde;

use core::convert::TryInto;
use core::str::FromStr;
use std::io::{Cursor, Write, Seek, SeekFrom};

use wasm_bindgen::{Clamped, prelude::*};
use js_sys::{Array, Promise, Uint8Array};

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
}

#[wasm_bindgen]
extern "C" {
    fn alert(s: &str);
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    fn log_u32(a: u32);
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    fn log_f32(a: f32);
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    fn log_f64(a: f64);
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
    pixel_data: Vec<u8>
}

#[wasm_bindgen]
impl ZxSpectrumEmu {
    #[wasm_bindgen(constructor)]
    pub fn new(audio_buffer_duration: f32, model: &str) -> Result<ZxSpectrumEmu> {
        let mut bandlim = create_blep();
        let audio_stream = AudioStream::new(audio_buffer_duration)?;
        let model_request = ModelRequest::from_str(model)?;
        let model = ZxSpectrumModel::new(model_request);
        model.ensure_audio_frame_time(&mut bandlim, audio_stream.sample_rate());
        let animation_sync = AnimationFrameSyncTimer::new(utils::now(), model.effective_frame_duration_nanos());
        Ok(ZxSpectrumEmu {
            audio_stream, model, animation_sync, bandlim, pixel_data: Vec::new()
        })
    }
    /// Returns the required target canvas dimensions.
    #[wasm_bindgen(getter = canvasSize)]
    pub fn canvas_size(&self) -> Box<[u32]> {
        let (w, h) = self.spectrum_control_ref().target_size_pixels();
        Box::new([w, h])
    }
    /// Returns `true` if emulator's state changes (tape stopped or turbo started/ended).
    /// Returns `false` if emulator's state didn't change but at least one frame was run at this iteration.
    /// Returns `undefined` if no frame was run at this iteration.
    ///
    /// `time` should be provided from `requestAnimationFrame` callback or `performance.now()`.
    #[wasm_bindgen(js_name = runFramesWithAudio)]
    pub fn run_frames_with_audio(&mut self, time: f64) -> Result<Option<bool>> {
        let mut state_changed = false;
        let model = spectrum_control_from_model_mut(&mut self.model);
        if model.emulator_state_ref().turbo {
            state_changed = model.run_frames_accelerated(&mut self.animation_sync)
                                 .js_err()?.1;
        }
        else {
            let num_frames = match self.animation_sync.num_frames_to_synchronize(time) {
                Ok(num) => num,
                Err(time) => {
                    crate::log_f64(time);
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
    /// Returns an ImageData with pixels rendered from the last frame's data.
    ///
    /// # NOTE
    /// The returned image's `data` property points to wasm memory. In order to use the image in an
    /// asynchronous context you need to copy its data first.
    #[wasm_bindgen(js_name = renderVideo)]
    pub fn render_video(&mut self) -> Result<ImageData> {
        let model = spectrum_control_from_model_mut(&mut self.model);
        let pixel_data = &mut self.pixel_data;
        let (width, height) = model.render_video_frame(pixel_data);
        ImageData::new_with_u8_clamped_array_and_sh(Clamped(pixel_data), width, height)
    }

    #[wasm_bindgen(js_name = updateStateFromKeyEvent)]
    pub fn update_state_from_key_event(&mut self, event: &KeyboardEvent, pressed: bool) {
        event.prevent_default();
        let shift_down = event.shift_key();
        let ctrl_down = event.ctrl_key();
        let num_lock = event.get_modifier_state("NumLock");
        let key = event.code();
        self.spectrum_control_mut().
            process_keyboard_event(&key, pressed, shift_down, ctrl_down, num_lock);
    }

    #[wasm_bindgen(js_name = selectModel)]
    pub fn select_model(&mut self, model: &str) -> Result<()> {
        let model_request = ModelRequest::from_str(model)?;
        self.model.change_model(model_request);
        self.update_on_frame_duration_changed();
        Ok(())
    }

    #[wasm_bindgen(getter)]
    pub fn model(&mut self) -> String {
        ModelRequest::from(&self.model).to_string()
    }

    #[wasm_bindgen]
    pub fn reset(&mut self, hard: bool) {
        self.spectrum_control_mut().reset(hard)
    }

    #[wasm_bindgen(js_name = powerCycle)]
    pub fn power_cycle(&mut self) -> Result<()> {
        let mut model = ZxSpectrumModel::new((&self.model).into());
        core::mem::swap(&mut self.model, &mut model);
        recreate_model_dynamic_devices(&model, &mut self.model)?;
        let (_, state) = model.into_cpu_and_state();
        self.model.set_emulator_state(state);
        Ok(())
    }

    #[wasm_bindgen(js_name = triggerNmi)]
    pub fn trigger_nmi(&mut self) {
        self.spectrum_control_mut().trigger_nmi()
    }

    #[wasm_bindgen(js_name = selectBorderSize)]
    pub fn select_border_size(&mut self, border_size: &str) -> Result<()> {
        self.model.emulator_state_mut().border_size = BorderSize::from_str(border_size)
                                                            .js_err()?;
        Ok(())
    }

    #[wasm_bindgen(getter = borderSize)]
    pub fn border_size(&self) -> String {
        self.model.emulator_state_ref().border_size.to_string()
    }

    #[wasm_bindgen(setter)]
    pub fn set_interlace(&mut self, value: u8) -> Result<()> {
        self.model.emulator_state_mut().interlace = value.try_into().js_err()?;
        Ok(())
    }

    #[wasm_bindgen(getter)]
    pub fn interlace(&self) -> u8 {
        self.model.emulator_state_ref().interlace.into()
    }
    /// Sets the CPU rate factor, between 0.2 and 5.0.
    #[wasm_bindgen(js_name = setCpuRateFactor)]
    pub fn set_cpu_rate_factor(&mut self, rate: f32) {
        let rate = rate.max(0.2).min(5.0);
        self.model.emulator_state_mut().clock_rate_factor = rate;
        self.update_on_frame_duration_changed();
    }

    #[wasm_bindgen(getter = cpuRateFactor)]
    pub fn cpu_rate_factor(&self) -> f32 {
        self.model.emulator_state_ref().clock_rate_factor
    }

    #[wasm_bindgen(getter)]
    pub fn turbo(&mut self) -> bool {
        self.model.emulator_state_ref().turbo
    }

    #[wasm_bindgen(setter)]
    pub fn set_turbo(&mut self, is_turbo: bool) {
        self.model.emulator_state_mut().turbo = is_turbo;
    }
    /// Sets the gain for this oscillator, between 0 and 100.
    #[wasm_bindgen(getter)]
    pub fn gain(&self) -> u32 {
        (self.audio_stream.gain() * 100.0) as u32
    }
    /// Sets the gain for this oscillator, between 0 and 100.
    #[wasm_bindgen(setter)]
    pub fn set_gain(&self, gain: u32) {
        self.audio_stream.set_gain(gain as f32 / 100.0);
    }

    #[wasm_bindgen(getter = audibleTape)]
    pub fn audible_tape(&self) -> bool {
        self.model.emulator_state_ref().audible_tape
    }

    #[wasm_bindgen(setter = audibleTape)]
    pub fn set_audible_tape(&mut self, is_audible: bool) {
        self.model.emulator_state_mut().audible_tape = is_audible;
    }

    #[wasm_bindgen(getter = fastTape)]
    pub fn fast_tape(&self) -> bool {
        self.model.emulator_state_ref().flash_tape
    }

    #[wasm_bindgen(setter = fastTape)]
    pub fn set_fast_tape(&mut self, is_fast: bool) {
        self.model.emulator_state_mut().flash_tape = is_fast;
    }

    #[wasm_bindgen(js_name = pauseAudio)]
    pub fn pause_audio(&self) -> Result<Promise> {
        self.audio_stream.pause()
    }

    #[wasm_bindgen(js_name = resumeAudio)]
    pub fn resume_audio(&self) -> Result<Promise> {
        self.audio_stream.resume()
    }

    #[wasm_bindgen(js_name = showScr)]
    pub fn show_scr(&mut self, scr_data: Vec<u8>) -> Result<()> {
        self.spectrum_control_mut().load_scr(scr_data.as_slice()).js_err()
    }

    #[wasm_bindgen(js_name = loadSna)]
    pub fn load_sna(&mut self, sna_data: Vec<u8>) -> Result<()> {
        load_sna(Cursor::new(sna_data), self).js_err()?;
        self.update_on_frame_duration_changed();
        // self.spectrum_control_mut().load_sna(sna_data.as_slice()).js_err()?;
        // self.model.lock_48k_mode();
        Ok(())
    }

    #[wasm_bindgen(js_name = loadZ80)]
    pub fn load_z80(&mut self, z80_data: Vec<u8>) -> Result<()> {
        load_z80(&z80_data[..], self).js_err()?;
        self.update_on_frame_duration_changed();
        Ok(())
    }

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

    #[wasm_bindgen(js_name = ejectTape)]
    pub fn eject_tape(&mut self) {
        self.model.emulator_state_mut().tape.eject();
    }

    #[wasm_bindgen(js_name = appendTape)]
    pub fn append_tape(&mut self, tape_data: Vec<u8>) -> Result<Array> {
        let tape = &mut self.model.emulator_state_mut().tape;
        let mut old_pos = 0;
        let file = tape.eject()
                       .map(|tap| tap.try_into_file()
                           .and_then(|mut crs| {
                                old_pos = crs.seek(SeekFrom::End(0))?;
                                crs.write(&tape_data)?;
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
    /// Returns content of the inserted TAPE.
    #[wasm_bindgen(js_name = tapeData)]
    pub fn tape_data(&mut self) -> Option<Uint8Array> {
        self.model.emulator_state_mut().tape.try_reader_mut().unwrap()
        .map(|reader| {
            Uint8Array::from(&**reader.get_ref().get_ref().get_ref())
        })
    }
    /// Returns a **SNA** file data on success.
    #[wasm_bindgen(js_name = saveSNA)]
    pub fn save_sna(&mut self) -> Result<Vec<u8>> {
        self.model.ensure_cpu_is_safe_for_snapshot();
        let mut buf = Vec::new();
        let result = save_sna(self, &mut buf).js_err()?;
        report_result(result);
        Ok(buf)
    }
    /// Returns a **Z80** file data on success.
    #[wasm_bindgen(js_name = saveZ80)]
    pub fn save_z80(&mut self, ver: u32) -> Result<Vec<u8>> {
        self.model.ensure_cpu_is_safe_for_snapshot();
        let mut buf = Vec::new();
        let result = match ver {
            1 => save_z80v1(self, &mut buf).js_err()?,
            2 => save_z80v2(self, &mut buf).js_err()?,
            3 => save_z80v3(self, &mut buf).js_err()?,
            _ => Err("Z80 version should be: 1,2 or 3")?
        };
        report_result(result);
        Ok(buf)
    }
    /// Serializes ZX Spectrum to JSON.
    #[wasm_bindgen(js_name = toJSON)]
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(&self.model).js_err()
    }
    /// Deserializes ZX Spectrum from JSON.
    #[wasm_bindgen(js_name = parseJSON)]
    pub fn parse_json(&mut self, json: &str) -> Result<()> {
        self.model = serde_json::from_str(json).js_err()?;
        self.model.rebuild_device_index();
        self.update_on_frame_duration_changed();
        Ok(())
    }
    /// Returns [chunk_index, bytes_left]
    #[wasm_bindgen(js_name = tapeProgress)]
    pub fn tape_progress(&self) -> Box<[i32]> {
        if let Some(reader) = self.model.emulator_state_ref().tape.reader_ref() {
            let chunk_index = reader.chunk_no() as i32 - 1;
            let chunk_limit = reader.chunk_limit() as i32 + 1;
            return Box::new([chunk_index, chunk_limit])
        }
        Box::new([-1, 0])
    }

    #[wasm_bindgen(js_name = selectTapeChunk)]
    pub fn select_tape_chunk(&mut self, chunk_index: u32) -> Result<()> {
        self.model.emulator_state_mut().tape.rewind_nth_chunk(chunk_index + 1)
            .js_err()?;
        Ok(())
    }

    #[wasm_bindgen(js_name = tapeStatus)]
    pub fn tape_status(&mut self) -> u32 {
        match self.model.emulator_state_ref().tape.tap_state() {
            TapState::Idle => 0,
            TapState::Playing => 1,
            TapState::Recording => 2,
        }
    }

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

    #[wasm_bindgen(getter = ayAmps)]
    pub fn ay_amps(&self) -> String {
        self.model.emulator_state_ref().ay_amps.to_string()
    }

    #[wasm_bindgen(setter = ayAmps)]
    pub fn set_ay_amps(&mut self, amps: &str) -> Result<()> {
        self.model.emulator_state_mut().ay_amps = amps.parse()?;
        Ok(())
    }

    #[wasm_bindgen(getter = ayChannels)]
    pub fn ay_channels(&self) -> String {
        self.model.emulator_state_ref().ay_channels.to_string()
    }

    #[wasm_bindgen(setter = ayChannels)]
    pub fn set_ay_channels(&mut self, channels: &str) -> Result<()> {
        self.model.emulator_state_mut().ay_channels = channels.parse()?;
        Ok(())
    }

    #[wasm_bindgen(js_name = selectJoystick)]
    pub fn select_joystick(&mut self, joy: usize) {
        self.model.select_joystick(joy);
    }

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

    #[wasm_bindgen(js_name = attachDevice)]
    pub fn attach_device(&mut self, device_name: &str) -> Result<bool> {
        device_name.parse().map(|dt: DeviceType|
            dt.attach_device_to_model(&mut self.model)
        ).js_err()
    }

    #[wasm_bindgen(js_name = detachDevice)]
    pub fn detach_device(&mut self, device_name: &str) -> Result<()> {
        device_name.parse().map(|dt: DeviceType|
            dt.detach_device_from_model(&mut self.model)
        ).js_err()
    }

    #[wasm_bindgen(js_name = hasDevice)]
    pub fn has_device(&self, device_name: &str) -> Result<bool> {
        device_name.parse().map(|dt: DeviceType|
            dt.has_device_in_model(&self.model)
        ).js_err()
    }

    #[wasm_bindgen(setter = keyboardIssue)]
    pub fn set_keyboard_issue(&mut self, mode: &str) -> Result<()> {
        let mode = ReadEarMode::from_str(mode).js_err()?;
        self.spectrum_control_mut().set_read_ear_mode(mode);
        Ok(())
    }

    #[wasm_bindgen(getter = keyboardIssue)]
    pub fn keyboard_issue(&mut self) -> String {
        self.spectrum_control_mut().read_ear_mode().to_string()
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
