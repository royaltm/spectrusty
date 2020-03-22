//! A [BusDevice] for connecting pointing devices / mouses.
use core::num::NonZeroU16;
use core::fmt::Debug;
use core::convert::TryFrom;
use core::fmt;
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};

#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};

use super::ay::PassByAyAudioBusDevice;
use crate::bus::{BusDevice, NullDevice, PortAddress};
use crate::clock::VideoTs;
use crate::memory::ZxMemory;

pub use crate::peripherals::mouse::{
    MouseDevice, MouseInterface, kempston::*
};

/// A convenient Kempston Mouse [BusDevice] type.
pub type KempstonMouse<D=NullDevice<VideoTs>> = MouseBusDevice<VideoTs,
                                                            KempstonMousePortAddress,
                                                            KempstonMouseDevice,
                                                            D>;

impl<D> fmt::Display for KempstonMouse<D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Kempston Mouse")
    }
}
/// A mouse controller, providing a [BusDevice] implementation that can be used with [mouse devices][MouseDevice].
#[derive(Clone, Default, Debug)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
pub struct MouseBusDevice<T, P, M, D=NullDevice<T>> {
    /// A [MouseDevice] implementation, which may also implement [MouseInterface] trait
    /// for providing user input.
    #[cfg_attr(feature = "snapshot", serde(default))]
    pub mouse: M,
    #[cfg_attr(feature = "snapshot", serde(default))]
    bus: D,
    #[cfg_attr(feature = "snapshot", serde(skip))]
    _port_decode: PhantomData<P>,
    #[cfg_attr(feature = "snapshot", serde(skip))]
    _ts: PhantomData<T>
}

/// Kempston Mouse [PortAddress].
#[derive(Clone, Copy, Default, Debug)]
pub struct KempstonMousePortAddress;
impl PortAddress for KempstonMousePortAddress {
    const ADDRESS_MASK: u16 = 0b0000_0000_0010_0000;
    const ADDRESS_BITS: u16 = 0b1111_1010_1101_1111;
}

impl<T, P, M, D> Deref for MouseBusDevice<T, P, M, D> {
    type Target = M;
    fn deref(&self) -> &Self::Target {
        &self.mouse
    }
}

impl<T, P, M, D> DerefMut for MouseBusDevice<T, P, M, D> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.mouse
    }
}

impl<T, P, M, D> PassByAyAudioBusDevice for MouseBusDevice<T, P, M, D> {}

impl<T: Debug, P, M, D> BusDevice for MouseBusDevice<T, P, M, D>
    where P: PortAddress,
          D: BusDevice<Timestamp=VideoTs>,
          M: MouseDevice
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
    fn read_io(&mut self, port: u16, timestamp: Self::Timestamp) -> Option<(u8, Option<NonZeroU16>)> {
        let bus_data = self.bus.read_io(port, timestamp);
        if P::match_port(port) {
            let mouse_data = self.mouse.port_read(port);
            if let Some((data, ws)) = bus_data {
                return Some((data & mouse_data, ws))
            }
            return Some((mouse_data, None))
        }
        bus_data
    }

    #[inline]
    fn write_io(&mut self, port: u16, data: u8, timestamp: Self::Timestamp) -> Option<u16> {
        if P::match_port(port) {
            if self.mouse.port_write(port, data) {
                return Some(0);
            }
        }
        self.bus.write_io(port, data, timestamp)
    }
}
