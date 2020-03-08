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

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PulseState {
    /// Idle state, no synchronization yet.
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
        /// A current data byte.
        current: u8,
        /// A pulse counter for the current byte.
        pulse: u8,
        /// How many byttes written.
        written: u32
    },
}

#[derive(Debug)]
pub struct MicPulseWriter<W> {
    state: PulseState,
    wr: W,
}

impl<W> MicPulseWriter<W> {
    /// Resets to idle state.
    pub fn reset(&mut self) {
        self.state = PulseState::Idle;
    }
    /// Returns the underlying writer.
    pub fn into_inner(self) -> W {
        self.wr
    }
    /// Returns a reference to the current state.
    pub fn state(&self) -> PulseState {
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
    /// Returns an optional number of bytes written if some bytes were written
    /// up to this moment and current data wasn't flushed yet.
    pub fn data_size(&self) -> Option<NonZeroU32> {
        NonZeroU32::new(if let PulseState::Data { written, .. } = self.state {
            written
        }
        else {
            0
        })
    }
}

impl<W> MicPulseWriter<W>
    where W: Write
{
    /// Creates a new `MicPulseWriter` from a given [Writer][Write].
    pub fn new(wr: W) -> Self {
        MicPulseWriter { state: PulseState::Idle, wr }
    }
    /// Optionally flushes a current data byte as if there will be no more pulses.
    ///
    /// Returns `Ok(None)` if there was no data.
    ///
    /// Returns `Ok(Some(size))` if data block has been written, providing the number of bytes written.
    ///
    /// In case of [std::io::Error] the information about number of bytes written is lost.
    pub fn flush(&mut self) -> Result<Option<NonZeroU32>> {
        let res = if let PulseState::Data { mut current, pulse, written } = self.state {
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
        self.state = PulseState::Idle;
        Ok(res)
    }
    /// Interprets pulses from the provided frame pulse iterator as *TAPE* bytes and writes them
    /// to the underlying writer.
    ///
    /// Returns `Ok(None)` if there was no data or more pulses are being expected.
    ///
    /// Returns `Ok(Some(size))` if data block has been written, providing the number of bytes written.
    /// In this instance there can still be some pulses left in the iterator.
    ///
    /// In case of [std::io::Error] the information about number of bytes written is lost.
    pub fn write_pulses_as_bytes<I>(&mut self, iter: &mut I) -> Result<Option<NonZeroU32>>
        where I: Iterator<Item=NonZeroU32>
    {
        let mut iter = iter.peekable();
        if iter.peek().is_none() {
            return self.flush(); // empty frame
        }

        for delta in iter {
            match self.state {
                PulseState::Idle => match delta.get() {
                    LEAD_PULSE_MIN..=LEAD_PULSE_MAX => {
                        self.state = PulseState::Lead { counter: 1 };
                    }
                    _ => {},
                }
                PulseState::Lead { counter } => match delta.get() {
                    SYNC_PULSE1_MIN..=SYNC_PULSE1_MAX if counter >= MIN_LEAD_COUNT => {
                        self.state = PulseState::Sync1;
                    }
                    LEAD_PULSE_MIN..=LEAD_PULSE_MAX => {
                        self.state = PulseState::Lead { counter: counter.saturating_add(1) };
                    }
                    _ => { self.state = PulseState::Idle },
                }
                PulseState::Sync1 => match delta.get() {
                    SYNC_PULSE2_MIN..=SYNC_PULSE2_MAX => {
                        self.state = PulseState::Sync2;
                    }                    
                    _ => { self.state = PulseState::Idle },
                }
                PulseState::Sync2 => match delta.get() {
                    delta @ DATA_PULSE_MIN..=DATA_PULSE_MAX => {
                        let current: u8 = if delta > DATA_PULSE_THRESHOLD { 1 } else { 0 };
                        self.state = PulseState::Data { current, pulse: 1, written: 0 }
                    }
                    _ => { self.state = PulseState::Idle },
                }
                PulseState::Data { current, pulse, written } => match delta.get() {
                    delta @ DATA_PULSE_MIN..=DATA_PULSE_MAX => {
                        let bit: u8 = if delta > DATA_PULSE_THRESHOLD { 1 } else { 0 };
                        let current = if pulse & 1 == 1 {
                            if (current ^ bit) & 1 == 1 {
                                return self.flush();
                            }
                            if pulse == 15 {
                                self.state = PulseState::Data { current: 0, pulse: 0, written: written + 1 };
                                self.write_byte(current)?;
                                continue;
                            }
                            current
                        }
                        else {
                            (current << 1) | bit
                        };
                        self.state = PulseState::Data { current, pulse: pulse + 1, written }
                    }
                    _ => return self.flush()
                }
            }
        }
        Ok(None)
    }

    #[inline]
    fn write_byte(&mut self, byte: u8) -> Result<()> {
        // eprint!("{:02x} ", byte);
        if let Err(e) = self.wr.write_all(slice::from_ref(&byte)) {
            self.state = PulseState::Idle;
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
        let mut mpw = MicPulseWriter::new(wr);
        let pulses = [];
        assert_eq!(PulseState::Idle, mpw.state());
        assert_eq!(None, mpw.write_pulses_as_bytes(&mut pulses.iter().copied()).unwrap());
        assert_eq!(PulseState::Idle, mpw.state());
        assert_eq!(None, mpw.data_size());
        let pulses = [PAUSE_PULSE_LENGTH, LEAD_PULSE_LENGTH];
        assert_eq!(None, mpw.write_pulses_as_bytes(&mut pulses.iter().copied()).unwrap());
        assert_eq!(PulseState::Lead {counter:1} , mpw.state());
        assert_eq!(None, mpw.data_size());
        let pulses = vec![LEAD_PULSE_LENGTH;LEAD_PULSES_HEAD as usize];
        assert_eq!(None, mpw.write_pulses_as_bytes(&mut pulses.into_iter()).unwrap());
        assert_eq!(PulseState::Lead {counter:LEAD_PULSES_HEAD as u32 + 1} , mpw.state());
        assert_eq!(None, mpw.data_size());
        let pulses = [SYNC_PULSE1_LENGTH, SYNC_PULSE2_LENGTH,
                      ZERO_PULSE_LENGTH, ZERO_PULSE_LENGTH,
                      ONE_PULSE_LENGTH, ONE_PULSE_LENGTH];
        assert_eq!(None, mpw.write_pulses_as_bytes(&mut pulses.iter().copied()).unwrap());
        assert_eq!(PulseState::Data {current: 0b0000_0001, pulse: 4, written: 0} , mpw.state());
        assert_eq!(None, mpw.data_size());
        assert_eq!(0, mpw.get_ref().position());
        let pulses = [ZERO_PULSE_LENGTH, ZERO_PULSE_LENGTH,
                      ONE_PULSE_LENGTH, ONE_PULSE_LENGTH,
                      ZERO_PULSE_LENGTH, ZERO_PULSE_LENGTH,
                      ONE_PULSE_LENGTH, ONE_PULSE_LENGTH,
                      ZERO_PULSE_LENGTH, ZERO_PULSE_LENGTH,
                      ONE_PULSE_LENGTH, ONE_PULSE_LENGTH];
        assert_eq!(None, mpw.write_pulses_as_bytes(&mut pulses.iter().copied()).unwrap());
        assert_eq!(PulseState::Data {current: 0, pulse: 0, written: 1} , mpw.state());
        assert_eq!(NonZeroU32::new(1), mpw.data_size());
        assert_eq!(1, mpw.get_ref().position());
        assert_eq!(&[0b0101_0101], &mpw.get_ref().get_ref()[..]);
        assert_eq!(NonZeroU32::new(1), mpw.flush().unwrap());
        assert_eq!(PulseState::Idle, mpw.state());
        assert_eq!(None, mpw.data_size());
    }

    #[test]
    fn write_mic_missing_bits_works() {
        let wr = Cursor::new(Vec::new());
        let mut mpw = MicPulseWriter::new(wr);
        let pulses = vec![LEAD_PULSE_LENGTH;LEAD_PULSES_DATA as usize];
        assert_eq!(None, mpw.write_pulses_as_bytes(&mut pulses.into_iter()).unwrap());
        assert_eq!(PulseState::Lead {counter:LEAD_PULSES_DATA as u32} , mpw.state());
        assert_eq!(None, mpw.data_size());
        let pulses = [SYNC_PULSE1_LENGTH, SYNC_PULSE2_LENGTH,
                      ONE_PULSE_LENGTH, ONE_PULSE_LENGTH,
                      ZERO_PULSE_LENGTH, ZERO_PULSE_LENGTH,
                      ONE_PULSE_LENGTH];
        assert_eq!(None, mpw.write_pulses_as_bytes(&mut pulses.iter().copied()).unwrap());
        assert_eq!(PulseState::Data {current: 0b0000_0101, pulse: 5, written: 0} , mpw.state());
        assert_eq!(None, mpw.data_size());
        assert_eq!(NonZeroU32::new(1), mpw.flush().unwrap());
        assert_eq!(1, mpw.get_ref().position());
        assert_eq!(&[0b1000_0000], &mpw.get_ref().get_ref()[..]);
        assert_eq!(PulseState::Idle, mpw.state());
        assert_eq!(None, mpw.data_size());
    }
}
