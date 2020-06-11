//! Chipset emulation building blocks.
use core::num::NonZeroU32;
use core::time::Duration;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

use z80emu::{CpuDebug, Cpu, host::Result};

use crate::bus::BusDevice;
use crate::clock::FTs;
use crate::memory::{ZxMemory, MemoryExtension};

mod flags;
pub use flags::*;

/// A trait for directly accessing an emulated memory implementation and memory extensions.
pub trait MemoryAccess {
    type Memory: ZxMemory;
    type MemoryExt: MemoryExtension;
    /// Returns a read-only reference to the memory extension.
    fn memory_ext_ref(&self) -> &Self::MemoryExt;
    /// Returns a mutable reference to the memory extension.
    fn memory_ext_mut(&mut self) -> &mut Self::MemoryExt;
    /// Returns a reference to the memory.
    fn memory_ref(&self) -> &Self::Memory;    
    /// Returns a mutable reference to the memory.
    fn memory_mut(&mut self) -> &mut Self::Memory;
}

/// A trait for controling an emulated chipset.
pub trait ControlUnit {
    /// The type of the first attached [BusDevice].
    type BusDevice: BusDevice;
    /// Returns a mutable reference to the first bus device.
    fn bus_device_mut(&mut self) -> &mut Self::BusDevice;
    /// Returns a reference to the first bus device.
    fn bus_device_ref(&self) -> &Self::BusDevice;
    /// Destructs self and returns the instance of the bus device.
    fn into_bus_device(self) -> Self::BusDevice;
    /// Returns the value of the current execution frame counter. The [ControlUnit] implementation should
    /// count passing frames infinitely wrapping at 2^64.
    fn current_frame(&self) -> u64;
    /// Returns a normalized frame counter and a T-state counter values.
    ///
    /// T-states are counted from 0 at the start of each frame.
    /// This method never returns the T-state counter value below 0 or past the frame counter limit.
    fn frame_tstate(&self) -> (u64, FTs);
    /// Returns the current value of the T-state counter.
    /// 
    /// Unlike [ControlUnit::frame_tstate] values return by this method can sometimes be negative as well as
    /// exceeding the maximum nuber of T-states per frame.
    fn current_tstate(&self) -> FTs;
    /// Returns `true` if the value of the current T-state counter has reached a certain arbitrary limit which
    /// is very close to the maximum number of T-states per frame.
    fn is_frame_over(&self) -> bool;
    /// Performs a system reset.
    ///
    /// When `hard` is:
    /// * `true` emulates a **RESET** signal being active for the `cpu` and all bus devices.
    /// * `false` executes a `RST 0` instruction on the `cpu` but without forwarding the clock counter.
    ///
    /// In any case this operation is always instant.
    fn reset<C: Cpu>(&mut self, cpu: &mut C, hard: bool);
    /// Triggers a non-maskable interrupt. Returns `true` if the **NMI** was successfully executed.
    ///
    /// Returns `false` when the `cpu` has just executed an `EI` instruction or one of `0xDD`, `0xFD` prefixes.
    /// In this instance this is a no-op.
    ///
    /// For more details see [z80emu::Cpu::nmi].
    fn nmi<C: Cpu>(&mut self, cpu: &mut C) -> bool;
    /// Conditionally prepares the internal state for the next frame and executes instructions on the `cpu` as fast
    /// as possible untill the near end of that frame.
    fn execute_next_frame<C: Cpu>(&mut self, cpu: &mut C);
    /// Conditionally prepares the internal state for the next frame, advances the frame counter and wraps the T-state
    /// counter if it is near the end of a frame.
    ///
    /// This method should be called after all side effects (e.g. video and audio rendering) has been performed.
    /// Implementations would usually clear some internal data from a previous frame.
    ///
    /// Both [ControlUnit::execute_next_frame] and [ControlUnit::execute_single_step] invoke this method internally,
    /// so the only reason to call this method from the emulator program would be to make sure internal buffers are
    /// empty before feeding the implementation with external data to be consumed by devices during the next frame
    /// or to serialize the structure.
    fn ensure_next_frame(&mut self);
    /// Executes a single instruction on the `cpu` with the option to pass a debugging function.
    ///
    /// If the T-state counter value is near the end of a frame prepares the internal state for the next frame
    /// before executing the next instruction.
    fn execute_single_step<C: Cpu,
                           F: FnOnce(CpuDebug)>(
            &mut self,
            cpu: &mut C,
            debug: Option<F>
    ) -> Result<(), ()>;
}

/// A trait for reading the MIC line output.
pub trait MicOut<'a> {
    type PulseIter: Iterator<Item=NonZeroU32> + 'a;
    /// Returns a frame buffered MIC output as a pulse iterator.
    fn mic_out_pulse_iter(&'a self) -> Self::PulseIter;
}

/// A trait for feeding the EAR line input.
pub trait EarIn {
    /// Sets `EAR in` bit state after the provided interval in ∆ T-States counted from the last recorded change.
    fn set_ear_in(&mut self, ear_in: bool, delta_fts: u32);
    /// Feeds the `EAR in` buffer with changes.
    ///
    /// The provided iterator should yield time intervals measured in T-state ∆ differences after which the state
    /// of the `EAR in` bit should be toggled.
    ///
    /// `max_frames_threshold` may be optionally provided as a number of frames to limit the buffered changes.
    /// This is usefull if the given iterator provides data largely exceeding the duration of a single frame.
    fn feed_ear_in<I: Iterator<Item=NonZeroU32>>(&mut self, fts_deltas: I, max_frames_threshold: Option<usize>);
    /// Removes all buffered so far `EAR in` changes.
    ///
    /// Changes are usually consumed only when a call is made to [crate::chip::ControlUnit::ensure_next_frame].
    /// Provide the current value of `EAR in` bit as `ear_in`.
    ///
    /// This may be usefull when tape data is already buffered but the user decided to stop the tape playback
    /// immediately.
    fn purge_ear_in_changes(&mut self, ear_in: bool);
    /// Returns the counter of how many times the EAR input line was read since the beginning of the current frame.
    ///
    /// This can be used to help implementing the auto loading of tape data.
    fn read_ear_in_count(&self) -> u32;
    /// Returns the current mode.
    fn read_ear_mode(&self) -> ReadEarMode {
        ReadEarMode::Clear
    }
    /// Changes the current mode.
    fn set_read_ear_mode(&mut self, _mode: ReadEarMode) {}
}

/// A helper trait for accessing parameters of well known host configurations.
pub trait HostConfig {
    /// The number of CPU cycles (T-states) per second.
    const CPU_HZ: u32;
    /// The number of CPU cycles (T-states) in a single execution frame.
    const FRAME_TSTATES: FTs;
    /// Returns the CPU rate (T-states / second) after multiplying it by the `multiplier`.
    #[inline]
    fn effective_cpu_rate(multiplier: f64) -> f64 {
        Self::CPU_HZ as f64 * multiplier
    }
    /// Returns the duration of a single execution frame in nanoseconds after multiplying
    /// the CPU rate by the `multiplier`.
    #[inline]
    fn effective_frame_duration_nanos(multiplier: f64) -> u32 {
        let cpu_rate = Self::effective_cpu_rate(multiplier).round() as u32;
        nanos_from_frame_tc_cpu_hz(Self::FRAME_TSTATES as u32, cpu_rate) as u32
    }
    /// Returns the duration of a single execution frame after multiplying the CPU rate by
    /// the `multiplier`.
    #[inline]
    fn effective_frame_duration(multiplier: f64) -> Duration {
        let cpu_rate = Self::effective_cpu_rate(multiplier).round() as u32;
        duration_from_frame_tc_cpu_hz(Self::FRAME_TSTATES as u32, cpu_rate)
    }
    /// Returns the duration of a single execution frame in nanoseconds.
    #[inline]
    fn frame_duration_nanos() -> u32 {
        nanos_from_frame_tc_cpu_hz(Self::FRAME_TSTATES as u32, Self::CPU_HZ) as u32
    }
    /// Returns the duration of a single execution frame.
    #[inline]
    fn frame_duration() -> Duration {
        duration_from_frame_tc_cpu_hz(Self::FRAME_TSTATES as u32, Self::CPU_HZ)
    }
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
    /// The start time of a current synchronization period.
    pub time: Instant,
    /// The desired duration of a single synchronization period.
    pub frame_duration: Duration,
}

#[cfg(not(target_arch = "wasm32"))]
// #[allow(clippy::new_without_default)]
impl ThreadSyncTimer {
    /// Pass the real time duration of a desired synchronization period (usually a duration of a video frame).
    pub fn new(frame_duration_nanos: u32) -> Self {
        let frame_duration = Duration::from_nanos(frame_duration_nanos as u64);
        ThreadSyncTimer { time: Instant::now(), frame_duration }
    }
    /// Sets [ThreadSyncTimer::frame_duration] from the provided `frame_duration_nanos`.
    pub fn set_frame_duration(&mut self, frame_duration_nanos: u32) {
        self.frame_duration = Duration::from_nanos(frame_duration_nanos as u64);
    }
    /// Restarts the synchronization period. Usefull e.g. for resuming paused emulation.
    pub fn restart(&mut self) -> Instant {
        core::mem::replace(&mut self.time, Instant::now())
    }
    /// Calculates the difference between the desired duration of a synchronization period
    /// and the real time that has passed from the start of the current period and levels
    /// the difference by calling [std::thread::sleep].
    ///
    /// This method may be called at the end of each iteration of an emulation loop to
    /// synchronize the running thread with a desired iteration period.
    ///
    /// Returns `Ok` if the thread is in sync with the emulation. In this instance the value
    /// of [ThreadSyncTimer::frame_duration] is being added to [ThreadSyncTimer::time] to mark
    /// the beginning of a new period.
    ///
    /// Returns `Err(missed_periods)` if the elapsed time exceeds the desired period duration.
    /// In this intance the start of a new period is set to [Instant::now].
    pub fn synchronize_thread_to_frame(&mut self) -> core::result::Result<(), u32> {
        let frame_duration = self.frame_duration;
        let now = Instant::now();
        let elapsed = now.duration_since(self.time);
        if let Some(duration) = frame_duration.checked_sub(elapsed) {
            std::thread::sleep(duration);
            self.time += frame_duration;
            Ok(())
        }
        else {
            let missed_frames = (elapsed.as_secs_f64() / frame_duration.as_secs_f64()).trunc() as u32;
            self.time = now;
            Err(missed_frames)
        }
    }
    /// Returns `Some(excess_duration)` if the time elapsed from the beginning of the current
    /// period exceeds or is equal to the desired duration of a synchronization period.
    /// Otherwise returns `None`.
    ///
    /// The value returned is the excess time that has elapsed above the desired duration.
    /// If the time elapsed equals to the `frame_duration` the returned value equals to zero.
    ///
    /// In case `Some` variant is returned the value of [ThreadSyncTimer::frame_duration] is
    /// being added to [ThreadSyncTimer::time] to mark the beginning of a new period.
    pub fn check_frame_elapsed(&mut self) -> Option<Duration> {
        let frame_duration = self.frame_duration;
        let elapsed = self.time.elapsed();
        if let Some(duration) = elapsed.checked_sub(frame_duration) {
            self.time += frame_duration;
            return Some(duration)
        }
        None
    }
}

#[cfg(target_arch = "wasm32")]
pub struct AnimationFrameSyncTimer {
    pub time: f64,
    pub frame_duration_millis: f64,
}

#[cfg(target_arch = "wasm32")]
const NANOS_PER_MILLISEC: f64 = 1_000_000.0;

#[cfg(target_arch = "wasm32")]
impl AnimationFrameSyncTimer {
    const MAX_FRAME_LAG_THRESHOLD: u32 = 10;
    /// Pass the `DOMHighResTimeStamp` as a first and the real time duration of a desired synchronization
    /// period (usually a duration of a video frame) as a second argument.
    pub fn new(time: f64, frame_duration_nanos: u32) -> Self {
        let frame_duration_millis = frame_duration_nanos as f64 / NANOS_PER_MILLISEC;
        AnimationFrameSyncTimer { time, frame_duration_millis }
    }
     /// Sets [AnimationFrameSyncTimer::frame_duration_millis] from the provided `frame_duration_nanos`.
    pub fn set_frame_duration(&mut self, frame_duration_nanos: u32) {
        self.frame_duration_millis = frame_duration_nanos as f64 / NANOS_PER_MILLISEC;
    }
   /// Restarts the synchronizaiton period. Usefull for e.g. resuming paused emulation.
    ///
    /// Pass the value of `DOMHighResTimeStamp` as the argument.
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
        let frame_duration_millis = self.frame_duration_millis;
        let time_elapsed = time - self.time;
        if time_elapsed > frame_duration_millis {
            let nframes = (time_elapsed / frame_duration_millis).trunc() as u32;
            if nframes <= Self::MAX_FRAME_LAG_THRESHOLD {
                self.time = frame_duration_millis.mul_add(nframes as f64, self.time);
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
    /// Returns `Some(excess_duration_millis)` if the `time` elapsed from the beginning of the current
    /// period exceeds or is equal to the desired duration of a synchronization period.
    /// Otherwise returns `None`.
    ///
    /// The value returned is the excess time that has elapsed above the desired duration.
    /// If the time elapsed equals to the `frame_duration` the returned value equals to zero.
    ///
    /// In case `Some` variant is returned the value of [AnimationFrameSyncTimer::frame_duration] is
    /// being added to [AnimationFrameSyncTimer::time] to mark the beginning of a new period.
   pub fn check_frame_elapsed(&mut self, time: f64) -> Option<f64> {
        let frame_duration_millis = self.frame_duration_millis;
        let elapsed = time - self.time;
        let excess_duration_millis = elapsed - frame_duration_millis;
        if excess_duration_millis >= 0.0 {
            self.time += frame_duration_millis;
            return Some(excess_duration_millis)
        }
        None
    }
}
