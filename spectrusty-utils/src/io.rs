/*
    Copyright (C) 2020-2022  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
use std::io;

#[cfg(feature = "snapshot")] use core::fmt;
#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize, Serializer, Deserializer, de::{self, Visitor}};

/// Special version of [io::Sink] that implements Default and Clone traits.
#[derive(Debug)]
pub struct Sink(io::Sink);

/// Special version of [io::Empty] that implements Default and Clone traits.
#[derive(Debug)]
pub struct Empty(io::Empty);

impl Default for Sink {
    fn default() -> Self {
        Sink(io::sink())
    }
}

impl Clone for Sink {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl io::Write for Sink {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    #[inline]
    fn write_vectored(&mut self, bufs: &[io::IoSlice<'_>]) -> io::Result<usize> {
        self.0.write_vectored(bufs)
    }

    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl io::Read for Empty {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl io::BufRead for Empty {
    #[inline]
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.0.fill_buf()
    }
    #[inline]
    fn consume(&mut self, n: usize) {
        self.0.consume(n)
    }
}

impl Default for Empty {
    fn default() -> Self {
        Empty(io::empty())
    }
}

impl Clone for Empty {
    fn clone(&self) -> Self {
        Self::default()
    }
}

#[cfg(feature = "snapshot")]
macro_rules! impl_serde_unit {
    ($ty:ty, $name:expr) => {
        impl Serialize for $ty {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_unit_struct($name)
            }
        }

        impl<'de> Deserialize<'de> for $ty {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
                where D: Deserializer<'de>
            {
                struct UnitVisitor;

                impl<'de> Visitor<'de> for UnitVisitor {
                    type Value = $ty;

                    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                        formatter.write_str("unit")
                    }

                    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                        Ok(<$ty>::default())
                    }
                }
                deserializer.deserialize_unit_struct($name, UnitVisitor)
            }
        }
    };
}

#[cfg(feature = "snapshot")]
impl_serde_unit!(Sink, "Sink");
#[cfg(feature = "snapshot")]
impl_serde_unit!(Empty, "Empty");
