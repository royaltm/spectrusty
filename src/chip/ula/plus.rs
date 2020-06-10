use core::iter::Empty;

use crate::bus::BusDevice;
use crate::clock::{VideoTs, MemoryContention, VFrameTsCounter};
use crate::chip::{UlaPortFlags, ula::frame_cache::UlaFrameCache};
use crate::memory::{ZxMemory, MemoryExtension};
use crate::video::{BorderColor, VideoFrame};
use super::Ula;
use super::super::plus::UlaPlusInner;

impl<'a, M, B, X, V> UlaPlusInner<'a> for Ula<M, B, X, V>
    where M: ZxMemory,
          B: BusDevice<Timestamp=VideoTs>,
          X: MemoryExtension,
          V: VideoFrame
{
    type ScreenSwapIter = Empty<VideoTs>;

    fn is_ula_port(port: u16) -> bool {
        port & 1 == 0
    }

    fn ula_write_earmic(&mut self, flags: UlaPortFlags, ts: VideoTs) {
        self.ula_write_earmic(flags, ts)
    }

    fn push_screen_change(&mut self, _ts: VideoTs) {}

    fn update_last_border_color(&mut self, border: BorderColor) -> bool {
        if self.last_border != border {
            self.last_border = border;
            return true
        }
        false
    }

    fn prepare_next_frame<T: MemoryContention>(
        &mut self,
        vtsc: VFrameTsCounter<Self::VideoFrame, T>
    ) -> VFrameTsCounter<Self::VideoFrame, T> {
        self.prepare_next_frame(vtsc)
    }

    fn set_video_counter(&mut self, vts: VideoTs) {
        self.tsc = vts;
    }

    fn page1_screen0_shadow_bank(&self) -> Option<bool> {
        Some(false)
    }

    fn page1_screen1_shadow_bank(&self) -> Option<bool> {
        Some(false)
    }

    fn page3_screen0_shadow_bank(&self) -> Option<bool> {
        None
    }

    fn page3_screen1_shadow_bank(&self) -> Option<bool> {
        None
    }

    fn frame_cache_mut_mem_ref(&mut self) -> (&mut UlaFrameCache<Self::VideoFrame>, &Self::Memory) {
        (&mut self.frame_cache, &self.memory)
    }

    fn shadow_frame_cache_mut_mem_ref(&mut self) -> (&mut UlaFrameCache<Self::VideoFrame>, &Self::Memory) {
        (&mut self.frame_cache, &self.memory)
    }

    fn beg_screen_shadow(&self) -> bool {
        false
    }

    fn cur_screen_shadow(&self) -> bool {
        false
    }

    fn video_render_data_view(
        &mut self
    ) -> (
            Self::ScreenSwapIter,
            &Self::Memory,
            &UlaFrameCache<Self::VideoFrame>,
            &UlaFrameCache<Self::VideoFrame>
        )
    {
        (
            core::iter::empty(),
            &self.memory,
            &self.frame_cache,
            &self.frame_cache
        )
    }
}
