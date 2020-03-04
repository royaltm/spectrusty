use core::fmt;
use core::marker::PhantomData;
use core::slice;
use std::io::{Read, Write, ErrorKind};

#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

use crate::clock::{FTs, VideoTs, Ts};
use crate::peripherals::serial::{SerialPortDevice, DataState, ControlState};
use crate::peripherals::ay::{AyIoPort, AyIoNullPort, Ay128kPortDecode};
use crate::bus::ay::{Ay3_891xBusDevice};
use crate::video::VideoFrame;
use crate::chip::ula128::CPU_HZ;
/// The RS-232 serial port remote device.
///
/// Both ZX Spectrum's Interface 1 and 128k for communication via RS-232 use `DTR` and `CTS` lines to signal
/// readiness and transmit or receive data using one `START` bit, 8 data bits and 2 `STOP` bits without parity.
///
/// Spectrum's ROM routines can send and transmit data with the following baud rates only:
/// 50, 110, 300, 600, 1200, 2400, 4800, 9600 (default).
///
/// This type implements [SerialPortDevice] that transforms Spectrum's RS-232 signals to byte streams
/// and vice-versa.
///
/// `Rs232Io` actually doesn't emulate any particular device, but rather writes transmitted bytes to a
/// generic [writer][Write] and reads bytes from a generic [reader][Read].
///
/// Both `reader` and `writer` needs to be implemented by the user and its types should be provided as
/// generics `R` and `W` accordingly.
///
/// An implementaion of [VideoFrame] is required to be provided as `V` for timestamp calculations.
///
/// The baud rate is not needed to be set up, as it is being auto-detected.
///
/// # Panics
/// The [Read] and [Write] implementation methods must not return any error other than [ErrorKind::Interrupted].
/// If any other error is returned the [SerialPortDevice] implementation will panic.
#[derive(Clone, Debug)]
pub struct Rs232Io<V, R, W> {
    /// A reader providing data received by Spectrum.
    pub reader: R,
    /// A writer receiving data from Spectrum.
    pub writer: W,
    // bit_interval: u32, // CPU_HZ / BAUDS
    read_io: ReadStatus,
    read_max_delay_ts: VideoTs,
    read_event_ts: VideoTs,
    write_io: WriteStatus,
    write_max_delay_ts: VideoTs,
    write_event_ts: VideoTs,
    _video_frame: PhantomData<V>
}

const MIN_STOP_BIT_DELAY: u32 = CPU_HZ / 9600;
const MAX_STOP_BIT_DELAY: u32 = CPU_HZ / 50;
const ERROR_GRACE_DELAY: u32 = MAX_STOP_BIT_DELAY * 11;

impl<V, R: Default, W: Default> Default for Rs232Io<V, R, W> {
    fn default() -> Self {
        let reader = R::default();
        let writer = W::default();
        let read_io = ReadStatus::NotReady;
        let read_max_delay_ts = Default::default();
        let read_event_ts = Default::default();
        let write_io = WriteStatus::Idle;
        let write_max_delay_ts = Default::default();
        let write_event_ts = Default::default();
        Rs232Io {
            reader, writer,
            read_io,
            read_max_delay_ts,
            read_event_ts,
            write_io,
            write_max_delay_ts,
            write_event_ts,
            _video_frame: PhantomData }
    }
}

impl<V: VideoFrame, R: Read, W: Write> SerialPortDevice for Rs232Io<V, R, W> {
    type Timestamp = VideoTs;
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
    fn end_frame(&mut self, _timestamp: Self::Timestamp) {
        self.read_event_ts = V::vts_saturating_sub_frame(self.read_event_ts);
        self.write_event_ts = V::vts_saturating_sub_frame(self.write_event_ts);
    }
}


#[derive(Clone, Copy, Debug, PartialEq)]
enum WriteStatus {
    Idle, // waiting for a start bit, DTR = 0
    StartBit,
    ReceivingData(u8), // ts of next bit after write -> Idle
    Full(u8), // writer full DTR = 1
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum ReadStatus {
    NotReady,
    StartBit(u8), // ts
    Synchronize(u8),     // ts of next bit
    SendingData(u8),     // ts of next bit
}

//V::vts_add_ts(timestamp, self.bit_interval / 2);
impl<V: VideoFrame, R: Read, W: Write> Rs232Io<V, R, W> {
    fn reset_status(&mut self, _timestamp: VideoTs) {
        self.read_io = ReadStatus::NotReady;
        self.write_io = WriteStatus::Idle;
        // self.read_event_ts = V::vts_add_ts(timestamp, RESET_WRITE_TIMEOUT);
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

    fn process_update_cts(&mut self, cts: ControlState, _timestamp: VideoTs) {
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

    fn process_read(&mut self, timestamp: VideoTs) -> DataState { // -> txd
        match self.read_io {
            ReadStatus::NotReady => DataState::Mark,
            ReadStatus::StartBit(byte) => {
                self.read_event_ts = V::vts_add_ts(timestamp, MIN_STOP_BIT_DELAY);
                self.read_io = ReadStatus::Synchronize(byte);
                DataState::Space
            }
            ReadStatus::Synchronize(byte) => {
                if timestamp >= self.read_event_ts {
                    if timestamp < V::vts_add_ts(self.read_event_ts, MAX_STOP_BIT_DELAY*3/2) {
                        let interval_vts = V::vts_add_ts(
                            V::vts_saturating_sub_vts_normalized(timestamp, self.read_event_ts),
                            MIN_STOP_BIT_DELAY);

                        let bit = byte & 1 == 1;
                        let one_half_bit_delta_ts = V::vts_to_tstates(interval_vts);
                        // self.baud_delta_fts = one_half_bit_delta_ts;
                        let bit_delta_fts = one_half_bit_delta_ts*2/3;
                        debug!("read: {} ts ~{} bauds", bit_delta_fts, CPU_HZ as i32 / bit_delta_fts);
                        self.read_max_delay_ts = interval_vts;
                        self.read_event_ts = V::vts_saturating_add_vts_normalized(timestamp, interval_vts);
                        // self.read_event_ts = V::vts_saturating_add_vts_normalized(timestamp, self.read_max_delay_ts);
                        self.read_io = ReadStatus::SendingData(0x80 | (byte >> 1));
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
                    self.read_event_ts = V::vts_saturating_add_vts_normalized(timestamp, self.read_max_delay_ts);
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
    fn write_failed(&mut self, timestamp: VideoTs) -> ControlState {
        self.write_io = WriteStatus::Idle;
        self.write_event_ts = V::vts_add_ts(timestamp, ERROR_GRACE_DELAY);
        ControlState::Inactive
    }

    fn process_poll(&mut self, timestamp: VideoTs) -> ControlState {
        match self.write_io {
            WriteStatus::Idle => {
                if timestamp >= self.write_event_ts {
                    ControlState::Active
                }
                else {
                    ControlState::Inactive
                }
            }
            WriteStatus::Full(byte) => {
                if self.write_byte_to_writer(byte) {
                    self.write_io = WriteStatus::Idle;
                    ControlState::Active
                }
                else {
                    ControlState::Inactive
                }
            }
            _ => ControlState::Active
        }
    }

    fn process_write(&mut self, rxd: DataState, timestamp: VideoTs) -> ControlState { // -> dtr
        match self.write_io {
            WriteStatus::Idle => {
                if timestamp >= self.write_event_ts {
                    if rxd.is_space() { // START
                        self.write_event_ts = V::vts_add_ts(timestamp, MIN_STOP_BIT_DELAY);
                        self.write_io = WriteStatus::StartBit;
                    }
                    ControlState::Active
                }
                else {
                    ControlState::Inactive
                }
            }
            WriteStatus::StartBit => {
                if timestamp >= self.write_event_ts &&
                   timestamp < V::vts_add_ts(self.write_event_ts, MAX_STOP_BIT_DELAY) {
                    let bit: u8 = rxd.into();
                    let bit_delta_fts = V::vts_to_tstates(
                                            V::vts_saturating_sub_vts_normalized(timestamp, self.write_event_ts))
                                            + MIN_STOP_BIT_DELAY as i32;
                    self.write_max_delay_ts = V::tstates_to_vts(bit_delta_fts*3/2);
                    debug!("write: {} ts ~{} bauds", bit_delta_fts, CPU_HZ as i32 / bit_delta_fts);
                    self.write_event_ts = V::vts_saturating_add_vts_normalized(timestamp, self.write_max_delay_ts);
                    self.write_io = WriteStatus::ReceivingData((bit|0x80).rotate_right(1));
                    ControlState::Active
                }
                else {
                    self.write_failed(timestamp)
                }
            }
            WriteStatus::ReceivingData(prev_bits) => {
                if timestamp < self.write_event_ts {
                    let bit: u8 = rxd.into();
                    let next_bits = (prev_bits & !1 | bit).rotate_right(1);
                    if prev_bits & 1 == 1 {
                        if !self.write_byte_to_writer(next_bits) {
                            self.write_io = WriteStatus::Full(next_bits);
                            return ControlState::Inactive
                        }
                        self.write_event_ts = timestamp;
                        self.write_io = WriteStatus::Idle;
                    }
                    else {
                        self.write_event_ts = V::vts_saturating_add_vts_normalized(timestamp, self.write_max_delay_ts);
                        self.write_io = WriteStatus::ReceivingData(next_bits);
                    }
                    ControlState::Active
                }
                else {
                    self.write_failed(timestamp)
                }
            }
            WriteStatus::Full(..) => ControlState::Inactive
        }
    }
}
