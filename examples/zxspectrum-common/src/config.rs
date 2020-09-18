/*
    zxspectrum-common: High-level ZX Spectrum emulator library example.
    Copyright (C) 2020  Rafal Michalski

    For the full copyright notice, see the lib.rs file.
*/
use core::convert::TryFrom;
use core::iter::FromIterator;
use core::ops::{Deref, DerefMut};
use core::any::TypeId;
use core::fmt;
use core::str::FromStr;
use std::collections::hash_map::Entry;

use serde::{Serialize, Deserialize};

/// An enum for determining PSG D/A conversion function.
#[derive(Clone, Copy, Debug, PartialEq)]
#[derive(Serialize, Deserialize)]
pub enum AyAmpSelect {
    Spec,
    Fuse,
}

/// An enum for determining PSG channel mixing.
#[derive(Clone, Copy, Debug, PartialEq)]
#[derive(Serialize, Deserialize)]
pub enum AyChannelsMode {
    ABC,
    ACB,
    BAC,
    BCA,
    CAB,
    CBA,
    Mono
}

/// An enum for determining mode of de-interlacing video frames.
#[derive(Clone, Copy, Debug, PartialEq)]
#[derive(Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
#[repr(u8)]
pub enum InterlaceMode {
    Disabled        = 0,
    OddFramesFirst  = 1,
    EvenFramesFirst = 2,
}

/// A wrapper type for the dynamic device index.
///
/// It maps the concrete device [TypeId] to the index position of the attached dynamic device.
#[derive(Default, Debug, Clone)]
pub struct DeviceIndex(fnv::FnvHashMap<TypeId, usize>);

impl Default for AyAmpSelect {
    fn default() -> Self {
        AyAmpSelect::Spec
    }
}

impl From<AyAmpSelect> for &str {
    fn from(select: AyAmpSelect) -> Self {
        match select {
            AyAmpSelect::Spec  => "Spec",
            AyAmpSelect::Fuse  => "Fuse",
        }
    }
}

impl fmt::Display for AyAmpSelect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        <&str>::from(*self).fmt(f)
    }
}

impl FromStr for AyAmpSelect {
    type Err = &'static str;

    fn from_str(channels: &str) -> Result<Self, Self::Err> {
        match channels {
            "Spec"|"spec" => Ok(AyAmpSelect::Spec),
            "Fuse"|"fuse" => Ok(AyAmpSelect::Fuse),
            _ => Err("Unrecognized AY-3-891x D/A amplitude conversion")
        }
    }
}

impl Default for AyChannelsMode {
    fn default() -> Self {
        AyChannelsMode::ACB
    }
}

impl AyChannelsMode {
    /// Returns `true` if `self` is [AyChannelsMode::Mono]. Otherwise returns `false`.
    pub fn is_mono(self) -> bool {
        AyChannelsMode::Mono == self
    }
}

impl From<AyChannelsMode> for [usize;3] {
    fn from(channels: AyChannelsMode) -> Self {
        use AyChannelsMode::*;
        match channels {
                  // A, B, C -> 0: left, 1: right, 2: center
            ABC  => [0, 2, 1],
            ACB  => [0, 1, 2],
            BAC  => [2, 0, 1],
            BCA  => [1, 0, 2],
            CAB  => [2, 1, 0],
            CBA  => [1, 2, 0],
            Mono => [2, 2, 2]
        }
    }
}

impl From<AyChannelsMode> for &str {
    fn from(channels: AyChannelsMode) -> Self {
        use AyChannelsMode::*;
        match channels {
            ABC  => "ABC",
            ACB  => "ACB",
            BAC  => "BAC",
            BCA  => "BCA",
            CAB  => "CAB",
            CBA  => "CBA",
            Mono => "mono"
        }
    }
}

impl fmt::Display for AyChannelsMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        <&str>::from(*self).fmt(f)
    }
}

impl FromStr for AyChannelsMode {
    type Err = &'static str;

    fn from_str(channels: &str) -> Result<Self, Self::Err> {
        match channels {
            "ABC"  => Ok(AyChannelsMode::ABC),
            "ACB"  => Ok(AyChannelsMode::ACB),
            "BAC"  => Ok(AyChannelsMode::BAC),
            "BCA"  => Ok(AyChannelsMode::BCA),
            "CAB"  => Ok(AyChannelsMode::CAB),
            "CBA"  => Ok(AyChannelsMode::CBA),
            "mono"|"Mono"|"MONO" => Ok(AyChannelsMode::Mono),
            _ => Err("Unrecognized AY-3-891x channel mode")
        }
    }
}

impl Default for InterlaceMode {
    fn default() -> Self {
        InterlaceMode::Disabled
    }
}

impl From<InterlaceMode> for u8 {
    fn from(mode: InterlaceMode) -> u8 {
        mode as u8
    }
}

impl TryFrom<u8> for InterlaceMode {
    type Error = &'static str;
    fn try_from(mode: u8) -> Result<Self, Self::Error> {
        match mode {
            0 => Ok(InterlaceMode::Disabled),
            1 => Ok(InterlaceMode::OddFramesFirst),
            2 => Ok(InterlaceMode::EvenFramesFirst),
            _ => Err("Unknown interlace mode")
        }
    }
}

impl From<InterlaceMode> for &str {
    fn from(mode: InterlaceMode) -> Self {
        match mode {
            InterlaceMode::Disabled        => "Disabled",
            InterlaceMode::OddFramesFirst  => "Odd 1st",
            InterlaceMode::EvenFramesFirst => "Even 1st",
        }
    }
}

impl fmt::Display for InterlaceMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        <&str>::from(*self).fmt(f)
    }
}

impl InterlaceMode {
    /// Returns `true` if the de-interlacing is enabled. Otherwise returns `false`.
    #[inline]
    pub fn is_enabled(self) -> bool {
        self != InterlaceMode::Disabled
    }
}

impl DeviceIndex {
    /// Returns `true` if a device of type `D` is present. Otherwise returns `false`.
    pub fn has_device<D: 'static>(&self) -> bool {
        self.0.contains_key(&TypeId::of::<D>())
    }

    /// Returns index position of a device of type `D`, if the device is present.
    pub fn get_device_index<D: 'static>(&self) -> Option<usize> {
        self.0.get(&TypeId::of::<D>()).copied()
    }

    /// Gets the specified (by its type) device's hash map [Entry] for in-place manipulation.
    pub fn device_entry<D: 'static>(&mut self) -> Entry<'_, TypeId, usize> {
        self.0.entry(TypeId::of::<D>())
    }

    /// Removes a device of type `D` if present and returns its index position.
    pub fn remove_device_index<D: 'static>(&mut self) -> Option<usize> {
        self.0.remove(&TypeId::of::<D>())
    }

    /// Inserts a device of type `D` and returns its previous index position if 
    /// a device of the same type has been previously present.
    pub fn insert_device_index<D: 'static>(&mut self, index: usize) -> Option<usize> {
        self.0.insert(TypeId::of::<D>(), index)
    }
}

impl Deref for DeviceIndex {
    type Target = fnv::FnvHashMap<TypeId, usize>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for DeviceIndex {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl FromIterator<(TypeId, usize)> for DeviceIndex {
    fn from_iter<T: IntoIterator<Item = (TypeId, usize)>>(iter: T) -> DeviceIndex {
        DeviceIndex(fnv::FnvHashMap::from_iter(iter))
    }
}
