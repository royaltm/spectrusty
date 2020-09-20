/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
//! An emulator of a video-less chip for ZX Spectrum to be used in a music player.
use core::num::NonZeroU16;
use core::marker::PhantomData;
use core::num::Wrapping;

use crate::z80emu::{
    Cpu, Clock, Io, Memory, CpuDebug, CpuDebugFn, BreakCause,
    opconsts,
    host::{TsCounter, Result, cycles::M1_CYCLE_TS}
};
#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};

use crate::audio::*;
use crate::peripherals::ay::{audio::*, Ay3_8913Io, AyPortDecode, AyRegRecorder};
use crate::clock::{FTs, FTsData2};
use crate::memory::{ZxMemory, Memory64k};
use crate::bus::{BusDevice, NullDevice};
use crate::chip::{
    FrameState, ControlUnit, ZxSpectrum128Config, HostConfig,
    nanos_from_frame_tc_cpu_hz
};

#[derive(Clone)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "snapshot", serde(rename_all = "camelCase"))]
pub struct AyPlayer<P> {
    pub frames: Wrapping<u64>,
    pub tsc: TsCounter<FTs>,
    pub memory: Memory64k,
    pub ay_sound: Ay3_891xAudio,
    pub ay_io: Ay3_8913Io<FTs>,
    #[cfg_attr(feature = "snapshot", serde(skip))]
    pub earmic_changes: Vec<FTsData2>,
    pub last_earmic: u8,
    pub prev_earmic: u8,
        cpu_rate: u32,
        frame_tstates: FTs,
        bus: NullDevice<FTs>,
    #[cfg_attr(feature = "snapshot", serde(skip))]
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
            cpu_rate: ZxSpectrum128Config::CPU_HZ,
            frame_tstates: ZxSpectrum128Config::FRAME_TSTATES,
            bus: NullDevice::default(),
            _port_decode: PhantomData
        }
    }
}

impl<P, A: Blep> AudioFrame<A> for AyPlayer<P> {
    fn ensure_audio_frame_time(&self, blep: &mut A, sample_rate: u32, cpu_hz: f64) {
        blep.ensure_frame_time(sample_rate, cpu_hz, self.frame_tstates, MARGIN_TSTATES)
    }

    fn get_audio_frame_end_time(&self) -> FTs {
        let ts = self.tsc.as_timestamp();
        assert!(ts >= self.frame_tstates, "AyPlayer::get_audio_frame_end_time:: frame execution didn't finish yet: {} < {}", ts, self.frame_tstates);
        ts
    }
}

impl<P, A> AyAudioFrame<A> for AyPlayer<P>
    where A: Blep
{
    fn render_ay_audio_frame<V: AmpLevels<A::SampleDelta>>(&mut self, blep: &mut A, chans: [usize; 3]) {
        let end_ts = self.tsc.as_timestamp();
        let changes = self.ay_io.recorder.drain_ay_reg_changes();
        self.ay_sound.render_audio::<V,_,A>(changes, blep, end_ts, self.frame_tstates, chans)
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

impl<P: AyPortDecode> FrameState for AyPlayer<P> {
    fn current_frame(&self) -> u64 {
        self.frames.0
    }

    fn set_frame_counter(&mut self, fc: u64) {
        self.frames = Wrapping(fc);
    }

    fn frame_tstate(&self) -> (u64, FTs) {
        let mut frames = self.frames;
        let ts = self.tsc.as_timestamp();
        let ts_norm = ts.rem_euclid(self.frame_tstates);
        if ts_norm != ts {
            frames = if ts < 0 {
                frames - Wrapping(1)
            }
            else {
                frames + Wrapping(1)
            }
        }
        (frames.0, ts_norm)
    }

    fn current_tstate(&self) -> FTs {
        self.tsc.as_timestamp()
    }

    fn set_frame_tstate(&mut self, ts: FTs) {
        *self.tsc = Wrapping(ts);
    }

    fn is_frame_over(&self) -> bool {
        self.tsc.as_timestamp() >= self.frame_tstates
    }
}

impl<P: AyPortDecode> ControlUnit for AyPlayer<P> {
    type BusDevice = NullDevice<FTs>;

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
            self.ay_sound.reset();
            let ts = self.tsc.as_timestamp();
            self.ay_io.reset(ts);
            self.bus.reset(ts);
        }
        else {
            self.execute_instruction(cpu, opconsts::RST_00H_OPCODE).unwrap();
        }
    }

    fn nmi<C: Cpu>(&mut self, cpu: &mut C) -> bool {
        let mut tsc = self.ensure_next_frame_tsc();
        let res = cpu.nmi(self, &mut tsc);
        self.tsc = tsc;
        res
    }

    fn execute_next_frame<C: Cpu>(&mut self, cpu: &mut C) {
        let mut tsc = self.ensure_next_frame_tsc();
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

    fn ensure_next_frame(&mut self) {
        self.ensure_next_frame_tsc();
    }

    fn execute_single_step<C: Cpu, F>(&mut self,
                cpu: &mut C,
                debug: Option<F>
            ) -> Result<(), ()>
        where F: FnOnce(CpuDebug)
    {
        let mut tsc = self.ensure_next_frame_tsc();
        let res = cpu.execute_next(self, &mut tsc, debug);
        self.tsc = tsc;
        res
    }
}

impl<P: AyPortDecode> AyPlayer<P> {
    pub fn cpu_clock_rate(&self) -> u32 {
        self.cpu_rate
    }

    pub fn frame_cycle_count(&self) -> FTs {
        self.frame_tstates
    }

    pub fn frame_duration_nanos(&self) -> u32 {
        nanos_from_frame_tc_cpu_hz(self.frame_tstates as u32, self.cpu_rate) as u32
    }

    pub fn ensure_audio_frame_time<B: Blep>(&self, blep: &mut B, sample_rate: u32) {
        <Self as AudioFrame<B>>::ensure_audio_frame_time(self, blep, sample_rate, self.cpu_rate as f64)
    }

    fn ensure_next_frame_tsc(&mut self) -> TsCounter<FTs> {
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

    fn execute_instruction<C: Cpu>(&mut self, cpu: &mut C, code: u8) -> Result<(), ()> {
        const DEBUG: Option<CpuDebugFn> = None;
        let mut tsc = self.ensure_next_frame_tsc();
        let res = cpu.execute_instruction(self, &mut tsc, DEBUG, code);
        self.tsc = tsc;
        res
    }
    /// Changes the cpu clock frequency and the duration of frames.
    pub fn set_host_config<H: HostConfig>(&mut self) {
        self.ensure_next_frame();
        self.cpu_rate = H::CPU_HZ;
        self.frame_tstates = H::FRAME_TSTATES;
    }
    /// Changes the cpu clock frequency and the duration of frames.
    pub fn set_config(&mut self, cpu_rate: u32, frame_tstates: FTs) {
        assert!(frame_tstates > 0 && frame_tstates as u32 <= cpu_rate);
        self.ensure_next_frame();
        self.cpu_rate = cpu_rate;
        self.frame_tstates = frame_tstates;
    }
    /// Resets the frames counter.
    pub fn reset_frames(&mut self) {
        self.frames.0 = 0;
    }
    /// Writes data directly into the AY registers, recording changes and advancing the clock counter.
    ///
    /// Sets the clock counter to `timestamp`.
    ///
    /// # Panics
    /// The `timestamp` must be larger than or equal to the current clock counter value.
    pub fn write_ay(&mut self, timestamp: FTs, reg: u8, val: u8) {
        assert!(timestamp >= self.tsc.as_timestamp());
        *self.tsc = Wrapping(timestamp);
        let reg = reg.into();
        self.ay_io.set(reg, val);
        self.ay_io.recorder.record_ay_reg_change(reg, val, timestamp);
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
