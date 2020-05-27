use core::fmt;
use core::ops::{Deref, DerefMut};
use core::marker::PhantomData;
use ::serde::{
    Serialize, Serializer, Deserialize, Deserializer,
    ser::{SerializeStruct, SerializeSeq},
    de::{self, Visitor, SeqAccess, MapAccess}
};
use super::*;
use super::super::{NullDevice, BusDevice};

/// This trait needs to be implemented for [DynamicSerdeBus] to be able to serialize dynamic devices.
pub trait SerializeDynDevice {
    /// The type used as dynamic device's [BusDevice::Timestamp].
    type Timestamp;
    /// This function should serialize the provided dynamic device.
    ///
    /// The serialized form of the `device` depends completely on the implementation of this function,
    /// however it should probably include some device identifier along with the device data.
    fn serialize_dyn_device<S: Serializer>(
        device: &Box<dyn NamedBusDevice<Self::Timestamp>>,
        serializer: S
    ) -> Result<S::Ok, S::Error>;
}

/// This trait needs to be implemented for [DynamicSerdeBus] to be able to deserialize dynamic devices.
pub trait DeserializeDynDevice<'de> {
    /// The type used as dynamic device's [BusDevice::Timestamp].
    type Timestamp;
    fn deserialize_dyn_device<D: Deserializer<'de>>(
        deserializer: D
    ) -> Result<Box<dyn NamedBusDevice<Self::Timestamp>>, D::Error>;
}

/// A wrapper for [DynamicBus] that is able to serialize and deserialize together with devices attached to it.
///
/// Use this instead of [DynamicBus] when specifying [crate::chip::ControlUnit::BusDevice].
///
/// Provide a type that implements [SerializeDynDevice] and [DeserializeDynDevice] as struct's generic parameter `S`.
#[repr(transparent)]
pub struct DynamicSerdeBus<S, D: BusDevice=NullDevice<VideoTs>>(DynamicBus<D>, PhantomData<S>);

impl<SDD, B> Serialize for DynamicSerdeBus<SDD, B>
    where SDD: SerializeDynDevice<Timestamp=B::Timestamp>,
          B: BusDevice + Serialize
{
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {

        struct DevWrap<'a, SDD: SerializeDynDevice>(&'a Box<dyn NamedBusDevice<SDD::Timestamp>>);

        impl<'a, SDD: SerializeDynDevice> Serialize for DevWrap<'a, SDD> {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                SDD::serialize_dyn_device(self.0, serializer)
            }
        }

        struct SliceDevWrap<'a, SDD: SerializeDynDevice>(&'a [BoxNamedDynDevice<SDD::Timestamp>]);

        impl<'a, SDD: SerializeDynDevice> Serialize for SliceDevWrap<'a, SDD> {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
                for device in self.0 {
                    seq.serialize_element(&DevWrap::<SDD>(device))?;
                }
                seq.end()
            }
        }

        let mut state = serializer.serialize_struct("DynamicBus", 2)?;
        state.serialize_field("bus", &self.0.bus)?;
        let devices = SliceDevWrap::<SDD>(&self.0.devices);
        state.serialize_field("devices", &devices)?;
        state.end()
    }
}

impl<'de, DDD, B> Deserialize<'de> for DynamicSerdeBus<DDD, B>
    where DDD: DeserializeDynDevice<'de, Timestamp=B::Timestamp>,
          B: BusDevice + Deserialize<'de> + Default
{
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {

        struct BoxedDevWrap<'de, DDD: DeserializeDynDevice<'de>>(BoxNamedDynDevice<DDD::Timestamp>);

        impl<'de, DDD> Deserialize<'de> for BoxedDevWrap<'de, DDD>
            where DDD: DeserializeDynDevice<'de>
        {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                Ok(BoxedDevWrap(DDD::deserialize_dyn_device(deserializer)?))
            }
        }

        struct DevicesWrap<'de, DDD: DeserializeDynDevice<'de>>(Vec<BoxNamedDynDevice<DDD::Timestamp>>);

        impl<'de, DDD> Deserialize<'de> for DevicesWrap<'de, DDD>
            where DDD: DeserializeDynDevice<'de>
        {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                struct SeqDevVisitor<DDD>(PhantomData<DDD>);

                impl<'de, DDD> Visitor<'de> for SeqDevVisitor<DDD>
                    where DDD: DeserializeDynDevice<'de>
                {
                    type Value = DevicesWrap<'de, DDD>;

                    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                        formatter.write_str("a sequence of dynamic bus devices")
                    }

                    fn visit_seq<V: SeqAccess<'de>>(self, mut seq: V) -> Result<Self::Value, V::Error> {
                        let mut vec = Vec::new();
                        if let Some(size) = seq.size_hint() {
                            vec.reserve_exact(size);
                        }
                        while let Some(BoxedDevWrap::<DDD>(dev)) = seq.next_element()? {
                            vec.push(dev);
                        }
                        Ok(DevicesWrap(vec))
                    }
                }

                deserializer.deserialize_seq(SeqDevVisitor::<DDD>(PhantomData))
            }
        }

        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "lowercase")]
        enum Field { Bus, Devices }

        struct DynamicBusVisitor<DDD, B>(PhantomData<DDD>, PhantomData<B>);

        impl<'de, DDD, B> Visitor<'de> for DynamicBusVisitor<DDD, B>
            where DDD: DeserializeDynDevice<'de, Timestamp=B::Timestamp>,
                  B: BusDevice + Deserialize<'de> + Default
        {
            type Value = DynamicSerdeBus<DDD, B>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct DynamicBus")
            }

            fn visit_seq<V: SeqAccess<'de>>(self, mut seq: V) -> Result<Self::Value, V::Error> {
                let bus = seq.next_element()?.unwrap_or_default();
                let DevicesWrap::<DDD>(devices) = seq.next_element()?
                                                     .unwrap_or_else(|| DevicesWrap(Vec::new()));
                Ok(DynamicSerdeBus(DynamicBus { devices, bus }, PhantomData))
            }

            fn visit_map<V: MapAccess<'de>>(self, mut map: V) -> Result<Self::Value, V::Error> {
                let mut devices = Vec::new();
                let mut bus = None;
                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Bus => {
                            if bus.is_some() {
                                return Err(de::Error::duplicate_field("bus"));
                            }
                            bus = Some(map.next_value()?);
                        }
                        Field::Devices => {
                            if !devices.is_empty() {
                                return Err(de::Error::duplicate_field("devices"));
                            }
                            devices = map.next_value::<DevicesWrap<DDD>>()?.0;
                        }
                    }
                }
                let bus = bus.unwrap_or_default();
                Ok(DynamicSerdeBus(DynamicBus { devices, bus }, PhantomData))
            }
        }

        const FIELDS: &[&str] = &["bus", "devices"];
        deserializer.deserialize_struct("DynamicBus", FIELDS,
                     DynamicBusVisitor::<DDD, B>(PhantomData, PhantomData))
    }
}

impl<S, D> Default for DynamicSerdeBus<S, D>
    where D: BusDevice + Default,
          D::Timestamp: Default
{
    fn default() -> Self {
        DynamicSerdeBus(DynamicBus::default(), PhantomData)
    }
}

impl<S, D> fmt::Debug for DynamicSerdeBus<S, D>
    where D: BusDevice + fmt::Debug,
          D::Timestamp: fmt::Debug
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<S, D: BusDevice> Deref for DynamicSerdeBus<S, D> {
    type Target = DynamicBus<D>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<S, D: BusDevice> DerefMut for DynamicSerdeBus<S, D> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<S, D: BusDevice> From<DynamicBus<D>> for DynamicSerdeBus<S, D> {
    fn from(dynamic_bus: DynamicBus<D>) -> Self {
        DynamicSerdeBus(dynamic_bus, PhantomData)
    }
}

impl<S, D: BusDevice> From<DynamicSerdeBus<S, D>> for DynamicBus<D> {
    fn from(dynamic_bus: DynamicSerdeBus<S, D>) -> Self {
        dynamic_bus.0
    }
}

impl<S, D: BusDevice> BusDevice for DynamicSerdeBus<S, D>
    where D: BusDevice,
          D::Timestamp: Debug + Copy
{
    type Timestamp = D::Timestamp;
    type NextDevice = D;

    #[inline(always)]
    fn next_device_mut(&mut self) -> &mut Self::NextDevice {
        self.0.next_device_mut()
    }
    #[inline(always)]
    fn next_device_ref(&self) -> &Self::NextDevice {
        self.0.next_device_ref()
    }
    #[inline(always)]
    fn into_next_device(self) -> Self::NextDevice {
        self.0.into_next_device()
    }
    #[inline(always)]
    fn reset(&mut self, timestamp: Self::Timestamp) {
        self.0.reset(timestamp)
    }
    #[inline(always)]
    fn update_timestamp(&mut self, timestamp: Self::Timestamp) {
        self.0.update_timestamp(timestamp)
    }
    #[inline(always)]
    fn next_frame(&mut self, timestamp: Self::Timestamp) {
        self.0.next_frame(timestamp)
    }
    #[inline(always)]
    fn read_io(&mut self, port: u16, timestamp: Self::Timestamp) -> Option<(u8, Option<NonZeroU16>)> {
        self.0.read_io(port, timestamp)
    }
    #[inline(always)]
    fn write_io(&mut self, port: u16, data: u8, timestamp: Self::Timestamp) -> Option<u16> {
        self.0.write_io(port, data, timestamp)
    }
}
