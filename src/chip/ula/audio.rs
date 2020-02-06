use core::marker::PhantomData;
use core::convert::TryInto;
use core::num::NonZeroU32;
use crate::audio::*;
use crate::clock::{Ts, VideoTs};
use crate::bus::BusDevice;
use crate::memory::ZxMemory;
use crate::video::VideoFrame;
use super::{Ula, UlaTsCounter, CPU_HZ};

// TODO put this on some trait
// const TSTATES_FRAME: f64 = 69888.0;
// const FRAME_TIME: f64 = TSTATES_FRAME / CPU_HZ;
/*
Value output to bit: 4  3  |  Iss 2  Iss 3   Iss 2 V    Iss 3 V
                     1  1  |    1      1       3.79       3.70
                     1  0  |    1      1       3.66       3.56
                     0  1  |    1      0       0.73       0.66
                     0  0  |    0      0       0.39       0.34
*/
pub const AMPS_EAR_MIC: [f32; 4] = [0.34/3.70, 0.66/3.70, 3.56/3.70, 3.70/3.70];
pub const AMPS_EAR_OUT: [f32; 4] = [0.34/3.70, 0.34/3.70, 3.70/3.70, 3.70/3.70];
pub const AMPS_EAR_IN:  [f32; 2] = [0.34/3.70, 3.70/3.70];

pub const AMPS_EAR_MIC_I32: [i32; 4] = [0x0bc31d10, 0x16d51a60, 0x7b2820ff, 0x7fffffff];
pub const AMPS_EAR_OUT_I32: [i32; 4] = [0x0bc31d10, 0x0bc31d10, 0x7fffffff, 0x7fffffff];
pub const AMPS_EAR_IN_I32:  [i32; 2] = [0x0bc31d10, 0x7fffffff];

pub const AMPS_EAR_MIC_I16: [i16; 4] = [0x0bc3, 0x16d5, 0x7b27, 0x7fff];
pub const AMPS_EAR_OUT_I16: [i16; 4] = [0x0bc3, 0x0bc3, 0x7fff, 0x7fff];
pub const AMPS_EAR_IN_I16:  [i16; 2] = [0x0bc3, 0x7fff];

pub struct EarMicAmps4<T>(PhantomData<T>);
pub struct EarOutAmps4<T>(PhantomData<T>);
pub struct EarInAmps2<T>(PhantomData<T>);

macro_rules! impl_amp_levels {
    ($([$ty:ty, $ear_mic:ident, $ear_out:ident, $ear_in:ident]),*) => { $(
        impl AmpLevels<$ty> for EarMicAmps4<$ty> {
            #[inline(always)]
            fn amp_level(level: u32) -> $ty {
                $ear_mic[(level & 3) as usize]
            }
        }

        impl AmpLevels<$ty> for EarOutAmps4<$ty> {
            #[inline(always)]
            fn amp_level(level: u32) -> $ty {
                $ear_out[(level & 3) as usize]
            }
        }

        impl AmpLevels<$ty> for EarInAmps2<$ty> {
            #[inline(always)]
            fn amp_level(level: u32) -> $ty {
                $ear_in[(level & 1) as usize]
            }
        }
    )* };
}
impl_amp_levels!([f32, AMPS_EAR_MIC, AMPS_EAR_OUT, AMPS_EAR_IN],
                 [i32, AMPS_EAR_MIC_I32, AMPS_EAR_OUT_I32, AMPS_EAR_IN_I32],
                 [i16, AMPS_EAR_MIC_I16, AMPS_EAR_OUT_I16, AMPS_EAR_IN_I16]);

impl<M, B, A, FT> AudioFrame<A> for Ula<M, B>
where M: ZxMemory, B: BusDevice<Timestamp=VideoTs>,
      A: Blep<SampleTime=FT>, FT: SampleTime
{
    fn ensure_audio_frame_time(blep: &mut A, sample_rate: u32) -> FT {
        let time_rate = FT::time_rate(sample_rate, CPU_HZ);
        blep.ensure_frame_time(time_rate.at_timestamp(Self::FRAME_TSTATES_COUNT),
                               time_rate.at_timestamp(MARGIN_TSTATES));
        time_rate
    }

    #[inline]
    fn get_audio_frame_end_time(&self, time_rate: FT) -> FT {
        time_rate.at_timestamp(UlaTsCounter::<M,B>::from(self.tsc).as_tstates())
    }
}

impl<M, B, A, L, FT> EarMicAudioFrame<A> for Ula<M, B>
where M: ZxMemory, B: BusDevice<Timestamp=VideoTs>,
      A: Blep<SampleDelta=L, SampleTime=FT>,
      L: Copy + SampleDelta,
      FT: SampleTime
{
    #[inline(always)]
    fn render_earmic_out_audio_frame<V: AmpLevels<L>>(&self, blep: &mut A, time_rate: FT, channel: usize) {
        render_audio_frame::<Self,V,L,A,FT,_>(self.prev_earmic,
                                         None,
                                         &self.earmic_out_changes,
                                         blep, time_rate, channel)
    }

    #[inline(always)]
    fn render_ear_in_audio_frame<V: AmpLevels<L>>(&self, blep: &mut A, time_rate: FT, channel: usize) {
        render_audio_frame::<Self,V,L,A,FT,_>(self.prev_ear_in,
                                         Some(self.tsc),
                                         &self.ear_in_changes,
                                         blep, time_rate, channel)
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

impl<M, B> Ula<M, B> where M: ZxMemory, B: BusDevice<Timestamp=VideoTs>
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
            Some(changes) if changes.len() != 0 => {
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
