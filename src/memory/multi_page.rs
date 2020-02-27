use core::ops::{Deref, DerefMut};
use core::ops::RangeBounds;
use core::ptr::NonNull;
use core::slice;

use super::{Result,
    ZxMemory,
    ZxMemoryError,
    // MemoryFeatures,
    MemoryKind,
    MemPageOffset,
    normalize_address_range};

/// A memory type with 128kb RAM memory.
pub type Memory128k = PagedMemory<MemRom2Ram8>;

#[derive(Clone, Default)]
pub struct PagedMemory<M>(M);

const BANK16K_SIZE: usize = 0x4000;
const BANK16K_MASK: u16 = (BANK16K_SIZE - 1) as u16;
const NUM_BANK16K_PAGES: usize = 4;

const NUM_MEM128_RAM_BANKS: usize = 8;
const NUM_MEM128_ROM_BANKS: usize = 2;
const MEM128_RAM_SIZE: usize = 0x20000;
const MEM128_ROM_SIZE: usize = 0x8000;
pub struct MemRom2Ram8 {
    mem: Box<[u8;MEM128_ROM_SIZE+MEM128_RAM_SIZE]>,
    pages: [NonNull<[u8;BANK16K_SIZE]>;NUM_BANK16K_PAGES]
}

pub enum WordPageOffset {
    Fits { page: u8, offset: u16 },
    Boundary { page: u8, offset: u16, next_page: u8 },
}

/// Methods and constants common for all multi-bank ROM and RAM memory types.
pub trait MemoryBanks {
    const ROMSIZE: usize;
    const BANK_SIZE: usize;
    const PAGES_MAX: u8;
    const SCR_BANKS_MAX: usize = 1;
    const RAM_BANKS_MAX: usize;
    const ROM_BANKS_MAX: usize;
    const SCR_BANK_OFFSETS: &'static [usize] = &[5*BANK16K_SIZE, 7*BANK16K_SIZE];
    fn as_slice(&self) -> &[u8];
    fn as_mut_slice(&mut self) -> &mut[u8];
    fn mem_page_ptr(&self, page: u8) -> *const u8;
    fn mem_page_mut_ptr(&mut self, page: u8) -> *mut u8;
    fn page_mem_offset(&self, page: u8) -> usize;
    fn reset_banks(&mut self);
    fn byte_address_to_page_offset(address: u16) -> (u8, u16);
    fn word_address_to_page_offset(address: u16) -> WordPageOffset;
    fn ram_bank_to_page(&mut self, bank: usize, page: u8);
    fn rom_bank_to_page(&mut self, bank: usize, page: u8);
}

impl Default for MemRom2Ram8 {
    fn default() -> Self {
        let mem = Box::new([0;MEM128_ROM_SIZE + MEM128_RAM_SIZE]);
        let pages = [cast_bank16(&mem[..BANK16K_SIZE]);NUM_BANK16K_PAGES];
        let mut mem = MemRom2Ram8 {
            mem,
            pages
        };
        mem.reset_banks();
        mem
    }
}

impl Clone for MemRom2Ram8 {
    fn clone(&self) -> Self {
        let mem = self.mem.clone();
        // FIXIT!!!!!!!!
        let mut pages = [NonNull::dangling();NUM_BANK16K_PAGES];
        for (i, p) in (0..NUM_BANK16K_PAGES as u8).zip(pages.iter_mut()) {
            let offset = self.page_mem_offset(i);
            *p = cast_bank16(&mem[offset..offset + BANK16K_SIZE]);
        }
        MemRom2Ram8 { mem, pages }
    }
}

impl MemoryBanks for MemRom2Ram8 {
    const ROMSIZE: usize = MEM128_ROM_SIZE;
    const BANK_SIZE: usize = BANK16K_SIZE;
    const PAGES_MAX: u8 = NUM_BANK16K_PAGES as u8 - 1;
    const SCR_BANKS_MAX: usize = 1;
    const RAM_BANKS_MAX: usize = NUM_MEM128_RAM_BANKS - 1;
    const ROM_BANKS_MAX: usize = NUM_MEM128_ROM_BANKS - 1;
    #[inline]
    fn as_slice(&self) -> &[u8] {
        self.mem.as_ref()
    }
    #[inline]
    fn as_mut_slice(&mut self) -> &mut[u8] {
        self.mem.as_mut()
    }
    #[inline]
    fn mem_page_ptr(&self, page: u8) -> *const u8 {
        self.pages[page as usize & 3].as_ptr() as *const u8
    }
    #[inline]
    fn mem_page_mut_ptr(&mut self, page: u8) -> *mut u8 {
        self.pages[page as usize & 3].as_ptr() as *mut u8
    }
    #[inline]
    fn page_mem_offset(&self, page: u8) -> usize {
        self.pages[page as usize & 3].as_ptr() as usize - self.mem.as_ptr() as usize
    }
    #[inline]
    fn reset_banks(&mut self) {
        self.rom_bank_to_page(0, 0);
        self.ram_bank_to_page(5, 1);
        self.ram_bank_to_page(2, 2);
        self.ram_bank_to_page(0, 3);
    }
    #[inline]
    fn byte_address_to_page_offset(address: u16) -> (u8, u16) {
        ((address >> (16 - 2)) as u8, address & BANK16K_MASK)
    }

    #[inline]
    fn word_address_to_page_offset(address: u16) -> WordPageOffset {
        let (page, offset) = ((address >> (16 - 2)) as u8, address & BANK16K_MASK);
        let next_page = (address.wrapping_add(1) >> (16 - 2)) as u8;
        if page == next_page {
            WordPageOffset::Fits {
                page, offset
            }
        }
        else {
            WordPageOffset::Boundary {
                page, offset, next_page
            }
        }
    }
    #[inline]
    fn ram_bank_to_page(&mut self, bank: usize, page: u8) {
        let offset = (bank & 7) * BANK16K_SIZE + MEM128_ROM_SIZE;
        self.pages[page as usize] = cast_bank16(&self.mem[offset..offset+BANK16K_SIZE]);
    }
    #[inline]
    fn rom_bank_to_page(&mut self, bank: usize, page: u8) {
        let offset = (bank & 1) * BANK16K_SIZE;
        self.pages[page as usize] = cast_bank16(&self.mem[offset..offset+BANK16K_SIZE]);
    }
}

impl<M: MemoryBanks> Deref for PagedMemory<M> {
    type Target = M;
    fn deref(&self) -> &M {
        &self.0
    }
}

impl<M: MemoryBanks> DerefMut for PagedMemory<M> {
    fn deref_mut(&mut self) -> &mut M {
        &mut self.0
    }
}

#[inline(always)]
fn cast_bank16(slice: &[u8]) -> NonNull<[u8;BANK16K_SIZE]> {
    assert_eq!(slice.len(), BANK16K_SIZE);
    let ptr = slice.as_ptr() as *const [u8; BANK16K_SIZE];
    // SAFETY: ok because we just checked that the length fits
    NonNull::from(unsafe { &(*ptr) })
} 


impl<M: MemoryBanks> ZxMemory for PagedMemory<M> {
    const PAGE_SIZE: usize = BANK16K_SIZE;
    const ROM_SIZE: usize = M::ROMSIZE;
    const RAMTOP: u16 = u16::max_value();
    const PAGES_MAX: u8 = M::PAGES_MAX;
    const SCR_BANKS_MAX: usize = M::SCR_BANKS_MAX;
    const ROM_BANKS_MAX: usize = M::ROM_BANKS_MAX;
    const RAM_BANKS_MAX: usize = M::RAM_BANKS_MAX;
    // const FEATURES: MemoryFeatures = MemoryFeatures::NONE;

    #[inline(always)]
    fn read(&self, addr: u16) -> u8 {
        let (page, offset) = M::byte_address_to_page_offset(addr);
        unsafe {
            self.mem_page_ptr(page).add(offset as usize).read()
        }
    }

    #[allow(clippy::cast_ptr_alignment)]
    #[inline]
    fn read16(&self, addr: u16) -> u16 {
        match M::word_address_to_page_offset(addr) {
            WordPageOffset::Fits { page, offset } => unsafe {
                let ptr: *const u8 = self.mem_page_ptr(page).add(offset as usize);
                let ptr16 = ptr as *const u16;
                ptr16.read_unaligned().to_le()
            }
            WordPageOffset::Boundary { page, offset, next_page } => unsafe {
                let lo = self.mem_page_ptr(page).add(offset as usize).read();
                let hi = self.mem_page_ptr(next_page).read();
                u16::from_le_bytes([lo, hi])
            }
        }
    }

    #[inline(always)]
    fn write(&mut self, addr: u16, val: u8) {
        let (page, offset) = M::byte_address_to_page_offset(addr);
        unsafe {
            self.mem_page_mut_ptr(page).add(offset as usize).write(val);
        }
    }

    #[allow(clippy::cast_ptr_alignment)]
    #[inline]
    fn write16(&mut self, addr: u16, val: u16) {
        match M::word_address_to_page_offset(addr) {
            WordPageOffset::Fits { page, offset } => unsafe {
                let ptr: *mut u8 = self.mem_page_mut_ptr(page).add(offset as usize);
                let ptr16 = ptr as *mut u16;
                ptr16.write_unaligned(val.to_le());
            }
            WordPageOffset::Boundary { page, offset, next_page } => unsafe {
                let [lo, hi] = val.to_le_bytes();
                self.mem_page_mut_ptr(page).add(offset as usize).write(lo);
                self.mem_page_mut_ptr(next_page).write(hi);
            }
        }
    }

    #[inline]
    fn mem_ref(&self) -> &[u8] {
        self.as_slice()
    }
    #[inline]
    fn mem_mut(&mut self) -> &mut[u8] {
        self.as_mut_slice()
    }
    #[inline]
    fn screen_ref(&self, screen_bank: usize) -> Result<&[u8]> {
        if screen_bank >= M::SCR_BANK_OFFSETS.len() {
            return Err(ZxMemoryError::InvalidBankIndex)
        }
        let offset = M::SCR_BANK_OFFSETS[screen_bank];
        Ok(&self.as_slice()[offset..offset+0x1B00])
    }
    #[inline]
    fn screen_mut(&mut self, screen_bank: usize) -> Result<&mut [u8]> {
        if screen_bank >= M::SCR_BANK_OFFSETS.len() {
            return Err(ZxMemoryError::InvalidBankIndex)
        }
        let offset = M::SCR_BANK_OFFSETS[screen_bank];
        Ok(&mut self.as_mut_slice()[offset..offset+0x1B00])
    }
    #[inline]
    fn page_kind(&self, page: u8) -> Result<MemoryKind> {
        if page > Self::PAGES_MAX {
            Err(ZxMemoryError::InvalidPageIndex)
        }
        else if self.page_mem_offset(page) < Self::ROM_SIZE {
            Ok(MemoryKind::Rom)
        }
        else {
            Ok(MemoryKind::Ram)
        }
    }
    #[inline]
    fn page_ref(&self, page: u8) -> Result<&[u8]> {
        if page > Self::PAGES_MAX {
            return Err(ZxMemoryError::InvalidPageIndex)
        }
        Ok(unsafe {
            slice::from_raw_parts(self.mem_page_ptr(page), Self::PAGE_SIZE)
        })
    }
    #[inline]
    fn page_mut(&mut self, page: u8) -> Result<& mut[u8]> {
        if page > Self::PAGES_MAX {
            return Err(ZxMemoryError::InvalidPageIndex)
        }
        Ok(unsafe {
            slice::from_raw_parts_mut(self.mem_page_mut_ptr(page), Self::PAGE_SIZE)
        })
    }
    fn rom_bank_ref(&self, rom_bank: usize) -> Result<&[u8]> {
        if rom_bank > Self::ROM_BANKS_MAX {
            return Err(ZxMemoryError::InvalidBankIndex)
        }
        let offset = M::BANK_SIZE * rom_bank;
        Ok(&self.as_slice()[offset..offset+M::BANK_SIZE])
    }

    fn rom_bank_mut(&mut self, rom_bank: usize) -> Result<&[u8]> {
        if rom_bank > Self::ROM_BANKS_MAX {
            return Err(ZxMemoryError::InvalidBankIndex)
        }
        let offset = M::BANK_SIZE * rom_bank;
        Ok(&self.as_mut_slice()[offset..offset+M::BANK_SIZE])
    }

    fn ram_bank_ref(&self, ram_bank: usize) -> Result<&[u8]> {
        if ram_bank > Self::RAM_BANKS_MAX {
            return Err(ZxMemoryError::InvalidBankIndex)
        }
        let offset = M::BANK_SIZE * ram_bank + M::ROMSIZE;
        Ok(&self.as_slice()[offset..offset+M::BANK_SIZE])
    }


    fn ram_bank_mut(&mut self, ram_bank: usize) -> Result<&[u8]> {
        if ram_bank > Self::RAM_BANKS_MAX {
            return Err(ZxMemoryError::InvalidBankIndex)
        }
        let offset = M::BANK_SIZE * ram_bank + M::ROMSIZE;
        Ok(&self.as_mut_slice()[offset..offset+M::BANK_SIZE])
    }

    fn map_rom_bank(&mut self, rom_bank: usize, page: u8) -> Result<()> {
        if rom_bank > Self::ROM_BANKS_MAX {
            return Err(ZxMemoryError::InvalidBankIndex)
        }
        if page > Self::PAGES_MAX {
            return Err(ZxMemoryError::InvalidPageIndex)
        }
        self.rom_bank_to_page(rom_bank, page);
        Ok(())
    }

    fn map_ram_bank(&mut self, ram_bank: usize, page: u8) -> Result<()> {
        if ram_bank > Self::RAM_BANKS_MAX {
            return Err(ZxMemoryError::InvalidBankIndex)
        }
        if page > Self::PAGES_MAX {
            return Err(ZxMemoryError::InvalidPageIndex)
        }
        self.ram_bank_to_page(ram_bank, page);
        Ok(())
    }

    fn page_index_at(&self, address: u16) -> Result<MemPageOffset> {
        let index = (address / Self::PAGE_SIZE as u16) as u8;
        let offset = address % Self::PAGE_SIZE as u16;
        let kind = self.page_kind(index)?;
        Ok(MemPageOffset {kind, index, offset})
    }
}
