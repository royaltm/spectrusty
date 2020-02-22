use core::fmt::Debug;
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};
use crate::bus::BusDevice;
use super::ay::PassByAyAudioBusDevice;

#[derive(Clone, Default)]
pub struct DebugBusDevice<T, D> {
    bus: D,
    _ts: PhantomData<T>
}

impl<T: Debug, D: BusDevice<Timestamp=T>> BusDevice for DebugBusDevice<T, D> {
    type Timestamp = T;
    type NextDevice = D;

    fn next_device_mut(&mut self) -> &mut Self::NextDevice {
        &mut self.bus
    }
    fn next_device_ref(&self) -> &Self::NextDevice {
        &self.bus
    }
    fn read_io(&mut self, port: u16, timestamp: Self::Timestamp) -> Option<u8> {
        println!("read_io: {:04x} {:?}", port, timestamp);
        self.bus.read_io(port, timestamp)
    }
    /// Called by the control unit on IO::write_io.
    fn write_io(&mut self, port: u16, data: u8, timestamp: Self::Timestamp) -> bool {
        println!("write_io: {:04x} {:02x} {:?}", port, data, timestamp);
        self.bus.write_io(port, data, timestamp)
    }
}

impl<T, D> PassByAyAudioBusDevice for DebugBusDevice<T, D> {}
