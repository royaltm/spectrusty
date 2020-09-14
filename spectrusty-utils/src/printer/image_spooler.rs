/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
use std::io;

#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};
#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

use spectrusty::peripherals::zxprinter::{DOTS_PER_LINE, BYTES_PER_LINE, Spooler};
use super::*;

/// A simple **ZXPrinter** spooler that produces images via [DotMatrixGfx] trait.
/// Use this type as a substitute for the `S` generic parameter of [ZxPrinterDevice] or [ZxPrinterBusDevice].
///
/// [ZxPrinterDevice]: spectrusty::peripherals::zxprinter::ZxPrinterDevice
/// [ZxPrinterBusDevice]: spectrusty::peripherals::bus::zxprinter::ZxPrinterBusDevice
#[derive(Clone, Default, Debug)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "snapshot", serde(default))]
pub struct ImageSpooler {
    spooling: bool,
    #[cfg_attr(feature = "snapshot", serde(skip))]
    buf: Vec<u8>
}

impl Spooler for ImageSpooler {
    fn motor_on(&mut self) {
        self.spooling = true;
    }

    fn push_line(&mut self, line: &[u8]) {
        assert!(line.len() <= BYTES_PER_LINE as usize);
        self.buf.extend_from_slice(line);
        self.buf.resize(self.buf.len() + BYTES_PER_LINE as usize - line.len(), 0);
        self.spooling = true;
        debug!("printed line: {}", self.buf.len() / BYTES_PER_LINE as usize);
    }

    fn motor_off(&mut self) {
        self.spooling = false;
        debug!("end of printing");
    }
}

impl DotMatrixGfx for ImageSpooler {
    fn is_spooling(&self) -> bool {
        self.spooling
    }

    fn lines_buffered(&self) -> usize {
        (self.buf.len() + BYTES_PER_LINE as usize - 1) / BYTES_PER_LINE as usize
    }

    fn clear(&mut self) {
        self.buf.clear();
    }

    fn write_svg_dot_gfx_lines(&self, description: &str, target: &mut dyn io::Write) -> io::Result<bool> {
        let lines = self.lines_buffered();
        if lines == 0 {
            return Ok(false)
        }
        write!(target, r##"<?xml version="1.0" standalone="no"?>
<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" 
  "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd">
<svg width="110mm" height="{height_mm:.2}mm" version="1.1"
     viewBox="0 0 {pixel_width} {pixel_height}" preserveAspectRatio="none"
     xmlns="http://www.w3.org/2000/svg">
  <desc>{description}</desc>
  <g stroke-width="0.2" stroke="#b8b" fill="#646">
"##,
            description=description,
            height_mm=0.4*lines as f32,
            pixel_width=DOTS_PER_LINE,
            pixel_height = lines)?;
        for (index, mut dots) in self.buf.iter().copied().enumerate() {
            let x = (index % BYTES_PER_LINE as usize) * 8;
            let y = index / BYTES_PER_LINE as usize;
            for i in 0..8 {
                dots = dots.rotate_left(1);
                if dots & 1 == 1 {
                    write!(target,
                        r##"<circle cx="{}" cy="{}" r="0.5"/>"##,
                        x + i, y)?;
                }
            }
        }
        target.write_all(b"</g></svg>")?;
        Ok(true)
    }

    fn write_gfx_data(&mut self, target: &mut Vec<u8>) -> Option<(u32, u32)> {
        let height = self.lines_buffered();
        if height == 0 {
            return None;
        }
        target.reserve_exact(DOTS_PER_LINE as usize * height);
        target.extend(self.buf.iter().copied().flat_map(|mut bits| {
            (0..8).map(move |_| {
                bits = bits.rotate_left(1);
                if bits & 1 == 1 { 0 } else { !0 }
            })
        }));
        Some((DOTS_PER_LINE, height as u32))
    }
}
