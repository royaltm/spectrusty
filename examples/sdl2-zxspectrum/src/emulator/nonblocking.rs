use core::fmt;
use std::io::{self, Read};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;

use serde::{Serialize, Deserialize, de::{self, Deserializer, Visitor}};

/// The stdin reader that doesn't block while waiting for the input data.
///
/// This will be used as an input device connected to RS-232 ports.
#[derive(Serialize, Debug)]
pub struct NonBlockingStdinReader {
    #[serde(skip)]
    rx: Receiver<u8>
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
