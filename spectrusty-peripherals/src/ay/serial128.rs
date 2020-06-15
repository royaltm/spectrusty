use core::fmt::Debug;

#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};

use spectrusty_core::clock::VideoTs;
use crate::serial::{SerialPortDevice, DataState, ControlState};

use super::AyIoPort;

/// The bridge between ZX Spectrum 128 [AY-3-8912][crate::ay::Ay3_891xIo] I/O ports and emulators
/// of serial port devices.
///
/// The *RS232C*/*MIDI* interface is implemented using the Port `A` Data Store in the sound generator chip IC32.
/// The data store is a special register which accesses an 8-bit bi-directional port A7-AO.
/// The port occupies the same I/O space as the sound generator registers and is accessed in much the same way.
///
/// Bits of an I/O port `A` register have the following meaning:
///
/// * A0: `KEYPAD CTS` (OUT) = 0: active - Spectrum ready to receive, 1: inactive - busy.
/// * A1: `KEYPAD RXD` (OUT) = 0: transmit bit (high voltage), 1: transmit bit (low voltage).
/// * A2: `RS232  CTS` (OUT) = 0: active - Spectrum ready to receive, 1: inactive - busy.
/// * A3: `RS232  RXD` (OUT) = 0: transmit (high voltage), 1: transmit bit (low voltage).
/// * A4: `KEYPAD DTR` (IN)  = 0: active - a device ready for data, 1: inactive - busy.
/// * A5: `KEYPAD TXD` (IN)  = 0: receive bit (high voltage), 1: Receive bit (low voltage).
/// * A6: `RS232  DTR` (IN)  = 0: active - a device ready for data, 1: inactive - busy.
/// * A7: `RS232  TXD` (IN)  = 0: receive bit (high voltage), 1: Receive bit (low voltage).
///
/// This type implements a single [AyIoPort] as two serial ports of ZX Spectrum 128.
///
/// One of these ports is primarily being used for the extension [keypad][crate::serial::SerialKeypad]
/// and the other for communicating with a serial printer or other [RS-232][crate::serial::Rs232Io]
/// device in either direction.
///
/// To "connect" something to these ports, provide some types implementing [SerialPortDevice] as
/// `S1` and `S2`.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "snapshot", serde(rename_all = "camelCase"))]
pub struct SerialPorts128<S1,S2> {
    /// A [SerialPortDevice] connected to a `KEYPAD` port.
    #[cfg_attr(feature = "snapshot", serde(default))]
    pub serial1: S1,
    /// A [SerialPortDevice] connected to a `RS232` port.
    #[cfg_attr(feature = "snapshot", serde(default))]
    pub serial2: S2,
    io_state: Serial128Io,
}

bitflags! {
    #[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "snapshot", serde(from = "u8", into = "u8"))]
    struct Serial128Io: u8 {
        const SER1_CTS      = 0b0000_0001;
        const SER1_RXD      = 0b0000_0010;
        const SER2_CTS      = 0b0000_0100;
        const SER2_RXD      = 0b0000_1000;
        const SER1_OUT_MASK = 0b0000_0011;
        const SER2_OUT_MASK = 0b0000_1100;
        const OUTPUT_MASK   = Self::SER1_OUT_MASK.bits()|Self::SER2_OUT_MASK.bits();
        const SER1_DTR      = 0b0001_0000;
        const SER1_TXD      = 0b0010_0000;
        const SER2_DTR      = 0b0100_0000;
        const SER2_TXD      = 0b1000_0000;
        const SER1_INP_MASK = 0b0011_0000;
        const SER2_INP_MASK = 0b1100_0000;
    }
}

impl Default for Serial128Io {
    fn default() -> Self {
        Serial128Io::all()
    }
}

impl From<u8> for Serial128Io {
    fn from(keys: u8) -> Self {
        Serial128Io::from_bits_truncate(keys)
    }
}

impl From<Serial128Io> for u8 {
    fn from(keys: Serial128Io) -> Self {
        keys.bits()
    }
}

impl Serial128Io {
    #[inline]
    fn is_ser_any_cts(self) -> bool {
        self.intersects(Serial128Io::SER1_CTS|Serial128Io::SER2_CTS)
    }
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
    where S1: Debug + SerialPortDevice<Timestamp=VideoTs>,
          S2: Debug + SerialPortDevice<Timestamp=VideoTs>
{
    type Timestamp = VideoTs;

    #[inline]
    fn ay_io_reset(&mut self, _timestamp: Self::Timestamp) {}

    #[inline]
    fn ay_io_write(&mut self, _addr: u16, data: u8, timestamp: Self::Timestamp) {
        let mut io_state = self.io_state;
        let io_diff = (io_state ^ Serial128Io::from_bits_truncate(data)) & Serial128Io::OUTPUT_MASK;
        io_state ^= io_diff;
        // println!("port A write: {:02x} {:?} {:?}", data, io_diff, timestamp);
        if io_diff.is_ser1_cts() {
            self.serial1.update_cts(io_state.ser1_cts_state(), timestamp);
        }
        if io_diff.is_ser2_cts() {
            self.serial2.update_cts(io_state.ser2_cts_state(), timestamp);
        }
        if !io_diff.is_ser_any_cts() {
            io_state.set_ser1_dtr(
                self.serial1.write_data(io_state.ser1_rxd_state(), timestamp)
            );
            io_state.set_ser2_dtr(
                self.serial2.write_data(io_state.ser2_rxd_state(), timestamp)
            );
        }
        self.io_state = io_state;
    }

    #[inline]
    fn ay_io_read(&mut self, _addr: u16, timestamp: Self::Timestamp) -> u8 {
        let mut io_state = self.io_state;
        io_state.set_ser1_txd(self.serial1.read_data(timestamp));
        io_state.set_ser2_txd(self.serial2.read_data(timestamp));
        self.io_state = io_state;
        // println!("port A read: {:02x}", io_state.bits());
        (io_state | Serial128Io::OUTPUT_MASK).bits()
    }

    #[inline]
    fn end_frame(&mut self, timestamp: Self::Timestamp) {
        self.io_state.set_ser1_dtr(self.serial1.poll_ready(timestamp));
        self.io_state.set_ser2_dtr(self.serial2.poll_ready(timestamp));
        self.serial1.next_frame(timestamp);
        self.serial2.next_frame(timestamp);
    }
}

#[cfg(test)]
mod tests {
    // use super::*;
    #[test]
    fn serial_ports_128_work() {
        // println!("SerialKeypad<Ula128VidFrame> {}", core::mem::size_of::<SerialKeypad<Ula128VidFrame>>());
    }
}
