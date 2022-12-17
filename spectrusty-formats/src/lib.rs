/*
    Copyright (C) 2020-2022  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    SPECTRUSTY is free software: you can redistribute it and/or modify it under
    the terms of the GNU Lesser General Public License (LGPL) as published
    by the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    SPECTRUSTY is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU Lesser General Public License for more details.

    You should have received a copy of the GNU Lesser General Public License
    along with this program.  If not, see <https://www.gnu.org/licenses/>.

    Author contact information: see Cargo.toml file, section [package.authors].
*/
//! ZX Spectrum related file format utilities.
use std::io::{self, Read, Write};

pub mod ay;
pub mod mdr;
pub mod sna;
pub mod tap;
pub mod snapshot;
pub mod scr;
pub mod z80;
// pub mod tzx;

/// A trait that extends [Read] with methods that ease reading from chunked files.
pub trait ReadExactEx: Read {
    /// Reads all bytes to fill `buf` or until EOF. If successful, returns the total number of bytes read.
    ///
    /// This function behaves like [Read::read_to_end] but it reads data into the mutable slice
    /// instead of into a Vec and stops reading when the whole `buf` has been filled.
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
    /// Reads the exact number of bytes required to fill `buf` and returns `Ok(true)` or returns
    /// `Ok(false)` if exactly zero bytes were read. In this instance, `buf` will be left unmodified.
    ///
    /// If at least one byte was read, this function behaves exactly like [Read::read_exact].
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
}

impl<R: Read> ReadExactEx for R {}

/// # Safety
/// This trait can be implemented safely only with packed structs that solely consist of
/// `u8` or array of `u8` primitives.
pub(crate) unsafe trait StructRead: Copy {
    fn read_new_struct<R: Read>(rd: R) -> io::Result<Self> where Self: Default {
        let mut obj = Self::default();
        obj.read_struct(rd)?;
        Ok(obj)
    }

    fn read_struct<R: Read>(&mut self, mut rd: R) -> io::Result<()> {
        rd.read_exact(unsafe { struct_slice_mut(self) })
    }

    fn read_struct_or_nothing<R: ReadExactEx>(&mut self, mut rd: R) -> io::Result<bool> {
        rd.read_exact_or_none(unsafe { struct_slice_mut(self) })
    }
    /// Reads the struct only up to the given `limit` bytes.
    ///
    /// # Panics
    /// Panics if `limit` is larger than the size of the struct.
    fn read_struct_with_limit<R: Read>(&mut self, mut rd: R, limit: usize) -> io::Result<()> {
        let slice = unsafe { struct_slice_mut(self) };
        rd.read_exact(&mut slice[0..limit])
    }
}

/// # Safety
/// This trait can be implemented safely only with packed structs that solely consist of
/// `u8` or array of `u8` primitives.
pub(crate) unsafe trait StructWrite: Copy {
    fn write_struct<W: Write>(&self, mut wr: W) -> io::Result<()> {
        let slice = unsafe { struct_slice_ref(self) };
        wr.write_all(slice)
    }

    fn write_struct_with_limit<W: Write>(&self, mut wr: W, limit: usize) -> io::Result<()> {
        let slice = unsafe { struct_slice_ref(self) };
        wr.write_all(&slice[0..limit])
    }
}

/// # Safety
/// This function can be used safely only with packed structs that solely consist of
/// `u8` or array of `u8` primitives.
unsafe fn struct_slice_mut<T: Copy>(obj: &mut T) -> &mut [u8] {
    let len = core::mem::size_of::<T>() / core::mem::size_of::<u8>();
    core::slice::from_raw_parts_mut(obj as *mut T as *mut u8, len)
}

/// # Safety
/// This function can be used safely only with packed structs that solely consist of
/// `u8` or array of `u8` primitives.
unsafe fn struct_slice_ref<T: Copy>(obj: &T) -> &[u8] {
    let len = core::mem::size_of::<T>() / core::mem::size_of::<u8>();
    core::slice::from_raw_parts(obj as *const T as *const u8, len)
}
