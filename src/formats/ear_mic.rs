//! **TAPE** EAR IN pulse signal feeder.
#![warn(unused_imports)]

mod ear_in;
mod mic_out;

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

pub use ear_in::*;
pub use mic_out::*;

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::{Cursor, Result, Seek, SeekFrom, Read};
    use crate::formats::tap::*;

    #[test]
    fn mic_ear_works() -> Result<()> {
        let file = File::open("tests/read_tap_test.tap")?;
        let mut pulse_iter = read_tap_ear_pulse_iter(file);
        let mut tap_writer = write_tap(Cursor::new(Vec::new()));
        assert_eq!(5, tap_writer.write_pulses_as_tap_chunks(&mut pulse_iter)?);
        assert_eq!(1, tap_writer.flush()?);
        let tgt: Vec<u8> = tap_writer.into_inner().into_inner().into_inner();
        let mut file: File = pulse_iter.into_inner().into_inner().into_inner();
        file.seek(SeekFrom::Start(0))?;
        let mut src = Vec::new();
        file.read_to_end(&mut src)?;
        assert_eq!(src, tgt);
        Ok(())
    }
}
