use core::fmt;
use crate::clock::VideoTs;
use crate::peripherals::ay::{AyIoPort, AyIoNullPort, Ay128kPortDecode};
use crate::bus::ay::{Ay3_891xBusDevice};

// <- TODO: keypad and serport impl...
pub type Ay3_8912SerKeypad<D> = Ay3_891xBusDevice<VideoTs,
                                                  Ay128kPortDecode,
                                                  SerialKeypad,
                                                  AyIoNullPort<VideoTs>, D>;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SerialKeypad {
    data: u8
}

impl Default for SerialKeypad {
    fn default() -> Self {
        SerialKeypad { data: 0xff }
    }
}

impl AyIoPort for SerialKeypad {
    type Timestamp = VideoTs;
    /// Resets the device at the given `timestamp`.
    #[inline]
    fn ay_io_reset(&mut self, _timestamp: Self::Timestamp) {
        self.data = 0xff;
    }
    /// Writes value to the device at the given `timestamp`.
    #[inline]
    fn ay_io_write(&mut self, _addr: u16, _val: u8, _timestamp: Self::Timestamp) {
        self.data = 0xff;
    }
    /// Reads value to the device at the given `timestamp`.
    #[inline]
    fn ay_io_read(&mut self, _addr: u16, _timestamp: Self::Timestamp) -> u8 {
        self.data
    }
    /// Signals the device when the current frame ends.
    #[inline]
    fn end_frame(&mut self, _timestamp: Self::Timestamp) {

    }
}
