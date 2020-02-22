use core::num::NonZeroU16;
use z80emu::{Io, Memory};
use crate::bus::BusDevice;
use crate::clock::VideoTs;
use crate::peripherals::{KeyboardInterface, ZXKeyboardMap};
use crate::memory::ZxMemory;
use crate::video::{VideoFrame, pixel_line_offset, color_line_offset};
// use crate::io::keyboard::*;
// use crate::ts::*;
use super::{Ula, UlaVideoFrame, UlaTsCounter};

impl<M, B> Io for Ula<M, B> where M: ZxMemory, B: BusDevice<Timestamp=VideoTs>
{
    type Timestamp = VideoTs;
    type WrIoBreak = ();
    type RetiBreak = ();

    #[inline(always)]
    fn is_irq(&mut self, ts: VideoTs) -> bool {
        ts.vc == 0 && ts.hc & !31 == 0
    }

    fn read_io(&mut self, port: u16, ts: VideoTs) -> (u8, Option<NonZeroU16>) {
        let bus_data = self.bus.read_io(port, ts);
        let val = if port & 1 == 0 {
            self.keyboard.read_keyboard((port >> 8) as u8) &
            ((self.read_ear_in(ts) << 6) | 0b1011_1111) &
            bus_data.unwrap_or(u8::max_value())
        }
        else {
            bus_data.unwrap_or_else(|| self.floating_bus(ts))
        };
        (val, None)
    }

    fn write_io(&mut self, port: u16, data: u8, ts: VideoTs) -> (Option<()>, Option<NonZeroU16>) {
        if port & 1 == 0 {
            let border = data & 7;
            if self.last_border != border {
                // println!("border: {} {:?}", border, ts);
                self.last_border = border;
                self.border_out_changes.push((ts, border).into());
            }
            let earmic = data >> 3 & 3;
            if self.last_earmic_data != earmic {
                self.last_earmic_data = earmic;
                self.earmic_out_changes.push((ts, earmic).into());
            }
        }
        else {
            self.bus.write_io(port, data, ts);
        }
        (None, None)
    }
}

impl<M, B> Memory for Ula<M, B> where M: ZxMemory, B: BusDevice<Timestamp=VideoTs>
{
    type Timestamp = VideoTs;

    #[inline(always)]
    fn read_debug(&self, addr: u16) -> u8 {
        self.memory.read(addr)
    }

    #[inline(always)]
    fn read_mem(&self, addr: u16, _ts: VideoTs) -> u8 {
        self.memory.read(addr)
    }

    #[inline(always)]
    fn read_mem16(&self, addr: u16, _ts: VideoTs) -> u16 {
        self.memory.read16(addr)
    }

    #[inline(always)]
    fn read_opcode(&mut self, pc: u16, _ir: u16, ts: VideoTs) -> u8 {
        self.bus.m1(&mut self.memory, pc, ts);
        self.memory.read(pc)
    }

    #[inline(always)]
    fn write_mem(&mut self, addr: u16, val: u8, ts: VideoTs) {
        self.update_frame_pixels_and_colors(addr, ts);
        self.memory.write(addr, val);
    }
}

impl<M, B> KeyboardInterface for Ula<M, B>// where M: ZxMemory, B: BusDevice<Timestamp=VideoTs>
{
    fn get_key_state(&self) -> ZXKeyboardMap {
        self.keyboard
    }
    fn set_key_state(&mut self, keymap: ZXKeyboardMap)  {
        self.keyboard = keymap;
    }
}

trait FloatingBusOffset: VideoFrame {
    #[inline]
    fn floating_bus_offset(VideoTs{vc, hc}: VideoTs) -> Option<u16> {
        if Self::VSL_PIXELS.contains(&vc) {
            // println!("floating_bus_offset: {},{} {}", vc, hc, crate::clock::VFrameTsCounter::<Self>::vc_hc_to_tstates(vc, hc));
            match hc {
                c @ 0..=123 if c & 4 == 0 => Some(c as u16),
                _ => None
            }
        }
        else {
            None
        }
    }
}

impl FloatingBusOffset for UlaVideoFrame {}
    // where M: ZxMemory, B: BusDevice<Timestamp=VideoTs> {}

impl<M: ZxMemory, B> Ula<M,B> {
    fn floating_bus(&self, ts: VideoTs) -> u8 {
        if let Some(offs) = UlaVideoFrame::floating_bus_offset(ts) {
            let y = (ts.vc - UlaVideoFrame::VSL_PIXELS.start) as u16;
            let col = (offs >> 3) << 1;
            // println!("got offs: {} col:{} y:{}", offs, col, y);
            let addr = match offs & 3 {
                0 => 0x4000 + pixel_line_offset(y) + col,
                1 => 0x5800 + color_line_offset(y) + col,
                2 => 0x4001 + pixel_line_offset(y) + col,
                3 => 0x5801 + color_line_offset(y) + col,
                _ => unsafe { core::hint::unreachable_unchecked() }
            };
            self.memory.read(addr)
        }
        else {
            u8::max_value()
        }
    }
}
