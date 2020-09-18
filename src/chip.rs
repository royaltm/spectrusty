/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
//! Chipset emulation building blocks and implementations.
pub mod ula;
pub mod ula128;
pub mod ula3;
pub mod scld;
pub mod plus;
#[cfg(feature = "peripherals")]
pub mod ay_player;
use crate::memory::{ZxMemory, PagedMemory8k};
use crate::video::{VideoFrame, Video};
use crate::clock::FTs;
use crate::peripherals::KeyboardInterface;
use ula::{Ula, UlaVideoFrame, UlaNTSC, UlaNTSCVidFrame};
use ula128::{Ula128, Ula128VidFrame};
use ula3::Ula3;
use scld::Scld;
use plus::UlaPlus;
pub use spectrusty_core::chip::*;

/// ZX Spectrum PAL configuration parameters.
pub struct ZxSpectrumPALConfig;
impl HostConfig for ZxSpectrumPALConfig {
    const CPU_HZ: u32 = 3_500_000;
    const FRAME_TSTATES: FTs = UlaVideoFrame::FRAME_TSTATES_COUNT;
}

/// ZX Spectrum NTSC configuration parameters.
pub struct ZxSpectrumNTSCConfig;
impl HostConfig for ZxSpectrumNTSCConfig {
    const CPU_HZ: u32 = 3_527_500;
    const FRAME_TSTATES: FTs = UlaNTSCVidFrame::FRAME_TSTATES_COUNT;
}

/// ZX Spectrum 128k/+2/+2A/+3 configuration parameters.
pub struct ZxSpectrum128Config;
impl HostConfig for ZxSpectrum128Config {
    const CPU_HZ: u32 = 3_546_900;
    const FRAME_TSTATES: FTs = Ula128VidFrame::FRAME_TSTATES_COUNT;
}

/// A grouping trait of all common control traits for all emulated `Ula` chipsets except audio rendering.
///
/// For audio rendering see [crate::audio::UlaAudioFrame].
pub trait UlaCommon: UlaControl
                   + FrameState
                   + ControlUnit
                   + MemoryAccess
                   + Video
                   + KeyboardInterface
                   + EarIn
                   + for<'a> MicOut<'a> {}

/// Additional function access methods for Ula chipsets.
pub trait UlaControl {
    /// Returns the state of the "late timings" mode.
    fn has_late_timings(&self) -> bool;
    /// Sets the "late timings" mode on or off.
    ///
    /// In this mode interrupts are being requested just one T-state earlier than normally.
    /// This results in all other timings being one T-state later.
    fn set_late_timings(&mut self, late_timings: bool);
    /// Returns the last value sent to the memory port `0x7FFD` if supported.
    fn ula128_mem_port_value(&self) -> Option<Ula128MemFlags> { None }
    /// Sets the current value of the memory port `0x7FFD`. Returns `true` if supported.
    /// Otherwise, returns `false` and no writing is performed.
    fn set_ula128_mem_port_value(&mut self, _value: Ula128MemFlags) -> bool { false }
    /// Returns the last value sent to the memory port `0x1FFD` if supported.
    fn ula3_ctrl_port_value(&self) -> Option<Ula3CtrlFlags> { None }
    /// Sets the current value of the memory port `0x1FFD`. Returns `true` if supported.
    /// Otherwise, returns `false` and no writing is performed.
    fn set_ula3_ctrl_port_value(&mut self, _value: Ula3CtrlFlags) -> bool { false }
    /// Returns the last value sent to the memory port `0xFF`.
    fn scld_ctrl_port_value(&self) -> Option<ScldCtrlFlags> { None }
    /// Sets the current value of the memory port `0xFF`. Returns `true` if supported.
    /// Otherwise, returns `false` and no writing is performed.
    fn set_scld_ctrl_port_value(&mut self, _value: ScldCtrlFlags) -> bool { false }
    /// Returns the last value sent to the memory port `0xF4`.
    fn scld_mmu_port_value(&self) -> Option<u8> { None }
    /// Sets the current value of the memory port `0xF4`. Returns `true` if supported.
    /// Otherwise, returns `false` and no writing is performed.
    fn set_scld_mmu_port_value(&mut self, _value: u8) -> bool { false }
    /// Returns the last value sent to the register port `0xBF3B`.
    fn ulaplus_reg_port_value(&self) -> Option<UlaPlusRegFlags> { None }
    /// Sets the current value of the memory port `0xBF3B`. Returns `true` if supported.
    /// Otherwise, returns `false` and no writing is performed.
    fn set_ulaplus_reg_port_value(&mut self, _value: UlaPlusRegFlags) -> bool { false }
    /// Returns the last value sent to the data port `0xFF3B`.
    fn ulaplus_data_port_value(&self) -> Option<u8> { None }
    /// Sets the current value of the memory port `0xFF3B`. Returns `true` if supported.
    /// Otherwise, returns `false` and no writing is performed.
    fn set_ulaplus_data_port_value(&mut self, _value: u8) -> bool { false }
}

impl<M: ZxMemory, B, X> HostConfig for Ula<M, B, X, UlaVideoFrame> {
    const CPU_HZ: u32 = ZxSpectrumPALConfig::CPU_HZ;
    const FRAME_TSTATES: FTs = <Self as Video>::VideoFrame::FRAME_TSTATES_COUNT;
}

impl<M: PagedMemory8k, B, X> HostConfig for Scld<M, B, X, UlaVideoFrame> {
    const CPU_HZ: u32 = ZxSpectrumPALConfig::CPU_HZ;
    const FRAME_TSTATES: FTs = <Self as Video>::VideoFrame::FRAME_TSTATES_COUNT;
}

impl<M: ZxMemory, B, X> HostConfig for UlaNTSC<M, B, X> {
    const CPU_HZ: u32 = ZxSpectrumNTSCConfig::CPU_HZ;
    const FRAME_TSTATES: FTs = <Self as Video>::VideoFrame::FRAME_TSTATES_COUNT;
}

impl<B, X> HostConfig for Ula128<B, X> {
    const CPU_HZ: u32 = ZxSpectrum128Config::CPU_HZ;
    const FRAME_TSTATES: FTs = <Self as Video>::VideoFrame::FRAME_TSTATES_COUNT;
}

impl<B, X> HostConfig for Ula3<B, X> {
    const CPU_HZ: u32 = ZxSpectrum128Config::CPU_HZ;
    const FRAME_TSTATES: FTs = <Self as Video>::VideoFrame::FRAME_TSTATES_COUNT;
}

impl<U: HostConfig + Video> HostConfig for UlaPlus<U> {
    const CPU_HZ: u32 = U::CPU_HZ;
    const FRAME_TSTATES: FTs = U::FRAME_TSTATES;
}

impl<U> UlaCommon for U
    where U: UlaControl
           + FrameState
           + ControlUnit
           + MemoryAccess
           + Video
           + KeyboardInterface
           + EarIn
           + for<'a> MicOut<'a>
{}
