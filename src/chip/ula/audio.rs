/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
use crate::audio::*;

#[cfg(feature = "peripherals")]
use crate::peripherals::ay::audio::AyAudioFrame;
#[cfg(feature = "peripherals")]
use crate::peripherals::bus::ay::AyAudioBusDevice;
use crate::clock::{VFrameTs, FrameTimestamp};
use crate::bus::BusDevice;
use crate::video::VideoFrame;
use super::Ula;

#[cfg(feature = "peripherals")]
impl<B, M, D, X, V> AyAudioFrame<B> for Ula<M, D, X, V>
    where B: Blep,
          D: AyAudioBusDevice + BusDevice<Timestamp=VFrameTs<V>>,
          V: VideoFrame
{
    #[inline]
    fn render_ay_audio_frame<L: AmpLevels<B::SampleDelta>>(&mut self, blep: &mut B, chans: [usize; 3]) {
        self.bus.render_ay_audio::<L, B>(blep, self.tsc, V::FRAME_TSTATES_COUNT, chans)
    }
}

impl<A, M, B, X, V> AudioFrame<A> for Ula<M, B, X, V>
    where A: Blep,
          V: VideoFrame
{
    #[inline]
    fn ensure_audio_frame_time(&self, blep: &mut A, sample_rate: u32, cpu_hz: f64) {
        blep.ensure_frame_time(sample_rate, cpu_hz, V::FRAME_TSTATES_COUNT, MARGIN_TSTATES)
    }

    #[inline]
    fn get_audio_frame_end_time(&self) -> FTs {
        let vts = self.tsc;
        assert!(vts.is_eof(), "Ula::get_audio_frame_end_time: frame execution didn't finish yet");
        vts.into_tstates()
    }
}

impl<A, M, B, X, V> EarMicOutAudioFrame<A> for Ula<M, B, X, V>
    where A: Blep,
          V: VideoFrame
{
    #[inline(always)]
    fn render_earmic_out_audio_frame<L: AmpLevels<A::SampleDelta>>(&self, blep: &mut A, channel: usize) {
        render_audio_frame_vts::<V,L,A::SampleDelta,A,_>(
                                        self.prev_earmic_data.into(),
                                        None,
                                        &self.earmic_out_changes,
                                        blep, channel)
    }
}

impl<A, M, B, X, V> EarInAudioFrame<A> for Ula<M, B, X, V>
    where A: Blep,
          V: VideoFrame
{
    #[inline(always)]
    fn render_ear_in_audio_frame<L: AmpLevels<A::SampleDelta>>(&self, blep: &mut A, channel: usize) {
        render_audio_frame_vts::<V,L,A::SampleDelta,A,_>(
                                        self.prev_ear_in.into(),
                                        Some(self.tsc),
                                        &self.ear_in_changes,
                                        blep, channel)
    }
}
