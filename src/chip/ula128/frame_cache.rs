use crate::clock::{Ts, VideoTs};
use crate::video::{
    VideoFrame,
    frame_cache::VideoFrameDataIterator
};
use crate::memory::ScreenArray;
use crate::chip::ula::frame_cache::{UlaFrameRef, UlaFrameCache, UlaFrameProducer};

// const COL_HTS: [Ts;32] = [3, 5, 11, 13, 19, 21, 27, 29, 35, 37, 43, 45, 51, 53, 59, 61, 67, 69, 75, 77, 83, 85, 91, 93, 99, 101, 107, 109, 115, 117, 123, 125];
#[inline]
fn col_hts(x: i16) -> i16 {
    ((x >> 1) << 3) + ((x & 1) << 1) + 3
}

/// Implements a [VideoFrameDataIterator] for 128k ULA based Spectrum models.
pub struct Ula128FrameProducer<'a, V, I> {
    ula_frame_prod: UlaFrameProducer<'a, V>,
    shadow_frame: UlaFrameRef<'a, V>,
    screen_changes: I,
    swap_at: VideoTs,
}

#[inline(always)]
pub(crate) fn vc_to_line<V: VideoFrame>(mut vts: VideoTs, line: Ts) -> VideoTs {
    vts.vc -= V::VSL_PIXELS.start + line;
    vts
}

impl<'a, V, I> VideoFrameDataIterator for Ula128FrameProducer<'a, V, I>
    where V: VideoFrame,
          I: Iterator<Item=VideoTs>
{
    fn next_line(&mut self) {
        self.ula_frame_prod.next_line();
        self.swap_at.vc -= 1;
    }
}

impl<'a, V, I> Ula128FrameProducer<'a, V, I>
    where V: VideoFrame,
          I: Iterator<Item=VideoTs>
{
    pub fn new(
            swap_screens: bool,
            screen0: &'a ScreenArray,
            screen1: &'a ScreenArray,
            frame_cache0: &'a UlaFrameCache<V>,
            frame_cache1: &'a UlaFrameCache<V>,
            mut screen_changes: I
        ) -> Self
    {
        let swap_at = screen_changes.next()
                      .map(|ts| vc_to_line::<V>(ts, 0))
                      .unwrap_or_else(V::vts_max);
        let mut ula_frame_prod = UlaFrameProducer::new(screen0, frame_cache0);
        let mut shadow_frame = UlaFrameRef::new(screen1, frame_cache1);
        if swap_screens {
            ula_frame_prod.swap_frame(&mut shadow_frame);
        }
        Ula128FrameProducer {
            ula_frame_prod,
            shadow_frame,
            screen_changes,
            swap_at
        }
    }

    fn swap_frames(&mut self) {
        self.ula_frame_prod.swap_frame(&mut self.shadow_frame);
    }
}

impl<'a, V, I> Iterator for Ula128FrameProducer<'a, V, I>
    where V: VideoFrame,
          I: Iterator<Item=VideoTs>
{
    type Item = (u8, u8);

    fn next(&mut self) -> Option<Self::Item> {
        while self.swap_at.vc <= 0 {
            if self.swap_at.vc < 0 ||
               self.swap_at.hc < col_hts(self.ula_frame_prod.column() as i16) {
                self.swap_frames();
                self.swap_at = self.screen_changes.next()
                            .map(|ts| vc_to_line::<V>(ts, self.ula_frame_prod.line() as Ts))
                            .unwrap_or_else(V::vts_max)
            }
            else {
                break;
            }
        }
        self.ula_frame_prod.next()
    }
}
