//! Cursor Joystick implementation.
use super::{JoystickDevice, Directions, JoystickInterface};

const PORT1_MASK:  u16 = 0b0000_1000_0000_0000;
                    //        5_4321
const UNUSED_MASK1: u8 = 0b1110_1111;
const LEFT_MASK:    u8 = 0b0001_0000;

const PORT2_MASK:  u16 = 0b0001_0000_0000_0000;
                    //        6_7890
const UNUSED_MASK2: u8 = 0b1110_0010;
const FIRE_MASK:    u8 = 0b0000_0001;
const RIGHT_MASK:   u8 = 0b0000_0100;
const UP_MASK:      u8 = 0b0000_1000;
const DOWN_MASK:    u8 = 0b0001_0000;

/// The Cursor Joystick device implements [JoystickDevice] and [JoystickInterface].
#[derive(Clone, Copy, Debug)]
pub struct CursorJoystickDevice {
    data1: u8,
    data2: u8,
    directions: Directions
}

impl Default for CursorJoystickDevice {
    fn default() -> Self {
        CursorJoystickDevice {
            data1: !0,
            data2: !0,
            directions: Directions::empty()
        }
    }
}

impl JoystickDevice for CursorJoystickDevice {
    #[inline]
    fn port_read(&self, port: u16) -> u8 {
        (if port & PORT1_MASK == 0 { self.data1 } else { !0 }) & 
        (if port & PORT2_MASK == 0 { self.data2 } else { !0 })
    }
}

impl JoystickInterface for CursorJoystickDevice {
    fn fire(&mut self, _btn: u8, pressed: bool) {
        if pressed {
            self.data2 &= !FIRE_MASK;
        }
        else {
            self.data2 |= FIRE_MASK;
        }
    }

    fn get_fire(&self, _btn: u8) -> bool {
        self.data2 & FIRE_MASK == 0
    }

    fn set_directions(&mut self, dir: Directions) {
        self.directions = dir;
        self.data1 = (self.data1 & UNUSED_MASK1) | 
            if dir.intersects(Directions::LEFT)  { 0 } else { LEFT_MASK  };
        self.data2 = (self.data2 & (FIRE_MASK|UNUSED_MASK2)) |
            if dir.intersects(Directions::UP)    { 0 } else { UP_MASK    } |
            if dir.intersects(Directions::RIGHT) { 0 } else { RIGHT_MASK } |
            if dir.intersects(Directions::DOWN)  { 0 } else { DOWN_MASK  };
    }

    fn get_directions(&self) -> Directions {
        self.directions
    }
}
