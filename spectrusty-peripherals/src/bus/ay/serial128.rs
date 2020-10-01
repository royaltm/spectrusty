/*
    Copyright (C) 2020  Rafal Michalski

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
pub type Ay3_8912Keypad<T, D> = Ay3_891xBusDevice<
                                                Ay128kPortDecode,
                                                SerialPorts128<
                                                    SerialKeypad<T>,
                                                    NullSerialPort<T>
                                                >,
                                                AyIoNullPort<T>, D>;

/// This type implements a [BusDevice][spectrusty_core::bus::BusDevice] emulating AY-3-8912 with a [RS-232][Rs232Io]
/// communication.
pub type Ay3_8912Rs232<T, D, R, W> = Ay3_891xBusDevice<
                                                Ay128kPortDecode,
                                                SerialPorts128<
                                                    NullSerialPort<T>,
                                                    Rs232Io<T, R, W>
                                                >,
                                                AyIoNullPort<T>, D>;

/// This type implements a [BusDevice][spectrusty_core::bus::BusDevice] emulating AY-3-8912 with extension
/// [keypad][SerialKeypad] and [RS-232][Rs232Io] communication.
pub type Ay3_8912KeypadRs232<T, D, R, W> = Ay3_891xBusDevice<
                                                Ay128kPortDecode,
                                                SerialPorts128<
                                                    SerialKeypad<T>,
                                                    Rs232Io<T, R, W>
                                                >,
                                                AyIoNullPort<T>, D>;

impl<T, D: BusDevice> fmt::Display for Ay3_8912Keypad<T, D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AY-3-8912 + Keypad")
    }
}

impl<T, D: BusDevice, R, W> fmt::Display for Ay3_8912Rs232<T, D, R, W> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AY-3-8912 + RS-232")
    }
}

impl<T, D: BusDevice, R, W> fmt::Display for Ay3_8912KeypadRs232<T, D, R, W> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AY-3-8912 + Keypad + RS-232")
    }
}
