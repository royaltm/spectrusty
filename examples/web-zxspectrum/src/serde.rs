use std::io::Read;
use core::marker::PhantomData;
use core::convert::TryFrom;
use core::str::FromStr;
use core::fmt;

use serde::{
    Serialize, Serializer, Deserialize, Deserializer,
    de::{self, Visitor, SeqAccess},
    ser
};
use spectrusty::z80emu::Cpu;
use spectrusty::clock::{FTs, VFrameTs};
use spectrusty::chip::{
    ReadEarMode, MemoryAccess, Ula128MemFlags, Ula3CtrlFlags, ScldCtrlFlags
};
use spectrusty::bus::{
    NullDevice, NamedBusDevice, SerializeDynDevice, DeserializeDynDevice,
    NamedDynDevice, BoxNamedDynDevice,
    ay
};
use spectrusty::formats::snapshot::*;
use spectrusty::peripherals::ay::AyRegister;
use spectrusty::memory::{ZxMemory, ZxMemoryError};
use spectrusty::video::{BorderColor, Video, VideoFrame};
use zxspectrum_common::{
    ZxSpectrum, ZxSpectrumModel, ModelRequest, JoystickAccess, DeviceAccess, DynamicDevices,
    update_ay_state, joy_index_from_joystick_model, joystick_model_from_name,
    spectrum_model_dispatch
};

use crate::{ZxSpectrumEmu, ZxSpectrumEmuModel};

type Ay3_891xMelodik<T> = ay::Ay3_891xMelodik<NullDevice<T>>;
type Ay3_891xFullerBox<T> = ay::Ay3_891xFullerBox<NullDevice<T>>;

pub struct SerdeDynDevice;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum DeviceType {
    Ay3_891xMelodik,
    Ay3_891xFullerBox
}

impl FromStr for DeviceType {
    type Err = &'static str;
    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "Melodik"    => Ok(DeviceType::Ay3_891xMelodik),
            "Fuller Box" => Ok(DeviceType::Ay3_891xFullerBox),
            _ => Err("Unrecognized device name")
        }
    }
}

macro_rules! device_type_dispatch {
    (($dt:expr) => ($expr:expr)($fn:ident::@device<$ts:ty>)$($tt:tt)*) => {
        match $dt {
            DeviceType::Ay3_891xMelodik => { $expr.$fn::<Ay3_891xMelodik<$ts>>()$($tt)* }
            DeviceType::Ay3_891xFullerBox => { $expr.$fn::<Ay3_891xFullerBox<$ts>>()$($tt)* }
        }
    };
}

impl DeviceType {
    pub fn create_device<V: 'static + VideoFrame>(self) -> BoxNamedDynDevice<VFrameTs<V>> {
        match self {
            DeviceType::Ay3_891xMelodik => Ay3_891xMelodik::default().into(),
            DeviceType::Ay3_891xFullerBox => Ay3_891xFullerBox::default().into(),
        }
    }

    pub fn attach_device_to_model(self, model: &mut ZxSpectrumEmuModel) -> bool {
        spectrum_model_dispatch!(model(spec) => spec.attach_device(self.create_device()))
    }

    pub fn detach_device<C: Cpu, U, F>(self, spec: &mut ZxSpectrum<C, U, F>)
        where U: DeviceAccess + Video + 'static
    {
        device_type_dispatch!((self) => (spec)(detach_device::@device<VFrameTs<U::VideoFrame>>);)
    }

    pub fn detach_device_from_model(self, model: &mut ZxSpectrumEmuModel) {
        spectrum_model_dispatch!(model(spec) => self.detach_device(spec))
    }

    pub fn has_device<C: Cpu, U, F>(self, spec: &ZxSpectrum<C, U, F>) -> bool
        where U: Video + 'static
    {
        device_type_dispatch!((self) => (spec.state.devices)(has_device::@device<VFrameTs<U::VideoFrame>>))
    }

    pub fn has_device_in_model(self, model: &ZxSpectrumEmuModel) -> bool {
        spectrum_model_dispatch!(model(spec) => self.has_device(spec))
    }


}

pub fn recreate_model_dynamic_devices(
        src_model: &ZxSpectrumEmuModel,
        dst_model: &mut ZxSpectrumEmuModel
    ) -> Result<(), &'static str>
{
    spectrum_model_dispatch!(src_model(src_spec) => {
        spectrum_model_dispatch!(dst_model(dst_spec) => recreate_dynamic_devices(src_spec, dst_spec))
    })
}

fn recreate_dynamic_devices<C: Cpu, U1, U2, F>(
        src_spec: &ZxSpectrum<C, U1, F>,
        dst_spec: &mut ZxSpectrum<C, U2, F>
    ) -> Result<(), &'static str>
    where U1: DeviceAccess + Video + 'static,
          U2: DeviceAccess + Video + 'static
{
    if let Some(dynbus) = src_spec.ula.dyn_bus_device_ref() {
        for device in dynbus {
            let device = recreate_dynamic_device(&**device)?;
            dst_spec.attach_device(device);
        }
    }
    Ok(())
}

#[inline(never)]
fn recreate_dynamic_device<V1: VideoFrame + 'static, V2: VideoFrame + 'static>(
        device: &NamedDynDevice<VFrameTs<V1>>
    ) -> Result<BoxNamedDynDevice<VFrameTs<V2>>, &'static str>
{
    if device.is::<Ay3_891xMelodik<VFrameTs<V1>>>() {
        Ok(Ay3_891xMelodik::<VFrameTs<V2>>::default().into())
    }
    else if device.is::<Ay3_891xFullerBox<VFrameTs<V1>>>() {
        Ok(Ay3_891xFullerBox::<VFrameTs<V2>>::default().into())
    }
    else {
        Err("unknown device")
    }
}

impl SerializeDynDevice for SerdeDynDevice {
    fn serialize_dyn_device<T: 'static + fmt::Debug + Copy, S: Serializer>(
            device: &Box<dyn NamedBusDevice<T>>,
            serializer: S
        ) -> Result<S::Ok, S::Error>
    {
        if let Some(device) = device.downcast_ref::<Ay3_891xMelodik<T>>() {
            (DeviceType::Ay3_891xMelodik, device).serialize(serializer)
        }
        else if let Some(device) = device.downcast_ref::<Ay3_891xFullerBox<T>>() {
            (DeviceType::Ay3_891xFullerBox, device).serialize(serializer)
        }
        else {
            Err(ser::Error::custom("unknown device"))
        }
    }
}

impl<'de> DeserializeDynDevice<'de> for SerdeDynDevice {
    fn deserialize_dyn_device<T: 'static + Default + fmt::Debug + Copy, D: Deserializer<'de>>(
            deserializer: D
        ) -> Result<Box<dyn NamedBusDevice<T>>, D::Error>
    {
        struct DeviceVisitor<T>(PhantomData<T>);

        impl<'de, T: 'static + Default + fmt::Debug + Copy> Visitor<'de> for DeviceVisitor<T> {
            type Value = Box<dyn NamedBusDevice<T>>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a device tuple descriptor")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let dev_type = seq.next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                device_type_dispatch!((dev_type) => (seq)(next_element::@device<T>)?.map(Into::into))
                    .ok_or_else(|| de::Error::invalid_length(1, &self))
            }
        }

        deserializer.deserialize_tuple(2, DeviceVisitor::<T>(PhantomData))
    }
}

impl SnapshotCreator for ZxSpectrumEmu {
    fn model(&self) -> ComputerModel {
        let model = ModelRequest::from(&self.model);
        console_log!("model: {}", model);
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
        console_log!("clock: {}", ts);
        ts
    }

    fn border_color(&self) -> BorderColor {
        let color = self.model.border_color();
        console_log!("border color: {:?}", color);
        color
    }

    fn issue(&self) -> ReadEarMode {
        let issue = self.spectrum_control_ref().read_ear_mode();
        console_log!("keyboard issue: {:?}", issue);
        issue
    }

    fn memory_ref(&self, range: MemoryRange) -> Result<&[u8], ZxMemoryError> {
        console_log!("mem range ref: {:?}", range);
        spectrum_model_dispatch!((&self.model)(spec) => memory_ref(range, spec.ula.memory_ref()))
    }

    fn joystick(&self) -> Option<JoystickModel> {
        let joy = self.model.current_joystick().and_then(|name| {
            let sub_joy = self.model.emulator_state_ref().sub_joy;
            joystick_model_from_name(name, sub_joy)
        });
        console_log!("joystick: {:?}", joy);
        joy
    }

    fn ay_state(&self, choice: Ay3_891xDevice) -> Option<(AyRegister, &[u8;16])> {
        console_log!("AY req: {:?}", choice);

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
                console_log!("AY: {:?}, {:?}", sel, regs);
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
        console_log!("ULA 128 flags: {:?}", flags);
        flags
    }

    fn ula3_flags(&self) -> Ula3CtrlFlags {
        let flags = self.model.ula3_ctrl_port_value().unwrap();
        console_log!("ULA 3 flags: {:?}", flags);
        flags
    }

    fn timex_flags(&self) -> ScldCtrlFlags {
        let flags = self.model.scld_ctrl_port_value().unwrap();
        console_log!("SCLD flags: {:?}", flags);
        flags
    }

    fn timex_memory_banks(&self) -> u8 {
        let flags = self.model.scld_mmu_port_value().unwrap();
        console_log!("SCLD banks: {:b}", flags);
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
        console_log!("{} {} {:?} {:?}", model, extensions, border, issue);
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
        console_log!("CPU: {:?}", cpu);
        self.model.set_cpu(cpu)
    }

    fn set_clock(&mut self, tstates: FTs) {
        console_log!("set clock: {:?}", tstates);
        self.model.set_frame_tstate(tstates)
    }

    fn write_port(&mut self, port: u16, data: u8) {
        console_log!("write port: {:x} {}", port, data);
        self.model.write_port(port, data)
    }

    fn select_joystick(&mut self, joystick: JoystickModel) {
        console_log!("joystick {:?}", joystick);
        self.model.select_joystick(joy_index_from_joystick_model(Some(joystick)));
    }

    fn setup_ay(&mut self, choice: Ay3_891xDevice, reg_selected: AyRegister, reg_values: &[u8;16]) {
        console_log!("AY: {:?}, {:?}, {:?}", choice, reg_selected, reg_values);

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
        console_log!("mem range: {:?}", range);
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
