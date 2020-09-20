/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
//! Keyboard related utilities.
//!
//! To make use of one of the event loop dependent implementation of the keyboard utilities add one of the
//! available features to the `[dependencies]` section in the Cargo configuration file.
use spectrusty::peripherals::{
    joystick::{JoystickInterface, Directions}
};

#[cfg(feature = "minifb")]
pub mod minifb;

#[cfg(feature = "sdl2")]
pub mod sdl2;

#[cfg(feature = "winit")]
pub mod winit;

#[cfg(feature = "web-sys")]
pub mod web_sys;

/// Updates the state of the joystick device via [JoystickInterface] from a key down or up event.
///
/// Returns `true` if the state of the joystick device was updated.
/// Returns `false` if the `key` didn't correspond to any of the keys mapped to joystick directions
/// or its fire button or if `get_joy` returns `None`.
///
/// * `key` is the key code.
/// * `pressed` indicates if the `key` was pressed (`true`) or released (`false`).
/// * `fire_key` is the key code for the `FIRE` button.
/// * `key_to_dir` is a function that returns the joystick direction flags with a single direction
///   bit set if a `key` is one of ← → ↑ ↓ keys.
/// * `get_joy` should return a mutable reference to the [JoystickInterface] implementation instance
///   if such instance is available.
pub fn update_joystick_from_key_event<'a, K, J, D, F>(
            key: K,
            pressed: bool,
            fire_key: K,
            key_to_dir: D,
            get_joy: F
        ) -> bool
    where K: PartialEq + Copy,
          J: 'a + JoystickInterface + ?Sized,
          D: FnOnce(K) -> Directions,
          F: FnOnce() -> Option<&'a mut J>
{
    let directions = key_to_dir(key);
    if !directions.is_empty() {
        if let Some(joy) = get_joy() {
            let mut cur_dirs = joy.get_directions();
            cur_dirs.set(directions, pressed);
            joy.set_directions(cur_dirs);
            return true
        }
    }
    else if key == fire_key {
        if let Some(joy) = get_joy() {
            joy.fire(0, pressed);
            return true
        }
    }
    false
}
