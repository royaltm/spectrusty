use core::num::NonZeroU16;
use z80emu::{Io, Memory};
use crate::bus::{BusDevice, PortAddress};
use crate::clock::VideoTs;
use crate::peripherals::{KeyboardInterface, ZXKeyboardMap};
use crate::memory::ZxMemory;
use crate::video::{VideoFrame, pixel_line_offset, color_line_offset};
// use crate::io::keyboard::*;
// use crate::ts::*;
use super::{Ula128, Ula128VidFrame};

#[derive(Clone, Copy, Default, Debug)]
struct Ula128MemPortAddress;
impl PortAddress for Ula128MemPortAddress {
    const ADDRESS_MASK: u16 = 0b1000_0000_0000_0010;
    const ADDRESS_BITS: u16 = 0b0111_1111_1111_1101;
}

bitflags! {
    #[derive(Default)]
    struct Ula128Mem: u8 {
        const RAM_BANK_MASK = 0b0000_0111;
        const SCREEN_BANK   = 0b0000_1000;
        const ROM_BANK      = 0b0001_0000;
        const LOCK_MMU      = 0b0010_0000;
    }
}

const ROM_SWAP_PAGE: u8 = 0;
const RAM_SWAP_PAGE: u8 = 3;

impl<B> Io for Ula128<B>
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
            // Reads from port 0x7ffd cause a crash, as the 128's HAL10H8 chip does not distinguish between reads and writes to this port, resulting in a floating data bus being used to set the paging registers.
            let data = self.floating_bus(ts);
            self.write_mem_port(data, ts); // FIXIT: read_io should allow breaks too
            (data, None)
        }
        else {
            let data = self.ula.ula_read_io(port, ts)
                           .unwrap_or_else(|| self.floating_bus(ts));
            (data, None)
        }
    }

    fn write_io(&mut self, port: u16, data: u8, ts: VideoTs) -> (Option<()>, Option<NonZeroU16>) {
        if Ula128MemPortAddress::match_port(port) {
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

impl<B> Memory for Ula128<B>
    where B: BusDevice<Timestamp=VideoTs>
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
        // self.bus.m1(&mut self.memory, pc, ts);
        self.ula.memory.read(pc)
    }

    #[inline(always)]
    fn write_mem(&mut self, addr: u16, val: u8, ts: VideoTs) {
        self.update_frame_pixels_and_colors(addr, ts);
        self.ula.memory.write(addr, val);
    }
}

impl<B> KeyboardInterface for Ula128<B>
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

impl<B> Ula128<B> {
    #[inline]
    fn write_mem_port(&mut self, data: u8, ts: VideoTs) -> bool {
        if !self.mem_locked {
            let flags = Ula128Mem::from_bits_truncate(data);
            if flags.intersects(Ula128Mem::LOCK_MMU) {
                self.mem_locked = true;
            }

            let cur_screen_shadow = flags.intersects(Ula128Mem::SCREEN_BANK);
            if self.cur_screen_shadow != cur_screen_shadow {
                self.cur_screen_shadow = cur_screen_shadow;
                self.screen_changes.push(ts);
            }
            let rom_bank = if flags.intersects(Ula128Mem::ROM_BANK) { 1 } else { 0 };
            self.ula.memory.map_rom_bank(rom_bank, ROM_SWAP_PAGE).unwrap();

            let mem_page3_bank = (flags & Ula128Mem::RAM_BANK_MASK).bits();
            // println!("\nscr: {} pg3: {} ts: {}x{}", self.cur_screen_shadow, mem_page3_bank, ts.vc, ts.hc);
            if mem_page3_bank != self.mem_page3_bank {
                let contention_before = self.is_page3_contended();
                self.ula.memory.map_ram_bank(mem_page3_bank.into(), RAM_SWAP_PAGE).unwrap();
                self.mem_page3_bank = mem_page3_bank;
                if contention_before != self.is_page3_contended() {
                    return true
                }
            }
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
