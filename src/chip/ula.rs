mod audio;
mod io;
mod render_pixels;
mod video;

use core::num::Wrapping;
use z80emu::{*, host::{Result, cycles::M1_CYCLE_TS}};
use crate::audio::{AudioFrame};
use crate::bus::BusDevice;
use crate::chip::{ControlUnit, nanos_from_frame_tc_cpu_hz};
use crate::video::VideoFrame;
use crate::memory::ZxMemory;
use crate::peripherals::ZXKeyboardMap;
// use crate::io::*;
use crate::clock::{VideoTs, FTs, Ts, VideoTsData1, VideoTsData2, VideoTsData3};
pub use self::video::{UlaVideoFrame, UlaTsCounter};

pub const CPU_HZ: u32 = 3_500_000;

// #[derive(Clone)]
// pub struct SnowyUla<M, B> {
//     pub ula: Ula<M, B>
// }

/// ZX Spectrum 16k/48k ULA
/// The memory::Memory48k or memory::Memory16k can be used with it.
#[derive(Clone)]
pub struct Ula<M, B> {
    pub frames: Wrapping<u64>, // frame counter
    pub tsc: VideoTs, // current T-state timestamp
    pub memory: M,
    pub bus: B,
    // keyboard
    keyboard: ZXKeyboardMap,
    // video related
    frame_pixels: [(u32, [u8;32]);192],       // pixel read precedence pixels > memory
    frame_colors: [(u32, [u8;32]);192],
    frame_colors_coarse: [(u32, [u8;32]);24], // color read precedence colors > colors_coarse > memory
    border_out_changes: Vec<VideoTsData3>, // frame timestamp with packed border on 3 bits
    earmic_out_changes: Vec<VideoTsData2>, // frame timestamp with packed earmic on 2 bits
    ear_in_changes: Vec<VideoTsData1>,  // frame timestamp with packed earin on 1 bit
    // read_ear: MaybeReadEar,
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

impl<M, B> Default for Ula<M, B>
where M: ZxMemory + Default, B: Default
{
    fn default() -> Self {
        Ula {
            frames: Wrapping(0),   // frame counter
            tsc: VideoTs::default(),
            memory: M::default(),
            bus: B::default(),
            // video related
            frame_pixels: [(0, [0;32]);192],       // pixel read precedence pixels > memory
            frame_colors: [(0, [0;32]);192],
            frame_colors_coarse: [(0, [0;32]);24], // color read precedence colors > colors_coarse > memory
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
            prev_earmic_data: 3,
            last_earmic_data: 3,
            // keyboard
            keyboard: ZXKeyboardMap::empty()
        }
    }
}

impl<M, B> core::fmt::Debug for Ula<M,B> where M: ZxMemory
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Ula {{ frames: {:?}, tsc: {:?}, border: {} border_changes: {} earmic_changes: {} }}",
            self.frames, self.tsc, self.last_border, self.border_out_changes.len(), self.earmic_out_changes.len())
    }
}

impl<M, B> ControlUnit for Ula<M,B>
where M: ZxMemory, B: BusDevice<Timestamp=VideoTs>
{
    type TsCounter = UlaTsCounter;
    type BusDevice = B;

    fn cpu_clock_rate(&self) -> u32 {
        CPU_HZ
    }

    fn frame_duration_nanos(&self) -> u32 {
        nanos_from_frame_tc_cpu_hz(UlaVideoFrame::FRAME_TSTATES_COUNT as u32, CPU_HZ) as u32
    }

    fn bus_device(&mut self) -> &mut Self::BusDevice {
        &mut self.bus
    }

    fn current_frame(&self) -> u64 {
        self.frames.0
    }

    fn frame_tstate(&self) -> FTs {
        Self::TsCounter::from(self.tsc).as_frame_tstates()
    }

    fn current_tstate(&self) -> FTs {
        Self::TsCounter::from(self.tsc).as_tstates()
    }

    fn is_frame_over(&self) -> bool {
        Self::TsCounter::from(self.tsc).is_eof()
    }

    fn reset<C: Cpu>(&mut self, cpu: &mut C, hard: bool) {
        if hard {
            cpu.reset();
            self.bus.reset(self.tsc);
        }
        else {
            self.execute_instruction(cpu, opconsts::RST_00H_OPCODE).unwrap();
        }
    }

    fn nmi<C: Cpu>(&mut self, cpu: &mut C) -> bool {
        let mut tsc = self.ensure_next_frame();
        let res = cpu.nmi(self, &mut tsc);
        self.tsc = tsc.into();
        res
    }

    fn execute_next_frame<C: Cpu>(&mut self, cpu: &mut C) {
        let mut tsc = self.ensure_next_frame();
        loop {
            match cpu.execute_with_limit(self, &mut tsc, UlaVideoFrame::VSL_COUNT) {
                Ok(()) => break,
                Err(BreakCause::Halt) => {
                    tsc.tsc = execute_halted_state_until_eof::<UlaVideoFrame,_>(tsc.tsc, cpu);
                    break
                }
                Err(_) => {}
            }
        }
        self.bus.update_timestamp(tsc.as_timestamp());
        self.tsc = tsc.into();
    }

    fn ensure_next_frame(&mut self) -> Self::TsCounter {
        let mut tsc = Self::TsCounter::from(self.tsc);
        if tsc.is_eof() {
            self.bus.next_frame(tsc.as_timestamp());
            self.frames += Wrapping(1);
            self.cleanup_video_frame_data();
            self.cleanup_audio_frame_data();
            tsc.wrap_frame();
            self.tsc = tsc.tsc;
        }
        tsc
    }

    fn execute_single_step<C: Cpu, F>(&mut self, cpu: &mut C, debug: Option<F>) -> Result<(),()>
    where F: FnOnce(CpuDebug)
    {
        let mut tsc = self.ensure_next_frame();
        let res = cpu.execute_next(self, &mut tsc, debug);
        self.tsc = tsc.into();
        res
    }
}

impl<M, B> Ula<M, B> where M: ZxMemory, B: BusDevice<Timestamp=VideoTs>
{
    fn execute_instruction<C: Cpu>(&mut self, cpu: &mut C, code: u8) -> Result<(), ()> {
        const DEBUG: Option<CpuDebugFn> = None;
        let mut tsc = self.ensure_next_frame();
        let res = cpu.execute_instruction(self, &mut tsc, DEBUG, code);
        self.tsc = tsc.into();
        res
    }
}

/// Returns with a VideoTs at the frame interrupt and with cpu refresh register set accordingly.
/// The VideoTs passed here must be normalized.
pub fn execute_halted_state_until_eof<V: VideoFrame,
                                      C: Cpu>(
            mut tsc: VideoTs,
            cpu: &mut C
        ) -> VideoTs
{
    debug_assert_eq!(0, V::HTS_COUNT % M1_CYCLE_TS as Ts);
    let mut r_incr: i32 = 0;
    if V::is_contended_address(cpu.get_pc()) && tsc.vc < V::VSL_PIXELS.end {
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
    use z80emu::opconsts::HALT_OPCODE;
    use crate::bus::NullDevice;
    use crate::memory::Memory64k;
    use super::*;
    type TestUla = Ula::<Memory64k, NullDevice<VideoTs>>;

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
        let mut tsc1 = ula1.ensure_next_frame();
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
