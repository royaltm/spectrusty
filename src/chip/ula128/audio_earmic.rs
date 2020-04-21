use core::num::NonZeroU32;
use crate::audio::*;
#[cfg(feature = "peripherals")]
use crate::peripherals::ay::audio::AyAudioFrame;
#[cfg(feature = "peripherals")]
use crate::peripherals::bus::ay::AyAudioVBusDevice;
use crate::video::VideoFrame;

use crate::chip::{EarIn, MicOut};
use super::{CPU_HZ, Ula128, Ula128VidFrame, InnerUla};

impl<A, B, X> AyAudioFrame<A> for Ula128<B, X>
    where A: Blep,
          B: AyAudioVBusDevice
{
    #[inline]
    fn render_ay_audio_frame<V: AmpLevels<A::SampleDelta>>(&mut self, blep: &mut A, chans: [usize; 3]) {
        self.ula.render_ay_audio_frame::<V>(blep, chans)
    }
}

impl<A, B, X> AudioFrame<A> for Ula128<B, X>
    where A: Blep,
          InnerUla<B, X>: AudioFrame<A>
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

impl<A, B, X> EarMicOutAudioFrame<A> for Ula128<B, X>
    where A: Blep
{
    #[inline(always)]
    fn render_earmic_out_audio_frame<V: AmpLevels<A::SampleDelta>>(&self, blep: &mut A, channel: usize) {
        self.ula.render_earmic_out_audio_frame::<V>(blep, channel)
    }
}

impl<A, B, X> EarInAudioFrame<A> for Ula128<B, X>
    where A: Blep
{
    #[inline(always)]
    fn render_ear_in_audio_frame<V: AmpLevels<A::SampleDelta>>(&self, blep: &mut A, channel: usize) {
        self.ula.render_ear_in_audio_frame::<V>(blep, channel)
    }
}

impl<B, X> EarIn for Ula128<B, X> {
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
}

impl<'a, B: 'a, X: 'a> MicOut<'a> for Ula128<B, X> {
    type PulseIter = <InnerUla<B, X> as MicOut<'a>>::PulseIter;
    /// Returns a frame buffered mic output as a pulse iterator.
    fn mic_out_pulse_iter(&'a self) -> Self::PulseIter {
        self.ula.mic_out_pulse_iter()
    }
}
