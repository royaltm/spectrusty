use core::convert::TryInto;
use core::fmt;
use core::marker::PhantomData;
use crate::clock::{Ts, VideoTs};
use crate::memory::{ZxMemory};
use crate::video::{pixel_line_offset, color_line_offset, VideoFrame, CellCoords,
    frame_cache::VideoFrameDataIterator
};

const ATTRS_OFFSET: usize = 0x1800;
const LINE_SIZE: usize = 32;
const PIXEL_LINES: usize = 192;
// static COL_INK_HTS:  &[Ts] = &[4, 6, 12, 14, 20, 22, 28, 30, 36, 38, 44, 46, 52, 54, 60, 62, 68, 70, 76, 78, 84, 86, 92, 94, 100, 102, 108, 110, 116, 118, 124, 126];
// static COL_ATTR_HTS: &[Ts] = &[5, 7, 13, 15, 21, 23, 29, 31, 37, 39, 45, 47, 53, 55, 61, 63, 69, 71, 77, 79, 85, 87, 93, 95, 101, 103, 109, 111, 117, 119, 125, 127];
const COL_INK_HTS:  &[Ts;32] = &[1, 3,  9, 11, 17, 19, 25, 27, 33, 35, 41, 43, 49, 51, 57, 59, 65, 67, 73, 75, 81, 83, 89, 91, 97,  99, 105, 107, 113, 115, 121, 123];
const COL_ATTR_HTS: &[Ts;32] = &[2, 4, 10, 12, 18, 20, 26, 28, 34, 36, 42, 44, 50, 52, 58, 60, 66, 68, 74, 76, 82, 84, 90, 92, 98, 100, 106, 108, 114, 116, 122, 124];

#[inline(always)]
pub fn pixel_address_coords(addr: u16) -> Option<CellCoords> {
    match addr {
        0x4000..=0x57FF => {
            let column = addr & 0b11111;
            let line = (addr >> 5 & 0b1100_0000 |
                        addr >> 2 & 0b0011_1000 |
                        addr >> 8 & 0b0000_0111) as i16;
            Some(CellCoords { column, line })
        }
        _ => None
    }
}

#[inline(always)]
pub fn color_address_coords(addr: u16) -> Option<CellCoords> {
    match addr {
        0x5800..=0x5AFF => {
            let column = addr & 0b11111;
            let line = (addr >> 5 & 0b0001_1111) as i16;
            Some(CellCoords { column, line })
        }
        _ => None
    }
}

#[derive(Clone)]
pub struct UlaFrameCache<V> {
    pub frame_pixels: [(u32, [u8;32]);192],       // pixel read precedence pixels > memory
    pub frame_colors: [(u32, [u8;32]);192],
    pub frame_colors_coarse: [(u32, [u8;32]);24], // color read precedence colors > colors_coarse > memory
    _video_frame: PhantomData<V>
}

pub struct UlaFrameProducer<'a, V> {
    line: usize,
    screen: &'a[u8],
    frame_cache: &'a UlaFrameCache<V>,
    line_iter: UlaFrameLineIter<'a>
}

pub struct UlaFrameLineIter<'a> {
    column: usize,
    ink_line: &'a[u8;32],
    attr_line: &'a[u8;32],
    pixels_mask: u32,
    frame_pixels: &'a[u8;32],
    colors_mask: u32,
    frame_colors: &'a[u8;32],
    colors_coarse_mask: u32,
    frame_colors_coarse: &'a[u8;32]
}

impl<'a, V> UlaFrameProducer<'a, V> {
    pub fn new(screen: &'a[u8], frame_cache: &'a UlaFrameCache<V>) -> Self {
        let line = 0;
        let line_iter = Self::iter_line(line, screen, frame_cache);
        UlaFrameProducer { line, screen, frame_cache, line_iter }
    }

    #[inline(always)]
    pub fn set_column(&mut self, column: usize) {
        self.line_iter.column = column;
    }

    #[inline(always)]
    pub fn column(&mut self) -> usize {
        self.line_iter.column
    }

    #[inline(always)]
    pub fn line(&mut self) -> usize {
        self.line
    }

    fn iter_line(line: usize, screen: &'a [u8], fc: &'a UlaFrameCache<V>) -> UlaFrameLineIter<'a> {
        let (pixels_mask, ref frame_pixels) = fc.frame_pixels[line];
        let (colors_mask, ref frame_colors) = fc.frame_colors[line];
        let (colors_coarse_mask, ref frame_colors_coarse) = fc.frame_colors_coarse[line >> 3];
        let (ink_line, attr_line) = ink_attr_line(line, screen);
        UlaFrameLineIter {
            column: 0,
            ink_line,
            attr_line,
            pixels_mask,
            frame_pixels,
            colors_mask,
            frame_colors,
            colors_coarse_mask,
            frame_colors_coarse
        }
    }
}

impl<'a, V> VideoFrameDataIterator for UlaFrameProducer<'a, V> {
    fn next_line(&mut self) {
        let line = self.line + 1;
        if line < PIXEL_LINES {
            self.line = line;
            self.line_iter = Self::iter_line(line, &self.screen, &self.frame_cache);
        }
    }
}

impl<'a, V> Iterator for UlaFrameProducer<'a, V> {
    type Item = (u8, u8);

    #[inline]
    fn next(&mut self) -> Option<(u8, u8)> {
        let line_iter = &mut self.line_iter;
        let column = line_iter.column;
        if column >= LINE_SIZE {
            return None
        }
        line_iter.column = column + 1;
        let bitmask = 1 << column;
        let ink: u8 = if line_iter.pixels_mask & bitmask != 0 {
            line_iter.frame_pixels[column & (LINE_SIZE - 1)]
        }
        else {
            line_iter.ink_line[column & (LINE_SIZE - 1)]
        };
        let attr: u8 = if line_iter.colors_mask & bitmask != 0 {
            line_iter.frame_colors[column & (LINE_SIZE - 1)]
        }
        else if line_iter.colors_coarse_mask & bitmask != 0 {
            line_iter.frame_colors_coarse[column & (LINE_SIZE - 1)]
        }
        else {
            line_iter.attr_line[column & (LINE_SIZE - 1)]
        };
        Some((ink, attr))
    }
}

impl<V> Default for UlaFrameCache<V> {
    fn default() -> Self {
        UlaFrameCache {
            frame_pixels: [(0, [0;32]);192],
            frame_colors: [(0, [0;32]);192],
            frame_colors_coarse: [(0, [0;32]);24],
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
    #[inline(always)]
    pub fn update_frame_pixels<M: ZxMemory>(
            &mut self,
            memory: &M,
            CellCoords { column, line }: CellCoords,
            addr: u16,
            ts: VideoTs
        )
    {
        let column = column as usize & 31;
        let cur_line_index = ts.vc - V::VSL_PIXELS.start;
        if line < cur_line_index || line == cur_line_index && ts.hc > COL_INK_HTS[column] {
            let (mask, pixels) = &mut self.frame_pixels[line as usize];
            let mbit = 1 << column;
            if *mask & mbit == 0 {
                pixels[column] = memory.read(addr);
                *mask |= mbit;
            }
        }
    }

    #[inline(always)]
    pub fn update_frame_colors<M: ZxMemory>(
            &mut self,
            memory: &M,
            CellCoords { column, line }: CellCoords,
            addr: u16,
            ts: VideoTs
        )
    {
        let column = column as usize & 31;
        let cur_line_index = ts.vc - V::VSL_PIXELS.start;
        let coarse_cur_line_index = cur_line_index >> 3;
        if line < coarse_cur_line_index ||
                line == coarse_cur_line_index &&
                cur_line_index & 0b111 == 0b111 &&
                ts.hc > COL_ATTR_HTS[column] {
            let (mask, colors) = &mut self.frame_colors_coarse[line as usize];
            let mbit = 1 << column;
            if *mask & mbit == 0 {
                *mask |= mbit;
                colors[column] = memory.read(addr);
            }
        }
        else if line == coarse_cur_line_index {
            let line_top = coarse_cur_line_index << 3;
            let line_bot = if ts.hc > COL_ATTR_HTS[column] {
                cur_line_index
            } else {
                cur_line_index - 1
            };
            if line_top <= line_bot {
                let memval = memory.read(addr);
                let mbit = 1 << column;
                for (mask, colors) in self.frame_colors[line_top as usize..=line_bot as usize].iter_mut().rev() {
                // for cur_line_index in (line_top..=line_bot).rev() {
                    // let (mask, colors) = &mut self.frame_colors[cur_line_index as usize];
                    // if *mask & mbit != 0 {
                    //     break;
                    // }
                    if *mask & mbit == 0 {
                        *mask |= mbit;
                        colors[column] = memval;
                    }
                }
            }
        }
    }

    pub fn apply_snow_interference(&mut self, screen: &[u8], CellCoords { column, line }: CellCoords, r: u8) {
        let (line, column) = (line as usize, column as usize);
        let mbit = 1 << column;
        let (mask, pixels) = &mut self.frame_pixels[line];
        let offset = (pixel_line_offset(line) & 0xFF00) | r as usize;
        pixels[column] = screen[offset];
        *mask |= mbit;
        let (mask, colors) = &mut self.frame_colors[line];
        let offset = (color_line_offset(line) & 0xFF00) | r as usize;
        colors[column] = screen[ATTRS_OFFSET + offset];
        *mask |= mbit;
    }
}

#[inline(always)]
fn ink_attr_line(line: usize, screen: &[u8]) -> (&[u8;32], &[u8;32]) {
    let offset = pixel_line_offset(line);
    let ink_line = cast_line(&screen[offset..offset + LINE_SIZE]);
    let offset = ATTRS_OFFSET + color_line_offset(line);
    let attr_line = cast_line(&screen[offset..offset + LINE_SIZE]);
    (ink_line, attr_line)
}

#[inline(always)]
fn cast_line(line_slice: &[u8]) -> &[u8; LINE_SIZE] {
    line_slice.try_into().unwrap()
}
