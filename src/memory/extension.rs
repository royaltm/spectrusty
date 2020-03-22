use std::rc::Rc;
use std::io::{self, Read};

use super::{ExRom, ZxMemory};
#[cfg(feature = "snapshot")]
use super::serde::{serialize_mem, deserialize_mem};

#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};

/// An interface for memory paging extensions of [ZxMemory].
///
/// Provide an implementation as the associated type [crate::chip::MemoryAccess::MemoryExt].
pub trait MemoryExtension {
    /// Read op-code from the given `memory` at the given `pc` address, optionally altering provided memory.
    #[inline]
    fn opcode_read<M: ZxMemory>(&mut self, pc: u16, memory: &mut M) -> u8 {
        memory.read(pc)
    }
}

#[derive(Clone, Copy, Default, Debug)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
pub struct NoMemoryExtension;

impl MemoryExtension for NoMemoryExtension {}

/// The ZX Interface 1 memory [extension][MemoryExtension].
///
/// Interface 1 ROM is paged in if the processor executes the instruction at address `0x0008` or `0x1708`,
/// the error handler and `CLOSE #` routines. It is paged out after the Z80 executes the `RET` at address `0x0700`.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
pub struct ZxInterface1MemExt {
    #[cfg_attr(feature = "snapshot", serde(serialize_with = "serialize_mem", deserialize_with = "deserialize_mem"))]
    exrom: ExRom
}

impl Default for ZxInterface1MemExt {
    fn default() -> Self {
        let exrom = Rc::new([]);
        ZxInterface1MemExt { exrom }
    }
}

impl MemoryExtension for ZxInterface1MemExt {
    fn opcode_read<M: ZxMemory>(&mut self, pc: u16, memory: &mut M) -> u8 {
        match pc {
            0x0008|0x1708 => {
                memory.map_exrom(Rc::clone(&self.exrom), 0);
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
}
