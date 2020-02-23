//! Hosts [JoystickBusDevice] implementing [BusDevice] for joysticks.
use core::convert::TryFrom;
use core::fmt;
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};

use super::ay::PassByAyAudioBusDevice;
use crate::bus::{BusDevice, NullDevice, PortAddress};
use crate::clock::VideoTs;
use crate::memory::ZxMemory;

pub use crate::peripherals::joystick::{
    JoystickDevice, JoystickInterface,
    kempston::*, fuller::*, sinclair::*, cursor::*
};

/// A convenient Kempston Joystick [BusDevice] type.
pub type KempstonJoystick<D=NullDevice<VideoTs>> = JoystickBusDevice<VideoTs,
                                                            KempstonJoyPortAddress,
                                                            KempstonJoystickDevice,
                                                            D>;
/// A convenient Fuller Joystick [BusDevice] type.
pub type FullerJoystick<D=NullDevice<VideoTs>> = JoystickBusDevice<VideoTs,
                                                            FullerJoyPortAddress,
                                                            FullerJoystickDevice,
                                                            D>;
/// A convenient pair of Left and Right Joystick [BusDevice] type.
pub type SinclairJoystick<D=NullDevice<VideoTs>> = SinclairLeftJoystick<SinclairRightJoystick<D>>;
/// A convenient Left Sinclair Joystick [BusDevice] type.
pub type SinclairLeftJoystick<D=NullDevice<VideoTs>> = JoystickBusDevice<VideoTs,
                                                            SinclairLeftJoyPortAddress,
                                                            SinclairJoystickDevice<SinclairJoyLeftMap>,
                                                            D>;
/// A convenient Right Sinclair Joystick [BusDevice] type.
pub type SinclairRightJoystick<D=NullDevice<VideoTs>> = JoystickBusDevice<VideoTs,
                                                            SinclairRightJoyPortAddress,
                                                            SinclairJoystickDevice<SinclairJoyRightMap>,
                                                            D>;
/// A convenient Cursor Joystick [BusDevice] type.
pub type CursorJoystick<D=NullDevice<VideoTs>> = JoystickBusDevice<VideoTs,
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
    SinclairLeftJoystick<D>: "Sinclair #1 Joystick",
    SinclairRightJoystick<D>: "Sinclair #2 Joystick",
    CursorJoystick<D>: "Cursor Joystick"
}

/// A joystick controller, providing a [BusDevice] implementation that can be used with [joystick devices][JoystickDevice].
#[derive(Clone, Default, Debug)]
pub struct JoystickBusDevice<T, P, J, D=NullDevice<T>>
{
    /// A [JoystickDevice] implementation, which may also implement [JoystickInterface] trait
    /// for providing user input.
    pub joystick: J,
    bus: D,
    _port_decode: PhantomData<P>,
    _ts: PhantomData<T>
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

impl<T, P, J, D> Deref for JoystickBusDevice<T, P, J, D> {
    type Target = J;
    fn deref(&self) -> &Self::Target {
        &self.joystick
    }
}

impl<T, P, J, D> DerefMut for JoystickBusDevice<T, P, J, D> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.joystick
    }
}

impl<T, P, J, D> PassByAyAudioBusDevice for JoystickBusDevice<T, P, J, D> {}

impl<T, P, J, D> BusDevice for JoystickBusDevice<T, P, J, D>
    where T: fmt::Debug,
          P: PortAddress,
          D: BusDevice<Timestamp=VideoTs>,
          J: JoystickDevice
{
    type Timestamp = VideoTs;
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
    fn read_io(&mut self, port: u16, timestamp: Self::Timestamp) -> Option<u8> {
        let data = self.bus.read_io(port, timestamp);
        if P::match_port(port) {
            return Some(data.unwrap_or(!0) & self.joystick.port_read(port))
        }
        data
    }

    #[inline]
    fn write_io(&mut self, port: u16, data: u8, timestamp: Self::Timestamp) -> bool {
        if P::match_port(port) {
            if self.joystick.port_write(port, data) {
                return true;
            }
        }
        self.bus.write_io(port, data, timestamp)
    }
}

/// A selectable joystick controller, providing a [BusDevice] implementation.
///
/// This controller allows changing the implementation of joystick device in runtime.
#[derive(Clone, Copy, Default, Debug)]
pub struct MultiJoystickBusDevice<T=VideoTs, D=NullDevice<T>> {
    pub joystick: JoystickSelect,
    bus: D,
    _ts: PhantomData<T>
}

impl<T, D> Deref for MultiJoystickBusDevice<T, D> {
    type Target = JoystickSelect;
    fn deref(&self) -> &Self::Target {
        &self.joystick
    }
}

impl<T, D> DerefMut for MultiJoystickBusDevice<T, D> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.joystick
    }
}

/// An enum of joystick device implementation variants.
///
/// Some of the variants contain more than one joystick device.
#[derive(Clone, Copy, Debug)]
pub enum JoystickSelect {
    Kempston(KempstonJoystickDevice),
    Fuller(FullerJoystickDevice),
    Sinclair(SinclairJoystickDevice<SinclairJoyLeftMap>, SinclairJoystickDevice<SinclairJoyRightMap>),
    Cursor(CursorJoystickDevice),
}

impl fmt::Display for JoystickSelect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use JoystickSelect::*;
        write!(f, "{}", match self {
            Kempston(..) => "Kempston",
            Fuller(..)   => "Fuller",
            Sinclair(..) => "Sinclair",
            Cursor(..)   => "Cursor",
        })
    }
}

impl Default for JoystickSelect {
    fn default() -> Self {
        JoystickSelect::Kempston(Default::default())
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct JoystickSelectError<'a>(pub &'a str);
impl fmt::Display for JoystickSelectError<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Unknown joystick name: {}", self.0)
    }
}

impl std::error::Error for JoystickSelectError<'_> {}

impl<'a> TryFrom<&'a str> for JoystickSelect {
    type Error = JoystickSelectError<'a>;
    fn try_from(name: &'a str) -> Result<Self, Self::Error> {
        match JoystickSelect::new_from_name(name) {
            Some((joy, _)) => Ok(joy),
            None => Err(JoystickSelectError(name))
        }
    }
}

impl JoystickSelect {
    /// The largest value that can be passed as a `global_index` to [JoystickSelect::new_with_index].
    const MAX_GLOBAL_INDEX: usize = 4;
    /// Creates a new joystick device variant from a given name.
    ///
    /// On success returns a tuple with one of the joystick variants and a number of
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
    /// selected joystick in this variant.
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
    /// Returns a number of joysticks in the current variant.
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
    pub fn joystick_interface(&mut self, index: usize) -> Option<&mut dyn JoystickInterface>
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

impl<T, D> PassByAyAudioBusDevice for MultiJoystickBusDevice<T, D> {}

impl<T: fmt::Debug, D> BusDevice for MultiJoystickBusDevice<T, D>
    where D: BusDevice<Timestamp=T>
{
    type Timestamp = T;
    type NextDevice = D;

    #[inline(always)]
    fn next_device_mut(&mut self) -> &mut Self::NextDevice {
        &mut self.bus
    }

    #[inline(always)]
    fn next_device_ref(&self) -> &Self::NextDevice {
        &self.bus
    }

    #[inline(always)]
    fn read_io(&mut self, port: u16, timestamp: Self::Timestamp) -> Option<u8> {
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
                let joy_data1 = if SinclairLeftJoyPortAddress::match_port(port) {
                    Some(joy1.port_read(port))
                }
                else {
                    None
                };
                let joy_data2 = if SinclairRightJoyPortAddress::match_port(port) {
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
        if let Some(data) = joy_data {
            Some(bus_data.unwrap_or(!0) & data)
        }
        else {
            bus_data
        }
    }
}
