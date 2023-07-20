/*
    Copyright (C) 2020-2023  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
#![allow(clippy::inconsistent_digit_grouping)]
#![allow(clippy::unusual_byte_groupings)]
#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};

use rand::{Rng, SeedableRng};
use rand::rngs::SmallRng;

use spectrusty_core::{bitflags_masks, clock::{FTs, TimestampOps}};
use super::{SerialPortDevice, DataState, ControlState};

bitflags! {
    /// Every key's state is encoded as a single bit on this 20-bit flag type.
    /// * Bit = 1 a key is being pressed.
    /// * Bit = 0 a key is not being pressed.
    #[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "snapshot", serde(from = "u32", into = "u32"))]
    #[derive(Default, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
    pub struct KeypadKeys: u32 {
        const RESERVED0 = 0b0000_0000_0000_0000_0001;
        const DOT       = 0b0000_0000_0000_0000_0010;
        const RESERVED2 = 0b0000_0000_0000_0000_0100;
        const N0        = 0b0000_0000_0000_0000_1000;
        const ENTER     = 0b0000_0000_0000_0001_0000;
        const N3        = 0b0000_0000_0000_0010_0000;
        const N2        = 0b0000_0000_0000_0100_0000;
        const N1        = 0b0000_0000_0000_1000_0000;
        const RPAREN    = 0b0000_0000_0001_0000_0000;
        const LPAREN    = 0b0000_0000_0010_0000_0000;
        const ASTERISK  = 0b0000_0000_0100_0000_0000;
        const SLASH     = 0b0000_0000_1000_0000_0000;
        const MINUS     = 0b0000_0001_0000_0000_0000;
        const N9        = 0b0000_0010_0000_0000_0000;
        const N8        = 0b0000_0100_0000_0000_0000;
        const N7        = 0b0000_1000_0000_0000_0000;
        const PLUS      = 0b0001_0000_0000_0000_0000;
        const N6        = 0b0010_0000_0000_0000_0000;
        const N5        = 0b0100_0000_0000_0000_0000;
        const N4        = 0b1000_0000_0000_0000_0000;
     // const ROW1_MASK = 0b0000_0000_1111_0000_0000;
     // const ROW2_MASK = 0b0000_1111_0000_0000_0000;
     // const ROW3_MASK = 0b1111_0000_0000_0000_0000;
     // const ROW4_MASK = 0b0000_0000_0000_1111_0000;
     // const ROW5_MASK = 0b0000_0000_0000_0000_1111;
    }
}
bitflags_masks!(KeypadKeys {
    /// The 1st (top) physical keypad row's mask.
    pub const ROW1_MASK = SLASH|ASTERISK|LPAREN|RPAREN;
    /// The 2nd physical keypad row's mask.
    pub const ROW2_MASK = N7|N8|N9|MINUS;
    /// The 3rd physical keypad row's mask.
    pub const ROW3_MASK = N4|N5|N6|PLUS;
    /// The 4th physical keypad row's mask.
    pub const ROW4_MASK = N1|N2|N3|ENTER;
    /// The 5th (bottom) physical keypad row's mask.
    pub const ROW5_MASK = N0|RESERVED2|DOT|RESERVED0;
    /// An alias of `DOT`.
    pub const PERIOD    = DOT;
    /// An alias of `N0`.
    pub const SHIFT     = N0;
    /// An alias of `RPAREN`
    pub const TOGGLE    = RPAREN;
    /// An alias of `ASTERISK`
    pub const MULTIPLY  = ASTERISK;
    /// An alias of `SLASH`
    pub const DIVIDE    = SLASH;
    /// An alias of `MINUS`
    pub const CMND      = MINUS;
});

mod intervals {
    use super::FTs;
    pub const PRESENT_MAX_INTERVAL   : FTs = 3593;
    pub const PRESENT_MIN_INTERVAL   : FTs = PRESENT_MAX_INTERVAL/10;
    pub const CORRECT_MAX_INTERVAL   : FTs = 3917;
    pub const CORRECT_MIN_INTERVAL   : FTs = CORRECT_MAX_INTERVAL/10;
    pub const WAITING_MAX_INTERVAL   : FTs = 4121;
    pub const WAITING_MIN_INTERVAL   : FTs = WAITING_MAX_INTERVAL/2;
    pub const BITLOOP_MAX_INTERVAL   : FTs = 570;
    pub const BITLOOP_MIN_INTERVAL   : FTs = BITLOOP_MAX_INTERVAL/4;
    pub const READY_MAX_INTERVAL     : FTs = 250;
    pub const READY_MIN_INTERVAL     : FTs = 50;
    pub const RESET_MIN_INTERVAL     : FTs = 2_000_000;
    pub const RESET_MAX_INTERVAL     : FTs = 7_000_000;
    pub const SET_GO_TIMEOUT         : FTs = 2128; // 0.6 ms
    pub const READY_START_TIMEOUT    : FTs = 709;  // 0.2 ms
    pub const STOP_STAND_EASY_TIMEOUT: FTs = 4610; // 1.3 ms
}
use intervals::*;
/// The ZX Spectrum 128 extension keypad.
///
/// For the history of this rare and often omitted in emulators device see:
/// [Spectrum128Keypad.htm](http://www.fruitcake.plus.com/Sinclair/Spectrum128/Keypad/Spectrum128Keypad.htm).
///
/// ```text
/// +-------+  +-------+  +-------+  +-------+
/// | DEL ← |  | ↑     |  | DEL → |  |TOGGLE |
/// |   /   |  |   *   |  |   (   |  |   )   |
/// +-------+  +-------+  +-------+  +-------+
///
/// +-------+  +-------+  +-------+  +-------+
/// | ←     |  | ↓     |  | →     |  | CMND  |
/// |   7   |  |   8   |  |   9   |  |   -   |
/// +-------+  +-------+  +-------+  +-------+
///
/// +-------+  +-------+  +-------+  +-------+
/// |<< DEL |  |>> DEL |  |↑↑     |  |↑↑     |
/// |   4   |  |   5   |  |   6   |  |   +   |
/// +-------+  +-------+  +-------+  +-------+
///
/// +-------+  +-------+  +-------+  +-------+
/// ||<< DEL|  |>>| DEL|  |⭱⭱     |  |   E   |
/// |   1   |  |   2   |  |   3   |  |   N   |
/// +-------+  +-------+  +-------+  |   T   |
///                                  |   E   |
/// +------------------+  +-------+  |   R   |
/// |            SHIFT |  |⭳⭳     |  |       |
/// |         0        |  |   .   |  |   =   |
/// +------------------+  +-------+  +-------+
///```
///
/// The extension keypad communicates with Spectrum using a unique protocol over the computer's serial connector.
///
/// This type implements [SerialPortDevice] bringing the extension keypad to its virtual existence.
///
/// To change the keypad state use methods directly on the implementation of this type.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "snapshot", serde(rename_all = "camelCase"))]
pub struct SerialKeypad<T> {
    keys: KeypadKeys,
    keys_changed: u32,
    next_row: u8,
    output_bits: u8,
    keypad_io: KeypadIoStatus,
    keypad_event_ts: T,
    #[cfg_attr(feature = "snapshot", serde(skip, default = "SmallRng::from_entropy"))]
    rng: SmallRng,
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
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
enum KeypadIoStatus {
    Reset,
    SyncPreset,
    SyncCorrect,
    Waiting,
    Ready,
    Data,
    Stopped,
}

impl<T: Default + TimestampOps> Default for SerialKeypad<T> {
    fn default() -> Self {
        let keys = KeypadKeys::default();
        let keys_changed = 0;
        let next_row = 0;
        let output_bits = 0;
        let rng = SmallRng::from_entropy();
        let keypad_event_ts = T::default() + RESET_MAX_INTERVAL;
        let keypad_io = KeypadIoStatus::Reset;
        SerialKeypad {
            keys, keys_changed, next_row, output_bits,
            keypad_event_ts, rng, keypad_io,
        }
    }
}

impl<T: TimestampOps> SerialPortDevice for SerialKeypad<T> {
    type Timestamp = T;
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
    fn next_frame(&mut self, eof_timestamp: Self::Timestamp) {
        if self.keypad_io != KeypadIoStatus::Reset {
            // timeout everything other than reset state
            if eof_timestamp.diff_from(self.keypad_event_ts) > RESET_MIN_INTERVAL as FTs {
                self.reset_status(eof_timestamp);
            }
        }
        self.keypad_event_ts = self.keypad_event_ts.saturating_sub(eof_timestamp);
    }
}

impl<T> SerialKeypad<T> {
    /// Reads the current state of the keypad.
    #[inline]
    pub fn get_key_state(&self) -> KeypadKeys {
        self.keys
    }
    /// Sets the state of the keypad.
    pub fn set_key_state(&mut self, keys: KeypadKeys) {
        let keys_changed = (self.keys ^ keys).bits();
        self.keys_changed |= keys_changed;
        self.keys = keys;
    }
}

impl<T: TimestampOps> SerialKeypad<T> {
    #[inline]
    fn gen_range_ts(&mut self, ts: T, lo: FTs, hi: FTs) -> T {
        let delta = self.rng.gen_range(lo..hi);
        ts + delta
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

    fn reset_status(&mut self, timestamp: T) {
        self.keypad_io = KeypadIoStatus::Reset;
        self.keypad_event_ts = self.gen_range_ts(timestamp, RESET_MIN_INTERVAL, RESET_MAX_INTERVAL);
        // println!("reset status {}", V::vts_to_tstates(self.keypad_event_ts) - V::vts_to_tstates(timestamp));
    }

    fn read_state(&mut self, timestamp: T) -> DataState {
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

    fn update_state(&mut self, cts: ControlState, timestamp: T) {
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
                   timestamp < self.keypad_event_ts + SET_GO_TIMEOUT { // timeout 0.6 ms
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
                   timestamp < self.keypad_event_ts + READY_START_TIMEOUT { // timeout 0.2 ms
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
                self.keypad_event_ts = timestamp + STOP_STAND_EASY_TIMEOUT; // timeout 1.3 ms
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

impl From<u32> for KeypadKeys {
    fn from(keys: u32) -> Self {
        KeypadKeys::from_bits_truncate(keys)
    }
}

impl From<KeypadKeys> for u32 {
    fn from(keys: KeypadKeys) -> Self {
        keys.bits()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spectrusty_core::test_bitflags_all_bits_defined_no_masks;

    #[test]
    fn flags_all_bits_defined() {
        test_bitflags_all_bits_defined_no_masks!(KeypadKeys, 20);
    }
}
