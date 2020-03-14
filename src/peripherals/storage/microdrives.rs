//! ZX Microdrives for the ZX Interface 1.
use core::ops::{Index, IndexMut};
use core::marker::PhantomData;
use core::num::{NonZeroU8, NonZeroU16};
use core::fmt;
use core::iter::{Enumerate, FilterMap, Zip, IntoIterator};
use core::slice;

use std::io::{self, Read, Write};
use std::vec;

use bitvec::prelude::*;

use crate::clock::{FTs, VideoTs};
use crate::video::VideoFrame;

/// The maximum additional T-states the Interface 1 will halt Z80 during IN/OUT with DATA port.
///
///
/// Normally accessing port 0xe7 will halt the Z80 until the Interface I has collected 8 bits from the
/// microdrive head. Reading from this port while the drive motor is off results in halting the Spectrum.
pub const MAX_HALT_TS: FTs = 2 * BYTE_TS as FTs;

/// The number of T-states to signal to the control unit that the IF-1 has hanged the Spectrum indefinitely
/// (IN 0 bug).
pub const HALT_FOREVER_TS: Option<NonZeroU16> = unsafe {
    Some(NonZeroU16::new_unchecked(u16::max_value()))
};

/// [Sector] reference iterator of the [IntoIterator] implementation for [MicroCartridge].
pub type MicroCartridgeSecIter<'a> = FilterMap<
                                            Zip<
                                                slice::Iter<'a, Sector>,
                                                bitvec::slice::Iter<'a, Local, u32>
                                            >,
                                            &'a dyn Fn((&'a Sector, &'a bool)) -> Option<&'a Sector>
                                        >;

/// [Sector] mutable reference iterator of the [IntoIterator] implementation for [MicroCartridge].
pub type MicroCartridgeSecIterMut<'a> = FilterMap<
                                            Zip<
                                                slice::IterMut<'a, Sector>,
                                                bitvec::slice::Iter<'a, Local, u32>
                                            >,
                                            &'a dyn Fn((&'a mut Sector, &'a bool)) -> Option<&'a mut Sector>
                                        >;
/// An iterator returned by [MicroCartridge::iter_with_indices] method.
pub type MicroCartridgeIdSecIter<'a> = FilterMap<
                                            Enumerate<
                                                Zip<
                                                    slice::Iter<'a, Sector>,
                                                    bitvec::slice::Iter<'a, Local, u32>
                                            >>,
                                            &'a dyn Fn((usize, (&'a Sector, &'a bool))) -> Option<(u8, &'a Sector)>
                                        >;
/// The maximum number of emulated physical ZX Microdrive tape sectors.
pub const MAX_SECTORS: usize = 256;
/// The maximum number of usable ZX Microdrive tape sectors.
pub const MAX_USABLE_SECTORS: usize = 254;
/// The maximum number of drives that the ZX Interface 1 software can handle.
pub const MAX_DRIVES: usize = 8;

/// The size of the sector header in bytes, excluding the 12 preamble bytes.
pub const HEAD_SIZE: usize = 15;
/// The size of the sector data in bytes, excluding the 12 preamble bytes.
pub const DATA_SIZE: usize = 528;
// The size of the sector preamble.
const PREAMBLE_SIZE: u16 = 12;

// T-states per byte written or read by the drive
const BYTE_TS: u32 = 162;
// T-states per preamble after 10 zeroes
const PREAMBLE_SYN_TS: u32 = 1620;
// T-states per preamble after 10 zeroes and 2 0xFFs;
const HEAD_START_TS: u32 = 1945;
// Header T-states including preamble
const HEAD_TS: u32 = 4375; // 1,25 ms
// First gap T-states
const GAP1_TS: u32 = 13125; // 3,75 ms
// Data preamble T-states after 10 zeroes and 2 0xFFs;
const DATA_START_TS: u32 = 1964;
// Data T-states including preamble
const DATA_TS: u32 = 87500; // 25 ms
// Second gap T-states
const GAP2_TS: u32 = 24500; // 7 ms
// T-states / sector
const SECTOR_TS: u32 = HEAD_TS + GAP1_TS + DATA_TS + GAP2_TS; // 37 ms

pub(crate) const SECTOR_MAP_SIZE: usize = (MAX_SECTORS + 31)/32;

/// This struct represents a single [MicroCartridge] tape sector.
#[derive(Clone, Copy)]
pub struct Sector {
    /// Header data.
    pub head: [u8;HEAD_SIZE],
    /// Block data.
    pub data: [u8;DATA_SIZE],
}

/// This struct represents an emulated Microdrive tape cartridge.
///
/// It consist of up to [MAX_SECTORS] [Sector]s. Instances of this struct can be "inserted" into one
/// of 8 [ZXMicrodrives]'s emulator drives.
#[derive(Clone)]
pub struct MicroCartridge {
    sectors: Box<[Sector]>,
    sector_map: [u32;SECTOR_MAP_SIZE],
    tape_cursor: TapeCursor,
    protec: bool,
    written: Option<NonZeroU16>
}

/// Implementation of this type emulates ZX Microdrives.
///
/// Used by [ZX Interface 1][crate::bus::zxinterface1::ZxInterface1BusDevice] emulator.
#[derive(Clone, Default, Debug)]
pub struct ZXMicrodrives<V> {
    drives: [Option<MicroCartridge>;MAX_DRIVES],
    write: bool,
    erase: bool,
    comms_clk: bool,
    motor_on_drive: Option<NonZeroU8>,
    last_ts: VideoTs,
    _video_frame: PhantomData<V>
}

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub(crate) struct CartridgeState {
    pub gap: bool,
    pub syn: bool,
    pub write_protect: bool
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum SecPosition {
    Preamble1(u16),
    Header(u16),
    Gap1,
    Preamble2(u16),
    Data(u16),
    Gap2,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TapeCursor {
    cursor: u32,
    sector: u8,
    secpos: SecPosition,
}

impl Default for Sector {
    fn default() -> Self {
        Sector {
            head: [!0;HEAD_SIZE],
            data: [!0;DATA_SIZE],
        }
    }
}

impl Default for TapeCursor {
    fn default() -> Self {
        TapeCursor {
            cursor: 0,
            sector: 0,
            secpos: SecPosition::Preamble1(0)
        }
    }
}

impl Default for MicroCartridge {
    fn default() -> Self {
        MicroCartridge {
            sectors: vec![Sector::default();MAX_SECTORS].into_boxed_slice(),
            sector_map: [0;SECTOR_MAP_SIZE],
            tape_cursor: TapeCursor::default(),
            protec: false,
            written: None
        }
    }
}


impl fmt::Debug for MicroCartridge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MicroCartridge {{ sectors: {}, formatted: {}, protected: {:?}, writing: {:?}, cursor: {:?} }}",
            self.sectors.len(),
            self.sector_map.bits::<Local>().count_ones(),
            self.protec,
            self.written,
            self.tape_cursor
        )
    }
}

impl MicroCartridge {
    /// Returns the current drive's head position counted in sectors as floating point value.
    ///
    /// The fractional part indicates how far the head position is within a sector.
    pub fn head_at(&self) -> f32 {
        let TapeCursor { sector, cursor, .. } = self.tape_cursor;
        sector as f32 + cursor as f32 / SECTOR_TS as f32
    }
    /// Returns the number of the emulated physical sectors on the tape.
    #[inline]
    pub fn max_sectors(&self) -> usize {
        self.sectors.len()
    }
    /// Returns the number of formatted sectors.
    pub fn count_formatted(&self) -> usize {
        self.sector_map.bits::<Local>().count_ones()
    }
    /// Returns an iterator of formatted sectors with their original indices.
    pub fn iter_with_indices(&self) -> MicroCartridgeIdSecIter<'_> {
        let iter_map = self.sector_map.bits::<Local>().iter();
        self.sectors.iter().zip(iter_map).enumerate().filter_map(&|(i, (s, &valid))| {
            if valid { Some((i as u8, s)) } else { None }
        })
    }
    /// Returns `true` if the cartridge is write protected.
    #[inline]
    pub fn is_write_protected(&self) -> bool {
        self.protec
    }
    /// Changes the write protected flag of the cartridge.
    #[inline]
    pub fn set_write_protected(&mut self, protect: bool) {
        self.protec = protect;
    }
    /// Returns `true` if the given `sector` is formatted.
    ///
    /// # Panics
    /// Panics if `sector` equals to or is above the `max_sectors` limit.
    #[inline]
    pub fn is_sector_formatted(&self, sector: u8) -> bool {
        self.sector_map.bits::<Local>()[sector as usize]
    }
    /// Creates a new instance of [MicroCartridge] with provided sectors.
    ///
    /// # Note
    /// Content of the sectors is not verified and is assumed to be properly formatted.
    ///
    /// # Panics
    /// The number of sectors provided must not be greater than [MAX_USABLE_SECTORS].
    /// `max_sectors` must not be 0 and must be grater or equal to the number of provided sectors and
    ///  must not be greater than [MAX_SECTORS].
    pub fn new_with_sectors<S: Into<Vec<Sector>>>(
            sectors: S,
            write_protect: bool,
            max_sectors: usize
        ) -> Self
    {
        let mut sectors = sectors.into();
        assert!(max_sectors > 0 && max_sectors <= MAX_SECTORS &&
                sectors.len() <= max_sectors && sectors.len() <= MAX_USABLE_SECTORS);
        let mut sector_map = [0u32;SECTOR_MAP_SIZE];
        sector_map.bits_mut::<Local>()[0..sectors.len()].set_all(true);
        sectors.resize(max_sectors, Sector::default());
        let sectors = sectors.into_boxed_slice();
        MicroCartridge {
            sectors,
            sector_map,
            tape_cursor: TapeCursor::default(),
            protec: write_protect,
            written: None
        }
    }
    /// Creates a new instance of [MicroCartridge] with custom `max_sectors` number.
    ///
    /// # Panics
    /// `max_sectors` must not be 0 and must not be greater than [MAX_SECTORS].
    pub fn new(max_sectors: usize) -> Self {
        assert!(max_sectors > 0 && max_sectors <= MAX_SECTORS);
        MicroCartridge {
            sectors: vec![Sector::default();max_sectors].into_boxed_slice(),
            sector_map: [0;SECTOR_MAP_SIZE],
            tape_cursor: TapeCursor::default(),
            protec: false,
            written: None
        }
    }
}

/// Iterates through formatted sectors.
impl<'a> IntoIterator for &'a MicroCartridge {
    type Item = &'a Sector;
    type IntoIter = MicroCartridgeSecIter<'a>;
    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        let iter_map = self.sector_map.bits::<Local>().iter();
        self.sectors.iter().zip(iter_map).filter_map(&|(s, &valid)| {
            if valid { Some(s) } else { None }
        })
    }
}

/// Iterates through formatted sectors.
impl<'a> IntoIterator for &'a mut MicroCartridge {
    type Item = &'a mut Sector;
    type IntoIter = MicroCartridgeSecIterMut<'a>;
    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        let iter_map = self.sector_map.bits::<Local>().iter();
        self.sectors.iter_mut().zip(iter_map).filter_map(&|(s, &valid)| {
            if valid { Some(s) } else { None }
        })
    }
}

impl Index<u8> for MicroCartridge {
    type Output = Sector;
    #[inline]
    fn index(&self, index: u8) -> &Self::Output {
        &self.sectors[index as usize]
    }
}

impl IndexMut<u8> for MicroCartridge {
    #[inline]
    fn index_mut(&mut self, index: u8) -> &mut Self::Output {
        &mut self.sectors[index as usize]
    }
}

const HEAD_PREAMBLE: u32 = 0;
const HEAD_SYN_END:  u32 = HEAD_START_TS - 1;
const HEAD_START:    u32 = HEAD_START_TS;
const HEAD_END:      u32 = HEAD_TS - 1;
const GAP1_START:    u32 = HEAD_TS;
const GAP1_END:      u32 = HEAD_TS + GAP1_TS - 1;
const DATA_PREAMBLE: u32 = HEAD_TS + GAP1_TS;
const DATA_SYN_END:  u32 = DATA_PREAMBLE + DATA_START_TS - 1;
const DATA_START:    u32 = DATA_PREAMBLE + DATA_START_TS;
const DATA_END:      u32 = HEAD_TS + GAP1_TS + DATA_TS - 1;
const GAP2_START:    u32 = HEAD_TS + GAP1_TS + DATA_TS;

impl SecPosition {
    #[inline]
    fn from_sector_cursor(cursor: u32) -> Self {
        assert_eq!((HEAD_END + 1 - HEAD_START) / HEAD_SIZE as u32, BYTE_TS as u32);
        assert_eq!((DATA_END + 1 - DATA_START) / DATA_SIZE as u32, BYTE_TS as u32);
        match cursor {
            HEAD_PREAMBLE..=HEAD_SYN_END => SecPosition::Preamble1(((cursor - HEAD_PREAMBLE)/BYTE_TS) as u16),
            HEAD_START..=HEAD_END        => SecPosition::Header(((cursor - HEAD_START)/BYTE_TS) as u16),
            GAP1_START..=GAP1_END        => SecPosition::Gap1,
            DATA_PREAMBLE..=DATA_SYN_END => SecPosition::Preamble2(((cursor - DATA_PREAMBLE)/BYTE_TS) as u16),
            DATA_START..=DATA_END        => SecPosition::Data(((cursor - DATA_START)/BYTE_TS) as u16),
            GAP2_START..=core::u32::MAX  => SecPosition::Gap2,
        }
    }
}

impl TapeCursor {
    #[inline]
    fn forward(&mut self, ts: u32, seclen: u32) { // cursor + sectors
        let mut cursor = self.cursor.saturating_add(ts);
        if cursor >= SECTOR_TS {
            let diffsec = (cursor / SECTOR_TS) as u32;
            self.sector = Self::add_sectors(self.sector, diffsec, seclen);
            cursor %= SECTOR_TS;
        }
        self.cursor = cursor;
        self.secpos = SecPosition::from_sector_cursor(cursor);
    }

    #[inline]
    fn add_sectors(sector: u8, delta: u32, seclen: u32) -> u8 {
        ((sector as u32 + delta) % seclen) as u8
    }
}

impl MicroCartridge {
    #[inline(always)]
    fn set_valid_sector(&mut self, sector: u8, valid: bool) {
        self.sector_map.bits_mut::<Local>().set(sector as usize, valid);
    }

    #[inline(always)]
    fn clear_all_sectors(&mut self) {
        for p in self.sector_map.iter_mut() {
            *p = 0
        }
    }

    // called when erasing began
    fn erase_start(&mut self, delta_ts: u32) {
        self.forward(delta_ts);
        let TapeCursor { sector, secpos, .. } = self.tape_cursor;
        self.written = None;
        if self.is_sector_formatted(sector) {
            match secpos {
                SecPosition::Gap2 |
                SecPosition::Gap1 => {} // synchronized sector write may follow
                _ => { // otherwise clear sector
                    self.set_valid_sector(sector, false);
                }
            }
        }
    }

    // called when writing began or just erasing ended / motor stopped etc
    fn erase_forward(&mut self, delta_ts: u32) {
        let prev_cursor = self.tape_cursor;
        self.forward(delta_ts);
        let TapeCursor { sector, secpos, .. } = self.tape_cursor;
        if prev_cursor.sector != sector { // clear all previous sectors
            if delta_ts >= SECTOR_TS * (self.sectors.len() as u32 - 1) {
                self.clear_all_sectors();
            }
            else {
                let prev_sector = if let SecPosition::Gap2 = prev_cursor.secpos {
                    TapeCursor::add_sectors(prev_cursor.sector, 1, self.sectors.len() as u32)
                }
                else {
                    prev_cursor.sector
                };
                if sector < prev_sector {
                    self.sector_map.bits_mut::<Local>()[prev_sector.into()..].set_all(false);
                    self.sector_map.bits_mut::<Local>()[..sector.into()].set_all(false);
                }
                else {
                    self.sector_map.bits_mut::<Local>()[prev_sector.into()..=sector.into()].set_all(false);
                }
            }
        }
        else {
            match (prev_cursor.secpos, secpos) {
                // synchronized write has ended
                (SecPosition::Gap2, SecPosition::Gap2)|
                // synchronized write should follow
                (SecPosition::Gap1, SecPosition::Gap1)|
                (SecPosition::Gap1, SecPosition::Preamble2(..))|
                (SecPosition::Preamble2(..), SecPosition::Preamble2(..)) => {}
                _ => { // otherwise clear sector
                    self.set_valid_sector(sector, false);
                }
            }
        }
    }

    fn write_end(&mut self, delta_ts: u32) { // switched r/w w -> r, erase off, or motor off
        self.forward(delta_ts);
        // println!("wr: {:?}, delta: {} sec: {} cur: {} {:?}", self.written, delta_ts, self.tape_cursor.sector, self.tape_cursor.cursor, self.tape_cursor.secpos);
        if let Some(written) = self.written {
            if delta_ts < 2*BYTE_TS {
                const HEAD_SIZE_MIN: u16 = PREAMBLE_SIZE + HEAD_SIZE as u16;
                const HEAD_SIZE_MAX: u16 = HEAD_SIZE_MIN + 55;
                // const DATA_SIZE_MIN: u16 = PREAMBLE_SIZE + DATA_SIZE as u16;
                // const DATA_SIZE_MAX: u16 = DATA_SIZE_MIN + 110;
                match (written.get(), self.tape_cursor.secpos) {
                    (HEAD_SIZE_MIN..=HEAD_SIZE_MAX, SecPosition::Gap1) => {
                        // this may yield a "valid" sector with invalid data, but harmless
                        self.set_valid_sector(self.tape_cursor.sector, true);
                    }
                    // (DATA_SIZE_MIN..=DATA_SIZE_MAX, SecPosition::Gap2) => {
                    //     println!("ok valid data");
                    // }
                    _ => {}
                }
            }
        }
        self.written = None;
    }

    fn write_data_forward(&mut self, data: u8, delta_ts: u32) -> u16 {
        if let Some(written) = self.written {
            self.written = NonZeroU16::new(written.get().saturating_add(1));
            self.forward(delta_ts);
            let TapeCursor { sector, cursor, secpos } = self.tape_cursor;
            if delta_ts < BYTE_TS*3/2 {
                // println!("wr: {}, data: {:x}, sec: {} cur: {} {:?}", written.get(), data, sector, cursor, secpos);
                match (written.get(), data, secpos) {
                    (wr, 0x00, SecPosition::Preamble1(offs @ 1..=9))|
                    (wr, 0xff, SecPosition::Preamble1(offs @ 10..=11)) if wr == offs => {
                        return (BYTE_TS - (cursor - HEAD_PREAMBLE) % BYTE_TS) as u16;
                    }
                    (wr, _, SecPosition::Preamble1(PREAMBLE_SIZE)) if wr == PREAMBLE_SIZE => {
                        self.sectors[sector as usize].head[0] = data;
                        return (HEAD_START + BYTE_TS - cursor) as u16;
                    }
                    (wr, _, SecPosition::Header(offset)) if wr == PREAMBLE_SIZE + offset => {
                        self.sectors[sector as usize].head[offset as usize] = data;
                        return (BYTE_TS - (cursor - HEAD_START) % BYTE_TS) as u16;
                    }
                    (wr, _, SecPosition::Gap1) if wr >= PREAMBLE_SIZE + HEAD_SIZE as u16 => {
                        return (BYTE_TS - (cursor - GAP1_START) % BYTE_TS) as u16;
                    }
                    (wr, 0x00, SecPosition::Preamble2(offs @ 1..=9))|
                    (wr, 0xff, SecPosition::Preamble2(offs @ 10..=11)) if wr == offs => {
                        return (BYTE_TS - (cursor - DATA_PREAMBLE) % BYTE_TS) as u16;
                    }
                    (wr, _, SecPosition::Preamble2(PREAMBLE_SIZE)) if wr == PREAMBLE_SIZE => {
                        self.sectors[sector as usize].data[0] = data;
                        return (DATA_START + BYTE_TS - cursor) as u16;
                    }
                    (wr, _, SecPosition::Data(offset)) if wr == PREAMBLE_SIZE + offset => {
                        self.sectors[sector as usize].data[offset as usize] = data;
                        return (BYTE_TS - (cursor - DATA_START) % BYTE_TS) as u16;
                    }
                    (wr, _, SecPosition::Gap2) if wr >= PREAMBLE_SIZE + DATA_SIZE as u16 => {
                        return (BYTE_TS - (cursor - GAP2_START) % BYTE_TS) as u16;
                    }
                    _=> {}
                }
            }
            self.set_valid_sector(sector, false); // just erase all sector
            (BYTE_TS - cursor % BYTE_TS) as u16
        }
        else {
            // println!("start: {}", delta_ts);
            self.erase_forward(delta_ts);
            self.written = NonZeroU16::new(1);
            let TapeCursor { mut sector, secpos, .. } = self.tape_cursor;
            // println!("sector: {} cur: {} wr: {:?}, data: {:x}, {:?}", sector, self.tape_cursor.cursor, self.written, data, secpos);
            if data == 0 {
                if self.is_sector_formatted(sector) {
                    // println!("overwrite data block");
                    if let SecPosition::Preamble2(0..=2)|SecPosition::Gap1 = secpos { // synchronized write data sector
                        self.tape_cursor.cursor = DATA_PREAMBLE;
                        self.tape_cursor.secpos = SecPosition::Preamble2(0);
                        return BYTE_TS as u16;
                    }
                }
                if let SecPosition::Gap2 = secpos { // write starts on next sector
                    sector = TapeCursor::add_sectors(sector, 1, self.sectors.len() as u32);
                }
                // println!("begin sector");
                // write start somewhere in the middle of the sector, we just make it the new beginning of a sector 
                self.tape_cursor.sector = sector;
                self.tape_cursor.cursor = 0;
                self.tape_cursor.secpos = SecPosition::Preamble1(0);
            }
            self.set_valid_sector(sector, false); // just erase this sector
            BYTE_TS as u16
        }
    }

    fn read_data_forward(&mut self, delta_ts: u32) -> (u8, u16) { // data, delay
        self.forward(delta_ts);
        let TapeCursor { sector, cursor, secpos, .. } = self.tape_cursor;
        if self.is_sector_formatted(sector) {
            // println!("sec: {} cur: {} {:?} {:?}", sector, cursor, secpos, res);
            return match secpos {
                SecPosition::Preamble1(10..=11) => {
                    let data = self.sectors[sector as usize].head[0];
                    let delay = HEAD_START + BYTE_TS - cursor;
                    (data, delay as u16)
                }
                SecPosition::Header(offset) => {
                    let data = self.sectors[sector as usize].head[offset as usize];
                    let delay = BYTE_TS - (cursor - HEAD_START) % BYTE_TS;
                    (data, delay as u16)
                }
                SecPosition::Preamble1(..) => {
                    (0, (BYTE_TS - (cursor - HEAD_PREAMBLE) % BYTE_TS) as u16)
                }
                SecPosition::Gap1 => {
                    (!0, (BYTE_TS - (cursor - GAP1_START) % BYTE_TS) as u16)
                }
                SecPosition::Preamble2(10..=11) => {
                    let data = self.sectors[sector as usize].data[0];
                    let delay = DATA_START + BYTE_TS - cursor;
                    (data, delay as u16)
                }
                SecPosition::Data(offset) => {
                    let data = self.sectors[sector as usize].data[offset as usize];
                    let delay = BYTE_TS - (cursor - DATA_START) % BYTE_TS;
                    (data, delay as u16)
                }
                SecPosition::Preamble2(..) => {
                    (0, (BYTE_TS - (cursor - HEAD_PREAMBLE) % BYTE_TS) as u16)
                }
                SecPosition::Gap2 => {
                    let data = self.sectors[sector as usize].data[DATA_SIZE - 1];
                    (data, (BYTE_TS - (cursor - GAP2_START) % BYTE_TS) as u16)
                }
            }
        }
        (!0, (BYTE_TS - cursor % BYTE_TS) as u16)
    }

    #[inline(always)]
    fn forward(&mut self, delta_ts: u32) {
        self.tape_cursor.forward(delta_ts, self.sectors.len() as u32);
    }

    #[inline]
    fn gap_syn_protect(&self) -> CartridgeState {
        let mut gap = false;
        let mut syn = false;
        let TapeCursor { sector, secpos, .. } = self.tape_cursor;
        if self.is_sector_formatted(sector) {
            match secpos {
                SecPosition::Preamble1(0..=9)|
                SecPosition::Preamble2(0..=9) => {
                    gap = true;
                }
                SecPosition::Preamble1(10..=11)|
                SecPosition::Preamble2(10..=11) => {
                    gap = true;
                    syn = true;
                }
                _ => {}
            };
        }
        CartridgeState {
            gap, syn, write_protect: self.protec
        }
    }
}

impl<V> ZXMicrodrives<V> {
    /// Inserts a `cartridge` into the `drive_index` optionally returning a cartridge
    /// that was previously in the same drive.
    ///
    /// `drive_index` is a drive index number from 0 to 7.
    ///
    /// # Panics
    /// Panics if the `drive_index` is above 7.
    pub fn replace_cartridge(
            &mut self,
            drive_index: usize,
            cartridge: MicroCartridge
        ) -> Option<MicroCartridge>
    {
        assert!(drive_index < MAX_DRIVES);
        let prev_cartridge = self.drives[drive_index].replace(cartridge);
        if self.erase {
            if let Some(cartridge) = self.drive_if_current(drive_index) {
                cartridge.erase_start(0);
            }
        }
        prev_cartridge
    }
    /// Removes and optionally returns a cartridge from the `drive_index`.
    ///
    /// `drive_index` is a drive index number from 0 to 7.
    ///
    /// # Panics
    /// Panics if the `drive_index` is above 7.
    pub fn take_cartridge(&mut self, index: usize) -> Option<MicroCartridge> {
        assert!(index < MAX_DRIVES);
        self.drives[index].take()
    }
    /// Returns a reference to the cartridge that is being currently in use along with its drive index.
    pub fn cartridge_in_use(&self) -> Option<(usize, &MicroCartridge)> {
        self.motor_on_drive.and_then(move |drive_on| {
            let drive_index = (drive_on.get() - 1) as usize;
            self.drives[drive_index & 7].as_ref()
            .map(|mc| (drive_index, mc))
        })
    }

    fn current_drive(&mut self) -> Option<&mut MicroCartridge> {
        self.motor_on_drive.and_then(move |drive_on|
            self.drives[(drive_on.get() - 1) as usize & 7].as_mut()
        )
    }

    fn drive_if_current(&mut self, index: usize) -> Option<&mut MicroCartridge> {
        self.motor_on_drive.and_then(move |drive_on| {
            let drive_index = (drive_on.get() - 1) as usize;
            if drive_index == index {
                self.drives[drive_index & 7].as_mut()
            }
            else {
                None
            }
        })
    }
}

impl<V: VideoFrame> ZXMicrodrives<V> {
    fn vts_diff_update(&mut self, timestamp: VideoTs) -> u32 {
        let delta_ts = V::vts_diff(self.last_ts, timestamp);
        self.last_ts = timestamp;
        debug_assert!(delta_ts >= 0);
        delta_ts as u32
    }

    pub(crate) fn reset(&mut self, timestamp: VideoTs) {
        let delta_ts = self.vts_diff_update(timestamp);
        self.stop_motor(delta_ts);
        self.write = false;
        self.erase = false;
        self.comms_clk = false;
    }

    /// This method should be called after each emulated frame.
    pub(crate) fn next_frame(&mut self, timestamp: VideoTs) {
        let delta_ts = self.vts_diff_update(timestamp);
        let (erase, write) = (self.erase, self.write);
        if let Some(cartridge) = self.current_drive() {
            if erase && (!write || delta_ts > 2*BYTE_TS) {
                cartridge.erase_forward(delta_ts);
            }
            else {
                cartridge.forward(delta_ts);
            }
        }
        self.last_ts = V::vts_saturating_sub_frame(self.last_ts);
    }


    pub(crate) fn read_state(&mut self, timestamp: VideoTs) -> CartridgeState {
        let delta_ts = self.vts_diff_update(timestamp);
        let erase = self.erase;
        if let Some(cartridge) = self.current_drive() {
            if erase {
                cartridge.erase_forward(delta_ts);
            }
            else {
                cartridge.forward(delta_ts);
            }
            return cartridge.gap_syn_protect();
        }
        CartridgeState::default()
    }

    // called when one of: erase, r/w, comms clk has changed
    // NOTE: comms_out is just a data
    pub(crate) fn write_control(
            &mut self,
            timestamp: VideoTs,
            erase: bool,
            write: bool,
            comms_clk: bool,
            comms_out: bool
        )
    {
        let delta_ts = self.vts_diff_update(timestamp);
        if comms_clk != self.comms_clk {
            self.comms_clk = comms_clk;
            if comms_clk { // change drive motor
                if comms_out { // turn on first drive
                    self.stop_motor(delta_ts);
                    self.motor_on_drive = NonZeroU8::new(1);
                }
                else { // shift drive motor signal
                    self.motor_on_drive = self.stop_motor(delta_ts).and_then(|n_drive|
                        if n_drive.get() == 8 {
                            None
                        } else {
                            NonZeroU8::new(n_drive.get() + 1)
                        }
                    );
                }
                // allow to erase immediately if erase is set (wrongly, but possibly)
                self.erase = false;
                self.write = false;
            }
        }

        if erase && !self.erase {
            if let Some(cartridge) = self.current_drive() {
                cartridge.erase_start(delta_ts);
            }
        }
        else if self.erase {
            if !write && self.write {
                if let Some(cartridge) = self.current_drive() {
                    cartridge.write_end(delta_ts);
                }
            }
            else if (write && !self.write) || (!erase && self.erase) {
                if let Some(cartridge) = self.current_drive() {
                    cartridge.erase_forward(delta_ts);
                }
            }
        }
        self.erase = erase;
        self.write = write;
    }

    pub(crate) fn write_data(&mut self, data: u8, timestamp: VideoTs) -> u16 {
        let delta_ts = self.vts_diff_update(timestamp);
        if self.write && self.erase { // what happens when write is on and erase off?
            if let Some(cartridge) = self.current_drive() {
                return cartridge.write_data_forward(data, delta_ts);
            }
        }
        0
    }

    pub(crate) fn read_data(&mut self, timestamp: VideoTs) -> (u8, Option<NonZeroU16>) {
        let delta_ts = self.vts_diff_update(timestamp);
        if self.erase {
            if let Some(cartridge) = self.current_drive() {
                cartridge.erase_forward(delta_ts);
            }
        }
        if !(self.write || self.erase) {
            if let Some(cartridge) = self.current_drive() {
                let (data, delay) = cartridge.read_data_forward(delta_ts);
                return (data, NonZeroU16::new(delay))
            }
        }
        // we could hang Spectrum here according to ZX Interface 1 IN 0 bug.
        (!0, HALT_FOREVER_TS)
    }

    fn stop_motor(&mut self, delta_ts: u32) -> Option<NonZeroU8> {
        let motor_on_drive = self.motor_on_drive;
        if self.erase {
            if let Some(cartridge) = self.current_drive() {
                cartridge.erase_forward(delta_ts);
                cartridge.written = None;
            }
        }
        self.motor_on_drive = None;
        motor_on_drive
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chip::ula::{UlaTsCounter, UlaVideoFrame};

    #[test]
    fn microdrives_works() {
        let mut drive: ZXMicrodrives<UlaVideoFrame> = Default::default();
        assert_eq!(drive.write, false);
        assert_eq!(drive.erase, false);
        assert_eq!(drive.comms_clk, false);
        assert_eq!(drive.motor_on_drive, None);
        assert_eq!(drive.read_data(VideoTs::new(0, 10)), (!0, NonZeroU16::new(65535)));
        assert_eq!(drive.last_ts, VideoTs::new(0, 10));
        assert_eq!(drive.write_data(0xAA, VideoTs::new(0, 11)), 0);
        assert_eq!(drive.last_ts, VideoTs::new(0, 11));
        assert_eq!(drive.read_state(VideoTs::new(0, 20)), CartridgeState{ gap: false, syn: false, write_protect: false});
        assert_eq!(drive.last_ts, VideoTs::new(0, 20));
        drive.write_control(VideoTs::new(0, 30), true, false, true, true);
        assert_eq!(drive.write, false);
        assert_eq!(drive.erase, true);
        assert_eq!(drive.comms_clk, true);
        assert_eq!(drive.last_ts, VideoTs::new(0, 30));
        assert_eq!(drive.motor_on_drive, NonZeroU8::new(1));
        drive.write_control(VideoTs::new(0, 40), true, false, false, false);
        assert_eq!(drive.write, false);
        assert_eq!(drive.erase, true);
        assert_eq!(drive.comms_clk, false);
        assert_eq!(drive.last_ts, VideoTs::new(0, 40));
        assert_eq!(drive.motor_on_drive, NonZeroU8::new(1));
        assert_eq!(drive.replace_cartridge(1, MicroCartridge::new(5)).is_none(), true);
        drive.write_control(VideoTs::new(0, 40), true, false, true, false);
        // will write header block
        drive.write_control(VideoTs::new(0, 50), true, true, false, false);
        assert_eq!(drive.write, true);
        assert_eq!(drive.erase, true);
        assert_eq!(drive.comms_clk, false);
        assert_eq!(drive.last_ts, VideoTs::new(0, 50));
        assert_eq!(drive.motor_on_drive, NonZeroU8::new(2));
        assert_eq!(drive.read_state(VideoTs::new(0, 60)), CartridgeState { gap: false, syn: false, write_protect: false});
        let mut utsc = UlaTsCounter::new(1, 0);
        for _ in 0..10 {
            let delay = drive.write_data(0, utsc.into());
            println!("delay 0x00: {} {:?}", delay, utsc.tsc);
            utsc += delay as u32 + 21 + 16;
        }
        for _ in 0..2 {
            let delay = drive.write_data(!0, utsc.into());
            println!("delay 0xff: {} {:?}", delay, utsc.tsc);
            utsc += delay as u32 + 11 + 13;
        }
        for i in 1..19 {
            let delay = drive.write_data(i, utsc.into());
            println!("delay: 0x{:02x} {} {:?}", i, delay, utsc.tsc);
            utsc += delay as u32 + 21 + 16;
        }
        let cartridge = drive.current_drive().unwrap();
        assert_eq!(cartridge.sector_map, [0x00000000, 0x00000000, 0x00000000, 0x00000000,
                                          0x00000000, 0x00000000, 0x00000000, 0x00000000]);
        assert_eq!(cartridge.written, NonZeroU16::new(30));
        assert_eq!(cartridge.sectors[0].head, [1u8,2,3,4,5,6,7,8,9,10,11,12,13,14,15]);
        assert_eq!(&cartridge.sectors[0].data[..], &[!0u8;528][..]);
        assert_eq!(cartridge.tape_cursor, TapeCursor {
            cursor: GAP1_START + 2 * BYTE_TS + 21 + 16,
            sector: 0,
            secpos: SecPosition::Gap1,
        });
        utsc += 36;
        // writing ends, erasing contitnues
        drive.write_control(utsc.into(), true, false, false, false);
        let cartridge = drive.current_drive().unwrap();
        assert_eq!(cartridge.written, None);
        let sector_map = cartridge.sector_map.bits::<Local>();
        assert_eq!(sector_map[0], true);
        for valid in &sector_map[1..] {
            assert_eq!(*valid, false);
        }
        assert_eq!(cartridge.sectors[0].head, [1u8,2,3,4,5,6,7,8,9,10,11,12,13,14,15]);
        assert_eq!(&cartridge.sectors[0].data[..], &[!0u8;528][..]);
        assert_eq!(cartridge.tape_cursor, TapeCursor {
            cursor: GAP1_START + 3 * BYTE_TS + 21 + 16 + 36,
            sector: 0,
            secpos: SecPosition::Gap1,
        });
        // erasing gap
        utsc += 11443;
        // will write data block
        drive.write_control(utsc.into(), true, true, false, false);
        utsc += 48;
        for _ in 0..10 {
            let delay = drive.write_data(0, utsc.into());
            println!("delay 0x00: {} {:?}", delay, utsc.tsc);
            utsc += delay as u32 + 11 + 13;
        }
        for _ in 0..2 {
            let delay = drive.write_data(!0, utsc.into());
            println!("delay 0xff: {} {:?}", delay, utsc.tsc);
            utsc += delay as u32 + 19;
        }
        for i in 1..630u16 { // format block
            if utsc.is_eof() {
                drive.next_frame(utsc.into());
                utsc.wrap_frame();
            }
            let delay = drive.write_data(!(i as u8), utsc.into());
            println!("delay: 0x{:02x} {} {:?}", i, delay, utsc.tsc);
            utsc += delay as u32 + 11 + 13;
        }
        let cartridge = drive.current_drive().unwrap();
        assert_eq!(cartridge.written, NonZeroU16::new(641));
        assert_eq!(cartridge.sectors[0].head, [1u8,2,3,4,5,6,7,8,9,10,11,12,13,14,15]);
        for (i, data) in cartridge.sectors[0].data.iter().enumerate() {
            assert_eq!(!(i + 1) as u8, *data);
        }
        assert_eq!(cartridge.tape_cursor, TapeCursor {
            cursor: 121224, sector: 0, secpos: SecPosition::Gap2 });
        let sector_map = cartridge.sector_map.bits::<Local>();
        assert_eq!(sector_map[0], true);
        // writing ends, erasing contitnues
        drive.write_control(utsc.into(), true, false, false, false);
        let cartridge = drive.current_drive().unwrap();
        let sector_map = cartridge.sector_map.bits::<Local>();
        assert_eq!(sector_map[0], true);
        // erasing gap
        utsc += 8941;
        // writing ends
        drive.write_control(utsc.into(), false, false, false, false);
        let cartridge = drive.current_drive().unwrap();
        assert_eq!(cartridge.tape_cursor, TapeCursor {
            cursor: 827, sector: 1, secpos: SecPosition::Preamble1(5) });
        let sector_map = cartridge.sector_map.bits::<Local>();
        assert_eq!(sector_map[0], true);
        for valid in &sector_map[1..] {
            assert_eq!(*valid, false);
        }

        fn find_gap_sync(mut utsc: UlaTsCounter, drive: &mut ZXMicrodrives<UlaVideoFrame>) -> UlaTsCounter {
            let mut counter = 0;
            while !drive.read_state(utsc.into()).gap {
                counter += 1;
                utsc += 108;
                if utsc.is_eof() {
                    drive.next_frame(utsc.into());
                    utsc.wrap_frame();
                }
            }
            println!("counter: {}", counter);
            for _ in 1..6 {
                utsc += 88;
                assert!(drive.read_state(utsc.into()).gap);
            }
            utsc += 25;
            'outer: loop {
                for i in 1..=60 {
                    let state = drive.read_state(utsc.into());
                    utsc += 38;
                    assert!(state.gap);
                    if state.syn { break 'outer }
                    println!("sync not found: {}", i);
                }
                assert!(false, "failed to find syn");
            }
            utsc
        }

        // find a gap and read header
        utsc += 108;
        utsc = find_gap_sync(utsc, &mut drive);
        let cartridge = drive.current_drive().unwrap();
        println!("{:?} {:?}", cartridge.tape_cursor, utsc);
        utsc += 92;
        for i in 1..=15 {
            let (data, delay) = drive.read_data(utsc.into());
            println!("data: {:02x} delay: {:?}  {:?}", data, delay, utsc);
            assert_eq!(i, data);
            utsc += delay.unwrap().get() as u32 + 21 + 16;
        }
        let (data, delay) = drive.read_data(utsc.into());
        println!("data: {:02x} delay: {:?}  {:?}", data, delay, utsc);
        assert_eq!(data, !0);
        utsc += delay.unwrap().get() as u32 + 121;

        // find a gap and read data
        assert_eq!(drive.read_state(utsc.into()).gap, false);
        utsc += 10000;
        assert_eq!(drive.read_state(utsc.into()).gap, false);
        utsc = find_gap_sync(utsc, &mut drive);
        let cartridge = drive.current_drive().unwrap();
        println!("{:?} {:?}", cartridge.tape_cursor, utsc);
        utsc += 92;
        for i in 1..=528 {
            let (data, delay) = drive.read_data(utsc.into());
            println!("data: {:02x} delay: {:?}  {:?}", data, delay, utsc);
            assert_eq!(!i as u8, data);
            utsc += delay.unwrap().get() as u32 + 21 + 16;
            if utsc.is_eof() {
                drive.next_frame(utsc.into());
                utsc.wrap_frame();
            }
        }
        let (data, delay) = drive.read_data(utsc.into());
        println!("data: {:02x} delay: {:?}  {:?}", data, delay, utsc);
        assert_eq!(data, 239);
        utsc += delay.unwrap().get() as u32 + 21;

        // find a gap again but this time override data after reading header
        utsc += 10800;
        utsc = find_gap_sync(utsc, &mut drive);
        let cartridge = drive.current_drive().unwrap();
        println!("{:?} {:?}", cartridge.tape_cursor, utsc);
        utsc += 92;
        for i in 1..=15 {
            let (data, delay) = drive.read_data(utsc.into());
            println!("data: {:02x} delay: {:?}  {:?}", data, delay, utsc);
            assert_eq!(i, data);
            utsc += delay.unwrap().get() as u32 + 21 + 16;
        }
        // overwrite data sector
        assert_eq!(drive.read_state(utsc.into()), CartridgeState {
            gap: false, syn: false, write_protect: false});
        utsc += 98;
        drive.write_control(utsc.into(), true, false, false, false);
        utsc += 9524;
        drive.write_control(utsc.into(), true, true, false, false);
        // now write data
        utsc += 47;
        for _ in 0..10 {
            let delay = drive.write_data(0, utsc.into());
            println!("delay 0x00: {} {:?}", delay, utsc.tsc);
            utsc += delay as u32 + 21;
        }
        for _ in 0..2 {
            let delay = drive.write_data(!0, utsc.into());
            println!("delay 0xff: {} {:?}", delay, utsc.tsc);
            utsc += delay as u32 + 21;
        }
        for i in 0..531u16 { // format block
            if utsc.is_eof() {
                drive.next_frame(utsc.into());
                utsc.wrap_frame();
            }
            let delay = drive.write_data(0xA5^(i as u8), utsc.into());
            println!("delay: 0x{:02x} {} {:?}", i, delay, utsc.tsc);
            utsc += delay as u32 + 21 + 16;
        }
        let cartridge = drive.current_drive().unwrap();
        assert_eq!(cartridge.written, NonZeroU16::new(543));
        assert_eq!(cartridge.sectors[0].head, [1u8,2,3,4,5,6,7,8,9,10,11,12,13,14,15]);
        for (i, data) in cartridge.sectors[0].data.iter().enumerate() {
            assert_eq!(0xA5^(i as u8), *data);
        }
        assert_eq!(cartridge.tape_cursor, TapeCursor {
            cursor: 105361, sector: 0, secpos: SecPosition::Gap2 });
        let sector_map = cartridge.sector_map.bits::<Local>();
        assert_eq!(sector_map[0], true);
        // writing ends, erasing continues for short time
        utsc += 18;
        drive.write_control(utsc.into(), true, false, false, false);
        // writing ends
        utsc += 112;
        drive.write_control(utsc.into(), false, false, false, false);
        let cartridge = drive.current_drive().unwrap();
        assert_eq!(cartridge.tape_cursor, TapeCursor {
            cursor: 105653, sector: 0, secpos: SecPosition::Gap2 });
        let sector_map = cartridge.sector_map.bits::<Local>();
        assert_eq!(sector_map[0], true);
        for valid in &sector_map[1..] {
            assert_eq!(*valid, false);
        }
        // turn all motors off
        utsc += 1000;
        if utsc.is_eof() {
            drive.next_frame(utsc.into());
            utsc.wrap_frame();
        }
        for i in 0..7 {
            assert_eq!(drive.motor_on_drive, NonZeroU8::new(i + 2));
            drive.write_control(utsc.into(), true, false, true, false);
            utsc += 3500;
            drive.write_control(utsc.into(), true, false, false, false);
            utsc += 3500;
        }
        assert_eq!(drive.motor_on_drive, None);
    }
}
