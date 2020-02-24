//! Implementation of [BusDevice] for `AY-3-8910/2/3`.
use core::fmt::{self, Debug};
use core::ops::{Deref, DerefMut};
use core::marker::PhantomData;

use crate::clock::{VFrameTsCounter, VideoTs, FTs};
use crate::bus::{BusDevice, NullDevice, OptionalBusDevice, DynamicBusDevice, DynamicDevice};
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

impl<D> fmt::Display for Ay3_891xMelodik<D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AY-3-8913 (Melodik)")
    }
}

impl<D> fmt::Display for Ay3_891xFullerBox<D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AY-3-8913 (Fuller Box)")
    }
}
/// This trait is being used by [AyAudioFrame][crate::audio::ay::AyAudioFrame] implementations to render
/// `AY-3-891x` audio with bus devices.
///
/// This trait is implemented autmatically for all [BusDevice]s which implement [PassByAyAudioBusDevice].
pub trait AyAudioVBusDevice {
    fn render_ay_audio_vts<L: AmpLevels<B::SampleDelta>,
                           V: VideoFrame,
                           B: Blep>(&mut self, blep: &mut B, end_ts: VideoTs, chans: [usize; 3]);
}

/// Implements [Ay3_891xAudio] and [Ay3_8910Io] as a [BusDevice].
#[derive(Clone, Default, Debug)]
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

impl<D, N> AyAudioVBusDevice for D
    where D: BusDevice<Timestamp=VideoTs, NextDevice=N> + PassByAyAudioBusDevice,
          N: BusDevice<Timestamp=VideoTs> + AyAudioVBusDevice
{
    #[inline(always)]
    fn render_ay_audio_vts<S, V, E>(&mut self, blep: &mut E, end_ts: VideoTs, chans: [usize; 3])
        where S: AmpLevels<E::SampleDelta>,
              V: VideoFrame,
              E: Blep
    {
        self.next_device_mut().render_ay_audio_vts::<S, V, E>(blep, end_ts, chans)
    }
}

impl<D, N> AyAudioVBusDevice for OptionalBusDevice<D, N>
    where D: AyAudioVBusDevice,
          N: AyAudioVBusDevice
{
    /// # Note
    /// If a device is being attached to an optional device the call will be forwarded to
    /// both: an optional device and to the next bus device.
    #[inline(always)]
    fn render_ay_audio_vts<S, V, E>(&mut self, blep: &mut E, end_ts: VideoTs, chans: [usize; 3])
        where S: AmpLevels<E::SampleDelta>,
              V: VideoFrame,
              E: Blep
    {
        if let Some(ref mut device) = self.device {
            device.render_ay_audio_vts::<S, V, E>(blep, end_ts, chans)
        }
        self.next_device.render_ay_audio_vts::<S, V, E>(blep, end_ts, chans)
    }
}

impl AyAudioVBusDevice for DynamicDevice<VideoTs> {
    /// # Note
    /// Because we need to guess the conrete type of the dynamic `BusDevice` we can currently handle
    /// ony the most common cases: [Ay3_891xMelodik] and [Ay3_891xFullerBox]. If you use a customized
    /// [Ay3_891xBusDevice] for a dynamic `BusDevice` you need to render audio directly on the device
    /// downcasted to your custom type.
    #[inline]
    fn render_ay_audio_vts<S, V, E>(&mut self, blep: &mut E, end_ts: VideoTs, chans: [usize; 3])
        where S: AmpLevels<E::SampleDelta>,
              V: VideoFrame,
              E: Blep
    {
        if let Some(ay_dev) = self.downcast_mut::<Ay3_891xMelodik>() {
            ay_dev.render_ay_audio::<S, V, E>(blep, end_ts, chans)
        }
        else if let Some(ay_dev) = self.downcast_mut::<Ay3_891xFullerBox>() {
            ay_dev.render_ay_audio::<S, V, E>(blep, end_ts, chans)
        }
    }
}

impl<D> AyAudioVBusDevice for DynamicBusDevice<D>
    where D: AyAudioVBusDevice + BusDevice<Timestamp=VideoTs>
{
    /// # Note
    /// This implementation forwards a call to any recognizable [Ay3_891xBusDevice] device in a
    /// dynamic daisy-chain as well as to an upstream device.
    #[inline]
    fn render_ay_audio_vts<S, V, E>(&mut self, blep: &mut E, end_ts: VideoTs, chans: [usize; 3])
        where S: AmpLevels<E::SampleDelta>,
              V: VideoFrame,
              E: Blep
    {
        for dev in self.iter_mut() {
            dev.render_ay_audio_vts::<S, V, E>(blep, end_ts, chans)
        }
        self.next_device_mut().render_ay_audio_vts::<S, V, E>(blep, end_ts, chans)
    }
}

impl<P, A, B, N> AyAudioVBusDevice for Ay3_891xBusDevice<VideoTs, P, A, B, N> {
    #[inline(always)]
    fn render_ay_audio_vts<S, V, E>(&mut self, blep: &mut E, end_ts: VideoTs, chans: [usize; 3])
        where S: AmpLevels<E::SampleDelta>,
              V: VideoFrame,
              E: Blep
    {
        self.render_ay_audio::<S, V, E>(blep, end_ts, chans)
    }
}

impl AyAudioVBusDevice for NullDevice<VideoTs> {
    #[inline(always)]
    fn render_ay_audio_vts<S, V, E>(&mut self, _blep: &mut E, _end_ts: VideoTs, _chans: [usize; 3])
        where S: AmpLevels<E::SampleDelta>,
              V: VideoFrame,
              E: Blep
    {}
}

impl<T, P, A, B, D> BusDevice for Ay3_891xBusDevice<T, P, A, B, D>
    where P: AyPortDecode,
          A: AyIoPort<Timestamp=T>,
          B: AyIoPort<Timestamp=T>,
          T: Debug + Copy,
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
