use core::fmt::Debug;
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};
use std::io::Write;

use crate::video::VideoFrame;
use crate::clock::*;
// ; bit 7 set - activate stylus.
// ; bit 7 low - deactivate stylus.
// ; bit 2 set - stops printer.
// ; bit 2 reset - starts printer
// ; bit 1 set - slows printer.
// ; bit 1 reset - normal speed.
// (D6) Will be read as low if the printer is there, high if it is not, and is used solely to check if the printer is connected.
// (D0) This is high when the printer is ready for the next bit.
// (D7) This line is high for the start of a new line.
// 2 lines / sec
pub const PIXEL_LINE_WIDTH: u32 = 256;
pub const BYTES_PER_LINE: u32 = 32;

const STATUS_OFF: u8       = 0b0011_1110;
const STATUS_NEW_LINE: u8  = 0b1011_1111;
const STATUS_BIT_READY: u8 = 0b0011_1111;
const STATUS_NOT_READY: u8 = 0b0011_1110;

const STYLUS_MASK: u8 = 0b1000_0000;
const MOTOR_MASK: u8  = 0b0000_0100;
const SLOW_MASK: u8   = 0b0000_0010;

const BIT_DELAY: FTs = 400;//6835;
const LINE_DELAY: FTs = BIT_DELAY*8;

pub trait Spooler: Debug {
    fn motor_on(&mut self) {}
    fn motor_off(&mut self) {}
    fn push_line(&mut self, line: &[u8]);
}

#[derive(Clone, Copy, Default, Debug)]
pub struct DebugSpooler;

#[derive(Clone, Default, Debug)]
pub struct ZxPrinterDevice<V, S> {
    pub spooler: S,
    motor: bool,
    ready: bool,
    cursor: u8,
    last_ts: VideoTs,
    line: [u8;32],
    _video_frame: PhantomData<V>
}

impl<V, S> Deref for ZxPrinterDevice<V, S> {
    type Target = S;
    fn deref(&self) -> &Self::Target {
        &self.spooler
    }
}

impl<V, S> DerefMut for ZxPrinterDevice<V, S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.spooler
    }
}

impl<V: VideoFrame, S: Spooler> ZxPrinterDevice<V, S> {
    pub fn next_frame(&mut self, _end_ts: VideoTs) {
        self.last_ts = V::vts_saturating_sub_frame(self.last_ts);
    }

    pub fn reset(&mut self) {
        self.motor = false;
        self.ready = false;
        for p in self.line.iter_mut() {
            *p = 0;
        }
        self.cursor = 0;
    }

    pub fn write_control(&mut self, data: u8, timestamp: VideoTs) {
        if data & MOTOR_MASK == 0 { // start 
            self.start(data, timestamp);
        }
        else {
            self.stop();
        }
    }

    pub fn read_status(&mut self, timestamp: VideoTs) -> u8 {
        if self.motor {
            if self.ready || self.update_ready(timestamp) {
                if self.cursor == 0 {
                    STATUS_NEW_LINE
                }
                else {
                    STATUS_BIT_READY
                }
            }
            else {
                STATUS_NOT_READY
            }
        }
        else {
            STATUS_OFF
        }
    }

    fn update_ready(&mut self, timestamp: VideoTs) -> bool {
        if timestamp > self.last_ts {
            self.ready = true;
            return true;
        }
        false
    }

    fn flush(&mut self) {
        let cursor = self.cursor;
        self.cursor = 0;
        let mask = 0x80 >> (cursor.wrapping_sub(1) & 7);
        self.line[usize::from(cursor) >> 3] &= mask;
        let index = (usize::from(cursor) + 7) >> 3;
        self.spooler.push_line(&self.line[..index]);
    }

    fn stop(&mut self) {
        if self.motor {
            self.motor = false;
            self.ready = false;
            if self.cursor != 0 {
                self.flush()
            }
            self.spooler.motor_off();
        }
    }

    fn start(&mut self, data: u8, timestamp: VideoTs) {
        if self.motor {
            if self.ready {
                let delay = if self.write_bit(data & STYLUS_MASK == STYLUS_MASK) {
                    LINE_DELAY
                }
                else {
                    BIT_DELAY
                };
                let ts = V::vts_to_tstates(timestamp) +
                         delay * if data & SLOW_MASK == SLOW_MASK { 2 } else { 1 };
                self.last_ts = V::tstates_to_vts(ts);
                self.ready = false;
            }
        }
        else {
            self.motor = true;
            self.ready = false;
            self.cursor = 0;
            let ts = V::vts_to_tstates(timestamp) +
                     LINE_DELAY * if data & SLOW_MASK == SLOW_MASK { 2 } else { 1 };
            self.last_ts = V::tstates_to_vts(ts);
            self.spooler.motor_on();
        }
    }

    fn write_bit(&mut self, stylus: bool) -> bool {
        let cursor = self.cursor;
        let index = usize::from(cursor) >> 3;
        let mask = 0x80 >> (cursor & 7);
        if stylus {
            self.line[index] |= mask;
        }
        else {
            self.line[index] &= !mask;
        }
        let (cursor, over) = cursor.overflowing_add(1);
        self.cursor = cursor;
        if over {
            self.spooler.push_line(&self.line);
        }
        over
    }
}

impl Spooler for DebugSpooler {
    fn push_line(&mut self, line: &[u8]) {
        for b in line.iter().copied() {
            print!("{:02x}", b);
        }
        println!();
    }
}
