use crate::z80emu::{*, host::Result};
use crate::clock::{
    FTs, VideoTs,
    VideoTsData2, VideoTsData6,
    VFrameTsCounter, MemoryContention
};
use crate::bus::{BusDevice, NullDevice};
use crate::chip::{
    ScldCtrlFlags,
    EarIn, ReadEarMode, ControlUnit, MemoryAccess,
    ula::{
        Ula, UlaVideoFrame, UlaMemoryContention,
        UlaTimestamp, UlaCpuExt,
        frame_cache::UlaFrameCache
    }
};
use crate::memory::{
    PagedMemory8k, MemoryExtension, NoMemoryExtension,
};
use crate::video::{VideoFrame, BorderColor, RenderMode};

#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};

pub mod frame_cache;
pub mod audio_earmic;
pub mod io;
pub mod video;

use frame_cache::SourceMode;

/// Timex SCLD.
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "snapshot", serde(rename_all = "camelCase"))]
#[derive(Clone)]
pub struct Scld<M: PagedMemory8k,
                B=NullDevice<VideoTs>,
                X=NoMemoryExtension,
                V=UlaVideoFrame>
{
    #[cfg_attr(feature = "snapshot", serde(bound(
        deserialize = "Ula<M, B, X, V>: Deserialize<'de>"
    )))]
    ula: Ula<M, B, X, V>,
    beg_ctrl_flags: ScldCtrlFlags,
    cur_ctrl_flags: ScldCtrlFlags,
    mem_paged: u8,
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
          X: MemoryExtension
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Scld")
            .field("ula", &self.ula)
            .field("beg_ctrl_flags", &self.beg_ctrl_flags)
            .field("cur_ctrl_flags", &self.cur_ctrl_flags)
            .field("mem_paged", &self.mem_paged)
            .field("mode_changes", &self.mode_changes.len())
            .field("source_changes", &self.source_changes.len())
            .finish()
    }
}

impl<M, B, X, V> Scld<M, B, X, V>
    where M: PagedMemory8k,
          V: VideoFrame
{
    /// Sets the frame counter to the specified value.
    pub fn set_frame_counter(&mut self, fc: u64) {
        self.ula.set_frame_counter(fc)
    }
    /// Sets the T-state counter to the specified value modulo `V::FRAME_TSTATES_COUNT`.
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
    /// Returns the last value sent to the memory port `0xFF`.
    ///
    /// Usefull for creating snapshots.
    pub fn scld_ctrl_port_value(&self) -> ScldCtrlFlags {
        self.cur_ctrl_flags
    }
    /// Returns the last value sent to the memory port `0xF4`.
    ///
    /// Usefull for creating snapshots.
    pub fn scld_mmu_port_value(&self) -> u8 {
        self.mem_paged
    }

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

    fn change_ctrl_flag(&mut self, flags: ScldCtrlFlags, ts: VideoTs) {
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
            let source_mode = SourceMode::from(flags);
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

    fn change_mem_paged(&mut self, paged: u8) {
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
}

impl<M, B, X, V> ControlUnit for Scld<M, B, X, V>
    where M: PagedMemory8k,
          B: BusDevice<Timestamp=VideoTs>,
          X: MemoryExtension,
          V: VideoFrame,
          Ula<M, B, X, V>: ControlUnit<BusDevice=B>
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
            self.mem_paged = 0;
            self.change_ctrl_flag(ScldCtrlFlags::empty(), self.ula.video_ts());
        }
    }

    fn nmi<C: Cpu>(&mut self, cpu: &mut C) -> bool {
        self.ula_nmi::<UlaMemoryContention, _>(cpu)
    }

    fn execute_next_frame<C: Cpu>(&mut self, cpu: &mut C) {
        while !self.ula_execute_next_frame_with_breaks::<UlaMemoryContention, _>(cpu) {}
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
        self.ula_execute_single_step::<UlaMemoryContention,_,_>(cpu, debug)
    }
}

impl<M, B, X, V> Scld<M, B, X, V>
    where M: PagedMemory8k,
          B: BusDevice<Timestamp=VideoTs>,
          V: VideoFrame
{
    #[inline]
    fn prepare_next_frame<T: MemoryContention>(
            &mut self,
            vtsc: VFrameTsCounter<V, T>
        ) -> VFrameTsCounter<V, T>
    {
        self.beg_ctrl_flags = self.cur_ctrl_flags;
        self.sec_frame_cache.clear();
        self.mode_changes.clear();
        self.source_changes.clear();
        self.ula.prepare_next_frame(vtsc)
    }

}

impl<M, B, X, V> UlaTimestamp for Scld<M, B, X, V>
    where M: PagedMemory8k,
          B: BusDevice<Timestamp=VideoTs>,
          V: VideoFrame
{
    type VideoFrame = V;

    #[inline]
    fn video_ts(&self) -> VideoTs {
        self.ula.video_ts()
    }

    #[inline]
    fn set_video_ts(&mut self, vts: VideoTs) {
        self.ula.set_video_ts(vts)
    }

    fn ensure_next_frame_vtsc<T: MemoryContention>(
            &mut self
        ) -> VFrameTsCounter<V, T>
    {
        let mut vtsc = VFrameTsCounter::from(self.ula.tsc);
        if vtsc.is_eof() {
            vtsc = self.prepare_next_frame(vtsc);
        }
        vtsc
    }
}
