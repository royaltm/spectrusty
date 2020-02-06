pub mod synth;
pub mod carousel;
pub mod sample;
pub mod ay;
pub mod music;

pub(crate) const MARGIN_TSTATES: FTs = 2 * 23;

use core::num::NonZeroU32;
use crate::clock::{FTs, VideoTs, VFrameTsCounter};
use crate::video::VideoFrame;
pub use synth::BandLimited;
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

pub trait EarMicAudioFrame<B: Blep> {
    /// Returns the current frame's `time_end` for the blep buffer to end with for the given sample_rate.
    fn render_earmic_out_audio_frame<V: AmpLevels<B::SampleDelta>>(&self, blep: &mut B, time_rate: B::SampleTime, channel: usize);
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

pub(crate) fn render_audio_frame<VF,VL,L,A,FT,T>(prev_state: u8,
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
            if ts > end_ts {
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
