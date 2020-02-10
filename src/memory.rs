use core::ops::Range;
use core::ops::{Bound, RangeBounds};
use std::io::Read;

bitflags! {
    #[derive(Default)]
    pub struct MemoryFeatures: u8 {
        const ROM16K   = 0x01; // 2 ROMS available
        const ROM32K   = 0x02; // 4 ROMS available
        const RAM_BANKS_16K = 0x04; // 16k ram banks
        const RAM_BANKS_8K  = 0x08; // 8k ram banks
        const NONE = MemoryFeatures::empty().bits;
    }
}

#[derive(Debug)]
pub enum ZxMemoryError {
    PageOutOfRange,
    AddressRangeNotSupported,
    Unwritable,
    Io(std::io::Error)
}

#[derive(Clone, Copy, Debug)]
pub enum MemoryPage {
    Rom {index: u8, offset: u16},
    Ram {index: u8, offset: u16}
}

#[derive(Debug)]
pub enum PageSlice<'a> {
    Rom(&'a mut [u8]),
    Ram(&'a mut [u8])
}

pub type Result<T> = core::result::Result<T, ZxMemoryError>;

pub trait ZxMemory: Sized {
    const RAMTOP: u16;
    const FEATURES: MemoryFeatures;
    /// Page should be a supported page rom index.
    fn rom_page_ref(&self, rom_page: u8) -> Result<&[u8]>;
    /// Page should be a supported page ram index.
    fn ram_page_ref(&self, ram_page: u8) -> Result<&[u8]>;
    /// Page should be a supported page rom index.
    fn rom_page_mut(&mut self, rom_page: u8) -> Result<& mut[u8]>;
    /// Page should be a supported page ram index.
    fn ram_page_mut(&mut self, ram_page: u8) -> Result<& mut[u8]>;
    /// Page should be a supported page ram index and the address range should be a supported page-able address range.
    fn map_ram_page<A: RangeBounds<u16>>(&mut self, ram_page: u8, address_range: A) -> Result<()>;
    /// Page should be a supported page rom index and the address range should be a supported page-able address range.
    fn map_rom_page<A: RangeBounds<u16>>(&mut self, rom_page: u8, address_range: A) -> Result<()>;
    /// Page should be a supported page ram index and the address range should be a supported page-able address range.
    fn page_index_at(&mut self, address: u16) -> Result<MemoryPage>;
    /// Get view of the screen_page memory.
    fn screen_ref(&self, screen_page: u8) -> Result<&[u8]>;
    /// If addr is above RAMTOP the function should return std::u8::MAX.
    fn read(&self, addr: u16) -> u8;
    /// If addr is above RAMTOP the function should return std::u16::MAX.
    fn read16(&self, addr: u16) -> u16;
    /// If addr is above RAMTOP the function should do nothing.
    fn write(&mut self, addr: u16, val: u8);
    /// If addr is above RAMTOP the function should do nothing.
    fn write16(&mut self, addr: u16, val: u16);
    /// The data read depends on how big rom pages are.
    /// Results in an error when the rom data size is less than the rom page size or invalid page index is given.
    fn load_into_rom_page<R: Read>(&mut self, rom_page: u8, mut rd: R) -> Result<()> {
        let slice = self.rom_page_mut(rom_page)?;
        rd.read_exact(slice).map_err(ZxMemoryError::Io)?;
        Ok(())
    }

    fn for_each_page<A, F>(&mut self, address_range: A, mut f: F) -> Result<()>
    where A: RangeBounds<u16>, F: FnMut(PageSlice<'_>) -> Result<()>
    {
        let range = normalize_address_range(address_range, 0, Self::RAMTOP).map_err(|_| ZxMemoryError::AddressRangeNotSupported)?;
        let mut cursor = range.start;
        let end = range.end;
        while cursor < end {
            let page = self.page_index_at(cursor as u16)?;
            match page {
                MemoryPage::Rom { index, offset } => {
                    let offset = offset as usize;
                    let page = self.rom_page_mut(index)?;
                    let read_len = end - cursor;
                    let read_end = page.len().min(offset + read_len);
                    let read_page = &mut page[offset..read_end];
                    cursor += read_page.len();
                    f(PageSlice::Rom(read_page))?;
                },
                MemoryPage::Ram { index, offset } => {
                    let offset = offset as usize;
                    let page = self.ram_page_mut(index)?;
                    let read_len = end - cursor;
                    let read_end = page.len().min(offset + read_len);
                    let read_page = &mut page[offset..read_end];
                    cursor += read_page.len();
                    f(PageSlice::Ram(read_page))?;
                }
            }
        }
        Ok(())
    }

    /// Reads data into a paged-in memory area at the given address range.
    /// Attempting to write into the ROM area or above the RAMTOP results in an Error.
    fn load_into_ram<A: RangeBounds<u16>, R: Read>(&mut self, address_range: A, mut rd: R) -> Result<()> {
        self.for_each_page(address_range, |page| match page {
            PageSlice::Ram(slice) => {
                rd.read_exact(slice).map_err(ZxMemoryError::Io)
            },
            PageSlice::Rom(_) => Err(ZxMemoryError::Unwritable)
        })
    }
    /// Fills currently paged-in RAM pages with the data produced by the closure F. Usefull to fill the RAM with random bytes.
    /// Provide the RAM address, trying to write into the address above RAMTOP results in an Error.
    /// The function is skipping ROM paged-in areas.
    fn fill_ram<R, F>(&mut self, address_range: R, mut f: F) -> Result<()>
    where R: RangeBounds<u16>, F: FnMut() -> u8
    {
        self.for_each_page(address_range, |page| match page {
            PageSlice::Ram(slice) => {
                for p in slice.iter_mut() {
                    *p = f()
                }
                Ok(())
            },
            PageSlice::Rom(_) => Ok(()),
        })
    }
}

///////////////////////////////////////////////////////////////////
///////////////////////////////////////////////////////////////////

#[derive(Clone)]
pub struct Memory16k {
    mem: Box<[u8;0x8000]>
}

#[derive(Clone)]
pub struct Memory48k {
    mem: Box<[u8;0x10000]>
}

#[derive(Clone)]
pub struct Memory64k {
    mem: Box<[u8;0x10000]>
}

impl Default for Memory16k {
    fn default() -> Self {
        Memory16k { mem: Box::new([0; 0x8000]) }
    }
}

impl Default for Memory48k {
    fn default() -> Self {
        Memory48k { mem: Box::new([0;0x10000]) }
    }
}

impl Default for Memory64k {
    fn default() -> Self {
        Memory64k { mem: Box::new([0;0x10000]) }
    }
}

pub trait SinglePageMemory {
    const ROMSIZE: u16;
    const ROMTOP: u16;
    const RAMBOT: u16;
    const RAMTOP: u16;
    fn as_slice(&self) -> &[u8];
    fn as_mut_slice(&mut self) -> &mut[u8];

    #[inline(always)]
    fn mem(&self) -> *const u8 {
        self.as_slice().as_ptr()
    }

    #[inline(always)]
    fn mem_mut(&mut self) -> *mut u8 {
        self.as_mut_slice().as_mut_ptr()
    }
}

impl SinglePageMemory for Memory16k {
    const ROMSIZE: u16 = 0x4000;
    const ROMTOP: u16 = Self::ROMSIZE-1;
    const RAMBOT: u16 = Self::ROMTOP+1;
    const RAMTOP: u16 = 0x7FFF;

    fn as_slice(&self) -> &[u8] {
        &*self.mem
    }

    fn as_mut_slice(&mut self) -> &mut[u8] {
        &mut *self.mem
    }

    fn mem(&self) -> *const u8 {
        self.mem.as_ptr()
    }

    fn mem_mut(&mut self) -> *mut u8 {
        self.mem.as_mut_ptr()
    }
}

impl SinglePageMemory for Memory48k {
    const ROMSIZE: u16 = 0x4000;
    const ROMTOP: u16 = Self::ROMSIZE-1;
    const RAMBOT: u16 = Self::ROMTOP+1;
    const RAMTOP: u16 = 0xFFFF;

    fn as_slice(&self) -> &[u8] {
        &*self.mem
    }

    fn as_mut_slice(&mut self) -> &mut[u8] {
        &mut *self.mem
    }

    fn mem(&self) -> *const u8 {
        self.mem.as_ptr()
    }

    fn mem_mut(&mut self) -> *mut u8 {
        self.mem.as_mut_ptr()
    }
}

impl SinglePageMemory for Memory64k {
    const ROMSIZE: u16 = 0x4000;
    const ROMTOP: u16 = Self::ROMSIZE-1;
    const RAMBOT: u16 = 0x0000;
    const RAMTOP: u16 = 0xFFFF;

    fn as_slice(&self) -> &[u8] {
        &*self.mem
    }

    fn as_mut_slice(&mut self) -> &mut[u8] {
        &mut *self.mem
    }

    fn mem(&self) -> *const u8 {
        self.mem.as_ptr()
    }

    fn mem_mut(&mut self) -> *mut u8 {
        self.mem.as_mut_ptr()
    }
}

impl<M: SinglePageMemory> ZxMemory for M {
    const RAMTOP: u16 = M::RAMTOP;
    const FEATURES: MemoryFeatures = MemoryFeatures::NONE;

    #[inline(always)]
    fn read(&self, addr: u16) -> u8 {
        if addr <= Self::RAMTOP {
            unsafe {
                self.mem().offset(addr as isize).read()
            }
        }
        else {
            u8::max_value()
        }
    }

    #[allow(clippy::cast_ptr_alignment)]
    #[inline]
    fn read16(&self, addr: u16) -> u16 {
        match addr {
            a if a < Self::RAMTOP => unsafe {
                let ptr: *const u8 = self.mem().offset(a as isize);
                let ptr16 = ptr as *const u16;
                ptr16.read_unaligned().to_le()
            }
            a if a == Self::RAMTOP || a == std::u16::MAX => {
                self.read(a) as u16|(self.read(a.wrapping_add(1)) as u16) << 8
            }
            _ => {
                u16::max_value()
            }
        }
    }

    #[inline(always)]
    fn write(&mut self, addr: u16, val: u8) {
        if addr >= Self::RAMBOT && addr <= Self::RAMTOP  {
            unsafe {
                self.mem_mut().offset(addr as isize).write(val);
            }
        }
    }

    #[allow(clippy::cast_ptr_alignment)]
    #[inline]
    fn write16(&mut self, addr: u16, val: u16) {
        match addr {
            a if a >= Self::RAMBOT && a < Self::RAMTOP => unsafe {
                let ptr: *mut u8 = self.mem_mut().offset(a as isize);
                let ptr16 = ptr as *mut u16;
                ptr16.write_unaligned(val.to_le());
            }
            a if a == Self::ROMTOP => {
                self.write(a.wrapping_add(1), (val >> 8) as u8);
            }
            a if a == Self::RAMTOP => {
                self.write(a, val as u8);
                self.write(a.wrapping_add(1), (val >> 8) as u8);
            }
            _ => {}
        }
    }

    fn rom_page_ref(&self, rom_page: u8) -> Result<&[u8]> {
        if rom_page != 0 {
            return Err(ZxMemoryError::PageOutOfRange)
        }
        Ok(&self.as_slice()[0..=Self::ROMTOP as usize])
    }

    fn ram_page_ref(&self, ram_page: u8) -> Result<&[u8]> {
        if ram_page != 0 {
            return Err(ZxMemoryError::PageOutOfRange)
        }
        Ok(&self.as_slice()[Self::RAMBOT as usize..=Self::RAMTOP as usize])
    }

    fn rom_page_mut(&mut self, rom_page: u8) -> Result<&mut[u8]> {
        if rom_page != 0 {
            return Err(ZxMemoryError::PageOutOfRange)
        }
        Ok(&mut self.as_mut_slice()[0..=Self::ROMTOP as usize])
    }

    fn ram_page_mut(&mut self, ram_page: u8) -> Result<&mut[u8]> {
        if ram_page != 0 {
            return Err(ZxMemoryError::PageOutOfRange)
        }
        Ok(&mut self.as_mut_slice()[Self::RAMBOT as usize..=Self::RAMTOP as usize])
    }

    fn page_index_at(&mut self, address: u16) -> Result<MemoryPage> {
        match address {
            a if a <= Self::ROMTOP => Ok(MemoryPage::Rom {index: 0, offset: a}),
            a if a >= Self::RAMBOT && a <=Self::RAMTOP => Ok(MemoryPage::Ram {index: 0, offset: a - Self::RAMBOT}),
            _ => Err(ZxMemoryError::AddressRangeNotSupported)
        }
    }

    fn screen_ref(&self, screen_page: u8) -> Result<&[u8]> {
        if screen_page != 0 {
            return Err(ZxMemoryError::PageOutOfRange)
        }
        Ok(&self.as_slice()[0x4000..0x5B00])
    }

    fn map_ram_page<R: RangeBounds<u16>>(&mut self, ram_page: u8, address_range: R) -> Result<()> {
        if ram_page != 0 {
            return Err(ZxMemoryError::PageOutOfRange)
        }
        let range = normalize_address_range(address_range, Self::RAMBOT, Self::RAMTOP).
                    map_err(|_| ZxMemoryError::AddressRangeNotSupported)?;
        if range != (Self::RAMBOT as usize..Self::RAMTOP as usize) {
            return Err(ZxMemoryError::AddressRangeNotSupported)
        }
        Ok(())
    }

    fn map_rom_page<R: RangeBounds<u16>>(&mut self, rom_page: u8, address_range: R) -> Result<()> {
        if rom_page != 0 {
            return Err(ZxMemoryError::PageOutOfRange)
        }
        let range = normalize_address_range(address_range, 0, Self::ROMTOP).
                    map_err(|_| ZxMemoryError::AddressRangeNotSupported)?;
        if range != (0..Self::ROMTOP as usize) {
            return Err(ZxMemoryError::AddressRangeNotSupported)
        }
        Ok(())
    }
}

// struct Memory128k {
//     rom: usize,
//     ram: [usize;4],
//     banks: [Box<[u8;16384]>;8],
//     roms: [Box<[u8;16384]>;2]
// }

// struct Memory2a {
//     rom: usize,
//     ram: [usize;4],
//     banks: [Box<[u8;16384]>;8],
//     roms: [Box<[u8;16384]>;4]
// }

// struct MemoryTimex {
//     rom: usize,
//     ram: u8,
//     exrom_active: bool,
//     exrom: [Box<[u8;8192]>;8],
//     home: [Box<[u8;8192]>;8],
//     dock: [Box<[u8;8192]>;4]
// }
enum AddressRangeError {
    StartBoundTooLow,
    EndBoundTooHigh
}

fn normalize_address_range<R: RangeBounds<u16>>(range: R, min_inclusive: u16, max_inclusive: u16) -> core::result::Result<Range<usize>, AddressRangeError> {
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
