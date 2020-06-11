use core::fmt::{self, Write};
use core::slice;
use std::io::{self, Read};
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::TryRecvError;
use std::thread;

use serde::{Serialize, Deserialize, Serializer, de::{self, Deserializer, Visitor}};
use arrayvec::ArrayString;

use crate::printer::SerialPrinterGfxGrabber;

#[derive(Debug)]
pub struct NonBlockingStdinReader {
    rx: Receiver<u8>
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FilterGfxStdoutWriter {
    pub grabber: SerialPrinterGfxGrabber,
    #[serde(skip, default = "io::stdout")]
    stdout: io::Stdout,
}

impl Default for NonBlockingStdinReader {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel::<u8>();
        thread::spawn(move || -> io::Result<()> {
            let stdin = io::stdin();
            let bytes = stdin.lock().bytes();
            for byte in bytes {
                tx.send(byte?)
                  .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
            }
            Ok(())
        });
        NonBlockingStdinReader { rx }
    }
}

impl io::Read for NonBlockingStdinReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.len() == 0 {
            Ok(0)
        }
        else {
            match self.rx.try_recv() {
                Ok(data) => {
                    buf[0] = data;
                    Ok(1)
                },
                Err(TryRecvError::Empty) => Ok(0),
                Err(TryRecvError::Disconnected) => Ok(0) // no more input
            }
        }
    }
}

impl Default for FilterGfxStdoutWriter {
    fn default() -> Self {
        let stdout = io::stdout();
        let grabber = SerialPrinterGfxGrabber::default();
        FilterGfxStdoutWriter { stdout, grabber }
    }
}

impl io::Write for FilterGfxStdoutWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let ch = buf[0];
        if let Some(buf) = self.grabber.grab_or_passthrough(&ch) {
            for ch in buf.iter().copied() {
                if ch.is_ascii() && (ch == 10 || ch == 13 || !ch.is_ascii_control()) {
                    if self.stdout.write(slice::from_ref(&ch))? != 1 {
                        return Ok(0)
                    }
                }
                else {
                    let mut s = ArrayString::<[_;8]>::new();
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

impl Serialize for NonBlockingStdinReader {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_unit_struct("NonBlockingStdinReader")
    }
}

impl<'de> Deserialize<'de> for NonBlockingStdinReader {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de>
    {
        struct NonBlockingStdinReaderVisitor;

        impl<'de> Visitor<'de> for NonBlockingStdinReaderVisitor {
            type Value = NonBlockingStdinReader;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("unit")
            }

            fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(NonBlockingStdinReader::default())
            }
        }
        deserializer.deserialize_unit_struct("NonBlockingStdinReader", NonBlockingStdinReaderVisitor)
    }
}

