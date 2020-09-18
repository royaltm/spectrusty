/*
    sdl2-zxspectrum: ZX Spectrum emulator example as a SDL2 application.
    Copyright (C) 2020  Rafal Michalski

    For the full copyright notice, see the main.rs file.
*/
//! (De)serialization of dynamic devices.
use core::fmt;
use core::marker::PhantomData;

use serde::{
    Serialize, Serializer, Deserialize, Deserializer,
    de::{self, Visitor, SeqAccess},
    ser
};

use spectrusty::clock::FrameTimestamp;
use spectrusty::bus::{
    NamedBusDevice, SerializeDynDevice, DeserializeDynDevice
};
use super::{ZxInterface1, ZxPrinter};

pub use zxspectrum_common::{Ay3_891xMelodik, Ay3_891xFullerBox, KempstonMouse};

#[derive(Debug, Default, Copy, Clone)]
pub struct SerdeDynDevice;

macro_rules! define_device_types {
    ($dol:tt $($name:ident),*) => {

        #[derive(Serialize, Deserialize)]
        enum DeviceType { $($name),* }

        macro_rules! serialize_dyn_devices {
            ($device:ident, $serializer:ident, @device<$ts:ty>) => {
                $(
                    if let Some(device) = $device.downcast_ref::<$name<$ts>>() {
                        (DeviceType::$name, device).serialize($serializer)
                    }
                    else
                )*
                {
                    Err(ser::Error::custom("unknown device"))
                }
            };
        }

        macro_rules! device_type_dispatch {
            (($dt:expr) => ($expr:expr).$fn:ident::@device<$ts:ty>()$dol($tt:tt)*) => {
                match $dt {
                    $(DeviceType::$name => { $expr.$fn::<$name<$ts>>()$dol($tt)* })*
                }
            };
        }
    }
}

define_device_types!{$
    Ay3_891xMelodik,
    Ay3_891xFullerBox,
    KempstonMouse,
    ZxPrinter,
    ZxInterface1
}

impl SerializeDynDevice for SerdeDynDevice {
    fn serialize_dyn_device<T, S: Serializer>(
            device: &Box<dyn NamedBusDevice<T>>,
            serializer: S
        ) -> Result<S::Ok, S::Error>
        where T: FrameTimestamp + Serialize + 'static
    {
        serialize_dyn_devices!(device, serializer, @device<T>)
    }
}

impl<'de> DeserializeDynDevice<'de> for SerdeDynDevice {
    fn deserialize_dyn_device<T, D: Deserializer<'de>>(
            deserializer: D
        ) -> Result<Box<dyn NamedBusDevice<T>>, D::Error>
        where T: Default + FrameTimestamp + Deserialize<'de> + 'static
    {
        struct DeviceVisitor<T>(PhantomData<T>);

        impl<'de, T: Default + FrameTimestamp + Deserialize<'de> + 'static> Visitor<'de> for DeviceVisitor<T> {
            type Value = Box<dyn NamedBusDevice<T>>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a device tuple descriptor")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let dt: DeviceType = seq.next_element()?
                                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;

                device_type_dispatch!(
                    (dt) => (seq).next_element::@device<T>()?.map(Into::into)
                )
                .ok_or_else(|| de::Error::invalid_length(1, &self))
            }
        }
        deserializer.deserialize_tuple(2, DeviceVisitor::<T>(PhantomData))
    }
}
