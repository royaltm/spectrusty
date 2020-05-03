mod audio_earmic;
pub(crate) mod frame_cache;
mod io;
mod video;

use core::fmt;
use core::convert::TryFrom;

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
use crate::memory::{Memory128k, ZxMemory, MemoryExtension, NoMemoryExtension};
use crate::clock::{VideoTs, FTs, VFrameTsCounter, MemoryContention};

pub use video::Ula128VidFrame;
pub(crate) use io::Ula128MemFlags;

/// The ZX Spectrum 128k CPU clock in cycles per second.
pub const CPU_HZ: u32 = 3_546_900;

/// Implements [MemoryContention] in a way that addresses in the range: [0x4000, 0x7FFF] and [0xC000, 0xFFFF]
/// are being contended.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Ula128MemContention;

impl MemoryContention for Ula128MemContention {
    #[inline]
    fn is_contended_address(addr: u16) -> bool {
        addr & 0x4000 == 0x4000
    }
}

pub(self) type InnerUla<B, X> = Ula<Memory128k, B, X, Ula128VidFrame>;

#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "snapshot", serde(try_from = "u8", into = "u8"))]
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemPage8 {
    Bank0 = 0,
    Bank1 = 1,
    Bank2 = 2,
    Bank3 = 3,
    Bank4 = 4,
    Bank5 = 5,
    Bank6 = 6,
    Bank7 = 7
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TryFromU8MemPage8Error(pub u8);

/// ZX Spectrum 128k ULA.
#[derive(Clone)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "snapshot", serde(rename_all = "camelCase"))]
pub struct Ula128<B=NullDevice<VideoTs>, X=NoMemoryExtension> {
    ula: InnerUla<B, X>,
    mem_page3_bank: MemPage8,
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
            mem_page3_bank: MemPage8::Bank0,
            cur_screen_shadow: false,
            beg_screen_shadow: false,
            mem_locked: false,
            shadow_frame_cache: Default::default(),
            screen_changes: Vec::new()
        }
    }
}

impl<B, X> core::fmt::Debug for Ula128<B, X>
    where B: BusDevice, X: MemoryExtension
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Ula128")
            .field("ula", &self.ula)
            .field("mem_page3_bank", &self.mem_page3_bank)
            .field("cur_screen_shadow", &self.cur_screen_shadow)
            .field("beg_screen_shadow", &self.beg_screen_shadow)
            .field("mem_locked", &self.mem_locked)
            .field("screen_changes", &self.screen_changes.len())
            .finish()
    }
}

impl std::error::Error for TryFromU8MemPage8Error {}

impl fmt::Display for TryFromU8MemPage8Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "converted integer ({}) out of range for `MemPage8`", self.0)
    }
}

impl TryFrom<u8> for MemPage8 {
    type Error = TryFromU8MemPage8Error;
    fn try_from(bank: u8) -> core::result::Result<Self, Self::Error> {
        Ok(match bank {
            0 => MemPage8::Bank0,
            1 => MemPage8::Bank1,
            2 => MemPage8::Bank2,
            3 => MemPage8::Bank3,
            4 => MemPage8::Bank4,
            5 => MemPage8::Bank5,
            6 => MemPage8::Bank6,
            7 => MemPage8::Bank7,
            _ => return Err(TryFromU8MemPage8Error(bank))
        })
    }
}

macro_rules! impl_ram_page_from {
    ($($ty:ty),*) => {$(
        impl From<MemPage8> for $ty {
            fn from(bank: MemPage8) -> $ty {
                bank as $ty
            }
        }
    )*};
}

impl_ram_page_from!(u8, usize);

impl<B, X> Ula128<B, X> {
    #[inline(always)]
    fn is_page3_contended(&self) -> bool {
        self.mem_page3_bank as u8 & 1 == 1 // banks: 1, 3, 5 and 7 are contended
    }

    #[inline(always)]
    fn page3_screen_shadow_bank(&self) -> Option<bool> {
        match self.mem_page3_bank {
            MemPage8::Bank5 => Some(false),
            MemPage8::Bank7 => Some(true),
            _ => None
        }
    }

    #[inline]
    fn set_mem_page3_bank(&mut self, bank: MemPage8) -> bool {
        let bank_diff = self.mem_page3_bank as u8 ^ bank as u8;
        if bank_diff != 0 {
            self.mem_page3_bank = bank;
            self.ula.memory.map_ram_bank(bank.into(), 3).unwrap();
            return bank_diff & 1 == 1;
        }
        false
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

    fn into_bus_device(self) -> Self::BusDevice {
        self.ula.bus
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
        self.ula_reset(cpu, hard);
        self.mem_page3_bank = MemPage8::Bank0;
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
    use crate::video::Video;
    use super::*;
    type TestUla128 = Ula128;

    #[test]
    fn test_ula128() {
        let ula128 = TestUla128::default();
        assert_eq!(<TestUla128 as Video>::VideoFrame::FRAME_TSTATES_COUNT, 70908);
        assert_eq!(ula128.cpu_clock_rate(), CPU_HZ);
        assert_eq!(ula128.cpu_clock_rate(), 3_546_900);
        assert_eq!(ula128.frame_duration_nanos(), u32::try_from(70908u64 * 1_000_000_000 / 3_546_900).unwrap());
    }
}
