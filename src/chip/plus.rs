/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
/*! An emulator of the ULAplus enhanced graphics.

This emulator wraps one of the ULA chipset emulators that implements [UlaPlusInner] trait and enhances its graphics
with ULAplus capabilities.

The [UlaPlus] chipset responds to the ports (fully decoded):
* `0xFF` - the main screen mode port (read and write, has priority),
* `0xBF3B` - the register port (write-only),
* `0xFF3B` - the data port (read and write)

and provides the new graphics capabilities according to the ULAplus version 1.1 [specification](https://faqwiki.zxnet.co.uk/wiki/ULAplus).

Implementation specifics (may change in the future releases):

* No HSL or CMYK 1.0 support, instead a grayscale mode has been implemented (1.1, see below for details).
* The new way of multiplexing the 2-bit blue value to obtain a 3-bit blue value (1.1).
* [SCLD][super::scld] (Timex) modes are also supported: secondary screen, hi-color, hi-res, and shadow screen bank.
* Port `0xFF` has priority over the register port. Bits 0 to 5 written to port `0xBF3B` with mode group set
  are bitwise ORed with the bits written previously to port `0xFF`. When in palette mode, bits are not modified.
* Reading of the value previously written to port previously written to port `0xFF` is disabled by default,
  to not confuse some programs (BBC BASIC), that the chipset supports SCLD MMU paging. This can be enabled
  with [UlaPlus::enable_reading_scld_mode].
* The ULAplus capabilities can be disabled or enabled in run time with [UlaPlus::enable_ulaplus_modes].
* Hard reset sets the screen and color mode to the default and sets all palette entries to 0, but doesn't change
  the ULAplus capabilities disable flag.
* The inner chip features and bugs (e.g. floating bus, keyboard issue, snow interference) are carried through,
  and in case of the snow effect, only the primary screens are being affected.
* The "locking" bit of Spectrum's 128k memory paging doesn't affect the ULAplus modes, but it makes the shadow memory
  inaccessible if it's being paged out when the bit is being enabled.
* When used with 48k ULA the shadow screen page is the same as the normal screen page.
* The grayscale mode is applied to all graphic modes: classic spectrum's 15 colors, palette, and high resolution.
* In grayscale mode with palette enabled, each palette entry describes a grayscale intensity from 0 to 255.
* In classic 15 color and high-resolution modes, each color is converted to grayscale based on the color intensity.

In 128k mode, there are four available screen memory areas and in this documentation and source code they are 
referenced as:

| identifier      | 16kb page | address range¹| aliases     | idx²|
|-----------------|-----------|---------------|-------------|-----|
| screen 0        |     5     | 0x0000-0x1AFF | primary     |  0  |
| screen 1        |     5     | 0x2000-0x3AFF | secondary   |  2  |
| shadow screen 0 |     7     | 0x0000-0x1AFF | pri. shadow |  1  |
| shadow screen 1 |     7     | 0x2000-0x3AFF | sec. shadow |  3  |

In 48k mode, there are two available screen memory areas:

| identifier | address range | aliases     | ref²|
|------------|---------------|-------------|-----|
| screen 0   | 0x4000-0x5AFF | primary     |  0  |
| screen 1   | 0x6000-0x7AFF | secondary   |  1  |

¹ - Relative to the beginning of the 16kb page.
² - A screen bank argument for e.g. [ZxMemory::screen_ref] and [ZxMemory::screen_mut].

[ZxMemory::screen_ref]: crate::memory::ZxMemory::screen_ref
[ZxMemory::screen_mut]: crate::memory::ZxMemory::screen_mut
*/
use core::fmt;

#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};

pub mod frame_cache;
mod audio_earmic;
mod io;
mod video;
#[cfg(feature = "formats")]
mod screen;

use crate::z80emu::{*, host::Result};
use crate::bus::BusDevice;
use crate::clock::{
    VFrameTs, VideoTs, VFrameTsCounter, MemoryContention,
    VideoTsData2, VideoTsData6
};
use crate::chip::{
    ControlUnit, MemoryAccess,
    UlaPortFlags, ScldCtrlFlags, UlaPlusRegFlags, ColorMode, Ula128MemFlags, Ula3CtrlFlags,
    UlaControl,
    InnerAccess,
    scld::frame_cache::SourceMode,
    ula::{
        UlaControlExt, UlaCpuExt,
        frame_cache::UlaFrameCache
    },
};
use crate::video::{
    BorderColor, Video, RenderMode, UlaPlusPalette, PaletteChange
};
use crate::memory::MemoryExtension;

/*
  G G i i i h a s UlaPlusRegFlags
  0 0 0 0 0 0 g p ColorMode

  0 0 0 0 0 S s s SourceMode
  0 0 g p h i i i RenderMode
*/

/// Provides ULAplus screen and color modes for [Ula], [Ula128] and [Ula3].
///
/// [Ula]: crate::chip::Ula
/// [Ula128]: crate::chip::Ula128
/// [Ula3]: crate::chip::Ula3
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "snapshot", serde(rename_all = "camelCase"))]
#[derive(Clone)]
pub struct UlaPlus<U: Video> {
    ula: U,
    ulaplus_disabled: bool,
    scld_mode_rw: bool,
    scld_mode: ScldCtrlFlags, // the last entry written
    register_flags: UlaPlusRegFlags, // the last entry written
    color_mode: ColorMode, // the last entry written
    beg_render_mode: RenderMode,
    cur_render_mode: RenderMode,
    beg_source_mode: SourceMode,
    cur_source_mode: SourceMode,
    #[cfg_attr(feature = "snapshot", serde(default))]
    beg_palette: UlaPlusPalette,
    #[cfg_attr(feature = "snapshot", serde(default))]
    cur_palette: UlaPlusPalette,
    #[cfg_attr(feature = "snapshot", serde(skip))]
    sec_frame_cache: UlaFrameCache<U::VideoFrame>,
    #[cfg_attr(feature = "snapshot", serde(skip))]
    shadow_sec_frame_cache: UlaFrameCache<U::VideoFrame>,
    #[cfg_attr(feature = "snapshot", serde(skip))]
    palette_changes: Vec<PaletteChange>,
    #[cfg_attr(feature = "snapshot", serde(skip))]
    mode_changes: Vec<VideoTsData6>,
    #[cfg_attr(feature = "snapshot", serde(skip))]
    source_changes: Vec<VideoTsData2>,
}

/// Implemented by chipsets that UlaPlus can enhance.
pub trait UlaPlusInner<'a>: Video + MemoryAccess {
    type ScreenSwapIter: Iterator<Item=VideoTs> + 'a;
    /// Returns `true` if `port` matches the ULA port.
    fn is_ula_port(port: u16) -> bool {
        port & 1 == 0
    }
    /// Sets the state of EarMic flags and optionally records an EarMic change.
    fn ula_write_earmic(&mut self, flags: UlaPortFlags, ts: VideoTs);
    /// Records a shadow screen swap.
    fn push_screen_change(&mut self, ts: VideoTs);
    /// Updates the border color, returns `true` if the border color has changed.
    fn update_last_border_color(&mut self, border: BorderColor) -> bool;
    /// Returns `Some(is_shadow)` if a screen memory is accessible at page address: 0x4000-0x5FFF.
    fn page1_screen0_shadow_bank(&self) -> Option<bool>;
    /// Returns `Some(is_shadow)` if a screen memory is accessible at page address: 0x6000-0x7FFF.
    fn page1_screen1_shadow_bank(&self) -> Option<bool>;
    /// Returns `Some(is_shadow)` if a screen memory is accessible at page address: 0xC000-0xDFFF.
    fn page3_screen0_shadow_bank(&self) -> Option<bool>;
    /// Returns `Some(is_shadow)` if a screen memory is accessible at page address: 0xE000-0xFFFF.
    fn page3_screen1_shadow_bank(&self) -> Option<bool>;
    /// Returns a mutable reference to the normal frame cache and a memory reference.
    fn frame_cache_mut_mem_ref(&mut self) -> (&mut UlaFrameCache<Self::VideoFrame>, &Self::Memory);
    /// Returns a mutable reference to the shadow frame cache and a memory reference.
    fn shadow_frame_cache_mut_mem_ref(&mut self) -> (&mut UlaFrameCache<Self::VideoFrame>, &Self::Memory);
    /// Returns true if the shadow screen is displayed at the beginning of the video frame.
    fn beg_screen_shadow(&self) -> bool;
    /// Returns true if the shadow screen is currently being displayed.
    fn cur_screen_shadow(&self) -> bool;
    /// Returns references to components necessary for video rendering.
    fn video_render_data_view(
        &'a mut self
    ) -> (
        Self::ScreenSwapIter,
        &'a Self::Memory,
        &'a UlaFrameCache<Self::VideoFrame>,
        &'a UlaFrameCache<Self::VideoFrame>
    );
}

impl<U> Default for UlaPlus<U>
    where U: Video + Default
{
    fn default() -> Self {
        let ula = U::default();
        let border = ula.border_color();
        UlaPlus {
            ula: U::default(),
            ulaplus_disabled: false,
            scld_mode_rw: false,
            scld_mode: ScldCtrlFlags::default(),
            register_flags: UlaPlusRegFlags::default(),
            color_mode: ColorMode::default(),
            beg_render_mode: RenderMode::from_border_color(border),
            cur_render_mode: RenderMode::from_border_color(border),
            beg_source_mode: SourceMode::default(),
            cur_source_mode: SourceMode::default(),
            sec_frame_cache: UlaFrameCache::default(),
            shadow_sec_frame_cache: UlaFrameCache::default(),
            beg_palette: UlaPlusPalette::default(),
            cur_palette: UlaPlusPalette::default(),
            palette_changes: Vec::new(),
            source_changes: Vec::new(),
            mode_changes: Vec::new(),
        }
    }
}

impl<U> fmt::Debug for UlaPlus<U>
    where U: fmt::Debug + Video
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UlaPlus")
            .field("ula", &self.ula)
            .field("ulaplus_disabled", &self.ulaplus_disabled)
            .field("scld_mode_rw", &self.scld_mode_rw)
            .field("scld_mode", &self.scld_mode)
            .field("register_flags", &self.register_flags)
            .field("color_mode", &self.color_mode)
            .field("beg_render_mode", &self.beg_render_mode)
            .field("cur_render_mode", &self.cur_render_mode)
            .field("beg_source_mode", &self.beg_source_mode)
            .field("cur_source_mode", &self.cur_source_mode)
            .field("sec_frame_cache", &self.sec_frame_cache)
            .field("shadow_sec_frame_cache", &self.shadow_sec_frame_cache)
            .field("beg_palette", &self.beg_palette)
            .field("cur_palette", &self.cur_palette)
            .finish()
    }
}

impl<U> InnerAccess for UlaPlus<U>
    where U: Video
{
    type Inner = U;

    fn inner_ref(&self) -> &Self::Inner {
        &self.ula
    }

    fn inner_mut(&mut self) -> &mut Self::Inner {
        &mut self.ula
    }

    fn into_inner(self) -> Self::Inner {
        self.ula
    }
}

impl<'a, U> UlaControl for UlaPlus<U>
    where U: Video + UlaControl + UlaPlusInner<'a>
{
    fn has_late_timings(&self) -> bool {
        self.ula.has_late_timings()
    }

    fn set_late_timings(&mut self, late_timings: bool) {
        self.ula.set_late_timings(late_timings)
    }

    fn ula128_mem_port_value(&self) -> Option<Ula128MemFlags> {
        self.ula.ula128_mem_port_value()
    }

    fn set_ula128_mem_port_value(&mut self, value: Ula128MemFlags) -> bool {
        self.ula.set_ula128_mem_port_value(value)
    }

    fn ula3_ctrl_port_value(&self) -> Option<Ula3CtrlFlags> {
        self.ula.ula3_ctrl_port_value()
    }

    fn set_ula3_ctrl_port_value(&mut self, value: Ula3CtrlFlags) -> bool {
        self.ula.set_ula3_ctrl_port_value(value)
    }

    fn scld_ctrl_port_value(&self) -> Option<ScldCtrlFlags> {
        Some(self.scld_mode)
    }

    fn set_scld_ctrl_port_value(&mut self, value: ScldCtrlFlags) -> bool {
        self.write_scld_ctrl_port(value, self.ula.current_video_ts());
        true
    }

    fn scld_mmu_port_value(&self) -> Option<u8> {
        self.ula.scld_mmu_port_value()
    }

    fn set_scld_mmu_port_value(&mut self, value: u8) -> bool {
        self.ula.set_scld_mmu_port_value(value)
    }

    fn ulaplus_reg_port_value(&self) -> Option<UlaPlusRegFlags> {
        Some(self.register_flags)
    }

    fn set_ulaplus_reg_port_value(&mut self, value: UlaPlusRegFlags) -> bool {
        self.write_plus_regs_port(value, self.ula.current_video_ts());
        true
    }

    fn ulaplus_data_port_value(&self) -> Option<u8> {
        Some(self.read_plus_data_port())
    }

    fn set_ulaplus_data_port_value(&mut self, value: u8) -> bool {
        self.write_plus_data_port(value, self.ula.current_video_ts());
        true
    }
}

impl<'a, U> UlaPlus<U>
    where U: UlaPlusInner<'a>
{
    /// Returns `true` if reading the `0xFF` port returns the previous write or `false` if bus data is being read.
    pub fn can_read_scld_mode(&mut self) -> bool {
        self.scld_mode_rw
    }
    /// Determines if reading the `0xFF` port returns the previous write or bus data.
    pub fn enable_reading_scld_mode(&mut self, enable_reading: bool) {
        self.scld_mode_rw = enable_reading;
    }
    /// Returns `true` if ULAplus modes and ports are enabled or `false` otherwise.
    pub fn is_ulaplus_enabled(&self) -> bool {
        !self.ulaplus_disabled
    }
    /// Controls the ULAplus ports and graphics availability.
    ///
    /// When disabled, ULAplus ports become available to the underlying devices and the screen and color modes
    /// are being reset to the default. However, palette entries are left unmodified. The last register port
    /// value written is also left unmodified.
    ///
    /// When enabled, ULAplus ports become responsive again but the screen mode is not restored unless it has
    /// been previously written to `0xFF` port. In this instance the screen mode gets restored from the last
    /// value written to `0xFF` port. The color mode is always being restored.
    pub fn enable_ulaplus_modes(&mut self, enable: bool) {
        let ts = self.ula.current_video_ts();
        self.ulaplus_disabled = !enable;
        if enable {
            let flags = self.scld_mode.into_plus_mode();
            let render_mode = self.render_mode_from_plus_flags(flags)
                                  .with_color_mode(self.color_mode);
            self.update_render_mode(render_mode, ts);
            self.update_source_mode(SourceMode::from_plus_reg_flags(flags), ts);
        }
        else if !enable {
            let render_mode = RenderMode::from_border_color(self.ula.border_color());
            self.update_render_mode(render_mode, ts);
            self.update_source_mode(SourceMode::default(), ts);
        }
    }
    /// Returns the current rendering mode.
    pub fn render_mode(&self) -> RenderMode {
        self.cur_render_mode
    }
    /// Returns the current source mode.
    pub fn source_mode(&self) -> SourceMode {
        self.cur_source_mode
    }
    /// Returns the current palette view.
    pub fn ulaplus_palette(&self) -> &UlaPlusPalette {
        &self.cur_palette
    }
    /// Returns the mutable reference to the current palette.
    pub fn ulaplus_palette_mut(&mut self) -> &mut UlaPlusPalette {
        &mut self.cur_palette
    }

    fn push_mode_change(&mut self, ts: VideoTs) {
        self.mode_changes.push((ts, self.cur_render_mode.bits()).into());
    }

    fn update_render_mode(&mut self, render_mode: RenderMode, ts: VideoTs) {
        if render_mode != self.cur_render_mode {
            self.cur_render_mode = render_mode;
            self.push_mode_change(ts);
        }
    }

    fn update_source_mode(&mut self, source_mode: SourceMode, ts: VideoTs) {
        let source_diff = source_mode ^ self.cur_source_mode;
        if source_diff.intersects(SourceMode::SOURCE_MASK) {
            self.source_changes.push((ts, source_mode.bits()).into());
        }
        if source_diff.intersects(SourceMode::SHADOW_BANK) {
            self.ula.push_screen_change(ts);
        }
        self.cur_source_mode = source_mode;
    }

    #[inline]
    fn change_border_color(&mut self, border: BorderColor, ts: VideoTs) {
        if self.ula.update_last_border_color(border) {
            if !self.cur_render_mode.is_hi_res() {
                self.cur_render_mode = self.cur_render_mode.with_color(border.into());
                self.push_mode_change(ts);
            }
        }
    }

    fn render_mode_from_plus_flags(&self, flags: UlaPlusRegFlags) -> RenderMode {
        if flags.is_screen_hi_res() {
            (self.cur_render_mode | RenderMode::HI_RESOLUTION)
                        .with_color(flags.hires_color_index())
        }
        else {
            (self.cur_render_mode & !RenderMode::HI_RESOLUTION)
                        .with_color(self.ula.border_color().into())
        }
    }

    fn update_screen_mode(&mut self, flags: UlaPlusRegFlags, ts: VideoTs) {
        self.update_source_mode(SourceMode::from_plus_reg_flags(flags), ts);
        self.update_render_mode(self.render_mode_from_plus_flags(flags), ts);
    }

    fn write_scld_ctrl_port(&mut self, flags: ScldCtrlFlags, ts: VideoTs) {
        self.scld_mode = flags;
        self.update_screen_mode(flags.into_plus_mode(), ts);
    }

    fn write_plus_regs_port(&mut self, flags: UlaPlusRegFlags, ts: VideoTs) {
        if flags.is_mode_group() {
            self.update_screen_mode(flags|self.scld_mode.into_plus_mode(), ts);
        }
        self.register_flags = flags;
    }

    fn write_plus_data_port(&mut self, data: u8, ts: VideoTs) {
        if let Some(index) = self.register_flags.palette_group() {
            let entry = &mut self.cur_palette[index as usize];
            if data != *entry {
                *entry = data;
                self.palette_changes.push((ts, index, data).into());
            }
        }
        else if self.register_flags.is_mode_group() {
            let color_mode = ColorMode::from_bits_truncate(data);
            self.color_mode = color_mode;
            self.update_render_mode(self.cur_render_mode.with_color_mode(color_mode), ts);
        }        
    }

    fn read_plus_data_port(&self) -> u8 {
        if let Some(index) = self.register_flags.palette_group() {
            self.cur_palette[index as usize]
        }
        else {
            self.color_mode.bits()
        }
    }
}

impl<U> MemoryAccess for UlaPlus<U>
    where U: Video + MemoryAccess,
{
    type Memory = U::Memory;
    type MemoryExt = U::MemoryExt;

    #[inline(always)]
    fn memory_ext_ref(&self) -> &Self::MemoryExt {
        self.ula.memory_ext_ref()
    }
    #[inline(always)]
    fn memory_ext_mut(&mut self) -> &mut Self::MemoryExt {
        self.ula.memory_ext_mut()
    }
    #[inline(always)]
    fn memory_mut(&mut self) -> &mut Self::Memory {
        self.ula.memory_mut()
    }
    #[inline(always)]
    fn memory_ref(&self) -> &Self::Memory {
        self.ula.memory_ref()
    }

    fn memory_with_ext_mut(&mut self) -> (&mut Self::Memory, &mut Self::MemoryExt) {
        self.ula.memory_with_ext_mut()
    }
}

impl<U, B, X> ControlUnit for UlaPlus<U>
    where U: for<'a> UlaPlusInner<'a>
             + ControlUnit<BusDevice=B>
             + UlaControlExt
             + MemoryAccess<MemoryExt=X>
             + Memory<Timestamp=VideoTs>
             + Io<Timestamp=VideoTs, WrIoBreak=(), RetiBreak=()>,
          B: BusDevice,
          B::Timestamp: From<VFrameTs<U::VideoFrame>>,
          X: MemoryExtension
{
    type BusDevice = U::BusDevice;

    #[inline]
    fn bus_device_mut(&mut self) -> &mut Self::BusDevice {
        self.ula.bus_device_mut()
    }
    #[inline]
    fn bus_device_ref(&self) -> &Self::BusDevice {
        self.ula.bus_device_ref()
    }
    #[inline]
    fn into_bus_device(self) -> Self::BusDevice {
        self.ula.into_bus_device()
    }

    fn reset<C: Cpu>(&mut self, cpu: &mut C, hard: bool) {
        self.ula.reset(cpu, hard);
        if hard {
            let ts = self.ula.current_video_ts();
            self.scld_mode = ScldCtrlFlags::default();
            self.color_mode = ColorMode::default();
            self.register_flags = UlaPlusRegFlags::default();
            let render_mode = RenderMode::from_border_color(self.ula.border_color());
            self.update_render_mode(render_mode, ts);
            self.update_source_mode(SourceMode::default(), ts);
            for (p, index) in self.cur_palette.iter_mut().zip(0..64) {
                if *p != 0 {
                    self.palette_changes.push((ts, index, 0).into());
                }
                *p = 0;
            }
        }
    }

    fn nmi<C: Cpu>(&mut self, cpu: &mut C) -> bool {
        self.ula_nmi(cpu)
    }

    fn execute_next_frame<C: Cpu>(&mut self, cpu: &mut C) {
        while !self.ula_execute_next_frame_with_breaks(cpu) {}
    }

    fn ensure_next_frame(&mut self) {
        self.ensure_next_frame_vtsc();
    }

    fn execute_single_step<C: Cpu, F: FnOnce(CpuDebug)>(
            &mut self,
            cpu: &mut C,
            debug: Option<F>
        ) -> Result<(),()>
    {
        self.ula_execute_single_step(cpu, debug)
    }
}

impl<U> UlaControlExt for UlaPlus<U>
    where U: for<'a> UlaPlusInner<'a> + ControlUnit + UlaControlExt
{
    fn prepare_next_frame<C: MemoryContention>(
            &mut self,
            vtsc: VFrameTsCounter<U::VideoFrame, C>
        ) -> VFrameTsCounter<U::VideoFrame, C>
    {
        self.beg_render_mode = self.cur_render_mode;
        self.beg_source_mode = self.cur_source_mode;
        self.beg_palette = self.cur_palette;

        self.sec_frame_cache.clear();
        self.shadow_sec_frame_cache.clear();
        self.palette_changes.clear();
        self.source_changes.clear();
        self.mode_changes.clear();
        self.ula.prepare_next_frame(vtsc)
    }
}
