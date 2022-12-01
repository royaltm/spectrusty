/*
    sdl2-zxspectrum: ZX Spectrum emulator example as a SDL2 application.
    Copyright (C) 2020  Rafal Michalski

    For the full copyright notice, see the main.rs file.
*/
use core::fmt::{self, Write};
use core::slice;
use std::io;

use serde::{Serialize, Deserialize};
use arrayvec::ArrayString;

use spectrusty::z80emu::Cpu;
use spectrusty::bus::{
    BusDevice, NullDevice,
    zxprinter::Alphacom32
};
use spectrusty::chip::{
    ula::Ula,
    scld::Scld
};
use spectrusty::clock::TimestampOps;
use spectrusty::memory::PagedMemory8k;

use zxspectrum_common::{
    DynamicDevices, DeviceAccess, BusTs, SpecBusTs,
    Ula3Ay, Plus48, Plus128, Plus3, Ula128AyKeypad
};
use spectrusty_utils::printer::{EpsonPrinterGfx, ImageSpooler};

use super::ZxSpectrum;
use super::interface1::ZxInterface1Access;

pub type ZxPrinter<T> = Alphacom32<ImageSpooler, NullDevice<T>>;

/// The printer connected to various serial and parallel ports.
///
/// It passes to stdout any printer data but intercept EPSON graphic lines and buffers them internally.
#[derive(Serialize, Deserialize, Debug)]
pub struct EpsonGfxFilteredStdoutWriter {
    pub interceptor: EpsonPrinterGfx,
    #[serde(skip, default = "io::stdout")]
    stdout: io::Stdout,
}

// 2 different printers can be connected to static devices (serial / parallel)
pub trait SpoolerAccess: DeviceAccess {
    fn plus3centronics_spooler_ref(&self) -> Option<&EpsonPrinterGfx> { None }
    fn plus3centronics_spooler_mut(&mut self) -> Option<&mut EpsonPrinterGfx> { None }
    fn ay128rs232_spooler_ref(&self) -> Option<&EpsonPrinterGfx> { None }
    fn ay128rs232_spooler_mut(&mut self) -> Option<&mut EpsonPrinterGfx> { None }
}

// And 2 printers can be connected to dynamic bus devices
pub trait DynSpoolerAccess  {
    fn if1rs232_spooler_ref(&self) -> Option<&EpsonPrinterGfx> { None }
    fn if1rs232_spooler_mut(&mut self) -> Option<&mut EpsonPrinterGfx> { None }
    fn zxprinter_spooler_ref(&self) -> Option<&ImageSpooler> { None }
    fn zxprinter_spooler_mut(&mut self) -> Option<&mut ImageSpooler> { None }
}

impl Default for EpsonGfxFilteredStdoutWriter {
    fn default() -> Self {
        let stdout = io::stdout();
        let interceptor = EpsonPrinterGfx::default();
        EpsonGfxFilteredStdoutWriter { stdout, interceptor }
    }
}

impl io::Write for EpsonGfxFilteredStdoutWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let ch = buf[0];
        if let Some(buf) = self.interceptor.intercept(&ch) {
            for ch in buf.iter().copied() {
                if ch.is_ascii() && (ch == 10 || ch == 13 || !ch.is_ascii_control()) {
                    if self.stdout.write(slice::from_ref(&ch))? != 1 {
                        return Ok(0)
                    }
                }
                else {
                    let mut s = ArrayString::<8>::new();
                    write!(s, " {:02x} ", buf[0]).unwrap();
                    if self.stdout.write(s.as_bytes())? != s.len() {
                        return Ok(0)
                    }
                }
            }
        }
        Ok(1)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stdout.flush()
    }
}

// Spooler access to different static bus devices
macro_rules! impl_spooler_access_ula128 {
    ($ula:ident) => {
        impl<D, X, R> SpoolerAccess for $ula<D, X, R, EpsonGfxFilteredStdoutWriter>
            where R: io::Read + fmt::Debug,
                  D: BusDevice,
                  Self: DeviceAccess<CommWr=EpsonGfxFilteredStdoutWriter>
        {
            fn plus3centronics_spooler_ref(&self) -> Option<&EpsonPrinterGfx> {
                self.plus3centronics_writer_ref().map(|wr| &wr.interceptor)
            }

            fn plus3centronics_spooler_mut(&mut self) -> Option<&mut EpsonPrinterGfx> {
                self.plus3centronics_writer_mut().map(|wr| &mut wr.interceptor)
            }

            fn ay128rs232_spooler_ref(&self) -> Option<&EpsonPrinterGfx> {
                self.ay128_ref().map(|(_, ay_io)| &ay_io.port_a.serial2.writer.interceptor)
            }

            fn ay128rs232_spooler_mut(&mut self) -> Option<&mut EpsonPrinterGfx> {
                self.ay128_mut().map(|(_, ay_io)| &mut ay_io.port_a.serial2.writer.interceptor)
            }
        }        
    };
}

impl_spooler_access_ula128!(Ula128AyKeypad);
impl_spooler_access_ula128!(Ula3Ay);
impl_spooler_access_ula128!(Plus3);
impl_spooler_access_ula128!(Plus128);

impl<M, D, X, V> SpoolerAccess for Ula<M, D, X, V> where Self: DeviceAccess {}
impl<D, X> SpoolerAccess for Plus48<D, X> where Self: DeviceAccess {}
impl<M: PagedMemory8k, D, X, V> SpoolerAccess for Scld<M, D, X, V>
    where Self: DeviceAccess {}

// Spooler access to dynamic devices
impl<C: Cpu, U> DynSpoolerAccess for ZxSpectrum<C, U>
    where U: DeviceAccess,
          BusTs<U>: TimestampOps + 'static
{
    fn if1rs232_spooler_ref(&self) -> Option<&EpsonPrinterGfx> {
        self.zxif1_serial_ref().map(|s| &s.writer.interceptor)
    }

    fn if1rs232_spooler_mut(&mut self) -> Option<&mut EpsonPrinterGfx> {
        self.zxif1_serial_mut().map(|s| &mut s.writer.interceptor)
    }

    fn zxprinter_spooler_ref(&self) -> Option<&ImageSpooler> {
        self.device_ref::<ZxPrinter<SpecBusTs<Self>>>().map(|p| &p.spooler)
    }

    fn zxprinter_spooler_mut(&mut self) -> Option<&mut ImageSpooler> {
        self.device_mut::<ZxPrinter<SpecBusTs<Self>>>().map(|p| &mut p.spooler)
    }
}
