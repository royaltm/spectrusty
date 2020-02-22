//! Hosts [MouseBusDevice] implementing [BusDevice].
use core::convert::TryFrom;
use core::fmt;
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};

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

/// A mouse controller, providing a [BusDevice] implementation that can be used with [mouse devices][MouseDevice].
#[derive(Clone, Default)]
pub struct MouseBusDevice<T, P, M, D=NullDevice<T>> {
    /// A [MouseDevice] implementation, which may also implement [MouseInterface] trait
    /// for providing user input.
    pub mouse: M,
    bus: D,
    _port_decode: PhantomData<P>,
    _ts: PhantomData<T>
}

/// Kempston Mouse [PortAddress].
#[derive(Clone, Copy, Default)]
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

impl<T, P, M, D> BusDevice for MouseBusDevice<T, P, M, D>
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
    fn read_io(&mut self, port: u16, timestamp: Self::Timestamp) -> Option<u8> {
        let data = self.bus.read_io(port, timestamp);
        if P::match_port(port) {
            return Some(data.unwrap_or(!0) & self.mouse.port_read(port))
        }
        data
    }

    #[inline]
    fn write_io(&mut self, port: u16, data: u8, timestamp: Self::Timestamp) -> bool {
        if P::match_port(port) {
            if self.mouse.port_write(port, data) {
                return true;
            }
        }
        self.bus.write_io(port, data, timestamp)
    }
}
