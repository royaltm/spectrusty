//! A passthrough debugging device.
use core::num::NonZeroU16;
use core::fmt::Debug;
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};

#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};

#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

use crate::bus::BusDevice;
use super::ay::PassByAyAudioBusDevice;

/// A passthrough [BusDevice] that outputs I/O data read and written by CPU using [log] `debug`.
#[derive(Clone, Default, Debug)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
pub struct DebugBusDevice<T, D> {
    #[cfg_attr(feature = "snapshot", serde(default))]
    bus: D,
    #[cfg_attr(feature = "snapshot", serde(skip))]
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
    fn read_io(&mut self, port: u16, timestamp: Self::Timestamp) -> Option<(u8, Option<NonZeroU16>)> {
        debug!("read_io: {:04x} {:?}", port, timestamp);
        self.bus.read_io(port, timestamp)
    }
    /// Called by the control unit on IO::write_io.
    fn write_io(&mut self, port: u16, data: u8, timestamp: Self::Timestamp) -> Option<u16> {
        debug!("write_io: {:04x} {:02x} {:?}", port, data, timestamp);
        self.bus.write_io(port, data, timestamp)
    }
}

impl<T, D> PassByAyAudioBusDevice for DebugBusDevice<T, D> {}
