/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
//! A pointing device / mouse communication interface and emulators of mouse devices.
use core::fmt::Debug;

pub mod kempston;

bitflags! {
    /// Flags for mouse buttons.
    /// * Bit = 1 button is pressed.
    /// * Bit = 0 button is released.
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

/// This type is being used to provide input data for the mouse emulator.
///
/// The pointing device coordinates are measured in PAL pixels (704x576 including border).
/// * Horizontal values increase from left to right.
/// * Vertical values increase from top to bottom.
#[derive(Clone, Copy, Default, Debug)]
pub struct MouseMovement {
    pub horizontal: i16,
    pub vertical: i16
}

/// An interface for providing user input data for a [MouseDevice] implementation.
pub trait MouseInterface {
    /// Sets the state of all mouse buttons.
    fn set_buttons(&mut self, buttons: MouseButtons);
    /// Returns a state of all mouse buttons.
    fn get_buttons(&self) -> MouseButtons;
    /// Moves the mouse by the given interval.
    fn move_mouse(&mut self, mov: MouseMovement);
}

/// A mouse device interface used by the mouse [bus][crate::bus::mouse] device implementation.
pub trait MouseDevice: Debug {
    /// Should return a current mouse state. The `port` argument can be used to decide which
    /// information will be returned - one of the axes or a button state.
    fn port_read(&self, port: u16) -> u8;
    /// Allows to implement writing data to a mouse device.
    ///
    /// If a device does not support writes, this method should return `false`.
    /// A default implementation does exactly just that.
    fn port_write(&mut self, _port: u16, _data: u8) -> bool { false }
}

/// The mouse device that can be used as a placeholder type.
#[derive(Clone, Copy, Default, Debug)]
pub struct NullMouseDevice;

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

impl MouseInterface for NullMouseDevice {
    fn set_buttons(&mut self, _buttons: MouseButtons) {}
    fn get_buttons(&self) -> MouseButtons {
        MouseButtons::empty()
    }
    fn move_mouse(&mut self, _mov: MouseMovement) {}
}

impl MouseDevice for NullMouseDevice {
    fn port_read(&self, _port: u16) -> u8 {
        u8::max_value()
    }   
}
