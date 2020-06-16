use core::slice;
use std::io::{self, Read, Write, Seek, SeekFrom};

pub use spectrusty_core::memory::ScreenArray;

const PIXELS_SIZE: usize = 6144;
const SCR_SIZE: u64 = 6912;
const SCR_PLUS_SIZE: u64 = SCR_SIZE + 64;
const SCR_HICOLOR_SIZE: u64 = PIXELS_SIZE as u64 * 2;
const SCR_HICOLOR_PLUS_SIZE: u64 = SCR_HICOLOR_SIZE + 64;
const SCR_HIRES_SIZE: u64 = SCR_HICOLOR_SIZE + 1;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrMode {
    Classic(bool),
    HighColor(bool),
    HighRes(u8),
}

pub trait LoadScr: ScreenDataProvider {
    fn load_scr<R: Read + Seek>(&mut self, scr: R) -> io::Result<()>;
    fn save_scr<W: Write>(&self, dst: W) -> io::Result<()>;
}

pub trait ScreenDataProvider {
    fn get_screen_mode(&self) -> ScrMode;
    fn set_screen_mode(&mut self, mode: ScrMode) -> bool;
    fn screen_primary_ref(&self) -> &ScreenArray;
    fn screen_primary_mut(&mut self) -> &mut ScreenArray;
    fn screen_secondary_ref(&self) -> &ScreenArray {
        unimplemented!()
    }
    fn screen_secondary_mut(&mut self) -> &mut ScreenArray {
        unimplemented!()
    }
    fn screen_palette_ref(&self) -> &[u8;64] {
        unimplemented!()
    }
    fn screen_palette_mut(&mut self) -> &mut [u8;64] {
        unimplemented!()
    }
}

impl<T> LoadScr for T where T: ScreenDataProvider {
    fn load_scr<R: Read + Seek>(&mut self, mut src: R) -> io::Result<()> {
        let end = src.seek(SeekFrom::End(0))?;
        let mode = match end {
            SCR_SIZE => ScrMode::Classic(false),
            SCR_PLUS_SIZE => ScrMode::Classic(true),
            SCR_HICOLOR_SIZE => ScrMode::HighColor(false),
            SCR_HICOLOR_PLUS_SIZE => ScrMode::HighColor(true),
            SCR_HIRES_SIZE => {
                src.seek(SeekFrom::Current(-1))?;
                let mut color: u8 = 0;
                src.read_exact(slice::from_mut(&mut color))?;
                ScrMode::HighRes(color)
            }
            _ => {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "Screen format not recognized"));
            }
        };
        if !self.set_screen_mode(mode) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Screen format not supported"));
        }
        src.seek(SeekFrom::Start(0))?;
        match end {
            SCR_SIZE|SCR_PLUS_SIZE => {
                src.read_exact(self.screen_primary_mut())?;
            }
            SCR_HICOLOR_SIZE|SCR_HICOLOR_PLUS_SIZE|SCR_HIRES_SIZE => {
                src.read_exact(&mut self.screen_primary_mut()[0..PIXELS_SIZE])?;
                src.read_exact(&mut self.screen_secondary_mut()[0..PIXELS_SIZE])?;
            }
            _ => {}
        }
        if end == SCR_PLUS_SIZE || end == SCR_HICOLOR_PLUS_SIZE {
            src.read_exact(self.screen_palette_mut())?;
        }
        Ok(())
    }

    fn save_scr<W: Write>(&self, mut dst: W) -> io::Result<()> {
        let mode = self.get_screen_mode();
        match mode {
            ScrMode::Classic(..) => dst.write_all(self.screen_primary_ref())?,
            ScrMode::HighColor(..)|ScrMode::HighRes(..) => {
                dst.write_all(&self.screen_primary_ref()[0..PIXELS_SIZE])?;
                dst.write_all(&self.screen_secondary_ref()[0..PIXELS_SIZE])?;
            }
        }
        match mode {
            ScrMode::Classic(true)|ScrMode::HighColor(true) => {
                dst.write_all(self.screen_palette_ref())
            }
            ScrMode::HighRes(mode) => {
                dst.write_all(slice::from_ref(&mode))
            }
            _ => Ok(())
        }
        
    }
}
