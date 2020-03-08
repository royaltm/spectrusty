use core::ops::{Deref, DerefMut};
use core::ops::RangeBounds;
use core::ptr::NonNull;
use core::slice;
use std::rc::Rc;

use super::{
    Result,
    ZxMemory,
    ZxMemoryError,
    // MemoryFeatures,
    MemoryKind,
    MemPageOffset,
    ExRom,
    normalize_address_range};

/// An EX-ROM attachable memory type with 16kb RAM and 16kb ROM.
pub type Memory16kEx = MemPageableRomRamExRom<[u8; MEM48K_SIZE]>;
/// An EX-ROM attachable memory type with 48kb RAM and 16kb ROM.
pub type Memory48kEx = MemPageableRomRamExRom<[u8; MEM64K_SIZE]>;
/// An EX-ROM attachable memory type with 128kb RAM and 32kb ROM.
pub type Memory128k = MemPageableRomRamExRom<[u8; MEM32K_SIZE + MEM128K_SIZE]>;
/// An EX-ROM attachable memory type with 128kb RAM and 64kb ROM.
pub type Memory128kPlus = MemPageableRomRamExRom<[u8; MEM64K_SIZE + MEM128K_SIZE]>;

const MEM16K_SIZE : usize = 0x4000;
const MEM32K_SIZE : usize = 2 * MEM16K_SIZE;
const MEM48K_SIZE : usize = 3 * MEM16K_SIZE;
const MEM64K_SIZE : usize = 4 * MEM16K_SIZE;
const MEM128K_SIZE: usize = 8 * MEM16K_SIZE;
const BANK16K_MASK: u16 = (MEM16K_SIZE - 1) as u16;
const NUM_BANK16K_PAGES: usize = 4;
const SCREEN_SIZE: u16 = 0x1B00;

pub struct MemPageableRomRamExRom<M: MemoryBlock> {
    mem: Box<M>,
    ro_pages: u8,
    pages: M::Pages,
    ex_rom: Option<ExRomAttachment<<M::Pages as MemoryPages>::PagePtrType>>,
}

#[doc(hidden)]
pub trait MemoryBlock {
    type Pages: MemoryPages;
    const ROM_BANKS: usize;
    const RAM_BANKS: usize;
    const SCR_BANKS: usize;
    const BANK_SIZE: usize;
    const DEFAULT_PAGES: &'static [usize];
    const SCR_BANK_OFFSETS: &'static [usize];
    fn new() -> Self;
    // fn as_slice(&self) -> &[u8] where Self: AsRef<[u8]> {
    //     self.as_ref()
    // }
    // fn as_mut_slice(&mut self) -> &mut [u8] where Self: AsMut<[u8]> {
    //     self.as_mut()
    // }
    fn as_slice(&self) -> &[u8];
    fn as_mut_slice(&mut self) -> &mut [u8];
    // fn pages_ref(&self) -> &Self::Pages;
    // fn pages_mut(&mut self) -> &mut Self::Pages;
    fn cast_slice_as_bank_ptr(slice: &[u8]) -> NonNull<<Self::Pages as MemoryPages>::PagePtrType> {
        assert_eq!(slice.len(), Self::BANK_SIZE);
        let ptr = slice.as_ptr() as *const <Self::Pages as MemoryPages>::PagePtrType;
        // SAFETY: ok because we just checked that the length fits
        NonNull::from(unsafe { &(*ptr) })
    }
}

#[doc(hidden)]
pub trait MemoryPages {
    type PagePtrType;
    const PAGES_MASK: u8;
    fn new() -> Self;
    fn len() -> usize {
        Self::PAGES_MASK as usize + 1
    }
    fn page_ref(&self, page: u8) -> &NonNull<Self::PagePtrType>;
    fn page_mut(&mut self, page: u8) -> &mut NonNull<Self::PagePtrType>;
}

impl MemoryPages for [NonNull<[u8; MEM16K_SIZE]>; NUM_BANK16K_PAGES] {
    type PagePtrType = [u8; MEM16K_SIZE]; // must be the same as BANK_SIZE
    const PAGES_MASK: u8 = 3;
    fn new() -> Self {
        [NonNull::dangling();NUM_BANK16K_PAGES]
    }
    #[inline(always)]
    fn page_ref(&self, page: u8) -> &NonNull<[u8; MEM16K_SIZE]> {
        &self[(page & Self::PAGES_MASK) as usize]
    }
    #[inline(always)]
    fn page_mut(&mut self, page: u8) -> &mut NonNull<[u8; MEM16K_SIZE]> {
        &mut self[(page & Self::PAGES_MASK) as usize]
    }
}

macro_rules! impl_memory_block {
    (@repl $_t:tt $sub:expr) => {$sub};
    (@count $($x:expr),*) => {0usize $(+ impl_memory_block!(@repl $x 1usize))*};
    (ROM($n:literal)) => { $n };
    (RAM($n:literal)) => { Self::ROM_BANKS + $n };
    ($memsize:expr, $roms:literal, $rams:literal, [$($memtype:ident $bank:literal),*], [$($scr:expr),*]) => {
        impl MemoryBlock for [u8; $memsize] {
            type Pages = [NonNull<[u8; MEM16K_SIZE]>; NUM_BANK16K_PAGES];
            const ROM_BANKS: usize = $roms;
            const RAM_BANKS: usize = $rams;
            const SCR_BANKS: usize = impl_memory_block!(@count $($scr),*);
            const BANK_SIZE: usize = MEM16K_SIZE;
            const DEFAULT_PAGES: &'static [usize] = &[$(impl_memory_block!($memtype($bank))),*];
            const SCR_BANK_OFFSETS: &'static [usize] = &[$(
                (Self::ROM_BANKS + $scr) * Self::BANK_SIZE
            ),*];

            fn new() -> Self {
                [!0; $memsize]
            }

            #[inline(always)]
            fn as_slice(&self) -> &[u8] {
                &self[..]
            }

            #[inline(always)]
            fn as_mut_slice(&mut self) -> &mut [u8] {
                &mut self[..]
            }
        }
    };
}

impl_memory_block!(MEM48K_SIZE, 2, 1, [ROM 0, RAM 0, ROM 1, ROM 1], [0]);
impl_memory_block!(MEM64K_SIZE, 1, 3, [ROM 0, RAM 0, RAM 1, RAM 2], [0]);
impl_memory_block!(MEM32K_SIZE + MEM128K_SIZE, 2, 8, [ROM 0, RAM 5, RAM 2, RAM 0], [5, 7]);
impl_memory_block!(MEM64K_SIZE + MEM128K_SIZE, 4, 6, [ROM 0, RAM 5, RAM 2, RAM 0], [5, 7]);

struct ExRomAttachment<P> {
    page: u8,
    ro: bool,
    ptr: NonNull<P>,
    rom: ExRom
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum WordPageOffset {
    Fits { page: u8, offset: u16 },
    Boundary { page: u8, offset: u16, next_page: u8 },
}

impl<M: MemoryBlock> Default for MemPageableRomRamExRom<M> {
    fn default() -> Self {
        let mem = Box::new(M::new());
        let pages = M::Pages::new();
        let mut mem = MemPageableRomRamExRom {
            mem,
            pages,
            ex_rom: None,
            ro_pages: 0
        };
        mem.reset_banks();
        mem
    }
}

impl<M: MemoryBlock> Clone for MemPageableRomRamExRom<M>
    where M: Clone, M::Pages: Clone
{
    fn clone(&self) -> Self {
        let ro_pages = self.ro_pages;
        let mem = self.mem.clone(); // just an array of bytes
        let mut pages = self.pages.clone(); // need to re-create
        let mut ex_rom = self.ex_rom.as_ref().map(|exr| {
            let pages_p = pages.page_mut(exr.page);
            let ptr = *pages_p; // get pointer to exrom temporarily
            *pages_p = exr.ptr; // restore shadow pointer to mem temporarily
            ExRomAttachment {
                page: exr.page,
                ro: exr.ro,
                ptr,
                rom: Rc::clone(&exr.rom)
            }
        });
        // now lets re-map pointers to our copy of mem
        let old_mem_ptr = self.mem.as_slice().as_ptr() as usize;
        for page in 0..M::Pages::len() as u8 {
            let p = pages.page_mut(page);
            let offset = p.as_ptr() as usize - old_mem_ptr;
            *p = M::cast_slice_as_bank_ptr(&mem.as_slice()[offset..offset + M::BANK_SIZE]);
        }
        if let Some(exr) = ex_rom.as_mut() { // now swap pointers back
            core::mem::swap(&mut exr.ptr, pages.page_mut(exr.page));
        }
        MemPageableRomRamExRom { mem, pages, ex_rom, ro_pages }
    }
}

#[inline(always)]
fn ro_flag_mask(page: u8) -> u8 {
    1 << page
}

impl<M: MemoryBlock> MemPageableRomRamExRom<M> {
    #[inline]
    fn as_slice(&self) -> &[u8] {
        self.mem.as_slice()
    }
    #[inline]
    fn as_mut_slice(&mut self) -> &mut[u8] {
        self.mem.as_mut_slice()
    }
    #[inline]
    fn mem_page_ptr(&self, page: u8) -> *const u8 {
        self.pages.page_ref(page).as_ptr() as *const u8
    }
    #[inline]
    fn mem_page_mut_ptr(&mut self, page: u8) -> *mut u8 {
        self.pages.page_mut(page).as_ptr() as *mut u8
    }
    #[inline]
    fn reset_banks(&mut self) {
        self.ro_pages = 0;
        self.ex_rom = None;
        for (page, &bank) in M::DEFAULT_PAGES.iter().enumerate() {
            self.bank_to_page(bank, page as u8);
        }
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
    fn is_exrom_attached(&mut self, ex_rom: &ExRom) -> bool {
        if let Some(ExRomAttachment { ref rom, .. }) = self.ex_rom.as_ref() {
            Rc::ptr_eq(&ex_rom, rom)
        }
        else {
            false
        }
    }

    fn unpage_exrom(&mut self) {
        if let Some(ExRomAttachment { page, ro, ptr, .. }) = self.ex_rom.take() {
            *self.pages.page_mut(page) = ptr;
            let mask = ro_flag_mask(page);
            if ro {
                self.ro_pages |= mask;
            }
            else {
                self.ro_pages &= !mask;
            }
        }
    }

    fn attach_exrom(&mut self, rom: ExRom, page: u8) {
        self.unpage_exrom();
        let page = page & M::Pages::PAGES_MASK;
        let page_p = self.pages.page_mut(page);
        let ptr = *page_p;
        *page_p = M::cast_slice_as_bank_ptr(&*rom);
        let ro = (self.ro_pages & ro_flag_mask(page)) != 0;
        self.ex_rom = Some(ExRomAttachment { page, ro, ptr, rom });
        self.ro_pages |= ro_flag_mask(page);
    }

    fn bank_to_page(&mut self, bank: usize, page: u8) {
        let page = page & M::Pages::PAGES_MASK;
        let offset = bank * M::BANK_SIZE;
        let slice = &self.as_slice()[offset..offset + M::BANK_SIZE];
        let ptr = M::cast_slice_as_bank_ptr(slice);
        let ro = bank < M::ROM_BANKS;
        if let Some(ex_rom_att) = self.ex_rom.as_mut() {
            if ex_rom_att.page == page {
                ex_rom_att.ptr = ptr;
                ex_rom_att.ro = ro;
                return;
            }
        }
        let mask = ro_flag_mask(page);
        if ro {
            self.ro_pages |= mask;
        }
        else {
            self.ro_pages &= !mask;
        }
        *self.pages.page_mut(page) = ptr;
    }

    #[inline(always)]
    fn is_writable_page(&self, page: u8) -> bool {
        self.ro_pages & ro_flag_mask(page & M::Pages::PAGES_MASK) == 0
    }
}

impl<M> ZxMemory for MemPageableRomRamExRom<M>
    where M: MemoryBlock
{
    const PAGE_SIZE: usize = M::BANK_SIZE;
    const ROM_SIZE: usize = M::ROM_BANKS*M::BANK_SIZE;
    const RAMTOP: u16 = u16::max_value();
    const PAGES_MAX: u8 = <M::Pages as MemoryPages>::PAGES_MASK;
    const SCR_BANKS_MAX: usize = M::SCR_BANKS as usize - 1;
    const RAM_BANKS_MAX: usize = M::RAM_BANKS as usize - 1;
    const ROM_BANKS_MAX: usize = M::ROM_BANKS as usize - 1;
    // const FEATURES: MemoryFeatures = MemoryFeatures::NONE;

    #[inline(always)]
    fn reset(&mut self) {
        self.reset_banks();
    }

    #[inline(always)]
    fn read(&self, addr: u16) -> u8 {
        let (page, offset) = Self::byte_address_to_page_offset(addr);
        unsafe {
            self.mem_page_ptr(page).add(offset as usize).read()
        }
    }

    #[allow(clippy::cast_ptr_alignment)]
    #[inline]
    fn read16(&self, addr: u16) -> u16 {
        match Self::word_address_to_page_offset(addr) {
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
        let (page, offset) = Self::byte_address_to_page_offset(addr);
        if self.is_writable_page(page) {
            unsafe {
                self.mem_page_mut_ptr(page).add(offset as usize).write(val);
            }
        }
    }

    #[allow(clippy::cast_ptr_alignment)]
    #[inline]
    fn write16(&mut self, addr: u16, val: u16) {
        match Self::word_address_to_page_offset(addr) {
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
        if addr < SCREEN_SIZE {
            unsafe {
                self.as_slice().as_ptr().add(offset + addr as usize).read()
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
        Ok(&self.as_slice()[offset..offset + SCREEN_SIZE as usize])
    }
    #[inline]
    fn screen_mut(&mut self, screen_bank: usize) -> Result<&mut [u8]> {
        if screen_bank >= M::SCR_BANK_OFFSETS.len() {
            return Err(ZxMemoryError::InvalidBankIndex)
        }
        let offset = M::SCR_BANK_OFFSETS[screen_bank];
        Ok(&mut self.as_mut_slice()[offset..offset + SCREEN_SIZE as usize])
    }
    #[inline]
    fn page_kind(&self, page: u8) -> Result<MemoryKind> {
        if page > Self::PAGES_MAX {
            Err(ZxMemoryError::InvalidPageIndex)
        }
        else if self.is_writable_page(page) {
            Ok(MemoryKind::Ram)
        }
        else {
            Ok(MemoryKind::Rom)
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
    fn page_mut(&mut self, page: u8) -> Result<&mut [u8]> {
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

    fn rom_bank_mut(&mut self, rom_bank: usize) -> Result<&mut[u8]> {
        if rom_bank > Self::ROM_BANKS_MAX {
            return Err(ZxMemoryError::InvalidBankIndex)
        }
        let offset = M::BANK_SIZE * rom_bank;
        Ok(&mut self.as_mut_slice()[offset..offset+M::BANK_SIZE])
    }

    fn ram_bank_ref(&self, ram_bank: usize) -> Result<&[u8]> {
        if ram_bank > Self::RAM_BANKS_MAX {
            return Err(ZxMemoryError::InvalidBankIndex)
        }
        let offset = M::BANK_SIZE * ram_bank + Self::ROM_SIZE;
        Ok(&self.as_slice()[offset..offset + M::BANK_SIZE])
    }


    fn ram_bank_mut(&mut self, ram_bank: usize) -> Result<&mut[u8]> {
        if ram_bank > Self::RAM_BANKS_MAX {
            return Err(ZxMemoryError::InvalidBankIndex)
        }
        let offset = M::BANK_SIZE * ram_bank + Self::ROM_SIZE;
        Ok(&mut self.as_mut_slice()[offset..offset+M::BANK_SIZE])
    }

    fn map_rom_bank(&mut self, rom_bank: usize, page: u8) -> Result<()> {
        if rom_bank > Self::ROM_BANKS_MAX {
            return Err(ZxMemoryError::InvalidBankIndex)
        }
        if page > Self::PAGES_MAX {
            return Err(ZxMemoryError::InvalidPageIndex)
        }
        self.bank_to_page(rom_bank, page);
        Ok(())
    }

    fn map_ram_bank(&mut self, ram_bank: usize, page: u8) -> Result<()> {
        if ram_bank > Self::RAM_BANKS_MAX {
            return Err(ZxMemoryError::InvalidBankIndex)
        }
        if page > Self::PAGES_MAX {
            return Err(ZxMemoryError::InvalidPageIndex)
        }
        self.bank_to_page(ram_bank + M::ROM_BANKS, page);
        Ok(())
    }
    /// # Panics
    /// Panics if provided `exrom_bank` size does not match [ZxMemory::PAGE_SIZE].
    fn map_exrom(&mut self, exrom_bank: ExRom, page: u8) -> Result<()> {
        if page > Self::PAGES_MAX {
            return Err(ZxMemoryError::InvalidPageIndex)
        }
        self.attach_exrom(exrom_bank, page);
        Ok(())
    }
    fn unmap_exrom(&mut self, exrom_bank: &ExRom) {
        if self.is_exrom_attached(exrom_bank) {
            self.unpage_exrom();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{Rng, SeedableRng};
    use rand::rngs::SmallRng;

    fn page_mem_offset(mem: &Memory128k, page: u8) -> usize {
        let mem_ptr = mem.mem_ref().as_ptr() as usize;
        mem.pages.page_ref(page).as_ptr() as usize - mem_ptr
    }

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
        assert_eq!(chunks.len(), SCREEN_SIZE as usize / 4);
        for chunk in chunks {
            assert_eq!(chunk, patt);
        }
    }

    fn init_mem(mem: &mut Memory128k) {
        let slab = mem.mem_mut();
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
    }

    fn make_exrom(patt: &[u8;4]) -> ExRom {
        let mut exslab = [0u8;0x4000];
        for chunk in exslab.chunks_mut(4) {
            chunk.copy_from_slice(patt);
        }
        Rc::new(exslab)
    }

    #[test]
    fn memory_banks_work() {
        assert_eq!(Memory128k::ROM_SIZE, 0x8000);
        assert_eq!(Memory128k::PAGE_SIZE, 0x4000);
        assert_eq!(Memory128k::PAGES_MAX, 3);
        assert_eq!(Memory128k::SCR_BANKS_MAX, 1);
        assert_eq!(Memory128k::RAM_BANKS_MAX, 7);
        assert_eq!(Memory128k::ROM_BANKS_MAX, 1);
        let mut mem = Memory128k::default();
        assert_eq!(mem.ro_pages, 0b1);
        init_mem(&mut mem);
        assert_eq!(page_mem_offset(&mem, 0), 0x0000);
        assert_eq!(page_mem_offset(&mem, 1), 0x8000 + 5*0x4000);
        assert_eq!(page_mem_offset(&mem, 2), 0x8000 + 2*0x4000);
        assert_eq!(page_mem_offset(&mem, 3), 0x8000);
        assert_eq!(Memory128k::byte_address_to_page_offset(0), (0, 0));
        assert_eq!(Memory128k::byte_address_to_page_offset(0x3fff), (0, 0x3fff));
        assert_eq!(Memory128k::byte_address_to_page_offset(0x4000), (1, 0));
        assert_eq!(Memory128k::byte_address_to_page_offset(0x7fff), (1, 0x3fff));
        assert_eq!(Memory128k::byte_address_to_page_offset(0x8000), (2, 0));
        assert_eq!(Memory128k::byte_address_to_page_offset(0xbfff), (2, 0x3fff));
        assert_eq!(Memory128k::byte_address_to_page_offset(0xc000), (3, 0));
        assert_eq!(Memory128k::byte_address_to_page_offset(0xffff), (3, 0x3fff));
        assert_eq!(Memory128k::word_address_to_page_offset(0), WordPageOffset::Fits { page: 0, offset: 0 });
        assert_eq!(Memory128k::word_address_to_page_offset(0x3fff),
                                        WordPageOffset::Boundary { page: 0, offset: 0x3fff, next_page: 1 });
        assert_eq!(Memory128k::word_address_to_page_offset(0x4000), WordPageOffset::Fits { page: 1, offset: 0 });
        assert_eq!(Memory128k::word_address_to_page_offset(0x7fff),
                                        WordPageOffset::Boundary { page: 1, offset: 0x3fff, next_page: 2 });
        assert_eq!(Memory128k::word_address_to_page_offset(0x8000), WordPageOffset::Fits { page: 2, offset: 0 });
        assert_eq!(Memory128k::word_address_to_page_offset(0xbfff),
                                        WordPageOffset::Boundary { page: 2, offset: 0x3fff, next_page: 3 });
        assert_eq!(Memory128k::word_address_to_page_offset(0xc000), WordPageOffset::Fits { page: 3, offset: 0 });
        assert_eq!(Memory128k::word_address_to_page_offset(0xffff),
                                        WordPageOffset::Boundary { page: 3, offset: 0x3fff, next_page: 0 });
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
        assert_eq!(page_mem_offset(&mem, 0), 0x4000);
        assert_eq!(mem.read16(0x3fff), u16::from_le_bytes(['1' as u8, 'R' as u8]));
        assert_eq!(mem.read16(0x7fff), u16::from_le_bytes(['5' as u8, 'R' as u8]));
        assert_eq!(mem.read16(0xbfff), u16::from_le_bytes(['2' as u8, 'R' as u8]));
        assert_eq!(mem.read16(0xffff), u16::from_le_bytes(['0' as u8, 'R' as u8]));
        mem.map_ram_bank(5, 3).unwrap();
        test_page(&mem, 0, b"ROM1");
        test_page(&mem, 1, b"RAM5");
        test_page(&mem, 2, b"RAM2");
        test_page(&mem, 3, b"RAM5");
        assert_eq!(page_mem_offset(&mem, 1), 0x8000 + 5*0x4000);
        assert_eq!(page_mem_offset(&mem, 3), 0x8000 + 5*0x4000);
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
        assert_eq!(page_mem_offset(&mem, 3), 0x8000 + 7*0x4000);
        assert_eq!(mem.read16(0x3fff), u16::from_le_bytes(['0' as u8, 'R' as u8]));
        assert_eq!(mem.read16(0x7fff), u16::from_le_bytes(['5' as u8, 'R' as u8]));
        assert_eq!(mem.read16(0xbfff), u16::from_le_bytes(['2' as u8, 'R' as u8]));
        assert_eq!(mem.read16(0xffff), u16::from_le_bytes(['7' as u8, 'R' as u8]));
        assert_eq!(mem.ro_pages, 0b1);
        assert_eq!(mem.page_kind(0).unwrap(), MemoryKind::Rom);
        assert_eq!(mem.page_kind(1).unwrap(), MemoryKind::Ram);
        assert_eq!(mem.page_kind(2).unwrap(), MemoryKind::Ram);
        assert_eq!(mem.page_kind(3).unwrap(), MemoryKind::Ram);
        assert!(mem.page_kind(4).is_err());
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

    #[test]
    fn memory_ex_rom_work() {
        let mut mem = Memory128k::default();
        init_mem(&mut mem);
        let exrom = make_exrom(b"EROM");
        let exrom2 = make_exrom(b"MORE");
        assert_eq!(Rc::strong_count(&exrom), 1);
        test_page(&mem, 0, b"ROM0");
        test_page(&mem, 1, b"RAM5");
        test_page(&mem, 2, b"RAM2");
        test_page(&mem, 3, b"RAM0");
        mem.unmap_exrom(&exrom);
        assert_eq!(mem.page_kind(0).unwrap(), MemoryKind::Rom);
        assert_eq!(mem.page_kind(1).unwrap(), MemoryKind::Ram);
        assert_eq!(mem.page_kind(2).unwrap(), MemoryKind::Ram);
        assert_eq!(mem.page_kind(3).unwrap(), MemoryKind::Ram);
        test_page(&mem, 0, b"ROM0");
        test_page(&mem, 1, b"RAM5");
        test_page(&mem, 2, b"RAM2");
        test_page(&mem, 3, b"RAM0");
        let mem2 = mem.clone();
        mem.map_exrom(exrom.clone(), 0).unwrap();
        assert_eq!(mem.page_kind(0).unwrap(), MemoryKind::Rom);
        assert_eq!(mem.page_kind(1).unwrap(), MemoryKind::Ram);
        assert_eq!(mem.page_kind(2).unwrap(), MemoryKind::Ram);
        assert_eq!(mem.page_kind(3).unwrap(), MemoryKind::Ram);
        test_page(&mem, 0, b"EROM");
        test_page(&mem, 1, b"RAM5");
        test_page(&mem, 2, b"RAM2");
        test_page(&mem, 3, b"RAM0");
        assert_eq!(mem2.page_kind(0).unwrap(), MemoryKind::Rom);
        assert_eq!(mem2.page_kind(1).unwrap(), MemoryKind::Ram);
        assert_eq!(mem2.page_kind(2).unwrap(), MemoryKind::Ram);
        assert_eq!(mem2.page_kind(3).unwrap(), MemoryKind::Ram);
        test_page(&mem2, 0, b"ROM0");
        test_page(&mem2, 1, b"RAM5");
        test_page(&mem2, 2, b"RAM2");
        test_page(&mem2, 3, b"RAM0");
        assert_eq!(Rc::strong_count(&exrom), 2);
        mem.unmap_exrom(&exrom);
        assert_eq!(Rc::strong_count(&exrom), 1);
        assert_eq!(mem.page_kind(0).unwrap(), MemoryKind::Rom);
        assert_eq!(mem.page_kind(1).unwrap(), MemoryKind::Ram);
        assert_eq!(mem.page_kind(2).unwrap(), MemoryKind::Ram);
        assert_eq!(mem.page_kind(3).unwrap(), MemoryKind::Ram);
        test_page(&mem, 0, b"ROM0");
        test_page(&mem, 1, b"RAM5");
        test_page(&mem, 2, b"RAM2");
        test_page(&mem, 3, b"RAM0");
        assert_eq!(Rc::strong_count(&exrom), 1);
        mem.map_exrom(exrom.clone(), 2).unwrap();
        assert_eq!(Rc::strong_count(&exrom), 2);
        assert_eq!(mem.page_kind(0).unwrap(), MemoryKind::Rom);
        assert_eq!(mem.page_kind(1).unwrap(), MemoryKind::Ram);
        assert_eq!(mem.page_kind(2).unwrap(), MemoryKind::Rom);
        assert_eq!(mem.page_kind(3).unwrap(), MemoryKind::Ram);
        test_page(&mem, 0, b"ROM0");
        test_page(&mem, 1, b"RAM5");
        test_page(&mem, 2, b"EROM");
        test_page(&mem, 3, b"RAM0");
        mem.map_ram_bank(7, 2).unwrap();
        mem.map_ram_bank(3, 1).unwrap();
        assert_eq!(mem.page_kind(0).unwrap(), MemoryKind::Rom);
        assert_eq!(mem.page_kind(1).unwrap(), MemoryKind::Ram);
        assert_eq!(mem.page_kind(2).unwrap(), MemoryKind::Rom);
        assert_eq!(mem.page_kind(3).unwrap(), MemoryKind::Ram);
        test_page(&mem, 0, b"ROM0");
        test_page(&mem, 1, b"RAM3");
        test_page(&mem, 2, b"EROM");
        test_page(&mem, 3, b"RAM0");
        assert_eq!(Rc::strong_count(&exrom), 2);
        mem.unmap_exrom(&exrom);
        assert_eq!(Rc::strong_count(&exrom), 1);
        assert_eq!(mem.page_kind(0).unwrap(), MemoryKind::Rom);
        assert_eq!(mem.page_kind(1).unwrap(), MemoryKind::Ram);
        assert_eq!(mem.page_kind(2).unwrap(), MemoryKind::Ram);
        assert_eq!(mem.page_kind(3).unwrap(), MemoryKind::Ram);
        test_page(&mem, 0, b"ROM0");
        test_page(&mem, 1, b"RAM3");
        test_page(&mem, 2, b"RAM7");
        test_page(&mem, 3, b"RAM0");
        mem.map_exrom(exrom.clone(), 3).unwrap();
        assert_eq!(mem.page_kind(0).unwrap(), MemoryKind::Rom);
        assert_eq!(mem.page_kind(1).unwrap(), MemoryKind::Ram);
        assert_eq!(mem.page_kind(2).unwrap(), MemoryKind::Ram);
        assert_eq!(mem.page_kind(3).unwrap(), MemoryKind::Rom);
        test_page(&mem, 0, b"ROM0");
        test_page(&mem, 1, b"RAM3");
        test_page(&mem, 2, b"RAM7");
        test_page(&mem, 3, b"EROM");
        mem.unmap_exrom(&exrom2);
        assert_eq!(Rc::strong_count(&exrom2), 1);
        assert_eq!(mem.page_kind(0).unwrap(), MemoryKind::Rom);
        assert_eq!(mem.page_kind(1).unwrap(), MemoryKind::Ram);
        assert_eq!(mem.page_kind(2).unwrap(), MemoryKind::Ram);
        assert_eq!(mem.page_kind(3).unwrap(), MemoryKind::Rom);
        test_page(&mem, 0, b"ROM0");
        test_page(&mem, 1, b"RAM3");
        test_page(&mem, 2, b"RAM7");
        test_page(&mem, 3, b"EROM");
        mem.map_rom_bank(1, 3).unwrap();
        mem.map_rom_bank(1, 1).unwrap();
        assert_eq!(mem.page_kind(0).unwrap(), MemoryKind::Rom);
        assert_eq!(mem.page_kind(1).unwrap(), MemoryKind::Rom);
        assert_eq!(mem.page_kind(2).unwrap(), MemoryKind::Ram);
        assert_eq!(mem.page_kind(3).unwrap(), MemoryKind::Rom);
        test_page(&mem, 0, b"ROM0");
        test_page(&mem, 1, b"ROM1");
        test_page(&mem, 2, b"RAM7");
        test_page(&mem, 3, b"EROM");
        assert_eq!(Rc::strong_count(&exrom), 2);
        mem.unmap_exrom(&exrom);
        assert_eq!(Rc::strong_count(&exrom), 1);
        assert_eq!(mem.page_kind(0).unwrap(), MemoryKind::Rom);
        assert_eq!(mem.page_kind(1).unwrap(), MemoryKind::Rom);
        assert_eq!(mem.page_kind(2).unwrap(), MemoryKind::Ram);
        assert_eq!(mem.page_kind(3).unwrap(), MemoryKind::Rom);
        test_page(&mem, 0, b"ROM0");
        test_page(&mem, 1, b"ROM1");
        test_page(&mem, 2, b"RAM7");
        test_page(&mem, 3, b"ROM1");
        mem.map_exrom(exrom.clone(), 1).unwrap();
        mem.map_ram_bank(6, 1).unwrap();
        assert_eq!(Rc::strong_count(&exrom2), 1);
        assert_eq!(Rc::strong_count(&exrom), 2);
        let mut mem3 = mem.clone();
        assert_eq!(Rc::strong_count(&exrom2), 1);
        assert_eq!(Rc::strong_count(&exrom), 3);
        assert_eq!(mem.page_kind(0).unwrap(), MemoryKind::Rom);
        assert_eq!(mem.page_kind(1).unwrap(), MemoryKind::Rom);
        assert_eq!(mem.page_kind(2).unwrap(), MemoryKind::Ram);
        assert_eq!(mem.page_kind(3).unwrap(), MemoryKind::Rom);
        test_page(&mem, 0, b"ROM0");
        test_page(&mem, 1, b"EROM");
        test_page(&mem, 2, b"RAM7");
        test_page(&mem, 3, b"ROM1");
        assert_eq!(Rc::strong_count(&exrom), 3);
        mem.map_exrom(exrom2.clone(), 3).unwrap();
        mem.map_ram_bank(4, 3).unwrap();
        assert_eq!(Rc::strong_count(&exrom2), 2);
        assert_eq!(Rc::strong_count(&exrom), 2);
        assert_eq!(mem.page_kind(0).unwrap(), MemoryKind::Rom);
        assert_eq!(mem.page_kind(1).unwrap(), MemoryKind::Ram);
        assert_eq!(mem.page_kind(2).unwrap(), MemoryKind::Ram);
        assert_eq!(mem.page_kind(3).unwrap(), MemoryKind::Rom);
        test_page(&mem, 0, b"ROM0");
        test_page(&mem, 1, b"RAM6");
        test_page(&mem, 2, b"RAM7");
        test_page(&mem, 3, b"MORE");
        assert_eq!(Rc::strong_count(&exrom), 2);
        mem.unmap_exrom(&exrom);
        assert_eq!(Rc::strong_count(&exrom), 2);
        assert_eq!(mem.page_kind(0).unwrap(), MemoryKind::Rom);
        assert_eq!(mem.page_kind(1).unwrap(), MemoryKind::Ram);
        assert_eq!(mem.page_kind(2).unwrap(), MemoryKind::Ram);
        assert_eq!(mem.page_kind(3).unwrap(), MemoryKind::Rom);
        test_page(&mem, 0, b"ROM0");
        test_page(&mem, 1, b"RAM6");
        test_page(&mem, 2, b"RAM7");
        test_page(&mem, 3, b"MORE");
        assert_eq!(Rc::strong_count(&exrom2), 2);
        mem.unmap_exrom(&exrom2);
        assert_eq!(Rc::strong_count(&exrom2), 1);
        assert_eq!(mem.page_kind(0).unwrap(), MemoryKind::Rom);
        assert_eq!(mem.page_kind(1).unwrap(), MemoryKind::Ram);
        assert_eq!(mem.page_kind(2).unwrap(), MemoryKind::Ram);
        assert_eq!(mem.page_kind(3).unwrap(), MemoryKind::Ram);
        test_page(&mem, 0, b"ROM0");
        test_page(&mem, 1, b"RAM6");
        test_page(&mem, 2, b"RAM7");
        test_page(&mem, 3, b"RAM4");
        assert_eq!(mem2.page_kind(0).unwrap(), MemoryKind::Rom);
        assert_eq!(mem2.page_kind(1).unwrap(), MemoryKind::Ram);
        assert_eq!(mem2.page_kind(2).unwrap(), MemoryKind::Ram);
        assert_eq!(mem2.page_kind(3).unwrap(), MemoryKind::Ram);
        test_page(&mem2, 0, b"ROM0");
        test_page(&mem2, 1, b"RAM5");
        test_page(&mem2, 2, b"RAM2");
        test_page(&mem2, 3, b"RAM0");
        assert_eq!(mem3.page_kind(0).unwrap(), MemoryKind::Rom);
        assert_eq!(mem3.page_kind(1).unwrap(), MemoryKind::Rom);
        assert_eq!(mem3.page_kind(2).unwrap(), MemoryKind::Ram);
        assert_eq!(mem3.page_kind(3).unwrap(), MemoryKind::Rom);
        test_page(&mem3, 0, b"ROM0");
        test_page(&mem3, 1, b"EROM");
        test_page(&mem3, 2, b"RAM7");
        test_page(&mem3, 3, b"ROM1");
        assert_eq!(Rc::strong_count(&exrom), 2);
        mem3.reset();
        assert_eq!(Rc::strong_count(&exrom), 1);
        assert_eq!(mem3.page_kind(0).unwrap(), MemoryKind::Rom);
        assert_eq!(mem3.page_kind(1).unwrap(), MemoryKind::Ram);
        assert_eq!(mem3.page_kind(2).unwrap(), MemoryKind::Ram);
        assert_eq!(mem3.page_kind(3).unwrap(), MemoryKind::Ram);
        test_page(&mem3, 0, b"ROM0");
        test_page(&mem3, 1, b"RAM5");
        test_page(&mem3, 2, b"RAM2");
        test_page(&mem3, 3, b"RAM0");
    }
}
