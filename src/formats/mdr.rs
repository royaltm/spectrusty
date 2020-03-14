/*! **MDR** file format utilities.

**MDR** files are data images of formatted sectors of ZX Microdrive tape cartridges.
The number of sectors may vary but it will never be a larger than 254.
The last byte determines the write protection flag: if it's not `0` the cartridge should be protected.

```text
Offset Length Name    Contents
------------------------------
  0      1   HDFLAG   bit 0 = 1, indicates header block
  1      1   HDNUMB   sector number (values 254 down to 1)
  2      2            not used (and of undetermined value)
  4     10   HDNAME   microdrive cartridge name (blank padded)
 14      1   HDCHK    header checksum (of first 14 bytes)

 15      1   RECFLG   - bit 0: always 0 to indicate record block
                      - bit 1: set for the EOF block
                      - bit 2: reset for a PRINT file
                      - bits 3-7: not used (value 0)
                      
 16      1   RECNUM   data block sequence number (value starts at 0)
 17      2   RECLEN   data block length (<=512, LSB first)
 19     10   RECNAM   filename (blank padded)
 29      1   DESCHK   record descriptor checksum (of previous 14 bytes)
 30    512            data block
542      1   DCHK     data block checksum (of all 512 bytes of data
                      block, even when not all bytes are used)
```
!*/
use core::mem;
use core::convert::{TryInto, TryFrom};
use core::borrow::Borrow;
use core::fmt;
use core::iter::Cycle;
use core::slice;
use std::borrow::Cow;
use std::io::{self, Read, Write, Seek};
use std::collections::{HashMap, HashSet};

use bitvec::prelude::*;

use crate::peripherals::storage::microdrives::{
    MAX_SECTORS, MAX_USABLE_SECTORS, HEAD_SIZE, DATA_SIZE, SECTOR_MAP_SIZE,
    Sector, MicroCartridge, MicroCartridgeIdSecIter};
use super::tap::{BlockType, Header, TapChunkWriter, TapChunkReader, TapChunkInfo, DATA_BLOCK_FLAG, array_name};

/// Checksum calculating routine used by Spectrum for Microdrive data.
pub fn checksum<I: IntoIterator<Item=B>, B: Borrow<u8>>(iter: I) -> u8 {
    iter.into_iter().fold(0, |acc, x| {
        let (mut acc, carry) = acc.overflowing_add(*x.borrow());
        acc = acc.wrapping_add(carry as u8);
        if acc == u8::max_value() { 0 } else { acc }
    })
}

/// Extends [MicroCartridge] with methods for reading and manipulating Microdrive's file system.
pub trait MicroCartridgeExt: Sized {
    /// Returns file meta data if the file with `file_name` exists.
    fn file_info<S: AsRef<[u8]>>(&self, file_name: S) -> Result<Option<CatFile>, MdrValidationError>;
    /// Returns a file type if the file with `file_name` exists.
    fn file_type<S: AsRef<[u8]>>(&self, file_name: S) -> Result<Option<CatFileType>, MdrValidationError>;
    /// Retrieves content of a file from a [MicroCartridge] and writes it to `wr` if the file with `file_name` exists.
    /// Returns the type and the size of the file on success.
    ///
    /// If the file is a `SAVE *` type [file][CatFileType::File] the first 9 bytes of data constitute its
    /// [file block header][MdrFileHeader].
    fn retrieve_file<S: AsRef<[u8]>, W: Write>(&self, file_name: S, wr: W) -> io::Result<Option<(CatFileType, usize)>>;
    /// Stores content read from `rd` as a new file on a [MicroCartridge].
    /// Returns the number of newly occupied sectors on success.
    ///
    /// The `is_save` argument determines if the file is a binary file (`SAVE *`) or a data file (`OPEN #`).
    /// In case `is_save` is `true` the first 9 bytes of the data read from `rd` must represent a
    /// [file block header][MdrFileHeader].
    ///
    /// Returns an error if a file with the same name already exists or if there is not enough free sectors
    /// to store the complete file.
    ///
    /// In case of an error of [io::ErrorKind::WriteZero] kind, you may delete the partial file data
    /// with [MicroCartridgeExt::erase_file].
    fn store_file<S: AsRef<[u8]>, R: Read>(&mut self, file_name: S, is_save: bool, rd: R) -> io::Result<u8>;
    /// Marks all sectors (including copies and unclosed files) belonging to a provided `file_name` as free.
    /// Returns the number of erased sectors.
    fn erase_file<S: AsRef<[u8]>>(&mut self, file_name: S) -> u8;
    /// Retrieves content of a binary file and writes it to a *TAP* chunk writer with
    /// a proper *TAP* header.
    ///
    /// Returns `Ok(true)` if a `file_name` exists and the file was successfully written.
    /// Returns `Ok(false)` if a `file_name` is missing or a file is not a binary (`SAVE *`) file.
    fn file_to_tap_writer<S: AsRef<[u8]>, W: Write + Seek>(&self, file_name: S, wr: &mut TapChunkWriter<W>) -> io::Result<bool>;
    /// Stores content read from a *TAP* chunk reader as a new file on a [MicroCartridge].
    /// Returns the number of newly occupied sectors on success.
    ///
    /// The first *TAP* chunk must represent a proper [*TAP* header][Header] and the folowing chunk must be
    /// a data block.
    ///
    /// Returns an error if a file with the same name already exists or if there is not enough free sectors
    /// to store the complete file.
    ///
    /// In case of an error of [io::ErrorKind::WriteZero] kind, you may delete the partial file data
    /// with [MicroCartridgeExt::erase_file].
    fn file_from_tap_reader<R: Read + Seek>(&mut self, rd: &mut TapChunkReader<R>) -> io::Result<u8>;
    /// Returns an iterator of sector indices with unordered blocks of the provided `file_name`.
    ///
    /// The same block numbers may be returned multiple times if there were multiple copies of the file.
    fn file_sector_ids_unordered<S: AsRef<[u8]>>(&self, file_name: S) -> FileSectorIdsUnordIter<'_>;
    /// Returns an iterator of sectors with ordered blocks belonging to the provided `file_name`.
    ///
    /// Does not return duplicate blocks if there are more than one copy of the file.
    fn file_sectors<S: AsRef<[u8]>>(&self, file_name: S) -> FileSectorIter<'_>;
    /// Validates formatted sectors and returns a catalog of files.
    ///
    /// Does not check the integrity of file blocks.
    ///
    /// Returns `Ok(None)` if there are no formatted sectors.
    fn catalog(&self) -> Result<Option<Catalog>, MdrValidationError>;
    /// Validates formatted sectors and returns a catalog name if all formatted sectors
    /// contain the same header indicating a properly formatted cartridge.
    ///
    /// Does not check the integrity of file blocks.
    ///
    /// Returns `Ok(None)` if there are no formatted sectors.
    fn catalog_name(&self) -> Result<Option<Cow<'_, str>>, MdrValidationError>;
    /// Checks if each formatted sector has flags property set and if all checksums are valid.
    fn validate_sectors(&self) -> Result<usize, MdrValidationError>;
    /// Reads the content of an `.mdr` file into the [MicroCartridge] sectors.
    ///
    /// The content of sectors is not being validated. The file can contain from 1 to 254 sectors.
    /// If the file contains an additional byte, it is being read as a flag which is non-zero
    /// if the cartridge is write protected.
    ///
    /// Use [MicroCartridgeExt::validate_sectors] to verify sector's content.
    fn from_mdr<R: Read>(rd: R, max_sectors: usize) -> io::Result<Self>;
    /// Writes all formatted sectors to an `.mdr` file using a provided writer.
    ///
    /// Returns the number of bytes written.
    ///
    /// Writes an additional byte as a flag which is non-zero if the cartridge is write protected.
    ///
    /// If there are no formatted sectors in the cartridge, doesn't write anything.
    fn write_mdr<W: Write>(&self, wr: W) -> io::Result<usize>;
    /// Creates a new instance of [MicroCartridge] with formatted sectors.
    ///
    /// # Panics
    /// `max_sectors` must not be 0 and must not be greater than [MAX_SECTORS].
    fn new_formatted<S: AsRef<[u8]>>(max_sectors: usize, catalog_name: S) -> Self;
}

/// Extends [MicroCartridge]'s [Sector] with methods for reading and manipulating Microdrive's file system.
pub trait SectorExt {
    /// Returns `true` if sector is free.
    fn is_free(&self) -> bool;
    /// Returns the name in sector's header.
    fn catalog_name(&self) -> &[u8;10];
    /// Returns the name in blocks's header.
    fn file_name(&self) -> &[u8;10];
    /// Returns `true` if the given `name` matches name in the blocks's header.
    fn file_name_matches(&self, name: &[u8]) -> bool;
    /// Returns a file type of this sector if the sector is occupied.
    fn file_type(&self) -> Result<Option<CatFileType>, &'static str>;
    /// Returns `HDNUMB` entry of the sector's header.
    fn sector_seq(&self) -> u8;
    /// Returns `RECNUM` entry of the blocks's header.
    fn file_block_seq(&self) -> u8;
    /// Returns `RECLEN` entry of the blocks's header.
    fn file_block_len(&self) -> u16;
    /// Returns an `EOF` flag's state of `RECFLG` entry of the blocks's header.
    fn is_last_file_block(&self) -> bool;
    /// Returns a `SAVE` flag's state of `RECFLG` entry of the blocks's header.
    fn is_save_file(&self) -> bool;
    /// Returns a reference to the data segment.
    fn data(&self) -> &[u8];
    /// Returns a mutable reference to the data segment.
    fn data_mut(&mut self) -> &mut [u8];
    /// Returns a reference to the data segment limited in size by the `RECLEN` entry.
    fn data_record(&self) -> &[u8];
    /// Changes the name in blocks's header.
    fn set_file_name(&mut self, file_name: &[u8]);
    /// Changes `RECNUM` entry of the blocks's header.
    fn set_file_block_seq(&mut self, block_seq: u8);
    /// Changes `RECLEN` entry of the blocks's header.
    fn set_file_block_len(&mut self, len: u16);
    /// Changes an `EOF` flag of `RECFLG` entry of the blocks's header.
    fn set_last_file_block(&mut self, eof: bool);
    /// Changes a `SAVE` flag of `RECFLG` entry of the blocks's header.
    fn set_save_file_flag(&mut self, is_save: bool);
    /// Updates checksums of a block segment.
    fn update_block_checksums(&mut self);
    /// Marks current sector as free. Updates block's header checksum.
    fn erase(&mut self);
    /// Formats this sector with the given sector number (1 - 254) and `catalog_name`.
    ///
    /// # Panics
    /// If `seq` is not in [1, 254] range.
    fn format(&mut self, seq: u8, catalog_name: &[u8]);
    /// Validates the integrity of sector's content.
    fn validate(&self) -> Result<(), &'static str>;
    /// Returns a reference to the file header if this sector is the first sector of a `SAVE *` file.
    fn file_header(&self) -> Result<Option<&MdrFileHeader>, &'static str>;
    /// Creates a `TAP` [Header] if this sector is the first sector of a `SAVE *` file.
    fn tap_header(&self) -> Result<Option<Header>, &'static str>;
}


/// An error returned when a [Sector]'s content is invalid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MdrValidationError {
    /// An index of a faulty sector.
    pub index: u8,
    /// A description of an error.
    pub description: &'static str
}

/// The structure returned by [MicroCartridgeExt::catalog] method.
#[derive(Clone, Debug)]
pub struct Catalog {
    /// The name of the formatted cartridge.
    pub name: String,
    /// File names and their meta data.
    pub files: HashMap<[u8;10], CatFile>,
    /// A number of free sectors.
    pub sectors_free: u8
}

/// [MicroCartridge]'s file meta data.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CatFile {
    /// Size of the file in bytes (single copy).
    pub size: u32,
    /// The number of sectors (blocks) the file occupies (all copies).
    pub blocks: u8,
    /// The number of copies of the file saved.
    pub copies: u8,
    /// The type of the file.
    pub file_type: CatFileType
}

/// A [MicroCartridge]'s file type.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CatFileType {
    /// Represents a file created by the `OPEN #` command. Also called a `PRINT` file.
    Data,
    /// Represents a file created by the `SAVE *` command.
    File(BlockType)
}

/// An iterator of sectors belonging to a single file.
///
/// It may scan all sectors, possibly more than once to find all the blocks belonging to a file in an ordered fashion.
pub struct FileSectorIter<'a> {
    counter: u8,
    stop: u8,
    block_seq: u8,
    name: [u8; 10],
    iter: Cycle<MicroCartridgeIdSecIter<'a>>
}

/// An iterator of sector indices and unordered block numbers belonging to a single file.
pub struct FileSectorIdsUnordIter<'a> {
    name: [u8; 10],
    iter: MicroCartridgeIdSecIter<'a>
}

/// Instances of this type are returned by [FileSectorIdsUnordIter] iterator.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SectorBlock {
    /// A file sector index.
    pub index: u8,
    /// A file block number.
    pub block_seq: u8
}

/// Binary (`SAVE *`) file's header.
///
/// Occupies the first 9 bytes of a first data block.
///
/// This is the equivalent of an [audio tape header][Header].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C,packed)]
pub struct MdrFileHeader {
    /// The type of the file this header represents.
    pub block_type: BlockType,
    /// Length of data.
    pub length: [u8;2],
    /// Start of data.
    pub start: [u8;2],
    /// Program length.
    pub prog_length: [u8;2],
    /// Program's starting line number.
    pub line: [u8;2]
}

#[repr(C)]
union MdrFileHeaderArray {
    header: MdrFileHeader,
    array: [u8; mem::size_of::<MdrFileHeader>()],
}

impl MdrFileHeader {
    pub fn array_name(&self) -> char {
        array_name(self.prog_length[0])
    }

    pub fn length(&self) -> u16 {
        u16::from_le_bytes(self.length)
    }

    pub fn start(&self) -> u16 {
        u16::from_le_bytes(self.start)
    }

    pub fn prog_length(&self) -> u16 {
        u16::from_le_bytes(self.prog_length)
    }

    pub fn line(&self) -> u16 {
        u16::from_le_bytes(self.line)
    }

    pub fn into_array(self) -> [u8; mem::size_of::<MdrFileHeader>()] {
        unsafe { MdrFileHeaderArray { header: self }.array }
    }
}

impl From<&'_ Header> for MdrFileHeader {
    fn from(header: &Header) -> Self {
        let block_type = header.block_type;
        let length = header.length.to_le_bytes();
        let mut start = header.par1;
        let mut prog_length = [0, 0];
        let mut line = [0, 0];
        match block_type {
            BlockType::Program => {
                start = 0x5CCBu16.to_le_bytes();
                line = header.par1;
                prog_length = header.par2;
            }
            BlockType::NumberArray|BlockType::CharArray => {
                start = 0x5CCBu16.to_le_bytes();
                prog_length[0] = header.par1[1];
            }
            BlockType::Code => {}
        }
        MdrFileHeader { block_type, length, start, prog_length, line }
    }
}

impl From<&'_ MdrFileHeader> for Header {
    fn from(fbh: &MdrFileHeader) -> Self {
        let length = u16::from_le_bytes(fbh.length);
        match fbh.block_type {
            BlockType::Program => {
                let vars = u16::from_le_bytes(fbh.prog_length);
                let line = u16::from_le_bytes(fbh.line);
                Header::new_program(length).with_start(line).with_vars(vars)
            }
            BlockType::NumberArray => {
                Header::new_number_array(length).with_array_name(fbh.array_name())
            }
            BlockType::CharArray => {
                Header::new_char_array(length).with_array_name(fbh.array_name())
            }
            BlockType::Code => {
                let start = u16::from_le_bytes(fbh.start);
                Header::new_code(length).with_start(start)
            }
        }
    }
}

impl TryFrom<&[u8]> for &MdrFileHeader {
    type Error = &'static str;
    fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
        let data: &[u8; mem::size_of::<MdrFileHeader>()] = data.try_into()
                            .map_err(|_| "wrong size of slice for MdrFileHeader")?;
        BlockType::try_from(data[0]).map_err(|_| "file type not recognized" )?;
        let ptr = data as *const _ as  *const MdrFileHeader;
        unsafe { Ok(&*ptr) }
    }
}

const HDFLAG:  usize =   0; //  1
const HDNUMB:  usize =   1; //  1
const HDNAME:  usize =   4; // 10
const HDCHK:   usize =  14; //  1

const RECFLG:  usize =   0; //  1
const RECNUM:  usize =   1; //  1
const RECLEN:  usize =   2; //  2
const RECNAM:  usize =   4; // 10
const DESCHK:  usize =  14; //  1
const HD_00:   usize =  15; //  1 File block type.
const HD_0B:   usize =  16; //  2 Length of data.
const HD_0D:   usize =  18; //  2 Start of data.
const HD_0F:   usize =  20; //  2 Program length. 
const HD_11:   usize =  22; //  2 Line number.
const HD_SIZE: usize =   9; //
const DCHK:    usize = 527; //  1

const RECFLG_EOF:  u8 = 2;
const RECFLG_SAVE: u8 = 4;

const BLOCK_DATA_MAX: u16 = 512;

fn copy_name(target: &mut [u8;10], name: &[u8]) {
    let name_len = name.len().min(10);
    target[..name_len].copy_from_slice(&name[..name_len]);
    target[name_len..].iter_mut().for_each(|p| *p=b' ');
}

trait ReadExt {
    fn read_exact_or_less(&mut self, buf: &mut[u8]) -> io::Result<usize>;
}

impl<R> ReadExt for R where R: Read {
    fn read_exact_or_less(&mut self, mut buf: &mut[u8]) -> io::Result<usize> {
        let mut len = 0;
        while !buf.is_empty() {
            match self.read(buf) {
                Ok(0) => break,
                Ok(n) => {
                    len += n;
                    buf = &mut buf[n..];
                }
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        Ok(len)
    }
}

impl SectorExt for Sector {
    fn is_free(&self) -> bool {
        !self.is_last_file_block() && self.file_block_len() < BLOCK_DATA_MAX
    }

    fn catalog_name(&self) -> &[u8;10] {
        self.head[HDNAME..HDNAME + 10].try_into().unwrap()
    }

    fn file_name(&self) -> &[u8;10] {
        self.data[RECNAM..RECNAM + 10].try_into().unwrap()
    }

    fn file_name_matches(&self, name: &[u8]) -> bool {
        let name = &name[0..name.len().min(10)];
        if &self.data[RECNAM..RECNAM + name.len()] == name {
            return self.data[RECNAM + name.len()..RECNAM + 10].iter().all(|p| *p==b' ')
        }
        false
    }

    fn file_type(&self) -> Result<Option<CatFileType>, &'static str> {
        if self.is_free() {
            return Ok(None)
        }
        Ok(Some(if self.is_save_file() {
            CatFileType::File(BlockType::try_from(self.data[HD_00])
                .map_err(|_| "file type not recognized" )?)
        }
        else {
            CatFileType::Data
        }))
    }

    fn sector_seq(&self) -> u8 {
        self.head[HDNUMB]
    }

    fn file_block_seq(&self) -> u8 {
        self.data[RECNUM]
    }

    fn file_block_len(&self) -> u16 {
        let reclen = &self.data[RECLEN..RECLEN + 2];
        u16::from_le_bytes(<[u8;2]>::try_from(reclen).unwrap())
    }

    fn is_last_file_block(&self) -> bool {
        self.data[RECFLG] & RECFLG_EOF != 0
    }

    fn is_save_file(&self) -> bool {
        self.data[RECFLG] & RECFLG_SAVE != 0
    }

    fn data(&self) -> &[u8] {
        &self.data[DESCHK + 1..DCHK]
    }

    fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data[DESCHK + 1..DCHK]
    }

    fn data_record(&self) -> &[u8] {
        let reclen = self.file_block_len() as usize;
        &self.data[DESCHK + 1..DESCHK + 1 + reclen]
    }

    fn set_file_name(&mut self, file_name: &[u8]) {
        let block_name = &mut self.data[RECNAM..RECNAM + 10];
        copy_name(block_name.try_into().unwrap(), file_name);
    }

    fn set_file_block_seq(&mut self, block_seq: u8) {
        self.data[RECNUM] = block_seq;
    }

    fn set_file_block_len(&mut self, len: u16) {
        if len > BLOCK_DATA_MAX {
            panic!("RECLEN must be less than 512");
        }
        let reclen = len.to_le_bytes();
        self.data[RECLEN..RECLEN + 2].copy_from_slice(&reclen);
    }

    fn set_last_file_block(&mut self, eof: bool) {
        if eof {
            self.data[RECFLG] |= RECFLG_EOF;
        }
        else {
            self.data[RECFLG] &= !RECFLG_EOF;
        }
    }

    fn set_save_file_flag(&mut self, is_save: bool) {
        if is_save {
            self.data[RECFLG] |= RECFLG_SAVE;
        }
        else {
            self.data[RECFLG] &= !RECFLG_SAVE;
        }
    }

    fn update_block_checksums(&mut self) {
        self.data[DESCHK] = checksum(&self.data[0..DESCHK]);
        self.data[DCHK]   = checksum(&self.data[DESCHK + 1..DCHK])
    }

    fn erase(&mut self) {
        self.data[RECFLG] = 0;
        self.data[RECLEN..RECLEN + 2].iter_mut().for_each(|p| *p=0);
        self.data[DESCHK] = checksum(&self.data[0..DESCHK]);
    }

    fn format(&mut self, seq: u8, catalog_name: &[u8]) {
        if seq < 1 || seq > 245 {
            panic!("HDNUMB must be between 1 and 254");
        }
        self.head[HDFLAG] = 1;
        self.head[HDNUMB] = seq;
        let head_name = &mut self.head[HDNAME..HDNAME + 10];
        copy_name(head_name.try_into().unwrap(), catalog_name);
        self.head[HDCHK] = checksum(&self.head[0..HDCHK]);
        self.erase();
    }

    fn validate(&self) -> Result<(), &'static str> {
        if self.head[HDFLAG] & 1 != 1 {
            return Err("bad sector header: HDFLAG bit 0 is reset");
        }
        if self.data[RECFLG] & 1 != 0 {
            return Err("bad data block: RECFLG bit 0 is set");
        }
        if !(1..=254).contains(&self.head[HDNUMB]) {
            return Err("bad sector header: HDNUMB is outside the allowed range: [1-254]");
        }
        if self.file_block_len() > BLOCK_DATA_MAX {
            return Err("bad data block: RECLEN is larger than the sector size: 512");
        }
        if self.head[HDCHK] != checksum(&self.head[0..HDCHK]) {
            return Err("bad sector header: HDCHK invalid checksum");
        }
        if self.data[DESCHK] != checksum(&self.data[0..DESCHK]) {
            return Err("bad data block: DESCHK invalid header checksum");
        }
        if !self.is_free() {
            if self.data[DCHK] != checksum(&self.data[DESCHK+1..DCHK]) {
                return Err("bad data block: DESCHK invalid data checksum");
            }
        }
        Ok(())
    }

    fn file_header(&self) -> Result<Option<&MdrFileHeader>, &'static str> {
        if !self.is_free() && self.is_save_file() && self.file_block_seq() == 0 {
            let mdrhd = <&MdrFileHeader>::try_from(&self.data[HD_00..HD_00 + HD_SIZE])?;
            Ok(Some(mdrhd))
        }
        else {
            Ok(None)
        }

    }

    fn tap_header(&self) -> Result<Option<Header>, &'static str> {
        self.file_header().map(|m|
            m.map(|hd| Header::from(hd).with_name(&self.data[RECNAM..RECNAM + 10]))
        )
    }
}

impl Default for CatFileType {
    fn default() -> Self {
        CatFileType::Data
    }
}

impl fmt::Display for Catalog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:10}:\n", self.name)?;
        for (name, file) in self.files.iter() {
            write!(f, "  {:10} {}\n", String::from_utf8_lossy(name), file)?;
        }
        write!(f, "free: {} sec. {} bytes\n",
                        self.sectors_free,
                        self.sectors_free as u32 * BLOCK_DATA_MAX as u32)
    }
}

impl fmt::Display for CatFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} bytes {} blocks {} copies", self.file_type, self.size, self.blocks, self.copies)
    }
}

impl fmt::Display for CatFileType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CatFileType::Data => f.write_str("Data"),
            CatFileType::File(bt) => bt.fmt(f),
        }
    }
}

impl std::error::Error for MdrValidationError {}

impl fmt::Display for MdrValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MDR validation error in sector #{}: {}", self.index, self.description)
    }
}

impl<'a> Iterator for FileSectorIter<'a> {
    type Item = Result<&'a Sector, MdrValidationError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.counter == 0 {
            return None
        }
        while let Some((index, sector)) = self.iter.next() {
            if self.stop == index {
                self.counter = 0;
            }
            else {
                self.counter -= 1;
                if !sector.is_free() && sector.file_name() == &self.name {
                    if sector.file_block_seq() == self.block_seq {
                        if sector.is_last_file_block() {
                            self.counter = 0;
                        }
                        else {
                            self.stop = index;
                            self.counter = !0;
                            self.block_seq += 1;
                        }
                        return Some(Ok(sector))
                    }
                }
            }
            if self.counter == 0 {
                if self.block_seq != 0 {
                    return Some(Err(MdrValidationError { index, description:
                                    "missing end of file block" }))
                }
                break
            }
        }
        None
    }
}

impl<'a> Iterator for FileSectorIdsUnordIter<'a> {
    type Item = SectorBlock;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some((index, sector)) = self.iter.next() {
            if !sector.is_free() && sector.file_name() == &self.name {
                let block_seq = sector.file_block_seq();
                return Some(SectorBlock { index, block_seq })
            }
        }
        None
    }
}

impl MicroCartridgeExt for MicroCartridge {
    fn file_info<S: AsRef<[u8]>>(
            &self,
            file_name: S
        ) -> Result<Option<CatFile>, MdrValidationError>
    {
        let file_name = file_name.as_ref();
        let mut meta = CatFile::default();
        for (index, sector) in self.iter_with_indices() {
            if !sector.is_free() && sector.file_name_matches(file_name) {
                if sector.file_block_seq() == 0 {
                    meta.copies += 1;
                    meta.file_type = sector.file_type()
                        .map_err(|e| MdrValidationError { index, description: e })?
                        .unwrap();
                }
                meta.blocks += 1;
                meta.size += sector.file_block_len() as u32;
            }
        }
        if meta.copies != 0 {
            meta.size /= meta.copies as u32;
            Ok(Some(meta))
        }
        else {
            Ok(None)
        }
    }

    fn file_type<S: AsRef<[u8]>>(
            &self,
            file_name: S
        ) -> Result<Option<CatFileType>, MdrValidationError>
    {
        let file_name = file_name.as_ref();
        for (index, sector) in self.iter_with_indices() {
            if !sector.is_free() && sector.file_block_seq() == 0 && sector.file_name_matches(file_name) {
                return sector.file_type().map_err(|e| MdrValidationError { index, description: e })
            }
        }
        Ok(None)
    }

    fn file_to_tap_writer<S: AsRef<[u8]>, W: Write + Seek>(
            &self,
            file_name: S,
            wr: &mut TapChunkWriter<W>
        ) -> io::Result<bool>
    {
        let mut sector_iter = self.file_sectors(file_name);
        let mut tran = loop { 
            if let Some(sector) = sector_iter.next() {
                let sector = sector.unwrap();
                if let Some(header) = sector.tap_header()
                                      .map_err(|e| io::Error::new(io::ErrorKind::Other, e))? {
                    wr.write_header(&header)?;
                    let mut tran = wr.begin()?;
                    tran.write_all(slice::from_ref(&DATA_BLOCK_FLAG))?;
                    tran.write_all(&sector.data_record()[HD_SIZE..])?;
                    break tran
                }
            }
            return Ok(false)
        };
        for sector in sector_iter {
            let sector = sector.map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
            let record = sector.data_record();
            tran.write_all(record)?;
        }
        tran.commit(true)?;
        Ok(true)
    }

    fn file_from_tap_reader<R: Read + Seek>(&mut self, rd: &mut TapChunkReader<R>) -> io::Result<u8> {
        if rd.chunk_limit() == 0 {
            rd.next_chunk()?;
        }
        let header = if let TapChunkInfo::Head(header) = TapChunkInfo::try_from(rd.get_mut())? {
            header
        }
        else {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "not a TAP header"))
        };
        if let Some(chunk_size) = rd.next_chunk()? {
            if chunk_size < 2 || header.length != chunk_size - 2 {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "not a TAP block"))
            }
            let mut flag = 0u8;
            rd.read_exact(slice::from_mut(&mut flag))?;
            if flag != DATA_BLOCK_FLAG {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid TAP block flag"))
            }
        }
        else {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "missing TAP chunk"))
        }
        let fbheader = MdrFileHeader::from(&header).into_array();
        let hdrd = io::Cursor::new(fbheader).chain(rd.take(header.length as u64));
        let res = self.store_file(&header.name, true, hdrd)?;
        {
            let mut checksum = 0u8;
            let res = rd.read_exact(slice::from_mut(&mut checksum));
            if res.is_err() || rd.checksum != 0 {
                self.erase_file(&header.name);
                return Err(io::Error::new(io::ErrorKind::InvalidData, "TAP block checksum error"))
            }
        }
        Ok(res)
    }

    fn retrieve_file<S: AsRef<[u8]>, W: Write>(
            &self,
            file_name: S,
            mut wr: W
        ) -> io::Result<Option<(CatFileType, usize)>>
    {
        let mut len = 0;
        let mut file_type = None;
        for sector in self.file_sectors(file_name) {
            let sector = sector.map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
            if sector.file_block_seq() == 0 {
                file_type = sector.file_type().map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
            }
            let record = sector.data_record();
            wr.write_all(record)?;
            len += record.len();
        }
        Ok(file_type.map(|ft| (ft, len)))
    }

    fn store_file<S: AsRef<[u8]>, R: Read>(
            &mut self,
            file_name: S,
            is_save: bool,
            mut rd: R
        ) -> io::Result<u8>
    {
        let file_name = file_name.as_ref();
        if let Some((index, _)) = self.iter_with_indices().find(|(_,s)|
                                    !s.is_free() && s.file_name_matches(file_name)) {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists,
                        MdrValidationError { index, description: "file name already exists" }))
        }
        let mut block_seq = 0;
        let mut first_byte = 0u8;
        let len = rd.read_exact_or_less(slice::from_mut(&mut first_byte))?;
        if len == 0 {
            return Ok(0);
        }
        for sector in self.into_iter() {
            if sector.is_free() {
                let buf = sector.data_mut();
                buf[0] = first_byte;
                let len = 1 + rd.read_exact_or_less(&mut buf[1..])?;
                sector.set_file_name(file_name);
                sector.set_file_block_seq(block_seq);
                block_seq += 1;
                sector.set_file_block_len(len.try_into().unwrap());
                sector.set_save_file_flag(is_save);
                let len = rd.read_exact_or_less(slice::from_mut(&mut first_byte))?;
                sector.set_last_file_block(len == 0);
                sector.update_block_checksums();
                if len == 0 {
                    return Ok(block_seq);
                }
            }
        }
        Err(io::Error::new(io::ErrorKind::WriteZero, "not enough free sectors to fit the whole file"))
    }

    fn erase_file<S: AsRef<[u8]>>(&mut self, file_name: S) -> u8 {
        let file_name = file_name.as_ref();
        let mut block_seq = 0;
        for sector in self.into_iter() {
            if !sector.is_free() && sector.file_name_matches(file_name) {
                sector.erase();
                block_seq += 1;
            }
        }
        block_seq
    }

    fn file_sector_ids_unordered<S: AsRef<[u8]>>(
            &self,
            file_name: S
        ) -> FileSectorIdsUnordIter<'_>
    {
        let file_name = file_name.as_ref();
        let mut name = [b' '; 10];
        copy_name(&mut name, file_name);
        FileSectorIdsUnordIter {
            name,
            iter: self.iter_with_indices()
        }
    }

    fn file_sectors<S: AsRef<[u8]>>(
            &self,
            file_name: S
        ) -> FileSectorIter<'_>
    {
        let file_name = file_name.as_ref();
        let mut name = [b' '; 10];
        copy_name(&mut name, file_name);
        let counter = self.count_formatted().try_into().unwrap();
        FileSectorIter {
            counter,
            stop: !0,
            block_seq: 0,
            name,
            iter: self.iter_with_indices().cycle()
        }
    }

    fn catalog(&self) -> Result<Option<Catalog>, MdrValidationError> {
        let name = if let Some(name) = self.catalog_name()? {
            name.to_string()
        }
        else {
            return Ok(None)
        };
        let mut files: HashMap<[u8;10], CatFile> = HashMap::new();
        let mut sectors_free = 0u8;
        for (index, sector) in self.iter_with_indices() {
            if sector.is_free() {
                sectors_free += 1;
            }
            else {
                let meta = files.entry(*sector.file_name()).or_default();
                if sector.file_block_seq() == 0 {
                    meta.copies += 1;
                    meta.file_type = sector.file_type()
                        .map_err(|e| MdrValidationError { index, description: e })?
                        .unwrap();
                }
                meta.blocks += 1;
                meta.size += sector.file_block_len() as u32;
            }
        }
        for meta in files.values_mut() {
            if meta.copies == 0 {
                return Err(MdrValidationError { index: u8::max_value(), description: "block 0 is missing" })
            }
            meta.size /= meta.copies as u32;
        }
        Ok(Some(Catalog {
            name, files, sectors_free
        }))
    }

    fn catalog_name(&self) -> Result<Option<Cow<'_, str>>, MdrValidationError> {
        let mut header: Option<&[u8]> = None;
        let mut seqs: HashSet<u8> = HashSet::new();
        for (index, sector) in self.iter_with_indices() {
            sector.validate().map_err(|description| MdrValidationError { index, description })?;
            if let Some(name) = header {
                if name != sector.catalog_name() {
                    return Err(MdrValidationError { index, description:
                        "catalog name is inconsistent" })
                }
            }
            else {
                header = Some(sector.catalog_name());
            }
            if !seqs.insert(sector.sector_seq()) {
                return Err(MdrValidationError { index, description:
                    "at least two sectors have the same identification number" })
            }
        }
        Ok(header.map(|name| String::from_utf8_lossy(name)))
    }

    fn validate_sectors(&self) -> Result<usize, MdrValidationError> {
        let mut count = 0usize;
        for (index, sector) in self.iter_with_indices() {
            sector.validate().map_err(|description| MdrValidationError { index, description })?;
            count += 1;
        }
        Ok(count)
    }

    fn from_mdr<R: Read>(mut rd: R, max_sectors: usize) -> io::Result<Self> {
        let mut sectors: Vec<Sector> = Vec::with_capacity(max_sectors);
        let mut write_protect = false;
        'sectors: loop {
            let mut sector = Sector::default();
            loop {
                match rd.read(&mut sector.head) {
                    Ok(0) => break 'sectors,
                    Ok(1) => {
                        write_protect = sector.head[0] != 0;
                        break 'sectors
                    }
                    Ok(HEAD_SIZE) => break,
                    Ok(..) => return Err(io::Error::new(io::ErrorKind::UnexpectedEof,
                                                "failed to fill whole sector header")),
                    Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(e) => return Err(e),
                }
            }
            if sectors.len() == MAX_USABLE_SECTORS {
                return Err(io::Error::new(io::ErrorKind::InvalidData,
                                            "only 254 sectors are supported"));
            }
            rd.read_exact(&mut sector.data)?;
            sectors.push(sector);
        }
        if sectors.len() == 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "no sectors read"));
        }
        Ok(MicroCartridge::new_with_sectors(sectors, write_protect, max_sectors))
    }

    fn write_mdr<W: Write>(&self, mut wr: W) -> io::Result<usize> {
        let mut bytes: usize = 0;
        for sector in self.into_iter() {
            wr.write_all(&sector.head)?;
            wr.write_all(&sector.data)?;
            bytes += sector.head.len() + sector.data.len();
        }
        if bytes == 0 {
            return Ok(0)
        }
        wr.write_all(slice::from_ref(&self.is_write_protected().into()))?;
        Ok(bytes + 1)
    }

    fn new_formatted<S: AsRef<[u8]>>(max_sectors: usize, catalog_name: S) -> Self {
        let mut sectors = vec![Sector::default(); max_sectors.min(254)];
        for (sector, seq) in sectors.iter_mut().rev().zip(1..254) {
            sector.format(seq, catalog_name.as_ref());
        }
        MicroCartridge::new_with_sectors(sectors, false, max_sectors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use rand::prelude::*;
    use crate::formats::tap;

    #[test]
    fn mdr_works() {
        let mdr = MicroCartridge::default();
        assert!(mdr.catalog_name().unwrap().is_none());
        assert!(mdr.catalog().unwrap().is_none());
        assert!(mdr.file_type("hello world").unwrap().is_none());
        assert!(mdr.file_info("hello world").unwrap().is_none());
        let mut mdr = MicroCartridge::new_formatted(10, "testing");
        for i in 0..10 {
            assert_eq!(mdr[i].is_free(), true);
            assert_eq!(mdr[i].sector_seq(), 10 - i);
            assert_eq!(mdr[i].catalog_name(), b"testing   ");
            assert_eq!(mdr[i].validate().unwrap(), ());
        }
        assert_eq!(mdr.catalog_name().unwrap().unwrap(), "testing   ");
        let catalog = mdr.catalog().unwrap().unwrap();
        assert_eq!(catalog.name, "testing   ");
        assert_eq!(catalog.files, HashMap::<[u8;10],CatFile>::new());
        assert_eq!(catalog.sectors_free, 10);
        assert_eq!(mdr.store_file("hello world", false, io::Cursor::new([1,2,3,4,5])).unwrap(), 1);
        assert_eq!(mdr.file_type("hello world").unwrap().unwrap(), CatFileType::Data);
        assert_eq!(mdr.file_info("hello world").unwrap().unwrap(),
                CatFile { size: 5, blocks: 1, copies: 1, file_type: CatFileType::Data});
        let catalog = mdr.catalog().unwrap().unwrap();
        assert_eq!(catalog.name, "testing   ");
        assert_eq!(catalog.files.len(), 1);
        assert_eq!(catalog.files.get(b"hello worl").unwrap(), &CatFile {
            size: 5,
            blocks: 1,
            copies: 1,
            file_type: CatFileType::Data
        });
        assert_eq!(catalog.sectors_free, 9);
        let mut wr = io::Cursor::new(Vec::new());
        assert_eq!(mdr.retrieve_file("hello world", &mut wr).unwrap().unwrap(), (CatFileType::Data, 5));
        assert_eq!(wr.get_ref(), &vec![1u8,2,3,4,5]);
        assert_eq!(mdr.erase_file("hello world"), 1);
        let catalog = mdr.catalog().unwrap().unwrap();
        assert_eq!(catalog.name, "testing   ");
        assert_eq!(catalog.files.len(), 0);
        assert_eq!(catalog.sectors_free, 10);
        let mut big_file = vec![0u8;5000];
        thread_rng().fill(&mut big_file[..]);
        assert_eq!(mdr.store_file("big file", false, io::Cursor::new(&big_file[..])).unwrap(), 10);
        let catalog = mdr.catalog().unwrap().unwrap();
        assert_eq!(catalog.name, "testing   ");
        assert_eq!(catalog.files.len(), 1);
        assert_eq!(catalog.files.get(b"big file  ").unwrap(), &CatFile {
            size: 5000,
            blocks: 10,
            copies: 1,
            file_type: CatFileType::Data
        });
        assert_eq!(catalog.sectors_free, 0);
        assert_eq!(mdr.file_type("big file").unwrap().unwrap(), CatFileType::Data);
        assert_eq!(mdr.file_info("big file").unwrap().unwrap(),
                CatFile { size: 5000, blocks: 10, copies: 1, file_type: CatFileType::Data});
        let err = mdr.store_file("hello world", false, io::Cursor::new([1]));
        assert!(err.is_err());
        let err = err.err().unwrap();
        assert_eq!(err.kind(), io::ErrorKind::WriteZero);
        assert_eq!(err.to_string(), "not enough free sectors to fit the whole file");
        let mut wr = io::Cursor::new(Vec::new());
        assert_eq!(mdr.retrieve_file("big file", &mut wr).unwrap().unwrap(), (CatFileType::Data, 5000));
        assert_eq!(wr.get_ref(), &big_file);
        assert_eq!(mdr.erase_file("hello world"), 0);
        assert_eq!(mdr.erase_file("big file"), 10);
        let catalog = mdr.catalog().unwrap().unwrap();
        assert_eq!(catalog.name, "testing   ");
        assert_eq!(catalog.files.len(), 0);
        assert_eq!(catalog.sectors_free, 10);

        let file = File::open("tests/read_tap_test.tap").unwrap();
        let mut tap_reader = tap::read_tap(file);
        assert_eq!(mdr.file_from_tap_reader(tap_reader.by_ref()).unwrap(), 1);
        assert_eq!(mdr.file_from_tap_reader(tap_reader.by_ref()).unwrap(), 1);
        assert_eq!(mdr.file_from_tap_reader(tap_reader.by_ref()).unwrap(), 1);
        let catalog = mdr.catalog().unwrap().unwrap();
        assert_eq!(catalog.name, "testing   ");
        assert_eq!(catalog.files.len(), 3);
        assert_eq!(catalog.sectors_free, 7);
        assert_eq!(catalog.files.get(b"HelloWorld").unwrap(), &CatFile {
            size: 9+6, blocks: 1, copies: 1, file_type: CatFileType::File(BlockType::Program)
        });
        assert_eq!(mdr.file_type("HelloWorld").unwrap().unwrap(), CatFileType::File(BlockType::Program));
        assert_eq!(mdr.file_info("HelloWorld").unwrap().unwrap(),
                CatFile { size: 9+6, blocks: 1, copies: 1, file_type: CatFileType::File(BlockType::Program)});
        assert_eq!(catalog.files.get(b"a(10)     ").unwrap(), &CatFile {
            size: 9+53, blocks: 1, copies: 1, file_type: CatFileType::File(BlockType::NumberArray)
        });
        assert_eq!(mdr.file_type("a(10)").unwrap().unwrap(), CatFileType::File(BlockType::NumberArray));
        assert_eq!(mdr.file_info("a(10)").unwrap().unwrap(),
                CatFile { size: 9+53, blocks: 1, copies: 1, file_type: CatFileType::File(BlockType::NumberArray)});
        assert_eq!(catalog.files.get(b"weekdays  ").unwrap(), &CatFile {
            size: 9+26, blocks: 1, copies: 1, file_type: CatFileType::File(BlockType::CharArray)
        });
        assert_eq!(mdr.file_type("weekdays").unwrap().unwrap(), CatFileType::File(BlockType::CharArray));
        assert_eq!(mdr.file_info("weekdays").unwrap().unwrap(),
                CatFile { size: 9+26, blocks: 1, copies: 1, file_type: CatFileType::File(BlockType::CharArray)});
        let tap_data = io::Cursor::new(Vec::new());
        let mut writer = tap::write_tap(tap_data);
        assert_eq!(mdr.file_to_tap_writer("HelloWorld", &mut writer).unwrap(), true);
        assert_eq!(mdr.file_to_tap_writer("a(10)", &mut writer).unwrap(), true);
        assert_eq!(mdr.file_to_tap_writer("weekdays", &mut writer).unwrap(), true);
        let tap_file = std::fs::read("tests/read_tap_test.tap").unwrap();
        assert_eq!(tap_file, writer.into_inner().into_inner().into_inner());
    }
}
