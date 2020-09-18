/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
use core::fmt::{self, Debug};
use core::ops::{Deref, DerefMut};
use core::marker::PhantomData;
use ::serde::{
    Serialize, Serializer, Deserialize, Deserializer,
    ser::{SerializeStruct, SerializeSeq},
    de::{self, Visitor, SeqAccess, MapAccess}
};
use crate::clock::FrameTimestamp;
use super::*;
use super::super::{VFNullDevice, BusDevice};

/// This trait needs to be implemented by a type provided to [DynamicSerdeBus] as a parameter `S`,
/// to serialize dynamic devices.
pub trait SerializeDynDevice {
    /// This function should serialize the provided dynamic device.
    ///
    /// The serialized form of the `device` depends completely on the implementation of this function,
    /// however it should probably include some device identifier along with the device data.
    fn serialize_dyn_device<T: FrameTimestamp + Serialize + 'static,
                            S: Serializer>(
        device: &Box<dyn NamedBusDevice<T>>,
        serializer: S
    ) -> Result<S::Ok, S::Error>;
}

/// This trait needs to be implemented by a type provided to [DynamicSerdeBus] as a parameter `S`,
/// to deserialize dynamic devices.
pub trait DeserializeDynDevice<'de> {
    /// This function should deserialize and return the dynamic device on success.
    fn deserialize_dyn_device<T: Default + FrameTimestamp + Deserialize<'de> + 'static,
                              D: Deserializer<'de>>(
        deserializer: D
    ) -> Result<Box<dyn NamedBusDevice<T>>, D::Error>;
}

/// A terminated [DynamicSerdeBus] pseudo-device with [`VFNullDevice<V>`][VFNullDevice].
pub type DynamicSerdeVBus<S, V> = DynamicSerdeBus<S, VFNullDevice<V>>;

/// A wrapper for [DynamicBus] that is able to serialize and deserialize together with currently
/// attached dynamic devices.
///
/// Use this type instead of [DynamicBus] when specifying [crate::chip::ControlUnit::BusDevice] or
/// [BusDevice::NextDevice].
///
/// A type that implements [SerializeDynDevice] and [DeserializeDynDevice] must be declared as
/// generic parameter `S`.
#[repr(transparent)]
pub struct DynamicSerdeBus<S, D: BusDevice>(DynamicBus<D>, PhantomData<S>);

impl<SDD, B> Serialize for DynamicSerdeBus<SDD, B>
    where SDD: SerializeDynDevice,
          B: BusDevice + Serialize,
          B::Timestamp: FrameTimestamp + Serialize + 'static
{
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {

        struct DevWrap<'a, T, SDD>(
            &'a Box<dyn NamedBusDevice<T>>, PhantomData<SDD>
        );

        impl<'a, T, SDD> Serialize for DevWrap<'a, T, SDD>
            where T: FrameTimestamp + Serialize + 'static,
                  SDD: SerializeDynDevice
        {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                SDD::serialize_dyn_device(self.0, serializer)
            }
        }

        struct SliceDevWrap<'a, T, SDD>(&'a [BoxNamedDynDevice<T>], PhantomData<SDD>);

        impl<'a, T, SDD> Serialize for SliceDevWrap<'a, T, SDD>
            where T: FrameTimestamp + Serialize + 'static,
                  SDD: SerializeDynDevice
        {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
                for device in self.0 {
                    seq.serialize_element(&DevWrap::<T, SDD>(device, PhantomData))?;
                }
                seq.end()
            }
        }

        let mut state = serializer.serialize_struct("DynamicBus", 2)?;
        state.serialize_field("bus", &self.0.bus)?;
        let devices = SliceDevWrap::<B::Timestamp, SDD>(&self.0.devices, PhantomData);
        state.serialize_field("devices", &devices)?;
        state.end()
    }
}

impl<'de, DDD, B> Deserialize<'de> for DynamicSerdeBus<DDD, B>
    where DDD: DeserializeDynDevice<'de> + 'de,
          B: BusDevice + Deserialize<'de> + Default,
          B::Timestamp: Default + FrameTimestamp + Deserialize<'de> + 'static
{
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {

        struct BoxedDevWrap<'de, T, DDD>(
            BoxNamedDynDevice<T>, PhantomData<&'de DDD>
        );

        impl<'de, T, DDD> Deserialize<'de> for BoxedDevWrap<'de, T, DDD>
            where T: Default + FrameTimestamp + Deserialize<'de> + 'static,
                  DDD: DeserializeDynDevice<'de>
        {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                Ok(BoxedDevWrap(DDD::deserialize_dyn_device(deserializer)?, PhantomData))
            }
        }

        struct DevicesWrap<'de, T, DDD>(
            Vec<BoxNamedDynDevice<T>>, PhantomData<&'de DDD>
        );

        impl<'de, T, DDD> Deserialize<'de> for DevicesWrap<'de, T, DDD>
            where T: Default + FrameTimestamp + Deserialize<'de> + 'static,
                  DDD: DeserializeDynDevice<'de>
        {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {

                struct SeqDevVisitor<T, DDD>(PhantomData<T>, PhantomData<DDD>);

                impl<'de, T, DDD> Visitor<'de> for SeqDevVisitor<T, DDD>
                    where T: Default + FrameTimestamp + Deserialize<'de> + 'static,
                          DDD: DeserializeDynDevice<'de> + 'de
                {
                    type Value = DevicesWrap<'de, T, DDD>;

                    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                        formatter.write_str("a sequence of dynamic bus devices")
                    }

                    fn visit_seq<V: SeqAccess<'de>>(self, mut seq: V) -> Result<Self::Value, V::Error> {
                        let mut vec = Vec::new();
                        if let Some(size) = seq.size_hint() {
                            vec.reserve_exact(size);
                        }
                        while let Some(BoxedDevWrap::<T, DDD>(dev, ..)) = seq.next_element()? {
                            vec.push(dev);
                        }
                        Ok(DevicesWrap(vec, PhantomData))
                    }
                }

                deserializer.deserialize_seq(SeqDevVisitor::<T, DDD>(PhantomData, PhantomData))
            }
        }

        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "lowercase")]
        enum Field { Bus, Devices }

        struct DynamicBusVisitor<DDD, B>(PhantomData<DDD>, PhantomData<B>);

        impl<'de, DDD, B> Visitor<'de> for DynamicBusVisitor<DDD, B>
            where DDD: DeserializeDynDevice<'de> + 'de,
                  B: BusDevice + Deserialize<'de> + Default,
                  B::Timestamp: Default + FrameTimestamp + Deserialize<'de> + 'static
        {
            type Value = DynamicSerdeBus<DDD, B>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct DynamicBus")
            }

            fn visit_seq<V: SeqAccess<'de>>(self, mut seq: V) -> Result<Self::Value, V::Error> {
                let bus = seq.next_element()?.unwrap_or_default();
                let DevicesWrap::<B::Timestamp, DDD>(devices, ..) = seq.next_element()?
                                .unwrap_or_else(|| DevicesWrap(Vec::new(), PhantomData));
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
                            devices = map.next_value::<DevicesWrap<B::Timestamp, DDD>>()?.0;
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
          D::Timestamp: Copy + Debug
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
