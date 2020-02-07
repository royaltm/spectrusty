pub mod synth;
pub mod carousel;
pub mod sample;
pub mod ay;
pub mod music;

pub(crate) const MARGIN_TSTATES: FTs = 2 * 23;

use core::marker::PhantomData;
use core::num::NonZeroU32;
use crate::clock::{FTs, VideoTs, VFrameTsCounter};
use crate::video::VideoFrame;
pub use sample::{SampleDelta, SampleTime, MulNorm, FromSample};

/// A trait interfacing a Bandwidth-Limited Pulse Buffer implementation.
/// All square-waveish sound is being created via this interface.
pub trait Blep {
    /// A type for delta pulses.
    type SampleDelta: SampleDelta;
    /// A type for sample time indices.
    type SampleTime: SampleTime;
    /// `frame_time` is the frame time in samples (1.0 is one sample),
    /// `margin_time` is the margin time of the frame duration fluctuations.
    fn ensure_frame_time(&mut self, frame_time: Self::SampleTime, margin_time: Self::SampleTime);
    /// `time` is the time in samples when the frame should be finalized.
    /// The caller must ensure that no delta should be rendered past this time.
    /// Returns the number of samples (in a single channel) rendered.
    fn end_frame(&mut self, time: Self::SampleTime) -> usize;
    /// `time` is the time in samples, delta should be a normalized [-1.0, 1.0] float or an integer.
    fn add_step(&mut self, channel: usize, time: Self::SampleTime, delta: Self::SampleDelta);
}

/// A wrapper Blep implementation that filters delta value before sending it downstream.
/// You may use it to adjust generated audio volume dynamically.
pub struct BlepAmpFilter<B, T> {
    /// A downstream Blep implementation.
    pub blep: B,
    /// A normalized filter value in the range [0.0, 1.0] (floats) or [0, int::max_value()] (integers).
    pub filter: T
}

/// A wrapper Blep implementation that redirects channels to a stereo Blep.
/// Requires a downstream Blep implementation that implements at least 2 channels.
/// Channel 0 and 1 are redirected downstream but any other channel is being filtered before being redirected to both channels (0 and 1).
pub struct BlepStereo<B, T> {
    /// A downstream Blep implementation.
    pub blep: B,
    /// A monophonic filter value in the range [0.0, 1.0] (floats) or [0, int::max_value()] (integers).
    pub mono_filter: T
}

/// A digital level to sound amplitude convertion trait.
pub trait AmpLevels<T: Copy> {
    /// Should return the appropriate sound sample amplitude for the given `level`.
    /// The best approximation is a*(x/max_level*b).exp().
    /// Most callbacks uses only limited number of bits of the level.
    fn amp_level(level: u32) -> T;
}

pub trait AudioFrame<B: Blep> {
    /// Ensures Blep frame size and returns `time_rate` to be passed to `render_*_audio_frame` methods.
    fn ensure_audio_frame_time(blep: &mut B, sample_rate: u32) -> B::SampleTime;
    /// Returns a sample time to be passed to Blep to end the frame.
    fn get_audio_frame_end_time(&self, time_rate: B::SampleTime) -> B::SampleTime;
    /// Calls blep.end_frame(..) to finalize the frame and prepare it to be rendered.
    #[inline]
    fn end_audio_frame(&self, blep: &mut B, time_rate: B::SampleTime) -> usize {
        blep.end_frame(self.get_audio_frame_end_time(time_rate))
    }
}

pub trait EarMicOutAudioFrame<B: Blep> {
    fn render_earmic_out_audio_frame<V: AmpLevels<B::SampleDelta>>(&self, blep: &mut B, time_rate: B::SampleTime, channel: usize);
}

pub trait EarInAudioFrame<B: Blep> {
    fn render_ear_in_audio_frame<V: AmpLevels<B::SampleDelta>>(&self, blep: &mut B, time_rate: B::SampleTime, channel: usize);
}

pub trait EarIn {
    /// Sets `ear_in` bit state for the provided delta T-states from the last ear_in change or current counter's value.
    fn set_ear_in(&mut self, ear_in: bool, delta_fts: u32);
    /// Feeds the `ear_in` buffer with changes propagated as T-state deltas via iterator.
    fn feed_ear_in<I: Iterator<Item=NonZeroU32>>(&mut self, fts_deltas: &mut I, max_frames_threshold: Option<usize>);
    /// Removes all buffered ear in changes.
    fn purge_ear_in_changes(&mut self, ear_in: bool);
}

/*
Value output to bit: 4  3  |  Iss 2  Iss 3   Iss 2 V    Iss 3 V
                     1  1  |    1      1       3.79       3.70
                     1  0  |    1      1       3.66       3.56
                     0  1  |    1      0       0.73       0.66
                     0  0  |    0      0       0.39       0.34
*/
pub const AMPS_EAR_MIC: [f32; 4] = [0.34/3.70, 0.66/3.70, 3.56/3.70, 3.70/3.70];
pub const AMPS_EAR_OUT: [f32; 4] = [0.34/3.70, 0.34/3.70, 3.70/3.70, 3.70/3.70];
pub const AMPS_EAR_IN:  [f32; 2] = [0.34/3.70, 3.70/3.70];

pub const AMPS_EAR_MIC_I32: [i32; 4] = [0x0bc31d10, 0x16d51a60, 0x7b2820ff, 0x7fffffff];
pub const AMPS_EAR_OUT_I32: [i32; 4] = [0x0bc31d10, 0x0bc31d10, 0x7fffffff, 0x7fffffff];
pub const AMPS_EAR_IN_I32:  [i32; 2] = [0x0bc31d10, 0x7fffffff];

pub const AMPS_EAR_MIC_I16: [i16; 4] = [0x0bc3, 0x16d5, 0x7b27, 0x7fff];
pub const AMPS_EAR_OUT_I16: [i16; 4] = [0x0bc3, 0x0bc3, 0x7fff, 0x7fff];
pub const AMPS_EAR_IN_I16:  [i16; 2] = [0x0bc3, 0x7fff];

#[derive(Clone, Default, Debug)]
pub struct EarMicAmps4<T>(PhantomData<T>);
#[derive(Clone, Default, Debug)]
pub struct EarOutAmps4<T>(PhantomData<T>);
#[derive(Clone, Default, Debug)]
pub struct EarInAmps2<T>(PhantomData<T>);

macro_rules! impl_amp_levels {
    ($([$ty:ty, $ear_mic:ident, $ear_out:ident, $ear_in:ident]),*) => { $(
        impl AmpLevels<$ty> for EarMicAmps4<$ty> {
            #[inline(always)]
            fn amp_level(level: u32) -> $ty {
                $ear_mic[(level & 3) as usize]
            }
        }

        impl AmpLevels<$ty> for EarOutAmps4<$ty> {
            #[inline(always)]
            fn amp_level(level: u32) -> $ty {
                $ear_out[(level & 3) as usize]
            }
        }

        impl AmpLevels<$ty> for EarInAmps2<$ty> {
            #[inline(always)]
            fn amp_level(level: u32) -> $ty {
                $ear_in[(level & 1) as usize]
            }
        }
    )* };
}
impl_amp_levels!([f32, AMPS_EAR_MIC, AMPS_EAR_OUT, AMPS_EAR_IN],
                 [i32, AMPS_EAR_MIC_I32, AMPS_EAR_OUT_I32, AMPS_EAR_IN_I32],
                 [i16, AMPS_EAR_MIC_I16, AMPS_EAR_OUT_I16, AMPS_EAR_IN_I16]);

impl <B, T> BlepAmpFilter<B, T> {
    pub fn new(blep: B, filter: T) -> Self {
        BlepAmpFilter { blep, filter }
    }
}

impl<B> Blep for BlepStereo<B, B::SampleDelta> where B: Blep, B::SampleDelta: MulNorm + SampleDelta {
    type SampleDelta = B::SampleDelta;
    type SampleTime = B::SampleTime;
    #[inline]
    fn ensure_frame_time(&mut self, frame_time: Self::SampleTime, margin_time: Self::SampleTime) {
        self.blep.ensure_frame_time(frame_time, margin_time)
    }
    #[inline]
    fn end_frame(&mut self, time: Self::SampleTime) -> usize {
        self.blep.end_frame(time)
    }
    #[inline]
    fn add_step(&mut self, channel: usize, time: Self::SampleTime, delta: B::SampleDelta) {
        match channel {
            0|1 => self.blep.add_step(channel, time, delta),
            _ => {
                self.blep.add_step(0, time, delta.mul_norm(self.mono_filter));
                self.blep.add_step(1, time, delta.mul_norm(self.mono_filter));
            }
        }
    }
}

impl<B> Blep for BlepAmpFilter<B, B::SampleDelta> where B: Blep, B::SampleDelta: MulNorm + SampleDelta {
    type SampleDelta = B::SampleDelta;
    type SampleTime = B::SampleTime;
    #[inline]
    fn ensure_frame_time(&mut self, frame_time: Self::SampleTime, margin_time: Self::SampleTime) {
        self.blep.ensure_frame_time(frame_time, margin_time)
    }
    #[inline]
    fn end_frame(&mut self, time: Self::SampleTime) -> usize {
        self.blep.end_frame(time)
    }
    #[inline]
    fn add_step(&mut self, channel: usize, time: Self::SampleTime, delta: Self::SampleDelta) {
        self.blep.add_step(channel, time, delta.mul_norm(self.filter))
    }
}

pub(crate) fn render_audio_frame_vts<VF,VL,L,A,FT,T>(prev_state: u8,
                                                end_ts: Option<VideoTs>,
                                                changes: &[T],
                                                blep: &mut A, time_rate: FT, channel: usize)
where VF: VideoFrame,
      VL: AmpLevels<L>,
      L: SampleDelta,
      A: Blep<SampleDelta=L, SampleTime=FT>,
      FT: SampleTime,
      T: Copy, (VideoTs, u8): From<T>,
{
    let mut last_vol = VL::amp_level(prev_state.into());
    for &tsd in changes.iter() {
        let (ts, state) = tsd.into();
        if let Some(end_ts) = end_ts {
            if ts >= end_ts { // TODO >= or >
                break
            }
        }
        let next_vol = VL::amp_level(state.into());
        if let Some(delta) = last_vol.sample_delta(next_vol) {
            let time = time_rate.at_timestamp(VFrameTsCounter::<VF>::from(ts).as_tstates());
            blep.add_step(channel, time, delta);
            last_vol = next_vol;
        }
    }
}

pub(crate) fn render_audio_frame_ts<VL,L,A,FT,T>(prev_state: u8,
                                                end_ts: Option<FTs>,
                                                changes: &[T],
                                                blep: &mut A, time_rate: FT, channel: usize)
where VL: AmpLevels<L>,
      L: SampleDelta,
      A: Blep<SampleDelta=L, SampleTime=FT>,
      FT: SampleTime,
      T: Copy, (FTs, u8): From<T>,
{
    let mut last_vol = VL::amp_level(prev_state.into());
    for &tsd in changes.iter() {
        let (ts, state) = tsd.into();
        if let Some(end_ts) = end_ts {
            if ts >= end_ts { // TODO >= or >
                break
            }
        }
        // print!("{}:{}  ", state, ts);
        let next_vol = VL::amp_level(state.into());
        if let Some(delta) = last_vol.sample_delta(next_vol) {
            let time = time_rate.at_timestamp(ts);
            blep.add_step(channel, time, delta);
            last_vol = next_vol;
        }
    }
}
