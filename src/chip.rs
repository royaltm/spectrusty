pub mod ula;
pub mod ay_player;

use core::time::Duration;
use core::num::NonZeroU32;
use std::time::Instant;
use z80emu::{Clock, CpuDebug, Io, Cpu, host::Result};
use crate::bus::BusDevice;
use crate::clock::FTs;

pub trait ControlUnit : Io {
    type TsCounter: Clock;
    type BusDevice: BusDevice;
    /// A frequency in Hz of the Cpu unit. This is the same as a number of T-states per second.
    fn cpu_clock_rate(&self) -> u32;
    /// A single frame duration in nanoseconds.
    fn frame_duration_nanos(&self) -> u32;
    /// Returns a mutable reference to the first bus device.
    fn bus_device_mut(&mut self) -> &mut Self::BusDevice;
    /// Returns a reference to the first bus device.
    fn bus_device_ref(&self) -> &Self::BusDevice;
    /// Returns current frame counter value.
    fn current_frame(&self) -> u64;
    /// Returns current frame's T-state.
    fn frame_tstate(&self) -> FTs;
    /// Returns current T-state (can be negative as well as exceed a single frame).
    fn current_tstate(&self) -> FTs;
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
                           F: FnOnce(CpuDebug)>(
            &mut self,
            cpu: &mut C,
            debug: Option<F>
    ) -> Result<Self::WrIoBreak, Self::RetiBreak>;
}

pub trait HostConfig {
    const CPU_HZ: u32;
    const FRAME_TSTATES: FTs;

    #[inline]
    fn frame_time_nanos() -> u64 {
        nanos_from_frame_tc_cpu_hz(Self::FRAME_TSTATES as u32, Self::CPU_HZ)
    }

    #[inline]
    fn frame_time_duration() -> Duration {
        duration_from_frame_tc_cpu_hz(Self::FRAME_TSTATES as u32, Self::CPU_HZ)
    }
}

pub struct HostConfig48k;
impl HostConfig for HostConfig48k {
    const CPU_HZ: u32 = 3_500_000;
    const FRAME_TSTATES: FTs = 69888;
}

pub struct HostConfig128k;
impl HostConfig for HostConfig128k {
    const CPU_HZ: u32 = 3_546_900;
    const FRAME_TSTATES: FTs = 70908;
}

// pub const fn duration_from_frame_time(frame_time: f64) -> std::time::Duration {
//     const NANOS_PER_SEC: f64 = 1_000_000_000.0;
//     let nanos: u64 = (frame_time * NANOS_PER_SEC) as u64;
//     std::time::Duration::from_nanos(nanos)
// }

/// Returns a number of nanoseconds from the number of T-states in a single frame and a cpu clock rate.
pub const fn nanos_from_frame_tc_cpu_hz(frame_ts_count: u32, cpu_hz: u32) -> u64 {
    const NANOS_PER_SEC: u64 = 1_000_000_000;
    frame_ts_count as u64 * NANOS_PER_SEC / cpu_hz as u64
}

/// Returns a duration from the number of T-states in a single frame and a cpu clock rate.
pub const fn duration_from_frame_tc_cpu_hz(frame_ts_count: u32, cpu_hz: u32) -> Duration {
    let nanos = nanos_from_frame_tc_cpu_hz(frame_ts_count, cpu_hz);
    Duration::from_nanos(nanos)
}

/// A tool for synchronizing emulation with a running thread.
#[cfg(not(target_arch = "wasm32"))]
pub struct ThreadSyncTimer {
    pub time: Instant,
    pub frame_duration: Duration,
}

#[cfg(not(target_arch = "wasm32"))]
// #[allow(clippy::new_without_default)]
impl ThreadSyncTimer {
    /// Pass the real time duration of a single time frame (usually a video frame) being emulated.
    pub fn new(frame_duration_nanos: u32) -> Self {
        let frame_duration = Duration::from_nanos(frame_duration_nanos as u64);
        ThreadSyncTimer { time: Instant::now(), frame_duration }
    }
    /// Restarts the internal timer, usefull for resuming paused emulation.
    pub fn restart(&mut self) -> Instant {
        core::mem::replace(&mut self.time, Instant::now())
    }

    pub fn check_frame_elapsed(&mut self) -> Option<Duration> {
        let frame_duration = self.frame_duration;
        let elapsed = self.time.elapsed();
        if let Some(duration) = elapsed.checked_sub(frame_duration) {
            self.time += frame_duration;
            return Some(duration)
        }
        None
    }
    /// This should be called at the end of each frame iteration of an emulation loop to synchronize
    /// thread using [std::thread::sleep].
    ///
    /// Returns `Ok` if the thread is in sync with the emulation or Err<missed_frames> if emulation
    /// is lagging behind.
    pub fn synchronize_thread_to_frame(&mut self) -> core::result::Result<(), u32> {
        let frame_duration = self.frame_duration;
        let elapsed = self.time.elapsed();
        if let Some(duration) = frame_duration.checked_sub(elapsed) {
            std::thread::sleep(duration);
            self.time += frame_duration;
            Ok(())
        }
        else {
            let missed_frames = (elapsed.as_secs_f64() / frame_duration.as_secs_f64()).trunc() as u32;
            // self.time += frame_duration * missed_frames;
            self.restart();
            Err(missed_frames)
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub struct AnimationFrameSyncTimer {
    pub time: f64,
    pub frame_duration: f64,
}

#[cfg(target_arch = "wasm32")]
impl AnimationFrameSyncTimer {
    const MAX_FRAME_LAG_THRESHOLD: u32 = 5;
    /// Pass the value of DOMHighResTimeStamp and the real time duration of a single time frame 
    /// (usually a video frame) being emulated.
   pub fn new(time: f64, frame_duration_nanos: u32) -> Self {
        const NANOS_PER_SEC: f64 = 1_000_000_000.0;
        let frame_duration = frame_duration_nanos as f64 / NANOS_PER_SEC;
        AnimationFrameSyncTimer { time, frame_duration }
    }
    /// Restarts the internal timer, usefull for resuming paused emulation.
    /// Pass the value of DOMHighResTimeStamp.
    pub fn restart(&mut self, time: f64) -> f64 {
        core::mem::replace(&mut self.time, time)
    }
    /// Returns `Ok(number_of_frames)` required to be rendered to synchronize with the native animation frame rate.
    ///
    /// Pass the value of DOMHighResTimeStamp obtained from a callback argument invoked by `request_animation_frame`.
    ///
    /// Returns `Err(time)` if the time elapsed between this and previous call to this method is larger
    /// than the duration of number of frames specified by [MAX_FRAME_LAG_THRESHOLD]. In this instance the
    /// previous value of `time` is returned.
    pub fn num_frames_to_synchronize(&mut self, time: f64) -> core::result::Result<u32, f64> {
        let frame_duration = self.frame_duration;
        let time_elapsed = time - self.time;
        if time_elapsed > frame_duration {
            let nframes = (time_elapsed / frame_duration).trunc() as u32;
            if nframes <= Self::MAX_FRAME_LAG_THRESHOLD {
                self.time = frame_duration.mul_add(nframes as f64, self.time);
                Ok(nframes)
            }
            else {
                Err(self.restart(time))
            }
        }
        else {
            Ok(0)
        }
            
    }
}