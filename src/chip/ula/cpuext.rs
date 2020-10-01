/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

use crate::z80emu::{
    Cpu, CpuDebug, CpuDebugFn, Memory, Io, BreakCause,
    host::{
        cycles::M1_CYCLE_TS, Result
    }
};
use crate::bus::BusDevice;
use crate::chip::{MemoryAccess, ControlUnit};
use crate::clock::{
    HALT_VC_THRESHOLD,
    VideoTs, VFrameTs, Ts, VFrameTsCounter, MemoryContention
};
use crate::memory::MemoryExtension;
use crate::video::{Video, VideoFrame};

pub trait UlaControlExt: Video {
    /// This method is used by wrappers with a different contention scheme.
    fn prepare_next_frame<C: MemoryContention>(
            &mut self,
            vtsc: VFrameTsCounter<Self::VideoFrame, C>
        ) -> VFrameTsCounter<Self::VideoFrame, C>;

    fn ensure_next_frame_vtsc(&mut self) -> VFrameTsCounter<Self::VideoFrame, Self::Contention> {
        let mut vtsc = self.current_video_clock();
        if vtsc.is_eof() {
            vtsc = self.prepare_next_frame(vtsc);
        }
        vtsc
    }
}

pub(crate) trait UlaCpuExt: UlaControlExt {
    fn ula_nmi<C: Cpu>(&mut self, cpu: &mut C) -> bool;
    fn ula_execute_next_frame_with_breaks<C: Cpu>(
            &mut self,
            cpu: &mut C
        ) -> bool;
    fn ula_execute_single_step<C: Cpu, F: FnOnce(CpuDebug)>(
            &mut self,
            cpu: &mut C,
            debug: Option<F>
        ) -> Result<(),()>;
    fn ula_execute_instruction<C: Cpu>(
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
    where U: UlaControlExt +
             ControlUnit<BusDevice=B> +
             MemoryAccess<MemoryExt=X> +
             Memory<Timestamp=VideoTs> +
             Io<Timestamp=VideoTs, WrIoBreak=(), RetiBreak=()>,
          B: BusDevice<Timestamp=VFrameTs<U::VideoFrame>>,
          X: MemoryExtension
{
    fn ula_nmi<C: Cpu>(&mut self, cpu: &mut C) -> bool {
        let mut vtsc = self.ensure_next_frame_vtsc();
        let res = cpu.nmi(self, &mut vtsc);
        self.set_video_ts(vtsc.into());
        self.bus_device_mut().update_timestamp(vtsc.into());
        res
    }

    fn ula_execute_next_frame_with_breaks<C: Cpu>(
            &mut self,
            cpu: &mut C
        ) -> bool
    {
        let mut vtsc = self.ensure_next_frame_vtsc();
        while !vtsc.is_eof() {
            let vc_limit = if vtsc.vc < 1 { 1 }
            else {
                Self::VideoFrame::VSL_COUNT
            };
            match cpu.execute_with_limit(self, &mut vtsc, vc_limit) {
                Ok(()) => {
                    **vtsc = Self::ula_check_halt(vtsc.into(), cpu);
                },
                Err(BreakCause::Halt) => {
                    if vtsc.vc < 1 {
                        // if before frame interrupt
                        continue
                    }
                }
                Err(_) => {
                    **vtsc = Self::ula_check_halt(vtsc.into(), cpu);
                    if vtsc.is_eof() {
                        break
                    }
                    self.set_video_ts(vtsc.into());
                    return false
                }
            }
            if cpu.is_halt() {
                vtsc = execute_halted_state_until_eof(vtsc, cpu);
                break;
            }
        }
        self.set_video_ts(vtsc.into());
        self.bus_device_mut().update_timestamp(vtsc.into());
        true
    }

    fn ula_execute_single_step<C: Cpu, F>(
            &mut self,
            cpu: &mut C,
            debug: Option<F>
        ) -> Result<(),()>
        where F: FnOnce(CpuDebug),
    {
        let mut vtsc = self.ensure_next_frame_vtsc();
        let res = cpu.execute_next(self, &mut vtsc, debug);
        **vtsc = Self::ula_check_halt(vtsc.into(), cpu);
        self.set_video_ts(vtsc.into());
        self.bus_device_mut().update_timestamp(vtsc.into());
        res
    }

    fn ula_execute_instruction<C: Cpu>(
            &mut self,
            cpu: &mut C,
            code: u8
        ) -> Result<(), ()>
    {
        const DEBUG: Option<CpuDebugFn> = None;
        let mut vtsc = self.ensure_next_frame_vtsc();
        let res = cpu.execute_instruction(self, &mut vtsc, DEBUG, code);
        **vtsc = Self::ula_check_halt(vtsc.into(), cpu);
        self.set_video_ts(vtsc.into());
        self.bus_device_mut().update_timestamp(vtsc.into());
        res
    }
}

/// Emulates the CPU's halted state at the given video timestamp.
///
/// Returns a video timestamp set to just before the end of current frame and with the `cpu` memory refresh
/// register increased accordingly, applying any needed memory contention if the `PC` register addresses
/// a contended memory.
///
/// This method should be called after the `cpu` executed the `HALT` instruction for the optimal emulator
/// performance.
///
/// # Panics
///
/// The timestamp - `vtsc` passed here must be normalized and its vertical component must be positive and
/// its composite value must be less than [V::FRAME_TSTATES_COUNT][VideoFrame::FRAME_TSTATES_COUNT].
/// Otherwise this method panics.
pub fn execute_halted_state_until_eof<V, T, C>(
        mut vtsc: VFrameTsCounter<V, T>,
        cpu: &mut C
    ) -> VFrameTsCounter<V, T>
    where V: VideoFrame,
          T: MemoryContention,
          C: Cpu
{
    debug_assert_eq!(0, V::HTS_COUNT % M1_CYCLE_TS as Ts);
    if vtsc.vc < 0 || vtsc.vc > V::VSL_COUNT || !vtsc.is_normalized() {
        panic!("halt: a timestamp must be within the video frame range and normalized");
    }
    let mut r_incr: i32 = 0;
    if vtsc.is_contended_address(cpu.get_pc()) && vtsc.vc < V::VSL_PIXELS.end {
        let VideoTs { mut vc, mut hc } = **vtsc; // assume vtsc is normalized
        // border top
        if vc < V::VSL_PIXELS.start {
            // move hc to the beginning of range
            let hc_end = V::HTS_RANGE.end + (hc - V::HTS_RANGE.end).rem_euclid(M1_CYCLE_TS as Ts);
            vc += 1;
            r_incr = (i32::from(V::VSL_PIXELS.start - vc) * V::HTS_COUNT as i32 +
                      i32::from(hc_end - hc)) / M1_CYCLE_TS as i32;
            hc = hc_end - V::HTS_COUNT;
            vc = V::VSL_PIXELS.start;
        }
        // contended area, normalize hc by iterating to the end of one line
        while vc < V::VSL_PIXELS.end {
            let hc0 = hc - V::HTS_COUNT;
            let r_incr0 = r_incr;
            while hc < V::HTS_RANGE.end {
                hc = V::contention(hc) + M1_CYCLE_TS as Ts;
                r_incr += 1;
            }
            vc += 1;
            hc -= V::HTS_COUNT;
            if (r_incr - r_incr0) * (M1_CYCLE_TS as i32) < (hc - hc0) as i32 {
                break; // only when at least one contention encountered
            }
        }
        // still contended area, calculate an R increase for a whole single line
        if vc < V::VSL_PIXELS.end {
            let mut r_line = 0;
            while hc < V::HTS_RANGE.end {
                hc = V::contention(hc) + M1_CYCLE_TS as Ts;
                r_line += 1;
            }
            hc -= V::HTS_COUNT;
            r_incr += i32::from(V::VSL_PIXELS.end - vc) * r_line;
        }
        // bottom border
        vtsc.vc = V::VSL_PIXELS.end;
        vtsc.hc = hc;
    }

    let vc = V::VSL_COUNT;
    let hc = vtsc.hc.rem_euclid(M1_CYCLE_TS as Ts) - M1_CYCLE_TS as Ts;
    r_incr += (
                (i32::from(vc) - i32::from(vtsc.vc)) * V::HTS_COUNT as i32 +
                (i32::from(hc) - i32::from(vtsc.hc))
              ) / M1_CYCLE_TS as i32;
    if r_incr > 0 {
        vtsc.hc = hc;
        vtsc.vc = vc;
        cpu.add_r(r_incr);
    }
    vtsc
}
