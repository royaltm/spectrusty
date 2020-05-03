use core::iter::StepBy;
use core::ops::Range;

#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};

use crate::memory::ZxMemory;
use crate::clock::{VFrameTsCounter, MemoryContention, VideoTs, Ts, VideoTsData3};
use crate::video::{
    Renderer, BorderSize, BorderColor, PixelBuffer, Palette,
    VideoFrame, Video, CellCoords, MAX_BORDER_SIZE
};
use super::{Ula, UlaMemoryContention, frame_cache::{pixel_address_coords, color_address_coords}};
use super::frame_cache::{UlaFrameCache, UlaFrameProducer};

#[derive(Clone, Copy, Default, Debug, PartialEq)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
pub struct UlaVideoFrame;

pub type UlaTsCounter = VFrameTsCounter<UlaVideoFrame, UlaMemoryContention>;

impl VideoFrame for UlaVideoFrame {
    /// A range of horizontal T-states, 0 should be when the frame starts.
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
        let invborder = ((MAX_BORDER_SIZE - Self::border_size_pixels(border_size))/2) as Ts;
        (-20+invborder..156-invborder).step_by(4)
    }

    fn border_left_hts_iter(border_size: BorderSize) -> Self::HtsIter {
        let invborder = ((MAX_BORDER_SIZE - Self::border_size_pixels(border_size))/2) as Ts;
        (-20+invborder..4).step_by(4)
    }

    fn border_right_hts_iter(border_size: BorderSize) -> Self::HtsIter {
        let invborder = ((MAX_BORDER_SIZE - Self::border_size_pixels(border_size))/2) as Ts;
        (132..156-invborder).step_by(4)
    }

    #[inline]
    fn contention(hc: Ts) -> Ts {
        if hc >= -1 && hc < 125 {
            let ct = (hc + 1) & 7;
            if ct < 6 {
                return hc + 6 - ct;
            }
        }
        hc
    }

    #[inline(always)]
    fn floating_bus_offset(hc: Ts) -> Option<u16> {
        // println!("floating_bus_offset: {},{} {}", vc, hc, crate::clock::VFrameTsCounter::<Self>::vc_hc_to_tstates(vc, hc));
        match hc {
            c @ 0..=123 if c & 4 == 0 => Some(c as u16),
            _ => None
        }
    }

    #[inline(always)]
    fn snow_interference_coords(VideoTs { vc, hc }: VideoTs) -> Option<CellCoords> {
        let row = vc - Self::VSL_PIXELS.start;
        if row >= 0 && vc < Self::VSL_PIXELS.end {
            let hc = hc - 2;
            if hc >= 0 && hc <= 122 {
                return match hc & 7 {
                    0 => Some(0),
                    2 => Some(1),
                    _ => None
                }.map(|offs| {
                    let column = (((hc >> 2) & !1) | offs) as u8;
                    CellCoords { column, row: row as u8 }
                })
            }
        }
        None
    }
}

impl<M: ZxMemory, D, X> Video for Ula<M, D, X> {
    type VideoFrame = UlaVideoFrame;

    #[inline]
    fn border_color(&self) -> BorderColor {
        self.ula_border_color()
    }

    fn set_border_color(&mut self, border: BorderColor) {
        self.ula_set_border_color(border)
    }

    fn render_video_frame<'a, B: PixelBuffer<'a>, P: Palette<Pixel=B::Pixel>>(
            &mut self,
            buffer: &'a mut [u8],
            pitch: usize,
            border_size: BorderSize
        )
    {
        self.create_renderer(border_size).render_pixels::<B, P, Self::VideoFrame>(buffer, pitch)
    }
}

impl<M: ZxMemory, B, X> Ula<M, B, X> {
    #[inline(always)]
    pub(super) fn update_frame_cache(&mut self, addr: u16, ts: VideoTs) {
        if let Some(coords) = pixel_address_coords(addr) {
            self.frame_cache.update_frame_pixels(&self.memory, coords, addr, ts);
        }
        else if let Some(coords) = color_address_coords(addr) {
            self.frame_cache.update_frame_colors(&self.memory, coords, addr, ts);
        }
    }

    #[inline(always)]
    pub(super) fn update_snow_interference(&mut self, ts: VideoTs, ir: u16) {
        if UlaMemoryContention::is_contended_address(ir) {
            if let Some(coords) = UlaVideoFrame::snow_interference_coords(ts) {
                let screen = self.memory.screen_ref(0).unwrap();
                self.frame_cache.apply_snow_interference(screen, coords, ir as u8)
            }
        }
    }
}

impl<M: ZxMemory, B, X, V> Ula<M, B, X, V> {
    #[inline]
    pub(crate) fn cleanup_video_frame_data(&mut self) {
        self.border = self.last_border;
        self.border_out_changes.clear();
        self.frame_cache.clear();
    }
    #[inline]
    pub(crate) fn ula_border_color(&self) -> BorderColor {
        self.last_border
    }
    #[inline]
    pub(crate) fn ula_set_border_color(&mut self, border: BorderColor) {
        self.last_border = border;
        self.border = border;
        self.border_out_changes.clear();
    }
    #[inline]
    pub(crate) fn ula_video_render_data_view(&mut self) -> (&mut Vec<VideoTsData3>, &M, &UlaFrameCache<V>) {
        (&mut self.border_out_changes, &self.memory, &self.frame_cache)
    }

    fn create_renderer(
            &mut self,
            border_size: BorderSize
        ) -> Renderer<UlaFrameProducer<'_, V>, std::vec::Drain<'_, VideoTsData3>>
    {
        let border = self.border.into();
        let screen = &self.memory.screen_ref(0).unwrap();
        // print!("render: {} {:?}", screen_bank, screen.as_ptr());
        Renderer {
            frame_image_producer: UlaFrameProducer::new(screen, &self.frame_cache),
            border,
            border_size,
            border_changes: self.border_out_changes.drain(..),
            invert_flash: self.frames.0 & 16 != 0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contention() {
        let vts0 = VideoTs::new(0, 0);
        let tstates = [(14335, 14341),
                       (14336, 14341),
                       (14337, 14341),
                       (14338, 14341),
                       (14339, 14341),
                       (14340, 14341),
                       (14341, 14341),
                       (14342, 14342)];
        for offset in (0..16).map(|x| x * 8i32) {
            for (testing, target) in tstates.iter().copied() {
                let mut vts = UlaVideoFrame::vts_add_ts(vts0, testing + offset as u32);
                vts.hc = UlaVideoFrame::contention(vts.hc);
                assert_eq!(UlaVideoFrame::normalize_vts(vts),
                           UlaVideoFrame::tstates_to_vts(target + offset));
            }
        }
        let refts = tstates[0].0 as i32;
        for ts in (refts - 96..refts)
            .chain(refts + 128..refts+UlaVideoFrame::HTS_COUNT as i32) {
            let vts = UlaVideoFrame::tstates_to_vts(ts);
            assert_eq!(UlaVideoFrame::contention(vts.hc), vts.hc);
        }
    }
}
