/*
    Copyright (C) 2020-2022  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
use core::num::NonZeroU32;
use crate::audio::*;
#[cfg(feature = "peripherals")]
use crate::peripherals::ay::audio::AyAudioFrame;
#[cfg(feature = "peripherals")]
use crate::peripherals::bus::ay::AyAudioBusDevice;
use crate::bus::BusDevice;
use crate::clock::VFrameTs;
use crate::memory::PagedMemory8k;
use crate::chip::{
    EarIn, MicOut, ReadEarMode,
    ula::Ula
};
use crate::video::VideoFrame;
use super::Scld;

#[cfg(feature = "peripherals")]
impl<A, M, B, X, V> AyAudioFrame<A> for Scld<M, B, X, V>
    where A: Blep,
          M: PagedMemory8k,
          B: AyAudioBusDevice + BusDevice,
          B::Timestamp: From<VFrameTs<V>>,
          V: VideoFrame
{
    #[inline]
    fn render_ay_audio_frame<L: AmpLevels<A::SampleDelta>>(&mut self, blep: &mut A, chans: [usize; 3]) {
        self.ula.render_ay_audio_frame::<L>(blep, chans)
    }
}

impl<A, M, B, X, V> AudioFrame<A> for Scld<M, B, X, V>
    where A: Blep,
          M: PagedMemory8k,
          Ula<M, B, X, V>: AudioFrame<A>
{
    #[inline]
    fn ensure_audio_frame_time(&self, blep: &mut A, sample_rate: u32, cpu_hz: f64) {
        self.ula.ensure_audio_frame_time(blep, sample_rate, cpu_hz)
    }

    #[inline]
    fn get_audio_frame_end_time(&self) -> FTs {
        self.ula.get_audio_frame_end_time()
    }
}

impl<A, M, B, X, F> EarMicOutAudioFrame<A> for Scld<M, B, X, F>
    where A: Blep,
          M: PagedMemory8k,
          F: VideoFrame
{
    #[inline(always)]
    fn render_earmic_out_audio_frame<V: AmpLevels<A::SampleDelta>>(&self, blep: &mut A, channel: usize) {
        self.ula.render_earmic_out_audio_frame::<V>(blep, channel)
    }
}

impl<A, M, B, X, F> EarInAudioFrame<A> for Scld<M, B, X, F>
    where A: Blep,
          M: PagedMemory8k,
          F: VideoFrame
{
    #[inline(always)]
    fn render_ear_in_audio_frame<V: AmpLevels<A::SampleDelta>>(&self, blep: &mut A, channel: usize) {
        self.ula.render_ear_in_audio_frame::<V>(blep, channel)
    }
}

impl<M, B, X, V> EarIn for Scld<M, B, X, V>
    where M: PagedMemory8k,
          V: VideoFrame
{
    fn set_ear_in(&mut self, ear_in: bool, delta_fts: u32) {
        self.ula.set_ear_in(ear_in, delta_fts)
    }

    fn feed_ear_in<I>(&mut self, fts_deltas: I, max_frames_threshold: Option<usize>)
        where I: Iterator<Item=NonZeroU32>
    {
        self.ula.feed_ear_in(fts_deltas, max_frames_threshold)
    }

    fn purge_ear_in_changes(&mut self, ear_in: bool) {
        self.ula.purge_ear_in_changes(ear_in)
    }

    fn read_ear_in_count(&self) -> u32 {
        self.ula.read_ear_in_count()
    }

    fn read_ear_mode(&self) -> ReadEarMode {
        self.ula.read_ear_mode()
    }

    fn set_read_ear_mode(&mut self, mode: ReadEarMode) {
        self.ula.set_read_ear_mode(mode)
    }
}

impl<'a, M: 'a, B: 'a, X: 'a, V: 'a> MicOut<'a> for Scld<M, B, X, V>
    where M: PagedMemory8k,
          V: VideoFrame
{
    type PulseIter = <Ula<M, B, X, V> as MicOut<'a>>::PulseIter;
    fn mic_out_pulse_iter(&'a self) -> Self::PulseIter {
        self.ula.mic_out_pulse_iter()
    }
}
