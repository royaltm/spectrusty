//! This module hosts traits for interfacing mouse controllers and mouse device implementations.
use core::fmt::Debug;

pub mod kempston;

bitflags! {
    /// Bitflags for mouse buttons.
    /// * Bit = 1 is active.
    /// * Bit = 0 is inactive.
    #[derive(Default)]
    pub struct MouseButtons: u8 {
        const LEFT    = 0b0000_0001;
        const RIGHT   = 0b0000_0010;
        const MIDDLE  = 0b0000_0100;
        const FOURTH  = 0b0001_0000;
        const FIFTH   = 0b0010_0000;
        const SIXTH   = 0b0100_0000;
        const SEVENTH = 0b1000_0000;
    }
}

/// The mouse coordinates are measured in PAL pixels.
/// Horizontal values increase from left to right.
/// Vertical values increase from top to bottom.
#[derive(Clone, Copy, Default, Debug)]
pub struct MouseMovement {
    pub horizontal: i16,
    pub vertical: i16
}

impl From<(i16, i16)> for MouseMovement {
    fn from((x, y): (i16, i16)) -> Self {
        MouseMovement {
            horizontal: x,
            vertical: y
        }
    }
}

impl From<[i16;2]> for MouseMovement {
    fn from([x, y]: [i16;2]) -> Self {
        (x, y).into()
    }
}

/// A user input interface for a [MouseDevice].
pub trait MouseInterface {
    /// Sets the state of all mouse buttons.
    fn set_buttons(&mut self, buttons: MouseButtons);
    /// Returns a state of all mouse buttons.
    fn get_buttons(&self) -> MouseButtons;
    /// Moves the mouse by the given interval.
    fn move_mouse<M: Into<MouseMovement>>(&mut self, mov: M);
}

/// A mouse device interface used by the mouse [bus][crate::bus::mouse] device.
pub trait MouseDevice: Debug {
    /// Reads current mouse state as I/O data.
    fn port_read(&self, port: u16) -> u8;
    /// Writes I/O data to a mouse device.
    ///
    /// If a device does not support writes, this method should return `false`.
    /// A default implementation does exactly just that.
    fn port_write(&mut self, _port: u16, _data: u8) -> bool { false }
}
