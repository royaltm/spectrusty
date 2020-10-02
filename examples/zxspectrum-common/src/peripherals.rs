/*
    zxspectrum-common: High-level ZX Spectrum emulator library example.
    Copyright (C) 2020  Rafal Michalski

    For the full copyright notice, see the lib.rs file.
*/
use core::fmt;
use std::io;
use spectrusty::bus::BoxNamedDynDevice;
use spectrusty::clock::TimestampOps;
use spectrusty::z80emu::Cpu;
use spectrusty::memory::{MemoryExtension, ZxMemory, ZxMemoryError};
use spectrusty::bus::{
    NullDevice, BusDevice,
    ay::{
        AyRegister, AyIoPort, Ay3_891xAudio, Ay3_8910Io
    },
    joystick::{MultiJoystickBusDevice, JoystickSelect, JoystickInterface},
    mouse::{KempstonMouse, MouseInterface}
};
use spectrusty::formats::snapshot::{MemoryRange, JoystickModel, Ay3_891xDevice};

use super::spectrum::ZxSpectrum;
use super::devices::{BusTs, DeviceAccess, DynamicDevices, PluggableJoystick};
use super::models::*;

/// A helper method returning a joystick index for [JoystickSelect] enum from optional [JoystickModel].
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

/// On success returns a [JoystickModel] based on joystick name and `sub_joy` index.
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

/// A helper method for updating the AY-3-8910 PSG registers from `reg_selected` and `reg_values`.
pub fn update_ay_state<T, A, B>(
        ay_sound: &mut Ay3_891xAudio,
        ay_io: &mut Ay3_8910Io<T, A, B>,
        reg_selected: AyRegister,
        reg_values: &[u8]
    )
    where A: AyIoPort<Timestamp=T>,
          B: AyIoPort<Timestamp=T>
{
    ay_io.select_port_write(reg_selected.into());
    for (reg, val) in AyRegister::enumerate().zip(reg_values) {
        ay_io.set(reg, *val);
    }
    for (reg, val) in ay_io.iter_sound_gen_regs() {
        ay_sound.update_register(reg, val);
    }
}

/// Returns the current state of AY-3-891x PSG registers from one of
/// the dynamically added devices, indicated by [Ay3_891xDevice].
pub fn get_ay_state_from_dyn_device<C: Cpu, U, F>(
        spec: &ZxSpectrum<C, U, F>,
        choice: Ay3_891xDevice
    ) -> Option<(AyRegister, &[u8;16])>
    where U: DeviceAccess,
          BusTs<U>: fmt::Debug + Copy + 'static
{
    match choice {
        Ay3_891xDevice::Melodik => {
            spec.device_ref::<super::Ay3_891xMelodik<BusTs<U>>>().map(|ay_dev| {
                (ay_dev.ay_io.selected_register(),
                 ay_dev.ay_io.registers())
            })
        }
        Ay3_891xDevice::FullerBox => {
            spec.device_ref::<super::Ay3_891xFullerBox<BusTs<U>>>().map(|ay_dev| {
                (ay_dev.ay_io.selected_register(),
                 ay_dev.ay_io.registers())
            })
        }
        _ => None
    }
}

/// Creates one of the indicated by `choice` AY-3-891x PSG dynamic devices, and
/// attaches it to the [ZxSpectrum] referenced by `spec`.
///
/// Initializes the created PSG registers to values from `reg_values` and the 
/// currently selected register to `reg_selected`.
///
/// Returns `true` if a device has been attached. Otherwise returns `false`.
pub fn create_ay_dyn_device<C: Cpu, U, F>(
        spec: &mut ZxSpectrum<C, U, F>,
        choice: Ay3_891xDevice,
        reg_selected: AyRegister,
        reg_values: &[u8;16]
    ) -> bool
    where U: DeviceAccess,
          BusTs<U>: fmt::Debug + Copy + Default + 'static
{
    let device: BoxNamedDynDevice<BusTs<U>> = match choice {
        Ay3_891xDevice::Melodik => {
            let mut ay_dev = super::Ay3_891xMelodik::<BusTs<U>>::default();
            update_ay_state(&mut ay_dev.ay_sound, &mut ay_dev.ay_io, reg_selected, reg_values);
            ay_dev.into()
        },
        Ay3_891xDevice::FullerBox => {
            let mut ay_dev = super::Ay3_891xFullerBox::<BusTs<U>>::default();
            update_ay_state(&mut ay_dev.ay_sound, &mut ay_dev.ay_io, reg_selected, reg_values);
            ay_dev.into()
        }
        _ => return false
    };

    spec.attach_device(device)
}

/// Returns a reference to the slice of bytes from the `memory` fragment indicated by `range`.
pub fn memory_range_ref<M: ZxMemory>(memory: &M, range: MemoryRange) -> Result<&[u8], ZxMemoryError> {
    match range {
        MemoryRange::Rom(range) => memory.rom_ref().get(range),
        MemoryRange::Ram(range) => memory.ram_ref().get(range),
        _ => return Err(ZxMemoryError::UnsupportedExRomPaging)
    }.ok_or_else(|| ZxMemoryError::UnsupportedAddressRange)
}

/// Convenient function for populating fragments of `memory` indicated by `range`
/// with bytes from `reader`.
pub fn read_into_memory_range<R: io::Read, M: ZxMemory>(
        memory: &mut M,
        range: MemoryRange,
        mut reader: R
    ) -> Result<(), ZxMemoryError>
{
    let mem_slice = match range {
        MemoryRange::Rom(range) => memory.rom_mut().get_mut(range),
        MemoryRange::Ram(range) => memory.ram_mut().get_mut(range),
        _ => return Err(ZxMemoryError::UnsupportedExRomPaging)
    }.ok_or_else(|| ZxMemoryError::UnsupportedAddressRange)?;

    reader.read_exact(mem_slice).map_err(ZxMemoryError::Io)
}

/// A trait for easy accessing and controling [JoystickInterface].
pub trait JoystickAccess {
    fn joystick_interface(&mut self) -> Option<&mut (dyn JoystickInterface + 'static)> {
        None
    }
    /// Selects joystick by specifying its index.
    fn select_joystick(&mut self, _joy: usize) {}
    /// Changes joystick implementation to the next available.
    fn select_next_joystick(&mut self) {}
    /// Returns the name of the joystick if present.
    fn current_joystick(&self) -> Option<&str> { None }
}

/// A trait for easy accessing and controling [MouseInterface].
pub trait MouseAccess {
    fn mouse_interface(&mut self) -> Option<&mut (dyn MouseInterface + 'static)> {
        None
    }
}

impl<C: Cpu, U, F, D> JoystickAccess for ZxSpectrum<C, U, F>
    where D: BusDevice<Timestamp=BusTs<U>> + 'static,
          U: DeviceAccess<JoystickBusDevice = PluggableJoystick<D>>
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
          U: DeviceAccess,
          BusTs<U>: 'static,
{
    fn mouse_interface(&mut self) -> Option<&mut (dyn MouseInterface + 'static)> {
        self.device_mut::<KempstonMouse<NullDevice<BusTs<U>>>>()
            .map(|m| -> &mut (dyn MouseInterface) { &mut **m })
    }
}

impl<C: Cpu, U, F> ZxSpectrum<C, U, F>
    where U: DeviceAccess,
          BusTs<U>: TimestampOps
{
    /// Returns a current view of AY-3-8910 registers if a PSG is attached.
    pub fn ay128k_state(&self) -> Option<(AyRegister, &[u8;16])> {
        self.ula.ay128_ref().map(|(_, ay_io)| (ay_io.selected_register(), ay_io.registers()))
    }
    /// Updates AY-3-8910 registers' state if a PSG is attached.
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
