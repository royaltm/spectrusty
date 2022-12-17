/*
    Copyright (C) 2020-2022  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
/*! An emulator of the Timex's SCLD chip used in TC2048 / TC2068 and TS2068 models.

Implementation specifics:

* Currently, there are no dedicated [VideoFrame] implementations for TC2048 / TC2068 / TS2068 models,
  in the meantime, you may use the [UlaVideoFrame] and [UlaNTSCVidFrame] with [Scld].
* SCLD ports: `0xFF` and `0xF4` as well as ULA `0xFE` port are decoded on all 8 lowest address bits as per original machines.
* The hard reset defaults the screen mode and memory paging but leaves the border-color unmodified.
* The DOCK and EX-ROM memory pages are mapped from ROM banks as follows and depend on [ZxMemory::ROM_BANKS_MAX]:

```text
   DOCK: [0, 7]
 EX-ROM: [8, ROM_BANKS_MAX - 2] modulo (ROM_BANKS_MAX - 9)
16k ROM: [ROM_BANKS_MAX - 1, ROM_BANKS_MAX]
```

In case of TC2048 the 16k ROM should be loaded to ROM banks: `[ROM_BANKS_MAX - 1, ROM_BANKS_MAX]`.
In case of Tx2068 the 24k ROM should be loaded to ROM banks: `[ROM_BANKS_MAX - 1, ROM_BANKS_MAX, 8]`.

[UlaVideoFrame]: crate::chip::ula::UlaVideoFrame
[UlaNTSCVidFrame]: crate::chip::ula::UlaNTSCVidFrame
[ZxMemory::ROM_BANKS_MAX]: crate::memory::ZxMemory::ROM_BANKS_MAX
*/
use crate::z80emu::{*, host::Result};
use crate::clock::{
    VFrameTs, VideoTs,
    VideoTsData2, VideoTsData6,
    VFrameTsCounter, MemoryContention
};
use crate::bus::{BusDevice};
use crate::chip::{
    ScldCtrlFlags, UlaControl,
    InnerAccess, EarIn, ReadEarMode, ControlUnit, MemoryAccess,
    ula::{
        Ula,
        UlaControlExt, UlaCpuExt,
        frame_cache::UlaFrameCache
    }
};
use crate::memory::{
    PagedMemory8k, MemoryExtension
};
use crate::video::{Video, VideoFrame, BorderColor, RenderMode};

#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};

pub mod frame_cache;
mod audio_earmic;
pub(crate) mod io;
mod video;
#[cfg(feature = "formats")]
mod screen;

use frame_cache::SourceMode;

/// This is the emulator of SCLD chip used with Timex's TC2048 / TC2068 / TS2068 models.
///
/// The generic type `M` must implement [PagedMemory8k] trait in addition to [ZxMemory].
///
/// See [Ula] for description of other generic parameters.
///
/// [ZxMemory]: crate::memory::ZxMemory
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "snapshot", serde(rename_all = "camelCase"))]
#[derive(Clone)]
pub struct Scld<M: PagedMemory8k, B, X, V>
{
    #[cfg_attr(feature = "snapshot", serde(bound(
        serialize = "Ula<M, B, X, V>: Serialize",
        deserialize = "Ula<M, B, X, V>: Deserialize<'de>"
    )))]
    ula: Ula<M, B, X, V>,
    beg_ctrl_flags: ScldCtrlFlags,
    cur_ctrl_flags: ScldCtrlFlags,
    mem_paged: u8,

    #[cfg(feature = "boxed_frame_cache")]
    #[cfg_attr(feature = "snapshot", serde(skip))]
    sec_frame_cache: Box<UlaFrameCache<V>>,

    #[cfg(not(feature = "boxed_frame_cache"))]
    #[cfg_attr(feature = "snapshot", serde(skip))]
    sec_frame_cache: UlaFrameCache<V>,

    #[cfg_attr(feature = "snapshot", serde(skip))]
    mode_changes: Vec<VideoTsData6>,
    #[cfg_attr(feature = "snapshot", serde(skip))]
    source_changes: Vec<VideoTsData2>,
}

impl<M, B, X, V> Default for Scld<M, B, X, V>
    where M: PagedMemory8k,
          V: VideoFrame,
          Ula<M, B, X, V>: Default
{
    fn default() -> Self {
        let mut ula = Ula::default();
        ula.set_read_ear_mode(ReadEarMode::Set);
        Scld {
            ula,
            beg_ctrl_flags: ScldCtrlFlags::empty(),
            cur_ctrl_flags: ScldCtrlFlags::empty(),
            mem_paged: 0,
            sec_frame_cache: Default::default(),
            mode_changes: Vec::new(),
            source_changes: Vec::new(),
        }
    }
}

impl<M, B, X, V> core::fmt::Debug for Scld<M, B, X, V>
    where M: PagedMemory8k,
          B: BusDevice,
          X: MemoryExtension,
          V: VideoFrame
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Scld")
            .field("ula", &self.ula)
            .field("beg_ctrl_flags", &self.beg_ctrl_flags)
            .field("cur_ctrl_flags", &self.cur_ctrl_flags)
            .field("mem_paged", &self.mem_paged)
            .field("sec_frame_cache", &self.sec_frame_cache)
            .field("mode_changes", &self.mode_changes.len())
            .field("source_changes", &self.source_changes.len())
            .finish()
    }
}

impl<M, B, X, V> InnerAccess for Scld<M, B, X, V>
    where M: PagedMemory8k
{
    type Inner = Ula<M, B, X, V>;

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

impl<M, B, X, V> UlaControl for Scld<M, B, X, V>
    where M: PagedMemory8k,
          V: VideoFrame
{
    fn has_late_timings(&self) -> bool {
        self.ula.has_late_timings()
    }

    fn set_late_timings(&mut self, late_timings: bool) {
        self.ula.set_late_timings(late_timings)
    }

    fn scld_ctrl_port_value(&self) -> Option<ScldCtrlFlags> {
        Some(self.cur_ctrl_flags)
    }

    fn set_scld_ctrl_port_value(&mut self, value: ScldCtrlFlags) -> bool {
        self.set_ctrl_flags_value(value, self.current_video_ts());
        true
    }

    fn scld_mmu_port_value(&self) -> Option<u8> {
        Some(self.mem_paged)
    }

    fn set_scld_mmu_port_value(&mut self, value: u8) -> bool {
        self.set_mmu_flags_value(value);
        true
    }
}

impl<M, B, X, V> Scld<M, B, X, V>
    where M: PagedMemory8k,
{
    #[inline]
    fn push_mode_change(&mut self, ts: VideoTs, render_mode: RenderMode) {
        self.mode_changes.push((ts, render_mode.bits()).into())
    }

    #[inline]
    fn change_border_color(&mut self, border: BorderColor, ts: VideoTs) {
        if self.ula.last_border != border {
            if !self.cur_ctrl_flags.is_screen_hi_res() {
                self.push_mode_change(ts,
                        RenderMode::with_color(RenderMode::empty(), border.into()));
            }
            self.ula.last_border = border;
        }
    }

    fn beg_render_mode(&self) -> RenderMode {
        let (flags, color) = if self.beg_ctrl_flags.is_screen_hi_res() {
            (RenderMode::HI_RESOLUTION, self.beg_ctrl_flags.hires_color_index())
        }
        else {
            (RenderMode::empty(), self.ula.border.into())
        };
        RenderMode::with_color(flags, color)
    }

    fn set_ctrl_flags_value(&mut self, flags: ScldCtrlFlags, ts: VideoTs) {
        let diff_flags = self.cur_ctrl_flags ^ flags;
        // mmu
        if self.mem_paged != 0 && diff_flags.is_map_ex_rom() {
            let map_ex_rom = flags.is_map_ex_rom();
            for page in 0..8u8 {
                let mask = 1 << page;
                if self.mem_paged & mask != 0 { // DOCK/EX-ROM
                    if map_ex_rom { // EX-ROM
                        self.ula.memory.map_rom_bank(8 + (page as usize) % (M::ROM_BANKS_MAX - 9), page)
                    }
                    else { // DOCK
                        self.ula.memory.map_rom_bank(page as usize, page)
                    }.unwrap();

                }
            }
        }
        // source mode
        if diff_flags.intersects(ScldCtrlFlags::SCREEN_SOURCE_MASK) {
            let source_mode = SourceMode::from_scld_flags(flags);
            self.source_changes.push((ts, source_mode.bits()).into());
        }
        // render mode
        if flags.is_screen_hi_res() {
            if diff_flags.intersects(ScldCtrlFlags::SCREEN_HI_RES|ScldCtrlFlags::HIRES_COLOR_MASK) {
                self.push_mode_change(ts, 
                    RenderMode::with_color(
                        RenderMode::HI_RESOLUTION, flags.hires_color_index()));
            }
        }
        else if diff_flags.is_screen_hi_res() {
            self.push_mode_change(ts, 
                RenderMode::with_color(
                    RenderMode::empty(), self.ula.last_border.into()));
        }

        self.cur_ctrl_flags = flags;
    }

    fn set_mmu_flags_value(&mut self, paged: u8) {
        let diff_pages = self.mem_paged ^ paged;
        if diff_pages == 0 {
            return;
        }
        let map_ex_rom = self.cur_ctrl_flags.is_map_ex_rom();
        for page in 0..8u8 {
            let mask = 1 << page;
            if diff_pages & mask != 0 {
                if paged & mask == 0 { // HOME
                    if page > 1 { // RAM
                        self.ula.memory.map_ram_bank(page as usize - 2, page)
                    }
                    else { // ROM
                        self.ula.memory.map_rom_bank(M::ROM_BANKS_MAX - 1 + page as usize, page)
                    }
                }
                else if map_ex_rom { // EX-ROM
                    self.ula.memory.map_rom_bank(8 + (page as usize) % (M::ROM_BANKS_MAX - 9), page)
                }
                else { // DOCK
                    self.ula.memory.map_rom_bank(page as usize, page)
                }.unwrap();
            }
        }
        self.mem_paged = paged;
    }
}

impl<M, B, X, V> MemoryAccess for Scld<M, B, X, V>
    where M: PagedMemory8k,
          X: MemoryExtension
{
    type Memory = M;
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

impl<M, B, X, V> ControlUnit for Scld<M, B, X, V>
    where M: PagedMemory8k,
          B: BusDevice,
          B::Timestamp: From<VFrameTs<V>>,
          X: MemoryExtension,
          V: VideoFrame
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
            self.mem_paged = 0;
            self.set_ctrl_flags_value(ScldCtrlFlags::empty(), self.current_video_ts());
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

impl<M, B, X, V> UlaControlExt for Scld<M, B, X, V>
    where M: PagedMemory8k,
          B: BusDevice,
          B::Timestamp: From<VFrameTs<V>>,
          V: VideoFrame
{
    fn prepare_next_frame<C: MemoryContention>(
            &mut self,
            vtsc: VFrameTsCounter<V, C>
        ) -> VFrameTsCounter<V, C>
    {
        self.beg_ctrl_flags = self.cur_ctrl_flags;
        self.sec_frame_cache.clear();
        self.mode_changes.clear();
        self.source_changes.clear();
        self.ula.prepare_next_frame(vtsc)
    }
}
