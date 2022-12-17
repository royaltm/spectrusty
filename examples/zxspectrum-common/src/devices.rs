/*
    zxspectrum-common: High-level ZX Spectrum emulator library example.
    Copyright (C) 2020-2022  Rafal Michalski

    For the full copyright notice, see the lib.rs file.
*/
use core::fmt;
use std::collections::hash_map::Entry;
use std::io;

use spectrusty::z80emu::Cpu;
use spectrusty::clock::{TimestampOps, VFrameTs};
use spectrusty::bus::{
    BusDevice, OptionalBusDevice, DynamicBus, DynamicSerdeBus, NullDevice,
    NamedBusDevice, BoxNamedDynDevice,
    ay::{
        self,
        Ay3_891xAudio, Ay3_8912Io,
        serial128::{
            SerialKeypad, NullSerialPort,
            Rs232Io, SerialPorts128
        }
    },
    mouse,
    joystick::MultiJoystickBusDevice,
};
use spectrusty::chip::{
    ControlUnit,
    ula::UlaVideoFrame,
    ula128::Ula128VidFrame,
    ula3::Ula3VidFrame,
};
use spectrusty::memory::{PagedMemory8k, ZxMemory, MemoryExtension};
use spectrusty::video::VideoFrame;
use spectrusty::peripherals::serial::SerialPortDevice;
use spectrusty_utils::io::{Empty, Sink};

use super::spectrum::{SpectrumUla, ZxSpectrum};
use super::models::*;

/// A static pluggable multi-joystick bus device.
pub type PluggableJoystick<D> = OptionalBusDevice<
                                    MultiJoystickBusDevice<NullDevice<<D as BusDevice>::Timestamp>>,
                                    D>;

/// A static pluggable multi-joystick bus device with the attached dynamic bus.
pub type PluggableJoystickDynamicBus<SD, T> = PluggableJoystick<DynamicSerdeBus<SD, NullDevice<T>>>;

/// AY-3-8912 I/O ports for 128/+2/+2A/+3/+2B models with serial ports controller, AUX device plugged 
/// to the aux port, and a RS-232 device with custom reader and writer.
pub type Ay128Io<T, AUX, R, W> = Ay3_8912Io<T,
                                         SerialPorts128<AUX,
                                                        Rs232Io<T, R, W>
                                                       >
                                        >;

/// Melodik AY-3-8913 [BusDevice] that can be used as a dynamic device.
pub type Ay3_891xMelodik<T> = ay::Ay3_891xMelodik<NullDevice<T>>;
/// Fuller Box AY-3-8913 [BusDevice] that can be used as a dynamic device.
pub type Ay3_891xFullerBox<T> = ay::Ay3_891xFullerBox<NullDevice<T>>;
/// Kempston Mouse [BusDevice] that can be used as a dynamic device.
pub type KempstonMouse<T> = mouse::KempstonMouse<NullDevice<T>>;
/// A shortcut to the associated type [Timestamp][BusDevice::Timestamp] from the type `U`
/// implementing [ControlUnit].
pub type BusTs<U> = <<U as ControlUnit>::BusDevice as BusDevice>::Timestamp;
/// A shortcut to the associated type [Timestamp][BusDevice::Timestamp] from the type `S`
/// implementing [SpectrumUla].
pub type SpecBusTs<S> = BusTs<<S as SpectrumUla>::Chipset>;

/// Trait with helpers for accessing static bus devices.
///
/// This trait can be used for different BUS configuration.
///
/// All trait methods return optional values depending if a device is present in the static BUS configuration.
/// Thus it's easier to write re-usable code which accesses certain devices.
#[allow(clippy::type_complexity)]
pub trait DeviceAccess: ControlUnit {
    /// A device used as a joystick bus device.
    type JoystickBusDevice;
    /// A serial port device attached to AY-3-8912 I/O port A / serial1 - AUX port.
    type Ay128Serial1: SerialPortDevice<Timestamp=BusTs<Self>>;
    /// RS232 input.
    type CommRd: io::Read + fmt::Debug;
    /// RS232/Centronics output.
    type CommWr: io::Write + fmt::Debug;

    fn dyn_bus_device_mut(&mut self) -> Option<&mut DynamicBus<NullDevice<BusTs<Self>>>> { None }
    fn dyn_bus_device_ref(&self) -> Option<&DynamicBus<NullDevice<BusTs<Self>>>> { None }
    fn joystick_bus_device_mut(&mut self) -> Option<&mut Self::JoystickBusDevice> { None }
    fn joystick_bus_device_ref(&self) -> Option<&Self::JoystickBusDevice> { None }
    fn keypad128_mut(&mut self) -> Option<&mut SerialKeypad<BusTs<Self>>> { None }
    fn ay128_ref(&self) -> Option<(&Ay3_891xAudio,
                                   &Ay128Io<BusTs<Self>, Self::Ay128Serial1, Self::CommRd, Self::CommWr>)> { None }
    fn ay128_mut(&mut self) -> Option<(&mut Ay3_891xAudio,
                                       &mut Ay128Io<BusTs<Self>, Self::Ay128Serial1, Self::CommRd, Self::CommWr>)> { None }
    fn plus3centronics_writer_ref(&self) -> Option<&Self::CommWr> { None }
    fn plus3centronics_writer_mut(&mut self) -> Option<&mut Self::CommWr> { None }
}

/// Trait with helpers for managing dynamic bus devices.
///
/// This trait can be used to easily manage dynamic devices in run time.
pub trait DynamicDevices: SpectrumUla {
    /// Returns `true` if a dynamic bus [DynamicBus] is present in the static device chain
    /// and the provided `device` has been attached to it. Returns `false` otherwise.
    ///
    /// Any previous dynamic device of the same type `D` will be replaced by the provided
    /// `device` instance.
    fn attach_device<D: Into<BoxNamedDynDevice<SpecBusTs<Self>>>>(&mut self, device: D) -> bool;
    fn detach_device<D: NamedBusDevice<SpecBusTs<Self>> + 'static>(&mut self) -> Option<Box<D>>;
    fn device_mut<D: NamedBusDevice<SpecBusTs<Self>> + 'static>(&mut self) -> Option<&mut D>;
    fn device_ref<D: NamedBusDevice<SpecBusTs<Self>> + 'static>(&self) -> Option<&D>;
    /// This must be called after deserializing a struct with dynamic devices.
    ///
    /// Otherwise methods in this trait won't be able to find already present devices.
    fn rebuild_device_index(&mut self);
}

macro_rules! impl_device_access_ula {
    (@impl) => {
        type JoystickBusDevice = PluggableJoystickDynamicBus<SD, T>;
        type Ay128Serial1 = NullSerialPort<T>;
        type CommRd = Empty;
        type CommWr = Sink;

        fn dyn_bus_device_mut(&mut self) -> Option<&mut DynamicBus<NullDevice<T>>> {
            Some(self.bus_device_mut().next_device_mut())
        }

        fn dyn_bus_device_ref(&self) -> Option<&DynamicBus<NullDevice<T>>> {
            Some(self.bus_device_ref().next_device_ref())
        }

        fn joystick_bus_device_mut(&mut self) -> Option<&mut Self::JoystickBusDevice> {
            Some(self.bus_device_mut())
        }

        fn joystick_bus_device_ref(&self) -> Option<&Self::JoystickBusDevice> {
            Some(self.bus_device_ref())
        }
    };
    (@impl_nodyn) => {
        type JoystickBusDevice = PluggableJoystick<NullDevice<T>>;
        type Ay128Serial1 = NullSerialPort<T>;
        type CommRd = Empty;
        type CommWr = Sink;

        fn joystick_bus_device_mut(&mut self) -> Option<&mut Self::JoystickBusDevice> {
            Some(self.bus_device_mut())
        }

        fn joystick_bus_device_ref(&self) -> Option<&Self::JoystickBusDevice> {
            Some(self.bus_device_ref())
        }
    };
    ($ula:ident<M:$membound:ident>) => {
        impl<T, M, X, V, SD> DeviceAccess for $ula<M, PluggableJoystickDynamicBus<SD, T>, X, V>
            where T: From<VFrameTs<V>> + fmt::Debug + Copy,
                  M: $membound, X: MemoryExtension, V: VideoFrame
        {
            impl_device_access_ula! { @impl }
        }
        impl<T, M, X, V> DeviceAccess for $ula<M, PluggableJoystick<NullDevice<T>>, X, V>
            where T: From<VFrameTs<V>> + fmt::Debug + Copy,
                  M: $membound, X: MemoryExtension, V: VideoFrame
        {
            impl_device_access_ula! { @impl_nodyn }
        }
    };
    ($ula:ident<$vidfrm:ty>) => {
        impl<T, X, SD> DeviceAccess for $ula<PluggableJoystickDynamicBus<SD, T>, X>
            where T: From<VFrameTs<$vidfrm>> + fmt::Debug + Copy,
                  X: MemoryExtension
        {
            impl_device_access_ula! { @impl }
        }
        impl<T, X> DeviceAccess for $ula<PluggableJoystick<NullDevice<T>>, X>
            where T: From<VFrameTs<$vidfrm>> + fmt::Debug + Copy,
                  X: MemoryExtension
        {
            impl_device_access_ula! { @impl_nodyn }
        }
    };
}

impl_device_access_ula!(Ula<M: ZxMemory>);
impl_device_access_ula!(Plus48<UlaVideoFrame>);
impl_device_access_ula!(Scld<M: PagedMemory8k>);

macro_rules! impl_device_access_ula128 {
    (@impl_shared) => {
            fn joystick_bus_device_mut(&mut self) -> Option<&mut Self::JoystickBusDevice> {
                Some(self.bus_device_mut().next_device_mut())
            }

            fn joystick_bus_device_ref(&self) -> Option<&Self::JoystickBusDevice> {
                Some(self.bus_device_ref().next_device_ref())
            }

            fn keypad128_mut(&mut self) -> Option<&mut SerialKeypad<T>> {
                Some(&mut self.bus_device_mut().ay_io.port_a.serial1)
            }

            fn ay128_ref(&self) -> Option<(&Ay3_891xAudio,
                                           &Ay128Io<T, Self::Ay128Serial1, R, W>)> {
                let dev = self.bus_device_ref();
                Some((&dev.ay_sound, &dev.ay_io))
            }

            fn ay128_mut(&mut self) -> Option<(&mut Ay3_891xAudio,
                                               &mut Ay128Io<T, Self::Ay128Serial1, R, W>)> {
                let dev = self.bus_device_mut();
                Some((&mut dev.ay_sound, &mut dev.ay_io))
            }
    };
    ($ula:ident<$vidfrm:ty>) => {
        impl<T, X, SD, R, W> DeviceAccess for $ula<PluggableJoystickDynamicBus<SD, T>, X, R, W>
            where T: From<VFrameTs<$vidfrm>> + TimestampOps,
                  X: MemoryExtension,
                  R: io::Read + fmt::Debug,
                  W: io::Write + fmt::Debug
        {
            type JoystickBusDevice = PluggableJoystickDynamicBus<SD, T>;
            type Ay128Serial1 = SerialKeypad<T>;
            type CommRd = R;
            type CommWr = W;

            fn dyn_bus_device_mut(&mut self) -> Option<&mut DynamicBus<NullDevice<T>>> {
                Some(self.bus_device_mut().next_device_mut().next_device_mut())
            }

            fn dyn_bus_device_ref(&self) -> Option<&DynamicBus<NullDevice<T>>> {
                Some(self.bus_device_ref().next_device_ref().next_device_ref())
            }

            impl_device_access_ula128!(@impl_shared);
        }

        impl<T, X, R, W> DeviceAccess for $ula<PluggableJoystick<NullDevice<T>>, X, R, W>
            where T: From<VFrameTs<$vidfrm>> + TimestampOps,
                  X: MemoryExtension,
                  R: io::Read + fmt::Debug,
                  W: io::Write + fmt::Debug
        {
            type JoystickBusDevice = PluggableJoystick<NullDevice<T>>;
            type Ay128Serial1 = SerialKeypad<T>;
            type CommRd = R;
            type CommWr = W;

            impl_device_access_ula128!(@impl_shared);
        }
    };
}

impl_device_access_ula128!(Ula128AyKeypad<Ula128VidFrame>);
impl_device_access_ula128!(Plus128<Ula128VidFrame>);

macro_rules! impl_device_access_ula3 {
    (@impl_shared) => {
            fn joystick_bus_device_mut(&mut self) -> Option<&mut Self::JoystickBusDevice> {
                Some(self.bus_device_mut().next_device_mut().next_device_mut())
            }

            fn joystick_bus_device_ref(&self) -> Option<&Self::JoystickBusDevice> {
                Some(self.bus_device_ref().next_device_ref().next_device_ref())
            }

            fn ay128_ref(&self) -> Option<(&Ay3_891xAudio, 
                                           &Ay128Io<T, Self::Ay128Serial1, R, W>)> {
                let dev = self.bus_device_ref().next_device_ref();
                Some((&dev.ay_sound, &dev.ay_io))
            }

            fn ay128_mut(&mut self) -> Option<(&mut Ay3_891xAudio,
                                               &mut Ay128Io<T, Self::Ay128Serial1, R, W>)> {
                let dev = self.bus_device_mut().next_device_mut();
                Some((&mut dev.ay_sound, &mut dev.ay_io))
            }

            fn plus3centronics_writer_ref(&self) -> Option<&W> {
                Some(&self.bus_device_ref().writer)
            }

            fn plus3centronics_writer_mut(&mut self) -> Option<&mut W> {
                Some(&mut self.bus_device_mut().writer)
            }
    };
    ($ula:ident<$vidfrm:ty>) => {
        impl<T, X, SD, R, W> DeviceAccess for $ula<PluggableJoystickDynamicBus<SD, T>, X, R, W>
            where T: From<VFrameTs<$vidfrm>> + TimestampOps,
                  X: MemoryExtension,
                  R: io::Read + fmt::Debug,
                  W: io::Write + fmt::Debug
        {
            type JoystickBusDevice = PluggableJoystickDynamicBus<SD, T>;
            type Ay128Serial1 =  NullSerialPort<T>;
            type CommRd = R;
            type CommWr = W;

            fn dyn_bus_device_mut(&mut self) -> Option<&mut DynamicBus<NullDevice<T>>> {
                Some(self.bus_device_mut().next_device_mut().next_device_mut().next_device_mut())
            }

            fn dyn_bus_device_ref(&self) -> Option<&DynamicBus<NullDevice<T>>> {
                Some(self.bus_device_ref().next_device_ref().next_device_ref().next_device_ref())
            }

            impl_device_access_ula3!(@impl_shared);
        }

        impl<T, X, R, W> DeviceAccess for $ula<PluggableJoystick<NullDevice<T>>, X, R, W>
            where T: From<VFrameTs<$vidfrm>> + TimestampOps,
                  X: MemoryExtension,
                  R: io::Read + fmt::Debug,
                  W: io::Write + fmt::Debug
        {
            type JoystickBusDevice = PluggableJoystick<NullDevice<T>>;
            type Ay128Serial1 =  NullSerialPort<T>;
            type CommRd = R;
            type CommWr = W;

            impl_device_access_ula3!(@impl_shared);
        }
    };
}

impl_device_access_ula3!(Ula3Ay<Ula3VidFrame>);
impl_device_access_ula3!(Plus3<Ula3VidFrame>);

impl<C, U, F> DynamicDevices for ZxSpectrum<C, U, F>
        where C: Cpu,
              U: DeviceAccess,
              BusTs<U>: 'static
{
    fn device_mut<D>(&mut self) -> Option<&mut D>
        where D: NamedBusDevice<BusTs<U>> + 'static
    {
        let devices = &mut self.state.devices;
        self.ula.dyn_bus_device_mut().and_then(|dynbus| {
            devices.get_device_index::<D>().map(move |index| {
                dynbus.as_device_mut::<D>(index)
            })
        })
    }

    fn device_ref<D>(&self) -> Option<&D>
        where D: NamedBusDevice<BusTs<U>> + 'static
    {
        self.ula.dyn_bus_device_ref().and_then(|dynbus| {
            self.state.devices.get_device_index::<D>().map(|index| {
                dynbus.as_device_ref::<D>(index)
            })
        })
    }

    fn attach_device<D>(&mut self, device: D) -> bool
        where D: Into<BoxNamedDynDevice<BusTs<U>>>
    {
        if let Some(dynbus) = self.ula.dyn_bus_device_mut() {
            let device: BoxNamedDynDevice<BusTs<U>> = device.into();
            match self.state.devices.entry(device.type_id()) {
                Entry::Occupied(e) => { dynbus.replace_device(*e.get(), device); },
                Entry::Vacant(e) => { e.insert(dynbus.append_device(device)); }
            }
            return true
        }
        false
    }

    fn detach_device<D>(&mut self) -> Option<Box<D>>
        where D: NamedBusDevice<BusTs<U>> + 'static
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
    /// This must be called after deserializing a struct with dynamic devices.
    ///
    /// Otherwise methods managing dynamic devices won't be able to find devices that are already present.
    pub fn rebuild_device_index(&mut self) {
        spectrum_model_dispatch!(self(spec) => spec.rebuild_device_index());
    }
}
