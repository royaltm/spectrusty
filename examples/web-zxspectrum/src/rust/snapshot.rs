/*
    web-zxspectrum: ZX Spectrum emulator example as a Web application.
    Copyright (C) 2020-2022  Rafal Michalski

    For the full copyright notice, see the lib.rs file.
*/
use std::io::Read;
use core::convert::TryFrom;

use spectrusty::clock::FTs;
use spectrusty::chip::{
    ReadEarMode, MemoryAccess, UlaControl,
    Ula128MemFlags, Ula3CtrlFlags, ScldCtrlFlags
};
use spectrusty::formats::snapshot::*;
use spectrusty::peripherals::ay::AyRegister;
use spectrusty::memory::ZxMemoryError;
use spectrusty::video::BorderColor;
use zxspectrum_common::{
    ZxSpectrumModel, ModelRequest, JoystickAccess,
    create_ay_dyn_device, get_ay_state_from_dyn_device,
    joy_index_from_joystick_model, joystick_model_from_name,
    memory_range_ref, read_into_memory_range,
    spectrum_model_dispatch,
};

use crate::ZxSpectrumEmu;

impl SnapshotCreator for ZxSpectrumEmu {
    fn model(&self) -> ComputerModel {
        let model = ModelRequest::from(&self.model);
        ComputerModel::from(model)
    }

    fn extensions(&self) -> Extensions {
        Extensions::empty()
    }

    fn cpu(&self) -> CpuModel {
        CpuModel::NMOS(self.model.cpu_ref().clone())
    }

    fn current_clock(&self) -> FTs {
        self.model.current_tstate()
    }

    fn border_color(&self) -> BorderColor {
        self.model.border_color()
    }

    fn issue(&self) -> ReadEarMode {
        self.spectrum_control_ref().read_ear_mode()
    }

    fn memory_ref(&self, range: MemoryRange) -> Result<&[u8], ZxMemoryError> {
        spectrum_model_dispatch!((&self.model)(spec) => {
            memory_range_ref(spec.ula.memory_ref(), range)
        })
    }

    fn joystick(&self) -> Option<JoystickModel> {
        self.model.current_joystick().and_then(|name| {
            let sub_joy = self.model.emulator_state_ref().sub_joy;
            joystick_model_from_name(name, sub_joy)
        })
    }

    fn ay_state(&self, choice: Ay3_891xDevice) -> Option<(AyRegister, &[u8;16])> {
        match choice {
            Ay3_891xDevice::Ay128k => {
                spectrum_model_dispatch!((&self.model)(spec) => spec.ay128k_state())
            }
            Ay3_891xDevice::Melodik|Ay3_891xDevice::FullerBox => {
                spectrum_model_dispatch!((&self.model)(spec) => get_ay_state_from_dyn_device(spec, choice))
            }
            _ => None
        }
    }

    fn ula128_flags(&self) -> Ula128MemFlags {
        spectrum_model_dispatch!((&self.model)(spec) => spec.ula.ula128_mem_port_value().unwrap())
    }

    fn ula3_flags(&self) -> Ula3CtrlFlags {
        spectrum_model_dispatch!((&self.model)(spec) => spec.ula.ula3_ctrl_port_value().unwrap())
    }

    fn timex_flags(&self) -> ScldCtrlFlags {
        spectrum_model_dispatch!((&self.model)(spec) => spec.ula.scld_ctrl_port_value().unwrap())
    }

    fn timex_memory_banks(&self) -> u8 {
        spectrum_model_dispatch!((&self.model)(spec) => spec.ula.scld_mmu_port_value().unwrap())
    }
}

impl SnapshotLoader for ZxSpectrumEmu {
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
        if extensions != Extensions::NONE {
            alert!("The extensions: {} are not supported\n\
                    this may result in an undefined behaviour of the emulated computer",
                    extensions);
        }
        self.model = ZxSpectrumModel::new(model_req);
        let spectrum = self.spectrum_control_mut();
        spectrum.set_border_color(border);
        spectrum.set_read_ear_mode(issue);
        Ok(())
    }

    fn assign_cpu(&mut self, cpu: CpuModel) {
        self.model.set_cpu(cpu)
    }

    fn set_clock(&mut self, tstates: FTs) {
        self.model.set_frame_tstate(tstates)
    }

    fn write_port(&mut self, port: u16, data: u8) {
        self.model.write_port(port, data)
    }

    fn select_joystick(&mut self, joystick: JoystickModel) {
        self.model.select_joystick(joy_index_from_joystick_model(Some(joystick)));
    }

    fn setup_ay(&mut self, choice: Ay3_891xDevice, reg_selected: AyRegister, reg_values: &[u8;16]) {
        match choice {
            Ay3_891xDevice::Ay128k => {
                spectrum_model_dispatch!((&mut self.model)(spec) => {
                    spec.update_ay128k_state(reg_selected, reg_values);
                });
            }
            Ay3_891xDevice::Melodik|Ay3_891xDevice::FullerBox => {
                spectrum_model_dispatch!((&mut self.model)(spec) => {
                    create_ay_dyn_device(spec, choice, reg_selected, reg_values);
                });
            }
            _ => {}
        }
    }

    fn read_into_memory<R: Read>(&mut self, range: MemoryRange, reader: R) -> Result<(), ZxMemoryError> {
        spectrum_model_dispatch!((&mut self.model)(spec) => {
            read_into_memory_range(spec.ula.memory_mut(), range, reader)
        })
    }

    fn interface1_rom_paged_in(&mut self) {
        crate::alert("The snapshot requested that IF1 ROM is paged in\n\
                      this will result in an undefined behaviour of the emulated computer");
    }

    fn plus_d_rom_paged_in(&mut self) {
        crate::alert("The snapshot requested that MGT +D ROM is paged in\n\
                      this will result in an undefined behaviour of the emulated computer");
    }

    fn tr_dos_rom_paged_in(&mut self) {
        crate::alert("The snapshot requested that TR-DOS ROM is paged in\n\
                      this will result in an undefined behaviour of the emulated computer");
    }
}
