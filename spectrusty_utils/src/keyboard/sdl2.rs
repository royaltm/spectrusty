use sdl2::keyboard::{Mod as Modifier, Keycode};
use spectrusty_peripherals::ZXKeyboardMap;

type ZXk = ZXKeyboardMap;

/// Returns a Spectrum key map state with a single bit set corresponding to the provided `key`
/// if the provided key matches one of the Spectrum's.
///
/// The alphanumeric keys, `ENTER` and `SPACE` are mapped as such.
/// Left and right `SHIFT` is mapped as [CAPS SHIFT][ZXKeyboardMap::CS] and left and right `CTRL`
/// is mapped as [SYMBOL SHIFT][ZXKeyboardMap::SS].
///
/// Otherwise returns an empty set.
pub fn map_direct_keys(key: Keycode) -> ZXKeyboardMap {
    match key {
        Keycode::Num1 => ZXk::N1,
        Keycode::Num2 => ZXk::N2,
        Keycode::Num3 => ZXk::N3,
        Keycode::Num4 => ZXk::N4,
        Keycode::Num5 => ZXk::N5,
        Keycode::Num6 => ZXk::N6,
        Keycode::Num7 => ZXk::N7,
        Keycode::Num8 => ZXk::N8,
        Keycode::Num9 => ZXk::N9,
        Keycode::Num0 => ZXk::N0,
        Keycode::A => ZXk::A,
        Keycode::B => ZXk::B,
        Keycode::C => ZXk::C,
        Keycode::D => ZXk::D,
        Keycode::E => ZXk::E,
        Keycode::F => ZXk::F,
        Keycode::G => ZXk::G,
        Keycode::H => ZXk::H,
        Keycode::I => ZXk::I,
        Keycode::J => ZXk::J,
        Keycode::K => ZXk::K,
        Keycode::L => ZXk::L,
        Keycode::M => ZXk::M,
        Keycode::N => ZXk::N,
        Keycode::O => ZXk::O,
        Keycode::P => ZXk::P,
        Keycode::Q => ZXk::Q,
        Keycode::R => ZXk::R,
        Keycode::S => ZXk::S,
        Keycode::T => ZXk::T,
        Keycode::U => ZXk::U,
        Keycode::V => ZXk::V,
        Keycode::W => ZXk::W,
        Keycode::X => ZXk::X,
        Keycode::Y => ZXk::Y,
        Keycode::Z => ZXk::Z,
        Keycode::LShift|Keycode::RShift => ZXk::CS,
        Keycode::LCtrl|Keycode::RCtrl => ZXk::SS,
        Keycode::Space => ZXk::BR,
        Keycode::Return => ZXk::EN,
        _ => ZXk::empty()
    }
}

/// Returns a Spectrum key map state with some bits set corresponding to the provided `key`
/// if the provided key matches one or more of the Spectrum keys.
///
/// The second returned argument if `true` indicates that the [ZXKeyboardMap::CS] should be removed from
/// the updated keymap.
///
/// The key combination includes cursor keys and some non alphanumeric keys as shortcuts to corresponding
/// Spectrum keys reachable via pressing `SYMBOL` or `CAPS` with some other Spectrum key.
///
/// * `pressed` should be `true` if the `key` has been pressed down and `false` if it has been released.
/// * `shift_down` should be `true` if one of the `SHIFT` key modifiers has been held down and `false` otherwise.
pub fn map_combined_keys(key: Keycode, pressed: bool, shift_down: bool) -> (ZXKeyboardMap, bool) {
    let mut removecs = false;
    let zxk = match key {
        Keycode::Left => ZXk::CS|ZXk::N5,
        Keycode::Down => ZXk::CS|ZXk::N6,
        Keycode::Up => ZXk::CS|ZXk::N7,
        Keycode::Right => ZXk::CS|ZXk::N8,

        Keycode::CapsLock => ZXk::CS|ZXk::N2,
        Keycode::Backspace => ZXk::CS|ZXk::N0,

        Keycode::LAlt|Keycode::RAlt => ZXk::CS|ZXk::SS,

        Keycode::LeftBracket => ZXk::SS|ZXk::N8,
        Keycode::RightBracket => ZXk::SS|ZXk::N9,
        Keycode::Backquote => ZXk::SS|ZXk::X,

        Keycode::Minus if pressed => if shift_down {
            removecs = true;
            ZXk::SS|ZXk::N0
        }
        else {
            ZXk::SS|ZXk::J
        },
        Keycode::Minus => ZXk::SS|ZXk::J|ZXk::N0,

        Keycode::Equals if pressed => if shift_down {
            removecs = true;
            ZXk::SS|ZXk::K
        }
        else {
            ZXk::SS|ZXk::L
        },
        Keycode::Equals => ZXk::SS|ZXk::K|ZXk::L,

        Keycode::Comma if pressed => if shift_down {
            removecs = true;
            ZXk::SS|ZXk::R
        }
        else {
            ZXk::SS|ZXk::N
        },
        Keycode::Comma => ZXk::SS|ZXk::R|ZXk::N,

        Keycode::Period if pressed => if shift_down {
            removecs = true;
            ZXk::SS|ZXk::T
        }
        else {
            ZXk::SS|ZXk::M
        },
        Keycode::Period => ZXk::SS|ZXk::T|ZXk::M,

        Keycode::Quote if pressed => if shift_down {
            removecs = true;
            ZXk::SS|ZXk::P
        }
        else {
            ZXk::SS|ZXk::N7
        },
        Keycode::Quote => ZXk::SS|ZXk::P|ZXk::N7,

        Keycode::Slash if pressed => if shift_down {
            removecs = true;
            ZXk::SS|ZXk::C
        }
        else {
            ZXk::SS|ZXk::V
        },
        Keycode::Slash => ZXk::SS|ZXk::C|ZXk::V,

        Keycode::Semicolon if pressed => if shift_down {
            removecs = true;
            ZXk::SS|ZXk::Z
        }
        else {
            ZXk::SS|ZXk::O
        },
        Keycode::Semicolon => ZXk::SS|ZXk::Z|ZXk::O,

        k => map_direct_keys(k)
    };
    (zxk, removecs)
}

/// Returns an updated Spectrum key map state with some bits set and reset corresponding to the provided `key`.
///
/// * `cur` is the current key map state.
/// * `pressed` should be `true` if the `key` has been pressed down and `false` if it has been released.
/// * `shift_down` should be `true` if one of the `SHIFT` key modifiers has been held down and `false` otherwise.
/// * `ctrl_down` should be `true` if one of the `CTRL` key modifiers has been held down and `false` otherwise.
pub fn update_keymap(
        mut cur: ZXKeyboardMap,
        key: Keycode,
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

/// Returns an updated Spectrum key map state with some bits set and reset corresponding to the provided `key`.
///
/// * `cur` is the current key map state.
/// * `pressed` should be `true` if the `key` has been pressed down and `false` if it has been released.
/// * `modifier` is the `keymod` property from the `KeyDown` or `KeyUp` events.
pub fn update_keymap_with_modifier(
        cur: ZXKeyboardMap,
        key: Keycode,
        pressed: bool,
        modifier: Modifier
    ) -> ZXKeyboardMap
{
    let shift_down = modifier.intersects(Modifier::LSHIFTMOD|Modifier::RSHIFTMOD);
    let ctrl_down = modifier.intersects(Modifier::LCTRLMOD|Modifier::RCTRLMOD);
    update_keymap(cur, key, pressed, shift_down, ctrl_down)
}
