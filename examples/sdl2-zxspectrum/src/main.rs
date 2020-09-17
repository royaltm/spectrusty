/*
    sdl2-zxspectrum: ZX Spectrum emulator example as a SDL2 application.
    Copyright (C) 2020  Rafal Michalski

    sdl2-zxspectrum is free software: you can redistribute it and/or modify
    it under the terms of the GNU General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    sdl2-zxspectrum is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU General Public License for more details.

    You should have received a copy of the GNU General Public License
    along with this program.  If not, see <https://www.gnu.org/licenses/>.

    Author contact information: see Cargo.toml file, section [package.authors].
*/
// #![windows_subsystem = "windows"] // it is "console" by default
// #![allow(unused_assignments)]
// #![allow(unused_imports)]
// #![allow(dead_code)]
// #![allow(unused_variables)]
// #![allow(unused_mut)]

use core::convert::TryInto;
use core::fmt;
use std::fs::{File, OpenOptions};
use std::str::FromStr;

#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

#[allow(unused_imports)]
use sdl2::{Sdl,
           event::{Event, WindowEvent},
           keyboard::{Scancode, Keycode, Mod as Modifier},
           mouse::MouseButton,
           pixels::PixelFormatEnum,
           rect::Rect,
           video::{WindowPos, FullscreenType, Window, WindowContext}};
use chrono::prelude::*;

use spectrusty::z80emu::{Z80Any, Cpu};
use spectrusty::audio::UlaAudioFrame;
use spectrusty::clock::VFrameTs;
use spectrusty::chip::{MemoryAccess, UlaCommon, HostConfig};

use spectrusty::peripherals::memory::ZxInterface1MemExt;
use spectrusty::bus::{
    ay::{Ay3_891xMelodik, Ay3_891xFullerBox},
    mouse::KempstonMouse,
    zxinterface1::{MicroCartridge}
};
use spectrusty::formats::{
    mdr::MicroCartridgeExt,
    scr::ScreenDataProvider
};
use spectrusty::video::{BorderSize, pixel};
use zxspectrum_common::{
    ModelRequest,
    DynamicDevices,
    JoystickAccess,
    UlaPlusMode,
    VideoControl,
    spectrum_model_dispatch
};

#[macro_use]
mod utils;
mod emulator;

use utils::*;
use emulator::*;

type PixelBuf<'a> = pixel::PixelBufP32<'a>;
type SpectrumPal = pixel::SpectrumPalA8R8G8B8;
const PIXEL_FORMAT: PixelFormatEnum = PixelFormatEnum::RGB888;
const KEYBOARD_IMAGE: &[u8] = include_bytes!("../../../resources/keyboard48.jpg");

const HEAD: &str = r#"SPECTRUSTY: a desktop SDL2 example emulator."#;
const COPY: &str = r###"
This program is an example of how to use the SPECTRUSTY library 
to create a ZX Spectrum emulator using Simple DirectMedia Layer 2.

ZX Spectrum ROM © 1982-1987 Amstrad PLC.
OpenSE BASIC © 2000-2012 Nine Tiles Networks Ltd, Andrew Owen.
BBC BASIC (Z80) © 1982-2000 R.T. Russell, 1989-2005 J.G. Harston.

© 2020 Rafał Michalski.
This program comes with ABSOLUTELY NO WARRANTY.
This is free software, and you are welcome to redistribute it
under certain conditions.

See: https://royaltm.github.io/spectrusty/
"###;
const HELP: &str = r###"
Drag & drop TAP, SNA or SCR files over the emulator window in order to load them.

Esc: Release grabbed pointer.
F1: Shows this help.
F2: Turbo - runs as fast as possible, while key is being pressed.
F3: Saves printer spooler content.
F4: Changes joystick implementation (cursor keys).
F5: Plays current TAP file.
F6: Shows current TAP file info.
F7: Cycles through: fast load on/off, then tape audio on/off.
F8: Starts recording of TAP chunks appending them to the current TAP file.
F9: Soft reset.
F10: Hard reset.
F11: Triggers non-maskable interrupt.
F12: Cycles through border sizes.
ScrLck: Show/hide keyboard image.
PrtScr: Save screen as .SCR file.
Pause: Pauses/resumes emulation.
Insert: Creates a new TAP file.
Delete: Removes the current TAP from the emulator.
Home/End: Moves a TAP cursor to the beginning/end of current TAP file.
PgUp/PgDown: Moves a TAP cursor to the previous/next block of current TAP file.
Tab: Toggles ULAPlus mode if a computer model supports it.
"###;

use clap::clap_app;

const REQUESTED_AUDIO_LATENCY: usize = 1;
use std::thread;

fn main() -> Result<()> {
    simple_logger::SimpleLogger::new().with_level(log::LevelFilter::Info).init()?;

    let matches = clap_app!(SDL2_SPECTRUSTY_Example =>
        (version: "1.0")
        (author: "Rafał Michalski")
        (about: HEAD)
        (@arg audio: --audio +takes_value "Audio latency")
        (@arg border: -b --border +takes_value "Initial border size")
        (@arg cpu: -c --cpu +takes_value "Select CPU type")
        (@arg model: -m --model +takes_value "Selects emulated model")
        (@arg joystick: -j --joystick +takes_value "Selects joystick at startup")
        (@arg printer: -p --printer "Inserts ZX Printer device")
        (@arg interface1: -i --interface1 +takes_value "Installs ZX Interface 1 (ROM path required)")
        (@arg zxnet_bind: --bind +takes_value "Bind address of ZX-NET UDP socket")
        (@arg zxnet_conn: --conn +takes_value "Connect address of ZX-NET UDP socket")
        (@arg melodik: --ay "Inserts Melodik AY-3-8910 device")
        (@arg fuller: --fuller "Inserts Fuller AY-3-8910 device")
        (@arg mouse: --mouse "Inserts Kempston Mouse device")
        (@arg FILES: ... "Sets the input file(s) to load at startup")
    ).get_matches();

    set_dpi_awareness()?;

    // println!("{:?}", matches.values_of("FILES"));

    thread::Builder::new().stack_size(4096 * 1024)
    .spawn(move || {
        select_model(matches).unwrap()
    })?
    .join().unwrap();
    Ok(())
}

fn select_model(matches: clap::ArgMatches) -> Result<()> {
    let mut mreq = ModelRequest::SpectrumPlus2B;

    let model;
    let allow_autoload;

    let files = matches.values_of("FILES");

    if let Some(filepath) = files.and_then(|mut f|
                                    f.find(|&fpath|
                                        snap_file_ext(fpath) == Some("json")
                                    )
                                )
    {
        let file = File::open(filepath)?;
        model = serde_json::from_reader(file)?;
        mreq = ModelRequest::from(&model);
        allow_autoload = false;
    }
    else {
        if let Some(model) = matches.value_of("model") {
            match model.parse() {
                Ok(m) => mreq = m,
                Err(e) => {
                    eprintln!("{}!", e);
                    eprintln!("Available models (\"ZX Spectrum\" or \"Timex\" prefixes may be omitted):");
                    for m in ModelRequest::iter() {
                        eprintln!("- {}", m);
                    }
                    return Ok(())
                }
            }
        }
        model = ZxSpectrumModel::new(mreq);
        allow_autoload = true;
    }

    info!("{}: cpu_hz: {} T-states/frame: {} pixel density: {}",
        ModelRequest::from(&model),
        model.cpu_rate(),
        model.frame_tstates_count(),
        model.pixel_density()
    );

    spectrum_model_dispatch!(model(spec) => config_and_run(mreq, spec, matches, allow_autoload))
}

fn config_and_run<U: 'static>(
        mreq: ModelRequest,
        mut spec: ZxSpectrum<Z80Any, U>,
        matches: clap::ArgMatches,
        allow_autoload: bool,
    ) -> Result<()>
    where U: HostConfig
           + UlaCommon
           + UlaAudioFrame<BandLim>
           + SpoolerAccess
           + ScreenDataProvider
           + UlaPlusMode
           + MemoryAccess<MemoryExt = ZxInterface1MemExt>
           + serde::Serialize,
          ZxSpectrum<Z80Any, U>: JoystickAccess
{
    let sdl_context = sdl2::init()?;

    let latency = matches.value_of("audio").map(usize::from_str).transpose()?
                             .unwrap_or(REQUESTED_AUDIO_LATENCY);

    if let Some(cpu) = matches.value_of("cpu") {
        if cpu.eq_ignore_ascii_case("nmos") {
            spec.cpu = spec.cpu.clone().into_nmos();
        }
        else if cpu.eq_ignore_ascii_case("cmos") {
            spec.cpu = spec.cpu.clone().into_cmos();
        }
        else if cpu.eq_ignore_ascii_case("bm1") || cpu.eq_ignore_ascii_case("bm") {
            spec.cpu = spec.cpu.clone().into_bm1();
        }
        else {
            eprintln!(r#"Unknown CPU type "{}".\nSelect one of: NMOS, CMOS, BM1"#, cpu);
            return Ok(())
        }
    }

    if let Some(border_size) = matches.value_of("border") {
        spec.state.border_size = BorderSize::from_str(border_size)?
    }

    if let Some(rom_path) = matches.value_of("interface1") {
        let rom_file = File::open(rom_path)?;
        spec.ula.memory_ext_mut().load_if1_rom(rom_file)?;
        spec.attach_device(ZxInterface1::<VFrameTs<U::VideoFrame>>::default());
        info!("Interface 1 installed");
        let network = spec.zxif1_network_mut().unwrap();
        if let Some(bind) = matches.value_of("zxnet_bind") {
            info!("ZX-NET bind: {}", bind);
            network.socket.bind(bind)?;
        }
        if let Some(conn) = matches.value_of("zxnet_conn") {
            info!("ZX-NET conn: {}", conn);
            network.socket.connect(conn)?;
        }
        for n in 0..1 {
            spec.microdrives_mut().unwrap().replace_cartridge(n,
                    MicroCartridge::new_formatted(180, format!("blank {}", n + 1)));
        }
    }
    else if matches.is_present("zxnet_bind") || matches.is_present("zxnet_conn") {
        eprintln!(r#"--bind and --conn options require --interface1"#, );
        return Ok(())
    }

    if matches.is_present("printer") {
        spec.attach_device(ZxPrinter::<VFrameTs<U::VideoFrame>>::default());
        info!("ZX Printer installed");
    }

    if matches.is_present("mouse") {
        spec.attach_device(KempstonMouse::default());
        info!("Kempston Mouse installed");
    }

    if matches.is_present("melodik") {
        spec.attach_device(Ay3_891xMelodik::<_>::default());
        info!("AY-3-8910 Melodik installed");
    }

    if matches.is_present("fuller") {
        spec.attach_device(Ay3_891xFullerBox::default());
        info!("AY-3-8910 Fuller Box installed");
    }

    if let Some(joy) = matches.value_of("joystick") {
        let index = if joy.eq_ignore_ascii_case("K") || joy.eq_ignore_ascii_case("Kempston")  { 0 }
        else if joy.eq_ignore_ascii_case("F") || joy.eq_ignore_ascii_case("Fuller") { 1 }
        else if joy.eq_ignore_ascii_case("S1") || joy.eq_ignore_ascii_case("Sinclair1") { 2 }
        else if joy.eq_ignore_ascii_case("S2") || joy.eq_ignore_ascii_case("Sinclair2") { 3 }
        else if joy.eq_ignore_ascii_case("C") || joy.eq_ignore_ascii_case("Cursor") { 4 }
        else {
            eprintln!("Unknown joystick: {}", joy);
            return Ok(())
        };
        spec.select_joystick(index);
    }

    let mut zx = ZxSpectrumEmu::new_with(mreq, spec, &sdl_context, latency)?;
    let mut show_copyright = allow_autoload;

    // load non-snapshot files
    if let Some(files) = matches.values_of("FILES") {
        for filepath in files.filter(|&filepath|
                                        snap_file_ext(filepath).is_none()
                                    )
        {
            info!("Loading file: {}", filepath);
            match zx.handle_file(filepath)? {
                Some(msg) => if !msg.is_empty() {
                    info!("{}", msg)
                },
                None => warn!("Unknown file type: {}", filepath),
            }
        }

        if zx.spectrum.state.tape.is_inserted() && allow_autoload {
            zx.spectrum.reset_and_load(zx.model)?;
            show_copyright = false;
        }
    }

    // run the emulator
    run(&mut zx, &sdl_context, show_copyright)?;

    // save the snapshot
    let json_name = format!("spectrusty_{}.json", Utc::now().format("%Y-%m-%d_%H%M%S%.f"));
    let json_file = std::fs::File::create(&json_name)?;
    serde_json::to_writer(json_file, &zx)?;
    info!("Saved a snapshot file: {}", json_name);

    Ok(())
}

fn run<C, U: 'static>(
        zx: &mut ZxSpectrumEmu<C, U>,
        sdl_context: &Sdl,
        show_copyright: bool
    ) -> Result<()>
    where C: Cpu + fmt::Display,
          U: UlaCommon
           + UlaAudioFrame<BandLim>
           + SpoolerAccess
           + ScreenDataProvider
           + UlaPlusMode,
          ZxSpectrum<C, U>: JoystickAccess
{
    // SDL2 related code follows
    // the context variables, created below could be a part of some struct in a mature program
    let video_subsystem = sdl_context.video()?;
    debug!("driver: {}", video_subsystem.current_video_driver());

    let (screen_width, screen_height) = zx.target_screen_size();
    debug!("{:?} {}x{}", zx.spectrum.state.border_size, screen_width, screen_height);
    let window = video_subsystem.window(zx.short_info()?, screen_width, screen_height)
                            // .resizable()
                            .allow_highdpi()
                            .position_centered()
                            .build()
                            .map_err(err_str)?;

    let mut canvas = window.into_canvas()
                    .accelerated()
                    // .present_vsync()
                    .build()
                    .map_err(err_str)?;
    let canvas_id = canvas.window().id();

    let texture_creator = canvas.texture_creator();
    let create_texture = |(width, height)| {
        texture_creator.create_texture_streaming(PIXEL_FORMAT, width, height)
                       .map_err(err_str)
    };

    let mut texture = create_texture(zx.spectrum.render_size_pixels())?;
    debug!("canvas: {:?}", canvas.window().window_pixel_format());
    debug!("stream: {:?}", texture.query());

    let mut keyboard_visible = false;
    let mut keyboard_canvas = create_image_canvas_window(&video_subsystem, KEYBOARD_IMAGE)?;
    let keyboard_canvas_id = keyboard_canvas.window().id();
    keyboard_canvas.window_mut().hide();

    // sdl_context.mouse().show_cursor(false);

    // let timer_subsystem = sdl_context.timer()?;

    let mut event_pump = sdl_context.event_pump()?;

    let mut file_count = 0;
    // mouse move resultant on keyboard helper window
    let mut move_keyboard: Option<(i32, i32)> = None;

    if show_copyright {
        info_window(HEAD, COPY.into());
    }

    // here we run a simple: read events, emulate and draw loop
    'mainloop: loop {
        let mut update_info = false;
        for event in event_pump.poll_iter() {
            match event {
                Event::Window { win_event: WindowEvent::Close, .. }|
                Event::Quit { .. } => {
                    break 'mainloop
                }
                Event::MouseMotion{ window_id, xrel, yrel, .. } if window_id == canvas_id => {
                    // println!("{}x{}", xrel, yrel);
                    zx.move_mouse(xrel, yrel);
                    continue
                }
                Event::MouseMotion {
                    window_id, mousestate, xrel, yrel, ..
                }
                if mousestate.left() && window_id == keyboard_canvas_id => {
                    let (dx, dy) = move_keyboard.unwrap_or((0, 0));
                    let (dx, dy) = (dx + xrel, dy + yrel);
                    if dx == 0 && dy == 0 {
                        move_keyboard = None;
                    }
                    else {
                        move_keyboard = Some((dx, dy));
                        let (x, y) = keyboard_canvas.window().position();
                        keyboard_canvas.window_mut().set_position(
                            WindowPos::Positioned(x + dx),
                            WindowPos::Positioned(y + dy));
                        keyboard_canvas.present();
                        continue
                    }
                }
                Event::KeyDown { keycode: Some(Keycode::Escape), repeat: false, .. } => {
                    if canvas.window().grab() { // escape mouse cursor grab mode
                        sdl_context.mouse().show_cursor(true);
                        canvas.window_mut().set_grab(false);
                    }
                }
                Event::MouseButtonDown { window_id, mouse_btn, .. }
                if window_id == canvas_id => {
                    if canvas.window().grab() {
                        zx.update_mouse_button(mouse_btn, true);
                    }
                    else {
                        sdl_context.mouse().show_cursor(false);
                        canvas.window_mut().set_grab(true);
                    }
                }
                Event::MouseButtonUp { window_id, mouse_btn, .. }
                if window_id == canvas_id => {
                    zx.update_mouse_button(mouse_btn, false);
                }
                Event::MouseButtonDown { window_id, mouse_btn: MouseButton::Left, .. }
                if window_id == keyboard_canvas_id => {
                    move_keyboard = None;
                }
                Event::MouseButtonUp { window_id, mouse_btn: MouseButton::Left, .. }
                if window_id == keyboard_canvas_id => {
                    move_keyboard = None;
                }
                Event::KeyDown{ keycode: Some(Keycode::F1), repeat: false, ..} => {
                    zx.audio.pause();
                    info_window(HEAD, format!("Hardware configuration:\n{}\n{}",
                                    zx.device_info()?, HELP).into());
                    zx.resume_audio();
                }
                Event::KeyDown{ keycode: Some(Keycode::F11), repeat: false, ..} => {
                    zx.spectrum.trigger_nmi();
                }
                Event::KeyDown{ keycode: Some(Keycode::F9), repeat: false, ..} => {
                    zx.spectrum.reset(false);
                }
                Event::KeyDown{ keycode: Some(Keycode::F10), repeat: false, ..} => {
                    zx.spectrum.reset(true);
                }
                Event::KeyDown{ keycode: Some(Keycode::Pause), repeat: false, ..} => {
                    let state = &mut zx.spectrum.state;
                    state.paused = if state.paused {
                        if !state.turbo {
                            zx.audio.play();
                        }
                        false
                    }
                    else {
                        if !state.turbo {
                            zx.audio.pause();
                        }
                        true
                    };
                    update_info = true;
                }
                Event::KeyDown { keycode: Some(Keycode::F2), repeat: false, ..}
                if !zx.spectrum.state.paused => {
                    if !zx.spectrum.state.turbo {
                        zx.audio.pause();
                        zx.spectrum.state.turbo = true;
                        update_info = true;
                    }
                }
                Event::KeyUp { keycode: Some(Keycode::F2), repeat: false, ..} => {
                    if zx.spectrum.state.turbo {
                        if !zx.spectrum.state.paused {
                            zx.audio.play();
                        }
                        zx.spectrum.state.turbo = false;
                        update_info = true;
                    }
                }
                Event::KeyDown { keycode: Some(Keycode::F3), repeat: false, ..} => {
                    zx.audio.pause();
                    info_window("Printer", zx.save_printed_images()?.into());
                    zx.resume_audio();
                    update_info = true;
                }
                Event::KeyDown { keycode: Some(Keycode::F4), repeat: false, ..} => {
                    zx.spectrum.select_next_joystick();
                    update_info = true;
                }
                Event::KeyDown { keycode: Some(Keycode::Insert), repeat: false, ..} => {
                    loop {
                        file_count += 1;
                        if file_count > 999 {
                            panic!("too much files!");
                        }
                        let name = format!("new_file{:04}.tap", file_count);
                        match OpenOptions::new().create_new(true).read(true).write(true).open(&name) {
                            Ok(file) => {
                                zx.spectrum.state.tape.insert_as_reader(file);
                                info!("Created TAP: {}", name);
                                break;
                            }
                            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
                            Err(e) => Err(e)?
                        }
                    }
                    update_info = true;
                }
                Event::KeyDown { keycode: Some(Keycode::Delete), repeat: false, ..} => {
                    if let Some(..) = zx.spectrum.state.tape.eject() {
                        info!("TAP removed");
                        update_info = true;
                    }
                }
                Event::KeyDown { keycode: Some(Keycode::End), repeat: false, ..} => {
                    if let Some(..) = zx.spectrum.state.tape.rewind_nth_chunk(u32::max_value())? {
                        update_info = true;
                    }
                }
                Event::KeyDown { keycode: Some(Keycode::Home), repeat: false, ..} => {
                    if let Some(..) = zx.spectrum.state.tape.rewind_nth_chunk(1)? {
                        update_info = true;
                    }
                }
                Event::KeyDown { keycode: Some(Keycode::PageUp), repeat: false, ..} => {
                    if let Some(..) = zx.spectrum.state.tape.rewind_prev_chunk()? {
                        update_info = true;
                    }
                }
                Event::KeyDown { keycode: Some(Keycode::PageDown), repeat: false, ..} => {
                    if let Some(..) = zx.spectrum.state.tape.forward_chunk()? {
                        update_info = true;
                    }
                }
                Event::KeyDown { keycode: Some(Keycode::F5), repeat: false, ..} => {
                    if zx.spectrum.state.tape.is_idle() {
                        zx.spectrum.state.tape.play()?;
                        update_info = true;
                    }
                    else if zx.spectrum.state.tape.is_playing() {
                        zx.spectrum.state.tape.stop();
                        update_info = true;
                    }
                }
                Event::KeyDown { keycode: Some(Keycode::F6), repeat: false, ..} => {
                    zx.audio.pause();
                    info_window("Tape recorder", zx.tape_info()?);
                    zx.resume_audio();
                }
                Event::KeyDown { keycode: Some(Keycode::F7), repeat: false, ..} => {
                    if zx.spectrum.state.instant_tape {
                        zx.spectrum.state.instant_tape = false;
                        zx.spectrum.state.flash_tape = false;
                        zx.spectrum.state.audible_tape = !zx.spectrum.state.audible_tape;
                    }
                    else if zx.spectrum.state.flash_tape {
                        zx.spectrum.state.instant_tape = true;
                    }
                    else {
                        zx.spectrum.state.flash_tape = true;
                    }
                    update_info = true;
                }
                Event::KeyDown { keycode: Some(Keycode::F8), repeat: false, ..} => {
                    if zx.spectrum.state.tape.is_idle() {
                        zx.spectrum.state.tape.record()?;
                        update_info = true;
                    }
                    else if zx.spectrum.state.tape.is_recording() {
                        zx.spectrum.state.tape.stop();
                        update_info = true;
                    }
                }
                Event::KeyDown { keycode: Some(Keycode::F12), repeat: false, ..} => {
                    let border_size = &mut zx.spectrum.state.border_size;
                    *border_size = u8::from(*border_size).wrapping_sub(1).try_into().unwrap_or(BorderSize::Full);
                    let (w, h) = zx.target_screen_size();
                    canvas.window_mut().set_size(w, h)?;
                    texture = create_texture(zx.spectrum.render_size_pixels())?;
                }
                Event::KeyDown { keycode: Some(Keycode::ScrollLock), repeat: false, ..} => {
                    if keyboard_visible {
                        keyboard_canvas.window_mut().hide();
                    }
                    else {
                        keyboard_canvas.window_mut().show();
                        let (mut x, mut y) = canvas.window().position();
                        let (w, h) = canvas.window().size();
                        y += h as i32;
                        let (kw, _) = keyboard_canvas.window().size();
                        x += (w as i32 - kw as i32)/2;
                        keyboard_canvas.window_mut().set_position(WindowPos::Positioned(x), WindowPos::Positioned(y));
                        keyboard_canvas.present();
                        move_keyboard = None;
                    }
                    keyboard_visible = !keyboard_visible;
                }
                Event::KeyDown { keycode: Some(Keycode::PrintScreen), repeat: false, ..} => {
                    zx.audio.pause();
                    let filename = zx.save_screen()?;
                    info_window("Screen captured", format!("Screen saved as:\n{}", filename).into());
                    zx.resume_audio();
                }
                Event::KeyDown { keycode: Some(Keycode::Tab), repeat: false, ..} => {
                    update_info |= zx.spectrum.ula.enable_ulaplus_modes(
                        !zx.spectrum.ula.is_ulaplus_enabled());

                }
                Event::KeyDown{ keycode: Some(keycode), keymod, repeat: false, ..} => {
                    zx.handle_keypress_event(keycode, keymod, true);
                }
                Event::KeyUp{ keycode: Some(keycode), keymod, repeat: false, ..} => {
                    zx.handle_keypress_event(keycode, keymod, false);
                }
                Event::DropFile { filename, .. } => {
                    zx.audio.pause();
                    match zx.handle_file(&filename) {
                        Ok(Some(msg)) => {
                            if !msg.is_empty() {
                                info_window("File", msg.into());
                            }
                            update_info = true;
                        }
                        Ok(None) => info_window("File", format!("Unknown file type: {}", filename).into()),
                        Err(e) => alert_window(e.to_string().into()),
                    }
                    zx.resume_audio();
                }
                _ => {
                    // continue 'mainloop
                }
            }
        }

        if !zx.spectrum.state.paused {
            let viewport = canvas.window().drawable_size();
            zx.send_mouse_move(viewport);

            let state_changed = if zx.spectrum.state.turbo {
                zx.spectrum.run_frames_accelerated(&mut zx.time_sync)?.1
            }
            else {
                let sc = zx.spectrum.run_frame()?.1;
                zx.render_audio()?;
                zx.synchronize_thread_to_frame();
                sc
            };
            update_info |= state_changed;
            if state_changed {
                if zx.spectrum.state.turbo {
                    zx.audio.pause();
                }
                else {
                    zx.resume_audio();
                }
            }
        }

        if update_info {
            canvas.window_mut().set_title(zx.short_info()?)?;
        }
        else if let Some(title) = zx.update_dynamic_info()? {
            canvas.window_mut().set_title(title)?;
        }

        if zx.spectrum.state.paused {
            zx.synchronize_thread_to_frame();
            continue;
        }

        // render pixels
        texture.with_lock(None, |buffer: &mut [u8], pitch: usize| {
            // measure_performance!("Video frame at: {:.4} Hz frames: {} wall: {} s"; 1.0f64, timer_subsystem, counter_elapsed, counter_iters, counter_sum; {
            zx.spectrum.ula.render_video_frame::<PixelBuf, SpectrumPal>(buffer, pitch, zx.spectrum.state.border_size);
            // 1.0});
        })?;
        // present pixels
        canvas.copy(&texture, None, None)?;
        canvas.present();
    }

    if let Some(mdrive) = zx.spectrum.microdrives_mut() {
        for i in 1..8 {
            mdrive.take_cartridge(i);
        }
        if let Some(mdr) = mdrive.take_cartridge(0) {
            if mdr.count_sectors_in_use() != 0 {
                let name = format!("microdrive_{}.mdr", Utc::now().format("%Y-%m-%d_%H%M%S%.f"));
                let mut file = std::fs::File::create(&name)?;
                mdr.write_mdr(&mut file)?;
                info!("Saved a microdrive: {}", name);
            }
        }
    }

    Ok(())
}
