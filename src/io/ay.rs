//! AY-3-8910/8912/8913 I/O.
use core::fmt;
use core::ops::{Deref, DerefMut};
use crate::clock::{VFrameTsCounter, FTs, VideoTs};
use crate::video::VideoFrame;
use core::marker::PhantomData;
use std::convert::TryFrom;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
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

const REG_MASKS: [u8;16] = [
    0xff, 0x0f, 0xff, 0x0f, 0xff, 0x0f, 0x1f, 0xff,
    0x1f, 0x1f, 0x1f, 0xff, 0xff, 0x0f, 0xff, 0xff
];

/// Timestamps a change to one of AY registers.
///
/// Instances of this type are being used by [Ay3_891xAudio][crate::audio::ay::Ay3_891xAudio]
/// for sound generation. See also [AyRegRecorder].
#[derive(Clone, Copy, Debug)]
pub struct AyRegChange {
    /// A timestamp in Cpu cycle (T-states) counter values, relative to the current frame.
    pub time: FTs,
    /// Which register is being changed.
    pub reg: AyRegister,
    /// New value loaded into the register.
    ///
    /// *NOTE*: may contain bits set to 1 that are unused by the indicated register.
    pub val: u8
}

/// This trait should be implemented by devices attached to [Ay3_891xIo] A/B ports.
pub trait AyIoPort {
    type Timestamp;
    /// Resets the device at the given `timestamp`.
    #[inline]
    fn ay_io_reset(&mut self, _timestamp: Self::Timestamp) {}
    /// Writes value to the device at the given `timestamp`.
    #[inline]
    fn ay_io_write(&mut self, _val: u8, _timestamp: Self::Timestamp) {}
    /// Reads value to the device at the given `timestamp`.
    #[inline]
    fn ay_io_read(&mut self, _timestamp: Self::Timestamp) -> u8 { 0xff }
    /// Signals the device when the current frame ends.
    #[inline]
    fn end_frame(&mut self, _timestamp: Self::Timestamp) {}
}

/// An [Ay3_891xIo] port device implementation that does nothing.
#[derive(Default, Clone, Copy, Debug)]
pub struct AyIoNullPort<T>(PhantomData<T>);

/// Allows recording of changes to AY registers with timestamps.
pub trait AyRegRecorder {
    type Timestamp;
    /// Records the new value of indicated register with the given `timestamp`.
    ///
    /// *NOTE*: It is up to the caller to ensure the timestamps are added in an ascending order.
    fn record_ay_reg_change(&mut self, reg: AyRegister, val: u8, timestamp: Self::Timestamp);
}

/// The type of [Ay3_891xIo] with two I/O ports (A and B) and [AyRegVecRecorder].
pub type Ay3_8910Io<T,A,B> = Ay3_891xIo<T, AyRegVecRecorder<T>, A, B>;
/// The type of [Ay3_891xIo] with one I/O port (A only) and [AyRegVecRecorder].
pub type Ay3_8912Io<T,A> = Ay3_891xIo<T, AyRegVecRecorder<T>, A, AyIoNullPort<T>>;
/// The type of [Ay3_891xIo] without I/O ports and [AyRegVecRecorder].
pub type Ay3_8913Io<T> = Ay3_891xIo<T, AyRegVecRecorder<T>, AyIoNullPort<T>, AyIoNullPort<T>>;

/// Implements AY-3-8910/8912/8913 I/O ports and provides an interface for I/O operations.
///
/// For the implementation of a sound generator see [crate::audio::ay].
#[derive(Default, Clone, Debug)]
pub struct Ay3_891xIo<T,R,A,B> {
    /// The recorded changes can be used to generate sound with [crate::audio::ay::Ay3_891xAudio].
    pub recorder: R,
    /// Accesses port A device.
    pub port_a: A,
    /// Accesses port B device.
    pub port_b: B,
    regs: [u8; 16],
    selected_reg: AyRegister,
    _ts: PhantomData<T>
}

/// A convenient recorder for [Ay3_891xIo] that records nothing.
///
/// Usefull when no sound will be generated.
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
    /// Resets the state of internal registers, port devices and the selected register.
    ///
    /// *NOTE*: does nothing to the recorded register changes.
    pub fn reset(&mut self, timestamp: T) where T: Copy {
        self.regs = Default::default();
        self.selected_reg = Default::default();
        self.port_a.ay_io_reset(timestamp);
        self.port_b.ay_io_reset(timestamp);
    }
    /// Retrieves a current value of the indicated register.
    #[inline]
    pub fn get(&self, reg: AyRegister) -> u8 {
        let index = usize::from(reg);
        self.regs[index]
    }
    /// Sets a current value of the indicated register.
    #[inline]
    pub fn set(&mut self, reg: AyRegister, val: u8) {
        let index = usize::from(reg);
        self.regs[index] = val & REG_MASKS[index];
    }
    /// Returns true if the control register bit controlling I/O port A input is reset.
    #[inline]
    pub fn is_ioa_input(&self) -> bool {
        self.get(AyRegister::MixerControl) & 0x80 == 0
    }
    /// Returns true if the control register bit controlling I/O port B input is reset.
    #[inline]
    pub fn is_iob_input(&self) -> bool {
        self.get(AyRegister::MixerControl) & 0x40 == 0
    }
    /// Returns true if the control register bit controlling I/O port A input is set.
    #[inline]
    pub fn is_ioa_output(&self) -> bool {
        !self.is_ioa_input()
    }
    /// Returns true if the control register bit controlling I/O port B input is reset.
    #[inline]
    pub fn is_iob_output(&self) -> bool {
        !self.is_iob_input()
    }
    /// Returns a previously selected register with [Ay3_891xIo::select_port].
    #[inline]
    pub fn selected_port(&self) -> AyRegister {
        self.selected_reg
    }
    /// Bits 0-3 of `data` selects a register to be read from or written to.
    ///
    /// This method is being used to interface the host controller I/O operation.
    #[inline]
    pub fn select_port(&mut self, data: u8) {
        self.selected_reg = AyRegister::from(data & 0x0F)
    }
    /// Writes data to a previously selected register and records a change
    /// in the attached recorder unless a write is performed to one of the I/O ports.
    ///
    /// This method is being used to interface the host controller I/O operation.
    #[inline]
    pub fn data_port_write(&mut self, data: u8, timestamp: T) {
        self.set(self.selected_reg, data); // What to do when control is set to output for IO?
        match self.selected_reg {
            AyRegister::IoA => {
                self.port_a.ay_io_write(data, timestamp)
            }
            AyRegister::IoB => {
                self.port_b.ay_io_write(data, timestamp)
            }
            reg => self.recorder.record_ay_reg_change(reg, data, timestamp)
        }
    }
    /// Reads data from a previously selected register.
    ///
    /// This method is being used to interface the host controller I/O operation.
    #[inline]
    pub fn data_port_read(&mut self, timestamp: T) -> u8 {
        match self.selected_reg {
            AyRegister::IoA => {
                let port_input = self.port_a.ay_io_read(timestamp);
                if self.is_ioa_input() {
                    port_input
                }
                else {
                    port_input & self.get(AyRegister::IoA)
                }
            }
            AyRegister::IoB => {
                let port_input = self.port_b.ay_io_read(timestamp);
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
    pub fn new_from_vts<V: VideoFrame>(timestamp: VideoTs, reg: AyRegister, val: u8) -> Self {
        let time = VFrameTsCounter::<V>::from(timestamp).as_tstates();
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
}

impl<T> AyRegRecorder for AyRegVecRecorder<T> {
    type Timestamp = T;
    #[inline]
    fn record_ay_reg_change(&mut self, reg: AyRegister, val: u8, timestamp: T) {
        self.0.push((timestamp, reg, val));
    }
}

impl AyRegVecRecorder<FTs> {
    /// Constructs a draining iterator of [AyRegChange] items from an inner [Vec].
    pub fn drain_ay_reg_changes<'a>(&'a mut self) -> impl Iterator<Item=AyRegChange> + 'a {
        self.0.drain(..).map(|(timestamp,reg,val)| AyRegChange::new(timestamp,reg,val))
    }
}

impl AyRegVecRecorder<VideoTs> {
    /// Constructs a draining iterator of [AyRegChange] items from an inner [Vec].
    pub fn drain_ay_reg_changes<'a, V: VideoFrame>(&'a mut self) -> impl Iterator<Item=AyRegChange> + 'a {
        self.0.drain(..).map(|(timestamp,reg,val)| AyRegChange::new_from_vts::<V>(timestamp,reg,val))
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
