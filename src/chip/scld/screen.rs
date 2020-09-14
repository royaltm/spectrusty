/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
use crate::chip::{ScldCtrlFlags, MemoryAccess};
use crate::memory::{MemoryExtension, PagedMemory8k, ZxMemory};
use crate::formats::scr::*;
use crate::video::{Video, VideoFrame};
use super::Scld;

impl<M, B, X, V> ScreenDataProvider for Scld<M, B, X, V>
    where M: ZxMemory + PagedMemory8k,
          X: MemoryExtension,
          V: VideoFrame
{
    fn get_screen_mode(&self) -> ScrMode {
        let flags = self.cur_ctrl_flags;
        if flags.is_screen_hi_res() {
            ScrMode::HighRes(flags.bits())
        }
        else if flags.is_screen_hi_attrs() {
            ScrMode::HighColor(false)
        }
        else {
            ScrMode::Classic(false)
        }
    }

    fn set_screen_mode(&mut self, mode: ScrMode) -> bool {
        let mut flags = self.cur_ctrl_flags;
        match mode {
            ScrMode::Classic(false) =>  {
                flags.remove(ScldCtrlFlags::SCREEN_MODE_MASK);
            }
            ScrMode::HighColor(false) => {
                flags.remove(ScldCtrlFlags::SCREEN_MODE_MASK);
                flags.insert(ScldCtrlFlags::SCREEN_HI_ATTRS);
            }
            ScrMode::HighRes(mode) => {
                flags.remove(ScldCtrlFlags::SCREEN_MODE_MASK|ScldCtrlFlags::HIRES_COLOR_MASK);
                flags.insert(ScldCtrlFlags::SCREEN_HI_ATTRS|ScldCtrlFlags::SCREEN_HI_RES|
                             (ScldCtrlFlags::from(mode) & ScldCtrlFlags::HIRES_COLOR_MASK));
            }
            _ => return false
        }
        self.set_ctrl_flags_value(flags, self.ula.current_video_ts());
        true
    }

    fn screen_primary_ref(&self) -> &ScreenArray {
        let screen = self.cur_ctrl_flags.is_screen_secondary();
        self.memory_ref().screen_ref(screen.into()).unwrap()
    }

    fn screen_primary_mut(&mut self) -> &mut ScreenArray {
        self.memory_mut().screen_mut(0).unwrap()
    }

    fn screen_secondary_ref(&self) -> &ScreenArray {
        self.memory_ref().screen_ref(1).unwrap()
    }

    fn screen_secondary_mut(&mut self) -> &mut ScreenArray {
        self.memory_mut().screen_mut(1).unwrap()
    }
}
