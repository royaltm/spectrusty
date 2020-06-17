use spectrusty::z80emu::Cpu;
use spectrusty::bus::{
    zxinterface1::{
        ZxInterface1BusDevice, ZxNetUdpSyncSocket, ZXMicrodrives, ZxNet, Rs232Io
    }
};
use spectrusty::video::Video;
use zxspectrum_common::DynamicDevices;
use super::{ZxSpectrum, NonBlockingStdinReader};
use super::printer::EpsonGfxFilteredStdoutWriter;

pub type ZxInterface1<V> = ZxInterface1BusDevice<V,
                                        NonBlockingStdinReader,
                                        EpsonGfxFilteredStdoutWriter,
                                        ZxNetUdpSyncSocket>;

pub type If1Rs232Io<V> = Rs232Io<V, NonBlockingStdinReader, EpsonGfxFilteredStdoutWriter>;
pub type If1ZxNet<V> = ZxNet<V, ZxNetUdpSyncSocket>;

pub trait ZxInterface1Access<U: Video + 'static> {
    // ZX Interface 1 microdrive access
    fn zxinterface1_ref(&self) -> Option<&ZxInterface1<U::VideoFrame>>;
    fn zxinterface1_mut(&mut self) -> Option<&mut ZxInterface1<U::VideoFrame>>;

    fn microdrives_ref(&self) -> Option<&ZXMicrodrives<U::VideoFrame>> {
        self.zxinterface1_ref().map(|zif1| &zif1.microdrives)
    }
    fn microdrives_mut(&mut self) -> Option<&mut ZXMicrodrives<U::VideoFrame>> {
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
    where ZxSpectrum<C, U>: DynamicDevices,
          U: Video +  'static
{
    fn zxinterface1_ref(&self) -> Option<&ZxInterface1<U::VideoFrame>> {
        self.device_ref::<ZxInterface1<U::VideoFrame>>()
    }

    fn zxinterface1_mut(&mut self) -> Option<&mut ZxInterface1<U::VideoFrame>> {
        self.device_mut::<ZxInterface1<U::VideoFrame>>()
    }
}
