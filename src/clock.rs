use core::marker::PhantomData;
use core::num::{NonZeroU8, NonZeroU16};
use core::ops::{AddAssign, Deref, DerefMut};
use core::convert::TryInto;
use z80emu::{Clock, host::cycles::*};
use crate::video::VideoFrame;

// Frame timestamp measured in T-states.
pub type FTs = i32;
// T-state/Video scanline counter types.
pub type Ts = i16;

/// V/H T-states timestamp.
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct VideoTs {
    // A video scanline counter.
    pub vc: Ts,
    // A horizontal T-state counter.
    pub hc: Ts,
}

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

            #[inline(always)]
            pub fn wrap_vc(&mut self, vc_frame_count: Ts) {
                self.vc %= vc_frame_count;
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

/// V/H T-states counter for the specific VideoFrame.
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct VFrameTsCounter<V: VideoFrame>  {
    pub tsc: VideoTs,
    video: PhantomData<V>
}

impl VideoTs {
    #[inline]
    pub const fn new(vc: Ts, hc: Ts) -> Self {
        VideoTs{ vc, hc }
    }

    #[inline(always)]
    pub fn wrap_vc(&mut self, vc_frame_count: Ts) {
        self.vc %= vc_frame_count;
    }
}

impl<V: VideoFrame> VFrameTsCounter<V> {
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
        let hts_count: FTs = V::HTS_COUNT as FTs;
        let vc = ts / hts_count;
        let hc = ts % hts_count;
        VFrameTsCounter::new(vc.try_into().unwrap(), hc.try_into().unwrap())
    }
    /// Convert video scan line and horizontal T-state counters to the frame T-state counter without wrapping.
    #[inline(always)]
    pub fn vc_hc_to_tstates(vc: Ts, hc: Ts) -> FTs {
        vc as FTs * V::HTS_COUNT as FTs + hc as FTs
    }
    /// Convert video timestamp to the frame T-state timestamp without wrapping.
    #[inline(always)]
    pub fn vts_to_tstates(vts: VideoTs) -> FTs {
        Self::vc_hc_to_tstates(vts.vc, vts.hc)
    }
    /// Returns the number of T-states from the start of the frame.
    /// The frame starts when the horizontal and vertical counter are both 0.
    /// The returned value can be negative as well as exceeding the FRAME_TSTATES_COUNT.
    /// This function should be used to ensure that a single frame events timestamped with T-states
    /// will be always in the ascending order.
    #[inline(always)]
    pub fn as_tstates(&self) -> FTs {
        Self::vc_hc_to_tstates(self.tsc.vc, self.tsc.hc)
    }
    /// Returns the number of T-states in a single frame window.
    /// The frame starts when the horizontal and vertical counter are both 0.
    /// The value returned is 0 <= T-states < FRAME_TSTATES_COUNT
    #[inline(always)]
    pub fn as_frame_tstates(&self) -> FTs {
        Self::vc_hc_to_tstates(self.tsc.vc, self.tsc.hc).rem_euclid(
            V::HTS_COUNT as FTs * V::VSL_COUNT as FTs)
    }

    #[inline(always)]
    pub fn wrap_frame(&mut self) {
        self.tsc.wrap_vc(V::VSL_COUNT);
    }

    /// Is counter past or near the end of frame
    #[inline]
    pub fn is_eof(&self) -> bool {
        self.vc >= V::VSL_COUNT
    }

    pub fn max_tstates() -> FTs {
        Self::max_value().as_tstates()
    }

    pub fn max_value() -> Self {
        VFrameTsCounter { tsc: VideoTs { vc: Ts::max_value(), hc: V::HTS_COUNT - 1 }, video: PhantomData }
    }

    pub fn saturating_wrap_with_normalized(ts: VideoTs, end_ts: VideoTs) -> VideoTs {
        let mut vc = ts.vc.saturating_sub(end_ts.vc);
        let mut hc = ts.hc - end_ts.hc;
        while hc >= V::HTS_RANGE.end {
            hc -= V::HTS_COUNT as Ts;
            vc += 1;
        }
        while hc < V::HTS_RANGE.start {
            hc += V::HTS_COUNT as Ts;
            vc -= 1;
        }
        VideoTs::new(vc, hc)
    }

    #[inline(always)]
    fn set_hc(&mut self, mut hc: Ts) {
        while hc >= V::HTS_RANGE.end {
            hc -= V::HTS_COUNT as Ts;
            self.tsc.vc += 1;
        }
        self.tsc.hc = hc;
    }
}

impl<V: VideoFrame> AddAssign<u32> for VFrameTsCounter<V> {
    #[allow(clippy::suspicious_op_assign_impl)]
    fn add_assign(&mut self, delta: u32) {
        let dvc = (delta / V::HTS_COUNT as u32).try_into().expect("delta too large");
        let dhc = (delta % V::HTS_COUNT as u32) as Ts;
        self.tsc.vc = self.tsc.vc.checked_add(dvc).expect("delta too large");
        self.set_hc(self.tsc.hc + dhc);
    }
}

impl<V: VideoFrame> Clock for VFrameTsCounter<V> {
    type Limit = Ts;
    type Timestamp = VideoTs;
    fn is_past_limit(&self, limit: Self::Limit) -> bool {
        self.tsc.vc >= limit
    }

    fn add_irq(&mut self, _pc: u16) -> Self::Timestamp {
        self.set_hc(self.tsc.hc + IRQ_ACK_CYCLE_TS as Ts);
        self.tsc
    }

    fn add_no_mreq(&mut self, address: u16, add_ts: NonZeroU8) {
        let mut hc = self.tsc.hc;
        if V::is_contended_address(address) && V::is_contended_line_no_mreq(self.tsc.vc) {
            for _ in 0..add_ts.get() {
                hc = V::contention(hc) + 1;
            }
        }
        else {
            hc += add_ts.get() as Ts;
        }
        self.set_hc(hc);
    }

    fn add_m1(&mut self, address: u16) -> Self::Timestamp {
        // match address {
        //     0x80BB => println!("0x{:04x}: {} {:?}", address, self.as_tstates(), self.tsc),
        //     // 0x80F3..=0x80F9 => println!("0x{:04x}: {} {:?}", address, self.as_tstates(), self.tsc),
        //     // 0xC008..=0xC011 => println!("0x{:04x}: {} {:?}", address, self.as_tstates(), self.tsc),
        //     _ => {}
        // }
        let hc = if V::is_contended_address(address) && V::is_contended_line_mreq(self.tsc.vc) {
            V::contention(self.tsc.hc)
        }
        else {
            self.tsc.hc
        };
        self.set_hc(hc + M1_CYCLE_TS as Ts);
        self.tsc
    }

    fn add_mreq(&mut self, address: u16) -> Self::Timestamp {
        let hc = if V::is_contended_address(address) && V::is_contended_line_mreq(self.tsc.vc) {
            V::contention(self.tsc.hc)
        }
        else {
            self.tsc.hc
        };
        self.set_hc(hc + MEMRW_CYCLE_TS as Ts);
        self.tsc
    }

    // fn add_io(&mut self, port: u16) -> Self::Timestamp {
    //     let VideoTs{ vc, hc } = self.tsc;
    //     let hc = if V::is_contended_line_no_mreq(vc) {
    //         if V::is_contended_address(port) {
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
    //     self.set_hc(hc);
    //     Self::new(vc, hc - 1).as_timestamp() // data read at last cycle
    // }

    fn add_io(&mut self, port: u16) -> Self::Timestamp {
        let VideoTs{ vc, mut hc } = self.tsc;
        let hc1 = if V::is_contended_line_no_mreq(vc) {
            if V::is_contended_address(port) {
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
        self.set_hc(hc1);
        Self::new(vc, hc).as_timestamp()
    }

    fn add_wait_states(&mut self, _bus: u16, _wait_states: NonZeroU16) {
        unreachable!()
    }

    #[inline(always)]
    fn as_timestamp(&self) -> Self::Timestamp {
        self.tsc
    }
}


impl<V: VideoFrame> From<VFrameTsCounter<V>> for VideoTs {
    #[inline(always)]
    fn from(vftsc: VFrameTsCounter<V>) -> VideoTs {
        vftsc.tsc
    }
}

impl<V: VideoFrame> From<VideoTs> for VFrameTsCounter<V> {
    #[inline(always)]
    fn from(tsc: VideoTs) -> VFrameTsCounter<V> {
        VFrameTsCounter { tsc, video: PhantomData }
    }
}

impl<V: VideoFrame> Deref for VFrameTsCounter<V> {
    type Target = VideoTs;

    fn deref(&self) -> &Self::Target {
        &self.tsc
    }
}

impl<V: VideoFrame> DerefMut for VFrameTsCounter<V> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.tsc
    }
}
