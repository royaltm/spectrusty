use core::marker::PhantomData;
use core::convert::TryInto;
use core::num::NonZeroU32;
use crate::audio::*;
use crate::audio::ay::AyAudioFrame;
use crate::bus::ay::AyAudioVBusDevice;
use crate::clock::{Ts, VideoTs, VideoTsData2, VFrameTsCounter};
use crate::memory::ZxMemory;
use crate::video::{Video, VideoFrame};
use super::{Ula, UlaTsCounter, UlaVideoFrame, CPU_HZ};

impl<E, M, D> AyAudioFrame<E> for Ula<M, D>
    where E: Blep,
          D: AyAudioVBusDevice
          
{
    #[inline]
    fn render_ay_audio_frame<V: AmpLevels<E::SampleDelta>>(&mut self, blep: &mut E, chans: [usize; 3]) {
        self.bus.render_ay_audio_vts::<V, UlaVideoFrame, E>(blep, self.tsc, chans)
    }
}

impl<M, B, A> AudioFrame<A> for Ula<M, B>
    where A: Blep
{
    #[inline]
    fn ensure_audio_frame_time(&self, blep: &mut A, sample_rate: u32) {
        blep.ensure_frame_time(sample_rate, CPU_HZ, UlaVideoFrame::FRAME_TSTATES_COUNT, MARGIN_TSTATES)
    }

    #[inline]
    fn get_audio_frame_end_time(&self) -> FTs {
        let ts = UlaTsCounter::from(self.tsc).as_tstates();
        debug_assert!(ts >= UlaVideoFrame::FRAME_TSTATES_COUNT);
        ts
    }
}

impl<M, B, A> EarMicOutAudioFrame<A> for Ula<M, B>
    where A: Blep
{
    #[inline(always)]
    fn render_earmic_out_audio_frame<V: AmpLevels<A::SampleDelta>>(&self, blep: &mut A, channel: usize) {
        render_audio_frame_vts::<UlaVideoFrame,V,A::SampleDelta,A,_>(self.prev_earmic_data,
                                         None,
                                         &self.earmic_out_changes,
                                         blep, channel)
    }
}

impl<M, B, A> EarInAudioFrame<A> for Ula<M, B>
    where A: Blep
{
    #[inline(always)]
    fn render_ear_in_audio_frame<V: AmpLevels<A::SampleDelta>>(&self, blep: &mut A, channel: usize) {
        render_audio_frame_vts::<UlaVideoFrame,V,A::SampleDelta,A,_>(self.prev_ear_in,
                                         Some(self.tsc),
                                         &self.ear_in_changes,
                                         blep, channel)
    }
}

impl<M, B> EarIn for Ula<M, B>
{
    fn set_ear_in(&mut self, ear_in: bool, delta_fts: u32) {
        if delta_fts == 0 {
            match self.ear_in_changes.last_mut() {
                Some(tscd) => tscd.set_data(ear_in.into()),
                None => { self.prev_earmic_data = ear_in.into() }
            }
        }
        else {
            let ts: VideoTs = self.ear_in_changes.last()
                                           .map(|&ts| ts.into())
                                           .unwrap_or_else(|| self.tsc);
            let mut vtsc = UlaTsCounter::from(ts);
            vtsc += delta_fts;
            self.ear_in_changes.push((vtsc.tsc, ear_in.into()).into());
        }
    }

    /// It's most optimal to be done after ensure_next_frame is called, but is not necessary.
    ///
    /// # Panics
    /// Panics if adding the delta would exceed the TsCounter max_value (Ts::max_value() as u32 * UlaVideoFrame::HTS_COUNT as u32).
    fn feed_ear_in<I>(&mut self, fts_deltas: &mut I, max_frames_threshold: Option<usize>)
        where I: Iterator<Item=NonZeroU32>
    {
        let (ts, mut ear_in) = self.ear_in_changes.last()
                                       .map(|&ts| ts.into())
                                       .unwrap_or_else(||
                                            (self.tsc, self.prev_ear_in)
                                       );
        let mut vtsc = UlaTsCounter::from(ts);
        let max_vc = max_frames_threshold
                       .and_then(|mf| mf.checked_mul(UlaVideoFrame::VSL_COUNT as usize))
                       .and_then(|mvc| mvc.checked_add(1))
                       .and_then(|mvc| mvc.try_into().ok())
                       .unwrap_or(Ts::max_value() - UlaVideoFrame::VSL_COUNT - Ts::max_value()%UlaVideoFrame::VSL_COUNT + 1);
        for dlt in fts_deltas {
            vtsc += dlt.get();
            ear_in ^= 1;
            self.ear_in_changes.push((vtsc.tsc, ear_in).into());
            if vtsc.vc >= max_vc {
                break;
            }
        }
    }

    fn purge_ear_in_changes(&mut self, ear_in: bool) {
        self.ear_in_changes.clear();
        self.prev_ear_in = ear_in.into();
    }
}

/// A trait for reading `MIC` output.
impl<'a, M: 'a, B: 'a> MicOut<'a> for Ula<M, B>
{
    type PulseIter = MicPulseIter<core::slice::Iter<'a, VideoTsData2>, UlaVideoFrame>;
    /// Returns a frame buffered mic output as a pulse iterator.
    fn mic_out_iter_pulses(&'a self) -> Self::PulseIter {
        MicPulseIter::new(
                self.prev_earmic_ts,
                self.prev_earmic_data,
                self.earmic_out_changes.iter())
    }
}

/// `MIC out` pulse iterator.
pub struct MicPulseIter<I, V>
{
    iter: I,
    last_pulse_ts: FTs,
    last_data: u8,
    _video: PhantomData<V>
}

impl<'a, I, V> MicPulseIter<I, V>
    where I: Iterator<Item=&'a VideoTsData2>,
          V: VideoFrame
{
    fn new(last_pulse_ts: FTs, last_data: u8, iter: I) -> Self {
        MicPulseIter { last_pulse_ts, last_data, iter, _video: PhantomData }
    }
}

impl<'a, I, V> Iterator for MicPulseIter<I, V>
    where I: Iterator<Item=&'a VideoTsData2>,
          V: VideoFrame
{
    type Item = NonZeroU32;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(&vtsd) = self.iter.next() {
                let (vts, data):(VideoTs, u8) = vtsd.into();
                if (self.last_data ^ data) & 1 == 1 {
                    let ts = VFrameTsCounter::<V>::from(vts).as_tstates();
                    let maybe_delta = ts.checked_sub(self.last_pulse_ts);

                    self.last_pulse_ts = ts;
                    self.last_data = data;
                    let pulse = match maybe_delta {
                        Some(delta) => {
                            let pulse = NonZeroU32::new(
                                            delta.try_into()
                                                 .expect("mic out timestamps must always ascend"));
                            if pulse.is_none() {
                                panic!("mic out timestamp intervals must not be 0");
                            }
                            pulse
                        }
                        None => NonZeroU32::new(u32::max_value())
                    };
                    break pulse
                }
                else {
                    continue
                }
            }
            break None
        }
    }
}

impl<M, B> Ula<M, B> {
    pub(super) fn cleanup_audio_frame_data(&mut self) {
        // FIXME! (but how?)
        self.prev_earmic_ts = match self.earmic_out_changes.last() {
            Some(&vtsd) => {
                let vts = VideoTs::from(vtsd);
                UlaTsCounter::from(vts).as_tstates()
            }
            None => self.prev_earmic_ts
        }.saturating_sub(UlaVideoFrame::FRAME_TSTATES_COUNT);
        self.earmic_out_changes.clear();
        self.prev_earmic_data = self.last_earmic_data;
        self.prev_ear_in = self.read_ear_in(self.tsc);
        {
            let index = match self.ear_in_changes.get(self.ear_in_last_index) {
                Some(&tscd) if VideoTs::from(tscd) <= self.tsc => self.ear_in_last_index + 1,
                _ => self.ear_in_last_index
            };
            let num_elems = self.ear_in_changes.len() - index;
            self.ear_in_changes.copy_within(index.., 0);
            for p in self.ear_in_changes[0..num_elems].iter_mut() {
                p.vc -= UlaVideoFrame::VSL_COUNT;
            }
            self.ear_in_changes.truncate(num_elems);
        }
        self.ear_in_last_index = 0;
    }

    pub (super) fn read_ear_in(&mut self, ts: VideoTs) -> u8 {
        match self.ear_in_changes.get(self.ear_in_last_index..) {
            Some(changes) if !changes.is_empty() => {
                let maybe_index = match changes.binary_search(&(ts, 1).into()) {
                    Err(0) => None,
                    Ok(index) => Some(index),
                    Err(index) => Some(index - 1)
                };
                match maybe_index {
                    Some(index) => {
                        self.ear_in_last_index += index;
                        unsafe { changes.as_ptr().add(index).read().into_data() }
                    }
                    None => self.prev_ear_in
                }
            }
            _ => if self.last_earmic_data == 0 { 0 } else { 1 } // issue 2
        }
    }
}
