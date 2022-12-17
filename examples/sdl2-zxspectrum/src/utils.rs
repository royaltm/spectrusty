/*
    sdl2-zxspectrum: ZX Spectrum emulator example as a SDL2 application.
    Copyright (C) 2020-2022  Rafal Michalski

    For the full copyright notice, see the main.rs file.
*/
#![allow(unused_macros)]
#![allow(unused_imports)]
use std::{borrow::Cow, error::Error, ptr};

use log::{error, warn, info, debug, trace};

use sdl2::{VideoSubsystem,
            // video::Window,
            pixels::PixelFormatEnum,
            // rect::Rect,
            render::WindowCanvas,
            messagebox::{ show_simple_message_box, MessageBoxFlag }};

pub fn alert_window(text: Cow<str>) {
    show_simple_message_box(MessageBoxFlag::ERROR, "ZX Spectrum", &text, None)
    .unwrap_or_else(|_| eprintln!("{}", text));
}

pub fn info_window(head: &str, text: Cow<str>) {
    show_simple_message_box(MessageBoxFlag::INFORMATION, head, &text, None)
    .unwrap_or_else(|_| println!("{}", text));
}

#[cfg(not(windows))]
pub fn set_dpi_awareness() -> Result<(), String> { Ok(()) }

#[cfg(windows)]
pub fn set_dpi_awareness() -> Result<(), String> {
    use winapi::{shared::winerror::{E_INVALIDARG, S_OK},
                 um::shellscalingapi::{GetProcessDpiAwareness, SetProcessDpiAwareness, PROCESS_DPI_UNAWARE,
                                       PROCESS_PER_MONITOR_DPI_AWARE}};

    match unsafe { SetProcessDpiAwareness(PROCESS_PER_MONITOR_DPI_AWARE) } {
        S_OK => Ok(()),
        E_INVALIDARG => Err("Could not set DPI awareness.".into()),
        _ => {
            let mut awareness = PROCESS_DPI_UNAWARE;
            match unsafe { GetProcessDpiAwareness(ptr::null_mut(), &mut awareness) } {
                S_OK if awareness == PROCESS_PER_MONITOR_DPI_AWARE => Ok(()),
                _ => Err("Please disable DPI awareness override in program properties.".into()),
            }
        },
    }
}

pub fn err_str<E: Error>(e: E) -> String { e.to_string() }

pub fn create_image_canvas_window(video_sdl: &VideoSubsystem, image_data: &[u8]) -> Result<WindowCanvas, Box<dyn Error>> {
    let img = image::load_from_memory_with_format(image_data,
                                                  image::ImageFormat::Jpeg)?
              .into_rgb8();
    let (width, height) = img.dimensions();
    let window = video_sdl.window("ZX Spectrum", width, height)
                            .borderless()
                            // .hidden()
                            .build()
                            .map_err(err_str)?;
    
    let mut canvas = window.into_canvas().build()?;
    let texture_creator = canvas.texture_creator();
    let mut texture = texture_creator.create_texture_static(PixelFormatEnum::RGB24, width, height)?;
    // println!("{:?}", texture.query());
    let samples = img.as_flat_samples();
    let pitch = samples.layout.height_stride;
    texture.update(None,
                   samples.as_view::<image::Rgb<u8>>().unwrap().image_slice(),
                   pitch)?;
    canvas.copy(&texture, None, None)?;
    canvas.present();
    Ok(canvas)
}

macro_rules! now_timestamp_format {
 () => {
    ::chrono::Utc::now().format("%Y-%m-%d_%H%M%S%.f")
 };
}

macro_rules! measure_performance {
    ($label:expr; $time_unit:expr, $timer_subsystem:expr, $counter_elapsed:ident, $counter_iters:ident, $units_sum:ident; $run:expr) => {
        {
            let start = $timer_subsystem.performance_counter();
            $counter_iters += 1;
            $units_sum += $run as f64;
            $counter_elapsed += $timer_subsystem.performance_counter() - start;
            if $counter_iters % 50 == 0 {
                let elapsed: f64 = $counter_elapsed as f64 / $timer_subsystem.performance_frequency() as f64;
                let hz = (elapsed * $time_unit / $units_sum as f64).recip();
                eprintln!($label, hz, $units_sum, elapsed);
            }
        }
    };
}
