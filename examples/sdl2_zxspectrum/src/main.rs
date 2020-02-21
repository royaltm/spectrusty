// #![windows_subsystem = "windows"] // it is "console" by default
#![allow(unused_assignments)]
#![allow(unused_imports)]
#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_mut)]

#[macro_use]
mod utils;
mod audio;
mod spectrum;
mod printer;

use std::io::{Seek, Read};
use std::thread;
use std::{cmp::{max, min},
          rc::Rc,
          sync::Arc};
use rand::{thread_rng, Rng};

// use scoped_threadpool::Pool;
use sdl2::{event::{Event, WindowEvent},
           keyboard::{Keycode, Mod as Modifier},
           pixels::PixelFormatEnum,
           rect::Rect,
           video::{FullscreenType, Window, WindowContext}};
use sdl2_sys::SDL_WindowFlags;
use spectrum::*;
use crate::utils::*;
use zxspecemu::formats::{
    // read_ear::*,
    tap,
    sna
};

type ZXSpectrum = ZXSpectrum48Ay;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    set_dpi_awareness()?;
    let sdl_context = sdl2::init()?;
    let video_subsystem = sdl_context.video()?;
    // eprintln!("driver: {}", video_subsystem.current_video_driver());

    let border_size = BorderSize::Full;
    let window: Window;
    let (screen_width, screen_height) = ZXSpectrum::screen_size(border_size);
    println!("{:?} {}x{}", border_size, screen_width, screen_height);

    window = video_subsystem.window("ZX Spectrum Sdl2", screen_width*2, screen_height*2)
                            .resizable()
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

    let mut texture =
        texture_creator.create_texture_streaming(PixelFormatEnum::RGB24, screen_width, screen_height)
                       .map_err(err_str)?;

    let mut event_pump = sdl_context.event_pump()?;

    let mut zx = ZXSpectrum::create(&sdl_context)?;
    zx.reset();

    for filename in std::env::args().skip(1) {
        println!("File: {} {}", zx.handle_file(&filename)?, filename);
    }
    /*
        F10 - reset
        F11 - nmi
        F5 - play/pause current tape
        F6 - tap info
        F8 - record tape (appends chunks)/stops recording
        PgUp - previous tap chunk
        PgDn - next tap chunk
        Home - first tap chunk
        End - last tap chunk
        Shift + PgUp/PgDn/Home/End - tap files instead of tap chunks
        Insert - adds a new empty tap file at the end, and sets cursor to it
    */
    let mut counter_elapsed = 0u64;
    let mut counter_iters = 0usize;
    let mut counter_sum = 0.0;
    let mut file_count = 0;
    let mut turbo = false;
    'mainloop: loop {
        for event in event_pump.poll_iter() {
            match event {
                Event::Window { win_event: WindowEvent::Close, .. } | Event::Quit { .. } => break 'mainloop,
                Event::KeyDown{ keycode: Some(Keycode::F11), repeat: false, ..} => {
                    zx.trigger_nmi();
                }
                Event::KeyDown{ keycode: Some(Keycode::F10), repeat: false, ..} => {
                    zx.reset();
                }
                Event::KeyDown{ keycode: Some(Keycode::F2), repeat: false, ..} => {
                    if !turbo {
                        turbo = true;
                        zx.audio.pause();
                    }
                }
                Event::KeyUp{ keycode: Some(Keycode::F2), repeat: false, ..} => {
                    if turbo {
                        turbo = false;
                        zx.audio.resume();
                    }
                }
                Event::KeyDown{ keycode: Some(Keycode::F3), repeat: false, ..} => {
                    if let Some(spooler) = zx.spooler_mut() {
                        if !spooler.is_spooling() {
                            println!("saving printed image");
                            spooler.save_image();
                            spooler.clear();
                        }
                        else {
                            println!("still spooling, can't save");
                        }
                    }
                    else {
                        println!("no spoooler");
                    }
                }
                Event::KeyDown{ keycode: Some(Keycode::Insert), keymod, repeat: false, ..} => {
                    loop {
                        file_count += 1;
                        if file_count > 999 {
                            panic!("too much files!");
                        }
                        let name = format!("new_file{:04}.tap", file_count);
                        match zx.tap_cabinet.create_file(&name) {
                            Ok(()) => {
                                println!("Created TAP: {}", name);
                                break;
                            }
                            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
                            e @ Err(_) => e?,
                        }
                    }
                    zx.tap_cabinet.select_last_tap();
                    zx.print_current_tap();
                }
                Event::KeyDown{ keycode: Some(Keycode::Delete), keymod, repeat: false, ..} => {
                    if let Some((_, path)) = zx.tap_cabinet.remove_current_tap() {
                        println!("Removed: {}", path.to_string_lossy());
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
                        println!("{}", if zx.tap_cabinet.is_playing() {
                            "Playing"
                        }
                        else {
                            "Paused"
                        });
                    }
                }
                Event::KeyDown{ keycode: Some(Keycode::F6), repeat: false, ..} => {
                    zx.print_current_tap();
                    if let Some(iter) = zx.tap_cabinet.current_tap_info()? {
                        for (info, no) in iter.zip(1..) {
                            println!("#{} {}", no, info?);
                        }
                        zx.tap_cabinet.rewind_tap()?;
                    }
                    else if zx.tap_cabinet.is_playing() {
                        println!("Playing...");
                    }
                    else if zx.tap_cabinet.is_recording() {
                        println!("Recording...");
                    }
                }
                Event::KeyDown{ keycode: Some(Keycode::F8), repeat: false, ..} => {
                    if zx.tap_cabinet.toggle_record()? {
                        println!("{}", if zx.tap_cabinet.is_recording() {
                            "Recording"
                        }
                        else {
                            "Paused"
                        });
                    }
                }
                Event::KeyDown{ keycode: Some(Keycode::F9), repeat: false, ..} => {
                    zx.audible_tap = !zx.audible_tap;
                    println!("TAP sound: {}", if zx.audible_tap { "audible" } else { "silent" });
                }
                Event::KeyDown{ keycode: Some(keycode), keymod, repeat: false, ..} => {
                    // eprintln!("{:?}", event);
                    zx.update_keypress(keycode, keymod, true);
                }
                Event::KeyUp{ keycode: Some(keycode), keymod, repeat: false, ..} => {
                    // eprintln!("{:?}", event);
                    zx.update_keypress(keycode, keymod, false);
                }
                Event::DropFile { filename, .. } => {
                    println!("File: {} {}", zx.handle_file(&filename)?, filename);
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
        if turbo {
            measure_performance!("CPU: {:.4} MHz T-states: {} wall: {} s"; 1e6f64, timer_subsystem, counter_elapsed, counter_iters, counter_sum; {
                zx.run_frames_accelerated()
            })
            ;            
        }
        else {
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
        // let now = timer_subsystem.performance_counter();
        // let elapsed: f64 = (now - start) as f64 / timer_subsystem.performance_frequency() as f64;
        // eprintln!("{} {}", 1.0 / elapsed, timer_subsystem.performance_frequency());
        // start = now;
    }

    Ok(())
}
