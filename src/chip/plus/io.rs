use core::num::NonZeroU16;

use crate::z80emu::{Io, Memory};
use crate::bus::{PortAddress};
use crate::clock::VideoTs;
use crate::chip::{UlaPortFlags, scld::io::ScldCtrlPortAddress};
use crate::peripherals::{KeyboardInterface, ZXKeyboardMap};
use crate::memory::ZxMemory;
use crate::video::{Video, BorderColor};
use super::{UlaPlus, UlaPlusInner};

#[derive(Clone, Copy, Default, Debug)]
struct PlusRegisterPortAddress;
impl PortAddress for PlusRegisterPortAddress {
    const ADDRESS_MASK: u16 = 0xFFFF;
    const ADDRESS_BITS: u16 = 0xBF3B; //48955
}

#[derive(Clone, Copy, Default, Debug)]
struct PlusDataPortAddress;
impl PortAddress for PlusDataPortAddress {
    const ADDRESS_MASK: u16 = 0xFFFF;
    const ADDRESS_BITS: u16 = 0xFF3B; //65339
}

impl<'a, U> Io for UlaPlus<U>
    where U: UlaPlusInner<'a>
           + Io<Timestamp=VideoTs, WrIoBreak = (), RetiBreak = ()>
{
    type Timestamp = VideoTs;
    type WrIoBreak = ();
    type RetiBreak = ();

    #[inline(always)]
    fn is_irq(&mut self, ts: VideoTs) -> bool {
        self.ula.is_irq(ts)
    }

    fn read_io(&mut self, port: u16, ts: VideoTs) -> (u8, Option<NonZeroU16>) {
        if PlusDataPortAddress::match_port(port) && !self.ulaplus_disabled {
            (self.read_plus_data_port(), None)
        }
        else if ScldCtrlPortAddress::match_port(port) && !self.ulaplus_disabled
                                                      && self.scld_mode_rw {
            (self.scld_mode.bits(), None)
        }
        else {
            self.ula.read_io(port, ts)
        }
    }

    fn write_io(&mut self, port: u16, data: u8, ts: VideoTs) -> (Option<()>, Option<NonZeroU16>) {
        if U::is_ula_port(port) {
            let border = BorderColor::from_bits_truncate(data);
            self.change_border_color(border, ts);
            self.ula.ula_write_earmic(UlaPortFlags::from_bits_truncate(data), ts);
        }
        else if ScldCtrlPortAddress::match_port(port) && !self.ulaplus_disabled {
            self.write_scld_ctrl_port(data, ts);
        }
        else if PlusRegisterPortAddress::match_port(port) && !self.ulaplus_disabled {
            self.write_plus_regs_port(data, ts);
        }
        else if PlusDataPortAddress::match_port(port) && !self.ulaplus_disabled {
            self.write_plus_data_port(data, ts);
        }
        else {
            return self.ula.write_io(port, data, ts)
        }
        (None, None)
    }
}

impl<'a, U> Memory for UlaPlus<U>
    where U: UlaPlusInner<'a>
           + Memory<Timestamp=VideoTs>
{
    type Timestamp = VideoTs;

    #[inline(always)]
    fn read_debug(&self, addr: u16) -> u8 {
        self.ula.read_debug(addr)
    }

    #[inline(always)]
    fn read_mem(&self, addr: u16, ts: VideoTs) -> u8 {
        self.ula.read_mem(addr, ts)
    }

    #[inline(always)]
    fn read_mem16(&self, addr: u16, ts: VideoTs) -> u16 {
        self.ula.read_mem16(addr, ts)
    }

    #[inline]
    fn read_opcode(&mut self, pc: u16, ir: u16, ts: VideoTs) -> u8 {
        self.ula.read_opcode(pc, ir, ts)
    }

    #[inline]
    fn write_mem(&mut self, addr: u16, val: u8, ts: VideoTs) {
        self.update_frame_cache(addr, ts);
        self.ula.memory_mut().write(addr, val);
    }
}

impl<U> KeyboardInterface for UlaPlus<U>
    where U: Video + KeyboardInterface
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
