use core::convert::TryFrom;
use core::ptr::NonNull;
use arrayvec::ArrayVec;
use serde::{Serialize, Deserialize};
use serde::ser::{self, Serializer, SerializeStruct};
use serde::de::{self, Deserializer};
use super::{MAX_PAGES,
    ExRom, MemoryBlock, MemoryPages, MemPageableRomRamExRom, MemSerExt, MemDeExt,
    ro_flag_mask, serialize_mem, deserialize_mem};

impl<M: MemoryBlock> Serialize for MemPageableRomRamExRom<M>
    where M::Pages: Copy,
          for<'a> &'a Box<M>: MemSerExt,
          for<'a> &'a M::Pages: IntoIterator<Item=&'a NonNull<<M::Pages as MemoryPages>::PagePtrType>>
{
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {

        #[derive(Serialize)]
        #[serde(transparent)]
        struct MemSerWrap<'a, M>(
            #[serde(serialize_with = "serialize_mem")] &'a M
        ) where &'a M: MemSerExt;

        let mut pages = self.pages;
        if let Some(exrom) = &self.ex_rom {
            *pages.page_mut(exrom.page) = exrom.ptr;
        }
        let mem_ptr = self.mem.as_slice().as_ptr() as usize;
        let banks: ArrayVec<[u8;MAX_PAGES]> = pages.into_iter()
                                     .map(|p| p.as_ptr() as usize - mem_ptr)
                                     .map(|offset| {
                                        assert_eq!(offset % M::BANK_SIZE, 0);
                                        u8::try_from(offset / M::BANK_SIZE)
                                     })
                                     .collect::<Result<_,_>>()
                                     .map_err(|err| ser::Error::custom(err.to_string()))?;
        let mut state = serializer.serialize_struct("MemPageableRomRamExRom", 3)?;
        state.serialize_field("mem", &MemSerWrap(&self.mem))?;
        state.serialize_field("pages", &banks)?;
        state.serialize_field("exrom", &self.ex_rom)?;
        state.end()
    }
}

impl<'de, M: MemoryBlock> Deserialize<'de> for MemPageableRomRamExRom<M>
    where Box<M>: MemDeExt,
          for<'a> &'a mut M::Pages: IntoIterator<Item=&'a mut NonNull<<M::Pages as MemoryPages>::PagePtrType>>
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de>
    {
        #[derive(Deserialize)]
        struct MemTemp<M: MemoryBlock> where Box<M>: MemDeExt {
            #[serde(deserialize_with = "deserialize_mem")]
            mem: Box<M>,
            #[serde(rename(deserialize = "pages"))]
            banks: ArrayVec<[u8;MAX_PAGES]>,
            exrom: Option<ExRomTemp>
        }

        #[derive(Deserialize)]
        struct ExRomTemp {
            page: u8,
            #[serde(deserialize_with = "deserialize_mem")]
            rom: ExRom
        }

        let MemTemp::<M> { mem, banks, exrom } = Deserialize::deserialize(deserializer)?;
        if banks.len() != M::Pages::len() {
            return Err(de::Error::custom(format!("memory pages: {} != expected {}",
                                                    banks.len(), M::Pages::len())));
        }

        let mut pages = M::Pages::new();
        let mut ro_pages = 0;
        let mem_slice = mem.as_slice();
        let offset_max = mem_slice.len() - M::BANK_SIZE;
        for ((p, bank), page) in pages.into_iter().zip(banks).zip(0u8..) {
            let offset =  bank as usize * M::BANK_SIZE;
            if offset > offset_max {
                return Err(de::Error::custom(format!("memory bank: {} larger than max: {}",
                                                        bank, offset_max / M::BANK_SIZE)));
            }
            if (bank as usize) < M::ROM_BANKS {
                ro_pages |= ro_flag_mask(page);
            }
            *p = M::cast_slice_as_bank_ptr(&mem_slice[offset..offset + M::BANK_SIZE]);
        }

        let mut res = MemPageableRomRamExRom { mem, pages, ro_pages, ex_rom: None};
        if let Some(ExRomTemp { page, rom }) = exrom {
            if rom.len() != M::BANK_SIZE {
                return Err(de::Error::custom(format!("attached ex-rom size incorrect: {} != {}",
                                                        rom.len(), M::BANK_SIZE)));
            }
            if page > <M::Pages as MemoryPages>::PAGES_MASK {
                return Err(de::Error::custom(format!("attached ex-rom page: {}  larger than max: {}",
                                                        page, <M::Pages as MemoryPages>::PAGES_MASK)));
            }
            res.attach_exrom(rom, page);
        }
        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;
    use super::*;
    use crate::memory::multi_page::MEM16K_SIZE;
    use crate::memory::*;

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
        for (&a, &b) in pages.iter().zip(pages_de.iter()) {
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
        mem.map_exrom(Rc::clone(&exrom), 3);
        let sermem = serde_json::to_string(&mem).unwrap();
        let mem_de: Memory128k = serde_json::from_str(&sermem).unwrap();
        serde_compare(&mem, &mem_de, true);

        let encoded: Vec<u8> = bincode::serialize(&mem).unwrap();
        let mem_de: Memory128k = bincode::deserialize(&encoded).unwrap();
        serde_compare(&mem, &mem_de, true);
    }
}
