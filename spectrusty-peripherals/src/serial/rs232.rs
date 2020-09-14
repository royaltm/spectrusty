/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
use core::slice;
use std::io::{Read, Write, ErrorKind};

#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};
#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

use spectrusty_core::clock::FrameTimestamp;
use super::{SerialPortDevice, DataState, ControlState};

const CPU_HZ: u32 = 3_500_000;

/// The RS-232 serial port remote device.
///
/// Both ZX Spectrum's Interface 1 and 128k for communication via RS-232 use `DTR` and `CTS` lines to signal
/// readiness and transmit or receive data using one `START` bit, 8 data bits and 2 `STOP` bits without parity.
///
/// Spectrum's 128k ROM routines can send and transmit data with the following baud rates:
/// 50, 110, 300, 600, 1200, 2400, 4800, 9600 (default). The ZX Interface 1 allows additionally for 19200.
///
/// This type implements [SerialPortDevice] that transforms Spectrum's RS-232 signals to byte streams
/// and vice-versa.
///
/// `Rs232Io` actually doesn't emulate any particular device, but rather writes transmitted bytes to a
/// generic [writer][Write] and reads bytes from a generic [reader][Read].
///
/// Both `reader` and `writer` need to be implemented by the user and its types should be provided as
/// generics `R` and `W` accordingly.
///
/// An implementaion of [FrameTimestamp] is required to be provided as `T` for timestamp calculations.
///
/// The baud rate is not needed to be set up, as it is being auto-detected.
///
/// You may read the currently transmitted data baud rate for reading and writing using [Rs232Io::baud_rate].
///
/// # Panics
/// The [Read] and [Write] implementation methods must not return any error other than [ErrorKind::Interrupted].
/// If any other error is returned the [SerialPortDevice] implementation will panic.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "snapshot", serde(rename_all = "camelCase"))]
pub struct Rs232Io<T, R, W> {
    /// A reader providing data received by Spectrum.
    #[cfg_attr(feature = "snapshot", serde(default))]
    pub reader: R,
    /// A writer receiving data from Spectrum.
    #[cfg_attr(feature = "snapshot", serde(default))]
    pub writer: W,
    // bit_interval: u32, // CPU_HZ / BAUDS
    read_io: ReadStatus,
    read_max_delay: u32,
    read_event_ts: T,
    write_io: WriteStatus,
    write_max_delay: u32,
    write_event_ts: T
}

/// Spectrum's *BAUD RATES*.
pub const BAUD_RATES: &[u32;9] = &[50, 110, 300, 600, 1200, 2400, 4800, 9600, 19200];

/// A default *BAUD RATE* used by Spectrum.
pub const DEFAULT_BAUD_RATE: u32 = 9600;

const MIN_STOP_BIT_DELAY: u32 = CPU_HZ / 19200;
const MAX_STOP_BIT_DELAY: u32 = CPU_HZ / 49;
const STOP_BIT_GRACE_DELAY: u32 = 50;
const ERROR_GRACE_DELAY: u32 = MAX_STOP_BIT_DELAY * 11;

impl<T: Default, R: Default, W: Default> Default for Rs232Io<T, R, W> {
    fn default() -> Self {
        let reader = R::default();
        let writer = W::default();
        let read_io = ReadStatus::NotReady;
        let read_max_delay = Default::default();
        let read_event_ts = Default::default();
        let write_io = WriteStatus::Idle(ControlState::Active);
        let write_max_delay = Default::default();
        let write_event_ts = Default::default();
        Rs232Io {
            reader, writer,
            read_io,
            read_max_delay,
            read_event_ts,
            write_io,
            write_max_delay,
            write_event_ts
        }
    }
}

impl<T: FrameTimestamp, R: Read, W: Write> SerialPortDevice for Rs232Io<T, R, W> {
    type Timestamp = T;
    #[inline]
    fn write_data(&mut self, rxd: DataState, timestamp: Self::Timestamp) -> ControlState {
        self.process_write(rxd, timestamp)
    }
    #[inline]
    fn poll_ready(&mut self, timestamp: Self::Timestamp) -> ControlState {
        self.process_poll(timestamp)
    }
    #[inline]
    fn update_cts(&mut self, cts: ControlState, timestamp: Self::Timestamp) {
        self.process_update_cts(cts, timestamp) 
    }
    #[inline]
    fn read_data(&mut self, timestamp: Self::Timestamp) -> DataState {
        self.process_read(timestamp) 
    }
    #[inline]
    fn next_frame(&mut self, _timestamp: Self::Timestamp) {
        self.read_event_ts = self.read_event_ts.saturating_sub_frame();
        self.write_event_ts = self.write_event_ts.saturating_sub_frame();
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
enum WriteStatus {
    Idle(ControlState),
    StartBit,
    ReceivingData(u8),
    StopBits(u8),
    Full(u8),
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
enum ReadStatus {
    NotReady,
    StartBit(u8),
    Synchronize(u8),
    SendingData(u8),
}

impl<T: FrameTimestamp, R: Read, W: Write> Rs232Io<T, R, W> {
    /// Returns the detected *BAUD RATE* of the current or the last transmission.
    ///
    /// If there was no transmission since the start of the emulator, returns the default.
    pub fn baud_rate(&self) -> u32 {
        let bit_period = if self.write_event_ts > self.read_event_ts {
            self.write_max_delay
        }
        else {
            self.read_max_delay
        } * 2 / 3;
        if bit_period == 0 {
            return DEFAULT_BAUD_RATE;
        }

        let rate = CPU_HZ / bit_period;
        match BAUD_RATES.binary_search(&rate) {
            Ok(index) => BAUD_RATES[index],
            Err(0) => BAUD_RATES[0],
            Err(index) => if let Some(&max_rate) = BAUD_RATES.get(index) {
                if rate < (max_rate + BAUD_RATES[index - 1]) / 2 {
                    BAUD_RATES[index - 1]
                }
                else {
                    max_rate
                }
            }
            else {
                BAUD_RATES[index - 1]
            }
        }
    }

    fn write_byte_to_writer(&mut self, data: u8) -> bool {
        let buf = slice::from_ref(&data);
        loop {
            return match self.writer.write(buf) {
                Ok(0) => false,
                Ok(..) => true,
                Err(ref e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e) => panic!("an error occured while writing {}", e)
            }
        }
    }

    fn read_byte_from_reader(&mut self) -> Option<u8> {
        let mut byte = 0;
        loop {
            return match self.reader.read(slice::from_mut(&mut byte)) {
                Ok(0) => None,
                Ok(..) => Some(byte),
                Err(ref e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e) => panic!("an error occured while reading {}", e),
            };
        }
    }

    fn process_update_cts(&mut self, cts: ControlState, _timestamp: T) {
        if cts.is_inactive() {
            // debug!("CTS inactive {:?}", timestamp);
            self.read_io = ReadStatus::NotReady;
        }
        else if let Some(byte) = self.read_byte_from_reader() {
            // debug!("CTS active {:?} {} {:02x}", timestamp, byte as char, byte);
            // self.read_event_ts = timestamp;
            self.read_io = ReadStatus::StartBit(byte);
        }
    }

    fn process_read(&mut self, timestamp: T) -> DataState { // -> txd
        match self.read_io {
            ReadStatus::NotReady => DataState::Mark,
            ReadStatus::StartBit(byte) => {
                self.read_event_ts = timestamp + MIN_STOP_BIT_DELAY;
                self.read_io = ReadStatus::Synchronize(byte);
                DataState::Space
            }
            ReadStatus::Synchronize(byte) => {
                if timestamp >= self.read_event_ts {
                    let delay_fts = timestamp.diff_from(self.read_event_ts) as u32;
                    if delay_fts < MAX_STOP_BIT_DELAY * 3 / 2 {
                        self.read_max_delay = delay_fts + MIN_STOP_BIT_DELAY;
                        self.read_event_ts = timestamp + self.read_max_delay;
                        self.read_io = ReadStatus::SendingData(0x80 | (byte >> 1));
                        let bit = byte & 1 == 1;
                        bit.into()
                    }
                    else {
                        self.read_io = ReadStatus::NotReady;
                        DataState::Mark
                    }
                }
                else {
                    DataState::Space
                }
            }
            ReadStatus::SendingData(byte) => {
                if timestamp < self.read_event_ts {
                    let bit = byte & 1 == 1;
                    let byte = byte >> 1;
                    self.read_event_ts = timestamp + self.read_max_delay;
                    if byte != 0 {
                        self.read_io = ReadStatus::SendingData(byte);
                        return bit.into()
                    }
                }
                self.read_io = ReadStatus::NotReady;
                DataState::Mark
            }
        }
    }

    #[inline]
    fn write_failed(&mut self, timestamp: T) -> ControlState {
        self.write_io = WriteStatus::Idle(ControlState::Inactive);
        self.write_event_ts = timestamp + ERROR_GRACE_DELAY;
        ControlState::Inactive
    }

    fn process_poll(&mut self, timestamp: T) -> ControlState {
        match self.write_io {
            WriteStatus::Idle(dtr) => {
                if timestamp >= self.write_event_ts {
                    ControlState::Active
                }
                else {
                    dtr
                }
            }
            WriteStatus::Full(byte) => {
                if self.write_byte_to_writer(byte) {
                    self.write_io = WriteStatus::Idle(ControlState::Active);
                    ControlState::Active
                }
                else {
                    ControlState::Inactive
                }
            }
            _ => ControlState::Active
        }
    }

    fn process_write(&mut self, rxd: DataState, timestamp: T) -> ControlState { // -> dtr
        // println!("rxd: {:?} {:?}", rxd, V::vts_diff(self.read_event_ts, timestamp));
        self.read_event_ts = timestamp;
        match self.write_io {
            WriteStatus::Idle(dtr) => {
                if timestamp >= self.write_event_ts {
                    if rxd.is_space() { // START
                        self.write_event_ts = timestamp + MIN_STOP_BIT_DELAY;
                        self.write_io = WriteStatus::StartBit;
                    }
                    ControlState::Active
                }
                else {
                    dtr
                }
            }
            WriteStatus::StartBit => {
                if timestamp >= self.write_event_ts {
                    let delta_fts = timestamp.diff_from(self.write_event_ts) as u32;
                    if delta_fts < MAX_STOP_BIT_DELAY {
                        let bit: u8 = rxd.into();
                        self.write_max_delay = (delta_fts + MIN_STOP_BIT_DELAY) * 3 / 2;
                        self.write_event_ts = timestamp + self.write_max_delay;
                        // println!("bauds: {} {}", self.baud_rate(), (delta_fts + MIN_STOP_BIT_DELAY));
                        self.write_io = WriteStatus::ReceivingData((bit|0x80).rotate_right(1));
                        return ControlState::Active
                    }
                }
                self.write_failed(timestamp)
            }
            WriteStatus::ReceivingData(prev_bits) => {
                if timestamp < self.write_event_ts {
                    let bit: u8 = rxd.into();
                    let next_bits = (prev_bits & !1 | bit).rotate_right(1);
                    self.write_event_ts = timestamp + self.write_max_delay;
                    if prev_bits & 1 == 1 {
                        self.write_io = WriteStatus::StopBits(next_bits);
                    }
                    else {
                        self.write_io = WriteStatus::ReceivingData(next_bits);
                    }
                    ControlState::Active
                }
                else {
                    self.write_failed(timestamp)
                }
            }
            WriteStatus::StopBits(data) => {
                if rxd.is_mark() && timestamp < self.write_event_ts {
                    if self.write_byte_to_writer(data) {
                        self.write_event_ts = timestamp + self.write_max_delay * 4 / 3 + STOP_BIT_GRACE_DELAY;
                        self.write_io = WriteStatus::Idle(ControlState::Active);
                        ControlState::Active
                    }
                    else {
                        self.write_io = WriteStatus::Full(data);
                        ControlState::Inactive
                    }
                }
                else {
                    self.write_failed(timestamp)
                }
            }
            WriteStatus::Full(..) => ControlState::Inactive
        }
    }
}
