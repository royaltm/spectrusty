/*
    Copyright (C) 2020-2023  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
//! Extensions to the hard sync.
use core::ops::DerefMut;
use core::ops::AddAssign;
use spectrusty_core::{
    clock::FTs,
    audio::{SampleDelta, Blep, FromSample, MulNorm}
};
use super::{BandLimited, BandLimOpt, 
            BandLimWide, BandLimLowBass, BandLimLowTreb, BandLimNarrow};

/// A trait interface for working with the [BandLimited] utility. Can be used as a dynamic trait.
pub trait BandLimitedExt<T, S>: Blep<SampleDelta=T> {
    /// See [BandLimited::reset].
    fn reset(&mut self);
    /// See [BandLimited::shrink_to_fit].
    fn shrink_to_fit(&mut self);
    /// See [BandLimited::is_frame_ended].
    fn is_frame_ended(&self) -> bool;
    /// See [BandLimited::next_frame].
    fn next_frame(&mut self);
    /// A helper method for rendering audio samples from a specific channel.
    ///
    /// The size of `output` should be acquired from calling [BandLimited::end_frame] or [Blep::end_frame].
    ///
    /// `channel` is the [BandLimited] source channel index (0-based).
    ///
    /// For more information see [BandLimited::sum_iter].
    fn render_audio_channel(&self, output: &mut [S], channel: usize)
        where S: FromSample<T>;
    /// A helper method for rendering audio samples from multiple channels into a channel-interleaved sample buffer.
    ///
    /// The size of `output` should be acquired from calling [BandLimited::end_frame] or [Blep::end_frame]
    /// and multiplied by the number of output audio channels: `output_nchannels`.
    ///
    /// `output_nchannels` must specifiy the number of output audio channels.
    ///
    /// `channel_map` maps the source [BandLimited] channels to output audio channels.
    /// The position in the slice represents each source channel index, starting from 0.
    /// Values in the slice indicate the output channel index (0-based).
    ///
    /// For more information see [BandLimited::sum_iter].
    fn render_audio_map_interleaved(&self, output: &mut [S], output_nchannels: usize, channel_map: &[usize])
        where S: FromSample<T>;
    /// A helper method for rendering audio samples from a specific channel into a channel-interleaved sample buffer.
    ///
    /// Each sample is being copied to each output channel (spread).
    ///
    /// The size of `output` should be acquired from calling [BandLimited::end_frame] or [Blep::end_frame]
    /// and multiplied by the number of output audio channels: `output_nchannels`.
    ///
    /// `output_nchannels` must specifiy the number of output audio channels.
    ///
    /// `channel` is the [BandLimited] source channel index (0-based).
    ///
    /// For more information see [BandLimited::sum_iter].
    fn render_audio_fill_interleaved(&self, output: &mut [S], output_nchannels: usize, channel: usize)
        where S: FromSample<T> + Copy;
}

/// An enum implementing [BandLimited] for all present band-pass filters.
#[derive(Debug)]
pub enum BandLimitedAny<T> {
    /// A variant with a wide pass filter.
    Wide(BandLimited<T, BandLimWide>),
    /// A variant with a low-frequency pass filter.
    LowTreb(BandLimited<T, BandLimLowTreb>),
    /// A variant with a high-frequency pass filter.
    LowBass(BandLimited<T, BandLimLowBass>),
    /// A variant with a narrow pass filter.
    Narrow(BandLimited<T, BandLimNarrow>)
}

impl<T> BandLimitedAny<T> {
    pub fn new(channels: usize, low_pass: bool, high_pass: bool) -> Self
        where T: Copy + Default + AddAssign + MulNorm + FromSample<f32>
    {
        match (high_pass, low_pass) {
            (false, false) => BandLimitedAny::Wide(BandLimited::new(channels)),
            (false,  true) => BandLimitedAny::LowTreb(BandLimited::new(channels)),
            (true,  false) => BandLimitedAny::LowBass(BandLimited::new(channels)),
            (true,   true) => BandLimitedAny::Narrow(BandLimited::new(channels)),
        }
    }
}

macro_rules! implement_any {
    ($me:ident, $blim:ident, $ex:expr) => {
        match $me {
            BandLimitedAny::Wide($blim) => $ex,
            BandLimitedAny::LowTreb($blim) => $ex,
            BandLimitedAny::LowBass($blim) => $ex,
            BandLimitedAny::Narrow($blim) => $ex,
        }
    };
}

impl<D, B, T, S> BandLimitedExt<T, S> for D
    where D: DerefMut<Target=B> + Blep<SampleDelta=T> + ?Sized,
          B: BandLimitedExt<T, S> + ?Sized
{
    #[inline]
    fn reset(&mut self) {
        (**self).reset()
    }
    #[inline]
    fn shrink_to_fit(&mut self) {
        (**self).shrink_to_fit()
    }
    #[inline]
    fn is_frame_ended(&self) -> bool {
        (**self).is_frame_ended()
    }
    #[inline]
    fn next_frame(&mut self) {
        (**self).next_frame()
    }
    #[inline]
    fn render_audio_channel(&self, output: &mut [S], channel: usize)
        where S: FromSample<T>
    {
        (**self).render_audio_channel(output, channel)
    }
    #[inline]
    fn render_audio_map_interleaved(&self, output: &mut [S], output_nchannels: usize, channel_map: &[usize])
        where S: FromSample<T>
    {
        (**self).render_audio_map_interleaved(output, output_nchannels, channel_map)
    }
    #[inline]
    fn render_audio_fill_interleaved(&self, output: &mut [S], output_nchannels: usize, channel: usize)
        where S: FromSample<T> + Copy
    {
        (**self).render_audio_fill_interleaved(output, output_nchannels, channel)
    }
}

impl<T, O, S> BandLimitedExt<T, S> for BandLimited<T, O>
    where T: SampleDelta + MulNorm + AddAssign + FromSample<f32>,
          O: BandLimOpt
{
    #[inline]
    fn reset(&mut self) {
        self.reset()
    }

    #[inline]
    fn shrink_to_fit(&mut self) {
        self.shrink_to_fit()
    }

    #[inline]
    fn is_frame_ended(&self) -> bool {
        self.is_frame_ended()
    }

    #[inline]
    fn next_frame(&mut self) {
        self.next_frame()
    }

    fn render_audio_channel(&self, output: &mut [S], channel: usize)
        where S: FromSample<T>
    {
        for (p, sample) in output.iter_mut()
                           .zip(self.sum_iter::<S>(channel))
        {
            *p = sample;
        }
    }

    fn render_audio_map_interleaved(&self, output: &mut [S], output_nchannels: usize, channel_map: &[usize])
        where S: FromSample<T>
    {
        for (nchan, ochan) in channel_map.into_iter().copied().enumerate() {
            for (p, sample) in output[ochan..].iter_mut()
                               .step_by(output_nchannels)
                               .zip(self.sum_iter::<S>(nchan))
            {
                *p = sample;
            }
        }
    }

    fn render_audio_fill_interleaved(&self, output: &mut [S], output_nchannels: usize, channel: usize)
        where S: FromSample<T> + Copy
    {
        for (chans, sample) in output.chunks_mut(output_nchannels)
                                     .zip(self.sum_iter::<S>(channel))
        {
            chans.fill(sample);
        }
    }
}

impl<T> Blep for BandLimitedAny<T>
where T: SampleDelta + MulNorm
{
    type SampleDelta = T;

    #[inline]
    fn ensure_frame_time(&mut self, sample_rate: u32, ts_rate: f64, frame_ts: FTs, margin_ts: FTs) {
        implement_any! { self, b, b.ensure_frame_time(sample_rate, ts_rate, frame_ts, margin_ts) }
    }

    #[inline]
    fn end_frame(&mut self, timestamp: FTs) -> usize {
        implement_any! { self, b, Blep::end_frame(b, timestamp) }
    }

    #[inline]
    fn add_step(&mut self, channel: usize, timestamp: FTs, delta: T) {
        implement_any! { self, b, b.add_step(channel, timestamp, delta) }
    }    
}

impl<T, S> BandLimitedExt<T, S> for BandLimitedAny<T>
    where T: SampleDelta + MulNorm + AddAssign + FromSample<f32>,
{
    #[inline]
    fn reset(&mut self) {
        implement_any! { self, b, b.reset() }
    }
    #[inline]
    fn shrink_to_fit(&mut self) {
        implement_any! { self, b, b.shrink_to_fit() }
    }
    #[inline]
    fn is_frame_ended(&self) -> bool {
        implement_any! { self, b, b.is_frame_ended() }
    }
    #[inline]
    fn next_frame(&mut self) {
        implement_any! { self, b, b.next_frame() }
    }
    #[inline]
    fn render_audio_channel(&self, output: &mut [S], channel: usize)
        where S: FromSample<T>
    {
        implement_any! { self, b, b.render_audio_channel(output, channel) }
    }
    #[inline]
    fn render_audio_map_interleaved(&self, output: &mut [S], output_nchannels: usize, channel_map: &[usize])
        where S: FromSample<T>
    {
        implement_any! { self, b, b.render_audio_map_interleaved(output, output_nchannels, channel_map) }
    }
    #[inline]
    fn render_audio_fill_interleaved(&self, output: &mut [S], output_nchannels: usize, channel: usize)
        where S: FromSample<T> + Copy
    {
        implement_any! { self, b, b.render_audio_fill_interleaved(output, output_nchannels, channel) }
    }
}
