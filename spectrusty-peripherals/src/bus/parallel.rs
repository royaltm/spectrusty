//! Spectrum +3 CENTRONICS port bus device for parallel printers and other devices.
use core::num::NonZeroU16;
use core::fmt;
use core::ops::{Deref, DerefMut};

#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};

use spectrusty_core::{
    bus::{BusDevice, NullDevice, PortAddress},
    clock::VideoTs
};
use super::ay::PassByAyAudioBusDevice;

pub use crate::parallel::*;

pub type Plus3CentronicsWriterBusDevice<V, D, W> = Plus3CentronicsBusDevice<ParallelPortWriter<V, W>, D>;

impl<S, D> fmt::Display for Plus3CentronicsBusDevice<S, D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("+3 Centronics Port")
    }
}

bitflags! {
    #[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "snapshot", serde(from = "u8", into = "u8"))]
    #[derive(Default)]
    struct CentronicsFlags: u8 {
        const BUSY   = 0b0000_0001;
        const STROBE = 0b0001_0000;
    }
}

impl CentronicsFlags {
    fn set_busy(&mut self, busy: bool) {
        self.set(CentronicsFlags::BUSY, busy)
    }

    fn is_busy(self) -> bool {
        self.intersects(CentronicsFlags::BUSY)
    }

    fn is_strobe(self) -> bool {
        self.intersects(CentronicsFlags::STROBE)
    }
}

/// Connects the [ParallelPortDevice] emulator as a [BusDevice] via +3 Centronics Port.
///
/// Substitute `P` with a type implementing [ParallelPortDevice].
#[derive(Clone, Default, Debug)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
pub struct Plus3CentronicsBusDevice<P, D=NullDevice<VideoTs>>
{
    /// Provides a direct access to the [ParallelPortDevice].
    #[cfg_attr(feature = "snapshot", serde(default))]
    pub parallel: P,
    flags: CentronicsFlags,
    #[cfg_attr(feature = "snapshot", serde(default))]
    bus: D
}

#[derive(Clone, Copy, Default, Debug)]
struct Ula3CtrlPortAddress;
impl PortAddress for Ula3CtrlPortAddress {
    const ADDRESS_MASK: u16 = 0b1111_0000_0000_0010;
    const ADDRESS_BITS: u16 = 0b0001_1111_1111_1101;
}

#[derive(Clone, Copy, Default, Debug)]
struct Ula3CentronicsPortAddress;
impl PortAddress for Ula3CentronicsPortAddress {
    const ADDRESS_MASK: u16 = 0b1111_0000_0000_0010;
    const ADDRESS_BITS: u16 = 0b0000_1111_1111_1101;
}

impl<P, D> Deref for Plus3CentronicsBusDevice<P, D> {
    type Target = P;
    fn deref(&self) -> &Self::Target {
        &self.parallel
    }
}

impl<P, D> DerefMut for Plus3CentronicsBusDevice<P, D> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.parallel
    }
}

impl<P, D> PassByAyAudioBusDevice for Plus3CentronicsBusDevice<P, D> {}

impl<P, D> BusDevice for Plus3CentronicsBusDevice<P, D>
    where P: ParallelPortDevice<Timestamp=VideoTs> + fmt::Debug,
          D: BusDevice<Timestamp=VideoTs>,
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
        self.bus.reset(timestamp);
    }

    #[inline]
    fn next_frame(&mut self, timestamp: Self::Timestamp) {
        self.parallel.next_frame();
        self.bus.next_frame(timestamp)
    }

    #[inline]
    fn update_timestamp(&mut self, timestamp: Self::Timestamp) {
        if self.flags.is_busy() {
            self.flags.set_busy( self.parallel.poll_busy() );
        };
        self.bus.update_timestamp(timestamp)
    }

    #[inline]
    fn read_io(&mut self, port: u16, timestamp: Self::Timestamp) -> Option<(u8, Option<NonZeroU16>)> {
        if Ula3CentronicsPortAddress::match_port(port) {
            let data = (self.flags & CentronicsFlags::BUSY).bits() | !CentronicsFlags::BUSY.bits();
            return Some((data, None))
        }
        self.bus.read_io(port, timestamp)
    }

    #[inline]
    fn write_io(&mut self, port: u16, data: u8, timestamp: Self::Timestamp) -> Option<u16> {
        if Ula3CentronicsPortAddress::match_port(port) {
            // println!("print: {:04x} {:b}", port, data);
            self.parallel.write_data(data, timestamp);
            return Some(0);
        }
        else if Ula3CtrlPortAddress::match_port(port) {
            let flags = CentronicsFlags::from_bits_truncate(data);
            let flags_diff = (self.flags ^ flags) & CentronicsFlags::STROBE;
            if flags_diff == CentronicsFlags::STROBE {
                self.flags ^= flags_diff;
                self.flags.set_busy(
                    self.parallel.write_strobe(flags.is_strobe(), timestamp));
            }
        }
        self.bus.write_io(port, data, timestamp)
    }
}

impl From<CentronicsFlags> for u8 {
    #[inline]
    fn from(flags: CentronicsFlags) -> u8 {
        flags.bits()
    }
}

impl From<u8> for CentronicsFlags {
    #[inline]
    fn from(flags: u8) -> CentronicsFlags {
        CentronicsFlags::from_bits_truncate(flags)
    }
}
