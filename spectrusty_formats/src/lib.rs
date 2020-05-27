//! ZX Spectrum related various file format utilities.
// http://www.worldofspectrum.org/faq/reference/formats.htm
// http://www.worldofspectrum.org/TZXformat.html
use std::io;

pub mod ay;
pub mod mdr;
pub mod sna;
pub mod tap;
pub mod snapshot;
pub mod z80;
// pub mod tzx;

/// A trait that extends [io::Read] with methods that ease reading from chunked files.
pub trait ReadExactEx {
    /// Reads the exact number of bytes required to fill `buf` and returns `Ok(true)` or returns
    /// `Ok(false)` if exactly zero bytes were read. In this instance `buf` will be left unmodified.
    ///
    /// If at least one byte was read, this function behaves exactly like [io::Read::read_exact].
    fn read_exact_or_none(&mut self, buf: &mut [u8]) -> io::Result<bool>;
    /// Reads all bytes to fill `buf` or until EOF. If successful, returns the total number of bytes read.
    ///
    /// This function behaves like [io::Read::read_to_end] but it reads data into the mutable slice
    /// instead of into a Vec and stops reading when the whole `buf` has been filled.
    fn read_exact_or_to_end(&mut self, buf: &mut[u8]) -> io::Result<usize>;
}

impl<R: io::Read> ReadExactEx for R {
    fn read_exact_or_none(&mut self, buf: &mut [u8]) -> io::Result<bool> {
        let bytes_read = self.read_exact_or_to_end(buf)?;
        if bytes_read == 0 {
            Ok(false)
        }
        else if bytes_read == buf.len() {
            Ok(true)
        }
        else {
            Err(io::Error::new(io::ErrorKind::UnexpectedEof, "failed to fill whole buffer"))
        }
    }

    fn read_exact_or_to_end(&mut self, mut buf: &mut[u8]) -> io::Result<usize> {
        let orig_len = buf.len();
        while !buf.is_empty() {
            match self.read(buf) {
                Ok(0) => break,
                Ok(n) => buf = &mut buf[n..],
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        Ok(orig_len - buf.len())
    }
}
