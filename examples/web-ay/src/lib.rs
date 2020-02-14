#![allow(unused_imports)]
mod player;
use std::ops::Deref;
use core::ops::Range;
use core::cell::{RefCell, RefMut};
use std::rc::{Rc, Weak};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use js_sys::{self, Promise, Uint8Array, Function};
use web_sys::{
    window,
    ProgressEvent,
    Response,
    Event,
    File,
    FileReader,
    AudioBuffer,
    // AudioBufferSourceNode,
    AudioContext,
    AudioContextState,
    // AudioDestinationNode,
    // AudioNode,
    // AudioParam,
    // AudioScheduledSourceNode,
    // BaseAudioContext,
    // GainNode,
    // OscillatorType,
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

pub struct AyPlayer {
    player: AyFilePlayer,
    buffer0: Vec<f32>,
    buffer1: Vec<f32>,
    cursor: Range<usize>,
    buffer_sourced: AudioBuffer,
    buffer_next: AudioBuffer,
    buffer_length: u32,
    buffer_duration: f64,
    start_time: f64,
    ctx: AudioContext,
    gain: web_sys::GainNode,
}

impl Drop for AyPlayer {
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

#[wasm_bindgen]
pub struct AyPlayerHandle {
    player_closure: Rc<RefCell<(AyPlayer, OnEndClosure)>>,
    status: PlayerStatus
}

#[wasm_bindgen]
impl AyPlayerHandle {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<AyPlayerHandle, JsValue> {
        let player = AyPlayer::new()?;
        let closure = Closure::once(move || -> Result<(), JsValue> { Ok(()) });
        let player_closure = Rc::new(RefCell::new((player, closure)));
        Ok(AyPlayerHandle {
            player_closure,
            status: PlayerStatus::Idle
        })
    }

    #[wasm_bindgen]
    pub fn play(&mut self, song_index: u32) -> Result<JsValue, JsValue> {
        if let PlayerStatus::Playing = self.status {
            Err("Playing already.")?;
        }
        let song_info;
        {
            let player = &mut self.player_closure.borrow_mut().0;
            song_info = match player.player.set_song(song_index as usize) {
                Some(info) => info,
                None if song_index != 0 => return Err("No song at a given index.".into()),
                None => json!(null)
            };
            player.start_time = player.ctx.current_time();
        }
        Self::play_next_frame(&self.player_closure)
            .map(|_| {
                self.status = PlayerStatus::Playing;
                JsValue::from_serde(&song_info).unwrap()
            })
    }

    /// `file` can be an instance of `File` or an url string.
    #[wasm_bindgen]
    pub fn load(&mut self, file: JsValue) -> Promise {
        if let PlayerStatus::Playing = self.status {
            return Promise::reject(&"Can't load, already playing.".into());
        }
        let player_closure = Rc::clone(&self.player_closure);
        let url = match file.dyn_into::<File>() {
            Ok(file) => return Self::load_file(player_closure, file),
            Err(other) => other
        };
        if url.is_string() {
            if let Some(url) = url.as_string() {
                return future_to_promise(
                    Self::load_url_async(player_closure, url)
                );
            }
        }
        Promise::reject(&"Only url string or File instances please.".into())
    }

    /// Return true if paused, false if unpaused, null if not playing
    #[wasm_bindgen(js_name = togglePause)]
    pub fn toggle_pause(&self) -> Promise {
        match self.status {
            PlayerStatus::Idle => Promise::resolve(&JsValue::NULL),
            PlayerStatus::Playing => {
                let player_closure = Rc::clone(&self.player_closure);
                future_to_promise(
                    Self::toggle_pause_async(player_closure)
                )
            }
        }
    }

    async fn toggle_pause_async(
                player_closure: Rc<RefCell<(AyPlayer, OnEndClosure)>>
            ) -> Result<JsValue, JsValue>
    {
        let mut resumed = false;
        let promise = {
            let ctx = &player_closure.borrow().0.ctx;
            match ctx.state() {
                AudioContextState::Suspended => {
                    resumed = true;
                    ctx.resume()?
                }
                AudioContextState::Running => ctx.suspend()?,
                _ => Err("Audio context closed")?
            }
        };
        let _ = JsFuture::from(promise).await?;
        if resumed {
            // let ay_player = &mut player_closure.borrow_mut().0;
            // ay_player.start_time = ay_player.ctx.current_time();
            Ok(JsValue::FALSE)
        }
        else {
            Ok(JsValue::TRUE)
        }
    }

    fn load_file(
                player_closure: Rc<RefCell<(AyPlayer, OnEndClosure)>>,
                file: File
            ) -> Promise
    {
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
            let player_closure = Rc::clone(&player_closure);
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
                let info = match player_closure.borrow_mut().0.player.load_file(data) {
                    Ok(info) => info,
                    Err(err) => return reject(err.into())
                };
                let _ = resolve.call1(&JsValue::NULL, &JsValue::from_serde(&info).unwrap());
            });
            file_reader.set_onloadend(Some(cb.unchecked_ref()));
        })
    }

    async fn load_url_async(
                player_closure: Rc<RefCell<(AyPlayer, OnEndClosure)>>,
                url: String
            ) -> Result<JsValue, JsValue>
    {
        let response = window().unwrap().fetch_with_str(&url);
        let response = JsFuture::from(response).await?.dyn_into::<Response>()?;
        if !response.ok() {
            return Err(response.into());
        }
        let array_buffer = JsFuture::from(response.array_buffer()?).await?;
        let data = Uint8Array::new(&array_buffer).to_vec();
        let info = player_closure.borrow_mut().0.player.load_file(data)?;
        Ok(JsValue::from_serde(&info).unwrap())
    }

    fn play_next_frame(
                player_cb: &Rc<RefCell<(AyPlayer, OnEndClosure)>>
            ) -> Result<(), JsValue>
    {
        let (mut player, mut closure) = RefMut::map_split(player_cb.borrow_mut(),
                                                                    |(p,c)| (p,c));
        let source = {
            {
                let player = &mut *player;
                core::mem::swap(&mut player.buffer_sourced, &mut player.buffer_next);
            }
            let source = player.ctx.create_buffer_source()?;
            source.set_buffer(Some(&player.buffer_next));
            source.connect_with_audio_node(&player.gain)?;
            source
        };
        player.start_time += player.buffer_duration;
        source.start_with_when(player.start_time)?;
        let weakref = Rc::downgrade(player_cb);
        *closure = Closure::once(move || -> Result<(), JsValue> {
            if let Some(rc) = weakref.upgrade() {
                return Self::play_next_frame(&rc);
            }
            Ok(())
        });
        source.set_onended(Some( closure.as_ref().unchecked_ref() ));
        player.render_frames()?;
        Ok(())
    }

    /// Sets the gain for this oscillator, between 0.0 and 1.0.
    #[wasm_bindgen(js_name = setGain)]
    pub fn set_gain(&self, gain: f32) {
        self.player_closure.borrow_mut().0.set_gain(gain);
    }
}

impl AyPlayer {
    fn new() -> Result<AyPlayer, JsValue> {
        let ctx = web_sys::AudioContext::new()?;
        let sample_rate = ctx.sample_rate();
        let buffer_length = (sample_rate * 0.2).round() as u32;
        console_log!("sample_rate: {} buffer_length: {}", sample_rate, buffer_length);

        let player = AyFilePlayer::new(sample_rate as u32);

        let buffer0 = Vec::new();
        let buffer1 = Vec::new();
        let buffer_sourced = ctx.create_buffer(2, buffer_length, sample_rate)?;
        let buffer_duration = buffer_sourced.duration();
        let buffer_next = ctx.create_buffer(2, buffer_length, sample_rate)?;
        let gain = ctx.create_gain()?;

        gain.gain().set_value(0.0);
        gain.connect_with_audio_node(&ctx.destination())?;

        Ok(AyPlayer {
            player,
            start_time: 0.0,
            buffer_length,
            buffer_duration,
            ctx,
            buffer0,
            buffer1,
            cursor: 0..0,
            buffer_sourced,
            buffer_next,
            gain,
        })
    }

    fn render_frames(&mut self) -> Result<(), JsValue> {
        let buffer_length = self.buffer_length;
        let mut range = self.cursor.clone();
        let mut start = range.len() as u32;
        if start != 0 {
            self.buffer_next.copy_to_channel_with_start_in_channel(&mut self.buffer0[range.clone()], 0, 0)?;
            self.buffer_next.copy_to_channel_with_start_in_channel(&mut self.buffer1[range.clone()], 1, 0)?;
        }
        range.end = loop {
            let nsamples = self.player.run_frame();
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
    fn set_gain(&self, mut gain: f32) {
        if gain > 1.0 {
            gain = 1.0;
        }
        if gain < 0.0 {
            gain = 0.0;
        }
        self.gain.gain().set_value(gain);
    }
}
