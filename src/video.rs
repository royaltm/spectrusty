//! ZX Spectrum video interface
mod color;
mod pixel_buffer;

use core::ops::{BitAnd, BitOr, Shl, Shr, Range};
use crate::clock::{Ts, FTs};

pub use color::{PixelRgb, RgbIter, RgbaIter};
pub use pixel_buffer::{PixelBufRGB24, PixelBufRGBA8, PixelBuffer};

pub const PAL_VC: u32 = 576/2;
pub const PAL_HC: u32 = 704/2;

pub const MAX_BORDER_SIZE: u32 = 6*8;

#[derive(Copy,Clone,Debug)]
pub enum BorderSize {
    Full    = 6,
    Large   = 5,
    Medium  = 4,
    Small   = 3,
    Tiny    = 2,
    Minimal = 1
}

pub trait Video {
    type VideoFrame: VideoFrame;
    /// Get the current border area color number 0..7.
    fn border_color(&self) -> u8;
    /// Sets the border area to the given color number 0..7.
    fn set_border_color(&mut self, border: u8);
    /// This should be called after emulating each frame.
    fn render_video_frame<B: PixelBuffer>(&mut self, buffer: &mut [u8], pitch: usize, border_size: BorderSize);
}

pub trait VideoFrame {
    /// A range of horizontal T-states, 0 should be where the frame starts.
    const HTS_RANGE: Range<Ts>;
    /// Number of horizontal T-states.
    const HTS_COUNT: Ts = Self::HTS_RANGE.end - Self::HTS_RANGE.start;
    /// The first video scan line index of the top border.
    const VSL_BORDER_TOP: Ts;
    /// A range of video scan line indexes for the pixel area.
    const VSL_PIXELS: Range<Ts>;
    /// The last video scan line index of the bottom border.
    const VSL_BORDER_BOT: Ts;
    /// A total number of video scan lines.
    const VSL_COUNT: Ts;
    /// Total number of T-states per frame.
    const FRAME_TSTATES_COUNT: FTs = Self::HTS_COUNT as FTs * Self::VSL_COUNT as FTs;
    /// A rendered screen border size in pixels depending on the border size selection.
    fn border_size_pixels(border_size: BorderSize) -> u32 {
        match border_size {
            BorderSize::Full    => MAX_BORDER_SIZE,
            BorderSize::Large   => MAX_BORDER_SIZE -   8,
            BorderSize::Medium  => MAX_BORDER_SIZE - 2*8,
            BorderSize::Small   => MAX_BORDER_SIZE - 3*8,
            BorderSize::Tiny    => MAX_BORDER_SIZE - 4*8,
            BorderSize::Minimal => MAX_BORDER_SIZE - 5*8
        }
    }
    /// A rendered screen pixel size (horizontal, vertical) depending on the border size selection.
    fn screen_size_pixels(border_size: BorderSize) -> (u32, u32) {
        let border = 2 * Self::border_size_pixels(border_size);
        (PAL_HC - 2*MAX_BORDER_SIZE + border, PAL_VC - 2*MAX_BORDER_SIZE + border)
    }
    /// An iterator of the top border scan line indexes.
    fn border_top_vsl_iter(border_size: BorderSize) -> Range<Ts> {
        let border = Self::border_size_pixels(border_size) as Ts;
        (Self::VSL_PIXELS.start - border)..Self::VSL_PIXELS.start
    }
    /// An iterator of the bottom border scan line indexes.
    fn border_bot_vsl_iter(border_size: BorderSize) -> Range<Ts> {
        let border = Self::border_size_pixels(border_size) as Ts;
        Self::VSL_PIXELS.end..(Self::VSL_PIXELS.end + border)
    }

    /// Used for border rendering.
    type HtsIter: Iterator<Item=Ts>;
    /// An iterator of border latch horizontal T-states.
    fn border_whole_line_hts_iter(border_size: BorderSize) -> Self::HtsIter;
    /// An iterator of left border latch horizontal T-states.
    fn border_left_hts_iter(border_size: BorderSize) -> Self::HtsIter;
    /// An iterator of right border latch horizontal T-states.
    fn border_right_hts_iter(border_size: BorderSize) -> Self::HtsIter;

    /// Memory contention while rendering pixel lines.
    fn contention(hc: Ts) -> Ts;

    #[inline]
    fn is_contended_line_mreq(vsl: Ts) -> bool {
        vsl >= Self::VSL_PIXELS.start && vsl < Self::VSL_PIXELS.end
    }
    #[inline]
    fn is_contended_line_no_mreq(vsl: Ts) -> bool {
        vsl >= Self::VSL_PIXELS.start && vsl < Self::VSL_PIXELS.end
    }
    #[inline]
    fn is_contended_address(addr: u16) -> bool {
        addr & 0xC000 == 0x4000
    }
}

/// An offset into ink/paper bitmap memory of the given vertical coordinate y in 0..192 (0 on top).
#[inline(always)]
pub(crate) fn pixel_line_offset<T>(y: T) -> T
where T: Copy + From<u16> + BitAnd<Output=T> + Shl<u16, Output=T> + BitOr<Output=T>
{
    (y & T::from(0b0000_0111) ) << 8 |
    (y & T::from(0b0011_1000) ) << 2 |
    (y & T::from(0b1100_0000) ) << 5
}

/// An offset into attributes memory of the given vertical coordinate y in 0..192 (0 on top).
#[inline(always)]
pub(crate) fn color_line_offset<T>(y: T) -> T
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
