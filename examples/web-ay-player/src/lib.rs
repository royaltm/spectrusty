/*
    web-ay-player: Web ZX Spectrum AY file format audio player example.
    Copyright (C) 2020  Rafal Michalski

    web-ay-player is free software: you can redistribute it and/or modify
    it under the terms of the GNU General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    web-ay-player is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU General Public License for more details.

    You should have received a copy of the GNU General Public License
    along with this program.  If not, see <https://www.gnu.org/licenses/>.

    Author contact information: see Cargo.toml file, section [package.authors].
*/
mod player;
use core::mem;
use core::cell::RefCell;
use core::ops::Range;
use std::rc::Rc;

use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use js_sys::{self, Promise, Uint8Array, Function};
use web_sys::{
    window,
    ProgressEvent,
    Response,
    File,
    FileReader,
    AudioBuffer,
    AudioContext,
    AudioContextState,
    GainNode,
};
use wasm_bindgen_futures::{JsFuture, future_to_promise};
use serde_json::json;

use player::*;

macro_rules! console_log {
    ($($t:tt)*) => (log(&format_args!($($t)*).to_string()))
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    fn log_u32(a: u32);
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    fn log_f32(a: f32);
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    fn log_f64(a: f64);
}

/// This struct is wrapped inside the reference pointer in a AyPlayerHandle
pub struct AyWebPlayer {
    player: AyFilePlayer,
    buffer0: Vec<f32>,
    buffer1: Vec<f32>,
    cursor: Range<usize>,
    buffer_sourced: AudioBuffer,
    buffer_next: AudioBuffer,
    curr_end_closure: OnEndClosure,
    next_end_closure: OnEndClosure,
    buffer_length: u32,
    buffer_duration: f64,
    ctx: AudioContext,
    gain: GainNode,
    ay_amp_select: AyAmpSelect
}

impl Drop for AyWebPlayer {
    fn drop(&mut self) {
        log("closing");
        let _ = self.ctx.close();
    }
}

type OnEndClosure = Closure<dyn FnMut() -> Result<(), JsValue>>;

enum PlayerStatus {
    Idle,
    Playing
}

/// This is the main class being instantiated in javascript.
#[wasm_bindgen]
pub struct AyPlayerHandle {
    player: Rc<RefCell<AyWebPlayer>>,
    status: PlayerStatus
}

#[wasm_bindgen]
impl AyPlayerHandle {
    /// Creates a new instance of [AyPlayerHandle], may throw errors.
    #[wasm_bindgen(constructor)]
    pub fn new(buffer_duration: f32) -> Result<AyPlayerHandle, JsValue> {
        let player = Rc::new(RefCell::new( AyWebPlayer::new(buffer_duration)? ));
        Ok(AyPlayerHandle {
            player,
            status: PlayerStatus::Idle
        })
    }
    /// Plays the song at `song_index`. On success returns the optional JSON object
    /// with the information about the played song.
    /// The song should be loaded prior to calling `play`. It may be called only once per each instance.
    /// May throw errors.
    #[wasm_bindgen]
    pub fn play(&mut self, song_index: u32) -> Result<JsValue, JsValue> {
        if let PlayerStatus::Playing = self.status {
            return Err("Playing already.".into());
        }
        let song_info;
        let (time, duration) = {
            let mut player = self.player.borrow_mut();
            song_info = match player.player.set_song(song_index as usize) {
                Some(info) => info,
                None if song_index != 0 => return Err("No song at a given index.".into()),
                None => json!(null)
            };
            (player.ctx.current_time(), player.buffer_duration)
        };
        Self::play_next_frame(time, &self.player)?;
        Self::play_next_frame(time + duration, &self.player)
        .map(|_| {
            self.status = PlayerStatus::Playing;
            JsValue::from_serde(&song_info).unwrap()
        })
    }

    fn play_next_frame(
                time: f64,
                player_rc: &Rc<RefCell<AyWebPlayer>>
            ) -> Result<(), JsValue>
    {
        let mut player = player_rc.borrow_mut();
        let source = {
            { // swap buffers and create a new source for the next buffer
                let pl = &mut *player; // direct mutable reference for a mutable split call
                mem::swap(&mut pl.buffer_sourced, &mut pl.buffer_next);
                mem::swap(&mut pl.curr_end_closure, &mut pl.next_end_closure);
            }
            let source = player.ctx.create_buffer_source()?;
            source.set_buffer(Some(&player.buffer_next));
            source.connect_with_audio_node(&player.gain)?;
            source
        };
        source.start_with_when(time)?;
        // schedule next audio play with `buffer_next` after the end of play of `buffer_sourced`
        let time = time + 2.0 * player.buffer_duration;
        // keep only a weak reference in a closure, otherwise the drop of
        // AyPlayerHandle wouldn't drop AyWebPlayer
        let weakref = Rc::downgrade(player_rc);
        // overwrite prevous closure
        let closure = Closure::once(move || -> Result<(), JsValue> {
            if let Some(rc) = weakref.upgrade() {
                return Self::play_next_frame(time, &rc);
            }
            Ok(())
        });
        source.set_onended(Some( closure.as_ref().unchecked_ref() ));
        player.next_end_closure = closure;
        // when audio is scheduled, we have a whole duration of
        // `buffer_sourced` to render the next one
        match player.ay_amp_select {
            AyAmpSelect::Spec => player.render_frames::<AyAmps<f32>>(),
            AyAmpSelect::Fuse => player.render_frames::<AyFuseAmps<f32>>()
        }
    }
    /// Loads a song from the given `file`.
    /// `file` can be an instance of a WEBAPI `File` object or a remote url to the file as a string.
    /// On success returns a JSON object with information about the file.
    /// May throw errors.
    #[wasm_bindgen]
    pub fn load(&mut self, file: JsValue) -> Promise {
        if let PlayerStatus::Playing = self.status {
            return Promise::reject(&"Can't load, already playing.".into());
        }
        let player = Rc::clone(&self.player);
        let url = match file.dyn_into::<File>() {
            Ok(file) => return Self::load_file(player, file),
            Err(other) => other
        };
        if url.is_string() {
            if let Some(url) = url.as_string() {
                return future_to_promise(
                    Self::load_url_async(player, url)
                );
            }
        }
        Promise::reject(&"Only url string or File instances please.".into())
    }

    fn load_file(
                player: Rc<RefCell<AyWebPlayer>>,
                file: File
            ) -> Promise
    {   // Using an older event-based FileReader api.
        Promise::new(&mut move |resolve: Function, reject: Function| {
            let reject = move |err: JsValue| {
                let _ = reject.call1(&JsValue::NULL, &err);
            };
            let file_reader = match FileReader::new() {
                Ok(rd) => rd,
                Err(jserr) => return reject(jserr)
            };
            if let Err(jserr) = file_reader.read_as_array_buffer(&file) {
                return reject(jserr)
            }
            let player = Rc::clone(&player);
            let cb = Closure::once_into_js(move |event: ProgressEvent| {
                let target = event.target().unwrap();
                let file_reader: FileReader = match target.dyn_into() {
                    Ok(fr) => fr,
                    Err(_) => return reject("Not a file reader!".into())
                };
                let array_buffer = match file_reader.result() {
                    Ok(ab) => ab,
                    Err(jserr) => return reject(jserr)
                };
                let data = Uint8Array::new(&array_buffer).to_vec();
                let info = match player.borrow_mut().player.load_file(data) {
                    Ok(info) => info,
                    Err(err) => return reject(err.into())
                };
                let _ = resolve.call1(&JsValue::NULL, &JsValue::from_serde(&info).unwrap());
            });
            file_reader.set_onloadend(Some(cb.unchecked_ref()));
        })
    }

    async fn load_url_async(
                player: Rc<RefCell<AyWebPlayer>>,
                url: String
            ) -> Result<JsValue, JsValue>
    { // using fetch api
        let response = window().unwrap().fetch_with_str(&url);
        let response = JsFuture::from(response).await?.dyn_into::<Response>()?;
        if !response.ok() {
            return Err(response.into());
        }
        let array_buffer = JsFuture::from(response.array_buffer()?).await?;
        let data = Uint8Array::new(&array_buffer).to_vec();
        let info = player.borrow_mut().player.load_file(data)?;
        Ok(JsValue::from_serde(&info).unwrap())
    }
    /// Toggles paused state. Returns a `Promise` which resolves to `true` if paused
    /// or `false` if unpaused, `null` if not playing.
    #[wasm_bindgen(js_name = togglePause)]
    pub fn toggle_pause(&self) -> Promise {
        match self.status {
            PlayerStatus::Idle => Promise::resolve(&JsValue::NULL),
            PlayerStatus::Playing => {
                let player = Rc::clone(&self.player);
                future_to_promise(
                    Self::toggle_pause_async(player)
                )
            }
        }
    }

    async fn toggle_pause_async(
                player: Rc<RefCell<AyWebPlayer>>
            ) -> Result<JsValue, JsValue>
    {
        let mut resumed = false;
        let promise = {
            let ctx = &player.borrow().ctx;
            match ctx.state() {
                AudioContextState::Suspended => {
                    resumed = true;
                    ctx.resume()?
                }
                AudioContextState::Running => ctx.suspend()?,
                _ => return Err("Audio context closed".into())
            }
        };

        let _ = JsFuture::from(promise).await?;

        Ok(if resumed {
            JsValue::FALSE
        }
        else {
            JsValue::TRUE
        })
    }
    /// Sets the clocking for the player.
    #[wasm_bindgen(js_name = setClocking)]
    pub fn set_clocking(&self, cl_mode: &str) -> Result<(), JsValue> {
        let clocking = serde_json::from_str(&cl_mode)
                                    .map_err(|e| e.to_string())?;
        self.player.borrow_mut().player.set_clocking(clocking);
        Ok(())
    }
    /// Sets the channel mode for the player.
    #[wasm_bindgen(js_name = setAmps)]
    pub fn set_amps(&self, amp_mode: &str) -> Result<(), JsValue> {
        let select = serde_json::from_str(&amp_mode)
                                    .map_err(|e| e.to_string())?;
        self.player.borrow_mut().ay_amp_select = select;
        Ok(())
    }
    /// Sets the channel mode for the player.
    #[wasm_bindgen(js_name = setChannels)]
    pub fn set_channels(&self, chan_mode: &str) -> Result<(), JsValue> {
        let chan_mode = serde_json::from_str(&chan_mode)
                                    .map_err(|e| e.to_string())?;
        self.player.borrow_mut().player.set_channels_mode(chan_mode);
        Ok(())
    }
    /// Sets the gain for this audio source, between 0.0 and 1.0.
    #[wasm_bindgen(js_name = setGain)]
    pub fn set_gain(&self, gain: f32) {
        self.player.borrow_mut().set_gain(gain);
    }
}

// for a placeholder closure
fn nothing() -> Result<(), JsValue> { Ok(()) }

impl AyWebPlayer {
    fn new(buffer_duration: f32) -> Result<Self, JsValue> {
        if buffer_duration < 0.1 || buffer_duration > 1.0 {
            return Err("requested buffer duration should be between 0.1 and 1.0".into());
        }
        let ctx = web_sys::AudioContext::new()?;
        let sample_rate = ctx.sample_rate();
        let buffer_length = (sample_rate * buffer_duration).trunc() as u32;
        console_log!("sample_rate: {} buffer_length: {}", sample_rate, buffer_length);

        let player = AyFilePlayer::new(sample_rate as u32);

        let buffer0 = Vec::new();
        let buffer1 = Vec::new();
        let buffer_sourced = ctx.create_buffer(2, buffer_length, sample_rate)?;
        let buffer_duration = buffer_sourced.duration();
        let buffer_next = ctx.create_buffer(2, buffer_length, sample_rate)?;
        let curr_end_closure = Closure::once(nothing);
        let next_end_closure = Closure::once(nothing);
        let gain = ctx.create_gain()?;
        let ay_amp_select = AyAmpSelect::Spec;

        gain.gain().set_value(0.0);
        gain.connect_with_audio_node(&ctx.destination())?;

        Ok(AyWebPlayer {
            player,
            buffer_length,
            buffer_duration,
            ctx,
            buffer0,
            buffer1,
            cursor: 0..0,
            buffer_sourced,
            buffer_next,
            curr_end_closure,
            next_end_closure,
            gain,
            ay_amp_select
        })
    }
    // fills buffer_next with audio from rendered frames
    fn render_frames<V: AmpLevels<f32>>(&mut self) -> Result<(), JsValue> {
        let buffer_length = self.buffer_length;
        let mut range = self.cursor.clone();
        let mut start = range.len() as u32;
        if start != 0 {
            self.buffer_next.copy_to_channel_with_start_in_channel(&mut self.buffer0[range.clone()], 0, 0)?;
            self.buffer_next.copy_to_channel_with_start_in_channel(&mut self.buffer1[range.clone()], 1, 0)?;
        }
        range.end = loop {
            let nsamples = self.player.run_frame::<V>();
            self.buffer0.resize(nsamples, 0.0);
            self.buffer1.resize(nsamples, 0.0);
            self.player.render_audio_channel(0, &mut self.buffer0);
            self.player.render_audio_channel(1, &mut self.buffer1);
            self.buffer_next.copy_to_channel_with_start_in_channel(&mut self.buffer0, 0, start)?;
            self.buffer_next.copy_to_channel_with_start_in_channel(&mut self.buffer1, 1, start)?;
            self.player.next_frame();
            start += nsamples as u32;
            if start >= buffer_length {
                break nsamples
            }
        };
        range.start = range.end - (start - buffer_length) as usize;
        self.cursor = range;
        Ok(())
    }
    /// Sets the gain for this oscillator, between 0.0 and 1.0.
    fn set_gain(&self, gain: f32) {
        self.gain.gain().set_value(gain.min(1.0).max(0.0));
    }
}
