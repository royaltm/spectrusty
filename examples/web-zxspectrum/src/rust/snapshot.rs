use std::io::Read;
use core::convert::TryFrom;

use spectrusty::z80emu::Cpu;
use spectrusty::clock::{FTs, VFrameTs};
use spectrusty::chip::{
    ReadEarMode, MemoryAccess, Ula128MemFlags, Ula3CtrlFlags, ScldCtrlFlags
};
use spectrusty::bus::{
    BoxNamedDynDevice
};
use spectrusty::formats::snapshot::*;
use spectrusty::peripherals::ay::AyRegister;
use spectrusty::memory::{ZxMemory, ZxMemoryError};
use spectrusty::video::{BorderColor, Video};
use zxspectrum_common::{
    ZxSpectrum, ZxSpectrumModel, ModelRequest, JoystickAccess, DeviceAccess, DynamicDevices,
    update_ay_state, joy_index_from_joystick_model, joystick_model_from_name,
    spectrum_model_dispatch
};

use crate::ZxSpectrumEmu;
use crate::serde::{Ay3_891xMelodik, Ay3_891xFullerBox};

impl SnapshotCreator for ZxSpectrumEmu {
    fn model(&self) -> ComputerModel {
        let model = ModelRequest::from(&self.model);
        // log::debug!("model: {}", model);
        ComputerModel::from(model)
    }

    fn extensions(&self) -> Extensions {
        Extensions::empty()
    }

    fn cpu(&self) -> CpuModel {
        CpuModel::NMOS(self.model.cpu_ref().clone())
    }

    fn current_clock(&self) -> FTs {
        let ts = self.model.current_tstate();
        // log::debug!("clock: {}", ts);
        ts
    }

    fn border_color(&self) -> BorderColor {
        let color = self.model.border_color();
        // log::debug!("border color: {:?}", color);
        color
    }

    fn issue(&self) -> ReadEarMode {
        let issue = self.spectrum_control_ref().read_ear_mode();
        // log::debug!("keyboard issue: {:?}", issue);
        issue
    }

    fn memory_ref(&self, range: MemoryRange) -> Result<&[u8], ZxMemoryError> {
        // log::debug!("mem range ref: {:?}", range);
        spectrum_model_dispatch!((&self.model)(spec) => memory_ref(range, spec.ula.memory_ref()))
    }

    fn joystick(&self) -> Option<JoystickModel> {
        let joy = self.model.current_joystick().and_then(|name| {
            let sub_joy = self.model.emulator_state_ref().sub_joy;
            joystick_model_from_name(name, sub_joy)
        });
        // log::debug!("joystick: {:?}", joy);
        joy
    }

    fn ay_state(&self, choice: Ay3_891xDevice) -> Option<(AyRegister, &[u8;16])> {
        // log::debug!("AY req: {:?}", choice);

        fn get_ay_state<C: Cpu, U: Video + 'static, F>(
                spec: &ZxSpectrum<C, U, F>,
                choice: Ay3_891xDevice
            ) -> Option<(AyRegister, &[u8;16])>
            where U: DeviceAccess
        {
            match choice {
                Ay3_891xDevice::Melodik => {
                    spec.device_ref::<Ay3_891xMelodik<VFrameTs<U::VideoFrame>>>().map(|ay_dev| {
                        (ay_dev.ay_io.selected_register(),
                         ay_dev.ay_io.registers())
                    })
                }
                Ay3_891xDevice::FullerBox => {
                    spec.device_ref::<Ay3_891xFullerBox<VFrameTs<U::VideoFrame>>>().map(|ay_dev| {
                        (ay_dev.ay_io.selected_register(),
                         ay_dev.ay_io.registers())
                    })
                }
                _ => None
            }.map(|(sel, regs)|{
                // log::debug!("AY: {:?}, {:?}", sel, regs);
                (sel, regs)
            })
        }
        match choice {
            Ay3_891xDevice::Ay128k => {
                spectrum_model_dispatch!((&self.model)(spec) => spec.ay128k_state())
            }
            Ay3_891xDevice::Melodik|Ay3_891xDevice::FullerBox => {
                spectrum_model_dispatch!((&self.model)(spec) => get_ay_state(spec, choice))
            }
            _ => None
        }
    }

    fn ula128_flags(&self) -> Ula128MemFlags {
        let flags = self.model.ula128_mem_port_value().unwrap();
        // log::debug!("ULA 128 flags: {:?}", flags);
        flags
    }

    fn ula3_flags(&self) -> Ula3CtrlFlags {
        let flags = self.model.ula3_ctrl_port_value().unwrap();
        // log::debug!("ULA 3 flags: {:?}", flags);
        flags
    }

    fn timex_flags(&self) -> ScldCtrlFlags {
        let flags = self.model.scld_ctrl_port_value().unwrap();
        // log::debug!("SCLD flags: {:?}", flags);
        flags
    }

    fn timex_memory_banks(&self) -> u8 {
        let flags = self.model.scld_mmu_port_value().unwrap();
        // log::debug!("SCLD banks: {:b}", flags);
        flags
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
        // log::debug!("{} {} {:?} {:?}", model, extensions, border, issue);
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
        // log::debug!("CPU: {:?}", cpu);
        self.model.set_cpu(cpu)
    }

    fn set_clock(&mut self, tstates: FTs) {
        // log::debug!("set clock: {:?}", tstates);
        self.model.set_frame_tstate(tstates)
    }

    fn write_port(&mut self, port: u16, data: u8) {
        // log::debug!("write port: {:x} {}", port, data);
        self.model.write_port(port, data)
    }

    fn select_joystick(&mut self, joystick: JoystickModel) {
        // log::debug!("joystick {:?}", joystick);
        self.model.select_joystick(joy_index_from_joystick_model(Some(joystick)));
    }

    fn setup_ay(&mut self, choice: Ay3_891xDevice, reg_selected: AyRegister, reg_values: &[u8;16]) {
        // log::debug!("AY: {:?}, {:?}, {:?}", choice, reg_selected, reg_values);

        fn create_device<C: Cpu, U: Video + 'static, F>(
                spec: &mut ZxSpectrum<C, U, F>,
                choice: Ay3_891xDevice,
                reg_selected: AyRegister,
                reg_values: &[u8;16]
            )
            where ZxSpectrum<C, U, F>: DynamicDevices<U::VideoFrame>
        {
            let device: BoxNamedDynDevice<VFrameTs<U::VideoFrame>> = match choice {
                Ay3_891xDevice::Melodik => {
                    let mut ay_dev = Ay3_891xMelodik::<VFrameTs<U::VideoFrame>>::default();
                    update_ay_state(&mut ay_dev.ay_sound, &mut ay_dev.ay_io, reg_selected, reg_values);
                    ay_dev.into()
                },
                Ay3_891xDevice::FullerBox => {
                    let mut ay_dev = Ay3_891xFullerBox::<VFrameTs<U::VideoFrame>>::default();
                    update_ay_state(&mut ay_dev.ay_sound, &mut ay_dev.ay_io, reg_selected, reg_values);
                    ay_dev.into()
                }
                _ => return
            };
            spec.attach_device(device);
        }

        match choice {
            Ay3_891xDevice::Ay128k => {
                spectrum_model_dispatch!((&mut self.model)(spec) => {
                    spec.update_ay128k_state(reg_selected, reg_values);
                });
                return
            }
            Ay3_891xDevice::Melodik|Ay3_891xDevice::FullerBox => {
                spectrum_model_dispatch!((&mut self.model)(spec) => {
                    create_device(spec, choice, reg_selected, reg_values);
                });
            }
            _ => return
        }
    }

    fn read_into_memory<R: Read>(&mut self, range: MemoryRange, reader: R) -> Result<(), ZxMemoryError> {
        // log::debug!("mem range: {:?}", range);
        spectrum_model_dispatch!((&mut self.model)(spec) => {
            read_into_memory(range, reader, spec.ula.memory_mut())
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

fn memory_ref<M: ZxMemory>(range: MemoryRange, memory: &M) -> Result<&[u8], ZxMemoryError> {
    match range {
        MemoryRange::Rom(range) => memory.rom_ref().get(range),
        MemoryRange::Ram(range) => memory.ram_ref().get(range),
        _ => Err(ZxMemoryError::UnsupportedExRomPaging)?
    }.ok_or_else(|| ZxMemoryError::UnsupportedAddressRange)
}

fn read_into_memory<R: Read, M: ZxMemory>(
        range: MemoryRange,
        mut reader: R,
        memory: &mut M
    ) -> Result<(), ZxMemoryError>
{
    let mem_slice = match range {
        MemoryRange::Rom(range) => memory.rom_mut().get_mut(range),
        MemoryRange::Ram(range) => memory.ram_mut().get_mut(range),
        _ => Err(ZxMemoryError::UnsupportedExRomPaging)?
    }.ok_or_else(|| ZxMemoryError::UnsupportedAddressRange)?;

    reader.read_exact(mem_slice).map_err(ZxMemoryError::Io)
}
