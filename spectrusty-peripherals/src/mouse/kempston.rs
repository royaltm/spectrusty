//! A Kempston Mouse device implementation.
#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};

use super::{MouseInterface, MouseDevice, MouseMovement, MouseButtons};
/*
    Horizontal position: IN 64479
    Vertical postition: IN 65503
    Buttons: IN 64223 [255 = None], [254 = Left], [253 = Right], [252 = Both]
*/
const LEFT_BTN_MASK:   u8 = 0b0000_0001;
const RIGHT_BTN_MASK:  u8 = 0b0000_0010;
const UNUSED_BTN_MASK: u8 = !(LEFT_BTN_MASK|RIGHT_BTN_MASK); 

const PORT_BTN_MASK: u16 = 0b0000_0001_0000_0000;
const PORT_BTN_BITS: u16 = 0b0000_0000_0000_0000;
const PORT_POS_MASK: u16 = 0b0000_0101_0000_0000;
const PORT_X_MASK:   u16 = 0b0000_0001_0000_0000;
const PORT_Y_MASK:   u16 = 0b0000_0101_0000_0000;

/// The Kempston Mouse device implements [MouseDevice] and [MouseInterface] traits.
///
/// The horizontal position increases when moving to the right and decreases when moving to the left.
/// The vertical position increases when moving from bottom to the top or forward (away from the user)
/// and decreases then moving backward (towards the user).
///
/// A horizontal position is being provided when bits of the port address: A8 is 1 and A10 is 0.
/// A vertical position is being provided when bits of the port address: A8 and A10 is 1.
/// A button state is being provided when A8 bit of the port address is 0:
///
/// * bit 0 is 0 when the left button is being pressed and 1 when the left button is released.
/// * bit 1 is 0 when the right button is being pressed and 1 when the right button is released.
#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "snapshot", serde(rename_all = "camelCase"))]
pub struct KempstonMouseDevice {
    #[cfg_attr(feature = "snapshot", serde(skip, default = "u8::max_value"))]
    data_btn: u8,
    data_x: u8,
    data_y: u8,
    #[cfg_attr(feature = "snapshot", serde(skip))]
    buttons: MouseButtons,
}

impl Default for KempstonMouseDevice {
    fn default() -> Self {
        KempstonMouseDevice {
            data_btn: !0,
            data_x: !0,
            data_y: !0,
            buttons: Default::default(),
        }
    }
}

impl MouseDevice for KempstonMouseDevice {
    #[inline]
    fn port_read(&self, port: u16) -> u8 {
        if port & PORT_BTN_MASK == PORT_BTN_BITS {
            self.data_btn
        }
        else {
            match port & PORT_POS_MASK {
                PORT_X_MASK => self.data_x,
                PORT_Y_MASK => self.data_y,
                _ => unsafe { core::hint::unreachable_unchecked() }
            }
        }
    }
}

impl MouseInterface for KempstonMouseDevice {
    #[inline]
    fn set_buttons(&mut self, buttons: MouseButtons) {
        self.buttons = buttons;
        self.data_btn = (self.data_btn & UNUSED_BTN_MASK) |
            if buttons.intersects(MouseButtons::LEFT)  { 0 } else { LEFT_BTN_MASK  } |
            if buttons.intersects(MouseButtons::RIGHT) { 0 } else { RIGHT_BTN_MASK };
    }
    #[inline]
    fn get_buttons(&self) -> MouseButtons {
        self.buttons
    }
    #[inline]
    fn move_mouse(&mut self, movement: MouseMovement) {
        self.data_x = clamped_move(self.data_x, movement.horizontal);
        self.data_y = clamped_move(self.data_y, -movement.vertical);
    }
}

#[inline(always)]
fn clamped_move(prev: u8, mut delta: i16) -> u8 {
    delta >>= 1;
    if delta < i8::min_value().into() {
        delta = i8::min_value().into();
    }
    else if delta > i8::max_value().into() {
        delta = i8::max_value().into();
    }
    prev.wrapping_add(delta as u8)
}
