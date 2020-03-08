// use core::fmt::Display;
mod nonblocking;

use chrono::prelude::*;
use sdl2::keyboard::{Mod as Modifier, Keycode};
use sdl2::mouse::MouseButton;
use zxspecemu::bus::{DynamicBusDevice, OptionalBusDevice, NullDevice,
                    joystick::{JoystickSelect, MultiJoystickBusDevice},
                    zxinterface1::*,
                    zxprinter::*};
use zxspecemu::clock::VideoTs;
use zxspecemu::chip::MemoryAccess;
use zxspecemu::memory::ZxMemory;
use zxspecemu::peripherals::{KeyboardInterface, ZXKeyboardMap};
use zxspecemu::peripherals::ay::serial128::Ay3_8912KeypadRs232;
use zxspecemu::peripherals::joystick::Directions;
use zxspecemu::peripherals::mouse::{MouseInterface, MouseButtons, kempston::KempstonMouseDevice};
use zxspecemu::peripherals::serial::{KeypadKeys, SerialKeypad};
use zxspecemu::video::{Video, BorderSize, VideoFrame};

#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

use crate::spectrum::ZXSpectrum;
use super::printer::{ZxGfxPrinter, ImageSpooler};

pub use zxspecemu::bus::zxinterface1::{MicroCartridge, ZXMicrodrives};
pub use nonblocking::*;


pub type ZxInterface1<V,D=NullDevice<VideoTs>> = ZxInterface1BusDevice<V,
                                        NonBlockingStdinReader,
                                        FilterGfxStdoutWriter,
                                        D>;
pub type ZXPrinterToImage<V> = ZxPrinter<V, ImageSpooler>;
pub type OptJoystickBusDevice<D=NullDevice<VideoTs>> = OptionalBusDevice<MultiJoystickBusDevice, D>;
pub type Ay3_8912 = Ay3_8912KeypadRs232<OptJoystickBusDevice,
                                        NonBlockingStdinReader,
                                        FilterGfxStdoutWriter>;
pub type Rs232<V> = Rs232Io<V, NonBlockingStdinReader, FilterGfxStdoutWriter>;

pub trait DeviceAccess<V> {
    fn spooler_mut(&mut self) -> Option<&mut ImageSpooler> { None }
    fn spooler_ref(&self) -> Option<&ImageSpooler> { None }
    fn gfx_printer_mut(&mut self) -> Option<&mut dyn ZxGfxPrinter> {
        self.spooler_mut().map(|p| -> &mut dyn ZxGfxPrinter {p})
    }
    fn gfx_printer_ref(&self) -> Option<&dyn ZxGfxPrinter> {
        self.spooler_ref().map(|p| -> &dyn ZxGfxPrinter {p})
    }
    // fn gfx_printer_mut(&mut self) -> Option<&mut dyn ZxGfxPrinter> { None }
    // fn gfx_printer_ref(&self) -> Option<&dyn ZxGfxPrinter> { None }

    fn dynbus_devices_mut(&mut self) -> Option<&mut DynamicBusDevice> { None }
    fn dynbus_devices_ref(&self) -> Option<&DynamicBusDevice> { None }

    fn mouse_mut(&mut self) -> Option<&mut KempstonMouseDevice> { None }

    fn joystick_device_mut(&mut self) -> &mut Option<MultiJoystickBusDevice>;
    fn joystick_device_ref(&self) -> &Option<MultiJoystickBusDevice>;
    fn joystick_mut(&mut self) -> Option<&mut JoystickSelect> {
        self.joystick_device_mut().as_deref_mut()
    }
    fn joystick_ref(&self) -> Option<&JoystickSelect> {
        self.joystick_device_ref().as_deref()
    }
    fn keypad_mut(&mut self) -> Option<&mut SerialKeypad<V>> { None }
    // fn static_ay3_8912_mut(&mut self) -> Option<&mut Ay3_8912<J>> { None }
    fn static_ay3_8912_ref(&self) -> Option<&Ay3_8912> { None }
    fn microdrives_ref(&self) -> Option<&ZXMicrodrives<V>> { None }
    fn microdrives_mut(&mut self) -> Option<&mut ZXMicrodrives<V>> { None }
    // fn serial_ref(&self) -> Option<&Rs232<V>> { None }
    // fn serial_mut(&self) -> Option<&mut Rs232<V>> { None }
}

pub trait DynamicDeviceAccess {
    // dynamic devices
    fn remove_all_devices(&mut self) { unimplemented!() }
    fn add_mouse(&mut self) { unimplemented!() }
    fn add_ay_melodik(&mut self) { unimplemented!() }
    fn add_fullerbox(&mut self) { unimplemented!() }
    fn add_printer(&mut self) { unimplemented!() }
}

impl<C, U> ZXSpectrum<C, U>
    where U: Video + KeyboardInterface + MemoryAccess,
          Self: DeviceAccess<U::VideoFrame>,
          U::VideoFrame: 'static
{
    pub fn device_info(&self) -> String {
        use std::fmt::Write;
        let mut info = format!("{}kb", (self.ula.memory_ref().ram_ref().len() as u32)/1024);
        if let Some(ay) = self.static_ay3_8912_ref() {
            write!(info, " + {}", ay).unwrap();
            if let Some(spooler) = self.gfx_printer_ref() {
                info.write_str(" + Serial Printer").unwrap();
                let lines = spooler.data_lines_buffered();
                if lines != 0 || spooler.is_spooling() {
                    write!(info, "[{}]", lines).unwrap();
                }                
            }
        }
        if let Some(microdrives) = self.microdrives_ref() {
            write!(info, " + ZX Interface 1").unwrap();
        }
        if let Some(joy) = self.joystick_ref() {
            if self.joystick_index != 0 {
                write!(info, " + {} #{} Joy.", joy, self.joystick_index + 1).unwrap();
            }
            else {
                write!(info, " + {} Joystick", joy).unwrap();
            }
        }
        if let Some(devices) = self.dynbus_devices_ref() {
            let pr_index = self.bus_index.printer;
            for (i, dev) in devices.into_iter().enumerate() {
                write!(info, " + {}", dev).unwrap();
                if pr_index.filter(|&pri| pri == i).is_some() {
                    let spooler: &ImageSpooler = dev.downcast_ref::<ZXPrinterToImage<U::VideoFrame>>().unwrap();
                    let lines = spooler.data_lines_buffered();
                    if lines != 0 || spooler.is_spooling() {
                        write!(info, "[{}]", lines).unwrap();
                    }
                }
            }
        }
        info
    }

    pub fn save_printed_image(&mut self) ->  Result<(), Box<dyn std::error::Error>> {
        if let Some(spooler) = self.gfx_printer_mut() {
            if !spooler.is_spooling() {
                if !spooler.is_empty() {
                    let mut name = format!("printer_out_{}", Utc::now().format("%Y-%m-%d_%H%M%S%.f"));
                    if let Some(img) = spooler.to_image() {
                        name.push_str(".png");
                        img.save(&name)?;
                        info!("Printer output saved as: {}", name);
                        name.truncate(name.len() - 4);
                    }
                    name.push_str(".svg");
                    let mut file = std::fs::File::create(&name)?;
                    spooler.svg_dot_printed_lines(&mut file)?;
                    drop(file);
                    info!("Printer output saved as: {}", name);
                    spooler.clear();
                }
                else {
                    info!("Printer spooler is empty.");
                }
            }
            else {
                info!("Printer is busy, can't save at the moment.");
            }
        }
        else {
            info!("There is no printer.");
        }
        Ok(())
    }

    pub fn move_mouse(&mut self, dx: i32, dy: i32) {
        self.mouse_rel.0 += dx;
        self.mouse_rel.1 += dy;
    }

    /// Send mouse positions at most once every frame to prevent overflowing.
    pub fn send_mouse_move(&mut self, border: BorderSize, viewport: (u32, u32)) {
        match self.mouse_rel {
            (0, 0) => {},
            (dx, dy) => {
                if let Some(mouse) = self.mouse_mut() {
                    let (sx, sy) = <U as Video>::VideoFrame::screen_size_pixels(border);
                    let (vx, vy) = viewport;
                    let dx = (dx * 2 * sx as i32 / vx as i32) as i16;
                    let dy = (dy * 2 * sy as i32 / vy as i32) as i16;
                    // println!("{}x{}", dx, dy);
                    mouse.move_mouse((dx, dy))
                }
                self.mouse_rel = (0, 0);
            }
        }
    }

    pub fn update_mouse_button(&mut self, button: MouseButton, pressed: bool) {
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
    pub fn update_keypress(&mut self, keycode: Keycode, modifier: Modifier, pressed: bool) {
        self.ula.set_key_state(
            map_keys(self.ula.get_key_state(), keycode, modifier, pressed,
                     self.joystick_ref().is_none()));
        if let Some(keypad) = self.keypad_mut() {
            keypad.set_key_state(
                keypad_keys(keypad.get_key_state(), keycode, pressed, modifier)
            );
        }
    }

    pub fn next_joystick(&mut self) {
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
    pub fn cursor_to_joystick(&mut self, keycode: Keycode, pressed: bool) -> bool {
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

pub fn keypad_keys(mut keys: KeypadKeys, keycode: Keycode, pressed: bool, modifier: Modifier) -> KeypadKeys {
    let key_chg = match keycode {
        Keycode::KpDivide if modifier.intersects(Modifier::NUMMOD) => KeypadKeys::LPAREN,
        Keycode::KpDivide => KeypadKeys::DIVIDE,
        Keycode::KpMultiply if modifier.intersects(Modifier::NUMMOD) => KeypadKeys::RPAREN,
        Keycode::KpMultiply => KeypadKeys::MULTIPLY,
        Keycode::KpMinus => KeypadKeys::MINUS,
        Keycode::KpPlus => KeypadKeys::PLUS,
        Keycode::KpEnter => KeypadKeys::ENTER,
        Keycode::Kp1 => KeypadKeys::N1,
        Keycode::Kp2 => KeypadKeys::N2,
        Keycode::Kp3 => KeypadKeys::N3,
        Keycode::Kp4 => KeypadKeys::N4,
        Keycode::Kp5 => KeypadKeys::N5,
        Keycode::Kp6 => KeypadKeys::N6,
        Keycode::Kp7 => KeypadKeys::N7,
        Keycode::Kp8 => KeypadKeys::N8,
        Keycode::Kp9 => KeypadKeys::N9,
        Keycode::Kp0 => KeypadKeys::N0,
        Keycode::KpPeriod => KeypadKeys::PERIOD,
        Keycode::KpComma => KeypadKeys::PERIOD,
        _ => return keys
    };
    // println!("{:?}", keycode);
    if pressed {
        keys.insert(key_chg);
    }
    else {
        keys.remove(key_chg);
    }
    keys
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
        Keycode::Return => ZXk::EN,
        // Keycode::Return|Keycode::KpEnter => ZXk::EN,
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
        // Keycode::KpDivide => ZXk::SS|ZXk::V,
        // Keycode::KpMultiply => ZXk::SS|ZXk::B,
        // Keycode::KpPlus => ZXk::SS|ZXk::K,
        // Keycode::KpMinus => ZXk::SS|ZXk::J,
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
