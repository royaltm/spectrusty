/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
//! Fuller Joystick implementation.
use super::{JoystickDevice, Directions, JoystickInterface};
                       // F---_RLDU
const FIRE_MASK:   u8 = 0b1000_0000;
const UNUSED_MASK: u8 = 0b0111_0000;
const RIGHT_MASK:  u8 = 0b0000_1000;
const LEFT_MASK:   u8 = 0b0000_0100;
const DOWN_MASK:   u8 = 0b0000_0010;
const UP_MASK:     u8 = 0b0000_0001;

/// The Fuller Joystick device implements [JoystickDevice] and [JoystickInterface].
#[derive(Clone, Copy, Debug)]
pub struct FullerJoystickDevice {
    data: u8,
    directions: Directions
}

impl Default for FullerJoystickDevice {
    fn default() -> Self {
        FullerJoystickDevice {
            data: !0,
            directions: Directions::empty()
        }
    }
}

impl JoystickDevice for FullerJoystickDevice {
    #[inline]
    fn port_read(&self, _port: u16) -> u8 {
        self.data
    }
}

impl JoystickInterface for FullerJoystickDevice {
    fn fire(&mut self, _btn: u8, pressed: bool) {
        if pressed {
            self.data &= !FIRE_MASK;
        }
        else {
            self.data |= FIRE_MASK;
        }
    }

    fn get_fire(&self, _btn: u8) -> bool {
        self.data & FIRE_MASK == 0
    }

    fn set_directions(&mut self, dir: Directions) {
        self.directions = dir;
        self.data = (self.data & (FIRE_MASK|UNUSED_MASK)) |
            if dir.intersects(Directions::UP)    { 0 } else { UP_MASK    } |
            if dir.intersects(Directions::RIGHT) { 0 } else { RIGHT_MASK } |
            if dir.intersects(Directions::DOWN)  { 0 } else { DOWN_MASK  } |
            if dir.intersects(Directions::LEFT)  { 0 } else { LEFT_MASK  };
    }

    fn get_directions(&self) -> Directions {
        self.directions
    }
}
