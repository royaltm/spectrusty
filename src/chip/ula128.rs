mod audio;
pub(crate) mod frame_cache;
mod io;
mod video;

use crate::z80emu::{*, host::Result};
#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};

use crate::bus::{BusDevice, NullDevice};
use crate::chip::{
    ControlUnit, MemoryAccess, nanos_from_frame_tc_cpu_hz,
    ula::{
        frame_cache::UlaFrameCache,
        Ula, UlaTimestamp, UlaCpuExt, UlaMemoryContention
    }
};
use crate::video::VideoFrame;
use crate::memory::{Memory128k, MemoryExtension, NoMemoryExtension};
use crate::clock::{VideoTs, FTs, VFrameTsCounter, MemoryContention};

pub use video::{Ula128VidFrame, Ula128MemContention};

/// The ZX Spectrum 128k CPU clock in cycles per second.
pub const CPU_HZ: u32 = 3_546_900;

pub(self) type InnerUla<B, X> = Ula<Memory128k, B, X, Ula128VidFrame>;

/// ZX Spectrum 128k ULA.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "snapshot", serde(rename_all = "camelCase"))]
pub struct Ula128<B=NullDevice<VideoTs>, X=NoMemoryExtension> {
    ula: InnerUla<B, X>,
    mem_page3_bank: u8,
    cur_screen_shadow: bool, // current shadow screen
    beg_screen_shadow: bool, // shadow screen when a frame began
    mem_locked: bool,
    #[cfg_attr(feature = "snapshot", serde(skip))]
    shadow_frame_cache: UlaFrameCache<Ula128VidFrame>,
    #[cfg_attr(feature = "snapshot", serde(skip))]
    screen_changes: Vec<VideoTs>,
}

impl<B: Default, X: Default> Default for Ula128<B, X> {
    fn default() -> Self {
        Ula128 {
            ula: Default::default(),
            mem_page3_bank: 0,
            cur_screen_shadow: false,
            beg_screen_shadow: false,
            mem_locked: false,
            shadow_frame_cache: Default::default(),
            screen_changes: Vec::new()
        }
    }
}

impl<B, X> Ula128<B, X> {
    #[inline(always)]
    fn is_page3_contended(&self) -> bool {
        self.mem_page3_bank & 1 == 1
    }

    #[inline(always)]
    fn page3_screen_bank(&self) -> Option<u8> {
        match self.mem_page3_bank {
            5 => Some(0),
            7 => Some(1),
            _ => None
        }
    }
}

impl<B, X> MemoryAccess for Ula128<B, X>
    where X: MemoryExtension
{
    type Memory = Memory128k;
    type MemoryExt = X;

    #[inline(always)]
    fn memory_ext_ref(&self) -> &Self::MemoryExt {
        &self.ula.memext
    }
    #[inline(always)]
    fn memory_ext_mut(&mut self) -> &mut Self::MemoryExt {
        &mut self.ula.memext
    }
    #[inline(always)]
    fn memory_mut(&mut self) -> &mut Self::Memory {
        &mut self.ula.memory
    }
    #[inline(always)]
    fn memory_ref(&self) -> &Self::Memory {
        &self.ula.memory
    }
}

impl<B, X> ControlUnit for Ula128<B, X>
    where B: BusDevice<Timestamp=VideoTs>, X: MemoryExtension
{
    type BusDevice = B;

    fn cpu_clock_rate(&self) -> u32 {
        CPU_HZ
    }

    fn frame_duration_nanos(&self) -> u32 {
        nanos_from_frame_tc_cpu_hz(Ula128VidFrame::FRAME_TSTATES_COUNT as u32, CPU_HZ) as u32
    }

    fn bus_device_mut(&mut self) -> &mut Self::BusDevice {
        &mut self.ula.bus
    }

    fn bus_device_ref(&self) -> &Self::BusDevice {
        &self.ula.bus
    }

    fn current_frame(&self) -> u64 {
        self.ula.frames.0
    }

    fn frame_tstate(&self) -> (u64, FTs) {
        Ula128VidFrame::vts_to_norm_tstates(self.ula.tsc, self.current_frame())
    }

    fn current_tstate(&self) -> FTs {
        Ula128VidFrame::vts_to_tstates(self.ula.tsc)
    }

    fn is_frame_over(&self) -> bool {
        Ula128VidFrame::is_vts_eof(self.ula.tsc)
    }

    fn reset<C: Cpu>(&mut self, cpu: &mut C, hard: bool) {
        if self.is_page3_contended() {
            self.ula_reset::<Ula128MemContention, _>(cpu, hard)
        }
        else {
            self.ula_reset::<UlaMemoryContention, _>(cpu, hard)
        }
        self.mem_page3_bank = 0;
        self.cur_screen_shadow = false;
        self.beg_screen_shadow = false;
        self.mem_locked = false;
    }

    fn nmi<C: Cpu>(&mut self, cpu: &mut C) -> bool {
        if self.is_page3_contended() {
            self.ula_nmi::<Ula128MemContention, _>(cpu)
        }
        else {
            self.ula_nmi::<UlaMemoryContention, _>(cpu)
        }
    }

    fn execute_next_frame<C: Cpu>(&mut self, cpu: &mut C) {
        loop {
            if self.is_page3_contended() {
                if self.ula_execute_next_frame_with_breaks::<Ula128MemContention, _>(cpu) {
                    break
                }
            }
            else {
                if self.ula_execute_next_frame_with_breaks::<UlaMemoryContention, _>(cpu) {
                    break;
                }
            }
        }
    }

    fn ensure_next_frame(&mut self) {
        self.ensure_next_frame_vtsc::<UlaMemoryContention>();
    }

    fn execute_single_step<C: Cpu, F: FnOnce(CpuDebug)>(
            &mut self,
            cpu: &mut C,
            debug: Option<F>
        ) -> Result<(),()>
    {
        if self.is_page3_contended() {
            self.ula_execute_single_step::<Ula128MemContention,_,_>(cpu, debug)
        }
        else {
            self.ula_execute_single_step::<UlaMemoryContention,_,_>(cpu, debug)
        }
    }
}

impl<B, X> Ula128<B, X>
    where B: BusDevice<Timestamp=VideoTs>
{
    #[inline]
    fn prepare_next_frame<T: MemoryContention>(
            &mut self,
            vtsc: VFrameTsCounter<Ula128VidFrame, T>
        ) -> VFrameTsCounter<Ula128VidFrame, T>
    {
        // println!("vis: {} nxt: {} p3: {}", self.beg_screen_shadow, self.cur_screen_shadow, self.mem_page3_bank);
        self.beg_screen_shadow = self.cur_screen_shadow;
        self.shadow_frame_cache.clear();
        self.screen_changes.clear();
        self.ula.prepare_next_frame(vtsc)
    }

}

impl<B, X> UlaTimestamp for Ula128<B, X>
    where B: BusDevice<Timestamp=VideoTs>
{
    type VideoFrame = Ula128VidFrame;
    #[inline(always)]
    fn video_ts(&self) -> VideoTs {
        self.ula.video_ts()
    }
    #[inline(always)]
    fn set_video_ts(&mut self, vts: VideoTs) {
        self.ula.set_video_ts(vts)
    }
    #[inline(always)]
    fn ensure_next_frame_vtsc<T: MemoryContention>(
            &mut self
        ) -> VFrameTsCounter<Self::VideoFrame, T>
    {
        let mut vtsc = VFrameTsCounter::from(self.ula.tsc);
        if vtsc.is_eof() {
            vtsc = self.prepare_next_frame(vtsc);
        }
        vtsc

    }
}

#[cfg(test)]
mod tests {
    use core::convert::TryFrom;
    use crate::bus::NullDevice;
    use crate::video::Video;
    use super::*;
    type TestUla128 = Ula128::<NullDevice<VideoTs>>;

    #[test]
    fn test_ula128() {
        let ula128 = TestUla128::default();
        assert_eq!(<TestUla128 as Video>::VideoFrame::FRAME_TSTATES_COUNT, 70908);
        assert_eq!(ula128.cpu_clock_rate(), CPU_HZ);
        assert_eq!(ula128.cpu_clock_rate(), 3_546_900);
        assert_eq!(ula128.frame_duration_nanos(), u32::try_from(70908u64 * 1_000_000_000 / 3_546_900).unwrap());
    }
}
