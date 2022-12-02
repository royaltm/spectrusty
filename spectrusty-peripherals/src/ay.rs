/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
//! The **AY-3-8910** programmable sound generator chipset family.
//!
//! This module contains chipset I/O interface protocol traits and helper types.
//!
//! The sound emulation is in a separate module, please see [audio].
use core::fmt;
use core::ops::{Deref, DerefMut};
use core::marker::PhantomData;

#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};

pub mod audio;
pub mod serial128;

use spectrusty_core::clock::FTs;

/// An enumeration of AY-3-8910 registers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
#[repr(u8)]
pub enum AyRegister {
      ToneFineA      =  0,
      ToneCoarseA    =  1,
      ToneFineB      =  2,
      ToneCoarseB    =  3,
      ToneFineC      =  4,
      ToneCoarseC    =  5,
      NoisePeriod    =  6,
      MixerControl   =  7,
      AmpLevelA      =  8,
      AmpLevelB      =  9,
      AmpLevelC      = 10,
      EnvPerFine     = 11,
      EnvPerCoarse   = 12,
      EnvShape       = 13,
      IoA            = 14,
      IoB            = 15,
}

pub const NUM_SOUND_GEN_REGISTERS: usize = 14;

const REG_MASKS: [u8;16] = [
    0xff, 0x0f, 0xff, 0x0f, 0xff, 0x0f, 0x1f, 0xff,
    0x1f, 0x1f, 0x1f, 0xff, 0xff, 0x0f, 0xff, 0xff
];

/// A helper trait for matching I/O port addresses for AY-3-8910.
pub trait AyPortDecode: fmt::Debug {
    /// A mask of significant address bus bits for port decoding.
    const PORT_MASK: u16;
    /// A mask of address bus bit values - for the register selection function.
    const PORT_SELECT: u16;
    /// A mask of address bus bit values - for the reading from the selected register function.
    const PORT_DATA_READ: u16;
    /// A mask of address bus bit values - for the writing to the selected register function.
    const PORT_DATA_WRITE: u16;
    /// Return `true` if the port matches the register selection function.
    #[inline]
    fn is_select(port: u16) -> bool {
        port & Self::PORT_MASK == Self::PORT_SELECT & Self::PORT_MASK
    }
    /// Return `true` if the port matches the register reading function.
    #[inline]
    fn is_data_read(port: u16) -> bool {
        port & Self::PORT_MASK == Self::PORT_DATA_READ & Self::PORT_MASK
    }
    /// Return `true` if the port matches the register writing function.
    #[inline]
    fn is_data_write(port: u16) -> bool {
        port & Self::PORT_MASK == Self::PORT_DATA_WRITE & Self::PORT_MASK
    }
    /// A helper for writing data to one of the functions decoded from `port` address.
    #[inline]
    fn write_ay_io<T,R,A,B>(
                ay_io: &mut Ay3_891xIo<T,R,A,B>,
                port: u16,
                data: u8,
                timestamp: T
            ) -> bool
        where A: AyIoPort<Timestamp=T>,
              B: AyIoPort<Timestamp=T>,
              R: AyRegRecorder<Timestamp=T>
    {
        match port & Self::PORT_MASK {
            p if p == Self::PORT_SELECT => {
                ay_io.select_port_write(data);
                true
            }
            p if p == Self::PORT_DATA_WRITE => {
                ay_io.data_port_write(port, data, timestamp);
                true
            }
            _ => false
        }
    }
}

/// Matches I/O port addresses for AY-3-8912 used by the **ZX Spectrum+ 128k** or a *Melodik* interface.
#[derive(Clone, Copy, Default, Debug)]
pub struct Ay128kPortDecode;
impl AyPortDecode for Ay128kPortDecode {
    const PORT_MASK      : u16 = 0b1100_0000_0000_0010;
    const PORT_SELECT    : u16 = 0b1100_0000_0000_0000;
    const PORT_DATA_READ : u16 = 0b1100_0000_0000_0000;
    const PORT_DATA_WRITE: u16 = 0b1000_0000_0000_0000;
}

/// Matches I/O port addresses for AY-3-8912 used by the *Fuller Box* interface.
#[derive(Clone, Copy, Default, Debug)]
pub struct AyFullerBoxPortDecode;
impl AyPortDecode for AyFullerBoxPortDecode {
    const PORT_MASK      : u16 = 0x00ff;
    const PORT_SELECT    : u16 = 0x003f;
    const PORT_DATA_READ : u16 = 0x003f;
    const PORT_DATA_WRITE: u16 = 0x005f;
}

/// Matches I/O port addresses for AY-3-8912 used by the *Timex TC2068* computer series.
#[derive(Clone, Copy, Default, Debug)]
pub struct AyTC2068PortDecode;
impl AyPortDecode for AyTC2068PortDecode {
    const PORT_MASK      : u16 = 0x00ff;
    const PORT_SELECT    : u16 = 0x00f5;
    const PORT_DATA_READ : u16 = 0x00f6;
    const PORT_DATA_WRITE: u16 = 0x00f6;
}

/// A type for recording timestamped changes to one of AY-3-8910 audio registers.
///
/// Instances of this type are being used by [Ay3_891xAudio][audio::Ay3_891xAudio]
/// for sound generation. See also [AyRegRecorder].
#[derive(Clone, Copy, Debug)]
pub struct AyRegChange {
    /// A timestamp in `CPU` cycles (T-states), relative to the beginning of the current frame.
    pub time: FTs,
    /// Which register is being changed.
    pub reg: AyRegister,
    /// A new value loaded into the register.
    ///
    /// *NOTE*: may contain bits set to 1 that are unused by the indicated register.
    pub val: u8
}

/// This trait should be implemented by emulators of devices attached to one of two [Ay3_891xIo] I/O ports.
pub trait AyIoPort {
    type Timestamp: Sized;
    /// Resets the device at the given `timestamp`.
    #[inline]
    fn ay_io_reset(&mut self, _timestamp: Self::Timestamp) {}
    /// Writes data to the device at the given `timestamp`.
    #[inline]
    fn ay_io_write(&mut self, _addr: u16, _data: u8, _timestamp: Self::Timestamp) {}
    /// Reads data from the device at the given `timestamp`.
    #[inline]
    fn ay_io_read(&mut self, _addr: u16, _timestamp: Self::Timestamp) -> u8 { 0xff }
    /// Signals the device when the current frame ends, providing the value of the cycle clock after
    /// the frame execution stopped.
    #[inline]
    fn end_frame(&mut self, _timestamp: Self::Timestamp) {}
}

/// An [Ay3_891xIo] I/O device implementation that does nothing.
#[derive(Default, Clone, Copy, Debug)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
pub struct AyIoNullPort<T>(PhantomData<T>);

/// Allows recording of changes to AY-3-8910 audio registers with timestamps.
pub trait AyRegRecorder {
    type Timestamp;
    /// Should record a new value of indicated register with the given `timestamp`.
    ///
    /// *NOTE*: It is up to the caller to ensure the timestamps are added in an ascending order.
    fn record_ay_reg_change(&mut self, reg: AyRegister, val: u8, timestamp: Self::Timestamp);
    /// Should remove data from the recorder.
    fn clear_ay_reg_changes(&mut self);
}

/// The type of [Ay3_891xIo] with two optional I/O ports (A and B) and [AyRegVecRecorder].
pub type Ay3_8910Io<T,A=AyIoNullPort<T>,B=AyIoNullPort<T>> = Ay3_891xIo<T, AyRegVecRecorder<T>, A, B>;
/// The type of [Ay3_891xIo] with one I/O port (A only) and [AyRegVecRecorder].
pub type Ay3_8912Io<T,A> = Ay3_8910Io<T, A>;
/// The type of [Ay3_891xIo] without I/O ports and [AyRegVecRecorder].
pub type Ay3_8913Io<T> = Ay3_8910Io<T>;

/// Implements a communication protocol with programmable sound generator AY-3-8910 / 8912 / 8913 and
/// its I/O ports.
///
/// Records values written to audio registers which can be used to produce sound using
/// [audio::Ay3_891xAudio].
///
/// The `recorder` type `R` needs to implement [AyRegRecorder] trait.
/// The port types `A` and `B` need to implement [AyIoPort] trait.
#[derive(Default, Clone, Debug)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "snapshot", serde(rename_all = "camelCase"))]
pub struct Ay3_891xIo<T, R, A, B> {
    /// Provides access to the recorded changes of sound generator registers.
    /// The changes are required to generate sound with [audio::Ay3_891xAudio].
    #[cfg_attr(feature = "snapshot", serde(skip))]
    pub recorder: R,
    /// An instance of port `A` [AyIoPort] I/O device implementation.
    #[cfg_attr(feature = "snapshot", serde(default))]
    pub port_a: A,
    /// An instance of port `B` [AyIoPort] I/O device implementation.
    #[cfg_attr(feature = "snapshot", serde(default))]
    pub port_b: B,
    regs: [u8; 16],
    selected_reg: AyRegister,
    #[cfg_attr(feature = "snapshot", serde(skip))]
    _ts: PhantomData<T>
}

/// A recorder for [Ay3_891xIo] that records nothing.
///
/// Useful when no sound will be generated.
#[derive(Default, Clone, Debug)]
pub struct AyRegNullRecorder<T>(PhantomData<T>);

/// A convenient recorder for [Ay3_891xIo] that records changes in a [Vec].
#[derive(Default, Clone, Debug)]
pub struct AyRegVecRecorder<T>(pub Vec<(T,AyRegister,u8)>);

impl<T,R,A,B> Ay3_891xIo<T,R,A,B>
where A: AyIoPort<Timestamp=T>,
      B: AyIoPort<Timestamp=T>,
      R: AyRegRecorder<Timestamp=T>
{
    /// Resets the state of internal registers, port devices, and which register is being
    /// currently selected for reading and writing.
    ///
    /// *NOTE*: does nothing to the recorded register changes.
    pub fn reset(&mut self, timestamp: T) where T: Copy {
        self.regs = Default::default();
        self.selected_reg = Default::default();
        self.port_a.ay_io_reset(timestamp);
        self.port_b.ay_io_reset(timestamp);
    }
    /// Clears recorder data and forwards the call to I/O port implementations.
    /// It can be used to indicate an end-of-frame.
    pub fn next_frame(&mut self, timestamp: T) where T: Copy {
        self.recorder.clear_ay_reg_changes();
        self.port_a.end_frame(timestamp);
        self.port_b.end_frame(timestamp);
    }
    /// Retrieves a current value of the indicated register.
    #[inline]
    pub fn get(&self, reg: AyRegister) -> u8 {
        let index = usize::from(reg);
        self.regs[index]
    }
    /// Returns a reference to all registers as an array of their current values.
    #[inline]
    pub fn registers(&self) -> &[u8;16] {
        &self.regs
    }
    /// Returns an iterator of `(register, value)` pairs over current registers.
    #[inline]
    pub fn iter_regs(&'_ self) -> impl Iterator<Item=(AyRegister, u8)> + '_ {
        self.regs.iter().enumerate().map(|(n, val)| ((n as u8).into(), *val))
    }
    /// Returns an iterator of `(register, value)` pairs over current registers used by the
    /// sound generator.
    #[inline]
    pub fn iter_sound_gen_regs(&'_ self) -> impl Iterator<Item=(AyRegister, u8)> + '_ {
        self.iter_regs().take(NUM_SOUND_GEN_REGISTERS)
    }
    /// Sets a current value of the indicated register.
    #[inline]
    pub fn set(&mut self, reg: AyRegister, val: u8) {
        let index = usize::from(reg);
        self.regs[index] = val & REG_MASKS[index];
    }
    /// Returns `true` if the control register bit controlling I/O port `A` input is reset.
    #[inline]
    pub fn is_ioa_input(&self) -> bool {
        self.get(AyRegister::MixerControl) & 0x40 == 0
    }
    /// Returns `true` if the control register bit controlling I/O port `B` input is reset.
    #[inline]
    pub fn is_iob_input(&self) -> bool {
        self.get(AyRegister::MixerControl) & 0x80 == 0
    }
    /// Returns `true` if the control register bit controlling I/O port `A` input is set.
    #[inline]
    pub fn is_ioa_output(&self) -> bool {
        !self.is_ioa_input()
    }
    /// Returns `true` if the control register bit controlling I/O port `B` input is reset.
    #[inline]
    pub fn is_iob_output(&self) -> bool {
        !self.is_iob_input()
    }
    /// Returns a previously selected register with [Ay3_891xIo::select_port_write].
    #[inline]
    pub fn selected_register(&self) -> AyRegister {
        self.selected_reg
    }
    /// Bits 0-3 of `data` selects a register to be read from or written to.
    ///
    /// This method is being used to interface the host controller I/O operation.
    #[inline]
    pub fn select_port_write(&mut self, data: u8) {
        self.selected_reg = AyRegister::from(data)
    }
    /// Writes data to a previously selected register and records a change
    /// in the attached recorder unless a write is performed to one of the I/O ports.
    ///
    /// This method is being used to interface the host controller I/O operation.
    #[inline]
    pub fn data_port_write(&mut self, port: u16, data: u8, timestamp: T) {
        self.set(self.selected_reg, data); // What to do when control is set to output for IO?
        match self.selected_reg {
            AyRegister::IoA => {
                self.port_a.ay_io_write(port, data, timestamp)
            }
            AyRegister::IoB => {
                self.port_b.ay_io_write(port, data, timestamp)
            }
            reg => self.recorder.record_ay_reg_change(reg, data, timestamp)
        }
    }
    /// Reads data from a previously selected register.
    ///
    /// This method is being used to interface the host controller I/O operation.
    #[inline]
    pub fn data_port_read(&mut self, port: u16, timestamp: T) -> u8 {
        match self.selected_reg {
            AyRegister::IoA => {
                let port_input = self.port_a.ay_io_read(port, timestamp);
                if self.is_ioa_input() {
                    port_input
                }
                else {
                    port_input & self.get(AyRegister::IoA)
                }
            }
            AyRegister::IoB => {
                let port_input = self.port_b.ay_io_read(port, timestamp);
                if self.is_iob_input() {
                    port_input
                }
                else {
                    port_input & self.get(AyRegister::IoB)
                }
            }
            reg => self.get(reg)
        }
    }
}

impl AyRegister {
    /// Returns an iterator of all [AyRegister] values in an ascending order.
    pub fn enumerate() -> impl Iterator<Item=AyRegister> {
        (0..15).map(AyRegister::from)
    }
}

impl Default for AyRegister {
    fn default() -> Self {
        AyRegister::ToneFineA
    }
}

impl From<u8> for AyRegister {
    fn from(value: u8) -> Self {
        unsafe { core::mem::transmute(value & 0x0F) }
    }
}

macro_rules! impl_from_ay_reg {
    ($($ty:ty),*) => { $(
        impl From<AyRegister> for $ty {
            #[inline(always)]
            fn from(reg: AyRegister) -> $ty {
                reg as $ty
            }
        }        
    )* };
}
impl_from_ay_reg!(u8, u16, u32, u64, usize);

impl AyRegChange {
    /// Creates a new `AyRegChange` from the given arguments.
    #[inline]
    pub const fn new(time: FTs, reg: AyRegister, val: u8) -> Self {
        AyRegChange { time, reg, val }
    }
    /// Creates a new `AyRegChange` from the given arguments, converting a provided `timestamp` first.
    pub fn new_from_ts<T: Into<FTs>>(timestamp: T, reg: AyRegister, val: u8) -> Self {
        let time = timestamp.into();
        Self::new(time, reg, val)
    }
}

impl<T> AyIoPort for AyIoNullPort<T> {
    type Timestamp = T;
}

impl<T> AyRegRecorder for AyRegNullRecorder<T> {
    type Timestamp = T;
    #[inline]
    fn record_ay_reg_change(&mut self, _reg: AyRegister, _val: u8, _timestamp: T) {}
    #[inline]
    fn clear_ay_reg_changes(&mut self) {}
}

impl<T> AyRegRecorder for AyRegVecRecorder<T> {
    type Timestamp = T;
    #[inline]
    fn record_ay_reg_change(&mut self, reg: AyRegister, val: u8, timestamp: T) {
        self.0.push((timestamp, reg, val));
    }
    #[inline]
    fn clear_ay_reg_changes(&mut self) {
        self.0.clear()
    }
}

impl<T: Into<FTs>> AyRegVecRecorder<T> {
    /// Constructs a draining iterator of [AyRegChange] items from an inner [Vec].
    pub fn drain_ay_reg_changes(&'_ mut self) -> impl Iterator<Item=AyRegChange> + '_ {
        self.0.drain(..).map(|(timestamp,reg,val)| AyRegChange::new_from_ts(timestamp,reg,val))
    }
}

impl<T> Deref for AyRegVecRecorder<T> {
    type Target = Vec<(T,AyRegister,u8)>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for AyRegVecRecorder<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
