/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
use core::iter::StepBy;
use core::ops::Range;

#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};

use crate::memory::ZxMemory;
use crate::clock::{VideoTs, Ts, VideoTsData3, VFrameTsCounter, MemoryContention};
use crate::chip::ula::{
    Ula,
    frame_cache::UlaFrameCache
};
use crate::video::{
    Renderer, BorderSize, BorderColor, PixelBuffer, Palette,
    VideoFrame, Video, CellCoords, MAX_BORDER_SIZE,
    frame_cache::{pixel_address_coords, color_address_coords}
};
use super::{
    Ula128, Ula128MemContention,
    frame_cache::Ula128FrameProducer
};

/// Implements [VideoFrame] for ULA 128k.
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
pub struct Ula128VidFrame;

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

    type BorderHtsIter = StepBy<Range<Ts>>;

    fn border_whole_line_hts_iter(border_size: BorderSize) -> Self::BorderHtsIter {
        let invborder = ((MAX_BORDER_SIZE - Self::border_size_pixels(border_size))/2) as Ts;
        (-22+invborder..154-invborder).step_by(4)
    }

    fn border_left_hts_iter(border_size: BorderSize) -> Self::BorderHtsIter {
        let invborder = ((MAX_BORDER_SIZE - Self::border_size_pixels(border_size))/2) as Ts;
        (-22+invborder..2).step_by(4)
    }

    fn border_right_hts_iter(border_size: BorderSize) -> Self::BorderHtsIter {
        let invborder = ((MAX_BORDER_SIZE - Self::border_size_pixels(border_size))/2) as Ts;
        (130..154-invborder).step_by(4)
    }

    #[inline]
    fn contention(hc: Ts) -> Ts {
        if hc >= -3 && hc < 123 {
            let ct = (hc + 3) & 7;
            if ct < 6 {
                return hc + 6 - ct;
            }
        }
        hc
    }

    #[inline(always)]
    fn floating_bus_offset(hc: Ts) -> Option<u16> {
        // println!("floating_bus_offset: {},{} {}", vc, hc, crate::clock::VFrameTsCounter::<Self>::vc_hc_to_tstates(vc, hc));
        match hc + 2 {
            c @ 0..=123 if c & 4 == 0 => Some(c as u16),
            _ => None
        }
    }

    #[inline(always)]
    fn snow_interference_coords(VideoTs { vc, hc }: VideoTs) -> Option<CellCoords> {
        let row = vc - Self::VSL_PIXELS.start;
        if row >= 0 && vc < Self::VSL_PIXELS.end {
            if hc >= 0 && hc <= 123 {
                return match hc & 7 {
                    0|1 => Some(0),
                    2|3 => Some(1),
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

impl<D, X> Video for Ula128<D, X> {
    type VideoFrame = Ula128VidFrame;
    type Contention = Ula128MemContention;

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

    fn visible_screen_bank(&self) -> usize {
        self.cur_screen_shadow.into()
    }

    fn current_video_ts(&self) -> VideoTs {
        self.ula.current_video_ts()
    }

    fn current_video_clock(&self) -> VFrameTsCounter<Self::VideoFrame, Self::Contention> {
        let contention = self.memory_contention();
        VFrameTsCounter::from_video_ts(self.ula.current_video_ts(), contention)
    }

    fn set_video_ts(&mut self, vts: VideoTs) {
        self.ula.set_video_ts(vts);
    }

    fn flash_state(&self) -> bool {
        self.ula.flash_state()
    }
}

impl<B, X> Ula128<B, X> {
    #[inline]
    pub(super) fn update_frame_cache(&mut self, addr: u16, ts: VideoTs) {
        let frame_cache = match addr {
            0x4000..=0x5AFF => &mut self.ula.frame_cache,
            0xC000..=0xDAFF => match self.page3_screen_shadow_bank() {
                Some(false) => &mut self.ula.frame_cache,
                Some(true)  => &mut self.shadow_frame_cache,
                None => return
            }
            _ => return
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

    #[inline(always)]
    pub(super) fn update_snow_interference(&mut self, ts: VideoTs, ir: u16) {
        if self.memory_contention().is_contended_address(ir) {
            if let Some(coords) = Ula128VidFrame::snow_interference_coords(ts) {
                let (screen, frame_cache) = if self.cur_screen_shadow {
                    (self.ula.memory.screen_ref(1).unwrap(), &mut self.shadow_frame_cache)
                }
                else {
                    (self.ula.memory.screen_ref(0).unwrap(), &mut self.ula.frame_cache)
                };
                frame_cache.apply_snow_interference(screen, coords, ir as u8);
            }
        }
    }
}

pub(crate) fn create_ula128_renderer<'a, V, M, B, X>(
            border_size: BorderSize,
            ula: &'a mut Ula<M, B, X, V>,
            beg_screen_shadow: bool,
            shadow_frame_cache: &'a UlaFrameCache<V>,
            screen_changes:&'a mut Vec<VideoTs>
        ) -> Renderer<Ula128FrameProducer<'a, V, std::vec::Drain<'a, VideoTs>>, std::vec::Drain<'a, VideoTsData3>>
    where V: VideoFrame,
          M: ZxMemory,
          Ula<M, B, X, V>: Video
{
    let swap_screens = beg_screen_shadow;
    let border = ula.border_color().into();
    let invert_flash = ula.flash_state();
    let (border_changes, memory, frame_cache0) = ula.video_render_data_view();
    let frame_cache1 = shadow_frame_cache;
    let screen0 = memory.screen_ref(0).unwrap();
    let screen1 = memory.screen_ref(1).unwrap();
    let frame_image_producer = Ula128FrameProducer::new(
        swap_screens,
        screen0,
        screen1,
        frame_cache0,
        frame_cache1,
        screen_changes.drain(..)
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

#[cfg(test)]
mod tests {
    use crate::clock::{TimestampOps, VFrameTs};
    use super::*;
    type TestVideoFrame = Ula128VidFrame;
    type TestVFTs = VFrameTs<TestVideoFrame>;

    #[test]
    fn test_contention() {
        let vts0 = TestVFTs::new(0, 0);
        let tstates = [(14361, 14367),
                       (14362, 14367),
                       (14363, 14367),
                       (14364, 14367),
                       (14365, 14367),
                       (14366, 14367),
                       (14367, 14367),
                       (14368, 14368)];
        for offset in (0..16).map(|x| x * 8i32) {
            for (testing, target) in tstates.iter().copied() {
                let mut vts: TestVFTs = vts0 + testing + offset as u32;
                vts.hc = TestVideoFrame::contention(vts.hc);
                assert_eq!(vts.normalized(),
                           TestVFTs::from_tstates(target + offset));
            }
        }
        let refts = tstates[0].0 as i32;
        for ts in (refts - 100..refts)
            .chain(refts + 128..refts+TestVideoFrame::HTS_COUNT as i32) {
            let vts = TestVFTs::from_tstates(ts);
            assert_eq!(TestVideoFrame::contention(vts.hc), vts.hc);
        }
    }

    #[test]
    fn test_video_frame_vts_utils() {
        assert_eq!(TestVFTs::EOF, TestVFTs::from_tstates(TestVideoFrame::FRAME_TSTATES_COUNT));
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
            let vts = TestVFTs::new(vc, hc);
            let nvts = TestVFTs::new(nvc, nhc);
            assert_eq!(TestVideoFrame::vc_hc_to_tstates(vc, hc), fts);
            assert_eq!(vts.into_tstates(), fts);
            assert_eq!(TestVFTs::from_tstates(fts), nvts);
            assert_eq!(vts.into_frame_tstates(1), (nfr, nfts));
            assert_eq!(vts.is_eof(), eof);
            assert_eq!(vts.is_normalized(), is_norm);
            assert_eq!(vts.normalized(), nvts);
        }
        assert_eq!(TestVFTs::max_value(), TestVFTs::new(i16::max_value(), 154));
        assert_eq!(TestVFTs::min_value(), TestVFTs::new(i16::min_value(), -73));
        let items = [((  0,   0),     0, (  0,   0)),
                     ((  0,   0),     1, (  0,   1)),
                     (( -1, 154),     1, (  0, -73)),
                     ((  0,   0),   228, (  1,   0)),
                     (( -1,   1),   227, (  0,   0)),
                     ((  0,   0), 70908, (311,   0)),
                     ((  1,  -1), 70908, (312,  -1)),
                     ((  2, 228), 70908, (314,   0))];
        for ((vc0, hc0), delta, (vc1, hc1)) in items.iter().copied() {
            let vts0 = TestVFTs::new(vc0, hc0);
            let vts1 = TestVFTs::new(vc1, hc1);
            assert_eq!(vts0 + delta, vts1);
            assert_eq!(vts1.diff_from(vts0), delta as i32);
            assert_eq!(vts0.diff_from(vts1), -(delta as i32));
        }
        let items = [((   311,      0), (     0,      0)),
                     ((   311,    -73), (     0,    -73)),
                     ((   621,    154), (   310,    154)),
                     ((     0,    228), (  -311,    228)),
                     ((-32767, -32768), (-32768, -32768)),
                     ((-32768, -32768), (-32768, -32768))];
        for ((vc0, hc0), (vc1, hc1)) in items.iter().copied() {
            let vts0 = TestVFTs::new(vc0, hc0);
            let vts1 = TestVFTs::new(vc1, hc1);
            assert_eq!(vts0.saturating_sub_frame(), vts1);
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
            let vts0 = TestVFTs::new(vc0, hc0);
            let vts1 = TestVFTs::new(vc1, hc1);
            let subvts = TestVFTs::new(svc, shc);
            let addvts = TestVFTs::new(avc, ahc);
            assert_eq!(vts0.saturating_sub(vts1), subvts);
            assert_eq!(vts0.saturating_add(vts1), addvts);
            assert_eq!(vts1.saturating_add(vts0), addvts);
        }
    }
}
