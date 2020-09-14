/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
//! **TAPE** pulse signal encoding and decoding.
#![warn(unused_imports)]

mod decoding;
mod encoding;

pub mod consts {
    use core::num::NonZeroU32;
    /// Length of the lead pulse in T-states.
    pub const LEAD_PULSE_LENGTH : NonZeroU32 = unsafe { NonZeroU32::new_unchecked(2168) };
    /// Length of the 1st sync pulse in T-states.
    pub const SYNC_PULSE1_LENGTH: NonZeroU32 = unsafe { NonZeroU32::new_unchecked(667)  };
    /// Length of the 2nd sync pulse in T-states.
    pub const SYNC_PULSE2_LENGTH: NonZeroU32 = unsafe { NonZeroU32::new_unchecked(735)  };
    /// Length of the bit value 0 pulse in T-states.
    pub const ZERO_PULSE_LENGTH : NonZeroU32 = unsafe { NonZeroU32::new_unchecked(855)  };
    /// Length of the bit value 0 pulse in T-states.
    pub const ONE_PULSE_LENGTH  : NonZeroU32 = unsafe { NonZeroU32::new_unchecked(1710) };
    /// Length of the pause between *TAPE* blocks in T-states.
    pub const PAUSE_PULSE_LENGTH: NonZeroU32 = unsafe { NonZeroU32::new_unchecked(3_500_000) };

    /// The number of LEAD pulses for the header block.
    pub const LEAD_PULSES_HEAD: u16 = 8063;
    /// The number of LEAD pulses for the data block.
    pub const LEAD_PULSES_DATA: u16 = 3223;
}

pub use decoding::*;
pub use encoding::*;

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::{Cursor, Result, Seek, SeekFrom, Read};
    use crate::tap::*;

    #[test]
    fn tap_pulse_works() -> Result<()> { // env!("CARGO_MANIFEST_DIR")
        let file = File::open("../resources/read_tap_test.tap")?;
        let mut pulse_iter = read_tap_pulse_iter(file);
        let mut tap_writer = write_tap(Cursor::new(Vec::new()))?;
        assert_eq!(5, tap_writer.write_pulses_as_tap_chunks(&mut pulse_iter)?);
        assert_eq!(1, tap_writer.end_pulse_chunk()?);
        let tgt: Vec<u8> = tap_writer.into_inner().into_inner().into_inner();
        let mut file: File = pulse_iter.into_inner().into_inner().into_inner();
        file.seek(SeekFrom::Start(0))?;
        let mut src = Vec::new();
        file.read_to_end(&mut src)?;
        assert_eq!(src, tgt);
        Ok(())
    }
}
