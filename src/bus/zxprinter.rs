use core::ops::{Deref, DerefMut};
use core::marker::PhantomData;

use crate::clock::{VFrameTsCounter, VideoTs, FTs};
use crate::bus::{BusDevice, NullDevice, PortAddress};
use crate::chip::ula::{UlaTsCounter, Ula};
use crate::audio::ay::*;
use crate::audio::{Blep, AmpLevels};
use crate::audio::sample::SampleDelta;
use crate::memory::ZxMemory;
use crate::video::VideoFrame;
use super::ay::PassByAyAudioBusDevice;

pub use crate::peripherals::zxprinter::{ZxPrinterDevice, Spooler};

pub type ZxPrinter<V, S, D=NullDevice<VideoTs>> = ZxPrinterBusDevice<ZxPrinterPortAddress, V, S, D>;
pub type Alphacom32<V, S, D=NullDevice<VideoTs>> = ZxPrinterBusDevice<Alphacom32PortAddress, V, S, D>;
pub type TS2040<V, S, D=NullDevice<VideoTs>> = ZxPrinterBusDevice<TS2040PortAddress, V, S, D>;

#[derive(Clone, Default)]
pub struct ZxPrinterBusDevice<P, V, S, D=NullDevice<VideoTs>>
{
    pub printer: ZxPrinterDevice<V, S>,
    bus: D,
    _port_decode: PhantomData<P>
}

#[derive(Clone, Copy, Default)]
pub struct ZxPrinterPortAddress;
impl PortAddress for ZxPrinterPortAddress {
    const ADDRESS_MASK: u16 = 0b0000_0000_0000_0100;
    const ADDRESS_BITS: u16 = 0b0000_0000_0000_0000;
}

#[derive(Clone, Copy, Default)]
pub struct Alphacom32PortAddress;
impl PortAddress for Alphacom32PortAddress {
    const ADDRESS_MASK: u16 = 0b0000_0000_1000_0100;
    const ADDRESS_BITS: u16 = 0b0000_0000_1000_0000;
}

#[derive(Clone, Copy, Default)]
pub struct TS2040PortAddress;
impl PortAddress for TS2040PortAddress {
    const ADDRESS_MASK: u16 = 0b0000_0000_1111_1111;
    const ADDRESS_BITS: u16 = 0b0000_0000_1111_1011;
}

impl<P, V, S, D> PassByAyAudioBusDevice for ZxPrinterBusDevice<P, V, S, D> {}

impl<P, V, S, D> BusDevice for ZxPrinterBusDevice<P, V, S, D>
    where P: PortAddress,
          V: VideoFrame,
          D: BusDevice<Timestamp=VideoTs>,
          S: Spooler
{
    type Timestamp = VideoTs;
    type NextDevice = D;

    #[inline]
    fn next_device(&mut self) -> &mut Self::NextDevice {
        &mut self.bus
    }

    #[inline]
    fn reset(&mut self, timestamp: Self::Timestamp) {
        self.printer.reset();
        self.bus.reset(timestamp);
    }

    #[inline]
    fn next_frame(&mut self, timestamp: Self::Timestamp) {
        self.printer.next_frame(timestamp);
        self.bus.next_frame(timestamp)
    }

    #[inline]
    fn read_io(&mut self, port: u16, timestamp: Self::Timestamp) -> Option<u8> {
        if P::match_port(port) {
            // println!("reading: {:02x}", port);
            return Some(self.printer.read_status(timestamp))
        }
        self.bus.read_io(port, timestamp)
    }

    #[inline]
    fn write_io(&mut self, port: u16, data: u8, timestamp: Self::Timestamp) -> bool {
        if P::match_port(port) {
            // println!("print: {:04x} {:b}", port, data);
            self.printer.write_control(data, timestamp);
            return true;
        }
        self.bus.write_io(port, data, timestamp)
    }
}
