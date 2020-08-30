//! Memory API.
use core::fmt;
use core::ops::{Bound, Range, RangeBounds};
use std::rc::Rc;
use std::io::{self, Read};

mod extension;
#[cfg(feature = "snapshot")] pub mod serde;

pub use extension::*;

pub const MEM16K_SIZE : usize = 0x4000;
pub const MEM32K_SIZE : usize = 2 * MEM16K_SIZE;
pub const MEM48K_SIZE : usize = 3 * MEM16K_SIZE;
pub const MEM64K_SIZE : usize = 4 * MEM16K_SIZE;
pub const MEM128K_SIZE: usize = 8 * MEM16K_SIZE;
pub const MEM8K_SIZE  : usize = MEM16K_SIZE / 2;
pub const SCREEN_SIZE: u16 = 0x1B00;

/// Represents a single screen memory.
pub type ScreenArray = [u8;SCREEN_SIZE as usize];

/// Represents an external ROM as a shared pointer to a slice of bytes.
pub type ExRom = Rc<[u8]>;

#[non_exhaustive]
#[derive(Debug)]
pub enum ZxMemoryError {
    InvalidPageIndex,
    InvalidBankIndex,
    UnsupportedAddressRange,
    UnsupportedExRomPaging,
    InvalidExRomSize,
    Io(io::Error)
}

impl std::error::Error for ZxMemoryError {}

impl fmt::Display for ZxMemoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", match self {
            ZxMemoryError::InvalidPageIndex => "Memory page index is out of range",
            ZxMemoryError::InvalidBankIndex => "Memory bank index is out of range",
            ZxMemoryError::UnsupportedAddressRange => "Address range is not supported",
            ZxMemoryError::UnsupportedExRomPaging => "EX-ROM mapping is not supported",
            ZxMemoryError::InvalidExRomSize => "EX-ROM size is smaller than the memory page size",
            ZxMemoryError::Io(err) => return err.fmt(f)
        })
    }
}

impl From<ZxMemoryError> for io::Error {
    fn from(err: ZxMemoryError) -> Self {
        match err {
            ZxMemoryError::Io(err) => err,
            e => io::Error::new(io::ErrorKind::InvalidInput, e)
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

/// A type yielded by [ZxMemory::for_each_page_mut].
#[derive(Debug, PartialEq)]
pub enum PageMutSlice<'a> {
    Rom(&'a mut [u8]),
    Ram(&'a mut [u8])
}

impl<'a> PageMutSlice<'a> {
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        match self {
            PageMutSlice::Rom(slice)|PageMutSlice::Ram(slice) => slice
        }
    }

    pub fn into_mut_slice(self) -> &'a mut[u8] {
        match self {
            PageMutSlice::Rom(slice)|PageMutSlice::Ram(slice) => slice
        }
    }

    pub fn is_rom(&self) -> bool {
        match self {
            PageMutSlice::Rom(..) => true,
            _ => false
        }
    }

    pub fn as_mut_rom(&mut self) -> Option<&mut [u8]> {
        match self {
            PageMutSlice::Rom(slice) => Some(slice),
            _ => None
        }
    }

    pub fn into_mut_rom(self) -> Option<&'a mut [u8]> {
        match self {
            PageMutSlice::Rom(slice) => Some(slice),
            _ => None
        }
    }

    pub fn is_ram(&self) -> bool {
        match self {
            PageMutSlice::Ram(..) => true,
            _ => false
        }
    }

    pub fn as_mut_ram(&mut self) -> Option<&mut [u8]> {
        match self {
            PageMutSlice::Ram(slice) => Some(slice),
            _ => None
        }
    }

    pub fn into_mut_ram(self) -> Option<&'a mut [u8]> {
        match self {
            PageMutSlice::Ram(slice) => Some(slice),
            _ => None
        }
    }
}

/// A type returned by some of [ZxMemory] methods.
pub type Result<T> = core::result::Result<T, ZxMemoryError>;

/// A trait for interfacing ZX Spectrum's various memory types.
pub trait ZxMemory {
    /// This is just a hint. Actual page sizes may vary.
    const PAGE_SIZE: usize = 0x4000;
    /// The size of the whole ROM, not just one bank.
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

    /// Resets memory banks.
    fn reset(&mut self);
    /// If `addr` is above `RAMTOP` the function should return [std::u8::MAX].
    fn read(&self, addr: u16) -> u8;
    /// If `addr` is above `RAMTOP` the function should return [std::u16::MAX].
    fn read16(&self, addr: u16) -> u16;
    /// Reads a byte from screen memory at the given `addr`.
    ///
    /// The screen banks are different from memory banks.
    /// E.g. for Spectrum 128k then screen bank `0` resides in a memory bank 5 and screen bank `1`
    /// resides in a memory bank 7. For 16k/48k Spectrum there is only one screen bank: `0`.
    ///
    /// `addr` is in screen address space (0 addresses the first byte of screen memory).
    ///
    /// # Panics
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
    /// Returns a reference to the screen memory.
    ///
    /// See [ZxMemory::read_screen] for an explanation of `screen_bank` argument.
    fn screen_ref(&self, screen_bank: usize) -> Result<&ScreenArray>;
    /// Returns a mutable reference to the screen memory.
    ///
    /// See [ZxMemory::read_screen] for an explanation of `screen_bank` argument.
    fn screen_mut(&mut self, screen_bank: usize) -> Result<&mut ScreenArray>;
    /// Returns an enum describing what kind of memory is currently paged at the specified `page`.
    ///
    /// If an EX-ROM bank is currently mapped at the specified `page` a [MemoryKind::Rom] will be returned
    /// in this instance.
    ///
    /// `page` should be less or equal to PAGES_MAX.
    fn page_kind(&self, page: u8) -> Result<MemoryKind>;
    /// Returns a tuple of an enum describing what kind of memory and which bank of that memory is
    /// currently paged at the specified `page`.
    ///
    /// # Note
    /// Unlike [ZxMemory::page_kind] this method ignores if an EX-ROM bank is currently mapped at the
    /// specified `page`. In this instance the returned value corresponds to a memory bank that would
    /// be mapped at `page` if the EX-ROM bank wasn't mapped.
    ///
    /// `page` should be less or equal to PAGES_MAX.
    fn page_bank(&self, page: u8) -> Result<(MemoryKind, usize)>;
    /// `page` should be less or equal to PAGES_MAX.
    fn page_ref(&self, page: u8) -> Result<&[u8]>;
    /// `page` should be less or equal to PAGES_MAX.
    fn page_mut(&mut self, page: u8) -> Result<&mut[u8]>;
    /// `rom_bank` should be less or equal to `ROM_BANKS_MAX`.
    fn rom_bank_ref(&self, rom_bank: usize) -> Result<&[u8]>;
    /// `rom_bank` should be less or equal to `ROM_BANKS_MAX`.
    fn rom_bank_mut(&mut self, rom_bank: usize) -> Result<&mut[u8]>;
    /// `ram_bank` should be less or equal to `RAM_BANKS_MAX`.
    fn ram_bank_ref(&self, ram_bank: usize) -> Result<&[u8]>;
    /// `ram_bank` should be less or equal to `RAM_BANKS_MAX`.
    fn ram_bank_mut(&mut self, ram_bank: usize) -> Result<&mut[u8]>;
    /// `rom_bank` should be less or equal to `ROM_BANKS_MAX` and `page` should be less or equal to PAGES_MAX.
    fn map_rom_bank(&mut self, rom_bank: usize, page: u8) -> Result<()>;
    /// Maps EX-ROM bank at the specified `page`.
    ///
    /// `exrom_bank` should be one of the attachable EX-ROMS and `page` should be less or equal to PAGES_MAX.
    ///
    /// Only one EX-ROM can be mapped at the same time. If an EX-ROM bank is already mapped when calling
    /// this function it will be unmapped first, regardless of the page the previous EX-ROM bank has been
    /// mapped at.
    ///
    /// Not all types of memory support attaching external ROMs.
    fn map_exrom(&mut self, _exrom_bank: ExRom, _page: u8) -> Result<()> {
        return Err(ZxMemoryError::UnsupportedExRomPaging)
    }
    /// Unmaps an external ROM if the currently mapped EX-ROM bank is the same as in the argument.
    /// Otherwise does nothing.
    fn unmap_exrom(&mut self, _exrom_bank: &ExRom) { }
    /// Returns `true` if an EX-ROM bank is currently being mapped at the specified memory `page`.
    fn is_exrom_at(&mut self, _page: u8) -> bool { false }
    /// Returns `true` if a specified EX-ROM bank is currently being mapped.
    fn has_mapped_exrom(&mut self, _exrom_bank: &ExRom) -> bool { false }
    /// `ram_bank` should be less or equal to `RAM_BANKS_MAX` and `page` should be less or equal to PAGES_MAX.
    fn map_ram_bank(&mut self, ram_bank: usize, page: u8) -> Result<()>;
    /// Returns `Ok(MemPageOffset)` if address is equal to or less than [ZxMemory::RAMTOP].
    fn page_index_at(&self, address: u16) -> Result<MemPageOffset> {
        let index = (address / Self::PAGE_SIZE as u16) as u8;
        let offset = address % Self::PAGE_SIZE as u16;
        let kind = self.page_kind(index)?;
        Ok(MemPageOffset {kind, index, offset})
    }
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
    /// Results in an error when the rom data size is less than the rom bank's size.
    fn load_into_rom_bank<R: Read>(&mut self, rom_bank: usize, mut rd: R) -> Result<()> {
        let slice = self.rom_bank_mut(rom_bank)?;
        rd.read_exact(slice).map_err(ZxMemoryError::Io)
    }
    /// Returns an iterator of memory page slice references intersecting with a given address range.
    ///
    /// # Errors
    /// May return an [ZxMemoryError::UnsupportedAddressRange].
    fn iter_pages<A: RangeBounds<u16>>(
            &self,
            address_range: A,
        ) -> Result<MemPageRefIter<'_, Self>> {
        let range = normalize_address_range(address_range, 0, Self::RAMTOP)
                    .map_err(|_| ZxMemoryError::UnsupportedAddressRange)?;
        let cursor = range.start;
        let end = range.end;
        Ok(MemPageRefIter { mem: self, cursor, end })
    }
    /// Iterates over mutable memory page slices [PageMutSlice] intersecting with a given address range
    /// passing the slices to the closure `f`.
    ///
    /// # Errors
    /// May return an [ZxMemoryError::UnsupportedAddressRange] or an error returned by the provided closure.
    fn for_each_page_mut<A: RangeBounds<u16>, F>(
            &mut self,
            address_range: A,
            mut f: F
        ) -> Result<()>
        where for<'a> F: FnMut(PageMutSlice<'a>) -> Result<()>
    {
        let range = normalize_address_range(address_range, 0, Self::RAMTOP)
                    .map_err(|_| ZxMemoryError::UnsupportedAddressRange)?;
        let cursor = range.start;
        let end = range.end;
        let iter = MemPageMutIter { mem: self, cursor, end };
        for page in iter {
            f(page)?
        }
        Ok(())
    }
    /// Reads data into a paged-in memory area at the given address range.
    fn load_into_mem<A: RangeBounds<u16>, R: Read>(&mut self, address_range: A, mut rd: R) -> Result<()> {
        self.for_each_page_mut(address_range, |page| {
            match page {
                PageMutSlice::Rom(slice)|PageMutSlice::Ram(slice) => {
                    rd.read_exact(slice).map_err(ZxMemoryError::Io)
                }
            }
        })
    }
    /// Fills currently paged-in pages with the data produced by the closure F.
    ///
    /// Usefull to fill RAM with random bytes.
    ///
    /// Provide the address range.
    ///
    /// *NOTE*: this will overwrite both ROM and RAM locations.
    fn fill_mem<R, F>(&mut self, address_range: R, mut f: F) -> Result<()>
    where R: RangeBounds<u16>, F: FnMut() -> u8
    {
        self.for_each_page_mut(address_range, |page| {
            for p in page.into_mut_slice().iter_mut() {
                *p = f()
            }
            Ok(())
        })
    }
}

pub struct MemPageRefIter<'a, Z: ?Sized> {
    mem: &'a Z,
    cursor: usize,
    end: usize
}

impl<'a, Z: ZxMemory + ?Sized> Iterator for MemPageRefIter<'a, Z> {
    type Item = &'a [u8];
    fn next(&mut self) -> Option<Self::Item> {
        let cursor = self.cursor;
        let end = self.end;
        if cursor < end {
            let MemPageOffset { index, offset, .. } = self.mem.page_index_at(cursor as u16).unwrap();
            let offset = offset as usize;
            let page = self.mem.page_ref(index).unwrap();
            let read_len = end - cursor;
            let read_end = page.len().min(offset + read_len);
            let read_page = &page[offset..read_end];
            self.cursor += read_page.len();
            Some(read_page)
        }
        else {
            None
        }
    }
}

struct MemPageMutIter<'a, Z: ?Sized> {
    mem: &'a mut Z,
    cursor: usize,
    end: usize
}

impl<'a, Z: ZxMemory + ?Sized> Iterator for MemPageMutIter<'a, Z> {
    type Item = PageMutSlice<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        let cursor = self.cursor;
        let end = self.end;
        if cursor < end {
            let MemPageOffset { kind, index, offset } = self.mem.page_index_at(cursor as u16).unwrap();
            let offset = offset as usize;
            let page = self.mem.page_mut(index).unwrap();
            // this may lead to yielding pages that view the same memory bank slices;
            // so this struct isn't public, and it's only used internally when no
            // two iterated pages are allowed to be accessed at once.
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
