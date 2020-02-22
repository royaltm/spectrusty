use chrono::prelude::*;
use image;
use zxspecemu::bus::zxprinter::{PIXEL_LINE_WIDTH, Spooler};

#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

/// A simple ZX Printer spooler that saves spooled data as PNG files.
#[derive(Clone, Default)]
pub struct ImageSpooler {
    spooling: bool,
    buf: Vec<u8>
}

impl ImageSpooler {
    pub fn is_spooling(&self) -> bool {
        self.spooling
    }

    pub fn save_image(&mut self) -> Result<Option<String>, image::error::ImageError> {
        // calculate height from the buffer size
        let height: u32 = (self.buf.len() as u32 + PIXEL_LINE_WIDTH - 1) / PIXEL_LINE_WIDTH;
        if height == 0 {
            return Ok(None);
        }
        // make sure the last line has the same width
        self.buf.resize((PIXEL_LINE_WIDTH * height) as usize, u8::max_value());
        let name = format!("printer_out_{}.png", Utc::now().format("%Y-%m-%d_%H%M%S%.f"));
        image::save_buffer(&name, &self.buf, PIXEL_LINE_WIDTH, height, image::ColorType::L8)?;
        Ok(Some(name))
    }

    pub fn clear(&mut self) {
        self.buf.clear();
    }
}

impl Spooler for ImageSpooler {
    fn motor_on(&mut self) {
        self.spooling = true;
    }

    fn push_line(&mut self, line: &[u8]) {
        self.buf.reserve(PIXEL_LINE_WIDTH as usize);
        for mut b in line.iter().copied() {
            for _ in 0..8 {
                self.buf.push(if b & 0x80 == 0x80 {
                    u8::min_value()
                }
                else {
                    u8::max_value()
                });
                b = b.rotate_left(1);
            }
        }
        debug!("printed line: {}", self.buf.len() / PIXEL_LINE_WIDTH as usize);
    }

    fn motor_off(&mut self) {
        self.spooling = false;
        debug!("end of printing");
    }
}
