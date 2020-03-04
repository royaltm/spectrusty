// #![windows_subsystem = "windows"] // it is "console" by default
// #![allow(unused_assignments)]
// #![allow(unused_imports)]
#![allow(dead_code)]
// #![allow(unused_variables)]
// #![allow(unused_mut)]

#[macro_use]
mod utils;
mod spectrum;

use std::convert::TryInto;
use std::collections::HashSet;

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
// use sdl2_sys::SDL_WindowFlags;
use crate::spectrum::*;
use crate::peripherals::DeviceAccess;
use crate::utils::*;

const REQUESTED_AUDIO_LATENCY: usize = 2;

const KEYBOARD_IMAGE: &[u8] = include_bytes!("../../../resources/ZXSpectrumKeys704.jpg");

const HEAD: &str = r#"The SDL2 desktop example emulator for "rust-zxspecemu""#;
const HELP: &str = r###"
Drag & drop TAP, SNA or SCR files over the emulator window in order to load them.

Esc: Release grabbed pointer.
F1: Shows this help.
F2: Turbo - runs as fast as possible, while key is being pressed.
F3: Saves ZX Printer spooled image as a PNG file. [+printer opt. only]
F4: Changes joystick implementation (cursor keys).
F5: Plays current TAP file.
F6: Prints current TAP file info (only if not playing or recording).
F7: Lists tap file names.
F8: Starts recording of TAP chunks appending them to the current TAP file.
F9: Toggles on/off tape audio.
F10: Resets ZX Spectrum.
F11: Triggers non-maskable interrupt.
F12: Change border size.
ScrLck: Show/hide keyboard image.
Pause: Pauses/resumes emulation.
Insert: Creates a new TAP file for recording.
Delete: Removes a current TAP from the emulator.
Home/End: Moves a TAP cursor to the beginning/end of current TAP file.
PgUp/PgDown: Moves a TAP cursor to the previous/next block of current TAP file.
Home/End+SHIFT: Selects a first/last TAP file.
PgUp/PgDown+SHIFT: Selects a previous/next TAP file.
"###;

enum EmulatorStatus {
    Normal,
    Paused,
    Turbo
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    simple_logger::init_with_level(log::Level::Info).unwrap();

    set_dpi_awareness()?;
    let sdl_context = sdl2::init()?;

    let (mut cfg, files): (HashSet<_>,HashSet<_>) = std::env::args().skip(1)
                                        .partition(|arg| arg.starts_with("+"));

    if cfg.is_empty() {
        let zx = ZXSpectrum48::create(&sdl_context, REQUESTED_AUDIO_LATENCY)?;
        run(zx, sdl_context, files)
    }
    else if cfg.remove("+128") {
        let zx = ZXSpectrum128::create(&sdl_context, REQUESTED_AUDIO_LATENCY)?;
        run(zx, sdl_context, files)
    }
    else {
        let mut zx = ZXSpectrum48DynBus::create(&sdl_context, REQUESTED_AUDIO_LATENCY)?;
        let all = cfg.remove("+all");
        if cfg.remove("+mouse") || all {
            zx.add_mouse();
        }
        if cfg.remove("+printer") || all {
            zx.add_printer();
        }
        if cfg.remove("+ay") || cfg.remove("+melodik") || all {
            zx.add_ay_melodik();
        }
        if cfg.remove("+fullerbox") || all {
            zx.add_fullerbox();
        }
        if cfg.is_empty() {
            run(zx, sdl_context, files)
        }
        else {
            Err(format!("Unrecognized options: {}. Provide one of: +mouse, +printer, +ay|melodik, +fullerbox",
                cfg.drain().collect::<Vec<_>>().join(" ")))?
        }
    }
}

fn run<C, U, I>(
        mut zx: ZXSpectrum<C, U>,
        sdl_context: Sdl,
        files: I
    ) -> Result<(), Box<dyn std::error::Error>>
    where I: IntoIterator<Item=String>,
          C: Cpu + std::fmt::Debug,
          U: Default + UlaCommon + UlaAudioFrame<ZXBlep>,
          ZXSpectrum<C, U>: DeviceAccess<U::VideoFrame>,
          U::VideoFrame: 'static
{
    zx.reset();

    for filename in files {
        match zx.handle_file(&filename)? {
            FileType::Unknown => Err(format!("Unrecognized file: {}", filename))?,
            t => info!("File: {} {}", t, filename),
        }
        
    }

    let video_subsystem = sdl_context.video()?;
    // eprintln!("driver: {}", video_subsystem.current_video_driver());
    let mut border_size = BorderSize::Full;
    let window_title = |zx: &ZXSpectrum<C, U>| format!("ZX Spectrum {}", zx.device_info());

    let (screen_width, screen_height) = ZXSpectrum::<C, U>::screen_size(border_size);
    debug!("{:?} {}x{}", border_size, screen_width, screen_height);
    let window = video_subsystem.window(&window_title(&zx),
                                    screen_width*2, screen_height*2)
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
    let create_texture = |width, height| {
        texture_creator.create_texture_streaming(PixelFormatEnum::RGB24, width, height)
                       .map_err(err_str)
    };

    let mut texture = create_texture(screen_width, screen_height)?;

    let mut keyboard_visible = false;
    let mut keyboard_canvas = create_image_canvas_window(&video_subsystem, KEYBOARD_IMAGE)?;
    let keyboard_canvas_id = keyboard_canvas.window().id();
    keyboard_canvas.window_mut().hide();

    // sdl_context.mouse().show_cursor(false);

    let timer_subsystem = sdl_context.timer()?;

    let mut event_pump = sdl_context.event_pump()?;

    let mut counter_elapsed = 0u64;
    let mut counter_iters = 0usize;
    let mut counter_sum = 0.0;
    let mut file_count = 0;
    let mut status = EmulatorStatus::Normal;
    let mut printing: Option<usize> = None;
    let mut move_keyboard: Option<(i32, i32)> = None;

    'mainloop: loop {
        for event in event_pump.poll_iter() {
            match event {
                Event::Window { win_event: WindowEvent::Close, .. } | Event::Quit { .. } => break 'mainloop,
                Event::MouseMotion{ window_id, xrel, yrel, .. } if window_id == canvas_id => {
                    // let size = canvas.window().size();
                    // println!("{}x{}", xrel, yrel);
                    zx.move_mouse(xrel, yrel);
                }
                Event::MouseMotion {
                        window_id, mousestate, xrel, yrel, ..
                    } if mousestate.left() && window_id == keyboard_canvas_id => {
                    let (dx, dy) = move_keyboard.unwrap_or((0, 0));
                    let (dx, dy) = (dx + xrel, dy + yrel);
                    if dx == 0 && dy == 0 {
                        move_keyboard = None;
                    }
                    else {
                        move_keyboard = Some((dx, dy));
                        let (x, y) = keyboard_canvas.window().position();
                        keyboard_canvas.window_mut().set_position(WindowPos::Positioned(x + dx),
                                                      WindowPos::Positioned(y + dy));
                        keyboard_canvas.present();
                        continue;
                    }
                }
                Event::KeyDown{ keycode: Some(Keycode::Escape), repeat: false, ..} => {
                    if canvas.window().grab() {
                        sdl_context.mouse().show_cursor(true);
                        canvas.window_mut().set_grab(false);
                    }
                }
                Event::MouseButtonDown { window_id, mouse_btn, .. } if window_id == canvas_id => {
                    if canvas.window().grab() {
                        zx.update_mouse_button(mouse_btn, true);
                    }
                    else {
                        sdl_context.mouse().show_cursor(false);
                        canvas.window_mut().set_grab(true);
                    }
                }
                Event::MouseButtonDown { window_id, mouse_btn: MouseButton::Left, .. } if window_id == keyboard_canvas_id => {
                    move_keyboard = None;
                }
                Event::MouseButtonUp { window_id, mouse_btn, .. } if window_id == canvas_id => {
                    zx.update_mouse_button(mouse_btn, false);
                }
                Event::MouseButtonUp { window_id, mouse_btn: MouseButton::Left, .. } if window_id == keyboard_canvas_id => {
                    move_keyboard = None;
                }
                Event::KeyDown{ keycode: Some(Keycode::F1), repeat: false, ..} => {
                    zx.audio.pause();
                    info(format!("{}\nHardware configuration:\nRAM: {}\n{}",
                                    HEAD, zx.device_info(), HELP).into());
                    zx.audio.resume();
                }
                Event::KeyDown{ keycode: Some(Keycode::F11), repeat: false, ..} => {
                    zx.trigger_nmi();
                }
                Event::KeyDown{ keycode: Some(Keycode::F10), repeat: false, ..} => {
                    zx.reset();
                }
                Event::KeyDown{ keycode: Some(Keycode::Pause), repeat: false, ..} => {
                    status = match status {
                        EmulatorStatus::Turbo => {
                            EmulatorStatus::Paused
                        }
                        EmulatorStatus::Normal => {
                            zx.audio.pause();
                            EmulatorStatus::Paused
                        }
                        EmulatorStatus::Paused => {
                            zx.audio.resume();
                            EmulatorStatus::Normal
                        }
                    }
                }
                Event::KeyDown{ keycode: Some(Keycode::F2), repeat: false, ..} => {
                    if let EmulatorStatus::Normal = status {
                        zx.audio.pause();
                    }
                    status = EmulatorStatus::Turbo;
                }
                Event::KeyUp{ keycode: Some(Keycode::F2), repeat: false, ..} => {
                    if let EmulatorStatus::Turbo = status {
                        zx.audio.resume();
                        status = EmulatorStatus::Normal;
                    }
                }
                Event::KeyDown{ keycode: Some(Keycode::F3), repeat: false, ..} => {
                    zx.save_printed_image()?;
                    canvas.window_mut().set_title(&window_title(&zx))?;
                }
                Event::KeyUp{ keycode: Some(Keycode::F4), repeat: false, ..} => {
                    zx.next_joystick();
                    canvas.window_mut().set_title(&window_title(&zx))?;
                }
                Event::KeyDown{ keycode: Some(Keycode::Insert), repeat: false, ..} => {
                    loop {
                        file_count += 1;
                        if file_count > 999 {
                            panic!("too much files!");
                        }
                        let name = format!("new_file{:04}.tap", file_count);
                        match zx.tap_cabinet.create_file(&name) {
                            Ok(()) => {
                                info!("Created TAP: {}", name);
                                break;
                            }
                            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
                            e @ Err(_) => e?,
                        }
                    }
                    zx.tap_cabinet.select_last_tap();
                    zx.print_current_tap();
                }
                Event::KeyDown{ keycode: Some(Keycode::Delete), repeat: false, ..} => {
                    if let Some((_, path)) = zx.tap_cabinet.remove_current_tap() {
                        info!("TAP removed: {}", path.to_string_lossy());
                    }
                }
                Event::KeyDown{ keycode: Some(Keycode::End), keymod, repeat: false, ..} => {
                    if keymod.intersects(Modifier::LSHIFTMOD|Modifier::RSHIFTMOD) {
                        if zx.tap_cabinet.select_last_tap() {
                            zx.print_current_tap();
                        }
                    }
                    else if let Some(no) = zx.tap_cabinet.skip_tap_chunks(usize::max_value())? {
                        zx.print_current_tap_chunk(no.get());
                    }
                }
                Event::KeyDown{ keycode: Some(Keycode::Home), keymod, repeat: false, ..} => {
                    if keymod.intersects(Modifier::LSHIFTMOD|Modifier::RSHIFTMOD) {
                        if zx.tap_cabinet.select_first_tap() {
                            zx.print_current_tap();
                        }
                    }
                    else if zx.tap_cabinet.rewind_tap()? {
                            zx.print_current_tap_chunk(0);
                    }
                }
                Event::KeyDown{ keycode: Some(Keycode::PageUp), keymod, repeat: false, ..} => {
                    if keymod.intersects(Modifier::LSHIFTMOD|Modifier::RSHIFTMOD) {
                        if zx.tap_cabinet.select_prev_tap() {
                            zx.print_current_tap();
                        }
                    } else if let Some(no) = zx.tap_cabinet.prev_tap_chunk()? {
                        zx.print_current_tap_chunk(no.get());
                    }
                }
                Event::KeyDown{ keycode: Some(Keycode::PageDown), keymod, repeat: false, ..} => {
                    if keymod.intersects(Modifier::LSHIFTMOD|Modifier::RSHIFTMOD) {
                        if zx.tap_cabinet.select_next_tap() {
                            zx.print_current_tap();
                        }
                    }
                    else if let Some(no) = zx.tap_cabinet.next_tap_chunk()? {
                        zx.print_current_tap_chunk(no.get());
                    }
                }
                Event::KeyDown{ keycode: Some(Keycode::F5), repeat: false, ..} => {
                    if zx.tap_cabinet.toggle_play()? {
                        let index = zx.tap_cabinet.current_tap_index();
                        info!("TAP #{} {} {}", index,
                            zx.tap_cabinet.current_tap_file_name().unwrap(),
                            if zx.tap_cabinet.is_playing() {
                                "is playing"
                            }
                            else {
                                "stopped"
                            }
                        );
                    }
                }
                Event::KeyDown{ keycode: Some(Keycode::F6), repeat: false, ..} => {
                    zx.print_current_tap();
                    if let Some(iter) = zx.tap_cabinet.current_tap_info()? {
                        for (info, no) in iter.zip(1..) {
                            println!("  {}: {}", no, info?);
                        }
                        zx.tap_cabinet.rewind_tap()?;
                    }
                    else if zx.tap_cabinet.is_playing() {
                        warn!("Can't show tap information while the tape is playing...");
                    }
                    else if zx.tap_cabinet.is_recording() {
                        warn!("Can't show tap information while the tape is recording...");
                    }
                }
                Event::KeyDown{ keycode: Some(Keycode::F7), repeat: false, ..} => {
                    let index = zx.tap_cabinet.current_tap_index();
                    for (i, file) in zx.tap_cabinet.iter_meta().enumerate() {
                        println!("TAP {}: {}", 
                            if index == i { "->" } else { "  " },
                            file.to_string_lossy());
                    }
                }
                Event::KeyDown{ keycode: Some(Keycode::F8), repeat: false, ..} => {
                    zx.print_current_tap();
                    if zx.tap_cabinet.toggle_record()? {
                        info!("{}", if zx.tap_cabinet.is_recording() {
                            "Recording started. Save something now."
                        }
                        else {
                            "Recording ended."
                        });
                    }
                }
                Event::KeyDown{ keycode: Some(Keycode::F9), repeat: false, ..} => {
                    zx.audible_tap = !zx.audible_tap;
                    info!("TAP sound: {}", if zx.audible_tap { "audible" } else { "silent" });
                }
                Event::KeyDown{ keycode: Some(Keycode::F12), repeat: false, ..} => {
                    border_size = u8::from(border_size).wrapping_sub(1).try_into().unwrap_or(BorderSize::Full);
                    let (w, h) = ZXSpectrum::<C, U>::screen_size(border_size);
                    canvas.window_mut().set_size(w*2, h*2)?;
                    texture = create_texture(w, h)?;
                }
                Event::KeyDown{ keycode: Some(Keycode::ScrollLock), repeat: false, ..} => {
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
                Event::KeyDown{ keycode: Some(keycode), keymod, repeat: false, ..} => {
                    if !zx.cursor_to_joystick(keycode, true) {
                        zx.update_keypress(keycode, keymod, true);
                    }
                }
                Event::KeyUp{ keycode: Some(keycode), keymod, repeat: false, ..} => {
                    if !zx.cursor_to_joystick(keycode, false) {
                        zx.update_keypress(keycode, keymod, false);
                    }
                }
                Event::DropFile { filename, .. } => {
                    info!("Dropped file: {} {}", zx.handle_file(&filename)?, filename);
                }
                _ => {
                    // continue 'mainloop
                }
            }
        }

        if let EmulatorStatus::Paused = status {
            zx.synchronize_thread_to_frame();
            continue
        }

        let viewport = canvas.window().drawable_size();
        zx.send_mouse_move(border_size, viewport);

        // render pixels
        texture.with_lock(None, |buffer: &mut [u8], pitch: usize| {
            // measure_performance!("Video frame at: {:.4} Hz frames: {} wall: {} s"; 1.0f64, timer_subsystem, counter_elapsed, counter_iters, counter_sum; {
                zx.render_pixels::<PixelBufRGB24>(buffer, pitch, border_size);
            // 1.0});
        })?;

        canvas.copy(&texture, None, None)?;
        canvas.present();

        {
            let pr = zx.gfx_printer_ref().filter(|sp| sp.is_spooling())
                                         .map(|sp| sp.data_lines_buffered());
            if printing != pr {
                printing = pr;
                canvas.window_mut().set_title(&window_title(&zx))?;
            }
        }

        match status {
            EmulatorStatus::Turbo => {
                measure_performance!("CPU: {:.4} MHz T-states: {} wall: {} s"; 1e6f64, timer_subsystem, counter_elapsed, counter_iters, counter_sum; {
                    zx.run_frames_accelerated()
                })
                ;
            }
            EmulatorStatus::Normal => {
                zx.run_single_frame();
                // measure_performance!("Audio frame at: {:.4} Hz frames: {} wall: {} s"; 1.0f64, timer_subsystem, counter_elapsed, counter_iters, counter_sum; {
                zx.render_audio()?;
                // 1.0});
                if zx.synchronize_thread_to_frame() {
                    // canvas.copy(&texture, None, None)?;
                    // let mut y = 0;
                    // while y < ch {
                    //     let mut x = 0;
                    //     while x < cw {
                    //         canvas.copy(&texture, None, Some(Rect::new(x as i32, y as i32, cw, ch)))?;
                    //         x += SCREEN_WIDTH;
                    //     }
                    //     y += SCREEN_HEIGHT;
                    // }
                    //
                    // measure_performance!("present frame at: {:.4} Hz frames: {} wall: {} s"; 1.0f64, timer_subsystem, counter_elapsed, counter_iters, counter_sum; {
                    // canvas.present();
                    // 1.0});
                }
            }
            _ => {}
        }
        // let now = timer_subsystem.performance_counter();
        // let elapsed: f64 = (now - start) as f64 / timer_subsystem.performance_frequency() as f64;
        // eprintln!("{} {}", 1.0 / elapsed, timer_subsystem.performance_frequency());
        // start = now;
    }

    Ok(())
}
