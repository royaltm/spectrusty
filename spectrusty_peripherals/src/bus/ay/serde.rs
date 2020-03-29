use core::marker::PhantomData;
use core::fmt;
use serde::{Deserialize, de::{self, Deserializer, Visitor, SeqAccess, MapAccess}};

use super::Ay3_891xBusDevice;

impl<'de, T, P, A, B, D> Deserialize<'de> for Ay3_891xBusDevice<T, P, A, B, D>
    where T: Default,
          A: Deserialize<'de> + Default,
          B: Deserialize<'de> + Default,
          D: Deserialize<'de> + Default,
{
    fn deserialize<DE>(deserializer: DE) -> Result<Self, DE::Error>
        where DE: Deserializer<'de>,
    {

        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "camelCase")]
        enum Field { AySound, AyIo, Bus }

        struct Ay3_891xBusDeviceVisitor<T, P, A, B, D>(
            PhantomData<T>,
            PhantomData<P>,
            PhantomData<A>,
            PhantomData<B>,
            PhantomData<D>
        );

        impl<'de, T, P, A, B, D> Visitor<'de> for Ay3_891xBusDeviceVisitor<T, P, A, B, D>
            where T: Default,
                  A: Deserialize<'de> + Default,
                  B: Deserialize<'de> + Default,
                  D: Deserialize<'de> + Default,
        {
            type Value = Ay3_891xBusDevice<T, P, A, B, D>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct Ay3_891xBusDevice")
            }

            fn visit_seq<V>(self, mut seq: V) -> Result<Self::Value, V::Error>
                where V: SeqAccess<'de>
            {
                let ay_sound = seq.next_element()?.unwrap_or_default();
                let ay_io = seq.next_element()?.unwrap_or_default();
                let bus = seq.next_element()?.unwrap_or_default();
                Ok(Ay3_891xBusDevice { ay_sound, ay_io, bus, _port_decode: PhantomData })
            }

            fn visit_map<V>(self, mut map: V) -> Result<Self::Value, V::Error>
                where V: MapAccess<'de>,
            {
                let mut ay_sound = None;
                let mut ay_io = None;
                let mut bus = None;
                while let Some(key) = map.next_key()? {
                    match key {
                        Field::AySound => {
                            if ay_sound.is_some() {
                                return Err(de::Error::duplicate_field("aySound"));
                            }
                            ay_sound = Some(map.next_value()?);
                        }
                        Field::AyIo => {
                            if ay_io.is_some() {
                                return Err(de::Error::duplicate_field("ayIo"));
                            }
                            ay_io = Some(map.next_value()?);
                        }
                        Field::Bus => {
                            if bus.is_some() {
                                return Err(de::Error::duplicate_field("bus"));
                            }
                            bus = Some(map.next_value()?);
                        }
                    }
                }
                let ay_sound = ay_sound.unwrap_or_default();
                let ay_io = ay_io.unwrap_or_default();
                let bus = bus.unwrap_or_default();
                Ok(Ay3_891xBusDevice { ay_sound, ay_io, bus, _port_decode: PhantomData })
            }
        }

        const FIELDS: &'static [&'static str] = &["aySound", "ayIo", "bus"];
        deserializer.deserialize_struct("Ay3_891xBusDevice", FIELDS,
            Ay3_891xBusDeviceVisitor(PhantomData, PhantomData, PhantomData, PhantomData, PhantomData))
    }
}
