/*
    Copyright (C) 2020-2022  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
use core::num::NonZeroU32;
use std::io::{Error, Read};
use super::consts::*;

/// The current state of the [ReadEncPulseIter].
#[derive(Debug)]
pub enum PulseIterState {
    /// Emitting lead pulses.
    Lead{
        /// How many pulses left to the end of this lead.
        countdown: u16
    },
    /// Emitting the 1st sync pulse.
    Sync1,
    /// Emitting the 2nd sync pulse.
    Sync2,
    /// Emitting data pulses.
    Data{
        /// A current byte.
        /// The highest bit determines the last (`pulse` is odd) or next (`pulse` is even) pulse being emitted.
        current: u8,
        /// A pulse counter for the current byte.
        /// There are two pulses per each bit (16 pulses per byte).
        pulse: u8 },
    /// Emitting is done.
    Done,
    /// There was an error from an underlying reader.
    Error(Error)
}

/// Encodes data read from an underlying reader as *TAPE* T-state pulse intervals via an [Iterator] interface.
///
/// The timing of the pulses matches those expected by ZX Spectrum's ROM loading routines.
///
/// After invoking [ReadEncPulseIter::reset] or [ReadEncPulseIter::new] the first byte is read and checked
/// to determine the duration of the *LEAD PULSE* signal. If it's less than 128 the number of 
/// generated lead pulses is [LEAD_PULSES_HEAD]. Otherwise, it's [LEAD_PULSES_DATA].
///
/// After the lead pulses, two synchronization pulses are being emitted following by data pulses
/// for each byte read including the initial flag byte.
///
/// This iterator may be used to feed pulses to the `EAR IN` buffer of the ZX Spectrum emulator
/// (e.g. via [EarIn::feed_ear_in][spectrusty_core::chip::EarIn::feed_ear_in])
/// or to produce sound with a help of [Bandwidth-Limited Pulse Buffer][spectrusty_core::audio::Blep].
///
/// Best used with [tap][crate::tap] utilities.
#[derive(Debug)]
pub struct ReadEncPulseIter<R> {
    rd: R,
    state: PulseIterState,
    flag: u8,
}

impl PulseIterState {
    /// Returns an error from the underlying reader if there was one.
    pub fn err(&self) -> Option<&Error> {
        match self {
            PulseIterState::Error(ref error) => Some(error),
            _ => None
        }
    }
    /// Returns `true` if there are no more pulses to emit or there
    /// was an error while reading bytes.
    pub fn is_done(&self) -> bool {
        matches!(self, PulseIterState::Done|PulseIterState::Error(..))
    }
    /// Returns `true` if emitting lead pulses.
    pub fn is_lead(&self) -> bool {
        matches!(self, PulseIterState::Lead {..})
    }
    /// Returns `true` if emitting data pulses.
    pub fn is_data(&self) -> bool {
        matches!(self, PulseIterState::Data {..})
    }
    /// Returns `true` if emitting sync1 pulse.
    pub fn is_sync1(&self) -> bool {
        matches!(self, PulseIterState::Sync1)
    }
    /// Returns `true` if emitting sync2 pulse.
    pub fn is_sync2(&self) -> bool {
        matches!(self, PulseIterState::Sync2)
    }
}

impl<R> ReadEncPulseIter<R> {
    /// Returns a reference to the current state.
    pub fn state(&self) -> &PulseIterState {
        &self.state
    }
    /// Returns a flag byte.
    pub fn flag(&self) -> u8 {
        self.flag
    }
    /// Returns an error from the underlying reader if there was one.
    pub fn err(&self) -> Option<&Error> {
        self.state.err()
    }
    /// Returns `true` if there are no more pulses to emit or there
    /// was an error while bytes were read.
    pub fn is_done(&self) -> bool {
        self.state.is_done()
    }
    /// Returns a mutable reference to the inner reader.
    pub fn get_mut(&mut self) -> &mut R {
        &mut self.rd
    }
    /// Returns a shared reference to the inner reader.
    pub fn get_ref(&self) -> &R {
        &self.rd
    }
    /// Returns the underlying reader.
    pub fn into_inner(self) -> R {
        self.rd
    }
    /// Allows to manually assign a `state` and a `flag`.
    /// Can be used to deserialize ReadEncPulseIter.
    pub fn with_state_and_flag(mut self, state: PulseIterState, flag: u8) -> Self {
        self.state = state;
        self.flag = flag;
        self
    }
}

impl<R: Read> ReadEncPulseIter<R> {
    /// Creates a new `ReadEncPulseIter` from a given [Reader][Read].
    pub fn new(rd: R) -> Self {
        let mut epi = ReadEncPulseIter { rd, state: PulseIterState::Done, flag: 0 };
        epi.reset();
        epi
    }
    /// Resets the state of the iterator.
    ///
    /// The next byte read from the inner reader is interpreted as a flag byte to determine
    /// the number of lead pulses. In this instance the `state` becomes [PulseIterState::Lead].
    ///
    /// If there are no more bytes to be read the `state` becomes [PulseIterState::Done].
    ///
    /// In case of an error while reading from the underlying reader the `state` becomes [PulseIterState::Error].
    pub fn reset(&mut self) {
        let (flag, state) = match self.rd.by_ref().bytes().next() {
            Some(Ok(flag)) => (flag, PulseIterState::Lead {
                countdown: if flag & 0x80 == 0 {
                    LEAD_PULSES_HEAD
                } else {
                    LEAD_PULSES_DATA
                }
            }),
            Some(Err(error)) => (0, PulseIterState::Error(error)),
            None => (0, PulseIterState::Done)
        };
        self.flag = flag;
        self.state = state;
    }
    /// Attempts to set the state of the iterator as [PulseIterState::Data] from the next byte.
    ///
    /// The next byte read from the inner reader is interpreted as a data byte.
    /// In this instance, the `state` becomes [PulseIterState::Data].
    ///
    /// If there are no more bytes to be read the `state` becomes [PulseIterState::Done].
    ///
    /// In case of an error while reading from the underlying reader the `state` becomes [PulseIterState::Error].
    pub fn data_from_next(&mut self) {
        self.state = match self.rd.by_ref().bytes().next() {
            Some(Ok(current)) => PulseIterState::Data { current, pulse: 0 },
            Some(Err(error)) => PulseIterState::Error(error),
            None => PulseIterState::Done
        };
    }
}

impl<R: Read> Iterator for ReadEncPulseIter<R> {
    type Item = NonZeroU32;
    fn next(&mut self) -> Option<NonZeroU32> {
        match self.state {
            PulseIterState::Lead {ref mut countdown} => {
                match *countdown - 1 {
                    0 => {
                        self.state = PulseIterState::Sync1
                    }
                    res => {
                        *countdown = res
                    }
                }
                Some(LEAD_PULSE_LENGTH)
            }
            PulseIterState::Sync1 => {
                self.state = PulseIterState::Sync2;
                Some(SYNC_PULSE1_LENGTH)
            }
            PulseIterState::Sync2 => {
                self.state = PulseIterState::Data { current: self.flag, pulse: 0 };
                Some(SYNC_PULSE2_LENGTH)
            }
            PulseIterState::Data { ref mut current, ref mut pulse } => {
                let bit_one: bool = *current & 0x80 != 0;
                if *pulse == 15 {
                    self.state = match self.rd.by_ref().bytes().next() {
                        Some(Ok(current)) => PulseIterState::Data { current, pulse: 0 },
                        Some(Err(error)) => PulseIterState::Error(error),
                        None => PulseIterState::Done
                    };
                }
                else {
                    if *pulse & 1 == 1 {
                        *current = current.rotate_left(1);
                    }
                    *pulse += 1;
                }
                Some(if bit_one { ONE_PULSE_LENGTH } else { ZERO_PULSE_LENGTH })
            }
            _ => None
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn read_enc_pulse_iter_works() {
        let data = [0xFF, 0xA5, 0x00];
        let mut iter = ReadEncPulseIter::new(Cursor::new(data));
        assert_eq!(false, iter.is_done());
        for delta in iter.by_ref().take(LEAD_PULSES_DATA as usize) {
            assert_eq!(LEAD_PULSE_LENGTH, delta);
        }
        assert_eq!(false, iter.is_done());
        assert_eq!(Some(SYNC_PULSE1_LENGTH), iter.next());
        assert_eq!(Some(SYNC_PULSE2_LENGTH), iter.next());
        assert_eq!(false, iter.is_done());
        assert_eq!(vec![
            ONE_PULSE_LENGTH, ONE_PULSE_LENGTH, ONE_PULSE_LENGTH, ONE_PULSE_LENGTH, 
            ONE_PULSE_LENGTH, ONE_PULSE_LENGTH, ONE_PULSE_LENGTH, ONE_PULSE_LENGTH, 
            ONE_PULSE_LENGTH, ONE_PULSE_LENGTH, ONE_PULSE_LENGTH, ONE_PULSE_LENGTH, 
            ONE_PULSE_LENGTH, ONE_PULSE_LENGTH, ONE_PULSE_LENGTH, ONE_PULSE_LENGTH, 

            ONE_PULSE_LENGTH, ONE_PULSE_LENGTH, ZERO_PULSE_LENGTH, ZERO_PULSE_LENGTH,
            ONE_PULSE_LENGTH, ONE_PULSE_LENGTH, ZERO_PULSE_LENGTH, ZERO_PULSE_LENGTH,
            ZERO_PULSE_LENGTH, ZERO_PULSE_LENGTH, ONE_PULSE_LENGTH, ONE_PULSE_LENGTH,
            ZERO_PULSE_LENGTH, ZERO_PULSE_LENGTH, ONE_PULSE_LENGTH, ONE_PULSE_LENGTH,

            ZERO_PULSE_LENGTH, ZERO_PULSE_LENGTH, ZERO_PULSE_LENGTH, ZERO_PULSE_LENGTH, 
            ZERO_PULSE_LENGTH, ZERO_PULSE_LENGTH, ZERO_PULSE_LENGTH, ZERO_PULSE_LENGTH, 
            ZERO_PULSE_LENGTH, ZERO_PULSE_LENGTH, ZERO_PULSE_LENGTH, ZERO_PULSE_LENGTH, 
            ZERO_PULSE_LENGTH, ZERO_PULSE_LENGTH, ZERO_PULSE_LENGTH, ZERO_PULSE_LENGTH, 
        ], iter.by_ref().collect::<Vec<_>>());
        assert_eq!(true, iter.is_done());

        let data = [0x00];
        let mut iter = ReadEncPulseIter::new(Cursor::new(data));
        assert_eq!(false, iter.is_done());
        for delta in iter.by_ref().take(LEAD_PULSES_HEAD as usize) {
            assert_eq!(LEAD_PULSE_LENGTH, delta);
        }
        assert_eq!(false, iter.is_done());
        assert_eq!(Some(SYNC_PULSE1_LENGTH), iter.next());
        assert_eq!(Some(SYNC_PULSE2_LENGTH), iter.next());
        assert_eq!(false, iter.is_done());
        assert_eq!(vec![ZERO_PULSE_LENGTH; 16], iter.by_ref().collect::<Vec<_>>());
        assert_eq!(true, iter.is_done());

        let mut iter = ReadEncPulseIter::new(Cursor::new([]));
        assert_eq!(true, iter.is_done());
        assert_eq!(None, iter.next());
    }
}
