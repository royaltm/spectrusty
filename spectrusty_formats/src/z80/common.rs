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
/*
        Offset  Length  Description
        ---------------------------
        0       1       A register
        1       1       F register
        2       2       BC register pair (LSB, i.e. C, first)
        4       2       HL register pair
        6       2       Program counter
        8       2       Stack pointer
        10      1       Interrupt register
        11      1       Refresh register (Bit 7 is not significant!)
        12      1       Bit 0  : Bit 7 of the R-register
                        Bit 1-3: Border colour
                        Bit 4  : 1=Basic SamRom switched in
                        Bit 5  : 1=Block of data is compressed
                        Bit 6-7: No meaning
            if byte 12 is 255, it has to be regarded as being 1.
        13      2       DE register pair
        15      2       BC' register pair
        17      2       DE' register pair
        19      2       HL' register pair
        21      1       A' register
        22      1       F' register
        23      2       IY register (Again LSB first)
        25      2       IX register
        27      1       Interrupt flipflop, 0=DI, otherwise EI
        28      1       IFF2 (not particularly important...)
        29      1       Bit 0-1: Interrupt mode (0, 1 or 2)
                        Bit 2  : 1=Issue 2 emulation
                        Bit 3  : 1=Double interrupt frequency
                        Bit 4-5: 1=High video synchronisation
                                 3=Low video synchronisation
                                 0,2=Normal
                        Bit 6-7: 0=Cursor/Protek/AGF joystick
                                 1=Kempston joystick
                                 2=Sinclair 2 Left joystick (or user
                                   defined, for version 3 .z80 files)
                                 3=Sinclair 2 Right joystick

Version 2 (*), 3 (when PC = 0 above):

        Offset  Length  Description
        ---------------------------
      * 30      2       Length of additional header block (see below)
            The value of the word at position 30 is 23 for version 2 files, and 54 or 55 for version 3
      * 32      2       Program counter
      * 34      1       Hardware mode (see below)
      * 35      1       If in SamRam mode, bitwise state of 74ls259.
                        For example, bit 6=1 after an OUT 31,13 (=2*6+1);
                        If in 128 mode, contains last OUT to 0x7ffd;
                        If in Timex mode, contains last OUT to 0xf4;
      * 36      1       Contains 0xff if Interface I rom paged
                        If in Timex mode, contains last OUT to 0xff
      * 37      1       Bit 0: 1 if R register emulation on
                        Bit 1: 1 if LDIR emulation on
                        Bit 2: AY sound in use, even on 48K machines
                        Bit 6: (if bit 2 set) Fuller Audio Box emulation
                        Bit 7: Modify hardware (see below)
            If bit 7 of byte 37 is set, the hardware types are modified slightly: any 48K machine becomes a 16K machine,
            any 128K machines becomes a +2 and any +3 machine becomes a +2A.
      * 38      1       Last OUT to port 0xfffd (soundchip register number)
      * 39      16      Contents of the sound chip registers
        55      2       Low T state counter
        57      1       Hi T state counter
        58      1       Flag byte used by Spectator (QL spec. emulator)
                        Ignored by Z80 when loading, zero when saving
        59      1       0xff if MGT Rom paged
        60      1       0xff if Multiface Rom paged. Should always be 0.
        61      1       0xff if 0-8191 is ROM, 0 if RAM
        62      1       0xff if 8192-16383 is ROM, 0 if RAM
        63      10      5 x keyboard mappings for user defined joystick
        73      10      5 x ASCII word: keys corresponding to mappings above
03 01 03 02 03 04 03 08 03 10 Sinclair #2
31 00 32 00 33 00 34 00 35 00
        83      1       MGT type: 0=Disciple+Epson,1=Disciple+HP,16=Plus D
        84      1       Disciple inhibit button status: 0=out, 0ff=in
        85      1       Disciple inhibit flag: 0=rom pageable, 0ff=not
     ** 86      1       Last OUT to port 0x1ffd

Hardware mode

        Value:          Meaning in v2           Meaning in v3
        -----------------------------------------------------
          0             48k                     48k
          1             48k + If.1              48k + If.1
          2             SamRam                  SamRam
          3             128k                    48k + M.G.T.
          4             128k + If.1             128k
          5             -                       128k + If.1
          6             -                       128k + M.G.T.

        Value:          Meaning
        -----------------------------------------------------
          7             Spectrum +3
          8             [mistakenly used by some versions of
                         XZX-Pro to indicate a +3]
          9             Pentagon (128K)
         10             Scorpion (256K)
         11             Didaktik-Kompakt
         12             Spectrum +2
         13             Spectrum +2A
         14             TC2048
         15             TC2068
        128             TS2068

Hereafter a number of memory blocks follow, each containing the compressed data of a 16K block.
The compression is according to the old scheme, except for the end-marker, which is now absent.
The structure of a memory block is:

        Byte    Length  Description
        ---------------------------
        0       2       Length of compressed data (without this 3-byte header)
                        If length=0xffff, data is 16384 bytes long and not compressed
        2       1       Page number of block
        3       [0]     Data

The pages are numbered, depending on the hardware mode, in the following way:

        Page    In '48 mode     In '128 mode    In SamRam mode
        ------------------------------------------------------
         0      48K rom         rom (basic)     48K rom
         1      Interface I, Disciple or Plus D rom, according to setting
         2      -               rom (reset)     samram rom (basic)
         3      -               page 0          samram rom (monitor,..)
         4      8000-bfff       page 1          Normal 8000-bfff
         5      c000-ffff       page 2          Normal c000-ffff
         6      -               page 3          Shadow 8000-bfff
         7      -               page 4          Shadow c000-ffff
         8      4000-7fff       page 5          4000-7fff
         9      -               page 6          -
        10      -               page 7          -
        11      Multiface rom   Multiface rom   -

In 48K mode, pages 4,5 and 8 are saved. In SamRam mode, pages 4 to 8 are saved.
In 128K mode, all pages from 3 to 10 are saved.
Pentagon snapshots are very similar to 128K snapshots, while Scorpion snapshots have the 16 RAM pages saved in pages 3 to 18.
There is no end marker.

version, registers, iff1, iff2, border, im, issue, joystick
*/
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
