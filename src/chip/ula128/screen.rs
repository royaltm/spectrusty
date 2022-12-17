/*
    Copyright (C) 2020-2022  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
use crate::chip::MemoryAccess;
use crate::memory::{MemoryExtension, ZxMemory};
use crate::video::Video;
use crate::formats::scr::*;
use super::Ula128;

impl<B, X: MemoryExtension> ScreenDataProvider for Ula128<B, X> {
    fn get_screen_mode(&self) -> ScrMode {
        ScrMode::Classic(false)
    }

    fn set_screen_mode(&mut self, mode: ScrMode) -> bool {
        mode == ScrMode::Classic(false)
    }

    fn screen_primary_ref(&self) -> &ScreenArray {
        let screen_bank = self.visible_screen_bank();
        self.memory_ref().screen_ref(screen_bank).unwrap()
    }

    fn screen_primary_mut(&mut self) -> &mut ScreenArray {
        let screen_bank = self.visible_screen_bank();
        self.memory_mut().screen_mut(screen_bank).unwrap()
    }
}
