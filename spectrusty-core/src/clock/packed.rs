/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
use super::{Ts, FTs, VideoTs, VFrameTs, VideoFrame};

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

        impl<V> From<(VFrameTs<V>, u8)> for $name {
            fn from(tuple: (VFrameTs<V>, u8)) -> Self {
                let (vts, data) = tuple;
                $name::new(vts.into(), data)
            }
        }

        impl From<$name> for (VideoTs, u8) {
            fn from(vtsd: $name) -> Self {
                let $name { vc, hc_data } = vtsd;
                let data = (hc_data as u8) & ((1 << $bits) - 1);
                (VideoTs { vc, hc: hc_data >> $bits}, data)
            }
        }

        impl<V: VideoFrame> From<$name> for (VFrameTs<V>, u8) {
            fn from(vtsd: $name) -> Self {
                let $name { vc, hc_data } = vtsd;
                let data = (hc_data as u8) & ((1 << $bits) - 1);
                (VFrameTs::new(vc, hc_data >> $bits), data)
            }
        }

        impl From<$name> for VideoTs {
            fn from(vtsd: $name) -> Self {
                let $name { vc, hc_data } = vtsd;
                VideoTs { vc, hc: hc_data >> $bits}
            }
        }

        impl<V: VideoFrame> From<$name> for VFrameTs<V> {
            fn from(vtsd: $name) -> Self {
                let $name { vc, hc_data } = vtsd;
                VFrameTs::new(vc, hc_data >> $bits)
            }
        }

        impl From<&$name> for VideoTs {
            fn from(vtsd: &$name) -> Self {
                Self::from(*vtsd)
            }
        }

        impl<V: VideoFrame> From<&$name> for VFrameTs<V> {
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
