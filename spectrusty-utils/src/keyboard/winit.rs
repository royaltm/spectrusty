/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
//! Keyboard related functions to be used with [winit](https://crates.io/crates/winit).
//!
//! Requires "winit" feature to be enabled.
use winit::event::{VirtualKeyCode, ElementState, ModifiersState};
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
pub fn map_direct_key(key: VirtualKeyCode) -> ZXKeyboardMap {
    match key {
        VirtualKeyCode::Key1 => ZXk::N1,
        VirtualKeyCode::Key2 => ZXk::N2,
        VirtualKeyCode::Key3 => ZXk::N3,
        VirtualKeyCode::Key4 => ZXk::N4,
        VirtualKeyCode::Key5 => ZXk::N5,
        VirtualKeyCode::Key6 => ZXk::N6,
        VirtualKeyCode::Key7 => ZXk::N7,
        VirtualKeyCode::Key8 => ZXk::N8,
        VirtualKeyCode::Key9 => ZXk::N9,
        VirtualKeyCode::Key0 => ZXk::N0,
        VirtualKeyCode::A => ZXk::A,
        VirtualKeyCode::B => ZXk::B,
        VirtualKeyCode::C => ZXk::C,
        VirtualKeyCode::D => ZXk::D,
        VirtualKeyCode::E => ZXk::E,
        VirtualKeyCode::F => ZXk::F,
        VirtualKeyCode::G => ZXk::G,
        VirtualKeyCode::H => ZXk::H,
        VirtualKeyCode::I => ZXk::I,
        VirtualKeyCode::J => ZXk::J,
        VirtualKeyCode::K => ZXk::K,
        VirtualKeyCode::L => ZXk::L,
        VirtualKeyCode::M => ZXk::M,
        VirtualKeyCode::N => ZXk::N,
        VirtualKeyCode::O => ZXk::O,
        VirtualKeyCode::P => ZXk::P,
        VirtualKeyCode::Q => ZXk::Q,
        VirtualKeyCode::R => ZXk::R,
        VirtualKeyCode::S => ZXk::S,
        VirtualKeyCode::T => ZXk::T,
        VirtualKeyCode::U => ZXk::U,
        VirtualKeyCode::V => ZXk::V,
        VirtualKeyCode::W => ZXk::W,
        VirtualKeyCode::X => ZXk::X,
        VirtualKeyCode::Y => ZXk::Y,
        VirtualKeyCode::Z => ZXk::Z,
        VirtualKeyCode::LShift|VirtualKeyCode::RShift => ZXk::CS,
        VirtualKeyCode::LControl|VirtualKeyCode::RControl => ZXk::SS,
        VirtualKeyCode::Space => ZXk::BR,
        VirtualKeyCode::Return => ZXk::EN,
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
/// * `state` should be `Pressed` if the `key` has been pressed down or `Released` if it has been released.
/// * `shift_down` should be `true` if one of the `SHIFT` key modifiers has been held down and `false` otherwise.
pub fn map_combined_keys(key: VirtualKeyCode, state: ElementState, shift_down: bool) -> (ZXKeyboardMap, bool) {
    let mut removecs = false;
    let pressed = state == ElementState::Pressed;
    let zxk = match key {
        VirtualKeyCode::Left => ZXk::CS|ZXk::N5,
        VirtualKeyCode::Down => ZXk::CS|ZXk::N6,
        VirtualKeyCode::Up => ZXk::CS|ZXk::N7,
        VirtualKeyCode::Right => ZXk::CS|ZXk::N8,

        VirtualKeyCode::Capital => ZXk::CS|ZXk::N2,
        VirtualKeyCode::Back => ZXk::CS|ZXk::N0,

        VirtualKeyCode::LAlt|VirtualKeyCode::RAlt => ZXk::CS|ZXk::SS,

        VirtualKeyCode::LBracket => ZXk::SS|ZXk::N8,
        VirtualKeyCode::RBracket => ZXk::SS|ZXk::N9,
        VirtualKeyCode::Grave => ZXk::SS|ZXk::X,

        VirtualKeyCode::Minus if pressed => if shift_down {
            removecs = true;
            ZXk::SS|ZXk::N0
        }
        else {
            ZXk::SS|ZXk::J
        },
        VirtualKeyCode::Minus => ZXk::SS|ZXk::J|ZXk::N0,

        VirtualKeyCode::Equals if pressed => if shift_down {
            removecs = true;
            ZXk::SS|ZXk::K
        }
        else {
            ZXk::SS|ZXk::L
        },
        VirtualKeyCode::Equals => ZXk::SS|ZXk::K|ZXk::L,

        VirtualKeyCode::Comma if pressed => if shift_down {
            removecs = true;
            ZXk::SS|ZXk::R
        }
        else {
            ZXk::SS|ZXk::N
        },
        VirtualKeyCode::Comma => ZXk::SS|ZXk::R|ZXk::N,

        VirtualKeyCode::Period if pressed => if shift_down {
            removecs = true;
            ZXk::SS|ZXk::T
        }
        else {
            ZXk::SS|ZXk::M
        },
        VirtualKeyCode::Period => ZXk::SS|ZXk::T|ZXk::M,

        VirtualKeyCode::Apostrophe if pressed => if shift_down {
            removecs = true;
            ZXk::SS|ZXk::P
        }
        else {
            ZXk::SS|ZXk::N7
        },
        VirtualKeyCode::Apostrophe => ZXk::SS|ZXk::P|ZXk::N7,

        VirtualKeyCode::Slash if pressed => if shift_down {
            removecs = true;
            ZXk::SS|ZXk::C
        }
        else {
            ZXk::SS|ZXk::V
        },
        VirtualKeyCode::Slash => ZXk::SS|ZXk::C|ZXk::V,

        VirtualKeyCode::Semicolon if pressed => if shift_down {
            removecs = true;
            ZXk::SS|ZXk::Z
        }
        else {
            ZXk::SS|ZXk::O
        },
        VirtualKeyCode::Semicolon => ZXk::SS|ZXk::Z|ZXk::O,

        k => map_direct_key(k)
    };
    (zxk, removecs)
}

/// Returns an updated Spectrum key map state from a keyboard input event.
///
/// * `cur` is the current key map state.
/// * `key` is the key code.
/// * `pressed` should be `true` if the `key` has been pressed down and `false` if it has been released.
/// * `shift_down` should be `true` if one of the `SHIFT` key modifiers has been held down and `false` otherwise.
/// * `ctrl_down` should be `true` if one of the `CTRL` key modifiers has been held down and `false` otherwise.
pub fn update_keymap(
        mut cur: ZXKeyboardMap,
        key: VirtualKeyCode,
        state: ElementState,
        shift_down: bool,
        ctrl_down: bool
    ) -> ZXKeyboardMap
{
    let (chg, removecs) = map_combined_keys(key, state, shift_down);
    if let ElementState::Pressed = state {
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

/// Returns an updated Spectrum key map state from a keyboard input event.
///
/// * `cur` is the current key map state.
/// * `key` is the key code.
/// * `pressed` should be `true` if the `key` has been pressed down and `false` if it has been released.
/// * `modifier` is the `modifiers` property from the `KeyboardInput` events.
pub fn update_keymap_with_modifier(
        cur: ZXKeyboardMap,
        key: VirtualKeyCode,
        state: ElementState,
        modifier: ModifiersState
    ) -> ZXKeyboardMap
{
    update_keymap(cur, key, state, modifier.shift(), modifier.ctrl())
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
pub fn map_keypad_key(key: VirtualKeyCode, parens: bool) -> KeypadKeys {
    match key {
        VirtualKeyCode::Divide if parens => KeypadKeys::LPAREN,
        VirtualKeyCode::Divide => KeypadKeys::DIVIDE,
        VirtualKeyCode::Multiply if parens => KeypadKeys::RPAREN,
        VirtualKeyCode::Multiply => KeypadKeys::MULTIPLY,
        VirtualKeyCode::Subtract => KeypadKeys::MINUS,
        VirtualKeyCode::Add => KeypadKeys::PLUS,
        VirtualKeyCode::NumpadEnter => KeypadKeys::ENTER,
        VirtualKeyCode::Numpad1 => KeypadKeys::N1,
        VirtualKeyCode::Numpad2 => KeypadKeys::N2,
        VirtualKeyCode::Numpad3 => KeypadKeys::N3,
        VirtualKeyCode::Numpad4 => KeypadKeys::N4,
        VirtualKeyCode::Numpad5 => KeypadKeys::N5,
        VirtualKeyCode::Numpad6 => KeypadKeys::N6,
        VirtualKeyCode::Numpad7 => KeypadKeys::N7,
        VirtualKeyCode::Numpad8 => KeypadKeys::N8,
        VirtualKeyCode::Numpad9 => KeypadKeys::N9,
        VirtualKeyCode::Numpad0 => KeypadKeys::N0,
        VirtualKeyCode::Decimal => KeypadKeys::PERIOD,
        _ => KeypadKeys::empty()
    }
}

/// Returns an updated keypad's key map state from a keyboard input event.
///
/// * `cur` is the current Spectrum 128k keypad's key map state.
/// * `key` is the key code.
/// * `pressed` should be `true` if the `key` has been pressed down or `false` if it has been released.
/// * `parens` should be `true` if `/` and `*` keys should be mapped as [LPAREN] and [RPAREN], `false` otherwise.
///
/// [LPAREN]: KeypadKeys::LPAREN
/// [RPAREN]: KeypadKeys::RPAREN
pub fn update_keypad_keys(mut cur: KeypadKeys, key: VirtualKeyCode, pressed: bool, parens: bool) -> KeypadKeys {
    let chg = map_keypad_key(key, parens);
    if !chg.is_empty() {
        cur.set(chg, pressed);
    }
    cur
}

/// Returns joystick direction flags with a single direction bit set if a `key` is one of ← → ↑ ↓ keys.
pub fn map_key_to_direction(key: VirtualKeyCode) -> Directions {
    match key {
        VirtualKeyCode::Up    => Directions::UP,
        VirtualKeyCode::Right => Directions::RIGHT,
        VirtualKeyCode::Down  => Directions::DOWN,
        VirtualKeyCode::Left  => Directions::LEFT,
        _                     => Directions::empty()
    }
}

/// Updates the state of joystick device via [JoystickInterface] from a keyboard input event.
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
            key: VirtualKeyCode,
            pressed: bool,
            fire_key: VirtualKeyCode,
            get_joy: F
        ) -> bool
    where J: 'a + JoystickInterface + ?Sized,
          F: FnOnce() -> Option<&'a mut J>
{
    super::update_joystick_from_key_event(key, pressed, fire_key, map_key_to_direction, get_joy)
}
