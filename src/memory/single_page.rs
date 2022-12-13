/*
    Copyright (C) 2020-2022  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};
#[cfg(feature = "snapshot")]
use super::serde::{serialize_mem, deserialize_mem};

use super::{
    MemPageOffset,
    MemoryKind,
    Result,
    ZxMemory,
    ZxMemoryError,
    SCREEN_SIZE,
    ScreenArray,
    screen_slice_to_array_ref, screen_slice_to_array_mut
};

#[derive(Clone)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
pub struct SinglePageMemory<const MEM_SIZE: usize, const RAM_BOT: u16> {
    #[cfg_attr(feature = "snapshot", serde(serialize_with = "serialize_mem", deserialize_with = "deserialize_mem"))]
    mem: Box<[u8;MEM_SIZE]>
}

pub const ROM_SIZE: u16 = 0x4000;
pub const ROM_TOP: u16 = ROM_SIZE - 1;

/// A single page memory type with 16kb RAM.
pub type Memory16k = SinglePageMemory<0x8000, ROM_SIZE>;

/// A single page memory type with 48kb RAM.
pub type Memory48k = SinglePageMemory<0x10000, ROM_SIZE>;

/// A single page memory type with 64kb RAM.
///
/// The first 16kb of RAM will be also available as ROM, however
/// the implementation doesn't prevent writes to the ROM area.
pub type Memory64k = SinglePageMemory<0x10000, 0>;

impl<const MEM_SIZE: usize, const RAM_BOT: u16> Default for SinglePageMemory<MEM_SIZE, RAM_BOT> {
    fn default() -> Self {
        Self { mem: Box::new([0; MEM_SIZE]) }
    }
}

impl<const MEM_SIZE: usize, const RAM_BOT: u16> ZxMemory for SinglePageMemory<MEM_SIZE, RAM_BOT> {
    const ROM_SIZE: usize = ROM_SIZE as usize;
    const RAMTOP: u16 = (MEM_SIZE - 1) as u16;
    const PAGES_MAX: u8 = 1;
    const SCR_BANKS_MAX: usize = 1;
    const ROM_BANKS_MAX: usize = 0;
    const RAM_BANKS_MAX: usize = 0;

    #[inline(always)]
    fn reset(&mut self) {}

    #[inline(always)]
    fn read(&self, addr: u16) -> u8 {
        self.mem.get(addr as usize).copied().unwrap_or(u8::max_value())
    }

    // #[allow(clippy::cast_ptr_alignment)]
    #[inline]
    fn read16(&self, addr: u16) -> u16 {
        match addr {
            a if a < Self::RAMTOP => unsafe {
                let ptr: *const u8 = self.mem.as_ptr().add(a as usize);
                let ptr16 = ptr as *const u16;
                ptr16.read_unaligned().to_le()
            }
            a if a == Self::RAMTOP || a == std::u16::MAX => {
                u16::from_le_bytes([self.read(addr), self.read(addr.wrapping_add(1))])
            }
            _ => {
                u16::max_value()
            }
        }
    }

    #[inline(always)]
    fn write(&mut self, addr: u16, val: u8) {
        if addr >= RAM_BOT && addr <= Self::RAMTOP {
            self.mem[addr as usize] = val;
        }
    }

    // #[allow(clippy::cast_ptr_alignment)]
    #[inline]
    fn write16(&mut self, addr: u16, val: u16) {
        match addr {
            #[allow(unused_comparisons)]
            a if a >= RAM_BOT && a < Self::RAMTOP => unsafe {
                let ptr: *mut u8 = self.mem.as_mut_ptr().add(a as usize);
                let ptr16 = ptr as *mut u16;
                ptr16.write_unaligned(val.to_le());
            }
            a if a == ROM_TOP => {
                self.write(a.wrapping_add(1), (val >> 8) as u8);
            }
            a if a == Self::RAMTOP => {
                self.write(a, (val & 0xff) as u8);
                self.write(a.wrapping_add(1), (val >> 8) as u8);
            }
            _ => {}
        }
    }
    #[inline]
    fn read_screen(&self, _screen_bank: usize, addr: u16) -> u8 {
        if addr < SCREEN_SIZE {
            self.mem[0x4000 + addr as usize]
        }
        else {
            panic!("trying to read outside of screen area");
        }
    }
    #[inline]
    fn mem_ref(&self) -> &[u8] {
        &self.mem[..]
    }
    #[inline]
    fn mem_mut(&mut self) -> &mut[u8] {
        &mut self.mem[..]
    }
    #[inline]
    fn screen_ref(&self, screen_bank: usize) -> Result<&ScreenArray> {
        let start = match screen_bank {
            0 => 0x4000,
            1 => 0x6000,
            _ => return Err(ZxMemoryError::InvalidBankIndex)
        };
        Ok(screen_slice_to_array_ref(
            &self.mem[start..start+SCREEN_SIZE as usize]))
    }
    #[inline]
    fn screen_mut(&mut self, screen_bank: usize) -> Result<&mut ScreenArray> {
        let start = match screen_bank {
            0 => 0x4000,
            1 => 0x6000,
            _ => return Err(ZxMemoryError::InvalidBankIndex)
        };
        Ok(screen_slice_to_array_mut(
            &mut self.mem[start..start+SCREEN_SIZE as usize]))
    }
    #[inline]
    fn page_kind(&self, page: u8) -> Result<MemoryKind> {
        match page {
            0 => Ok(MemoryKind::Rom),
            1 => Ok(MemoryKind::Ram),
            _ => Err(ZxMemoryError::InvalidPageIndex)
        }
    }
    fn page_bank(&self, page: u8) -> Result<(MemoryKind, usize)> {
        self.page_kind(page).map(|kind| (kind, 0))
    }
    #[inline]
    fn page_ref(&self, page: u8) -> Result<&[u8]> {
        match page {
            0 => Ok(&self.mem[..Self::ROM_SIZE]),
            1 => Ok(&self.mem[Self::ROM_SIZE..]),
            _ => Err(ZxMemoryError::InvalidPageIndex)
        }
    }
    #[inline]
    fn page_mut(&mut self, page: u8) -> Result<&mut[u8]> {
        match page {
            0 => Ok(&mut self.mem[..Self::ROM_SIZE]),
            1 => Ok(&mut self.mem[Self::ROM_SIZE..]),
            _ => Err(ZxMemoryError::InvalidPageIndex)
        }
    }
    fn rom_bank_ref(&self, rom_bank: usize) -> Result<&[u8]> {
        if rom_bank > Self::ROM_BANKS_MAX {
            return Err(ZxMemoryError::InvalidBankIndex)
        }
        Ok(&self.mem[..Self::ROM_SIZE])
    }

    fn rom_bank_mut(&mut self, rom_bank: usize) -> Result<&mut[u8]> {
        if rom_bank > Self::ROM_BANKS_MAX {
            return Err(ZxMemoryError::InvalidBankIndex)
        }
        Ok(&mut self.mem[..Self::ROM_SIZE])
    }

    fn ram_bank_ref(&self, ram_bank: usize) -> Result<&[u8]> {
        if ram_bank > Self::RAM_BANKS_MAX {
            return Err(ZxMemoryError::InvalidBankIndex)
        }
        Ok(&self.mem[Self::ROM_SIZE..=Self::RAMTOP as usize])
    }

    fn ram_bank_mut(&mut self, ram_bank: usize) -> Result<&mut[u8]> {
        if ram_bank > Self::RAM_BANKS_MAX {
            return Err(ZxMemoryError::InvalidBankIndex)
        }
        Ok(&mut self.mem[Self::ROM_SIZE..=Self::RAMTOP as usize])
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
        #[allow(unreachable_patterns)]
        if address < Self::ROM_SIZE as u16 {
            Ok(MemPageOffset {kind: MemoryKind::Rom, index: 0, offset: address})
        }
        else if address <= Self::RAMTOP {
            Ok(MemPageOffset {kind: MemoryKind::Ram, index: 1, offset: address - Self::ROM_SIZE as u16})
        }
        else {
            Err(ZxMemoryError::UnsupportedAddressRange)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::memory::*;
    use super::*;

    #[test]
    fn test_memory_single_page_work() {
       memory_single_page_work::<Memory16k>();
       memory_single_page_work::<Memory48k>();
       memory_single_page_work::<Memory64k>();

       memory_single_page_rom::<Memory16k>(true);
       memory_single_page_rom::<Memory48k>(true);
       memory_single_page_rom::<Memory64k>(false);
    }

    fn memory_single_page_rom<M: ZxMemory + Default>(is_ro: bool) {
        let mut mem = M::default();
        for addr in 0..=ROM_TOP {
            assert_eq!(mem.read(addr), 0);
            mem.write(addr, 201);
            if is_ro {
                assert_eq!(mem.read(addr), 0);
            }
            else {
                assert_eq!(mem.read(addr), 201);
            }
        }        
        for addr in ROM_TOP + 1..=M::RAMTOP {
            assert_eq!(mem.read(addr), 0);
            mem.write(addr, 201);
            assert_eq!(mem.read(addr), 201);
        }        
    }

    fn memory_single_page_work<M: ZxMemory + Default + Clone>() {
        assert_eq!(M::ROM_SIZE, 0x4000);
        assert_eq!(M::PAGE_SIZE, 0x4000);
        assert_eq!(M::PAGES_MAX, 1);
        assert_eq!(M::SCR_BANKS_MAX, 1);
        assert_eq!(M::RAM_BANKS_MAX, 0);
        assert_eq!(M::ROM_BANKS_MAX, 0);
        let mut mem = M::default();
        let mut mem1 = mem.clone();
        let mut index = 0;
        mem.for_each_page_mut(.., |page| {
            if index == 0 {
                assert!(page.is_rom());
            }
            else {
                assert!(page.is_ram());
            }
            assert_eq!(page.into_mut_slice(), mem1.page_mut(index).unwrap());
            index += 1;
            Ok(())
        }).unwrap();
        assert_eq!(index, 2);
   }
}