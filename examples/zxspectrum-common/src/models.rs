/*
    zxspectrum-common: High-level ZX Spectrum emulator library example.
    Copyright (C) 2020-2023  Rafal Michalski

    For the full copyright notice, see the lib.rs file.
*/
use core::convert::TryFrom;
use core::fmt;
use core::iter;
use core::str::FromStr;
use core::time::Duration;
use std::io::{self, Read};
use rand::prelude::*;

use serde::{Serialize, Deserialize};

use spectrusty::z80emu::{Cpu, Z80, Z80Any, {z80::Flavour}, host::Io};
use spectrusty::audio::Blep;
#[allow(unused_imports)] use spectrusty::clock::{FTs, VFrameTs};
use spectrusty::formats::snapshot::ComputerModel;
use spectrusty::memory::{
    ZxMemory, PagedMemory8k,
    Memory16kEx, Memory48kEx, Memory48kDock64kEx,
    MemoryExtension, NoMemoryExtension
};
use spectrusty::bus::{
    BusDevice, DeserializeDynDevice, SerializeDynDevice,
    ay::{
        serial128::{
            Ay3_8912KeypadRs232, Ay3_8912Rs232
        }
    },
    parallel::{Plus3CentronicsBusDevice, ParallelPortWriter}
    // parallel::Plus3CentronicsWriterBusDevice
};
#[allow(unused_imports)]
use spectrusty::chip::{
    FrameState, UlaControl,
    MemoryAccess, HostConfig, UlaCommon,
    Ula128MemFlags, Ula3CtrlFlags,
    ula::{self, UlaVideoFrame, UlaNTSCVidFrame},
    ula128::{self, Ula128VidFrame},
    ula3::{self, Ula3VidFrame},
    scld,
    plus::{UlaPlus, UlaPlusInner},
};
use spectrusty::formats::snapshot::ensure_cpu_is_safe_for_snapshot;
use spectrusty::video::{Video, VideoFrame, BorderColor};
use spectrusty_utils::io::{Empty, Sink};

use super::devices::{DynamicDevices, PluggableJoystickDynamicBus};
use super::spectrum::{MemTap, EmulatorState, ZxSpectrum, SpectrumUla};

pub static ROM48: &[&[u8]] = &[include_bytes!("../../../resources/roms/48.rom")];
pub static ROM_TC2048: &[&[u8]] = &[include_bytes!("../../../resources/roms/tc2048.rom")];
pub static ROM128: &[&[u8]] = &[include_bytes!("../../../resources/roms/128-0.rom"),
                                include_bytes!("../../../resources/roms/128-1.rom")];
pub static ROM_PLUS2: &[&[u8]] = &[include_bytes!("../../../resources/roms/plus2-0.rom"),
                                   include_bytes!("../../../resources/roms/plus2-1.rom")];
pub static ROM_PLUS3: &[&[u8]] = &[include_bytes!("../../../resources/roms/plus3-0.rom"),
                                   include_bytes!("../../../resources/roms/plus3-1.rom"),
                                   include_bytes!("../../../resources/roms/plus3-2.rom"),
                                   include_bytes!("../../../resources/roms/plus3-3.rom")];
pub static ROM_PLUS2B: &[&[u8]] = &[include_bytes!("../../../resources/roms/plus2B.rom"),
                                    include_bytes!("../../../resources/roms/48.rom"),
                                    include_bytes!("../../../resources/roms/BBCBasic.rom"),
                                    include_bytes!("../../../resources/roms/opense.rom")];

/* First some chipset type declarations */

// redeclare chipsets with boxed devices.
#[cfg(feature = "boxed_devices")]
mod chipsets {
    pub type Ula<M, D, X, V>  =    super::ula::Ula<M, Box<D>, X, V>;
    pub type UlaPAL<M, D, X>  =    super::ula::UlaPAL<M, Box<D>, X>;
    pub type UlaNTSC<M, D, X> =    super::ula::UlaNTSC<M, Box<D>, X>;
    pub type Scld<M, D, X, V> =   super::scld::Scld<M, Box<D>, X, V>;
    pub type Ula128<D, X>     = super::ula128::Ula128<Box<D>, X>;
    pub type Ula3<D, X>       =   super::ula3::Ula3<Box<D>, X>;
}
// chipsets with inline devices
#[cfg(not(feature = "boxed_devices"))]
mod chipsets {
    pub use    super::ula::Ula;
    pub use    super::ula::UlaPAL;
    pub use    super::ula::UlaNTSC;
    pub use   super::scld::Scld;
    pub use super::ula128::Ula128;
    pub use   super::ula3::Ula3;
}
pub use chipsets::*;

/// Timex TC2048 chipset.
pub type TC2048<D, X=NoMemoryExtension> = Scld<Memory48kDock64kEx, D, X, UlaVideoFrame>;
/// ULA 128 with a AY-3-8912 sound processor + Keypad and RS232 in its I/O port A.
pub type Ula128AyKeypad<D,
                        X=NoMemoryExtension,
                        R=Empty,
                        W=Sink> = Ula128<Ay3_8912KeypadRs232<D, R, W>, X>;
/// ULA +3 with +3 Centronics Port and with a AY-3-8912 sound processor + RS232 in its I/O port A.
pub type Ula3Ay<D,
                X=NoMemoryExtension,
                R=Empty,
                W=Sink> = Ula3<Plus3CentronicsBusDevice<
                                        ParallelPortWriter<<D as BusDevice>::Timestamp, W>,
                                        Ay3_8912Rs232<D, R, W>
                                    >,
                                X>;
/// ULAplus with ULA 48k.
pub type Plus48<D, X=NoMemoryExtension> = UlaPlus<UlaPAL<Memory48kEx, D, X>>;

/// ULAplus with ULA 128k.
pub type Plus128<D,
                 X=NoMemoryExtension,
                 R=Empty,
                 W=Sink> = UlaPlus<Ula128AyKeypad<D, X, R, W>>;

/// ULAplus with ULA +3.
pub type Plus3<D,
               X=NoMemoryExtension,
               R=Empty,
               W=Sink> = UlaPlus<Ula3Ay<D, X, R, W>>;

/* Then some model type declaration */

pub type ZxSpectrum16k<C, D, X=NoMemoryExtension, F=MemTap> = ZxSpectrum<C, UlaPAL<Memory16kEx, D, X>, F>;
pub type ZxSpectrum48k<C, D, X=NoMemoryExtension, F=MemTap> = ZxSpectrum<C, UlaPAL<Memory48kEx, D, X>, F>;
pub type ZxSpectrum48kPlus<C, D, X=NoMemoryExtension, F=MemTap> = ZxSpectrum<C, Plus48<D, X>, F>;
pub type ZxSpectrumNTSC<C, D, X=NoMemoryExtension, F=MemTap> = ZxSpectrum<C, UlaNTSC<Memory48kEx, D, X>, F>;
pub type TimexTC2048<C, D, X=NoMemoryExtension, F=MemTap> = ZxSpectrum<C, TC2048<D, X>, F>;
pub type ZxSpectrum128k<C, D, X=NoMemoryExtension,
                              F=MemTap,
                              R=Empty,
                              W=Sink> = ZxSpectrum<C, Ula128AyKeypad<D, X, R, W>, F>;
pub type ZxSpectrum128kPlus<C, D, X=NoMemoryExtension,
                              F=MemTap,
                              R=Empty,
                              W=Sink> = ZxSpectrum<C, Plus128<D, X, R, W>, F>;
pub type ZxSpectrum2A<C, D, X=NoMemoryExtension,
                            F=MemTap,
                            R=Empty,
                            W=Sink> = ZxSpectrum<C, Ula3Ay<D, X, R, W>, F>;
// pub type ZxSpectrum3<C, D> = ZxSpectrum<C, Ula3Ay<FloppyDrive<D>>>;
pub type ZxSpectrum2B<C, D, X=NoMemoryExtension,
                            F=MemTap,
                            R=Empty,
                            W=Sink> = ZxSpectrum<C, Plus3<D, X, R, W>, F>;

/*   The device types used in ZxSpectrumModel.  */
#[cfg(feature = "universal_dev_ts")]
mod devices {
    use super::*;
    /// The device type used by models with [UlaPAL] in [ZxSpectrumModel].
    pub type UlaPALDevice<S> = PluggableJoystickDynamicBus<S, FTs>;
    /// The device type used by models with [TC2048] in [ZxSpectrumModel].
    pub type TC2048Device<S> = PluggableJoystickDynamicBus<S, FTs>;
    /// The device type used by models with [UlaNTSC] in [ZxSpectrumModel].
    pub type UlaNTSCDevice<S> = PluggableJoystickDynamicBus<S, FTs>;
    /// The device type used by models with [Ula128AyKeypad] in [ZxSpectrumModel].
    pub type Ula128Device<S> = PluggableJoystickDynamicBus<S, FTs>;
    /// The device type used by models with [Ula3Ay] in [ZxSpectrumModel].
    pub type Ula3Device<S> = PluggableJoystickDynamicBus<S, FTs>;
}
// Alternatively the definitions below can be swapped with the ones below to use VFrameTs timestamps instead.
// This will however result in a larger executable size.
#[cfg(not(feature = "universal_dev_ts"))]
mod devices {
    use super::*;
    /// The device type used by models with [UlaPAL] in [ZxSpectrumModel].
    pub type UlaPALDevice<S> = PluggableJoystickDynamicBus<S, VFrameTs<UlaVideoFrame>>;
    /// The device type used by models with [TC2048] in [ZxSpectrumModel].
    pub type TC2048Device<S> = PluggableJoystickDynamicBus<S, VFrameTs<UlaVideoFrame>>;
    /// The device type used by models with [UlaNTSC] in [ZxSpectrumModel].
    pub type UlaNTSCDevice<S> = PluggableJoystickDynamicBus<S, VFrameTs<UlaNTSCVidFrame>>;
    /// The device type used by models with [Ula128AyKeypad] in [ZxSpectrumModel].
    pub type Ula128Device<S> = PluggableJoystickDynamicBus<S, VFrameTs<Ula128VidFrame>>;
    /// The device type used by models with [Ula3Ay] in [ZxSpectrumModel].
    pub type Ula3Device<S> = PluggableJoystickDynamicBus<S, VFrameTs<Ula3VidFrame>>;
}
pub use devices::*;

/// This enum is being used for querying or creating a new [ZxSpectrumModel].
#[derive(Debug, Clone, Copy, PartialEq)]
#[derive(Serialize)]
#[serde(into="&str")]
pub enum ModelRequest {
    Spectrum16,
    Spectrum48,
    SpectrumNTSC,
    Spectrum128,
    SpectrumPlus2,
    SpectrumPlus2A,
    SpectrumPlus3,
    TimexTC2048,
    Spectrum48Plus,
    SpectrumPlusPlus2,
    SpectrumPlus2B,
}

/// The handy iterator of all model types.
///
/// Usefull for e.g. listing available model names.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelRequestIter(Option<ModelRequest>);

/// Enumerates all available models.
///
/// Generic types should implement:
/// * `C` - [Cpu]
/// * `S` - [SerializeDynDevice] + [DeserializeDynDevice]
/// * `X` - [MemoryExtension]
/// * `F` - Tape data provider [io::Read] + [io::Write] + [io::Seek] (e.g. [std::fs::File] or [io::Cursor]).
/// * `R` - [io::Read] for RS-232 external input
/// * `W` - [io::Write] for RS-232 and Centronics external output
#[derive(Serialize, Deserialize)]
#[serde(tag = "model")]
#[serde(bound(
    deserialize = "C: Deserialize<'de>,
                   S: DeserializeDynDevice<'de> + 'de,
                   X: Deserialize<'de>,
                   R: Deserialize<'de> + Default,
                   W: Deserialize<'de> + Default",
    serialize = "C: Serialize, S: SerializeDynDevice, X: Serialize, R: Serialize, W: Serialize")
)]
pub enum ZxSpectrumModel<C: Cpu,
                         S,
                         X,
                         F=MemTap,
                         R=Empty,
                         W=Sink>
{
    #[serde(rename = "ZX Spectrum 16k")]
    Spectrum16(ZxSpectrum16k<C, UlaPALDevice<S>, X, F>),
    #[serde(rename = "ZX Spectrum 48k")]
    Spectrum48(ZxSpectrum48k<C, UlaPALDevice<S>, X, F>),
    #[serde(rename = "ZX Spectrum NTSC")]
    SpectrumNTSC(ZxSpectrumNTSC<C, UlaNTSCDevice<S>, X, F>),
    #[serde(rename = "ZX Spectrum 128k")]
    Spectrum128(ZxSpectrum128k<C, Ula128Device<S>, X, F, R, W>),
    #[serde(rename = "ZX Spectrum +2")]
    SpectrumPlus2(ZxSpectrum128k<C, Ula128Device<S>, X, F, R, W>),
    #[serde(rename = "ZX Spectrum +2A")]
    SpectrumPlus2A(ZxSpectrum2A<C, Ula3Device<S>, X, F, R, W>),
    // #[serde(rename = "ZX Spectrum +3")]
    // SpectrumPlus3(ZxSpectrum3<C, D, X, F>),
    #[serde(rename = "Timex TC2048")]
    TimexTC2048(TimexTC2048<C, TC2048Device<S>, X, F>),
    #[serde(rename = "ZX Spectrum 48k+")]
    Spectrum48Plus(ZxSpectrum48kPlus<C, UlaPALDevice<S>, X, F>),
    #[serde(rename = "ZX Spectrum ++2")]
    SpectrumPlusPlus2(ZxSpectrum128kPlus<C, Ula128Device<S>, X, F, R, W>),
    #[serde(rename = "ZX Spectrum +2B")]
    SpectrumPlus2B(ZxSpectrum2B<C, Ula3Device<S>, X, F, R, W>)
}

/// A helper trait for controling ULAplus mode availability.
pub trait UlaPlusMode {
    fn is_ulaplus_enabled(&self) -> bool {
        false
    }

    fn enable_ulaplus_modes(&mut self, _enable: bool) -> bool {
        false
    }
}

/// Macro for dispatching expressions to the current [ZxSpectrumModel]'s inner variant of [ZxSpectrum] type.
///
/// * `$model` - should be the variable name (or using the 3rd syntax it can be an expression) referring
///   to the instance of [ZxSpectrumModel].
/// * `$spec` - should be the variable name that will hold the reference to the instance of [ZxSpectrum];
///   it can be used inside the `$expr` to access the inner [ZxSpectrum].
/// * `$expr` - an expression that needs to be evaluated.
///
/// # Example
///
/// ```
/// use spectrusty::{
///     chip::MemoryAccess,
///     memory::{ZxMemory, ZxMemoryError, MemoryExtension},
///     formats::snapshot::MemoryRange,
///     z80emu::Cpu};
/// use zxspectrum_common::{ZxSpectrumModel, spectrum_model_dispatch};
///
/// fn memory_ref<C,S,X,F,R,W>(
///         model: &ZxSpectrumModel<C,S,X,F,R,W>,
///         range: MemoryRange
///     ) -> Result<&[u8], ZxMemoryError>
///     where C: Cpu,
///           X: MemoryExtension
/// {
///     fn get_ref<M: ZxMemory>(memory: &M, range: MemoryRange) -> Result<&[u8], ZxMemoryError> {
///         match range {
///             MemoryRange::Rom(range) => memory.rom_ref().get(range),
///             MemoryRange::Ram(range) => memory.ram_ref().get(range),
///             _ => Err(ZxMemoryError::UnsupportedExRomPaging)?
///         }.ok_or_else(|| ZxMemoryError::UnsupportedAddressRange)
///     }
///
///     spectrum_model_dispatch!(model(spec) => get_ref(spec.ula.memory_ref(), range))
/// }
/// ```
#[macro_export]
macro_rules! spectrum_model_dispatch {
    ($model:ident($spec:ident) => $expr:expr) => {
        spectrum_model_dispatch!(($model)($spec) => $expr)
    };
    ($model:ident(mut $spec:ident) => $expr:expr) => {
        spectrum_model_dispatch!(($model)(mut $spec) => $expr)
    };
    (($model:expr)($($spec:tt)*) => $expr:expr) => {
        match $model {
            $crate::ZxSpectrumModel::Spectrum16($($spec)*) => $expr,
            $crate::ZxSpectrumModel::Spectrum48($($spec)*) => $expr,
            $crate::ZxSpectrumModel::Spectrum48Plus($($spec)*) => $expr,
            $crate::ZxSpectrumModel::SpectrumNTSC($($spec)*) => $expr,
            $crate::ZxSpectrumModel::Spectrum128($($spec)*)|
            $crate::ZxSpectrumModel::SpectrumPlus2($($spec)*)=> $expr,
            $crate::ZxSpectrumModel::SpectrumPlusPlus2($($spec)*) => $expr,
            $crate::ZxSpectrumModel::SpectrumPlus2A($($spec)*) => $expr,
            $crate::ZxSpectrumModel::SpectrumPlus2B($($spec)*) => $expr,
            $crate::ZxSpectrumModel::TimexTC2048($($spec)*) => $expr,
        }
    };
}

/// Macro for accessing associated constants and methods of the current [ZxSpectrumModel]'s chipset type.
///
/// * `$model` - should be the variable name (or using the 3rd syntax it can be an expression) referring
///   to the instance of [ZxSpectrumModel].
/// * `$sdd` - the type `S` that implements [SerializeDynDevice].
/// * `$ext` - the type `X` that implements [MemoryExtension].
/// * `$expr` - a method expression or a constant name that should be called on the chipset type.
///
/// # Example
///
/// ```
/// use spectrusty::{
///     z80emu::Cpu,
///     chip::HostConfig,
///     memory::MemoryExtension,
///     video::Video
/// };
/// use zxspectrum_common::{
///     ModelRequest, ZxSpectrumModel, spectrum_model_ula_static_dispatch
/// };
///
/// fn info<C,S,X,F,R,W>(
///         model: &ZxSpectrumModel<C,S,X,F,R,W>,
///     ) -> String
///     where C: Cpu,
///           X: MemoryExtension
/// {
///     format!("model: {} cpu_hz: {} T-states/frame: {} pixel density: {}",
///         ModelRequest::from(model),
///         spectrum_model_ula_static_dispatch!(model(S, X)::CPU_HZ),
///         spectrum_model_ula_static_dispatch!(model(S, X)::FRAME_TSTATES),
///         spectrum_model_ula_static_dispatch!(model(S, X)::pixel_density())
///     )
/// }
/// ```
#[macro_export]
macro_rules! spectrum_model_ula_static_dispatch {
    ($model:ident($sdd:ty, $ext:ty)::$($expr:tt)*) => {
        spectrum_model_ula_static_dispatch!(($model)($sdd, $ext)::$($expr)*)
    };
    (($model:expr)($sdd:ty, $ext:ty)::$($expr:tt)*) => {
        {
            use spectrusty::memory;
            match $model {
                $crate::ZxSpectrumModel::Spectrum16(..) => {
                    $crate::UlaPAL::<memory::Memory16kEx, $crate::UlaPALDevice<$sdd>, $ext>::$($expr)*
                }
                $crate::ZxSpectrumModel::Spectrum48(..) => {
                    $crate::UlaPAL::<memory::Memory48kEx, $crate::UlaPALDevice<$sdd>, $ext>::$($expr)*
                }
                $crate::ZxSpectrumModel::Spectrum48Plus(..) => {
                    $crate::Plus48::<$crate::UlaPALDevice<$sdd>, $ext>::$($expr)*
                }
                $crate::ZxSpectrumModel::SpectrumNTSC(..) => {
                    $crate::UlaNTSC::<memory::Memory48kEx, $crate::UlaNTSCDevice<$sdd>, $ext>::$($expr)*
                }
                $crate::ZxSpectrumModel::Spectrum128(..)|
                $crate::ZxSpectrumModel::SpectrumPlus2(..) => {
                    $crate::Ula128AyKeypad::<$crate::Ula128Device<$sdd>, $ext>::$($expr)*
                }
                $crate::ZxSpectrumModel::SpectrumPlusPlus2(..) => {
                    $crate::Plus128::<$crate::Ula128Device<$sdd>, $ext>::$($expr)*
                }
                $crate::ZxSpectrumModel::SpectrumPlus2A(..) => {
                    $crate::Ula3Ay::<$crate::Ula3Device<$sdd>, $ext>::$($expr)*
                }
                $crate::ZxSpectrumModel::SpectrumPlus2B(..) => {
                    $crate::Plus3::<$crate::Ula3Device<$sdd>, $ext>::$($expr)*
                }
                $crate::ZxSpectrumModel::TimexTC2048(..) => {
                    $crate::TC2048::<$crate::UlaPALDevice<$sdd>, $ext>::$($expr)*
                }
            }
        }
    };
}

impl ModelRequest {
    /// Returns an iterator of all available model types.
    pub fn iter() -> ModelRequestIter {
        ModelRequestIter(Some(ModelRequest::Spectrum16))
    }
}

impl Iterator for ModelRequestIter {
    type Item = ModelRequest;
    fn next(&mut self) -> Option<Self::Item> {
        use ModelRequest::*;
        let res = self.0;
        self.0 = match self.0 {
            Some(Spectrum16)        => Some(Spectrum48),
            Some(Spectrum48)        => Some(SpectrumNTSC),
            Some(SpectrumNTSC)      => Some(Spectrum128),
            Some(Spectrum128)       => Some(SpectrumPlus2),
            Some(SpectrumPlus2)     => Some(SpectrumPlus2A),
            Some(SpectrumPlus2A)    => Some(SpectrumPlus3),
            Some(SpectrumPlus3)     => Some(TimexTC2048),
            Some(TimexTC2048)       => Some(Spectrum48Plus),
            Some(Spectrum48Plus)    => Some(SpectrumPlusPlus2),
            Some(SpectrumPlusPlus2) => Some(SpectrumPlus2B),
            Some(SpectrumPlus2B)    => None,
            None                    => None
        };
        res
    }
}

impl From<ModelRequest> for &str {
    fn from(model: ModelRequest) -> Self {
        use ModelRequest::*;
        match model {
            Spectrum16     => "ZX Spectrum 16k",
            Spectrum48     => "ZX Spectrum 48k",
            SpectrumNTSC   => "ZX Spectrum NTSC",
            Spectrum128    => "ZX Spectrum 128k",
            SpectrumPlus2  => "ZX Spectrum +2",
            SpectrumPlus2A => "ZX Spectrum +2A",
            SpectrumPlus3  => "ZX Spectrum +3",
            // SpectrumPlus3e => "ZX Spectrum +3e",
            // SpectrumSE     => "ZX Spectrum SE",
            TimexTC2048    => "Timex TC2048",
            // TimexTC2068    => "Timex TC2068",
            // TimexTS2068    => "Timex TS2068",
            Spectrum48Plus => "ZX Spectrum 48k+",
            SpectrumPlusPlus2 => "ZX Spectrum ++2",
            SpectrumPlus2B => "ZX Spectrum +2B",
        }

    }
}

impl From<&'_ ModelRequest> for &str {
    fn from(model: &ModelRequest) -> Self {
        (*model).into()
    }
}

impl TryFrom<ComputerModel> for ModelRequest {
    type Error = String;

    fn try_from(model: ComputerModel) -> Result<Self, Self::Error> {
        use ComputerModel::*;
        Ok(match model {
            Spectrum16 => ModelRequest::Spectrum16,
            Spectrum48 => ModelRequest::Spectrum48,
            SpectrumNTSC => ModelRequest::SpectrumNTSC,
            Spectrum128 => ModelRequest::Spectrum128,
            SpectrumPlus2 => ModelRequest::SpectrumPlus2,
            SpectrumPlus2A => ModelRequest::SpectrumPlus2A,
            SpectrumPlus3 => ModelRequest::SpectrumPlus3,
            TimexTC2048 => ModelRequest::TimexTC2048,
            _ => return Err(
                format!("computer model: {} is currently not supported by this emulator", model)
            )
        })
    }
}

impl From<ModelRequest> for ComputerModel {
    fn from(req: ModelRequest) -> ComputerModel {
        use ModelRequest::*;
        match req {
            Spectrum16 => ComputerModel::Spectrum16,
            Spectrum48 => ComputerModel::Spectrum48,
            Spectrum48Plus => ComputerModel::Spectrum48,
            SpectrumNTSC => ComputerModel::SpectrumNTSC,
            Spectrum128 => ComputerModel::Spectrum128,
            SpectrumPlus2 => ComputerModel::SpectrumPlus2,
            SpectrumPlusPlus2 => ComputerModel::SpectrumPlus2,
            SpectrumPlus2A => ComputerModel::SpectrumPlus2A,
            SpectrumPlus2B => ComputerModel::SpectrumPlus2A,
            SpectrumPlus3 => ComputerModel::SpectrumPlus3,
            TimexTC2048 => ComputerModel::TimexTC2048,
        }
    }
}

impl fmt::Display for ModelRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        <&str>::from(self).fmt(f)
    }
}

impl FromStr for ModelRequest {
    type Err = &'static str;
    fn from_str(mut model: &str) -> Result<Self, Self::Err> {
        // if let Some(unfixed) = model.strip_prefix("ZX Spectrum ") {
        //     model = unfixed;
        // }
        if model.starts_with("ZX Spectrum ") {
            model = &model["ZX Spectrum ".len()..];
        }
        else if model.starts_with("Timex ") {
            model = &model["Timex ".len()..];
        }
        match model {
            "16k"    => Ok(ModelRequest::Spectrum16),
            "48k"    => Ok(ModelRequest::Spectrum48),
            "48k+"   => Ok(ModelRequest::Spectrum48Plus),
            "NTSC"   => Ok(ModelRequest::SpectrumNTSC),
            "128k"   => Ok(ModelRequest::Spectrum128),
            "+2"     => Ok(ModelRequest::SpectrumPlus2),
            "+2A"    => Ok(ModelRequest::SpectrumPlus2A),
            "+3"     => Ok(ModelRequest::SpectrumPlus3),
            "TC2048" => Ok(ModelRequest::TimexTC2048),
            "++2"    => Ok(ModelRequest::SpectrumPlusPlus2),
            "+2B"    => Ok(ModelRequest::SpectrumPlus2B),
                   _ => Err("Unrecognized computer model")
        }
    }
}

impl<C: Cpu, S, X, F, R, W> From<&ZxSpectrumModel<C, S, X, F, R, W>> for ModelRequest {
    fn from(model: &ZxSpectrumModel<C, S, X, F, R, W>) -> Self {
        match model {
            ZxSpectrumModel::Spectrum16(..) => ModelRequest::Spectrum16,
            ZxSpectrumModel::Spectrum48(..) => ModelRequest::Spectrum48,
            ZxSpectrumModel::SpectrumNTSC(..) => ModelRequest::SpectrumNTSC,
            ZxSpectrumModel::Spectrum128(..) => ModelRequest::Spectrum128,
            ZxSpectrumModel::SpectrumPlus2(..) => ModelRequest::SpectrumPlus2,
            ZxSpectrumModel::SpectrumPlus2A(..) => ModelRequest::SpectrumPlus2A,
            ZxSpectrumModel::TimexTC2048(..) => ModelRequest::TimexTC2048,
            ZxSpectrumModel::Spectrum48Plus(..) => ModelRequest::Spectrum48Plus,
            ZxSpectrumModel::SpectrumPlusPlus2(..) => ModelRequest::SpectrumPlusPlus2,
            ZxSpectrumModel::SpectrumPlus2B(..) => ModelRequest::SpectrumPlus2B,
        }
    }
}

impl<C, S, X, F, R, W> ZxSpectrumModel<C, S, X, F, R, W>
    where C: Cpu,
          X: MemoryExtension + Default,
          R: Default,
          W: Default
{
    /// Returns an instance of a requested model.
    ///
    /// The model is initialized with the `ROM` present in the model's memory.
    pub fn new(req: ModelRequest) -> Self {
        use ModelRequest::*;
        match req {
            Spectrum16 => ZxSpectrumModel::Spectrum16(ZxSpectrum::default().with_roms(ROM48, 0)),
            Spectrum48 => ZxSpectrumModel::Spectrum48(ZxSpectrum::default().with_roms(ROM48, 0)),
            SpectrumNTSC => ZxSpectrumModel::SpectrumNTSC(ZxSpectrum::default().with_roms(ROM48, 0)),
            Spectrum128 => ZxSpectrumModel::Spectrum128(ZxSpectrum::default().with_roms(ROM128, 0)),
            SpectrumPlus2 => ZxSpectrumModel::SpectrumPlus2(ZxSpectrum::default().with_roms(ROM_PLUS2, 0)),
            SpectrumPlus2A => ZxSpectrumModel::SpectrumPlus2A(ZxSpectrum::default().with_roms(ROM_PLUS3, 0)),
            TimexTC2048 => ZxSpectrumModel::TimexTC2048(ZxSpectrum::default().with_roms(ROM_TC2048, 9)),
            Spectrum48Plus => ZxSpectrumModel::Spectrum48Plus(ZxSpectrum::default().with_roms(ROM48, 0)),
            SpectrumPlusPlus2 => ZxSpectrumModel::SpectrumPlusPlus2(ZxSpectrum::default().with_roms(ROM_PLUS2, 0)),
            SpectrumPlus2B => ZxSpectrumModel::SpectrumPlus2B(ZxSpectrum::default().with_roms(ROM_PLUS2B, 0)),
            SpectrumPlus3 => {
                unimplemented!()
                // ZxSpectrumModel::SpectrumPlus3(ZxSpectrum::default().with_roms(ROM_PLUS3, 0))
            }
        }
    }
}

impl<C: Cpu, U, F> ZxSpectrum<C, U, F>
    where U: MemoryAccess
{
    /// Returns a model with the provided ROM data as `roms` pages and a rom bank `offset`.
    pub fn with_roms(mut self, roms: &[&[u8]], offset: usize) -> Self {
        let mem = self.ula.memory_mut();
        mem.fill_mem(.., random).unwrap();
        for (n, rom) in roms.iter().flat_map(|rom| rom.chunks_exact(U::Memory::PAGE_SIZE)).enumerate() {
            mem.load_into_rom_bank(n + offset, rom).unwrap();
        }
        self
    }
}

impl<C: Cpu, U, F> ZxSpectrum<C, U, F>
    where Self: DynamicDevices
{
    /// Overrides the state of the emulator from the provided `state` value.
    ///
    /// Takes care of rebuilding the device index.
    pub fn set_state(&mut self, state: EmulatorState<F>) {
        self.state = state;
        self.rebuild_device_index();
    }
}

impl<C: Cpu, M, D, X, V, F> ZxSpectrum<C, Ula<M, D, X, V>, F>
    where M: ZxMemory,
          X: MemoryExtension,
          V: VideoFrame
{
    /// Copies the paged in RAM memory, the border color, the CPU state and the emulator 
    /// state into `self` from the provided `model`.
    pub fn copy_from<S, R, W>(&mut self, model: ZxSpectrumModel<C, S, X, F, R, W>)
        where Self: DynamicDevices,
              R: io::Read + fmt::Debug + Default,
              W: io::Write + fmt::Debug + Default,
    {
        let border = model.border_color();
        let mem_rd = model.read_ram();
        let _ = self.ula.memory_mut().load_into_mem(M::PAGE_SIZE as u16.., mem_rd);
        let (cpu, state) = model.into_cpu_and_state();
        self.cpu = cpu;
        self.set_state(state);
        self.ula.set_border_color(border);
    }
}

impl<C: Cpu, D, X, F> ZxSpectrum<C, Plus48<D, X>, F>
    where X: MemoryExtension
{
    /// Copies the paged in RAM memory, the border color, the CPU state and the emulator 
    /// state into `self` from the provided `model`.
    pub fn copy_from<S, R, W>(&mut self, model: ZxSpectrumModel<C, S, X, F, R, W>)
        where Self: DynamicDevices,
              <Self as SpectrumUla>::Chipset: MemoryAccess,
              R: io::Read + fmt::Debug + Default,
              W: io::Write + fmt::Debug + Default,
    {
        let border = model.border_color();
        let mem_rd = model.read_ram();
        let _ = self.ula.memory_mut().load_into_mem(
            <<Self as SpectrumUla>::Chipset as MemoryAccess>::Memory::PAGE_SIZE as u16.., mem_rd);
        let (cpu, state) = model.into_cpu_and_state();
        self.cpu = cpu;
        self.set_state(state);
        self.ula.set_border_color(border);
    }
}

impl<C: Cpu, D, X, V, F> ZxSpectrum<C, Scld<Memory48kDock64kEx, D, X, V>, F>
    where X: MemoryExtension,
          V: VideoFrame,
{
    /// Copies the paged in RAM memory, the border color, the CPU state and the emulator 
    /// state into `self` from the provided `model`.
    pub fn copy_from<S, R, W>(&mut self, model: ZxSpectrumModel<C, S, X, F, R, W>)
        where Self: DynamicDevices,
              R: io::Read + fmt::Debug + Default,
              W: io::Write + fmt::Debug + Default
    {
        let border = model.border_color();
        let mem_rd = model.read_ram();
        let _ = self.ula.memory_mut().load_into_mem(
                    2 * Memory48kDock64kEx::PAGE_SIZE as u16.., mem_rd);
        let (cpu, state) = model.into_cpu_and_state();
        self.cpu = cpu;
        self.set_state(state);
        self.ula.set_border_color(border);
    }
}

impl<C: Cpu, D, X, F, R, W> ZxSpectrum128k<C, D, X, F, R, W>
    where D: BusDevice,
          X: MemoryExtension,
{
    /// Copies the paged in RAM memory, the border color, the CPU state and the emulator 
    /// state into `self` from the provided `model`. Locks `self` into 48k mode.
    pub fn copy_from<S>(&mut self, model: ZxSpectrumModel<C, S, X, F, R, W>)
        where Self: DynamicDevices,
              <Self as SpectrumUla>::Chipset: MemoryAccess,
              R: io::Read + fmt::Debug,
              W: io::Write + fmt::Debug
    {
        let border = model.border_color();
        let mem_rd = model.read_ram();
        let _ = self.ula.memory_mut().load_into_mem(
                <<Self as SpectrumUla>::Chipset as MemoryAccess>::Memory::PAGE_SIZE as u16..,
                mem_rd);
        let (cpu, state) = model.into_cpu_and_state();
        self.cpu = cpu;
        self.set_state(state);
        self.ula.set_border_color(border);
        // lock in 48k mode until reset
        self.lock_48k_mode();
    }

    pub fn lock_48k_mode(&mut self) {
        self.ula.set_ula128_mem_port_value(Ula128MemFlags::ROM_BANK
                                          |Ula128MemFlags::LOCK_MMU);
        // self.ula.write_io(0x7ffd, 0b0011_0000, VideoTs::default());
    }
}

impl<C: Cpu, D, X, F, R, W> ZxSpectrum128kPlus<C, D, X, F, R, W>
    where D: BusDevice,
          X: MemoryExtension
{
    /// Copies the paged in RAM memory, the border color, the CPU state and the emulator 
    /// state into `self` from the provided `model`. Locks `self` into 48k mode.
    pub fn copy_from<S>(&mut self, model: ZxSpectrumModel<C, S, X, F, R, W>)
        where Self: DynamicDevices,
              <Self as SpectrumUla>::Chipset: MemoryAccess,
              R: io::Read + fmt::Debug,
              W: io::Write + fmt::Debug
    {
        let border = model.border_color();
        let mem_rd = model.read_ram();
        let _ = self.ula.memory_mut().load_into_mem(
                <<Self as SpectrumUla>::Chipset as MemoryAccess>::Memory::PAGE_SIZE as u16..,
                mem_rd);
        let (cpu, state) = model.into_cpu_and_state();
        self.cpu = cpu;
        self.set_state(state);
        self.ula.set_border_color(border);
        // lock in 48k mode until reset
        self.lock_48k_mode();
    }

    pub fn lock_48k_mode(&mut self) {
        self.ula.set_ula128_mem_port_value(Ula128MemFlags::ROM_BANK
                                          |Ula128MemFlags::LOCK_MMU);
        // self.ula.write_io(0x7ffd, 0b0011_0000, VideoTs::default());
    }
}

impl<C: Cpu, D, X, F, R, W> ZxSpectrum2A<C, D, X, F, R, W>
    where D: BusDevice,
          X: MemoryExtension,
          R: io::Read + fmt::Debug,
          W: io::Write + fmt::Debug,
{
    /// Copies the paged in RAM memory, the border color, the CPU state and the emulator 
    /// state into `self` from the provided `model`. Locks `self` into 48k mode.
    pub fn copy_from<S>(&mut self, model: ZxSpectrumModel<C, S, X, F, R, W>)
        where Self: DynamicDevices,
              <Self as SpectrumUla>::Chipset: MemoryAccess
    {
        let border = model.border_color();
        let mem_rd = model.read_ram();
        let _ = self.ula.memory_mut().load_into_mem(
                <<Self as SpectrumUla>::Chipset as MemoryAccess>::Memory::PAGE_SIZE as u16..,
                mem_rd);
        let (cpu, state) = model.into_cpu_and_state();
        self.cpu = cpu;
        self.set_state(state);
        self.ula.set_border_color(border);
        // lock in 48k mode until reset
        self.lock_48k_mode();
    }

    pub fn lock_48k_mode(&mut self) {
        self.ula.set_ula3_ctrl_port_value(Ula3CtrlFlags::ROM_BANK_HI);
        // self.ula.write_io(0x1ffd, 0b0000_0100, VideoTs::default());
        self.ula.set_ula128_mem_port_value(Ula128MemFlags::ROM_BANK
                                          |Ula128MemFlags::LOCK_MMU);
        // self.ula.write_io(0x7ffd, 0b0011_0000, VideoTs::default());
    }
}

impl<C: Cpu, D, X, F, R, W> ZxSpectrum2B<C, D, X, F, R, W>
    where D: BusDevice,
          X: MemoryExtension,
          R: io::Read + fmt::Debug,
          W: io::Write + fmt::Debug,
{
    /// Copies the paged in RAM memory, the border color, the CPU state and the emulator 
    /// state into `self` from the provided `model`. Locks `self` into 48k mode.
    pub fn copy_from<S>(&mut self, model: ZxSpectrumModel<C, S, X, F, R, W>)
        where Self: DynamicDevices,
              <Self as SpectrumUla>::Chipset: MemoryAccess
     {
        let border = model.border_color();
        let mem_rd = model.read_ram();
        let _ = self.ula.memory_mut().load_into_mem(
                <<Self as SpectrumUla>::Chipset as MemoryAccess>::Memory::PAGE_SIZE as u16..,
                mem_rd);
        let (cpu, state) = model.into_cpu_and_state();
        self.cpu = cpu;
        self.set_state(state);
        self.ula.set_border_color(border);
        // lock in 48k mode until reset
        self.lock_48k_mode();
    }

    pub fn lock_48k_mode(&mut self) {
        self.ula.set_ula3_ctrl_port_value(Ula3CtrlFlags::empty());
        // self.ula.write_io(0x1ffd, 0b0000_0000, VideoTs::default());
        self.ula.set_ula128_mem_port_value(Ula128MemFlags::ROM_BANK
                                          |Ula128MemFlags::LOCK_MMU);
        // self.ula.write_io(0x7ffd, 0b0011_0000, VideoTs::default());
    }
}

impl<C, S, X, F, R, W> ZxSpectrumModel<C, S, X, F, R, W>
    where C: Cpu,
          X: MemoryExtension,
          R: io::Read + fmt::Debug,
          W: io::Write + fmt::Debug
{
    pub fn emulator_state_mut(&mut self) -> &mut EmulatorState<F> {
        spectrum_model_dispatch!(self(spec) => &mut spec.state)
    }

    pub fn emulator_state_ref(&self) -> &EmulatorState<F> {
        spectrum_model_dispatch!(self(spec) => &spec.state)
    }

    pub fn set_emulator_state(&mut self, state: EmulatorState<F>) {
        spectrum_model_dispatch!(self(spec) => spec.set_state(state))
    }

    /// Returns the adjusted CPU clock frequency of this model.
    pub fn effective_cpu_rate(&self) -> f64 {
        spectrum_model_dispatch!(self(spec) => spec.effective_cpu_rate())
    }

    /// Returns the adjusted duration of a single execution frame in nanoseconds.
    pub fn effective_frame_duration_nanos(&self) -> u32 {
        spectrum_model_dispatch!(self(spec) => spec.effective_frame_duration_nanos())
    }

    /// Returns the adjusted duration of a single execution frame.
    pub fn effective_frame_duration(&self) -> Duration {
        spectrum_model_dispatch!(self(spec) => spec.effective_frame_duration())
    }

    pub fn ensure_audio_frame_time<B: Blep>(&self, blep: &mut B, sample_rate: u32) {
        spectrum_model_dispatch!(self(spec) => spec.ensure_audio_frame_time(blep, sample_rate))
    }

    /// Returns the original CPU clock frequency of this model.
    pub fn cpu_rate(&self) -> u32 {
        spectrum_model_ula_static_dispatch!(self(S, X)::CPU_HZ)
    }

    /// Returns the number of T-states per frame of this model.
    pub fn frame_tstates_count(&self) -> FTs {
        spectrum_model_ula_static_dispatch!(self(S, X)::FRAME_TSTATES)
    }

    /// Returns the original duration of a single execution frame in nanoseconds.
    pub fn frame_duration_nanos(&self) -> u32 {
        spectrum_model_ula_static_dispatch!(self(S, X)::frame_duration_nanos())
    }

    /// Returns the original duration of a single execution frame.
    pub fn frame_duration(&self) -> Duration {
        spectrum_model_ula_static_dispatch!(self(S, X)::frame_duration())
    }

    pub fn lock_48k_mode(&mut self) -> bool {
        match self {
            ZxSpectrumModel::Spectrum16(..) => return false,
            ZxSpectrumModel::Spectrum128(spec128)|
            ZxSpectrumModel::SpectrumPlus2(spec128) => spec128.lock_48k_mode(),
            ZxSpectrumModel::SpectrumPlus2A(spec3) => spec3.lock_48k_mode(),
            ZxSpectrumModel::SpectrumPlusPlus2(plus128) => plus128.lock_48k_mode(),
            ZxSpectrumModel::SpectrumPlus2B(plus128) => plus128.lock_48k_mode(),
            _ => {}
        }
        true
    }

    /// Returns the [Video::PIXEL_DENSITY] of this model.
    pub fn pixel_density(&self) -> u32 {
        spectrum_model_ula_static_dispatch!(self(S, X)::PIXEL_DENSITY)
    }

    /// Destructs `self` into [Cpu] and [EmulatorState].
    pub fn into_cpu_and_state(self) -> (C, EmulatorState<F>) {
        spectrum_model_dispatch!(self(spec) => (spec.cpu, spec.state))
    }
    /// Returns a dynamicaly dispatched reader from paged in RAM.
    pub fn read_ram<'a>(&'a self) -> Box<dyn Read + 'a> {
        match self {
            ZxSpectrumModel::Spectrum16(spec16) => Box::new(spec16.ula.memory_ref().ram_ref()
                                                                      .chain(std::io::repeat(!0))),
            ZxSpectrumModel::Spectrum48(spec48) => Box::new(spec48.ula.memory_ref().ram_ref()),
            ZxSpectrumModel::Spectrum48Plus(plus48) => Box::new(plus48.ula.memory_ref().ram_ref()),
            ZxSpectrumModel::SpectrumNTSC(spec48) => Box::new(spec48.ula.memory_ref().ram_ref()),
            ZxSpectrumModel::TimexTC2048(timex) => Box::new(timex.ula.memory_ref().ram_ref()),
            ZxSpectrumModel::Spectrum128(spec128)|
            ZxSpectrumModel::SpectrumPlus2(spec128) => {
                let mem = spec128.ula.memory_ref();
                // returns paged in RAM banks as a chained reader
                Box::new(mem.page_ref(1).unwrap()
                    .chain(mem.page_ref(2).unwrap())
                    .chain(mem.page_ref(3).unwrap()))
            }
            ZxSpectrumModel::SpectrumPlusPlus2(spec128) => {
                let mem = spec128.ula.memory_ref();
                // returns paged in RAM banks as a chained reader
                Box::new(mem.page_ref(1).unwrap()
                    .chain(mem.page_ref(2).unwrap())
                    .chain(mem.page_ref(3).unwrap()))
            }
            ZxSpectrumModel::SpectrumPlus2A(spec3) => {
                let mem = spec3.ula.memory_ref();
                // returns paged in RAM banks as a chained reader
                Box::new(mem.page_ref(1).unwrap()
                    .chain(mem.page_ref(2).unwrap())
                    .chain(mem.page_ref(3).unwrap()))
            }
            ZxSpectrumModel::SpectrumPlus2B(plus128) => {
                let mem = plus128.ula.memory_ref();
                // returns paged in RAM banks as a chained reader
                Box::new(mem.page_ref(1).unwrap()
                    .chain(mem.page_ref(2).unwrap())
                    .chain(mem.page_ref(3).unwrap()))
            }
        }
    }

    pub fn border_color(&self) -> BorderColor {
        spectrum_model_dispatch!(self(spec) => spec.ula.border_color())
    }

    pub fn cpu_mut(&mut self) -> &mut C {
        spectrum_model_dispatch!(self(spec) => &mut spec.cpu)
    }

    pub fn cpu_ref(&self) -> &C {
        spectrum_model_dispatch!(self(spec) => &spec.cpu)
    }

    pub fn current_tstate(&self) -> FTs {
        spectrum_model_dispatch!(self(spec) => spec.ula.current_tstate())
    }

    /// Hot-swaps hardware model.
    pub fn change_model(&mut self, request: ModelRequest)
        where X: Default, R: Default, W: Default,
    {
        use ZxSpectrumModel::*;
        match (&*self, request) {
            (Spectrum16(..), ModelRequest::Spectrum16)|
            (Spectrum48(..), ModelRequest::Spectrum48)|
            (SpectrumNTSC(..), ModelRequest::SpectrumNTSC)|
            (Spectrum128(..), ModelRequest::Spectrum128)|
            (SpectrumPlus2(..), ModelRequest::SpectrumPlus2)|
            (SpectrumPlus2A(..), ModelRequest::SpectrumPlus2A)|
            (TimexTC2048(..), ModelRequest::TimexTC2048)|
            (Spectrum48Plus(..), ModelRequest::Spectrum48Plus)|
            (SpectrumPlusPlus2(..), ModelRequest::SpectrumPlusPlus2)|
            (SpectrumPlus2B(..), ModelRequest::SpectrumPlus2B) => {
                return
            }
            _ => {}
        }
        let prev_model = core::mem::replace(self, Self::new(request));
        spectrum_model_dispatch!(self(spec) => spec.copy_from(prev_model))
    }

    pub fn set_frame_tstate(&mut self, ts: FTs) {
        spectrum_model_dispatch!(self(spec) => spec.ula.set_frame_tstate(ts))
    }

    pub fn write_port(&mut self, port: u16, data: u8) {
        spectrum_model_dispatch!(self(spec) => spec.ula.write_io(port, data, spec.ula.current_video_ts()));
    }

    pub fn ensure_cpu_is_safe_for_snapshot(&mut self) {
        spectrum_model_dispatch!(self(spec) => ensure_cpu_is_safe_for_snapshot(&mut spec.cpu, &mut spec.ula))
    }
}

impl<C: Cpu, U, F> ZxSpectrum<C, U, F>
    where F: io::Read + io::Write + io::Seek,
          U: UlaCommon
{
    /// Resets Spectrum as a specified `model`, waits for the boot sequence to end and performs auto-type
    /// so the data from the tape can be loaded.
    ///
    /// Runs as many frames as necessary to accomplish the task.
    ///
    /// Returns the tuple from [ZxSpectrum::run_with_auto_type].
    pub fn reset_and_load(&mut self, model: ModelRequest) -> crate::spectrum::Result<(FTs, bool)> {
        self.reset(true);
        use ModelRequest::*;
        type Zk = spectrusty::peripherals::ZXKeyboardMap;
        const LOAD_SE: Zk = Zk::from_bits_retain(Zk::SS.bits()|Zk::Q.bits());
        const QUOTE: Zk = Zk::from_bits_retain(Zk::SS.bits()|Zk::P.bits());
        const LOAD_QQ_EN: &[(Zk, u32)] = &[(Zk::J, 1), (QUOTE, 1), (Zk::SS, 4), (QUOTE, 1), (Zk::EN, 1)];
        const LOAD_QQ_EN_SE: &[(Zk, u32)] = &[(Zk::EN, 1), (Zk::empty(), 17), (LOAD_SE, 1), (QUOTE, 1), (Zk::SS, 4), (QUOTE, 1), (Zk::EN, 1)];
        match model {
            Spectrum16 => self.run_with_auto_type(48, LOAD_QQ_EN),
            Spectrum48|Spectrum48Plus|TimexTC2048 => self.run_with_auto_type(87, LOAD_QQ_EN),
            SpectrumNTSC => self.run_with_auto_type(103, LOAD_QQ_EN),
            Spectrum128|SpectrumPlus2|SpectrumPlusPlus2 => {
                self.run_with_auto_type(67, iter::once(&(Zk::EN, 1)))
            }
            SpectrumPlus2A => self.run_with_auto_type(87, iter::once(&(Zk::EN, 1))),
            SpectrumPlus2B => self.run_with_auto_type(63, LOAD_QQ_EN_SE),
            _ => unimplemented!("reset_and_load unimplemented for specified model")
        }
    }
}

impl<C, S, X, F, R, W> ZxSpectrumModel<C, S, X, F, R, W>
    where C: Cpu,
          S: 'static,
          X: MemoryExtension + 'static,
          F: io::Read + io::Write + io::Seek,
          R: io::Read + fmt::Debug + 'static,
          W: io::Write + fmt::Debug + 'static,
{
    /// Resets the current model, waits for the boot sequence to end and performs auto-type
    /// so the data from the tape can be loaded.
    ///
    /// Runs as many frames as necessary to accomplish the task.
    ///
    /// Returns the tuple from [ZxSpectrum::run_with_auto_type].
    pub fn reset_and_load(&mut self) -> crate::spectrum::Result<(FTs, bool)> {
        let model = ModelRequest::from(&*self);
        spectrum_model_dispatch!(self(spec) => spec.reset_and_load(model))
    }
}

impl<CF: Flavour, S, X, F, R, W> ZxSpectrumModel<Z80<CF>, S, X, F, R, W> {
    pub fn set_cpu<I: Into<Z80<CF>>>(&mut self, cpu: I) {
        let cpu = cpu.into();
        spectrum_model_dispatch!(self(spec) => spec.cpu = cpu)
    }
}

impl<S, X, F, R, W> ZxSpectrumModel<Z80Any, S, X, F, R, W> {
    pub fn set_cpu_any<I: Into<Z80Any>>(&mut self, cpu: I) {
        let cpu = cpu.into();
        spectrum_model_dispatch!(self(spec) => spec.cpu = cpu)
    }
}

impl<M, B, X, V> UlaPlusMode for Ula<M, B, X, V> {}
impl<M: PagedMemory8k, B, X, V> UlaPlusMode for Scld<M, B, X, V> {}
impl<D, X> UlaPlusMode for Ula128<D, X> {}
impl<D, X> UlaPlusMode for Ula3<D, X> {}
impl<'a, U> UlaPlusMode for UlaPlus<U>
    where U: UlaPlusInner<'a>
{
    fn is_ulaplus_enabled(&self) -> bool {
        self.is_ulaplus_enabled()
    }

    fn enable_ulaplus_modes(&mut self, enable: bool) -> bool {
        self.enable_ulaplus_modes(enable);
        true
    }
}
