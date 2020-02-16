// use core::marker::PhantomData;
use core::convert::TryInto;
use core::num::NonZeroU32;
use crate::audio::*;
use crate::clock::{Ts, VideoTs};
use crate::bus::BusDevice;
use crate::memory::ZxMemory;
use crate::video::VideoFrame;
use super::{Ula, UlaTsCounter, CPU_HZ};

impl<M, B, A> AudioFrame<A> for Ula<M, B>
    where M: ZxMemory, B: BusDevice<Timestamp=VideoTs>,
          A: Blep
{
    fn ensure_audio_frame_time(&self, blep: &mut A, sample_rate: u32) {
        blep.ensure_frame_time(sample_rate, CPU_HZ, Self::FRAME_TSTATES_COUNT, MARGIN_TSTATES)
    }

    #[inline]
    fn get_audio_frame_end_time(&self) -> FTs {
        UlaTsCounter::<M,B>::from(self.tsc).as_tstates()
    }
}

impl<M, B, A, L> EarMicOutAudioFrame<A> for Ula<M, B>
    where M: ZxMemory, B: BusDevice<Timestamp=VideoTs>,
          A: Blep<SampleDelta=L>,
          L: SampleDelta
{
    #[inline(always)]
    fn render_earmic_out_audio_frame<V: AmpLevels<L>>(&self, blep: &mut A, channel: usize) {
        render_audio_frame_vts::<Self,V,L,A,_>(self.prev_earmic,
                                         None,
                                         &self.earmic_out_changes,
                                         blep, channel)
    }
}

impl<M, B, A, L> EarInAudioFrame<A> for Ula<M, B>
    where M: ZxMemory, B: BusDevice<Timestamp=VideoTs>,
          A: Blep<SampleDelta=L>,
          L: SampleDelta
{
    #[inline(always)]
    fn render_ear_in_audio_frame<V: AmpLevels<L>>(&self, blep: &mut A, channel: usize) {
        render_audio_frame_vts::<Self,V,L,A,_>(self.prev_ear_in,
                                         Some(self.tsc),
                                         &self.ear_in_changes,
                                         blep, channel)
    }
}

impl<M, B> EarIn for Ula<M, B>
    where M: ZxMemory, B: BusDevice<Timestamp=VideoTs>
{
    fn set_ear_in(&mut self, ear_in: bool, delta_fts: u32) {
        if delta_fts == 0 {
            match self.ear_in_changes.last_mut() {
                Some(tscd) => tscd.set_data(ear_in.into()),
                None => { self.prev_earmic = ear_in.into() }
            }
        }
        else {
            let ts: VideoTs = self.ear_in_changes.last()
                                           .map(|&ts| ts.into())
                                           .unwrap_or_else(|| self.tsc);
            let mut vtsc = UlaTsCounter::<M,B>::from(ts);
            vtsc += delta_fts;
            self.ear_in_changes.push((vtsc.tsc, ear_in.into()).into());
        }
    }

    /// It's most optimal to be done after ensure_next_frame is called, but is not necessary.
    ///
    /// # Panics
    /// Panics if adding the delta would exceed the TsCounter max_value (Ts::max_value() as u32 * Self::HTS_COUNT as u32).
    fn feed_ear_in<I>(&mut self, fts_deltas: &mut I, max_frames_threshold: Option<usize>)
        where I: Iterator<Item=NonZeroU32>
    {
        let (ts, mut ear_in) = self.ear_in_changes.last()
                                       .map(|&ts| ts.into())
                                       .unwrap_or_else(||
                                            (self.tsc, self.prev_ear_in)
                                       );
        let mut vtsc = UlaTsCounter::<M,B>::from(ts);
        let max_vc = max_frames_threshold
                       .and_then(|mf| mf.checked_mul(Self::VSL_COUNT as usize))
                       .and_then(|mvc| mvc.checked_add(1))
                       .and_then(|mvc| mvc.try_into().ok())
                       .unwrap_or(Ts::max_value() - Self::VSL_COUNT - Ts::max_value()%Self::VSL_COUNT + 1);
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

impl<M, B> Ula<M, B>
    where M: ZxMemory, B: BusDevice<Timestamp=VideoTs>
{
    // pub const DEFAULT_VOLUME: f32 = 0.1;

    pub(super) fn cleanup_audio_frame_data(&mut self) {
        self.earmic_out_changes.clear();
        self.prev_earmic = self.last_earmic;
        self.prev_ear_in = self.read_ear_in(self.tsc);
        {
            let index = match self.ear_in_changes.get(self.ear_in_last_index) {
                Some(&tscd) if VideoTs::from(tscd) <= self.tsc => self.ear_in_last_index + 1,
                _ => self.ear_in_last_index
            };
            let num_elems = self.ear_in_changes.len() - index;
            self.ear_in_changes.copy_within(index.., 0);
            for p in self.ear_in_changes[0..num_elems].iter_mut() {
                p.vc -= Self::VSL_COUNT;
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
            _ => if self.last_earmic == 0 { 0 } else { 1 } // issue 2
        }
    }
}
