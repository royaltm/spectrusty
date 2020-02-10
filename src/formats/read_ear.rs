//! **TAPE** pulse signal emulation.
#[allow(unused_imports)]
use core::num::NonZeroU32;
use std::io::{Error, Read};

/// Length of the lead pulse in T-states.
pub const LEAD_PULSE_LENGTH : NonZeroU32 = unsafe { NonZeroU32::new_unchecked(2168) };
/// Length of the 1st sync pulse in T-states.
pub const SYNC_PULSE1_LENGTH: NonZeroU32 = unsafe { NonZeroU32::new_unchecked(667)  };
/// Length of the 2nd sync pulse in T-states.
pub const SYNC_PULSE2_LENGTH: NonZeroU32 = unsafe { NonZeroU32::new_unchecked(735)  };
/// Length of the bit value 0 pulse in T-states.
pub const ZERO_PULSE_LENGTH : NonZeroU32 = unsafe { NonZeroU32::new_unchecked(855)  };
/// Length of the bit value 0 pulse in T-states.
pub const ONE_PULSE_LENGTH  : NonZeroU32 = unsafe { NonZeroU32::new_unchecked(1710) };
/// Length of the pause between *TAPE* blocks in T-states.
pub const PAUSE_PULSE_LENGTH: NonZeroU32 = unsafe { NonZeroU32::new_unchecked(3_500_000/2) };

/// The number of LEAD pulses for the header block.
pub const LEAD_PULSES_HEAD: u16 = 8063;
/// The number of LEAD pulses for the data block.
pub const LEAD_PULSES_DATA: u16 = 3223;

/// The state of the [EarPulseIter].
#[derive(Debug)]
pub enum IterState {
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
        /// A current data byte.
        /// The highest bit represent the last or current pulse being emitted.
        current: u8,
        /// A pulse counter for the current byte.
        /// There are two pulses per each bit (16 pulses per byte).
        pulse: u8 },
    /// Emitting is done.
    Done,
    /// There was an error from an underlying reader.
    Error(Error)
}

impl IterState {
    /// Returns an error from the underying reader if there was one.
    pub fn err(&self) -> Option<&Error> {
        match self {
            IterState::Error(ref error) => Some(error),
            _ => None
        }
    }
    /// Returns `true` if there are no more pulses to emit or there
    /// was an error while reading bytes.
    pub fn is_done(&self) -> bool {
        match self {
            IterState::Done|IterState::Error(_) => true,
            _ => false
        }
    }
    /// Returns `true` if emitting lead pulses.
    pub fn is_lead(&self) -> bool {
        match self {
            IterState::Lead {..} => true,
            _ => false
        }
    }
    /// Returns `true` if emitting data pulses.
    pub fn is_data(&self) -> bool {
        match self {
            IterState::Data {..} => true,
            _ => false
        }
    }
    /// Returns `true` if emitting sync1 pulse.
    pub fn is_sync1(&self) -> bool {
        match self {
            IterState::Sync1 => true,
            _ => false
        }
    }
    /// Returns `true` if emitting sync2 pulse.
    pub fn is_sync2(&self) -> bool {
        match self {
            IterState::Sync2 => true,
            _ => false
        }
    }
}

/// Encodes bytes read from an underlying reader as T-state pulse intervals as expected by ZX Spactrum's *TAPE*
/// loading ROM routines.
///
/// Expects that the underlying reader provides bytes in the *TAP* compatible format.
///
/// Best used with [tap][crate::formats::tap] utilites.
///
/// This iterator may be used to feed the pulses to the `EAR in` buffer of the ZX Spectrum emulator
/// (e.g. with [EarIn::feed_ear_in][crate::audio::EarIn::feed_ear_in])
/// or to produce *TAPE* sound with a help of [Bandwidth-Limited Pulse Buffer](crate::audio::Blep).
#[derive(Debug)]
pub struct EarPulseIter<R> {
    rd: R,
    state: IterState,
    head: u8,
}

impl<R> EarPulseIter<R> {
    /// Returns the underlying reader.
    pub fn into_inner(self) -> R {
        self.rd
    }
}

impl<R: Read> EarPulseIter<R> {
    /// Creates a new `EarPulseIter` from the given [Reader][Read].
    pub fn new(rd: R) -> Self {
        let mut epi = EarPulseIter { rd, state: IterState::Done, head: 0 };
        epi.reset();
        epi
    }
    /// Resets own state to [IterState::Lead] as if the next byte read from the inner reader was
    /// the first *TAP* byte (a flag).
    pub fn reset(&mut self) {
        let (head, state) = match self.rd.by_ref().bytes().next() {
            Some(Ok(head)) => (head, IterState::Lead {
                countdown: if head & 0x80 == 0 {
                    LEAD_PULSES_HEAD
                } else {
                    LEAD_PULSES_DATA
                }
            }),
            Some(Err(error)) => (0, IterState::Error(error)),
            None => (0, IterState::Done)
        };
        self.head = head;
        self.state = state;
    }
    /// Returns an error from the underying reader if there was one.
    pub fn err(&self) -> Option<&Error> {
        self.state.err()
    }
    /// Returns a reference to the current state.
    pub fn state(&self) -> &IterState {
        &self.state
    }
    /// Returns `true` if there are no more pulses to emit or there
    /// was an error while reading bytes.
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
}

impl<R: Read> Iterator for EarPulseIter<R> {
    type Item = NonZeroU32;
    fn next(&mut self) -> Option<NonZeroU32> {
        match self.state {
            IterState::Lead {ref mut countdown} => {
                match *countdown - 1 {
                    0 => {
                        self.state = IterState::Sync1
                    }
                    res => {
                        *countdown = res
                    }
                }
                Some(LEAD_PULSE_LENGTH)
            }
            IterState::Sync1 => {
                self.state = IterState::Sync2;
                Some(SYNC_PULSE1_LENGTH)
            }
            IterState::Sync2 => {
                self.state = IterState::Data { current: self.head, pulse: 0 };
                Some(SYNC_PULSE2_LENGTH)
            }
            IterState::Data { ref mut current, ref mut pulse } => {
                let bit_one: bool = *current & 0x80 != 0;
                if *pulse == 15 {
                    self.state = match self.rd.by_ref().bytes().next() {
                        Some(Ok(current)) => IterState::Data { current, pulse: 0 },
                        Some(Err(error)) => IterState::Error(error),
                        None => IterState::Done
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
    fn read_ear_work() {
        let data = [0xFF, 0xA5, 0x00];
        let mut iter = EarPulseIter::new(Cursor::new(data));
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
        let mut iter = EarPulseIter::new(Cursor::new(data));
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

        let mut iter = EarPulseIter::new(Cursor::new([]));
        assert_eq!(true, iter.is_done());
        assert_eq!(None, iter.next());
    }
}
