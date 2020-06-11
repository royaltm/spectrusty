//! A passthrough debugging device.
use core::num::NonZeroU16;
use core::fmt::Debug;

#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};

#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

use spectrusty_core::bus::BusDevice;
use super::ay::PassByAyAudioBusDevice;

/// A passthrough [BusDevice] that outputs I/O data read and written by CPU using [log] `debug`.
#[derive(Clone, Default, Debug)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
pub struct DebugBusDevice<D> {
    #[cfg_attr(feature = "snapshot", serde(default))]
    bus: D,
}

impl<D: BusDevice> BusDevice for DebugBusDevice<D>
    where D::Timestamp: Debug
{
    type Timestamp = D::Timestamp;
    type NextDevice = D;

    fn next_device_mut(&mut self) -> &mut Self::NextDevice {
        &mut self.bus
    }
    fn next_device_ref(&self) -> &Self::NextDevice {
        &self.bus
    }
    fn into_next_device(self) -> Self::NextDevice {
        self.bus
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

impl<D> PassByAyAudioBusDevice for DebugBusDevice<D> {}
