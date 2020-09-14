/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
use super::ZxMemory;

#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};

/// An interface for memory paging extensions of [ZxMemory].
///
/// Provide an implementation as the associated type [crate::chip::MemoryAccess::MemoryExt].
pub trait MemoryExtension: core::fmt::Debug {
    /// Read op-code from the given `memory` at the given `pc` address, optionally altering provided memory.
    #[inline]
    fn read_opcode<M: ZxMemory>(&mut self, pc: u16, memory: &mut M) -> u8 {
        memory.read(pc)
    }
    /// Writes to the memory extension port. Should return optionally modified `data` if the extension wants
    /// to influence some other chipset functions.
    #[inline]
    fn write_io<M: ZxMemory>(&mut self, _port: u16, data: u8, _memory: &mut M) -> u8 {
        data
    }
    /// Reads from the memory extension port. 
    fn read_io<M: ZxMemory>(&mut self, _port: u16, _memory: &mut M) -> u8 {
        u8::max_value()
    }
}

/// A placeholder memory extension. Usefull as a default.
#[derive(Clone, Copy, Default, Debug)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
pub struct NoMemoryExtension;

impl MemoryExtension for NoMemoryExtension {}
