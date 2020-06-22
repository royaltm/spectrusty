use core::num::NonZeroU32;
use crate::audio::*;
#[cfg(feature = "peripherals")]
use crate::peripherals::ay::audio::AyAudioFrame;
#[cfg(feature = "peripherals")]
use crate::peripherals::bus::ay::AyAudioBusDevice;
use crate::clock::VFrameTs;
use crate::bus::BusDevice;
use crate::chip::{EarIn, MicOut, ReadEarMode};
use super::{Ula3, InnerUla, Ula3VidFrame};

#[cfg(feature = "peripherals")]
impl<B, D, X> AyAudioFrame<B> for Ula3<D, X>
    where B: Blep,
          D: AyAudioBusDevice + BusDevice<Timestamp=VFrameTs<Ula3VidFrame>>
{
    #[inline]
    fn render_ay_audio_frame<L: AmpLevels<B::SampleDelta>>(&mut self, blep: &mut B, chans: [usize; 3]) {
        self.ula.render_ay_audio_frame::<L>(blep, chans)
    }
}

impl<B, D, X> AudioFrame<B> for Ula3<D, X>
    where B: Blep,
          InnerUla<D, X>: AudioFrame<B>
{
    #[inline]
    fn ensure_audio_frame_time(&self, blep: &mut B, sample_rate: u32, cpu_hz: f64) {
        self.ula.ensure_audio_frame_time(blep, sample_rate, cpu_hz)
    }

    #[inline]
    fn get_audio_frame_end_time(&self) -> FTs {
        self.ula.get_audio_frame_end_time()
    }
}

impl<B, D, X> EarMicOutAudioFrame<B> for Ula3<D, X>
    where B: Blep
{
    #[inline(always)]
    fn render_earmic_out_audio_frame<L: AmpLevels<B::SampleDelta>>(&self, blep: &mut B, channel: usize) {
        self.ula.render_earmic_out_audio_frame::<L>(blep, channel)
    }
}

impl<B, D, X> EarInAudioFrame<B> for Ula3<D, X>
    where B: Blep
{
    #[inline(always)]
    fn render_ear_in_audio_frame<L: AmpLevels<B::SampleDelta>>(&self, blep: &mut B, channel: usize) {
        self.ula.render_ear_in_audio_frame::<L>(blep, channel)
    }
}

impl<D, X> EarIn for Ula3<D, X> {
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

impl<'a, D: 'a, X: 'a> MicOut<'a> for Ula3<D, X> {
    type PulseIter = <InnerUla<D, X> as MicOut<'a>>::PulseIter;
    fn mic_out_pulse_iter(&'a self) -> Self::PulseIter {
        self.ula.mic_out_pulse_iter()
    }
}
