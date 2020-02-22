use sdl2::keyboard::{Mod as Modifier, Keycode};
use sdl2::mouse::MouseButton;
use zxspecemu::memory::ZxMemory;
use zxspecemu::bus::joystick::MultiJoystickBusDevice;
use zxspecemu::bus::joystick::JoystickSelect;
use zxspecemu::peripherals::{KeyboardInterface, ZXKeyboardMap};
use zxspecemu::peripherals::joystick::Directions;
use zxspecemu::peripherals::mouse::{MouseInterface, MouseButtons, kempston::KempstonMouseDevice};
use zxspecemu::chip::ula::Ula;
use zxspecemu::video::{Video, BorderSize, VideoFrame};

#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

use super::spectrum::ZXSpectrum;
use super::printer::ImageSpooler;

pub trait SpoolerAccess {
    fn spooler_mut(&mut self) -> Option<&mut ImageSpooler> { None }
}

pub trait MouseAccess {
    fn mouse_mut(&mut self) -> Option<&mut KempstonMouseDevice> { None }
}

pub trait JoystickAccess {
    fn joystick_device_mut(&mut self) -> &mut Option<MultiJoystickBusDevice>;// { &mut None }
    fn joystick_device_ref(&self) -> &Option<MultiJoystickBusDevice>;// { &None }
    fn joystick_mut(&mut self) -> Option<&mut JoystickSelect> {
        self.joystick_device_mut().as_deref_mut()
    }
    fn joystick_ref(&self) -> Option<&JoystickSelect> {
        self.joystick_device_ref().as_deref()
    }
}

impl<C, M, B> ZXSpectrum<C, M, B>
    where M: ZxMemory
{
    pub fn update_mouse_position(&mut self, dx: i32, dy: i32, border: BorderSize, viewport: (u32, u32))
        where Self: MouseAccess
    {
        if let Some(mouse) = self.mouse_mut() {
            let (sx, sy) = <Ula::<M, B> as Video>::VideoFrame::screen_size_pixels(border);
            let (vx, vy) = viewport;
            let dx = (dx * 2 * sx as i32 / vx as i32) as i16;
            let dy = (dy * 2 * sy as i32 / vy as i32) as i16;
            // println!("{}x{}", dx, dy);
            mouse.move_mouse((dx, dy))
        }
    }

    pub fn update_mouse_button(&mut self, button: MouseButton, pressed: bool)
        where Self: MouseAccess
    {
        if let Some(mouse) = self.mouse_mut() {
            let button_mask = match button {
                MouseButton::Left => MouseButtons::LEFT,
                MouseButton::Right => MouseButtons::RIGHT,
                _ => return
            };
            let buttons = mouse.get_buttons();
            mouse.set_buttons(if pressed {
                buttons|button_mask
            }
            else {
                buttons&!button_mask
            })
        }
    }

    #[inline]
    pub fn update_keypress(&mut self, keycode: Keycode, modifier: Modifier, pressed: bool)
        where Self: JoystickAccess
    {
        self.ula.set_key_state(
            map_keys(self.ula.get_key_state(), keycode, modifier, pressed,
                     self.joystick_ref().is_none()));
    }

    pub fn next_joystick(&mut self)
        where Self: JoystickAccess
    {
        let joystick_index = self.joystick_index;
        if let Some(joystick) = self.joystick_mut() {
            if joystick.is_last() {
                *self.joystick_device_mut() = None;
            }
            else {
                self.joystick_index = joystick.select_next_joystick(joystick_index);
            }
        }
        else {
            *self.joystick_device_mut() = Some(Default::default());
        }
        if let Some(joy) = self.joystick_ref() {
            println!("Cursor keys: {} joystick: #{}", joy, self.joystick_index + 1);
        }
        else {
            println!("Cursor keys: keyboard");
        }
    }

    #[inline]
    pub fn cursor_to_joystick(&mut self, keycode: Keycode, pressed: bool) -> bool
        where Self: JoystickAccess
    {
        let joystick_index = self.joystick_index;
        if let Some(joy) = self.joystick_mut() {
            if let Some(directions) = try_keys_to_directions(keycode) {
                let joy = joy.joystick_interface(joystick_index).unwrap();
                joy.set_directions(if pressed {
                    joy.get_directions()|directions
                }
                else {
                    joy.get_directions()&!directions
                });
                return true
            }
            else if keycode == Keycode::RCtrl {
                joy.joystick_interface(joystick_index).unwrap()
                   .fire(0, pressed);
                return true
            }
        }
        false
    }
}

/// Optionally convert cursor key codes to joystick directions.
pub fn try_keys_to_directions(keycode: Keycode) -> Option<Directions> {
    match keycode {
        Keycode::Right => Some(Directions::RIGHT),
        Keycode::Left => Some(Directions::LEFT),
        Keycode::Down => Some(Directions::DOWN),
        Keycode::Up => Some(Directions::UP),
        _ => None
    }
}

/// Updates ZXKeyboardMap from SDL2 key event data
pub fn map_keys(mut zxk: ZXKeyboardMap, keycode: Keycode, modifier: Modifier, pressed: bool, use_rctrl: bool) -> ZXKeyboardMap {
    type ZXk = ZXKeyboardMap;
    let zxk_chg = match keycode {
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
        Keycode::Return|Keycode::KpEnter => ZXk::EN,
        Keycode::CapsLock => ZXk::CS|ZXk::N2,
        Keycode::Backspace => ZXk::CS|ZXk::N0,
        Keycode::LAlt|Keycode::RAlt => ZXk::CS|ZXk::SS,

        Keycode::Left => ZXk::CS|ZXk::N5,
        Keycode::Down => ZXk::CS|ZXk::N6,
        Keycode::Up => ZXk::CS|ZXk::N7,
        Keycode::Right => ZXk::CS|ZXk::N8,

        Keycode::Minus => if modifier.intersects(Modifier::LSHIFTMOD|Modifier::RSHIFTMOD) {
            zxk.remove(ZXk::CS);
            ZXk::SS|ZXk::N0
        }
        else {
            ZXk::SS|ZXk::J
        },
        Keycode::Equals => if modifier.intersects(Modifier::LSHIFTMOD|Modifier::RSHIFTMOD) {
            zxk.remove(ZXk::CS);
            ZXk::SS|ZXk::K
        }
        else {
            ZXk::SS|ZXk::L
        },
        Keycode::LeftBracket => ZXk::SS|ZXk::N8,
        Keycode::RightBracket => ZXk::SS|ZXk::N9,
        Keycode::Comma => if modifier.intersects(Modifier::LSHIFTMOD|Modifier::RSHIFTMOD) {
            zxk.remove(ZXk::CS);
            ZXk::SS|ZXk::R
        }
        else {
            ZXk::SS|ZXk::N
        },
        Keycode::Period => if modifier.intersects(Modifier::LSHIFTMOD|Modifier::RSHIFTMOD) {
            zxk.remove(ZXk::CS);
            ZXk::SS|ZXk::T
        }
        else {
            ZXk::SS|ZXk::M
        },
        Keycode::Quote => if modifier.intersects(Modifier::LSHIFTMOD|Modifier::RSHIFTMOD) {
            zxk.remove(ZXk::CS);
            ZXk::SS|ZXk::P
        }
        else {
            ZXk::SS|ZXk::N7
        },
        Keycode::Slash => if modifier.intersects(Modifier::LSHIFTMOD|Modifier::RSHIFTMOD) {
            zxk.remove(ZXk::CS);
            ZXk::SS|ZXk::C
        }
        else {
            ZXk::SS|ZXk::V
        },
        Keycode::Semicolon => if modifier.intersects(Modifier::LSHIFTMOD|Modifier::RSHIFTMOD) {
            zxk.remove(ZXk::CS);
            ZXk::SS|ZXk::Z
        }
        else {
            ZXk::SS|ZXk::O
        },
        Keycode::Backquote => ZXk::SS|ZXk::X,
        Keycode::KpDivide => ZXk::SS|ZXk::V,
        Keycode::KpMultiply => ZXk::SS|ZXk::B,
        Keycode::KpPlus => ZXk::SS|ZXk::K,
        Keycode::KpMinus => ZXk::SS|ZXk::J,
        _ => ZXk::empty()
    };
    if pressed {
        zxk.insert(zxk_chg);
    }
    else {
        zxk.remove(zxk_chg);
    }
    if zxk.is_empty() {
        if modifier.intersects(Modifier::LSHIFTMOD|Modifier::RSHIFTMOD) {
            zxk.insert(ZXk::CS);
        }
        if modifier.intersects(if use_rctrl {
                Modifier::LCTRLMOD|Modifier::RCTRLMOD
            }
            else {
                Modifier::LCTRLMOD
            }) {
            zxk.insert(ZXk::SS);
        }
    }
    zxk
}
