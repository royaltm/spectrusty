//! An emulator of the family of printers: **ZX Printer** / **Alphacom 32** / **Timex TS2040**.
//!
//! **ZX Printer** is an extremely compact, 32 column printer which uses an aluminium coated paper.
//! The printed image is being burned onto the surface of the paper by two metal pins which travel
//! across the paper. A voltage is passed through these pins which causes a spark to be produced, leaving
//! a black-ish (or blue-ish) dot.
//!
//! The Alphacom 32 is compatible with software for the ZX Printer, but uses thermal paper instead
//! of the ZX Printer's metallised paper.
//!
//! ZX Printer used special 4" (100 mm) wide black paper which was supplied coated with a thin layer of aluminium.
//!
//! Alphacom 32 printer's thermographic paper has 4.33" (110 mm) x 1.9" (48 mm) diameter.
//! 
//! TS2040 was a branded version of Alphacom 32 distributed by Timex to the US.
//!
//! Also see: [Printers](https://www.worldofspectrum.org/faq/reference/peripherals.htm#Printers).
use core::fmt::Debug;
use core::ops::{Deref, DerefMut};

#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};

use spectrusty_core::clock::*;

/// A number of printed dots in each line.
pub const DOTS_PER_LINE: u32 = 256;
/// A maximum number of bytes in each line.
pub const BYTES_PER_LINE: u32 = 32;

/// An interface to the **ZX Printer** spooler.
///
/// An implementation of this trait must be provided in order to complete the emulation of [ZxPrinterDevice].
///
/// Both [Debug] and [Default] traits also need to be implemented.
pub trait Spooler: Debug + Default {
    /// Can be implemented to get the notification that the printing will begin shortly.
    fn motor_on(&mut self) {}
    /// Can be implemented to get the notification that the printing has ended.
    fn motor_off(&mut self) {}
    /// After each printed line this method is being called with a reference to a slice containing 
    /// an information about up to 256 printed dots stored in up to 32 bytes.
    ///
    /// 8 dots are encoded from the most significant to the least significant bit in each byte.
    /// So the leftmost dot is encoded in the bit 7 of the first byte in a given slice and the
    /// rightmost dot is encoded in the bit 0 of the last byte.
    ///
    /// A value of 1 in each bit signifies a presence of a dot, a 0 means there is no dot.
    ///
    /// A received slice will never be larger than 32 bytes. If a user presses a `BREAK` key during
    /// `COPY`, the slice may be smaller than 32 bytes but it will never be empty.
    fn push_line(&mut self, line: &[u8]);
}

/// A simple **ZX Printer** spooler that outputs each line to the stdout as hexadecimal digits.
#[derive(Clone, Copy, Default, Debug)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
pub struct DebugSpooler;

/// This type implements a communication protocol of the **ZX Printer**.
///
/// A data port is being used with the following bits for reading:
///
/// * bit 6 is 0 if the printer is there, 1 if it is not and is used solely to check if the printer
///   is connected.
/// * bit 0 is 1 if the printer is ready for the next bit or 0 if it's busy printing.
/// * bit 7 is 1 if the printer is ready for the new line or 0 if it's not there yet.
///
/// and for writing:
///
/// * bit 7 - a value of 1 activates a stylus, a dot will be printed in this instance.
///   When a 0 is written, the stylus is being dactivated.
/// * bit 2 - a value of 1 stops the printer motor and when a 0 is written it starts
///   the printer motor.
/// * bit 1 - a value of 1 slows the printer motor, a value of 0 sets a normal motor speed.
///
/// An implementation of a [Spooler] trait is required as generic `S` to complete this type.
///
/// There's also a dedicated [bus device][crate::bus::zxprinter::ZxPrinterBusDevice]
/// implementation to be used solely with the `ZxPrinterDevice`.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "snapshot", serde(rename_all="camelCase"))]
pub struct ZxPrinterDevice<T, S> {
    /// An instance of the [Spooler] trait implementation type.
    #[cfg_attr(feature = "snapshot", serde(skip))]
    pub spooler: S,
    /// Can be changed to adjust speed. Default is 855 T-states (~16 dot lines / sec.).
    pub bit_delay: u16,
    motor: bool,
    ready: bool,
    cursor: u8,
    ready_ts: T,
    line: [u8;32]
}

const STATUS_OFF: u8       = 0b0011_1110;
const STATUS_NEW_LINE: u8  = 0b1011_1111;
const STATUS_BIT_READY: u8 = 0b0011_1111;
const STATUS_NOT_READY: u8 = 0b0011_1110;

const STYLUS_MASK: u8 = 0b1000_0000;
const MOTOR_MASK: u8  = 0b0000_0100;
const SLOW_MASK: u8   = 0b0000_0010;

// the approximate speed of Alphacom 32
const BIT_DELAY: u16 = 855;

impl<T: Default, S: Default> Default for ZxPrinterDevice<T, S> {
    fn default() -> Self {
        ZxPrinterDevice {
            spooler: S::default(),
            bit_delay: BIT_DELAY,
            motor: false,
            ready: false,
            cursor: 0,
            ready_ts: T::default(),
            line: [0u8;32]
        }
    }
}

impl<T, S> Deref for ZxPrinterDevice<T, S> {
    type Target = S;
    fn deref(&self) -> &Self::Target {
        &self.spooler
    }
}

impl<T, S> DerefMut for ZxPrinterDevice<T, S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.spooler
    }
}

// 2 lines / sec
impl<T: FrameTimestamp, S: Spooler> ZxPrinterDevice<T, S> {
    /// This method should be called after each emulated frame.
    pub fn next_frame(&mut self) {
        self.ready_ts = self.ready_ts.saturating_sub_frame();
    }
    /// This method should be called when device is being reset.
    pub fn reset(&mut self) {
        self.motor = false;
        self.ready = false;
        for p in self.line.iter_mut() {
            *p = 0;
        }
        self.cursor = 0;
    }
    /// This method should be called when a `CPU` writes to the **ZX Printer** port.
    pub fn write_control(&mut self, data: u8, timestamp: T) {
        if data & MOTOR_MASK == 0 { // start 
            self.start(data, timestamp);
        }
        else {
            self.stop();
        }
    }
    /// This method should be called when a `CPU` reads from the **ZX Printer** port.
    pub fn read_status(&mut self, timestamp: T) -> u8 {
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

    fn update_ready(&mut self, timestamp: T) -> bool {
        if timestamp > self.ready_ts {
            self.ready = true;
            return true;
        }
        false
    }

    #[inline]
    fn flush(&mut self) {
        if self.cursor != 0 {
            let cursor = self.cursor;
            self.cursor = 0;
            let mask = 0x80 >> (cursor.wrapping_sub(1) & 7);
            self.line[usize::from(cursor) >> 3] &= mask;
            let index = (usize::from(cursor) + 7) >> 3;
            self.spooler.push_line(&self.line[..index]);
        }
    }

    fn stop(&mut self) {
        if self.motor {
            self.motor = false;
            self.ready = false;
            self.flush();
            self.spooler.motor_off();
        }
    }

    #[inline]
    fn set_delay(&mut self, timestamp: T, data: u8) {
        let mut delay: u32 = self.bit_delay.into();
        if data & SLOW_MASK == SLOW_MASK {
            delay *= 2;
        }
        self.ready_ts = timestamp + delay;
    }

    fn start(&mut self, data: u8, timestamp: T) {
        if self.motor {
            if self.ready {
                self.write_bit(data & STYLUS_MASK == STYLUS_MASK);
                self.set_delay(timestamp, data);
                self.ready = false;
            }
        }
        else {
            self.motor = true;
            self.ready = false;
            self.cursor = 0;
            self.set_delay(timestamp, data);
            self.spooler.motor_on();
        }
    }

    fn write_bit(&mut self, stylus: bool) {
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
