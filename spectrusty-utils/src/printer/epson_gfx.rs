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

use super::*;

/// A simple EPSON printer graphics data interceptor that can produce images via [DotMatrixGfx] trait.
///
/// Use its [EpsonPrinterGfx::intercept] method to filter data being sent to the printer.
///
/// # Note
/// This is a very simple implementation, which in reality is able to catch only the output of COPY and COPY EXP
/// commands found in Spectrum's ROMs: 128k, +2, and +3.
#[derive(Clone, Default, Debug)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "snapshot", serde(default))]
pub struct EpsonPrinterGfx {
    #[cfg_attr(feature = "snapshot", serde(skip))]
    state: GrabberState,
    #[cfg_attr(feature = "snapshot", serde(skip))]
    line_mm: f32,
    #[cfg_attr(feature = "snapshot", serde(skip))]
    eo_line: usize,
    #[cfg_attr(feature = "snapshot", serde(skip))]
    buf: Vec<u8>
}

// http://www.lprng.com/RESOURCES/EPSON/epson.htm
const ESC_CODE: u8 = 0x1B;
const CR_CODE: u8 = 0x0D;
const LF_CODE: u8 = 0x0A;
const DATA_LINE_WIDTH: usize = 3 * 256;
const GRAPHICS_LINE_START: &[u8;6] = &[ESC_CODE, b'1', ESC_CODE, b'L', 0x00, 0x03]; // with 7/72 inch line spacing
const GRAPHICS_EXP_START: &[u8;6] = &[CR_CODE, LF_CODE, ESC_CODE, b'L', 0x00, 0x03];
const GRAPHICS_SPACING: u8 = b'3';//, n]; // Select n/216 inch line spacing
const GRAPHICS_ENDS:  u8 = b'2';
const GRAPHICS_RESET: u8 = b'@';

const INCH_MM: f32 = 25.4;
const INCH7_BY_72_LINE_MM: f32 = 2.47;

#[derive(Clone, Copy, Debug, PartialEq)]
enum GrabberState {
    NoMatch,
    LineStart(usize),
    SelectSpacing,
    LineData(usize),
}

impl Default for GrabberState {
    fn default() -> Self {
        GrabberState::NoMatch
    }
}

impl EpsonPrinterGfx {
    /// Returns a reference to the byte sent to the printer or captures it if an escape code arrives.
    ///
    /// In this instance all subsequent writes are being buffered until one of two things happens:
    ///
    /// * Either a collected escape sequence matches the signature of graphic data being sent to the printer.
    ///   In this instance, the image is being spooled and will be available via [DotMatrixGfx] methods.
    /// * Or a collected escape sequence data does not match the signature. In this instance, a reference to
    ///   the buffered data is being returned so it can be passed back to the upstream.
    pub fn intercept<'a, 'b: 'a>(&'a mut self, ch: &'b u8) -> Option<&'a [u8]> {
        match self.state {
            GrabberState::NoMatch if *ch == ESC_CODE => {
                self.buf.truncate(self.eo_line);
                self.buf.push(*ch);
                self.state = GrabberState::LineStart(1);
                self.line_mm = INCH7_BY_72_LINE_MM;
                None
            }
            GrabberState::NoMatch => Some(core::slice::from_ref(ch)),
            GrabberState::SelectSpacing => {
                self.line_mm = *ch as f32 / 216.0 * INCH_MM;
                self.buf.truncate(self.eo_line);
                self.state = GrabberState::LineStart(0);
                None
            }
            GrabberState::LineStart(mut index) => {
                self.buf.push(*ch);
                if ch == &GRAPHICS_LINE_START[index] || ch == &GRAPHICS_EXP_START[index] {
                    index +=1;
                    if index == GRAPHICS_LINE_START.len() {
                        debug_assert_eq!(GRAPHICS_LINE_START.len(), GRAPHICS_EXP_START.len());
                        self.buf.truncate(self.eo_line);
                        self.state = GrabberState::LineData(0);
                    }
                    else {
                        self.state = GrabberState::LineStart(index);
                    }
                    None
                }
                else if index == 1 && (*ch == GRAPHICS_ENDS ||
                                       *ch == GRAPHICS_RESET ||
                                       *ch == GRAPHICS_SPACING) {
                    self.buf.truncate(self.eo_line);
                    self.state = if *ch == GRAPHICS_SPACING {
                        GrabberState::SelectSpacing
                    }
                    else {
                        GrabberState::NoMatch
                    };
                    None
                }
                else {
                    self.state = GrabberState::NoMatch;
                    Some(&self.buf[self.eo_line..])
                }
            }
            GrabberState::LineData(width) => {
                if width == DATA_LINE_WIDTH {
                    self.state = GrabberState::NoMatch;
                    if *ch == LF_CODE { // COPY line
                        self.eo_line = self.buf.len();
                        None
                    }
                    else if *ch == CR_CODE || *ch == ESC_CODE { // COPY EXP next line or end
                        self.eo_line = self.buf.len();
                        self.state = GrabberState::LineStart(1);
                        None
                    }
                    else { // the printing must have been interrupted
                           // since we don't know where did it stop exactly,
                           // just flush out buffered data
                        self.buf.push(*ch);
                        Some(&self.buf[self.eo_line..])
                    }
                }
                else {
                    self.buf.push(*ch);
                    self.state = GrabberState::LineData(width + 1);
                    None
                }
            }
        }
    }
}

impl DotMatrixGfx for EpsonPrinterGfx {
    fn is_spooling(&self) -> bool {
        self.state != GrabberState::NoMatch
    }

    fn lines_buffered(&self) -> usize {
        self.eo_line / DATA_LINE_WIDTH
    }

    fn clear(&mut self) {
        self.buf.drain(..self.eo_line);
        self.eo_line = 0;
    }

    fn write_svg_dot_gfx_lines(&self, description: &str, target: &mut dyn io::Write) -> io::Result<bool> {
        let lines = self.lines_buffered();
        if lines == 0 {
            return Ok(false)
        }
        write!(target, r##"<?xml version="1.0" standalone="no"?>
<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" 
  "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd">
<svg width="190mm" height="{height_mm:.2}mm" version="1.1"
     viewBox="0 0 {pixel_width} {pixel_height}" preserveAspectRatio="none"
     xmlns="http://www.w3.org/2000/svg">
  <desc>{description}</desc>
  <g stroke-width="0.125" stroke="#333" fill="#000">
"##,
            description=description,
            height_mm=self.line_mm * lines as f32,
            pixel_width=DATA_LINE_WIDTH,
            pixel_height = lines * 8)?;
        for (row, line) in self.buf[..self.eo_line].chunks(DATA_LINE_WIDTH).enumerate() {
            for (x, mut dots) in line.iter().copied().enumerate() {
                let y = row * 8;
                for i in 0..8 {
                    dots = dots.rotate_left(1);
                    if dots & 1 == 1 {
                        write!(target, r##"<circle cx="{}" cy="{}" r="0.5"/>"##,
                                x, y + i)?;
                    }
                }
            }
        }
        target.write_all(b"</g></svg>")?;
        Ok(true)
    }

    fn write_gfx_data(&mut self, _target: &mut Vec<u8>) -> Option<(u32, u32)> {
        // TODO: implement
        None
    }
}
