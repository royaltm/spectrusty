use core::num::NonZeroU32;
use std::borrow::Cow;
use std::convert::TryFrom;
use std::io::{ErrorKind, Error, Read, Result, Cursor, Take};
use super::read_ear::{PAUSE_PULSE_LENGTH, EarPulseIter};
/*
https://faqwiki.zxnet.co.uk/wiki/TAP_format

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

      |------ Spectrum-generated data -------|       |---------|
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
*/
const HEAD_BLOCK_FLAG: u8 = 0x00;
const DATA_BLOCK_FLAG: u8 = 0xFF;
const HEADER_SIZE: usize = core::mem::size_of::<Header>();

#[derive(Clone, Copy, Debug)]
#[repr(u8)]
pub enum BlockType {
    Program     = 0,
    NumberArray = 1,
    CharArray   = 2,
    Code        = 3
}

#[derive(Clone, Copy, Debug)]
#[repr(packed)]
pub struct Header {
    head: u8,
    block_type: BlockType,
    name: [u8;10],
    length: [u8;2],
    par1: [u8;2],
    par2: [u8;2],
    cksum: u8
}

#[repr(packed)]
union HeaderArray {
    header: Header,
    array: [u8;HEADER_SIZE]
}

#[derive(Clone, Debug)]
pub enum TapChunk {
    Head(Header),
    Body(Vec<u8>)
}

impl Default for BlockType {
    fn default() -> Self {
        BlockType::Code
    }
}

fn checksum(data: &[u8]) -> u8 {
    data.iter().fold(0, |acc, x| acc ^ x)
}

impl Default for Header {
    fn default() -> Self {
        Header {
            head: HEAD_BLOCK_FLAG,
            block_type: BlockType::Code,
            name: [b' ';10],
            length: [0;2],
            par1: [0;2],
            par2: [0x00, 0x80],
            cksum: HEAD_BLOCK_FLAG ^ BlockType::Code as u8 ^ 0x80
        }
    }
}

impl Header {
    pub fn new(block_type: BlockType, header_name: &[u8], length: u16, par1: [u8;2], par2: [u8;2]) -> Self {
        let name_len = header_name.len().min(10);
        let mut name =  [b' ';10];
        name.copy_from_slice(&header_name[..name_len]);
        let header_array = HeaderArray {
            header: Header {
                head: HEAD_BLOCK_FLAG,
                block_type,
                name,
                length: length.to_le_bytes(),
                par1,
                par2,
                cksum: 0
            }
        };
        let cksum = checksum(unsafe { &header_array.array });
        let mut header = unsafe { header_array.header };
        header.cksum = cksum;
        header
    }

    pub fn new_code(name: &[u8], length: u16, start: u16) -> Self {
        Header::new(BlockType::Code, name, length, start.to_le_bytes(), [0x00, 0x80])
    }

    pub fn new_program(name: &[u8], length: u16, start: u16, vars: u16) -> Self {
        assert!(vars <= length);
        Header::new(BlockType::Program, name, length, start.to_le_bytes(), vars.to_le_bytes())
    }

    pub fn new_char_array(name: &[u8], length: u16, var: u8) -> Self {
        Header::new(BlockType::CharArray, name, length, [0x00, var], [0;2])
    }

    pub fn new_number_array(name: &[u8], length: u16, var: u8) -> Self {
        Header::new(BlockType::NumberArray, name, length, [0x00, var], [0;2])
    }

    pub fn into_array(self) -> [u8;HEADER_SIZE] {
        unsafe { HeaderArray { header: self }.array }
    }

    pub fn into_reader(self) -> Cursor<[u8;HEADER_SIZE]> {
        Cursor::new(self.into_array())
    }

    unsafe fn from_array(array: [u8; HEADER_SIZE]) -> Self {
        HeaderArray { array }.header
    }

    pub fn block_type(&self) -> BlockType {
        self.block_type
    }

    pub fn name(&self) -> Cow<'_,str> {
        String::from_utf8_lossy(&self.name[..])
    }

    pub fn data_len(&self) -> u16 {
        u16::from_le_bytes(self.length)
    }

    pub fn par1(&self) -> u16 {
        u16::from_le_bytes(self.par1) as u16
    }

    pub fn par2(&self) -> u16 {
        u16::from_le_bytes(self.par2) as u16
    }

    pub fn start(&self) -> Option<u16> {
        match self.block_type {
            BlockType::Program|
            BlockType::Code => Some(self.par1()),
            _ => None
        }
    }

    pub fn vars_offset(&self) -> Option<u16> {
        match self.block_type {
            BlockType::Program => Some(self.par2()),
            _ => None
        }
    }

    pub fn array_name(&self) -> Option<char> {
        match self.block_type {
            BlockType::NumberArray|
            BlockType::CharArray => Some(char::from(self.par1[1])),
            _ => None
        }
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

impl TryFrom<&'_[u8]> for Header {
    type Error = Error;

    #[inline]
    fn try_from(raw: &[u8]) -> Result<Self> {
        if raw.len() != HEADER_SIZE {
            Err(Error::new(ErrorKind::InvalidData, "Not a proper TAP header: invalid length"))?;
        }
        if raw[0] != HEAD_BLOCK_FLAG {
            Err(Error::new(ErrorKind::InvalidData, "Not a proper TAP header: invalid flag"))?;
        }
        if checksum(raw) != 0 {
            Err(Error::new(ErrorKind::InvalidData, "Not a proper TAP header: invalid checksum"))?;
        }
        let block_type = BlockType::try_from(raw[1])?;
        let mut name: [u8;10] = [0;10]; name.copy_from_slice(&raw[2..12]);
        let mut length: [u8;2] = [0;2]; length.copy_from_slice(&raw[12..14]);
        let mut par1: [u8;2] = [0;2]; par1.copy_from_slice(&raw[14..16]);
        let mut par2: [u8;2] = [0;2]; par2.copy_from_slice(&raw[16..18]);
        Ok(Header {
            head: HEAD_BLOCK_FLAG, block_type, name, length, par1, par2, cksum: raw[18]
        })
    }
}

impl TryFrom<[u8;HEADER_SIZE]> for Header {
    type Error = Error;

    #[inline]
    fn try_from(raw: [u8;HEADER_SIZE]) -> Result<Self> {
        if raw[0] != HEAD_BLOCK_FLAG {
            Err(Error::new(ErrorKind::InvalidData, "Not a proper TAP header: invalid header flag byte"))?;
        }
        if checksum(&raw) != 0 {
            Err(Error::new(ErrorKind::InvalidData, "Not a proper TAP header: invalid header checksum"))?;
        }
        Ok(unsafe { Header::from_array(raw) })
    }
}

impl Default for TapChunk {
    fn default() -> Self {
        TapChunk::Head(Header::default())
    }
}

impl TapChunk {
    // pub fn new(header: Header) -> Self
    // pub fn header(&self) -> Option<&Header> {
    //     self.header.as_ref()
    // }
    // pub fn data(&self) -> &[u8] {
    //     &self.data[1..self.data.len()-1]
    // }
    // pub fn raw_data(&self) -> &[u8] {
    //     self.data.as_slice()
    // }
    // pub fn into_ear_pulse_iter(self) -> TapChunkEarPulseIter {
    //     TapChunkEarPulseIter::new(self)
    // }
}

pub fn read_tap_chunk<R: Read>(mut rd: R) -> Result<TapChunk> {
    let mut header_buf = [0u8;HEADER_SIZE];
    rd.read_exact(&mut header_buf[0..1])?;
    let chunk = match header_buf[0] {
        HEAD_BLOCK_FLAG => {
            rd.read_exact(&mut header_buf[1..])?;
            let header = Header::try_from(header_buf)?;
            TapChunk::Head(header)
        } 
        DATA_BLOCK_FLAG => {
            let mut data = Vec::new();
            rd.read_to_end(&mut data)?;
            if checksum(&data) != 0 {
                Err(Error::new(ErrorKind::InvalidData, "Not a proper TAP data block: invalid checksum"))?;
            }
            TapChunk::Body(data)
        }
        _ => {
            Err(Error::new(ErrorKind::InvalidData, "Not a proper TAP block: invalid block flag byte"))?
        }
    };
    Ok(chunk)
}


pub fn read_tap<R: Read>(rd: R) -> TapChunkIter<R> {
    TapChunkIter { rd }
}

pub struct TapChunkIter<R: Read> {
    rd: R
}

impl<R: Read> Iterator for TapChunkIter<R> {
    type Item = Result<TapChunk>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut size: [u8; 2] = Default::default();
        if let Err(e) = self.rd.read_exact(&mut size) {
            return match e.kind() {
                ErrorKind::UnexpectedEof => None,
                _ => Some(Err(e))
            };
        };
        let length = u16::from_le_bytes(size);
        let chunk_read = self.rd.by_ref().take(length as u64);
        Some(read_tap_chunk(chunk_read))
    }
}

pub fn read_tap_ear_pulse_iter<R: Read>(rd: R) -> TapEarPulseIter<R> {
    let ep_iter = EarPulseIter::new(rd.take(0));
    TapEarPulseIter { ep_iter }
}

pub struct TapEarPulseIter<R: Read> {
    ep_iter: EarPulseIter<Take<R>>
}

impl<R: Read> TapEarPulseIter<R> {
    pub fn into_inner(self) -> R {
        self.ep_iter.into_inner().into_inner()
    }

    pub fn get_mut(&mut self) -> &mut R {
        self.ep_iter.get_mut().get_mut()
    }

    pub fn get_ref(&self) -> &R {
        self.ep_iter.get_ref().get_ref()
    }
}

impl<R: Read> Iterator for TapEarPulseIter<R> {
    type Item = NonZeroU32;

    fn next(&mut self) -> Option<Self::Item> {
        match self.ep_iter.next() {
            pulse @ Some(_) => pulse,
            None => {
                if self.ep_iter.err().is_some() {
                    return None;
                }
                let mut size: [u8; 2] = Default::default();
                let take_rd = self.ep_iter.get_mut();
                if let Err(_) = take_rd.get_mut().read_exact(&mut size) {
                    return None;
                };
                let length = u16::from_le_bytes(size) as u64;
                take_rd.set_limit(length);
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

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use core::mem;
//     // use std::io::Cursor;

//     #[test]
//     fn tap_works() {
//     }
// }
