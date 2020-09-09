use core::fmt;
use std::collections::hash_map::Entry;
use std::io;

use spectrusty::z80emu::Cpu;
use spectrusty::clock::VFrameTs;
use spectrusty::bus::{
    BusDevice, OptionalBusDevice, DynamicVBus, DynamicSerdeVBus,
    NamedBusDevice, BoxNamedDynDevice, VFNullDevice,
    ay::{
        Ay3_891xAudio, Ay3_8912Io,
        serial128::{
            SerialKeypad, NullSerialPort,
            Rs232Io, SerialPorts128
        }
    },
    joystick::MultiJoystickBusDevice,
};
use spectrusty::chip::{
    ControlUnit,
    ula::Ula,
    ula128::Ula128VidFrame,
    ula3::Ula3VidFrame,
    scld::Scld
};
use spectrusty::memory::{PagedMemory8k, ZxMemory, MemoryExtension};
use spectrusty::video::{Video, VideoFrame};
use spectrusty::peripherals::serial::SerialPortDevice;
use spectrusty_utils::io::{Empty, Sink};

use super::spectrum::ZxSpectrum;
use super::models::*;

/// A static pluggable multi-joystick device with the attached dynamic bus.
pub type PluggableJoystickDynamicBus<SD, V> = OptionalBusDevice<MultiJoystickBusDevice<VFNullDevice<V>>,
                                                                DynamicSerdeVBus<SD, V>>;
/// AY-3-8912 I/O ports for 128/+2/+2A/+3/+2B models with serial ports controller, AUX device plugged 
/// to the aux port, and a RS-232 device with custom reader and writer.
pub type Ay128Io<V, AUX, R, W> = Ay3_8912Io<VFrameTs<V>, SerialPorts128<AUX, Rs232Io<VFrameTs<V>, R, W>>>;

/// Trait with helpers for accessing static bus devices.
pub trait DeviceAccess: Video {
    /// A device used as joystick bus device
    type JoystickBusDevice;
    /// A serial port device attached to AY-3-8912 I/O port A / serial1 - AUX port
    type Ay128Serial1: SerialPortDevice<Timestamp=VFrameTs<Self::VideoFrame>> + fmt::Debug;
    /// RS232 input
    type CommRd: io::Read + fmt::Debug;
    /// RS232/Centronics output
    type CommWr: io::Write + fmt::Debug;

    fn dyn_bus_device_mut(&mut self) -> Option<&mut DynamicVBus<Self::VideoFrame>> { None }
    fn dyn_bus_device_ref(&self) -> Option<&DynamicVBus<Self::VideoFrame>> { None }
    fn joystick_bus_device_mut(&mut self) -> Option<&mut Self::JoystickBusDevice> { None }
    fn joystick_bus_device_ref(&self) -> Option<&Self::JoystickBusDevice> { None }
    fn keypad128_mut(&mut self) -> Option<&mut SerialKeypad<VFrameTs<Self::VideoFrame>>> { None }
    fn ay128_ref(&self) -> Option<(&Ay3_891xAudio,
                                   &Ay128Io<Self::VideoFrame, Self::Ay128Serial1, Self::CommRd, Self::CommWr>)> { None }
    fn ay128_mut(&mut self) -> Option<(&mut Ay3_891xAudio,
                                       &mut Ay128Io<Self::VideoFrame, Self::Ay128Serial1, Self::CommRd, Self::CommWr>)> { None }
    fn plus3centronics_writer_ref(&self) -> Option<&Self::CommWr> { None }
    fn plus3centronics_writer_mut(&mut self) -> Option<&mut Self::CommWr> { None }
}

/// Trait with helpers for managing dynamic bus devices.
pub trait DynamicDevices<V: VideoFrame> {
    /// Returns `true` if dynamic bus device is present in the bus device chain.
    fn attach_device<D: Into<BoxNamedDynDevice<VFrameTs<V>>>>(&mut self, device: D) -> bool;
    fn detach_device<D: NamedBusDevice<VFrameTs<V>> + 'static>(&mut self) -> Option<Box<D>>;
    fn device_mut<D: NamedBusDevice<VFrameTs<V>> + 'static>(&mut self) -> Option<&mut D>;
    fn device_ref<D: NamedBusDevice<VFrameTs<V>> + 'static>(&self) -> Option<&D>;
    /// This needs to be called after deserializing dynamic devices.
    fn rebuild_device_index(&mut self);
}

macro_rules! impl_device_access_ula {
    ($ula:ident<M:$membound:ident>) => {
        impl<M, X, V, SD> DeviceAccess for $ula<M, PluggableJoystickDynamicBus<SD, V>, X, V>
            where M: $membound, X: MemoryExtension, V: VideoFrame
        {
            type JoystickBusDevice = PluggableJoystickDynamicBus<SD, Self::VideoFrame>;
            type Ay128Serial1 = NullSerialPort<VFrameTs<Self::VideoFrame>>;
            type CommRd = Empty;
            type CommWr = Sink;

            fn dyn_bus_device_mut(&mut self) -> Option<&mut DynamicVBus<Self::VideoFrame>> {
                Some(self.bus_device_mut().next_device_mut())
            }

            fn dyn_bus_device_ref(&self) -> Option<&DynamicVBus<Self::VideoFrame>> {
                Some(self.bus_device_ref().next_device_ref())
            }

            fn joystick_bus_device_mut(&mut self) -> Option<&mut Self::JoystickBusDevice> {
                Some(self.bus_device_mut())
            }

            fn joystick_bus_device_ref(&self) -> Option<&Self::JoystickBusDevice> {
                Some(&mut self.bus_device_ref())
            }
        }
    };
}

impl_device_access_ula!(Ula<M: ZxMemory>);
impl_device_access_ula!(Scld<M: PagedMemory8k>);

macro_rules! impl_device_access_ula128 {
    ($ula:ident<$vidfrm:ty>) => {
        impl<X, SD, R, W> DeviceAccess for $ula<PluggableJoystickDynamicBus<SD, $vidfrm>, X, R, W>
            where X: MemoryExtension,
                  R: io::Read + fmt::Debug,
                  W: io::Write + fmt::Debug
        {
            type JoystickBusDevice = PluggableJoystickDynamicBus<SD, Self::VideoFrame>;
            type Ay128Serial1 = SerialKeypad<VFrameTs<Self::VideoFrame>>;
            type CommRd = R;
            type CommWr = W;

            fn dyn_bus_device_mut(&mut self) -> Option<&mut DynamicVBus<Self::VideoFrame>> {
                Some(self.bus_device_mut().next_device_mut().next_device_mut())
            }

            fn dyn_bus_device_ref(&self) -> Option<&DynamicVBus<Self::VideoFrame>> {
                Some(self.bus_device_ref().next_device_ref().next_device_ref())
            }

            fn joystick_bus_device_mut(&mut self) -> Option<&mut Self::JoystickBusDevice> {
                Some(self.bus_device_mut().next_device_mut())
            }

            fn joystick_bus_device_ref(&self) -> Option<&Self::JoystickBusDevice> {
                Some(self.bus_device_ref().next_device_ref())
            }

            fn keypad128_mut(&mut self) -> Option<&mut SerialKeypad<VFrameTs<Self::VideoFrame>>> {
                Some(&mut self.bus_device_mut().ay_io.port_a.serial1)
            }

            fn ay128_ref(&self) -> Option<(&Ay3_891xAudio,
                                           &Ay128Io<Self::VideoFrame, Self::Ay128Serial1, R, W>)> {
                let dev = self.bus_device_ref();
                Some((&dev.ay_sound, &dev.ay_io))
            }

            fn ay128_mut(&mut self) -> Option<(&mut Ay3_891xAudio,
                                               &mut Ay128Io<Self::VideoFrame, Self::Ay128Serial1, R, W>)> {
                let dev = self.bus_device_mut();
                Some((&mut dev.ay_sound, &mut dev.ay_io))
            }
        }
    };
}

impl_device_access_ula128!(Ula128AyKeypad<Ula128VidFrame>);
impl_device_access_ula128!(Plus128<Ula128VidFrame>);

macro_rules! impl_device_access_ula3 {
    ($ula:ident<$vidfrm:ty>) => {
        impl<X, SD, R, W> DeviceAccess for $ula<PluggableJoystickDynamicBus<SD, $vidfrm>, X, R, W>
            where X: MemoryExtension,
                  R: io::Read + fmt::Debug,
                  W: io::Write + fmt::Debug
        {
            type JoystickBusDevice = PluggableJoystickDynamicBus<SD, Self::VideoFrame>;
            type Ay128Serial1 =  NullSerialPort<VFrameTs<Self::VideoFrame>>;
            type CommRd = R;
            type CommWr = W;

            fn dyn_bus_device_mut(&mut self) -> Option<&mut DynamicVBus<Self::VideoFrame>> {
                Some(self.bus_device_mut().next_device_mut().next_device_mut().next_device_mut())
            }

            fn dyn_bus_device_ref(&self) -> Option<&DynamicVBus<Self::VideoFrame>> {
                Some(self.bus_device_ref().next_device_ref().next_device_ref().next_device_ref())
            }

            fn joystick_bus_device_mut(&mut self) -> Option<&mut Self::JoystickBusDevice> {
                Some(self.bus_device_mut().next_device_mut().next_device_mut())
            }

            fn joystick_bus_device_ref(&self) -> Option<&Self::JoystickBusDevice> {
                Some(self.bus_device_ref().next_device_ref().next_device_ref())
            }

            fn ay128_ref(&self) -> Option<(&Ay3_891xAudio, 
                                           &Ay128Io<Self::VideoFrame, Self::Ay128Serial1, R, W>)> {
                let dev = self.bus_device_ref().next_device_ref();
                Some((&dev.ay_sound, &dev.ay_io))
            }

            fn ay128_mut(&mut self) -> Option<(&mut Ay3_891xAudio,
                                               &mut Ay128Io<Self::VideoFrame, Self::Ay128Serial1, R, W>)> {
                let dev = self.bus_device_mut().next_device_mut();
                Some((&mut dev.ay_sound, &mut dev.ay_io))
            }

            fn plus3centronics_writer_ref(&self) -> Option<&W> {
                Some(&self.bus_device_ref().writer)
            }

            fn plus3centronics_writer_mut(&mut self) -> Option<&mut W> {
                Some(&mut self.bus_device_mut().writer)
            }
        }
    };
}

impl_device_access_ula3!(Ula3Ay<Ula3VidFrame>);
impl_device_access_ula3!(Plus3<Ula3VidFrame>);

impl<C, U, F> DynamicDevices<U::VideoFrame> for ZxSpectrum<C, U, F>
        where C: Cpu,
              U: DeviceAccess,
              U::VideoFrame: 'static
{
    fn device_mut<D>(&mut self) -> Option<&mut D>
        where D: NamedBusDevice<VFrameTs<U::VideoFrame>> + 'static
    {
        let devices = &mut self.state.devices;
        self.ula.dyn_bus_device_mut().and_then(|dynbus| {
            devices.get_device_index::<D>().map(move |index| {
                dynbus.as_device_mut::<D>(index)
            })
        })
    }

    fn device_ref<D>(&self) -> Option<&D>
        where D: NamedBusDevice<VFrameTs<U::VideoFrame>> + 'static
    {
        self.ula.dyn_bus_device_ref().and_then(|dynbus| {
            self.state.devices.get_device_index::<D>().map(|index| {
                dynbus.as_device_ref::<D>(index)
            })
        })
    }

    fn attach_device<D>(&mut self, device: D) -> bool
        where D: Into<BoxNamedDynDevice<VFrameTs<U::VideoFrame>>>
    {
        if let Some(dynbus) = self.ula.dyn_bus_device_mut() {
            let device: BoxNamedDynDevice<VFrameTs<U::VideoFrame>> = device.into();
            match self.state.devices.entry(device.type_id()) {
                Entry::Occupied(e) => { dynbus.replace_device(*e.get(), device); },
                Entry::Vacant(e) => { e.insert(dynbus.append_device(device)); }
            }
            return true
        }
        false
    }

    fn detach_device<D>(&mut self) -> Option<Box<D>>
        where D: NamedBusDevice<VFrameTs<U::VideoFrame>> + 'static
    {
        let devices = &mut self.state.devices;
        self.ula.dyn_bus_device_mut().and_then(|dynbus| {
            devices.remove_device_index::<D>().map(|index| {
                let removed_device = dynbus.swap_remove_as_device::<D>(index);
                if dynbus.len() > index {
                    let moved_device = &dynbus[index];
                    let prev_index = devices.insert(moved_device.type_id(), index)
                                            .expect("the device should exist before");
                    assert_eq!(prev_index, dynbus.len());
                }
                removed_device
            })
        })
    }

    fn rebuild_device_index(&mut self) {
        self.state.devices.clear();
        if let Some(dynbus) = self.ula.dyn_bus_device_ref() {
            self.state.devices.extend(
                dynbus.into_iter().enumerate()
                      .map(|(i, dev)| (dev.type_id(), i) )
            );
        }
    }
}


impl<C: Cpu, S, X, F, R, W> ZxSpectrumModel<C, S, X, F, R, W>
    where X: MemoryExtension,
          R: io::Read + fmt::Debug,
          W: io::Write + fmt::Debug,
{
    pub fn rebuild_device_index(&mut self) {
        spectrum_model_dispatch!(self(spec) => spec.rebuild_device_index());
    }
}
