/*
    Copyright (C) 2020-2022  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
/*! **SCR** file format utilities.

Utilities in this module can work with the ULAplus extensions to the **SCR** format:

The type of the file is recognized by its total byte size:

|  size | description                                                                             |
|-------|-----------------------------------------------------------------------------------------|
|  6912 | Standard screen data.                                                                   |
|  6976 | Standard screen data + 64 palette registers.                                            |
| 12288 | Hi-color screen data as primary screen data followed by the secondary screen data.      |
| 12352 | Hi-color screen data + 64 palette registers.                                            |
| 12289 | Hi-res screen data (primary + secondary) followed by the hi-res color information byte. |
| 12353 | Hi-res screen data (primary + secondary + hi-res color) + 64 palette registers.         |

*/
use core::slice;
use std::io::{self, Read, Write, Seek, SeekFrom};

pub use spectrusty_core::memory::ScreenArray;

const PIXELS_SIZE: usize = 6144;
const SCR_SIZE: u64 = 6912;
const PALETTE_SIZE: u64 = 64;
const SCR_PLUS_SIZE: u64 = SCR_SIZE + PALETTE_SIZE;
const SCR_HICOLOR_SIZE: u64 = PIXELS_SIZE as u64 * 2;
const SCR_HICOLOR_PLUS_SIZE: u64 = SCR_HICOLOR_SIZE + PALETTE_SIZE;
const SCR_HIRES_SIZE: u64 = SCR_HICOLOR_SIZE + 1;
const SCR_HIRES_PLUS_SIZE: u64 = SCR_HICOLOR_SIZE + 1 + PALETTE_SIZE;

/// This enum indicates a current or requested screen mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrMode {
    /// The default ULA screen mode.
    /// The `bool` should be `true` if the ULAplus palette is or should be supported. Otherwise it's `false`.
    Classic(bool),
    /// The high color SCLD screen mode.
    /// The `bool` should be `true` if the ULAplus palette is or should be supported. Otherwise it's `false`.
    HighColor(bool),
    /// The high resultion SCLD screen mode.
    /// The `u8` is a high resolution color scheme on bits 3 to 5.
    /// The `bool` should be `true` if the ULAplus palette is or should be supported. Otherwise it's `false`.
    HighRes(u8, bool),
}

/// Utilities for loading and saving **SCR** files.
///
/// Methods of this trait are implemented automatically for types that implement [ScreenDataProvider].
pub trait LoadScr: ScreenDataProvider {
    /// Attempts to read the `SCR` file from the `src` and load it into the underlying implementation.
    ///
    /// # Errors
    /// This function will return an error if a file is not recognized as an `SCR` file
    /// or the requested screen mode exceeds the capacity of the underlying implementation.
    /// Other errors may also be returned from attempts to read the file.
    fn load_scr<R: Read + Seek>(&mut self, scr: R) -> io::Result<()>;
    /// Attempts to save the screen from the underlying implementation and write as the `SCR`
    /// file into the `dst`.
    ///
    /// # Errors
    /// This function may return an error from attempts to write the file.
    fn save_scr<W: Write>(&self, dst: W) -> io::Result<()>;
}

/// This trait should be implemented by the core chipset emulator types.
pub trait ScreenDataProvider {
    /// Should return a currently selected screen mode.
    fn get_screen_mode(&self) -> ScrMode;
    /// Should set a requested screen `mode` and return `true`.
    /// If the provided `mode` can not be set this method should return `false`.
    fn set_screen_mode(&mut self, mode: ScrMode) -> bool;
    /// Should return a reference to the primary screen data.
    fn screen_primary_ref(&self) -> &ScreenArray;
    /// Should return a mutable reference to the primary screen data.
    fn screen_primary_mut(&mut self) -> &mut ScreenArray;
    /// Should return a reference to the secondary screen data.
    ///
    /// # Panics
    /// This method may panic if the secondary screen is not supported by the underlying implementation.
    fn screen_secondary_ref(&self) -> &ScreenArray {
        unimplemented!()
    }
    /// Should return a mutable reference to the secondary screen data.
    ///
    /// # Panics
    /// This method may panic if the secondary screen is not supported by the underlying implementation.
    fn screen_secondary_mut(&mut self) -> &mut ScreenArray {
        unimplemented!()
    }
    /// Should return a reference to the ULAplus palette.
    ///
    /// # Panics
    /// This method may panic if ULAplus is not supported by the underlying implementation.
    fn screen_palette_ref(&self) -> &[u8;PALETTE_SIZE as usize] {
        unimplemented!()
    }
    /// Should return a mutable reference to the ULAplus palette.
    ///
    /// # Panics
    /// This method may panic if ULAplus is not supported by the underlying implementation.
    fn screen_palette_mut(&mut self) -> &mut [u8;PALETTE_SIZE as usize] {
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
            SCR_HIRES_SIZE|SCR_HIRES_PLUS_SIZE => {
                src.seek(SeekFrom::Current(
                    if end == SCR_HIRES_PLUS_SIZE {
                        -(PALETTE_SIZE as i64) - 1
                    }
                    else {
                        -1
                    }
                ))?;
                let mut color: u8 = 0;
                src.read_exact(slice::from_mut(&mut color))?;
                ScrMode::HighRes(color, end == SCR_HIRES_PLUS_SIZE)
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
            SCR_HICOLOR_SIZE|SCR_HICOLOR_PLUS_SIZE|
            SCR_HIRES_SIZE|SCR_HIRES_PLUS_SIZE => {
                src.read_exact(&mut self.screen_primary_mut()[0..PIXELS_SIZE])?;
                src.read_exact(&mut self.screen_secondary_mut()[0..PIXELS_SIZE])?;
            }
            _ => {}
        }
        if let SCR_PLUS_SIZE|SCR_HICOLOR_PLUS_SIZE|SCR_HIRES_PLUS_SIZE = end {
            src.seek(SeekFrom::Start(end - PALETTE_SIZE))?;
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
        if let ScrMode::HighRes(outmode, ..) = mode {
            dst.write_all(slice::from_ref(&outmode))?;
        }
        if let ScrMode::Classic(true)|
               ScrMode::HighColor(true)|
               ScrMode::HighRes(.., true) = mode {
            dst.write_all(self.screen_palette_ref())?;
        }
        Ok(())
    }
}
