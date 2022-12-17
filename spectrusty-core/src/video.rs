/*
    Copyright (C) 2020-2022  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
//! # Video API.
pub mod pixel;

use core::str::FromStr;
use core::convert::TryFrom;
use core::fmt::{self, Debug};
use core::ops::{BitAnd, BitOr, Shl, Shr, Range};

#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};

use bitflags::bitflags;

use crate::clock::{Ts, FTs, VideoTs, VFrameTsCounter, MemoryContention};
use crate::chip::UlaPortFlags;

pub use pixel::{Palette, PixelBuffer};

/// A halved count of PAL `pixel lines` (low resolution).
pub const PAL_VC: u32 = 576/2;
/// A halved count of PAL `pixel columns` (low resolution).
pub const PAL_HC: u32 = 704/2;
/// Maximum border size measured in low-resolution pixels.
pub const MAX_BORDER_SIZE: u32 = 6*8;

/// This enum is used to select border size when rendering video frames.
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "snapshot", serde(try_from = "u8", into = "u8"))]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum BorderSize {
    Full    = 6,
    Large   = 5,
    Medium  = 4,
    Small   = 3,
    Tiny    = 2,
    Minimal = 1,
    Nil     = 0
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TryFromUIntBorderSizeError(pub u8);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseBorderSizeError;

/// General-purpose coordinates, used by various video related methods as a return or an argument type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellCoords {
    /// 5 lowest bits: 0 - 31 indicates cell column
    pub column: u8,
    /// INK/PAPER row range is: [0, 192), screen attributes row range is: [0, 24).
    pub row: u8
}

bitflags! {
    /// Bitflags defining ZX Spectrum's border colors.
    #[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "snapshot", serde(try_from = "u8", into = "u8"))]
    #[derive(Default)]
    pub struct BorderColor: u8 {
        const BLACK   = 0b000;
        const BLUE    = 0b001;
        const RED     = 0b010;
        const MAGENTA = 0b011;
        const GREEN   = 0b100;
        const CYAN    = 0b101;
        const YELLOW  = 0b110;
        const WHITE   = 0b111;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TryFromU8BorderColorError(pub u8);

/// An interface for rendering Spectrum's pixel data to frame buffers.
pub trait Video {
    /// The horizontal pixel density.
    ///
    /// This is: 1 for chipsets that can render only low-resolution modes, and 2 for chipsets that
    /// are capable of displaying high-resolution screen modes.
    const PIXEL_DENSITY: u32 = 1;
    /// The type implementing [VideoFrame], that is being used by the chipset emulator.
    type VideoFrame: VideoFrame;
    /// The type implementing [MemoryContention], that is being used by the chipset emulator.
    type Contention: MemoryContention;
    /// Returns the current border color.
    fn border_color(&self) -> BorderColor;
    /// Force sets the border area to the given color.
    fn set_border_color(&mut self, border: BorderColor);
    /// Renders last emulated frame's video data into the provided pixel `buffer`.
    ///
    /// * `pitch` is the number of bytes in a single row of pixel data, including padding between lines.
    /// * `border_size` determines the size of the border rendered around the INK and PAPER area.
    ///
    /// Note that different [BorderSize]s will result in different sizes of the rendered `buffer` area.
    ///
    /// To predetermine the size of the rendered buffer area use [Video::render_size_pixels].
    /// To get the size of the video screen in low-resolution pixels only, call [VideoFrame::screen_size_pixels]
    /// e.g. for an aspect ratio calculation.
    ///
    /// * [PixelBuffer] implementation is used to write pixels into the `buffer`.
    /// * [Palette] implementation is used to create colors from the Spectrum colors.
    ///
    /// **NOTE**: Currently this is a one-time action (per frame), as internal data will be drained
    /// during the rendering. Calling it twice will succeed but the image rendered the second time
    /// will be most probably incorrect due to the missing data.
    fn render_video_frame<'a, B: PixelBuffer<'a>, P: Palette<Pixel=B::Pixel>>(
        &mut self,
        buffer: &'a mut [u8],
        pitch: usize,
        border_size: BorderSize
    );
    /// Returns rendered screen pixel size (horizontal, vertical), including the border area, measured
    /// in pixels depending on [Video::PIXEL_DENSITY].
    ///
    /// The size depends on the given `border_size`.
    fn render_size_pixels(border_size: BorderSize) -> (u32, u32) {
        let (width, height) = Self::VideoFrame::screen_size_pixels(border_size);
        (width * Self::pixel_density(), height)
    }
    /// Returns the horizontal pixel density.
    fn pixel_density() -> u32 {
        Self::PIXEL_DENSITY
    }
    /// Returns the screen bank index of the currently visible screen.
    ///
    /// The screen banks are different from memory banks.
    /// E.g. Spectrum 128k returns `0` for the screen bank which resides in a memory bank 5 and `1`
    /// for the screen bank which resides in a memory bank 7. For 16k/48k Spectrum, this method always
    /// returns `0`.
    fn visible_screen_bank(&self) -> usize { 0 }
    /// Returns the current value of the video T-state counter.
    fn current_video_ts(&self) -> VideoTs;
    /// Modifies the current value of the video T-state counter.
    fn set_video_ts(&mut self, vts: VideoTs);
    /// Returns the current value of the video T-state clock.
    fn current_video_clock(&self) -> VFrameTsCounter<Self::VideoFrame, Self::Contention>;
    /// Returns the temporary video flash attribute state.
    fn flash_state(&self) -> bool;
}
/// A collection of static methods and constants related to video parameters.
/// ```text
///                               - 0
///     +-------------------------+ VSL_BORDER_TOP
///     |                         |
///     |  +-------------------+  | -
///     |  |                   |  | |
///     |  |                   |  |  
///     |  |                   |  | VSL_PIXELS
///     |  |                   |  |  
///     |  |                   |  | |
///     |  +-------------------+  | -
///     |                         |
///     +-------------------------+ VSL_BORDER_BOT
///                               - VSL_COUNT
/// |----- 0 -- HTS_RANGE ---------|
/// |           HTS_COUNT          |
/// ```
pub trait VideoFrame: Copy + Debug {
    /// A range of horizontal T-states, 0 should be where the frame starts.
    const HTS_RANGE: Range<Ts>;
    /// The number of horizontal T-states.
    const HTS_COUNT: Ts = Self::HTS_RANGE.end - Self::HTS_RANGE.start;
    /// The first visible video scan line index of the top border.
    const VSL_BORDER_TOP: Ts;
    /// A range of video scan line indexes where pixel data is being drawn.
    const VSL_PIXELS: Range<Ts>;
    /// The last visible video scan line index of the bottom border.
    const VSL_BORDER_BOT: Ts;
    /// The total number of video scan lines including the beam retrace.
    const VSL_COUNT: Ts;
    /// The total number of T-states per frame.
    const FRAME_TSTATES_COUNT: FTs = Self::HTS_COUNT as FTs * Self::VSL_COUNT as FTs;
    /// A rendered screen border size in pixels depending on the border size selection.
    ///
    /// **NOTE**: The upper and lower border size may be lower than the value returned here
    ///  e.g. in the NTSC video frame.
    fn border_size_pixels(border_size: BorderSize) -> u32 {
        match border_size {
            BorderSize::Full    => MAX_BORDER_SIZE,
            BorderSize::Large   => MAX_BORDER_SIZE -   8,
            BorderSize::Medium  => MAX_BORDER_SIZE - 2*8,
            BorderSize::Small   => MAX_BORDER_SIZE - 3*8,
            BorderSize::Tiny    => MAX_BORDER_SIZE - 4*8,
            BorderSize::Minimal => MAX_BORDER_SIZE - 5*8,
            BorderSize::Nil     => 0
        }
    }
    /// Returns output screen pixel size (horizontal, vertical), including the border area, measured
    /// in low-resolution pixels.
    ///
    /// The size depends on the given `border_size`.
    fn screen_size_pixels(border_size: BorderSize) -> (u32, u32) {
        let border = 2 * Self::border_size_pixels(border_size);
        let w = PAL_HC - 2*MAX_BORDER_SIZE + border;
        let h = (PAL_VC - 2*MAX_BORDER_SIZE + border)
                .min((Self::VSL_BORDER_BOT + 1 - Self::VSL_BORDER_TOP) as u32);
        (w, h)
    }
    /// Returns an iterator of the top border low-resolution scan line indexes.
    fn border_top_vsl_iter(border_size: BorderSize) -> Range<Ts> {
        let border = Self::border_size_pixels(border_size) as Ts;
        let top = (Self::VSL_PIXELS.start - border).max(Self::VSL_BORDER_TOP);
        top..Self::VSL_PIXELS.start
    }
    /// Returns an iterator of the bottom border low-resolution scan line indexes.
    fn border_bot_vsl_iter(border_size: BorderSize) -> Range<Ts> {
        let border = Self::border_size_pixels(border_size) as Ts;
        let bot = (Self::VSL_PIXELS.end + border).min(Self::VSL_BORDER_BOT);
        Self::VSL_PIXELS.end..bot
    }

    /// An iterator for rendering borders.
    type BorderHtsIter: Iterator<Item=Ts>;
    /// Returns an iterator of border latch horizontal T-states.
    fn border_whole_line_hts_iter(border_size: BorderSize) -> Self::BorderHtsIter;
    /// Returns an iterator of left border latch horizontal T-states.
    fn border_left_hts_iter(border_size: BorderSize) -> Self::BorderHtsIter;
    /// Returns an iterator of right border latch horizontal T-states.
    fn border_right_hts_iter(border_size: BorderSize) -> Self::BorderHtsIter;

    /// Returns a horizontal T-state counter after adding an additional T-states required for emulating 
    /// a memory contention, while rendering lines that require reading video memory.
    fn contention(hc: Ts) -> Ts;
    /// Returns an optional floating bus horizontal offset for the given horizontal timestamp.
    fn floating_bus_offset(_hc: Ts) -> Option<u16> {
        None
    }
    /// Returns an optional floating bus screen address (in the screen address space) for the given timestamp.
    ///
    /// The returned screen address range is: [0x0000, 0x1B00).
    #[inline]
    fn floating_bus_screen_address(VideoTs { vc, hc }: VideoTs) -> Option<u16> {
        let line = vc - Self::VSL_PIXELS.start;
        if line >= 0 && vc < Self::VSL_PIXELS.end {
            Self::floating_bus_offset(hc).map(|offs| {
                let y = line as u16;
                let col = (offs >> 3) << 1;
                // println!("got offs: {} col:{} y:{}", offs, col, y);
                match offs & 3 {
                    0 =>          pixel_line_offset(y) + col,
                    1 => 0x1800 + color_line_offset(y) + col,
                    2 => 0x0001 + pixel_line_offset(y) + col,
                    3 => 0x1801 + color_line_offset(y) + col,
                    _ => unsafe { core::hint::unreachable_unchecked() }
                }
            })
        }
        else {
            None
        }
    }
    /// Returns an optional cell coordinates of a "snow effect" interference.
    fn snow_interference_coords(_ts: VideoTs) -> Option<CellCoords> {
        None
    }
    /// Returns `true` if the given scan line index is contended for MREQ (memory request) access.
    ///
    /// This indicates if the contention should be applied during the indicated video scan line.
    #[inline]
    fn is_contended_line_mreq(vsl: Ts) -> bool {
        vsl >= Self::VSL_PIXELS.start && vsl < Self::VSL_PIXELS.end
    }
    /// Returns `true` if the given scan line index is contended for other than MREQ (memory request) access.
    ///
    /// This indicates if the contention should be applied during the indicated video scan line.
    /// Other accesses include IORQ and instruction cycles not requiring memory access.
    #[inline]
    fn is_contended_line_no_mreq(vsl: Ts) -> bool {
        vsl >= Self::VSL_PIXELS.start && vsl < Self::VSL_PIXELS.end
    }
    /// Converts video scan line and horizontal T-state counters to the frame T-state count without any normalization.
    #[inline]
    fn vc_hc_to_tstates(vc: Ts, hc: Ts) -> FTs {
        vc as FTs * Self::HTS_COUNT as FTs + hc as FTs
    }
}

impl From<BorderSize> for &'static str {
    fn from(border: BorderSize) -> &'static str {
        match border {
            BorderSize::Full    => "full",
            BorderSize::Large   => "large",
            BorderSize::Medium  => "medium",
            BorderSize::Small   => "small",
            BorderSize::Tiny    => "tiny",
            BorderSize::Minimal => "minimal",
            BorderSize::Nil     => "none",
        }
    }
}

impl fmt::Display for BorderSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(<&str>::from(*self))
    }
}

impl std::error::Error for ParseBorderSizeError {}

impl fmt::Display for ParseBorderSizeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("unrecognized border size")
    }
}

impl FromStr for BorderSize {
    type Err = ParseBorderSizeError;
    /// Parses a single word describing border size using case insensitive matching
    /// or a single digit from 0 to 6.
    fn from_str(name: &str) -> Result<Self, Self::Err> {
        if name.eq_ignore_ascii_case("full") ||
           name.eq_ignore_ascii_case("maxi") ||
           name.eq_ignore_ascii_case("max") {
            Ok(BorderSize::Full)
        }
        else if name.eq_ignore_ascii_case("large") ||
                name.eq_ignore_ascii_case("big") {
            Ok(BorderSize::Large)
        }
        else if name.eq_ignore_ascii_case("medium") ||
                name.eq_ignore_ascii_case("medi")   ||
                name.eq_ignore_ascii_case("med") {
            Ok(BorderSize::Medium)
        }
        else if name.eq_ignore_ascii_case("small") ||
                name.eq_ignore_ascii_case("sm") {
            Ok(BorderSize::Small)
        }
        else if name.eq_ignore_ascii_case("tiny") {
            Ok(BorderSize::Tiny)
        }
        else if name.eq_ignore_ascii_case("minimal") ||
                name.eq_ignore_ascii_case("mini")    ||
                name.eq_ignore_ascii_case("min") {
            Ok(BorderSize::Minimal)
        }
        else if name.eq_ignore_ascii_case("none") ||
                name.eq_ignore_ascii_case("nil")  ||
                name.eq_ignore_ascii_case("null") ||
                name.eq_ignore_ascii_case("zero") {
            Ok(BorderSize::Nil)
        }
        else {
            u8::from_str(name).map_err(|_| ParseBorderSizeError)
            .and_then(|size|
                BorderSize::try_from(size).map_err(|_| ParseBorderSizeError)
            )
        }
    }
}

impl From<BorderSize> for u8 {
    fn from(border: BorderSize) -> u8 {
        border as u8
    }
}

impl std::error::Error for TryFromUIntBorderSizeError {}

impl fmt::Display for TryFromUIntBorderSizeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "converted integer ({}) out of range for `BorderSize`", self.0)
    }
}

impl TryFrom<u8> for BorderSize {
    type Error = TryFromUIntBorderSizeError;
    fn try_from(border: u8) -> Result<Self, Self::Error> {
        use BorderSize::*;
        Ok(match border {
            6 => Full,
            5 => Large,
            4 => Medium,
            3 => Small,
            2 => Tiny,
            1 => Minimal,
            0 => Nil,
            _ => return Err(TryFromUIntBorderSizeError(border))
        })
    }
}

impl std::error::Error for TryFromU8BorderColorError {}

impl fmt::Display for TryFromU8BorderColorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "converted integer ({}) out of range for `BorderColor`", self.0)
    }
}

impl TryFrom<u8> for BorderColor {
    type Error = TryFromU8BorderColorError;
    fn try_from(color: u8) -> core::result::Result<Self, Self::Error> {
        BorderColor::from_bits(color).ok_or(TryFromU8BorderColorError(color))
    }
}

impl From<UlaPortFlags> for BorderColor {
    #[inline]
    fn from(flags: UlaPortFlags) -> Self {
        BorderColor::from_bits_truncate((flags & UlaPortFlags::BORDER_MASK).bits())
    }
}

impl From<BorderColor> for u8 {
    fn from(color: BorderColor) -> u8 {
        color.bits()
    }
}

/// Returns an offset into INK/PAPER bitmap memory of the given vertical coordinate `y` [0, 192) (0 on top).
#[inline(always)]
pub fn pixel_line_offset<T>(y: T) -> T
    where T: Copy + From<u16> + BitAnd<Output=T> + Shl<u16, Output=T> + BitOr<Output=T>
{
    (y & T::from(0b0000_0111) ) << 8 |
    (y & T::from(0b0011_1000) ) << 2 |
    (y & T::from(0b1100_0000) ) << 5
}

/// Returns an offset into attributes memory of the given vertical coordinate `y` [0, 192) (0 on top).
#[inline(always)]
pub fn color_line_offset<T>(y: T) -> T
    where T: Copy + From<u16> + Shr<u16, Output=T> + Shl<u16, Output=T>
{
    (y >> 3) << 5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_offsets_works() {
        assert_eq!(pixel_line_offset(0usize), 0usize);
        assert_eq!(pixel_line_offset(1usize), 256usize);
        assert_eq!(pixel_line_offset(8usize), 32usize);
        assert_eq!(color_line_offset(0usize), 0usize);
        assert_eq!(color_line_offset(1usize), 0usize);
        assert_eq!(color_line_offset(8usize), 32usize);
        assert_eq!(color_line_offset(191usize), 736usize);
    }
}
