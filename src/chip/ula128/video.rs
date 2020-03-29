use core::iter::StepBy;
use core::ops::Range;

#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};

use crate::memory::ZxMemory;
use crate::clock::{MemoryContention, VideoTs, Ts, VideoTsData3};
use crate::chip::ula::{
    frame_cache::{pixel_address_coords, color_address_coords}
};
use crate::video::{Renderer, BorderSize, PixelBuffer, VideoFrame, Video, MAX_BORDER_SIZE};
use super::{Ula128, frame_cache::Ula128FrameProducer};

#[derive(Clone, Copy, Default, Debug, PartialEq)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
pub struct Ula128VidFrame;

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Ula128MemContention;

impl MemoryContention for Ula128MemContention {
    #[inline]
    fn is_contended_address(addr: u16) -> bool {
        addr & 0x4000 == 0x4000
    }
}

impl VideoFrame for Ula128VidFrame {
    /// A range of horizontal T-states, 0 should be where the frame starts.
    const HTS_RANGE: Range<Ts> = -73..155;
    /// The first video scan line index of the top border.
    const VSL_BORDER_TOP: Ts = 15;
    /// A range of video scan line indexes for the pixel area.
    const VSL_PIXELS: Range<Ts> = 63..255;
    /// The last video scan line index of the bottom border.
    const VSL_BORDER_BOT: Ts = 303;
    /// A total number of video scan lines.
    const VSL_COUNT: Ts = 311;

    type HtsIter = StepBy<Range<Ts>>;

    fn border_whole_line_hts_iter(border_size: BorderSize) -> Self::HtsIter {
        let invborder = ((MAX_BORDER_SIZE - Self::border_size_pixels(border_size))/2) as Ts;
        (-22+invborder..154-invborder).step_by(4)
    }

    fn border_left_hts_iter(border_size: BorderSize) -> Self::HtsIter {
        let invborder = ((MAX_BORDER_SIZE - Self::border_size_pixels(border_size))/2) as Ts;
        (-22+invborder..2).step_by(4)
    }

    fn border_right_hts_iter(border_size: BorderSize) -> Self::HtsIter {
        let invborder = ((MAX_BORDER_SIZE - Self::border_size_pixels(border_size))/2) as Ts;
        (130..154-invborder).step_by(4)
    }

    #[inline]
    fn contention(hc: Ts) -> Ts {
        if hc >= -3 && hc < 122 {
            let ct = (hc + 3) & 7;
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
            match hc + 2 {
                c @ 0..=123 if c & 4 == 0 => Some(c as u16),
                _ => None
            }
        }
        else {
            None
        }
    }
}

impl<B, X> Video for Ula128<B, X> {
    type VideoFrame = Ula128VidFrame;

    #[inline]
    fn border_color(&self) -> u8 {
        self.ula.ula_border_color()
    }

    fn set_border_color(&mut self, border: u8) {
        self.ula.ula_set_border_color(border)
    }

    fn render_video_frame<P: PixelBuffer>(&mut self, buffer: &mut [u8], pitch: usize, border_size: BorderSize) {
        self.create_renderer(border_size).render_pixels::<P, Self::VideoFrame>(buffer, pitch)
    }
}

impl<B, X> Ula128<B, X> {
    #[inline(always)]
    pub(super) fn update_frame_pixels_and_colors(&mut self, addr: u16, ts: VideoTs) {
        let (frame_cache, match_addr) = match self.page3_screen_bank() {
            Some(0) => (&mut self.ula.frame_cache, addr & !0x8000),  // both page1 and page3 as screen 0
            Some(1) if addr & 0x8000 == 0x8000 => {
                       (&mut self.shadow_frame_cache, addr ^ 0x8000) // page3 as screen 1
            }
            _       => (&mut self.ula.frame_cache, addr),            // only page1 as screen 0
        };
        if let Some(coords) = pixel_address_coords(match_addr) {
            frame_cache.update_frame_pixels(&self.ula.memory, coords, addr, ts);
        }
        else if let Some(coords) = color_address_coords(match_addr) {
            frame_cache.update_frame_colors(&self.ula.memory, coords, addr, ts);
        }
    }
}

impl<B, X> Ula128<B, X> {
    fn create_renderer<'a>(
            &'a mut self,
            border_size: BorderSize
        ) -> Renderer<Ula128FrameProducer<'a>, std::vec::Drain<'a, VideoTsData3>>
    {
        let swap_screens = self.beg_screen_shadow;
        let border = self.ula.ula_border_color() as usize;
        let invert_flash = self.ula.frames.0 & 16 != 0;
        let (border_changes, memory, frame_cache0) = self.ula.ula_video_render_data_view();
        let frame_cache1 = &self.shadow_frame_cache;
        let screen0 = &memory.screen_ref(0).unwrap();
        let screen1 = &memory.screen_ref(1).unwrap();
        let frame_image_producer = Ula128FrameProducer::new(
            swap_screens,
            screen0,
            screen1,
            frame_cache0,
            frame_cache1,
            self.screen_changes.drain(..)
        );
        // print!("render: {} {:?}", screen_bank, screen.as_ptr());
        Renderer {
            frame_image_producer,
            border,
            border_size,
            border_changes: border_changes.drain(..),
            invert_flash
        }
    }
}
