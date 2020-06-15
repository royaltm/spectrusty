//! Parallel port device designed for parallel printer but other devices can be also emulated.
//!
use core::marker::PhantomData;
use std::io;

use spectrusty_core::clock::{FTs, VideoTs};
use spectrusty_core::video::VideoFrame;

#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};

/// An interface for emulating communication between a Spectrum's or other peripherals' hardware port
/// and remote parallel devices.
///
/// Emulators of peripheral devices should implement this trait.
///
/// Methods of this trait are being called by bus devices implementing parallel port communication.
pub trait ParallelPortDevice {
    type Timestamp: Sized;
    /// A device receives a byte written to the parallel data port.
    fn write_data(&mut self, data: u8, timestamp: Self::Timestamp);
    /// A device receives a changed `STROBE` bit status.
    /// `strobe` is `true` if the `STROBE` went high, `false` when it went low.
    ///
    /// Should return the `BUSY` status signal:
    /// * `true` if the `BUSY` singal is high,
    /// * `false` when it's low.
    fn write_strobe(&mut self, strobe: bool, timestamp: Self::Timestamp) -> bool;
    /// This method is being called once every frame, near the end of it and should return a `BUSY` signal state.
    /// Returns `true` if the `BUSY` signal is high, and `false` when it's low.
    fn poll_busy(&mut self) -> bool;
    /// Called when the current frame ends to allow emulators to wrap stored timestamps.
    fn next_frame(&mut self);
}

/// Emulates a parallel port device with a custom writer.
#[derive(Default, Clone, Debug)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "snapshot", serde(rename_all = "camelCase"))]
pub struct ParallelPortWriter<V, W> {
    pub writer: W,
    busy: bool,
    data: u8,
    last_ts: VideoTs,
    #[cfg_attr(feature = "snapshot", serde(skip))]
    _video_frame: PhantomData<V>
}

/// A parallel port device that does nothing and provides a constant low `BUSY` signal.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
pub struct NullParallelPort<T>(core::marker::PhantomData<T>);

const STROBE_TSTATES_MAX: FTs = 10000;

impl<V, W: io::Write> ParallelPortWriter<V, W> {
    fn write_byte_to_writer(&mut self) -> bool {
        let buf = core::slice::from_ref(&self.data);
        loop {
            return match self.writer.write(buf) {
                Ok(0) => false,
                Ok(..) => true,
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => panic!("an error occured while writing {}", e)
            }
        }
    }
}

impl<V: VideoFrame, W: io::Write> ParallelPortDevice for ParallelPortWriter<V, W> {
    type Timestamp = VideoTs;

    fn write_data(&mut self, data: u8, timestamp: Self::Timestamp) {
        self.data = data;
        self.last_ts = timestamp;
    }

    fn write_strobe(&mut self, strobe: bool, timestamp: Self::Timestamp) -> bool {
        if strobe {}
        else if V::vts_diff(self.last_ts, timestamp) <  STROBE_TSTATES_MAX {
            self.busy = !self.write_byte_to_writer();
            self.last_ts = V::vts_min();
        }
        else {
            // println!("centronics timeout: {} >= {}", V::vts_diff(self.last_ts, timestamp), STROBE_TSTATES_MAX);
        }
        self.busy
    }

    fn poll_busy(&mut self) -> bool {
        if self.busy {
            self.busy = !self.write_byte_to_writer();
        }
        self.busy
    }

    fn next_frame(&mut self) {
        self.last_ts = V::vts_saturating_sub_frame(self.last_ts);
    }
}

impl<T> ParallelPortDevice for NullParallelPort<T> {
    type Timestamp = T;

    #[inline(always)]
    fn write_data(&mut self, _data: u8, _timestamp: Self::Timestamp) {}
    #[inline(always)]
    fn write_strobe(&mut self, _strobe: bool, _timestamp: Self::Timestamp) -> bool {
        false
    }
    #[inline(always)]
    fn poll_busy(&mut self) -> bool {
        false
    }
    #[inline(always)]
    fn next_frame(&mut self) {}
}
