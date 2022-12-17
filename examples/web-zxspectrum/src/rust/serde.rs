/*
    web-zxspectrum: ZX Spectrum emulator example as a Web application.
    Copyright (C) 2020-2022  Rafal Michalski

    For the full copyright notice, see the lib.rs file.
*/
use core::marker::PhantomData;
use core::str::FromStr;
use core::fmt;

use serde::{
    Serialize, Serializer, Deserialize, Deserializer,
    de::{self, Visitor, SeqAccess},
    ser
};
use spectrusty::z80emu::Cpu;
use spectrusty::chip::ControlUnit;
use spectrusty::bus::{
    NamedBusDevice, SerializeDynDevice, DeserializeDynDevice,
    NamedDynDevice, BoxNamedDynDevice,
};
use zxspectrum_common::{
    Ay3_891xMelodik, Ay3_891xFullerBox, KempstonMouse,
    ZxSpectrum, DeviceAccess, DynamicDevices,
    BusTs,
    spectrum_model_dispatch
};

use crate::ZxSpectrumEmuModel;

pub struct SerdeDynDevice;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum DeviceType {
    Ay3_891xMelodik,
    Ay3_891xFullerBox,
    KempstonMouse
}

impl FromStr for DeviceType {
    type Err = &'static str;
    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "Melodik"    => Ok(DeviceType::Ay3_891xMelodik),
            "Fuller Box" => Ok(DeviceType::Ay3_891xFullerBox),
            "Kempston Mouse" => Ok(DeviceType::KempstonMouse),
            _ => Err("Unrecognized device name")
        }
    }
}

macro_rules! device_type_dispatch {
    (($dt:expr) => ($expr:expr)($fn:ident::@device<$ts:ty>)$($tt:tt)*) => {
        match $dt {
            DeviceType::Ay3_891xMelodik   => { $expr.$fn::<Ay3_891xMelodik<$ts>>()$($tt)* }
            DeviceType::Ay3_891xFullerBox => { $expr.$fn::<Ay3_891xFullerBox<$ts>>()$($tt)* }
            DeviceType::KempstonMouse     => { $expr.$fn::<KempstonMouse<$ts>>()$($tt)* }
        }
    };
}

impl DeviceType {
    pub fn create_device<T>(self) -> BoxNamedDynDevice<T>
        where T: fmt::Debug + Copy + Default + 'static
    {
        match self {
            DeviceType::Ay3_891xMelodik => Ay3_891xMelodik::default().into(),
            DeviceType::Ay3_891xFullerBox => Ay3_891xFullerBox::default().into(),
            DeviceType::KempstonMouse => KempstonMouse::default().into(),
        }
    }

    pub fn attach_device_to_model(self, model: &mut ZxSpectrumEmuModel) -> bool {
        spectrum_model_dispatch!(model(spec) => spec.attach_device(self.create_device()))
    }

    pub fn detach_device<C: Cpu, U, F>(self, spec: &mut ZxSpectrum<C, U, F>)
        where U: DeviceAccess + 'static,
              BusTs<U>: fmt::Debug + Copy
    {
        device_type_dispatch!((self) => (spec)(detach_device::@device<BusTs<U>>);)
    }

    pub fn detach_device_from_model(self, model: &mut ZxSpectrumEmuModel) {
        spectrum_model_dispatch!(model(spec) => self.detach_device(spec))
    }

    pub fn has_device<C: Cpu, U, F>(self, spec: &ZxSpectrum<C, U, F>) -> bool
        where U: ControlUnit + 'static
    {
        device_type_dispatch!((self) => (spec.state.devices)(has_device::@device<BusTs<U>>))
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
    where U1: DeviceAccess,
          BusTs<U1>: fmt::Debug + Copy + 'static,
          U2: DeviceAccess,
          BusTs<U2>: fmt::Debug + Copy + Default + 'static,
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
fn recreate_dynamic_device<T1: fmt::Debug + Copy + 'static,
                           T2: fmt::Debug + Default + Copy + 'static>(
        device: &NamedDynDevice<T1>
    ) -> Result<BoxNamedDynDevice<T2>, &'static str>
{
    if device.is::<Ay3_891xMelodik<T1>>() {
        Ok(Ay3_891xMelodik::<T2>::default().into())
    }
    else if device.is::<Ay3_891xFullerBox<T1>>() {
        Ok(Ay3_891xFullerBox::<T2>::default().into())
    }
    else if device.is::<KempstonMouse<T1>>() {
        Ok(KempstonMouse::<T2>::default().into())
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
        else if let Some(device) = device.downcast_ref::<KempstonMouse<T>>() {
            (DeviceType::KempstonMouse, device).serialize(serializer)
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
