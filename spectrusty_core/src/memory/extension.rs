use super::ZxMemory;

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

/// A placeholder memory extension. Usefull as a default.
#[derive(Clone, Copy, Default, Debug)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
pub struct NoMemoryExtension;

impl MemoryExtension for NoMemoryExtension {}
