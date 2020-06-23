use core::fmt;
use core::marker::PhantomData;

use serde::{
    Serialize, Serializer, Deserialize, Deserializer,
    de::{self, Visitor, SeqAccess},
    ser
};
use spectrusty::clock::FrameTimestamp;
use spectrusty::chip::{
    ula::{UlaVideoFrame, UlaNTSCVidFrame},
    ula128::Ula128VidFrame,
    ula3::Ula3VidFrame
};
use spectrusty::bus::{
    NullDevice, NamedBusDevice, SerializeDynDevice, DeserializeDynDevice, 
    ay,
    mouse,
};
use spectrusty::video::VideoFrame;

use super::{ZxInterface1, ZxPrinter};

pub type Ay3_891xMelodik<T> = ay::Ay3_891xMelodik<NullDevice<T>>;
pub type Ay3_891xFullerBox<T> = ay::Ay3_891xFullerBox<NullDevice<T>>;
pub type KempstonMouse<T> = mouse::KempstonMouse<NullDevice<T>>;

#[derive(Debug, Default, Copy, Clone)]
pub struct SerdeDynDevice;

#[derive(Serialize, Deserialize)]
enum DeviceType {
    Ay3_891xMelodik,
    Ay3_891xFullerBox,
    KempstonMouse,
    ZxPrinter,
    ZxInterface1
}

macro_rules! device_type_dispatch {
    (($dt:expr) => ($expr:expr).$fn:ident::@device<$ts:ty>()$($tt:tt)*) => {
        match $dt {
            DeviceType::Ay3_891xMelodik   => { $expr.$fn::<Ay3_891xMelodik<$ts>>()$($tt)* }
            DeviceType::Ay3_891xFullerBox => { $expr.$fn::<Ay3_891xFullerBox<$ts>>()$($tt)* }
            DeviceType::KempstonMouse     => { $expr.$fn::<KempstonMouse<$ts>>()$($tt)* }
            DeviceType::ZxPrinter         => { $expr.$fn::<ZxPrinter<$ts>>()$($tt)* }
            DeviceType::ZxInterface1      => { $expr.$fn::<ZxInterface1<$ts>>()$($tt)* }
        }
    };
}

macro_rules! serialize_dyn_devices {
    ($device:ident, $serializer:ident {$($name:ident<$ts:ty>),*}) => {
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

impl SerializeDynDevice for SerdeDynDevice {
    fn serialize_dyn_device<T, S: Serializer>(
            device: &Box<dyn NamedBusDevice<T>>,
            serializer: S
        ) -> Result<S::Ok, S::Error>
        where T: FrameTimestamp + Serialize + 'static
    {
        serialize_dyn_devices!(device, serializer {
            Ay3_891xMelodik<T>,
            Ay3_891xFullerBox<T>,
            KempstonMouse<T>,
            ZxPrinter<T>,
            ZxInterface1<T>
        })
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
