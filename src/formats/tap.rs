/*! **TAP** file format utilities.

[TAP_format](https://faqwiki.zxnet.co.uk/wiki/TAP_format)
```text
A flag byte: this is 0x00 for header blocks and 0xff for data blocks.
The actual data.
A checksum byte, calculated such that XORing all the data bytes together (including the flag byte) produces 0x00.
The structure of the 17 byte tape header is as follows.

Byte    Length  Description
---------------------------
0       1       Type (0,1,2 or 3)
1       10      Filename (padded with blanks)
11      2       Length of data block
13      2       Parameter 1
15      2       Parameter 2
These 17 bytes are prefixed by the flag byte (0x00) and suffixed by the checksum byte to produce the 19 byte block seen on tape. The type is 0,1,2 or 3 for a PROGRAM, Number array, Character array or CODE file. A SCREEN$ file is regarded as a CODE file with start address 16384 and length 6912 decimal. If the file is a PROGRAM file, parameter 1 holds the autostart line number (or a number >=32768 if no LINE parameter was given) and parameter 2 holds the start of the variable area relative to the start of the program. If it's a CODE file, parameter 1 holds the start of the code block when saved, and parameter 2 holds 32768. For data files finally, the byte at position 14 decimal holds the variable name.

For example, SAVE "ROM" CODE 0,2 will produce the following data on tape (the vertical bar denotes the break between blocks):

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
*/
#![allow(unused_imports)]
use core::borrow::Borrow;
use core::slice;
use core::ops::Deref;
use core::num::NonZeroU32;
use std::borrow::Cow;
use std::fmt;
use core::convert::TryFrom;
use std::io::{ErrorKind, Error, Read, Seek, SeekFrom, Result, Cursor, Take};
use super::read_ear::{PAUSE_PULSE_LENGTH, EarPulseIter};
/*
let tapfile = File.open()?;
tapfile.read_to_end(&mut buffer)?;
for chunk in TapChunkIter::from(&buffer) { TapChunk (deref to slice)
    print!("{}", chunk?) // chunk?.is_data()
}

let tapfile = File::open("some.tap")?;
let tap_reader = read_tap(tapfile);
let buf = Vec::new();
while tap_reader.next_chunk()? != 0 {
    buf.clear();
    tap_reader.read_to_end(&mut buf)?;
    let chunk = TapChunk::from(&buf);
    res.push(format!("{}", chunk));
}
for rd in tap::read_chunks(tapfile) {
    rd.read_to_end(&mut buf)?;
    let chunk = TapChunk::new_no_checksum(&buf)?
}

for rd in tap::read_chunks(tapfile) {
    let chunk_info = TapChunkInfo::new(rd)?
    print!("{}", chunk)
}
*/

const HEAD_BLOCK_FLAG: u8 = 0x00;
const DATA_BLOCK_FLAG: u8 = 0xFF;
const HEADER_SIZE: usize = 19;

/// Calculates bit-xor checksum from the given iterator of `u8`.
pub fn checksum<I: IntoIterator<Item=B>, B: Borrow<u8>>(iter: I) -> u8 {
    iter.into_iter().fold(0, |acc, x| acc ^ x.borrow())
}

/// Calculates bit-xor checksum from the given iterator of result of `u8`.
/// Usefull with [std::io::Bytes].
pub fn try_checksum<I: IntoIterator<Item=Result<u8>>>(iter: I) -> Result<u8> {
    iter.into_iter().try_fold(0, |acc, x| {
        match x {
            Ok(x) => Ok(acc ^ x),
            e @ Err(_) => e
        }
    })
}

/// Creates [TapChunkReader] from the given reader.
pub fn read_tap<T, R>(rd: T) -> TapChunkReader<R>
where T: Into<TapChunkReader<R>>
{
    rd.into()
}

/// Creates [TapReadInfoIter] from the given reader.
pub fn read_tap_info_iter<T, R>(rd: T) -> TapReadInfoIter<R>
where T: Into<TapReadInfoIter<R>>
{
    rd.into()
}

/// Creates [TapEarPulseIter] from the given reader.
pub fn read_tap_ear_pulse_iter<T, R>(rd: T) -> TapEarPulseIter<R>
where R: Read, T: Into<TapChunkReader<R>>
{
    let ep_iter = EarPulseIter::new(rd.into());
    TapEarPulseIter { ep_iter }
}

#[inline(always)]
fn array_name(c: u8) -> char {
    (c & 0b0001_1111 | 0b0100_0000).into()
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
    /// The type of the block following this header.
    pub block_type: BlockType,
    /// The name of this chunk.
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

/// Implements a [Reader][Read] of *TAP* chunks data.
///
/// Implements reader that reads only up to the size of the current *TAP* chunk.
#[derive(Debug)]
pub struct TapChunkReader<R> {
    next_pos: u64,
    inner: Take<R>,
}

/// Implements an iterator of [TapChunkInfo] over the [TapChunkReader].
#[derive(Debug)]
pub struct TapReadInfoIter<R> {
    inner: TapChunkReader<R>,
}

/// Implements an iterator of T-state pulse intervals over the [TapChunkReader].
/// See also: [EarPulseIter].
#[derive(Debug)]
pub struct TapEarPulseIter<R> {
    ep_iter: EarPulseIter<TapChunkReader<R>>
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
                write!(f, "{}: {}", header.block_type, header.name_str())
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

impl Header {
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
    fn tap_chunk_size(&self) -> usize {
        match self {
            TapChunkInfo::Head(_) => HEADER_SIZE,
            &TapChunkInfo::Data {length, ..} => length as usize + 2,
            &TapChunkInfo::Unknown {size, ..} => size as usize,
            TapChunkInfo::Empty => 0
        }
    }
}

impl<R: Read> TryFrom<&'_ mut Take<R>> for TapChunkInfo {
    type Error = Error;

    #[inline]
    fn try_from(rd: &mut Take<R>) -> Result<Self> {
        let limit = match rd.limit() {
            0 => {
                return Ok(TapChunkInfo::Empty)
            }
            limit if limit > u16::max_value().into() => {
                return Err(Error::new(ErrorKind::InvalidData, "Not a proper TAP chunk: too large"));
            }
            limit => limit
        };
        let mut flag: u8 = 0;
        rd.read_exact(slice::from_mut(&mut flag))?;
        if limit == 1 {
            return Ok(TapChunkInfo::Unknown { size: 1, flag })
        }
        match flag {
            HEAD_BLOCK_FLAG => {
                let mut header: [u8; HEADER_SIZE - 1] = Default::default();
                rd.read_exact(&mut header)?;
                if rd.limit() != 0 {
                    return Err(Error::new(ErrorKind::InvalidData, "Not a proper TAP header: invalid length"));
                }
                if checksum(&header) != flag {
                    return Err(Error::new(ErrorKind::InvalidData, "Not a proper TAP header: invalid checksum"));
                }
                Ok(TapChunkInfo::Head(Header::try_from(&header[..HEADER_SIZE - 2])?))
            }
            DATA_BLOCK_FLAG => {
                let checksum = try_checksum(rd.by_ref().bytes())? ^ flag;
                if rd.limit() != 0 {
                    return Err(Error::new(ErrorKind::InvalidData, "Not a proper TAP header: invalid length"));
                }
                Ok(TapChunkInfo::Data{ length: limit as u16 - 2, checksum })
            }
            flag => {
                // TODO: check length and read to eof
                Ok(TapChunkInfo::Unknown { size: limit as u16, flag })
            }
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
        match bytes.get(0..2) {
            Some(&[HEAD_BLOCK_FLAG, t]) if t & 3 == t => {
                if size != HEADER_SIZE {
                    return Err(Error::new(ErrorKind::InvalidData, "Not a proper TAP header: invalid length"));
                }
                if checksum(bytes) != 0 {
                    return Err(Error::new(ErrorKind::InvalidData, "Not a proper TAP header: invalid checksum"));
                }
                Ok(TapChunkInfo::Head(Header::try_from(&bytes[1..HEADER_SIZE-1])?))
            }
            Some(&[DATA_BLOCK_FLAG, _]) => {
                let checksum = checksum(bytes);
                Ok(TapChunkInfo::Data{ length: size as u16 - 2, checksum })
            }
            Some(&[flag, _]) => {
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
    /// Attempts to create [TapChunkInfo] from underlying data.
    pub fn info(&self) -> Result<TapChunkInfo> {
        TapChunkInfo::try_from(self.data.as_ref())
    }

    /// Calculates bit-xor checksum of underlying data.
    pub fn checksum(&self) -> u8 {
        checksum(self.data.as_ref())
    }

    /// Checks if this *TAP* chunk is a [Header] block.
    pub fn is_head(&self) -> bool {
        match self.data.as_ref().get(0..2) {
            Some(&[HEAD_BLOCK_FLAG, t]) if t & 3 == t => true,
            _ => false
        }
    }

    /// Checks if this *TAP* chunk is a data block.
    pub fn is_data(&self) -> bool {
        match self.data.as_ref().get(0) {
            Some(&DATA_BLOCK_FLAG) => true,
            _ => false
        }
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

    /// Returns a name if the underlying chunk is a [Header]
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

    /// Creates a pulse interval iterator from this *TAP* chunk.
    pub fn ear_pulse_iter(&self) -> EarPulseIter<Cursor<&[u8]>> {
        EarPulseIter::new(Cursor::new(self.as_ref()))
    }

    /// Converts this *TAP* chunk into a pulse interval iterator.
    pub fn into_ear_pulse_iter(self) -> EarPulseIter<Cursor<T>> {
        EarPulseIter::new(Cursor::new(self.into_inner()))
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

impl<R> TapChunkReader<R> {
    /// Returns the wrapped reader.
    pub fn into_inner(self) -> R {
        self.inner.into_inner()
    }
}

impl<R: Read> TapChunkReader<R> {
    /// Returns this chunk's remaining bytes limit to be read.
    pub fn chunk_limit(&self) -> u16 {
        self.inner.limit() as u16
    }

    /// Returns a reference to the chunk's [Take] reader.
    pub fn get_ref(&self) -> &Take<R> {
        &self.inner
    }

    /// Returns a mutable reference to the chunk's [Take] reader.
    pub fn get_mut(&mut self) -> &mut Take<R> {
        &mut self.inner
    }
}

impl<R: Read + Seek> TapChunkReader<R> {
    /// Forwards the inner reader to the position of a next *TAP* chunk.
    /// Returns `Ok(None)` if end of file has been reached.
    /// On success returns `Ok(size)` in bytes of the next *TAP* chunk
    /// and limits the inner [Take] reader to that size.
    pub fn next_chunk(&mut self) -> Result<Option<u16>> {
        let rd = self.inner.get_mut();
        if self.next_pos != rd.seek(SeekFrom::Start(self.next_pos))? {
            return Err(Error::new(ErrorKind::UnexpectedEof, "stream unexpectedly ended"));
        }

        let mut size: [u8; 2] = Default::default();
        loop {
            match rd.read(&mut size) {
                Ok(0) => {
                    self.inner.set_limit(0);
                    return Ok(None)
                },
                Ok(2) => break,
                Ok(_) => return Err(Error::new(ErrorKind::UnexpectedEof, "failed to fill whole buffer")),
                Err(ref e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            };
        }
        let size = u16::from_le_bytes(size);
        self.inner.set_limit(size as u64);
        self.next_pos += size as u64 + 2;
        Ok(Some(size))
    }

    /// Forwards the inner reader to the position of a next `skip` + 1 *TAP* chunks.
    /// Returns `Ok(None)` if end of file has been reached.
    /// On success returns `Ok(size)` in bytes of the next *TAP* chunk
    /// and limits the inner [Take] reader to that size.
    pub fn skip_chunks(&mut self, skip: usize) -> Result<Option<u16>> {
        for _ in 0..skip {
            if self.next_chunk()?.is_none() {
                return Ok(None)
            }
        }
        self.next_chunk()
    }

    /// Repositions the inner reader to the start of a file and sets inner limit to 0.
    /// To read the first chunk you need to call [TapChunkReader::next_chunk] first.
    pub fn rewind(&mut self) {
        self.inner.set_limit(0);
        self.next_pos = 0;
    }
}

impl<R: Read> Read for TapChunkReader<R> {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        self.inner.read(buf)
    }

    #[inline]
    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> Result<usize> {
        self.inner.read_to_end(buf)
    }
}

impl<R: Read + Seek> From<R> for TapChunkReader<R> {
    fn from(mut rd: R) -> Self {
        let next_pos = rd.seek(SeekFrom::Current(0)).unwrap();
        let inner = rd.take(0);
        TapChunkReader { next_pos, inner }
    }
}

impl<T, R> From<T> for TapReadInfoIter<R>
where T: Into<TapChunkReader<R>>
{
    #[inline]
    fn from(rd: T) -> Self {
        let inner = rd.into();
        TapReadInfoIter { inner }
    }
}

impl<R> TapReadInfoIter<R> {
    /// Returns the wrapped reader.
    #[inline]
    pub fn into_inner(self) -> R {
        self.inner.into_inner()
    }
}

impl<R: Read + Seek> Iterator for TapReadInfoIter<R> {
    type Item = Result<TapChunkInfo>;

    fn next(&mut self) -> Option<Self::Item> {
        let info = match self.inner.next_chunk() {
            Ok(None) => return None,
            Ok(_) => TapChunkInfo::try_from(self.inner.get_mut()),
            Err(e) => return Some(Err(e))
        };
        Some(info)
    }
}


impl<R: Read + Seek> From<TapChunkReader<R>> for TapEarPulseIter<R> {
    fn from(rd: TapChunkReader<R>) -> Self {
        let ep_iter = EarPulseIter::new(rd);
        TapEarPulseIter { ep_iter }        
    }
}

impl<R> TapEarPulseIter<R> {
    pub fn into_inner(self) -> TapChunkReader<R> {
        self.ep_iter.into_inner()
    }
}

impl<R: Read> TapEarPulseIter<R> {
    pub fn get_mut(&mut self) -> &mut TapChunkReader<R> {
        self.ep_iter.get_mut()
    }

    pub fn get_ref(&self) -> &TapChunkReader<R> {
        self.ep_iter.get_ref()
    }
}

impl<R: Read + Seek> Iterator for TapEarPulseIter<R> {
    type Item = NonZeroU32;

    fn next(&mut self) -> Option<Self::Item> {
        match self.ep_iter.next() {
            pulse @ Some(_) => pulse,
            None => {
                if self.ep_iter.err().is_some() ||
                        self.ep_iter.get_mut().next_chunk().is_err() {
                    return None;
                }
                self.ep_iter.reset();
                if self.ep_iter.is_done() {
                    None
                }
                else {
                    Some(PAUSE_PULSE_LENGTH)
                }
            }
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
        assert_eq!("Bytes: ROM       ", format!("{}", chunks[0]));
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
        let file = File::open("tests/read_tap_test.tap")?;
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
        assert_eq!("Program: HelloWorld", res[0]);
        assert_eq!("(data 6)", res[1]);
        assert_eq!("Number array: a(10)     ", res[2]);
        assert_eq!("(data 53)", res[3]);
        assert_eq!("Character array: weekdays  ", res[4]);
        assert_eq!("(data 26)", res[5]);

        tap_reader.rewind();
        let infos: Vec<_> = read_tap_info_iter(tap_reader).collect::<Result<Vec<_>>>()?;
        assert_eq!(6, infos.len());
        assert_eq!("Program: HelloWorld", format!("{}", infos[0]));
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
        assert_eq!("Number array: a(10)     ", format!("{}", infos[2]));
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
        assert_eq!("Character array: weekdays  ", format!("{}", infos[4]));
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
