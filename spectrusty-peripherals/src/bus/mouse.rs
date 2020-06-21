//! A bus device for connecting pointing devices / mouses.
use core::num::NonZeroU16;
use core::fmt::{self, Debug};
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};

#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};

use spectrusty_core::{
    bus::{BusDevice, PortAddress}
};

use super::ay::PassByAyAudioBusDevice;

pub use crate::mouse::{
    MouseDevice, MouseInterface, NullMouseDevice,
    kempston::*
};

/// A convenient Kempston Mouse [BusDevice] type.
pub type KempstonMouse<D> = MouseBusDevice<
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
pub struct MouseBusDevice<P, M, D> {
    /// A [MouseDevice] implementation, which may also implement [MouseInterface] trait
    /// for providing user input.
    #[cfg_attr(feature = "snapshot", serde(default))]
    pub mouse: M,
    #[cfg_attr(feature = "snapshot", serde(default))]
    bus: D,
    #[cfg_attr(feature = "snapshot", serde(skip))]
    _port_decode: PhantomData<P>,
}

/// Kempston Mouse [PortAddress].
#[derive(Clone, Copy, Default, Debug)]
pub struct KempstonMousePortAddress;
impl PortAddress for KempstonMousePortAddress {
    const ADDRESS_MASK: u16 = 0b0000_0000_0010_0000;
    const ADDRESS_BITS: u16 = 0b1111_1010_1101_1111;
}

impl<P, M: MouseInterface, D> Deref for MouseBusDevice<P, M, D> {
    type Target = M;
    fn deref(&self) -> &Self::Target {
        &self.mouse
    }
}

impl<P, M: MouseInterface, D> DerefMut for MouseBusDevice<P, M, D> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.mouse
    }
}

impl<P, M, D> PassByAyAudioBusDevice for MouseBusDevice<P, M, D> {}

impl<P, M, D> BusDevice for MouseBusDevice<P, M, D>
    where P: PortAddress,
          D: BusDevice,
          M: MouseDevice
{
    type Timestamp = D::Timestamp;
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
