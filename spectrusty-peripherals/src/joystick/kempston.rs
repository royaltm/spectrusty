/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
//! Kempston Joystick implementation.
use super::{JoystickDevice, Directions, JoystickInterface};
                      // 000F_UDLR
const FIRE_MASK:  u8 = 0b0001_0000;
const RIGHT_MASK: u8 = 0b0000_0001;
const LEFT_MASK:  u8 = 0b0000_0010;
const DOWN_MASK:  u8 = 0b0000_0100;
const UP_MASK:    u8 = 0b0000_1000;

/// The Kempston Joystick device implements [JoystickDevice] and [JoystickInterface].
#[derive(Clone, Copy, Default, Debug)]
pub struct KempstonJoystickDevice {
    data: u8,
    directions: Directions
}

impl JoystickDevice for KempstonJoystickDevice {
    #[inline]
    fn port_read(&self, _port: u16) -> u8 {
        self.data
    }
}

impl JoystickInterface for KempstonJoystickDevice {
    fn fire(&mut self, _btn: u8, pressed: bool) {
        if pressed {
            self.data |= FIRE_MASK;
        }
        else {
            self.data &= !FIRE_MASK;
        }
    }

    fn get_fire(&self, _btn: u8) -> bool {
        self.data & FIRE_MASK == FIRE_MASK
    }

    fn set_directions(&mut self, dir: Directions) {
        self.directions = dir;
        self.data = (self.data & FIRE_MASK) | 
            if dir.intersects(Directions::UP)    { UP_MASK    } else { 0 } |
            if dir.intersects(Directions::RIGHT) { RIGHT_MASK } else { 0 } |
            if dir.intersects(Directions::DOWN)  { DOWN_MASK  } else { 0 } |
            if dir.intersects(Directions::LEFT)  { LEFT_MASK  } else { 0 };
    }
    fn get_directions(&self) -> Directions {
        self.directions
    }
}
