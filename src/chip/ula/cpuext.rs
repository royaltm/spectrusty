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
    HALT_VC_THRESHOLD, VideoTs, Ts, VFrameTsCounter,
    MemoryContention
};
use crate::memory::MemoryExtension;
use crate::video::VideoFrame;

/// Methods for code execution with memory contention specific to a chipset.
pub trait ControlUnitContention {
    fn nmi_with_contention<C: Cpu>(&mut self, cpu: &mut C) -> bool;
    fn execute_next_frame_with_contention<C: Cpu>(&mut self, cpu: &mut C);
    fn execute_single_step_with_contention<C: Cpu, F: FnOnce(CpuDebug)>(
            &mut self,
            cpu: &mut C,
            debug: Option<F>
        ) -> Result<(),()>;
}

pub(crate) trait UlaTimestamp {
    type VideoFrame: VideoFrame;
    fn video_ts(&self) -> VideoTs;
    fn set_video_ts(&mut self, vts: VideoTs);
    fn ensure_next_frame_vtsc<T: MemoryContention>(&mut self) -> VFrameTsCounter<Self::VideoFrame, T>;
}

pub(crate) trait UlaCpuExt: UlaTimestamp {
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
    fn ula_nmi<T: MemoryContention, C: Cpu>(&mut self, cpu: &mut C) -> bool {
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
        if cpu.is_halt() && !cpu.get_iffs().0 {
            // HALT forever (INT disabled)
            *vtsc = execute_halted_state_until_eof::<Self::VideoFrame,T,_>(vtsc.into(), cpu);
        }
        else {
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
/// The timestamp - `tsc` passed here must be normalized and its vertical component must be positive and
/// its composite value must be less than [<V as VideoFrame>::FRAME_TSTATES_COUNT][VideoFrame::FRAME_TSTATES_COUNT].
/// Otherwise this method panics.
pub fn execute_halted_state_until_eof<V: VideoFrame,
                                      M: MemoryContention,
                                      C: Cpu>(
            mut tsc: VideoTs,
            cpu: &mut C
        ) -> VideoTs
{
    debug_assert_eq!(0, V::HTS_COUNT % M1_CYCLE_TS as Ts);
    if tsc.vc < 0 || tsc.vc > V::VSL_COUNT || !V::is_normalized_vts(tsc) {
        panic!("halt: a timestamp must be within the video frame range and normalized");
    }
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
    let hc = tsc.hc.rem_euclid(M1_CYCLE_TS as Ts) - M1_CYCLE_TS as Ts;
    r_incr += (
                (i32::from(vc) - i32::from(tsc.vc)) * V::HTS_COUNT as i32 +
                (i32::from(hc) - i32::from(tsc.hc))
              ) / M1_CYCLE_TS as i32;
    tsc.hc = hc;
    tsc.vc = vc;
    if r_incr >= 0 {
        cpu.add_r(r_incr);
    }
    else {
        panic!("halt: a video timestamp exceeds the end of frame boundary");
    }
    tsc
}

#[cfg(test)]
mod tests {
    use crate::z80emu::{*, opconsts::HALT_OPCODE};
    use crate::memory::{Memory64k, ZxMemory};
    use super::*;
    use super::super::{Ula, UlaMemoryContention, UlaVideoFrame};
    type TestUla = Ula::<Memory64k>;

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
        // execute_halted_state_until_eof::<UlaVideoFrame,UlaMemoryContention,_>(ula.tsc, &mut cpu);
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
        while tsc1.tsc.hc < -(M1_CYCLE_TS as Ts) {
            match cpu1.execute_next::<_,_,CpuDebugFn>(&mut ula1, &mut tsc1, None) {
                Ok(()) => (),
                Err(_) => unreachable!()
            }
        }
        // println!("{:?} {:?} fr: {:?}", tsc1.tsc, ula.tsc, ula.frame_tstate());
        assert_eq!(tsc1.tsc, ula.tsc);
        assert_eq!(cpu1, cpu);
        // println!("=================================");
    }

    #[test]
    fn ula_works() {
        for vc in 0..=UlaVideoFrame::VSL_COUNT {
            // println!("vc: {:?}", vc);
            for hc in UlaVideoFrame::HTS_RANGE {
                test_ula_contended_halt(0, vc, hc);
                test_ula_contended_halt(0x4000, vc, hc);
            }
        }
    }
}
