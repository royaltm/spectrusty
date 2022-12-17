/*
    Copyright (C) 2020-2022  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
//! Sinclair Left and Right Joysticks' implementation.
use core::fmt::Debug;
use core::marker::PhantomData;
use super::{JoystickDevice, Directions, JoystickInterface};
const UNUSED_MASK:      u8 = 0b1110_0000;

/// The Sinclair Joystick device implements [JoystickDevice] and [JoystickInterface].
///
/// Provide one of [SinclairJoyLeftMap] or [SinclairJoyRightMap] as `S`.
#[derive(Clone, Copy, Debug)]
pub struct SinclairJoystickDevice<S> {
    data: u8,
    directions: Directions,
    _side: PhantomData<S>
}

pub trait SinclairJoyKeyMap: Debug {
    const FIRE_MASK:  u8;
    const UP_MASK:    u8;
    const DOWN_MASK:  u8;
    const RIGHT_MASK: u8;
    const LEFT_MASK:  u8;
}

/// A key map for the left Sinclair joystick.
#[derive(Clone, Copy, Default, Debug)]
pub struct SinclairJoyLeftMap;
impl SinclairJoyKeyMap for SinclairJoyLeftMap {
                       //        5 4321
                       //        F_UDRL
    const FIRE_MASK:   u8 = 0b0001_0000;
    const UP_MASK:     u8 = 0b0000_1000;
    const DOWN_MASK:   u8 = 0b0000_0100;
    const RIGHT_MASK:  u8 = 0b0000_0010;
    const LEFT_MASK:   u8 = 0b0000_0001;
}

/// A key map for the right Sinclair joystick.
#[derive(Clone, Copy, Default, Debug)]
pub struct SinclairJoyRightMap;
impl SinclairJoyKeyMap for SinclairJoyRightMap {
                       //        6_7890
                       //        L_RDUF
    const FIRE_MASK:   u8 = 0b0000_0001;
    const UP_MASK:     u8 = 0b0000_0010;
    const DOWN_MASK:   u8 = 0b0000_0100;
    const RIGHT_MASK:  u8 = 0b0000_1000;
    const LEFT_MASK:   u8 = 0b0001_0000;
}

impl<S> Default for SinclairJoystickDevice<S> {
    fn default() -> Self {
        SinclairJoystickDevice {
            data: !0,
            directions: Directions::empty(),
            _side: PhantomData
        }
    }
}

impl<S: Debug> JoystickDevice for SinclairJoystickDevice<S> {
    #[inline]
    fn port_read(&self, _port: u16) -> u8 {
        self.data
    }
}

impl<S> JoystickInterface for SinclairJoystickDevice<S>
    where S: SinclairJoyKeyMap
{
    fn fire(&mut self, _btn: u8, pressed: bool) {
        if pressed {
            self.data &= !S::FIRE_MASK;
        }
        else {
            self.data |= S::FIRE_MASK;
        }
    }

    fn get_fire(&self, _btn: u8) -> bool {
        self.data & S::FIRE_MASK == 0
    }

    fn set_directions(&mut self, dir: Directions) {
        self.directions = dir;
        self.data = (self.data & (S::FIRE_MASK|UNUSED_MASK)) |
            if dir.intersects(Directions::UP)    { 0 } else { S::UP_MASK    } |
            if dir.intersects(Directions::RIGHT) { 0 } else { S::RIGHT_MASK } |
            if dir.intersects(Directions::DOWN)  { 0 } else { S::DOWN_MASK  } |
            if dir.intersects(Directions::LEFT)  { 0 } else { S::LEFT_MASK  };
    }

    fn get_directions(&self) -> Directions {
        self.directions
    }
}
