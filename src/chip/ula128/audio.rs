use core::marker::PhantomData;
use core::convert::TryInto;
use core::num::NonZeroU32;
use crate::audio::*;
use crate::audio::ay::AyAudioFrame;
use crate::bus::ay::AyAudioVBusDevice;
use crate::chip::ControlUnit;
use crate::clock::{Ts, VideoTs, VideoTsData2, VFrameTsCounter};
use crate::memory::ZxMemory;
use crate::video::{Video, VideoFrame};
use super::{CPU_HZ, Ula128, Ula128VidFrame, InnerUla};

impl<A, B> AyAudioFrame<A> for Ula128<B>
    where A: Blep,
          B: AyAudioVBusDevice
{
    #[inline]
    fn render_ay_audio_frame<V: AmpLevels<A::SampleDelta>>(&mut self, blep: &mut A, chans: [usize; 3]) {
        self.ula.render_ay_audio_frame::<V>(blep, chans)
    }
}

impl<A, B> AudioFrame<A> for Ula128<B>
    where A: Blep,
          InnerUla<B>: AudioFrame<A>
{
    #[inline]
    fn ensure_audio_frame_time(&self, blep: &mut A, sample_rate: u32) {
        blep.ensure_frame_time(sample_rate, CPU_HZ, Ula128VidFrame::FRAME_TSTATES_COUNT, MARGIN_TSTATES)
    }

    #[inline]
    fn get_audio_frame_end_time(&self) -> FTs {
        self.ula.get_audio_frame_end_time()
    }
}

impl<A, B> EarMicOutAudioFrame<A> for Ula128<B>
    where A: Blep
{
    #[inline(always)]
    fn render_earmic_out_audio_frame<V: AmpLevels<A::SampleDelta>>(&self, blep: &mut A, channel: usize) {
        self.ula.render_earmic_out_audio_frame::<V>(blep, channel)
    }
}

impl<A, B> EarInAudioFrame<A> for Ula128<B>
    where A: Blep
{
    #[inline(always)]
    fn render_ear_in_audio_frame<V: AmpLevels<A::SampleDelta>>(&self, blep: &mut A, channel: usize) {
        self.ula.render_ear_in_audio_frame::<V>(blep, channel)
    }
}

impl<B> EarIn for Ula128<B> {
    fn set_ear_in(&mut self, ear_in: bool, delta_fts: u32) {
        self.ula.set_ear_in(ear_in, delta_fts)
    }

    fn feed_ear_in<I>(&mut self, fts_deltas: &mut I, max_frames_threshold: Option<usize>)
        where I: Iterator<Item=NonZeroU32>
    {
        self.ula.feed_ear_in(fts_deltas, max_frames_threshold)
    }

    fn purge_ear_in_changes(&mut self, ear_in: bool) {
        self.ula.purge_ear_in_changes(ear_in)
    }
}

/// A trait for reading `MIC` output.
impl<'a, B: 'a> MicOut<'a> for Ula128<B> {
    type PulseIter = <InnerUla<B> as MicOut<'a>>::PulseIter;
    /// Returns a frame buffered mic output as a pulse iterator.
    fn mic_out_iter_pulses(&'a self) -> Self::PulseIter {
        self.ula.mic_out_iter_pulses()
    }
}

