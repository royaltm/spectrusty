use minifb::{Key, KeyRepeat, Window};
use bitflags::bitflags;
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
pub fn map_direct_keys(key: Key) -> ZXKeyboardMap {
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

bitflags! {
    pub struct Modifiers: u8 {
        const LSHIFT  = 0x01;
        const RSHIFT  = 0x02;
        const LCTRL   = 0x04;
        const RCTRL   = 0x08;
        const LRSHIFT = Modifiers::LSHIFT.bits|Modifiers::RSHIFT.bits;
        const LRCTRL  = Modifiers::LCTRL.bits|Modifiers::RCTRL.bits;
        const LONLY   = Modifiers::LSHIFT.bits|Modifiers::LCTRL.bits;
        const RONLY   = Modifiers::RSHIFT.bits|Modifiers::RCTRL.bits;
        const ALL     = Modifiers::LRSHIFT.bits|Modifiers::LRCTRL.bits;
        const NONE    = Modifiers::empty().bits;
    }
}

impl Default for Modifiers {
    fn default() -> Self {
        Modifiers::ALL
    }
}

/// Returns an updated Spectrum key map state with some bits set and reset corresponding to keyboard events
/// sent to the provided `window`.
///
/// `modmask` selects which `SHIFT` and `CTRL` key modifiers should be taken into consideration.
pub fn update_keymap_from_window_events(
        window: &Window,
        mut cur: ZXKeyboardMap,
        modmask: Modifiers
    ) -> ZXKeyboardMap
{
    let shift_dn = modmask.intersects(Modifiers::LSHIFT) && window.is_key_down(Key::LeftShift) ||
                   modmask.intersects(Modifiers::RSHIFT) && window.is_key_down(Key::RightShift);
    let ctrl_dn = modmask.intersects(Modifiers::LCTRL) && window.is_key_down(Key::LeftCtrl) ||
                  modmask.intersects(Modifiers::RCTRL) && window.is_key_down(Key::RightCtrl);
    window.get_keys_pressed(KeyRepeat::No).map(|keys| {
        for k in keys {
            cur = update_keymap(cur, k, true, shift_dn, ctrl_dn);
        }
    });
    window.get_keys_released().map(|keys| {
        for k in keys {
            cur = update_keymap(cur, k, false, shift_dn, ctrl_dn);
        }
    });
    cur
}
