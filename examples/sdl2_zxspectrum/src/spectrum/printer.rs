use std::io::{self, Write};

use image::{ImageBuffer, Pixel, Luma};

use zxspecemu::bus::zxprinter::{PIXEL_LINE_WIDTH, BYTES_PER_LINE, Spooler};

#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

type PixelT = Luma<u8>;

/// A simple printer spooler trait that produces PNG or SVG images.
pub trait ZxGfxPrinter {
    // type SerialDataIter: Iterator<Item=u8> + 'a;
    fn is_spooling(&self) -> bool;
    fn data_lines_buffered(&self) -> usize;
    // fn raw_lines_serial_data_iter(&'a mut self) -> Self::SerialDataIter;
    fn svg_dot_printed_lines(&self, wr: &mut dyn Write) -> io::Result<()>;
    fn to_image(&mut self) -> Option<ImageBuffer<PixelT, Vec<u8>>> { None }
    fn clear(&mut self);
    fn is_empty(&self) -> bool {
        self.data_lines_buffered() == 0
    }
}

/// A simple ZXPrinter spooler that produces PNG or SVG images.
#[derive(Clone, Default, Debug)]
pub struct ImageSpooler {
    spooling: bool,
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
        debug!("printed line: {}", self.buf.len() / BYTES_PER_LINE as usize);
    }

    fn motor_off(&mut self) {
        self.spooling = false;
        debug!("end of printing");
    }
}

impl ZxGfxPrinter for ImageSpooler {
    fn is_spooling(&self) -> bool {
        self.spooling
    }

    fn data_lines_buffered(&self) -> usize {
        (self.buf.len() + BYTES_PER_LINE as usize - 1) / BYTES_PER_LINE as usize
    }

    fn svg_dot_printed_lines(&self, wr: &mut dyn Write) -> io::Result<()> {
        let lines = self.data_lines_buffered();
        write!(wr, r##"<?xml version="1.0" standalone="no"?>
<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" 
  "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd">
<svg width="110mm" height="{mm_height:.2}mm" version="1.1"
     viewBox="0 0 {pixel_width} {pixel_height}" preserveAspectRatio="none"
     xmlns="http://www.w3.org/2000/svg">
  <desc>rust-zxspecemu example</desc>
  <g stroke-width="0.2" stroke="#b8b" fill="#646">
"##,
            mm_height=0.4*lines as f32,
            pixel_width=PIXEL_LINE_WIDTH,
            pixel_height = lines)?;
        for (index, mut dots) in self.buf.iter().copied().enumerate() {
            let x = (index % BYTES_PER_LINE as usize) * 8;
            let y = index / BYTES_PER_LINE as usize;
            for i in 0..8 {
                dots = dots.rotate_left(1);
                if dots & 1 == 1 {
                    write!(wr,
                        r##"<circle cx="{}" cy="{}" r="0.5"/>"##,
                        x + i, y)?;
                }
            }
        }
        wr.write_all(b"</g></svg>")
    }

    // fn raw_lines_serial_data_iter(&'a mut self) -> Self::SerialDataIter {
    //     core::iter::empty()
    // }

    fn to_image(&mut self) -> Option<ImageBuffer<PixelT, Vec<u8>>> {
        // calculate height from the buffer size
        let height = self.data_lines_buffered();
        if height == 0 {
            return None;
        }
        assert_eq!(PixelT::CHANNEL_COUNT, 1);
        let mut imgbuf = Vec::with_capacity(PIXEL_LINE_WIDTH as usize*height);
        imgbuf.extend(self.buf.iter().copied().flat_map(|mut bits| {
            (0..8).map(move |_| {
                bits = bits.rotate_left(1);
                if bits & 1 == 1 { 0 } else { !0 }
            })
        }));
        ImageBuffer::from_vec(PIXEL_LINE_WIDTH, height as u32, imgbuf)
        // let name = format!("printer_out_{}.png", Utc::now().format("%Y-%m-%d_%H%M%S%.f"));
        // image::save_buffer(&name, &self.buf, PIXEL_LINE_WIDTH, height as , ColorType::L8)?;
    }

    fn clear(&mut self) {
        self.buf.clear();
    }
}

/* ////////////////// SerialPrinterGfxGrabber ////////////////// */

/// A simple serial printer graphics line grabber that produces PNG or SVG images.
#[derive(Clone, Default, Debug)]
pub struct SerialPrinterGfxGrabber {
    state: GrabberState,
    eo_line: usize,
    buf: Vec<u8>
}

const ESC_CODE: u8 = 0x1B;
const EOL_CODE: u8 = 0x0A;
const DATA_LINE_WIDTH: usize = 3*256;
const GRAPHIC_LINE_START: &[u8;6] = &[ESC_CODE, 0x31, 0x1B, 0x4C, 0x00, 0x03];
const GRAPHIC_ENDS: &[u8;2] = &[ESC_CODE, 0x32];

#[derive(Clone, Copy, Debug, PartialEq)]
enum GrabberState {
    NoMatch,
    LineStart(usize),
    LineData(usize),
}

impl Default for GrabberState {
    fn default() -> Self {
        GrabberState::NoMatch
    }
}

impl SerialPrinterGfxGrabber {
    pub fn grab_or_passthrough<'a, 'b: 'a>(&'a mut self, ch: &'b u8) -> Option<&'a [u8]> {
        match self.state {
            GrabberState::NoMatch if *ch == ESC_CODE => {
                if self.eo_line < self.buf.len() {
                    self.buf.truncate(self.eo_line);
                }
                self.buf.push(*ch);
                self.state = GrabberState::LineStart(1);
                None
            }
            GrabberState::NoMatch => Some(core::slice::from_ref(ch)),
            GrabberState::LineStart(mut index) => {
                self.buf.push(*ch);
                if ch == &GRAPHIC_LINE_START[index] {
                    index +=1;
                    if index == GRAPHIC_LINE_START.len() {
                        self.buf.truncate(self.eo_line);
                        self.state = GrabberState::LineData(0);
                    }
                    else {
                        self.state = GrabberState::LineStart(index);
                    }
                    None
                }
                else if index == 1 && ch == &GRAPHIC_ENDS[index] {
                    self.buf.truncate(self.eo_line);
                    self.state = GrabberState::NoMatch;
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
                    if *ch == EOL_CODE {
                        self.eo_line = self.buf.len();
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

impl ZxGfxPrinter for SerialPrinterGfxGrabber {
    // type SerialDataIter = Box<dyn Iterator<Item=u8> + 'a>;
    fn is_spooling(&self) -> bool {
        self.state != GrabberState::NoMatch
    }

    fn data_lines_buffered(&self) -> usize {
        self.eo_line / DATA_LINE_WIDTH
    }

    fn svg_dot_printed_lines(&self, wr: &mut dyn Write) -> io::Result<()> {
        let data_lines = self.data_lines_buffered();
        write!(wr, r##"<?xml version="1.0" standalone="no"?>
<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" 
  "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd">
<svg width="210mm" height="{mm_height:.2}mm" version="1.1"
     viewBox="0 0 {pixel_width} {pixel_height}" preserveAspectRatio="none"
     xmlns="http://www.w3.org/2000/svg">
  <desc>rust-zxspecemu example</desc>
  <g stroke-width="0.25" stroke="#333" fill="#000">
"##,
            mm_height=2.47*data_lines as f32,
            pixel_width=DATA_LINE_WIDTH,
            pixel_height = data_lines * 8)?;
        for (index, mut dots) in self.buf[..self.eo_line].iter().step_by(3).copied().enumerate() {
            let x = index % (DATA_LINE_WIDTH/3) * 3;
            let y = index / (DATA_LINE_WIDTH/3) * 8;
            for i in (0..8).step_by(2) {
                dots = dots.rotate_left(2);
                if dots & 1 == 1 {
                    write!(wr, r##"<circle cx="{}" cy="{}" r="1"/>"##,
                            x, y + i)?;
                }
            }
        }
        wr.write_all(b"</g></svg>")
    }

    // fn raw_lines_serial_data_iter(&'a mut self) -> Self::SerialDataIter {
    //     let graphic_line_start_size: usize = GRAPHIC_LINE_START.len();
    //     let graphic_ends_size: usize = GRAPHIC_ENDS.len();
    //     let raw_line_width: usize = graphic_line_start_size + DATA_LINE_WIDTH;
    //     let lines = self.data_lines_buffered();
    //     let lines_over_index = lines * raw_line_width;
    //     let mut iter = self.buf[..self.eo_line].iter().copied();
    //     Box::new((0..lines_over_index + graphic_ends_size).map(move |index| {
    //         let col_index = index % raw_line_width;
    //         if index >= lines_over_index {
    //             GRAPHIC_ENDS[index - lines_over_index]
    //         }
    //         else if col_index < graphic_line_start_size {
    //             GRAPHIC_LINE_START[col_index]
    //         }
    //         else {
    //             iter.next().unwrap()
    //         }
    //     }))
    // }

    fn clear(&mut self) {
        self.buf.drain(..self.eo_line);
        self.eo_line = 0;
    }
}
