// http://www.fruitcake.plus.com/Sinclair/Spectrum128/Keypad/
use core::fmt;
use core::marker::PhantomData;
use crate::clock::{VideoTs, Ts};
// use crate::chip::ula128::{SerialIo, SerialPort};
use crate::peripherals::serial::{SerialPort, DataState, ControlState};
use rand::{Rng, SeedableRng};
use rand::rngs::SmallRng;
use crate::video::VideoFrame;

bitflags! {
    /// Every key's state is encoded as a single bit on this 20-bit flag type.
    /// * Bit = 1 key is being pressed.
    /// * Bit = 0 key not being pressed.
    #[derive(Default)]
    pub struct KeypadKeys: u32 {
        /// The 1st (top) physical keypad row's mask
        const ROW1_MASK = 0b0000_0000_1111_0000_0000;
        /// The 2nd physical keypad row's mask
        const ROW2_MASK = 0b0000_1111_0000_0000_0000;
        /// The 3rd physical keypad row's mask
        const ROW3_MASK = 0b1111_0000_0000_0000_0000;
        /// The 4th physical keypad row's mask
        const ROW4_MASK = 0b0000_0000_0000_1111_0000;
        /// The 5th physical keypad row's mask
        const ROW5_MASK = 0b0000_0000_0000_0000_1111;
        const DOT       = 0b0000_0000_0000_0000_0010;
        const PERIOD    = Self::DOT.bits();
        const SHIFT     = 0b0000_0000_0000_0000_1000;
        const N0        = Self::SHIFT.bits();
        const ENTER     = 0b0000_0000_0000_0001_0000;
        const N3        = 0b0000_0000_0000_0010_0000;
        const N2        = 0b0000_0000_0000_0100_0000;
        const N1        = 0b0000_0000_0000_1000_0000;
        const RPAREN    = 0b0000_0000_0001_0000_0000;
        const TOGGLE    = Self::RPAREN.bits();
        const LPAREN    = 0b0000_0000_0010_0000_0000;
        const ASTERISK  = 0b0000_0000_0100_0000_0000;
        const MULTIPLY  = Self::ASTERISK.bits();
        const SLASH     = 0b0000_0000_1000_0000_0000;
        const DIVIDE    = Self::SLASH.bits();
        const MINUS     = 0b0000_0001_0000_0000_0000;
        const CMND      = Self::MINUS.bits();
        const N9        = 0b0000_0010_0000_0000_0000;
        const N8        = 0b0000_0100_0000_0000_0000;
        const N7        = 0b0000_1000_0000_0000_0000;
        const PLUS      = 0b0001_0000_0000_0000_0000;
        const N6        = 0b0010_0000_0000_0000_0000;
        const N5        = 0b0100_0000_0000_0000_0000;
        const N4        = 0b1000_0000_0000_0000_0000;
    }
}

mod intervals {
    pub const PRESENT_MAX_INTERVAL   : u32 = 3593;
    pub const PRESENT_MIN_INTERVAL   : u32 = PRESENT_MAX_INTERVAL/10;
    pub const CORRECT_MAX_INTERVAL   : u32 = 3917;
    pub const CORRECT_MIN_INTERVAL   : u32 = CORRECT_MAX_INTERVAL/10;
    pub const WAITING_MAX_INTERVAL   : u32 = 4121;
    pub const WAITING_MIN_INTERVAL   : u32 = WAITING_MAX_INTERVAL/2;
    pub const BITLOOP_MAX_INTERVAL   : u32 = 570;
    pub const BITLOOP_MIN_INTERVAL   : u32 = BITLOOP_MAX_INTERVAL/4;
    pub const READY_MAX_INTERVAL     : u32 = 250;
    pub const READY_MIN_INTERVAL     : u32 = 50;
    pub const RESET_MIN_INTERVAL     : u32 = 2_000_000;
    pub const RESET_MAX_INTERVAL     : u32 = 7_000_000;
    pub const SET_GO_TIMEOUT         : u32 = 2128; // 0.6 ms
    pub const READY_START_TIMEOUT    : u32 = 709;  // 0.2 ms
    pub const STOP_STAND_EASY_TIMEOUT: u32 = 4610; // 1.3 ms
}
use intervals::*;

#[derive(Clone, Debug)]
pub struct SerialKeypad<V> {
    keys: KeypadKeys,
    keys_changed: u32,
    next_row: u8,
    output_bits: u8,
    keypad_io: KeypadIoStatus,
    keypad_event_ts: VideoTs,
    rng: SmallRng,
    _video_frame: PhantomData<V>
}

// status = 0 and a stop bit
const STATUS0_ROW_DATA:     u8 = 0b_____10;
// status = 1, 4 bit places and a stop bit
const STATUS1_ROW_TEMPLATE: u8 = 0b10_0001;
// transmitted after synchronization
const SERIAL_DATA:          u8 = 0b_1_0100;

#[inline(always)]
fn row_data_to_output_bits(nibble: u8) -> u8 {
    STATUS1_ROW_TEMPLATE | ((nibble & 0xF) << 1)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KeypadIoStatus {
    Reset,
    SyncPreset,
    SyncCorrect,
    Waiting,
    Ready,
    Data,
    Stopped,
}

impl<V> Default for SerialKeypad<V> {
    fn default() -> Self {
        let keys = KeypadKeys::default();
        let keys_changed = 0;
        let next_row = 0;
        let output_bits = 0;
        let keypad_event_ts = VideoTs::default();
        let rng = SmallRng::from_entropy();
        let keypad_io = KeypadIoStatus::Reset;
        SerialKeypad {
            keys, keys_changed, next_row, output_bits,
            keypad_event_ts, rng, keypad_io,
            _video_frame: PhantomData }
    }
}

impl<V: VideoFrame> SerialPort for SerialKeypad<V> {
    type Timestamp = VideoTs;
    fn reset(&mut self, timestamp: Self::Timestamp) {
        self.rng = SmallRng::from_entropy();
        self.reset_status(timestamp);
    }
    #[inline(always)]
    fn write_data(&mut self, _rxd: DataState, _timestamp: Self::Timestamp) -> ControlState {
        ControlState::Inactive
    }
    #[inline(always)]
    fn poll_ready(&mut self, _timestamp: Self::Timestamp) -> ControlState {
        ControlState::Inactive
    }
    #[inline]
    fn update_cts(&mut self, cts: ControlState, timestamp: Self::Timestamp) {
        self.update_state(cts, timestamp)
    }
    #[inline]
    fn read_data(&mut self, timestamp: Self::Timestamp) -> DataState {
        self.read_state(timestamp)   
    }
    #[inline]
    fn end_frame(&mut self, _timestamp: Self::Timestamp) {
        self.keypad_event_ts = V::vts_saturating_sub_frame(self.keypad_event_ts);
    }
}

impl<V: VideoFrame> SerialKeypad<V> {
    #[inline]
    pub fn get_key_state(&self) -> KeypadKeys {
        self.keys
    }

    pub fn set_key_state(&mut self, keys: KeypadKeys) {
        let keys_changed = (self.keys ^ keys).bits();
        self.keys_changed |= keys_changed;
        self.keys = keys;
    }

    #[inline]
    fn gen_range_ts(&mut self, ts: VideoTs, lo: u32, hi: u32) -> VideoTs {
        let delta = self.rng.gen_range(lo, hi);
        V::vts_add_ts(ts, delta)
    }

    #[inline]
    fn initiate_output_data(&mut self) {
        self.next_row = 0;
        // we''ll transmit a full status of each row on the next scan
        self.keys_changed = KeypadKeys::all().bits();
        self.output_bits = SERIAL_DATA;
    }

    #[inline]
    fn next_output_bit(&mut self) {
        let bits = self.output_bits >> 1;
        if (bits & !1) == 0 {
            self.next_output_data();
        }
        else {
            self.output_bits = bits
        }
    }

    #[inline]
    fn next_output_data(&mut self) {
        let row = self.next_row % 5;
        self.next_row = row + 1;
        let row_bit_shift = 4 * row;
        let row_mask = 0xF << row_bit_shift;
        self.output_bits = if self.keys_changed & row_mask == 0 {
            STATUS0_ROW_DATA
        }
        else {
            let nibble = (self.keys.bits() >> row_bit_shift) as u8;
            self.keys_changed &= !row_mask;
            row_data_to_output_bits(nibble)
        }
    }

    fn reset_status(&mut self, timestamp: VideoTs) {
        self.keypad_io = KeypadIoStatus::Reset;
        self.keypad_event_ts = self.gen_range_ts(timestamp, RESET_MIN_INTERVAL, RESET_MAX_INTERVAL);
        // println!("reset status {}", V::vts_to_tstates(self.keypad_event_ts) - V::vts_to_tstates(timestamp));
    }

    fn read_state(&mut self, timestamp: VideoTs) -> DataState {
        match self.keypad_io {
            KeypadIoStatus::Reset|
            KeypadIoStatus::Waiting => {
                DataState::Mark
            }
            KeypadIoStatus::SyncPreset|
            KeypadIoStatus::Ready => {
                if timestamp >= self.keypad_event_ts { // PRESENT|READY
                    DataState::Space
                }
                else {
                    DataState::Mark
                }
            }
            KeypadIoStatus::SyncCorrect => {
                if timestamp >= self.keypad_event_ts { // CORRECT
                    DataState::Mark
                }
                else {
                    DataState::Space
                }
            }
            KeypadIoStatus::Data => {
                (self.output_bits & 1 == 1).into()
            }
            KeypadIoStatus::Stopped => DataState::Space
        }
    }

    fn update_state(&mut self, cts: ControlState, timestamp: VideoTs) {
        match self.keypad_io {
            KeypadIoStatus::Reset => {
                if timestamp >= self.keypad_event_ts {
                    if cts.is_active() { // spectrum polls low (MARKS), respond with PRESENT after delay
                        self.keypad_event_ts = self.gen_range_ts(timestamp, PRESENT_MIN_INTERVAL, PRESENT_MAX_INTERVAL);
                        // println!("idle -> marks {}", V::vts_to_tstates(self.keypad_event_ts) - V::vts_to_tstates(timestamp));
                        self.keypad_io = KeypadIoStatus::SyncPreset;
                    }
                    else {
                        // println!("idle -> reset {}", V::vts_to_tstates(self.keypad_event_ts) - V::vts_to_tstates(timestamp));
                        self.reset_status(timestamp);
                    }
                }
            }
            KeypadIoStatus::SyncPreset => {
                assert!(cts.is_inactive(), "CTS must have been active before");
                if timestamp >= self.keypad_event_ts { // SET
                    // println!("prsent -> set {}", V::vts_to_tstates(timestamp) - V::vts_to_tstates(self.keypad_event_ts));
                    self.keypad_event_ts = self.gen_range_ts(timestamp, CORRECT_MIN_INTERVAL, CORRECT_MAX_INTERVAL);
                    self.keypad_io = KeypadIoStatus::SyncCorrect;
                }
                else {
                    // println!("prsent -> reset {}", V::vts_to_tstates(timestamp) - V::vts_to_tstates(self.keypad_event_ts));
                    self.reset_status(timestamp);
                }
            }
            KeypadIoStatus::SyncCorrect => {
                assert!(cts.is_active(), "CTS must have been inactive before");
                if timestamp >= self.keypad_event_ts && // GO
                   timestamp < V::vts_add_ts(self.keypad_event_ts, SET_GO_TIMEOUT) { // timeout 0.6 ms
                    // println!("correct -> waiting {}", V::vts_to_tstates(timestamp) - V::vts_to_tstates(self.keypad_event_ts));
                    self.keypad_event_ts = self.gen_range_ts(timestamp, WAITING_MIN_INTERVAL, WAITING_MAX_INTERVAL);
                    self.keypad_io = KeypadIoStatus::Waiting;
                    self.initiate_output_data();
                }
                else {
                    // println!("correct -> reset {}", V::vts_to_tstates(timestamp) - V::vts_to_tstates(self.keypad_event_ts));
                    self.reset_status(timestamp);
                }
            }
            KeypadIoStatus::Waiting => { // synchronized, we are high, cts is low
                assert!(cts.is_inactive(), "CTS must have been active before");
                // cts went high (ATTENTION), respond with READY after delay
                // println!("waiting -> ready {}", V::vts_to_tstates(timestamp) - V::vts_to_tstates(self.keypad_event_ts));
                self.keypad_event_ts = self.gen_range_ts(self.keypad_event_ts.max(timestamp),
                                                        READY_MIN_INTERVAL, READY_MAX_INTERVAL); // timeout is 17643
                self.keypad_io = KeypadIoStatus::Ready;
            }
            KeypadIoStatus::Ready => {
                assert!(cts.is_active(), "CTS must have been inactive before");
                if timestamp >= self.keypad_event_ts && // START
                   timestamp < V::vts_add_ts(self.keypad_event_ts, READY_START_TIMEOUT) { // timeout 0.2 ms
                    // println!("ready -> data {}", V::vts_to_tstates(timestamp) - V::vts_to_tstates(self.keypad_event_ts));
                    self.keypad_event_ts = timestamp; // for debug only
                    self.keypad_io = KeypadIoStatus::Data;
                    // println!("data: [{}] {:05b}", self.next_row, self.output_bits);
                    // ? no timeout nor min limit?
                }
                else {
                    // println!("ready -> reset {}", V::vts_to_tstates(timestamp) - V::vts_to_tstates(self.keypad_event_ts));
                    self.reset_status(timestamp);
                }
            }
            KeypadIoStatus::Data => { // transmitting data
                assert!(cts.is_inactive(), "CTS must have been active before");
                // cts went high (STOP), stop transmitting STOPPED (immediate)
                // println!("data -> stop {}", V::vts_to_tstates(timestamp) - V::vts_to_tstates(self.keypad_event_ts));
                self.keypad_event_ts = V::vts_add_ts(timestamp, STOP_STAND_EASY_TIMEOUT); // timeout 1.3 ms
                self.keypad_io = KeypadIoStatus::Stopped;
                // println!("{:06b}", self.output_bits);
                self.next_output_bit();
            }
            KeypadIoStatus::Stopped => { // transmissison over
                assert!(cts.is_active(), "CTS must have been inactive before");
                if timestamp < self.keypad_event_ts { // STAND EASY
                    // println!("stopped -> stand easy {}", V::vts_to_tstates(timestamp) - V::vts_to_tstates(self.keypad_event_ts) + STOP_STAND_EASY_TIMEOUT as i32);
                    self.keypad_event_ts = self.gen_range_ts(timestamp, BITLOOP_MIN_INTERVAL, BITLOOP_MAX_INTERVAL);
                    self.keypad_io = KeypadIoStatus::Waiting;
                }
                else { // timeout
                    // println!("stopped -> reset {}", V::vts_to_tstates(timestamp) - V::vts_to_tstates(self.keypad_event_ts));
                    self.reset_status(timestamp);
                }
            }
        }
    }
}