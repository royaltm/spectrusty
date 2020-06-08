//! Keyboard related functions to be used with [web-sys](https://crates.io/crates/web-sys).
//!
//! Requires "web-sys" feature to be enabled.
use web_sys::KeyboardEvent;
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
pub fn map_direct_key(key: &str) -> ZXKeyboardMap {
    match key {
        "Digit1" => ZXk::N1,
        "Digit2" => ZXk::N2,
        "Digit3" => ZXk::N3,
        "Digit4" => ZXk::N4,
        "Digit5" => ZXk::N5,
        "Digit6" => ZXk::N6,
        "Digit7" => ZXk::N7,
        "Digit8" => ZXk::N8,
        "Digit9" => ZXk::N9,
        "Digit0" => ZXk::N0,
        "KeyA"   => ZXk::A,
        "KeyB"   => ZXk::B,
        "KeyC"   => ZXk::C,
        "KeyD"   => ZXk::D,
        "KeyE"   => ZXk::E,
        "KeyF"   => ZXk::F,
        "KeyG"   => ZXk::G,
        "KeyH"   => ZXk::H,
        "KeyI"   => ZXk::I,
        "KeyJ"   => ZXk::J,
        "KeyK"   => ZXk::K,
        "KeyL"   => ZXk::L,
        "KeyM"   => ZXk::M,
        "KeyN"   => ZXk::N,
        "KeyO"   => ZXk::O,
        "KeyP"   => ZXk::P,
        "KeyQ"   => ZXk::Q,
        "KeyR"   => ZXk::R,
        "KeyS"   => ZXk::S,
        "KeyT"   => ZXk::T,
        "KeyU"   => ZXk::U,
        "KeyV"   => ZXk::V,
        "KeyW"   => ZXk::W,
        "KeyX"   => ZXk::X,
        "KeyY"   => ZXk::Y,
        "KeyZ"   => ZXk::Z,
        "ShiftLeft"|"ShiftRight" => ZXk::CS,
        "ControlLeft"|"ControlRight" => ZXk::SS,
        "Space" => ZXk::BR,
        "Enter" => ZXk::EN,
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
pub fn map_combined_keys(key: &str, pressed: bool, shift_down: bool) -> (ZXKeyboardMap, bool) {
    let mut removecs = false;
    let zxk = match key {
        "ArrowLeft" => ZXk::CS|ZXk::N5,
        "ArrowDown" => ZXk::CS|ZXk::N6,
        "ArrowUp" => ZXk::CS|ZXk::N7,
        "ArrowRight" => ZXk::CS|ZXk::N8,

        "CapsLock" => ZXk::CS|ZXk::N2,
        "Backspace" => ZXk::CS|ZXk::N0,

        "AltLeft"|"AltRight" => ZXk::CS|ZXk::SS,

        "BracketLeft" => ZXk::SS|ZXk::N8,
        "BracketRight" => ZXk::SS|ZXk::N9,
        "Backquote" => ZXk::SS|ZXk::X,

        "Minus" if pressed => if shift_down {
            removecs = true;
            ZXk::SS|ZXk::N0
        }
        else {
            ZXk::SS|ZXk::J
        },
        "Minus" => ZXk::SS|ZXk::J|ZXk::N0,

        "Equal" if pressed => if shift_down {
            removecs = true;
            ZXk::SS|ZXk::K
        }
        else {
            ZXk::SS|ZXk::L
        },
        "Equal" => ZXk::SS|ZXk::K|ZXk::L,

        "Comma" if pressed => if shift_down {
            removecs = true;
            ZXk::SS|ZXk::R
        }
        else {
            ZXk::SS|ZXk::N
        },
        "Comma" => ZXk::SS|ZXk::R|ZXk::N,

        "Period" if pressed => if shift_down {
            removecs = true;
            ZXk::SS|ZXk::T
        }
        else {
            ZXk::SS|ZXk::M
        },
        "Period" => ZXk::SS|ZXk::T|ZXk::M,

        "Quote" if pressed => if shift_down {
            removecs = true;
            ZXk::SS|ZXk::P
        }
        else {
            ZXk::SS|ZXk::N7
        },
        "Quote" => ZXk::SS|ZXk::P|ZXk::N7,

        "Slash" if pressed => if shift_down {
            removecs = true;
            ZXk::SS|ZXk::C
        }
        else {
            ZXk::SS|ZXk::V
        },
        "Slash" => ZXk::SS|ZXk::C|ZXk::V,

        "Semicolon" if pressed => if shift_down {
            removecs = true;
            ZXk::SS|ZXk::Z
        }
        else {
            ZXk::SS|ZXk::O
        },
        "Semicolon" => ZXk::SS|ZXk::Z|ZXk::O,

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
        key: &str,
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

/// Returns an updated Spectrum key map state from a `key` down or up event.
///
/// * `cur` is the current key map state.
/// * `key` is the key code.
/// * `pressed` should be `true` if the `key` has been pressed down and `false` if it has been released.
/// * `modifier` is the `keymod` property from the `KeyDown` or `KeyUp` events.
pub fn update_keymap_with_event(
        cur: ZXKeyboardMap,
        event: &KeyboardEvent,
        pressed: bool,
    ) -> ZXKeyboardMap
{
    let shift_down = event.shift_key();
    let ctrl_down = event.ctrl_key();
    let key = event.code();
    update_keymap(cur, &key, pressed, shift_down, ctrl_down)
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
pub fn map_keypad_key(key: &str, parens: bool) -> KeypadKeys {
    match key {
        "NumpadDivide" if parens => KeypadKeys::LPAREN,
        "NumpadDivide" => KeypadKeys::DIVIDE,
        "NumpadMultiply" if parens => KeypadKeys::RPAREN,
        "NumpadMultiply" => KeypadKeys::MULTIPLY,
        "NumpadSubtract" => KeypadKeys::MINUS,
        "NumpadAdd" => KeypadKeys::PLUS,
        "NumpadEnter" => KeypadKeys::ENTER,
        "Numpad1" => KeypadKeys::N1,
        "Numpad2" => KeypadKeys::N2,
        "Numpad3" => KeypadKeys::N3,
        "Numpad4" => KeypadKeys::N4,
        "Numpad5" => KeypadKeys::N5,
        "Numpad6" => KeypadKeys::N6,
        "Numpad7" => KeypadKeys::N7,
        "Numpad8" => KeypadKeys::N8,
        "Numpad9" => KeypadKeys::N9,
        "Numpad0" => KeypadKeys::N0,
        "NumpadDecimal" => KeypadKeys::PERIOD,
        _ => KeypadKeys::empty()
    }
}

/// Returns an updated keypad's key map state from a key down or up event.
///
/// * `cur` is the current Spectrum 128k keypad's key map state.
/// * `key` is the key code.
/// * `pressed` should be `true` if the `key` has been pressed down or `false` if it has been released.
/// * `parens` should be `true` if `/` and `*` keys should be mapped as [LPAREN] and [RPAREN], `false` otherwise.
pub fn update_keypad_keys(mut cur: KeypadKeys, key: &str, pressed: bool, parens: bool) -> KeypadKeys {
    let chg = map_keypad_key(key, parens);
    if !chg.is_empty() {
        cur.set(chg, pressed);
    }
    cur
}

/// Returns an updated keypad key map state from a key down or up event.
///
/// * `cur` is the current key map state.
/// * `key` is the key code.
/// * `pressed` should be `true` if the `key` has been pressed down or `false` if it has been released.
/// * `modifier` is the `keymod` property from the `KeyDown` or `KeyUp` events.
#[inline]
pub fn update_keypad_keys_with_event(cur: KeypadKeys, event: &KeyboardEvent, pressed: bool) -> KeypadKeys {
    let parens = event.get_modifier_state("NumLock");
    let key = event.code();
    update_keypad_keys(cur, &key, pressed, parens)
}

/// Returns joystick direction flags with a single direction bit set if a `key` is one of ← → ↑ ↓ keys.
pub fn map_key_to_direction(key: &str) -> Directions {
    match key {
        "ArrowUp"    => Directions::UP,
        "ArrowRight" => Directions::RIGHT,
        "ArrowDown"  => Directions::DOWN,
        "ArrowLeft"  => Directions::LEFT,
        _            => Directions::empty()
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
            key: &str,
            pressed: bool,
            fire_key: &str,
            get_joy: F
        ) -> bool
    where J: 'a + JoystickInterface + ?Sized,
          F: FnOnce() -> Option<&'a mut J>
{
    super::update_joystick_from_key_event(key, pressed, fire_key, map_key_to_direction, get_joy)
}
