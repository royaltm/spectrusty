//! This module hosts an emulation of modular system bus devices, that can be used with any [ControlUnit][crate::chip::ControlUnit].
pub mod ay;
pub mod debug;
pub mod zxprinter;
pub mod joystick;

use core::marker::PhantomData;

use crate::clock::{FTs, VFrameTsCounter, VideoTs};
use crate::memory::ZxMemory;

/// An interface that allows attaching many, different devices in a daisy chain.
///
/// Implementations of system bus devices should be provided to [ControlUnit][crate::chip::ControlUnit]
/// as its type argument.
pub trait BusDevice {
    /// A frame timestamp type. Must be the same as `Io::Timestamp` implemented by a [ControlUnit][crate::chip::ControlUnit]
    /// and for all devices in a days chain.
    type Timestamp: Sized;
    /// A type of the next device in a daisy chain.
    type NextDevice: BusDevice<Timestamp=Self::Timestamp>;

    /// Returns a mutable reference to the next device.
    fn next_device(&mut self) -> &mut Self::NextDevice;
    /// Resets the device and all devices in this chain. Used by [ControlUnit::reset][crate::chip::ControlUnit::reset].
    ///
    /// Default implementation forwards this call to the next device.
    ///
    /// **NOTE**: Implementors of bus devices should always forward this call after optionally applying it to `self`.
    #[inline(always)]
    fn reset(&mut self, timestamp: Self::Timestamp) {
        self.next_device().reset(timestamp)
    }
    /// This method should be called at the end of each frame by [ControlUnit::execute_next_frame][crate::chip::ControlUnit::execute_next_frame].
    ///
    /// If you need more fine grained timestamp increments implement [BusDevice::m1].
    ///
    /// Default implementation forwards this call to the next device.
    ///
    /// **NOTE**: Implementors of bus devices should always forward this call after optionally applying it to `self`.
    #[inline(always)]
    fn update_timestamp(&mut self, timestamp: Self::Timestamp) {
        self.next_device().update_timestamp(timestamp)
    }
    /// This method should be called just before a clock of control unit is adjusted when preparing for the next frame.
    ///
    /// It allows the devices that are tracking time to adjust stored timestamps accordingly using the provided
    /// end-of-frame `timestamp`, usually by subtracting it from the stored ones.
    ///
    /// Default implementation forwards this call to the next device.
    ///
    /// **NOTE**: Implementors of bus devices should always forward this call after optionally applying it to `self`.
    #[inline(always)]
    fn next_frame(&mut self, timestamp: Self::Timestamp) {
        self.next_device().next_frame(timestamp)
    }
    /// This method is called by the control unit during an op-code fetch `M1` cycles of Cpu.
    ///
    /// It can be used e.g. for implementing ROM traps.
    ///
    /// Default implementation forwards this call to the next device.
    ///
    /// **NOTE**: Implementors of bus devices should always forward this call after optionally applying it to `self`.
    #[inline(always)]
    fn m1<Z: ZxMemory>(&mut self, memory: &mut Z, pc: u16, timestamp: Self::Timestamp) {
        self.next_device().m1(memory, pc, timestamp)
    }
    /// This method is called by the control unit during an I/O read cycle.
    ///
    /// Default implementation forwards this call to the next device.
    ///
    /// **NOTE**: Implementors of bus devices should only forward this call if it does not apply to this device.
    /// If however the device should provide the result for the pending I/O read cycle, this method must return
    /// `Some(data)` and must not forward the call any further.
    #[inline(always)]
    fn read_io(&mut self, port: u16, timestamp: Self::Timestamp) -> Option<u8> {
        self.next_device().read_io(port, timestamp)
    }
    /// This method is called by the control unit during an I/O write cycle.
    ///
    /// Default implementation forwards this call to the next device.
    ///
    /// **NOTE**: Implementors of bus devices should only forward this call if it does not apply to this device.
    /// If however the device should use the data of the pending I/O write cycle, this method must return
    /// `true` and must not forward the call any further.
    #[inline(always)]
    fn write_io(&mut self, port: u16, data: u8, timestamp: Self::Timestamp) -> bool {
        self.next_device().write_io(port, data, timestamp)
    }
}

/// A helper trait for matching I/O port addresses.
pub trait PortAddress {
    /// Relevant address bits should be set to 1.
    const ADDRESS_MASK: u16;
    /// Bits from this constant will be matching only if `ADDRESS_MASK` constains 1 for bits in the same positions.
    const ADDRESS_BITS: u16;
    /// Returns `true` if a provided `address` masked with `ADDRESS_MASK` matches `ADDRESS_BITS`.
    #[inline]
    fn match_port(address: u16) -> bool {
        address & Self::ADDRESS_MASK == Self::ADDRESS_BITS & Self::ADDRESS_MASK
    }
}

/// A daisy-chain terminator device. Use it as the last device in a chain.
#[derive(Clone, Default, Debug)]
pub struct NullDevice<T: Sized>(PhantomData<T>);

impl<T: Sized> BusDevice for NullDevice<T> {
    type Timestamp = T;
    type NextDevice = Self;

    #[inline(always)]
    fn next_device(&mut self) -> &mut Self::NextDevice {
        self
    }
    #[inline(always)]
    fn reset(&mut self, _timestamp: Self::Timestamp) {}

    #[inline(always)]
    fn update_timestamp(&mut self, _timestamp: Self::Timestamp) {}

    #[inline(always)]
    fn next_frame(&mut self, _timestamp: Self::Timestamp) {}

    #[inline(always)]
    fn m1<Z: ZxMemory>(&mut self, _memory: &mut Z, _pc: u16, _timestamp: Self::Timestamp) {}

    #[inline(always)]
    fn read_io(&mut self, _port: u16, _timestamp: Self::Timestamp) -> Option<u8> {
        None
    }

    #[inline(always)]
    fn write_io(&mut self, _port: u16, _data: u8, _timestamp: Self::Timestamp) -> bool {
        false
    }
}

/// This is highly experimental, use at your own risk.
/// It may be removed or completely changed in a future.
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

    fn update_timestamp(&mut self, timestamp: Self::Timestamp) {
        if let Some(device) = &mut self.device {
            device.update_timestamp(timestamp);
        }
        self.next_device.update_timestamp(timestamp);
    }

    fn next_frame(&mut self, timestamp: Self::Timestamp) {
        if let Some(device) = &mut self.device {
            device.next_frame(timestamp);
        }
        self.next_device.next_frame(timestamp);
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
