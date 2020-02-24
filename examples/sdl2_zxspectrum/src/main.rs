// #![windows_subsystem = "windows"] // it is "console" by default
// #![allow(unused_assignments)]
// #![allow(unused_imports)]
#![allow(dead_code)]
// #![allow(unused_variables)]
// #![allow(unused_mut)]

#[macro_use]
mod utils;
mod audio;
mod spectrum;
mod printer;
mod peripherals;

use std::collections::HashSet;

#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

#[allow(unused_imports)]
use sdl2::{Sdl, 
           event::{Event, WindowEvent},
           keyboard::{Keycode, Mod as Modifier},
           pixels::PixelFormatEnum,
           rect::Rect,
           video::{FullscreenType, Window, WindowContext}};
// use sdl2_sys::SDL_WindowFlags;
use crate::spectrum::*;
use crate::peripherals::{SpoolerAccess, MouseAccess, JoystickAccess, DynBusAccess};
use crate::utils::*;

const REQUESTED_AUDIO_LATENCY: usize = 2;


const HEAD: &str = r#"The SDL2 desktop example emulator for "rust-zxspecemu""#;
const HELP: &str = r###"
Drag & drop TAP, SNA or SCR files over the emulator window in order to load them.

F1: Show this help.
F2: Turbo - runs as fast as possible, while key is being pressed.
F3: Save ZX Printer spooled image as a PNG file. [+printer opt. only]
F4: Change joystick implementation (cursor keys).
F5: Play current TAP file.
F6: Print current TAP file info (only if not playing or recording).
F7: List tap file names.
F8: Record TAP chunks appending them to the current TAP file.
F9: Turn On/Off tape audio.
F10: Reset Zx Spectrum.
F11: Trigger non-maskable interrupt.
Pause: Pauses/resumes emulation.
Insert: Create a new TAP file for recording.
Delete: Remove a current TAP from the emulator.
Home/End: Move TAP cursor to the beginning/end of current TAP file.
PgUp/PgDown: Move TAP cursor to the previous/next block of current TAP file.
Home/End+SHIFT: Select a first/last TAP file.
PgUp/PgDown+SHIFT: Select a previous/next TAP file.
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

    let (mut cfg, files): (HashSet<_>,HashSet<_>) = std::env::args().skip(1).partition(|arg| arg.starts_with("+"));

    if cfg.is_empty() {
        let zx = ZXSpectrum48::create(&sdl_context, REQUESTED_AUDIO_LATENCY)?;
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

fn run<C, M, B, I>(mut zx: ZXSpectrum<C, M, B>, sdl_context: Sdl, files: I) -> Result<(), Box<dyn std::error::Error>>
    where I: IntoIterator<Item=String>,
          M: ZxMemory + Default,
          B: BusDevice<Timestamp=VideoTs>,
          C: Cpu + std::fmt::Debug,
          Ula<M, B>: Default + AyAudioFrame<ZXBlep>,
          ZXSpectrum<C, M, B>: SpoolerAccess + MouseAccess + JoystickAccess + DynBusAccess
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

    let border_size = BorderSize::Full;
    let window: Window;
    let (screen_width, screen_height) = ZXSpectrum::<C, M, B>::screen_size(border_size);
    info!("{:?} {}x{}", border_size, screen_width, screen_height);

    window = video_subsystem.window("ZX Spectrum Sdl2", screen_width*2, screen_height*2)
                            // .resizable()
                            .allow_highdpi()
                            .position_centered()
                            .build()
                            .map_err(err_str)?;

    sdl_context.mouse().show_cursor(false);

    let timer_subsystem = sdl_context.timer()?;

    let mut canvas = window.into_canvas()
                    .accelerated()
                    // .present_vsync()
                    .build()
                    .map_err(err_str)?;

    let texture_creator = canvas.texture_creator();

    let mut texture = texture_creator
                      .create_texture_streaming(PixelFormatEnum::RGB24, screen_width, screen_height)
                      .map_err(err_str)?;

    let mut event_pump = sdl_context.event_pump()?;

    let mut counter_elapsed = 0u64;
    let mut counter_iters = 0usize;
    let mut counter_sum = 0.0;
    let mut file_count = 0;
    let mut status = EmulatorStatus::Normal;

    'mainloop: loop {
        for event in event_pump.poll_iter() {
            match event {
                Event::Window { win_event: WindowEvent::Close, .. } | Event::Quit { .. } => break 'mainloop,
                Event::MouseMotion{ xrel, yrel, .. } => {
                    // println!("{:?} {:?} {}x{} | {}x{}", which, mousestate, x, y, xrel, yrel);
                    // let size = canvas.window().size();
                    let viewport = canvas.window().drawable_size();
                    // println!("{:?} {:?}", viewport, size);
                    zx.update_mouse_position(xrel, yrel, border_size, viewport);
                }
                Event::MouseButtonDown { mouse_btn, .. } => {
                    zx.update_mouse_button(mouse_btn, true);
                }
                Event::MouseButtonUp { mouse_btn, .. } => {
                    zx.update_mouse_button(mouse_btn, false);
                }
                Event::KeyDown{ keycode: Some(Keycode::F1), repeat: false, ..} => {
                    zx.audio.pause();
                    info(format!("{}\nHardware configuration:\n{}\n{}", HEAD, zx.device_info(), HELP).into());
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
                    if let Some(spooler) = zx.spooler_mut() {
                        if !spooler.is_spooling() {
                            if let Some(name) = spooler.save_image()? {
                                info!("Printer output saved as: {}", name);
                                spooler.clear();
                            }
                            else {
                                info!("Printer spooler is empty.");
                            }
                        }
                        else {
                            info!("Printer is busy, can't save at the moment.");
                        }
                    }
                    else {
                        info!("There is no printer.");
                    }
                }
                Event::KeyUp{ keycode: Some(Keycode::F4), repeat: false, ..} => {
                    zx.next_joystick();
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
                    continue 'mainloop
                }
            }
        }

        // render pixels
        texture.with_lock(None, |buffer: &mut [u8], pitch: usize| {
            // measure_performance!("Video frame at: {:.4} Hz frames: {} wall: {} s"; 1.0f64, timer_subsystem, counter_elapsed, counter_iters, counter_sum; {
                zx.render_pixels::<PixelBufRGB24>(buffer, pitch, border_size);
            // 1.0});
        })?;
        canvas.copy(&texture, None, None)?;
        canvas.present();
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
