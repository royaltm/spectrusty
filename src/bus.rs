//! This module hosts an emulation of system bus devices, that can be used with any [ControlUnit][crate::chip::ControlUnit].
pub mod ay;
pub mod debug;
pub mod zxprinter;
pub mod joystick;

use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};

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
    fn next_device_mut(&mut self) -> &mut Self::NextDevice;
    /// Returns a reference to the next device.
    fn next_device_ref(&self) -> &Self::NextDevice;
    /// Resets the device and all devices in this chain. Used by [ControlUnit::reset][crate::chip::ControlUnit::reset].
    ///
    /// Default implementation forwards this call to the next device.
    ///
    /// **NOTE**: Implementors of bus devices should always forward this call after optionally applying it to `self`.
    #[inline(always)]
    fn reset(&mut self, timestamp: Self::Timestamp) {
        self.next_device_mut().reset(timestamp)
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
        self.next_device_mut().update_timestamp(timestamp)
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
        self.next_device_mut().next_frame(timestamp)
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
        self.next_device_mut().m1(memory, pc, timestamp)
    }
    /// This method is called by the control unit during an I/O read cycle.
    ///
    /// Default implementation forwards this call to the next device.
    ///
    /// **NOTE**: Implementations of bus devices should only need to forward this call if it does not apply
    /// to this device or if not all bits are modified by the implementing device. In the latter case
    /// the result from the forwarded call should be logically `ANDed` with the result of reading form this
    ///  device and if the upstream result is `None` the result should be returned with all unused bits set to 1.
    #[inline(always)]
    fn read_io(&mut self, port: u16, timestamp: Self::Timestamp) -> Option<u8> {
        self.next_device_mut().read_io(port, timestamp)
    }
    /// This method is called by the control unit during an I/O write cycle.
    ///
    /// Returns `true` if the device blocked writing through.
    ///
    /// Default implementation forwards this call to the next device.
    ///
    /// **NOTE**: Implementations of bus devices should only forward this call to the next device
    /// if it does not apply to this device or if the device doesn't block writing.
    /// If the device blocks writing to next devices and the port matches the device this method must
    /// return `true`. Otherwise this method should return the forwarded result.
    #[inline(always)]
    fn write_io(&mut self, port: u16, data: u8, timestamp: Self::Timestamp) -> bool {
        self.next_device_mut().write_io(port, data, timestamp)
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
    fn next_device_mut(&mut self) -> &mut Self::NextDevice {
        self
    }
    #[inline(always)]
    fn next_device_ref(&self) -> &Self::NextDevice {
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

/// A [BusDevice] allowing for plugging in and out the device during run time.
#[derive(Clone, Default, Debug)]
pub struct OptionalBusDevice<D, N=NullDevice<VideoTs>> {
    pub device: Option<D>,
    next_device: N
}

impl<D, N> OptionalBusDevice<D, N>
    where D: BusDevice, N: BusDevice
{
    pub fn new(device: Option<D>, next_device: N) -> Self {
        OptionalBusDevice { device, next_device }
    }
}

impl<D, N> Deref for OptionalBusDevice<D, N> {
    type Target = Option<D>;
    fn deref(&self) -> &Self::Target {
        &self.device
    }
}

impl<D, N> DerefMut for OptionalBusDevice<D, N> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.device
    }
}

impl<D, N> BusDevice for OptionalBusDevice<D, N>
    where D: BusDevice,
          N: BusDevice<Timestamp=D::Timestamp>,
          D::Timestamp: Copy
{
    type Timestamp = D::Timestamp;
    type NextDevice = N;

    #[inline]
    fn next_device_mut(&mut self) -> &mut Self::NextDevice {
        &mut self.next_device
    }
    #[inline]
    fn next_device_ref(&self) -> &Self::NextDevice {
        &self.next_device
    }
    #[inline]
    fn reset(&mut self, timestamp: Self::Timestamp) {
        if let Some(device) = &mut self.device {
            device.reset(timestamp);
        }
        self.next_device.reset(timestamp);
    }
    #[inline]
    fn update_timestamp(&mut self, timestamp: Self::Timestamp) {
        if let Some(device) = &mut self.device {
            device.update_timestamp(timestamp);
        }
        self.next_device.update_timestamp(timestamp);
    }
    #[inline]
    fn next_frame(&mut self, timestamp: Self::Timestamp) {
        if let Some(device) = &mut self.device {
            device.next_frame(timestamp);
        }
        self.next_device.next_frame(timestamp);
    }
    #[inline]
    fn m1<Z: ZxMemory>(&mut self, memory: &mut Z, pc: u16, timestamp: Self::Timestamp) {
        if let Some(device) = &mut self.device {
            device.m1(memory, pc, timestamp);
        }
        self.next_device.m1(memory, pc, timestamp);
    }
    #[inline]
    fn read_io(&mut self, port: u16, timestamp: Self::Timestamp) -> Option<u8> {
        let bus_data = self.next_device.read_io(port, timestamp);
        let dev_data = self.device.as_mut().and_then(|dev| dev.read_io(port, timestamp));
        match (bus_data, dev_data) {
            (Some(bus_data), Some(dev_data)) => Some(bus_data & dev_data),
            (Some(data), None)|(None, Some(data)) => Some(data),
            (None, None) => None
        }
    }
    #[inline]
    fn write_io(&mut self, port: u16, data: u8, timestamp: Self::Timestamp) -> bool {
        if let Some(device) = &mut self.device {
            if device.write_io(port, data, timestamp) {
                return true;
            }
        }
        self.next_device.write_io(port, data, timestamp)
    }
}
