/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
//! Common snapshot formats utilities.
use core::fmt;
use core::ops::Range;
use std::io::Read;
use bitflags::bitflags;

use spectrusty_core::z80emu::{*, z80::*};
use spectrusty_core::clock::{VideoTs, FTs};
use spectrusty_core::chip::{
    ControlUnit, MemoryAccess, ReadEarMode,
    Ula128MemFlags, Ula3CtrlFlags, ScldCtrlFlags
};
use spectrusty_core::video::{Video, BorderColor};
use spectrusty_core::memory::{ZxMemory, ZxMemoryError};
use spectrusty_peripherals::ay::AyRegister;

#[non_exhaustive]
#[derive(Debug,Clone,Copy,PartialEq,Eq,Hash)]
pub enum ComputerModel {
    Spectrum16,
    Spectrum48,
    SpectrumNTSC,
    Spectrum128,
    SpectrumPlus2,
    SpectrumPlus2A,
    SpectrumPlus3,
    SpectrumPlus3e,
    SpectrumSE,
    TimexTC2048,
    TimexTC2068,
    TimexTS2068,
}

bitflags! {
    #[derive(Default)]
    pub struct Extensions: u64 {
        const NONE       = 0x0000_0000_0000_0000;
        const IF1        = 0x0000_0000_0000_0001;
        const PLUS_D     = 0x0000_0000_0000_0002;
        const DISCIPLE   = 0x0000_0000_0000_0004;
        const SAM_RAM    = 0x0000_0000_0000_0008;
        const ULA_PLUS   = 0x0000_0000_0000_0010;
        const TR_DOS     = 0x0000_0000_0000_0020;
        const RESERVED   = 0xFFFF_FFFF_FFFF_FFC0;
    }
}

#[non_exhaustive]
#[derive(Debug,Clone,Copy,PartialEq,Eq,Hash)]
pub enum JoystickModel {
    Kempston,
    Sinclair1,
    Sinclair2,
    Cursor,
    Fuller,
}

#[non_exhaustive]
#[derive(Debug,Clone,Copy,PartialEq,Eq,Hash)]
pub enum Ay3_891xDevice {
    /// The device attached to one of the 128k/+2/+3/... models.
    Ay128k,
    /// The device attached to one of the 16k/48k models with the same port mappings as 128k version.
    Melodik,
    /// The Fuller Box port mapped AY chipset.
    FullerBox,
    /// The Tx2068 port mapped AY chipset.
    Timex,
}

#[non_exhaustive]
#[derive(Debug,Clone,PartialEq,Eq)]
pub enum CpuModel {
    NMOS(Z80NMOS),
    CMOS(Z80CMOS),
    BM1(Z80BM1),
}

impl<F: Flavour> From<CpuModel> for Z80<F>
    where F: From<NMOS> + From<CMOS> + From<BM1>
{
    fn from(cpu: CpuModel) -> Self {
        match cpu {
            CpuModel::NMOS(z80) => z80.into_flavour(),
            CpuModel::CMOS(z80) => z80.into_flavour(),
            CpuModel::BM1(z80)   => z80.into_flavour(),
        }
    }
}

impl From<CpuModel> for Z80Any {
    fn from(cpu: CpuModel) -> Self {
        match cpu {
            CpuModel::NMOS(z80) => Z80Any::NMOS(z80),
            CpuModel::CMOS(z80) => Z80Any::CMOS(z80),
            CpuModel::BM1(z80)  => Z80Any::BM1(z80),
        }
    }
}

impl From<Z80Any> for CpuModel {
    fn from(cpu: Z80Any) -> Self {
        match cpu {
            Z80Any::NMOS(z80) => CpuModel::NMOS(z80),
            Z80Any::CMOS(z80) => CpuModel::CMOS(z80),
            Z80Any::BM1(z80)  => CpuModel::BM1(z80),
        }
    }
}

/// The memory range specifies which part of the emulated hardware the data should be loaded to.
#[non_exhaustive]
#[derive(Debug,Clone,PartialEq,Eq,Hash)]
pub enum MemoryRange {
    /// Load into the main ROM.
    /// The address space starts from the top of the first ROM bank and extends towards the last ROM bank.
    Rom(Range<usize>),
    /// Load into the main RAM.
    /// The address space starts from the top of the first RAM bank and extends towards the last RAM bank.
    Ram(Range<usize>),
    /// Load into the Interface1 ROM.
    Interface1Rom,
    /// Load into the MGT +D ROM.
    PlusDRom,
    /// Load into the MGT DISCiPLE ROM.
    DiscipleRom,
    /// Load into the Multiface ROM.
    MultifaceRom,
    /// Load into the SamRam ROM.
    /// The address space starts from the top of the first ROM bank and extends towards the last ROM bank.
    SamRamRom(Range<usize>),
}

/// The methods can be called more than one time.
pub trait SnapshotCreator {
    fn model(&self) -> ComputerModel;
    fn extensions(&self) -> Extensions;
    fn cpu(&self) -> CpuModel;
    fn current_clock(&self) -> FTs;
    fn border_color(&self) -> BorderColor;
    fn issue(&self) -> ReadEarMode;
    fn memory_ref(&self, range: MemoryRange) -> Result<&[u8], ZxMemoryError>;
    fn joystick(&self) -> Option<JoystickModel> { None }
    fn ay_state(&self, _choice: Ay3_891xDevice) -> Option<(AyRegister, &[u8;16])> { None }
    fn ula128_flags(&self) -> Ula128MemFlags { unimplemented!() }
    fn ula3_flags(&self) -> Ula3CtrlFlags { unimplemented!() }
    fn timex_flags(&self) -> ScldCtrlFlags { unimplemented!() }
    fn timex_memory_banks(&self) -> u8 { unimplemented!() }
    // fn ulaplus_flags(&self) -> UlaPlusRegFlags;
    fn is_interface1_rom_paged_in(&self) -> bool { unimplemented!() }
    fn is_plus_d_rom_paged_in(&self) -> bool { unimplemented!() }
    fn is_disciple_rom_paged_in(&self) -> bool { unimplemented!() }
    fn is_tr_dos_rom_paged_in(&self) -> bool { unimplemented!() }
}

bitflags! {
    #[derive(Default)]
    pub struct SnapshotResult: u64 {
        const OK              = 0x0000_0000_0000_0000;
        const MODEL_NSUP      = 0x0000_0000_0000_0001;
        const EXTENSTION_NSUP = 0x0000_0000_0000_0010;
        const CPU_MODEL_NSUP  = 0x0000_0000_0000_0100;
        const JOYSTICK_NSUP   = 0x0000_0000_0000_1000;
        const SOUND_CHIP_NSUP = 0x0000_0000_0001_0000;
        const KEYB_ISSUE_NSUP = 0x0000_0000_0010_0000;
    }
}

/// Implement this trait to be able to load snapshot from files supported by this crate.
///
/// The method [SnapshotLoader::select_model] is always being called first.
/// Then the other methods are being called in unspecified order.
/// Not every method is always being called though.
/// Some methods are not called when it makes no sense in the context of the loaded snapshot.
pub trait SnapshotLoader {
    /// The error type returned by the [SnapshotLoader::select_model] method.
    type Error: Into<Box<(dyn std::error::Error + Send + Sync + 'static)>>;
    /// Should create an instance of an emulated model from the given `model` and other arguments.
    ///
    /// If the model can not be emulated this method should return an `Err(Self::Error)`.
    ///
    /// This method is always being called first, before any other methods in this trait.
    fn select_model(
        &mut self,
        model: ComputerModel,
        extensions: Extensions,
        border: BorderColor,
        issue: ReadEarMode,
    ) -> Result<(), Self::Error>;
    /// Should read a memory chunk from the given `reader` source according to the specified `range`.
    ///
    /// This method should not fail.
    fn read_into_memory<R: Read>(&mut self, range: MemoryRange, reader: R) -> Result<(), ZxMemoryError>;
    /// Should attach an instance of the `cpu`.
    ///
    /// This method should not fail.
    fn assign_cpu(&mut self, cpu: CpuModel);
    /// Should set the frame T-states clock to the value given in `tstates`.
    ///
    /// This method should not fail.
    fn set_clock(&mut self, tstates: FTs);
    /// Should emulate sending the `data` to the given `port` of the main chipset.
    ///
    /// This method should not fail.
    fn write_port(&mut self, port: u16, data: u8);
    /// Should select the given joystick model if one is available.
    ///
    /// This method should not fail. Default implementation does nothing.
    fn select_joystick(&mut self, _joystick: JoystickModel) {}
    /// Should attach the amulated instance of `AY-3-891x` sound processor and initialize it from
    /// the given arguments if one is available.
    ///
    /// This method is called n-times if more than one AY chipset is attached and there
    /// are different register values for each of them.
    ///
    /// This method should not fail. Default implementation does nothing.
    fn setup_ay(&mut self, _choice: Ay3_891xDevice, _reg_selected: AyRegister, _reg_values: &[u8;16]) {}
    /// Should page in the Interface 1 ROM if one is available.
    ///
    /// This method should not fail. If an Interface 1 is not supported [SnapshotLoader::select_model]
    /// may return with an error if [Extensions] would indicate such support is being requested.
    ///
    /// # Panics
    /// The default implementation always panics.
    fn interface1_rom_paged_in(&mut self) { unimplemented!() }
    /// Should page in the MGT +D ROM if one is available.
    ///
    /// This method should not fail. If an MGT +D is not supported [SnapshotLoader::select_model]
    /// may return with an error if [Extensions] would indicate such support is being requested.
    ///
    /// # Panics
    /// The default implementation always panics.
    fn plus_d_rom_paged_in(&mut self) { unimplemented!() }
    /// Should page in the TR-DOS ROM if one is available.
    ///
    /// This method should not fail. If an TR-DOS is not supported [SnapshotLoader::select_model]
    /// may return with an error if [Extensions] would indicate such support is being requested.
    ///
    /// # Panics
    /// The default implementation always panics.
    fn tr_dos_rom_paged_in(&mut self) { unimplemented!() }
}

/// Returns `true` if a `cpu` is safe for a snapshot using lossy formats.
pub fn is_cpu_safe_for_snapshot<C: Cpu>(cpu: &C) -> bool {
    !cpu.is_after_prefix()
}

/// Makes sure CPU is in a state that is safe for a snapshot, otherwise executes instructions until it's safe.
///
/// In some rare cases, a CPU will be in a state that most snapshot formats are unable to preserve it correctly.
///
/// That is while CPU is in a state after: 
/// * Executing one of the `0xFD` or `0xDD` prefixes before executing the prefixed instruction,
///   in this instance the "after the prefix" state is being lost, which will end up executing the next
///   instruction in the wrong way.
/// * Executing the `EI` instruction which temporarily prevents interrupts until the next instruction is executed,
///   in this instance the "after EI" state is lost and an interrupt may be accepted while it shouldn't be just yet.
pub fn ensure_cpu_is_safe_for_snapshot<C: Cpu, M: ControlUnit + Video + MemoryAccess>(
        cpu: &mut C,
        chip: &mut M
    )
{
    loop {
        if cpu.is_halt() {
            return
        }
        else if cpu.is_after_prefix() {
            match chip.memory_ref().read(cpu.get_pc()) {
                0xFD|0xDD => return,
                _ => {}
            }
        }
        else if cpu.is_after_ei() {
            let VideoTs { vc, hc } = chip.current_video_ts();
            if vc != 0 || hc < -1 || hc > 31 {
                return
            }
        }
        else {
            return
        }
        let _ = chip.execute_single_step::<_,CpuDebugFn>(cpu, None);
    }
}

impl ComputerModel {
    /// Returns the number of T-states per single frame.
    pub fn frame_tstates(self) -> FTs {
        use ComputerModel::*;
        match self {
            SpectrumNTSC => {
                59136
            }
            Spectrum16|Spectrum48|
            TimexTC2048|TimexTC2068|TimexTS2068 => {
                69888
            }
            Spectrum128|SpectrumPlus2|
            SpectrumPlus2A|SpectrumPlus3|SpectrumPlus3e|
            SpectrumSE=> {
                70908
            }
        }
    }
    /// Returns `issue` or [ReadEarMode::Clear] when it does not apply to the model.
    pub fn applicable_issue(self, issue: ReadEarMode) -> ReadEarMode {
        use ComputerModel::*;
        match self {
            Spectrum16|Spectrum48|SpectrumNTSC|
            Spectrum128|SpectrumPlus2
              => issue,
            _ => ReadEarMode::Clear
        }
    }

    pub fn validate_extensions(self, ext: Extensions) -> Result<(), Extensions> {
        use ComputerModel::*;
        match self {
            Spectrum128|SpectrumPlus2|SpectrumSE if ext.intersects(Extensions::SAM_RAM) => Err(Extensions::SAM_RAM),
            SpectrumPlus2A|SpectrumPlus3|SpectrumPlus3e
            if ext.intersects(Extensions::SAM_RAM|Extensions::IF1|Extensions::PLUS_D|Extensions::DISCIPLE) => {
                    Err(ext & (Extensions::SAM_RAM|Extensions::IF1|Extensions::PLUS_D|Extensions::DISCIPLE))
            }
            TimexTC2068|TimexTS2068 if ext.intersects(Extensions::SAM_RAM) => Err(Extensions::SAM_RAM),
            _ => Ok(())
        }
    }
}

impl From<ComputerModel> for &str {
    fn from(model: ComputerModel) -> Self {
        use ComputerModel::*;
        match model {
            Spectrum16     => "ZX Spectrum 16k",
            Spectrum48     => "ZX Spectrum 48k",
            SpectrumNTSC   => "ZX Spectrum NTSC",
            Spectrum128    => "ZX Spectrum 128k",
            SpectrumPlus2  => "ZX Spectrum +2",
            SpectrumPlus2A => "ZX Spectrum +2A",
            SpectrumPlus3  => "ZX Spectrum +3",
            SpectrumPlus3e => "ZX Spectrum +3e",
            SpectrumSE     => "ZX Spectrum SE",
            TimexTC2048    => "Timex TC2048",
            TimexTC2068    => "Timex TC2068",
            TimexTS2068    => "Timex TS2068",
        }
    }
}

impl From<&'_ ComputerModel> for &'static str {
    fn from(model: &ComputerModel) -> Self {
        (*model).into()
    }
}

impl fmt::Display for ComputerModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        <&str>::from(self).fmt(f)
    }
}

impl fmt::Display for Extensions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.intersects(Extensions::IF1) {
            f.write_str(" + IF1")?;
        }
        if self.intersects(Extensions::ULA_PLUS) {
            f.write_str(" + ULAPlus")?;
        }
        if self.intersects(Extensions::PLUS_D) {
            f.write_str(" + MGT+D")?;
        }
        if self.intersects(Extensions::DISCIPLE) {
            f.write_str(" + DISCiPLE")?;
        }
        if self.intersects(Extensions::SAM_RAM) {
            f.write_str(" + SamRam")?;
        }
        Ok(())
    }
}
