/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
/*! Memory API.
# Memory banks and pages.

* [ZxMemory] ROM and RAM is a continuous slice of memory divided into logical banks.
* 16 bit memory addresses are mapped to memory banks depending on the implementation.
* Each page can have any fitting bank of ROM or RAM memory switched in.
* Some implementations allow to attach an external ROM bank to any of the pages.

Below are examples of continuous memory: all ROM banks followed by RAM banks.
[ZxMemory::mem_ref] and [ZxMemory::mem_mut] both give access to the whole area.
```text
|00000h  |04000h  |8000h   |0c000h  |10000h  |14000h  |18000h |1c000h  |20000h  | 24000h
Memory16k
|ROM_SIZE|
+--------+--------+
|        | Screen |
|        | Bank 0 |
| ROM    | RAM    |
| Bank 0 | Bank 0 |
+--------+--------+
Memory48k
|ROM_SIZE|
+--------+--------+--------+--------+
|        | Screen Bank 0            |
|        |                          |
| ROM    | RAM Bank 0               |
| Bank 0 |                          |
+--------+--------+--------+--------+
Memory128k
|<-   ROM_SIZE  ->|
+--------+--------+--------+--------+--------+--------+--------+--------+--------+--------+
|        |        |        |        |        |        |        | Screen |        | Screen |
|        |        |        |        |        |        |        | Bank 0 |        | Bank 1 |
| ROM    | ROM    | RAM    | RAM    | RAM    | RAM    | RAM    | RAM    | RAM    | RAM    |
| Bank 0 | Bank 1 | Bank 0 | Bank 1 | Bank 2 | Bank 3 | Bank 4 | Bank 5 | Bank 6 | Bank 7 |
+--------+--------+--------+--------+--------+--------+--------+--------+--------+--------+
```
Below are examples of memory pages mappings.

A constant memory map of [Memory16k] and [Memory48k]:
```text
        Memory48k  Memory16k
Page 0 +--------+  +--------+ Page 0 0x0000
       |        |  |        |
       |        |  |        |
       | ROM    |  | ROM    |
       | Bank 0 |  | Bank 0 |
Page 1 +--------+  +--------+ Page 1 0x4000
       | Screen |  | Screen |
       | Bank 0 |  | Bank 0 |
       | RAM    |  | RAM    |
       | Bank 0 |  | Bank 0 | RAMTOP 0x7fff
       +        +  +--------+        0x8000
       |        |
       |        |
       |        |
       |        |
       +        +                    0xc000
       |        |
       |        |
       |        |
RAMTOP |        |                    0xffff
       +--------+
```
A memory map with example banks switched in for [Memory128k] or [Memory128kPlus]:
```text
Page 0 +--------+ 0x1000
       |        |
       |        |
       | ROM    |
       | Bank 0 |
Page 1 +--------+ 0x4000
       | Screen |
       | Bank 0 |
       | RAM    |
       | Bank 5 |
Page 2 +--------+ 0x8000
       |        |
       |        |
       | RAM    |
       | Bank 2 |
Page 3 +--------+ 0xc000
       |        |
       |        |
       | RAM    |
RAMTOP | Bank 0 | 0xffff
       +--------+
```
*/
mod single_page;
mod multi_page;

pub use spectrusty_core::memory::*;
pub use single_page::*;
pub use multi_page::*;

#[inline(always)]
pub(self) fn screen_slice_to_array_ref(slice: &[u8]) -> &ScreenArray {
    assert!(slice.len() == core::mem::size_of::<ScreenArray>());
    let ptr = slice.as_ptr() as *const ScreenArray;
    // SAFETY: ok because we just checked that the length fits
    unsafe { &*ptr }
}

#[inline(always)]
pub(self) fn screen_slice_to_array_mut(slice: &mut [u8]) -> &mut ScreenArray {
    assert!(slice.len() == core::mem::size_of::<ScreenArray>());
    let ptr = slice.as_mut_ptr() as *mut ScreenArray;
    // SAFETY: ok because we just checked that the length fits
    unsafe { &mut *ptr }
}
