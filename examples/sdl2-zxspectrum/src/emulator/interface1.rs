/*
    sdl2-zxspectrum: ZX Spectrum emulator example as a SDL2 application.
    Copyright (C) 2020  Rafal Michalski

    For the full copyright notice, see the main.rs file.
*/
use spectrusty::clock::TimestampOps;
// use spectrusty::chip::ControlUnit;
use spectrusty::z80emu::Cpu;
use spectrusty::bus::{
    NullDevice,
    zxinterface1::{
        ZxInterface1BusDevice, ZxNetUdpSyncSocket, ZxMicrodrives, ZxNet, Rs232Io
    }
};
use zxspectrum_common::{BusTs, SpecBusTs, DeviceAccess, SpectrumUla, DynamicDevices};
use super::{ZxSpectrum, NonBlockingStdinReader};
use super::printer::EpsonGfxFilteredStdoutWriter;

pub type ZxInterface1<T> = ZxInterface1BusDevice<
                                        NonBlockingStdinReader,
                                        EpsonGfxFilteredStdoutWriter,
                                        ZxNetUdpSyncSocket,
                                        NullDevice<T>>;

pub type If1Rs232Io<T> = Rs232Io<T, NonBlockingStdinReader, EpsonGfxFilteredStdoutWriter>;
pub type If1ZxNet<T> = ZxNet<T, ZxNetUdpSyncSocket>;
pub type If1Microdrives<T> = ZxMicrodrives<T>;

pub trait ZxInterface1Access: SpectrumUla {
    // ZX Interface 1 microdrive access
    fn zxinterface1_ref(&self) -> Option<&ZxInterface1<SpecBusTs<Self>>>;
    fn zxinterface1_mut(&mut self) -> Option<&mut ZxInterface1<SpecBusTs<Self>>>;

    fn microdrives_ref(&self) -> Option<&If1Microdrives<SpecBusTs<Self>>> {
        self.zxinterface1_ref().map(|zif1| &zif1.microdrives)
    }
    fn microdrives_mut(&mut self) -> Option<&mut If1Microdrives<SpecBusTs<Self>>> {
        self.zxinterface1_mut().map(|zif1| &mut zif1.microdrives)
    }
    fn zxif1_serial_ref(&self) -> Option<&If1Rs232Io<SpecBusTs<Self>>> {
        self.zxinterface1_ref().map(|zif1| &zif1.serial)
    }
    fn zxif1_serial_mut(&mut self) -> Option<&mut If1Rs232Io<SpecBusTs<Self>>> {
        self.zxinterface1_mut().map(|zif1| &mut zif1.serial)
    }
    fn zxif1_network_ref(&self) -> Option<&If1ZxNet<SpecBusTs<Self>>> {
        self.zxinterface1_ref().map(|zif1| &zif1.network)
    }
    fn zxif1_network_mut(&mut self) -> Option<&mut If1ZxNet<SpecBusTs<Self>>> {
        self.zxinterface1_mut().map(|zif1| &mut zif1.network)
    }
}

impl<C: Cpu, U> ZxInterface1Access for ZxSpectrum<C, U>
    where U: DeviceAccess,
          BusTs<U>: TimestampOps + 'static
{
    fn zxinterface1_ref(&self) -> Option<&ZxInterface1<BusTs<U>>> {
        self.device_ref::<ZxInterface1<BusTs<U>>>()
    }

    fn zxinterface1_mut(&mut self) -> Option<&mut ZxInterface1<BusTs<U>>> {
        self.device_mut::<ZxInterface1<BusTs<U>>>()
    }
}
