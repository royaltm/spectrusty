/*
    Copyright (C) 2020-2022  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
//! # Audio API.
mod sample;

use core::ops::{Deref, DerefMut};
use core::marker::PhantomData;

use crate::clock::{VFrameTs, VideoTs};
use crate::video::VideoFrame;
pub use sample::{
    AudioSample,
    FromSample,
    IntoSample,
    SampleDelta,
    MulNorm
};
pub use crate::clock::FTs;

/// A trait for interfacing Bandwidth-Limited Pulse Buffer implementations by square-wave audio generators.
///
/// The perfect square wave can be represented as an infinite sum of sinusoidal waves. The problem is
/// that the frequency of those waves tends to infinity. The digitalization of sound is limited by the
/// finite sample frequency and the maximum frequency that can be sampled is called the [Nyquist frequency].
///
/// When sampling of a square wave is naively implemented it produces a perceptible, unpleasant aliasing noise.
///
/// Square waves that sound "clear" should be constructed from a limited number of sinusoidal waves, but the
/// computation of such a wave could be costly.
///
/// However thanks to the [Hard Sync] technique it is not necessary. Instead, a precomputed pattern is being
/// applied to each "pulse step". The implementation of this is called the Bandwidth-Limited Pulse Buffer
/// in short `Blimp` or `Blep`.
///
/// The audio stream is being produced in frames by the `Blep` implementation. First, the pulse steps are
/// being added, then the frame is being finalized and after that, the audio samples can be generated from
/// the collected pulses.
///
/// The way audio samples are being generated is outside of the scope of the `Blep` implementation. This trait
/// only defines an interface for adding pulse steps and finalizing frames.
///
/// [Nyquist frequency]: https://en.wikipedia.org/wiki/Nyquist_frequency
/// [Hard Sync]: https://www.cs.cmu.edu/~eli/papers/icmc01-hardsync.pdf
pub trait Blep {
    /// A type for sample ∆ amplitudes (pulse height).
    type SampleDelta: SampleDelta;
    /// This method allows the `Blep` implementation to reserve enough memory for the audio 
    /// frame with an additional margin and to set up a sample time rate and other internals.
    ///
    /// This method should not be called again unless any of the provided parameters changes.
    ///
    /// * `sample_rate` is a number of output audio samples per second.
    /// * `ts_rate` is a number of time units per second which are being used as input time stamps.
    /// * `frame_ts` is a duration of a single frame measured in time units specified with `ts_rate`.
    /// * `margin_ts` specifies a largest required margin in time units for frame duration fluctuations.
    ///
    /// Each frame's duration may fluctuate randomly as long as it's not constantly growing nor shrinking
    /// and on average equals to `frame_ts`.
    ///
    /// Specifically `frame_ts` + `margin_ts` - `frame start` specifies the largest value of a time stamp
    /// that can be passed to [Blep::add_step] or [Blep::end_frame].
    /// `frame_ts` - `margin_ts` - `frame start` specifies the smallest value of a time stamp
    /// that can be passed to [Blep::end_frame].
    ///
    /// The smallest possible time stamp value that can be passed to [Blep::add_step] is a `frame start`
    /// time stamp. It starts at 0 after calling this method and is modified by the `timestamp` value passed
    /// to the last [Blep::end_frame] to `timestamp` - `frame_ts`. In other words, the next frame starts
    /// when the previous ends minus frame duration.
    fn ensure_frame_time(&mut self, sample_rate: u32, ts_rate: f64, frame_ts: FTs, margin_ts: FTs);
    /// This method is being used to add square-wave pulse steps within a boundary of a single frame.
    ///
    //  * `channel` specifies an output audio channel.
    /// * `timestamp` specifies the time stamp of the pulse.
    /// * `delta` specifies the pulse height (∆ amplitude).
    ///
    /// The implementation may panic if `timestamp` boundary limits are not uphold.
    fn add_step(&mut self, channel: usize, timestamp: FTs, delta: Self::SampleDelta);
    /// Finalizes audio frame.
    ///
    /// Some frames can end little late or earlier and this method should allow for such a flexibility.
    ///
    /// `timestamp` specifies when the frame should be finalized marking the timestamp of the next frame.
    ///
    /// Returns the number of samples that will be produced, single channel wise.
    ///
    /// The caller must ensure that no pulse step should be generated with a time stamp past `timstamp`
    /// given here.
    ///
    /// The implementation may panic if this requirement is not uphold.
    fn end_frame(&mut self, timestamp: FTs) -> usize;
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

/// A digital level to a sample amplitude conversion trait.
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
    /// Sets up the [Blep] time rate and ensures there is enough room for the single frame's audio data.
    ///
    /// * `sample_rate` is the number of samples per second of the rendered audio,
    /// * `cpu_hz` is the number of the emulated CPU cycles (T-states) per second.
    fn ensure_audio_frame_time(&self, blep: &mut B, sample_rate: u32, cpu_hz: f64);
    /// Returns a timestamp to be passed to [Blep] to end the frame.
    ///
    /// # Panics
    /// Panics if the current frame execution didn't get to the near of end-of-frame.
    /// To check if you can actually call this method, invoke [FrameState::is_frame_over][crate::chip::FrameState::is_frame_over].
    fn get_audio_frame_end_time(&self) -> FTs;
    /// Calls [Blep::end_frame] to finalize the frame and prepare it for rendition.
    ///
    /// Returns a number of samples ready to be rendered in a single channel.
    ///
    /// # Panics
    /// Panics if the current frame execution didn't get to the near of end-of-frame.
    /// To check if you can actually call this method, invoke [FrameState::is_frame_over][crate::chip::FrameState::is_frame_over].
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

/*
Value output to bit: 4  3  |  Iss 2  Iss 3   Iss 2 V    Iss 3 V
                     1  1  |    1      1       3.79       3.70
                     1  0  |    1      1       3.66       3.56
                     0  1  |    1      0       0.73       0.66
                     0  0  |    0      0       0.39       0.34
*/
pub const AMPS_EAR_MIC: [f32; 4] = [0.34/3.70, 0.66/3.70, 3.56/3.70, 1.0];
pub const AMPS_EAR_OUT: [f32; 4] = [0.34/3.70, 0.34/3.70, 1.0, 1.0];
pub const AMPS_EAR_IN:  [f32; 2] = [0.34/3.70, 0.66/3.70];

pub const AMPS_EAR_MIC_I32: [i32; 4] = [0x0bc3_1d10, 0x16d5_1a60, 0x7b28_20ff, 0x7fff_ffff];
pub const AMPS_EAR_OUT_I32: [i32; 4] = [0x0bc3_1d10, 0x0bc3_1d10, 0x7fff_ffff, 0x7fff_ffff];
pub const AMPS_EAR_IN_I32:  [i32; 2] = [0x0bc3_1d10, 0x16d5_1a60];

pub const AMPS_EAR_MIC_I16: [i16; 4] = [0x0bc3, 0x16d5, 0x7b27, 0x7fff];
pub const AMPS_EAR_OUT_I16: [i16; 4] = [0x0bc3, 0x0bc3, 0x7fff, 0x7fff];
pub const AMPS_EAR_IN_I16:  [i16; 2] = [0x0bc3, 0x16d5];

/// Implements [AmpLevels] trait, useful when rendering combined EAR OUT and MIC OUT audio signal.
///
/// Uses 2 lowest bits of a given `level`.
#[derive(Clone, Default, Debug)]
pub struct EarMicAmps4<T>(PhantomData<T>);
/// Implements [AmpLevels] trait, useful when rendering EAR OUT audio ignoring MIC OUT signal.
///
/// Uses 2 lowest bits of a given `level`, but ignores the lowest bit.
#[derive(Clone, Default, Debug)]
pub struct EarOutAmps4<T>(PhantomData<T>);
/// Implements [AmpLevels] trait, useful when rendering EAR IN audio.
///
/// Uses only one bit of a given `level`.
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

impl<B: Blep> BlepStereo<B> {
    pub fn build(mono_filter: B::SampleDelta) -> impl FnOnce(B) -> Self {
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

impl<B: Blep + ?Sized> Blep for &mut B {
    type SampleDelta = B::SampleDelta;

    #[inline]
    fn ensure_frame_time(&mut self, sample_rate: u32, ts_rate: f64, frame_ts: FTs, margin_ts: FTs) {
        (*self).ensure_frame_time(sample_rate, ts_rate, frame_ts, margin_ts)
    }
    #[inline]
    fn end_frame(&mut self, timestamp: FTs) -> usize {
        (*self).end_frame(timestamp)
    }
    #[inline]
    fn add_step(&mut self, channel: usize, timestamp: FTs, delta: Self::SampleDelta) {
        (*self).add_step(channel, timestamp, delta)
    }
}

impl<B> Blep for BlepAmpFilter<B>
    where B: Blep, B::SampleDelta: MulNorm + SampleDelta
{
    type SampleDelta = B::SampleDelta;

    #[inline]
    fn ensure_frame_time(&mut self, sample_rate: u32, ts_rate: f64, frame_ts: FTs, margin_ts: FTs) {
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

    fn ensure_frame_time(&mut self, sample_rate: u32, ts_rate: f64, frame_ts: FTs, margin_ts: FTs) {
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

/// A helper method for rendering square-wave audio from slices containing updates of audio
/// digital levels, sorted by time encoded in [VideoTs] time stamps.
pub fn render_audio_frame_vts<VF,VL,L,A,T>(
            prev_state: u8,
            end_ts: Option<VFrameTs<VF>>,
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
        let vts: VFrameTs<_> = ts.into();
        if let Some(end_ts) = end_ts {
            if vts >= end_ts { // TODO >= or >
                break
            }
        }
        let next_vol = VL::amp_level(state.into());
        if let Some(delta) = last_vol.sample_delta(next_vol) {
            let timestamp = vts.into_tstates();
            blep.add_step(channel, timestamp, delta);
            last_vol = next_vol;
        }
    }
}

/// A helper method for rendering square-wave audio from slices containing updates of audio
/// digital levels, sorted by T-state counter value.
pub fn render_audio_frame_ts<VL,L,A,T>(
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
