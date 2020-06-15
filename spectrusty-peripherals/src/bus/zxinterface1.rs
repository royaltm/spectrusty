//! A bus device for **ZX Interface I**.
/*!

### I/O Port **0xe7**.

Used to send or receive data to and from the microdrive.

Accessing this port will halt the Z80 until the Interface I has collected 8 bits from the microdrive head;
therefore, if the microdrive motor isn't running, or there is no formatted cartridge in the microdrive,
the Spectrum hangs. This is the famous 'IN 0 crash'.

### I/O Port **0xef**.

Bits *DTR* and *CTS* are used by the RS-232 interface.

The *WAIT* bit is used by the ZX Network to synchronise incoming bytes.
It is being activated just before reading bits of each incoming byte.

*GAP*, *SYNC*, *WR_PROT*, *ERASE*, *R/W*, *COMMS CLK* and *COMMS DATA* are used by the microdrive system.

```text
       Bit    7   6    5    4    3    2    1     0
            +---------------------------------------+
        READ|   |   |    |busy| dtr |gap| sync|write|
            |   |   |    |    |     |   |     |prot.|
            |---+---+----+----+-----+---+-----+-----|
       WRITE|   |   |wait| cts|erase|r/w|comms|comms|
            |   |   |    |    |     |   | clk | data|
            +---------------------------------------+
```
### I/O Port **0xf7**.

If the microdrive is not being used, the *COMMS DATA* output selects the function of OUT bit 0 of **0xf7** port:
```text
       Bit      7    6   5   4   3   2   1       0
            +------------------------------------------+
        READ|txdata|   |   |   |   |   |   |    net    |
            |      |   |   |   |   |   |   |   input   |
            |------+---+---+---+---+---+---+-----------|
       WRITE|      |   |   |   |   |   |   |net output/|
            |      |   |   |   |   |   |   |   rxdata  |
            +------------------------------------------+
```
If a 0 was written to *COMMS DATA* the lowest bit is written to the *net output*, if 1 was written to *COMMS DATA*
the lowest written bit is *rxdata*.
!*/
use core::num::NonZeroU16;
use core::fmt;
use std::io;

#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};

use spectrusty_core::{
    clock::VideoTs,
    bus::{BusDevice, NullDevice},
    video::VideoFrame
};

use super::ay::PassByAyAudioBusDevice;

pub use crate::{
    storage::microdrives::*,
    serial::{SerialPortDevice, DataState, ControlState, Rs232Io, BAUD_RATES, DEFAULT_BAUD_RATE},
    network::zxnet::*
};

impl<V, R, W, D> fmt::Display for ZxInterface1BusDevice<V, R, W, D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ZX Interface I")
    }
}

/// Connects the ZX Interface 1 I/O port emulator as a [BusDevice].
///
/// [Rs232Io]'s [io::Read] as `R` and [io::Write] as `W` implementations are needed to complete this type.
/// [ZxNetSocket] implmentation is needed as `N` for the underlying [ZxNet].
#[derive(Default, Debug)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "snapshot", serde(rename_all = "camelCase"))]
pub struct ZxInterface1BusDevice<V, R, W, N, D=NullDevice<VideoTs>>
{
    /// A direct access to the **Microdrives**.
    #[cfg_attr(feature = "snapshot", serde(default))]
    pub microdrives: ZXMicrodrives<V>,
    /// A direct access to the **RS-232** implementation.
    #[cfg_attr(feature = "snapshot", serde(default))]
    pub serial: Rs232Io<V, R, W>,
    /// A direct access to the **ZX NET** implementation.
    #[cfg_attr(feature = "snapshot", serde(skip))]
    pub network: ZxNet<V, N>,
    sernet: If1SerNetIo,
    ctrl_in: If1ControlIn,
    ctrl_out: If1ControlOut,
    #[cfg_attr(feature = "snapshot", serde(default))]
    bus: D
}

bitflags! {
    #[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
    struct If1SerNetIo: u8 {
        const RXD     = 0b0000_0001;
        const TXD     = 0b1000_0000;
        const NET     = 0b0000_0001;
        const DEFAULT = 0b0111_1110;
    }
}

bitflags! {
    #[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
    struct If1ControlIn: u8 {
        const MD_MASK  = 0b0000_0111;
        const MD_PROT  = 0b0000_0001;
        const MD_SYN   = 0b0000_0010;
        const MD_GAP   = 0b0000_0100;
        const SER_DTR  = 0b0000_1000;
        const NET_BUSY = 0b0001_0000;
        const DEFAULT  = 0b1110_0111;
    }
}

bitflags! {
    #[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
    struct If1ControlOut: u8 {
        const MD_MASK   = 0b0000_1111;
        const COMMS_OUT = 0b0000_0001;
        const COMMS_CLK = 0b0000_0010;
        const MD_R_W    = 0b0000_0100;
        const MD_ERASE  = 0b0000_1000;
        const SER_CTS   = 0b0001_0000;
        const NET_WAIT  = 0b0010_0000;
        const DEFAULT   = 0b1110_1110;
    }
}

macro_rules! default_io {
    ($($ty:ty),*) => {$(
        impl Default for $ty {
            fn default() -> Self {
                <$ty>::DEFAULT
            }
        }
    )*};
}

default_io!(If1ControlOut, If1ControlIn, If1SerNetIo);

impl If1SerNetIo {
    #[inline(always)]
    fn rxd(self) -> DataState {
        DataState::from(!self.intersects(If1SerNetIo::RXD))
    }
    #[inline(always)]
    fn net(self) -> bool {
        !self.intersects(If1SerNetIo::NET)
    }
    #[inline(always)]
    fn set_txd(&mut self, txd: DataState) {
        self.set(If1SerNetIo::TXD, !bool::from(txd))
    }
    #[inline(always)]
    fn set_net(&mut self, net: bool) {
        self.set(If1SerNetIo::NET, net)
    }
}

impl If1ControlIn {
    #[inline(always)]
    fn set_md_state(&mut self, CartridgeState { gap, syn, write_protect }: CartridgeState) {
        self.insert(If1ControlIn::MD_MASK);
        if gap {
            self.remove(If1ControlIn::MD_GAP);
        }
        if syn {
            self.remove(If1ControlIn::MD_SYN);
        }
        if write_protect {
            self.remove(If1ControlIn::MD_PROT);
        }
    }
    #[inline(always)]
    fn set_dtr(&mut self, dtr: ControlState) {
        self.set(If1ControlIn::SER_DTR, !bool::from(dtr))
    }
}

impl If1ControlOut {
    #[inline(always)]
    fn is_comms_out_ser(self) -> bool {
        self.intersects(If1ControlOut::COMMS_OUT)
    }
    #[inline(always)]
    fn erase(self) -> bool {
        !self.intersects(If1ControlOut::MD_ERASE)
    }
    #[inline(always)]
    fn write(self) -> bool {
        !self.intersects(If1ControlOut::MD_R_W)
    }
    #[inline(always)]
    fn comms_clk(self) -> bool {
        !self.intersects(If1ControlOut::COMMS_CLK)
    }
    #[inline(always)]
    fn comms_out(self) -> bool {
        !self.intersects(If1ControlOut::COMMS_OUT)
    }
    #[inline(always)]
    fn cts(self) -> ControlState {
        ControlState::from(!self.intersects(If1ControlOut::SER_CTS))
    }
    #[inline(always)]
    fn wait(self) -> bool {
        !self.intersects(If1ControlOut::NET_WAIT)
    }
}

const IF1_MASK:      u16 = 0b0000_0000_0001_1000;
const IF1_SERN_BITS: u16 = 0b0000_0000_0001_0000;
const IF1_CTRL_BITS: u16 = 0b0000_0000_0000_1000;
const IF1_DATA_BITS: u16 = 0b0000_0000_0000_0000;


impl<V, R, W, N, D> PassByAyAudioBusDevice for ZxInterface1BusDevice<V, R, W, N, D> {}

impl<V, R, W, N, D> BusDevice for ZxInterface1BusDevice<V, R, W, N, D>
    where V: VideoFrame,
          R: io::Read + fmt::Debug,
          W: io::Write + fmt::Debug,
          N: ZxNetSocket + fmt::Debug,
          D: BusDevice<Timestamp=VideoTs>
{
    type Timestamp = VideoTs;
    type NextDevice = D;

    #[inline]
    fn next_device_mut(&mut self) -> &mut Self::NextDevice {
        &mut self.bus
    }

    #[inline]
    fn next_device_ref(&self) -> &Self::NextDevice {
        &self.bus
    }

    #[inline]
    fn into_next_device(self) -> Self::NextDevice {
        self.bus
    }

    #[inline]
    fn reset(&mut self, timestamp: Self::Timestamp) {
        self.microdrives.reset(timestamp);
        self.bus.reset(timestamp);
    }

    #[inline]
    fn next_frame(&mut self, timestamp: Self::Timestamp) {
        // println!("frame ends");
        self.microdrives.next_frame(timestamp);
        self.ctrl_in.set_dtr(self.serial.poll_ready(timestamp));
        self.serial.next_frame(timestamp);
        self.network.next_frame(timestamp);
        self.bus.next_frame(timestamp)
    }

    #[inline]
    fn read_io(&mut self, port: u16, timestamp: Self::Timestamp) -> Option<(u8, Option<NonZeroU16>)> {
        match port & IF1_MASK {
            IF1_SERN_BITS => {
                self.sernet.set_txd(self.serial.read_data(timestamp));
                if self.ctrl_out.cts().is_inactive() {
                    self.sernet.set_net(self.network.poll_state(timestamp));
                }
                // println!("net read: {:02x} {:?}", self.sernet.bits(), timestamp);
                Some((self.sernet.bits(), None))
            }
            IF1_CTRL_BITS => {
                self.ctrl_in.set_md_state(self.microdrives.read_state(timestamp));
                // println!("ctrl read: {:02x} {:?}", self.ctrl_in.bits(), timestamp);
                // TODO: poll busy?
                Some((self.ctrl_in.bits(), None))
            }
            IF1_DATA_BITS => {
                Some(self.microdrives.read_data(timestamp))
            }
            _ => self.bus.read_io(port, timestamp)
        }
    }

    #[inline]
    fn write_io(&mut self, port: u16, data: u8, timestamp: Self::Timestamp) -> Option<u16> {
        match port & IF1_MASK {
            IF1_SERN_BITS => {
                let data = If1SerNetIo::from_bits_truncate(data);
                if self.ctrl_out.is_comms_out_ser() {
                    self.ctrl_in.set_dtr(self.serial.write_data(data.rxd(), timestamp));
                }
                else {
                    // println!("net write rxs: {:?} {:?}", (rxd as u8)^1, timestamp);
                    self.network.send_state(data.net(), timestamp);
                    // TODO: handle network
                }
                Some(0)
            }
            IF1_CTRL_BITS => {
                let data = If1ControlOut::from_bits_truncate(data);
                let diff = self.ctrl_out ^ data;
                // println!("ctrl write: {:?}", data);
                self.ctrl_out ^= diff;
                if !(diff & If1ControlOut::MD_MASK).is_empty() {
                    self.microdrives.write_control(timestamp,
                            data.erase(), data.write(), data.comms_clk(), data.comms_out());
                }
                if !(diff & If1ControlOut::SER_CTS).is_empty() {
                    self.serial.update_cts(data.cts(), timestamp);
                }
                if data.wait() {
                    self.network.wait_data(timestamp);
                    // TODO: handle network
                }
                Some(0)
            }
            IF1_DATA_BITS => Some(self.microdrives.write_data(data, timestamp)),
            _ => self.bus.write_io(port, data, timestamp)
        }
    }
}
