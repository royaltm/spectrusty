use core::convert::TryInto;
use crate::clock::Ts;
use crate::video::{
  pixel_line_offset, color_line_offset,
  CellCoords
};
use crate::memory::ScreenArray;

/// Implemented by screen data iterators for rendering an image of a video frame using [Renderer].
///
/// The iterator emulates the Spectrum's ULA reading two bytes of video data to compose a single cell
/// consisting of 8 pixels.
///
/// The first byte returned by the iterator is interpreted as INK/PAPER bit selector (an INK mask) and
/// the second as a color attribute.
///
/// [Renderer]: crate::video::Renderer
pub trait VideoFrameDataIterator: Iterator<Item=(u8, u8)> {
    /// Forwards the iterator to the beginning of the next video line.
    fn next_line(&mut self);
}
/// Implemented by screen data iterators for rendering an image of a video frame using [RendererPlus].
///
/// This emulates the Spectrum's ULAplus/Timex's SCLD reading two bytes of video data to compose a single cell
/// consisting of 8 (or 16 in hi-res) pixels.
///
/// In low resolution mode the first byte returned by the iterator is interpreted as INK/PAPER bit selector
/// (an INK mask) and the second as a color attribute.
///
/// In high resolution mode both bytes are being used to render 16 monochrome pixels.
///
/// The third value is a horizontal latch timestamp used to synchronize changes to the screen mode and the palette.
///
/// [RendererPlus]: crate::video::RendererPlus
pub trait PlusVidFrameDataIterator: Iterator<Item=(u8, u8, Ts)> {
    /// Forwards the iterator to the beginning of the next video line.
    fn next_line(&mut self);
}

/// The number of screen columns (bytes in line).
pub const COLUMNS: usize = 32;
/// The number of screen pixel lines.
pub const PIXEL_LINES: usize = 192;
/// The number of screen attribute rows.
pub const ATTR_ROWS: usize = 24;
/// The offset into screen memory where attributes data begin.
pub const ATTRS_OFFSET: usize = 0x1800;

/// Returns cell coordinates of a INK/PAPER bitmap cell ignoring the highest 3 address bits.
#[inline(always)]
pub fn pixel_address_coords(addr: u16) -> CellCoords {
    let column = (addr & 0b0001_1111) as u8;
    let row = (addr >> 5 & 0b1100_0000 |
               addr >> 2 & 0b0011_1000 |
               addr >> 8 & 0b0000_0111) as u8;
    CellCoords { column, row }
}
/// Returns cell coordinates of a screen attribute cell ignoring the highest 6 address bits.
#[inline(always)]
pub fn color_address_coords(addr: u16) -> CellCoords {
    let column = (addr & 0b0001_1111) as u8;
    let row = (addr >> 5 & 0b0001_1111) as u8;
    CellCoords { column, row }
}

/// Returns a reference to display attributes line data.
///
/// * `line` should be in range: [0, [PIXEL_LINES]).
/// * `screen` should be a reference to the screen data.
#[inline(always)]
pub fn attr_line_from(line: usize, screen: &ScreenArray) -> &[u8;COLUMNS] {
    let offset = ATTRS_OFFSET + color_line_offset(line);
    cast_line(&screen[offset..offset + COLUMNS])
}

/// Returns a reference to INK/PAPER line data.
///
/// * `line` should be in range: [0, [PIXEL_LINES]).
/// * `screen` should be a reference to the screen data.
#[inline(always)]
pub fn ink_line_from(line: usize, screen: &ScreenArray) -> &[u8;COLUMNS] {
    let offset = pixel_line_offset(line);
    cast_line(&screen[offset..offset + COLUMNS])
}

#[inline(always)]
fn cast_line(line_slice: &[u8]) -> &[u8; COLUMNS] {
    line_slice.try_into().unwrap()
}
