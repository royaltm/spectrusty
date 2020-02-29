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

/// A memory type with 128kb RAM and 32kb ROM.
pub type Memory128k = PagedMemory<MemRom2Ram8>;

#[derive(Clone, Default)]
pub struct PagedMemory<M>(M);

const BANK16K_SIZE: usize = 0x4000;
const BANK16K_MASK: u16 = (BANK16K_SIZE - 1) as u16;
const NUM_BANK16K_PAGES: usize = 4;
const SCREEN_SIZE: usize = 0x1B00;

const NUM_MEM128_RAM_BANKS: usize = 8;
const NUM_MEM128_ROM_BANKS: usize = 2;
const MEM128_RAM_SIZE: usize = 8 * BANK16K_SIZE;
const MEM128_ROM_SIZE: usize = 2 * BANK16K_SIZE;

pub struct MemRom2Ram8 {
    mem: Box<[u8; MEM128_ROM_SIZE + MEM128_RAM_SIZE]>,
    pages: [NonNull<[u8; BANK16K_SIZE]>; NUM_BANK16K_PAGES],
    ro_pages: u8
}

#[derive(Clone, Copy, Debug, PartialEq)]
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
    const SCR_BANK_OFFSETS: &'static [usize];
    fn as_slice(&self) -> &[u8];
    fn as_mut_slice(&mut self) -> &mut[u8];
    fn as_ptr(&self) -> *const u8;
    fn mem_page_ptr(&self, page: u8) -> *const u8;
    fn mem_page_mut_ptr(&mut self, page: u8) -> *mut u8;
    fn page_mem_offset(&self, page: u8) -> usize;
    fn reset_banks(&mut self);
    fn byte_address_to_page_offset(address: u16) -> (u8, u16);
    fn word_address_to_page_offset(address: u16) -> WordPageOffset;
    fn rom_bank_to_page(&mut self, bank: usize, page: u8);
    fn ram_bank_to_page(&mut self, bank: usize, page: u8);
    fn is_writable_page(&self, page: u8) -> bool;
}

impl Default for MemRom2Ram8 {
    fn default() -> Self {
        let mem = Box::new([0;MEM128_ROM_SIZE + MEM128_RAM_SIZE]);
        let pages = [NonNull::dangling();NUM_BANK16K_PAGES];
        let mut mem = MemRom2Ram8 {
            mem,
            pages,
            ro_pages: 0
        };
        mem.reset_banks();
        mem
    }
}

impl Clone for MemRom2Ram8 {
    fn clone(&self) -> Self {
        let mem = self.mem.clone();
        let mut pages = [NonNull::dangling();NUM_BANK16K_PAGES];
        let ro_pages = self.ro_pages;
        for (i, p) in (0..NUM_BANK16K_PAGES as u8).zip(pages.iter_mut()) {
            let offset = self.page_mem_offset(i);
            *p = cast_bank16(&mem[offset..offset + BANK16K_SIZE]);
        }
        MemRom2Ram8 { mem, pages, ro_pages }
    }
}

#[inline(always)]
fn ro_flag_mask(page: u8) -> u8 {
    1 << page
}

impl MemoryBanks for MemRom2Ram8 {
    const ROMSIZE: usize = MEM128_ROM_SIZE;
    const BANK_SIZE: usize = BANK16K_SIZE;
    const PAGES_MAX: u8 = NUM_BANK16K_PAGES as u8 - 1;
    const SCR_BANKS_MAX: usize = 1;
    const RAM_BANKS_MAX: usize = NUM_MEM128_RAM_BANKS - 1;
    const ROM_BANKS_MAX: usize = NUM_MEM128_ROM_BANKS - 1;
    const SCR_BANK_OFFSETS: &'static [usize] = &[Self::ROMSIZE + 5*Self::BANK_SIZE, Self::ROMSIZE + 7*Self::BANK_SIZE];
    #[inline]
    fn as_slice(&self) -> &[u8] {
        self.mem.as_ref()
    }
    #[inline]
    fn as_mut_slice(&mut self) -> &mut[u8] {
        self.mem.as_mut()
    }
    #[inline]
    fn as_ptr(&self) -> *const u8 {
        self.mem.as_ptr()
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
    fn rom_bank_to_page(&mut self, bank: usize, page: u8) {
        let offset = (bank & 1) * BANK16K_SIZE;
        self.ro_pages |= ro_flag_mask(page);
        self.pages[page as usize] = cast_bank16(&self.mem[offset..offset+BANK16K_SIZE]);
    }

    #[inline]
    fn ram_bank_to_page(&mut self, bank: usize, page: u8) {
        let offset = (bank & 7) * BANK16K_SIZE + MEM128_ROM_SIZE;
        self.ro_pages &= !ro_flag_mask(page);
        self.pages[page as usize] = cast_bank16(&self.mem[offset..offset+BANK16K_SIZE]);
    }

    #[inline(always)]
    fn is_writable_page(&self, page: u8) -> bool {
        self.ro_pages & ro_flag_mask(page) == 0
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
        if self.is_writable_page(page) {
            unsafe {
                self.mem_page_mut_ptr(page).add(offset as usize).write(val);
            }
        }
    }

    #[allow(clippy::cast_ptr_alignment)]
    #[inline]
    fn write16(&mut self, addr: u16, val: u16) {
        match M::word_address_to_page_offset(addr) {
            WordPageOffset::Fits { page, offset } => if self.is_writable_page(page) {
                unsafe {
                    let ptr: *mut u8 = self.mem_page_mut_ptr(page).add(offset as usize);
                    let ptr16 = ptr as *mut u16;
                    ptr16.write_unaligned(val.to_le());
                }
            }
            WordPageOffset::Boundary { page, offset, next_page } => {
                let [lo, hi] = val.to_le_bytes();
                if self.is_writable_page(page) {
                    unsafe {  self.mem_page_mut_ptr(page).add(offset as usize).write(lo); }
                }
                if self.is_writable_page(next_page) {
                    unsafe {  self.mem_page_mut_ptr(next_page).write(hi); }
                }
            }
        }
    }

    #[inline]
    fn read_screen(&self, screen_bank: usize, addr: u16) -> u8 {
        let offset = M::SCR_BANK_OFFSETS[screen_bank];
        if addr < 0x1B00 {
            unsafe {
                self.as_ptr().add(offset + addr as usize).read()
            }
        }
        else {
            panic!("trying to read outside of screen area");
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
        Ok(&self.as_slice()[offset..offset + SCREEN_SIZE])
    }
    #[inline]
    fn screen_mut(&mut self, screen_bank: usize) -> Result<&mut [u8]> {
        if screen_bank >= M::SCR_BANK_OFFSETS.len() {
            return Err(ZxMemoryError::InvalidBankIndex)
        }
        let offset = M::SCR_BANK_OFFSETS[screen_bank];
        Ok(&mut self.as_mut_slice()[offset..offset + SCREEN_SIZE])
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

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{Rng, SeedableRng};
    use rand::rngs::SmallRng;
    #[test]
    fn memory_banks_work() {
        assert_eq!(MemRom2Ram8::ROMSIZE, 0x8000);
        assert_eq!(MemRom2Ram8::BANK_SIZE, 0x4000);
        assert_eq!(MemRom2Ram8::PAGES_MAX, 3);
        assert_eq!(MemRom2Ram8::SCR_BANKS_MAX, 1);
        assert_eq!(MemRom2Ram8::RAM_BANKS_MAX, 7);
        assert_eq!(MemRom2Ram8::ROM_BANKS_MAX, 1);
        let mut mem = Memory128k::default();
        let slab = mem.mem_mut();
        assert_eq!(slab.len(), 0x28000);
        for chunk in slab[0..0x4000].chunks_mut(4) {
            chunk.copy_from_slice(b"ROM0");
        }
        for chunk in slab[0x4000..0x8000].chunks_mut(4) {
            chunk.copy_from_slice(b"ROM1");
        }
        let filler = &mut [0u8;4];
        filler.copy_from_slice(b"RAM0");
        for (i, bank) in slab[0x8000..].chunks_mut(0x4000).enumerate() {
            filler[3] = '0' as u8 + i as u8;
            for chunk in bank.chunks_mut(4) {
                chunk.copy_from_slice(filler);
            }
        }
        assert_eq!(mem.page_mem_offset(0), 0x0000);
        assert_eq!(mem.page_mem_offset(1), 0x8000 + 5*0x4000);
        assert_eq!(mem.page_mem_offset(2), 0x8000 + 2*0x4000);
        assert_eq!(mem.page_mem_offset(3), 0x8000);
        assert_eq!(MemRom2Ram8::byte_address_to_page_offset(0), (0, 0));
        assert_eq!(MemRom2Ram8::byte_address_to_page_offset(0x3fff), (0, 0x3fff));
        assert_eq!(MemRom2Ram8::byte_address_to_page_offset(0x4000), (1, 0));
        assert_eq!(MemRom2Ram8::byte_address_to_page_offset(0x7fff), (1, 0x3fff));
        assert_eq!(MemRom2Ram8::byte_address_to_page_offset(0x8000), (2, 0));
        assert_eq!(MemRom2Ram8::byte_address_to_page_offset(0xbfff), (2, 0x3fff));
        assert_eq!(MemRom2Ram8::byte_address_to_page_offset(0xc000), (3, 0));
        assert_eq!(MemRom2Ram8::byte_address_to_page_offset(0xffff), (3, 0x3fff));
        assert_eq!(MemRom2Ram8::word_address_to_page_offset(0), WordPageOffset::Fits { page: 0, offset: 0 });
        assert_eq!(MemRom2Ram8::word_address_to_page_offset(0x3fff),
                                        WordPageOffset::Boundary { page: 0, offset: 0x3fff, next_page: 1 });
        assert_eq!(MemRom2Ram8::word_address_to_page_offset(0x4000), WordPageOffset::Fits { page: 1, offset: 0 });
        assert_eq!(MemRom2Ram8::word_address_to_page_offset(0x7fff),
                                        WordPageOffset::Boundary { page: 1, offset: 0x3fff, next_page: 2 });
        assert_eq!(MemRom2Ram8::word_address_to_page_offset(0x8000), WordPageOffset::Fits { page: 2, offset: 0 });
        assert_eq!(MemRom2Ram8::word_address_to_page_offset(0xbfff),
                                        WordPageOffset::Boundary { page: 2, offset: 0x3fff, next_page: 3 });
        assert_eq!(MemRom2Ram8::word_address_to_page_offset(0xc000), WordPageOffset::Fits { page: 3, offset: 0 });
        assert_eq!(MemRom2Ram8::word_address_to_page_offset(0xffff),
                                        WordPageOffset::Boundary { page: 3, offset: 0x3fff, next_page: 0 });
        fn test_page(mem: &Memory128k, page: u8, patt: &[u8;4]) {
            let chunks = mem.page_ref(page).unwrap().chunks(4);
            assert_eq!(chunks.len(), 0x1000);
            for chunk in chunks {
                assert_eq!(chunk, patt);
            }
        }
        fn test_rom_bank(mem: &Memory128k, bank: usize, patt: &[u8;4]) {
            let chunks = mem.rom_bank_ref(bank).unwrap().chunks(4);
            assert_eq!(chunks.len(), 0x1000);
            for chunk in chunks {
                assert_eq!(chunk, patt);
            }
        }
        fn test_ram_bank(mem: &Memory128k, bank: usize, patt: &[u8;4]) {
            let chunks = mem.ram_bank_ref(bank).unwrap().chunks(4);
            assert_eq!(chunks.len(), 0x1000);
            for chunk in chunks {
                assert_eq!(chunk, patt);
            }
        }
        fn test_screen_bank(mem: &Memory128k, bank: usize, patt: &[u8;4]) {
            let chunks = mem.screen_ref(bank).unwrap().chunks(4);
            assert_eq!(chunks.len(), SCREEN_SIZE/4);
            for chunk in chunks {
                assert_eq!(chunk, patt);
            }
        }
        test_page(&mem, 0, b"ROM0");
        test_page(&mem, 1, b"RAM5");
        test_page(&mem, 2, b"RAM2");
        test_page(&mem, 3, b"RAM0");
        assert_eq!(mem.read16(0x3fff), u16::from_le_bytes(['0' as u8, 'R' as u8]));
        mem.map_rom_bank(1, 0).unwrap();
        test_page(&mem, 0, b"ROM1");
        test_page(&mem, 1, b"RAM5");
        test_page(&mem, 2, b"RAM2");
        test_page(&mem, 3, b"RAM0");
        assert_eq!(mem.page_mem_offset(0), 0x4000);
        assert_eq!(mem.read16(0x3fff), u16::from_le_bytes(['1' as u8, 'R' as u8]));
        assert_eq!(mem.read16(0x7fff), u16::from_le_bytes(['5' as u8, 'R' as u8]));
        assert_eq!(mem.read16(0xbfff), u16::from_le_bytes(['2' as u8, 'R' as u8]));
        assert_eq!(mem.read16(0xffff), u16::from_le_bytes(['0' as u8, 'R' as u8]));
        mem.map_ram_bank(5, 3).unwrap();
        test_page(&mem, 0, b"ROM1");
        test_page(&mem, 1, b"RAM5");
        test_page(&mem, 2, b"RAM2");
        test_page(&mem, 3, b"RAM5");
        assert_eq!(mem.page_mem_offset(1), 0x8000 + 5*0x4000);
        assert_eq!(mem.page_mem_offset(3), 0x8000 + 5*0x4000);
        assert_eq!(mem.read16(0x3fff), u16::from_le_bytes(['1' as u8, 'R' as u8]));
        assert_eq!(mem.read16(0x7fff), u16::from_le_bytes(['5' as u8, 'R' as u8]));
        assert_eq!(mem.read16(0xbfff), u16::from_le_bytes(['2' as u8, 'R' as u8]));
        assert_eq!(mem.read16(0xffff), u16::from_le_bytes(['5' as u8, 'R' as u8]));
        mem.map_rom_bank(0, 0).unwrap();
        mem.map_ram_bank(7, 3).unwrap();
        test_page(&mem, 0, b"ROM0");
        test_page(&mem, 1, b"RAM5");
        test_page(&mem, 2, b"RAM2");
        test_page(&mem, 3, b"RAM7");
        assert_eq!(mem.page_mem_offset(3), 0x8000 + 7*0x4000);
        assert_eq!(mem.read16(0x3fff), u16::from_le_bytes(['0' as u8, 'R' as u8]));
        assert_eq!(mem.read16(0x7fff), u16::from_le_bytes(['5' as u8, 'R' as u8]));
        assert_eq!(mem.read16(0xbfff), u16::from_le_bytes(['2' as u8, 'R' as u8]));
        assert_eq!(mem.read16(0xffff), u16::from_le_bytes(['7' as u8, 'R' as u8]));
        assert_eq!(mem.page_index_at(0x0000).unwrap(), MemPageOffset {kind: MemoryKind::Rom, index: 0, offset: 0});
        assert_eq!(mem.page_index_at(0x3fff).unwrap(), MemPageOffset {kind: MemoryKind::Rom, index: 0, offset: 0x3fff});
        assert_eq!(mem.page_index_at(0x4000).unwrap(), MemPageOffset {kind: MemoryKind::Ram, index: 1, offset: 0});
        assert_eq!(mem.page_index_at(0x7fff).unwrap(), MemPageOffset {kind: MemoryKind::Ram, index: 1, offset: 0x3fff});
        assert_eq!(mem.page_index_at(0x8000).unwrap(), MemPageOffset {kind: MemoryKind::Ram, index: 2, offset: 0});
        assert_eq!(mem.page_index_at(0xbfff).unwrap(), MemPageOffset {kind: MemoryKind::Ram, index: 2, offset: 0x3fff});
        assert_eq!(mem.page_index_at(0xc000).unwrap(), MemPageOffset {kind: MemoryKind::Ram, index: 3, offset: 0});
        assert_eq!(mem.page_index_at(0xffff).unwrap(), MemPageOffset {kind: MemoryKind::Ram, index: 3, offset: 0x3fff});
        test_rom_bank(&mem, 0, b"ROM0");
        test_rom_bank(&mem, 1, b"ROM1");
        test_ram_bank(&mem, 0, b"RAM0");
        test_ram_bank(&mem, 1, b"RAM1");
        test_ram_bank(&mem, 2, b"RAM2");
        test_ram_bank(&mem, 3, b"RAM3");
        test_ram_bank(&mem, 4, b"RAM4");
        test_ram_bank(&mem, 5, b"RAM5");
        test_ram_bank(&mem, 6, b"RAM6");
        test_ram_bank(&mem, 7, b"RAM7");
        test_screen_bank(&mem, 0, b"RAM5");
        test_screen_bank(&mem, 1, b"RAM7");
        mem.reset_banks();
        test_page(&mem, 0, b"ROM0");
        test_page(&mem, 1, b"RAM5");
        test_page(&mem, 2, b"RAM2");
        test_page(&mem, 3, b"RAM0");

        let mem2 = mem.clone();

        let mut rng = SmallRng::seed_from_u64(42);
        for addr in 0x0000..=0xffff {
            mem.write(addr, rng.gen());
        }
        let mut rng = SmallRng::seed_from_u64(42);
        let patt = b"ROM0";
        for addr in 0x0000..=0x3fff {
            assert_eq!(mem.read(addr), patt[(addr & 3) as usize]);
            let _: u8 = rng.gen();
        }
        let mut rng2 = rng.clone();
        for addr in 0x4000..=0xffff {
            assert_eq!(mem.read(addr), rng.gen());
        }
        test_rom_bank(&mem, 0, b"ROM0");
        test_rom_bank(&mem, 1, b"ROM1");
        test_ram_bank(&mem, 1, b"RAM1");
        test_ram_bank(&mem, 3, b"RAM3");
        test_ram_bank(&mem, 4, b"RAM4");
        test_ram_bank(&mem, 6, b"RAM6");
        test_ram_bank(&mem, 7, b"RAM7");
        for byte in mem.ram_bank_ref(5).unwrap().iter().copied() {
            assert_eq!(byte, rng2.gen());
        }
        for byte in mem.ram_bank_ref(2).unwrap().iter().copied() {
            assert_eq!(byte, rng2.gen());
        }
        for byte in mem.ram_bank_ref(0).unwrap().iter().copied() {
            assert_eq!(byte, rng2.gen());
        }

        let mut rng = SmallRng::seed_from_u64(667);
        for addr in (0x0001..=0xffff).step_by(2) {
            mem.write16(addr, rng.gen());
        }
        let mut rng = SmallRng::seed_from_u64(667);
        let patt = b"ROM0";
        let mut last: u16 = 0;
        for addr in 0x0000..=0x3fff {
            assert_eq!(mem.read(addr), patt[(addr & 3) as usize]);
            if addr & 1 == 1 {
                last = rng.gen();
            }
        }
        let [_, hi] = last.to_le_bytes();
        assert_eq!(mem.read(0x4000), hi);
        for addr in (0x4001..=0xfffd).step_by(2) {
            assert_eq!(mem.read16(addr), rng.gen());
        }
        let [lo, _] = rng.gen::<u16>().to_le_bytes();
        assert_eq!(mem.read(0xffff), lo);
        test_rom_bank(&mem, 0, b"ROM0");
        test_rom_bank(&mem, 1, b"ROM1");
        test_ram_bank(&mem, 1, b"RAM1");
        test_ram_bank(&mem, 3, b"RAM3");
        test_ram_bank(&mem, 4, b"RAM4");
        test_ram_bank(&mem, 6, b"RAM6");
        test_ram_bank(&mem, 7, b"RAM7");
        assert_eq!(mem.page_ref(0).unwrap(), mem.rom_bank_ref(0).unwrap());
        assert_eq!(mem.page_ref(1).unwrap(), mem.ram_bank_ref(5).unwrap());
        assert_eq!(mem.page_ref(2).unwrap(), mem.ram_bank_ref(2).unwrap());
        assert_eq!(mem.page_ref(3).unwrap(), mem.ram_bank_ref(0).unwrap());

        test_rom_bank(&mem2, 0, b"ROM0");
        test_rom_bank(&mem2, 1, b"ROM1");
        test_ram_bank(&mem2, 0, b"RAM0");
        test_ram_bank(&mem2, 1, b"RAM1");
        test_ram_bank(&mem2, 2, b"RAM2");
        test_ram_bank(&mem2, 3, b"RAM3");
        test_ram_bank(&mem2, 4, b"RAM4");
        test_ram_bank(&mem2, 5, b"RAM5");
        test_ram_bank(&mem2, 6, b"RAM6");
        test_ram_bank(&mem2, 7, b"RAM7");
        test_screen_bank(&mem2, 0, b"RAM5");
        test_screen_bank(&mem2, 1, b"RAM7");
        test_page(&mem2, 0, b"ROM0");
        test_page(&mem2, 1, b"RAM5");
        test_page(&mem2, 2, b"RAM2");
        test_page(&mem2, 3, b"RAM0");
    }
}
