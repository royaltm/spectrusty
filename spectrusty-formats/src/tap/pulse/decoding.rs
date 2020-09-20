/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
use core::num::NonZeroU32;
use core::slice;
use std::io::{Result, Write};

use super::consts::*;

const SYNC_PULSE_TOLERANCE: u32 = (SYNC_PULSE2_LENGTH.get() - SYNC_PULSE1_LENGTH.get())/2;
const SYNC_PULSE1_MIN: u32 = SYNC_PULSE1_LENGTH.get() - SYNC_PULSE_TOLERANCE;
const SYNC_PULSE1_MAX: u32 = SYNC_PULSE1_LENGTH.get() + SYNC_PULSE_TOLERANCE - 1;
const SYNC_PULSE2_MIN: u32 = SYNC_PULSE2_LENGTH.get() - SYNC_PULSE_TOLERANCE;
const SYNC_PULSE2_MAX: u32 = SYNC_PULSE2_LENGTH.get() + SYNC_PULSE_TOLERANCE - 1;
const LEAD_PULSE_TOLERANCE: u32 = 250;
const LEAD_PULSE_MIN: u32 = LEAD_PULSE_LENGTH.get() - LEAD_PULSE_TOLERANCE;
const LEAD_PULSE_MAX: u32 = LEAD_PULSE_LENGTH.get() + LEAD_PULSE_TOLERANCE - 1;
const DATA_PULSE_TOLERANCE: u32 = 250;
const DATA_PULSE_MIN: u32 = ZERO_PULSE_LENGTH.get() - DATA_PULSE_TOLERANCE;
const DATA_PULSE_MAX: u32 = ONE_PULSE_LENGTH.get() + DATA_PULSE_TOLERANCE - 1;
const DATA_PULSE_THRESHOLD: u32 = (ZERO_PULSE_LENGTH.get() + ONE_PULSE_LENGTH.get())/2;
const MIN_LEAD_COUNT: u32 = 16;

/// The current state of the [PulseDecodeWriter].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PulseDecodeState {
    /// Initial state, waiting for lead pulses.
    Idle,
    /// Receiving lead pulses.
    Lead {
        counter: u32
    },
    /// Received the 1st sync pulse.
    Sync1,
    /// Received the 2nd sync pulse.
    Sync2,
    /// Receiving data pulses.
    Data{
        /// A data byte being received currently.
        current: u8,
        /// A pulse counter for the current byte.
        pulse: u8,
        /// How many bytes written.
        written: u32
    },
}

/// Provides a decoder of *TAPE* T-state pulse intervals.
///
/// The timing of the pulses should match those produced by ZX Spectrum's ROM loading routines.
///
/// After invoking [PulseDecodeWriter::end] or [PulseDecodeWriter::new] [PulseDecodeWriter] expects
/// a data transfer which consists of:
///
/// * lead pulses followed by
/// * two synchronization pulses followed by 
/// * data pulses
///
/// Provide pulse iterators to [PulseDecodeWriter::write_decoded_pulses] method to write interpreted data
/// to the underlying writer.
///
/// Best used with [tap][crate::tap] utilities.
#[derive(Debug)]
pub struct PulseDecodeWriter<W> {
    state: PulseDecodeState,
    wr: W,
}

impl PulseDecodeState {
    /// Returns `true` if the state is [PulseDecodeState::Idle].
    pub fn is_idle(&self) -> bool {
        match self {
            PulseDecodeState::Idle => true,
            _ => false
        }
    }
    /// Returns `true` if receiving lead pulses.
    pub fn is_lead(&self) -> bool {
        match self {
            PulseDecodeState::Lead {..} => true,
            _ => false
        }
    }
    /// Returns `true` if receiving data pulses.
    pub fn is_data(&self) -> bool {
        match self {
            PulseDecodeState::Data {..} => true,
            _ => false
        }
    }
    /// Returns `true` if received sync1 pulse.
    pub fn is_sync1(&self) -> bool {
        match self {
            PulseDecodeState::Sync1 => true,
            _ => false
        }
    }
    /// Returns `true` if received sync2 pulse.
    pub fn is_sync2(&self) -> bool {
        match self {
            PulseDecodeState::Sync2 => true,
            _ => false
        }
    }
}

impl<W> PulseDecodeWriter<W> {
    /// Resets the state of the [PulseDecodeWriter] to `Idle`, discarding any partially received byte.
    ///
    /// The information about the number of bytes written so far is lost.
    pub fn reset(&mut self) {
        self.state = PulseDecodeState::Idle;
    }
    /// Returns `true` if the state is [PulseDecodeState::Idle].
    pub fn is_idle(&self) -> bool {
        self.state.is_idle()
    }
    /// Returns the underlying writer.
    pub fn into_inner(self) -> W {
        self.wr
    }
    /// Returns a reference to the current state.
    pub fn state(&self) -> PulseDecodeState {
        self.state
    }
    /// Returns a mutable reference to the inner writer.
    pub fn get_mut(&mut self) -> &mut W {
        &mut self.wr
    }
    /// Returns a shared reference to the inner writer.
    pub fn get_ref(&self) -> &W {
        &self.wr
    }
    /// Returns a number of bytes written during current data transfer
    /// if a data transfer is in progress.
    pub fn data_size(&self) -> Option<NonZeroU32> {
        NonZeroU32::new(if let PulseDecodeState::Data { written, .. } = self.state {
            written
        }
        else {
            0
        })
    }
    /// Allows to manually assign `state`.
    /// Can be used to deserialize PulseDecodeWriter.
    pub fn with_state(mut self, state: PulseDecodeState) -> Self {
        self.state = state;
        self
    }
}

impl<W: Write> PulseDecodeWriter<W> {
    /// Creates a new `PulseDecodeWriter` from a given [Writer][Write].
    pub fn new(wr: W) -> Self {
        PulseDecodeWriter { state: PulseDecodeState::Idle, wr }
    }
    /// Optionally writes a partially received data byte and ensures the state of [PulseDecodeWriter] is `Idle`.
    ///
    /// After calling this method, regardless of the return value, the state is changed to [PulseDecodeState::Idle].
    ///
    /// Returns `Ok(None)` if there was no data written in the current transfer.
    ///
    /// Returns `Ok(Some(size))` if data has been written, providing the number of written bytes.
    ///
    /// In the case of [std::io::Error] the information about the number of bytes written is lost.
    pub fn end(&mut self) -> Result<Option<NonZeroU32>> {
        let res = if let PulseDecodeState::Data { mut current, pulse, written } = self.state {
            NonZeroU32::new(if pulse <= 1 {
                written
            }
            else {
                if pulse & 1 == 1 {
                    current &= !1;
                }
                current <<= (16 - (pulse & 15)) >> 1;
                self.write_byte(current)?;
                written + 1
            })
        }
        else {
            None
        };
        self.state = PulseDecodeState::Idle;
        Ok(res)
    }
    /// Interprets pulse intervals from the provided pulse iterator as data bytes, writing them to
    /// the underlying writer.
    ///
    /// The pulse iterator is expected to provide only a fragment of pulses needed for the complete
    /// transfer such as an iterator returned from [MicOut::mic_out_pulse_iter].
    /// Providing an empty iterator is equivalent to calling [PulseDecodeWriter::end] thus ending
    /// the current transfer in progress if there is any.
    ///
    /// Returns `Ok(None)` if there was no data or more pulses are being expected.
    ///
    /// Returns `Ok(Some(size))` if data block has been written, providing the number of written bytes.
    /// In this instance, there can still be some pulses left in the iterator, e.g. for the next
    /// incoming transfer.
    ///
    /// In the case of [std::io::Error] the information about the number of bytes written is lost.
    ///
    /// [MicOut::mic_out_pulse_iter]: spectrusty_core::chip::MicOut::mic_out_pulse_iter
    pub fn write_decoded_pulses<I>(&mut self, iter: I) -> Result<Option<NonZeroU32>>
        where I: Iterator<Item=NonZeroU32>
    {
        let mut iter = iter.peekable();
        if iter.peek().is_none() {
            return self.end(); // empty frame
        }

        for delta in iter {
            match self.state {
                PulseDecodeState::Idle => match delta.get() {
                    LEAD_PULSE_MIN..=LEAD_PULSE_MAX => {
                        self.state = PulseDecodeState::Lead { counter: 1 };
                    }
                    _ => {},
                }
                PulseDecodeState::Lead { counter } => match delta.get() {
                    SYNC_PULSE1_MIN..=SYNC_PULSE1_MAX if counter >= MIN_LEAD_COUNT => {
                        self.state = PulseDecodeState::Sync1;
                    }
                    LEAD_PULSE_MIN..=LEAD_PULSE_MAX => {
                        self.state = PulseDecodeState::Lead { counter: counter.saturating_add(1) };
                    }
                    _ => { self.state = PulseDecodeState::Idle },
                }
                PulseDecodeState::Sync1 => match delta.get() {
                    SYNC_PULSE2_MIN..=SYNC_PULSE2_MAX => {
                        self.state = PulseDecodeState::Sync2;
                    }                    
                    _ => { self.state = PulseDecodeState::Idle },
                }
                PulseDecodeState::Sync2 => match delta.get() {
                    delta @ DATA_PULSE_MIN..=DATA_PULSE_MAX => {
                        let current: u8 = if delta > DATA_PULSE_THRESHOLD { 1 } else { 0 };
                        self.state = PulseDecodeState::Data { current, pulse: 1, written: 0 }
                    }
                    _ => { self.state = PulseDecodeState::Idle },
                }
                PulseDecodeState::Data { current, pulse, written } => match delta.get() {
                    delta @ DATA_PULSE_MIN..=DATA_PULSE_MAX => {
                        let bit: u8 = if delta > DATA_PULSE_THRESHOLD { 1 } else { 0 };
                        let current = if pulse & 1 == 1 {
                            if (current ^ bit) & 1 == 1 {
                                return self.end();
                            }
                            if pulse == 15 {
                                self.state = PulseDecodeState::Data { current: 0, pulse: 0, written: written + 1 };
                                self.write_byte(current)?;
                                continue;
                            }
                            current
                        }
                        else {
                            (current << 1) | bit
                        };
                        self.state = PulseDecodeState::Data { current, pulse: pulse + 1, written }
                    }
                    _ => return self.end()
                }
            }
        }
        Ok(None)
    }

    #[inline]
    fn write_byte(&mut self, byte: u8) -> Result<()> {
        // eprint!("{:02x} ", byte);
        if let Err(e) = self.wr.write_all(slice::from_ref(&byte)) {
            self.state = PulseDecodeState::Idle;
            return Err(e);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn write_mic_works() {
        let wr = Cursor::new(Vec::new());
        let mut mpw = PulseDecodeWriter::new(wr);
        let pulses = [];
        assert_eq!(PulseDecodeState::Idle, mpw.state());
        assert_eq!(None, mpw.write_decoded_pulses(&mut pulses.iter().copied()).unwrap());
        assert_eq!(PulseDecodeState::Idle, mpw.state());
        assert_eq!(None, mpw.data_size());
        let pulses = [PAUSE_PULSE_LENGTH, LEAD_PULSE_LENGTH];
        assert_eq!(None, mpw.write_decoded_pulses(&mut pulses.iter().copied()).unwrap());
        assert_eq!(PulseDecodeState::Lead {counter:1} , mpw.state());
        assert_eq!(None, mpw.data_size());
        let pulses = vec![LEAD_PULSE_LENGTH;LEAD_PULSES_HEAD as usize];
        assert_eq!(None, mpw.write_decoded_pulses(&mut pulses.into_iter()).unwrap());
        assert_eq!(PulseDecodeState::Lead {counter:LEAD_PULSES_HEAD as u32 + 1} , mpw.state());
        assert_eq!(None, mpw.data_size());
        let pulses = [SYNC_PULSE1_LENGTH, SYNC_PULSE2_LENGTH,
                      ZERO_PULSE_LENGTH, ZERO_PULSE_LENGTH,
                      ONE_PULSE_LENGTH, ONE_PULSE_LENGTH];
        assert_eq!(None, mpw.write_decoded_pulses(&mut pulses.iter().copied()).unwrap());
        assert_eq!(PulseDecodeState::Data {current: 0b0000_0001, pulse: 4, written: 0} , mpw.state());
        assert_eq!(None, mpw.data_size());
        assert_eq!(0, mpw.get_ref().position());
        let pulses = [ZERO_PULSE_LENGTH, ZERO_PULSE_LENGTH,
                      ONE_PULSE_LENGTH, ONE_PULSE_LENGTH,
                      ZERO_PULSE_LENGTH, ZERO_PULSE_LENGTH,
                      ONE_PULSE_LENGTH, ONE_PULSE_LENGTH,
                      ZERO_PULSE_LENGTH, ZERO_PULSE_LENGTH,
                      ONE_PULSE_LENGTH, ONE_PULSE_LENGTH];
        assert_eq!(None, mpw.write_decoded_pulses(&mut pulses.iter().copied()).unwrap());
        assert_eq!(PulseDecodeState::Data {current: 0, pulse: 0, written: 1} , mpw.state());
        assert_eq!(NonZeroU32::new(1), mpw.data_size());
        assert_eq!(1, mpw.get_ref().position());
        assert_eq!(&[0b0101_0101], &mpw.get_ref().get_ref()[..]);
        assert_eq!(NonZeroU32::new(1), mpw.end().unwrap());
        assert_eq!(PulseDecodeState::Idle, mpw.state());
        assert_eq!(None, mpw.data_size());
    }

    #[test]
    fn write_mic_missing_bits_works() {
        let wr = Cursor::new(Vec::new());
        let mut mpw = PulseDecodeWriter::new(wr);
        let pulses = vec![LEAD_PULSE_LENGTH;LEAD_PULSES_DATA as usize];
        assert_eq!(None, mpw.write_decoded_pulses(&mut pulses.into_iter()).unwrap());
        assert_eq!(PulseDecodeState::Lead {counter:LEAD_PULSES_DATA as u32} , mpw.state());
        assert_eq!(None, mpw.data_size());
        let pulses = [SYNC_PULSE1_LENGTH, SYNC_PULSE2_LENGTH,
                      ONE_PULSE_LENGTH, ONE_PULSE_LENGTH,
                      ZERO_PULSE_LENGTH, ZERO_PULSE_LENGTH,
                      ONE_PULSE_LENGTH];
        assert_eq!(None, mpw.write_decoded_pulses(&mut pulses.iter().copied()).unwrap());
        assert_eq!(PulseDecodeState::Data {current: 0b0000_0101, pulse: 5, written: 0} , mpw.state());
        assert_eq!(None, mpw.data_size());
        assert_eq!(NonZeroU32::new(1), mpw.end().unwrap());
        assert_eq!(1, mpw.get_ref().position());
        assert_eq!(&[0b1000_0000], &mpw.get_ref().get_ref()[..]);
        assert_eq!(PulseDecodeState::Idle, mpw.state());
        assert_eq!(None, mpw.data_size());
    }
}
