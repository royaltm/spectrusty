use core::iter::StepBy;
use core::ops::Range;

#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};

use crate::memory::ZxMemory;
use crate::clock::{VideoTs, Ts, VideoTsData3};
use crate::chip::{
    ula::frame_cache::{pixel_address_coords, color_address_coords},
    ula128::frame_cache::Ula128FrameProducer
};
use crate::video::{
    Renderer, BorderSize, BorderColor, PixelBuffer, Palette,
    VideoFrame, Video, MAX_BORDER_SIZE
};
use super::Ula3;

#[derive(Clone, Copy, Default, Debug, PartialEq)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
pub struct Ula3VidFrame;

impl VideoFrame for Ula3VidFrame {
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
    fn is_contended_line_no_mreq(_vsl: Ts) -> bool {
        false
    }

    #[inline]
    fn contention(hc: Ts) -> Ts {
        if hc >= -3 && hc < 125 {
            let ct = (hc + 2) & 7;
            if ct != 0 {
                return hc + 8 - ct;
            }
        }
        hc
    }
}

impl<D, X> Video for Ula3<D, X> {
    type VideoFrame = Ula3VidFrame;

    #[inline]
    fn border_color(&self) -> BorderColor {
        self.ula.ula_border_color()
    }

    fn set_border_color(&mut self, border: BorderColor) {
        self.ula.ula_set_border_color(border)
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

impl<B, X> Ula3<B, X> {
    #[inline]
    pub(super) fn update_frame_cache(&mut self, addr: u16, ts: VideoTs) {
        let maybe_shadow = match addr {
            0x4000..=0x5AFF => self.page1_screen_shadow_bank(),
            0xC000..=0xDAFF => self.page3_screen_shadow_bank(),
            _ => return
        };
        let frame_cache = match maybe_shadow {
            Some(false) => &mut self.ula.frame_cache,
            Some(true)  => &mut self.shadow_frame_cache,
            None => return
        };
        if addr & 0x1800 != 0x1800 {
            let coords = pixel_address_coords(addr);
            frame_cache.update_frame_pixels(&self.ula.memory, coords, addr, ts);
        }
        else {
            let coords = color_address_coords(addr);
            frame_cache.update_frame_colors(&self.ula.memory, coords, addr, ts);
        }
    }

    fn create_renderer<'a>(
            &'a mut self,
            border_size: BorderSize
        ) -> Renderer<Ula128FrameProducer<'a, Ula3VidFrame>, std::vec::Drain<'a, VideoTsData3>>
    {
        let swap_screens = self.beg_screen_shadow;
        let border = self.ula.ula_border_color().into();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contention() {
        let vts0 = VideoTs::new(0, 0);
        let tstates = [(14361, 14362),
                       (14362, 14362),
                       (14363, 14370),
                       (14364, 14370),
                       (14365, 14370),
                       (14366, 14370),
                       (14367, 14370),
                       (14368, 14370)];
        for offset in (0..16).map(|x| x * 8i32) {
            for (testing, target) in tstates.iter().copied() {
                let mut vts = Ula3VidFrame::vts_add_ts(vts0, testing + offset as u32);
                vts.hc = Ula3VidFrame::contention(vts.hc);
                assert_eq!(Ula3VidFrame::normalize_vts(vts),
                           Ula3VidFrame::tstates_to_vts(target + offset));
            }
        }
        let refts = tstates[0].0 as i32;
        for ts in (refts - 100..refts)
            .chain(refts + 128..refts+Ula3VidFrame::HTS_COUNT as i32) {
            let vts = Ula3VidFrame::tstates_to_vts(ts);
            assert_eq!(Ula3VidFrame::contention(vts.hc), vts.hc);
        }
    }
}
