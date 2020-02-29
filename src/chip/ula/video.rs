use core::iter::StepBy;
use core::ops::Range;
// use crate::bus::BusDevice;
use crate::memory::ZxMemory;
use crate::clock::{VFrameTsCounter, MemoryContention, VideoTs, Ts, VideoTsData3};
use crate::video::{Renderer, BorderSize, PixelBuffer, VideoFrame, Video, MAX_BORDER_SIZE};
use super::{Ula, frame_cache::{Coords, pixel_address_coords, color_address_coords}};
use super::frame_cache::{UlaFrameCache, UlaFrameProducer};

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct UlaVideoFrame;

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct UlaMemoryContention;

pub type UlaTsCounter = VFrameTsCounter<UlaVideoFrame, UlaMemoryContention>;

impl MemoryContention for UlaMemoryContention {}

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
        if hc >= -1 && hc < 124 {
            let ct = (hc + 1) & 7;
            if ct < 6 {
                return hc + 6 - ct;
            }
        }
        hc
    }

    #[inline(always)]
    fn floating_bus_offset(VideoTs{vc, hc}: VideoTs) -> Option<u16> {
        if Self::is_contended_line_mreq(vc) {
            // println!("floating_bus_offset: {},{} {}", vc, hc, crate::clock::VFrameTsCounter::<Self>::vc_hc_to_tstates(vc, hc));
            match hc {
                c @ 0..=123 if c & 4 == 0 => Some(c as u16),
                _ => None
            }
        }
        else {
            None
        }
    }
}

impl<M: ZxMemory, B> Video for Ula<M, B> {
    type VideoFrame = UlaVideoFrame;

    #[inline]
    fn border_color(&self) -> u8 {
        self.ula_border_color()
    }

    fn set_border_color(&mut self, border: u8) {
        self.ula_set_border_color(border)
    }

    fn render_video_frame<P: PixelBuffer>(&mut self, buffer: &mut [u8], pitch: usize, border_size: BorderSize) {
        self.create_renderer(border_size).render_pixels::<P, Self::VideoFrame>(buffer, pitch)
    }
}

impl<M: ZxMemory, B> Ula<M, B> {
    #[inline(always)]
    pub(super) fn update_frame_pixels_and_colors(&mut self, addr: u16, ts: VideoTs) {
        if let Some(coords) = pixel_address_coords(addr) {
            self.frame_cache.update_frame_pixels(&self.memory, coords, addr, ts);
        }
        else if let Some(coords) = color_address_coords(addr) {
            self.frame_cache.update_frame_colors(&self.memory, coords, addr, ts);
        }
    }
}

impl<M: ZxMemory, B, V> Ula<M, B, V> {
    #[inline]
    pub(crate) fn ula_border_color(&self) -> u8 {
        self.last_border
    }
    #[inline]
    pub(crate) fn ula_set_border_color(&mut self, border: u8) {
        let border = border & 7;
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
        let border = self.border as usize;
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

    #[inline]
    pub(crate) fn cleanup_video_frame_data(&mut self) {
        self.border = self.last_border;
        self.border_out_changes.clear();
        self.frame_cache.clear();
    }
}
