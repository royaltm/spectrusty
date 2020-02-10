pub mod ula;
pub mod ay_player;

use core::time::Duration;
use std::time::Instant;
use z80emu::{Clock, CpuDebug, Io, Cpu, host::Result};
use crate::bus::BusDevice;
use crate::clock::FTs;

pub trait ControlUnit: Io {
    type TsCounter: Clock;
    type BusDevice: BusDevice;
    /// A frequency in Hz of the Cpu unit.
    const CPU_HZ: u32;
    /// A single frame time in nanoseconds.
    const FRAME_TIME_NANOS: u32;
    /// Returns a mutable reference to the first bus device.
    fn bus_device(&mut self) -> &mut Self::BusDevice;
    /// Returns current frame counter value.
    fn current_frame(&self) -> u64;
    /// Returns current frame's T-state.
    fn frame_tstate(&self) -> FTs;
    /// Returns `true` if current frame is over.
    fn is_frame_over(&self) -> bool;
    /// Perform computer reset.
    fn reset<C: Cpu>(&mut self, cpu: &mut C, hard: bool);
    /// Triggers non-maskable interrupt. Returns true if the nmi was successfully executed.
    /// May return false when Cpu has just executed EI instruction or a 0xDD 0xFD prefix.
    /// In this instance, execute a step or more and then try again.
    fn nmi<C: Cpu>(&mut self, cpu: &mut C) -> bool;
    /// Advances the internal state for the next frame and 
    /// executes cpu instructions as fast as possible untill the end of the frame.
    fn execute_next_frame<C: Cpu>(&mut self, cpu: &mut C);
    /// Prepares the internal state for the next frame.
    /// This method should be called after Chip.is_frame_over returns true and after all the video and audio
    /// rendering has been performed.
    fn ensure_next_frame(&mut self) -> Self::TsCounter;
    /// Executes a single cpu instruction with the option to pass a debugging function.
    /// Returns true if the frame has ended.
    fn execute_single_step<C: Cpu,
                           F: FnOnce(CpuDebug)>(&mut self, cpu: &mut C, debug: Option<F>) ->
                                        Result<Self::WrIoBreak, Self::RetiBreak>;
}

// pub const fn duration_from_frame_time(frame_time: f64) -> std::time::Duration {
//     const NANOS_PER_SEC: f64 = 1_000_000_000.0;
//     let nanos: u64 = (frame_time * NANOS_PER_SEC) as u64;
//     std::time::Duration::from_nanos(nanos)
// }

pub const fn nanos_from_frame_tc_cpu_hz(frame_ts_count: u32, cpu_hz: u32) -> u64 {
    const NANOS_PER_SEC: u64 = 1_000_000_000;
    frame_ts_count as u64 * NANOS_PER_SEC / cpu_hz as u64
}

pub const fn duration_from_frame_tc_cpu_hz(frame_ts_count: u32, cpu_hz: u32) -> std::time::Duration {
    let nanos = nanos_from_frame_tc_cpu_hz(frame_ts_count, cpu_hz);
    std::time::Duration::from_nanos(nanos)
}


pub struct ThreadSyncTimer {
    pub time: Instant
}

#[allow(clippy::new_without_default)]
impl ThreadSyncTimer {
    pub fn new() -> Self {
        ThreadSyncTimer { time: Instant::now() }
    }

    pub fn start(&mut self) -> Instant {
        core::mem::replace(&mut self.time, Instant::now())
    }

    pub fn synchronize_thread_to_frame<C: ControlUnit>(&mut self) -> core::result::Result<(), Instant> {
        let frame_duration = Duration::from_nanos(C::FRAME_TIME_NANOS as u64);
        if let Some(duration) = frame_duration.checked_sub(self.time.elapsed()) {
            std::thread::sleep(duration);
            self.time += frame_duration;
            Ok(())
        }
        else {
            Err(self.start())
        }

    }
}
