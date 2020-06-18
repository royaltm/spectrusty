//! T-state timestamp types and counters.
use core::fmt::Debug;
use core::marker::PhantomData;
use core::num::{NonZeroU8, NonZeroU16};
use core::ops::{AddAssign, Deref, DerefMut};

use z80emu::{Clock, host::cycles::*};
#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};

use crate::video::VideoFrame;

/// A linear T-state timestamp type.
pub type FTs = i32;
/// A type used for a horizontal T-state timestamp or a video scanline index for [VideoTs].
pub type Ts = i16;

/// A timestamp type that consists of two video counters: vertical and horizontal.
///
/// `VideoTs { vc: 0, hc: 0 }` marks the start of the video frame.
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
pub struct VideoTs {
    /// A vertical counter - a video scan-line index.
    pub vc: Ts,
    /// A horizontal counter - measured in T-states.
    pub hc: Ts,
}

/// A trait used by [VFrameTsCounter] for checking if an `address` is a contended one.
pub trait MemoryContention: Copy + Debug {
    fn is_contended_address(self, address: u16) -> bool;
}

/// A generic [VideoTs] based T-states counter.
///
/// Implements [Clock] for counting T-states when code is being executed by [z80emu::Cpu].
///
/// Counts additional T-states according to contention specified by generic parameters:
/// `V:` [VideoFrame] and `C:` [MemoryContention].
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct VFrameTsCounter<V, C>  {
    /// The current timestamp value of this counter.
    pub tsc: VideoTs,
    /// Each bit represents an 8 kilobyte memory page starting from the lowest (rightmost) bit.
    ///
    /// When a bit value is 1 the address range of the page is considered contended.
    pub contention: C,
    _video: PhantomData<V>,
}

/// If a vertical counter of [VideoTs] exceeds this value, signals the control unit to emulate hanging
/// CPU indefinitely.
pub const HALT_VC_THRESHOLD: i16 = i16::max_value() >> 1;

const WAIT_STATES_THRESHOLD: u16 = i16::max_value() as u16 - 256;

macro_rules! video_ts_packed_data {
    ($name:ident, $bits:literal) => {
        /// A T-states timestamp with packed N-bits data.
        #[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Debug)]
        pub struct $name {
            pub vc: Ts,
            hc_data: Ts,
        }

        impl From<(VideoTs, u8)> for $name {
            fn from(tuple: (VideoTs, u8)) -> Self {
                let (vts, data) = tuple;
                $name::new(vts, data)
            }
        }

        impl From<$name> for (VideoTs, u8) {
            fn from(vtsd: $name) -> Self {
                let $name { vc, hc_data } = vtsd;
                let data = (hc_data as u8) & ((1 << $bits) - 1);
                (VideoTs { vc, hc: hc_data >> $bits}, data)
            }
        }

        impl From<$name> for VideoTs {
            fn from(vtsd: $name) -> Self {
                let $name { vc, hc_data } = vtsd;
                VideoTs { vc, hc: hc_data >> $bits}
            }
        }

        impl From<&$name> for VideoTs {
            fn from(vtsd: &$name) -> Self {
                Self::from(*vtsd)
            }
        }

        impl $name {
            const DATA_MASK: Ts = (1 << $bits) - 1;
            #[inline]
            pub const fn new(vts: VideoTs, data: u8) -> Self {
                let VideoTs { vc, hc } = vts;
                let hc_data = (hc << $bits) | (data as Ts & Self::DATA_MASK) as Ts;
                Self { vc, hc_data }
            }

            #[inline]
            pub fn into_data(self) -> u8 {
                (self.hc_data & Self::DATA_MASK) as u8
            }

            #[inline]
            pub fn data(&self) -> u8 {
                (self.hc_data & Self::DATA_MASK) as u8
            }

            #[inline]
            pub fn set_data(&mut self, data: u8) {
                self.hc_data = (self.hc_data & !Self::DATA_MASK) | (data as Ts & Self::DATA_MASK);
            }

            #[inline]
            pub fn hc(&self) -> Ts {
                self.hc_data >> $bits
            }
        }
    }
}

video_ts_packed_data! { VideoTsData1, 1 }
video_ts_packed_data! { VideoTsData2, 2 }
video_ts_packed_data! { VideoTsData3, 3 }
video_ts_packed_data! { VideoTsData6, 6 }

macro_rules! fts_packed_data {
    ($name:ident, $bits:literal) => {
        /// A T-states timestamp with packed N-bits data.
        #[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Debug)]
        pub struct $name(FTs);

        impl From<(FTs, u8)> for $name {
            fn from(tuple: (FTs, u8)) -> Self {
                let (ts, data) = tuple;
                $name::new(ts, data)
            }
        }

        impl From<$name> for (FTs, u8) {
            fn from(ftsd: $name) -> Self {
                let $name(pack) = ftsd;
                let data = (pack as u8) & ((1 << $bits) - 1);
                (pack >> $bits, data)
            }
        }

        impl From<$name> for FTs {
            fn from(ftsd: $name) -> Self {
                let $name(pack) = ftsd;
                pack >> $bits
            }
        }

        impl $name {
            const DATA_MASK: FTs = (1 << $bits) - 1;
            #[inline]
            pub const fn new(fts: FTs, data: u8) -> Self {
                let pack = (fts << $bits) | (data as FTs & Self::DATA_MASK);
                Self(pack)
            }

            #[inline]
            pub fn into_data(self) -> u8 {
                (self.0 & Self::DATA_MASK) as u8
            }

            #[inline]
            pub fn data(&self) -> u8 {
                (self.0 & Self::DATA_MASK) as u8
            }

            #[inline]
            pub fn set_data(&mut self, data: u8) {
                self.0 = (self.0 & !Self::DATA_MASK) | (data as FTs & Self::DATA_MASK);
            }
        }
    }
}

fts_packed_data! { FTsData1, 1 }
fts_packed_data! { FTsData2, 2 }
fts_packed_data! { FTsData3, 3 }

impl VideoTs {
    #[inline]
    pub const fn new(vc: Ts, hc: Ts) -> Self {
        VideoTs{ vc, hc }
    }
}

impl<V, C> VFrameTsCounter<V, C>
    where V: VideoFrame,
          C: MemoryContention
{
    /// Constructs a new and normalized `VFrameTsCounter` from the given vertical and horizontal counter values.
    ///
    /// # Panics
    /// Panics when values given lead to an overflow of the capacity of [VideoTs].
    #[inline]
    pub fn new(vc: Ts, hc: Ts, contention: C) -> Self {
        let tsc = V::normalize_vts(VideoTs::new(vc, hc));
        VFrameTsCounter { tsc, contention, _video: PhantomData }
    }
    /// Builds a [VFrameTsCounter] from the given count of T-states.
    ///
    /// # Panics
    ///
    /// Panics when the given `ts` overflows the capacity of [VideoTs].
    #[inline]
    pub fn from_tstates(ts: FTs, contention: C) -> Self {
        let tsc = V::tstates_to_vts(ts);
        VFrameTsCounter { tsc, contention, _video: PhantomData }
    }

    #[inline]
    pub fn from_video_ts(vts: VideoTs, contention: C) -> Self {
        let tsc = V::normalize_vts(vts);
        VFrameTsCounter { tsc, contention, _video: PhantomData }
    }
    /// Returns the number of T-states measured from the start of the frame.
    ///
    /// A frame starts when the horizontal and vertical counter are both 0.
    ///
    /// The returned value can be negative as well as exceeding the [VideoFrame::FRAME_TSTATES_COUNT].
    ///
    /// This function should be used to ensure that a single frame events, timestamped with T-states,
    /// will be always in the ascending order.
    #[inline(always)]
    pub fn as_tstates(&self) -> FTs {
        V::vts_to_tstates(self.tsc)
    }
    /// Returns a tuple with an adjusted frame counter and with the frame-normalized timestamp as
    /// a number of T-states measured from the start of the frame.
    ///
    /// A frame starts when the horizontal and vertical counter are both 0.
    ///
    /// The returned timestamp value is in the range [0, [VideoFrame::FRAME_TSTATES_COUNT]).
    #[inline(always)]
    pub fn as_frame_tstates(&self, frames: u64) -> (u64, FTs) {
        V::vts_to_norm_tstates(frames, self.tsc)
    }
    /// Ensures the verical counter is in the range: `(-VSL_COUNT, V::VSL_COUNT)` by calculating
    /// a remainder of the division of the vertical counter by [VideoFrame::VSL_COUNT].
    #[inline(always)]
    pub fn wrap_frame(&mut self) {
        self.tsc.vc %= V::VSL_COUNT
    }
    /// Returns `true` if the counter value is past or near the end of a frame. Otherwise returns `false`.
    ///
    /// Specifically the condition is met if the vertical counter is equal or greater than [VideoFrame::VSL_COUNT].
    #[inline]
    pub fn is_eof(&self) -> bool {
        V::is_vts_eof(self.tsc)
    }
    /// Returns the largest timestamp value that can be represented by a normalized [VFrameTsCounter]
    /// as a number of T-states.
    pub fn max_tstates() -> FTs {
        V::vts_to_tstates(V::vts_max())
    }

    #[inline]
    pub fn is_contended_address(self, address: u16) -> bool {
        self.contention.is_contended_address(address)
    }

    #[inline(always)]
    fn set_hc_after_small_increment(&mut self, mut hc: Ts) {
        if hc >= V::HTS_RANGE.end {
            hc -= V::HTS_COUNT as Ts;
            self.tsc.vc += 1;
        }
        self.tsc.hc = hc;
    }
}

impl<V: VideoFrame, C> AddAssign<u32> for VFrameTsCounter<V, C> {
    fn add_assign(&mut self, delta: u32) {
        self.tsc = V::vts_add_ts(self.tsc, delta);
    }
}

/// A macro being used to implement an ULA I/O contention scheme, for [z80emu::Clock::add_io] method of
/// [VFrameTsCounter][clock::VFrameTsCounter].
/// It's being exported for the purpose of performing FUSE tests.
///
/// * $mc should be a type implementing [clock::MemoryContention] trait.
/// * $port is a port address.
/// * $hc is an identifier of a mutable variable containing the `hc` property of a `VideoTs` timestamp.
/// * $contention should be a path to the [video::VideoFrame::contention] function.
///
/// The macro returns a horizontal timestamp pointing after the whole I/O cycle is over.
/// The `hc` variable is modified to contain a horizontal timestamp indicating when the data R/W operation 
/// takes place.
#[macro_export]
macro_rules! ula_io_contention {
    ($mc:expr, $port:expr, $hc:ident, $contention:path) => {
        {
            use $crate::z80emu::host::cycles::*;
            if $mc.is_contended_address($port) {
                $hc = $contention($hc) + IO_IORQ_LOW_TS as Ts;
                if $port & 1 == 0 { // C:1, C:3
                    $contention($hc) + (IO_CYCLE_TS - IO_IORQ_LOW_TS) as Ts
                }
                else { // C:1, C:1, C:1, C:1
                    let mut hc1 = $hc;
                    for _ in 0..(IO_CYCLE_TS - IO_IORQ_LOW_TS) {
                        hc1 = $contention(hc1) + 1;
                    }
                    hc1
                }
            }
            else {
                $hc += IO_IORQ_LOW_TS as Ts;
                if $port & 1 == 0 { // N:1 C:3
                    $contention($hc) + (IO_CYCLE_TS - IO_IORQ_LOW_TS) as Ts
                }
                else { // N:4
                    $hc + (IO_CYCLE_TS - IO_IORQ_LOW_TS) as Ts
                }
            }
        }
    };
}

impl<V: VideoFrame, C: MemoryContention> Clock for VFrameTsCounter<V, C> {
    type Limit = Ts;
    type Timestamp = VideoTs;
    fn is_past_limit(&self, limit: Self::Limit) -> bool {
        self.tsc.vc >= limit
    }

    fn add_irq(&mut self, _pc: u16) -> Self::Timestamp {
        self.set_hc_after_small_increment(self.tsc.hc + IRQ_ACK_CYCLE_TS as Ts);
        self.tsc
    }

    fn add_no_mreq(&mut self, address: u16, add_ts: NonZeroU8) {
        let mut hc = self.tsc.hc;
        if self.contention.is_contended_address(address) && V::is_contended_line_no_mreq(self.tsc.vc) {
            for _ in 0..add_ts.get() {
                hc = V::contention(hc) + 1;
            }
        }
        else {
            hc += add_ts.get() as Ts;
        }
        self.set_hc_after_small_increment(hc);
    }

    fn add_m1(&mut self, address: u16) -> Self::Timestamp {
        // match address {
        //     // 0x8043 => println!("0x{:04x}: {} {:?}", address, self.as_tstates(), self.tsc),
        //     0x806F..=0x8078 => println!("0x{:04x}: {} {:?}", address, self.as_tstates(), self.tsc),
        //     // 0xC008..=0xC011 => println!("0x{:04x}: {} {:?}", address, self.as_tstates(), self.tsc),
        //     _ => {}
        // }
        let hc = if self.contention.is_contended_address(address) && V::is_contended_line_mreq(self.tsc.vc) {
            V::contention(self.tsc.hc)
        }
        else {
            self.tsc.hc
        };
        self.set_hc_after_small_increment(hc + M1_CYCLE_TS as Ts);
        self.tsc
    }

    fn add_mreq(&mut self, address: u16) -> Self::Timestamp {
        let hc = if self.contention.is_contended_address(address) && V::is_contended_line_mreq(self.tsc.vc) {
            V::contention(self.tsc.hc)
        }
        else {
            self.tsc.hc
        };
        self.set_hc_after_small_increment(hc + MEMRW_CYCLE_TS as Ts);
        self.tsc
    }

    // fn add_io(&mut self, port: u16) -> Self::Timestamp {
    //     let VideoTs{ vc, hc } = self.tsc;
    //     let hc = if V::is_contended_line_no_mreq(vc) {
    //         if self.contention.is_contended_address(port) {
    //             let hc = V::contention(hc) + 1;
    //             if port & 1 == 0 { // C:1, C:3
    //                 V::contention(hc) + (IO_CYCLE_TS - 1) as Ts
    //             }
    //             else { // C:1, C:1, C:1, C:1
    //                 let mut hc1 = hc;
    //                 for _ in 1..IO_CYCLE_TS {
    //                     hc1 = V::contention(hc1) + 1;
    //                 }
    //                 hc1
    //             }
    //         }
    //         else {
    //             if port & 1 == 0 { // N:1 C:3
    //                 V::contention(hc + 1) + (IO_CYCLE_TS - 1) as Ts
    //             }
    //             else { // N:4
    //                 hc + IO_CYCLE_TS as Ts
    //             }
    //         }
    //     }
    //     else { // N:4
    //         hc + IO_CYCLE_TS as Ts
    //     };
    //     self.set_hc_after_small_increment(hc);
    //     Self::new(vc, hc - 1).as_timestamp() // data read at last cycle
    // }

    fn add_io(&mut self, port: u16) -> Self::Timestamp {
        let VideoTs{ vc, mut hc } = self.tsc;
        // if port == 0x7ffd {
        //     println!("0x{:04x}: {} {:?}", port, self.as_tstates(), self.tsc);
        // }
        let hc1 = if V::is_contended_line_no_mreq(vc) {
            ula_io_contention!(self.contention, port, hc, V::contention)
            // if is_contended_address(self.contention_mask, port) {
            //     hc = V::contention(hc) + IO_IORQ_LOW_TS as Ts;
            //     if port & 1 == 0 { // C:1, C:3
            //         V::contention(hc) + (IO_CYCLE_TS - IO_IORQ_LOW_TS) as Ts
            //     }
            //     else { // C:1, C:1, C:1, C:1
            //         let mut hc1 = hc;
            //         for _ in 0..(IO_CYCLE_TS - IO_IORQ_LOW_TS) {
            //             hc1 = V::contention(hc1) + 1;
            //         }
            //         hc1
            //     }
            // }
            // else {
            //     hc += IO_IORQ_LOW_TS as Ts;
            //     if port & 1 == 0 { // N:1 C:3
            //         V::contention(hc) + (IO_CYCLE_TS - IO_IORQ_LOW_TS) as Ts
            //     }
            //     else { // N:4
            //         hc + (IO_CYCLE_TS - IO_IORQ_LOW_TS) as Ts
            //     }
            // }
        }
        else {
            hc += IO_IORQ_LOW_TS as Ts;
            hc + (IO_CYCLE_TS - IO_IORQ_LOW_TS) as Ts
        };
        let mut vtsc = *self;
        vtsc.set_hc_after_small_increment(hc);
        self.set_hc_after_small_increment(hc1);
        vtsc.tsc
    }

    fn add_wait_states(&mut self, _bus: u16, wait_states: NonZeroU16) {
        let ws = wait_states.get();
        if ws > WAIT_STATES_THRESHOLD {
            // emulate hanging the Spectrum
            self.tsc.vc += HALT_VC_THRESHOLD;
        }
        else if ws < V::HTS_COUNT as u16 {
            self.set_hc_after_small_increment(self.tsc.hc + ws as i16);
        }
        else {
            *self += ws as u32;
        }
    }

    #[inline(always)]
    fn as_timestamp(&self) -> Self::Timestamp {
        self.tsc
    }
}


impl<V, C> From<VFrameTsCounter<V, C>> for VideoTs {
    #[inline(always)]
    fn from(vftsc: VFrameTsCounter<V, C>) -> VideoTs {
        vftsc.tsc
    }
}

impl<V, C> Deref for VFrameTsCounter<V, C> {
    type Target = VideoTs;

    fn deref(&self) -> &Self::Target {
        &self.tsc
    }
}

impl<V, C> DerefMut for VFrameTsCounter<V, C> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.tsc
    }
}
