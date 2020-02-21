use core::ops::{Deref, DerefMut};
use core::marker::PhantomData;

use crate::clock::{VFrameTsCounter, VideoTs, FTs};
use crate::bus::{BusDevice, NullDevice};
use crate::io::ay::*;
use crate::chip::ula::{UlaTsCounter, Ula};
use crate::audio::ay::*;
use crate::audio::{Blep, AmpLevels};
use crate::audio::sample::SampleDelta;
use crate::memory::ZxMemory;
use crate::video::VideoFrame;
use super::ay::PassByAyAudioBusDevice;

pub type ZxPrinter<D=NullDevice<VideoTs>> = ZxPrinterBusDevice<VideoTs, ZxPrinterPortDecode, D>;
pub type Alphacom32<D=NullDevice<VideoTs>> = ZxPrinterBusDevice<VideoTs, Alphacom32PortDecode, D>;
pub type TS2040<D=NullDevice<VideoTs>> = ZxPrinterBusDevice<VideoTs, TS2040PortDecode, D>;

#[derive(Clone, Default)]
pub struct ZxPrinterBusDevice<T, P, D=NullDevice<T>>
{
    last_ts: T,
    state: u8,
    bus: D,
    _port_decode: PhantomData<P>,
    // _video_frame: PhantomData<V>
}

pub trait PrinterPortDecode {
    const PORT_MASK: u16;
    const PORT_DATA: u16;
    #[inline]
    fn is_data_rw(port: u16) -> bool {
        port & Self::PORT_MASK == Self::PORT_DATA
    }
}

#[derive(Clone, Copy, Default)]
pub struct ZxPrinterPortDecode;
impl PrinterPortDecode for ZxPrinterPortDecode {
    const PORT_MASK: u16 = 0b0000_0000_0000_0100;
    const PORT_DATA: u16 = 0b0000_0000_0000_0000;
}

#[derive(Clone, Copy, Default)]
pub struct Alphacom32PortDecode;
impl PrinterPortDecode for Alphacom32PortDecode {
    const PORT_MASK: u16 = 0b0000_0000_1000_0100;
    const PORT_DATA: u16 = 0b0000_0000_1000_0000;
}

#[derive(Clone, Copy, Default)]
pub struct TS2040PortDecode;
impl PrinterPortDecode for TS2040PortDecode {
    const PORT_MASK: u16 = 0b0000_0000_1111_1111;
    const PORT_DATA: u16 = 0b0000_0000_1111_1011;
}

impl<T, P, D> PassByAyAudioBusDevice for ZxPrinterBusDevice<T, P, D> {}

impl<T, P, D> BusDevice for ZxPrinterBusDevice<T, P, D>
    where P: PrinterPortDecode,
          T: Copy,
          D: BusDevice<Timestamp=T>
{
    type Timestamp = T;
    type NextDevice = D;

    #[inline]
    fn next_device(&mut self) -> &mut Self::NextDevice {
        &mut self.bus
    }

    #[inline]
    fn reset(&mut self, timestamp: Self::Timestamp) {
        self.bus.reset(timestamp);
    }

    #[inline]
    fn update_timestamp(&mut self, timestamp: Self::Timestamp) {
        self.bus.update_timestamp(timestamp)
    }

    #[inline]
    fn next_frame(&mut self, timestamp: Self::Timestamp) {
        self.bus.next_frame(timestamp)
    }

    #[inline]
    fn m1<Z: ZxMemory>(&mut self, memory: &mut Z, pc: u16, timestamp: Self::Timestamp) {
        self.bus.m1(memory, pc, timestamp)
    }

    #[inline]
    fn read_io(&mut self, port: u16, timestamp: Self::Timestamp) -> Option<u8> {
        if P::is_data_rw(port) {
            // println!("reading: {:02x}", port);
            return Some(self.state)
        }
        self.bus.read_io(port, timestamp)
    }

    #[inline]
    fn write_io(&mut self, port: u16, data: u8, timestamp: Self::Timestamp) -> bool {
        if P::is_data_rw(port) {
            println!("print: {:04x} {:b}", port, data);
            return true;
        }
        self.bus.write_io(port, data, timestamp)
    }
}
