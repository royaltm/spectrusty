/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
use std::rc::Rc;
use std::io::{self, Read};

use spectrusty_core::memory::{
    MemoryExtension, ExRom, ZxMemory, ZxMemoryError
};
#[cfg(feature = "snapshot")]
use spectrusty_core::memory::serde::{serialize_mem, deserialize_mem};
#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};

/// The ZX Interface 1 memory [extension][MemoryExtension].
///
/// Interface 1 ROM is paged in if the processor executes the instruction at address `0x0008` or `0x1708`,
/// the error handler, and `CLOSE #` routines. It is paged out after the Z80 executes the `RET` at address `0x0700`.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
pub struct ZxInterface1MemExt {
    #[cfg_attr(feature = "snapshot",
        serde(serialize_with = "serialize_mem", deserialize_with = "deserialize_mem"))]
    #[cfg_attr(feature = "snapshot", serde(default = "exrom_default"))]
    exrom: ExRom
}

impl Default for ZxInterface1MemExt {
    fn default() -> Self {
        let exrom = Rc::new([]);
        ZxInterface1MemExt { exrom }
    }
}

impl MemoryExtension for ZxInterface1MemExt {
    #[inline(always)]
    fn read_opcode<M: ZxMemory>(&mut self, pc: u16, memory: &mut M) -> u8 {
        match pc {
            0x0008|0x1708 => {
                let _ = memory.map_exrom(Rc::clone(&self.exrom), 0);
            }
            0x0700 => {
                let res = memory.read(pc);
                memory.unmap_exrom(&self.exrom);
                return res
            }
            _ => {}
        }
        memory.read(pc)
    }
}

impl ZxInterface1MemExt {
    /// Provide a reader with 8kb of ZX Interface 1 ROM program code.
    pub fn load_if1_rom<R: Read>(&mut self, mut rd: R) -> io::Result<()> {
        let mut exrom = Rc::new([!0u8;0x4000]);
        let exrom_slice = &mut Rc::get_mut(&mut exrom).unwrap()[0..0x2000];
        rd.read_exact(exrom_slice)?;
        self.exrom = exrom;
        Ok(())
    }
    /// Returns a reference to the EX-ROM bank.
    pub fn exrom(&self) -> &ExRom {
        &self.exrom
    }
    /// Removes data from the EX-ROM bank.
    ///
    /// # Note
    /// If the EX-ROM bank has been paged in, it won't be paged out automatically after the EX-ROM data
    /// is cleared from the extension.
    pub fn clear_exrom(&mut self) {
        self.exrom = Rc::new([]);
    }
    /// Maps EX-ROM into `memory` page `0`.
    ///
    /// # Errors
    /// Returns an error if the extension's EX-ROM bank is not populated with ROM data.
    pub fn map_exrom<M: ZxMemory>(&self, memory: &mut M) -> Result<(), ZxMemoryError> {
        memory.map_exrom(Rc::clone(&self.exrom), 0)
    }
    /// Unmaps EX-ROM from `memory`.
    pub fn unmap_exrom<M: ZxMemory>(&self, memory: &mut M) {
        memory.unmap_exrom(&self.exrom)
    }
    /// Returns `true` if EX-ROM is currently paged in.
    pub fn is_mapped_exrom<M: ZxMemory>(&self, memory: &M) -> bool {
        memory.has_mapped_exrom(&self.exrom)
    }
}

#[cfg(feature = "snapshot")]
fn exrom_default() -> ExRom {
    Rc::new([])
}
