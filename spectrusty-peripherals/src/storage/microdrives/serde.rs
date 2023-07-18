/*
    Copyright (C) 2020-2022  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
use core::convert::TryFrom;
use core::mem;
use core::slice;
use base64::{Engine as _, engine::general_purpose};

use super::{HEAD_SIZE, DATA_SIZE, Sector};

use serde::{Serialize, Serializer, Deserialize, Deserializer, de};

impl<'a> From<&'a Sector> for &'a [u8] {
    fn from(sector: &Sector) -> &[u8] {
        debug_assert_eq!(mem::size_of_val(sector), HEAD_SIZE + DATA_SIZE);
        let data = sector as *const _ as *const u8;
        unsafe { slice::from_raw_parts(data, mem::size_of_val(sector)) }
    }
}

impl TryFrom<&'_[u8]> for Sector {
    type Error = &'static str;
    fn try_from(slice: &[u8]) -> Result<Self, Self::Error> {
        if slice.len() == HEAD_SIZE + DATA_SIZE {
            let ptr_head = slice.as_ptr() as *const [u8; HEAD_SIZE];
            let ptr_data = unsafe { slice.as_ptr().add(HEAD_SIZE) } as *const [u8; DATA_SIZE];
            let head = unsafe { *ptr_head };
            let data = unsafe { *ptr_data };
            Ok(Sector { head, data })
        }
        else {
            Err("incorrect size of the microdrive cartridge sector")
        }
    }
}

impl Serialize for Sector {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            serializer.serialize_str(&general_purpose::STANDARD.encode(<&[u8]>::from(self)))
        }
        else {
            serializer.serialize_bytes(self.into())
        }
    }
}

impl<'de> Deserialize<'de> for Sector {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de>
    {
        if deserializer.is_human_readable() {
            Deserialize::deserialize(deserializer).and_then(|string: &str|
                general_purpose::STANDARD.decode(string).map_err(de::Error::custom)
            )
            .and_then(|buf| Sector::try_from(buf.as_slice()).map_err(de::Error::custom))
        }
        else {
            Deserialize::deserialize(deserializer).and_then(|slice: &[u8]|
                Sector::try_from(slice).map_err(de::Error::custom)
            )
        }
    }
}
