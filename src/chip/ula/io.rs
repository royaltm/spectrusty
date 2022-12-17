/*
    Copyright (C) 2020-2022  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
use core::num::NonZeroU16;

use crate::z80emu::{Io, Memory};
use crate::bus::BusDevice;
use crate::clock::{Ts, VideoTs, VFrameTs};
use crate::chip::{EarMic, UlaPortFlags};
use crate::peripherals::{KeyboardInterface, ZXKeyboardMap};
use crate::memory::{ZxMemory, MemoryExtension};
use crate::video::{BorderColor, VideoFrame};
use super::Ula;

impl<M, B, X, V> Io for Ula<M, B, X, V>
    where M: ZxMemory,
          B: BusDevice,
          B::Timestamp: From<VFrameTs<V>>,
          V: VideoFrame
{
    type Timestamp = VideoTs;
    type WrIoBreak = ();
    type RetiBreak = ();

    #[inline(always)]
    fn is_irq(&mut self, VideoTs{ vc, hc }: VideoTs) -> bool {
        vc == 0 && (hc + Ts::from(self.late_timings)) & !31 == 0
    }

    fn read_io(&mut self, port: u16, ts: VideoTs) -> (u8, Option<NonZeroU16>) {
        self.ula_read_io(port, ts)
            .unwrap_or_else(|| (self.floating_bus(ts), None))
    }

    fn write_io(&mut self, port: u16, data: u8, ts: VideoTs) -> (Option<()>, Option<NonZeroU16>) {
        if port & 1 == 0 {
            let flags = UlaPortFlags::from_bits_truncate(data);
            let border = BorderColor::from(flags);
            if self.last_border != border {
                self.last_border = border;
                self.border_out_changes.push((ts, border.bits()).into());
            }
            self.ula_write_earmic(flags, ts);
        }
        else if let Some(ws) = self.bus.write_io(port, data, VFrameTs::from(ts).into()) {
            return (None, NonZeroU16::new(ws))
        }
        (None, None)
    }
}

impl<M, B, X, V> Memory for Ula<M, B, X, V>
    where M: ZxMemory,
          X: MemoryExtension,
          V: VideoFrame
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
    fn read_opcode(&mut self, pc: u16, ir: u16, ts: VideoTs) -> u8 {
        self.update_snow_interference(ts, ir);
        self.memext.read_opcode(pc, &mut self.memory)
    }

    #[inline(always)]
    fn write_mem(&mut self, addr: u16, val: u8, ts: VideoTs) {
        self.update_frame_cache(addr, ts);
        self.memory.write(addr, val);
    }
}

impl<M, B, X, V> KeyboardInterface for Ula<M, B, X, V> {
    fn get_key_state(&self) -> ZXKeyboardMap {
        self.keyboard
    }
    fn set_key_state(&mut self, keymap: ZXKeyboardMap)  {
        self.keyboard = keymap;
    }
}

impl<M, B, X, V> Ula<M, B, X, V>
    where V: VideoFrame
{
    #[inline(always)]
    pub(crate) fn ula_write_earmic(&mut self, flags: UlaPortFlags, ts: VideoTs) {
        let earmic = EarMic::from(flags);
        if self.last_earmic_data != earmic {
            self.last_earmic_data = earmic;
            self.earmic_out_changes.push((ts, earmic.bits()).into());
        }
    }

    #[inline(always)]
    pub(crate) fn ula_io_data(&mut self, port: u16, ts: VideoTs) -> u8 {
        self.keyboard.read_keyboard((port >> 8) as u8) &
                (u8::from(self.read_ear_in(ts)) << 6 | 0b1011_1111)
    }

    #[inline(always)]
    pub(crate) fn ula_read_io(&mut self, port: u16, ts: VideoTs) -> Option<(u8, Option<NonZeroU16>)>
        where B: BusDevice,
              B::Timestamp: From<VFrameTs<V>>
    {
        let bus_data = self.bus.read_io(port, VFrameTs::from(ts).into());
        if port & 1 == 0 {
            let ula_data = self.ula_io_data(port, ts);
            if let Some((data, ws)) = bus_data {
                return Some((ula_data & data, ws));
            }
            Some((ula_data, None))
        }
        else {
            bus_data
        }
    }

    #[inline]
    fn floating_bus(&self, ts: VideoTs) -> u8
        where M: ZxMemory
    {
        if let Some(addr) = V::floating_bus_screen_address(ts) {
            self.memory.read_screen(0, addr)
        }
        else {
            u8::max_value()
        }
    }
}
