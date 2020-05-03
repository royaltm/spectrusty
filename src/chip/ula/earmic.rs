use core::num::Wrapping;

use crate::chip::{EarIn, MicOut, EarMic, ReadEarMode};
use crate::clock::FTs;
use crate::video::VideoFrame;
use super::Ula;

use core::marker::PhantomData;
use core::convert::TryInto;
use core::num::NonZeroU32;

use crate::clock::{Ts, VideoTs, VideoTsData1, VideoTsData2};

impl<M, B, X, F> EarIn for Ula<M, B, X, F>
    where F: VideoFrame
{
    fn set_ear_in(&mut self, ear_in: bool, delta_fts: u32) {
        if delta_fts == 0 {
            match self.ear_in_changes.last_mut() {
                Some(tscd) => tscd.set_data(ear_in.into()),
                None => { self.prev_ear_in = ear_in }
            }
        }
        else {
            let vts: VideoTs = self.ear_in_changes.last()
                                           .map(|&ts| ts.into())
                                           .unwrap_or_else(|| self.tsc);
            let vts = F::vts_add_ts(vts, delta_fts);
            self.ear_in_changes.push((vts, ear_in.into()).into());
        }
    }

    /// It's most optimal to be done after ensure_next_frame is called, but is not necessary.
    ///
    /// # Panics
    /// Panics if adding the delta would exceed the TsCounter max_value (Ts::max_value() as u32 * F::HTS_COUNT as u32).
    fn feed_ear_in<I>(&mut self, fts_deltas: I, max_frames_threshold: Option<usize>)
        where I: Iterator<Item=NonZeroU32>
    {
        let (mut vts, mut ear_in) = self.ear_in_changes.last()
                                       .map(|&ts| ts.into())
                                       .unwrap_or_else(||
                                            (self.tsc, self.prev_ear_in.into())
                                       );
        let max_vc = max_frames_threshold
                       .and_then(|mf| mf.checked_mul(F::VSL_COUNT as usize))
                       .and_then(|mvc| mvc.checked_add(1))
                       .and_then(|mvc| mvc.try_into().ok())
                       .unwrap_or(Ts::max_value() - F::VSL_COUNT - Ts::max_value()%F::VSL_COUNT + 1);
        for dlt in fts_deltas {
            vts = F::vts_add_ts(vts, dlt.get());
            ear_in ^= 1;
            self.ear_in_changes.push((vts, ear_in).into());
            if vts.vc >= max_vc {
                break;
            }
        }
    }

    fn purge_ear_in_changes(&mut self, ear_in: bool) {
        self.ear_in_changes.clear();
        self.prev_ear_in = ear_in.into();
    }

    fn read_ear_in_count(&self) -> u32 {
        self.read_ear_in_count.0
    }

    fn get_read_ear_mode(&self) -> ReadEarMode {
        self.read_ear_mode
    }

    fn set_read_ear_mode(&mut self, mode: ReadEarMode) {
        self.read_ear_mode = mode;
    }
}

impl<'a, M: 'a, B: 'a, X: 'a, F: 'a> MicOut<'a> for Ula<M, B, X, F>
    where F: VideoFrame
{
    type PulseIter = MicPulseIter<core::slice::Iter<'a, VideoTsData2>, F>;
    fn mic_out_pulse_iter(&'a self) -> Self::PulseIter {
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
    fn new(last_pulse_ts: FTs, last_data: EarMic, iter: I) -> Self {
        let last_data = last_data.bits();
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
                if !(EarMic::from_bits_truncate(self.last_data ^ data) & EarMic::MIC).is_empty() {
                    let ts = V::vts_to_tstates(vts);
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

impl<M, B, X, F> Ula<M, B, X, F>
    where F: VideoFrame
{
    pub(super) fn cleanup_earmic_frame_data(&mut self) {
        // FIXME! (but how?)
        self.prev_earmic_ts = match self.earmic_out_changes.last() {
            Some(&vtsd) => {
                let vts = VideoTs::from(vtsd);
                F::vts_to_tstates(vts)
            }
            None => self.prev_earmic_ts
        }.saturating_sub(F::FRAME_TSTATES_COUNT);
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
                p.vc -= F::VSL_COUNT;
            }
            self.ear_in_changes.truncate(num_elems);
        }
        self.ear_in_last_index = 0;
        self.read_ear_in_count = Wrapping(0);
    }

    pub (super) fn read_ear_in(&mut self, ts: VideoTs) -> bool {
        self.read_ear_in_count += Wrapping(1);
        match self.ear_in_changes.get(self.ear_in_last_index..) {
            Some(changes) if !changes.is_empty() => {
                match ear_in_search_non_empty(changes, (ts, 1).into()) {
                    Some(index) => {
                        self.ear_in_last_index += index;
                        unsafe { changes.as_ptr().add(index).read().into_data() != 0 }
                    }
                    None => {
                        self.prev_ear_in
                    }
                }
            }
            _ => match self.read_ear_mode {
                ReadEarMode::Issue3 => !(self.last_earmic_data & EarMic::EAR).is_empty(),
                ReadEarMode::Issue2 => !(self.last_earmic_data & EarMic::EARMIC).is_empty(),
                ReadEarMode::Clear => false
            }
        }
    }
}

#[inline]
fn ear_in_search_non_empty(changes: &[VideoTsData1], item: VideoTsData1) -> Option<usize> {
    if changes.len() > 2 {
        if item < changes[0] {
            return None
        }
        else if item < changes[1] {
            return Some(0)
        }
    }
    match changes.binary_search(&item) {
        Err(0) => None,
        Ok(index) => Some(index),
        Err(index) => Some(index - 1)
    }
}
