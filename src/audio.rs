/*!
# Audio API
```text
                     /---- ensure_audio_frame_time ----\
  +----------------------+                         +--------+
  |      AudioFrame      |  render_*_audio_frame   |        |
  | EarMicOutAudioFrame  | ======================> |  Blep  |
  |   EarInAudioFrame    |     end_audio_frame     |        |
  |   [ AyAudioFrame ]   |                         |        |
  +----------------------+                         +--------+
  |      AmpLevels       |                             |     
  +----------------------+        /----------------- (sum)   
                                  |                    v       
                                  v                 carousel 
                           +-------------+   +--------------------+
                           | (WavWriter) |   | AudioFrameProducer |
                           +-------------+   +--------------------+
                                               || (AudioBuffer) ||
                                             +====================+
                                             |  * audio thread *  |
                                             |                    |
                                             | AudioFrameConsumer |
                                             +====================+
```
*/
#![allow(clippy::eq_op)]
pub mod synth;
pub mod carousel;
pub mod sample;
pub mod ay;
pub mod music;

pub(crate) const MARGIN_TSTATES: FTs = 2 * 23;

use core::ops::{Deref, DerefMut};
use core::marker::PhantomData;
use core::num::NonZeroU32;
use crate::clock::{FTs, VideoTs, VFrameTsCounter};
use crate::video::VideoFrame;
pub use sample::{SampleDelta, SampleTime, MulNorm, FromSample};

/// A trait for interfacing Bandwidth-Limited Pulse Buffer implementations by audio generators.
///
/// The digitalization of square-waveish sound is being performed via this interface.
pub trait Blep {
    /// A type for sample ∆ amplitudes (pulse height).
    type SampleDelta: SampleDelta;
    /// A type for sample time indices.
    type SampleTime: SampleTime;
    /// Ensures that the `Blep` implementation has enough memory reserved for the whole audio frame and then some.
    ///
    /// `frame_time` is the frame time measured in samples.
    /// `margin_time` is the required margin time of the frame duration fluctuations.
    fn ensure_frame_time(&mut self, frame_time: Self::SampleTime, margin_time: Self::SampleTime);
    /// Finalizes audio frame.
    ///
    /// `time` (measured in samples) is being provided when the frame should be finalized.
    /// The caller must ensure that no pulse should be generated within this frame past given `time` here.
    /// The implementation may panic if this requirement is not uphold.
    /// Returns the number of samples ready to be rendered, independent from the number of channels.
    fn end_frame(&mut self, time: Self::SampleTime) -> usize;
    /// This method is being used to generate square pulses.
    ///
    /// `time` is the time of the pulse measured in samples, `delta` is the pulse height (∆ amplitude).
    fn add_step(&mut self, channel: usize, time: Self::SampleTime, delta: Self::SampleDelta);
}

/// A wrapper [Blep] implementation that filters pulses' ∆ amplitude before sending them to the
/// underlying implementation.
///
/// `BlepAmpFilter` may be used to adjust generated audio volume dynamically.
///
/// *NOTE*: To emulate a linear volume control, `filter` value should be scaled
/// [logarithmically](https://www.dr-lex.be/info-stuff/volumecontrols.html).
pub struct BlepAmpFilter<B: Blep> {
    /// A normalized filter value in the range `[0.0, 1.0]` (floats) or `[0, int::max_value()]` (integers).
    pub filter: B::SampleDelta,
    /// A downstream [Blep] implementation.
    pub blep: B,
}
/// A wrapper [Blep] implementation that redirects extra channels to a stereo [Blep]
/// as a monophonic channel.
///
/// Requires a downstream [Blep] implementation that provides at least 2 audio channels.
/// ```text
/// BlepStereo channel      Blep impl channel
///     0 -----------------------> 0
///     1 -----------------------> 1
///  >= 2 ---- * mono_filter ----> 0
///                          \---> 1
/// ```
pub struct BlepStereo<B: Blep> {
    /// A monophonic filter value in the range [0.0, 1.0] (floats) or [0, int::max_value()] (integers).
    pub mono_filter: B::SampleDelta,
    /// A downstream [Blep] implementation.
    pub blep: B,
}

/// A digital level to a sample amplitude convertion trait.
pub trait AmpLevels<T: Copy> {
    /// This method should return the appropriate digital sample amplitude for the given `level`.
    ///
    /// The best approximation is `a*(level/max_level*b).exp()` according to
    /// [this document](https://www.dr-lex.be/info-stuff/volumecontrols.html).
    ///
    /// *Please note* that most callbacks use only a limited number of bits in the `level`.
    fn amp_level(level: u32) -> T;
}

/// A common trait for controllers rendering square-wave audio pulses.
/// 
/// This trait defines common methods to interface [Blep] implementations.
pub trait AudioFrame<B: Blep> {
    /// Ensures [Blep] has enough space for the audio frame and returns `time_rate` to be passed
    /// to `render_*_audio_frame` methods.
    fn ensure_audio_frame_time(blep: &mut B, sample_rate: u32) -> B::SampleTime;
    /// Returns a sample time to be passed to [Blep] to end the frame.
    fn get_audio_frame_end_time(&self, time_rate: B::SampleTime) -> B::SampleTime;
    /// Calls [Blep::end_frame] to finalize the frame and prepare it for rendition.
    ///
    /// Returns a number of samples ready to be rendered in a single channel.
    #[inline]
    fn end_audio_frame(&self, blep: &mut B, time_rate: B::SampleTime) -> usize {
        blep.end_frame(self.get_audio_frame_end_time(time_rate))
    }
}

/// A trait for controllers generating audio pulses from the EAR/MIC output.
pub trait EarMicOutAudioFrame<B: Blep> {
    /// Renders EAR/MIC output as square-wave pulses via [Blep] interface.
    ///
    /// Provide [AmpLevels] that can handle `level` values from 0 to 3 (2-bits).
    /// ```text
    ///   EAR  MIC  level
    ///    0    0     0
    ///    0    1     1
    ///    1    0     2
    ///    1    1     3
    ///```
    /// `time_rate` may be optained from calling [AudioFrame::ensure_audio_frame_time].
    /// `channel` - target [Blep] audio channel.
    fn render_earmic_out_audio_frame<V: AmpLevels<B::SampleDelta>>(&self, blep: &mut B, time_rate: B::SampleTime, channel: usize);
}

/// A trait for controllers generating audio pulses from the EAR input.
pub trait EarInAudioFrame<B: Blep> {
    /// Renders EAR input as square-wave pulses via [Blep] interface.
    ///
    /// Provide [AmpLevels] that can handle `level` values from 0 to 1 (1-bit).
    /// `time_rate` may be optained from calling [AudioFrame::ensure_audio_frame_time].
    /// `channel` - target [Blep] audio channel.
    fn render_ear_in_audio_frame<V: AmpLevels<B::SampleDelta>>(&self, blep: &mut B, time_rate: B::SampleTime, channel: usize);
}

/// A trait for feeding EAR input frame buffer.
pub trait EarIn {
    /// Sets `EAR in` bit state after the provided interval in ∆ T-States counted from the last recorded change.
    fn set_ear_in(&mut self, ear_in: bool, delta_fts: u32);
    /// Feeds the `EAR in` buffer with changes.
    ///
    /// The provided iterator should yield time intervals measured in T-state ∆ differences after which the state
    /// of the `EAR in` bit should be toggled.
    ///
    /// See also [read_ear][crate::formats::read_ear].
    ///
    /// `max_frames_threshold` may be optionally provided as a number of frames to limit the buffered changes.
    /// This is usefull if the given iterator provides data largely exceeding the duration of a single frame.
    fn feed_ear_in<I: Iterator<Item=NonZeroU32>>(&mut self, fts_deltas: &mut I, max_frames_threshold: Option<usize>);
    /// Removes all buffered so far `EAR in` changes.
    ///
    /// Changes are usually consumed only when a call is made to [crate::chip::ControlUnit::ensure_next_frame].
    /// Provide the current value of `EAR in` bit as `ear_in`.
    ///
    /// This may be usefull when tape data is already buffered but the user decided to stop the tape playback immediately.
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

pub const AMPS_EAR_MIC_I32: [i32; 4] = [0x0bc3_1d10, 0x16d5_1a60, 0x7b28_20ff, 0x7fff_ffff];
pub const AMPS_EAR_OUT_I32: [i32; 4] = [0x0bc3_1d10, 0x0bc3_1d10, 0x7fff_ffff, 0x7fff_ffff];
pub const AMPS_EAR_IN_I32:  [i32; 2] = [0x0bc3_1d10, 0x7fff_ffff];

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

impl<B: Blep> BlepAmpFilter<B> {
    pub fn build(filter: B::SampleDelta) -> impl FnOnce(B) -> Self
    // where D: Into<B>
    {
        move |blep| Self::new(filter, blep)
    }

    pub fn new(filter: B::SampleDelta, blep: B) -> Self {
        BlepAmpFilter { blep, filter }
    }
}

// let amp_build = BlepAmpFilter::build(0.777);
// let stereo_build = BlepStereo::build(0.5);
// let blep = BandLimited::<f32>::build(3);
// amp_build(stereo_build(blep))
// BandLimited::<f32>::new(3).wrap_with(BlepStereo::build(0.5)).wrap_with(BlepAmpFilter::build(0.777));
// BlepAmpFilter::build(0.777)
// .wrap(BlepStereo::build(0.5))
// .wrap(BandLimited::<f32>::build(3))
// .new();
impl<B: Blep> BlepStereo<B> {
    pub fn build(mono_filter: B::SampleDelta) -> impl FnOnce(B) -> Self
    // where D: Into<B>
    {
        move |blep| Self::new(mono_filter, blep)
    }

    pub fn new(mono_filter: B::SampleDelta, blep: B) -> Self {
        BlepStereo { blep, mono_filter }
    }
}

impl<B: Blep> Deref for BlepAmpFilter<B> {
    type Target = B;
    fn deref(&self) -> &B {
        &self.blep
    }
}

impl<B: Blep> DerefMut for BlepAmpFilter<B> {
    fn deref_mut(&mut self) -> &mut B {
        &mut self.blep
    }
}

impl<B: Blep> Deref for BlepStereo<B> {
    type Target = B;
    fn deref(&self) -> &B {
        &self.blep
    }
}

impl<B: Blep> DerefMut for BlepStereo<B> {
    fn deref_mut(&mut self) -> &mut B {
        &mut self.blep
    }
}

impl<B> Blep for BlepAmpFilter<B> where B: Blep, B::SampleDelta: MulNorm + SampleDelta {
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

impl<B> Blep for BlepStereo<B> where B: Blep, B::SampleDelta: MulNorm + SampleDelta {
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
                let delta = delta.mul_norm(self.mono_filter);
                self.blep.add_step(0, time, delta);
                self.blep.add_step(1, time, delta);
            }
        }
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
