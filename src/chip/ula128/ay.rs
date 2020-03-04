// http://www.fruitcake.plus.com/Sinclair/Spectrum128/Keypad/
use core::fmt::{self, Debug};
use core::marker::PhantomData;
use crate::clock::{FTs, VideoTs, Ts};
use crate::peripherals::ay::{AyIoPort, AyIoNullPort, Ay128kPortDecode};
use crate::peripherals::serial::{SerialPort, DataState, ControlState, NullSerialPort, SerialKeypad, SerPortIo};
use crate::bus::ay::{Ay3_891xBusDevice};
use rand::{Rng, SeedableRng};
use rand::rngs::SmallRng;
use super::video::Ula128VidFrame;
use crate::video::VideoFrame;
// <- TODO: serport impl...
pub type Ay3_8912Keypad<D> = Ay3_891xBusDevice<VideoTs,
                                                Ay128kPortDecode,
                                                SerialPorts128<
                                                    SerialKeypad<Ula128VidFrame>,
                                                    NullSerialPort<VideoTs>
                                                >,
                                                AyIoNullPort<VideoTs>, D>;

pub type Ay3_8912RS232<D, R, W> = Ay3_891xBusDevice<VideoTs,
                                                Ay128kPortDecode,
                                                SerialPorts128<
                                                    NullSerialPort<VideoTs>,
                                                    SerPortIo<Ula128VidFrame, R, W>
                                                >,
                                                AyIoNullPort<VideoTs>, D>;

pub type Ay3_8912KeypadRS232<D, R, W> = Ay3_891xBusDevice<VideoTs,
                                                Ay128kPortDecode,
                                                SerialPorts128<
                                                    SerialKeypad<Ula128VidFrame>,
                                                    SerPortIo<Ula128VidFrame, R, W>
                                                >,
                                                AyIoNullPort<VideoTs>, D>;

impl<D> fmt::Display for Ay3_8912Keypad<D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AY-3-8912 + Keypad")
    }
}

impl<D, R, W> fmt::Display for Ay3_8912RS232<D, R, W> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AY-3-8912 + RS-232")
    }
}

impl<D, R, W> fmt::Display for Ay3_8912KeypadRS232<D, R, W> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AY-3-8912 + Keypad + RS-232")
    }
}


#[derive(Clone, Debug, Default)]
pub struct SerialPorts128<S1,S2> {
    pub serial1: S1,
    pub serial2: S2,
    io_state: Serial128Io,
}

bitflags! {
    struct Serial128Io: u8 {
        const SER1_CTS      = 0b0000_0001; // 0=Spectrum ready to receive, 1=Busy
        const SER1_RXD      = 0b0000_0010; // unused
        const SER2_CTS      = 0b0000_0100; // 0=Spectrum ready to receive, 1=Busy 
        const SER2_RXD      = 0b0000_1000; // 0=Transmit high bit,         1=Transmit low bit
        const SER1_OUT_MASK = 0b0000_0011;
        const SER2_OUT_MASK = 0b0000_1100;
        const OUTPUT_MASK   = Self::SER1_OUT_MASK.bits()|Self::SER2_OUT_MASK.bits();
        const SER1_DTR      = 0b0001_0000; // unused
        const SER1_TXD      = 0b0010_0000; // 0=Receive high bit,          1=Receive low bit
        const SER2_DTR      = 0b0100_0000; // 0=Device ready for data,     1=Busy
        const SER2_TXD      = 0b1000_0000; // 0=Receive high bit,          1=Receive low bit
        const SER1_INP_MASK = 0b0011_0000;
        const SER2_INP_MASK = 0b1100_0000;
    }
}

impl Default for Serial128Io {
    fn default() -> Self {
        Serial128Io::all()
    }
}

impl Serial128Io {
    #[inline]
    fn is_ser1_cts(self) -> bool {
        self.intersects(Serial128Io::SER1_CTS)
    }
    #[inline]
    fn is_ser2_cts(self) -> bool {
        self.intersects(Serial128Io::SER2_CTS)
    }
    #[inline]
    fn ser1_cts_state(self) -> ControlState {
        self.intersects(Serial128Io::SER1_CTS).into()
    }
    #[inline]
    fn ser2_cts_state(self) -> ControlState {
        self.intersects(Serial128Io::SER2_CTS).into()
    }
    #[inline]
    fn ser1_rxd_state(self) -> DataState {
        self.intersects(Serial128Io::SER1_RXD).into()
    }
    #[inline]
    fn ser2_rxd_state(self) -> DataState {
        self.intersects(Serial128Io::SER2_RXD).into()
    }
    #[inline]
    fn set_ser1_txd(&mut self, txd: DataState) {
        self.set(Serial128Io::SER1_TXD, txd.into());
    }
    #[inline]
    fn set_ser2_txd(&mut self, txd: DataState) {
        self.set(Serial128Io::SER2_TXD, txd.into());
    }
    #[inline]
    fn set_ser1_dtr(&mut self, dtr: ControlState) {
        self.set(Serial128Io::SER1_DTR, dtr.into());
    }
    #[inline]
    fn set_ser2_dtr(&mut self, dtr: ControlState) {
        self.set(Serial128Io::SER2_DTR, dtr.into());
    }
}

impl<S1, S2> AyIoPort for SerialPorts128<S1, S2>
    where S1: Debug + SerialPort<Timestamp=VideoTs>,
          S2: Debug + SerialPort<Timestamp=VideoTs>
{
    type Timestamp = VideoTs;

    #[inline]
    fn ay_io_reset(&mut self, timestamp: Self::Timestamp) {
        self.io_state = Serial128Io::all();
        self.serial1.reset(timestamp);
        self.serial2.reset(timestamp);
    }

    #[inline]
    fn ay_io_write(&mut self, _addr: u16, data: u8, timestamp: Self::Timestamp) {
        let mut io_state = self.io_state;
        let io_diff = (io_state ^ Serial128Io::from_bits_truncate(data)) & Serial128Io::OUTPUT_MASK;
        io_state ^= io_diff;
        if io_diff.is_ser1_cts() {
            self.serial1.update_cts(io_state.ser1_cts_state(), timestamp);
        }
        else {
            io_state.set_ser1_dtr(
                self.serial1.write_data(io_state.ser1_rxd_state(), timestamp)
            );
        }
        if io_diff.is_ser2_cts() {
            self.serial2.update_cts(io_state.ser2_cts_state(), timestamp);
        }
        else {
            io_state.set_ser2_dtr(
                self.serial2.write_data(io_state.ser2_rxd_state(), timestamp)
            );
        }
        self.io_state = io_state;
        // println!("port a write: {:02x} {:?} {:?} {:?}", data, timestamp, self.keypad_io, self.keypad_event_ts);
    }

    #[inline]
    fn ay_io_read(&mut self, _addr: u16, timestamp: Self::Timestamp) -> u8 {
        let mut io_state = self.io_state;
        io_state.set_ser1_txd(self.serial1.read_data(timestamp));
        io_state.set_ser2_txd(self.serial2.read_data(timestamp));
        self.io_state = io_state;
        io_state.bits()
    }

    #[inline]
    fn end_frame(&mut self, timestamp: Self::Timestamp) {
        self.io_state.set_ser1_dtr(self.serial1.poll_ready(timestamp));
        self.io_state.set_ser2_dtr(self.serial2.poll_ready(timestamp));
        self.serial1.end_frame(timestamp);
        self.serial2.end_frame(timestamp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ayayay_works() {
        println!("SmallRng {}", core::mem::size_of::<SmallRng>());
        println!("{:?}", SmallRng::from_entropy());
        // println!("SerialKeypad<Ula128VidFrame> {}", core::mem::size_of::<SerialKeypad<Ula128VidFrame>>());
    }
}
