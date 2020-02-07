#![allow(unused_imports)]
use core::fmt;
use core::num::NonZeroI16;
use core::convert::TryFrom;
use core::ptr::NonNull;
use core::marker::{PhantomPinned, PhantomData};
use std::borrow::Cow;
use std::pin::Pin;

#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

use memchr::memchr;
use nom::bytes::complete::{tag, take_till, take_while};
use nom::character::is_alphabetic;
use nom::combinator::{iterator, map};
use nom::error::{ErrorKind, VerboseErrorKind};
use nom::multi::many1;
use nom::number::complete::{be_i16, be_u16, be_u8};
use nom::sequence::tuple;
use nom::{Offset, IResult, Err};

use z80emu::{Cpu, Prefix, Reg8, CpuFlags, InterruptMode};
// use crate::cpu_debug::print_debug_memory;
use crate::memory::{ZxMemory, SinglePageMemory};

pub struct AyFile {
    pub raw: Box<[u8]>,
    pub meta: AyMeta,
    pub songs: Box<[AySong]>,
    _pin: PhantomPinned,
}

#[derive(Debug)]
pub struct AyMeta {
    pub file_version:   u8,
    pub player_version: u8,
    pub special_player: bool,
    pub author:         AyString,
    pub misc:           AyString,
    pub first_song:     u8,
}

#[derive(Debug)]
pub struct AySong {
    pub name:        AyString,
    pub chan_a:      u8,
    pub chan_b:      u8,
    pub chan_c:      u8,
    pub noise:       u8,
    pub song_length: u16,
    pub fade_length: u16,
    pub hi_reg:      u8,
    pub lo_reg:      u8,
    pub stack:       u16,
    pub init:        u16,
    pub interrupt:   u16,
    pub blocks:      Box<[AySongData]>
}

#[derive(Debug)]
pub struct AySongData {
    pub address: u16,
    pub data: AyBlob,
}

pub struct AyBlob(NonNull<[u8]>);
pub struct AyString(NonNull<[u8]>);

impl fmt::Debug for AyBlob {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "AyBlob: {{ {} }}", self.len())
    }
}

impl fmt::Debug for AyString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "AyString: {:?}", self.to_str_lossy())
    }
}

impl fmt::Display for AyString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_str_lossy())
    }
}

impl fmt::Debug for AyFile {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "AyFile {{ raw: ({:?}), meta: {:?}, songs: {:?} }}", self.raw.len(), self.meta, self.songs)
    }
}



impl AyBlob {
    fn new(slice: &[u8]) -> Self {
        AyBlob(NonNull::from(slice))
    }

    pub fn as_slice(&self) -> &[u8] {
        unsafe { self.0.as_ref() } // creating of this type is private so it will be only made pinned
    }

    pub fn len(&self) -> usize {
        self.as_slice().len()
    }
}

impl AyString {
    fn new(slice: &[u8]) -> Self {
        AyString(NonNull::from(slice))
    }

    pub fn as_slice(&self) -> &[u8] {
        unsafe { self.0.as_ref() } // creating of this type is private so it will be only made pinned
    }

    pub fn byte_len(&self) -> usize {
        self.as_slice().len()
    }

    pub fn to_str(&self) -> Result<&str, core::str::Utf8Error> {
        core::str::from_utf8(self.as_slice()) // creating of this type is private so it will be only made pinned
    }

    pub fn to_str_lossy(&self) -> Cow<str> {
        String::from_utf8_lossy(self.as_slice()) // creating of this type is private so it will be only made pinned
    }

    pub fn to_string(&self) -> String {
        String::from_utf8_lossy(self.as_slice()).into_owned()
    }
}

static PLAYER_ONE: &[u8] = &[
/* 0000:*/ 0xF3,           /* di                  */
/* 0001:*/ 0xCD,0x00,0x00, /* call 0000H -> init  */
/* 0004:*/ 0xED,0x5E,      /* im   2     :loop1   */
/* 0006:*/ 0xFB,           /* ei                  */
/* 0007:*/ 0x76,           /* halt                */
/* 0008:*/ 0x18,0xFA,      /* jr   0004H -> loop1 */
];
static PLAYER_TWO: &[u8] = &[
/* 0000:*/ 0xF3,           /* di                      */
/* 0001:*/ 0xCD,0x00,0x00, /* call 0000H -> init      */
/* 0004:*/ 0xED,0x56,      /* im   1     :loop1       */
/* 0006:*/ 0xFB,           /* ei                      */
/* 0007:*/ 0x76,           /* halt                    */
/* 0008:*/ 0xCD,0x00,0x00, /* call 0000H -> interrupt */
// /* 000B:*/ 0x18,0xF7,      /* jr   0004H -> loop1     */
/* 000B:*/ 0x18,0xF7,      /* jr   0004H -> loop1     */
];
const PLAYER_INIT_OFFSET: usize = 2;
const PLAYER_TWO_INTERRUPT_OFFSET: usize = 9;

impl AyFile {
    pub fn initialize<C, M>(&self, cpu: &mut C, mem: &mut M, song_index: usize)
    where C: Cpu, M: ZxMemory + SinglePageMemory
    {
        if self.meta.special_player {
            panic!("can't initialize file with a special player");
        }
        let song = &self.songs[song_index];
        debug!("loading song: ({}) {}", song_index, song.name.to_str_lossy());
        let rawmem = mem.as_mut_slice();
        for p in rawmem[0x0000..0x0100].iter_mut() { *p = 0xC9 };
        for p in rawmem[0x0100..0x4000].iter_mut() { *p = 0xFF };
        for p in rawmem[0x4000..].iter_mut() { *p = 0x00 };
        rawmem[0x0038] = 0xFB;
        debug!("INIT: ${:x}", song.init);
        let init = if song.init == 0 {
            debug!("INIT using: ${:x}", song.blocks[0].address);
            song.blocks[0].address
        }
        else {
            song.init
        }.to_le_bytes();
        debug!("INTERRUPT: ${:x}", song.interrupt);
        let player = if song.interrupt == 0 {
            debug!("PLAYER ONE");
            PLAYER_ONE
        }
        else {
            debug!("PLAYER TWO");
            PLAYER_TWO
        };
        rawmem[0x0000..player.len()].copy_from_slice(&player);
        if song.interrupt != 0 {
            let intr = song.interrupt.to_le_bytes();
            rawmem[PLAYER_TWO_INTERRUPT_OFFSET..PLAYER_TWO_INTERRUPT_OFFSET+intr.len()].copy_from_slice(&intr);
        }
        rawmem[PLAYER_INIT_OFFSET..PLAYER_INIT_OFFSET+init.len()].copy_from_slice(&init);
        for block in song.blocks.iter() {
            debug!("Block: ${:x} <- {:?}", block.address, block.data);
            let address = block.address as usize;
            let mut data = block.data.as_slice();
            if address + data.len() > (u16::max_value() as usize) + 1 {
                debug!("Block too large: ${:x}", address + data.len());
                data = &data[..u16::max_value() as usize - address];
            }
            rawmem[address..address+data.len()].copy_from_slice(&data);
        }
        debug!("STACK: ${:x}", song.stack);
        // print_debug_memory(0x0000, &rawmem[0x0000..player.len()]);
        cpu.reset();
        cpu.set_i(self.meta.player_version);
        cpu.set_reg(Reg8::H, None, song.hi_reg);
        cpu.set_reg(Reg8::L, None, song.lo_reg);
        cpu.set_reg(Reg8::D, None, song.hi_reg);
        cpu.set_reg(Reg8::E, None, song.lo_reg);
        cpu.set_reg(Reg8::B, None, song.hi_reg);
        cpu.set_reg(Reg8::C, None, song.lo_reg);
        cpu.exx();
        cpu.set_acc(song.hi_reg);
        cpu.set_flags(CpuFlags::from_bits_truncate(song.lo_reg));
        cpu.ex_af_af();
        cpu.set_reg(Reg8::H, None, song.hi_reg);
        cpu.set_reg(Reg8::L, None, song.lo_reg);
        cpu.set_reg(Reg8::D, None, song.hi_reg);
        cpu.set_reg(Reg8::E, None, song.lo_reg);
        cpu.set_reg(Reg8::B, None, song.hi_reg);
        cpu.set_reg(Reg8::C, None, song.lo_reg);
        cpu.set_reg(Reg8::H, Some(Prefix::Yfd), song.hi_reg);
        cpu.set_reg(Reg8::L, Some(Prefix::Yfd), song.lo_reg);
        cpu.set_reg(Reg8::H, Some(Prefix::Xdd), song.hi_reg);
        cpu.set_reg(Reg8::L, Some(Prefix::Xdd), song.lo_reg);
        cpu.set_acc(song.hi_reg);
        cpu.set_flags(CpuFlags::from_bits_truncate(song.lo_reg));
        cpu.disable_interrupts();
        cpu.set_sp(song.stack);
        cpu.set_im(InterruptMode::Mode0);
        cpu.set_pc(0x0000);
    }
}

fn c_string(inp: &[u8]) -> IResult<&[u8], &[u8]> {
    match memchr(0, inp) {
        Some(offs) => Ok((&inp[offs..], &inp[..offs])),
        None => Err(Err::Error((inp, ErrorKind::Eof)))
    }
}

fn parse_ay_file(raw: &[u8]) -> IResult<&[u8], (AyMeta, Box<[AySong]>)> {
    // let raw = vec_in.into_boxed_slice();

    let ay_blob = |offset: usize, len: usize| {
        AyBlob::new(&raw[offset..offset+len])
    };

    let ay_string = |offset: Option<usize>| {
        offset.map(|offset|
            c_string(&raw[offset..]).map(|(_,s)| AyString::new(s))
        ).unwrap_or_else(|| Ok(AyString::new(b"")))
    };

    let maybe_offs_to = |inp| {
        let offset = isize::try_from(raw.offset(inp)).unwrap();
        let (inp, offs) = map(be_i16, NonZeroI16::new)(inp)?;
        let res = offs.map(|offs| usize::try_from(offset + offs.get() as isize).unwrap());
        Ok((inp, res))
    };

    let offset_to = |inp| {
        let (inp, offs) = maybe_offs_to(inp)?;
        match offs {
            None => Err(Err::Error((inp, ErrorKind::NonEmpty))),
            Some(offs) => Ok((inp, offs))
        }
    };

    let parse_address = |inp| -> IResult<_,_,_> {
        let (inp, address) = be_u16(inp)?;//.map_err(|e| nom::Err::Failure((inp, ErrorKind::Eof)))?;
        if address == 0 {
            Err(Err::Error((inp, ErrorKind::Complete)))
        }
        else {
            let (inp, (len, offset)) = tuple((be_u16, offset_to))(inp).map_err(|_| nom::Err::Failure((inp, ErrorKind::Eof)))?;
            Ok((inp, AySongData {
                address, data: ay_blob(offset, len.into())
            }))
        }
    };

    let parse_song_data = |data_offs| {
        map(many1(parse_address), |vec| vec.into_boxed_slice())(&raw[data_offs..]).map(|(_,s)| s)
    };

    let parse_song_info = |info_offs, name: AyString| {
        let (_, (chan_a, chan_b, chan_c, noise,
        song_length, fade_length,
        hi_reg, lo_reg,
        init_offs, data_offs)) = tuple(
            (be_u8, be_u8, be_u8, be_u8,
             be_u16, be_u16,
             be_u8, be_u8,
             offset_to, offset_to))( &raw[info_offs..] )?;
        let (_, (stack, init, interrupt)) = tuple((be_u16, be_u16, be_u16))( &raw[init_offs..] )?;
        let blocks = parse_song_data(data_offs)?;
        Ok(AySong {
            name, chan_a, chan_b, chan_c, noise,
            song_length, fade_length,
            hi_reg, lo_reg,
            stack, init, interrupt,
            blocks
        })
    };

    let parse_song = |inp| {
        let (inp, (name_offs, info_offs)) = tuple((maybe_offs_to, offset_to))(inp)?;
        let name = ay_string(name_offs)?;
        let song = parse_song_info(info_offs, name)?;
        Ok((inp, song))
    };

    let parse_songs = |songs_offs, num| {
        let mut it = iterator(&raw[songs_offs..], parse_song);
        let songs = it.take(num).collect::<Box<[_]>>();
        let (_,_) = it.finish()?;
        Ok(songs)
    };


    let (_, (
        _, _, file_version, player_version,
        special_player_offs, author_offs, misc_offs,
        num_of_songs, first_song,
        songs_offs)
    ) = tuple((tag(b"ZXAY"), tag(b"EMUL"), be_u8, be_u8,
             maybe_offs_to, maybe_offs_to, maybe_offs_to,
             be_u8, be_u8, 
             offset_to))( raw.as_ref() )?;
    let special_player = special_player_offs.is_some();
    let author = ay_string(author_offs)?;
    let misc = ay_string(misc_offs)?;
    let songs = parse_songs(songs_offs, num_of_songs as usize + 1)?;
    let meta = AyMeta {
        file_version, player_version,
        special_player, author, misc, first_song
    };

    Ok((&raw[raw.len()..], (meta, songs)))
}

pub fn parse_ay<B: Into<Box<[u8]>>>(data: B) -> Result<Pin<Box<AyFile>>, &'static str> {
    let raw: Box<_> = data.into();

    let (meta, songs) = match parse_ay_file(&raw) {
        Ok((_, ok)) => ok,
        Err(e) => {
            debug!("error parsing: {:?}", e);
            Err("error parsing ay file")?
        }
    };

    Ok(Box::pin(AyFile {
        raw, meta, songs, _pin: PhantomPinned }))
}


pub fn read_ay<R: std::io::Read>(mut rd: R) -> std::io::Result<Pin<Box<AyFile>>> {
    let mut data = Vec::new();
    rd.read_to_end(&mut data)?;
    parse_ay(data).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}
/*
struct AYFileHeader {
    // file_id:          [u8;4], // b"ZXAY"
    // type_id:          [u8;4], // b"EMUL"
    file_version:     u8,
    player_version:   u8,
    special_player_offs:  AYOffset, // i16
    author_offs:          AYOffset, // i16
    misc_offs:            AYOffset, // i16
    num_of_songs:     u8,
    first_song:       u8,
    songs_info_offs:  AYOffset<[Song]>, // i16
}

#[repr(packed)]
struct Song {
    song_name_offs:        AYOffset, // i16
    song_data_offs:        AYOffset<[SongInfo]>, // i16
}

#[repr(packed)]
struct SongInfo {
    chan_a: u8,
    chan_b: u8,
    chan_c: u8,
    noise: u8,
    song_length: AYWord, // u16
    fade_length: AYWord, // u16
    hi_reg: u8,
    lo_reg: u8,
    points_offs:    AYOffset<Pointers>, // i16,
    addresses_offs: AYOffset<[AYWord]>, // i16, // SongData
}

#[repr(packed)]
struct Pointers {
    stack: AYWord,
    init: AYWord,
    interrupt: AYWord
}

#[repr(packed)]
struct SongData {
    address: AYWord,
    length:  AYWord,
    offset:  AYOffset,
}
*/