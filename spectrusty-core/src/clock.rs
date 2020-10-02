/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
//! T-state timestamp types and counters.
use core::cmp::{Ordering, Ord, PartialEq, PartialOrd};
use core::convert::TryInto;
use core::fmt::Debug;
use core::hash::{Hash, Hasher};
use core::marker::PhantomData;
use core::num::{NonZeroU8, NonZeroU16};
use core::ops::{Deref, DerefMut};

use z80emu::{Clock, host::cycles::*};
#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};

use crate::video::VideoFrame;

mod packed;
mod ops;
pub use packed::*;
pub use ops::*;

/// A linear T-state timestamp type.
pub type FTs = i32;
/// A type used for a horizontal T-state timestamp or a video scanline index for [VideoTs].
pub type Ts = i16;

/// A timestamp type that consists of two video counters: vertical and horizontal.
///
/// `VideoTs { vc: 0, hc: 0 }` marks the start of the video frame.
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct VideoTs {
    /// A vertical counter - a video scan-line index.
    pub vc: Ts,
    /// A horizontal counter - measured in T-states.
    pub hc: Ts,
}

/// A [VideoTs] timestamp wrapper with a constraint to the `V:` [VideoFrame],
/// implementing methods and traits for timestamp calculations.
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "snapshot", serde(from="VideoTs", into="VideoTs"))]
#[derive(Copy, Debug)]
pub struct VFrameTs<V> {
    /// The current value of the timestamp.
    pub ts: VideoTs,
    _vframe: PhantomData<V>,
}

/// A trait used by [VFrameTsCounter] for checking if an `address` is a contended one.
pub trait MemoryContention: Copy + Debug {
    fn is_contended_address(self, address: u16) -> bool;
}

/// A generic [`VFrameTs<V>`][VFrameTs] based T-states counter.
///
/// Implements [Clock] for counting cycles when code is being executed by [z80emu::Cpu].
///
/// Inserts additional T-states according to the contention model specified by generic
/// parameters: `V:` [VideoFrame] and `C:` [MemoryContention].
///
/// [Clock]: /z80emu/%2A/z80emu/host/trait.Clock.html
/// [z80emu::Cpu]: /z80emu/%2A/z80emu/trait.Cpu.html
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct VFrameTsCounter<V, C>  {
    /// The current value of the counter.
    pub vts: VFrameTs<V>,
    /// An instance implementing a [MemoryContention] trait.
    pub contention: C,
}

/// If a vertical counter of [VideoTs] exceeds this value, it signals the control unit
/// to emulate hanging CPU indefinitely.
pub const HALT_VC_THRESHOLD: i16 = i16::max_value() >> 1;

const WAIT_STATES_THRESHOLD: u16 = i16::max_value() as u16 - 256;

impl VideoTs {
    #[inline]
    pub const fn new(vc: Ts, hc: Ts) -> Self {
        VideoTs { vc, hc }
    }
}

impl <V: VideoFrame> VFrameTs<V> {
    /// The end-of-frame timestamp, equal to the total number of T-states per frame.
    pub const EOF: VFrameTs<V> = VFrameTs { ts: VideoTs {
                                                vc: V::VSL_COUNT,
                                                hc: 0
                                            },
                                             _vframe: PhantomData };
    /// Constructs a new `VFrameTs` from the given vertical and horizontal counter values.
    ///
    /// __Note__: The returned `VFrameTs` is not normalized.
    #[inline]
    pub fn new(vc: Ts, hc: Ts) -> Self {
        VFrameTs { ts: VideoTs::new(vc, hc), _vframe: PhantomData }
    }
    /// Returns `true` if a video timestamp is normalized. Otherwise returns `false`.
    #[inline]
    pub fn is_normalized(self) -> bool {
        V::HTS_RANGE.contains(&self.ts.hc)
    }
    /// Normalizes self with a horizontal counter within the allowed range and a scan line
    /// counter adjusted accordingly.
    ///
    /// # Panics
    /// Panics when an attempt to normalize leads to an overflow of the capacity of [VideoTs].
    #[inline]
    pub fn normalized(self) -> Self {
        let VideoTs { mut vc, mut hc } = self.ts;
        if hc < V::HTS_RANGE.start || hc >= V::HTS_RANGE.end {
            let fhc: FTs = hc as FTs - if hc < 0 {
                V::HTS_RANGE.end
            }
            else {
                V::HTS_RANGE.start
            } as FTs;
            vc = vc.checked_add((fhc / V::HTS_COUNT as FTs) as Ts).expect("video ts overflow");
            hc = fhc.rem_euclid(V::HTS_COUNT as FTs) as Ts + V::HTS_RANGE.start;
        }
        VFrameTs::new(vc, hc)
    }
    /// Returns a video timestamp with a horizontal counter within the allowed range and a scan line
    /// counter adjusted accordingly. Saturates at [VFrameTs::min_value] or [VFrameTs::max_value].
    #[inline]
    pub fn saturating_normalized(self) -> Self {
        let VideoTs { mut vc, mut hc } = self.ts;
        if hc < V::HTS_RANGE.start || hc >= V::HTS_RANGE.end {
            let fhc: FTs = hc as FTs - if hc < 0 {
                V::HTS_RANGE.end
            }
            else {
                V::HTS_RANGE.start
            } as FTs;
            let dvc = (fhc / V::HTS_COUNT as FTs) as Ts;
            if let Some(vc1) = vc.checked_add(dvc) {
                vc = vc1;
                hc = fhc.rem_euclid(V::HTS_COUNT as FTs) as Ts + V::HTS_RANGE.start;
            }
            else {
                return if dvc < 0 { Self::min_value() } else { Self::max_value() };
            }
        }
        VFrameTs::new(vc, hc)
    }
    /// Returns the largest value that can be represented by a normalized timestamp.
    #[inline(always)]
    pub fn max_value() -> Self {
        VFrameTs { ts: VideoTs { vc: Ts::max_value(), hc: V::HTS_RANGE.end - 1 },
                   _vframe: PhantomData }
    }
    /// Returns the smallest value that can be represented by a normalized timestamp.
    #[inline(always)]
    pub fn min_value() -> Self {
        VFrameTs { ts: VideoTs { vc: Ts::min_value(), hc: V::HTS_RANGE.start },
                   _vframe: PhantomData }
    }
    /// Returns `true` if the counter value is past or near the end of a frame. Otherwise returns `false`.
    ///
    /// Specifically, the condition is met if the vertical counter is equal to or greater than [VideoFrame::VSL_COUNT].
    #[inline(always)]
    pub fn is_eof(self) -> bool {
        self.vc >= V::VSL_COUNT
    }
    /// Ensures the vertical counter is in the range: `(-VSL_COUNT, VSL_COUNT)` by calculating
    /// a remainder of the division of the vertical counter by [VideoFrame::VSL_COUNT].
    #[inline(always)]
    pub fn wrap_frame(&mut self) {
        self.ts.vc %= V::VSL_COUNT
    }
    /// Returns a video timestamp after subtracting the total number of frame video scanlines
    /// from the scan line counter.
    #[inline]
    pub fn saturating_sub_frame(self) -> Self {
        let VideoTs { vc, hc } = self.ts;
        let vc = vc.saturating_sub(V::VSL_COUNT);
        VFrameTs::new(vc, hc)
    }
    /// Returns a normalized timestamp from the given number of T-states.
    ///
    /// # Panics
    /// Panics when the given `ts` overflows the capacity of the timestamp.
    #[inline]
    pub fn from_tstates(ts: FTs) -> Self {
        let mut vc = (ts / V::HTS_COUNT as FTs).try_into().expect("video ts overflow");
        let mut hc: Ts = (ts % V::HTS_COUNT as FTs).try_into().unwrap();
        if hc >= V::HTS_RANGE.end {
            hc -= V::HTS_COUNT;
            vc += 1;
        }
        else if hc < V::HTS_RANGE.start {
            hc += V::HTS_COUNT;
            vc -= 1;
        }
        VFrameTs::new(vc, hc)
    }
    /// Converts the timestamp to FTs.
    ///
    /// # Panics
    /// Panics when `self` overflows the capacity of the result type.
    #[inline]
    pub fn into_tstates(self) -> FTs {
        let VideoTs { vc, hc } = self.ts;
        V::vc_hc_to_tstates(vc, hc)
    }
    /// Returns a tuple with an adjusted frame counter and with the frame-normalized timestamp as
    /// the number of T-states measured from the start of the frame.
    ///
    /// The frame starts when the horizontal and vertical counter are both 0.
    ///
    /// The returned timestamp value is in the range [0, [VideoFrame::FRAME_TSTATES_COUNT]).
    #[inline]
    pub fn into_frame_tstates(self, frames: u64) -> (u64, FTs) {
        let ts = TimestampOps::into_tstates(self);
        let frmdlt = ts / V::FRAME_TSTATES_COUNT;
        let ufrmdlt = if ts < 0 { frmdlt - 1 } else { frmdlt } as u64;
        let frames = frames.wrapping_add(ufrmdlt);
        let ts = ts.rem_euclid(V::FRAME_TSTATES_COUNT);
        (frames, ts)
    }

    #[inline]
    fn set_hc_after_small_increment(&mut self, mut hc: Ts) {
        if hc >= V::HTS_RANGE.end {
            hc -= V::HTS_COUNT as Ts;
            self.ts.vc += 1;
        }
        self.ts.hc = hc;
    }
}

impl<V, C> VFrameTsCounter<V, C>
    where V: VideoFrame,
          C: MemoryContention
{
    /// Constructs a new and normalized `VFrameTsCounter` from the given vertical and horizontal counter values.
    ///
    /// # Panics
    /// Panics when the given values lead to an overflow of the capacity of [VideoTs].
    #[inline]
    pub fn new(vc: Ts, hc: Ts, contention: C) -> Self {
        let vts = VFrameTs::new(vc, hc).normalized();
        VFrameTsCounter { vts, contention }
    }
    /// Builds a normalized [VFrameTsCounter] from the given count of T-states.
    ///
    /// # Panics
    ///
    /// Panics when the given `ts` overflows the capacity of [VideoTs].
    #[inline]
    pub fn from_tstates(ts: FTs, contention: C) -> Self {
        let vts = TimestampOps::from_tstates(ts);
        VFrameTsCounter { vts, contention }
    }
    /// Builds a normalized [VFrameTsCounter] from the given count of T-states.
    ///
    /// # Panics
    ///
    /// Panics when the given `ts` overflows the capacity of [VideoTs].
    #[inline]
    pub fn from_video_ts(vts: VideoTs, contention: C) -> Self {
        let vts = VFrameTs::from(vts).normalized();
        VFrameTsCounter { vts, contention }
    }
    /// Builds a normalized [VFrameTsCounter] from the given count of T-states.
    ///
    /// # Panics
    ///
    /// Panics when the given `ts` overflows the capacity of [VideoTs].
    #[inline]
    pub fn from_vframe_ts(vfts: VFrameTs<V>, contention: C) -> Self {
        let vts = vfts.normalized();
        VFrameTsCounter { vts, contention }
    }

    #[inline]
    pub fn is_contended_address(self, address: u16) -> bool {
        self.contention.is_contended_address(address)
    }
}

/// This macro is used to implement the ULA I/O contention scheme, for [z80emu::Clock::add_io] method of
/// [VFrameTsCounter].
/// It's being exported for the purpose of performing FUSE tests.
///
/// * $mc should be a type implementing [MemoryContention] trait.
/// * $port is a port address.
/// * $hc is an identifier of a mutable variable containing the `hc` property of a `VideoTs` timestamp.
/// * $contention should be a path to the [VideoFrame::contention] function.
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
/*
impl<V: VideoFrame> Clock for VFrameTs<V> {
    type Limit = Ts;
    type Timestamp = VideoTs;

    #[inline(always)]
    fn is_past_limit(&self, limit: Self::Limit) -> bool {
        self.vc >= limit
    }

    fn add_irq(&mut self, _pc: u16) -> Self::Timestamp {
        self.set_hc_after_small_increment(self.hc + IRQ_ACK_CYCLE_TS as Ts);
        self.as_timestamp()
    }

    fn add_no_mreq(&mut self, _address: u16, add_ts: NonZeroU8) {
        let hc = self.hc + add_ts.get() as Ts;
        self.set_hc_after_small_increment(hc);
    }

    fn add_m1(&mut self, _address: u16) -> Self::Timestamp {
        self.set_hc_after_small_increment(self.hc + M1_CYCLE_TS as Ts);
        self.as_timestamp()
    }

    fn add_mreq(&mut self, _address: u16) -> Self::Timestamp {
        self.set_hc_after_small_increment(self.hc + MEMRW_CYCLE_TS as Ts);
        self.as_timestamp()
    }

    fn add_io(&mut self, _port: u16) -> Self::Timestamp {
        let hc = self.hc + IO_IORQ_LOW_TS as Ts;
        let hc1 = hc + (IO_CYCLE_TS - IO_IORQ_LOW_TS) as Ts;

        let mut tsc = *self;
        tsc.set_hc_after_small_increment(hc);
        self.set_hc_after_small_increment(hc1);
        tsc.as_timestamp()
    }

    fn add_wait_states(&mut self, _bus: u16, wait_states: NonZeroU16) {
        let ws = wait_states.get();
        if ws > WAIT_STATES_THRESHOLD {
            // emulate hanging the Spectrum
            self.vc += HALT_VC_THRESHOLD;
        }
        else if ws < V::HTS_COUNT as u16 {
            self.set_hc_after_small_increment(self.hc + ws as i16);
        }
        else {
            *self += ws as u32;
        }
    }

    #[inline(always)]
    fn as_timestamp(&self) -> Self::Timestamp {
        self.ts
    }
}
*/
impl<V: VideoFrame, C: MemoryContention> Clock for VFrameTsCounter<V, C> {
    type Limit = Ts;
    type Timestamp = VideoTs;

    #[inline(always)]
    fn is_past_limit(&self, limit: Self::Limit) -> bool {
        self.vc >= limit
    }

    fn add_irq(&mut self, _pc: u16) -> Self::Timestamp {
        self.vts.set_hc_after_small_increment(self.hc + IRQ_ACK_CYCLE_TS as Ts);
        self.as_timestamp()
    }

    fn add_no_mreq(&mut self, address: u16, add_ts: NonZeroU8) {
        let mut hc = self.hc;
        if V::is_contended_line_no_mreq(self.vc) && self.contention.is_contended_address(address) {
            for _ in 0..add_ts.get() {
                hc = V::contention(hc) + 1;
            }
        }
        else {
            hc += add_ts.get() as Ts;
        }
        self.vts.set_hc_after_small_increment(hc);
    }

    fn add_m1(&mut self, address: u16) -> Self::Timestamp {
        // match address {
        //     // 0x8043 => println!("0x{:04x}: {} {:?}", address, self.as_tstates(), self.tsc),
        //     0x806F..=0x8078 => println!("0x{:04x}: {} {:?}", address, self.as_tstates(), self.tsc),
        //     // 0xC008..=0xC011 => println!("0x{:04x}: {} {:?}", address, self.as_tstates(), self.tsc),
        //     _ => {}
        // }
        let hc = if V::is_contended_line_mreq(self.vc) && self.contention.is_contended_address(address) {
            V::contention(self.hc)
        }
        else {
            self.hc
        };
        self.vts.set_hc_after_small_increment(hc + M1_CYCLE_TS as Ts);
        self.as_timestamp()
    }

    fn add_mreq(&mut self, address: u16) -> Self::Timestamp {
        let hc = if V::is_contended_line_mreq(self.vc) && self.contention.is_contended_address(address) {
            V::contention(self.hc)
        }
        else {
            self.hc
        };
        self.vts.set_hc_after_small_increment(hc + MEMRW_CYCLE_TS as Ts);
        self.as_timestamp()
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
    //     self.vts.set_hc_after_small_increment(hc);
    //     Self::new(vc, hc - 1).as_timestamp() // data read at last cycle
    // }

    fn add_io(&mut self, port: u16) -> Self::Timestamp {
        let VideoTs{ vc, mut hc } = self.as_timestamp();
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
        vtsc.vts.set_hc_after_small_increment(hc);
        self.vts.set_hc_after_small_increment(hc1);
        vtsc.as_timestamp()
    }

    fn add_wait_states(&mut self, _bus: u16, wait_states: NonZeroU16) {
        let ws = wait_states.get();
        if ws > WAIT_STATES_THRESHOLD {
            // emulate hanging the Spectrum
            self.vc += HALT_VC_THRESHOLD;
        }
        else if ws < V::HTS_COUNT as u16 {
            self.vts.set_hc_after_small_increment(self.hc + ws as i16);
        }
        else {
            *self += ws as u32;
        }
    }

    #[inline(always)]
    fn as_timestamp(&self) -> Self::Timestamp {
        ***self
    }
}

impl<V> Default for VFrameTs<V> {
    fn default() -> Self {
        VFrameTs::from(VideoTs::default())
    }
}

impl<V> Clone for VFrameTs<V> {
    fn clone(&self) -> Self {
        VFrameTs::from(self.ts)
    }
}

impl<V> Hash for VFrameTs<V> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.ts.hash(state);
    }
}

impl<V> Eq for VFrameTs<V> {}

impl<V> PartialEq for VFrameTs<V> {
    #[inline(always)]
    fn eq(&self, other: &Self) -> bool {
        self.ts == other.ts
    }
}

impl<V> Ord for VFrameTs<V> {
    #[inline(always)]
    fn cmp(&self, other: &Self) -> Ordering {
        self.ts.cmp(other)
    }
}

impl<V> PartialOrd for VFrameTs<V> {
    #[inline(always)]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<V: VideoFrame> From<VFrameTs<V>> for FTs {
    #[inline(always)]
    fn from(vfts: VFrameTs<V>) -> FTs {
        TimestampOps::into_tstates(vfts)
    }
}

impl<V: VideoFrame> From<FTs> for VFrameTs<V> {
    #[inline(always)]
    fn from(ts: FTs) -> VFrameTs<V> {
        TimestampOps::from_tstates(ts)
    }
}

impl<V> From<VFrameTs<V>> for VideoTs {
    #[inline(always)]
    fn from(vfts: VFrameTs<V>) -> VideoTs {
        vfts.ts
    }
}

impl<V> From<VideoTs> for VFrameTs<V> {
    /// Returns a [VFrameTs] from the given [VideoTs].
    /// A returned `VFrameTs` is not being normalized.
    ///
    /// # Panics
    ///
    /// Panics when the given `ts` overflows the capacity of [VideoTs].
    #[inline(always)]
    fn from(ts: VideoTs) -> Self {
        VFrameTs { ts, _vframe: PhantomData }
    }
}

impl<V, C> From<VFrameTsCounter<V, C>> for VideoTs {
    #[inline(always)]
    fn from(vftsc: VFrameTsCounter<V, C>) -> VideoTs {
        vftsc.vts.ts
    }
}

impl<V, C> From<VFrameTsCounter<V, C>> for VFrameTs<V> {
    #[inline(always)]
    fn from(vftsc: VFrameTsCounter<V, C>) -> VFrameTs<V> {
        vftsc.vts
    }
}

impl<V> Deref for VFrameTs<V> {
    type Target = VideoTs;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.ts
    }
}

impl<V> DerefMut for VFrameTs<V> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.ts
    }
}

impl<V, C> Deref for VFrameTsCounter<V, C> {
    type Target = VFrameTs<V>;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.vts
    }
}

impl<V, C> DerefMut for VFrameTsCounter<V, C> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.vts
    }
}
