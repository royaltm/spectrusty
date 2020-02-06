use core::marker::PhantomData;
use crate::memory::ZxMemory;

pub trait BusDevice {
    type Timestamp: Sized;
    type NextDevice: BusDevice;

    /// Returns a reference to the next device.
    fn next_device(&mut self) -> &mut Self::NextDevice;
    /// Resets the device and all devices in the chain.
    fn reset(&mut self, timestamp: Self::Timestamp);
    /// Called by the control unit on M1 cycles.
    fn m1<Z: ZxMemory>(&mut self, memory: &mut Z, pc: u16, timestamp: Self::Timestamp);
    /// Called by the control unit on IO::read_io.
    fn read_io(&mut self, port: u16, timestamp: Self::Timestamp) -> Option<u8>;
    /// Called by the control unit on IO::write_io.
    fn write_io(&mut self, port: u16, data: u8, timestamp: Self::Timestamp) -> bool;
}

#[derive(Clone, Default, Debug)]
pub struct NullDevice<T: Sized>(PhantomData<T>);

// impl<T: Sized> Default for NullDevice<T> {
//     fn default() -> Self {
//         NullDevice(PhantomData)
//     }
// }

impl<T: Sized> BusDevice for NullDevice<T> {
    type Timestamp = T;
    type NextDevice = Self;

    #[inline]
    fn next_device(&mut self) -> &mut Self::NextDevice {
        self
    }

    #[inline]
    fn reset(&mut self, _timestamp: Self::Timestamp) {}

    #[inline]
    fn m1<Z: ZxMemory>(&mut self, _memory: &mut Z, _pc: u16, _timestamp: Self::Timestamp) {}

    #[inline]
    fn read_io(&mut self, _port: u16, _timestamp: Self::Timestamp) -> Option<u8> {
        None
    }

    #[inline]
    fn write_io(&mut self, _port: u16, _data: u8, _timestamp: Self::Timestamp) -> bool {
        false
    }
}

pub struct OptionalDevice<D: BusDevice, N: BusDevice, T: Sized> {
    pub device: Option<D>,
    next_device: N,
    ts: PhantomData<T>
}

impl<D,N,T> OptionalDevice<D,N,T>
where D: BusDevice<Timestamp=T>, N: BusDevice<Timestamp=T>, T: Sized {
    pub fn new(device: Option<D>, next_device: N) -> Self {
        OptionalDevice { device, next_device, ts: PhantomData }
    }
}

impl<D,N,T> Default for OptionalDevice<D,N,T>
where D: BusDevice<Timestamp=T>, N: BusDevice<Timestamp=T> + Default, T: Sized {
    fn default() -> Self {
        OptionalDevice { device: None, next_device: N::default(), ts: PhantomData }
    }
}

impl<D,N,T> BusDevice for OptionalDevice<D,N,T>
where D: BusDevice<Timestamp=T>, N: BusDevice<Timestamp=T>, T: Sized + Copy {
    type Timestamp = T;
    type NextDevice = N;

    fn next_device(&mut self) -> &mut Self::NextDevice {
        &mut self.next_device
    }

    fn reset(&mut self, timestamp: Self::Timestamp) {
        if let Some(device) = &mut self.device {
            device.reset(timestamp);
        }
        self.next_device.reset(timestamp);
    }

    fn m1<Z: ZxMemory>(&mut self, memory: &mut Z, pc: u16, timestamp: Self::Timestamp) {
        if let Some(device) = &mut self.device {
            device.m1(memory, pc, timestamp);
        }
        self.next_device.m1(memory, pc, timestamp);
    }

    fn read_io(&mut self, port: u16, timestamp: Self::Timestamp) -> Option<u8> {
        self.device.as_mut().and_then(|device| device.read_io(port, timestamp))
                            .or_else(|| self.next_device.read_io(port, timestamp))
    }

    fn write_io(&mut self, port: u16, data: u8, timestamp: Self::Timestamp) -> bool {
        if let Some(device) = &mut self.device {
            if device.write_io(port, data, timestamp) {
                return true;
            }
        }
        self.next_device.write_io(port, data, timestamp)
    }
}
