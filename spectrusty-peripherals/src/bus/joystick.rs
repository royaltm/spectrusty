/*
    Copyright (C) 2020-2022  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
//! A bus device for connecting joysticks.
use core::str::FromStr;
use core::convert::TryFrom;
use core::num::NonZeroU16;
use core::fmt;
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};
use std::borrow::Cow;

#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};

use spectrusty_core::{
    bus::{BusDevice, PortAddress}
};

use super::ay::PassByAyAudioBusDevice;

pub use crate::joystick::{
    JoystickDevice, JoystickInterface, NullJoystickDevice,
    kempston::*, fuller::*, sinclair::*, cursor::*
};

/// A convenient Kempston Joystick [BusDevice] type.
pub type KempstonJoystick<D> = JoystickBusDevice<
                                                KempstonJoyPortAddress,
                                                KempstonJoystickDevice,
                                                D>;
/// A convenient Fuller Joystick [BusDevice] type.
pub type FullerJoystick<D> = JoystickBusDevice<
                                                FullerJoyPortAddress,
                                                FullerJoystickDevice,
                                                D>;
/// A convenient pair of Left and Right Joystick [BusDevice] type.
pub type SinclairJoystick<D> = SinclairLeftJoystick<SinclairRightJoystick<D>>;
/// A convenient Right (Player 1) Sinclair Joystick [BusDevice] type.
pub type SinclairRightJoystick<D> = JoystickBusDevice<
                                                    SinclairRightJoyPortAddress,
                                                    SinclairJoystickDevice<SinclairJoyRightMap>,
                                                    D>;
/// A convenient Left (Player 2) Sinclair Joystick [BusDevice] type.
pub type SinclairLeftJoystick<D> = JoystickBusDevice<
                                                    SinclairLeftJoyPortAddress,
                                                    SinclairJoystickDevice<SinclairJoyLeftMap>,
                                                    D>;
/// A convenient Cursor Joystick [BusDevice] type.
pub type CursorJoystick<D> = JoystickBusDevice<
                                                CursorJoyPortAddress,
                                                CursorJoystickDevice,
                                                D>;
macro_rules! joystick_names {
    ($($ty:ty: $name:expr),*) => { $(
        impl<D> fmt::Display for $ty {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str($name)
            }
        }
    )*};
}

joystick_names! {
    KempstonJoystick<D>: "Kempston Joystick",
    FullerJoystick<D>: "Fuller Joystick",
    SinclairRightJoystick<D>: "Sinclair #1 Joystick",
    SinclairLeftJoystick<D>: "Sinclair #2 Joystick",
    CursorJoystick<D>: "Cursor Joystick"
}

/// A joystick controller, providing a [BusDevice] implementation that can be used with [joystick devices][JoystickDevice].
#[derive(Clone, Default, Debug)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
pub struct JoystickBusDevice<P, J, D>
{
    /// A [JoystickDevice] implementation, which may also implement [JoystickInterface] trait
    /// for providing user input.
    #[cfg_attr(feature = "snapshot", serde(skip))]
    pub joystick: J,
    #[cfg_attr(feature = "snapshot", serde(default))]
    bus: D,
    #[cfg_attr(feature = "snapshot", serde(skip))]
    _port_decode: PhantomData<P>,
}

/// Kempston Joystick [PortAddress].
#[derive(Clone, Copy, Default, Debug)]
pub struct KempstonJoyPortAddress;
impl PortAddress for KempstonJoyPortAddress {
    const ADDRESS_MASK: u16 = 0x0020;
    const ADDRESS_BITS: u16 = 0x001f;
}
/// Fuller Joystick [PortAddress].
#[derive(Clone, Copy, Default, Debug)]
pub struct FullerJoyPortAddress;
impl PortAddress for FullerJoyPortAddress {
    const ADDRESS_MASK: u16 = 0x00ff;
    const ADDRESS_BITS: u16 = 0x007f;
}
/// Left Sinclair Joystick [PortAddress].
#[derive(Clone, Copy, Default, Debug)]
pub struct SinclairLeftJoyPortAddress;
impl PortAddress for SinclairLeftJoyPortAddress {
    const ADDRESS_MASK: u16 = 0x0800;
    const ADDRESS_BITS: u16 = 0xf7fe;
}
/// Right Sinclair Joystick [PortAddress].
#[derive(Clone, Copy, Default, Debug)]
pub struct SinclairRightJoyPortAddress;
impl PortAddress for SinclairRightJoyPortAddress {
    const ADDRESS_MASK: u16 = 0x1000;
    const ADDRESS_BITS: u16 = 0xeffe;
}
/// Cursor Joystick [PortAddress].
#[derive(Clone, Copy, Default, Debug)]
pub struct CursorJoyPortAddress;
impl PortAddress for CursorJoyPortAddress {
    const ADDRESS_MASK: u16 = 0x1800;
    const ADDRESS_BITS: u16 = 0xe7fe;
    /// Matches addresses: `0xeffe` or `0xf7fe` or `0xe7fe`.
    #[inline]
    fn match_port(address: u16) -> bool {
        address & Self::ADDRESS_MASK != Self::ADDRESS_MASK
    }
}

impl<P, J: JoystickInterface, D> Deref for JoystickBusDevice<P, J, D> {
    type Target = J;
    fn deref(&self) -> &Self::Target {
        &self.joystick
    }
}

impl<P, J: JoystickInterface, D> DerefMut for JoystickBusDevice<P, J, D> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.joystick
    }
}

impl<P, J, D> PassByAyAudioBusDevice for JoystickBusDevice<P, J, D> {}

impl<P, J, D> BusDevice for JoystickBusDevice<P, J, D>
    where P: PortAddress,
          D: BusDevice,
          J: JoystickDevice
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
    fn read_io(&mut self, port: u16, timestamp: Self::Timestamp) -> Option<(u8, Option<NonZeroU16>)> {
        let bus_data = self.bus.read_io(port, timestamp);
        if P::match_port(port) {
            let joy_data = self.joystick.port_read(port);
            if let Some((data, ws)) = bus_data {
                return Some((data & joy_data, ws))
            }
            return Some((joy_data, None))
        }
        bus_data
    }

    #[inline]
    fn write_io(&mut self, port: u16, data: u8, timestamp: Self::Timestamp) -> Option<u16> {
        if P::match_port(port) && self.joystick.port_write(port, data) {
            return Some(0);
        }
        self.bus.write_io(port, data, timestamp)
    }
}

/// A selectable joystick controller, providing a [BusDevice] implementation.
///
/// This controller allows changing the implementation of the joystick device at run time.
#[derive(Clone, Copy, Default, Debug)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
pub struct MultiJoystickBusDevice<D> {
    #[cfg_attr(feature = "snapshot", serde(default))]
    pub joystick: JoystickSelect,
    #[cfg_attr(feature = "snapshot", serde(default))]
    bus: D
}

impl<D: Default> MultiJoystickBusDevice<D> {
    pub fn new_with(joystick: JoystickSelect) -> Self {
        MultiJoystickBusDevice {
            joystick, bus: Default::default()
        }
    }
}

impl<D> Deref for MultiJoystickBusDevice<D> {
    type Target = JoystickSelect;
    fn deref(&self) -> &Self::Target {
        &self.joystick
    }
}

impl<D> DerefMut for MultiJoystickBusDevice<D> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.joystick
    }
}

/// An enum of joystick device implementation variants.
///
/// Some of the variants contain more than one joystick device.
#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "snapshot", serde(try_from = "Cow<str>", into = "&str"))]
pub enum JoystickSelect {
    Kempston(KempstonJoystickDevice),
    Fuller(FullerJoystickDevice),
    Sinclair(SinclairJoystickDevice<SinclairJoyRightMap>,
             SinclairJoystickDevice<SinclairJoyLeftMap>),
    Cursor(CursorJoystickDevice),
}

impl Default for JoystickSelect {
    fn default() -> Self {
        JoystickSelect::Kempston(Default::default())
    }
}

impl fmt::Display for JoystickSelect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        <&str>::from(self).fmt(f)
    }
}

impl From<&JoystickSelect> for &str {
   fn from(joy: &JoystickSelect) -> Self {
        <&str>::from(*joy)
   }
 }

impl From<JoystickSelect> for &str {
    fn from(joy: JoystickSelect) -> Self {
        use JoystickSelect::*;
        match joy {
            Kempston(..) => "Kempston",
            Fuller(..)   => "Fuller",
            Sinclair(..) => "Sinclair",
            Cursor(..)   => "Cursor",
        }
    }
}

/// An error that can be returned when parsing a [JoystickSelect] variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseJoystickSelectError;

impl fmt::Display for ParseJoystickSelectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        "unrecognized joystick name".fmt(f)
    }
}

impl std::error::Error for ParseJoystickSelectError {}

impl FromStr for JoystickSelect {
    type Err = ParseJoystickSelectError;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match JoystickSelect::new_from_name(name) {
            Some((joy, _)) => Ok(joy),
            None => Err(ParseJoystickSelectError)
        }
    }
}

/// The error type returned when a [JoystickSelect] variant conversion failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TryFromStrJoystickSelectError<'a>(pub Cow<'a, str>);

impl fmt::Display for TryFromStrJoystickSelectError<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unrecognized joystick name: {}", self.0)
    }
}

impl std::error::Error for TryFromStrJoystickSelectError<'_> {}

impl<'a> TryFrom<&'a str> for JoystickSelect {
    type Error = TryFromStrJoystickSelectError<'a>;
    fn try_from(name: &'a str) -> Result<Self, Self::Error> {
        match JoystickSelect::new_from_name(name) {
            Some((joy, _)) => Ok(joy),
            None => Err(TryFromStrJoystickSelectError(name.into()))
        }
    }
}

impl<'a> TryFrom<Cow<'a, str>> for JoystickSelect {
    type Error = TryFromStrJoystickSelectError<'a>;
    fn try_from(name: Cow<'a, str>) -> Result<Self, Self::Error> {
        match JoystickSelect::new_from_name(&name) {
            Some((joy, _)) => Ok(joy),
            None => Err(TryFromStrJoystickSelectError(name))
        }
    }
}

#[allow(clippy::len_without_is_empty)]
impl JoystickSelect {
    /// The largest value that can be passed as a `global_index` to [JoystickSelect::new_with_index].
    pub const MAX_GLOBAL_INDEX: usize = 4;
    /// Creates a new joystick device variant from a given name.
    ///
    /// On success returns a tuple with one of the joystick variants and the number of
    /// available joysticks in this variant.
    pub fn new_from_name<S: AsRef<str>>(name: S) -> Option<(Self, usize)> {
        let name = name.as_ref();
        use JoystickSelect::*;
        if name.eq_ignore_ascii_case("Kempston") {
            Some((Kempston(Default::default()), 1))
        }
        else if name.eq_ignore_ascii_case("Fuller") {
            Some((Fuller(Default::default()), 1))
        }
        else if name.eq_ignore_ascii_case("Cursor")
              ||name.eq_ignore_ascii_case("Protek")
              ||name.eq_ignore_ascii_case("AGF") {
            Some((Cursor(Default::default()), 1))
        }
        else if name.eq_ignore_ascii_case("Sinclair")
             ||name.eq_ignore_ascii_case("Interface II")
             ||name.eq_ignore_ascii_case("Interface 2")
             ||name.eq_ignore_ascii_case("IF II")
             ||name.eq_ignore_ascii_case("IF 2") {
            Some((Sinclair(Default::default(), Default::default()), 2))
        }
        else {
            None
        }
    }
    /// Creates a new joystick device variant from a given `global_index` which counts for
    /// all joysticks in every variant.
    ///
    /// E.g. `2` and `3` both select `Sinclair` joystick but with a different resulting index.
    ///
    /// On success returns a tuple with one of the joystick variants and an index of
    /// the selected joystick in this variant.
    ///
    /// See also [JoystickSelect::MAX_GLOBAL_INDEX].
    pub fn new_with_index(global_index: usize) -> Option<(Self, usize)> {
        use JoystickSelect::*;
        match global_index {
            0 => Some((Kempston(Default::default()), 0)),
            1 => Some((Fuller(Default::default()), 0)),
            i@2|i@3 => Some((Sinclair(Default::default(), Default::default()), i-2)),
            4 => Some((Cursor(Default::default()), 0)),
            _ => None
        }
    }
    /// Returns the number of joysticks in the current variant.
    pub fn len(&self) -> usize {
        if let JoystickSelect::Sinclair(..) = self {
            return 2
        }
        1
    }
    /// Provides a mutable reference to the selected joystick type via a dynamic trait.
    ///
    /// Some variants contain more than one joystick, so provide an appropriate `index` to
    /// select one of them.
    ///
    /// Returns `None` if a joystick with the given index doesn't exist.
    pub fn joystick_interface(&mut self, index: usize) -> Option<&mut (dyn JoystickInterface + 'static)>
    {
        match self {
            JoystickSelect::Kempston(ref mut joy) if index == 0 => Some(joy),
            JoystickSelect::Fuller(ref mut joy) if index == 0 => Some(joy),
            JoystickSelect::Sinclair(ref mut joy, _) if index == 0 => Some(joy),
            JoystickSelect::Sinclair(_, ref mut joy) if index == 1 => Some(joy),
            JoystickSelect::Cursor(ref mut joy) if index == 0 => Some(joy),
            _ => None
        }
    }
    /// Returns an `index` of the next joystick in the current variant.
    /// Optionally changes the current joystick device variant to the next one cyclically.
    ///
    /// Some variants contain more than one joystick, provide an appropriate `index` to
    /// select one of them. If `index + 1` exceeds the number of joysticks available then
    /// the next variant is being selected.
    pub fn select_next_joystick(&mut self, index: usize) -> usize {
        use JoystickSelect::*;
        *self = match self {
            Kempston(..) => Fuller(Default::default()),
            Fuller(..) => Sinclair(Default::default(), Default::default()),
            Sinclair(..) if index == 0 => return 1,
            Sinclair(..) => Cursor(Default::default()),
            Cursor(..) => Kempston(Default::default()),
        };
        0
    }
    #[inline]
    pub fn is_last(&self) -> bool {
        self.is_cursor()
    }
    #[inline]
    pub fn is_kempston(&self) -> bool {
        if let JoystickSelect::Kempston(..) = self {
            return true
        }
        false
    }
    #[inline]
    pub fn is_fuller(&self) -> bool {
        if let JoystickSelect::Fuller(..) = self {
            return true
        }
        false
    }
    #[inline]
    pub fn is_sinclair(&self) -> bool {
        if let JoystickSelect::Sinclair(..) = self {
            return true
        }
        false
    }
    #[inline]
    pub fn is_cursor(&self) -> bool {
        if let JoystickSelect::Cursor(..) = self {
            return true
        }
        false
    }
}

impl<D> PassByAyAudioBusDevice for MultiJoystickBusDevice<D> {}

impl<D> BusDevice for MultiJoystickBusDevice<D>
    where D: BusDevice
{
    type Timestamp = D::Timestamp;
    type NextDevice = D;

    #[inline(always)]
    fn next_device_mut(&mut self) -> &mut Self::NextDevice {
        &mut self.bus
    }

    #[inline(always)]
    fn next_device_ref(&self) -> &Self::NextDevice {
        &self.bus
    }

    #[inline]
    fn into_next_device(self) -> Self::NextDevice {
        self.bus
    }

    #[inline(always)]
    fn read_io(&mut self, port: u16, timestamp: Self::Timestamp) -> Option<(u8, Option<NonZeroU16>)> {
        use JoystickSelect::*;
        let bus_data = self.bus.read_io(port, timestamp);
        let joy_data = match self.joystick {
            Kempston(joystick) if KempstonJoyPortAddress::match_port(port) => {
                Some(joystick.port_read(port))
            }
            Fuller(joystick) if FullerJoyPortAddress::match_port(port) => {
                Some(joystick.port_read(port))
            }
            Sinclair(joy1, joy2) => {
                let joy_data1 = if SinclairRightJoyPortAddress::match_port(port) {
                    Some(joy1.port_read(port))
                }
                else {
                    None
                };
                let joy_data2 = if SinclairLeftJoyPortAddress::match_port(port) {
                    Some(joy2.port_read(port))
                }
                else {
                    None
                };
                match (joy_data1, joy_data2) {
                    (Some(data1), Some(data2)) => Some(data1 & data2),
                    (Some(data1), None) => Some(data1),
                    (None, Some(data2)) => Some(data2),
                    _ => None
                }
            }
            Cursor(joystick) if CursorJoyPortAddress::match_port(port) => {
                Some(joystick.port_read(port))
            }
            _ => None
        };
        if let Some(joy_data) = joy_data {
            if let Some((data, ws)) = bus_data {
                return Some((data & joy_data, ws))
            }
            Some((joy_data, None))
        }
        else {
            bus_data
        }
    }
}

#[cfg(test)]
#[cfg(feature = "snapshot")]
mod tests {
    use super::*;

    #[test]
    fn joystick_select_snapshot() {
        let (joy, len) = JoystickSelect::new_from_name("Sinclair").unwrap();
        assert_eq!(len, 2);
        assert!(joy.is_sinclair());
        assert_eq!(joy.len(), len);
        assert_eq!(<&str>::from(joy), "Sinclair");
        let json = r#""Sinclair""#;
        assert_eq!(serde_json::to_string(&joy).unwrap(), json);
        let joy0: JoystickSelect = serde_json::from_str(json).unwrap();
        let joy1: JoystickSelect = serde_json::from_reader(json.as_bytes()).unwrap();
        assert!(joy0.is_sinclair());
        assert!(joy1.is_sinclair());
        assert_eq!(format!("{}", joy0), "Sinclair");
    }
}