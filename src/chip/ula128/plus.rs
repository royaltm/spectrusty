/*
    Copyright (C) 2020-2022  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
use std::vec::Drain;

use crate::clock::VideoTs;
use crate::chip::{UlaPortFlags, ula::frame_cache::UlaFrameCache};
use crate::memory::MemoryExtension;
use crate::video::BorderColor;
use super::{Ula128, super::plus::{UlaPlusInner, VideoRenderDataView}};

impl<'a, B, X> UlaPlusInner<'a> for Ula128<B, X>
    where X: MemoryExtension
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
    ) -> VideoRenderDataView<'_, Drain<'_, VideoTs>, Self::Memory, Self::VideoFrame>
    {
        VideoRenderDataView {
            screen_changes: self.screen_changes.drain(..),
            memory: &self.ula.memory,
            frame_cache: &self.ula.frame_cache,
            frame_cache_shadow: &self.shadow_frame_cache
        }
    }
}
