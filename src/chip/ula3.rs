/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
//! An emulator of Amstrad Gate Array chip for ZX Spectrum +2A/+3.
#![macro_use]
mod audio_earmic;
mod io;
mod video;
mod plus;
#[cfg(feature = "formats")]
mod screen;

use core::convert::TryFrom;
use core::fmt;

#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};

use crate::z80emu::{*, host::Result};
use crate::bus::{BusDevice, VFNullDevice};
use crate::clock::{VFrameTs, VideoTs, VFrameTsCounter, MemoryContention};
use crate::chip::{
    Ula128MemFlags, Ula3CtrlFlags, Ula3Paging, UlaControl,
    InnerAccess, EarIn, ReadEarMode, ControlUnit, MemoryAccess,
    ula::{
        Ula, UlaControlExt, UlaCpuExt,
        frame_cache::UlaFrameCache
    },
    ula128::MemPage8
};
use crate::memory::{ZxMemory, Memory128kPlus, MemoryExtension, NoMemoryExtension};
use crate::video::Video;
pub use video::Ula3VidFrame;

/// A struct implementing [MemoryContention] for any 8kb memory page being contended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Ula3MemContention { pages: u8 }

impl Ula3MemContention {
    const NNNN: Self = Ula3MemContention { pages: 0b0000_0000 };
    const NCNN: Self = Ula3MemContention { pages: 0b0000_1100 };
    const NCNC: Self = Ula3MemContention { pages: 0b1100_1100 };
    const CCCN: Self = Ula3MemContention { pages: 0b0011_1111 };
    const CCCC: Self = Ula3MemContention { pages: 0b1111_1111 };
}

pub(self) type InnerUla<B, X> = Ula<Memory128kPlus, B, X, Ula3VidFrame>;

#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "snapshot", serde(try_from = "u8", into = "u8"))]
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemPage4 {
    Bank0 = 0,
    Bank1 = 1,
    Bank2 = 2,
    Bank3 = 3
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TryFromU8MemPage4Error(pub u8);

/// +2A/+3 Amstrad "ULA" (or AGA - Amstrad gate array).
///
/// See [Ula] for description of generic parameters.
#[derive(Clone)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "snapshot", serde(rename_all = "camelCase"))]
pub struct Ula3<B=VFNullDevice<Ula3VidFrame>, X=NoMemoryExtension> {
    ula: InnerUla<B, X>,
    mem_special_paging: Option<Ula3Paging>,
    mem_page3_bank: MemPage8, // only in normal paging mode
    rom_bank: MemPage4,       // only in normal paging mode
    beg_screen_shadow: bool,  // shadow screen when a frame began
    cur_screen_shadow: bool,  // current shadow screen
    mem_locked: bool,
    #[cfg_attr(feature = "snapshot", serde(skip))]
    shadow_frame_cache: UlaFrameCache<Ula3VidFrame>,
    #[cfg_attr(feature = "snapshot", serde(skip))]
    screen_changes: Vec<VideoTs>
}

impl MemoryContention for Ula3MemContention {
    #[inline(always)]
    fn is_contended_address(self, address: u16) -> bool {
        (self.pages & 1 << (address >> 13)) != 0
    }
}

impl<B: Default, X: Default> Default for Ula3<B, X> {
    fn default() -> Self {
        let mut ula = Ula::default();
        ula.set_read_ear_mode(ReadEarMode::Clear);
        Ula3 {
            ula,
            mem_special_paging: None,
            mem_page3_bank: MemPage8::Bank0,
            rom_bank: MemPage4::Bank0,
            beg_screen_shadow: false,
            cur_screen_shadow: false,
            mem_locked: false,
            shadow_frame_cache: Default::default(),
            screen_changes: Vec::new()
        }
    }
}

impl<B, X> core::fmt::Debug for Ula3<B, X>
    where B: BusDevice, X: MemoryExtension
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Ula3")
            .field("ula", &self.ula)
            .field("mem_special_paging", &self.mem_special_paging)
            .field("mem_page3_bank", &self.mem_page3_bank)
            .field("rom_bank", &self.rom_bank)
            .field("beg_screen_shadow", &self.beg_screen_shadow)
            .field("cur_screen_shadow", &self.cur_screen_shadow)
            .field("mem_locked", &self.mem_locked)
            .field("shadow_frame_cache", &self.shadow_frame_cache)
            .field("screen_changes", &self.screen_changes.len())
            .finish()
    }
}

impl std::error::Error for TryFromU8MemPage4Error {}

impl fmt::Display for TryFromU8MemPage4Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "converted integer ({}) out of range for `MemPage4`", self.0)
    }
}

impl TryFrom<u8> for MemPage4 {
    type Error = TryFromU8MemPage4Error;
    fn try_from(bank: u8) -> core::result::Result<Self, Self::Error> {
        Ok(match bank {
            0 => MemPage4::Bank0,
            1 => MemPage4::Bank1,
            2 => MemPage4::Bank2,
            3 => MemPage4::Bank3,
            _ => return Err(TryFromU8MemPage4Error(bank))
        })
    }
}

macro_rules! impl_enum_from {
    ($($enum:ty: $ty:ty),*) => {$(
        impl From<$enum> for $ty {
            fn from(bank: $enum) -> $ty {
                bank as $ty
            }
        }
    )*};
}

impl_enum_from!{MemPage4: u8, MemPage4: usize}

impl<B, X> InnerAccess for Ula3<B, X> {
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

impl<B, X> UlaControl for Ula3<B, X> {
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
        let rom_bank: u8 = self.rom_bank.into();
        if rom_bank & 1 != 0 {
            flags.insert(Ula128MemFlags::ROM_BANK);
        }            
        if self.mem_locked {
            flags.insert(Ula128MemFlags::LOCK_MMU);
        }
        Some(flags)
    }

    fn set_ula128_mem_port_value(&mut self, value: Ula128MemFlags) -> bool {
        self.set_mem1_port_value(value, self.ula.current_video_ts());
        true
    }

    fn ula3_ctrl_port_value(&self) -> Option<Ula3CtrlFlags> {
        let flags = if let Some(paging) = self.mem_special_paging {
            Ula3CtrlFlags::with_special_paging(Ula3CtrlFlags::empty(), paging)
        }
        else {
            Ula3CtrlFlags::with_rom_page_bank_hi(Ula3CtrlFlags::empty(),
                self.rom_bank.into())
        };
        Some(flags)
    }

    fn set_ula3_ctrl_port_value(&mut self, value: Ula3CtrlFlags) -> bool {
        self.set_mem2_port_value(value);
        true
    }
}

impl<B, X> Ula3<B, X> {
    #[inline(always)]
    pub(super) fn memory_contention(&self) -> Ula3MemContention {
        if let Some(paging) = self.mem_special_paging {
            match paging {
                Ula3Paging::Banks0123 => Ula3MemContention::NNNN,
                Ula3Paging::Banks4567 => Ula3MemContention::CCCC,
                _ => Ula3MemContention::CCCN
            }
        }
        else if self.mem_page3_bank as u8 & 4 == 4 { // banks: 4, 5, 6 and 7 are contended
            Ula3MemContention::NCNC
        }
        else {
            Ula3MemContention::NCNN
        }
    }

    #[inline(always)]
    fn page3_screen_shadow_bank(&self) -> Option<bool> {
        match self.mem_special_paging {
            Some(Ula3Paging::Banks4567) => Some(true),
            Some(..)                    => None,
            _ => match self.mem_page3_bank {
                MemPage8::Bank5 => Some(false),
                MemPage8::Bank7 => Some(true),
                              _ => None
            }
        }
    }

    #[inline(always)]
    fn page1_screen_shadow_bank(&self) -> Option<bool> {
        match self.mem_special_paging {
            Some(Ula3Paging::Banks0123) => None,
            Some(Ula3Paging::Banks4763) => Some(true),
                                      _ => Some(false)
        }
    }
    // Returns `true` if the memory contention has changed.
    #[inline]
    fn set_mem_page3_bank_and_rom_lo(&mut self, ram_bank: MemPage8, rom_lo: bool) -> bool {
        let no_special_paging = self.mem_special_paging.is_none();
        let rom_bank = MemPage4::try_from((self.rom_bank as u8 & !1) | rom_lo as u8).unwrap();
        if self.rom_bank != rom_bank {
            self.rom_bank = rom_bank;
            if no_special_paging {
                self.ula.memory.map_rom_bank(rom_bank.into(), 0).unwrap();
            }
        }
        let ram_bank_diff = self.mem_page3_bank as u8 ^ ram_bank as u8;
        if ram_bank_diff != 0 {
            self.mem_page3_bank = ram_bank;
            if no_special_paging {
                self.ula.memory.map_ram_bank(ram_bank.into(), 3).unwrap();
                return ram_bank_diff & 4 == 4; // contention changed
            }
        }
        false
    }
    // Returns `true` if the memory contention has changed.
    #[inline]
    fn set_rom_hi_no_special_paging(&mut self, rom_hi: bool) -> bool {
        let is_special_paging = self.mem_special_paging.is_some();
        let rom_bank = MemPage4::try_from((self.rom_bank as u8 & !2) | ((rom_hi as u8) << 1)).unwrap();
        if self.rom_bank != rom_bank || is_special_paging {
            self.rom_bank = rom_bank;
            self.ula.memory.map_rom_bank(rom_bank.into(), 0).unwrap();
        }
        if is_special_paging {
            self.mem_special_paging = None;
            self.ula.memory.map_ram_bank(5, 1).unwrap();
            self.ula.memory.map_ram_bank(2, 2).unwrap();
            self.ula.memory.map_ram_bank(self.mem_page3_bank.into(), 3).unwrap();
            return true; // contention always changes
        }
        false
    }
    // Returns `true` if the memory contention has changed.
    #[inline]
    fn set_mem_special_paging(&mut self, new_paging: Ula3Paging) -> bool {
        let contention_changed = match self.mem_special_paging {
            Some(paging) if paging == new_paging => return false,
            Some(paging) => {
                // only when changing between Banks4563 and Banks4763 contention is unchanged
                (paging as u8 & new_paging as u8) & 2 == 0
            }
            None => true
        };
        self.mem_special_paging = Some(new_paging);
        for (bank, page) in new_paging.ram_banks_with_pages_iter() {
            self.ula.memory.map_ram_bank(bank, page).unwrap();
        }
        contention_changed
    }
    // Returns `true` if the memory contention has changed.
    fn set_mem1_port_value(&mut self, flags: Ula128MemFlags, ts: VideoTs) -> bool {
        self.mem_locked = flags.is_mmu_locked();
        let cur_screen_shadow = flags.is_shadow_screen();
        if self.cur_screen_shadow != cur_screen_shadow {
            self.cur_screen_shadow = cur_screen_shadow;
            self.screen_changes.push(ts);
        }
        let rom_lo = flags.intersects(Ula128MemFlags::ROM_BANK);
        let page3_bank = MemPage8::from(flags);
        // println!("\nscr: {} pg3: {} ts: {}x{}", self.cur_screen_shadow, mem_page3_bank, ts.vc, ts.hc);
        self.set_mem_page3_bank_and_rom_lo(page3_bank, rom_lo)
    }
    // Returns `true` if the memory contention has changed.
    fn set_mem2_port_value(&mut self, flags: Ula3CtrlFlags) -> bool {
        if let Some(paging) = flags.special_paging() {
            self.set_mem_special_paging(paging)
        }
        else {
            let rom_hi = flags.intersects(Ula3CtrlFlags::ROM_BANK_HI);
            self.set_rom_hi_no_special_paging(rom_hi)
        }
    }
}

impl<B, X> MemoryAccess for Ula3<B, X>
    where X: MemoryExtension
{
    type Memory = Memory128kPlus;
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

impl<B, X> ControlUnit for Ula3<B, X>
    where B: BusDevice,
          B::Timestamp: From<VFrameTs<Ula3VidFrame>>,
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
            self.mem_special_paging = None;
            self.mem_page3_bank = MemPage8::Bank0;
            self.rom_bank = MemPage4::Bank0;
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

impl<B, X> UlaControlExt for Ula3<B, X>
    where B: BusDevice,
          B::Timestamp: From<VFrameTs<Ula3VidFrame>>
{
    fn prepare_next_frame<C: MemoryContention>(
            &mut self,
            vtsc: VFrameTsCounter<Ula3VidFrame, C>
        ) -> VFrameTsCounter<Ula3VidFrame, C>
    {
        self.beg_screen_shadow = self.cur_screen_shadow;
        self.shadow_frame_cache.clear();
        self.screen_changes.clear();
        self.ula.prepare_next_frame(vtsc)
    }
}

#[cfg(test)]
mod tests {
    use core::iter;
    use crate::video::{Video, VideoFrame};
    use super::*;

    #[test]
    fn test_ula3() {
        assert_eq!(<Ula3 as Video>::VideoFrame::FRAME_TSTATES_COUNT, 70908);
        let mut ula: Ula3 = Default::default();
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
                assert_eq!(clock.is_contended_address(addr), bank >= 4);
            }
        }
        let flags = Ula3CtrlFlags::with_special_paging(Ula3CtrlFlags::empty(), Ula3Paging::Banks0123);
        ula.set_ula3_ctrl_port_value(flags);
        let clock = ula.current_video_clock();
        for addr in 0x0000..=0xFFFF {
            assert_eq!(clock.is_contended_address(addr), false);
        }
        let flags = Ula3CtrlFlags::with_special_paging(Ula3CtrlFlags::empty(), Ula3Paging::Banks4567);
        ula.set_ula3_ctrl_port_value(flags);
        let clock = ula.current_video_clock();
        for addr in 0x0000..=0xFFFF {
            assert_eq!(clock.is_contended_address(addr), true);
        }
        for paging in iter::once(Ula3Paging::Banks4563).chain(
                      iter::once(Ula3Paging::Banks4763)) {
            let flags = Ula3CtrlFlags::with_special_paging(Ula3CtrlFlags::empty(), paging);
            ula.set_ula3_ctrl_port_value(flags);
            let clock = ula.current_video_clock();
            for addr in 0x0000..0xC000 {
                assert_eq!(clock.is_contended_address(addr), true);
            }
            for addr in 0xC000..=0xFFFF {
                assert_eq!(clock.is_contended_address(addr), false);
            }
        }
    }
}
