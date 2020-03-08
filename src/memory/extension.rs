use std::rc::Rc;
use std::io::{self, Read};
use super::{ExRom, ZxMemory};

/// An interface for memory paging extensions for the [ZxMemory].
pub trait MemoryExtension {
    /// Read op-code from the given `memory` at the given `pc` address, optionally altering provided memory.
    fn opcode_read<M: ZxMemory>(&mut self, pc: u16, memory: &mut M) -> u8 {
        memory.read(pc)
    }
}

#[derive(Clone, Copy, Default, Debug)]
pub struct NoMemoryExtension;

impl MemoryExtension for NoMemoryExtension {}

// pub struct DynamicMemoryExtension {
//     extensions: 
// }


#[derive(Clone, Debug)]
pub struct ZxInterface1MemExt {
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
    pub fn load_ex_rom<R: Read>(&mut self, mut rd: R) -> io::Result<()> {
        let mut exrom = Rc::new([!0u8;0x4000]);
        let exrom_slice = &mut Rc::get_mut(&mut exrom).unwrap()[0..0x2000];
        rd.read_exact(exrom_slice)?;
        self.exrom = exrom;
        Ok(())
    }
}
