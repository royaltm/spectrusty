#[allow(unused_imports)]
use core::num::NonZeroU32;
use std::io::{Error, Read};

pub const LEAD_PULSE_LENGTH : NonZeroU32 = unsafe { NonZeroU32::new_unchecked(2168) };
pub const SYNC_PULSE1_LENGTH: NonZeroU32 = unsafe { NonZeroU32::new_unchecked(667)  };
pub const SYNC_PULSE2_LENGTH: NonZeroU32 = unsafe { NonZeroU32::new_unchecked(735)  };
pub const ZERO_PULSE_LENGTH : NonZeroU32 = unsafe { NonZeroU32::new_unchecked(855)  };
pub const ONE_PULSE_LENGTH  : NonZeroU32 = unsafe { NonZeroU32::new_unchecked(1710) };
pub const PAUSE_PULSE_LENGTH: NonZeroU32 = unsafe { NonZeroU32::new_unchecked(3_500_000/2) };

pub const LEAD_PULSES_HEAD: u16 = 8063;
pub const LEAD_PULSES_DATA: u16 = 3223;


#[derive(Debug)]
pub enum IterState {
    Lead{ countdown: u16 },
    Sync1,
    Sync2,
    Data{ current: u8, pulse: u8 },
    Done,
    Error(Error)
}

impl IterState {
    pub fn err(&self) -> Option<&Error> {
        match self {
            IterState::Error(ref error) => Some(error),
            _ => None
        }
    }
    pub fn is_done(&self) -> bool {
        match self {
            IterState::Done|IterState::Error(_) => true,
            _ => false
        }
    }
    pub fn is_lead(&self) -> bool {
        match self {
            IterState::Lead {..} => true,
            _ => false
        }
    }
    pub fn is_data(&self) -> bool {
        match self {
            IterState::Data {..} => true,
            _ => false
        }
    }
    pub fn is_sync1(&self) -> bool {
        match self {
            IterState::Sync1 => true,
            _ => false
        }
    }
    pub fn is_sync2(&self) -> bool {
        match self {
            IterState::Sync2 => true,
            _ => false
        }
    }
}

#[derive(Debug)]
pub struct EarPulseIter<R> {
    rd: R,
    state: IterState,
    head: u8,
}

impl<R> EarPulseIter<R> {
    pub fn into_inner(self) -> R {
        self.rd
    }
}

impl<R: Read> EarPulseIter<R> {
    pub fn new(rd: R) -> Self {
        let mut epi = EarPulseIter { rd, state: IterState::Done, head: 0 };
        epi.reset();
        epi
    }

    /// Resets pulse state as if the next byte read from the inner reader was the tap chunk flag byte.
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

    pub fn err(&self) -> Option<&Error> {
        self.state.err()
    }

    pub fn state(&self) -> &IterState {
        &self.state
    }

    pub fn is_done(&self) -> bool {
        self.state.is_done()
    }

    pub fn get_mut(&mut self) -> &mut R {
        &mut self.rd
    }

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
