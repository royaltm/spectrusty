/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
use core::num::NonZeroU16;

use crate::z80emu::{Io, Memory};
use crate::bus::{BusDevice, PortAddress};
use crate::clock::{VideoTs, VFrameTs};
use crate::chip::{Ula128MemFlags, Ula3CtrlFlags};
use crate::peripherals::{KeyboardInterface, ZXKeyboardMap};
use crate::memory::{ZxMemory, MemoryExtension};
use super::{Ula3, Ula3VidFrame};

#[derive(Clone, Copy, Default, Debug)]
struct Ula3Mem1PortAddress;
impl PortAddress for Ula3Mem1PortAddress {
    const ADDRESS_MASK: u16 = 0b1100_0000_0000_0010;
    const ADDRESS_BITS: u16 = 0b0111_1111_1111_1101;
}

#[derive(Clone, Copy, Default, Debug)]
struct Ula3Mem2PortAddress;
impl PortAddress for Ula3Mem2PortAddress {
    const ADDRESS_MASK: u16 = 0b1111_0000_0000_0010;
    const ADDRESS_BITS: u16 = 0b0001_1111_1111_1101;
}

// #[derive(Clone, Copy, Default, Debug)]
// struct Ula3FDDataPortAddress;
// impl PortAddress for Ula3FDDataPortAddress {
//     const ADDRESS_MASK: u16 = 0b1111_0000_0000_0010;
//     const ADDRESS_BITS: u16 = 0b0011_1111_1111_1101;
// }

// #[derive(Clone, Copy, Default, Debug)]
// struct Ula3FDStatusPortAddress;
// impl PortAddress for Ula3FDStatusPortAddress {
//     const ADDRESS_MASK: u16 = 0b1111_0000_0000_0010;
//     const ADDRESS_BITS: u16 = 0b0010_1111_1111_1101;
// }

// #[derive(Clone, Copy, Default, Debug)]
// struct Ula3CentronicsPortAddress;
// impl PortAddress for Ula3CentronicsPortAddress {
//     const ADDRESS_MASK: u16 = 0b1111_0000_0000_0010;
//     const ADDRESS_BITS: u16 = 0b0000_1111_1111_1101;
// }

impl<B, X> Io for Ula3<B, X>
    where B: BusDevice,
          B::Timestamp: From<VFrameTs<Ula3VidFrame>>
{
    type Timestamp = VideoTs;
    type WrIoBreak = ();
    type RetiBreak = ();

    #[inline(always)]
    fn is_irq(&mut self, ts: VideoTs) -> bool {
        self.ula.is_irq(ts)
    }

    fn read_io(&mut self, port: u16, ts: VideoTs) -> (u8, Option<NonZeroU16>) {
        self.ula.ula_read_io(port, ts)
                .unwrap_or((u8::max_value(), None))
    }

    fn write_io(&mut self, port: u16, data: u8, ts: VideoTs) -> (Option<()>, Option<NonZeroU16>) {
        if Ula3Mem1PortAddress::match_port(port) {
            if !self.mem_locked {
                let flags = Ula128MemFlags::from_bits_truncate(data);
                if self.set_mem1_port_value(flags, ts) {
                    return (Some(()), None)
                }
            }
            (None, None)
        }
        else {
            let (mut res, ws) = self.ula.write_io(port, data, ts);
            if Ula3Mem2PortAddress::match_port(port) && !self.mem_locked {
                let flags = Ula3CtrlFlags::from_bits_truncate(data);
                if self.set_mem2_port_value(flags) {
                    res = Some(());
                }
            }
            (res, ws)
        }
    }
}

impl<B, X> Memory for Ula3<B, X>
    where B: BusDevice,
          B::Timestamp: From<VFrameTs<Ula3VidFrame>>,
          X: MemoryExtension
{
    type Timestamp = VideoTs;

    #[inline(always)]
    fn read_debug(&self, addr: u16) -> u8 {
        self.ula.memory.read(addr)
    }

    #[inline(always)]
    fn read_mem(&self, addr: u16, _ts: VideoTs) -> u8 {
        self.ula.memory.read(addr)
    }

    #[inline(always)]
    fn read_mem16(&self, addr: u16, _ts: VideoTs) -> u16 {
        self.ula.memory.read16(addr)
    }

    #[inline(always)]
    fn read_opcode(&mut self, pc: u16, _ir: u16, _ts: VideoTs) -> u8 {
        self.ula.memext.read_opcode(pc, &mut self.ula.memory)
    }

    #[inline]
    fn write_mem(&mut self, addr: u16, val: u8, ts: VideoTs) {
        self.update_frame_cache(addr, ts);
        self.ula.memory.write(addr, val);
    }
}

impl<B, X> KeyboardInterface for Ula3<B, X> {
    #[inline(always)]
    fn get_key_state(&self) -> ZXKeyboardMap {
        self.ula.get_key_state()
    }
    #[inline(always)]
    fn set_key_state(&mut self, keymap: ZXKeyboardMap)  {
        self.ula.set_key_state(keymap);
    }
}
