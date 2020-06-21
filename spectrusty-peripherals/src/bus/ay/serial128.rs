//! A collection of custom [Ay3_891xBusDevice] types for Spectrum's 128k with [SerialPorts128].
use core::fmt;
use spectrusty_core::{bus::BusDevice, clock::VFrameTs};
use crate::ay::{AyIoNullPort, Ay128kPortDecode};
pub use crate::ay::serial128::SerialPorts128;
pub use crate::serial::{NullSerialPort, Rs232Io, SerialKeypad};
use super::Ay3_891xBusDevice;


/// This type implements a [BusDevice][spectrusty_core::bus::BusDevice] emulating AY-3-8912 with an extension
/// [keypad][SerialKeypad].
pub type Ay3_8912Keypad<V, D> = Ay3_891xBusDevice<
                                                Ay128kPortDecode,
                                                SerialPorts128<
                                                    SerialKeypad<V>,
                                                    NullSerialPort<VFrameTs<V>>
                                                >,
                                                AyIoNullPort<VFrameTs<V>>, D>;

/// This type implements a [BusDevice][spectrusty_core::bus::BusDevice] emulating AY-3-8912 with a [RS-232][Rs232Io]
/// communication.
pub type Ay3_8912Rs232<V, D, R, W> = Ay3_891xBusDevice<
                                                Ay128kPortDecode,
                                                SerialPorts128<
                                                    NullSerialPort<VFrameTs<V>>,
                                                    Rs232Io<V, R, W>
                                                >,
                                                AyIoNullPort<VFrameTs<V>>, D>;

/// This type implements a [BusDevice][spectrusty_core::bus::BusDevice] emulating AY-3-8912 with extension
/// [keypad][SerialKeypad] and [RS-232][Rs232Io] communication.
pub type Ay3_8912KeypadRs232<V, D, R, W> = Ay3_891xBusDevice<
                                                Ay128kPortDecode,
                                                SerialPorts128<
                                                    SerialKeypad<V>,
                                                    Rs232Io<V, R, W>
                                                >,
                                                AyIoNullPort<VFrameTs<V>>, D>;

impl<V, D: BusDevice> fmt::Display for Ay3_8912Keypad<V, D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AY-3-8912 + Keypad")
    }
}

impl<V, D: BusDevice, R, W> fmt::Display for Ay3_8912Rs232<V, D, R, W> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AY-3-8912 + RS-232")
    }
}

impl<V, D: BusDevice, R, W> fmt::Display for Ay3_8912KeypadRs232<V, D, R, W> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AY-3-8912 + Keypad + RS-232")
    }
}
