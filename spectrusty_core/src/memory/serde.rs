//! Utilities for serializing memory as base64 strings or just bytes in binary serializers.
#[cfg(feature = "compression")] use core::iter::FromIterator;
use core::fmt;
use std::rc::Rc;

#[cfg(feature = "compression")] use compression::prelude::*;
#[cfg(feature = "compression")] use serde::ser;

use serde::{
    Serializer, Deserialize, Deserializer,
    de::{self, Visitor}
};

use super::{MEM8K_SIZE, MEM32K_SIZE, MEM48K_SIZE, MEM64K_SIZE, MEM128K_SIZE};

pub fn serialize_mem<T, S>(mem: &T, serializer: S) -> Result<S::Ok, S::Error>
    where T: MemSerExt,
          S: Serializer
{
    #[cfg(not(feature = "compression"))]
    {
        serialize_mem_slice(mem.as_slice(), serializer)
    }
    #[cfg(feature = "compression")]
    {
        let compr = mem.as_slice().into_iter().copied()
            .encode(&mut GZipEncoder::new(), Action::Finish)
            .collect::<Result<Vec<_>, _>>()
            .map_err(ser::Error::custom)?;
        serialize_mem_slice(&compr, serializer)
    }
}

pub fn serialize_mem_slice<S>(slice: &[u8], serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer
{
    if serializer.is_human_readable() {
        serializer.serialize_str(&base64::encode(slice))
    }
    else {
        serializer.serialize_bytes(slice)
    }
}

pub fn deserialize_mem<'de, T, D>(deserializer: D) -> Result<T, D::Error>
    where T: MemDeExt,
          D: Deserializer<'de>
{
    if deserializer.is_human_readable() {
        Deserialize::deserialize(deserializer).and_then(|string: &str|
            base64::decode(string).map_err(de::Error::custom)
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

            #[allow(unused_mut)]
            fn try_from_byte_buf<E: de::Error>(mut buf: Vec<u8>) -> Result<Self, E> {
                #[cfg(feature = "compression")]
                {
                    if is_compressed(&buf) {
                        buf = decompress(&buf)?;
                    }
                }
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
                #[cfg(feature = "compression")]
                {
                    if is_compressed(slice) {
                        return Self::try_from_byte_buf(decompress(slice)?)
                    }
                }
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
impl_mem_ser_de_ext!(MEM8K_SIZE + MEM128K_SIZE);
impl_mem_ser_de_ext!(MEM48K_SIZE + MEM128K_SIZE + MEM128K_SIZE);

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
        #[cfg(feature = "compression")]
        {
            if is_compressed(&buf) {
                return decompress(&buf)
            }
        }
        Ok(Rc::from(buf))
    }

    fn try_from_bytes<E: de::Error>(slice: &[u8]) -> Result<Self, E> {
        #[cfg(feature = "compression")]
        {
            if is_compressed(slice) {
                return decompress(slice)
            }
        }
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

#[cfg(feature = "compression")]
fn is_compressed(data: &[u8]) -> bool {
    match data.get(0..3) {
        Some(&[0x1f, 0x8b, 0x08]) => true,
        // Some(&[b'B', b'Z', b'h', b'1'..=b'9', 0x31, 0x41, 0x59, 0x26, 0x53, 0x59]) => true
        _ => false
    }
}

#[cfg(feature = "compression")]
fn decompress<T: FromIterator<u8>, E: de::Error>(data: &[u8]) -> Result<T, E> {
    data.into_iter().copied()
        .decode(&mut GZipDecoder::new())
        .collect::<Result<T, _>>()
        .map_err(de::Error::custom)
}
