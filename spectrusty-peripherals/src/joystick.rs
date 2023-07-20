/*
    Copyright (C) 2020-2023  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
//! A joystick communication interface and emulators of various joysticks.
use core::fmt::Debug;

pub mod cursor;
pub mod fuller;
pub mod kempston;
pub mod sinclair;

bitflags! {
    /// Flags for reading and writing the current stick direction.
    /// * Bit = 1 a direction is active.
    /// * Bit = 0 a direction is inactive.
    #[derive(Default, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
    pub struct Directions: u8 {
        const UP    = 0b0001;
        const RIGHT = 0b0010;
        const DOWN  = 0b0100;
        const LEFT  = 0b1000;
    }
}

/// An enum for specifying one of 8 possible joystick's directional positions and a center (neutral) state.
pub enum JoyDirection {
    Center,
    Up,
    UpRight,
    Right,
    DownRight,
    Down,
    DownLeft,
    Left,
    UpLeft
}

/// An interface for providing user input data for a [JoystickDevice] implementation.
pub trait JoystickInterface {
    /// Press or release a "fire" button. `btn` is the button number for cases when the joystick have more
    /// than one button.
    ///
    /// Currently, `btn` is not being used by any of the implemented devices.
    fn fire(&mut self, btn: u8, pressed: bool);
    /// Returns `true` if an indicated "fire" button is being pressed, otherwise returns `false`.
    fn get_fire(&self, btn: u8) -> bool;
    /// Changes the stick direction using provided flags.
    fn set_directions(&mut self, dir: Directions);
    /// Returns the current stick direction.
    fn get_directions(&self) -> Directions;
    /// Changes the stick direction using an enum.
    #[inline]
    fn direction(&mut self, dir: JoyDirection) {
        self.set_directions(match dir {
            JoyDirection::Center => Directions::empty(),
            JoyDirection::Up => Directions::UP,
            JoyDirection::UpRight => Directions::UP|Directions::RIGHT,
            JoyDirection::Right => Directions::RIGHT,
            JoyDirection::DownRight => Directions::DOWN|Directions::RIGHT,
            JoyDirection::Down => Directions::DOWN,
            JoyDirection::DownLeft => Directions::DOWN|Directions::LEFT,
            JoyDirection::Left => Directions::LEFT,
            JoyDirection::UpLeft => Directions::UP|Directions::LEFT,
        })
    }
    /// Resets a joystick to a central (neutral) position.
    #[inline]
    fn center(&mut self) {
        self.set_directions(Directions::empty());
    }
    /// Returns `true` if a joystick is in the up (forward) position.
    #[inline]
    fn is_up(&self) -> bool {
        self.get_directions().intersects(Directions::UP)
    }
    /// Returns `true` if a joystick is in the right position.
    #[inline]
    fn is_right(&self) -> bool {
        self.get_directions().intersects(Directions::RIGHT)
    }
    /// Returns `true` if a joystick is in the left position.
    #[inline]
    fn is_left(&self) -> bool {
        self.get_directions().intersects(Directions::LEFT)
    }
    /// Returns `true` if a joystick is in the down (backward) position.
    #[inline]
    fn is_down(&self) -> bool {
        self.get_directions().intersects(Directions::DOWN)
    }
    /// Returns `true` if a joystick is in the center (neutral) position.
    #[inline]
    fn is_center(&self) -> bool {
        self.get_directions().intersects(Directions::empty())
    }
}

/// A joystick device interface for the joystick [bus][crate::bus::joystick] device implementations.
pub trait JoystickDevice: Debug {
    /// Should return the joystick state.
    fn port_read(&self, port: u16) -> u8;
    /// Writes data to a joystick device.
    ///
    /// If a device does not support writes, this method should return `false`.
    /// The default implementation does exactly just that.
    fn port_write(&mut self, _port: u16, _data: u8) -> bool { false }
}

/// The joystick device that can be used as a placeholder type.
#[derive(Clone, Copy, Default, Debug)]
pub struct NullJoystickDevice;

impl JoystickDevice for NullJoystickDevice {
    fn port_read(&self, _port: u16) -> u8 {
        u8::max_value()
    }
}

impl JoystickInterface for NullJoystickDevice {
    fn fire(&mut self, _btn: u8, _pressed: bool) {}
    fn get_fire(&self, _btn: u8) -> bool { false }
    fn set_directions(&mut self, _dir: Directions) {}
    fn get_directions(&self) -> Directions {
        Directions::empty()
    }
}
