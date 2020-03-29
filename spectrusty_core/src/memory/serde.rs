//! Utilities for serializing memory as base64 strings or just bytes in binary serializers.
use core::fmt;
use std::rc::Rc;

use serde::{Serializer, Deserialize, Deserializer, de::{self, Visitor}};

use super::{MEM32K_SIZE, MEM48K_SIZE, MEM64K_SIZE, MEM128K_SIZE};

pub fn serialize_mem<T, S>(mem: &T, serializer: S) -> Result<S::Ok, S::Error>
    where T: MemSerExt,
          S: Serializer
{
    if serializer.is_human_readable() {
        serializer.serialize_str(&base64::encode(mem.as_slice()))
    }
    else {
        serializer.serialize_bytes(mem.as_slice())
    }
}

pub fn deserialize_mem<'de, T, D>(deserializer: D) -> Result<T, D::Error>
    where T: MemDeExt,
          D: Deserializer<'de>
{
    if deserializer.is_human_readable() {
        Deserialize::deserialize(deserializer).and_then(|string: &str|
            base64::decode(string).map_err(|err| de::Error::custom(err.to_string()) )
        )
        .and_then(T::try_from_byte_buf)
    }
    else if T::PREFER_FROM_BYTE_BUF {
        deserializer.deserialize_byte_buf(ByteBufVisitor)
                    .and_then(T::try_from_byte_buf)
    }
    else {
        Deserialize::deserialize(deserializer)
                    .and_then(T::try_from_bytes)
    }
}

pub trait MemSerExt: Sized {
    fn as_slice(&self) -> &[u8];
}

pub trait MemDeExt: Sized {
    const PREFER_FROM_BYTE_BUF: bool;
    fn try_from_byte_buf<E: de::Error>(buf: Vec<u8>) -> Result<Self, E>;
    fn try_from_bytes<E: de::Error>(slice: &[u8]) -> Result<Self, E>;
}

macro_rules! impl_mem_ser_de_ext {
    ($len:expr) => {
        impl MemSerExt for Box<[u8;$len]> {
            fn as_slice(&self) -> &[u8] {
                &self[..]
            }
        }

        impl MemDeExt for Box<[u8;$len]> {
            const PREFER_FROM_BYTE_BUF: bool = true;

            fn try_from_byte_buf<E: de::Error>(buf: Vec<u8>) -> Result<Self, E> {
                if buf.len() == $len {
                    let buf = buf.into_boxed_slice();
                    Ok(unsafe { Box::from_raw(Box::into_raw(buf) as *mut [u8; $len]) })
                }
                else {
                    Err(de::Error::custom(
                        format!("failed to deserialize memory, {} bytes required, received: {}",
                                $len, buf.len())
                    ))
                }
            }

            fn try_from_bytes<E: de::Error>(slice: &[u8]) -> Result<Self, E> {
                Self::try_from_byte_buf(Vec::from(slice))
            }
        }
    };
}

impl_mem_ser_de_ext!(MEM32K_SIZE);
impl_mem_ser_de_ext!(MEM48K_SIZE);
impl_mem_ser_de_ext!(MEM64K_SIZE);
impl_mem_ser_de_ext!(MEM32K_SIZE + MEM128K_SIZE);
impl_mem_ser_de_ext!(MEM64K_SIZE + MEM128K_SIZE);
impl_mem_ser_de_ext!(MEM128K_SIZE + MEM128K_SIZE);

impl<'a, T: MemSerExt> MemSerExt for &'a T {
    fn as_slice(&self) -> &[u8] {
        T::as_slice(self)
    }
}

impl MemSerExt for Rc<[u8]> {
    fn as_slice(&self) -> &[u8] {
        &self[..]
    }
}

impl MemDeExt for Rc<[u8]> {
    const PREFER_FROM_BYTE_BUF: bool = false;

    fn try_from_byte_buf<E: de::Error>(buf: Vec<u8>) -> Result<Self, E> {
        Ok(Rc::from(buf))
    }

    fn try_from_bytes<E: de::Error>(slice: &[u8]) -> Result<Self, E> {
        Ok(Rc::from(slice))
    }
}

struct ByteBufVisitor;

impl Visitor<'_> for ByteBufVisitor {
    type Value = Vec<u8>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a byte array")
    }

    fn visit_byte_buf<E: de::Error>(self, v: Vec<u8>) -> Result<Self::Value, E> {
        Ok(v)
    }

    fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<Self::Value, E> {
        Ok(Vec::from(v))
    }

    fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
        Ok(Vec::from(v))
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
        Ok(Vec::from(v))
    }

}
