use core::fmt;
use core::marker::PhantomData;

use serde::{
    Serialize, Serializer, Deserialize, Deserializer,
    de::{self, Visitor, SeqAccess},
    ser
};
use spectrusty::clock::VideoTs;
use spectrusty::chip::{
    ula::{UlaVideoFrame, UlaNTSCVidFrame},
    ula128::Ula128VidFrame,
    ula3::Ula3VidFrame
};
use spectrusty::bus::{
    NamedBusDevice, SerializeDynDevice, DeserializeDynDevice, 
    ay::{Ay3_891xMelodik, Ay3_891xFullerBox},
    mouse::KempstonMouse,
};
use spectrusty::video::VideoFrame;

use super::{ZxInterface1, ZxPrinter};

#[derive(Serialize, Deserialize)]
enum DeviceType {
    Ay3_891xMelodik,
    Ay3_891xFullerBox,
    KempstonMouse,
    ZxPrinter,
    ZxInterface1
}

#[derive(Serialize, Deserialize)]
enum VideoFrameType {
    UlaVideoFrame,
    UlaNTSCVidFrame,
    Ula128VidFrame,
    Ula3VidFrame
}

#[derive(Debug, Default, Copy, Clone)]
pub struct SerdeDynDevice;

macro_rules! serialize_dyn_devices {
    ($device:ident, $serializer:ident {$($name:ident),*} @<VideoFrame> {$($namev:ident),*}) => {
        $(
            if let Some(device) = $device.downcast_ref::<$name>() {
                (DeviceType::$name, device).serialize($serializer)
            }
            else
        )*
        $(
            if let Some(device) = $device.downcast_ref::<$namev<UlaVideoFrame>>() {
                (DeviceType::$namev, VideoFrameType::UlaVideoFrame, device).serialize($serializer)
            }
            else if let Some(device) = $device.downcast_ref::<$namev<UlaNTSCVidFrame>>() {
                (DeviceType::$namev, VideoFrameType::UlaNTSCVidFrame, device).serialize($serializer)
            }
            else if let Some(device) = $device.downcast_ref::<$namev<Ula128VidFrame>>() {
                (DeviceType::$namev, VideoFrameType::Ula128VidFrame, device).serialize($serializer)
            }
            else if let Some(device) = $device.downcast_ref::<$namev<Ula3VidFrame>>() {
                (DeviceType::$namev, VideoFrameType::Ula3VidFrame, device).serialize($serializer)
            }
            else
        )*
        {
            Err(ser::Error::custom("unknown device"))
        }
    };
}

impl SerializeDynDevice for SerdeDynDevice {
    type Timestamp = VideoTs;

    fn serialize_dyn_device<S: Serializer>(
            device: &Box<dyn NamedBusDevice<Self::Timestamp>>,
            serializer: S
        ) -> Result<S::Ok, S::Error>
    {
        serialize_dyn_devices!(device, serializer {
            Ay3_891xMelodik,
            Ay3_891xFullerBox,
            KempstonMouse
        }
        @<VideoFrame> {
            ZxPrinter,
            ZxInterface1
        })
    }
}

macro_rules! deserialize_device_with_video_frame {
    ($self:ident, $seq:ident, $device:ident) => {
        match $seq.next_element::<VideoFrameType>()?.ok_or_else(|| de::Error::invalid_length(1, &$self))? {
            VideoFrameType::UlaVideoFrame => $seq.next_element::<$device<UlaVideoFrame>>()?.map(Into::into),
            VideoFrameType::UlaNTSCVidFrame => $seq.next_element::<$device<UlaNTSCVidFrame>>()?.map(Into::into),
            VideoFrameType::Ula128VidFrame => $seq.next_element::<$device<Ula128VidFrame>>()?.map(Into::into),
            VideoFrameType::Ula3VidFrame => $seq.next_element::<$device<Ula3VidFrame>>()?.map(Into::into),
        }
    };
}

impl<'de> DeserializeDynDevice<'de> for SerdeDynDevice {
    type Timestamp = VideoTs;

    fn deserialize_dyn_device<D: Deserializer<'de>>(
            deserializer: D
        ) -> Result<Box<dyn NamedBusDevice<Self::Timestamp>>, D::Error>
    {
        struct DeviceVisitor;

        impl<'de> Visitor<'de> for DeviceVisitor {
            type Value = Box<dyn NamedBusDevice<VideoTs>>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a device tuple descriptor")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let dev_type = seq.next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                let device = match dev_type {
                    DeviceType::Ay3_891xMelodik => seq.next_element::<Ay3_891xMelodik>()?.map(Into::into),
                    DeviceType::Ay3_891xFullerBox => seq.next_element::<Ay3_891xFullerBox>()?.map(Into::into),
                    DeviceType::KempstonMouse => seq.next_element::<KempstonMouse>()?.map(Into::into),
                    DeviceType::ZxPrinter => deserialize_device_with_video_frame!(self, seq, ZxPrinter),
                    DeviceType::ZxInterface1 => deserialize_device_with_video_frame!(self, seq, ZxInterface1),
                };
                device.ok_or_else(|| de::Error::invalid_length(1, &self))
            }
        }

        deserializer.deserialize_tuple(2, DeviceVisitor)
    }
}
