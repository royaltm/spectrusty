/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
use core::fmt;
use core::convert::TryFrom;
use core::str::FromStr;

#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};

use bitflags::bitflags;

/// This enum determines the EAR input (bit 6) read from the 0xFE port when there is no EAR input feed.
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadEarMode {
    /// Issue 3 keyboard behavior - EAR input is 1 when EAR output is 1
    Issue3,
    /// Issue 2 keyboard behavior - EAR input is 1 when either EAR or MIC output is 1
    Issue2,
    /// Always clear - EAR input is always 0
    Clear,
    /// Always set - EAR input is always 1
    Set
}

#[derive(Clone, Debug)]
pub struct ParseReadEarModeError;

bitflags! {
    /// This type represents packed EAR and MIC output data.
    #[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "snapshot", serde(try_from = "u8", into = "u8"))]
    #[derive(Default)]
    pub struct EarMic: u8 {
        const MIC    = 0b01;
        const EAR    = 0b10;
        const EARMIC = 0b11;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TryFromU8EarMicError(pub u8);

bitflags! {
    /// ZX Spectrum's ULA port flags.
    #[derive(Default)]
    pub struct UlaPortFlags: u8 {
        const BORDER_MASK   = 0b0000_0111;
        const EAR_MIC_MASK  = 0b0001_1000;
        const MIC_OUT       = 0b0000_1000;
        const EAR_OUT       = 0b0001_0000;
        const KEYBOARD_MASK = 0b0001_1111;
        const EAR_IN        = 0b0100_0000;
        const UNUSED_MASK   = 0b1010_0000;
    }
}

bitflags! {
    /// ZX Spectrum 128K / +2 memory control flags and +2A / +3 primary control flags.
    #[derive(Default)]
    pub struct Ula128MemFlags: u8 {
        const RAM_BANK_MASK = 0b00_0111;
        const SCREEN_BANK   = 0b00_1000;
        const ROM_BANK      = 0b01_0000;
        const LOCK_MMU      = 0b10_0000;
    }
}

bitflags! {
    /// ZX Spectrum +2A / +3 secondary control flags.
    #[derive(Default)]
    pub struct Ula3CtrlFlags: u8 {
        const EXT_PAGING       = 0b0_0001;
        const PAGE_LAYOUT_MASK = 0b0_0110;
        const ROM_BANK_HI      = 0b0_0100;
        const DISC_MOTOR       = 0b0_1000;
        const PRINTER_STROBE   = 0b1_0000;
        const PAGE_LAYOUT0     = 0b0_0000;
        const PAGE_LAYOUT1     = 0b0_0010;
        const PAGE_LAYOUT2     = 0b0_0100;
        const PAGE_LAYOUT3     = 0b0_0110;
    }
}

#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "snapshot", serde(try_from = "u8", into = "u8"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Ula3Paging {
    Banks0123 = 0,
    Banks4567 = 1,
    Banks4563 = 2,
    Banks4763 = 3,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TryFromU8Ula3PagingError(pub u8);

bitflags! {
    /// Timex TC2048/TC2068/TS2068 screen and memory control flags.
    #[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "snapshot", serde(from = "u8", into = "u8"))]
    #[derive(Default)]
    pub struct ScldCtrlFlags: u8 {
        const SCREEN_MODE_MASK   = 0b0000_0111;
        const SCREEN_SOURCE_MASK = 0b0000_0011;
        const SCREEN_SECONDARY   = 0b0000_0001;
        const SCREEN_HI_ATTRS    = 0b0000_0010;
        const SCREEN_HI_RES      = 0b0000_0100;
        const HIRES_COLOR_MASK   = 0b0011_1000;
        const INTR_DISABLED      = 0b0100_0000;
        const MAP_EX_ROM         = 0b1000_0000;
    }
}

bitflags! {
    /// ULAplus register port flags.
    #[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "snapshot", serde(from = "u8", into = "u8"))]
    #[derive(Default)]
    pub struct UlaPlusRegFlags: u8 {
        const SCREEN_MODE_MASK   = 0b0000_0111;
        const SCREEN_HI_ATTRS    = 0b0000_0010;
        const SCREEN_HI_RES      = 0b0000_0100;
        const HIRES_COLOR_MASK   = 0b0011_1000;
        const PALETTE_MASK       = 0b0011_1111;
        const GROUP_MASK         = 0b1100_0000;
        const MODE_GROUP         = 0b0100_0000;
        const PALETTE_GROUP      = 0b0000_0000;
    }
}

bitflags! {
    /// ULAplus data port flags when register port is set to [UlaPlusRegFlags::MODE_GROUP].
    #[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "snapshot", serde(try_from = "u8", into = "u8"))]
    #[derive(Default)]
    pub struct ColorMode: u8 {
        const PALETTE   = 0b001;
        const GRAYSCALE = 0b010;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TryFromU8ColorModeError(pub u8);

/****************************** ReadEarMode ******************************/

impl From<ReadEarMode> for &str {
    fn from(mode: ReadEarMode) -> Self {
        match mode {
            ReadEarMode::Issue3 => "Issue 3",
            ReadEarMode::Issue2 => "Issue 2",
            ReadEarMode::Clear  => "Clear",
            ReadEarMode::Set    => "Set"
        }
    }
}

impl fmt::Display for ReadEarMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        <&str>::from(*self).fmt(f)
    }
}

impl std::error::Error for ParseReadEarModeError {}

impl fmt::Display for ParseReadEarModeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cannot parse `ReadEarMode`: unrecognized string")
    }
}

impl FromStr for ReadEarMode {
    type Err = ParseReadEarModeError;
    fn from_str(mode: &str) -> core::result::Result<Self, Self::Err> {
        if mode.eq_ignore_ascii_case("issue 3") {
            Ok(ReadEarMode::Issue3)
        }
        else if mode.eq_ignore_ascii_case("issue 2") {
            Ok(ReadEarMode::Issue2)
        }
        else if mode.eq_ignore_ascii_case("clear") {
            Ok(ReadEarMode::Clear)
        }
        else if mode.eq_ignore_ascii_case("set") {
            Ok(ReadEarMode::Set)
        }
        else {
            Err(ParseReadEarModeError)
        }
    }
}

/****************************** EarMic ******************************/

impl std::error::Error for TryFromU8EarMicError {}

impl fmt::Display for TryFromU8EarMicError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "converted integer (0x{:x}) contains extraneous bits for `EarMic`", self.0)
    }
}

impl TryFrom<u8> for EarMic {
    type Error = TryFromU8EarMicError;
    fn try_from(earmic: u8) -> core::result::Result<Self, Self::Error> {
        EarMic::from_bits(earmic).ok_or_else(|| TryFromU8EarMicError(earmic))
    }
}

impl From<EarMic> for u8 {
    fn from(earmic: EarMic) -> u8 {
        earmic.bits()
    }
}

/****************************** UlaPortFlags ******************************/

impl From<UlaPortFlags> for EarMic {
    #[inline]
    fn from(flags: UlaPortFlags) -> Self {
        EarMic::from_bits_truncate((flags & UlaPortFlags::EAR_MIC_MASK).bits() >> 3)
    }
}

/****************************** Ula128MemFlags ******************************/

impl Ula128MemFlags {
    /// Returns modified flags with the last memory page RAM bank index set to `bank`.
    pub fn with_last_ram_page_bank(self, bank: usize) -> Self {
        (self & !Ula128MemFlags::RAM_BANK_MASK) |
        (Ula128MemFlags::from_bits_truncate(bank as u8) & Ula128MemFlags::RAM_BANK_MASK)
    }
    /// Returns a RAM bank index mapped at the last memory page.
    pub fn last_ram_page_bank(self) -> usize {
        (self & Ula128MemFlags::RAM_BANK_MASK).bits().into()
    }
    /// Returns a ROM bank index mapped at the first memory page.
    pub fn rom_page_bank(self) -> usize {
        self.intersects(Ula128MemFlags::ROM_BANK).into()
    }
    /// Returns `true` if a shadow screen bank bit is 1. Otherwise returns `false`.
    pub fn is_shadow_screen(self) -> bool {
        self.intersects(Ula128MemFlags::SCREEN_BANK)
    }
    /// Returns `true` if a mmu lock bit is 1. Otherwise returns `false`.
    pub fn is_mmu_locked(self) -> bool {
        self.intersects(Ula128MemFlags::LOCK_MMU)
    }
}

/****************************** Ula3CtrlFlags ******************************/

impl Ula3CtrlFlags {
    /// Returns modified flags with the special paging enabled and its layout set to the specified value.
    pub fn with_special_paging(self, paging: Ula3Paging) -> Self {
        ((self | Ula3CtrlFlags::EXT_PAGING) & !Ula3CtrlFlags::PAGE_LAYOUT_MASK)
        | match paging {
            Ula3Paging::Banks0123 => Ula3CtrlFlags::PAGE_LAYOUT0,
            Ula3Paging::Banks4567 => Ula3CtrlFlags::PAGE_LAYOUT1,
            Ula3Paging::Banks4563 => Ula3CtrlFlags::PAGE_LAYOUT2,
            Ula3Paging::Banks4763 => Ula3CtrlFlags::PAGE_LAYOUT3,
        }
    }
    /// Returns modified flags with the special paging disabled and with the rom bank high flag set from
    /// the `rom_bank` bit1.
    pub fn with_rom_page_bank_hi(mut self, rom_bank: usize) -> Self {
        self.remove(Ula3CtrlFlags::EXT_PAGING);
        self.set(Ula3CtrlFlags::ROM_BANK_HI, rom_bank & 2 != 0);
        self
    }
    /// Returns `true` if a special paging is enabled. Otherwise returns `false`.
    pub fn has_special_paging(self) -> bool {
        self.intersects(Ula3CtrlFlags::EXT_PAGING)
    }
    /// Returns a special paging layout if it is enabled.
    pub fn special_paging(self) -> Option<Ula3Paging> {
        if self.has_special_paging() {
            Some(match self & Ula3CtrlFlags::PAGE_LAYOUT_MASK {
                Ula3CtrlFlags::PAGE_LAYOUT0 => Ula3Paging::Banks0123,
                Ula3CtrlFlags::PAGE_LAYOUT1 => Ula3Paging::Banks4567,
                Ula3CtrlFlags::PAGE_LAYOUT2 => Ula3Paging::Banks4563,
                Ula3CtrlFlags::PAGE_LAYOUT3 => Ula3Paging::Banks4763,
                _ => unreachable!()
            })
        }
        else {
            None
        }
    }
    /// Returns a bit 1 value of rom bank index mapped at the first RAM page.
    ///
    /// The complete rom page bank index can be obtained by bitwise ORing the returned value with
    /// the result from [Ula128MemFlags::rom_page_bank].
    pub fn rom_page_bank_hi(self) -> usize {
        ((self & Ula3CtrlFlags::ROM_BANK_HI).bits() >> 1).into()
    }
    /// Returns `true` if a disc motor bit is 1. Otherwise returns `false`.
    pub fn is_disc_motor_on(self) -> bool {
        self.intersects(Ula3CtrlFlags::DISC_MOTOR)
    }
    /// Returns `true` if a printer strobe bit is 1. Otherwise returns `false`.
    pub fn is_printer_strobe_on(self) -> bool {
        self.intersects(Ula3CtrlFlags::PRINTER_STROBE)
    }
}

/****************************** Ula3Paging ******************************/

impl From<Ula3Paging> for u8 {
    fn from(paging: Ula3Paging) -> u8 {
        paging as u8
    }
}

impl std::error::Error for TryFromU8Ula3PagingError {}

impl fmt::Display for TryFromU8Ula3PagingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "converted integer ({}) out of range for `Ula3Paging`", self.0)
    }
}

impl TryFrom<u8> for Ula3Paging {
    type Error = TryFromU8Ula3PagingError;
    fn try_from(bank: u8) -> core::result::Result<Self, Self::Error> {
        Ok(match bank {
            0 => Ula3Paging::Banks0123,
            1 => Ula3Paging::Banks4567,
            2 => Ula3Paging::Banks4563,
            3 => Ula3Paging::Banks4763,
            _ => return Err(TryFromU8Ula3PagingError(bank))
        })
    }
}

impl Ula3Paging {
    const RAM_BANKS0: [u8;4] = [0, 1, 2, 3];
    const RAM_BANKS1: [u8;4] = [4, 5, 6, 7];
    const RAM_BANKS2: [u8;4] = [4, 5, 6, 3];
    const RAM_BANKS3: [u8;4] = [4, 7, 6, 3];
    /// Returns a reference to an array of RAM bank indexes for a current paging variant layout.
    pub fn ram_banks(self) -> &'static [u8] {
        match self {
            Ula3Paging::Banks0123 => &Self::RAM_BANKS0,
            Ula3Paging::Banks4567 => &Self::RAM_BANKS1,
            Ula3Paging::Banks4563 => &Self::RAM_BANKS2,
            Ula3Paging::Banks4763 => &Self::RAM_BANKS3,
        }
    }
    /// Returns an iterator of tuples of RAM bank indexes with memory page indexes for a current
    /// paging variant layout.
    pub fn ram_banks_with_pages_iter(self) -> impl Iterator<Item=(usize, u8)> {
        self.ram_banks().iter().map(|&bank| bank as usize).zip(0..4)
    }
}

/****************************** ScldCtrlFlags ******************************/

impl ScldCtrlFlags {
    /// Returns scren mode with hi-res colors from `flags` as ULAplus register flags in mode group.
    #[inline]
    pub fn into_plus_mode(self) -> UlaPlusRegFlags {
        UlaPlusRegFlags::from_bits_truncate(
            (self & (ScldCtrlFlags::SCREEN_MODE_MASK
                     |ScldCtrlFlags::HIRES_COLOR_MASK)
            ).bits()
        )|UlaPlusRegFlags::MODE_GROUP
    }
    /// Returns a color index from high resolution color mask: [0, 7].
    #[inline]
    pub fn hires_color_index(self) -> u8 {
        (self & ScldCtrlFlags::HIRES_COLOR_MASK).bits() >> 3
    }
    /// Returns `true` if a map ex rom bit is 1. Otherwise returns `false`.
    #[inline]
    pub fn is_map_ex_rom(self) -> bool {
        self.intersects(ScldCtrlFlags::MAP_EX_ROM)
    }
    /// Returns `true` if a interrupt disabled bit is 1. Otherwise returns `false`.
    #[inline]
    pub fn is_intr_disabled(self) -> bool {
        self.intersects(ScldCtrlFlags::INTR_DISABLED)
    }
    /// Returns `true` if a screen high resolution bit is 1. Otherwise returns `false`.
    #[inline]
    pub fn is_screen_hi_res(self) -> bool {
        self.intersects(ScldCtrlFlags::SCREEN_HI_RES)
    }
    /// Returns `true` if a screen high color bit is 1. Otherwise returns `false`.
    #[inline]
    pub fn is_screen_hi_attrs(self) -> bool {
        self.intersects(ScldCtrlFlags::SCREEN_HI_ATTRS)
    }
    /// Returns `true` if a screen secondary bit is 1. Otherwise returns `false`.
    #[inline]
    pub fn is_screen_secondary(self) -> bool {
        self.intersects(ScldCtrlFlags::SCREEN_SECONDARY)
    }
}

impl From<ScldCtrlFlags> for u8 {
    #[inline]
    fn from(flags: ScldCtrlFlags) -> u8 {
        flags.bits()
    }
}

impl From<u8> for ScldCtrlFlags {
    #[inline]
    fn from(flags: u8) -> ScldCtrlFlags {
        ScldCtrlFlags::from_bits_truncate(flags)
    }
}

/****************************** UlaPlusRegFlags ******************************/

impl UlaPlusRegFlags {
    /// Returns an `index` [0, 63] to the palette entry if `self` selects a palette group.
    pub fn palette_group(self) -> Option<u8> {
        if (self & UlaPlusRegFlags::GROUP_MASK) == UlaPlusRegFlags::PALETTE_GROUP {
            return Some((self & UlaPlusRegFlags::PALETTE_MASK).bits())
        }
        None
    }

    /// Returns an `true` if `self` selects a mode group.
    pub fn is_mode_group(self) -> bool {
        (self & UlaPlusRegFlags::GROUP_MASK) == UlaPlusRegFlags::MODE_GROUP
    }
    /// Returns a color index from high resolution color mask: [0, 7].
    #[inline]
    pub fn hires_color_index(self) -> u8 {
        (self & UlaPlusRegFlags::HIRES_COLOR_MASK).bits() >> 3
    }
    /// Returns `true` if a screen high resolution bit is 1. Otherwise returns `false`.
    #[inline]
    pub fn is_screen_hi_res(self) -> bool {
        self.intersects(UlaPlusRegFlags::SCREEN_HI_RES)
    }
}

impl From<UlaPlusRegFlags> for u8 {
    #[inline]
    fn from(flags: UlaPlusRegFlags) -> u8 {
        flags.bits()
    }
}

impl From<u8> for UlaPlusRegFlags {
    #[inline]
    fn from(flags: u8) -> UlaPlusRegFlags {
        UlaPlusRegFlags::from_bits_truncate(flags)
    }
}

/****************************** ColorMode ******************************/

impl std::error::Error for TryFromU8ColorModeError {}

impl fmt::Display for TryFromU8ColorModeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "converted integer (0x{:x}) contains extraneous bits for `ColorMode`", self.0)
    }
}

impl TryFrom<u8> for ColorMode {
    type Error = TryFromU8ColorModeError;
    fn try_from(color_mode: u8) -> core::result::Result<Self, Self::Error> {
        ColorMode::from_bits(color_mode).ok_or_else(|| TryFromU8ColorModeError(color_mode))
    }
}

impl From<ColorMode> for u8 {
    fn from(color_mode: ColorMode) -> u8 {
        color_mode.bits()
    }
}
