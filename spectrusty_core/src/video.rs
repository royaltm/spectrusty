//! # Video API
pub mod pixel;

use core::convert::{TryInto, TryFrom};
use core::fmt::{self, Debug};
use core::ops::{BitAnd, BitOr, Shl, Shr, Range};
use crate::clock::{Ts, FTs, VideoTs};

pub use pixel::{Palette, PixelBuffer};

/// A halved count of PAL `pixel lines` (low resolution).
pub const PAL_VC: u32 = 576/2;
/// A halved count of PAL `pixel columns` (low resolution).
pub const PAL_HC: u32 = 704/2;
/// Maximum border size measured in low resolution pixels.
pub const MAX_BORDER_SIZE: u32 = 6*8;

/// An enum used to select border size when rendering video frames.
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BorderSizeTryFromError(pub u8);

/// An interface for renderering Spectrum's pixel data to frame buffers.
pub trait Video {
    /// The [VideoFrame] implementation used by the chipset emulator.
    type VideoFrame: VideoFrame;
    /// Returns the current border color number [0, 7].
    fn border_color(&self) -> u8;
    /// Force sets the border area to the given color number [0, 7].
    fn set_border_color(&mut self, border: u8);
    /// Renders last emulated frame's video data into the provided frame `buffer`.
    ///
    /// * `pitch` is the number of bytes in a single row of pixel data, including padding between lines.
    /// * `border_size` determines the size of border rendered around the INK and PAPER area.
    ///
    /// Note that different [BorderSize]s will result in a different size of the rendered buffer area.
    ///
    /// To predetermine the resolution of the rendered buffer area use [VideoFrame::screen_size_pixels].
    ///
    /// * [PixelBuffer] implementation is used to write pixels into the `buffer`.
    /// * [Palette] implementation is used to create colors from the Spectrum's color index.
    fn render_video_frame<'a, B: PixelBuffer<'a>, P: Palette<Pixel=B::Pixel>>(
        &mut self,
        buffer: &'a mut [u8],
        pitch: usize,
        border_size: BorderSize
    );
}
/// A collection of static methods and constants raleted to video parameters.
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
    /// The first video scan line index of the top border.
    const VSL_BORDER_TOP: Ts;
    /// A range of video scan line indexes for the INK and PAPER area.
    const VSL_PIXELS: Range<Ts>;
    /// The last video scan line index of the bottom border.
    const VSL_BORDER_BOT: Ts;
    /// The total number of video scan lines including the beam retrace.
    const VSL_COUNT: Ts;
    /// The total number of T-states per frame.
    const FRAME_TSTATES_COUNT: FTs = Self::HTS_COUNT as FTs * Self::VSL_COUNT as FTs;
    /// A rendered screen border size in pixels depending on the border size selection.
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
    /// A rendered screen pixel size (horizontal, vertical) measured in low resolution pixels
    /// depending on the border size selection.
    fn screen_size_pixels(border_size: BorderSize) -> (u32, u32) {
        let border = 2 * Self::border_size_pixels(border_size);
        (PAL_HC - 2*MAX_BORDER_SIZE + border, PAL_VC - 2*MAX_BORDER_SIZE + border)
    }
    /// An iterator of the top border low resolution scan line indexes.
    fn border_top_vsl_iter(border_size: BorderSize) -> Range<Ts> {
        let border = Self::border_size_pixels(border_size) as Ts;
        (Self::VSL_PIXELS.start - border)..Self::VSL_PIXELS.start
    }
    /// An iterator of the bottom border low resolution scan line indexes.
    fn border_bot_vsl_iter(border_size: BorderSize) -> Range<Ts> {
        let border = Self::border_size_pixels(border_size) as Ts;
        Self::VSL_PIXELS.end..(Self::VSL_PIXELS.end + border)
    }

    /// Used for rendering border.
    type HtsIter: Iterator<Item=Ts>;
    /// An iterator of border latch horizontal T-states.
    fn border_whole_line_hts_iter(border_size: BorderSize) -> Self::HtsIter;
    /// An iterator of left border latch horizontal T-states.
    fn border_left_hts_iter(border_size: BorderSize) -> Self::HtsIter;
    /// An iterator of right border latch horizontal T-states.
    fn border_right_hts_iter(border_size: BorderSize) -> Self::HtsIter;

    /// T-state contention while rendering ink+paper lines.
    fn contention(hc: Ts) -> Ts;
    /// Returns an optional floating bus horizontal offset for the given timestamp.
    fn floating_bus_offset(vts: VideoTs) -> Option<u16>;
    /// Returns an optional floating bus screen address (in screen address space) for the given timestamp.
    #[inline]
    fn floating_bus_screen_address(ts: VideoTs) -> Option<u16> {
        Self::floating_bus_offset(ts).map(|offs| {
            let y = (ts.vc - Self::VSL_PIXELS.start) as u16;
            let col = (offs >> 3) << 1;
            // if cur_screen_shadow
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
    /// Returns `true` if the given scan-line index is contended for MREQ access.
    #[inline]
    fn is_contended_line_mreq(vsl: Ts) -> bool {
        vsl >= Self::VSL_PIXELS.start && vsl < Self::VSL_PIXELS.end
    }
    /// Returns `true` if the given scan-line index is contended for other than MREQ access.
    #[inline]
    fn is_contended_line_no_mreq(vsl: Ts) -> bool {
        vsl >= Self::VSL_PIXELS.start && vsl < Self::VSL_PIXELS.end
    }
    /// Converts video scan line and horizontal T-state counters to the frame T-state timestamp without wrapping.
    #[inline]
    fn vc_hc_to_tstates(vc: Ts, hc: Ts) -> FTs {
        vc as FTs * Self::HTS_COUNT as FTs + hc as FTs
    }
    /// Converts a video timestamp to the frame T-state timestamp.
    #[inline(always)]
    fn vts_to_tstates(VideoTs { vc, hc }: VideoTs) -> FTs {
        Self::vc_hc_to_tstates(vc, hc)
    }
    /// Converts a frame T-state timestamp to a video timestamp.
    #[inline]
    fn tstates_to_vts(ts: FTs) -> VideoTs {
        let hts_count: FTs = Self::HTS_COUNT as FTs;
        let vc = ts / hts_count;
        let hc = ts % hts_count;
        VideoTs { vc: vc.try_into().unwrap(), hc: hc.try_into().unwrap() }
    }
    /// Converts a `VideoTs` timestamp and a frame counter to a frame counter and
    /// a normalized frame timestamp.
    ///
    /// The timestamp value returned is between: `[0, [VideoFrame::FRAME_TSTATES_COUNT])`.
    #[inline]
    fn vts_to_norm_tstates(vts: VideoTs, frames: u64) -> (u64, FTs) {
        let ts = Self::vts_to_tstates(vts);
        let frmdlt = ts / Self::FRAME_TSTATES_COUNT;
        let ts = ts.rem_euclid(Self::FRAME_TSTATES_COUNT);
        let frames = frames.wrapping_add(frmdlt as u64);
        (frames, ts)
    }
    /// Returns the largest value that can be represented by a `VideoTs`
    /// with normalized horizontal counter.
    #[inline]
    fn vts_max() -> VideoTs {
        VideoTs { vc: Ts::max_value(), hc: Self::HTS_RANGE.end - 1 }
    }
    /// Returns the smallest value that can be represented by a `VideoTs`
    /// with a normalized horizontal counter.
    #[inline]
    fn vts_min() -> VideoTs {
        VideoTs { vc: Ts::min_value(), hc: Self::HTS_RANGE.start }
    }
    /// Returns `true` if a video timestamp is at or past the last video scan line.
    #[inline]
    fn is_vts_eof(VideoTs { vc, .. }: VideoTs) -> bool {
        vc >= Self::VSL_COUNT
    }
    /// Returns a video timestamp with a horizontal counter within the normal range.
    #[inline]
    fn normalize_vts(VideoTs { mut vc, mut hc }: VideoTs) -> VideoTs {
        if hc < Self::HTS_RANGE.start || hc >= Self::HTS_RANGE.end {
            let fhc: FTs = hc as FTs - if hc < 0 {
                Self::HTS_RANGE.end
            }
            else {
                Self::HTS_RANGE.start
            } as FTs;
            vc = vc.checked_add((fhc / Self::HTS_COUNT as FTs) as Ts).expect("video ts overflow");
            hc = fhc.rem_euclid(Self::HTS_COUNT as FTs) as Ts + Self::HTS_RANGE.start;
        }
        VideoTs::new(vc, hc)
    }

    #[inline]
    fn vts_add_ts(VideoTs { vc, hc }: VideoTs, delta: u32) -> VideoTs {
        let dvc = (delta / Self::HTS_COUNT as u32).try_into().expect("delta too large");
        let dhc = (delta % Self::HTS_COUNT as u32) as Ts;
        let vc = vc.checked_add(dvc).expect("delta too large");
        Self::normalize_vts(VideoTs::new(vc, hc + dhc))
    }

    #[inline]
    fn vts_diff(vts_from: VideoTs, vts_to: VideoTs) -> FTs {
        (vts_to.vc as FTs - vts_from.vc as FTs) * Self::HTS_COUNT as FTs +
        (vts_to.hc as FTs - vts_from.hc as FTs)
    }

    #[inline]
    fn vts_saturating_sub_frame(VideoTs { vc, hc }: VideoTs) -> VideoTs {
        let vc = vc.saturating_sub(Self::VSL_COUNT);
        VideoTs { vc, hc }
    }

    fn vts_saturating_sub_vts_normalized(VideoTs { vc, hc }: VideoTs, other_vts: VideoTs) -> VideoTs {
        let vc = vc.saturating_sub(other_vts.vc);
        let hc = hc - other_vts.hc;
        Self::normalize_vts(VideoTs::new(vc, hc))
    }

    fn vts_saturating_add_vts_normalized(VideoTs { vc, hc }: VideoTs, other_vts: VideoTs) -> VideoTs {
        let vc = vc.saturating_add(other_vts.vc);
        let hc = hc + other_vts.hc;
        Self::normalize_vts(VideoTs::new(vc, hc))
    }
}

impl From<BorderSize> for u8 {
    fn from(border: BorderSize) -> u8 {
        border as u8
    }
}

impl fmt::Display for BorderSizeTryFromError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "converted integer ({}) out of range for `BorderSize`", self.0)
    }
}

impl std::error::Error for BorderSizeTryFromError {}

impl TryFrom<u8> for BorderSize {
    type Error = BorderSizeTryFromError;
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
            _ => return Err(BorderSizeTryFromError(border))
        })
    }
}

/// An offset into INK/PAPER bitmap memory of the given vertical coordinate y in 0..192 (0 on top).
#[inline(always)]
pub fn pixel_line_offset<T>(y: T) -> T
where T: Copy + From<u16> + BitAnd<Output=T> + Shl<u16, Output=T> + BitOr<Output=T>
{
    (y & T::from(0b0000_0111) ) << 8 |
    (y & T::from(0b0011_1000) ) << 2 |
    (y & T::from(0b1100_0000) ) << 5
}

/// An offset into attributes memory of the given vertical coordinate y in 0..192 (0 on top).
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
