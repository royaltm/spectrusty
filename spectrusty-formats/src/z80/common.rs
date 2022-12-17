/*
    Copyright (C) 2020-2022  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
use core::convert::TryFrom;
use std::io::{self, Read, Result};

use bitflags::bitflags;

use spectrusty_core::z80emu::InterruptMode;
use spectrusty_core::clock::FTs;
use spectrusty_core::chip::ReadEarMode;
use spectrusty_core::video::BorderColor;

use crate::{StructRead, StructWrite};
use crate::snapshot::*;

pub const PAGE_SIZE: usize = 0x4000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Z80Version { V1, V2, V3 }

bitflags! {
    #[derive(Default)]
    pub struct Flags1: u8 {
        const R_HIGH_BIT     = 0b0000_0001;
        const BORDER_COLOR   = 0b0000_1110;
        const BASIC_SAMROM   = 0b0001_0000;
        const MEM_COMPRESSED = 0b0010_0000;
    }
}

bitflags! {
    #[derive(Default)]
    pub struct Flags2: u8 {
        const INTR_MODE0       = 0b0000_0000;
        const INTR_MODE1       = 0b0000_0001;
        const INTR_MODE2       = 0b0000_0010;
        const INTR_MODE_MASK   = 0b0000_0011;
        const ISSUE2_EMULATION = 0b0000_0100;
        const DOUBLE_INTERRUPT = 0b0000_1000;
        const VIDEO_SYNC       = 0b0011_0000;
        const JOYSTICK_MODEL   = 0b1100_0000;
    }
}

bitflags! {
    #[derive(Default)]
    pub struct Flags3: u8 {
        const REG_R_EMU     = 0b0000_0001;
        const LDIR_EMU      = 0b0000_0010;
        const AY_SOUND_EMU  = 0b0000_0100;
        const AY_FULLER_BOX = 0b0100_0000;
        const ALT_HW_MODE   = 0b1000_0000;
    }
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
#[repr(packed)]
pub struct Header {
    pub a: u8,
    pub f: u8,
    pub bc: [u8;2],
    pub hl: [u8;2],
    pub pc: [u8;2],
    pub sp: [u8;2],
    pub i: u8,
    pub r7: u8,
    pub flags1: u8,
    pub de: [u8;2],
    pub bc_alt: [u8;2],
    pub de_alt: [u8;2],
    pub hl_alt: [u8;2],
    pub a_alt: u8,
    pub f_alt: u8,
    pub iy: [u8;2],
    pub ix: [u8;2],
    pub iff1: u8,
    pub iff2: u8,
    pub flags2: u8
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
#[repr(packed)]
pub struct HeaderEx {
    // version 2,3
    pub pc: [u8;2],
    pub hw_mode: u8,
    pub port1: u8,
    pub ifrom: u8,
    pub flags3: u8,
    pub ay_sel_reg: u8,
    pub ay_regs: [u8;16],
    // version 3
    pub ts_lo: [u8;2],
    pub ts_hi: u8,
    pub flags4: u8,
    pub mgt_rom: u8,
    pub mf_rom: u8,
    pub fn1: u8,
    pub fn2: u8,
    pub joy_bindings: [u8;10],
    pub joy_ascii: [u8;10],
    pub mgt_type: u8,
    pub disciple1: u8,
    pub disciple2: u8,
    pub port2: u8,
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
#[repr(packed)]
pub struct MemoryHeader {
    length: [u8;2],
    page: u8
}

pub const MEMORY_V1_TERM: &[u8] = &[0, 0xED, 0xED, 0];

// Structs must be packed and consist of `u8` or/and arrays of `u8` primitives only.
unsafe impl StructRead for Header {}
unsafe impl StructRead for HeaderEx {}
unsafe impl StructRead for MemoryHeader {}
unsafe impl StructWrite for Header {}
unsafe impl StructWrite for HeaderEx {}
unsafe impl StructWrite for MemoryHeader {}

impl From<u8> for Flags1 {
    fn from(mut byte: u8) -> Self {
        if byte == u8::max_value() {
            byte = 1;
        }
        Flags1::from_bits_truncate(byte)
    }
}

impl Flags1 {
    pub fn with_border_color(self, border: BorderColor) -> Self {
        (self & !Flags1::BORDER_COLOR) | Flags1::from(u8::from(border) << 1)
    }

    pub fn with_refresh_high_bit(mut self, r: u8) -> Self {
        self.set(Flags1::R_HIGH_BIT, (r & 0x80) != 0);
        self
    }

    pub fn border_color(self) -> BorderColor {
        let color = (self & Flags1::BORDER_COLOR).bits() >> 1;
        BorderColor::try_from(color).unwrap()
    }

    pub fn is_mem_compressed(self) -> bool {
        self.intersects(Flags1::MEM_COMPRESSED)
    }

    pub fn mix_r(self, r: u8) -> u8 {
        (r & 0x7F) | ((self & Flags1::R_HIGH_BIT).bits() << 7)
    }
}

impl From<u8> for Flags2 {
    fn from(byte: u8) -> Self {
        Flags2::from_bits_truncate(byte)
    }
}

impl Flags2 {
    pub fn with_interrupt_mode(self, im: InterruptMode) -> Self {
        (self & !Flags2::INTR_MODE_MASK) | match im {
            InterruptMode::Mode0 => Flags2::INTR_MODE0,
            InterruptMode::Mode1 => Flags2::INTR_MODE1,
            InterruptMode::Mode2 => Flags2::INTR_MODE2,
        }
    }

    pub fn with_issue2_emulation(mut self, issue: ReadEarMode) -> Self {
        self.set(Flags2::ISSUE2_EMULATION, issue == ReadEarMode::Issue2);
        self
    }

    pub fn insert_joystick_model(&mut self, joystick: JoystickModel) -> bool {
        let joy = match joystick {
            JoystickModel::Cursor    => 0,
            JoystickModel::Kempston  => 1,
            JoystickModel::Sinclair2 => 2,
            JoystickModel::Sinclair1 => 3,
            _ => return false
        } << 6;
        self.remove(Flags2::JOYSTICK_MODEL);
        self.insert(Flags2::from(joy));
        true
    }

    pub fn interrupt_mode(self) -> Result<InterruptMode> {
        InterruptMode::try_from((self & Flags2::INTR_MODE_MASK).bits())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid interrupt mode"))
    }

    pub fn is_issue2_emulation(self) -> bool {
        self.intersects(Flags2::ISSUE2_EMULATION)
    }

    pub fn joystick_model(self) -> JoystickModel {
        match (self & Flags2::JOYSTICK_MODEL).bits() >> 6 {
            0 => JoystickModel::Cursor,
            1 => JoystickModel::Kempston,
            2 => JoystickModel::Sinclair2,
            3 => JoystickModel::Sinclair1,
            _ => unreachable!()
        }
    }
}

impl From<u8> for Flags3 {
    fn from(byte: u8) -> Self {
        Flags3::from_bits_truncate(byte)
    }
}

impl Flags3 {
    pub fn is_ay_melodik(self) -> bool {
        self & (Flags3::AY_SOUND_EMU|Flags3::AY_FULLER_BOX) == Flags3::AY_SOUND_EMU
    }

    pub fn is_ay_fuller_box(self) -> bool {
        self.contains(Flags3::AY_SOUND_EMU|Flags3::AY_FULLER_BOX)
    }

    pub fn is_alt_hw_mode(self) -> bool {
        self.intersects(Flags3::ALT_HW_MODE)
    }
}

pub fn z80_to_cycles(ts_lo: u16, ts_hi: u8, model: ComputerModel) -> FTs {
    let total_ts = model.frame_tstates();
    let qts = total_ts / 4;
    let qcountdown = ts_lo as FTs;
    (((ts_hi + 1) % 4 + 1) as FTs * qts - (qcountdown + 1))
    .rem_euclid(total_ts)
}

pub fn cycles_to_z80(ts: FTs, model: ComputerModel) -> (u16, u8) {
    let total_ts = model.frame_tstates();
    let qts = total_ts / 4;
    let ts_lo = (qts - (ts.rem_euclid(qts)) - 1) as u16;
    let ts_hi = (ts.rem_euclid(total_ts) / qts - 1).rem_euclid(4) as u8;
    (ts_lo, ts_hi)
}

pub fn load_header_ex<R: Read>(mut rd: R) -> Result<(Z80Version, HeaderEx)> {
    let mut header_length = [0u8;2];
    rd.read_exact(&mut header_length)?;
    let header_length = u16::from_le_bytes(header_length);
    let version = match header_length {
        23    => Z80Version::V2,
        54|55 => Z80Version::V3,
        _ => return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid extended header size "))
    };
    let mut header_ex = HeaderEx::default();
    header_ex.read_struct_with_limit(rd.by_ref(), header_length as usize)?;
    Ok((version, header_ex))
}

/// Reads a **Z80** V2/V3 memory header and returns `(length, page, is_compressed)` on success.
pub fn load_mem_header<R: Read>(rd: R) -> Result<Option<(usize, u8, bool)>> {
    let mut header = MemoryHeader::default();
    if header.read_struct_or_nothing(rd)? {
        let length = u16::from_le_bytes(header.length);
        if length == u16::max_value() {
            Ok(Some((PAGE_SIZE, header.page, false)))
        }
        else {
            Ok(Some((length as usize, header.page, true)))
        }
    }
    else {
        Ok(None)
    }
}

impl MemoryHeader {
    pub fn new(length: u16, page: u8) -> Self {
        let length = length.to_le_bytes();
        MemoryHeader { length, page }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::convert::TryInto;
    #[test]
    fn z80_cycles_works() {
        let model = ComputerModel::Spectrum48;
        assert_eq!(model.frame_tstates(), 69888);
        let model = ComputerModel::Spectrum128;
        assert_eq!(model.frame_tstates(), 70908);
        for model in &[ComputerModel::Spectrum48, ComputerModel::Spectrum128] {
            let frame_ts = model.frame_tstates();
            let qts: u16 = (frame_ts / 4).try_into().unwrap();
            for ts in 0..frame_ts {
                let (lo, hi) = cycles_to_z80(ts, *model);
                assert!((0..=3).contains(&hi));
                assert!((0..qts).contains(&lo));
                assert_eq!(z80_to_cycles(lo, hi, *model), ts.rem_euclid(frame_ts));
            }
        }
    }
}
