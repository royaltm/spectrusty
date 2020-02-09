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

pub struct AyRegConvertError; // TODO: impl

pub const REG_MASKS: [u8;16] = [
    0xff, 0x0f, 0xff, 0x0f, 0xff, 0x0f, 0x1f, 0xff,
    0x1f, 0x1f, 0x1f, 0xff, 0xff, 0x0f, 0xff, 0xff
];

/// Used by Ay3_891xAudio
#[derive(Clone, Copy, Debug)]
pub struct AyRegChange {
    pub time: FTs,
    pub reg: AyRegister,
    pub val: u8
}

pub trait AyIoPort {
    type Timestamp;
    fn ay_io_reset(&mut self, timestamp: Self::Timestamp);
    fn ay_io_write(&mut self, val: u8, timestamp: Self::Timestamp);
    fn ay_io_read(&mut self, timestamp: Self::Timestamp) -> u8;
}

#[derive(Default, Clone, Copy, Debug)]
pub struct AyIoNullPort<T>(PhantomData<T>);

pub trait AyRegRecorder {
    type Timestamp;
    fn record_ay_reg_change(&mut self, reg: AyRegister, val: u8, timestamp: Self::Timestamp);
}

pub type Ay3_891N<T> = Ay3_891xIo<T, AyRegVecRecorder<T>, AyIoNullPort<T>, AyIoNullPort<T>>;
pub type Ay3_8910<T,A,B> = Ay3_891xIo<T, AyRegVecRecorder<T>, A, B>;
pub type Ay3_8912<T,A> = Ay3_891xIo<T, AyRegVecRecorder<T>, A, AyIoNullPort<T>>;

#[derive(Default, Clone, Debug)]
pub struct Ay3_891xIo<T,R,A,B> {
    pub recorder: R,
    pub port_a: A,
    pub port_b: B,
    regs: [u8; 16],
    selected_reg: AyRegister,
    _ts: PhantomData<T>
}

#[derive(Default, Clone, Debug)]
pub struct AyRegVecRecorder<T>(pub Vec<(T,AyRegister,u8)>);

impl<T,R,A,B> Ay3_891xIo<T,R,A,B>
where A: AyIoPort<Timestamp=T>,
      B: AyIoPort<Timestamp=T>,
      R: AyRegRecorder<Timestamp=T>
{
    pub fn reset(&mut self, timestamp: T) where T: Copy {
        self.regs = Default::default();
        self.selected_reg = Default::default();
        self.port_a.ay_io_reset(timestamp);
        self.port_b.ay_io_reset(timestamp);
    }
    #[inline]
    pub fn get(&self, reg: AyRegister) -> u8 {
        let index = usize::from(reg);
        self.regs[index]
    }

    #[inline]
    pub fn set(&mut self, reg: AyRegister, val: u8) {
        let index = usize::from(reg);
        self.regs[index] = val & REG_MASKS[index];
    }

    #[inline]
    pub fn is_ioa_input(&self) -> bool {
        self.get(AyRegister::MixerControl) & 0x80 == 0
    }

    #[inline]
    pub fn is_iob_input(&self) -> bool {
        self.get(AyRegister::MixerControl) & 0x40 == 0
    }

    #[inline]
    pub fn is_ioa_output(&self) -> bool {
        !self.is_ioa_input()
    }

    #[inline]
    pub fn is_iob_output(&self) -> bool {
        !self.is_iob_input()
    }

    #[inline]
    pub fn select_port(&mut self, val: u8) {
        if let Ok(reg) = AyRegister::try_from(val) {
            self.selected_reg = reg;
        }
    }

    #[inline]
    pub fn data_port_write(&mut self, val: u8, timestamp: T) {
        self.set(self.selected_reg, val); // What to do when control is set to output for IO?
        match self.selected_reg {
            AyRegister::IoA => {
                self.port_a.ay_io_write(val, timestamp)
            }
            AyRegister::IoB => {
                self.port_b.ay_io_write(val, timestamp)
            }
            reg => self.recorder.record_ay_reg_change(reg, val, timestamp)
        }
    }

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

impl TryFrom<u8> for AyRegister {
    type Error = AyRegConvertError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if value & 0x0F == value {
            Ok(unsafe { core::mem::transmute(value) })
        }
        else {
            Err(AyRegConvertError)
        }
    }
}

macro_rules! impl_from_ay_reg {
    ($($ty:ty),*) => { $(
        impl From<AyRegister> for $ty {
            fn from(reg: AyRegister) -> $ty {
                reg as $ty
            }
        }        
    )* };
}
impl_from_ay_reg!(u8, u16, u32, u64, usize);

impl AyRegChange {
    #[inline]
    pub const fn new(time: FTs, reg: AyRegister, val: u8) -> Self {
        AyRegChange { time, reg, val }
    }

    pub fn new_from_vts<V: VideoFrame>(vt: VideoTs, reg: AyRegister, val: u8) -> Self {
        let time = VFrameTsCounter::<V>::from(vt).as_tstates();
        Self::new(time, reg, val)
    }
}

impl<T> AyIoPort for AyIoNullPort<T> {
    type Timestamp = T;
    #[inline]
    fn ay_io_reset(&mut self, _timestamp: T) {}
    #[inline]
    fn ay_io_write(&mut self, _val: u8, _timestamp: T) {}
    #[inline]
    fn ay_io_read(&mut self, _timestamp: T) -> u8 { 0xff }
}

impl<T> AyRegRecorder for AyRegVecRecorder<T> {
    type Timestamp = T;
    #[inline]
    fn record_ay_reg_change(&mut self, reg: AyRegister, val: u8, timestamp: T) {
        self.0.push((timestamp, reg, val));
    }
}

impl AyRegVecRecorder<FTs> {
    pub fn drain_ay_reg_changes<'a>(&'a mut self) -> impl Iterator<Item=AyRegChange> + 'a {
        self.0.drain(..).map(|(timestamp,reg,val)| AyRegChange::new(timestamp,reg,val))
    }
}

impl AyRegVecRecorder<VideoTs> {
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
