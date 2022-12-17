/*
    sdl2-zxspectrum: ZX Spectrum emulator example as a SDL2 application.
    Copyright (C) 2020-2022  Rafal Michalski

    For the full copyright notice, see the main.rs file.
*/
use std::io::Read;
use core::convert::TryFrom;

#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

use spectrusty::z80emu::Cpu;
use spectrusty::clock::{FTs, TimestampOps};
use spectrusty::chip::{
    UlaCommon,
    ReadEarMode, EarIn,
    MemoryAccess, Ula128MemFlags, Ula3CtrlFlags, ScldCtrlFlags,
};
use spectrusty::formats::snapshot::*;
use spectrusty::peripherals::ay::AyRegister;
use spectrusty::memory::ZxMemoryError;
use spectrusty::video::{Video, BorderColor};
use zxspectrum_common::{
    BusTs, ModelRequest, JoystickAccess, DeviceAccess,
    create_ay_dyn_device, get_ay_state_from_dyn_device,
    joy_index_from_joystick_model, joystick_model_from_name,
    memory_range_ref, read_into_memory_range,
    spectrum_model_dispatch,
};

use super::{ZxSpectrumModel, ZxSpectrumEmu, ZxSpectrum, ZxInterface1MemExt};

#[derive(Default)]
pub struct ZxSpectrumModelSnap {
    pub model: Option<ZxSpectrumModel>,
    pub has_if1: bool,
    pub if1_rom_paged_in: bool
}


impl<'a, C, U: 'static> SnapshotCreator for ZxSpectrumEmu<'a, C, U>
    where C: Cpu + Into<CpuModel>,
          U: UlaCommon + DeviceAccess + MemoryAccess<MemoryExt=ZxInterface1MemExt>,
          BusTs<U>: TimestampOps,
          ZxSpectrum<C, U>: JoystickAccess
{
    fn model(&self) -> ComputerModel {
        self.model.into()
    }

    fn extensions(&self) -> Extensions {
        let memext = self.spectrum.ula.memory_ext_ref();
        if memext.exrom().is_empty() {
            Extensions::empty()
        }
        else {
            Extensions::IF1
        }
    }

    fn cpu(&self) -> CpuModel {
        self.spectrum.cpu.clone().into()
    }

    fn current_clock(&self) -> FTs {
        self.spectrum.ula.current_tstate()
    }

    fn border_color(&self) -> BorderColor {
        self.spectrum.ula.border_color()
    }

    fn issue(&self) -> ReadEarMode {
        self.spectrum.ula.read_ear_mode()
    }

    fn memory_ref(&self, range: MemoryRange) -> Result<&[u8], ZxMemoryError> {
        memory_range_ref(self.spectrum.ula.memory_ref(), range)
    }

    fn joystick(&self) -> Option<JoystickModel> {
        self.spectrum.current_joystick().and_then(|name| {
            let sub_joy = self.spectrum.state.sub_joy;
            joystick_model_from_name(name, sub_joy)
        })
    }

    fn ay_state(&self, choice: Ay3_891xDevice) -> Option<(AyRegister, &[u8;16])> {
        match choice {
            Ay3_891xDevice::Ay128k => self.spectrum.ay128k_state(),
            Ay3_891xDevice::Melodik|Ay3_891xDevice::FullerBox => {
                get_ay_state_from_dyn_device(&self.spectrum, choice)
            }
            _ => None
        }
    }

    fn ula128_flags(&self) -> Ula128MemFlags {
        self.spectrum.ula.ula128_mem_port_value().unwrap()
    }

    fn ula3_flags(&self) -> Ula3CtrlFlags {
        self.spectrum.ula.ula3_ctrl_port_value().unwrap()
    }

    fn timex_flags(&self) -> ScldCtrlFlags {
        self.spectrum.ula.scld_ctrl_port_value().unwrap()
    }

    fn timex_memory_banks(&self) -> u8 {
        self.spectrum.ula.scld_mmu_port_value().unwrap()
    }

    fn is_interface1_rom_paged_in(&self) -> bool {
        let memory = self.spectrum.ula.memory_ref();
        self.spectrum.ula.memory_ext_ref().is_mapped_exrom(memory)
    }
}

impl SnapshotLoader for ZxSpectrumModelSnap {
    type Error = String;

    fn select_model(
            &mut self,
            model: ComputerModel,
            extensions: Extensions,
            border: BorderColor,
            issue: ReadEarMode
        ) -> Result<(), Self::Error>
    {
        let model_req = ModelRequest::try_from(model)?;
        // in this example ZX Interface 1 extension must be specified in command line arguments
        // because a path is required to a file containing IF 1 ROM data.
        if extensions.intersects(Extensions::IF1) {
            // signal IF 1 should be enabled
            self.has_if1 = true;
        }
        else if extensions != Extensions::NONE {
            warn!("The extensions: {} are not supported\n\
                    this may result in an undefined behaviour of the emulated computer",
                    extensions);
        }
        let mut model = ZxSpectrumModel::new(model_req);
        spectrum_model_dispatch!((&mut model)(spec) => {
            spec.ula.set_border_color(border);
            spec.ula.set_read_ear_mode(issue);
        });
        self.model = Some(model);
        Ok(())
    }

    fn assign_cpu(&mut self, cpu: CpuModel) {
        self.model.as_mut().unwrap().set_cpu_any(cpu)
    }

    fn set_clock(&mut self, tstates: FTs) {
        self.model.as_mut().unwrap().set_frame_tstate(tstates)
    }

    fn write_port(&mut self, port: u16, data: u8) {
        self.model.as_mut().unwrap().write_port(port, data)
    }

    fn select_joystick(&mut self, joystick: JoystickModel) {
        self.model.as_mut().unwrap()
            .select_joystick(joy_index_from_joystick_model(Some(joystick)));
    }

    fn setup_ay(&mut self, choice: Ay3_891xDevice, reg_selected: AyRegister, reg_values: &[u8;16]) {
        match choice {
            Ay3_891xDevice::Ay128k => {
                spectrum_model_dispatch!((self.model.as_mut().unwrap())(spec) => {
                    spec.update_ay128k_state(reg_selected, reg_values);
                });
            }
            Ay3_891xDevice::Melodik|Ay3_891xDevice::FullerBox => {
                spectrum_model_dispatch!((self.model.as_mut().unwrap())(spec) => {
                    create_ay_dyn_device(spec, choice, reg_selected, reg_values);
                });
            }
            _ => {}
        }
    }

    fn read_into_memory<R: Read>(&mut self, range: MemoryRange, reader: R) -> Result<(), ZxMemoryError> {
        spectrum_model_dispatch!((self.model.as_mut().unwrap())(spec) => {
            read_into_memory_range(spec.ula.memory_mut(), range, reader)
        })
    }

    fn interface1_rom_paged_in(&mut self) {
        // signal IF 1 ROM should be paged in
        self.if1_rom_paged_in = true;
    }

    fn plus_d_rom_paged_in(&mut self) {
        warn!("The snapshot requested that MGT +D ROM is paged in\n\
                      this will result in an undefined behaviour of the emulated computer");
    }

    fn tr_dos_rom_paged_in(&mut self) {
        warn!("The snapshot requested that TR-DOS ROM is paged in\n\
                      this will result in an undefined behaviour of the emulated computer");
    }
}
