use std::vec::Drain;
// use core::slice;
use core::fmt;
use core::marker::PhantomData;
use crate::clock::{Ts, VideoTsData1, VideoTs};
use crate::memory::{ZxMemory};
use crate::video::{pixel_line_offset, color_line_offset, VideoFrame,
    frame_cache::{PixelPack8, FrameImageProducer}
};
use crate::chip::ula::frame_cache::{UlaFrameCache, UlaFrameLineIter, UlaFrameProducer};
use super::Ula128VidFrame;
// UlaFrameLineIter<'a>
const COL_HTS:  &[Ts;32] = &[3, 5, 11, 13, 19, 21, 27, 29, 35, 37, 43, 45, 51, 53, 59, 61, 67, 69, 75, 77, 83, 85, 91, 93, 99, 101, 107, 109, 115, 117, 123, 125];
// ((x >> 1) << 3) + ((x & 1) << 1)
pub struct Ula128FrameProducer<'a> {
    ula_frame0_prod: UlaFrameProducer<'a, Ula128VidFrame>,
    ula_frame1_prod: UlaFrameProducer<'a, Ula128VidFrame>,
    screen_changes: Drain<'a, VideoTs>,
    swap_at: Option<VideoTs>,
}

impl<'a> FrameImageProducer for Ula128FrameProducer<'a> {
    fn next_line(&mut self) {
        self.ula_frame0_prod.next_line();
        self.ula_frame1_prod.next_line();
    }
}

impl<'a> Ula128FrameProducer<'a> {
    pub fn new(
            swap_screens: bool,
            screen0: &'a[u8],
            screen1: &'a[u8],
            frame_cache0: &'a UlaFrameCache<Ula128VidFrame>,
            frame_cache1: &'a UlaFrameCache<Ula128VidFrame>,
            mut screen_changes: Drain<'a, VideoTs>
        ) -> Self
    {
        let swap_at = screen_changes.next();
        let mut ula_frame0_prod = UlaFrameProducer::new(screen0, frame_cache0);
        let mut ula_frame1_prod = UlaFrameProducer::new(screen1, frame_cache1);
        if swap_screens {
            core::mem::swap(&mut ula_frame0_prod, &mut ula_frame1_prod);
        }
        Ula128FrameProducer {
            ula_frame0_prod,
            ula_frame1_prod,
            screen_changes,
            swap_at
        }
    }

    fn swap_producers(&mut self) {
        core::mem::swap(&mut self.ula_frame0_prod, &mut self.ula_frame1_prod);
        self.ula_frame0_prod.set_column(self.ula_frame1_prod.column());
    }
}

impl<'a> Iterator for Ula128FrameProducer<'a> {
    type Item = PixelPack8;

    fn next(&mut self) -> Option<PixelPack8> {
        while let Some(swap_at) = self.swap_at {
            let vc = Ula128VidFrame::VSL_PIXELS.start + self.ula_frame0_prod.line() as Ts;
            let hc = COL_HTS[self.ula_frame0_prod.column() & 31];
            let ts = VideoTs::new(vc, hc);
            if ts < swap_at {
                break
            }
            self.swap_producers();
            self.swap_at = self.screen_changes.next();
        }
        self.ula_frame0_prod.next()
    }
}
