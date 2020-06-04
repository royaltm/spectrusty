//! Chipset emulation building blocks.
use core::fmt;
use core::convert::TryFrom;
use core::num::NonZeroU32;
use core::str::FromStr;
use core::time::Duration;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

use bitflags::bitflags;

#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};

use z80emu::{CpuDebug, Cpu, host::Result};

use crate::bus::BusDevice;
use crate::clock::{FTs, VideoTs, VideoTsData2};
use crate::memory::{ZxMemory, MemoryExtension};

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
    /// exceeding the maximum nuber of T-states per frame. See [ControlUnit::execute_next_frame] to learn why.
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
    /// If the T-state counter nears the end of a frame prepares the internal state for the next frame before
    /// executing the next instruction.
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

/// This enum determines the EAR input (bit 6) read from the 0xFE port when there is no EAR input feed.
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadEarMode {
    /// Issue 3 keyboard behaviour - EAR input is 1 when EAR output is 1
    Issue3,
    /// Issue 2 keyboard behaviour - EAR input is 1 when either EAR or MIC output is 1
    Issue2,
    /// Always clear - EAR input is always 0
    Clear
}

#[derive(Clone, Debug)]
pub struct ParseReadEarModeError;

bitflags! {
    /// This type represents packed EAR and MIC output data.
    #[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "snapshot", serde(try_from = "u8", into = "u8"))]
    #[derive(Default)]
    pub struct EarMic: u8 {
        const MIC    = 0b01;
        const EAR    = 0b10;
        const EARMIC = 0b11;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TryFromU8EarMicError(pub u8);

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

bitflags! {
    /// ZX Spectrum 128K / +2 memory control flags and +2A / +3 primary control flags.
    #[derive(Default)]
    pub struct Ula128MemFlags: u8 {
        const RAM_BANK_MASK = 0b00_0111;
        const SCREEN_BANK   = 0b00_1000;
        const ROM_BANK      = 0b01_0000;
        const LOCK_MMU      = 0b10_0000;
    }
}

impl Ula128MemFlags {
    /// Returns modified flags with the last memory page RAM bank index set to `bank`.
    pub fn with_last_ram_page_bank(self, bank: usize) -> Self {
        (self & !Ula128MemFlags::RAM_BANK_MASK) |
        (Ula128MemFlags::from_bits_truncate(bank as u8) & Ula128MemFlags::RAM_BANK_MASK)
    }
    /// Returns a RAM bank index mapped at the last memory page.
    pub fn last_ram_page_bank(self) -> usize {
        (self & Ula128MemFlags::RAM_BANK_MASK).bits().into()
    }
    /// Returns a ROM bank index mapped at the first memory page.
    pub fn rom_page_bank(self) -> usize {
        self.intersects(Ula128MemFlags::ROM_BANK).into()
    }
    /// Returns `true` if a shadow screen bank bit is 1. Otherwise returns `false`.
    pub fn is_shadow_screen(self) -> bool {
        self.intersects(Ula128MemFlags::SCREEN_BANK)
    }
    /// Returns `true` if a mmu lock bit is 1. Otherwise returns `false`.
    pub fn is_mmu_locked(self) -> bool {
        self.intersects(Ula128MemFlags::LOCK_MMU)
    }
}

bitflags! {
    /// ZX Spectrum +2A / +3 secondary control flags.
    #[derive(Default)]
    pub struct Ula3CtrlFlags: u8 {
        const EXT_PAGING       = 0b0_0001;
        const PAGE_LAYOUT_MASK = 0b0_0110;
        const ROM_BANK_HI      = 0b0_0100;
        const DISC_MOTOR       = 0b0_1000;
        const PRINTER_STROBE   = 0b1_0000;
        const PAGE_LAYOUT0     = 0b0_0000;
        const PAGE_LAYOUT1     = 0b0_0010;
        const PAGE_LAYOUT2     = 0b0_0100;
        const PAGE_LAYOUT3     = 0b0_0110;
    }
}

impl Ula3CtrlFlags {
    /// Returns modified flags with the special paging enabled and its layout set to the specified value.
    pub fn with_special_paging(self, paging: Ula3Paging) -> Self {
        ((self | Ula3CtrlFlags::EXT_PAGING) & !Ula3CtrlFlags::PAGE_LAYOUT_MASK)
        | match paging {
            Ula3Paging::Banks0123 => Ula3CtrlFlags::PAGE_LAYOUT0,
            Ula3Paging::Banks4567 => Ula3CtrlFlags::PAGE_LAYOUT1,
            Ula3Paging::Banks4563 => Ula3CtrlFlags::PAGE_LAYOUT2,
            Ula3Paging::Banks4763 => Ula3CtrlFlags::PAGE_LAYOUT3,
        }
    }
    /// Returns modified flags with the special paging disabled and with the rom bank high flag set from
    /// the `rom_bank` bit1.
    pub fn with_rom_page_bank_hi(mut self, rom_bank: usize) -> Self {
        self.remove(Ula3CtrlFlags::EXT_PAGING);
        self.set(Ula3CtrlFlags::ROM_BANK_HI, rom_bank & 2 != 0);
        self
    }
    /// Returns `true` if a special paging is enabled. Otherwise returns `false`.
    pub fn has_special_paging(self) -> bool {
        self.intersects(Ula3CtrlFlags::EXT_PAGING)
    }
    /// Returns a special paging layout if it is enabled.
    pub fn special_paging(self) -> Option<Ula3Paging> {
        if self.has_special_paging() {
            Some(match self & Ula3CtrlFlags::PAGE_LAYOUT_MASK {
                Ula3CtrlFlags::PAGE_LAYOUT0 => Ula3Paging::Banks0123,
                Ula3CtrlFlags::PAGE_LAYOUT1 => Ula3Paging::Banks4567,
                Ula3CtrlFlags::PAGE_LAYOUT2 => Ula3Paging::Banks4563,
                Ula3CtrlFlags::PAGE_LAYOUT3 => Ula3Paging::Banks4763,
                _ => unreachable!()
            })
        }
        else {
            None
        }
    }
    /// Returns a bit 1 value of rom bank index mapped at the first RAM page.
    ///
    /// The complete rom page bank index can be obtained by bitwise ORing the returned value with
    /// the result from [Ula128MemFlags::rom_page_bank].
    pub fn rom_page_bank_hi(self) -> usize {
        ((self & Ula3CtrlFlags::ROM_BANK_HI).bits() >> 1).into()
    }
    /// Returns `true` if a disc motor bit is 1. Otherwise returns `false`.
    pub fn is_disc_motor_on(self) -> bool {
        self.intersects(Ula3CtrlFlags::DISC_MOTOR)
    }
    /// Returns `true` if a printer strobe bit is 1. Otherwise returns `false`.
    pub fn is_printer_strobe_on(self) -> bool {
        self.intersects(Ula3CtrlFlags::PRINTER_STROBE)
    }
}

bitflags! {
    /// Timex TC2048/TC2068/TS2068 memory control flags.
    #[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "snapshot", serde(from = "u8", into = "u8"))]
    #[derive(Default)]
    pub struct ScldCtrlFlags: u8 {
        const SCREEN_MODE_MASK   = 0b0000_0111;
        const SCREEN_SOURCE_MASK = 0b0000_0011;
        const SCREEN_SHADOW      = 0b0000_0001;
        const SCREEN_HI_ATTRS    = 0b0000_0010;
        const SCREEN_HI_RES      = 0b0000_0100;
        const HIRES_COLOR_MASK   = 0b0011_1000;
        const INTR_DISABLED      = 0b0100_0000;
        const MAP_EX_ROM         = 0b1000_0000;
    }
}

impl ScldCtrlFlags {
    /// Returns a color index from high resolution color mask: [0, 7].
    #[inline]
    pub fn hires_color_index(self) -> u8 {
        (self & ScldCtrlFlags::HIRES_COLOR_MASK).bits() >> 3
    }
    /// Returns `true` if a map ex rom bit is 1. Otherwise returns `false`.
    #[inline]
    pub fn is_map_ex_rom(self) -> bool {
        self.intersects(ScldCtrlFlags::MAP_EX_ROM)
    }
    /// Returns `true` if a interrupt disabled bit is 1. Otherwise returns `false`.
    #[inline]
    pub fn is_intr_disabled(self) -> bool {
        self.intersects(ScldCtrlFlags::INTR_DISABLED)
    }
    /// Returns `true` if a shadow screen bit is 1. Otherwise returns `false`.
    #[inline]
    pub fn is_screen_shadow(self) -> bool {
        self.intersects(ScldCtrlFlags::SCREEN_SHADOW)
    }
    /// Returns `true` if a screen high resolution bit is 1. Otherwise returns `false`.
    #[inline]
    pub fn is_screen_hi_res(self) -> bool {
        self.intersects(ScldCtrlFlags::SCREEN_HI_RES)
    }
    /// Returns `true` if a screen high resolution attributes bit is 1. Otherwise returns `false`.
    #[inline]
    pub fn is_screen_hi_attrs(self) -> bool {
        self.intersects(ScldCtrlFlags::SCREEN_HI_ATTRS)
    }
}

impl From<ScldCtrlFlags> for u8 {
    #[inline]
    fn from(flags: ScldCtrlFlags) -> u8 {
        flags.bits()
    }
}

impl From<u8> for ScldCtrlFlags {
    #[inline]
    fn from(flags: u8) -> ScldCtrlFlags {
        ScldCtrlFlags::from_bits_truncate(flags)
    }
}

/// Convert into a video timestamped source mode change.
impl From<(VideoTs, ScldCtrlFlags)> for VideoTsData2 {
    fn from((vts, flags): (VideoTs, ScldCtrlFlags)) -> VideoTsData2 {
        let source_mode = (flags & (ScldCtrlFlags::SCREEN_SHADOW|ScldCtrlFlags::SCREEN_HI_ATTRS)).bits();
        (vts, source_mode).into()
    }
}

bitflags! {
    #[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "snapshot", serde(try_from = "u8", into = "u8"))]
    #[derive(Default)]
    pub struct ColorMode: u8 {
        const PALETTE   = 0b001;
        const GRAYSCALE = 0b010;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TryFromU8ColorModeError(pub u8);

impl std::error::Error for TryFromU8ColorModeError {}

impl fmt::Display for TryFromU8ColorModeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "converted integer ({}) out of range for `ColorMode`", self.0)
    }
}

impl TryFrom<u8> for ColorMode {
    type Error = TryFromU8ColorModeError;
    fn try_from(mode: u8) -> core::result::Result<Self, Self::Error> {
        ColorMode::from_bits(mode).ok_or_else(|| TryFromU8ColorModeError(mode))
    }
}

impl From<ColorMode> for u8 {
    #[inline]
    fn from(mode: ColorMode) -> u8 {
        mode.bits()
    }
}

#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "snapshot", serde(try_from = "u8", into = "u8"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Ula3Paging {
    Banks0123 = 0,
    Banks4567 = 1,
    Banks4563 = 2,
    Banks4763 = 3,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TryFromU8Ula3PagingError(pub u8);

impl From<Ula3Paging> for u8 {
    fn from(paging: Ula3Paging) -> u8 {
        paging as u8
    }
}

impl std::error::Error for TryFromU8Ula3PagingError {}

impl fmt::Display for TryFromU8Ula3PagingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "converted integer ({}) out of range for `Ula3Paging`", self.0)
    }
}

impl TryFrom<u8> for Ula3Paging {
    type Error = TryFromU8Ula3PagingError;
    fn try_from(bank: u8) -> core::result::Result<Self, Self::Error> {
        Ok(match bank {
            0 => Ula3Paging::Banks0123,
            1 => Ula3Paging::Banks4567,
            2 => Ula3Paging::Banks4563,
            3 => Ula3Paging::Banks4763,
            _ => return Err(TryFromU8Ula3PagingError(bank))
        })
    }
}

impl Ula3Paging {
    const RAM_BANKS0: [u8;4] = [0, 1, 2, 3];
    const RAM_BANKS1: [u8;4] = [4, 5, 6, 7];
    const RAM_BANKS2: [u8;4] = [4, 5, 6, 3];
    const RAM_BANKS3: [u8;4] = [4, 7, 6, 3];
    /// Returns a reference to an array of RAM bank indexes for a current paging variant layout.
    pub fn ram_banks(self) -> &'static [u8] {
        match self {
            Ula3Paging::Banks0123 => &Self::RAM_BANKS0,
            Ula3Paging::Banks4567 => &Self::RAM_BANKS1,
            Ula3Paging::Banks4563 => &Self::RAM_BANKS2,
            Ula3Paging::Banks4763 => &Self::RAM_BANKS3,
        }
    }
    /// Returns an iterator of tuples of RAM bank indexes with memory page indexes for a current
    /// paging variant layout.
    pub fn ram_banks_with_pages_iter(self) -> impl Iterator<Item=(usize, u8)> {
        self.ram_banks().iter().map(|&bank| bank as usize).zip(0..4)
    }
}

impl std::error::Error for TryFromU8EarMicError {}

impl fmt::Display for TryFromU8EarMicError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "converted integer ({}) out of range for `EarMic`", self.0)
    }
}

impl TryFrom<u8> for EarMic {
    type Error = TryFromU8EarMicError;
    fn try_from(earmic: u8) -> core::result::Result<Self, Self::Error> {
        EarMic::from_bits(earmic).ok_or_else(|| TryFromU8EarMicError(earmic))
    }
}

impl From<EarMic> for u8 {
    fn from(earmic: EarMic) -> u8 {
        earmic.bits()
    }
}

impl From<ReadEarMode> for &str {
    fn from(mode: ReadEarMode) -> Self {
        match mode {
            ReadEarMode::Issue3 => "Issue 3",
            ReadEarMode::Issue2 => "Issue 2",
            ReadEarMode::Clear => "Clear"
        }
    }
}

impl fmt::Display for ReadEarMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        <&str>::from(*self).fmt(f)
    }
}

impl std::error::Error for ParseReadEarModeError {}

impl fmt::Display for ParseReadEarModeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cannot parse `ReadEarMode`: unrecognized string")
    }
}

impl FromStr for ReadEarMode {
    type Err = ParseReadEarModeError;
    fn from_str(mode: &str) -> core::result::Result<Self, Self::Err> {
        if mode.eq_ignore_ascii_case("issue 3") {
            Ok(ReadEarMode::Issue3)
        }
        else if mode.eq_ignore_ascii_case("issue 2") {
            Ok(ReadEarMode::Issue2)
        }
        else if mode.eq_ignore_ascii_case("clear") {
            Ok(ReadEarMode::Clear)
        }
        else {
            Err(ParseReadEarModeError)
        }
    }
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