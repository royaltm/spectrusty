// use core::fmt::Display;
mod nonblocking;

use chrono::prelude::*;
use sdl2::keyboard::{Mod as Modifier, Keycode};
use sdl2::mouse::MouseButton;
use spectrusty::bus::{
    DynamicBusDevice, OptionalBusDevice, NullDevice,
    ay::serial128::Ay3_8912KeypadRs232,
    joystick::{JoystickSelect, MultiJoystickBusDevice},
    zxinterface1::*,
    zxprinter::*
};
use spectrusty::clock::VideoTs;
use spectrusty::chip::{MemoryAccess, ula128::Ula128VidFrame};
use spectrusty::memory::ZxMemory;
use spectrusty::peripherals::{
    KeyboardInterface,
    mouse::{MouseInterface, MouseButtons, kempston::KempstonMouseDevice},
    serial::SerialKeypad
};
use spectrusty::video::{Video, BorderSize, VideoFrame};
use spectrusty_utils::keyboard::sdl2::{
    update_joystick_from_key_event,
    update_keymap_with_modifier,
    update_keypad_keys_with_modifier
};

#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

use crate::spectrum::ZXSpectrum;
use super::printer::{ZxGfxPrinter, ImageSpooler};

pub use spectrusty::bus::zxinterface1::{ZxNetUdpSyncSocket, MicroCartridge, ZXMicrodrives};
pub use nonblocking::*;

pub type ZxInterface1<V,D=NullDevice<VideoTs>> = ZxInterface1BusDevice<V,
                                        NonBlockingStdinReader,
                                        FilterGfxStdoutWriter,
                                        ZxNetUdpSyncSocket,
                                        D>;
pub type ZXPrinterToImage<V> = ZxPrinter<V, ImageSpooler>;
pub type OptJoystickBusDevice<D=NullDevice<VideoTs>> = OptionalBusDevice<MultiJoystickBusDevice, D>;
pub type Ay3_8912 = Ay3_8912KeypadRs232<Ula128VidFrame,
                                        OptJoystickBusDevice,
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
    fn network_ref(&self) -> Option<&ZxNetUdpSyncSocket> { None }
    fn network_mut(&mut self) -> Option<&mut ZxNetUdpSyncSocket> { None }
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
        if let Some(microdrives) = self.microdrives_ref() {
            write!(info, " + ZX Interface 1").unwrap();
            if let Some((index, md)) = microdrives.cartridge_in_use() {
                write!(info, "[M{}*{:.2}]", index + 1, md.head_at()).unwrap();
            }
        }
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
    pub fn handle_keypress_event(&mut self, keycode: Keycode, mut modifier: Modifier, pressed: bool) {
        if self.joystick_ref().is_some() {
            modifier.remove(Modifier::RCTRLMOD);
            let joystick_index = self.joystick_index;
            if update_joystick_from_key_event(keycode, pressed, Keycode::RCtrl,
                || self.joystick_mut()
                       .and_then(|joy| joy.joystick_interface(joystick_index))
            ) {
                return
            }
        }

        self.ula.set_key_state(
            update_keymap_with_modifier(self.ula.get_key_state(), keycode, pressed, modifier));

        if let Some(keypad) = self.keypad_mut() {
            keypad.set_key_state(
                update_keypad_keys_with_modifier(keypad.get_key_state(), keycode, pressed, modifier)
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
}
