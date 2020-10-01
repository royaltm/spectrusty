/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
use core::cmp::{Ord, PartialEq, PartialOrd};
use core::convert::TryInto;
use core::fmt::Debug;
use core::ops::{Add, Sub, AddAssign, SubAssign};

use crate::video::VideoFrame;
use super::{VFrameTsCounter, VFrameTs, VideoTs, FTs, Ts};

/// A trait providing calculation methods for timestamps.
///
/// Allows [BusDevice][crate::bus::BusDevice] implementations to depend on a generic timestamp type.
pub trait TimestampOps: Copy
                        + PartialEq
                        + Eq
                        + PartialOrd
                        + Ord
                        + Debug
                        + Add<FTs, Output=Self>
                        + Sub<FTs, Output=Self>
                        + AddAssign<FTs>
                        + SubAssign<FTs>
{
    /// Returns a normalized timestamp from the given number of T-states.
    ///
    /// # Panics
    /// Panics when the given `ts` overflows the capacity of the timestamp.
    fn from_tstates(ts: FTs) -> Self;
    /// Converts the timestamp to FTs.
    ///
    /// # Panics
    /// Panics when `self` overflows the capacity of the result type.
    fn into_tstates(self) -> FTs;
    /// Returns the largest value that can be represented by a normalized timestamp.
    fn max_value() -> Self;
    /// Returns the smallest value that can be represented by a normalized timestamp.
    fn min_value() -> Self;
    /// Returns the difference between `ts_from` and `self` in the number of T-states.
    ///
    /// # Panics
    /// May panic if the result would exceed the capacity of the result type.
    fn diff_from(self, ts_from: Self) -> FTs;
    /// Returns a normalized timestamp after adding `other` to it.
    ///
    /// Saturates at [TimestampOps::min_value] or [TimestampOps::max_value].
    fn saturating_add(self, other: Self) -> Self;
    /// Returns a normalized timestamp after subtracting `other` from it.
    ///
    /// Saturates at [TimestampOps::min_value] or [TimestampOps::max_value].
    fn saturating_sub(self, other: Self) -> Self;
}

/// A trait providing simple calculation methods for frame-aware timestamps.
#[deprecated(since = "0.2",
    note = "Do not use. Instead use TimestampOps to decouple timestamps from frames")]
pub trait FrameTimestamp: Copy
                         + PartialEq
                         + Eq
                         + PartialOrd
                         + Ord
                         + Debug
                         + Add<u32, Output=Self>
                         + Sub<u32, Output=Self>
                         + AddAssign<u32>
                         + SubAssign<u32>
                         + Into<FTs>
                         + From<FTs>
{
    fn from_tstates(ts: FTs) -> Self;
    fn into_tstates(self) -> FTs;
    fn into_frame_tstates(self, frames: u64) -> (u64, FTs);
    fn max_value() -> Self;
    fn min_value() -> Self;
    fn is_eof(self) -> bool;
    fn diff_from(self, vts_from: Self) -> FTs;
    fn saturating_sub_frame(self) -> Self;
    fn saturating_sub(self, other: Self) -> Self;
    fn saturating_add(self, other: Self) -> Self;
    fn wrap_frame(&mut self);
    fn min_tstates() -> FTs {
        Self::min_value().into_tstates()
    }
    fn max_tstates() -> FTs {
        Self::max_value().into_tstates()
    }
}

impl TimestampOps for FTs {
    #[inline(always)]
    fn from_tstates(ts: FTs) -> Self {
        ts
    }
    #[inline(always)]
    fn into_tstates(self) -> FTs {
        self
    }
    #[inline(always)]
    fn max_value() -> Self {
        FTs::max_value()
    }
    #[inline(always)]
    fn min_value() -> Self {
        FTs::min_value()
    }
    #[inline(always)]
    fn diff_from(self, ts_from: Self) -> FTs {
        self - ts_from
    }
    #[inline(always)]
    fn saturating_add(self, other: Self) -> Self {
        self.saturating_add(other)
    }
    #[inline(always)]
    fn saturating_sub(self, other: Self) -> Self {
        self.saturating_sub(other)
    }
}

impl <V: VideoFrame> TimestampOps for VFrameTs<V> {
    #[inline]
    fn from_tstates(ts: FTs) -> Self {
        VFrameTs::from_tstates(ts)
    }
    #[inline]
    fn into_tstates(self) -> FTs {
        VFrameTs::into_tstates(self)
    }
    #[inline(always)]
    fn max_value() -> Self {
        VFrameTs::max_value()
    }
    #[inline(always)]
    fn min_value() -> Self {
        VFrameTs::min_value()
    }
    #[inline]
    fn diff_from(self, vts_from: Self) -> FTs {
        (self.vc as FTs - vts_from.vc as FTs) * V::HTS_COUNT as FTs +
        (self.hc as FTs - vts_from.hc as FTs)
    }
    /// # Panics
    /// May panic if `self` or `other` hasn't been normalized.
    fn saturating_add(self, other: Self) -> Self {
        let VideoTs { vc, hc } = self.ts;
        let vc = vc.saturating_add(other.vc);
        let hc = hc + other.hc;
        VFrameTs::new(vc, hc).saturating_normalized()
    }
    /// # Panics
    /// May panic if `self` or `other` hasn't been normalized.
    fn saturating_sub(self, other: Self) -> Self {
        let VideoTs { vc, hc } = self.ts;
        let vc = vc.saturating_sub(other.vc);
        let hc = hc - other.hc;
        VFrameTs::new(vc, hc).saturating_normalized()
    }
}

impl<V: VideoFrame> Add<FTs> for VFrameTs<V> {
    type Output = Self;
    /// Returns a normalized video timestamp after adding `delta` T-states.
    ///
    /// # Panics
    /// Panics when normalized timestamp after addition leads to an overflow of the capacity of [VideoTs].
    #[inline]
    fn add(self, delta: FTs) -> Self {
        let VideoTs { vc, hc } = self.ts;
        let dvc: Ts = (delta / V::HTS_COUNT as FTs).try_into().expect("absolute delta FTs is too large");
        let dhc = (delta % V::HTS_COUNT as FTs) as Ts;
        VFrameTs::new(vc + dvc, hc + dhc).normalized()
    }
}

impl<V: VideoFrame> AddAssign<FTs> for VFrameTs<V> {
    #[inline(always)]
    fn add_assign(&mut self, delta: FTs) {
        *self = *self + delta
    }
}

impl<V: VideoFrame> Sub<FTs> for VFrameTs<V> {
    type Output = Self;
    /// Returns a normalized video timestamp after subtracting `delta` T-states.
    ///
    /// # Panics
    /// Panics when normalized timestamp after addition leads to an overflow of the capacity of [VideoTs].
    #[inline]
    fn sub(self, delta: FTs) -> Self {
        let VideoTs { vc, hc } = self.ts;
        let dvc: Ts = (delta / V::HTS_COUNT as FTs).try_into().expect("delta too large");
        let dhc = (delta % V::HTS_COUNT as FTs) as Ts;
        VFrameTs::new(vc - dvc, hc - dhc).normalized()
    }
}

impl<V: VideoFrame> SubAssign<FTs> for VFrameTs<V> {
    #[inline(always)]
    fn sub_assign(&mut self, delta: FTs) {
        *self = *self - delta
    }
}

#[allow(deprecated)]
impl <V: VideoFrame> FrameTimestamp for VFrameTs<V> {
    #[inline]
    fn from_tstates(ts: FTs) -> Self {
        TimestampOps::from_tstates(ts)
    }
    #[inline]
    fn into_tstates(self) -> FTs {
        TimestampOps::into_tstates(self)
    }
    #[inline]
    fn into_frame_tstates(self, frames: u64) -> (u64, FTs) {
        VFrameTs::into_frame_tstates(self, frames)
    }
    #[inline(always)]
    fn max_value() -> Self {
        VFrameTs::max_value()
    }
    #[inline(always)]
    fn min_value() -> Self {
        VFrameTs::min_value()
    }
    #[inline(always)]
    fn is_eof(self) -> bool {
        VFrameTs::is_eof(self)
    }
    #[inline]
    fn diff_from(self, vts_from: Self) -> FTs {
        TimestampOps::diff_from(self, vts_from)
    }
    #[inline]
    fn saturating_sub_frame(self) -> Self {
        VFrameTs::saturating_sub_frame(self)
    }
    fn saturating_sub(self, other: Self) -> Self {
        TimestampOps::saturating_sub(self, other)
    }
    fn saturating_add(self, other: Self) -> Self {
        TimestampOps::saturating_add(self, other)
    }
    #[inline(always)]
    fn wrap_frame(&mut self) {
        VFrameTs::wrap_frame(self)
    }
}

impl<V: VideoFrame> Add<u32> for VFrameTs<V> {
    type Output = Self;
    /// Returns a normalized video timestamp after adding a `delta` T-state count.
    ///
    /// # Panics
    /// Panics when normalized timestamp after addition leads to an overflow of the capacity of [VideoTs].
    #[inline]
    fn add(self, delta: u32) -> Self {
        let VideoTs { vc, hc } = self.ts;
        let dvc = (delta / V::HTS_COUNT as u32).try_into().expect("delta too large");
        let dhc = (delta % V::HTS_COUNT as u32) as Ts;
        let vc = vc.checked_add(dvc).expect("delta too large");
        VFrameTs::new(vc, hc + dhc).normalized()
    }
}

impl<V: VideoFrame> AddAssign<u32> for VFrameTs<V> {
    #[inline(always)]
    fn add_assign(&mut self, delta: u32) {
        *self = *self + delta
    }
}

impl<V: VideoFrame> Sub<u32> for VFrameTs<V> {
    type Output = Self;
    /// Returns a normalized video timestamp after adding a `delta` T-state count.
    ///
    /// # Panics
    /// Panics when normalized timestamp after addition leads to an overflow of the capacity of [VideoTs].
    #[inline]
    fn sub(self, delta: u32) -> Self {
        let VideoTs { vc, hc } = self.ts;
        let dvc = (delta / V::HTS_COUNT as u32).try_into().expect("delta too large");
        let dhc = (delta % V::HTS_COUNT as u32) as Ts;
        let vc = vc.checked_sub(dvc).expect("delta too large");
        VFrameTs::new(vc, hc - dhc).normalized()
    }
}

impl<V: VideoFrame> SubAssign<u32> for VFrameTs<V> {
    #[inline(always)]
    fn sub_assign(&mut self, delta: u32) {
        *self = *self - delta
    }
}

impl<V: VideoFrame, C> AddAssign<u32> for VFrameTsCounter<V, C> {
    fn add_assign(&mut self, delta: u32) {
        self.vts += delta;
    }
}

impl<V: VideoFrame, C> SubAssign<u32> for VFrameTsCounter<V, C> {
    fn sub_assign(&mut self, delta: u32) {
        self.vts -= delta;
    }
}
