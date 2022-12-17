/*
    Copyright (C) 2020-2022  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
//! An emulator of Sinclair Uncommitted Logic Array chip for ZX Spectrum 128k/+2.
#![macro_use]
mod audio_earmic;
pub mod frame_cache;
mod io;
pub(crate) mod video;
mod plus;
#[cfg(feature = "formats")]
mod screen;

use core::fmt;
use core::convert::TryFrom;

use crate::z80emu::{*, host::Result};
#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};

use crate::bus::{BusDevice, VFNullDevice};
use crate::clock::{VFrameTs, VideoTs, VFrameTsCounter, MemoryContention};
use crate::chip::{
    InnerAccess, ControlUnit, MemoryAccess, Ula128MemFlags, UlaControl,
    ula::{
        Ula, UlaControlExt, UlaCpuExt,
        frame_cache::UlaFrameCache
    }
};
use crate::memory::{Memory128k, ZxMemory, MemoryExtension, NoMemoryExtension, MemoryKind};
use crate::video::Video;
pub use video::Ula128VidFrame;


/// A struct implementing [MemoryContention] for addresses in the range: [0x4000, 0x7FFF] or [0xC000, 0xFFFF]
/// being contended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Ula128MemContention { mask: u16 }

impl Ula128MemContention {
    const NCNN: Self = Ula128MemContention { mask: 0xC000 };
    const NCNC: Self = Ula128MemContention { mask: 0x4000 };
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

/// 128k ULA (Uncommitted Logic Array).
///
/// See [Ula] for description of generic parameters.
#[derive(Clone)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "snapshot", serde(rename_all = "camelCase"))]
pub struct Ula128<B=VFNullDevice<Ula128VidFrame>, X=NoMemoryExtension> {
    ula: InnerUla<B, X>,
    mem_page3_bank: MemPage8,
    beg_screen_shadow: bool, // shadow screen when a frame began
    cur_screen_shadow: bool, // current shadow screen
    mem_locked: bool,

    #[cfg(feature = "boxed_frame_cache")]
    #[cfg_attr(feature = "snapshot", serde(skip))]
    shadow_frame_cache: Box<UlaFrameCache<Ula128VidFrame>>,

    #[cfg(not(feature = "boxed_frame_cache"))]
    #[cfg_attr(feature = "snapshot", serde(skip))]
    shadow_frame_cache: UlaFrameCache<Ula128VidFrame>,

    #[cfg_attr(feature = "snapshot", serde(skip))]
    screen_changes: Vec<VideoTs>,
}

impl MemoryContention for Ula128MemContention {
    #[inline(always)]
    fn is_contended_address(self, address: u16) -> bool {
        address & self.mask == 0x4000
    }
}

impl<B: Default, X: Default> Default for Ula128<B, X> {
    fn default() -> Self {
        Ula128 {
            ula: Default::default(),
            mem_page3_bank: MemPage8::Bank0,
            beg_screen_shadow: false,
            cur_screen_shadow: false,
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
            .field("beg_screen_shadow", &self.beg_screen_shadow)
            .field("cur_screen_shadow", &self.cur_screen_shadow)
            .field("mem_locked", &self.mem_locked)
            .field("shadow_frame_cache", &self.shadow_frame_cache)
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

macro_rules! match_mempage8 {
    ($expr:expr; $else:expr) => {
        match $expr {
            0 => MemPage8::Bank0,
            1 => MemPage8::Bank1,
            2 => MemPage8::Bank2,
            3 => MemPage8::Bank3,
            4 => MemPage8::Bank4,
            5 => MemPage8::Bank5,
            6 => MemPage8::Bank6,
            7 => MemPage8::Bank7,
            _ => $else            
        }
    };
}

impl TryFrom<u8> for MemPage8 {
    type Error = TryFromU8MemPage8Error;
    fn try_from(bank: u8) -> core::result::Result<Self, Self::Error> {
        Ok(match_mempage8!{ bank; return Err(TryFromU8MemPage8Error(bank)) })
    }
}

impl From<Ula128MemFlags> for MemPage8 {
    fn from(flags: Ula128MemFlags) -> Self {
        match_mempage8!{ (flags & Ula128MemFlags::RAM_BANK_MASK).bits(); unreachable!() }
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

impl<B, X> InnerAccess for Ula128<B, X> {
    type Inner = InnerUla<B, X>;

    fn inner_ref(&self) -> &Self::Inner {
        &self.ula
    }

    fn inner_mut(&mut self) -> &mut Self::Inner {
        &mut self.ula
    }

    fn into_inner(self) -> Self::Inner {
        self.ula
    }
}

impl<B, X> UlaControl for Ula128<B, X> {
    fn has_late_timings(&self) -> bool {
        self.ula.has_late_timings()
    }

    fn set_late_timings(&mut self, late_timings: bool) {
        self.ula.set_late_timings(late_timings)
    }

    fn ula128_mem_port_value(&self) -> Option<Ula128MemFlags> {
        let mut flags = Ula128MemFlags::empty()
                        .with_last_ram_page_bank(self.mem_page3_bank.into());
        if self.cur_screen_shadow {
            flags.insert(Ula128MemFlags::SCREEN_BANK);
        };
        if let Ok((MemoryKind::Rom, rom_bank)) = self.ula.memory.page_bank(0) {
            if rom_bank != 0 {
                flags.insert(Ula128MemFlags::ROM_BANK);
            }            
        }
        if self.mem_locked {
            flags.insert(Ula128MemFlags::LOCK_MMU);
        }
        Some(flags)
    }

    fn set_ula128_mem_port_value(&mut self, value: Ula128MemFlags) -> bool {
        self.set_mem_port_value(value, self.ula.current_video_ts());
        true
    }
}

impl<B, X> Ula128<B, X> {
    #[inline(always)]
    pub(crate) fn memory_contention(&self) -> Ula128MemContention {
        if self.mem_page3_bank as u8 & 1 == 1 { // banks: 1, 3, 5 and 7 are contended
            Ula128MemContention::NCNC
        }
        else {
            Ula128MemContention::NCNN
        }
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

    fn set_mem_port_value(&mut self, flags: Ula128MemFlags, ts: VideoTs) -> bool {
        self.mem_locked = flags.is_mmu_locked();
        let cur_screen_shadow = flags.is_shadow_screen();
        if self.cur_screen_shadow != cur_screen_shadow {
            self.cur_screen_shadow = cur_screen_shadow;
            self.screen_changes.push(ts);
        }
        let rom_bank = flags.rom_page_bank();
        self.ula.memory.map_rom_bank(rom_bank, 0).unwrap();
        // println!("\nscr: {} pg3: {} ts: {}x{}", self.cur_screen_shadow, mem_page3_bank, ts.vc, ts.hc);
        self.set_mem_page3_bank(MemPage8::from(flags))
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

    fn memory_with_ext_mut(&mut self) -> (&mut Self::Memory, &mut Self::MemoryExt) {
        (&mut self.ula.memory, &mut self.ula.memext)
    }
}

impl<B, X> ControlUnit for Ula128<B, X>
    where B: BusDevice,
          B::Timestamp: From<VFrameTs<Ula128VidFrame>>,
          X: MemoryExtension
{
    type BusDevice = B;

    #[inline]
    fn bus_device_mut(&mut self) -> &mut Self::BusDevice {
        self.ula.bus_device_mut()
    }
    #[inline]
    fn bus_device_ref(&self) -> &Self::BusDevice {
        self.ula.bus_device_ref()
    }
    #[inline]
    fn into_bus_device(self) -> Self::BusDevice {
        self.ula.into_bus_device()
    }

    fn reset<C: Cpu>(&mut self, cpu: &mut C, hard: bool) {
        self.ula.reset(cpu, hard);
        if hard {
            self.mem_page3_bank = MemPage8::Bank0;
            if self.cur_screen_shadow {
                self.screen_changes.push(self.current_video_ts());
            }
            self.cur_screen_shadow = false;
            self.mem_locked = false;
        }
    }

    fn nmi<C: Cpu>(&mut self, cpu: &mut C) -> bool {
        self.ula_nmi(cpu)
    }

    fn execute_next_frame<C: Cpu>(&mut self, cpu: &mut C) {
        while !self.ula_execute_next_frame_with_breaks(cpu) {}
    }

    fn ensure_next_frame(&mut self) {
        self.ensure_next_frame_vtsc();
    }

    fn execute_single_step<C: Cpu, F: FnOnce(CpuDebug)>(
            &mut self,
            cpu: &mut C,
            debug: Option<F>
        ) -> Result<(),()>
    {
        self.ula_execute_single_step(cpu, debug)
    }
}

impl<B, X> UlaControlExt for Ula128<B, X>
    where B: BusDevice,
          B::Timestamp: From<VFrameTs<Ula128VidFrame>>,
{
    fn prepare_next_frame<C: MemoryContention>(
            &mut self,
            vtsc: VFrameTsCounter<Ula128VidFrame, C>
        ) -> VFrameTsCounter<Ula128VidFrame, C>
    {
        // println!("vis: {} nxt: {} p3: {}", self.beg_screen_shadow, self.cur_screen_shadow, self.mem_page3_bank);
        self.beg_screen_shadow = self.cur_screen_shadow;
        self.shadow_frame_cache.clear();
        self.screen_changes.clear();
        self.ula.prepare_next_frame(vtsc)
    }
}

#[cfg(test)]
mod tests {
    use crate::video::{Video, VideoFrame};
    use super::*;

    #[test]
    fn test_ula128() {
        assert_eq!(<Ula128 as Video>::VideoFrame::FRAME_TSTATES_COUNT, 70908);
        let mut ula: Ula128 = Default::default();
        for bank in 0..8 {
            let flags = Ula128MemFlags::with_last_ram_page_bank(Ula128MemFlags::empty(), bank);
            ula.set_ula128_mem_port_value(flags);
            let clock = ula.current_video_clock();
            for addr in 0x4000..0x8000 {
                assert_eq!(clock.is_contended_address(addr), true);
            }
            for addr in (0x0000..0x4000).chain(0x8000..0xC000) {
                assert_eq!(clock.is_contended_address(addr), false);
            }
            for addr in 0xC000..=0xFFFF {
                assert_eq!(clock.is_contended_address(addr), bank & 1 == 1);
            }
        }
    }
}
