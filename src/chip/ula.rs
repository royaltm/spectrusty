/*
    Copyright (C) 2020-2022  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
//! An emulator of Sinclair Uncommitted Logic Array chip for ZX Spectrum 16k/48k PAL/NTSC.
#![macro_use]
use core::fmt;

mod audio;
mod earmic;
pub mod frame_cache;
mod io;
mod video;
mod video_ntsc;
mod plus;
mod cpuext;
#[cfg(feature = "formats")]
mod screen;

use core::num::Wrapping;

#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

use crate::z80emu::{*, host::Result};
#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};

use crate::bus::{BusDevice, VFNullDevice};
use crate::chip::{
    UlaControl, FrameState, ControlUnit, MemoryAccess, EarMic, ReadEarMode
};
use crate::video::{BorderColor, VideoFrame};
use crate::memory::{ZxMemory, MemoryExtension, NoMemoryExtension};
use crate::peripherals::ZXKeyboardMap;
use crate::clock::{
    FTs, VFrameTs, VFrameTsCounter, MemoryContention,
    VideoTsData1, VideoTsData2, VideoTsData3
};
use frame_cache::UlaFrameCache;

pub use cpuext::*;
pub use video::UlaVideoFrame;
pub use video_ntsc::UlaNTSCVidFrame;

/// NTSC 16k/48k ULA (Uncommitted Logic Array).
pub type UlaNTSC<M, B=VFNullDevice<UlaNTSCVidFrame>, X=NoMemoryExtension> = Ula<M, B, X, UlaNTSCVidFrame>;
/// PAL 16k/48k ULA (Uncommitted Logic Array).
pub type UlaPAL<M, B=VFNullDevice<UlaVideoFrame>, X=NoMemoryExtension> = Ula<M, B, X, UlaVideoFrame>;

/// A struct implementing [MemoryContention] for addresses in the range: [0x4000, 0x7FFF] being contended.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct UlaMemoryContention;

/// Generic 16k/48k Ferranti ULA (Uncommitted Logic Array).
///
/// * `M` - [ZxMemory]
/// * `B` - [BusDevice]
/// * `X` - [MemoryExtension]
/// * `V` - [VideoFrame]
///
/// The type used for [`<B as BusDevice>::Timestamp`][BusDevice::Timestamp] should at least
/// satisfy the condition: `From<VFrameTs<V>>`.
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "snapshot", serde(rename_all = "camelCase"))]
#[derive(Clone)]
pub struct Ula<M, B, X, V> {
    pub(super) frames: Wrapping<u64>, // frame counter
    #[cfg_attr(feature = "snapshot", serde(bound = "V: VideoFrame"))]
    pub(super) tsc: VFrameTs<V>, // current T-state timestamp
    pub(super) memory: M,
    pub(super) bus: B,
    // #[cfg_attr(feature = "snapshot", serde(default))]
    pub(super) memext: X,
    // keyboard
    #[cfg_attr(feature = "snapshot", serde(skip))]
    keyboard: ZXKeyboardMap,
    read_ear_mode: ReadEarMode,
    late_timings: bool,
    // video related
    #[cfg(feature = "boxed_frame_cache")]
    #[cfg_attr(feature = "snapshot", serde(skip))]
    pub(super) frame_cache: Box<UlaFrameCache<V>>,

    #[cfg(not(feature = "boxed_frame_cache"))]
    #[cfg_attr(feature = "snapshot", serde(skip))]
    pub(super) frame_cache: UlaFrameCache<V>,

    #[cfg_attr(feature = "snapshot", serde(skip))]
    border_out_changes: Vec<VideoTsData3>, // frame timestamp with packed border on 3 bits
    pub(super) border: BorderColor, // video frame start border color
    pub(super) last_border: BorderColor, // last recorded change
    // EAR, MIC
    #[cfg_attr(feature = "snapshot", serde(skip))]
    ear_in_changes: Vec<VideoTsData1>,  // frame timestamp with packed earin on 1 bit
    prev_ear_in: bool, // EAR IN state before first change in ear_in_changes
    ear_in_last_index: usize, // index into ear_in_changes of the last probed EAR IN
    read_ear_in_count: Wrapping<u32>, // the number of EAR IN probes during the last frame
    #[cfg_attr(feature = "snapshot", serde(skip))]
    earmic_out_changes: Vec<VideoTsData2>, // frame timestamp with packed earmic on 2 bits
    prev_earmic_ts: FTs, // previously recorded change timestamp
    prev_earmic_data: EarMic, // previous frame last recorded data
    last_earmic_data: EarMic, // last recorded data
}

impl MemoryContention for UlaMemoryContention {
    #[inline(always)]
    fn is_contended_address(self, address: u16) -> bool {
        address & 0xC000 == 0x4000
    }
}

impl<M, B, X, V: VideoFrame> FrameState for Ula<M, B, X, V> {
    fn current_frame(&self) -> u64 {
        self.frames.0
    }

    fn set_frame_counter(&mut self, fc: u64) {
        self.frames = Wrapping(fc);
    }

    fn frame_tstate(&self) -> (u64, FTs) {
        self.tsc.into_frame_tstates(self.frames.0)
    }

    fn current_tstate(&self) -> FTs {
        self.tsc.into_tstates()
    }

    fn set_frame_tstate(&mut self, ts: FTs) {
        let ts = ts.rem_euclid(V::FRAME_TSTATES_COUNT);
        let tsc = VFrameTs::from_tstates(ts);
        self.tsc = tsc
    }

    fn is_frame_over(&self) -> bool {
        self.tsc.is_eof()
    }
}

impl<M, B, X, V> UlaControl for Ula<M, B, X, V> {
    fn has_late_timings(&self) -> bool {
        self.late_timings
    }

    fn set_late_timings(&mut self, late_timings: bool) {
        self.late_timings = late_timings;
    }
}

impl<M, B, X, V> Default for Ula<M, B, X, V>
where M: Default,
      B: Default,
      X: Default
{
    fn default() -> Self {
        Ula {
            frames: Wrapping(0),   // frame counter
            tsc: VFrameTs::default(),
            memory: M::default(),
            bus: B::default(),
            memext: X::default(),
            // keyboard
            keyboard: ZXKeyboardMap::empty(),
            read_ear_mode: ReadEarMode::Issue3,
            late_timings: false,
            // video related
            frame_cache: Default::default(),
            border_out_changes: Vec::new(),
            border: BorderColor::WHITE, // video frame start border color
            last_border: BorderColor::WHITE, // last changed border color
            // EAR, MIC
            ear_in_changes:  Vec::new(),
            prev_ear_in: false,
            ear_in_last_index: 0,
            read_ear_in_count: Wrapping(0),
            earmic_out_changes: Vec::new(),
            prev_earmic_ts: FTs::min_value(),
            prev_earmic_data: EarMic::empty(),
            last_earmic_data: EarMic::empty(),
        }
    }
}

impl<M, B, X, V> fmt::Debug for Ula<M, B, X, V>
    where M: ZxMemory,
          B: BusDevice,
          X: MemoryExtension,
          V: VideoFrame
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Ula")
            .field("frames", &self.frames.0)
            .field("tsc", &self.tsc)
            .field("memory", &self.memory.mem_ref().len())
            .field("bus", &self.bus)
            .field("memext", &self.memext)
            .field("keyboard", &self.keyboard)
            .field("read_ear_mode", &self.read_ear_mode)
            .field("late_timings", &self.late_timings)
            .field("frame_cache", &self.frame_cache)
            .field("border_out_changes", &self.border_out_changes.len())
            .field("border", &self.border)
            .field("last_border", &self.last_border)
            .field("prev_ear_in", &self.prev_ear_in)
            .field("ear_in_changes", &self.ear_in_changes.len())
            .field("read_ear_in_count", &self.read_ear_in_count.0)
            .field("earmic_out_changes", &self.earmic_out_changes.len())
            .field("prev_earmic_data", &self.prev_earmic_data)
            .field("last_earmic_data", &self.last_earmic_data)
            .finish()
    }
}

impl<M, B, X, V> MemoryAccess for Ula<M, B, X, V>
    where M: ZxMemory, X: MemoryExtension
{
    type Memory = M;
    type MemoryExt = X;

    #[inline(always)]
    fn memory_ext_ref(&self) -> &Self::MemoryExt {
        &self.memext
    }
    #[inline(always)]
    fn memory_ext_mut(&mut self) -> &mut Self::MemoryExt {
        &mut self.memext
    }
    #[inline(always)]
    fn memory_mut(&mut self) -> &mut Self::Memory {
        &mut self.memory
    }
    #[inline(always)]
    fn memory_ref(&self) -> &Self::Memory {
        &self.memory
    }

    fn memory_with_ext_mut(&mut self) -> (&mut Self::Memory, &mut Self::MemoryExt) {
        (&mut self.memory, &mut self.memext)
    }
}

impl<M, B, X, V> ControlUnit for Ula<M, B, X, V>
    where M: ZxMemory,
          B: BusDevice,
          B::Timestamp: From<VFrameTs<V>>,
          X: MemoryExtension,
          V: VideoFrame
{
    type BusDevice = B;

    fn bus_device_mut(&mut self) -> &mut Self::BusDevice {
        &mut self.bus
    }

    fn bus_device_ref(&self) -> &Self::BusDevice {
        &self.bus
    }

    fn into_bus_device(self) -> Self::BusDevice {
        self.bus
    }

    fn reset<C: Cpu>(&mut self, cpu: &mut C, hard: bool) {
        if hard {
            cpu.reset();
            self.bus.reset(self.tsc.into());
            self.memory.reset();
        }
        else {
            const DEBUG: Option<CpuDebugFn> = None;
            let mut vtsc = VFrameTsCounter::from_vframe_ts(VFrameTs::<V>::default(), UlaMemoryContention);
            let _ = cpu.execute_instruction(self, &mut vtsc, DEBUG, opconsts::RST_00H_OPCODE);
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

impl<M, B, X, V> UlaControlExt for Ula<M, B, X, V>
    where M: ZxMemory,
          B: BusDevice,
          B::Timestamp: From<VFrameTs<V>>,
          V: VideoFrame
{
    fn prepare_next_frame<C: MemoryContention>(
            &mut self,
            mut vtsc: VFrameTsCounter<V, C>
        ) -> VFrameTsCounter<V, C>
    {
        self.bus.next_frame(VFrameTs::<V>::EOF.into());
        self.frames += Wrapping(1);
        self.cleanup_video_frame_data();
        self.cleanup_earmic_frame_data();
        vtsc.wrap_frame();
        self.tsc = vtsc.into();
        vtsc
    }
}

#[cfg(test)]
mod tests {
    use crate::memory::Memory64k;
    use crate::video::Video;
    use super::*;
    type TestUla = UlaPAL::<Memory64k>;

    #[test]
    fn test_ula() {
        assert_eq!(<TestUla as Video>::VideoFrame::FRAME_TSTATES_COUNT, 69888);
        let ula = TestUla::default();
        let clock = ula.current_video_clock();
        for addr in 0x4000..0x8000 {
            assert_eq!(clock.is_contended_address(addr), true);
        }
        for addr in (0x0000..0x4000).chain(0x8000..=0xFFFF) {
            assert_eq!(clock.is_contended_address(addr), false);
        }
    }
}
