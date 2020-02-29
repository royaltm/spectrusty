/*! Emulator's memory api. This module defines [ZxMemory] trait.
# Memory banks and pages.

* [ZxMemory] ROM and RAM is a continuous slice of memory divided into logical banks.
* 16 bit memory addresses are mapped constantly to pages depending on the implementation: [SingleBankMemory] or [PagedMemory].
* Each page can have any fitting bank of ROM or RAM memory switched in.

Below are examples of continuous memory slab: all ROM banks followed by RAM banks.
[ZxMemory::mem_ref] and [ZxMemory::mem_mut] both give access to the whole area.
```text
|00000h  |04000h  |8000h   |0c000h  |10000h  |14000h  |18000h |1c000h  |20000h  | 24000h
Memory16k
|ROM_SIZE|
+--------+--------+
|        | Screen |
|        | Bank 0 |
| ROM    | RAM    |
| Bank 0 | Bank 0 |
+--------+--------+
Memory48k
|ROM_SIZE|
+--------+--------+--------+--------+
|        | Screen Bank 0            |
|        |                          |
| ROM    | RAM Bank 0               |
| Bank 0 |                          |
+--------+--------+--------+--------+
Memory128k
|<-   ROM_SIZE  ->|
+--------+--------+--------+--------+--------+--------+--------+--------+--------+--------+
|        |        |        |        |        |        |        | Screen |        | Screen |
|        |        |        |        |        |        |        | Bank 0 |        | Bank 1 |
| ROM    | ROM    | RAM    | RAM    | RAM    | RAM    | RAM    | RAM    | RAM    | RAM    |
| Bank 0 | Bank 1 | Bank 0 | Bank 1 | Bank 2 | Bank 3 | Bank 4 | Bank 5 | Bank 6 | Bank 7 |
+--------+--------+--------+--------+--------+--------+--------+--------+--------+--------+
```
Below are examples of memory pages mappings.

A constant memory map of [Memory16k] and [Memory48k]:
```text
        Memory48k  Memory16k
Page 0 +--------+  +--------+ Page 0 0x0000
       |        |  |        |
       |        |  |        |
       | ROM    |  | ROM    |
       | Bank 0 |  | Bank 0 |
Page 1 +--------+  +--------+ Page 1 0x4000
       | Screen |  | Screen |
       | Bank 0 |  | Bank 0 |
       | RAM    |  | RAM    |
       | Bank 0 |  | Bank 0 | RAMTOP 0x7fff
       +        +  +--------+        0x8000
       |        |
       |        |
       |        |
       |        |
       +        +                    0xc000
       |        |
       |        |
       |        |
RAMTOP |        |                    0xffff
       +--------+
```
A memory map with example banks switched in for [Memory128k] or [Memory128kPlus]:
```text
Page 0 +--------+ 0x1000
       |        |
       |        |
       | ROM    |
       | Bank 0 |
Page 1 +--------+ 0x4000
       | Screen |
       | Bank 0 |
       | RAM    |
       | Bank 5 |
Page 2 +--------+ 0x8000
       |        |
       |        |
       | RAM    |
       | Bank 2 |
Page 3 +--------+ 0xc000
       |        |
       |        |
       | RAM    |
RAMTOP | Bank 0 | 0xffff
       +--------+
```
*/
use core::ops::Range;
use core::ops::{Bound, RangeBounds};
use core::fmt;
use std::io::{self, Read};

mod single_page;
mod multi_page;

pub use single_page::*;
pub use multi_page::*;

// bitflags! {
//     #[derive(Default)]
//     pub struct MemoryFeatures: u8 {
//         const ROM16K   = 0x01; // 2 ROMS available
//         const ROM32K   = 0x02; // 4 ROMS available
//         const RAM_BANKS_16K = 0x04; // 16k ram banks
//         const RAM_BANKS_8K  = 0x08; // 8k ram banks
//         const NONE = MemoryFeatures::empty().bits;
//     }
// }

#[derive(Debug)]
pub enum ZxMemoryError {
    InvalidPageIndex,
    InvalidBankIndex,
    AddressRangeNotSupported,
    Io(io::Error)
}

impl std::error::Error for ZxMemoryError {}

impl fmt::Display for ZxMemoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", match self {
            ZxMemoryError::InvalidPageIndex => "Memory page index out of range",
            ZxMemoryError::InvalidBankIndex => "Memory bank index out of range",
            ZxMemoryError::AddressRangeNotSupported => "Address range not supported",
            ZxMemoryError::Io(err) => return err.fmt(f)
        })
    }
}

impl From<ZxMemoryError> for io::Error {
    fn from(err: ZxMemoryError) -> Self {
        match err {
            ZxMemoryError::InvalidPageIndex => {
                io::Error::new(io::ErrorKind::InvalidInput, err)
            }
            ZxMemoryError::InvalidBankIndex => {
                io::Error::new(io::ErrorKind::InvalidInput, err)
            }
            ZxMemoryError::AddressRangeNotSupported => {
                io::Error::new(io::ErrorKind::InvalidInput, err)
            }
            ZxMemoryError::Io(err) => err
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u8)]
pub enum MemoryKind {
    Rom,
    Ram
}

/// A type returned by [ZxMemory::page_index_at].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MemPageOffset {
    /// A kind of memory bank switched in.
    pub kind: MemoryKind,
    /// A page index.
    pub index: u8,
    /// An offset into the page slice.
    pub offset: u16
}

/// A type returned by [MemPageMutIter] iterator.
#[derive(Debug)]
pub enum PageMutSlice<'a> {
    Rom(&'a mut [u8]),
    Ram(&'a mut [u8])
}

/// A type returned by some of [ZxMemory] methods.
pub type Result<T> = core::result::Result<T, ZxMemoryError>;

/// A trait for interfacing ZX Spectrum's various memory types.
pub trait ZxMemory: Sized {
    /// This is just a hint. Actual page sizes may vary.
    const PAGE_SIZE: usize = 0x4000;
    /// The size of the whole rom, not just one bank.
    const ROM_SIZE: usize;
    /// The last available memory address.
    const RAMTOP: u16;
    /// A maximum value allowed for a `page` argument.
    const PAGES_MAX: u8;
    /// A maximum value allowed for a `screen_bank` argument
    const SCR_BANKS_MAX: usize;
    /// A maximum value allowed for a `rom_bank` argument
    const ROM_BANKS_MAX: usize;
    /// A maximum value allowed for a `ram_bank` argument
    const RAM_BANKS_MAX: usize;
    // /// A hint of available memory features.
    // const FEATURES: MemoryFeatures;
    /// If `addr` is above `RAMTOP` the function should return [std::u8::MAX].
    fn read(&self, addr: u16) -> u8;
    /// If `addr` is above `RAMTOP` the function should return [std::u16::MAX].
    fn read16(&self, addr: u16) -> u16;
    /// `addr` is in screen address space (0 addresses the first byte of screen memory).
    /// #Panics
    /// If `addr` is above upper limit of screen memory address space the function should panic.
    /// If `screen_bank` doesn't exist the function should also panic.
    fn read_screen(&self, screen_bank: usize, addr: u16) -> u8;
    /// If `addr` is above `RAMTOP` the function should do nothing.
    fn write(&mut self, addr: u16, val: u8);
    /// If addr is above `RAMTOP` the function should do nothing.
    fn write16(&mut self, addr: u16, val: u16);
    /// Provides a continuous view into the whole memory (all banks: ROM + RAM).
    fn mem_ref(&self) -> &[u8];
    /// Provides a continuous view into the whole memory (all banks: ROM + RAM).
    fn mem_mut(&mut self) -> &mut[u8];
    /// Returns a slice of the screen memory.
    fn screen_ref(&self, screen_bank: usize) -> Result<&[u8]>;
    /// Returns a mutable slice of the screen_page memory.
    fn screen_mut(&mut self, screen_bank: usize) -> Result<&mut [u8]>;
    /// `page` should be less or euqal to PAGES_MAX.
    fn page_kind(&self, page: u8) -> Result<MemoryKind>;
    /// `page` should be less or euqal to PAGES_MAX.
    fn page_ref(&self, page: u8) -> Result<&[u8]>;
    /// `page` should be less or euqal to PAGES_MAX.
    fn page_mut(&mut self, page: u8) -> Result<& mut[u8]>;
    /// `rom_bank` should be less or equal to `ROM_BANKS_MAX`.
    fn rom_bank_ref(&self, rom_bank: usize) -> Result<&[u8]>;
    /// `rom_bank` should be less or equal to `ROM_BANKS_MAX`.
    fn rom_bank_mut(&mut self, rom_bank: usize) -> Result<&[u8]>;
    /// `ram_bank` should be less or equal to `RAM_BANKS_MAX`.
    fn ram_bank_ref(&self, ram_bank: usize) -> Result<&[u8]>;
    /// `ram_bank` should be less or equal to `RAM_BANKS_MAX`.
    fn ram_bank_mut(&mut self, ram_bank: usize) -> Result<&[u8]>;
    /// `rom_bank` should be less or equal to `ROM_BANKS_MAX` and `page` should be less or euqal to PAGES_MAX.
    fn map_rom_bank(&mut self, rom_bank: usize, page: u8) -> Result<()>;
    /// `ram_bank` should be less or equal to `RAM_BANKS_MAX` and `page` should be less or euqal to PAGES_MAX.
    fn map_ram_bank(&mut self, ram_bank: usize, page: u8) -> Result<()>;
    /// Returns `Ok(MemPageOffset)` if address is equal to or less than [ZxMemory::RAMTOP].
    fn page_index_at(&self, address: u16) -> Result<MemPageOffset>;
    /// Provides a continuous view into the ROM memory (all banks).
    fn rom_ref(&self) -> &[u8] {
        &self.mem_ref()[0..Self::ROM_SIZE]
    }
    /// Provides a continuous mutable view into the ROM memory (all banks).
    fn rom_mut(&mut self) -> &mut [u8] {
        &mut self.mem_mut()[0..Self::ROM_SIZE]
    }
    /// Provides a continuous view into the RAM memory (all banks).
    fn ram_ref(&self) -> &[u8] {
        &self.mem_ref()[Self::ROM_SIZE..]
    }
    /// Provides a continuous mutable view into the RAM memory (all banks).
    fn ram_mut(&mut self) -> &mut [u8] {
        &mut self.mem_mut()[Self::ROM_SIZE..]
    }
    /// The data read depends on how big ROM is [ZxMemory::ROM_SIZE].
    /// Results in an error when the rom data size is less than the `ROM_SIZE`.
    fn load_into_rom<R: Read>(&mut self, mut rd: R) -> Result<()> {
        let slice = self.rom_mut();
        rd.read_exact(slice).map_err(ZxMemoryError::Io)
    }
    /// Returns an iterator of mutable memory page slices [PageMutSlice] intersecting with a given address range.
    fn page_slice_iter_mut<'a, A: RangeBounds<u16>>(
            &'a mut self,
            address_range: A
        ) -> Result<MemPageMutIter<'a, Self>>
    {
        let range = normalize_address_range(address_range, 0, Self::RAMTOP)
                    .map_err(|_| ZxMemoryError::AddressRangeNotSupported)?;
        let cursor = range.start;
        let end = range.end;
        Ok(MemPageMutIter { mem: self, cursor, end })
    }
    /// Reads data into a paged-in memory area at the given address range.
    fn load_into_mem<A: RangeBounds<u16>, R: Read>(&mut self, address_range: A, mut rd: R) -> Result<()> {
        for page in self.page_slice_iter_mut(address_range)? {
            match page {
                PageMutSlice::Rom(slice)|PageMutSlice::Ram(slice) => {
                    rd.read_exact(slice).map_err(ZxMemoryError::Io)
                }
            }?
        }
        Ok(())
    }
    /// Fills currently paged-in pages with the data produced by the closure F.
    ///
    /// Usefull to fill RAM with random bytes.
    /// Provide the address range, trying to write into the address above RAMTOP results in an Error.
    fn fill_mem<R, F>(&mut self, address_range: R, mut f: F) -> Result<()>
    where R: RangeBounds<u16>, F: FnMut() -> u8
    {
        for page in self.page_slice_iter_mut(address_range)? {
            match page {
                PageMutSlice::Rom(slice)|PageMutSlice::Ram(slice) => {
                    for p in slice.iter_mut() {
                        *p = f()
                    }
                }
            }
        }
        Ok(())
    }
}
/// Implements an iterator of [PageMutSlice]s. See [ZxMemory::page_slice_iter_mut].
pub struct MemPageMutIter<'a, Z> {
    mem: &'a mut Z,
    cursor: usize,
    end: usize
}

impl<'a, Z: ZxMemory> Iterator for MemPageMutIter<'a, Z> {
    type Item = PageMutSlice<'a>;
    fn next(&mut self) -> Option<PageMutSlice<'a>> {
        let cursor = self.cursor;
        let end = self.end;
        if cursor < end {
            let MemPageOffset { kind, index, offset } = self.mem.page_index_at(cursor as u16).unwrap();
            let offset = offset as usize;
            let page = self.mem.page_mut(index).unwrap();
            // we are borrowing from mem with 'a bound to mem, not self
            let page = unsafe { core::mem::transmute::<&mut[u8], &'a mut[u8]>(page) };
            let read_len = end - cursor;
            let read_end = page.len().min(offset + read_len);
            let read_page = &mut page[offset..read_end];
            self.cursor += read_page.len();
            match kind {
                MemoryKind::Rom => {
                    Some(PageMutSlice::Rom(read_page))
                },
                MemoryKind::Ram => {
                    Some(PageMutSlice::Ram(read_page))
                }
            }
        }
        else {
            None
        }
    }
}

enum AddressRangeError {
    StartBoundTooLow,
    EndBoundTooHigh
}

fn normalize_address_range<R: RangeBounds<u16>>(
        range: R,
        min_inclusive: u16,
        max_inclusive: u16
    ) -> core::result::Result<Range<usize>, AddressRangeError>
{
    let start = match range.start_bound() {
        Bound::Included(start) => if *start < min_inclusive {
                return Err(AddressRangeError::StartBoundTooLow)
            } else { *start as usize },
        Bound::Excluded(start) => if *start < min_inclusive.saturating_sub(1) {
                return Err(AddressRangeError::StartBoundTooLow)
            } else { *start as usize + 1 },
        Bound::Unbounded => min_inclusive as usize
    };
    let end = match range.end_bound() {
        Bound::Included(end) => if *end > max_inclusive {
                return Err(AddressRangeError::EndBoundTooHigh)
            } else { *end as usize + 1},
        Bound::Excluded(end) => if *end > max_inclusive.saturating_add(1) {
                return Err(AddressRangeError::EndBoundTooHigh)
            } else { *end as usize },
        Bound::Unbounded => max_inclusive as usize + 1
    };
    Ok(start..end)
}
