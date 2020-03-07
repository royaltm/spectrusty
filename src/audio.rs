/*!
# Audio API
```text
                     /---- ensure_audio_frame_time ----\
  +----------------------+                         +--------+
  |    UlaAudioFrame:    |  render_*_audio_frame   |        |
  |      AudioFrame +    | ======================> |  Blep  |
  | EarMicOutAudioFrame +|     end_audio_frame     |        |
  |   EarInAudioFrame +  |                         |        |
  |   [ AyAudioFrame ]   |                         +--------+
  +----------------------+                             |     
  |      AmpLevels       |        /----------------- (sum)   
  +----------------------+        |                    v       
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

// This is an arbitrary value for Blep implementation to reserve memory for additional samples.
// This is twice the value of the maximum number of wait-states added by an I/O device.
pub(crate) const MARGIN_TSTATES: FTs = 2800;

use core::ops::{Deref, DerefMut};
use core::marker::PhantomData;
use core::num::NonZeroU32;
use crate::clock::VideoTs;
use crate::video::VideoFrame;
pub use sample::{SampleDelta, MulNorm, FromSample};
pub use crate::clock::FTs;

/// A trait for interfacing Bandwidth-Limited Pulse Buffer implementations by audio generators.
///
/// The digitalization of square-waveish sound is being performed via this interface.
pub trait Blep {
    /// A type for sample ∆ amplitudes (pulse height).
    type SampleDelta: SampleDelta;
    /// Calculates a time rate and ensures that the `Blep` implementation has enough memory reserved
    /// for the whole audio frame with an additional margin.
    ///
    /// There should be no need to call this method again unless any of the provided parameters changes.
    ///
    /// * `sample_rate` is a number of audio samples per second.
    /// * `ts_rate` is a number of units per second which are being used as time units.
    /// * `frame_ts` is the duration of a single frame measured in time units.
    /// * `margin_ts` specifies a required margin in time units for frame duration fluctuations.
    fn ensure_frame_time(&mut self, sample_rate: u32, ts_rate: u32, frame_ts: FTs, margin_ts: FTs);
    /// Finalizes audio frame.
    ///
    /// `timestamp` (measured in time units defined with [Blep::ensure_frame_time]) when the frame should
    /// be finalized.
    ///
    /// Returns the number of samples, single channel wise, ready to be rendered.
    ///
    /// The caller must ensure that no pulse should be generated within the finalized frame past
    /// `timestamp` given here.
    /// The implementation may panic if this requirement is not uphold.
    fn end_frame(&mut self, timestamp: FTs) -> usize;
    /// This method is being used to generate square-wave pulses within a single frame.
    ///
    /// `timestamp` is the time of the pulse measured in time units, `delta` is the pulse height (∆ amplitude).
    fn add_step(&mut self, channel: usize, timestamp: FTs, delta: Self::SampleDelta);
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
    /// Sets up [Blep] time rate and ensures enough space for the audio frame is reserved.
    fn ensure_audio_frame_time(&self, blep: &mut B, sample_rate: u32);
    /// Returns a timestamp to be passed to [Blep] to end the frame.
    ///
    /// #Panics
    /// Panics if the current frame execution didn't get to the near of end-of-frame.
    /// To check if you can actually call this method, invoke [ControlUnit::is_frame_over][crate::chip::ControlUnit::is_frame_over].
    fn get_audio_frame_end_time(&self) -> FTs;
    /// Calls [Blep::end_frame] to finalize the frame and prepare it for rendition.
    ///
    /// Returns a number of samples ready to be rendered in a single channel.
    ///
    /// #Panics
    /// Panics if the current frame execution didn't get to the near of end-of-frame.
    /// To check if you can actually call this method, invoke [ControlUnit::is_frame_over][crate::chip::ControlUnit::is_frame_over].
    #[inline]
    fn end_audio_frame(&self, blep: &mut B) -> usize {
        blep.end_frame(self.get_audio_frame_end_time())
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
    /// `channel` - target [Blep] audio channel.
    fn render_earmic_out_audio_frame<V: AmpLevels<B::SampleDelta>>(&self, blep: &mut B, channel: usize);
}

/// A trait for controllers generating audio pulses from the EAR input.
pub trait EarInAudioFrame<B: Blep> {
    /// Renders EAR input as square-wave pulses via [Blep] interface.
    ///
    /// Provide [AmpLevels] that can handle `level` values from 0 to 1 (1-bit).
    /// `channel` - target [Blep] audio channel.
    fn render_ear_in_audio_frame<V: AmpLevels<B::SampleDelta>>(&self, blep: &mut B, channel: usize);
}

/// A trait for reading MIC output.
pub trait MicOut<'a> {
    type PulseIter: Iterator<Item=NonZeroU32> + 'a;
    /// Returns a frame buffered mic output as a pulse iterator.
    fn mic_out_iter_pulses(&'a self) -> Self::PulseIter;
}

/// A trait for feeding EAR input.
pub trait EarIn {
    /// Sets `EAR in` bit state after the provided interval in ∆ T-States counted from the last recorded change.
    fn set_ear_in(&mut self, ear_in: bool, delta_fts: u32);
    /// Feeds the `EAR in` buffer with changes.
    ///
    /// The provided iterator should yield time intervals measured in T-state ∆ differences after which the state
    /// of the `EAR in` bit should be toggled.
    ///
    /// See also [ear_mic][crate::formats::ear_mic].
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
pub const AMPS_EAR_IN:  [f32; 2] = [0.34/3.70, 0.66/3.70];

pub const AMPS_EAR_MIC_I32: [i32; 4] = [0x0bc3_1d10, 0x16d5_1a60, 0x7b28_20ff, 0x7fff_ffff];
pub const AMPS_EAR_OUT_I32: [i32; 4] = [0x0bc3_1d10, 0x0bc3_1d10, 0x7fff_ffff, 0x7fff_ffff];
pub const AMPS_EAR_IN_I32:  [i32; 2] = [0x0bc3_1d10, 0x16d5_1a60];

pub const AMPS_EAR_MIC_I16: [i16; 4] = [0x0bc3, 0x16d5, 0x7b27, 0x7fff];
pub const AMPS_EAR_OUT_I16: [i16; 4] = [0x0bc3, 0x0bc3, 0x7fff, 0x7fff];
pub const AMPS_EAR_IN_I16:  [i16; 2] = [0x0bc3, 0x16d5];

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

impl<B> Blep for BlepAmpFilter<B>
    where B: Blep, B::SampleDelta: MulNorm + SampleDelta
{
    type SampleDelta = B::SampleDelta;

    #[inline]
    fn ensure_frame_time(&mut self, sample_rate: u32, ts_rate: u32, frame_ts: FTs, margin_ts: FTs) {
        self.blep.ensure_frame_time(sample_rate, ts_rate, frame_ts, margin_ts)
    }
    #[inline]
    fn end_frame(&mut self, timestamp: FTs) -> usize {
        self.blep.end_frame(timestamp)
    }
    #[inline]
    fn add_step(&mut self, channel: usize, timestamp: FTs, delta: Self::SampleDelta) {
        self.blep.add_step(channel, timestamp, delta.mul_norm(self.filter))
    }
}

impl<B> Blep for BlepStereo<B>
    where B: Blep, B::SampleDelta: MulNorm + SampleDelta
{
    type SampleDelta = B::SampleDelta;
    #[inline]

    fn ensure_frame_time(&mut self, sample_rate: u32, ts_rate: u32, frame_ts: FTs, margin_ts: FTs) {
        self.blep.ensure_frame_time(sample_rate, ts_rate, frame_ts, margin_ts)
    }
    #[inline]
    fn end_frame(&mut self, timestamp: FTs) -> usize {
        self.blep.end_frame(timestamp)
    }
    #[inline]
    fn add_step(&mut self, channel: usize, timestamp: FTs, delta: B::SampleDelta) {
        match channel {
            0|1 => self.blep.add_step(channel, timestamp, delta),
            _ => {
                let delta = delta.mul_norm(self.mono_filter);
                self.blep.add_step(0, timestamp, delta);
                self.blep.add_step(1, timestamp, delta);
            }
        }
    }
}

pub(crate) fn render_audio_frame_vts<VF,VL,L,A,T>(
            prev_state: u8,
            end_ts: Option<VideoTs>,
            changes: &[T],
            blep: &mut A, channel: usize
        )
    where VF: VideoFrame,
          VL: AmpLevels<L>,
          L: SampleDelta,
          A: Blep<SampleDelta=L>,
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
            let timestamp = VF::vts_to_tstates(ts);
            blep.add_step(channel, timestamp, delta);
            last_vol = next_vol;
        }
    }
}

pub(crate) fn render_audio_frame_ts<VL,L,A,T>(
            prev_state: u8,
            end_ts: Option<FTs>,
            changes: &[T],
            blep: &mut A,
            channel: usize
        )
    where VL: AmpLevels<L>,
          L: SampleDelta,
          A: Blep<SampleDelta=L>,
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
            blep.add_step(channel, ts, delta);
            last_vol = next_vol;
        }
    }
}
