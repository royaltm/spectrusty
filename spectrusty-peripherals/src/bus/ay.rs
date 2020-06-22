//! `AY-3-8910` programmable sound generator.
use core::fmt::{self, Debug};
use core::num::NonZeroU16;
use core::marker::PhantomData;

pub mod serial128;
#[cfg(feature = "snapshot")] mod serde;
#[cfg(feature = "snapshot")] use ::serde::Serialize;

use spectrusty_core::{
    audio::{Blep, AmpLevels},
    bus::{
        BusDevice, NullDevice,
        OptionalBusDevice, DynamicBus, DynamicSerdeBus, NamedBusDevice
    },
    clock::FTs
};

pub use crate::ay::{
    audio::Ay3_891xAudio,
    Ay3_8910Io, Ay3_8912Io, Ay3_8913Io, AyIoPort, AyIoNullPort, AyRegister,
    AyPortDecode, Ay128kPortDecode, AyFullerBoxPortDecode
};

/// Implement this empty trait for [BusDevice] so methods from [AyAudioBusDevice]
/// will get auto implemented to pass method call to next devices.
pub trait PassByAyAudioBusDevice {}

/// A convenient [Ay3_891xBusDevice] type emulating a device with a `Melodik` port configuration.
pub type Ay3_891xMelodik<D> = Ay3_891xBusDevice<Ay128kPortDecode,
                                                AyIoNullPort<<D as BusDevice>::Timestamp>,
                                                AyIoNullPort<<D as BusDevice>::Timestamp>,
                                                D>;
/// A convenient [Ay3_891xBusDevice] type emulating a device with a `Fuller Box` port configuration.
pub type Ay3_891xFullerBox<D> = Ay3_891xBusDevice<AyFullerBoxPortDecode,
                                                AyIoNullPort<<D as BusDevice>::Timestamp>,
                                                AyIoNullPort<<D as BusDevice>::Timestamp>,
                                                D>;

impl<D: BusDevice> fmt::Display for Ay3_891xMelodik<D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AY-3-8913 (Melodik)")
    }
}

impl<D: BusDevice> fmt::Display for Ay3_891xFullerBox<D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AY-3-8913 (Fuller Box)")
    }
}
/// This trait is being used by [AyAudioFrame] implementations to render `AY-3-8910` audio with bus devices.
///
/// Allows for rendering audio frame using [AyAudioFrame] directly on the [ControlUnit] without the
/// need to "locate" the [Ay3_891xBusDevice] in the daisy chain.
///
/// This trait is implemented autmatically for all [BusDevice]s which implement [PassByAyAudioBusDevice].
///
/// [AyAudioFrame]: crate::ay::audio::AyAudioFrame
/// [ControlUnit]: spectrusty_core::chip::ControlUnit
pub trait AyAudioBusDevice: BusDevice {
    /// Renders square-wave pulses via [Blep] interface.
    ///
    /// Provide [AmpLevels] that can handle `level` values from 0 to 15 (4-bits).
    /// `channels` - target [Blep] audio channels for `[A, B, C]` AY-3-8910 channels.
    fn render_ay_audio<L: AmpLevels<B::SampleDelta>, B: Blep>(
            &mut self,
            blep: &mut B,
            end_ts: <Self as BusDevice>::Timestamp,
            frame_tstates: FTs,
            chans: [usize; 3]
        );
}

/// `AY-3-8910/8912/8913` programmable sound generator as a [BusDevice].
///
/// Envelops [Ay3_891xAudio] sound generator and [Ay3_8910Io] I/O ports peripherals.
///
/// Provides a helper method to produce sound generated by the last emulated frame.
#[derive(Clone, Default, Debug)]
#[cfg_attr(feature = "snapshot", derive(Serialize))]
#[cfg_attr(feature = "snapshot", serde(rename_all = "camelCase"))]
pub struct Ay3_891xBusDevice<P, A, B, D: BusDevice> {
    /// Provides a direct access to the sound generator.
    pub ay_sound: Ay3_891xAudio,
    /// Provides a direct access to the I/O ports.
    pub ay_io: Ay3_8910Io<D::Timestamp, A, B>,
        bus: D,
        #[cfg_attr(feature = "snapshot", serde(skip))]
        _port_decode: PhantomData<P>
}

impl<D, N> AyAudioBusDevice for D
    where D::Timestamp: Into<FTs>,
          D: BusDevice<NextDevice=N> + PassByAyAudioBusDevice,
          N: BusDevice<Timestamp=D::Timestamp> + AyAudioBusDevice
{
    fn render_ay_audio<L, B>(&mut self, blep: &mut B, end_ts: D::Timestamp, frame_tstates: FTs, chans: [usize; 3])
        where B: Blep,
              L: AmpLevels<B::SampleDelta>
    {
        self.next_device_mut().render_ay_audio::<L, B>(blep, end_ts, frame_tstates, chans)
    }
}

impl<D, N> AyAudioBusDevice for OptionalBusDevice<D, N>
    where <D as BusDevice>::Timestamp: Into<FTs> + Copy,
          D: AyAudioBusDevice + BusDevice,
          N: AyAudioBusDevice + BusDevice<Timestamp=D::Timestamp>
{
    /// # Note
    /// If a device is being attached to an optional device the call will be forwarded to
    /// both: an optional device and to the next bus device.
    #[inline(always)]
    fn render_ay_audio<L, B>(&mut self, blep: &mut B, end_ts: D::Timestamp, frame_tstates: FTs, chans: [usize; 3])
        where B: Blep,
              L: AmpLevels<B::SampleDelta>
    {
        if let Some(ref mut device) = self.device {
            device.render_ay_audio::<L, B>(blep, end_ts, frame_tstates, chans)
        }
        self.next_device.render_ay_audio::<L, B>(blep, end_ts, frame_tstates, chans)
    }
}

impl<T> AyAudioBusDevice for dyn NamedBusDevice<T>
    where T: Into<FTs> + Copy + fmt::Debug + 'static
{
    /// # Note
    /// Because we need to guess the concrete type of the dynamic `BusDevice` we can currently handle
    /// only the most common cases: [Ay3_891xMelodik] and [Ay3_891xFullerBox]. If you use a customized
    /// [Ay3_891xBusDevice] for a dynamic `BusDevice` you need to render audio directly on the device
    /// downcasted to your custom type.
    #[inline]
    fn render_ay_audio<L, B>(&mut self, blep: &mut B, end_ts: T, frame_tstates: FTs, chans: [usize; 3])
        where L: AmpLevels<B::SampleDelta>,
              B: Blep
    {
        if let Some(ay_dev) = self.downcast_mut::<Ay3_891xMelodik<NullDevice<T>>>() {
            ay_dev.render_ay_audio::<L, B>(blep, end_ts, frame_tstates, chans)
        }
        else if let Some(ay_dev) = self.downcast_mut::<Ay3_891xFullerBox<NullDevice<T>>>() {
            ay_dev.render_ay_audio::<L, B>(blep, end_ts, frame_tstates, chans)
        }
    }
}

impl<D> AyAudioBusDevice for DynamicBus<D>
    where <D as BusDevice>::Timestamp: Into<FTs> + Copy + fmt::Debug + 'static,
          D: AyAudioBusDevice + BusDevice
{
    /// # Note
    /// This implementation forwards a call to any recognizable [Ay3_891xBusDevice] device in a
    /// dynamic daisy-chain as well as to an upstream device.
    #[inline]
    fn render_ay_audio<L, B>(&mut self, blep: &mut B, end_ts: D::Timestamp, frame_tstates: FTs, chans: [usize; 3])
        where B: Blep,
              L: AmpLevels<B::SampleDelta>
    {
        for dev in self.into_iter() {
            dev.render_ay_audio::<L, B>(blep, end_ts, frame_tstates, chans)
        }
        self.next_device_mut().render_ay_audio::<L, B>(blep, end_ts, frame_tstates, chans)
    }
}

impl<SD, D> AyAudioBusDevice for DynamicSerdeBus<SD, D>
    where <D as BusDevice>::Timestamp: Into<FTs> + Copy + fmt::Debug + 'static,
          D: AyAudioBusDevice + BusDevice
{
    /// # Note
    /// This implementation forwards a call to any recognizable [Ay3_891xBusDevice] device in a
    /// dynamic daisy-chain as well as to an upstream device.
    #[inline]
    fn render_ay_audio<L, B>(&mut self, blep: &mut B, end_ts: D::Timestamp, frame_tstates: FTs, chans: [usize; 3])
        where B: Blep,
              L: AmpLevels<B::SampleDelta>
    {
        (&mut **self).render_ay_audio::<L, B>(blep, end_ts, frame_tstates, chans)
    }
}

impl<P, A, B, D> AyAudioBusDevice for Ay3_891xBusDevice<P, A, B, D>
    where <Self as BusDevice>::Timestamp: Into<FTs>,
          Self: BusDevice<Timestamp=D::Timestamp>,
          D: BusDevice
{
    #[inline(always)]
    fn render_ay_audio<L, E>(&mut self, blep: &mut E, end_ts: <Self as BusDevice>::Timestamp, frame_tstates: FTs, chans: [usize; 3])
        where E: Blep,
              L: AmpLevels<E::SampleDelta>
    {
        let end_ts = end_ts.into();
        let changes = self.ay_io.recorder.drain_ay_reg_changes();
        self.ay_sound.render_audio::<L,_,_>(changes, blep, end_ts, frame_tstates, chans)
    }
}

impl<T: Into<FTs> + fmt::Debug> AyAudioBusDevice for NullDevice<T> {
    #[inline(always)]
    fn render_ay_audio<L, B>(&mut self, _blep: &mut B, _end_ts: T, _frame_tstates: FTs, _chans: [usize; 3])
        where L: AmpLevels<B::SampleDelta>,
              B: Blep
    {}
}

impl<P, A, B, D> BusDevice for Ay3_891xBusDevice<P, A, B, D>
    where P: AyPortDecode,
          A: AyIoPort<Timestamp=D::Timestamp>,
          B: AyIoPort<Timestamp=D::Timestamp>,
          D: BusDevice,
          D::Timestamp: Debug + Copy
{
    type Timestamp = D::Timestamp;
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
    fn into_next_device(self) -> Self::NextDevice {
        self.bus
    }

    #[inline]
    fn reset(&mut self, timestamp: Self::Timestamp) {
        self.ay_sound.reset();
        self.ay_io.reset(timestamp);
        self.bus.reset(timestamp);
    }

    #[inline]
    fn read_io(&mut self, port: u16, timestamp: Self::Timestamp) -> Option<(u8, Option<NonZeroU16>)> {
        if P::is_data_read(port) {
            return Some((self.ay_io.data_port_read(port, timestamp), None))
        }
        self.bus.read_io(port, timestamp)
    }

    #[inline]
    fn write_io(&mut self, port: u16, data: u8, timestamp: Self::Timestamp) -> Option<u16> {
        if P::write_ay_io(&mut self.ay_io, port, data, timestamp) {
            return Some(0)
        }
        self.bus.write_io(port, data, timestamp)
    }

    #[inline]
    fn next_frame(&mut self, timestamp: Self::Timestamp) {
        // ensure sound register state is equal to the i/o state only if there are some unused register changes
        // in the recorder, meaning the audio wasn't rendered for the past frame;
        // this is not ideal, because when changes are applied in time they may affect the state of the sound
        // generator in a slightly different way in comparison to just setting their value to the current state;
        // it is better still than to just leave them completely unsynchronized
        if !self.ay_io.recorder.is_empty() {
            for (reg, val) in self.ay_io.iter_sound_gen_regs() {
                self.ay_sound.update_register(reg, val);
            }
        }
        self.ay_io.next_frame(timestamp);
        self.bus.next_frame(timestamp)
    }
}
