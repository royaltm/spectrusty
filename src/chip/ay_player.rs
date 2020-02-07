use core::num::NonZeroU16;
use core::marker::PhantomData;
use core::num::Wrapping;

use z80emu::{Cpu, Clock, Io, Memory, CpuDebug, CpuDebugFn, BreakCause, opconsts, host::{
        TsCounter, Result, cycles::M1_CYCLE_TS
    }};
use crate::audio::*;
use crate::audio::sample::{SampleDelta, SampleTime};
use crate::audio::ay::*;
use crate::io::ay::*;
use crate::clock::{FTs, FTsData2};
use crate::memory::{ZxMemory, Memory48k};
use crate::bus::{BusDevice, NullDevice};
use crate::chip::{ControlUnit, nanos_from_frame_tc_cpu_hz};

const FRAME_TSTATES: FTs = 70908;
const CPU_HZ: u32 = 3_546_900;

#[derive(Clone, Default)]
pub struct AyPlayer<P> {
    pub frames: Wrapping<u64>,
    pub tsc: TsCounter<FTs>,
    pub memory: Memory48k,
    pub ay_sound: Ay3_8891xAudio,
    pub ay_io: Ay3_8891N<FTs>,
    pub earmic_changes: Vec<FTsData2>,
    pub last_earmic: u8,
    pub prev_earmic: u8,
    bus: NullDevice<FTs>,
    _port_decode: PhantomData<P>
}

pub trait AyPortDecode {
    const PORT_MASK: u16;
    const PORT_SELECT: u16;
    const PORT_DATA_READ: u16;
    const PORT_DATA_WRITE: u16;
    #[inline]
    fn is_select(port: u16) -> bool {
        port & Self::PORT_MASK == Self::PORT_SELECT
    }
    #[inline]
    fn is_data_read(port: u16) -> bool {
        port & Self::PORT_MASK == Self::PORT_DATA_READ
    }
    #[inline]
    fn is_data_write(port: u16) -> bool {
        port & Self::PORT_MASK == Self::PORT_DATA_WRITE
    }
}

#[derive(Clone, Copy, Default)]
pub struct Ay128kPortDecode;
impl AyPortDecode for Ay128kPortDecode {
    const PORT_MASK      : u16 = 0b11000000_00000010;
    const PORT_SELECT    : u16 = 0b11000000_00000000;
    const PORT_DATA_READ : u16 = 0b11000000_00000000;
    const PORT_DATA_WRITE: u16 = 0b10000000_00000000;
}

#[derive(Clone, Copy, Default)]
pub struct AyFullerBoxPortDecode;
impl AyPortDecode for AyFullerBoxPortDecode {
    const PORT_MASK      : u16 = 0x00ff;
    const PORT_SELECT    : u16 = 0x003f;
    const PORT_DATA_READ : u16 = 0x003f;
    const PORT_DATA_WRITE: u16 = 0x005f;
}

#[derive(Clone, Copy, Default)]
pub struct AyTC2068PortDecode;
impl AyPortDecode for AyTC2068PortDecode {
    const PORT_MASK      : u16 = 0x00ff;
    const PORT_SELECT    : u16 = 0x00f5;
    const PORT_DATA_READ : u16 = 0x00f6;
    const PORT_DATA_WRITE: u16 = 0x00f6;
}

impl<P,A,FT> AudioFrame<A> for AyPlayer<P>
where A: Blep<SampleTime=FT>, FT: SampleTime
{
    fn ensure_audio_frame_time(blep: &mut A, sample_rate: u32) -> FT {
        let time_rate = FT::time_rate(sample_rate, CPU_HZ);
        blep.ensure_frame_time(time_rate.at_timestamp(FRAME_TSTATES),
                               time_rate.at_timestamp(MARGIN_TSTATES));
        time_rate
    }

    fn get_audio_frame_end_time(&self, time_rate: FT) -> FT {
        let ts = self.tsc.as_timestamp();
        debug_assert!(ts >= FRAME_TSTATES);
        time_rate.at_timestamp(ts)
    }
}

impl<P,A,L,FT> AyAudioFrame<A> for AyPlayer<P>
where A: Blep<SampleDelta=L, SampleTime=FT>,
      L: SampleDelta + Default,
      FT: SampleTime
{
    fn render_ay_audio_frame<V: AmpLevels<L>>(&mut self, blep: &mut A, time_rate: FT, chans: [usize; 3]) {
        let end_ts = self.tsc.as_timestamp();
        debug_assert!(end_ts >= FRAME_TSTATES);
        let changes = self.ay_io.recorder.drain_ay_reg_changes();
        self.ay_sound.render_audio::<V,L,_,A,FT>(changes, blep, time_rate, end_ts, chans)
    }
}

impl<P,A,L,FT> EarMicOutAudioFrame<A> for AyPlayer<P>
where A: Blep<SampleDelta=L, SampleTime=FT>,
      L: SampleDelta,
      FT: SampleTime
{
    #[inline(always)]
    fn render_earmic_out_audio_frame<V: AmpLevels<L>>(&self, blep: &mut A, time_rate: FT, channel: usize) {
        render_audio_frame_ts::<V,L,A,FT,_>(self.prev_earmic,
                                         None,
                                         &self.earmic_changes,
                                         blep, time_rate, channel)
    }
}

impl<P: AyPortDecode> ControlUnit for AyPlayer<P> {
    type TsCounter = TsCounter<FTs>;
    type BusDevice = NullDevice<FTs>;
    /// A frequency in Hz of the Cpu unit.
    const CPU_HZ: u32 = CPU_HZ;
    /// A single frame time in seconds.
    const FRAME_TIME_NANOS: u32 = nanos_from_frame_tc_cpu_hz(FRAME_TSTATES as u32, CPU_HZ) as u32;
    /// Returns a mutable reference to the first bus device.
    fn bus_device(&mut self) -> &mut Self::BusDevice {
        &mut self.bus
    }
    /// Returns current frame counter value.
    fn current_frame(&self) -> u64 {
        self.frames.0
    }
    /// Returns current frame's T-state.
    fn frame_tstate(&self) -> FTs {
        self.tsc.as_timestamp()
    }
    /// Returns `true` if current frame is over.
    fn is_frame_over(&self) -> bool {
        self.tsc.as_timestamp() >= FRAME_TSTATES
    }
    /// Perform computer reset.
    fn reset<C: Cpu>(&mut self, cpu: &mut C, hard: bool) {
        if hard {
            cpu.reset();
            self.ay_sound.reset();
            let ts = self.tsc.as_timestamp();
            self.ay_io.reset(ts);
            self.bus.reset(ts);
        }
        else {
            self.execute_instruction(cpu, opconsts::RST_00H_OPCODE).unwrap();
        }
    }
    /// Triggers non-maskable interrupt. Returns true if the nmi was successfully executed.
    /// May return false when Cpu has just executed EI instruction or a 0xDD 0xFD prefix.
    /// In this instance, execute a step or more and then try again.
    fn nmi<C: Cpu>(&mut self, cpu: &mut C) -> bool {
        let mut tsc = self.ensure_next_frame();
        let res = cpu.nmi(self, &mut tsc);
        self.tsc = tsc;
        res
    }
    /// Advances the internal state for the next frame and 
    /// executes cpu instructions as fast as possible untill the end of the frame.
    fn execute_next_frame<C: Cpu>(&mut self, cpu: &mut C) {
        let mut tsc = self.ensure_next_frame();
        loop {
            match cpu.execute_with_limit(self, &mut tsc, FRAME_TSTATES) {
                Ok(()) => break,
                Err(BreakCause::Halt) => {
                    tsc.0 = Wrapping(
                        FRAME_TSTATES + (
                            tsc.as_timestamp().wrapping_sub(FRAME_TSTATES)
                        ).rem_euclid(M1_CYCLE_TS as FTs)
                    );
                    break;
                }
                Err(_) => {}
            }
        }
        self.tsc = tsc;
    }
    /// Prepares the internal state for the next frame.
    /// This method should be called after Chip.is_frame_over returns true and after all the video and audio
    /// rendering has been performed.
    fn ensure_next_frame(&mut self) -> Self::TsCounter {
        let ts = self.tsc.as_timestamp();
        if ts >= FRAME_TSTATES {
            self.frames += Wrapping(1);
            self.ay_io.recorder.clear();
            self.earmic_changes.clear();
            self.prev_earmic = self.last_earmic;
            self.tsc.0 = Wrapping(ts) - Wrapping(FRAME_TSTATES);
        }
        self.tsc
    }
    /// Executes a single cpu instruction with the option to pass a debugging function.
    /// Returns true if the frame has ended.
    fn execute_single_step<C: Cpu, F>(&mut self, cpu: &mut C, debug: Option<F>) -> Result<Self::WrIoBreak, Self::RetiBreak>
        where F: FnOnce(CpuDebug)
    {
        let mut tsc = self.ensure_next_frame();
        let res = cpu.execute_next(self, &mut tsc, debug);
        self.tsc = tsc;
        res
    }
}

impl<P: AyPortDecode> AyPlayer<P>
{
    fn execute_instruction<C: Cpu>(&mut self, cpu: &mut C, code: u8) -> Result<(), ()> {
        const DEBUG: Option<CpuDebugFn> = None;
        let mut tsc = self.ensure_next_frame();
        let res = cpu.execute_instruction(self, &mut tsc, DEBUG, code);
        self.tsc = tsc;
        res
    }
}

impl<P> Io for AyPlayer<P> where P: AyPortDecode {
    type Timestamp = FTs;
    type WrIoBreak = ();
    type RetiBreak = ();

    #[inline(always)]
    fn is_irq(&mut self, ts: FTs) -> bool {
        ts & !31 == 0
    }

    fn read_io(&mut self, port: u16, ts: FTs) -> (u8, Option<NonZeroU16>) {
        let val = if P::is_data_read(port) {
            self.ay_io.data_port_read(ts)
        }
        else {
            0xff
        };
        (val, None)
    }

    fn write_io(&mut self, port: u16, data: u8, ts: FTs) -> (Option<()>, Option<NonZeroU16>) {
        if port & 1 == 0 {
            let earmic = data >> 3 & 3;
            if self.last_earmic != earmic {
                self.last_earmic = earmic;
                self.earmic_changes.push((ts, earmic).into());
            }
        }
        else {
            match port & P::PORT_MASK {
                p if p == P::PORT_SELECT => {
                    self.ay_io.select_port(data)
                }
                p if p == P::PORT_DATA_WRITE => {
                    self.ay_io.data_port_write(data, ts)
                }
                _ => {}
            }
        }
        (None, None)
    }
}

impl<P> Memory for AyPlayer<P> {
    type Timestamp = FTs;

    #[inline]
    fn read_debug(&self, addr: u16) -> u8 {
        self.memory.read(addr)
    }

    #[inline]
    fn read_mem(&self, addr: u16, _ts: FTs) -> u8 {
        self.memory.read(addr)
    }

    #[inline]
    fn read_mem16(&self, addr: u16, _ts: FTs) -> u16 {
        self.memory.read16(addr)
    }

    #[inline]
    fn read_opcode(&mut self, pc: u16, _ir: u16, _ts: FTs) -> u8 {
        self.memory.read(pc)
    }

    #[inline]
    fn write_mem(&mut self, addr: u16, val: u8, _ts: FTs) {
        self.memory.write(addr, val);
    }
}
