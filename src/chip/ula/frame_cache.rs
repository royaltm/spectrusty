use core::fmt;
use core::marker::PhantomData;
use crate::clock::{Ts, VideoTs};
use crate::memory::{ZxMemory, ScreenArray};
use crate::video::{
    pixel_line_offset, color_line_offset,
    VideoFrame, CellCoords,
    frame_cache::*
};

// static COL_INK_HTS:  &[Ts] = &[4, 6, 12, 14, 20, 22, 28, 30, 36, 38, 44, 46, 52, 54, 60, 62, 68, 70, 76, 78, 84, 86, 92, 94, 100, 102, 108, 110, 116, 118, 124, 126];
// static COL_ATTR_HTS: &[Ts] = &[5, 7, 13, 15, 21, 23, 29, 31, 37, 39, 45, 47, 53, 55, 61, 63, 69, 71, 77, 79, 85, 87, 93, 95, 101, 103, 109, 111, 117, 119, 125, 127];
const COL_INK_HTS:  &[Ts;COLUMNS] = &[1, 3,  9, 11, 17, 19, 25, 27, 33, 35, 41, 43, 49, 51, 57, 59, 65, 67, 73, 75, 81, 83, 89, 91, 97,  99, 105, 107, 113, 115, 121, 123];
const COL_ATTR_HTS: &[Ts;COLUMNS] = &[2, 4, 10, 12, 18, 20, 26, 28, 34, 36, 42, 44, 50, 52, 58, 60, 66, 68, 74, 76, 82, 84, 90, 92, 98, 100, 106, 108, 114, 116, 122, 124];

/// Spectrum's video data frame cache.
///
/// When a byte is being written to the video memory and the video beam position has already passed
/// the referenced screen cell, the cell value is being cached from memory before the data in memory
/// is being modified.
///
/// The caching is performed only once per frame for each potential cell so subsequent writes to the same
/// memory address won't modify the cached cell value.
///
/// When a screen is being drawn the data stored in cache will override any value currently residing in
/// video memory.
#[derive(Clone)]
pub struct UlaFrameCache<V> {
    pub frame_pixels: [(u32, [u8;COLUMNS]);PIXEL_LINES],      // read precedence pixels < memory
    pub frame_colors: [(u32, [u8;COLUMNS]);PIXEL_LINES],
    pub frame_colors_coarse: [(u32, [u8;COLUMNS]);ATTR_ROWS], // read precedence colors < colors_coarse < memory
    _video_frame: PhantomData<V>
}

/// A reference to screen data with a relevant frame cache.
pub struct UlaFrameRef<'a, V> {
    pub screen: &'a ScreenArray,
    pub frame_cache: &'a UlaFrameCache<V>,
}

pub(crate) struct UlaFrameLineIter<'a> {
    pub column: usize,
    pub ink_line: &'a[u8;COLUMNS],
    pub attr_line: &'a[u8;COLUMNS],
    pub frame_pixels: &'a(u32, [u8;COLUMNS]),
    pub frame_colors: &'a(u32, [u8;COLUMNS]),
    pub frame_colors_coarse: &'a(u32, [u8;COLUMNS])
}

/// Implements a [VideoFrameDataIterator] for 16k/48k ULA based Spectrum models.
pub struct UlaFrameProducer<'a, V> {
    frame_ref: UlaFrameRef<'a, V>,
    line: usize,
    line_iter: UlaFrameLineIter<'a>
}

impl<'a, V> UlaFrameRef<'a, V> {
    pub const fn new(screen: &'a ScreenArray, frame_cache: &'a UlaFrameCache<V>) -> Self {
        UlaFrameRef { screen, frame_cache }
    }
}

impl<'a> UlaFrameLineIter<'a> {
    pub fn new<V>(line: usize, screen: &'a ScreenArray, fc: &'a UlaFrameCache<V>) -> Self {
        let frame_pixels = &fc.frame_pixels[line];
        let frame_colors = &fc.frame_colors[line];
        let frame_colors_coarse = &fc.frame_colors_coarse[line >> 3];
        let ink_line = ink_line_from(line, screen);
        let attr_line = attr_line_from(line, screen);
        UlaFrameLineIter {
            column: 0,
            ink_line,
            attr_line,
            frame_pixels,
            frame_colors,
            frame_colors_coarse
        }
    }
}

impl<'a, V> UlaFrameProducer<'a, V> {
    pub fn new(screen: &'a ScreenArray, frame_cache: &'a UlaFrameCache<V>) -> Self {
        let line = 0;
        let line_iter = UlaFrameLineIter::new(line, screen, frame_cache);
        let frame_ref = UlaFrameRef::new(screen, frame_cache);
        UlaFrameProducer { line, frame_ref, line_iter }
    }

    pub fn swap_frame(&mut self, frame_ref: &mut UlaFrameRef<'a, V>) {
        core::mem::swap(&mut self.frame_ref, frame_ref);
        self.update_iter()
    }

    #[inline(always)]
    pub fn column(&mut self) -> usize {
        self.line_iter.column
    }

    #[inline(always)]
    pub fn line(&mut self) -> usize {
        self.line
    }

    pub fn update_iter(&mut self) {
        let line = self.line;
        let fc = &self.frame_ref.frame_cache;
        self.line_iter.frame_pixels = &fc.frame_pixels[line];
        self.line_iter.frame_colors = &fc.frame_colors[line];
        self.line_iter.frame_colors_coarse = &fc.frame_colors_coarse[line >> 3];
        self.line_iter.ink_line = ink_line_from(line, &self.frame_ref.screen);
        self.line_iter.attr_line = attr_line_from(line, &self.frame_ref.screen);
    }
}

impl<'a, V> VideoFrameDataIterator for UlaFrameProducer<'a, V> {
    fn next_line(&mut self) {
        let line = self.line + 1;
        if line < PIXEL_LINES {
            self.line = line;
            self.update_iter();
            self.line_iter.column = 0;
        }
    }
}

impl<'a, V> Iterator for UlaFrameProducer<'a, V> {
    type Item = (u8, u8);

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        self.line_iter.next()
    }
}

impl<'a> Iterator for UlaFrameLineIter<'a> {
    type Item = (u8, u8);

    fn next(&mut self) -> Option<Self::Item> {
        let column = self.column;
        if column >= COLUMNS {
            return None
        }
        self.column = column + 1;
        let bitmask = 1 << column;
        let ink: u8 = if self.frame_pixels.0 & bitmask != 0 {
            self.frame_pixels.1[column & (COLUMNS - 1)]
        }
        else {
            self.ink_line[column & (COLUMNS - 1)]
        };
        let attr: u8 = if self.frame_colors.0 & bitmask != 0 {
            self.frame_colors.1[column & (COLUMNS - 1)]
        }
        else if self.frame_colors_coarse.0 & bitmask != 0 {
            self.frame_colors_coarse.1[column & (COLUMNS - 1)]
        }
        else {
            self.attr_line[column & (COLUMNS - 1)]
        };
        Some((ink, attr))
    }
}

impl<V> Default for UlaFrameCache<V> {
    fn default() -> Self {
        UlaFrameCache {
            frame_pixels: [(0, [0;COLUMNS]);PIXEL_LINES],
            frame_colors: [(0, [0;COLUMNS]);PIXEL_LINES],
            frame_colors_coarse: [(0, [0;COLUMNS]);ATTR_ROWS],
            _video_frame: PhantomData
        }
    }
}

impl<V> fmt::Debug for UlaFrameCache<V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("UlaFrameCache {{ }}")
    }
}

impl<V> UlaFrameCache<V> {
    pub fn clear(&mut self) {
        for p in self.frame_pixels.iter_mut() {
            p.0 = 0;
        }
        for p in self.frame_colors.iter_mut() {
            p.0 = 0;
        }
        for p in self.frame_colors_coarse.iter_mut() {
            p.0 = 0;
        }
    }
}

impl<V: VideoFrame> UlaFrameCache<V> {
    /// Compares the given bitmap cell coordinates with the video timestamp and depending
    /// on the result of that comparison caches (or not) the bitmap cell with the value
    /// from the memory at the given address.
    #[inline]
    pub fn update_frame_pixels<M: ZxMemory>(
            &mut self,
            memory: &M,
            CellCoords { column, row }: CellCoords,
            addr: u16,
            ts: VideoTs
        )
    {
        let column = column as usize & 31;
        let vy = ts.vc - V::VSL_PIXELS.start;
        let y = Ts::from(row);
        if y < vy || y == vy && ts.hc > COL_INK_HTS[column] {
            let (mask, pixels) = &mut self.frame_pixels[row as usize];
            let mbit = 1 << column;
            if *mask & mbit == 0 {
                pixels[column] = memory.read(addr);
                *mask |= mbit;
            }
        }
    }
    /// Compares the given attribute cell coordinates with the video timestamp and depending
    /// on the result of that comparison caches (or not) the attribute cell or cells with
    /// the value from the memory at the given address.
    #[inline]
    pub fn update_frame_colors<M: ZxMemory>(
            &mut self,
            memory: &M,
            CellCoords { column, row }: CellCoords,
            addr: u16,
            ts: VideoTs
        )
    {
        let column = column as usize & 31;
        let vy = ts.vc - V::VSL_PIXELS.start;
        let coarse_vy = vy >> 3;
        let coarse_y = Ts::from(row);
        if coarse_y < coarse_vy ||
                coarse_y == coarse_vy &&
                vy & 0b111 == 0b111 &&
                ts.hc > COL_ATTR_HTS[column] {
            let (mask, colors) = &mut self.frame_colors_coarse[row as usize];
            let mbit = 1 << column;
            if *mask & mbit == 0 {
                *mask |= mbit;
                colors[column] = memory.read(addr);
            }
        }
        else if coarse_y == coarse_vy {
            let line_top = (coarse_vy << 3) as usize;
            let line_bot = if ts.hc > COL_ATTR_HTS[column] {
                vy + 1
            } else {
                vy
            } as usize;
            if line_top < line_bot {
                let memval = memory.read(addr);
                let mbit = 1 << column;
                for (mask, colors) in self.frame_colors[line_top..line_bot].iter_mut().rev() {
                    if *mask & mbit != 0 {
                        break;
                    }
                    *mask |= mbit;
                    colors[column] = memval;
                }
            }
        }
    }
    /// Caches the bitmap and attribute cell at the given coordinates with the `snow` distortion applied.
    pub fn apply_snow_interference(
            &mut self,
            screen: &ScreenArray,
            CellCoords { column, row }: CellCoords,
            snow: u8
        )
    {
        let (row, column) = (row as usize, column as usize & 31);
        let mbit = 1 << column;
        let (mask, pixels) = &mut self.frame_pixels[row];
        let offset_snow = (pixel_line_offset(row) & 0x1F00) | snow as usize;
        pixels[column] = screen[offset_snow];
        *mask |= mbit;

        let offset = ATTRS_OFFSET + color_line_offset(row);
        let offset_snow = (offset & 0x1F00) | snow as usize;
        let (mask, colors) = &mut self.frame_colors[row];
        colors[column] = screen[offset_snow];
        *mask |= mbit;
        // cache the remaining attribute sub-cell lines preceding the distorted cell
        let row_top = row & !7;
        if row_top < row {
            let memval = screen[offset];
            for (mask, colors) in self.frame_colors[row_top..row].iter_mut().rev() {
                if *mask & mbit != 0 {
                    break;
                }
                *mask |= mbit;
                colors[column] = memval;
            }
        }
    }
}
