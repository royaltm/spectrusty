mod audio;
mod earmic;
pub(crate) mod frame_cache;
mod io;
mod video;

use core::num::Wrapping;

#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

use crate::z80emu::{*, host::{Result, cycles::M1_CYCLE_TS}};
#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};

use crate::audio::{AudioFrame, Blep, EarInAudioFrame, EarMicOutAudioFrame};
#[cfg(feature = "peripherals")] use crate::peripherals::ay::audio::AyAudioFrame;

use crate::bus::{BusDevice, NullDevice};
use crate::chip::{ControlUnit, MemoryAccess, EarIn, MicOut, nanos_from_frame_tc_cpu_hz};
use crate::video::{Video, VideoFrame};
use crate::memory::{ZxMemory, MemoryExtension, NoMemoryExtension};
use crate::peripherals::{KeyboardInterface, ZXKeyboardMap};
use crate::clock::{
    HALT_VC_THRESHOLD, VideoTs, FTs, Ts, VFrameTsCounter,
    MemoryContention, VideoTsData1, VideoTsData2, VideoTsData3};
use frame_cache::UlaFrameCache;

pub use video::{UlaVideoFrame, UlaMemoryContention, UlaTsCounter};

/// The ZX Spectrum's CPU clock in cycles per second.
pub const CPU_HZ: u32 = 3_500_000;

/// A grouping trait of all common control traits for all emulated `Ula` chipsets except audio rendering.
///
/// For audio rendering see [UlaAudioFrame].
pub trait UlaCommon: ControlUnit +
                     MemoryAccess +
                     Video +
                     KeyboardInterface +
                     EarIn +
                     for<'a> MicOut<'a> {}

impl<U> UlaCommon for U
    where U: ControlUnit + MemoryAccess + Video + KeyboardInterface + EarIn + for<'a> MicOut<'a>
{}

/// A grouping trait of common audio rendering traits for all emulated `Ula` chipsets.
#[cfg(feature = "peripherals")] pub trait UlaAudioFrame<B: Blep>: AudioFrame<B> +
                                  EarMicOutAudioFrame<B> +
                                  EarInAudioFrame<B> +
                                  AyAudioFrame<B> {}
#[cfg(not(feature = "peripherals"))] pub trait UlaAudioFrame<B: Blep>: AudioFrame<B> +
                                  EarMicOutAudioFrame<B> +
                                  EarInAudioFrame<B> {}
#[cfg(feature = "peripherals")]
impl<B: Blep, U> UlaAudioFrame<B> for U
    where U: AudioFrame<B> + EarMicOutAudioFrame<B> + EarInAudioFrame<B> + AyAudioFrame<B>
{}
#[cfg(not(feature = "peripherals"))]
impl<B: Blep, U> UlaAudioFrame<B> for U
    where U: AudioFrame<B> + EarMicOutAudioFrame<B> + EarInAudioFrame<B>
{}

// #[derive(Clone)]
// pub struct SnowyUla<M, B> {
//     pub ula: Ula<M, B>
// }

/// ZX Spectrum 16k/48k ULA.
#[derive(Clone)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "snapshot", serde(rename_all = "camelCase"))]
pub struct Ula<M, B=NullDevice<VideoTs>, X=NoMemoryExtension, V=UlaVideoFrame> {
    pub(super) frames: Wrapping<u64>, // frame counter
    pub(super) tsc: VideoTs, // current T-state timestamp
    pub(super) memory: M,
    pub(super) bus: B,
    pub(super) memext: X,
    // keyboard
    #[cfg_attr(feature = "snapshot", serde(skip))]
    keyboard: ZXKeyboardMap,
    // video related
    #[cfg_attr(feature = "snapshot", serde(skip))]
    pub(super) frame_cache: UlaFrameCache<V>,
    #[cfg_attr(feature = "snapshot", serde(skip))]
    border_out_changes: Vec<VideoTsData3>, // frame timestamp with packed border on 3 bits
    #[cfg_attr(feature = "snapshot", serde(skip))]
    earmic_out_changes: Vec<VideoTsData2>, // frame timestamp with packed earmic on 2 bits
    #[cfg_attr(feature = "snapshot", serde(skip))]
    ear_in_changes: Vec<VideoTsData1>,  // frame timestamp with packed earin on 1 bit
    border: u8, // video frame start border color
    last_border: u8, // last recorded change
    // audio related
    sample_rate: u32,
    prev_ear_in: u8,
    ear_in_last_index: usize,
    prev_earmic_ts: FTs, // prev recorded change timestamp
    prev_earmic_data: u8, // prev recorded change data
    last_earmic_data: u8, // last recorded change data
}

impl<M, B, X, V> Default for Ula<M, B, X, V>
where M: Default,
      B: Default,
      X: Default
{
    fn default() -> Self {
        Ula {
            frames: Wrapping(0),   // frame counter
            tsc: VideoTs::default(),
            memory: M::default(),
            bus: B::default(),
            memext: X::default(),
            // video related
            frame_cache: Default::default(),
            border_out_changes: Vec::new(),
            earmic_out_changes: Vec::new(),
            ear_in_changes:  Vec::new(),
            // read_ear: MaybeReadEar(None),
            border: 7, // video frame start border color
            last_border: 7, // last changed border color
            // audio related
            sample_rate: 0,
            prev_ear_in: 0,
            ear_in_last_index: 0,
            prev_earmic_ts: FTs::min_value(),
            prev_earmic_data: 0,
            last_earmic_data: 0,
            // keyboard
            keyboard: ZXKeyboardMap::empty(),
        }
    }
}

impl<M, B, X, V> core::fmt::Debug for Ula<M, B, X, V> where M: ZxMemory
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Ula {{ frames: {:?}, tsc: {:?}, border: {} border_changes: {} earmic_changes: {} }}",
            self.frames, self.tsc, self.last_border, self.border_out_changes.len(), self.earmic_out_changes.len())
    }
}

impl<M, B, X> ControlUnit for Ula<M, B, X, UlaVideoFrame>
    where M: ZxMemory, B: BusDevice<Timestamp=VideoTs>, X: MemoryExtension
{
    type BusDevice = B;

    fn cpu_clock_rate(&self) -> u32 {
        CPU_HZ
    }

    fn frame_duration_nanos(&self) -> u32 {
        nanos_from_frame_tc_cpu_hz(UlaVideoFrame::FRAME_TSTATES_COUNT as u32, CPU_HZ) as u32
    }

    fn bus_device_mut(&mut self) -> &mut Self::BusDevice {
        &mut self.bus
    }

    fn bus_device_ref(&self) -> &Self::BusDevice {
        &self.bus
    }

    fn current_frame(&self) -> u64 {
        self.frames.0
    }

    fn frame_tstate(&self) -> (u64, FTs) {
        UlaVideoFrame::vts_to_norm_tstates(self.tsc, self.frames.0)
    }

    fn current_tstate(&self) -> FTs {
        UlaVideoFrame::vts_to_tstates(self.tsc)
    }

    fn is_frame_over(&self) -> bool {
        UlaVideoFrame::is_vts_eof(self.tsc)
    }

    fn reset<C: Cpu>(&mut self, cpu: &mut C, hard: bool) {
        self.ula_reset::<UlaMemoryContention, _>(cpu, hard)
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
    /// Returns a reference to the memory.
    #[inline(always)]
    fn memory_ref(&self) -> &Self::Memory {
        &self.memory
    }
}

impl<M, B, X, V> Ula<M, B, X, V>
    where M: ZxMemory, B: BusDevice<Timestamp=VideoTs>, V: VideoFrame
{
    #[inline]
    pub(super) fn prepare_next_frame<T: MemoryContention>(
            &mut self,
            mut vtsc: VFrameTsCounter<V, T>
        ) -> VFrameTsCounter<V, T>
    {
        self.bus.next_frame(vtsc.as_timestamp());
        self.frames += Wrapping(1);
        self.cleanup_video_frame_data();
        self.cleanup_earmic_frame_data();
        vtsc.wrap_frame();
        self.tsc = vtsc.into();
        vtsc
    }
}

pub(super) trait UlaTimestamp {
    type VideoFrame: VideoFrame;
    fn video_ts(&self) -> VideoTs;
    fn set_video_ts(&mut self, vts: VideoTs);
    fn ensure_next_frame_vtsc<T: MemoryContention>(&mut self) -> VFrameTsCounter<Self::VideoFrame, T>;
}

impl<M, B, X, V> UlaTimestamp for Ula<M, B, X, V>
    where M: ZxMemory, B: BusDevice<Timestamp=VideoTs>, V: VideoFrame
{
    type VideoFrame = V;
    #[inline]
    fn video_ts(&self) -> VideoTs {
        self.tsc
    }

    #[inline]
    fn set_video_ts(&mut self, vts: VideoTs) {
        self.tsc = vts
    }

    fn ensure_next_frame_vtsc<T: MemoryContention>(
            &mut self
        ) -> VFrameTsCounter<V, T>
    {
        let mut vtsc = VFrameTsCounter::from(self.tsc);
        if vtsc.is_eof() {
            vtsc = self.prepare_next_frame(vtsc);
        }
        vtsc
    }
}

pub(super) trait UlaCpuExt: UlaTimestamp {
    fn ula_reset<T: MemoryContention, C: Cpu>(&mut self, cpu: &mut C, hard: bool);
    fn ula_nmi<T: MemoryContention, C: Cpu>(&mut self, cpu: &mut C) -> bool;
    fn ula_execute_next_frame_with_breaks<T: MemoryContention, C: Cpu>(
            &mut self,
            cpu: &mut C
        ) -> bool;
    fn ula_execute_single_step<T: MemoryContention, C: Cpu, F: FnOnce(CpuDebug)>(
            &mut self,
            cpu: &mut C,
            debug: Option<F>
        ) -> Result<(),()>;
    fn ula_execute_instruction<T: MemoryContention, C: Cpu>(
            &mut self,
            cpu: &mut C,
            code: u8
        ) -> Result<(), ()>;

    #[inline]
    fn ula_check_halt<C: Cpu>(mut vts: VideoTs, cpu: &mut C) -> VideoTs {
        if vts.vc >= HALT_VC_THRESHOLD {
            debug!("HALT FOREVER: {:?}", vts);
            cpu.disable_interrupts();
            cpu.halt();
            vts.vc = (vts.vc - HALT_VC_THRESHOLD).max(Self::VideoFrame::VSL_COUNT);
        }
        vts
    }
}

impl<U, B, X> UlaCpuExt for U
    where U: UlaTimestamp +
             ControlUnit<BusDevice=B> +
             MemoryAccess<MemoryExt=X> +
             Memory<Timestamp=VideoTs> +
             Io<Timestamp=VideoTs, WrIoBreak=(), RetiBreak=()>,
          B: BusDevice<Timestamp=VideoTs>,
          X: MemoryExtension
{
    fn ula_reset<T: MemoryContention, C: Cpu>(&mut self, cpu: &mut C, hard: bool)
    {
        if hard {
            cpu.reset();
            let vts = self.video_ts();
            self.bus_device_mut().reset(vts);
            self.memory_mut().reset();
        }
        else {
            self.ula_execute_instruction::<T,_>(cpu, opconsts::RST_00H_OPCODE).unwrap();
        }
    }

    fn ula_nmi<T: MemoryContention, C: Cpu>(&mut self, cpu: &mut C) -> bool
    {
        let mut vtsc = self.ensure_next_frame_vtsc::<T>();
        let res = cpu.nmi(self, &mut vtsc);
        self.set_video_ts(vtsc.into());
        res
    }

    fn ula_execute_next_frame_with_breaks<T: MemoryContention, C: Cpu>(
            &mut self,
            cpu: &mut C
        ) -> bool
        where Self: Memory<Timestamp=VideoTs> + Io<Timestamp=VideoTs>
    {
        let mut vtsc = self.ensure_next_frame_vtsc::<T>();
        loop {
            match cpu.execute_with_limit(self, &mut vtsc, Self::VideoFrame::VSL_COUNT) {
                Ok(()) => {
                    *vtsc = Self::ula_check_halt(vtsc.into(), cpu);
                    break
                },
                Err(BreakCause::Halt) => {
                    *vtsc = execute_halted_state_until_eof::<Self::VideoFrame,T,_>(vtsc.into(), cpu);
                    break
                }
                Err(_) => {
                    *vtsc = Self::ula_check_halt(vtsc.into(), cpu);
                    if vtsc.is_eof() {
                        break
                    }
                    else {
                        self.set_video_ts(vtsc.into());
                        return false
                    }
                }
            }
        }
        let vts = vtsc.into();
        self.set_video_ts(vts);
        self.bus_device_mut().update_timestamp(vts);
        true
    }

    fn ula_execute_single_step<T: MemoryContention, C: Cpu, F>(
            &mut self,
            cpu: &mut C,
            debug: Option<F>
        ) -> Result<(),()>
        where F: FnOnce(CpuDebug),
              Self: Memory<Timestamp=VideoTs> + Io<Timestamp=VideoTs, WrIoBreak=(), RetiBreak=()>
    {
        let mut vtsc = self.ensure_next_frame_vtsc::<T>();
        let res = cpu.execute_next(self, &mut vtsc, debug);
        *vtsc = Self::ula_check_halt(vtsc.into(), cpu);
        self.set_video_ts(vtsc.into());
        res
    }

    fn ula_execute_instruction<T: MemoryContention, C: Cpu>(
            &mut self,
            cpu: &mut C,
            code: u8
        ) -> Result<(), ()>
        where Self: Memory<Timestamp=VideoTs> + Io<Timestamp=VideoTs, WrIoBreak=(), RetiBreak=()>
    {
        const DEBUG: Option<CpuDebugFn> = None;
        let mut vtsc = self.ensure_next_frame_vtsc::<T>();
        let res = cpu.execute_instruction(self, &mut vtsc, DEBUG, code);
        *vtsc = Self::ula_check_halt(vtsc.into(), cpu);
        self.set_video_ts(vtsc.into());
        res
    }
}

/// Returns with a VideoTs at the frame interrupt and with cpu refresh register set accordingly.
/// The VideoTs passed here must be normalized.
pub fn execute_halted_state_until_eof<V: VideoFrame,
                                      M: MemoryContention,
                                      C: Cpu>(
            mut tsc: VideoTs,
            cpu: &mut C
        ) -> VideoTs
{
    debug_assert_eq!(0, V::HTS_COUNT % M1_CYCLE_TS as Ts);
    let mut r_incr: i32 = 0;
    if M::is_contended_address(cpu.get_pc()) && tsc.vc < V::VSL_PIXELS.end {
        let VideoTs { mut vc, mut hc } = tsc; // assume tsc is normalized
        // move hc to the beginning of range
        if vc < V::VSL_PIXELS.start { // border top
            let hc_end = V::HTS_RANGE.end + (hc - V::HTS_RANGE.end).rem_euclid(M1_CYCLE_TS as Ts);
            vc += 1;
            r_incr = (i32::from(V::VSL_PIXELS.start - vc) * V::HTS_COUNT as i32 +
                      i32::from(hc_end - hc)) / M1_CYCLE_TS as i32;
            hc = hc_end - V::HTS_COUNT;
            vc = V::VSL_PIXELS.start;
        }
        else {
            while hc < V::HTS_RANGE.end {
                hc = V::contention(hc) + M1_CYCLE_TS as Ts;
                r_incr += 1;
            }
            vc += 1;
            hc -= V::HTS_COUNT;
        }
        // contended area
        if vc < V::VSL_PIXELS.end {
            let mut r_line = 0; // calculate an R increase for a whole single line
            while hc < V::HTS_RANGE.end {
                hc = V::contention(hc) + M1_CYCLE_TS as Ts;
                r_line += 1;
            }
            hc -= V::HTS_COUNT;
            r_incr += i32::from(V::VSL_PIXELS.end - vc) * r_line;
        }
        // bottom border
        tsc.vc = V::VSL_PIXELS.end;
        tsc.hc = hc;
    }
    let vc = V::VSL_COUNT;
    let hc = tsc.hc.rem_euclid(M1_CYCLE_TS as Ts);
    r_incr += (i32::from(vc - tsc.vc) * V::HTS_COUNT as i32 +
               i32::from(hc - tsc.hc)) / M1_CYCLE_TS as i32;
    tsc.hc = hc;
    tsc.vc = vc;
    if r_incr >= 0 {
        cpu.add_r(r_incr);
    }
    else {
        unreachable!();
    }
    tsc
}


#[cfg(test)]
mod tests {
    use core::convert::TryFrom;
    use crate::z80emu::opconsts::HALT_OPCODE;
    use crate::memory::Memory64k;
    use super::*;
    type TestUla = Ula::<Memory64k>;

    #[test]
    fn test_ula() {
        let ula = TestUla::default();
        assert_eq!(<TestUla as Video>::VideoFrame::FRAME_TSTATES_COUNT, 69888);
        assert_eq!(ula.cpu_clock_rate(), CPU_HZ);
        assert_eq!(ula.cpu_clock_rate(), 3_500_000);
        assert_eq!(ula.frame_duration_nanos(), u32::try_from(69888u64 * 1_000_000_000 / 3_500_000).unwrap());
    }

    fn test_ula_contended_halt(addr: u16, vc: Ts, hc: Ts) {
        let mut ula = TestUla::default();
        ula.tsc.vc = vc;
        ula.tsc.hc = hc;
        ula.memory.write(addr, HALT_OPCODE);
        let mut cpu = Z80NMOS::default();
        cpu.reset();
        cpu.set_pc(addr);
        let mut cpu1 = cpu.clone();
        let mut ula1 = ula.clone();
        assert_eq!(cpu.is_halt(), false);
        ula.execute_next_frame(&mut cpu);
        assert_eq!(cpu.is_halt(), true);
        // chek without execute_halted_state_until_eof
        assert_eq!(cpu1.is_halt(), false);
        let mut tsc1 = ula1.ensure_next_frame_vtsc::<UlaMemoryContention>();
        let mut was_halt = false;
        loop {
            match cpu1.execute_with_limit(&mut ula1, &mut tsc1, UlaVideoFrame::VSL_COUNT) {
                Ok(()) => break,
                Err(BreakCause::Halt) => {
                    if was_halt {
                        panic!("must not halt again");
                    }
                    was_halt = true;
                    continue
                },
                Err(_) => unreachable!()
            }
        }
        assert_eq!(cpu1.is_halt(), true);
        assert_eq!(was_halt, true);
        while tsc1.tsc.hc < 0 {
            match cpu1.execute_next::<_,_,CpuDebugFn>(&mut ula1, &mut tsc1, None) {
                Ok(()) => (),
                Err(_) => unreachable!()
            }
        }
        assert_eq!(tsc1.tsc, ula.tsc);
        // println!("{:?}", tsc1.tsc);
        assert_eq!(cpu1, cpu);
        // println!("{:?}", cpu1);
        // println!("=================================");
    }

    #[test]
    fn ula_works() {
        for vc in 0..UlaVideoFrame::VSL_COUNT {
            // println!("vc: {:?}", vc);
            for hc in UlaVideoFrame::HTS_RANGE {
                test_ula_contended_halt(0, vc, hc);
                test_ula_contended_halt(0x4000, vc, hc);
            }
        }
    }
}
