/*
    Copyright (C) 2020-2022  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
/*! **TAP** file format utilities.

# TAP format

A **TAP** file consists of blocks of data each prepended by a 2 byte (LSB) block length indicator.
Those blocks will be referenced here as *TAP chunks*.

The standard Spectrum's ROM *TAPE* routine produces 2 kinds of blocks:

* a [header][Header] block
* a data block

This is determined by the first byte of each block, here called a `flag` byte.

A flag byte is `0x00` for header blocks and `0xff` for data blocks.
After the flag byte, the actual data follows, after which a checksum byte, calculated such that
XORing all the data bytes together (including the flag byte) produces `0`.

The structure of the 17 byte header is as follows.

| offset | size | description                      |
|--------|------|----------------------------------|
|    0   |    1 | type (0,1,2,3)                   |
|    1   |   10 | filename (padded with space)     |
|   11   |    2 | length of data block (LSB first) |
|   13   |    2 | parameter 1 (LSB first)          |
|   15   |    2 | parameter 2 (LSB first)          |

These 17 bytes are prefixed by the flag byte (0x00) and suffixed by the checksum byte to produce
the 19-byte block seen on tape. The type is 0,1,2 or 3 for a `PROGRAM`, `Number array`, `Character array`,
or `CODE` file. A `SCREEN$` file is regarded as a `CODE` file with start address 16384 and length 6912 decimal.
If the file is a `PROGRAM` file, parameter 1 holds the autostart line number (or a number >=32768 if
no LINE parameter was given) and parameter 2 holds the start of the variable area relative to the start
of the program. If it's a `CODE` file, parameter 1 holds the start of the code block when saved, and
parameter 2 holds 32768. For array files finally, the byte at position 14 decimal holds the variable name.

For example, SAVE "ROM" CODE 0,2 will produce the following data on tape:

```text
      |-------------- TAP chunk -------------|       |TAP chunk|
13 00 00 03 52 4f 4d 7x20 02 00 00 00 00 80 f1 04 00 ff f3 af a3
^^^^^...... first block is 19 bytes (17 bytes+flag+checksum)
      ^^... flag byte (A reg, 00 for headers, ff for data blocks)
         ^^ first byte of header, indicating a code block
file name ..^^^^^^^^^^^^^
header info ..............^^^^^^^^^^^^^^^^^
checksum of header .........................^^
length of second block ........................^^^^^
flag byte ...........................................^^
first two bytes of rom .................................^^^^^
checksum (checkbittoggle would be a better name!).............^^
```

The above text uses material from the ["TAP format"](https://sinclair.wiki.zxnet.co.uk/wiki/TAP_format) article
on the [Sinclair FAQ wiki](https://sinclair.wiki.zxnet.co.uk/) and is released under the
[Creative Commons Attribution-Share Alike License](https://creativecommons.org/licenses/by-sa/3.0/).


## Interpreting *TAP* files

### Reading byte containers

To interpret a byte container as a *TAP chunk* the [TapChunk] wrapper is provided.
The [TapChunkIter] iterator can be used to produce [TapChunk]s over the byte container in the
*TAP* format.

```no_run
use std::io::Read;
use spectrusty_formats::tap::*;

let mut tapfile = std::fs::File::open("some.tap")?;
let mut buffer = Vec::new();
tapfile.read_to_end(&mut buffer)?;
for chunk in TapChunkIter::from(&buffer) {
    // do something with chunk
    println!("{}", chunk);
}
# Ok::<(), std::io::Error>(())
```

### Directly from readers

[TapChunkReader] helps to read *TAP* chunks directly from a byte stream that implements both [Read]
and [Seek] interfaces.

```no_run
use std::io::Read;
use spectrusty_formats::tap::*;

let mut tapfile = std::fs::File::open("some.tap")?;
let mut tap_reader = read_tap(tapfile);

let mut buf = Vec::new();
while let Some(size) = tap_reader.next_chunk()? {
    buf.clear();
    tap_reader.read_to_end(&mut buf)?;
    let chunk = TapChunk::from(&buf);
    // do something with chunk
    println!("{}", chunk);
}
# Ok::<(), std::io::Error>(())
```

### Browsing *TAP* chunks

A [TapChunkInfo] is an enum providing information about the chunk and can be created from both
byte containers as well as from [std::io::Take] readers using the [TryFrom] interface.

An [iterator][TapReadInfoIter] producing [TapChunkInfo] can be created [from][From] [TapChunkReader].

```no_run
use spectrusty_formats::tap::*;

let mut tapfile = std::fs::File::open("some.tap")?;
let mut tap_reader = read_tap(tapfile);
for info in TapReadInfoIter::from(&mut tap_reader) {
    println!("{}", info?);
}
# Ok::<(), std::io::Error>(())
```

### *TAPE* pulses

To provide a *TAPE* signal for the Spectrum emulator a pulse interval [encoder][pulse::ReadEncPulseIter]
is provided. It encodes data as *TAPE* pulse intervals providing results via the [Iterator] interface
while reading bytes from the underlying [reader][std::io::Read].

[pulse::ReadEncPulseIter] emits lead pulses following the sync and data pulses for the bytes read 
until the reader reaches the end of file or the [pulse::ReadEncPulseIter::reset] method is called.

For a more convenient way to encode *TAP* data which contains many chunks, the [TapChunkPulseIter] is provided.
It wraps [pulse::ReadEncPulseIter] providing it with the chunk data resetting it before each next chunk.

```no_run
use spectrusty::{memory::Memory48k, chip::{ula::UlaPAL, EarIn}};

let mut ula = UlaPAL::<Memory48k>::default();
//...
use spectrusty_formats::tap::*;

let mut tapfile = std::fs::File::open("some.tap")?;
let mut pulse_iter = read_tap_pulse_iter(tapfile);

// feed the buffer fragmentarily before each emulated frame
ula.feed_ear_in(&mut pulse_iter, Some(1));

// ... or feed the buffer at once ...
ula.feed_ear_in(pulse_iter, None);
# Ok::<(), std::io::Error>(())
```

## Writing data to *TAP* files

The structure of *TAP* files allows us to easily append more blocks to them.

[TapChunkWriter] provides methods to write *TAP chunks* to streams implementing [Write] and [Seek] interfaces.

```no_run
use std::io::Write;
use spectrusty_formats::tap::*;

let mut tapfile = std::fs::File::create("output.tap")?;
let mut tap_writer = write_tap(tapfile)?;

// let's create a TAP header for a CODE block
let header = Header::new_code(1)
                    .with_start(0x8000)
                    .with_name("return");
// now write it
tap_writer.write_header(&header)?;

// now the data block
let mut tran = tap_writer.begin()?;
tran.write(&[DATA_BLOCK_FLAG, 201])?;
// appends checksum byte and commits block
tran.commit(true)?;
# Ok::<(), std::io::Error>(())
```

### *TAPE* pulses

A *TAPE* signal generated by the Spectrum's SAVE routines can be written as *TAP* chunk data
with the same instance of [TapChunkWriter].

```no_run
use spectrusty::{memory::Memory48k, chip::{ula::UlaPAL, MicOut}};

let mut ula = UlaPAL::<Memory48k>::default();
//...
use std::io::Write;
use spectrusty_formats::tap::*;

let mut tapfile = std::fs::File::create("output.tap")?;
let mut tap_writer = write_tap(tapfile)?;

// get pulses from MIC out data after rendering each frame
let pulse_iter = ula.mic_out_pulse_iter();
// write them as *TAP chunks*
let chunks = tap_writer.write_pulses_as_tap_chunks(pulse_iter)?;
if chunks != 0 {
    println!("Saved: {} chunks", chunks);
}
# Ok::<(), std::io::Error>(())
```
*/
use core::borrow::Borrow;
use std::borrow::Cow;
use std::fmt;
use core::convert::TryFrom;
use std::io::{ErrorKind, Error, Read, Write, Seek, Result, Cursor};

use pulse::ReadEncPulseIter;

pub mod pulse;
mod read;
mod write;
pub use read::*;
pub use write::*;

pub const HEAD_BLOCK_FLAG: u8 = 0x00;
pub const DATA_BLOCK_FLAG: u8 = 0xFF;
pub const HEADER_SIZE: usize = 19;

/// Calculates bit toggle checksum from the given iterator of `u8`.
pub fn checksum<I: IntoIterator<Item=B>, B: Borrow<u8>>(iter: I) -> u8 {
    iter.into_iter().fold(0, |acc, x| acc ^ x.borrow())
}

/// Calculates bit toggle checksum from the given iterator of the result of `u8`.
/// Useful with [std::io::Bytes].
pub fn try_checksum<I: IntoIterator<Item=Result<u8>>>(iter: I) -> Result<u8> {
    iter.into_iter().try_fold(0, |acc, x| {
        match x {
            Ok(x) => Ok(acc ^ x),
            e @ Err(_) => e
        }
    })
}

/// Creates an instance of [TapChunkReader] from the given reader.
pub fn read_tap<T, R>(rd: T) -> TapChunkReader<R>
    where T: Into<TapChunkReader<R>>
{
    rd.into()
}

/// Creates an instance of [TapChunkPulseIter] from the given reader.
pub fn read_tap_pulse_iter<T, R>(rd: T) -> TapChunkPulseIter<R>
    where R: Read + Seek, T: Into<TapChunkReader<R>>
{
    let ep_iter = ReadEncPulseIter::new(rd.into());
    TapChunkPulseIter::from(ep_iter)
}

/// Creates an instance of [TapChunkWriter] from the given writer on success.
pub fn write_tap<W>(wr: W) -> Result<TapChunkWriter<W>>
    where W: Write + Seek
{
    TapChunkWriter::try_new(wr)
}

/// The *TAP* block type of the next chunk following a [Header].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum BlockType {
    Program     = 0,
    NumberArray = 1,
    CharArray   = 2,
    Code        = 3
}

/// Represents the *TAP* header block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    /// Length of the data block excluding a block flag and checksum byte.
    pub length: u16,
    /// The type of the file this header represents.
    pub block_type: BlockType,
    /// A name of the file.
    pub name: [u8;10],
    /// Additional header data.
    pub par1: [u8;2],
    /// Additional header data.
    pub par2: [u8;2]
}

/// The *TAP* chunk meta-data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TapChunkInfo {
    /// Represents a proper header block.
    Head(Header),
    /// Represents a data block.
    Data {
        /// The length of data excluding a block flag and checksum byte.
        length: u16,
        /// Checksum of the data, should be 0. Otherwise this block won't load properly.
        checksum: u8
    },
    /// Represents an unkown block.
    Unknown {
        /// The size of the whole block including the block flag.
        size: u16,
        /// The first byte of the block (a block flag).
        flag: u8
    },
    /// Represents an empty block.
    Empty
}

/// The *TAP* chunk.
///
/// Provides helper methods to interpret the underlying bytes as one of the *TAP* blocks.
/// This should usually be a [Header] or a data block.
///
/// Anything that implements `AsRef<[u8]>` can be used as `T` (e.g. `&[u8]` or `Vec<u8>`).
///
/// Instances of [TapChunk] can be created using [From]/[Into] interface or via [TapChunkIter].
#[derive(Clone, Copy, Debug)]
pub struct TapChunk<T> {
    data: T
}

/// Implements an iterator of [TapChunk] objects over the array of bytes.
#[derive(Clone, Debug)]
pub struct TapChunkIter<'a> {
    position: usize,
    data: &'a[u8]
}

#[inline(always)]
pub(super) fn array_name(c: u8) -> char {
    (c & 0b0001_1111 | 0b0100_0000).into()
}

#[inline(always)]
pub(super) fn char_array_var(n: char) -> u8 {
    if !n.is_ascii_alphabetic() {
        panic!("Only ascii alphabetic characters are allowed!");
    }
    let c = n as u8;
    c & 0b0001_1111 | 0b1100_0000
}

#[inline(always)]
pub(super) fn number_array_var(n: char) -> u8 {
    if !n.is_ascii_alphabetic() {
        panic!("Only ascii alphabetic characters are allowed!");
    }
    let c = n as u8;
    c & 0b0001_1111 | 0b1000_0000
}

impl fmt::Display for BlockType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", 
            match self {
                BlockType::Program => "Program",
                BlockType::NumberArray => "Number array",
                BlockType::CharArray => "Character array",
                BlockType::Code => "Bytes",
            })
    }
}

impl fmt::Display for TapChunkInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TapChunkInfo::Head(header) => {
                write!(f, "{}: \"{}\"", header.block_type, header.name_str().trim_end())?;
                match header.block_type {
                    BlockType::Program => {
                        if header.start() < 10000 {
                            write!(f, " LINE {}", header.start())?;
                        }
                        if header.vars() != header.length {
                            write!(f, " PROG {} VARS {}",
                                header.vars(), header.length - header.vars())?;
                        }
                        Ok(())
                    }
                    BlockType::NumberArray => {
                        write!(f, " DATA {}()", header.array_name())
                    }
                    BlockType::CharArray => {
                        write!(f, " DATA {}$()", header.array_name())
                    }
                    BlockType::Code => {
                        write!(f, " CODE {},{}", header.start(), header.length)
                    }
                }
            }
            TapChunkInfo::Data {length, ..} => {
                write!(f, "(data {})", length)
            }
            TapChunkInfo::Unknown {size, ..} => {
                write!(f, "(unkown {})", size)
            }
            TapChunkInfo::Empty => {
                write!(f, "(empty)")
            }            
        }
    }
}

impl<T> fmt::Display for TapChunk<T> where T: AsRef<[u8]> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.info() {
            Ok(info) => info.fmt(f),
            Err(e) => write!(f, "{}", e)
        }
    }
}

impl Default for BlockType {
    fn default() -> Self {
        BlockType::Code
    }
}

impl From<BlockType> for u8 {
    #[inline]
    fn from(block_type: BlockType) -> u8 {
        block_type as u8
    }
}

impl TryFrom<u8> for BlockType {
    type Error = Error;

    #[inline]
    fn try_from(block_type: u8) -> Result<Self> {
        match block_type {
            0 => Ok(BlockType::Program),
            1 => Ok(BlockType::NumberArray),
            2 => Ok(BlockType::CharArray),
            3 => Ok(BlockType::Code),
            _ => Err(Error::new(ErrorKind::InvalidData, "Unknown TAP type."))
        }
    }    
}

impl Default for Header {
    fn default() -> Self {
        let length = 0;
        let block_type = BlockType::Code;
        let name = [b' ';10];
        let par1 = 0u16.to_le_bytes();
        let par2 = 0x8000u16.to_le_bytes();
        Header { length, block_type, name, par1, par2 }
    }
}

impl Header {
    /// Creates a `Code` header.
    pub fn new_code(length: u16) -> Self {
        let block_type = BlockType::Code;
        let name = [b' ';10];
        let par1 = 0u16.to_le_bytes();
        let par2 = 0x8000u16.to_le_bytes();
        Header { length, block_type, name, par1, par2 }
    }
    /// Creates a `Program` header.
    pub fn new_program(length: u16) -> Self {
        let block_type = BlockType::Program;
        let name = [b' ';10];
        let par1 = 0x8000u16.to_le_bytes();
        let par2 = length.to_le_bytes();
        Header { length, block_type, name, par1, par2 }
    }
    /// Creates a `NumberArray` header.
    pub fn new_number_array(length: u16) -> Self {
        let block_type = BlockType::NumberArray;
        let name = [b' ';10];
        let par1 = [0, number_array_var('A')];
        let par2 = 0x8000u16.to_le_bytes();
        Header { length, block_type, name, par1, par2 }
    }
    /// Creates a `CharArray` header.
    pub fn new_char_array(length: u16) -> Self {
        let block_type = BlockType::CharArray;
        let name = [b' ';10];
        let par1 = [0, char_array_var('A')];
        let par2 = 0x8000u16.to_le_bytes();
        Header { length, block_type, name, par1, par2 }
    }
    /// Changes `name`, builder style.
    pub fn with_name<S: AsRef<[u8]>>(mut self, name: S) -> Self {
        let name = name.as_ref();
        let bname = &name[0..name.len().max(10)];
        self.name[0..bname.len()].copy_from_slice(bname);
        for p in self.name[bname.len()..].iter_mut() {
            *p = b' ';
        }
        self
    }
    /// Changes `start`, builder style.
    ///
    /// # Panics
    /// Panics if self is not a `Code` nor a `Program` header.
    pub fn with_start(mut self, start: u16) -> Self {
        if let BlockType::Code|BlockType::Program = self.block_type {
            self.par1 = start.to_le_bytes();            
            self
        }
        else {
            panic!("Can't set start for an array header");
        }
    }
    /// Changes `vars`, builder style.
    ///
    /// # Panics
    /// Panics if self is not a `Program` header or if given `vars` exceeds `length`.
    pub fn with_vars(mut self, vars: u16) -> Self {
        if let BlockType::Program = self.block_type {
            if vars <= self.length {
                self.par2 = vars.to_le_bytes();            
                self
            }
            else {
                panic!("Can't set vars larger than length");
            }
        }
        else {
            panic!("Can't set vars: not a program header");
        }
    }
    /// Changes array name, builder style.
    ///
    /// # Panics
    /// Panics if self is not an array header or if given character is not ASCII alphabetic.
    pub fn with_array_name(mut self, n: char) -> Self {
        self.par1[1] = match self.block_type {
            BlockType::CharArray => char_array_var(n),
            BlockType::NumberArray => number_array_var(n),
            _ => panic!("Can't set array name: not an array header")
        };
        self
    }
    /// Returns a header name as a string.
    #[inline]
    pub fn name_str(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(&self.name)
    }

    /// Returns a starting address.
    /// Depending of the type of this header it may be either a starting address of [BlockType::Code]
    /// or starting line of [BlockType::Program].
    #[inline]
    pub fn start(&self) -> u16 {
        u16::from_le_bytes(self.par1)
    }

    /// Returns an offset to `VARS`. Only valid for headers with [BlockType::Program].
    #[inline]
    pub fn vars(&self) -> u16 {
        u16::from_le_bytes(self.par2)
    }

    /// Returns an array variable name.
    ///
    /// Only valid for headers with [BlockType::CharArray] or [BlockType::NumberArray].
    #[inline]
    pub fn array_name(&self) -> char {
        array_name(self.par1[1])
    }

    /// Returns a tap chunk created from self.
    pub fn to_tap_chunk(&self) -> TapChunk<[u8;HEADER_SIZE]> {
        let mut buffer = <[u8;HEADER_SIZE]>::default();
        buffer[0] = HEAD_BLOCK_FLAG;
        buffer[1] = self.block_type.into();
        buffer[2..12].copy_from_slice(&self.name);
        buffer[12..14].copy_from_slice(&self.length.to_le_bytes());
        buffer[14..16].copy_from_slice(&self.par1);
        buffer[16..18].copy_from_slice(&self.par2);
        buffer[18] = checksum(buffer[0..18].iter());
        TapChunk::from(buffer)
    }
}

impl TryFrom<&'_[u8]> for Header {
    type Error = Error;
    fn try_from(header: &[u8]) -> Result<Self> {
        if header.len() != HEADER_SIZE - 2 {
            return Err(Error::new(ErrorKind::InvalidData, "Not a proper TAP header: invalid length"));
        }
        let block_type = BlockType::try_from(header[0])?;
        let mut name: [u8; 10] = Default::default();
        name.copy_from_slice(&header[1..11]);
        let mut length: [u8; 2] = Default::default();
        length.copy_from_slice(&header[11..13]);
        let length = u16::from_le_bytes(length);
        let mut par1: [u8; 2] = Default::default();
        par1.copy_from_slice(&header[13..15]);
        let mut par2: [u8; 2] = Default::default();
        par2.copy_from_slice(&header[15..17]);
        Ok(Header { length, block_type, name, par1, par2 })
    }
}

impl TapChunkInfo {
    /// Returns size in bytes of this chunk.
    pub fn tap_chunk_size(&self) -> usize {
        match self {
            TapChunkInfo::Head(_) => HEADER_SIZE,
            &TapChunkInfo::Data {length, ..} => length as usize + 2,
            &TapChunkInfo::Unknown {size, ..} => size as usize,
            TapChunkInfo::Empty => 0
        }
    }
}

impl TryFrom<&'_[u8]> for TapChunkInfo {
    type Error = Error;

    #[inline]
    fn try_from(bytes: &[u8]) -> Result<Self> {
        let size = match bytes.len() {
            0 => {
                return Ok(TapChunkInfo::Empty);
            }
            1 => {
                return Ok(TapChunkInfo::Unknown { size: 1, flag: bytes[0] })
            }
            size if size > u16::max_value().into() => {
                return Err(Error::new(ErrorKind::InvalidData, "Not a proper TAP chunk: too large"));
            }
            size => size
        };
        match bytes.first() {
            Some(&HEAD_BLOCK_FLAG) if size == HEADER_SIZE && checksum(bytes) == 0 => {
                Header::try_from(&bytes[1..HEADER_SIZE-1])
                .map(TapChunkInfo::Head)
                .or(Ok(TapChunkInfo::Unknown { size: size as u16, flag: HEAD_BLOCK_FLAG }))
            }
            Some(&DATA_BLOCK_FLAG) => {
                let checksum = checksum(bytes);
                Ok(TapChunkInfo::Data{ length: size as u16 - 2, checksum })
            }
            Some(&flag) => {
                Ok(TapChunkInfo::Unknown { size: size as u16, flag })
            }
            _ => unreachable!()
        }
    }    
}

impl<T> From<T> for TapChunk<T> where T: AsRef<[u8]> {
    fn from(data: T) -> Self {
        TapChunk { data }
    }
}

impl<T> AsRef<[u8]> for TapChunk<T> where T: AsRef<[u8]> {
    #[inline(always)]
    fn as_ref(&self) -> &[u8] {
        self.data.as_ref()
    }
}

impl<T> AsMut<[u8]> for TapChunk<T> where T: AsMut<[u8]> {
    #[inline(always)]
    fn as_mut(&mut self) -> &mut [u8] {
        self.data.as_mut()
    }
}

impl<T> TapChunk<T> {
    /// Returns the underlying bytes container.
    pub fn into_inner(self) -> T {
        self.data
    }
}

impl<T> TapChunk<T> where T: AsRef<[u8]> {
    /// Attempts to create an instance of [TapChunkInfo] from underlying data.
    pub fn info(&self) -> Result<TapChunkInfo> {
        TapChunkInfo::try_from(self.data.as_ref())
    }

    /// Calculates bit-xor checksum of underlying data.
    pub fn checksum(&self) -> u8 {
        checksum(self.data.as_ref())
    }

    /// Checks if this *TAP* chunk is a [Header] block.
    pub fn is_head(&self) -> bool {
        matches!(self.data.as_ref().get(0..2),
                 Some(&[HEAD_BLOCK_FLAG, t]) if t & 3 == t)
    }

    /// Checks if this *TAP* chunk is a data block.
    pub fn is_data(&self) -> bool {
        matches!(self.data.as_ref().first(), Some(&DATA_BLOCK_FLAG))
    }

    /// Checks if this *TAP* chunk is a valid data or [Header] block.
    pub fn is_valid(&self) -> bool {
        self.validate().is_ok()
    }

    /// Validates if this *TAP* chunk is a valid data or [Header] block returning `self` on success.
    pub fn validated(self) -> Result<Self> {
        self.validate().map(|_| self)
    }

    /// Validates if this *TAP* chunk is a valid data or [Header] block.
    pub fn validate(&self) -> Result<()> {
        let data = self.data.as_ref();
        match data.get(0..2) {
            Some(&[HEAD_BLOCK_FLAG, t]) if t & 3 == t => {
                if data.len() != HEADER_SIZE {
                    return Err(Error::new(ErrorKind::InvalidData, "Not a proper TAP header: invalid length"));
                }
            }
            Some(&[DATA_BLOCK_FLAG, _]) => {}
            _ => return Err(Error::new(ErrorKind::InvalidData, "Not a proper TAP chunk: Invalid block head byte"))
        }
        if checksum(data) != 0 {
            return Err(Error::new(ErrorKind::InvalidData, "Not a proper TAP chunk: Invalid checksum"))
        }
        Ok(())
    }

    /// Returns a name if the block is a [Header]
    pub fn name(&self) -> Option<Cow<'_,str>> {
        if self.is_head() {
            Some(String::from_utf8_lossy(&self.data.as_ref()[2..12]))
        }
        else {
            None
        }
    }

    /// Returns a reference to the underlying data block only if the underlying bytes represents the data block.
    ///
    /// The provided reference does not include block flag and checksum bytes.
    pub fn data(&self) -> Option<&[u8]> {
        let data = self.data.as_ref();
        if self.is_data() {
            Some(&data[1..data.len()-1])
        }
        else {
            None
        }
    }

    /// Returns a length in bytes of the next chunk's data block only if the underlying bytes represents
    /// the [Header] block.
    ///
    /// The provided length does not include block flag and checksum bytes.
    pub fn data_block_len(&self) -> Option<u16> {
        if self.is_head() {
            if let [lo, hi] = self.data.as_ref()[12..14] {
                return Some(u16::from_le_bytes([lo, hi]))
            }
        }
        None
    }
    /// Returns a starting address only if the underlying bytes represents the [Header] block.
    ///
    /// Depending of the type of this header it may be either a starting address of [BlockType::Code]
    /// or starting line of [BlockType::Program].
    pub fn start(&self) -> Option<u16> {
        if self.is_head() {
            if let [lo, hi] = self.data.as_ref()[14..16] {
                return Some(u16::from_le_bytes([lo, hi]))
            }
        }
        None
    }

    /// Returns an offset to `VARS` only if the underlying bytes represents the [Header] block.
    ///
    /// Only valid for headers with [BlockType::Program].
    pub fn vars(&self) -> Option<u16> {
        if self.is_head() {
            if let [lo, hi] = self.data.as_ref()[16..18] {
                return Some(u16::from_le_bytes([lo, hi]))
            }
        }
        None
    }

    /// Returns an array variable name only if the underlying bytes represents the [Header] block.
    ///
    /// Only valid for headers with [BlockType::CharArray] or [BlockType::NumberArray].
    pub fn array_name(&self) -> Option<char> {
        if self.is_head() {
            return Some(array_name(self.data.as_ref()[15]))
        }
        None
    }

    /// Returns a next chunk's data type only if the underlying bytes represents the [Header] block.
    pub fn block_type(&self) -> Option<BlockType> {
        if self.is_head() {
            return BlockType::try_from(self.data.as_ref()[1]).ok()
        }
        None
    }

    /// Returns a pulse interval iterator referencing this *TAP* chunk.
    pub fn as_pulse_iter(&self) -> ReadEncPulseIter<Cursor<&[u8]>> {
        ReadEncPulseIter::new(Cursor::new(self.as_ref()))
    }

    /// Converts this *TAP* chunk into a pulse interval iterator owning this chunk's data.
    pub fn into_pulse_iter(self) -> ReadEncPulseIter<Cursor<T>> {
        ReadEncPulseIter::new(Cursor::new(self.into_inner()))
    }

    /// Creates a new TapChunk with a specified storage, possibly cloning the data
    /// unless the target storage can be converted to without cloning,
    /// e.g. `Vec<u8> <=> Box<[u8]>`.
    pub fn with_storage<D>(self) -> TapChunk<D>
    where D: From<T> + AsRef<[u8]>
    {
        let data = D::from(self.into_inner());
        TapChunk::<D>::from(data)
    }
}


impl<'a, T> From<&'a T> for TapChunkIter<'a> where T: AsRef<[u8]> {
    fn from(data: &'a T) -> Self {
        let data = data.as_ref();
        TapChunkIter { position: 0 , data }
    }
}

impl<'a> Iterator for TapChunkIter<'a> {
    type Item = TapChunk<&'a[u8]>;

    fn next(&mut self) -> Option<Self::Item> {
        let data = self.data;
        let mut position = self.position;
        match data.get(position..position + 2) {
            Some(&[lo, hi]) => {
                let length = u16::from_le_bytes([lo, hi]) as usize;
                position += 2;
                let pos_end = position + length;
                self.position = pos_end;
                data.get(position..pos_end).map(TapChunk::from)
            }
            _ => None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use smallvec::SmallVec;

    #[test]
    fn slice_tap_works() {
        let chunks: Vec<_> = {
            let bytes = [0x13,0x00,0x00,0x03,0x52,0x4f,0x4d,0x20,0x20,0x20,0x20,0x20,0x20,0x20,0x02,0x00,0x00,0x00,0x00,0x80,0xf1,0x04,0x00,0xff,0xf3,0xaf,0xa3];
            for chunk in TapChunkIter::from(&bytes) {
                assert_eq!(true, chunk.is_valid());
            }
            TapChunkIter::from(&bytes).map(
                TapChunk::with_storage::<SmallVec<[u8;20]>>
            ).collect()
        };
        assert_eq!(2, chunks.len());
        assert_eq!("Bytes: \"ROM\" CODE 0,2", format!("{}", chunks[0]));
        assert_eq!(true, chunks[0].is_head());
        assert_eq!(false, chunks[0].is_data());
        assert_eq!(Some(Cow::Borrowed("ROM       ")), chunks[0].name());
        assert_eq!(None, chunks[0].data());
        assert_eq!(Some(2), chunks[0].data_block_len());
        assert_eq!(Some(0), chunks[0].start());
        assert_eq!(Some(32768), chunks[0].vars());
        assert_eq!(Some('@'), chunks[0].array_name());
        assert_eq!(Some(BlockType::Code), chunks[0].block_type());
        assert_eq!(0, chunks[0].checksum());
        assert!(match chunks[0].info() {
            Ok(TapChunkInfo::Head(Header {
                length: 2,
                block_type: BlockType::Code,
                name: [0x52,0x4f,0x4d,0x20,0x20,0x20,0x20,0x20,0x20,0x20],
                par1: [0x00,0x00],
                par2: [0x00,0x80]
            })) => true,
            _ => false
        });
        assert_eq!("(data 2)", format!("{}", chunks[1]));
        assert_eq!(false, chunks[1].is_head());
        assert_eq!(true, chunks[1].is_data());
        assert_eq!(None, chunks[1].name());
        assert_eq!(Some(&[0xf3,0xaf][..]), chunks[1].data());
        assert_eq!(None, chunks[1].data_block_len());
        assert_eq!(None, chunks[1].start());
        assert_eq!(None, chunks[1].vars());
        assert_eq!(None, chunks[1].array_name());
        assert_eq!(None, chunks[1].block_type());
        assert_eq!(0, chunks[1].checksum());
        assert!(match chunks[1].info() {
            Ok(TapChunkInfo::Data {
                length: 2,
                checksum: 0
            }) => true,
            _ => false
        });
    }

    #[test]
    fn read_tap_works() -> Result<()> {
        let file = File::open("../resources/read_tap_test.tap")?;
        let mut tap_reader = read_tap(file);
        let mut buf = Vec::new();
        let mut res = Vec::new();
        while tap_reader.next_chunk()?.is_some() {
            buf.clear();
            tap_reader.read_to_end(&mut buf)?;
            let chunk = TapChunk::from(&buf);
            res.push(format!("{}", chunk));
        }
        assert_eq!(res.len(), 6);
        assert_eq!("Program: \"HelloWorld\" LINE 10", res[0]);
        assert_eq!("(data 6)", res[1]);
        assert_eq!("Number array: \"a(10)\" DATA A()", res[2]);
        assert_eq!("(data 53)", res[3]);
        assert_eq!("Character array: \"weekdays\" DATA W$()", res[4]);
        assert_eq!("(data 26)", res[5]);

        tap_reader.rewind();
        let infos: Vec<_> = TapReadInfoIter::from(&mut tap_reader).collect::<Result<Vec<_>>>()?;
        assert_eq!(6, infos.len());
        assert_eq!("Program: \"HelloWorld\" LINE 10", format!("{}", infos[0]));
        if let TapChunkInfo::Head(header) = infos[0] {
            assert_eq!("HelloWorld", header.name_str());
            assert_eq!(6, header.length);
            assert_eq!(10, header.start());
            assert_eq!(6, header.vars());
        }
        else {
            unreachable!()
        }
        assert_eq!(19, infos[0].tap_chunk_size());
        assert_eq!("(data 6)", format!("{}", infos[1]));
        assert_eq!(8, infos[1].tap_chunk_size());
        assert_eq!("Number array: \"a(10)\" DATA A()", format!("{}", infos[2]));
        if let TapChunkInfo::Head(header) = infos[2] {
            assert_eq!("a(10)     ", header.name_str());
            assert_eq!(53, header.length);
            assert_eq!('A', header.array_name());
        }
        else {
            unreachable!()
        }
        assert_eq!(19, infos[2].tap_chunk_size());
        assert_eq!("(data 53)", format!("{}", infos[3]));
        assert_eq!(55, infos[3].tap_chunk_size());
        assert_eq!("Character array: \"weekdays\" DATA W$()", format!("{}", infos[4]));
        if let TapChunkInfo::Head(header) = infos[4] {
            assert_eq!("weekdays  ", header.name_str());
            assert_eq!(26, header.length);
            assert_eq!('W', header.array_name());
        }
        else {
            unreachable!()
        }
        assert_eq!(19, infos[4].tap_chunk_size());
        assert_eq!("(data 26)", format!("{}", infos[5]));
        assert_eq!(28, infos[5].tap_chunk_size());
        Ok(())
    }
}
