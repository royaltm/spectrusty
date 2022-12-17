/*
    Copyright (C) 2020-2022  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
use crate::chip::MemoryAccess;
use crate::memory::{MemoryExtension, ZxMemory};
use crate::formats::scr::*;
use super::Ula;

impl<M, B, X, V> ScreenDataProvider for Ula<M, B, X, V>
    where M: ZxMemory,
          X: MemoryExtension
{
    fn get_screen_mode(&self) -> ScrMode {
        ScrMode::Classic(false)
    }

    fn set_screen_mode(&mut self, mode: ScrMode) -> bool {
        mode == ScrMode::Classic(false)
    }

    fn screen_primary_ref(&self) -> &ScreenArray {
        self.memory_ref().screen_ref(0).unwrap()
    }

    fn screen_primary_mut(&mut self) -> &mut ScreenArray {
        self.memory_mut().screen_mut(0).unwrap()
    }
}
