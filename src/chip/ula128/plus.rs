use std::vec::Drain;

use crate::bus::BusDevice;
use crate::clock::{VideoTs, MemoryContention, VFrameTsCounter};
use crate::chip::{UlaPortFlags, ula::frame_cache::UlaFrameCache};
use crate::memory::MemoryExtension;
use crate::video::BorderColor;
use super::Ula128;
use super::super::plus::UlaPlusInner;

impl<'a, B, X> UlaPlusInner<'a> for Ula128<B, X>
    where B: BusDevice<Timestamp=VideoTs>,
          X: MemoryExtension
{
    type ScreenSwapIter = Drain<'a, VideoTs>;

    fn is_ula_port(port: u16) -> bool {
        port & 1 == 0
    }

    fn ula_write_earmic(&mut self, flags: UlaPortFlags, ts: VideoTs) {
        self.ula.ula_write_earmic(flags, ts)
    }

    fn push_screen_change(&mut self, ts: VideoTs) {
        self.screen_changes.push(ts);
    }

    fn update_last_border_color(&mut self, border: BorderColor) -> bool {
        self.ula.update_last_border_color(border)
    }

    fn prepare_next_frame<T: MemoryContention>(
        &mut self,
        vtsc: VFrameTsCounter<Self::VideoFrame, T>
    ) -> VFrameTsCounter<Self::VideoFrame, T> {
        self.prepare_next_frame(vtsc)
    }

    fn set_video_counter(&mut self, vts: VideoTs) {
        self.ula.tsc = vts;
    }

    fn page1_screen0_shadow_bank(&self) -> Option<bool> {
        Some(false)
    }

    fn page1_screen1_shadow_bank(&self) -> Option<bool> {
        Some(false)
    }

    fn page3_screen0_shadow_bank(&self) -> Option<bool> {
        self.page3_screen_shadow_bank()
    }

    fn page3_screen1_shadow_bank(&self) -> Option<bool> {
        self.page3_screen_shadow_bank()
    }

    fn frame_cache_mut_mem_ref(&mut self) -> (&mut UlaFrameCache<Self::VideoFrame>, &Self::Memory) {
        (&mut self.ula.frame_cache, &self.ula.memory)
    }

    fn shadow_frame_cache_mut_mem_ref(&mut self) -> (&mut UlaFrameCache<Self::VideoFrame>, &Self::Memory) {
        (&mut self.shadow_frame_cache, &self.ula.memory)
    }

    fn beg_screen_shadow(&self) -> bool {
        self.beg_screen_shadow
    }

    fn cur_screen_shadow(&self) -> bool {
        self.cur_screen_shadow
    }

    fn video_render_data_view(
        &mut self
    ) -> (
            Drain<'_, VideoTs>,
            &Self::Memory,
            &UlaFrameCache<Self::VideoFrame>,
            &UlaFrameCache<Self::VideoFrame>
        )
    {
        (
            self.screen_changes.drain(..),
            &self.ula.memory,
            &self.ula.frame_cache,
            &self.shadow_frame_cache
        )
    }
}
