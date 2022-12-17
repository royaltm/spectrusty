/*
    Copyright (C) 2020-2022  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
//! A collection of custom [Ay3_891xBusDevice] types for Spectrum's 128k with [SerialPorts128].
use core::fmt;
use spectrusty_core::bus::BusDevice;
use crate::ay::{AyIoNullPort, Ay128kPortDecode};
pub use crate::ay::serial128::SerialPorts128;
pub use crate::serial::{NullSerialPort, Rs232Io, SerialKeypad};
use super::Ay3_891xBusDevice;


/// This type implements a [BusDevice][spectrusty_core::bus::BusDevice] emulating AY-3-8912 with an extension
/// [keypad][SerialKeypad].
pub type Ay3_8912Keypad<D> = Ay3_891xBusDevice<
                                                Ay128kPortDecode,
                                                SerialPorts128<
                                                    SerialKeypad<<D as BusDevice>::Timestamp>,
                                                    NullSerialPort<<D as BusDevice>::Timestamp>
                                                >,
                                                AyIoNullPort<<D as BusDevice>::Timestamp>, D>;

/// This type implements a [BusDevice][spectrusty_core::bus::BusDevice] emulating AY-3-8912 with a [RS-232][Rs232Io]
/// communication.
pub type Ay3_8912Rs232<D, R, W> = Ay3_891xBusDevice<
                                                Ay128kPortDecode,
                                                SerialPorts128<
                                                    NullSerialPort<<D as BusDevice>::Timestamp>,
                                                    Rs232Io<<D as BusDevice>::Timestamp, R, W>
                                                >,
                                                AyIoNullPort<<D as BusDevice>::Timestamp>, D>;

/// This type implements a [BusDevice][spectrusty_core::bus::BusDevice] emulating AY-3-8912 with extension
/// [keypad][SerialKeypad] and [RS-232][Rs232Io] communication.
pub type Ay3_8912KeypadRs232<D, R, W> = Ay3_891xBusDevice<
                                                Ay128kPortDecode,
                                                SerialPorts128<
                                                    SerialKeypad<<D as BusDevice>::Timestamp>,
                                                    Rs232Io<<D as BusDevice>::Timestamp, R, W>
                                                >,
                                                AyIoNullPort<<D as BusDevice>::Timestamp>, D>;

impl<D: BusDevice> fmt::Display for Ay3_8912Keypad<D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AY-3-8912 + Keypad")
    }
}

impl<D: BusDevice, R, W> fmt::Display for Ay3_8912Rs232<D, R, W> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AY-3-8912 + RS-232")
    }
}

impl<D: BusDevice, R, W> fmt::Display for Ay3_8912KeypadRs232<D, R, W> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AY-3-8912 + Keypad + RS-232")
    }
}
