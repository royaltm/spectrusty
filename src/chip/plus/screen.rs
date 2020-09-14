/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
use crate::chip::{
    MemoryAccess,
    scld::frame_cache::SourceMode
};
use crate::memory::{ZxMemory};
use crate::formats::scr::*;
use crate::video::{Video, RenderMode};
use super::{UlaPlus, UlaPlusInner};

impl<'a, U> ScreenDataProvider for UlaPlus<U>
    where U: UlaPlusInner<'a>,
          Self: Video
{
    fn get_screen_mode(&self) -> ScrMode {
        let render_mode = self.cur_render_mode;
        if render_mode.is_hi_res() {
            let mode = ((render_mode & RenderMode::COLOR_MASK).bits() << 3) | 0b110;
            ScrMode::HighRes(mode)
        }
        else if self.cur_source_mode.is_hi_color() {
            ScrMode::HighColor(render_mode.is_palette())
        }
        else {
            ScrMode::Classic(render_mode.is_palette())
        }
    }

    fn set_screen_mode(&mut self, mode: ScrMode) -> bool {
        if self.ulaplus_disabled && mode != ScrMode::Classic(false) {
            return false;
        }
        let mut render_mode = self.cur_render_mode;
        let mut source_mode = self.cur_source_mode;
        let color = match mode {
            ScrMode::Classic(palette) =>  {
                source_mode.remove(SourceMode::SOURCE_MASK);
                render_mode.remove(RenderMode::COLOR_MODE|RenderMode::HI_RESOLUTION);
                render_mode.set(RenderMode::PALETTE, palette);
                self.ula.border_color().into()
            }
            ScrMode::HighColor(palette) => {
                source_mode.remove(SourceMode::SOURCE_MASK);
                source_mode.insert(SourceMode::ATTR_HI_COLOR);
                render_mode.remove(RenderMode::COLOR_MODE|RenderMode::HI_RESOLUTION);
                render_mode.set(RenderMode::PALETTE, palette);
                self.ula.border_color().into()
            }
            ScrMode::HighRes(mode) => {
                source_mode.remove(SourceMode::SOURCE_MASK);
                source_mode.insert(SourceMode::ATTR_HI_COLOR);
                render_mode.remove(RenderMode::COLOR_MODE);
                render_mode.insert(RenderMode::HI_RESOLUTION);
                mode >> 3
            }
        };
        let vts = self.ula.current_video_ts();
        self.update_source_mode(source_mode, vts);
        self.update_render_mode(render_mode.with_color(color), vts);
        true
    }

    fn screen_primary_ref(&self) -> &ScreenArray {
        let mut screen = self.visible_screen_bank();
        if self.cur_source_mode.is_second_screen() {
            screen += if U::Memory::SCR_BANKS_MAX >= 3 { 2 } else { 1 };
        }
        self.memory_ref().screen_ref(screen).unwrap()
    }

    fn screen_primary_mut(&mut self) -> &mut ScreenArray {
        let screen = self.visible_screen_bank();
        self.memory_mut().screen_mut(screen).unwrap()
    }

    fn screen_secondary_ref(&self) -> &ScreenArray {
        let screen = self.visible_screen_bank()
                     + if U::Memory::SCR_BANKS_MAX >= 3 { 2 } else { 1 };
        self.memory_ref().screen_ref(screen).unwrap()
    }

    fn screen_secondary_mut(&mut self) -> &mut ScreenArray {
        let screen = self.visible_screen_bank()
                     + if U::Memory::SCR_BANKS_MAX >= 3 { 2 } else { 1 };
        self.memory_mut().screen_mut(screen).unwrap()
    }

    fn screen_palette_ref(&self) -> &[u8;64] {
        &self.cur_palette
    }

    fn screen_palette_mut(&mut self) -> &mut [u8;64] {
        &mut self.cur_palette
    }
}
