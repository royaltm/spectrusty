use std::rc::Rc;
use core::ops::RangeBounds;
use super::{Result,
    ZxMemory,
    ZxMemoryError,
    // MemoryFeatures,
    MemPageOffset,
    MemoryKind,
    ExRom,
    normalize_address_range};

/// A single page memory type with 16kb RAM.
#[derive(Clone)]
pub struct Memory16k {
    mem: Box<[u8;0x8000]>
}

/// A single page memory type with 48kb RAM.
#[derive(Clone)]
pub struct Memory48k {
    mem: Box<[u8;0x10000]>
}

/// A single page memory type with 64kb RAM.
///
/// The first 16kb of RAM will be also available as ROM, however
/// the implementation doesn't prevent writes to the ROM area.
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

/// Methods and constants common for all single bank ROM and RAM memory types.
pub trait SingleBankMemory {
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

impl SingleBankMemory for Memory16k {
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

impl SingleBankMemory for Memory48k {
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

impl SingleBankMemory for Memory64k {
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

impl<M: SingleBankMemory + Sized> ZxMemory for M {
    const ROM_SIZE: usize = M::ROMSIZE as usize;
    const RAMTOP: u16 = M::RAMTOP;
    const PAGES_MAX: u8 = 1;
    const SCR_BANKS_MAX: usize = 0;
    const ROM_BANKS_MAX: usize = 0;
    const RAM_BANKS_MAX: usize = 0;
    // const FEATURES: MemoryFeatures = MemoryFeatures::NONE;

    #[inline(always)]
    fn reset(&mut self) {}

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
    #[inline]
    fn read_screen(&self, _screen_bank: usize, addr: u16) -> u8 {
        if addr < 0x1B00 {
            unsafe {
                self.mem().add(0x4000 + addr as usize).read()
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
        if screen_bank > Self::SCR_BANKS_MAX {
            return Err(ZxMemoryError::InvalidBankIndex)
        }
        Ok(&self.as_slice()[0x4000..0x5B00])
    }
    #[inline]
    fn page_kind(&self, page: u8) -> Result<MemoryKind> {
        match page {
            0 => Ok(MemoryKind::Rom),
            1 => Ok(MemoryKind::Ram),
            _ => Err(ZxMemoryError::InvalidPageIndex)
        }        
    }
    #[inline]
    fn page_ref(&self, page: u8) -> Result<&[u8]> {
        match page {
            0 => Ok(&self.as_slice()[..Self::ROM_SIZE]),
            1 => Ok(&self.as_slice()[Self::ROM_SIZE..]),
            _ => Err(ZxMemoryError::InvalidPageIndex)
        }        
    }
    #[inline]
    fn page_mut(&mut self, page: u8) -> Result<&mut[u8]> {
        match page {
            0 => Ok(&mut self.as_mut_slice()[..Self::ROM_SIZE]),
            1 => Ok(&mut self.as_mut_slice()[Self::ROM_SIZE..]),
            _ => Err(ZxMemoryError::InvalidPageIndex)
        }        
    }
    fn rom_bank_ref(&self, rom_bank: usize) -> Result<&[u8]> {
        if rom_bank > Self::ROM_BANKS_MAX {
            return Err(ZxMemoryError::InvalidBankIndex)
        }
        Ok(&self.as_slice()[..=Self::ROMTOP as usize])
    }

    fn rom_bank_mut(&mut self, rom_bank: usize) -> Result<&mut[u8]> {
        if rom_bank > Self::ROM_BANKS_MAX {
            return Err(ZxMemoryError::InvalidBankIndex)
        }
        Ok(&mut self.as_mut_slice()[..=Self::ROMTOP as usize])
    }

    fn ram_bank_ref(&self, ram_bank: usize) -> Result<&[u8]> {
        if ram_bank > Self::RAM_BANKS_MAX {
            return Err(ZxMemoryError::InvalidBankIndex)
        }
        Ok(&self.as_slice()[Self::RAMBOT as usize..=Self::RAMTOP as usize])
    }


    fn ram_bank_mut(&mut self, ram_bank: usize) -> Result<&mut[u8]> {
        if ram_bank > Self::RAM_BANKS_MAX {
            return Err(ZxMemoryError::InvalidBankIndex)
        }
        Ok(&mut self.as_mut_slice()[Self::RAMBOT as usize..=Self::RAMTOP as usize])
    }

    fn map_rom_bank(&mut self, rom_bank: usize, page: u8) -> Result<()> {
        if rom_bank > Self::ROM_BANKS_MAX {
            return Err(ZxMemoryError::InvalidBankIndex)
        }
        if page != 0 {
            return Err(ZxMemoryError::InvalidPageIndex)
        }
        Ok(())
    }

    fn map_ram_bank(&mut self, ram_bank: usize, page: u8) -> Result<()> {
        if ram_bank > Self::RAM_BANKS_MAX {
            return Err(ZxMemoryError::InvalidBankIndex)
        }
        if page != 1 {
            return Err(ZxMemoryError::InvalidPageIndex)
        }
        Ok(())
    }

    fn page_index_at(&self, address: u16) -> Result<MemPageOffset> {
        match address {
            a if a <= Self::ROMTOP => {
                Ok(MemPageOffset {kind: MemoryKind::Rom, index: 0, offset: a})
            }
            a if a >= Self::RAMBOT && a <=Self::RAMTOP => {
                Ok(MemPageOffset {kind: MemoryKind::Ram, index: 1, offset: a - Self::RAMBOT})
            }
            _ => Err(ZxMemoryError::AddressRangeNotSupported)
        }
    }
}
