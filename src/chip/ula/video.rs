use core::iter::StepBy;
use core::ops::Range;
use crate::bus::BusDevice;
use crate::memory::ZxMemory;
use crate::clock::{VFrameTsCounter, VideoTs, Ts, VideoTsData3};
use super::render_pixels::Renderer;
use super::Ula;
use crate::video::{BorderSize, PixelBuffer, VideoFrame, MAX_BORDER_SIZE};

pub type UlaTsCounter<M, B> = VFrameTsCounter<Ula<M, B>>;

impl<M: ZxMemory, B: BusDevice<Timestamp=VideoTs>> VideoFrame for Ula<M,B> {
    #[inline]
    fn border_color(&self) -> u8 {
        self.last_border
    }
    /// Sets the border area to the given color number 0..7.
    fn set_border_color(&mut self, border: u8) {
        let border = border & 7;
        self.last_border = border;
        self.border = border;
        self.border_out_changes.clear();
    }
    /// This should be called after emulating each frame.
    fn render_video_frame<P: PixelBuffer>(&mut self, buffer: &mut [u8], pitch: usize, border_size: BorderSize) {
        self.create_renderer(border_size).render_pixels::<P, Self>(buffer, pitch)
    }
    /// A range of horizontal T-states, 0 should be where the frame starts.
    const HTS_RANGE: Range<Ts> = -69..155;
    /// The first video scan line index of the top border.
    const VSL_BORDER_TOP: Ts = 16;
    /// A range of video scan line indexes for the pixel area.
    const VSL_PIXELS: Range<Ts> = 64..256;
    /// The last video scan line index of the bottom border.
    const VSL_BORDER_BOT: Ts = 304;
    /// A total number of video scan lines.
    const VSL_COUNT: Ts = 312;

    type HtsIter = StepBy<Range<Ts>>;

    fn border_whole_line_hts_iter(border_size: BorderSize) -> Self::HtsIter {
        let invborder = (MAX_BORDER_SIZE - Self::border_size_pixels(border_size)) as Ts;
        (-21+invborder..155-invborder).step_by(4)
    }

    fn border_left_hts_iter(border_size: BorderSize) -> Self::HtsIter {
        let invborder = (MAX_BORDER_SIZE - Self::border_size_pixels(border_size)) as Ts;
        (-21+invborder..3).step_by(4)
    }

    fn border_right_hts_iter(border_size: BorderSize) -> Self::HtsIter {
        let invborder = (MAX_BORDER_SIZE - Self::border_size_pixels(border_size)) as Ts;
        (131..155-invborder).step_by(4)
    }

    #[inline]
    fn contention(mut hc: Ts) -> Ts {
        if hc >= -1 && hc < 124 {
            let ct = (hc + 1) & 7;
            if ct < 6 {
                hc += 6 - ct;
            }
        }
        hc
    }
}

struct Coords {
    column: u16,
    line: i16
}

static COL_INK_HTS:  &[Ts] = &[4, 6, 12, 14, 20, 22, 28, 30, 36, 38, 44, 46, 52, 54, 60, 62, 68, 70, 76, 78, 84, 86, 92, 94, 100, 102, 108, 110, 116, 118, 124, 126];
static COL_ATTR_HTS: &[Ts] = &[5, 7, 13, 15, 21, 23, 29, 31, 37, 39, 45, 47, 53, 55, 61, 63, 69, 71, 77, 79, 85, 87, 93, 95, 101, 103, 109, 111, 117, 119, 125, 127];

fn pixel_address_coords(addr: u16) -> Option<Coords> {
    match addr {
        0x4000..=0x57FF => {
            let column = addr & 0b11111;
            let line = (addr >> 5 & 0b1100_0000 |
                        addr >> 2 & 0b0011_1000 |
                        addr >> 8 & 0b0000_0111) as i16;
            Some(Coords { column, line })
        }
        _ => None
    }
}

fn color_address_coords(addr: u16) -> Option<Coords> {
    match addr {
        0x5800..=0x5AFF => {
            let column = addr & 0b11111;
            let line = (addr >> 5 & 0b0001_1111) as i16;
            Some(Coords { column, line })
        }
        _ => None
    }
}

impl<M: ZxMemory, B: BusDevice<Timestamp=VideoTs>> Ula<M,B> {
    pub fn create_renderer(&mut self, border_size: BorderSize) -> Renderer<'_, std::vec::Drain<'_, VideoTsData3>> {
        let border = self.border as usize;
        Renderer {
            frame_pixels: &self.frame_pixels,
            frame_colors: &self.frame_colors,
            frame_colors_coarse: &self.frame_colors_coarse,
            pixel_memory: &self.memory.screen_ref(0).unwrap()[0x0000..0x1800],
            color_memory: &self.memory.screen_ref(0).unwrap()[0x1800..0x1B00],
            border,
            border_size,
            border_changes: self.border_out_changes.drain(..),
            invert_flash: self.frames.0 & 16 != 0
        }
    }

    #[inline(always)]
    pub fn cleanup_video_frame_data(&mut self) {
        self.border = self.last_border;
        self.border_out_changes.clear();
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

    #[inline(always)]
    pub(super) fn update_frame_pixels_and_colors(&mut self, addr: u16, ts: VideoTs) {
        if let Some(Coords { column, line }) = pixel_address_coords(addr) {
            let cur_line_index = ts.vc - Self::VSL_PIXELS.start;
            if line < cur_line_index || line == cur_line_index && ts.hc > COL_INK_HTS[column as usize] {
                let (mask, pixels) = &mut self.frame_pixels[line as usize];
                let mbit = 1 << column;
                if *mask & mbit == 0 {
                    pixels[column as usize] = self.memory.read(addr);
                    *mask |= mbit;
                }
            }
        }
        else {
            self.update_frame_colors(addr, ts);
        }
    }

    #[inline(always)]
    fn update_frame_colors(&mut self, addr: u16, ts: VideoTs) {
        if let Some(Coords { column, line }) = color_address_coords(addr) {
            let cur_line_index = ts.vc - Self::VSL_PIXELS.start;
            let coarse_cur_line_index = cur_line_index >> 3;
            if line < coarse_cur_line_index ||
               line == coarse_cur_line_index && cur_line_index & 0b111 == 0b111 && ts.hc > COL_ATTR_HTS[column as usize] {
                let (mask, colors) = &mut self.frame_colors_coarse[line as usize];
                let mbit = 1 << column;
                if *mask & mbit == 0 {
                    *mask |= mbit;
                    colors[column as usize] = self.memory.read(addr);
                }
            }
            else if line == coarse_cur_line_index {
                let line_top = coarse_cur_line_index << 3;
                let line_bot = if ts.hc > COL_ATTR_HTS[column as usize] { cur_line_index } else { cur_line_index - 1 };
                if line_top <= line_bot {
                    let memval = self.memory.read(addr);
                    for cur_line_index in (line_top..=line_bot).rev() {
                        let (mask, colors) = &mut self.frame_colors[cur_line_index as usize];
                        let mbit = 1 << column;
                        if *mask & mbit != 0 {
                            break;
                        }
                        *mask |= mbit;
                        colors[column as usize] = memval;
                    }
                }
            }
        }
    }
}
