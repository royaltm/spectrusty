use core::num::NonZeroU16;

use crate::z80emu::{Io, Memory};
use crate::chip::Ula128MemFlags;
use crate::bus::{BusDevice, PortAddress};
use crate::clock::VideoTs;
use crate::peripherals::{KeyboardInterface, ZXKeyboardMap};
use crate::memory::{ZxMemory, MemoryExtension};
use crate::video::VideoFrame;
use super::{Ula128, Ula128VidFrame, MemPage8};

#[derive(Clone, Copy, Default, Debug)]
struct Ula128MemPortAddress;
impl PortAddress for Ula128MemPortAddress {
    const ADDRESS_MASK: u16 = 0b1000_0000_0000_0010;
    const ADDRESS_BITS: u16 = 0b0111_1111_1111_1101;
}

impl<B, X> Io for Ula128<B, X>
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
        if Ula128MemPortAddress::match_port(port) {
            // Reads from port 0x7ffd cause a crash, as the 128's HAL10H8 chip does not distinguish
            // between reads and writes to this port, resulting in a floating data bus being used to
            // set the paging registers.
            let data = self.floating_bus(ts);
            self.write_mem_port(data, ts); // FIXIT: read_io should allow breaks too
            (data, None)
        }
        else {
            self.ula.ula_read_io(port, ts)
                    .unwrap_or_else(|| (self.floating_bus(ts), None))
        }
    }

    fn write_io(&mut self, port: u16, data: u8, ts: VideoTs) -> (Option<()>, Option<NonZeroU16>) {
        if Ula128MemPortAddress::match_port(port) {
            // (self.write_mem_port(data, ts).then_some(()), None) // after stabilizing # 64260
            if self.write_mem_port(data, ts) {
                return (Some(()), None)
            }
            (None, None)
        }
        else {
            self.ula.write_io(port, data, ts)
        }
    }
}

impl<B, X> Memory for Ula128<B, X>
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

    #[inline]
    fn read_opcode(&mut self, pc: u16, ir: u16, ts: VideoTs) -> u8 {
        self.update_snow_interference(ts, ir);
        self.ula.memext.opcode_read(pc, &mut self.ula.memory)
    }

    #[inline]
    fn write_mem(&mut self, addr: u16, val: u8, ts: VideoTs) {
        self.update_frame_cache(addr, ts);
        self.ula.memory.write(addr, val);
    }
}

impl<B, X> KeyboardInterface for Ula128<B, X> {
    #[inline(always)]
    fn get_key_state(&self) -> ZXKeyboardMap {
        self.ula.get_key_state()
    }
    #[inline(always)]
    fn set_key_state(&mut self, keymap: ZXKeyboardMap)  {
        self.ula.set_key_state(keymap);
    }
}

impl<B, X> Ula128<B, X> {
    #[inline]
    fn write_mem_port(&mut self, data: u8, ts: VideoTs) -> bool {
        if !self.mem_locked {
            let flags = Ula128MemFlags::from_bits_truncate(data);
            if flags.is_mmu_locked() {
                self.mem_locked = true;
            }

            let cur_screen_shadow = flags.is_shadow_screen();
            if self.cur_screen_shadow != cur_screen_shadow {
                self.cur_screen_shadow = cur_screen_shadow;
                self.screen_changes.push(ts);
            }
            let rom_bank = flags.rom_page_bank();
            self.ula.memory.map_rom_bank(rom_bank, 0).unwrap();
            let page3_bank = MemPage8::from(flags);
            // println!("\nscr: {} pg3: {} ts: {}x{}", self.cur_screen_shadow, mem_page3_bank, ts.vc, ts.hc);
            return self.set_mem_page3_bank(page3_bank)
        }
        false
    }

    #[inline]
    fn floating_bus(&self, ts: VideoTs) -> u8 {
        if let Some(addr) = Ula128VidFrame::floating_bus_screen_address(ts) {
            self.ula.memory.read_screen(self.cur_screen_shadow.into(), addr)
        }
        else {
            u8::max_value()
        }
    }
}
