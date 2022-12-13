/*
    Copyright (C) 2022  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
use core::convert::TryFrom;
use serde::{Serialize, Deserialize};
use serde::ser::{self, Serializer, SerializeStruct};
use serde::de::{self, Deserializer};
use super::super::arrays;
use super::{
    ExRom, MemoryConfig, MemoryBox, MemoryPages, MemPageableRomRamExRom, MemSerExt,
    cast_slice_as_bank_ptr,
    ro_flag_mask, serialize_mem, deserialize_mem};

impl<const MEM_SIZE: usize, const PAGE_SIZE: usize, const NUM_PAGES: usize
    > Serialize for MemPageableRomRamExRom<MEM_SIZE, PAGE_SIZE, NUM_PAGES>
    where Self: MemoryConfig
{
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {

        #[derive(Serialize)]
        #[serde(transparent)]
        struct MemSerWrap<'a, M>(
            #[serde(serialize_with = "serialize_mem")] &'a M
        ) where &'a M: MemSerExt;

        #[derive(Serialize)]
        #[serde(transparent)]
        struct BanksWrap<const NUM_PAGES: usize>(
            #[serde(with = "arrays")]
            [u8; NUM_PAGES]
        );

        let mut pages = self.pages;
        if let Some(exrom) = &self.ex_rom {
            *pages.page_mut(exrom.page) = exrom.ptr;
        }
        let mem_ptr: *const u8 = self.mem.as_ptr();
        let mut banks = BanksWrap([0u8; NUM_PAGES]);
        for (p, b) in pages.0.iter().zip(banks.0.iter_mut()) {
            let offset = unsafe { (p.as_ptr() as *const u8).offset_from(mem_ptr) };
            assert_eq!(offset % PAGE_SIZE as isize, 0);
            *b = u8::try_from(offset / PAGE_SIZE as isize)
                    .map_err(|err| ser::Error::custom(err.to_string()))?;
        }
        let mut state = serializer.serialize_struct("MemPageableRomRamExRom", 3)?;
        state.serialize_field("mem", &MemSerWrap(&self.mem))?;
        state.serialize_field("pages", &banks)?;
        state.serialize_field("exrom", &self.ex_rom)?;
        state.end()
    }
}

impl<'de,
     const MEM_SIZE: usize, const PAGE_SIZE: usize, const NUM_PAGES: usize
     > Deserialize<'de> for MemPageableRomRamExRom<MEM_SIZE, PAGE_SIZE, NUM_PAGES>
    where Self: MemoryConfig
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de>
    {
        #[derive(Deserialize)]
        struct MemTemp<const MEM_SIZE: usize, const NUM_PAGES: usize> {
            #[serde(deserialize_with = "deserialize_mem")]
            mem: MemoryBox<MEM_SIZE>,
            #[serde(with = "arrays",rename(deserialize = "pages"))]
            banks: [u8;NUM_PAGES],
            exrom: Option<ExRomTemp>
        }

        #[derive(Deserialize)]
        struct ExRomTemp {
            page: u8,
            #[serde(deserialize_with = "deserialize_mem")]
            rom: ExRom
        }

        let MemTemp::<MEM_SIZE, NUM_PAGES> { mem, banks, exrom } = Deserialize::deserialize(deserializer)?;

        let mut pages = MemoryPages::<PAGE_SIZE, NUM_PAGES>::new();
        let mut ro_pages = 0;
        let offset_max = mem.len() - PAGE_SIZE;
        for ((p, bank), page) in pages.0.iter_mut().zip(banks).zip(0u8..) {
            let offset =  bank as usize * PAGE_SIZE;
            if offset > offset_max {
                return Err(de::Error::custom(format!("memory bank: {} larger than max: {}",
                                                        bank, offset_max / PAGE_SIZE)));
            }
            if (bank as usize) < Self::ROM_BANKS {
                ro_pages |= ro_flag_mask(page);
            }
            *p = cast_slice_as_bank_ptr(&mem[offset..offset + PAGE_SIZE]);
        }

        let mut res = MemPageableRomRamExRom { mem, pages, ro_pages, ex_rom: None};
        if let Some(ExRomTemp { page, rom }) = exrom {
            if rom.len() != PAGE_SIZE {
                return Err(de::Error::custom(format!("attached ex-rom size incorrect: {} != {}",
                                                        rom.len(), PAGE_SIZE)));
            }
            if page > Self::PAGES_MASK {
                return Err(de::Error::custom(format!("attached ex-rom page: {}  larger than max: {}",
                                                        page, Self::PAGES_MASK)));
            }
            res.attach_exrom(rom, page);
        }
        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;
    use crate::memory::multi_page::MEM16K_SIZE;
    use crate::memory::*;
    use core::ptr::NonNull;

    fn page_mem_offset(mem: &[u8], ptr: NonNull<[u8; MEM16K_SIZE]>) -> usize {
        let mem_ptr = mem.as_ptr() as usize;
        ptr.as_ptr() as usize - mem_ptr
    }

    fn serde_compare(mem: &Memory128k, mem_de: &Memory128k, must_have_exrom: bool) {
        for (a, b) in mem.mem.iter().zip(mem_de.mem.iter()) {
            assert_eq!(*a, *b);
        }
        let mut pages = mem.pages;
        let mut pages_de = mem_de.pages;
        if let Some(ex_rom) = &mem.ex_rom {
            *pages.page_mut(ex_rom.page) = ex_rom.ptr;
        }
        if let Some(ex_rom) = &mem_de.ex_rom {
            *pages_de.page_mut(ex_rom.page) = ex_rom.ptr;
        }
        for (&a, &b) in pages.0.iter().zip(pages_de.0.iter()) {
            page_mem_offset(&mem.mem_ref(), a);
            page_mem_offset(&mem_de.mem_ref(), b);
        }
        assert_eq!(mem.ro_pages, mem_de.ro_pages);
        if must_have_exrom {
            assert!(mem.ex_rom.is_some());
            assert!(mem_de.ex_rom.is_some());
            assert_eq!(mem.ex_rom.as_ref().unwrap().page, mem_de.ex_rom.as_ref().unwrap().page);
            assert_eq!(mem.ex_rom.as_ref().unwrap().ro, mem_de.ex_rom.as_ref().unwrap().ro);
            assert_eq!(
                page_mem_offset(&mem.ex_rom.as_ref().unwrap().rom[..], mem.pages.page(mem.ex_rom.as_ref().unwrap().page)),
                page_mem_offset(&mem_de.ex_rom.as_ref().unwrap().rom[..], mem_de.pages.page(mem_de.ex_rom.as_ref().unwrap().page))
            );
        }
        else {
            assert!(mem.ex_rom.is_none());
            assert!(mem_de.ex_rom.is_none());
        }
    }

    #[test]
    fn memory_serde_works() {
        let mut mem = Memory128k::default();
        let sermem = serde_json::to_string(&mem).unwrap();
        let mem_de: Memory128k = serde_json::from_str(&sermem).unwrap();
        serde_compare(&mem, &mem_de, false);

        let encoded: Vec<u8> = bincode::serialize(&mem).unwrap();
        let mem_de: Memory128k = bincode::deserialize(&encoded).unwrap();
        serde_compare(&mem, &mem_de, false);

        let exrom: Rc<[u8]> = Rc::from(vec![0u8;Memory128k::PAGE_SIZE]);
        mem.map_exrom(Rc::clone(&exrom), 3).unwrap();
        let sermem = serde_json::to_string(&mem).unwrap();
        let mem_de: Memory128k = serde_json::from_str(&sermem).unwrap();
        serde_compare(&mem, &mem_de, true);

        let encoded: Vec<u8> = bincode::serialize(&mem).unwrap();
        let mem_de: Memory128k = bincode::deserialize(&encoded).unwrap();
        serde_compare(&mem, &mem_de, true);
    }
}
