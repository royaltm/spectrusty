/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
use core::mem;
use core::num::NonZeroU16;
use core::fmt::{Display, Debug};
use core::iter::IntoIterator;
use core::ops::{Index, IndexMut};

#[cfg(feature = "snapshot")]
use ::serde::{Serialize, Deserialize};

#[cfg(feature = "snapshot")]
mod serde;

#[cfg(feature = "snapshot")]
pub use self::serde::*;

use super::{BusDevice, VFNullDevice, NullDevice};

/// A trait for dynamic bus devices, which currently includes methods from [Display] and [BusDevice].
/// Devices implementing this trait can be used with a [DynamicBus].
///
/// Implemented for all types that implement dependent traits.
pub trait NamedBusDevice<T: Debug>: Display + BusDevice<Timestamp=T, NextDevice=NullDevice<T>>{}

impl<T: Debug, D> NamedBusDevice<T> for D where D: Display + BusDevice<Timestamp=T, NextDevice=NullDevice<T>>{}

/// A type of a dynamic [NamedBusDevice].
pub type NamedDynDevice<T> = dyn NamedBusDevice<T>;
/// A type of a boxed dynamic [NamedBusDevice].
///
/// This is a type of items stored by [DynamicBus].
pub type BoxNamedDynDevice<T> = Box<dyn NamedBusDevice<T>>;

/// A terminated [DynamicBus] pseudo-device with [`VFNullDevice<V>`][VFNullDevice].
pub type DynamicVBus<V> = DynamicBus<VFNullDevice<V>>;

/// A pseudo bus device that allows for adding and removing devices of different types at run time.
///
/// The penalty is that the access to the devices must be done using a virtual call dispatch.
/// Also the device of this type can't be cloned (nor the [ControlUnit][crate::chip::ControlUnit]
/// with this device attached).
///
/// `DynamicBus<D>` implements [BusDevice] itself, so obviously it's possible to declare a statically
/// dispatched downstream [BusDevice] as its parameter `D`.
///
/// Currently only types implementing [BusDevice] that are directly terminated with [NullDevice]
/// can be attached as dynamically dispatched objects.
#[derive(Default, Debug)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
pub struct DynamicBus<D: BusDevice> {
    #[cfg_attr(feature = "snapshot", serde(default))]
    bus: D,
    #[cfg_attr(feature = "snapshot", serde(skip))]
    devices: Vec<BoxNamedDynDevice<D::Timestamp>>
}

impl<'a, T: Debug, D: 'a> From<D> for Box<dyn NamedBusDevice<T> + 'a>
    where D: BusDevice<Timestamp=T, NextDevice=NullDevice<T>> + Display
{
    fn from(dev: D) -> Self {
        Box::new(dev)
    }
}

impl<D: BusDevice> AsRef<[BoxNamedDynDevice<D::Timestamp>]> for DynamicBus<D> {
    fn as_ref(&self) -> &[BoxNamedDynDevice<D::Timestamp>] {
        self.devices.as_slice()
    }
}

impl<D: BusDevice> AsMut<[BoxNamedDynDevice<D::Timestamp>]> for DynamicBus<D> {
    fn as_mut(&mut self) -> &mut[BoxNamedDynDevice<D::Timestamp>] {
        self.devices.as_mut_slice()
    }
}

impl<D> DynamicBus<D>
    where D: BusDevice
{
    /// Returns the number of attached devices.
    pub fn len(&self) -> usize {
        self.devices.len()
    }
    /// Returns `true` if there are no devices in the dynamic chain. Otherwise returns `false`.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Appends an instance of a `device` at the end of the daisy-chain.
    /// Returns its index position in the dynamic device chain.
    pub fn append_device<B>(&mut self, device: B) -> usize
        where B: Into<BoxNamedDynDevice<D::Timestamp>>
    {
        self.devices.push(device.into());
        self.devices.len() - 1
    }
    /// Removes the last device from the dynamic daisy-chain and returns an instance of the boxed
    /// dynamic object.
    pub fn remove_device(&mut self) -> Option<BoxNamedDynDevice<D::Timestamp>> {
        self.devices.pop()
    }
    /// Replaces a device at the given `index` position and returns it.
    /// 
    /// The removed device is replaced by the last device of the chain.
    ///
    /// # Panics
    /// Panics if a device doesn't exist at `index`.
    pub fn swap_remove_device(&mut self, index: usize) -> BoxNamedDynDevice<D::Timestamp> {
        self.devices.swap_remove(index)
    }
    /// Replaces a device at the given `index` position. Returns the previous device occupying the replaced spot.
    ///
    /// # Panics
    /// Panics if a device doesn't exist at `index`.
    pub fn replace_device<B>(&mut self, index: usize, device: B) -> BoxNamedDynDevice<D::Timestamp>
        where B: Into<BoxNamedDynDevice<D::Timestamp>>
    {
        mem::replace(&mut self.devices[index], device.into())
    }
    /// Removes all dynamic devices from the dynamic daisy-chain.
    pub fn clear(&mut self) {
        self.devices.clear();
    }
    /// Returns a reference to a dynamic device at `index` position in the dynamic daisy-chain.
    #[inline]
    pub fn get_device_ref(&self, index: usize) -> Option<&NamedDynDevice<D::Timestamp>> {
        // self.devices[index].as_ref()
        self.devices.get(index).map(|d| d.as_ref())
    }
    /// Returns a mutable reference to a dynamic device at `index` position in the dynamic daisy-chain.
    #[inline]
    pub fn get_device_mut(&mut self, index: usize) -> Option<&mut NamedDynDevice<D::Timestamp>> {
        self.devices.get_mut(index).map(|d| d.as_mut())
    }
}

impl<D> DynamicBus<D>
    where D: BusDevice,
          D::Timestamp: Debug + 'static
{
    /// Removes the last device from the dynamic daisy-chain.
    ///
    /// # Panics
    /// Panics if a device is not of a type given as parameter `B`.
    pub fn remove_as_device<B>(&mut self) -> Option<Box<B>>
        where B: NamedBusDevice<D::Timestamp> + 'static
    {
        self.remove_device().map(|boxdev|
            boxdev.downcast::<B>().expect("wrong dynamic device type removed")
        )
    }
    /// Replaces a device at the given `index` and returns it.
    /// 
    /// The removed device is replaced by the last device of the chain.
    ///
    /// # Panics
    /// Panics if a device doesn't exist at `index` or if a device is not of a type given as parameter `B`.
    pub fn swap_remove_as_device<B>(&mut self, index: usize) -> Box<B>
        where B: NamedBusDevice<D::Timestamp> + 'static
    {
        self.swap_remove_device(index).downcast::<B>().expect("wrong dynamic device type removed")
    }
    /// Returns a reference to a device of a type `B` at `index` in the dynamic daisy-chain.
    ///
    /// # Panics
    /// Panics if a device doesn't exist at `index` or if a device is not of a type given as parameter `B`.
    #[inline]
    pub fn as_device_ref<B>(&self, index: usize) -> &B
        where B: NamedBusDevice<D::Timestamp> + 'static
    {
        self.devices[index].downcast_ref::<B>().expect("wrong dynamic device type")
    }
    /// Returns a mutable reference to a device of a type `B` at `index` in the dynamic daisy-chain.
    ///
    /// # Panics
    /// Panics if a device doesn't exist at `index` or if a device is not of a type given as parameter `B`.
    #[inline]
    pub fn as_device_mut<B>(&mut self, index: usize) -> &mut B
        where B: NamedBusDevice<D::Timestamp> + 'static
    {
        self.devices[index].downcast_mut::<B>().expect("wrong dynamic device type")
    }
    /// Returns `true` if a device at `index` is of a type given as parameter `B`.
    #[inline]
    pub fn is_device<B>(&self, index: usize) -> bool
        where B: NamedBusDevice<D::Timestamp> + 'static
    {
        self.devices.get(index).map(|d| d.is::<B>()).unwrap_or(false)
    }
    /// Searches for a first device of a type given as parameter `B`, returning its index.
    #[inline]
    pub fn position_device<B>(&self) -> Option<usize>
        where B: NamedBusDevice<D::Timestamp> + 'static
    {
        self.devices.iter().position(|d| d.is::<B>())
    }
    /// Searches for a first device of a type given as parameter `B`, returning a reference to a device.
    #[inline]
    pub fn find_device_ref<B>(&self) -> Option<&B>
        where B: NamedBusDevice<D::Timestamp> + 'static
    {
        self.devices.iter().find_map(|d| d.downcast_ref::<B>())
    }
    /// Searches for a first device of a type given as parameter `B`, returning a mutable reference to a device.
    #[inline]
    pub fn find_device_mut<B>(&mut self) -> Option<&mut B>
        where B: NamedBusDevice<D::Timestamp> + 'static
    {
        self.devices.iter_mut().find_map(|d| d.downcast_mut::<B>())
    }
}

impl<D: BusDevice> Index<usize> for DynamicBus<D> {
    type Output = NamedDynDevice<D::Timestamp>;
    #[inline]
    fn index(&self, index: usize) -> &Self::Output {
        self.devices[index].as_ref()
    }
}

impl<D: BusDevice> IndexMut<usize> for DynamicBus<D> {
    #[inline]
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        self.devices[index].as_mut()
    }
}

impl<'a, D: BusDevice> IntoIterator for &'a DynamicBus<D> {
    type Item = &'a BoxNamedDynDevice<D::Timestamp>;
    type IntoIter = core::slice::Iter<'a, BoxNamedDynDevice<D::Timestamp>>;
    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.devices.iter()
    }
}

impl<'a, D: BusDevice> IntoIterator for &'a mut DynamicBus<D> {
    type Item = &'a mut BoxNamedDynDevice<D::Timestamp>;
    type IntoIter = core::slice::IterMut<'a, BoxNamedDynDevice<D::Timestamp>>;
    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.devices.iter_mut()
    }
}

impl<D: BusDevice> IntoIterator for DynamicBus<D> {
    type Item = BoxNamedDynDevice<D::Timestamp>;
    type IntoIter = std::vec::IntoIter<BoxNamedDynDevice<D::Timestamp>>;
    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.devices.into_iter()
    }
}

impl<D> BusDevice for DynamicBus<D>
    where D: BusDevice,
          D::Timestamp: Debug + Copy
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
    fn reset(&mut self, timestamp: Self::Timestamp) {
        for dev in self.devices.iter_mut() {
            dev.reset(timestamp);
        }
        self.bus.reset(timestamp);
    }

    #[inline]
    fn update_timestamp(&mut self, timestamp: Self::Timestamp) {
        for dev in self.devices.iter_mut() {
            dev.update_timestamp(timestamp);
        }
        self.bus.update_timestamp(timestamp);
    }

    #[inline]
    fn next_frame(&mut self, timestamp: Self::Timestamp) {
        for dev in self.devices.iter_mut() {
            dev.next_frame(timestamp);
        }
        self.bus.next_frame(timestamp);
    }

    #[inline]
    fn read_io(&mut self, port: u16, timestamp: Self::Timestamp) -> Option<(u8, Option<NonZeroU16>)> {
        let mut bus_data = None;
        for dev in self.devices.iter_mut() {
            if let Some((data, ws)) = dev.read_io(port, timestamp) {
                let data = data & bus_data.unwrap_or(!0);
                if ws.is_some() {
                    return Some((data, ws));
                }
                bus_data = Some(data);
            }
        }
        if let Some((data, ws)) = self.bus.read_io(port, timestamp) {
            return Some((data & bus_data.unwrap_or(!0), ws))
        }
        bus_data.map(|data| (data, None))
    }

    #[inline]
    fn write_io(&mut self, port: u16, data: u8, timestamp: Self::Timestamp) -> Option<u16> {
        for dev in self.devices.iter_mut() {
            if let Some(res) = dev.write_io(port, data, timestamp) {
                return Some(res);
            }
        }
        self.bus.write_io(port, data, timestamp)
    }
}

#[cfg(test)]
mod tests {
    use core::fmt;
    use super::*;

    #[derive(Default, Clone, PartialEq, Debug)]
    struct TestDevice {
        foo: i32,
        data: u8,
        bus: NullDevice<i32>
    }

    impl fmt::Display for TestDevice {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("Test Device")
        }
    }

    impl BusDevice for TestDevice {
        type Timestamp = i32;
        type NextDevice = NullDevice<i32>;

        fn next_device_mut(&mut self) -> &mut Self::NextDevice {
            &mut self.bus
        }
        fn next_device_ref(&self) -> &Self::NextDevice {
            &self.bus
        }
        fn into_next_device(self) -> Self::NextDevice {
            self.bus
        }
        fn reset(&mut self, timestamp: Self::Timestamp) {
            self.foo = i32::min_value() + timestamp;
        }
        fn update_timestamp(&mut self, timestamp: Self::Timestamp) {
            self.foo = timestamp
        }
        fn read_io(&mut self, _port: u16, timestamp: Self::Timestamp) -> Option<(u8, Option<NonZeroU16>)> {
            if self.foo == timestamp {
                Some((self.data, None))
            }
            else {
                None
            }
        }
        fn write_io(&mut self, _port: u16, data: u8, timestamp: Self::Timestamp) -> Option<u16> {
            self.data = data;
            self.foo = timestamp;
            Some(0)
        }
    }

    #[test]
    fn dynamic_bus_device_works() {
        let mut dchain: DynamicBus<NullDevice<i32>> = Default::default();
        assert_eq!(dchain.len(), 0);
        assert_eq!(dchain.write_io(0, 0, 0), None);
        assert_eq!(dchain.read_io(0, 0), None);
        let test_dev: Box<dyn NamedBusDevice<_>> = Box::new(TestDevice::default());
        let index = dchain.append_device(test_dev);
        assert_eq!(dchain.is_device::<TestDevice>(index), true);
        assert_eq!(index, 0);
        assert_eq!(dchain.len(), 1);
        let device = dchain.remove_device().unwrap();
        assert_eq!(device.is::<TestDevice>(), true);
        assert_eq!(dchain.len(), 0);

        let index0 = dchain.append_device(NullDevice::default());
        assert_eq!(index0, 0);
        assert_eq!(dchain.is_device::<TestDevice>(index0), false);
        assert_eq!(dchain.is_device::<NullDevice<_>>(index0), true);
        assert_eq!(dchain.len(), 1);
        let dev: &NullDevice<_> = dchain.as_device_ref(index0);
        assert_eq!(dev, &NullDevice::default());

        let index1 = dchain.append_device(TestDevice::default());
        assert_eq!(index1, 1);
        assert_eq!(dchain.is_device::<TestDevice>(index1), true);
        assert_eq!(dchain.is_device::<TestDevice>(index0), false);
        assert_eq!(dchain.is_device::<TestDevice>(usize::max_value()), false);
        assert_eq!(dchain.is_device::<NullDevice<_>>(index0), true);
        assert_eq!(dchain.is_device::<NullDevice<_>>(index1), false);
        assert_eq!(dchain.is_device::<NullDevice<_>>(usize::max_value()), false);
        let dev = dchain.get_device_ref(index1).unwrap();
        assert_eq!(dev.is::<TestDevice>(), true);
        assert_eq!(dev.is::<NullDevice<_>>(), false);
        assert_eq!(format!("{}", dev), "Test Device");
        if let Some(dev) = dchain.get_device_mut(index1) {
            dev.update_timestamp(777);
            assert_eq!(dev.read_io(0, 0), None);
            assert_eq!(dev.read_io(0, 777), Some((0, None)));
        }
        assert_eq!(dchain.len(), 2);
        assert_eq!(dchain.write_io(0, 42, 131999), Some(0));
        assert_eq!(dchain.read_io(0, 0), None);
        assert_eq!(dchain.read_io(0, 131999), Some((42, None)));
        let dev: &TestDevice = dchain.as_device_ref(index1);
        assert_eq!(dev.data, 42);
        assert_eq!(dev.foo, 131999);
        let dev: &mut TestDevice = dchain.as_device_mut(index1);
        dev.data = 199;
        dev.foo = -1;
        let dev: &TestDevice = dchain.as_device_ref(index1);
        assert_eq!(dev.data, 199);
        assert_eq!(dev.foo, -1);
        let device: TestDevice = *dchain.remove_as_device().unwrap();
        assert_eq!(dchain.len(), 1);
        assert_eq!(device, TestDevice {
            foo: -1,
            data: 199,
            bus: NullDevice::<i32>::default()
        });
    }
}
