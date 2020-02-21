//! Hosts [JoystickBusDevice] implementing [BusDevice] for joysticks.
use core::marker::PhantomData;

use crate::clock::VideoTs;
use crate::bus::{BusDevice, NullDevice, PortAddress};
use super::ay::PassByAyAudioBusDevice;

pub use crate::peripherals::joystick::{JoystickDevice, JoystickInterface, kempston::*, fuller::*};

/// A convenient Kempston Joystick [BusDevice] type.
pub type KempstonJoystick<D=NullDevice<VideoTs>> = JoystickBusDevice<VideoTs, KempstonJoyPortAddress, KempstonJoystickDevice, D>;
/// A convenient Fuller Joystick [BusDevice] type.
pub type FullerJoystick<D=NullDevice<VideoTs>> = JoystickBusDevice<VideoTs, FullerJoyPortAddress, FullerJoystickDevice, D>;

/// A joystick controller, providing a [BusDevice] implementation that can be used with [joystick devices][JoystickDevice].
#[derive(Clone, Default)]
pub struct JoystickBusDevice<T, P, J, D=NullDevice<T>>
{
    /// A [JoystickDevice] implementation, which may also implement [JoystickInterface] trait
    /// for providing user input.
    pub joystick: J,
    bus: D,
    _port_decode: PhantomData<P>,
    _ts: PhantomData<T>
}

/// Kempston Joystick [PortAddress].
#[derive(Clone, Copy, Default)]
pub struct KempstonJoyPortAddress;
impl PortAddress for KempstonJoyPortAddress {
    const ADDRESS_MASK: u16 = 0x00ff;
    const ADDRESS_BITS: u16 = 0x001f;
}
/// Fuller Joystick [PortAddress].
#[derive(Clone, Copy, Default)]
pub struct FullerJoyPortAddress;
impl PortAddress for FullerJoyPortAddress {
    const ADDRESS_MASK: u16 = 0x00ff;
    const ADDRESS_BITS: u16 = 0x007f;
}

impl<T, P, J, D> PassByAyAudioBusDevice for JoystickBusDevice<T, P, J, D> {}

impl<T, P, J, D> BusDevice for JoystickBusDevice<T, P, J, D>
    where P: PortAddress,
          D: BusDevice<Timestamp=VideoTs>,
          J: JoystickDevice
{
    type Timestamp = VideoTs;
    type NextDevice = D;

    #[inline]
    fn next_device(&mut self) -> &mut Self::NextDevice {
        &mut self.bus
    }

    #[inline]
    fn read_io(&mut self, port: u16, timestamp: Self::Timestamp) -> Option<u8> {
        if P::match_port(port) {
            // println!("reading: {:02x}", port);
            return Some(self.joystick.port_read(port))
        }
        self.bus.read_io(port, timestamp)
    }

    #[inline]
    fn write_io(&mut self, port: u16, data: u8, timestamp: Self::Timestamp) -> bool {
        if P::match_port(port) {
            // println!("print: {:04x} {:b}", port, data);
            return self.joystick.port_write(port, data)
        }
        self.bus.write_io(port, data, timestamp)
    }
}
