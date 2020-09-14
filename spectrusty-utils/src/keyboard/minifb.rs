/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
//! Keyboard related functions to be used with [minifb](https://crates.io/crates/minifb).
//!
//! Requires "minifb" feature to be enabled.
use minifb::Key;
use spectrusty::peripherals::{ZXKeyboardMap,
    joystick::{JoystickInterface, Directions},
    serial::KeypadKeys
};

type ZXk = ZXKeyboardMap;

/// Returns a Spectrum key map flags with a single bit set corresponding to the provided `key` code
/// if the key matches one of the Spectrum's.
///
/// The alphanumeric keys, `ENTER` and `SPACE` are mapped as such.
/// Left and right `SHIFT` is mapped as [CAPS SHIFT][ZXKeyboardMap::CS] and left and right `CTRL`
/// is mapped as [SYMBOL SHIFT][ZXKeyboardMap::SS].
///
/// Otherwise returns an empty set.
pub fn map_direct_key(key: Key) -> ZXKeyboardMap {
    match key {
        Key::Key1 => ZXk::N1,
        Key::Key2 => ZXk::N2,
        Key::Key3 => ZXk::N3,
        Key::Key4 => ZXk::N4,
        Key::Key5 => ZXk::N5,
        Key::Key6 => ZXk::N6,
        Key::Key7 => ZXk::N7,
        Key::Key8 => ZXk::N8,
        Key::Key9 => ZXk::N9,
        Key::Key0 => ZXk::N0,
        Key::A => ZXk::A,
        Key::B => ZXk::B,
        Key::C => ZXk::C,
        Key::D => ZXk::D,
        Key::E => ZXk::E,
        Key::F => ZXk::F,
        Key::G => ZXk::G,
        Key::H => ZXk::H,
        Key::I => ZXk::I,
        Key::J => ZXk::J,
        Key::K => ZXk::K,
        Key::L => ZXk::L,
        Key::M => ZXk::M,
        Key::N => ZXk::N,
        Key::O => ZXk::O,
        Key::P => ZXk::P,
        Key::Q => ZXk::Q,
        Key::R => ZXk::R,
        Key::S => ZXk::S,
        Key::T => ZXk::T,
        Key::U => ZXk::U,
        Key::V => ZXk::V,
        Key::W => ZXk::W,
        Key::X => ZXk::X,
        Key::Y => ZXk::Y,
        Key::Z => ZXk::Z,
        Key::LeftShift|Key::RightShift => ZXk::CS,
        Key::LeftCtrl|Key::RightCtrl => ZXk::SS,
        Key::Space => ZXk::BR,
        Key::Enter => ZXk::EN,
        _ => ZXk::empty()
    }
}

/// Returns a Spectrum key map flags with some bits set corresponding to the provided `key` code
/// if the provided key matches one or more of the Spectrum keys.
///
/// The second argument returned is `true` if the [ZXKeyboardMap::CS] should be removed from
/// the updated keymap.
///
/// The key combination includes cursor keys (←→↑↓) and some non alphanumeric keys as shortcuts to
/// corresponding Spectrum key combinations reachable via pressing `SYMBOL` or `CAPS` with another Spectrum key.
///
/// * `pressed` should be `true` if the `key` has been pressed down or `false` if it has been released.
/// * `shift_down` should be `true` if one of the `SHIFT` key modifiers has been held down and `false` otherwise.
pub fn map_combined_keys(key: Key, pressed: bool, shift_down: bool) -> (ZXKeyboardMap, bool) {
    let mut removecs = false;
    let zxk = match key {
        Key::Left => ZXk::CS|ZXk::N5,
        Key::Down => ZXk::CS|ZXk::N6,
        Key::Up => ZXk::CS|ZXk::N7,
        Key::Right => ZXk::CS|ZXk::N8,

        Key::CapsLock => ZXk::CS|ZXk::N2,
        Key::Backspace => ZXk::CS|ZXk::N0,

        Key::LeftAlt|Key::RightAlt => ZXk::CS|ZXk::SS,

        Key::LeftBracket => ZXk::SS|ZXk::N8,
        Key::RightBracket => ZXk::SS|ZXk::N9,
        Key::Backquote => ZXk::SS|ZXk::X,

        Key::Minus if pressed => if shift_down {
            removecs = true;
            ZXk::SS|ZXk::N0
        }
        else {
            ZXk::SS|ZXk::J
        },
        Key::Minus => ZXk::SS|ZXk::J|ZXk::N0,

        Key::Equal if pressed => if shift_down {
            removecs = true;
            ZXk::SS|ZXk::K
        }
        else {
            ZXk::SS|ZXk::L
        },
        Key::Equal => ZXk::SS|ZXk::K|ZXk::L,

        Key::Comma if pressed => if shift_down {
            removecs = true;
            ZXk::SS|ZXk::R
        }
        else {
            ZXk::SS|ZXk::N
        },
        Key::Comma => ZXk::SS|ZXk::R|ZXk::N,

        Key::Period if pressed => if shift_down {
            removecs = true;
            ZXk::SS|ZXk::T
        }
        else {
            ZXk::SS|ZXk::M
        },
        Key::Period => ZXk::SS|ZXk::T|ZXk::M,

        Key::Apostrophe if pressed => if shift_down {
            removecs = true;
            ZXk::SS|ZXk::P
        }
        else {
            ZXk::SS|ZXk::N7
        },
        Key::Apostrophe => ZXk::SS|ZXk::P|ZXk::N7,

        Key::Slash if pressed => if shift_down {
            removecs = true;
            ZXk::SS|ZXk::C
        }
        else {
            ZXk::SS|ZXk::V
        },
        Key::Slash => ZXk::SS|ZXk::C|ZXk::V,

        Key::Semicolon if pressed => if shift_down {
            removecs = true;
            ZXk::SS|ZXk::Z
        }
        else {
            ZXk::SS|ZXk::O
        },
        Key::Semicolon => ZXk::SS|ZXk::Z|ZXk::O,

        k => map_direct_key(k)
    };
    (zxk, removecs)
}

/// Returns an updated Spectrum key map state from a `key` down or up event.
///
/// * `cur` is the current key map state.
/// * `key` is the key code.
/// * `pressed` should be `true` if the `key` has been pressed down and `false` if it has been released.
/// * `shift_down` should be `true` if one of the `SHIFT` key modifiers has been held down and `false` otherwise.
/// * `ctrl_down` should be `true` if one of the `CTRL` key modifiers has been held down and `false` otherwise.
pub fn update_keymap(
        mut cur: ZXKeyboardMap,
        key: Key,
        pressed: bool,
        shift_down: bool,
        ctrl_down: bool
    ) -> ZXKeyboardMap
{
    let (chg, removecs) = map_combined_keys(key, pressed, shift_down);
    if pressed {
        cur.insert(chg);
        if removecs {
            cur.remove(ZXk::CS);
        }
    }
    else {
        cur.remove(chg);
    }

    if cur.is_empty() {
        if shift_down {
            cur.insert(ZXk::CS);
        }
        if ctrl_down {
            cur.insert(ZXk::SS);
        }
    }
    cur
}

/// Returns a keypad's key map flags with a single bit set corresponding to the provided `key` code
/// if the key matches one of the Spectrum 128k keypad's.
///
/// The numeric keypad keys, `ENTER`, `PERIOD`, `+`, `-` are mapped as such.
/// If `parens` is `true` the `/` key is mapped as [LPAREN] and `*` is mapped as [RPAREN].
/// If `parens` is `false` keys `/` and `*` are mapped as such.
///
/// Otherwise returns an empty set.
///
/// [LPAREN]: KeypadKeys::LPAREN
/// [RPAREN]: KeypadKeys::RPAREN
pub fn map_keypad_key(key: Key, parens: bool) -> KeypadKeys {
    match key {
        Key::NumPadSlash if parens => KeypadKeys::LPAREN,
        Key::NumPadSlash => KeypadKeys::DIVIDE,
        Key::NumPadAsterisk if parens => KeypadKeys::RPAREN,
        Key::NumPadAsterisk => KeypadKeys::MULTIPLY,
        Key::NumPadMinus => KeypadKeys::MINUS,
        Key::NumPadPlus => KeypadKeys::PLUS,
        Key::NumPadEnter => KeypadKeys::ENTER,
        Key::NumPad1 => KeypadKeys::N1,
        Key::NumPad2 => KeypadKeys::N2,
        Key::NumPad3 => KeypadKeys::N3,
        Key::NumPad4 => KeypadKeys::N4,
        Key::NumPad5 => KeypadKeys::N5,
        Key::NumPad6 => KeypadKeys::N6,
        Key::NumPad7 => KeypadKeys::N7,
        Key::NumPad8 => KeypadKeys::N8,
        Key::NumPad9 => KeypadKeys::N9,
        Key::NumPad0 => KeypadKeys::N0,
        Key::NumPadDot => KeypadKeys::PERIOD,
        _ => KeypadKeys::empty()
    }
}

/// Returns an updated keypad's key map state from a key down or up event.
///
/// * `cur` is the current Spectrum 128k keypad's key map state.
/// * `key` is the key code.
/// * `pressed` should be `true` if the `key` has been pressed down or `false` if it has been released.
/// * `parens` should be `true` if `/` and `*` keys should be mapped as [LPAREN] and [RPAREN], `false` otherwise.
///
/// [LPAREN]: KeypadKeys::LPAREN
/// [RPAREN]: KeypadKeys::RPAREN
pub fn update_keypad_keys(mut cur: KeypadKeys, key: Key, pressed: bool, parens: bool) -> KeypadKeys {
    let chg = map_keypad_key(key, parens);
    if !chg.is_empty() {
        cur.set(chg, pressed);
    }
    cur
}

/// Returns joystick direction flags with a single direction bit set if a `key` is one of ← → ↑ ↓ keys.
pub fn map_key_to_direction(key: Key) -> Directions {
    match key {
        Key::Up    => Directions::UP,
        Key::Right => Directions::RIGHT,
        Key::Down  => Directions::DOWN,
        Key::Left  => Directions::LEFT,
        _          => Directions::empty()
    }
}

/// Updates the state of joystick device via [JoystickInterface] from a key down or up event.
///
/// Returns `true` if the state of the joystick device was updated.
/// Returns `false` if the `key` wasn't any of the ← → ↑ ↓ or `fire_key` keys or if `get_joy`
/// returns `None`.
///
/// * `key` is the key code.
/// * `pressed` indicates if the `key` was pressed (`true`) or released (`false`).
/// * `fire_key` is the key code for the `FIRE` button.
/// * `get_joy` should return a mutable reference to the [JoystickInterface] implementation instance
///   if such instance is available.
#[inline]
pub fn update_joystick_from_key_event<'a, J, F>(
            key: Key,
            pressed: bool,
            fire_key: Key,
            get_joy: F
        ) -> bool
    where J: 'a + JoystickInterface + ?Sized,
          F: FnOnce() -> Option<&'a mut J>
{
    super::update_joystick_from_key_event(key, pressed, fire_key, map_key_to_direction, get_joy)
}
