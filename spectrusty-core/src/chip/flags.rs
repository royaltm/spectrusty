/*
    Copyright (C) 2020-2023  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
use core::fmt;
use core::convert::TryFrom;
use core::str::FromStr;

#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};

use bitflags::bitflags;

/// Creates `fn from_data(data: u8) -> Self` method for a [::bitflags] type.
///
/// The created function will avoid using `from_bits_truncate()` which can be pretty slow.
#[macro_export]
macro_rules! bitflags_from_data {
    ($bitflags:ty) => {
        impl $bitflags {
            /// Create flags from raw bits in `data` by truncating unused bits.
            ///
            /// Note: avoiding the bitflags from_bits_truncate(), it's too complex when complex flags are defined.
            #[inline]
            pub fn from_data(data: u8) -> Self {
                <$bitflags>::from_bits_retain(data) & <$bitflags>::all()
            }
        }
    };
}
pub use bitflags_from_data;

/// A macro for creating complex mask constants that won't get in the way of bitflags.
#[macro_export]
macro_rules! bitflags_masks {
    (@ pub const $mask:ident = $($flag:ident)|*;) => {
        pub const $mask: Self = Self::from_bits_retain($(Self::$flag.bits())|*);
    };
    (@#[doc = $doc:expr] pub const $mask:ident = $($flag:ident)|*;) => {
        #[doc = $doc] pub const $mask: Self = Self::from_bits_retain($(Self::$flag.bits())|*);
    };
    ($bitflags:ty {$($(#[doc = $doc:expr])? pub const $mask:ident = $($flag:ident)|*;)*}) => {
        impl $bitflags {$(
            bitflags_masks!(@$(#[doc = $doc])? pub const $mask = $($flag)|*;);
        )*}
    };
}
pub use bitflags_masks;

/// A macro for testing created flags, whether all bits up to `$nbits` are defined
/// and if all bitflags are a single bit-flags.
#[macro_export]
macro_rules! test_bitflags_all_bits_defined_no_masks {
    ($ty:ty, $nbits:expr) => {{
        type BITS = <$ty as bitflags::Flags>::Bits;
        let flags = <$ty as bitflags::Flags>::FLAGS;
        let mut last = 0;
        for f in flags.into_iter() {
            eprintln!("{:?}.{:20}: {:#0width$b}", <$ty>::empty(), f.name(), f.value().bits(), width = $nbits+2);
            let bits = f.value().bits();
            assert!(bits == 0 || bits.is_power_of_two());
            assert!(bits >= last);
            last = bits;
        }
        let all: BITS = 1;
        let all = all.checked_shl($nbits - 1).expect("overflowed");
        let all = all | (all - 1);
        assert_eq!(<$ty>::all().bits(), all);
        for bit in 0..$nbits {
            assert_eq!(<$ty>::from_bits_truncate(1 << bit).bits(), 1 << bit);
        }
    }};
}
pub use test_bitflags_all_bits_defined_no_masks;

/// This enum determines the EAR input (bit 6) read from the 0xFE port when there is no EAR input feed.
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    ///
    /// Bits `b3-b4` of the [UlaPortFlags] at the position of `b0-b1`.
    #[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "snapshot", serde(try_from = "u8", into = "u8"))]
    #[derive(Default, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
    pub struct EarMic: u8 {
        const MIC    = 0b01;
        const EAR    = 0b10;
     // const EARMIC = 0b11;
    }
}
bitflags_from_data!(EarMic);
bitflags_masks!(EarMic {
    pub const EARMIC = EAR|MIC;
});

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TryFromU8EarMicError(pub u8);

bitflags! {
    /// ZX Spectrum's [ULA] port flags.
    ///
    /// Any even-numbered I/O port:
    ///
    /// | Dir | b7  | b6  | b5  | b4  | b3  | b2  | b1  | b0  |
    /// |-----|-----|-----|-----|-----|-----|-----|-----|-----|
    /// | IN  |     | EAR |     | KB4 | KB3 | KB2 | KB1 | KB0 |
    /// | OUT |     |     |     | EAR | MIC | BO2 | BO1 | BO0 |
    ///
    ///
    /// Border color: `BO2 * 4 + BO1 * 2 + BO0`.
    ///
    /// The keyboard line is determined by the upper 8 bits of the I/O port:
    ///
    /// | Port | 7f | bf | df | ef | f7 | fb | fd | fe |
    /// |------|----|----|----|----|----|----|----|----|
    /// | KB4  |  B |  H |  Y |  6 |  5 |  T |  G |  V |
    /// | KB3  |  N |  J |  U |  7 |  4 |  R |  F |  C |
    /// | KB2  |  M |  K |  I |  8 |  3 |  E |  D |  X |
    /// | KB1  | SS |  L |  O |  9 |  2 |  W |  S |  Z |
    /// | KB0  | BR | EN |  P |  0 |  1 |  Q |  A | CS |
    ///
    /// 0 - key pressed, 1 - key not pressed.
    ///
    /// [ULA]: https://sinclair.wiki.zxnet.co.uk/wiki/ZX_Spectrum_ULA#ULA_Functions
    #[derive(Default, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
    pub struct UlaPortFlags: u8 {
        const BORDER0       = 0b0000_0001;
        const BORDER1       = 0b0000_0010;
        const BORDER2       = 0b0000_0100;
     // const BORDER_MASK   = 0b0000_0111;
        const MIC_OUT       = 0b0000_1000;
        const EAR_OUT       = 0b0001_0000;
     // const EAR_MIC_MASK  = 0b0001_1000;
     // const KEYBOARD_MASK = 0b0001_1111;
        const UNUSED5       = 0b0010_0000;
        const EAR_IN        = 0b0100_0000;
        const UNUSED7       = 0b1000_0000;
    }
}
bitflags_from_data!(UlaPortFlags);
bitflags_masks!(UlaPortFlags {
    pub const BORDER_MASK = BORDER2|BORDER1|BORDER0;
    pub const EAR_MIC_MASK = MIC_OUT|EAR_OUT;
    pub const KEYBOARD_MASK = EAR_MIC_MASK|BORDER_MASK;
    pub const UNUSED_MASK = UNUSED5|UNUSED7;
});

bitflags! {
    /// ZX Spectrum [128K] / +2 memory control flags and +2A / +3 primary control flags.
    ///
    /// Any I/O port matching: `0xxx xxxx  xxxx xx0x` (`0x7ffd`).
    ///
    /// | Dir | b7  | b6  | b5  | b4  | b3  | b2  | b1  | b0  |
    /// |-----|-----|-----|-----|-----|-----|-----|-----|-----|
    /// | OUT |     |     | LCK | ROM | SCR | RB2 | RB1 | RB0 |
    ///
    /// RAM bank: `RB2 * 4 + RB1 * 2 + RB0`.
    ///
    /// SCR bank: `0 in RAM5, 1 in RAM7`.
    ///
    /// | Start  | Top    | Memory Bank |
    /// |--------|--------|-------------|
    /// | 0x0000 | 0x3FFF | ROM0, ROM1  |
    /// | 0x4000 | 0x7FFF | RAM5        |
    /// | 0x8000 | 0xBFFF | RAM2        |
    /// | 0xC000 | 0XFFFF | RAM0 - RAM7 |
    ///
    /// [128K]: https://sinclair.wiki.zxnet.co.uk/wiki/ZX_Spectrum_128#Paging
    #[derive(Default, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
    pub struct Ula128MemFlags: u8 {
        const RAM_BANK0     = 0b00_0001;
        const RAM_BANK1     = 0b00_0010;
        const RAM_BANK2     = 0b00_0100;
     // const RAM_BANK_MASK = 0b00_0111;
        const SCREEN_BANK   = 0b00_1000;
        const ROM_BANK      = 0b01_0000;
        const LOCK_MMU      = 0b10_0000;
    }
}
bitflags_from_data!(Ula128MemFlags);
bitflags_masks!(Ula128MemFlags {
    pub const RAM_BANK_MASK = RAM_BANK2|RAM_BANK1|RAM_BANK0;
});

bitflags! {
    /// ZX Spectrum [+2A / +3] secondary control flags.
    ///
    /// Any I/O port matching: `0001_xxxx_xxxx_xx0x` (`0x1ffd`).
    ///
    /// | Dir | b7  | b6  | b5  | b4  | b3  | b2  | b1  | b0  |
    /// |-----|-----|-----|-----|-----|-----|-----|-----|-----|
    /// | OUT |     |     |     | PRT | DSK | ROH |     |  0  |
    /// | OUT |     |     |     | PRT | DSK | PL1 | PL0 |  1  |
    ///
    /// In pair with [Ula128MemFlags] I/O port matching: `01xx_xxxx_xxxx_xx0x` (`0x7ffd`).
    ///
    /// | Dir | b7  | b6  | b5  | b4  | b3  | b2  | b1  | b0  |
    /// |-----|-----|-----|-----|-----|-----|-----|-----|-----|
    /// | OUT |     |     | LCK | ROL | SCR | RB2 | RB1 | RB0 |
    ///
    /// RAM bank: `RB2 * 4 + RB1 * 2 + RB0`.
    ///
    /// SCR bank: `0 in RAM5, 1 in RAM7`.
    ///
    /// ROM bank: `ROH * 2 + ROL`.
    ///
    /// Standard paging: `b0 = 0`.
    ///
    /// | Bottom | Top    | Memory Bank |
    /// |--------|--------|-------------|
    /// | 0x0000 | 0x3FFF | ROM0 - ROM3 |
    /// | 0x4000 | 0x7FFF | RAM5        |
    /// | 0x8000 | 0xBFFF | RAM2        |
    /// | 0xC000 | 0XFFFF | RAM0 - RAM7 |
    ///
    /// Special [paging][Ula3Paging]: `b0 = 1`, page layout: `PL1 * 2 + PL0`.
    ///
    /// | Bottom | Top    | PL 0 | PL 1 | PL 2 | PL 3 |
    /// |--------|--------|------|------|------|------|
    /// | 0x0000 | 0x3FFF | RAM0 | RAM4 | RAM4 | RAM4 |
    /// | 0x4000 | 0x7FFF | RAM1 | RAM5 | RAM5 | RAM7 |
    /// | 0x8000 | 0xBFFF | RAM2 | RAM6 | RAM6 | RAM6 |
    /// | 0xC000 | 0XFFFF | RAM3 | RAM7 | RAM3 | RAM3 |
    ///
    /// [+2A / +3]: https://sinclair.wiki.zxnet.co.uk/wiki/ZX_Spectrum_%2B2A/2B,_%2B3/3B#Paging.
    #[derive(Default, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
    pub struct Ula3CtrlFlags: u8 {
        const PAGE_LAYOUT0     = 0b0_0000;
        const EXT_PAGING       = 0b0_0001;
        const PAGE_LAYOUT1     = 0b0_0010;
        const PAGE_LAYOUT2     = 0b0_0100;
     // const PAGE_LAYOUT_MASK = 0b0_0110;
     // const PAGE_LAYOUT3     = 0b0_0110;
        const ROM_BANK_HI      = 0b0_0100;
        const DISC_MOTOR       = 0b0_1000;
        const PRINTER_STROBE   = 0b1_0000;
    }
}
bitflags_from_data!(Ula3CtrlFlags);
bitflags_masks!(Ula3CtrlFlags {
    pub const PAGE_LAYOUT3     = PAGE_LAYOUT2|PAGE_LAYOUT1;
    pub const PAGE_LAYOUT_MASK = PAGE_LAYOUT2|PAGE_LAYOUT1;
});

/// A selection enum for [Ula3CtrlFlags] special paging mode.
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "snapshot", serde(try_from = "u8", into = "u8"))]
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
#[repr(u8)]
pub enum Ula3Paging {
    /// RAM0, RAM1, RAM2, RAM3
    Banks0123 = 0,
    /// RAM4, RAM5, RAM6, RAM7
    Banks4567 = 1,
    /// RAM4, RAM5, RAM6, RAM3
    Banks4563 = 2,
    /// RAM4, RAM7, RAM6, RAM3
    Banks4763 = 3,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TryFromU8Ula3PagingError(pub u8);

bitflags! {
    /// Timex [TC2048/TC2068/TS2068] screen and memory control flags.
    ///
    /// I/O port `0xff`:
    ///
    /// | Dir     | b7  | b6  | b5  | b4  | b3  | b2  | b1  | b0  |
    /// |---------|-----|-----|-----|-----|-----|-----|-----|-----|
    /// | IN/OUT  | ROM | INT | CL2 | CL1 | CL0 | HIR | ATR | SCR |
    ///
    /// Bits `b0-b2` control the screen mode:
    ///
    /// * `SCR` selects the primary screen data offset: `0: 0x0000, 1: 0x2000`.
    /// * `ATR` controls whether normal `8x8` or hi-res `8x1` attributes are in use: `0: 8x8, 1: 8x1`.
    ///   This also affects where the attributes/even-columns data is read from.
    /// * `HIR` controls whether high resolution `512x192` mode is enabled.
    ///
    /// Only some combinations of those bits give proper modes:
    ///
    /// | HIR | ATR | SCR | Mode | Resol.  | Attr | Pixels | Colors | Mode |
    /// |-----|-----|-----|------|---------|------|--------|--------|------|
    /// |  0  |  0  |  0  |  OK  | 256x192 | 8x8  | 0x0000 | 0x1800 |  0   |
    /// |  0  |  0  |  1  |  OK  | 256x192 | 8x8  | 0x2000 | 0x3800 |  1   |
    /// |  0  |  1  |  0  |  OK  | 256x192 | 8x1  | 0x0000 | 0x2000 |  2   |
    /// |  0  |  1  |  1  | Bad  | 256x192 | 8x1  | 0x2000 | 0x2000 |  3   |
    ///
    /// | HIR | ATR | SCR | Mode | Resol.  | Data | Odd    | Even   | Mode |
    /// |-----|-----|-----|------|---------|------|--------|--------|------|
    /// |  1  |  0  |  0  | Bad  | 512x192 | 8x8  | 0x0000 | 0x1800 |  4   |
    /// |  1  |  0  |  1  | Bad  | 512x192 | 8x8  | 0x2000 | 0x3800 |  5   |
    /// |  1  |  1  |  0  |  OK  | 512x192 | 8x1  | 0x0000 | 0x2000 |  6   |
    /// |  1  |  1  |  1  | Bad  | 512x192 | 8x1  | 0x2000 | 0x2000 |  7   |
    ///
    /// `Pixels` and `Colors` columns show offsets in memory from the start of the screen bank
    /// (e.g. `0x4000`), so are `Odd` and `Even`.
    /// `Data` determines how many pixels are affected by each byte read from memory of the even column.
    /// E.g. in very bizzare modes 4 and 5 the even columns of high resolution mode are composed
    /// of normal Spectrum attributes, each byte affecting `8x8` pixels - the pixel line is repeated 8 times.
    ///
    /// The screen `INK` color in hi-res mode is: `CL2 * 4 + CL1 + 2 + CL0`.
    /// The `PAPER` color in hi-res mode is the binary NOT of the `INK` value.
    /// The hi-res color selected is as it was always using the `BRIGHT=1` bit.
    ///
    /// `INT` disables the generation of the timer interrupt if set to `1`.
    ///
    /// `ROM` selects which bank the horizontal MMU should use: `0=DOCK, 1=EX-ROM`.
    /// The memory mapping is affected by I/O `0xf4`, where 8 bits represent each 8KB memory bank:
    /// `0: normal ROM and RAM, 1: DOCK/EX-ROM`.
    ///
    /// [TC2048/TC2068/TS2068]: https://sinclair.wiki.zxnet.co.uk/wiki/Timex_2000_series
    #[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "snapshot", serde(from = "u8", into = "u8"))]
    #[derive(Default, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
    pub struct ScldCtrlFlags: u8 {
        const SCREEN_SECONDARY   = 0b0000_0001;
        const SCREEN_HI_ATTRS    = 0b0000_0010;
     // const SCREEN_SOURCE_MASK = 0b0000_0011;
        const SCREEN_HI_RES      = 0b0000_0100;
     // const SCREEN_MODE_MASK   = 0b0000_0111;
        const HIRES_COLOR0       = 0b0000_1000;
        const HIRES_COLOR1       = 0b0001_0000;
        const HIRES_COLOR2       = 0b0010_0000;
     // const HIRES_COLOR_MASK   = 0b0011_1000;
        const INTR_DISABLED      = 0b0100_0000;
        const MAP_EX_ROM         = 0b1000_0000;
    }
}
bitflags_from_data!(ScldCtrlFlags);
bitflags_masks!(ScldCtrlFlags {
    pub const SCREEN_SOURCE_MASK = SCREEN_HI_ATTRS|SCREEN_SECONDARY;
    pub const SCREEN_MODE_MASK = SCREEN_HI_RES|SCREEN_SOURCE_MASK;
    pub const HIRES_COLOR_MASK = HIRES_COLOR2|HIRES_COLOR1|HIRES_COLOR0;
});

bitflags! {
    /// [ULAplus] register port flags.
    ///
    /// I/O port `0xbf3b`:
    ///
    /// | Dir | b7  | b6  | b5  | b4  | b3  | b2  | b1  | b0  |
    /// |-----|-----|-----|-----|-----|-----|-----|-----|-----|
    /// | OUT | 0   | 0   | PA5 | PA4 | PA3 | PA2 | PA1 | PA0 |
    /// | OUT | 0   | 1   | CL2 | CL1 | CL0 | HIR | ATR | SCR |
    ///
    /// `b6-b7` determine whether a `PALETTE: 0b00` or a `MODE: 0b01` group is selected.
    ///
    /// `b0-b5` in `PALETTE GROUP` selects the palette register. Reading to or writing from 
    /// data I/O `0xff3b` accesses the palette color value: `0bGGG_RRR_BB`.
    /// The blue color component has the lowest phantom bit which is `1` when any of the
    /// `BB` bits is `1` and `0` when `BB=0`.
    /// 
    /// `b0-b5` in `MODE GROUP` (almost) mirror the [ScldCtrlFlags] `b0-b5` bits.
    /// Reading to or writing from data I/O `0xff3b` enables palette or grayscale [ColorMode].
    ///
    /// There is a subtle difference how bits `SCR`, `ATR` and `HIR` select screen modes
    /// via this register:
    ///
    /// | HIR | ATR | SCR | Bank | Resol.  | Attr | Pixels | Colors | Mode |
    /// |-----|-----|-----|------|---------|------|--------|--------|------|
    /// |  0  |  0  |  0  |  5   | 256x192 | 8x8  | 0x0000 | 0x1800 |  0   |
    /// |  0  |  0  |  1  |  5   | 256x192 | 8x8  | 0x2000 | 0x3800 |  1   |
    /// |  0  |  1  |  0  |  5   | 256x192 | 8x1  | 0x0000 | 0x2000 |  2   |
    /// |  0  |  1  |  1  |  7   | 256x192 | 8x1  | 0x0000 | 0x2000 |  3   |
    /// |  1  |  0  |  0  |  7   | 256x192 | 8x8  | 0x0000 | 0x1800 |  4   |
    /// |  1  |  0  |  1  |  7   | 256x192 | 8x8  | 0x2000 | 0x3800 |  5   |
    ///
    /// | HIR | ATR | SCR | Bank | Resol.  | Data | Odd    | Even   | Mode |
    /// |-----|-----|-----|------|---------|------|--------|--------|------|
    /// |  1  |  1  |  0  |  5   | 512x192 | 8x1  | 0x0000 | 0x2000 |  6   |
    /// |  1  |  1  |  1  |  7   | 512x192 | 8x1  | 0x0000 | 0x2000 |  7   |
    ///
    /// Unlike with [ScldCtrlFlags], all combinations give proper screen modes.
    ///
    /// [ULAplus]: https://sinclair.wiki.zxnet.co.uk/wiki/ULAplus
    #[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "snapshot", serde(from = "u8", into = "u8"))]
    #[derive(Default, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
    pub struct UlaPlusRegFlags: u8 {
        const PALETTE_GROUP      = 0b0000_0000;
        const SCREEN_UNUSED      = 0b0000_0001;
        const SCREEN_HI_ATTRS    = 0b0000_0010;
        const SCREEN_HI_RES      = 0b0000_0100;
     // const SCREEN_MODE_MASK   = 0b0000_0111;
        const HIRES_COLOR0       = 0b0000_1000;
        const HIRES_COLOR1       = 0b0001_0000;
        const HIRES_COLOR2       = 0b0010_0000;
     // const HIRES_COLOR_MASK   = 0b0011_1000;
     // const PALETTE_MASK       = 0b0011_1111;
     // const GROUP_MASK         = 0b1100_0000;
        const MODE_GROUP         = 0b0100_0000;
        const RESERVED_GROUP     = 0b1000_0000;
    }
}
bitflags_from_data!(UlaPlusRegFlags);
bitflags_masks!(UlaPlusRegFlags {
    pub const SCREEN_MODE_MASK = SCREEN_HI_RES|SCREEN_HI_ATTRS|SCREEN_UNUSED;
    pub const HIRES_COLOR_MASK = HIRES_COLOR2|HIRES_COLOR1|HIRES_COLOR0;
    pub const GROUP_MASK = MODE_GROUP|RESERVED_GROUP;
    pub const PALETTE_MASK = HIRES_COLOR_MASK|SCREEN_MODE_MASK;
});

bitflags! {
    /// ULAplus data port flags when register port is set to [UlaPlusRegFlags::MODE_GROUP].
    ///
    /// Accessing I/O port `0xff3b` when in `MODE GROUP` determines the current color mode:
    ///
    /// | Dir    | b7 - b2  | b1   | b0  |
    /// |--------|----------|------|-----|
    /// | IN/OUT | RESERVED | GRAY | PAL |
    ///
    /// `PAL`: 1 - palette mode, 0 - legacy color mode.
    /// `GRAY`: 1 - grayscale mode, 0 - color mode.
    #[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "snapshot", serde(try_from = "u8", into = "u8"))]
    #[derive(Default, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
    pub struct ColorMode: u8 {
        const PALETTE   = 0b01;
        const GRAYSCALE = 0b10;
    }
}
bitflags_from_data!(ColorMode);

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
        EarMic::from_bits(earmic).ok_or(TryFromU8EarMicError(earmic))
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
        EarMic::from_bits_retain((flags & UlaPortFlags::EAR_MIC_MASK).bits() >> 3)
    }
}

/****************************** Ula128MemFlags ******************************/

impl Ula128MemFlags {
    /// Returns modified flags with the last memory page RAM bank index set to `bank`.
    pub fn with_last_ram_page_bank(mut self, bank: usize) -> Self {
        self.remove(Ula128MemFlags::RAM_BANK_MASK);
        self.insert(Ula128MemFlags::from_bits_retain(bank as u8) & Ula128MemFlags::RAM_BANK_MASK);
        self
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
        UlaPlusRegFlags::from_bits_retain(
            (self & (ScldCtrlFlags::SCREEN_MODE_MASK
                    |ScldCtrlFlags::HIRES_COLOR_MASK)
            ).bits()
        ) | UlaPlusRegFlags::MODE_GROUP
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
        ScldCtrlFlags::from_bits_retain(flags)
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
        UlaPlusRegFlags::from_bits_retain(flags)
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
        ColorMode::from_bits(color_mode).ok_or(TryFromU8ColorModeError(color_mode))
    }
}

impl From<ColorMode> for u8 {
    fn from(color_mode: ColorMode) -> u8 {
        color_mode.bits()
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_all_bits_defined() {
        test_bitflags_all_bits_defined_no_masks!(EarMic, 2);
        test_bitflags_all_bits_defined_no_masks!(UlaPortFlags, 8);
        test_bitflags_all_bits_defined_no_masks!(Ula128MemFlags, 6);
        test_bitflags_all_bits_defined_no_masks!(Ula3CtrlFlags, 5);
        test_bitflags_all_bits_defined_no_masks!(ScldCtrlFlags, 8);
        test_bitflags_all_bits_defined_no_masks!(UlaPlusRegFlags, 8);
        test_bitflags_all_bits_defined_no_masks!(ColorMode, 2);
    }

    #[test]
    fn earmic_flags_works() {
        assert_eq!(EarMic::from(UlaPortFlags::EAR_MIC_MASK), EarMic::EARMIC);
        assert_eq!(EarMic::from(UlaPortFlags::MIC_OUT), EarMic::MIC);
        assert_eq!(EarMic::from(UlaPortFlags::EAR_OUT), EarMic::EAR);
        assert_eq!(EarMic::from(!UlaPortFlags::EAR_MIC_MASK), EarMic::empty());
    }

    #[test]
    fn ula128mem_flags_works() {
        let flags = (Ula128MemFlags::ROM_BANK).with_last_ram_page_bank(usize::MAX);
        assert_eq!(flags, Ula128MemFlags::ROM_BANK|Ula128MemFlags::RAM_BANK_MASK);
        assert_eq!(flags.last_ram_page_bank(), 7);
        let flags = Ula128MemFlags::empty().with_last_ram_page_bank(usize::MAX);
        assert_eq!(flags, Ula128MemFlags::RAM_BANK_MASK);
        assert_eq!(flags.last_ram_page_bank(), 7);
        let flags = Ula128MemFlags::empty().with_last_ram_page_bank(0);
        assert_eq!(flags, Ula128MemFlags::empty());
        assert_eq!(flags.last_ram_page_bank(), 0);
        let flags = Ula128MemFlags::all().with_last_ram_page_bank(usize::MAX);
        assert_eq!(flags, Ula128MemFlags::all());
        assert_eq!(flags.last_ram_page_bank(), 7);
        let flags = Ula128MemFlags::all().with_last_ram_page_bank(0);
        assert_eq!(flags, Ula128MemFlags::SCREEN_BANK|Ula128MemFlags::ROM_BANK|Ula128MemFlags::LOCK_MMU);
        assert_eq!(flags.last_ram_page_bank(), 0);
        let flags = Ula128MemFlags::all().with_last_ram_page_bank(1);
        assert_eq!(flags, Ula128MemFlags::SCREEN_BANK|Ula128MemFlags::ROM_BANK|Ula128MemFlags::LOCK_MMU|Ula128MemFlags::RAM_BANK0);
        assert_eq!(flags.last_ram_page_bank(), 1);
        let flags = Ula128MemFlags::all().with_last_ram_page_bank(2);
        assert_eq!(flags, Ula128MemFlags::SCREEN_BANK|Ula128MemFlags::ROM_BANK|Ula128MemFlags::LOCK_MMU|Ula128MemFlags::RAM_BANK1);
        assert_eq!(flags.last_ram_page_bank(), 2);
        let flags = Ula128MemFlags::all().with_last_ram_page_bank(3);
        assert_eq!(flags, Ula128MemFlags::SCREEN_BANK|Ula128MemFlags::ROM_BANK|Ula128MemFlags::LOCK_MMU|Ula128MemFlags::RAM_BANK1|Ula128MemFlags::RAM_BANK0);
        assert_eq!(flags.last_ram_page_bank(), 3);
        let flags = Ula128MemFlags::all().with_last_ram_page_bank(4);
        assert_eq!(flags, Ula128MemFlags::SCREEN_BANK|Ula128MemFlags::ROM_BANK|Ula128MemFlags::LOCK_MMU|Ula128MemFlags::RAM_BANK2);
        assert_eq!(flags.last_ram_page_bank(), 4);
    }

    #[test]
    fn ula3ctrl_flags_works() {
        assert_eq!(!Ula3CtrlFlags::PAGE_LAYOUT_MASK, Ula3CtrlFlags::EXT_PAGING|Ula3CtrlFlags::DISC_MOTOR|Ula3CtrlFlags::PRINTER_STROBE);
        assert_eq!(Ula3CtrlFlags::empty().with_special_paging(Ula3Paging::Banks0123), Ula3CtrlFlags::EXT_PAGING|Ula3CtrlFlags::PAGE_LAYOUT0);
        assert_eq!(Ula3CtrlFlags::empty().with_special_paging(Ula3Paging::Banks4567), Ula3CtrlFlags::EXT_PAGING|Ula3CtrlFlags::PAGE_LAYOUT1);
        assert_eq!(Ula3CtrlFlags::empty().with_special_paging(Ula3Paging::Banks4563), Ula3CtrlFlags::EXT_PAGING|Ula3CtrlFlags::PAGE_LAYOUT2);
        assert_eq!(Ula3CtrlFlags::empty().with_special_paging(Ula3Paging::Banks4763), Ula3CtrlFlags::EXT_PAGING|Ula3CtrlFlags::PAGE_LAYOUT3);
        assert_eq!(Ula3CtrlFlags::all().with_special_paging(Ula3Paging::Banks0123), Ula3CtrlFlags::EXT_PAGING|Ula3CtrlFlags::PAGE_LAYOUT0|Ula3CtrlFlags::DISC_MOTOR|Ula3CtrlFlags::PRINTER_STROBE);
        assert_eq!(Ula3CtrlFlags::all().with_special_paging(Ula3Paging::Banks4567), Ula3CtrlFlags::EXT_PAGING|Ula3CtrlFlags::PAGE_LAYOUT1|Ula3CtrlFlags::DISC_MOTOR|Ula3CtrlFlags::PRINTER_STROBE);
        assert_eq!(Ula3CtrlFlags::all().with_special_paging(Ula3Paging::Banks4563), Ula3CtrlFlags::EXT_PAGING|Ula3CtrlFlags::PAGE_LAYOUT2|Ula3CtrlFlags::DISC_MOTOR|Ula3CtrlFlags::PRINTER_STROBE);
        assert_eq!(Ula3CtrlFlags::all().with_special_paging(Ula3Paging::Banks4763), Ula3CtrlFlags::EXT_PAGING|Ula3CtrlFlags::PAGE_LAYOUT3|Ula3CtrlFlags::DISC_MOTOR|Ula3CtrlFlags::PRINTER_STROBE);
        assert_eq!(Ula3CtrlFlags::empty().with_rom_page_bank_hi(0), Ula3CtrlFlags::empty());
        assert_eq!(Ula3CtrlFlags::empty().with_rom_page_bank_hi(1), Ula3CtrlFlags::empty());
        assert_eq!(Ula3CtrlFlags::empty().with_rom_page_bank_hi(2), Ula3CtrlFlags::ROM_BANK_HI);
        assert_eq!(Ula3CtrlFlags::empty().with_rom_page_bank_hi(3), Ula3CtrlFlags::ROM_BANK_HI);
        assert_eq!(Ula3CtrlFlags::empty().with_rom_page_bank_hi(usize::MAX), Ula3CtrlFlags::ROM_BANK_HI);
        assert_eq!(Ula3CtrlFlags::all().with_rom_page_bank_hi(0), Ula3CtrlFlags::PAGE_LAYOUT1|Ula3CtrlFlags::DISC_MOTOR|Ula3CtrlFlags::PRINTER_STROBE);
        assert_eq!(Ula3CtrlFlags::all().with_rom_page_bank_hi(1), Ula3CtrlFlags::PAGE_LAYOUT1|Ula3CtrlFlags::DISC_MOTOR|Ula3CtrlFlags::PRINTER_STROBE);
        assert_eq!(Ula3CtrlFlags::all().with_rom_page_bank_hi(2), Ula3CtrlFlags::ROM_BANK_HI|Ula3CtrlFlags::PAGE_LAYOUT1|Ula3CtrlFlags::DISC_MOTOR|Ula3CtrlFlags::PRINTER_STROBE);
        assert_eq!(Ula3CtrlFlags::all().with_rom_page_bank_hi(3), Ula3CtrlFlags::ROM_BANK_HI|Ula3CtrlFlags::PAGE_LAYOUT1|Ula3CtrlFlags::DISC_MOTOR|Ula3CtrlFlags::PRINTER_STROBE);
        assert_eq!(Ula3CtrlFlags::all().with_rom_page_bank_hi(usize::MAX), Ula3CtrlFlags::ROM_BANK_HI|Ula3CtrlFlags::PAGE_LAYOUT1|Ula3CtrlFlags::DISC_MOTOR|Ula3CtrlFlags::PRINTER_STROBE);
        assert_eq!((Ula3CtrlFlags::ROM_BANK_HI|Ula3CtrlFlags::PAGE_LAYOUT1|Ula3CtrlFlags::DISC_MOTOR|Ula3CtrlFlags::PRINTER_STROBE).rom_page_bank_hi(), 2);
        assert_eq!((Ula3CtrlFlags::PAGE_LAYOUT1|Ula3CtrlFlags::DISC_MOTOR|Ula3CtrlFlags::PRINTER_STROBE).rom_page_bank_hi(), 0);
        assert_eq!(Ula3CtrlFlags::empty().special_paging(), None);
        assert_eq!(Ula3CtrlFlags::all().with_rom_page_bank_hi(0).special_paging(), None);
        assert_eq!((Ula3CtrlFlags::EXT_PAGING|Ula3CtrlFlags::PAGE_LAYOUT0|Ula3CtrlFlags::DISC_MOTOR|Ula3CtrlFlags::PRINTER_STROBE).special_paging(), Some(Ula3Paging::Banks0123));
        assert_eq!((Ula3CtrlFlags::EXT_PAGING|Ula3CtrlFlags::PAGE_LAYOUT1|Ula3CtrlFlags::DISC_MOTOR|Ula3CtrlFlags::PRINTER_STROBE).special_paging(), Some(Ula3Paging::Banks4567));
        assert_eq!((Ula3CtrlFlags::EXT_PAGING|Ula3CtrlFlags::PAGE_LAYOUT2|Ula3CtrlFlags::DISC_MOTOR|Ula3CtrlFlags::PRINTER_STROBE).special_paging(), Some(Ula3Paging::Banks4563));
        assert_eq!((Ula3CtrlFlags::EXT_PAGING|Ula3CtrlFlags::PAGE_LAYOUT3|Ula3CtrlFlags::DISC_MOTOR|Ula3CtrlFlags::PRINTER_STROBE).special_paging(), Some(Ula3Paging::Banks4763));
    }

    #[test]
    fn scld_flags_works() {
        assert_eq!(ScldCtrlFlags::empty().into_plus_mode(), UlaPlusRegFlags::MODE_GROUP);
        assert_eq!(ScldCtrlFlags::all().into_plus_mode(), UlaPlusRegFlags::PALETTE_MASK|UlaPlusRegFlags::MODE_GROUP);
        assert_eq!(ScldCtrlFlags::empty().hires_color_index(), 0);
        assert_eq!(ScldCtrlFlags::all().hires_color_index(), 7);
        assert_eq!(ScldCtrlFlags::HIRES_COLOR0.hires_color_index(), 1);
        assert_eq!(ScldCtrlFlags::HIRES_COLOR1.hires_color_index(), 2);
        assert_eq!(ScldCtrlFlags::HIRES_COLOR2.hires_color_index(), 4);
    }

    #[test]
    fn ula_plus_flags_works() {
        assert_eq!(UlaPlusRegFlags::empty().palette_group(), Some(0));
        assert_eq!(UlaPlusRegFlags::all().palette_group(), None);
        for n in 0..63 {
            assert_eq!(UlaPlusRegFlags::from_bits_retain(n).palette_group(), Some(n));
            assert_eq!(UlaPlusRegFlags::from_bits_retain(n).is_mode_group(), false);
            assert_eq!((UlaPlusRegFlags::from_bits_retain(n)|UlaPlusRegFlags::GROUP_MASK).palette_group(), None);
            assert_eq!((UlaPlusRegFlags::from_bits_retain(n)|UlaPlusRegFlags::GROUP_MASK).is_mode_group(), false);
            assert_eq!((UlaPlusRegFlags::from_bits_retain(n)|UlaPlusRegFlags::MODE_GROUP).palette_group(), None);
            assert_eq!((UlaPlusRegFlags::from_bits_retain(n)|UlaPlusRegFlags::MODE_GROUP).is_mode_group(), true);
            assert_eq!((UlaPlusRegFlags::from_bits_retain(n)|UlaPlusRegFlags::RESERVED_GROUP).palette_group(), None);
            assert_eq!((UlaPlusRegFlags::from_bits_retain(n)|UlaPlusRegFlags::RESERVED_GROUP).is_mode_group(), false);
        }
        assert_eq!(UlaPlusRegFlags::empty().hires_color_index(), 0);
        assert_eq!(UlaPlusRegFlags::all().hires_color_index(), 7);
        assert_eq!(UlaPlusRegFlags::HIRES_COLOR0.hires_color_index(), 1);
        assert_eq!(UlaPlusRegFlags::HIRES_COLOR1.hires_color_index(), 2);
        assert_eq!(UlaPlusRegFlags::HIRES_COLOR2.hires_color_index(), 4);
    }
}
