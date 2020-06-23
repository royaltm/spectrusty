use core::fmt;
use std::io;
use spectrusty::z80emu::Cpu;
use spectrusty::clock::VFrameTs;
use spectrusty::memory::MemoryExtension;
use spectrusty::bus::{
    OptionalBusDevice, VFNullDevice,
    ay::{
        AyRegister, AyIoPort, Ay3_891xAudio, Ay3_8910Io
    },
    joystick::{MultiJoystickBusDevice, JoystickSelect, JoystickInterface},
    mouse::{KempstonMouse, MouseInterface}
};
use spectrusty::formats::snapshot::JoystickModel;
use spectrusty::video::Video;

use super::spectrum::ZxSpectrum;
use super::devices::{DeviceAccess, DynamicDevices};
use super::models::*;

pub fn joy_index_from_joystick_model(joy: Option<JoystickModel>) -> usize {
    use JoystickModel::*;
    match joy {
        Some(Kempston)  => 0,
        Some(Sinclair1) => 2,
        Some(Sinclair2) => 3,
        Some(Cursor)    => 4,
        Some(Fuller)    => 1,
                      _ => usize::max_value()
    }
}

pub fn joystick_model_from_name(name: &str, sub_joy: usize) -> Option<JoystickModel> {
    Some(match name {
        "Kempston" => JoystickModel::Kempston,
        "Sinclair" if sub_joy == 1 => JoystickModel::Sinclair2,
        "Sinclair" => JoystickModel::Sinclair1,
        "Cursor" => JoystickModel::Cursor,
        "Fuller" => JoystickModel::Fuller,
        _ => return None
    })
}

pub fn update_ay_state<V, A, B>(
        ay_sound: &mut Ay3_891xAudio,
        ay_io: &mut Ay3_8910Io<VFrameTs<V>, A, B>,
        reg_selected: AyRegister,
        reg_values: &[u8]
    )
    where A: AyIoPort<Timestamp=VFrameTs<V>>,
          B: AyIoPort<Timestamp=VFrameTs<V>>
{
    ay_io.select_port_write(reg_selected.into());
    for (reg, val) in AyRegister::enumerate().zip(reg_values) {
        ay_io.set(reg, *val);
    }
    for (reg, val) in ay_io.iter_sound_gen_regs() {
        ay_sound.update_register(reg, val);
    }
}

pub trait JoystickAccess {
    // Joystick interface access
    fn joystick_interface(&mut self) -> Option<&mut (dyn JoystickInterface + 'static)> {
        None
    }
    // Does nothing by default.
    fn select_joystick(&mut self, _joy: usize) {}
    // Returns `subjoy` by default.
    fn select_next_joystick(&mut self) {}
    // Returns joystick name
    fn current_joystick(&self) -> Option<&str> { None }
}

pub trait MouseAccess {
    // Mouse interface access
    fn mouse_interface(&mut self) -> Option<&mut (dyn MouseInterface + 'static)> {
        None
    }
}

impl<C: Cpu, U, F, D: 'static> JoystickAccess for ZxSpectrum<C, U, F>
    where U: DeviceAccess<JoystickBusDevice = OptionalBusDevice<
                                                MultiJoystickBusDevice<VFNullDevice<<U as Video>::VideoFrame>>,
                                                D>>
{
    fn joystick_interface(&mut self) -> Option<&mut (dyn JoystickInterface + 'static)> {
        let sub_joy = self.state.sub_joy;
        self.ula.joystick_bus_device_mut().and_then(|joy_bus_dev| {
            joy_bus_dev.as_deref_mut().and_then(|j| j.joystick_interface(sub_joy))
        })
    }

    fn select_joystick(&mut self, joy_index: usize) {
        if let Some(joy_bus_dev) = self.ula.joystick_bus_device_mut() {
            let (joy_dev, index) = JoystickSelect::new_with_index(joy_index)
                .map(|(joy_sel, index)| 
                    (Some(MultiJoystickBusDevice::new_with(joy_sel)), index)
                )
                .unwrap_or((None, 0));
            **joy_bus_dev = joy_dev;
            self.state.sub_joy = index;
        }
    }

    fn select_next_joystick(&mut self) {
        if let Some(dev) = self.ula.joystick_bus_device_mut() {
            self.state.sub_joy = if let Some(joy) = dev.as_deref_mut() {
                if joy.is_last() {
                    **dev = None;
                    0
                }
                else {
                    joy.select_next_joystick(self.state.sub_joy)
                }
            }
            else {
                **dev = Some(Default::default());
                0
            };
        }
    }

    fn current_joystick(&self) -> Option<&str> {
        self.ula.joystick_bus_device_ref()
                .and_then(|jbd| jbd.as_deref().map(Into::into))
    }
}

impl<C, U, F> MouseAccess for ZxSpectrum<C, U, F>
    where C: Cpu,
          U: Video,
          U::VideoFrame: 'static,
          Self: DynamicDevices<U::VideoFrame>
{
    fn mouse_interface(&mut self) -> Option<&mut (dyn MouseInterface + 'static)> {
        self.device_mut::<KempstonMouse<VFNullDevice<U::VideoFrame>>>()
            .map(|m| -> &mut (dyn MouseInterface) { &mut **m })
    }
}

impl<C: Cpu, U: DeviceAccess, F> ZxSpectrum<C, U, F> {
    pub fn ay128k_state(&self) -> Option<(AyRegister, &[u8;16])> {
        self.ula.ay128_ref().map(|(_, ay_io)| (ay_io.selected_register(), ay_io.registers()))
    }

    pub fn update_ay128k_state(&mut self, reg_selected: AyRegister, reg_values: &[u8]) -> bool {
        self.ula.ay128_mut()
        .map(|(ay_sound, ay_io)|
            update_ay_state(ay_sound, ay_io, reg_selected, reg_values)
        ).is_some()
    }
}

impl<C: Cpu, S: 'static, X, F, R, W> JoystickAccess for ZxSpectrumModel<C, S, X, F, R, W>
    where X: MemoryExtension,
          R: io::Read + fmt::Debug,
          W: io::Write + fmt::Debug,
{
    fn joystick_interface(&mut self) -> Option<&mut (dyn JoystickInterface + 'static)> {
        spectrum_model_dispatch!(self(spec) => spec.joystick_interface())
    }

    fn select_joystick(&mut self, joy: usize) {
        spectrum_model_dispatch!(self(spec) => spec.select_joystick(joy))
    }

    fn select_next_joystick(&mut self) {
        spectrum_model_dispatch!(self(spec) => spec.select_next_joystick())
    }

    fn current_joystick(&self) -> Option<&str> {
        spectrum_model_dispatch!(self(spec) => spec.current_joystick())
    }
}
