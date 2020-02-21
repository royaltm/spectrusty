use core::num::NonZeroU16;
use core::marker::PhantomData;
use core::num::Wrapping;

use z80emu::{Cpu, Clock, Io, Memory, CpuDebug, CpuDebugFn, BreakCause, opconsts, host::{
        TsCounter, Result, cycles::M1_CYCLE_TS
    }};
use crate::audio::*;
use crate::audio::sample::SampleDelta;
use crate::audio::ay::*;
use crate::io::ay::*;
use crate::clock::{FTs, FTsData2};
use crate::memory::{ZxMemory, Memory64k};
use crate::bus::{BusDevice, NullDevice};
use crate::chip::{ControlUnit, nanos_from_frame_tc_cpu_hz, HostConfig128k, HostConfig};

#[derive(Clone)]
pub struct AyPlayer<P> {
    pub frames: Wrapping<u64>,
    pub tsc: TsCounter<FTs>,
    pub memory: Memory64k,
    pub ay_sound: Ay3_891xAudio,
    pub ay_io: Ay3_8913Io<FTs>,
    pub earmic_changes: Vec<FTsData2>,
    pub last_earmic: u8,
    pub prev_earmic: u8,
        cpu_rate: u32,
        frame_tstates: FTs,
        bus: NullDevice<FTs>,
        _port_decode: PhantomData<P>
}

impl<P> Default for AyPlayer<P> {
    fn default() -> Self {
        AyPlayer {
            frames: Wrapping(0),
            tsc: TsCounter::default(),
            memory: Memory64k::default(),
            ay_sound: Ay3_891xAudio::default(),
            ay_io: Ay3_8913Io::default(),
            earmic_changes: Vec::new(),
            last_earmic: 3,
            prev_earmic: 3,
            cpu_rate: HostConfig128k::CPU_HZ,
            frame_tstates: HostConfig128k::FRAME_TSTATES,
            bus: NullDevice::default(),
            _port_decode: PhantomData
        }
    }
}

impl<P, A: Blep> AudioFrame<A> for AyPlayer<P>
{
    fn ensure_audio_frame_time(&self, blep: &mut A, sample_rate: u32) {
        blep.ensure_frame_time(sample_rate, self.cpu_rate, self.frame_tstates, MARGIN_TSTATES)
    }

    fn get_audio_frame_end_time(&self) -> FTs {
        let ts = self.tsc.as_timestamp();
        debug_assert!(ts >= self.frame_tstates);
        ts
    }
}

impl<P, A> AyAudioFrame<A> for AyPlayer<P>
    where A: Blep
{
    fn render_ay_audio_frame<V: AmpLevels<A::SampleDelta>>(&mut self, blep: &mut A, chans: [usize; 3]) {
        let end_ts = self.tsc.as_timestamp();
        debug_assert!(end_ts >= self.frame_tstates);
        let changes = self.ay_io.recorder.drain_ay_reg_changes();
        self.ay_sound.render_audio::<V,_,A>(changes, blep, end_ts, chans)
    }
}

impl<P, A, L> EarMicOutAudioFrame<A> for AyPlayer<P>
    where A: Blep<SampleDelta=L>,
          L: SampleDelta
{
    #[inline(always)]
    fn render_earmic_out_audio_frame<V: AmpLevels<L>>(&self, blep: &mut A, channel: usize) {
        render_audio_frame_ts::<V,L,A,_>(self.prev_earmic,
                                         None,
                                         &self.earmic_changes,
                                         blep, channel)
    }
}

impl<P: AyPortDecode> ControlUnit for AyPlayer<P> {
    type TsCounter = TsCounter<FTs>;
    type BusDevice = NullDevice<FTs>;
    /// A frequency in Hz of the Cpu unit.
    fn cpu_clock_rate(&self) -> u32 {
        self.cpu_rate
    }
    /// A single frame time in seconds.
    fn frame_duration_nanos(&self) -> u32 {
        nanos_from_frame_tc_cpu_hz(self.frame_tstates as u32, self.cpu_rate) as u32
    }
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
        self.tsc.as_timestamp().rem_euclid(self.frame_tstates)
    }
    /// Returns current frame's T-state.
    fn current_tstate(&self) -> FTs {
        self.tsc.as_timestamp()
    }
    /// Returns `true` if current frame is over.
    fn is_frame_over(&self) -> bool {
        self.tsc.as_timestamp() >= self.frame_tstates
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
            match cpu.execute_with_limit(self, &mut tsc, self.frame_tstates) {
                Ok(()) => break,
                Err(BreakCause::Halt) => {
                    assert_eq!(0, self.frame_tstates.rem_euclid(M1_CYCLE_TS as FTs));
                    let ts = self.frame_tstates + tsc.as_timestamp()
                                                     .rem_euclid(M1_CYCLE_TS as FTs);
                    let r_incr = (ts - tsc.as_timestamp()) / M1_CYCLE_TS as i32;
                    cpu.add_r(r_incr);
                    tsc.0 = Wrapping(ts);
                    break;
                }
                Err(_) => {}
            }
        }
        self.bus.update_timestamp(tsc.as_timestamp());
        self.tsc = tsc;
    }
    /// Prepares the internal state for the next frame.
    /// This method should be called after Chip.is_frame_over returns true and after all the video and audio
    /// rendering has been performed.
    fn ensure_next_frame(&mut self) -> Self::TsCounter {
        let ts = self.tsc.as_timestamp();
        if ts >= self.frame_tstates {
            self.bus.next_frame(ts);
            self.frames += Wrapping(1);
            self.ay_io.recorder.clear();
            self.earmic_changes.clear();
            self.prev_earmic = self.last_earmic;
            self.tsc.0 = Wrapping(ts) - Wrapping(self.frame_tstates);
        }
        self.tsc
    }
    /// Executes a single cpu instruction with the option to pass a debugging function.
    /// Returns true if the frame has ended.
    fn execute_single_step<C: Cpu, F>(&mut self,
                cpu: &mut C,
                debug: Option<F>
            ) -> Result<Self::WrIoBreak, Self::RetiBreak>
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
    /// Changes the cpu clock frequency and a duration of frames.
    pub fn set_host_config<H: HostConfig>(&mut self) {
        self.ensure_next_frame();
        self.cpu_rate = H::CPU_HZ;
        self.frame_tstates = H::FRAME_TSTATES;
    }
    /// Resets the frames counter.
    pub fn reset_frames(&mut self) {
        self.frames.0 = 0;
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
            self.ay_io.data_port_read(port, ts)
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
            P::write_ay_io(&mut self.ay_io, port, data, ts);
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
