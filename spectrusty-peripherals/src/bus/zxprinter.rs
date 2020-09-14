/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
//! A bus device for connecting **ZX Printer** family of printers.
use core::num::NonZeroU16;
use core::fmt;
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};

#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};

use spectrusty_core::{
    bus::{BusDevice, PortAddress},
    clock::FrameTimestamp
};
use super::ay::PassByAyAudioBusDevice;

pub use crate::zxprinter::*;

pub type ZxPrinter<S, D> = ZxPrinterBusDevice<ZxPrinterPortAddress, S, D>;
pub type Alphacom32<S, D> = ZxPrinterBusDevice<Alphacom32PortAddress, S, D>;
pub type TS2040<S, D> = ZxPrinterBusDevice<TS2040PortAddress, S, D>;

macro_rules! printer_names {
    ($($ty:ident: $name:expr),*) => { $(
        impl<S, D: BusDevice> fmt::Display for $ty<S, D> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str($name)
            }
        }
    )*};
}

printer_names! {
    ZxPrinter: "ZX Printer",
    Alphacom32: "Alphacom 32 Printer",
    TS2040: "TS2040 Printer"
}

/// Connects the [ZxPrinterDevice] emulator as a [BusDevice].
#[derive(Clone, Default, Debug)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "snapshot", serde(bound(deserialize = "
    S: Default,
    D: Deserialize<'de> + Default,
    D::Timestamp: Deserialize<'de> + Default",
serialize = "
    D: Serialize,
    D::Timestamp: Serialize")))]
pub struct ZxPrinterBusDevice<P, S, D: BusDevice>
{
    /// Provides a direct access to the [ZxPrinterDevice].
    #[cfg_attr(feature = "snapshot", serde(default))]
    pub printer: ZxPrinterDevice<D::Timestamp, S>,
    #[cfg_attr(feature = "snapshot", serde(default))]
    bus: D,
    #[cfg_attr(feature = "snapshot", serde(skip))]
    _port_decode: PhantomData<P>
}

#[derive(Clone, Copy, Default, Debug)]
pub struct ZxPrinterPortAddress;
impl PortAddress for ZxPrinterPortAddress {
    const ADDRESS_MASK: u16 = 0b0000_0000_0000_0100;
    const ADDRESS_BITS: u16 = 0b0000_0000_1111_1011;
}

#[derive(Clone, Copy, Default, Debug)]
pub struct Alphacom32PortAddress;
impl PortAddress for Alphacom32PortAddress {
    const ADDRESS_MASK: u16 = 0b0000_0000_0100_0100;
    const ADDRESS_BITS: u16 = 0b0000_0000_1111_1011;
}

#[derive(Clone, Copy, Default, Debug)]
pub struct TS2040PortAddress;
impl PortAddress for TS2040PortAddress {
    const ADDRESS_MASK: u16 = 0b0000_0000_1000_0100;
    const ADDRESS_BITS: u16 = 0b0000_0000_1111_1011;
}

impl<P, S, D: BusDevice> Deref for ZxPrinterBusDevice<P, S, D> {
    type Target = ZxPrinterDevice<D::Timestamp, S>;
    fn deref(&self) -> &Self::Target {
        &self.printer
    }
}

impl<P, S, D: BusDevice> DerefMut for ZxPrinterBusDevice<P, S, D> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.printer
    }
}

impl<P, S, D: BusDevice> PassByAyAudioBusDevice for ZxPrinterBusDevice<P, S, D> {}

impl<P, S, D> BusDevice for ZxPrinterBusDevice<P, S, D>
    where P: PortAddress,
          S: Spooler,
          D: BusDevice,
          D::Timestamp: FrameTimestamp
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
        self.printer.reset();
        self.bus.reset(timestamp);
    }

    #[inline]
    fn next_frame(&mut self, timestamp: Self::Timestamp) {
        self.printer.next_frame();
        self.bus.next_frame(timestamp)
    }

    #[inline]
    fn read_io(&mut self, port: u16, timestamp: Self::Timestamp) -> Option<(u8, Option<NonZeroU16>)> {
        let bus_data = self.bus.read_io(port, timestamp);
        if P::match_port(port) {
            let pr_data = self.printer.read_status(timestamp);
            if let Some((data, ws)) = bus_data {
                return Some((data & pr_data, ws))
            }
            return Some((pr_data, None))
        }
        bus_data
    }

    #[inline]
    fn write_io(&mut self, port: u16, data: u8, timestamp: Self::Timestamp) -> Option<u16> {
        if P::match_port(port) {
            // println!("print: {:04x} {:b}", port, data);
            self.printer.write_control(data, timestamp);
            return Some(0);
        }
        self.bus.write_io(port, data, timestamp)
    }
}
