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
use ula::{Ula, UlaVideoFrame, UlaNTSC, UlaNTSCVidFrame};
use ula128::{Ula128, Ula128VidFrame};
use ula3::Ula3;
use scld::Scld;
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
