use core::convert::TryFrom;
use core::num::NonZeroU16;

use crate::z80emu::{Io, Memory};
use crate::bus::{BusDevice, PortAddress};
use crate::clock::VideoTs;
use crate::peripherals::{KeyboardInterface, ZXKeyboardMap};
use crate::memory::{ZxMemory, MemoryExtension};
use super::{Ula3, SpecialPaging};
use crate::chip::ula128::{MemPage8, Ula128MemFlags};

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

bitflags! {
    #[derive(Default)]
    pub(crate) struct Ula3MemFlags: u8 {
        const EXT_PAGING  = 0b001;
        const PAGE_LAYOUT = 0b110;
        const ROM_BANK_HI = 0b100;
    }
}

impl From<Ula3MemFlags> for SpecialPaging {
    fn from(flags: Ula3MemFlags) -> SpecialPaging {
        match (flags & Ula3MemFlags::PAGE_LAYOUT).bits() >> 1 {
            0 => SpecialPaging::Banks0123,
            1 => SpecialPaging::Banks4567,
            2 => SpecialPaging::Banks4563,
            3 => SpecialPaging::Banks4763,
            _ => unreachable!()
        }
    }
}

impl<B, X> Io for Ula3<B, X>
    where B: BusDevice<Timestamp=VideoTs>
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
                .unwrap_or_else(|| (u8::max_value(), None))
    }

    fn write_io(&mut self, port: u16, data: u8, ts: VideoTs) -> (Option<()>, Option<NonZeroU16>) {
        if Ula3Mem1PortAddress::match_port(port) {
            // (self.write_mem_port(data, ts).then_some(()), None) // after stabilizing # 64260
            if self.write_mem1_port(data, ts) {
                return (Some(()), None)
            }
            (None, None)
        }
        else {
            let (mut res, ws) = self.ula.write_io(port, data, ts);
            if Ula3Mem2PortAddress::match_port(port) {
                if self.write_mem2_port(data) {
                    res = Some(());
                }
            }
            (res, ws)
        }
    }
}

impl<B, X> Memory for Ula3<B, X>
    where B: BusDevice<Timestamp=VideoTs>, X: MemoryExtension
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
        self.ula.memext.opcode_read(pc, &mut self.ula.memory)
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

impl<B, X> Ula3<B, X> {
    #[inline]
    fn write_mem1_port(&mut self, data: u8, ts: VideoTs) -> bool {
        if !self.mem_locked {
            let flags = Ula128MemFlags::from_bits_truncate(data);
            if flags.intersects(Ula128MemFlags::LOCK_MMU) {
                self.mem_locked = true;
            }

            let cur_screen_shadow = flags.intersects(Ula128MemFlags::SCREEN_BANK);
            if self.cur_screen_shadow != cur_screen_shadow {
                self.cur_screen_shadow = cur_screen_shadow;
                self.screen_changes.push(ts);
            }
            let rom_lo = flags.intersects(Ula128MemFlags::ROM_BANK);
            let page3_bank = MemPage8::try_from((flags & Ula128MemFlags::RAM_BANK_MASK).bits()).unwrap();
            // println!("\nscr: {} pg3: {} ts: {}x{}", self.cur_screen_shadow, mem_page3_bank, ts.vc, ts.hc);
            return self.set_mem_page3_bank_and_rom_lo(page3_bank, rom_lo)
        }
        false
    }

    #[inline]
    fn write_mem2_port(&mut self, data: u8) -> bool {
        if !self.mem_locked {
            let flags = Ula3MemFlags::from_bits_truncate(data);
            return if flags.intersects(Ula3MemFlags::EXT_PAGING) {
                self.set_mem_special_paging(SpecialPaging::from(flags))
            }
            else {
                let rom_hi = flags.intersects(Ula3MemFlags::ROM_BANK_HI);
                self.set_rom_hi_no_special_paging(rom_hi)
            }
        }
        false
    }
}
