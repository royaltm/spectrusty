use core::num::NonZeroU16;

use crate::z80emu::{Io, Memory};
use crate::chip::{UlaPortFlags, ScldCtrlFlags};
use crate::bus::{BusDevice, PortAddress};
use crate::clock::VideoTs;
use crate::peripherals::{KeyboardInterface, ZXKeyboardMap};
use crate::memory::{PagedMemory8k, MemoryExtension};
use crate::video::{VideoFrame, BorderColor};
use super::Scld;

#[derive(Clone, Copy, Default, Debug)]
struct UlaPortAddress;
impl PortAddress for UlaPortAddress {
    const ADDRESS_MASK: u16 = 0b0000_0000_1111_1111;
    const ADDRESS_BITS: u16 = 0b0000_0000_1111_1110;
}

#[derive(Clone, Copy, Default, Debug)]
pub(crate) struct ScldCtrlPortAddress;
impl PortAddress for ScldCtrlPortAddress {
    const ADDRESS_MASK: u16 = 0b0000_0000_1111_1111;
    const ADDRESS_BITS: u16 = 0b0000_0000_1111_1111;
}

#[derive(Clone, Copy, Default, Debug)]
pub(crate) struct ScldMmuPortAddress;
impl PortAddress for ScldMmuPortAddress {
    const ADDRESS_MASK: u16 = 0b0000_0000_1111_1111;
    const ADDRESS_BITS: u16 = 0b0000_0000_1111_0100;
}

impl<M, B, X, V> Io for Scld<M, B, X, V>
    where M: PagedMemory8k,
          B: BusDevice,
          B::Timestamp: From<VideoTs>,
          V: VideoFrame
{
    type Timestamp = VideoTs;
    type WrIoBreak = ();
    type RetiBreak = ();

    #[inline(always)]
    fn is_irq(&mut self, ts: VideoTs) -> bool {
        self.ula.is_irq(ts) && !self.cur_ctrl_flags.is_intr_disabled()
    }

    fn read_io(&mut self, port: u16, ts: VideoTs) -> (u8, Option<NonZeroU16>) {
        if ScldCtrlPortAddress::match_port(port) {
            (self.cur_ctrl_flags.bits(), None)
        }
        else if ScldMmuPortAddress::match_port(port) {
            (self.mem_paged, None)
        }
        else {
            let bus_data = self.ula.bus.read_io(port, ts.into());
            if UlaPortAddress::match_port(port) {
                let ula_data = self.ula.ula_io_data(port, ts) & 0b0101_1111;
                if let Some((data, ws)) = bus_data {
                    return (ula_data & data, ws);
                }
                (ula_data, None)
            }
            else {
                self.ula.bus.read_io(port, ts.into())
                            .unwrap_or_else(|| (!0, None) )
            }
        }
    }

    fn write_io(&mut self, port: u16, data: u8, ts: VideoTs) -> (Option<()>, Option<NonZeroU16>) {
        if UlaPortAddress::match_port(port) {
            let flags = UlaPortFlags::from_bits_truncate(data);
            let border = BorderColor::from(flags);
            self.change_border_color(border, ts);
            self.ula.ula_write_earmic(flags, ts);
        }
        else if ScldCtrlPortAddress::match_port(port) {
            let flags = ScldCtrlFlags::from_bits_truncate(data);
            self.set_ctrl_flags_value(flags, ts);
        }
        else if ScldMmuPortAddress::match_port(port) {
            self.set_mmu_flags_value(data);
        }
        else {
            if let Some(ws) = self.ula.bus.write_io(port, data, ts.into()) {
                return (None, NonZeroU16::new(ws))
            }
        }
        (None, None)
    }
}

impl<M, B, X, V> Memory for Scld<M, B, X, V>
    where M: PagedMemory8k,
          X: MemoryExtension,
          V: VideoFrame
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

    #[inline]
    fn read_opcode(&mut self, pc: u16, _ir: u16, _ts: VideoTs) -> u8 {
        self.ula.memext.read_opcode(pc, &mut self.ula.memory)
    }

    #[inline]
    fn write_mem(&mut self, addr: u16, val: u8, ts: VideoTs) {
        self.update_frame_cache(addr, ts);
        self.ula.memory.write(addr, val);
    }
}

impl<M, B, X, V> KeyboardInterface for Scld<M, B, X, V>
    where M: PagedMemory8k
{
    #[inline(always)]
    fn get_key_state(&self) -> ZXKeyboardMap {
        self.ula.get_key_state()
    }
    #[inline(always)]
    fn set_key_state(&mut self, keymap: ZXKeyboardMap)  {
        self.ula.set_key_state(keymap);
    }
}
