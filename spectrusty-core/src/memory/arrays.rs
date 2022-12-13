/*
    Copyright (C) 2022  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
//! Utilities for serializing arrays as tuples.
use std::{marker::PhantomData};
use std::mem::{MaybeUninit};
use std::ptr;

use serde::{
    de::{SeqAccess, Visitor},
    ser::SerializeTuple,
    Deserialize, Deserializer, Serialize, Serializer,
};

pub fn serialize<S: Serializer, T: Serialize, const N: usize>(
    data: &[T; N],
    ser: S,
) -> Result<S::Ok, S::Error> {
    let mut tuple = ser.serialize_tuple(N)?;
    for item in data {
        tuple.serialize_element(item)?;
    }
    tuple.end()
}

struct ArrayVisitor<T, const N: usize>(PhantomData<T>);

impl<'de, T, const N: usize> Visitor<'de> for ArrayVisitor<T, N>
    where T: Deserialize<'de>,
{
    type Value = [T; N];

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str(&format!("an array of length {}", N))
    }

    #[inline]
    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where A: SeqAccess<'de>,
    {
        struct ArrayUninit<T, const N: usize> {
            data: [MaybeUninit<T>; N],
            init_len: usize
        }

        impl<T, const N: usize> Drop for ArrayUninit<T, N> {
            fn drop(&mut self) {
                for elem in &mut self.data[0..self.init_len] {
                    unsafe { ptr::drop_in_place(elem.as_mut_ptr()); }
                }
            }
        }

        let mut ary: ArrayUninit<T, N> = ArrayUninit {
            data: unsafe { MaybeUninit::uninit().assume_init() },
            init_len: 0
        };
        let mut iter = ary.data.iter_mut();
        while let Some(val) = seq.next_element()? {
            if let Some(elem) = iter.next() {
                elem.write(val);
                ary.init_len += 1;
            }
            else {
                return Err(serde::de::Error::invalid_length(N, &self));
            }
        }
        if ary.init_len != N {
            return Err(serde::de::Error::invalid_length(N, &self));
        }
        // All elements in, prevent drop of data
        ary.init_len = 0;
        Ok(unsafe { (&ary.data as *const _ as *const [T; N]).read() })
    }
}

pub fn deserialize<'de, D, T, const N: usize>(deserializer: D) -> Result<[T; N], D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    deserializer.deserialize_tuple(N, ArrayVisitor::<T, N>(PhantomData))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Clone, Copy, PartialEq, Debug)]
    #[derive(Serialize, Deserialize)]
    #[serde(transparent)]
    struct ArrayWrap<T: Serialize, const N: usize>(
        #[serde(serialize_with = "serialize", deserialize_with = "deserialize")] [T;N])
    where for <'a> T: Deserialize<'a>;

    #[test]
    fn arrays_serde_works() {
        let ary: ArrayWrap<u8,5> = ArrayWrap([1,2,3,4,5]);
        let serary = serde_json::to_string(&ary).unwrap();
        assert_eq!(&serary, "[1,2,3,4,5]");
        assert!(serde_json::from_str::<ArrayWrap<u8,4>>(&serary).is_err());
        assert!(serde_json::from_str::<ArrayWrap<u8,6>>(&serary).is_err());
        let ary_de: ArrayWrap<u8,5> = serde_json::from_str(&serary).unwrap();
        assert_eq!(&ary, &ary_de);

        let encoded: Vec<u8> = bincode::serialize(&ary).unwrap();
        assert!(bincode::deserialize::<ArrayWrap<u8,6>>(&encoded).is_err());
        let ary_de: ArrayWrap<u8,5> = bincode::deserialize(&encoded).unwrap();
        assert_eq!(&ary, &ary_de);

        let ary: ArrayWrap<String,3> = ArrayWrap(["foo".to_string(), "bar".to_string(), "baz".to_string()]);
        let serary = serde_json::to_string(&ary).unwrap();
        assert_eq!(&serary, r#"["foo","bar","baz"]"#);
        assert!(serde_json::from_str::<ArrayWrap<String,2>>(&serary).is_err());
        assert!(serde_json::from_str::<ArrayWrap<String,4>>(&serary).is_err());
        let ary_de: ArrayWrap<String,3> = serde_json::from_str(&serary).unwrap();
        assert_eq!(&ary, &ary_de);
    }
}
