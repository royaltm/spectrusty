use crate::chip::MemoryAccess;
use crate::memory::{MemoryExtension, ZxMemory};
use crate::video::Video;
use crate::formats::scr::*;
use super::Ula3;

impl<B, X: MemoryExtension> ScreenDataProvider for Ula3<B, X> {
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
