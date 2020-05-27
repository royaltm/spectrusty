//! Common snapshot formats utilities.
use core::fmt;
use core::ops::Range;
use std::io::Read;
use bitflags::bitflags;

use spectrusty_core::z80emu::{*, z80::*};
use spectrusty_core::clock::FTs;
use spectrusty_core::chip::ReadEarMode;
use spectrusty_core::video::BorderColor;
use spectrusty_core::memory::ZxMemoryError;
use spectrusty_peripherals::ay::AyRegister;

#[non_exhaustive]
#[derive(Debug,Clone,Copy,PartialEq,Eq,Hash)]
pub enum ComputerModel {
    Spectrum16(Extensions),
    Spectrum48(Extensions),
    SpectrumNTSC(Extensions),
    Spectrum128(Extensions),
    SpectrumPlus2(Extensions),
    SpectrumPlus2A(Extensions),
    SpectrumPlus3(Extensions),
    SpectrumPlus3e(Extensions),
    SpectrumSE(Extensions),
    Tc2048(Extensions),
    Tc2068(Extensions),
    Ts2068(Extensions),
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
    BM(Z80BM),
}

impl<F: Flavour> From<CpuModel> for Z80<F>
    where F: From<NMOS> + From<CMOS> + From<BM1>
{
    fn from(cpu: CpuModel) -> Self {
        match cpu {
            CpuModel::NMOS(z80) => z80.into_flavour(),
            CpuModel::CMOS(z80) => z80.into_flavour(),
            CpuModel::BM(z80)   => z80.into_flavour(),
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
    /// This method is always being called first, before any other method in this trait.
    fn select_model(
        &mut self,
        model: ComputerModel,
        border: BorderColor,
        issue: ReadEarMode,
        joystick: Option<JoystickModel>
    ) -> Result<(), Self::Error>;
    /// Should load memory from the given `reader` source according to the specified `range`.
    fn load_memory_page<R: Read>(&mut self, range: MemoryRange, reader: R) -> Result<(), ZxMemoryError>;
    /// Should attach an instance of the `cpu`.
    ///
    /// This method should not fail.
    fn assign_cpu(&mut self, cpu: CpuModel);
    /// Should attach the amulated instance of `AY-3-891x` sound processor and initialize it from
    /// the given arguments.
    ///
    /// This method is called n-times if more than one AY chipset is attached and there
    /// are different register values for each of them.
    ///
    /// This method should not fail.
    fn setup_ay(&mut self, choice: Ay3_891xDevice, reg_selected: AyRegister, reg_values: &[u8;16]);
    /// Should set the frame T-states clock to the value given in `tstates`.
    ///
    /// This method should not fail.
    fn set_clock(&mut self, tstates: FTs);
    /// Should emulate sending the `data` to the given `port` of the main chipset.
    ///
    /// This method should not fail.
    fn write_port(&mut self, port: u16, data: u8);
    /// Should page in the Interface 1 ROM.
    ///
    /// This method should not fail. If an Interface 1 is not supported [SnapshotLoader::select_model]
    /// should return with an error if [Extensions] would indicate such support is being requested.
    fn interface1_rom_paged_in(&mut self) { unimplemented!() }
    /// Should page in the MGT +D ROM.
    ///
    /// This method should not fail. If an MGT +D is not supported [SnapshotLoader::select_model]
    /// should return with an error if [Extensions] would indicate such support is being requested.
    fn plus_d_rom_paged_in(&mut self) { unimplemented!() }
    /// Should page in the TR-DOS ROM.
    ///
    /// This method should not fail. If an TR-DOS is not supported [SnapshotLoader::select_model]
    /// should return with an error if [Extensions] would indicate such support is being requested.
    fn tr_dos_rom_paged_in(&mut self) { unimplemented!() }
}

impl ComputerModel {
    /// Returns the number of T-states per single frame.
    pub fn frame_tstates(self) -> FTs {
        use ComputerModel::*;
        match self {
            SpectrumNTSC(..) => {
                59136
            }
            Spectrum16(..)|Spectrum48(..)|
            Tc2048(..)|Tc2068(..)|Ts2068(..) => {
                69888
            }
            Spectrum128(..)|SpectrumPlus2(..)|
            SpectrumPlus2A(..)|SpectrumPlus3(..)|SpectrumPlus3e(..)|
            SpectrumSE(..)=> {
                70908
            }
        }
    }
}

impl fmt::Display for ComputerModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use ComputerModel::*;
        let (name, ext) = match *self {
            Spectrum16(ext)     => ("ZX Spectrum 16k", ext),
            Spectrum48(ext)     => ("ZX Spectrum 48k", ext),
            SpectrumNTSC(ext)   => ("ZX Spectrum NTSC", ext),
            Spectrum128(ext)    => ("ZX Spectrum 128k", ext),
            SpectrumPlus2(ext)  => ("ZX Spectrum +2", ext),
            SpectrumPlus2A(ext) => ("ZX Spectrum +2A", ext),
            SpectrumPlus3(ext)  => ("ZX Spectrum +3", ext),
            SpectrumPlus3e(ext) => ("ZX Spectrum +3e", ext),
            SpectrumSE(ext)     => ("ZX Spectrum SE", ext),
            Tc2048(ext)         => ("Timex TC2048", ext),
            Tc2068(ext)         => ("Timex TC2068", ext),
            Ts2068(ext)         => ("Timex TS2068", ext),
        };

        f.write_str(name)?;
        if !ext.is_empty() {
            ext.fmt(f)?;
        }
        Ok(())
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
