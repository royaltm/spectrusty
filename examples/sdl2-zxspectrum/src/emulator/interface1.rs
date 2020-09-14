/*
    sdl2-zxspectrum: ZX Spectrum emulator example as a SDL2 application.
    Copyright (C) 2020  Rafal Michalski

    For the full copyright notice, see the main.rs file.
*/
use spectrusty::z80emu::Cpu;
use spectrusty::bus::{
    NullDevice,
    zxinterface1::{
        ZxInterface1BusDevice, ZxNetUdpSyncSocket, ZxMicrodrives, ZxNet, Rs232Io
    }
};
use spectrusty::clock::VFrameTs;
use spectrusty::video::Video;
use zxspectrum_common::DynamicDevices;
use super::{ZxSpectrum, NonBlockingStdinReader};
use super::printer::EpsonGfxFilteredStdoutWriter;

pub type ZxInterface1<T> = ZxInterface1BusDevice<
                                        NonBlockingStdinReader,
                                        EpsonGfxFilteredStdoutWriter,
                                        ZxNetUdpSyncSocket,
                                        NullDevice<T>>;

pub type If1Rs232Io<V> = Rs232Io<VFrameTs<V>, NonBlockingStdinReader, EpsonGfxFilteredStdoutWriter>;
pub type If1ZxNet<V> = ZxNet<VFrameTs<V>, ZxNetUdpSyncSocket>;
pub type If1Microdrives<V> = ZxMicrodrives<VFrameTs<V>>;

pub trait ZxInterface1Access<U: Video + 'static> {
    // ZX Interface 1 microdrive access
    fn zxinterface1_ref(&self) -> Option<&ZxInterface1<VFrameTs<U::VideoFrame>>>;
    fn zxinterface1_mut(&mut self) -> Option<&mut ZxInterface1<VFrameTs<U::VideoFrame>>>;

    fn microdrives_ref(&self) -> Option<&If1Microdrives<U::VideoFrame>> {
        self.zxinterface1_ref().map(|zif1| &zif1.microdrives)
    }
    fn microdrives_mut(&mut self) -> Option<&mut If1Microdrives<U::VideoFrame>> {
        self.zxinterface1_mut().map(|zif1| &mut zif1.microdrives)
    }
    fn zxif1_serial_ref(&self) -> Option<&If1Rs232Io<U::VideoFrame>> {
        self.zxinterface1_ref().map(|zif1| &zif1.serial)
    }
    fn zxif1_serial_mut(&mut self) -> Option<&mut If1Rs232Io<U::VideoFrame>> {
        self.zxinterface1_mut().map(|zif1| &mut zif1.serial)
    }
    fn zxif1_network_ref(&self) -> Option<&If1ZxNet<U::VideoFrame>> {
        self.zxinterface1_ref().map(|zif1| &zif1.network)
    }
    fn zxif1_network_mut(&mut self) -> Option<&mut If1ZxNet<U::VideoFrame>> {
        self.zxinterface1_mut().map(|zif1| &mut zif1.network)
    }
}

impl<C: Cpu, U> ZxInterface1Access<U> for ZxSpectrum<C, U>
    where ZxSpectrum<C, U>: DynamicDevices<U::VideoFrame>,
          U: Video +  'static
{
    fn zxinterface1_ref(&self) -> Option<&ZxInterface1<VFrameTs<U::VideoFrame>>> {
        self.device_ref::<ZxInterface1<VFrameTs<U::VideoFrame>>>()
    }

    fn zxinterface1_mut(&mut self) -> Option<&mut ZxInterface1<VFrameTs<U::VideoFrame>>> {
        self.device_mut::<ZxInterface1<VFrameTs<U::VideoFrame>>>()
    }
}
