mod audio;
mod io;
mod render_pixels;
mod video;

use crate::audio::{AudioFrame};
use crate::bus::BusDevice;
use crate::chip::{ControlUnit, nanos_from_frame_tc_cpu_hz};
use crate::video::VideoFrame;
use core::num::Wrapping;

use crate::memory::ZxMemory;
use z80emu::{*, host::{Result, cycles::M1_CYCLE_TS}};
use crate::io::*;
use crate::clock::{VideoTs, FTs, Ts, VideoTsData1, VideoTsData2, VideoTsData3};
use self::video::UlaTsCounter;

pub const CPU_HZ: u32 = 3_500_000;

// struct MaybeReadEar(pub Option<Box<dyn ReadEar>>);

// impl Clone for MaybeReadEar {
//     fn clone(&self) -> Self {
//         MaybeReadEar(None)
//     }
// }

// ZX Spectrum 16k/48k ULA
// The memory::Memory48k or memory::Memory16k can be used with it.
#[derive(Clone)]
pub struct Ula<M: ZxMemory, B: BusDevice<Timestamp=VideoTs>> {
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
    prev_earmic: u8, // prev recorded change
    last_earmic: u8, // last recorded change
}

impl<M, B> Default for Ula<M, B>
where M: ZxMemory + Default, B: BusDevice<Timestamp=VideoTs> + Default//, Self: AudioFrame
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
            prev_earmic: 3,
            last_earmic: 3,
            // keyboard
            keyboard: ZXKeyboardMap::empty()
        }
    }
}

impl<M, B> core::fmt::Debug for Ula<M,B> where M: ZxMemory, B: BusDevice<Timestamp=VideoTs>
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Ula {{ frames: {:?}, tsc: {:?}, border: {} border_changes: {} earmic_changes: {} }}",
            self.frames, self.tsc, self.last_border, self.border_out_changes.len(), self.earmic_out_changes.len())
    }
}

impl<M, B> ControlUnit for Ula<M,B>
where Self: VideoFrame, M: ZxMemory, B: BusDevice<Timestamp=VideoTs>
{
    type TsCounter = UlaTsCounter<M,B>;
    type BusDevice = B;

    const CPU_HZ: u32 = CPU_HZ;
    const FRAME_TIME_NANOS: u32 = nanos_from_frame_tc_cpu_hz(Ula::<M,B>::FRAME_TSTATES_COUNT as u32, CPU_HZ) as u32;

    fn bus_device(&mut self) -> &mut Self::BusDevice {
        &mut self.bus
    }

    fn current_frame(&self) -> u64 {
        self.frames.0
    }

    fn frame_tstate(&self) -> FTs {
        Self::TsCounter::from(self.tsc).as_frame_tstates()
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
            match cpu.execute_with_limit(self, &mut tsc, Self::VSL_COUNT) {
                Ok(()) => break,
                Err(BreakCause::Halt) => {
                    if !Self::is_contended_address(cpu.get_pc()) {
                        tsc.tsc.vc = Self::VSL_COUNT;
                        tsc.tsc.hc = tsc.tsc.hc.rem_euclid(M1_CYCLE_TS as Ts);
                        break;
                    }
                }
                Err(_) => {}
            }
        }
        self.tsc = tsc.into();
    }

    fn ensure_next_frame(&mut self) -> Self::TsCounter {
        let mut tsc = Self::TsCounter::from(self.tsc);
        if tsc.is_eof() {
            self.frames += Wrapping(1);
            self.cleanup_video_frame_data();
            self.cleanup_audio_frame_data();
            // if let Some(read_ear) = &mut self.read_ear.0 {
            //     read_ear.next_frame(tsc.as_tstates());
            // }
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
