use core::fmt::Debug;
use core::marker::PhantomData;
use core::num::{NonZeroU8, NonZeroU16};
use core::ops::{AddAssign, Deref, DerefMut};

use z80emu::{Clock, host::cycles::*};
#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};

use crate::video::VideoFrame;

// Frame timestamp measured in T-states.
pub type FTs = i32;
// T-state/Video scanline counter types.
pub type Ts = i16;

/// A video timestamp that consist of two counters: vertical and horizontal.
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
pub struct VideoTs {
    // A vertical position and a video scan-line index.
    pub vc: Ts,
    // A horizontal position measured in T states.
    pub hc: Ts,
}

/// A video T states counter clock for the specific [VideoFrame] and [MemoryContention].
///
/// Used as a [Clock] by `Ula` to execute [z80emu::Cpu] code.
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct VFrameTsCounter<V, C>  {
    pub tsc: VideoTs,
    _video: PhantomData<V>,
    _contention: PhantomData<C>
}

pub trait MemoryContention: Copy + Debug {
    #[inline]
    fn is_contended_address(addr: u16) -> bool {
        addr & 0xC000 == 0x4000
    }
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

impl<V: VideoFrame, C> VFrameTsCounter<V, C> {
    #[inline]
    pub fn new(mut vc: Ts, mut hc: Ts) -> Self {
        while hc >= V::HTS_RANGE.end {
            vc += 1;
            hc -= V::HTS_COUNT as Ts;
        }
        while hc < V::HTS_RANGE.start {
            vc -= 1;
            hc += V::HTS_COUNT as Ts;
        }
        VFrameTsCounter::from(VideoTs::new(vc, hc))
    }
    /// Build VFrameTsCounter from the given number of frame tstates.
    ///
    /// # Panics
    ///
    /// When given ts is too large.
    #[inline]
    pub fn from_tstates(ts: FTs) -> Self {
        VFrameTsCounter::from(V::tstates_to_vts(ts))
    }
    /// Returns the number of T-states from the start of the frame.
    /// The frame starts when the horizontal and vertical counter are both 0.
    /// The returned value can be negative as well as exceeding the FRAME_TSTATES_COUNT.
    /// This function should be used to ensure that a single frame events timestamped with T-states
    /// will be always in the ascending order.
    #[inline(always)]
    pub fn as_tstates(&self) -> FTs {
        V::vts_to_tstates(self.tsc)
    }
    /// Returns a normalized number of T-states in a single frame window with a frame counter difference.
    /// The frame starts when the horizontal and vertical counter are both 0.
    /// The value returned is 0 <= T-states < FRAME_TSTATES_COUNT.
    #[inline(always)]
    pub fn as_frame_tstates(&self, frames: u64) -> (u64, FTs) {
        V::vts_to_norm_tstates(self.tsc, frames)
    }

    #[inline(always)]
    pub fn wrap_frame(&mut self) {
        self.tsc.vc %= V::VSL_COUNT
    }

    /// Is counter past or near the end of frame
    #[inline]
    pub fn is_eof(&self) -> bool {
        V::is_vts_eof(self.tsc)
    }

    pub fn max_tstates() -> FTs {
        V::vts_to_tstates(V::vts_max())
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
        if C::is_contended_address(address) && V::is_contended_line_no_mreq(self.tsc.vc) {
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
        let hc = if C::is_contended_address(address) && V::is_contended_line_mreq(self.tsc.vc) {
            V::contention(self.tsc.hc)
        }
        else {
            self.tsc.hc
        };
        self.set_hc_after_small_increment(hc + M1_CYCLE_TS as Ts);
        self.tsc
    }

    fn add_mreq(&mut self, address: u16) -> Self::Timestamp {
        let hc = if C::is_contended_address(address) && V::is_contended_line_mreq(self.tsc.vc) {
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
    //         if C::is_contended_address(port) {
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
            if C::is_contended_address(port) {
                hc = V::contention(hc) + IO_IORQ_LOW_TS as Ts;
                if port & 1 == 0 { // C:1, C:3
                    V::contention(hc) + (IO_CYCLE_TS - IO_IORQ_LOW_TS) as Ts
                }
                else { // C:1, C:1, C:1, C:1
                    let mut hc1 = hc;
                    for _ in 0..(IO_CYCLE_TS - IO_IORQ_LOW_TS) {
                        hc1 = V::contention(hc1) + 1;
                    }
                    hc1
                }
            }
            else {
                hc += IO_IORQ_LOW_TS as Ts;
                if port & 1 == 0 { // N:1 C:3
                    V::contention(hc) + (IO_CYCLE_TS - IO_IORQ_LOW_TS) as Ts
                }
                else { // N:4
                    hc + (IO_CYCLE_TS - IO_IORQ_LOW_TS) as Ts
                }
            }
        }
        else {
            hc += IO_IORQ_LOW_TS as Ts;
            hc + (IO_CYCLE_TS - IO_IORQ_LOW_TS) as Ts
        };
        self.set_hc_after_small_increment(hc1);
        Self::new(vc, hc).as_timestamp()
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

impl<V, C> From<VideoTs> for VFrameTsCounter<V, C> {
    #[inline(always)]
    fn from(tsc: VideoTs) -> VFrameTsCounter<V, C> {
        VFrameTsCounter { tsc, _video: PhantomData, _contention: PhantomData }
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
