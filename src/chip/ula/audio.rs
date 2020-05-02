use crate::audio::*;

#[cfg(feature = "peripherals")]
use crate::peripherals::ay::audio::AyAudioFrame;
#[cfg(feature = "peripherals")]
use crate::peripherals::bus::ay::AyAudioVBusDevice;

use crate::video::VideoFrame;
use super::{CPU_HZ, Ula};

#[cfg(feature = "peripherals")]
impl<A, M, D, X, F> AyAudioFrame<A> for Ula<M, D, X, F>
    where A: Blep,
          D: AyAudioVBusDevice,
          F: VideoFrame
{
    #[inline]
    fn render_ay_audio_frame<V: AmpLevels<A::SampleDelta>>(&mut self, blep: &mut A, chans: [usize; 3]) {
        self.bus.render_ay_audio_vts::<V, F, A>(blep, self.tsc, chans)
    }
}

impl<A, M, B, X, F> AudioFrame<A> for Ula<M, B, X, F>
    where A: Blep,
          F: VideoFrame
{
    #[inline]
    fn ensure_audio_frame_time(&self, blep: &mut A, sample_rate: u32) {
        blep.ensure_frame_time(sample_rate, CPU_HZ, F::FRAME_TSTATES_COUNT, MARGIN_TSTATES)
    }

    #[inline]
    fn get_audio_frame_end_time(&self) -> FTs {
        let vts = self.tsc;
        assert!(F::is_vts_eof(vts), "Ula::get_audio_frame_end_time: frame execution didn't finish yet");
        F::vts_to_tstates(vts)
    }
}

impl<A, M, B, X, F> EarMicOutAudioFrame<A> for Ula<M, B, X, F>
    where A: Blep,
          F: VideoFrame
{
    #[inline(always)]
    fn render_earmic_out_audio_frame<V: AmpLevels<A::SampleDelta>>(&self, blep: &mut A, channel: usize) {
        render_audio_frame_vts::<F,V,A::SampleDelta,A,_>(
                                        self.prev_earmic_data.into(),
                                        None,
                                        &self.earmic_out_changes,
                                        blep, channel)
    }
}

impl<A, M, B, X, F> EarInAudioFrame<A> for Ula<M, B, X, F>
    where A: Blep,
          F: VideoFrame
{
    #[inline(always)]
    fn render_ear_in_audio_frame<V: AmpLevels<A::SampleDelta>>(&self, blep: &mut A, channel: usize) {
        render_audio_frame_vts::<F,V,A::SampleDelta,A,_>(
                                        self.prev_ear_in.into(),
                                        Some(self.tsc),
                                        &self.ear_in_changes,
                                        blep, channel)
    }
}
