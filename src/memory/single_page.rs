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

/// A single page memory type with 16kb RAM.
#[derive(Clone)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
pub struct Memory16k {
    #[cfg_attr(feature = "snapshot", serde(serialize_with = "serialize_mem", deserialize_with = "deserialize_mem"))]
    mem: Box<[u8;0x8000]>
}

/// A single page memory type with 48kb RAM.
#[derive(Clone)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
pub struct Memory48k {
    #[cfg_attr(feature = "snapshot", serde(serialize_with = "serialize_mem", deserialize_with = "deserialize_mem"))]
    mem: Box<[u8;0x10000]>
}

/// A single page memory type with 64kb RAM.
///
/// The first 16kb of RAM will be also available as ROM, however
/// the implementation doesn't prevent writes to the ROM area.
#[derive(Clone)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
pub struct Memory64k {
    #[cfg_attr(feature = "snapshot", serde(serialize_with = "serialize_mem", deserialize_with = "deserialize_mem"))]
    mem: Box<[u8;0x10000]>
}

const ROM_SIZE: usize = 0x4000;
const ROM_TOP: u16 = (ROM_SIZE - 1) as u16;

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

macro_rules! impl_zxmemory {
    ($ty:ty: RAMBOT($rambot:expr) RAMTOP($ramtop:expr)) => {
        impl ZxMemory for $ty {
            const ROM_SIZE: usize = ROM_SIZE;
            const RAMTOP: u16 = $ramtop;
            const PAGES_MAX: u8 = 1;
            const SCR_BANKS_MAX: usize = 1;
            const ROM_BANKS_MAX: usize = 0;
            const RAM_BANKS_MAX: usize = 0;

            #[inline(always)]
            fn reset(&mut self) {}

            #[inline(always)]
            fn read(&self, addr: u16) -> u8 {
                #[allow(unreachable_patterns)]
                match addr {
                    0..=$ramtop => unsafe {
                        self.mem.as_ptr().offset(addr as isize).read()
                    }
                    _ => u8::max_value()
                }
            }

            #[allow(clippy::cast_ptr_alignment)]
            #[inline]
            fn read16(&self, addr: u16) -> u16 {
                match addr {
                    a if a < $ramtop => unsafe {
                        let ptr: *const u8 = self.mem.as_ptr().offset(a as isize);
                        let ptr16 = ptr as *const u16;
                        ptr16.read_unaligned().to_le()
                    }
                    a if a == $ramtop || a == std::u16::MAX => {
                        self.read(a) as u16|(self.read(a.wrapping_add(1)) as u16) << 8
                    }
                    _ => {
                        u16::max_value()
                    }
                }
            }

            #[inline(always)]
            fn write(&mut self, addr: u16, val: u8) {
                #[allow(unreachable_patterns)]
                match addr {
                    $rambot..=$ramtop => unsafe {
                        self.mem.as_mut_ptr().offset(addr as isize).write(val);
                    }
                    _ => {}
                }
            }

            #[allow(clippy::cast_ptr_alignment)]
            #[inline]
            fn write16(&mut self, addr: u16, val: u16) {
                match addr {
                    #[allow(unused_comparisons)]
                    a if a >= $rambot && a < $ramtop => unsafe {
                        let ptr: *mut u8 = self.mem.as_mut_ptr().offset(a as isize);
                        let ptr16 = ptr as *mut u16;
                        ptr16.write_unaligned(val.to_le());
                    }
                    a if a == ROM_TOP => {
                        self.write(a.wrapping_add(1), (val >> 8) as u8);
                    }
                    a if a == $ramtop => {
                        self.write(a, val as u8);
                        self.write(a.wrapping_add(1), (val >> 8) as u8);
                    }
                    _ => {}
                }
            }
            #[inline]
            fn read_screen(&self, _screen_bank: usize, addr: u16) -> u8 {
                if addr < SCREEN_SIZE {
                    unsafe {
                        self.mem.as_ptr().add(0x4000 + addr as usize).read()
                    }
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
                Ok(&self.mem[Self::ROM_SIZE..=$ramtop])
            }

            fn ram_bank_mut(&mut self, ram_bank: usize) -> Result<&mut[u8]> {
                if ram_bank > Self::RAM_BANKS_MAX {
                    return Err(ZxMemoryError::InvalidBankIndex)
                }
                Ok(&mut self.mem[Self::ROM_SIZE..=$ramtop])
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
                const RAM_BOT: u16 = ROM_TOP + 1;
                #[allow(unreachable_patterns)]
                match address {
                    a @ 0..=ROM_TOP => {
                        Ok(MemPageOffset {kind: MemoryKind::Rom, index: 0, offset: a})
                    }
                    a @ RAM_BOT..=$ramtop => {
                        Ok(MemPageOffset {kind: MemoryKind::Ram, index: 1, offset: a - RAM_BOT})
                    }
                    _ => Err(ZxMemoryError::UnsupportedAddressRange)
                }
            }
        }
    };
}

impl_zxmemory! { Memory16k: RAMBOT(0x4000) RAMTOP(0x7FFF) }
impl_zxmemory! { Memory48k: RAMBOT(0x4000) RAMTOP(0xFFFF) }
impl_zxmemory! { Memory64k: RAMBOT(0x0000) RAMTOP(0xFFFF) }

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