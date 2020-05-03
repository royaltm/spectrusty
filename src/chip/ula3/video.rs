use core::iter::StepBy;
use core::ops::Range;

#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};

use crate::clock::{VideoTs, Ts};
use crate::chip::{
    ula::frame_cache::{pixel_address_coords, color_address_coords},
    ula128::{Ula128VidFrame, video::create_ula128_renderer}
};
use crate::video::{
    BorderSize, BorderColor, PixelBuffer, Palette,
    VideoFrame, Video
};
use super::Ula3;

#[derive(Clone, Copy, Default, Debug, PartialEq)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
pub struct Ula3VidFrame;

impl VideoFrame for Ula3VidFrame {
    /// A range of horizontal T-states, 0 should be where the frame starts.
    const HTS_RANGE: Range<Ts> = Ula128VidFrame::HTS_RANGE;
    /// The first video scan line index of the top border.
    const VSL_BORDER_TOP: Ts = Ula128VidFrame::VSL_BORDER_TOP;
    /// A range of video scan line indexes for the pixel area.
    const VSL_PIXELS: Range<Ts> = Ula128VidFrame::VSL_PIXELS;
    /// The last video scan line index of the bottom border.
    const VSL_BORDER_BOT: Ts = Ula128VidFrame::VSL_BORDER_BOT;
    /// A total number of video scan lines.
    const VSL_COUNT: Ts = Ula128VidFrame::VSL_COUNT;

    type HtsIter = StepBy<Range<Ts>>;

    #[inline(always)]
    fn border_whole_line_hts_iter(border_size: BorderSize) -> Self::HtsIter {
        Ula128VidFrame::border_whole_line_hts_iter(border_size)
    }
    #[inline(always)]
    fn border_left_hts_iter(border_size: BorderSize) -> Self::HtsIter {
        Ula128VidFrame::border_left_hts_iter(border_size)
    }
    #[inline(always)]
    fn border_right_hts_iter(border_size: BorderSize) -> Self::HtsIter {
        Ula128VidFrame::border_right_hts_iter(border_size)
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
        self.ula.border_color()
    }

    fn set_border_color(&mut self, border: BorderColor) {
        self.ula.set_border_color(border)
    }

    fn render_video_frame<'a, B: PixelBuffer<'a>, P: Palette<Pixel=B::Pixel>>(
            &mut self,
            buffer: &'a mut [u8],
            pitch: usize,
            border_size: BorderSize
        )
    {
        create_ula128_renderer(border_size,
                               &mut self.ula,
                               self.beg_screen_shadow,
                               &self.shadow_frame_cache,
                               &mut self.screen_changes)
        .render_pixels::<B, P, Self::VideoFrame>(buffer, pitch)
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
}

#[cfg(test)]
mod tests {
    use super::*;
    type TestVideoFrame = Ula3VidFrame;
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
                let mut vts = TestVideoFrame::vts_add_ts(vts0, testing + offset as u32);
                vts.hc = TestVideoFrame::contention(vts.hc);
                assert_eq!(TestVideoFrame::normalize_vts(vts),
                           TestVideoFrame::tstates_to_vts(target + offset));
            }
        }
        let refts = tstates[0].0 as i32;
        for ts in (refts - 100..refts)
            .chain(refts + 128..refts+TestVideoFrame::HTS_COUNT as i32) {
            let vts = TestVideoFrame::tstates_to_vts(ts);
            assert_eq!(TestVideoFrame::contention(vts.hc), vts.hc);
        }
    }

    #[test]
    fn test_video_frame_vts_utils() {
        let items = [((  0, -73),   -73, ( 0, 70835), false, true , (  0, -73)),
                     ((  0,   0),     0, ( 1,     0), false, true , (  0,   0)),
                     ((  0,  -1),    -1, ( 0, 70907), false, true , (  0,  -1)),
                     (( -1,   0),  -228, ( 0, 70680), false, true , ( -1,   0)),
                     ((  1,   0),   228, ( 1,   228), false, true , (  1,   0)),
                     ((311,  -1), 70907, ( 1, 70907), true , true , (311,  -1)),
                     ((311,   0), 70908, ( 2,     0), true , true , (311,   0)),
                     ((  0, 228),   228, ( 1,   228), false, false, (  1,   0)),
                     ((622,-227),141589, ( 2, 70681), true,  false, (621,   1))];
        for ((vc, hc), fts, (nfr, nfts), eof, is_norm, (nvc, nhc)) in items.iter().copied() {
            let vts = VideoTs::new(vc, hc);
            let nvts = VideoTs::new(nvc, nhc);
            assert_eq!(TestVideoFrame::vc_hc_to_tstates(vc, hc), fts);
            assert_eq!(TestVideoFrame::vts_to_tstates(vts), fts);
            assert_eq!(TestVideoFrame::tstates_to_vts(fts), nvts);
            assert_eq!(TestVideoFrame::vts_to_norm_tstates(1, vts), (nfr, nfts));
            assert_eq!(TestVideoFrame::is_vts_eof(vts), eof);
            assert_eq!(TestVideoFrame::is_normalized_vts(vts), is_norm);
            assert_eq!(TestVideoFrame::normalize_vts(vts), nvts);
        }
        assert_eq!(TestVideoFrame::vts_max(), VideoTs::new(i16::max_value(), 154));
        assert_eq!(TestVideoFrame::vts_min(), VideoTs::new(i16::min_value(), -73));
        let items = [((  0,   0),     0, (  0,   0)),
                     ((  0,   0),     1, (  0,   1)),
                     (( -1, 154),     1, (  0, -73)),
                     ((  0,   0),   228, (  1,   0)),
                     (( -1,   1),   227, (  0,   0)),
                     ((  0,   0), 70908, (311,   0)),
                     ((  1,  -1), 70908, (312,  -1)),
                     ((  2, 228), 70908, (314,   0))];
        for ((vc0, hc0), delta, (vc1, hc1)) in items.iter().copied() {
            let vts0 = VideoTs::new(vc0, hc0);
            let vts1 = VideoTs::new(vc1, hc1);
            assert_eq!(TestVideoFrame::vts_add_ts(vts0, delta), vts1);
            assert_eq!(TestVideoFrame::vts_diff(vts0, vts1), delta as i32);
            assert_eq!(TestVideoFrame::vts_diff(vts1, vts0), -(delta as i32));
        }
        let items = [((   311,      0), (     0,      0)),
                     ((   311,    -73), (     0,    -73)),
                     ((   621,    154), (   310,    154)),
                     ((     0,    228), (  -311,    228)),
                     ((-32767, -32768), (-32768, -32768)),
                     ((-32768, -32768), (-32768, -32768))];
        for ((vc0, hc0), (vc1, hc1)) in items.iter().copied() {
            let vts0 = VideoTs::new(vc0, hc0);
            let vts1 = VideoTs::new(vc1, hc1);
            assert_eq!(TestVideoFrame::vts_saturating_sub_frame(vts0), vts1);
        }
        let items = [((     0,      0), (     0,      0), (     0,      0), (     0,      0)),
                     ((     1,      1), (     1,      1), (     0,      0), (     2,      2)),
                     ((     1,      1), (    -1,     -1), (     2,      2), (     0,      0)),
                     ((     1,    154), (     1,      1), (     0,    153), (     3,    -73)),
                     ((-32768,    -73), (     1,      1), (-32768,    -73), (-32767,    -72)),
                     ((-32768,    -73), (-32768,    -73), (     0,      0), (-32768,    -73)),
                     (( 32767,    154), (     1,      1), ( 32766,    153), ( 32767,    154)),
                     (( 32767,    154), ( 32767,    154), (     0,      0), ( 32767,    154))];
        for ((vc0, hc0), (vc1, hc1), (svc, shc), (avc, ahc)) in items.iter().copied() {
            let vts0 = VideoTs::new(vc0, hc0);
            let vts1 = VideoTs::new(vc1, hc1);
            let subvts = VideoTs::new(svc, shc);
            let addvts = VideoTs::new(avc, ahc);
            assert_eq!(TestVideoFrame::vts_saturating_sub_vts_normalized(vts0, vts1), subvts);
            assert_eq!(TestVideoFrame::vts_saturating_add_vts_normalized(vts0, vts1), addvts);
            assert_eq!(TestVideoFrame::vts_saturating_add_vts_normalized(vts1, vts0), addvts);
        }
    }
}
