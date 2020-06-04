//! An emulator of Amstrad Gate Array chip for ZX Spectrum +2A/+3.
mod audio_earmic;
mod io;
mod video;

use core::convert::TryFrom;
use core::fmt;

#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};

use crate::z80emu::{*, host::Result};
use crate::bus::{BusDevice, NullDevice};
use crate::chip::{
    Ula128MemFlags, Ula3CtrlFlags, Ula3Paging,
    EarIn, ReadEarMode, ControlUnit, MemoryAccess,
    ula::{
        frame_cache::UlaFrameCache,
        Ula, UlaTimestamp, UlaCpuExt, UlaMemoryContention
    },
    ula128::{
        MemPage8, Ula128MemContention
    }
};
use crate::memory::{ZxMemory, Memory128kPlus, MemoryExtension, NoMemoryExtension};
use crate::clock::{VideoTs, FTs, VFrameTsCounter, MemoryContention, NoMemoryContention};

pub use video::Ula3VidFrame;

/// Implements [MemoryContention] in a way that all addresses are being contended.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct FullMemContention;

impl MemoryContention for FullMemContention {
    #[inline]
    fn is_contended_address(_addr: u16) -> bool { true }
}

/// Implements [MemoryContention] in a way that addresses in the range: [0x0000, 0xBFFF] are being contended.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Ula3MemContention;

impl MemoryContention for Ula3MemContention {
    #[inline]
    fn is_contended_address(addr: u16) -> bool {
        addr & 0xC000 != 0xC000
    }
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

/// ZX Spectrum +2A/+3 ULA. (Amstrad gate array).
#[derive(Clone)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "snapshot", serde(rename_all = "camelCase"))]
pub struct Ula3<B=NullDevice<VideoTs>, X=NoMemoryExtension> {
    ula: InnerUla<B, X>,
    mem_special_paging: Option<Ula3Paging>,
    mem_page3_bank: MemPage8, // only in normal paging mode
    rom_bank: MemPage4,       // only in normal paging mode
    cur_screen_shadow: bool,  // current shadow screen
    beg_screen_shadow: bool,  // shadow screen when a frame began
    mem_locked: bool,
    #[cfg_attr(feature = "snapshot", serde(skip))]
    shadow_frame_cache: UlaFrameCache<Ula3VidFrame>,
    #[cfg_attr(feature = "snapshot", serde(skip))]
    screen_changes: Vec<VideoTs>
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(self) enum ContentionKind {
    Ula,    // UlaMemoryContention - NCNN
    Ula128, // Ula128MemContention - NCNC
    None,   // NoMemoryContention  - NNNN
    Full,   // FullMemContention   - CCCC
    Ula3    // Ula3MemContention   - CCCN
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
            cur_screen_shadow: false,
            beg_screen_shadow: false,
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
            .field("cur_screen_shadow", &self.cur_screen_shadow)
            .field("beg_screen_shadow", &self.beg_screen_shadow)
            .field("mem_locked", &self.mem_locked)
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

impl<B, X> Ula3<B, X> {
    /// Sets the frame counter to the specified value.
    pub fn set_frame_counter(&mut self, fc: u64) {
        self.ula.set_frame_counter(fc)
    }
    /// Sets the T-state counter to the specified value modulo `Ula3VidFrame::FRAME_TSTATES_COUNT`.
    pub fn set_frame_tstate(&mut self, ts: FTs) {
        self.ula.set_frame_tstate(ts)
    }
    /// Returns the state of the "late timings" mode.
    pub fn has_late_timings(&self) -> bool {
        self.ula.has_late_timings()
    }
    /// Sets the "late timings" mode on or off.
    ///
    /// In this mode interrupts are being requested just one T-state earlier than normally.
    /// This results in all other timings being one T-state later.
    pub fn set_late_timings(&mut self, late_timings: bool) {
        self.ula.set_late_timings(late_timings)
    }
    /// Returns the last value sent to the memory port `0x7FFD`.
    ///
    /// Usefull for creating snapshots.
    pub fn mem_port1_value(&self) -> Ula128MemFlags {
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
        flags
    }
    /// Returns the last value sent to the memory port `0x1FFD`.
    ///
    /// Usefull for creating snapshots.
    pub fn mem_port2_value(&self) -> Ula3CtrlFlags {
        if let Some(paging) = self.mem_special_paging {
            Ula3CtrlFlags::with_special_paging(Ula3CtrlFlags::empty(), paging)
        }
        else {
            Ula3CtrlFlags::with_rom_page_bank_hi(Ula3CtrlFlags::empty(),
                self.rom_bank.into())
        }
    }

    #[inline(always)]
    fn contention_kind(&self) -> ContentionKind {
        if let Some(paging) = self.mem_special_paging {
            match paging {
                Ula3Paging::Banks0123 => ContentionKind::None,
                Ula3Paging::Banks4567 => ContentionKind::Full,
                _ => ContentionKind::Ula3
            }
        }
        else if self.mem_page3_bank as u8 & 4 == 4 { // banks: 4, 5, 6 and 7 are contended
            ContentionKind::Ula128
        }
        else {
            ContentionKind::Ula
        }
    }

    #[inline(always)]
    fn page3_screen_shadow_bank(&self) -> Option<bool> {
        match self.mem_special_paging {
            Some(Ula3Paging::Banks4567) => Some(true),
            Some(..)                       => None,
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
}

impl<B, X> ControlUnit for Ula3<B, X>
    where B: BusDevice<Timestamp=VideoTs>,
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

    fn current_frame(&self) -> u64 {
        self.ula.current_frame()
    }

    fn frame_tstate(&self) -> (u64, FTs) {
        self.ula.frame_tstate()
    }

    fn current_tstate(&self) -> FTs {
        self.ula.current_tstate()
    }

    fn is_frame_over(&self) -> bool {
        self.ula.is_frame_over()
    }

    fn reset<C: Cpu>(&mut self, cpu: &mut C, hard: bool) {
        self.ula.reset(cpu, hard);
        if hard {
            self.mem_special_paging = None;
            self.mem_page3_bank = MemPage8::Bank0;
            self.rom_bank = MemPage4::Bank0;
            if self.cur_screen_shadow {
                self.screen_changes.push(self.video_ts());
            }
            self.cur_screen_shadow = false;
            self.mem_locked = false;
        }
    }

    fn nmi<C: Cpu>(&mut self, cpu: &mut C) -> bool {
        use ContentionKind::*;
        match self.contention_kind() {
            Ula    => self.ula_nmi::<UlaMemoryContention, _>(cpu),
            Ula128 => self.ula_nmi::<Ula128MemContention, _>(cpu),
            None   => self.ula_nmi::<NoMemoryContention, _>(cpu),
            Full   => self.ula_nmi::<FullMemContention, _>(cpu),
            Ula3   => self.ula_nmi::<Ula3MemContention, _>(cpu)
        }
    }

    fn execute_next_frame<C: Cpu>(&mut self, cpu: &mut C) {
        use ContentionKind::*;
        loop {
            if match self.contention_kind() {
                Ula    => self.ula_execute_next_frame_with_breaks::<UlaMemoryContention, _>(cpu),
                Ula128 => self.ula_execute_next_frame_with_breaks::<Ula128MemContention, _>(cpu),
                None   => self.ula_execute_next_frame_with_breaks::<NoMemoryContention, _>(cpu),
                Full   => self.ula_execute_next_frame_with_breaks::<FullMemContention, _>(cpu),
                Ula3   => self.ula_execute_next_frame_with_breaks::<Ula3MemContention, _>(cpu)
            } {
                break
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
        use ContentionKind::*;
        match self.contention_kind() {
            Ula    => self.ula_execute_single_step::<UlaMemoryContention,_,_>(cpu, debug),
            Ula128 => self.ula_execute_single_step::<Ula128MemContention,_,_>(cpu, debug),
            None   => self.ula_execute_single_step::<NoMemoryContention,_,_>(cpu, debug),
            Full   => self.ula_execute_single_step::<FullMemContention,_,_>(cpu, debug),
            Ula3   => self.ula_execute_single_step::<Ula3MemContention,_,_>(cpu, debug)
        }
    }
}

impl<B, X> Ula3<B, X>
    where B: BusDevice<Timestamp=VideoTs>
{
    #[inline]
    fn prepare_next_frame<T: MemoryContention>(
            &mut self,
            vtsc: VFrameTsCounter<Ula3VidFrame, T>
        ) -> VFrameTsCounter<Ula3VidFrame, T>
    {
        self.beg_screen_shadow = self.cur_screen_shadow;
        self.shadow_frame_cache.clear();
        self.screen_changes.clear();
        self.ula.prepare_next_frame(vtsc)
    }
}

impl<B, X> UlaTimestamp for Ula3<B, X>
    where B: BusDevice<Timestamp=VideoTs>
{
    type VideoFrame = Ula3VidFrame;
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
    use crate::video::{Video, VideoFrame};
    use super::*;

    #[test]
    fn test_ula3() {
        assert_eq!(<Ula3 as Video>::VideoFrame::FRAME_TSTATES_COUNT, 70908);
    }
}
