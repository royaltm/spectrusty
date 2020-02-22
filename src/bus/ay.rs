//! Implementation of [BusDevice] for `AY-3-8910/2/3`.
use core::ops::{Deref, DerefMut};
use core::marker::PhantomData;

use crate::clock::{VFrameTsCounter, VideoTs, FTs};
use crate::bus::{BusDevice, NullDevice, OptionalBusDevice};
use crate::peripherals::ay::{Ay3_8910Io, AyPortDecode, AyIoPort, AyIoNullPort, Ay128kPortDecode, AyFullerBoxPortDecode};
use crate::chip::ula::{UlaTsCounter, Ula};
use crate::audio::ay::Ay3_891xAudio;
use crate::audio::{Blep, AmpLevels};
use crate::audio::sample::SampleDelta;
use crate::memory::ZxMemory;
use crate::video::VideoFrame;

/// Implement this empty trait for BusDevice so methods from AyAudioVBusDevice
/// will get auto implemented to pass method call to next devices.
pub trait PassByAyAudioBusDevice {}

/// A convenient [Ay3_891xBusDevice] type emulating a device with a `Melodik` port configuration.
pub type Ay3_891xMelodik<D=NullDevice<VideoTs>,
                         A=AyIoNullPort<VideoTs>,
                         B=AyIoNullPort<VideoTs>> = Ay3_891xBusDevice<
                                                        VideoTs,
                                                        Ay128kPortDecode,
                                                        A, B, D>;
/// A convenient [Ay3_891xBusDevice] type emulating a device with a `Fuller Box` port configuration.
pub type Ay3_891xFullerBox<D=NullDevice<VideoTs>,
                           A=AyIoNullPort<VideoTs>,
                           B=AyIoNullPort<VideoTs>> = Ay3_891xBusDevice<
                                                        VideoTs,
                                                        AyFullerBoxPortDecode,
                                                        A, B, D>;

pub trait AyAudioVBusDevice<B: Blep> {
    fn render_ay_audio_vts<L: AmpLevels<B::SampleDelta>,
                           V: VideoFrame>(&mut self, blep: &mut B, end_ts: VideoTs, chans: [usize; 3]);
}

/// Implements [Ay3_891xAudio] and [Ay3_8910Io] as a [BusDevice].
#[derive(Clone, Default)]
pub struct Ay3_891xBusDevice<T, P,
                             A=AyIoNullPort<T>,
                             B=AyIoNullPort<T>,
                             D=NullDevice<T>>
{
    pub ay_sound: Ay3_891xAudio,
    pub ay_io: Ay3_8910Io<T, A, B>,
        bus: D,
        _port_decode: PhantomData<P>
}

impl<E, D, N> AyAudioVBusDevice<E> for D
    where E: Blep,
          D: BusDevice<Timestamp=VideoTs, NextDevice=N> + PassByAyAudioBusDevice,
          N: BusDevice<Timestamp=VideoTs> + AyAudioVBusDevice<E>
{
    #[inline(always)]
    fn render_ay_audio_vts<S, V>(&mut self, blep: &mut E, end_ts: VideoTs, chans: [usize; 3])
        where S: AmpLevels<E::SampleDelta>,
              V: VideoFrame
    {
        <Self as BusDevice>::next_device_mut(self).render_ay_audio_vts::<S, V>(blep, end_ts, chans)
    }
}

impl<E, D, N> AyAudioVBusDevice<E> for OptionalBusDevice<D, N>
    where E: Blep,
          D: AyAudioVBusDevice<E>,
          N: AyAudioVBusDevice<E>
{
    /// # Note
    /// If a device is being attached to an optional device the call will be forwarded to
    /// both: an optional device and to the next bus device.
    #[inline(always)]
    fn render_ay_audio_vts<S, V>(&mut self, blep: &mut E, end_ts: VideoTs, chans: [usize; 3])
        where S: AmpLevels<E::SampleDelta>,
              V: VideoFrame
    {
        if let Some(ref mut device) = self.device {
          device.render_ay_audio_vts::<S, V>(blep, end_ts, chans)
        }
        self.next_device.render_ay_audio_vts::<S, V>(blep, end_ts, chans)
    }
}

impl<E: Blep, P, A, B, N> AyAudioVBusDevice<E> for Ay3_891xBusDevice<VideoTs, P, A, B, N> {
    #[inline(always)]
    fn render_ay_audio_vts<S, V>(&mut self, blep: &mut E, end_ts: VideoTs, chans: [usize; 3])
        where S: AmpLevels<E::SampleDelta>,
              V: VideoFrame
    {
        self.render_ay_audio::<S,V,E>(blep, end_ts, chans)
    }
}

impl<E: Blep> AyAudioVBusDevice<E> for NullDevice<VideoTs> {
    #[inline(always)]
    fn render_ay_audio_vts<S, V>(&mut self, _blep: &mut E, _end_ts: VideoTs, _chans: [usize; 3])
        where S: AmpLevels<E::SampleDelta>,
              V: VideoFrame
    {}
}

impl<T, P, A, B, D> BusDevice for Ay3_891xBusDevice<T, P, A, B, D>
    where P: AyPortDecode,
          A: AyIoPort<Timestamp=T>,
          B: AyIoPort<Timestamp=T>,
          T: Copy,
          D: BusDevice<Timestamp=T>
{
    type Timestamp = T;
    type NextDevice = D;

    #[inline]
    fn next_device_mut(&mut self) -> &mut Self::NextDevice {
        &mut self.bus
    }

    #[inline]
    fn next_device_ref(&self) -> &Self::NextDevice {
        &self.bus
    }

    #[inline]
    fn reset(&mut self, timestamp: Self::Timestamp) {
        self.ay_sound.reset();
        self.ay_io.reset(timestamp);
        self.bus.reset(timestamp);
    }

    #[inline]
    fn read_io(&mut self, port: u16, timestamp: Self::Timestamp) -> Option<u8> {
        if P::is_data_read(port) {
            return Some(self.ay_io.data_port_read(port, timestamp))
        }
        self.bus.read_io(port, timestamp)
    }

    #[inline]
    fn write_io(&mut self, port: u16, data: u8, timestamp: Self::Timestamp) -> bool {
        if P::write_ay_io(&mut self.ay_io, port, data, timestamp) {
            return true    
        }
        self.bus.write_io(port, data, timestamp)
    }
}

impl<P, A, B, D> Ay3_891xBusDevice<VideoTs, P, A, B, D> {
    /// Renders square-wave pulses via [Blep] interface.
    ///
    /// Provide [AmpLevels] that can handle `level` values from 0 to 15 (4-bits).
    /// `channels` - target [Blep] audio channels for `[A, B, C]` AY-3-891x channels.
    pub fn render_ay_audio<S,V,E>(&mut self, blep: &mut E, end_ts: VideoTs, chans: [usize; 3])
        where S: AmpLevels<E::SampleDelta>,
              V: VideoFrame,
              E: Blep
    {
        let end_ts = VFrameTsCounter::<V>::from(end_ts).as_tstates();
        let changes = self.ay_io.recorder.drain_ay_reg_changes::<V>();
        self.ay_sound.render_audio::<S,_,E>(changes, blep, end_ts, chans)
    }

}

impl<P, A, B, D> Ay3_891xBusDevice<FTs, P, A, B, D> {
    /// Renders square-wave pulses via [Blep] interface.
    ///
    /// Provide [AmpLevels] that can handle `level` values from 0 to 15 (4-bits).
    /// `channels` - target [Blep] audio channels for `[A, B, C]` AY-3-891x channels.
    pub fn render_ay_audio<S,E>(&mut self, blep: &mut E, end_ts: FTs, chans: [usize; 3])
        where S: AmpLevels<E::SampleDelta>,
              E: Blep
    {
        let changes = self.ay_io.recorder.drain_ay_reg_changes();
        self.ay_sound.render_audio::<S,_,E>(changes, blep, end_ts, chans)
    }

}
