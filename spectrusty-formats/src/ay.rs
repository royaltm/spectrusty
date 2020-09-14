/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
//! **AY** file format parser and player initializer. See: [ProjectAY](https://www.worldofspectrum.org/projectay/).
use core::fmt::{self, Write};
use core::num::NonZeroU16;
use core::ops::Deref;
use core::convert::{TryFrom, AsRef};
use core::ptr::NonNull;
use core::marker::PhantomPinned;
use std::borrow::Cow;
use std::pin::Pin;
use std::io;

#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

use memchr::memchr;
use nom::bytes::complete::tag;
use nom::combinator::{iterator, map};
use nom::error::{ErrorKind, context, ParseError};
use nom::multi::many1;
use nom::number::complete::{be_u16, be_u8};
use nom::sequence::tuple;
use nom::{Offset, IResult, Err};

use spectrusty_core::z80emu::{Cpu, Prefix, Reg8, CpuFlags, InterruptMode};
// use crate::cpu_debug::print_debug_memory;
use spectrusty_core::memory::ZxMemory;

/// This is the main type produced by methods of this module.
pub type PinAyFile = Pin<Box<AyFile>>;

/// This type is being ever created only as [PinAyFile] to prevent destructing and moving data out.
///
/// The `AyFile` struct uses pointers referencing self owned data under the hood.
/// Users may inspect its content safely and initialize player with it.
/// See: [AyFile::initialize_player].
///
/// This struct can't be used to construct *AY* files.
pub struct AyFile {
    /// The original parsed data owned by this struct.
    pub raw: Box<[u8]>,
    /// *AY* file meta-data.
    pub meta: AyMeta,
    /// The list of *AY* songs.
    pub songs: Box<[AySong]>,
    _pin: PhantomPinned,
}

/// The type of a parsed *AY* file meta-data.
#[derive(Debug)]
pub struct AyMeta {
    /// *AY* file version.
    pub file_version:   u8,
    /// Player minimum version required.
    pub player_version: u8,
    /// Does the *AY* file require a special player written in MC68k code?
    pub special_player: bool,
    /// The *AY* file author's name.
    pub author:         AyString,
    /// Miscellaneous text.
    pub misc:           AyString,
    /// Index of the first song.
    pub first_song:     u8,
}

/// The type of a parsed *AY* file's song.
#[derive(Debug)]
pub struct AySong {
    /// The name of the song.
    pub name:        AyString,
    /// Mapping of AY-3-891x channel A to Amiga channel.
    pub chan_a:      u8,
    /// Mapping of AY-3-891x channel B to Amiga channel.
    pub chan_b:      u8,
    /// Mapping of AY-3-891x channel C to Amiga channel.
    pub chan_c:      u8,
    /// Mapping of AY-3-891x noise to Amiga channel.
    pub noise:       u8,
    /// The duration of the song in frames.
    pub song_duration: u16,
    /// The fade duration of the song in frames.
    pub fade_duration: u16,
    /// The content loaded into the `Z80` registers: `A,B,D,H,IXh,IYh`.
    pub hi_reg:      u8,
    /// The content loaded into the `Z80` registers: `F,C,E,L,IXl,IYl`.
    pub lo_reg:      u8,
    /// The content loaded into the `Z80` `SP` register.
    pub stack:       u16,
    /// The address in 64kb memory of the initialization routine.
    pub init:        u16,
    /// The address in 64kb memory of the interrupt routine.
    pub interrupt:   u16,
    /// The list of song's memory blocks.
    pub blocks:      Box<[AySongBlock]>
}

/// The type of a parsed *AY* song's memory block.
#[derive(Debug)]
pub struct AySongBlock {
    /// A target 64kb memory address of this block.
    pub address: u16,
    /// A block of data.
    pub data: AyBlob,
}

/// The type of a parsed *AY* song's blocks of data.
pub struct AyBlob(NonNull<[u8]>);
/// The type of a parsed *AY* file's strings.
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

impl AsRef<[u8]> for AyBlob {
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl AsRef<[u8]> for AyString {
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl Deref for AyBlob {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl AyBlob {
    fn new(slice: &[u8]) -> Self {
        AyBlob(NonNull::from(slice))
    }

    /// Returns a reference to data.
    pub fn as_slice(&self) -> &[u8] {
        unsafe { self.0.as_ref() } // creating of this type is private so it will be only made pinned
    }
}

impl AyString {
    fn new(slice: &[u8]) -> Self {
        AyString(NonNull::from(slice))
    }

    /// Returns a reference to the string as array of bytes.
    pub fn as_slice(&self) -> &[u8] {
        unsafe { self.0.as_ref() } // creating of this type is private so it will be only made pinned
    }

    /// Returns a string reference if the underlying bytes can form a proper UTF-8 string.
    pub fn to_str(&self) -> Result<&str, core::str::Utf8Error> {
        core::str::from_utf8(self.as_slice())
    }

    /// Returns a `str` reference if the underlying bytes can form a proper UTF-8 string.
    /// Returns a string, including invalid characters.
    pub fn to_str_lossy(&self) -> Cow<str> {
        String::from_utf8_lossy(self.as_slice())
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
/* 000B:*/ 0x18,0xF7,      /* jr   0004H -> loop1     */
];
const PLAYER_INIT_OFFSET: usize = 2;
const PLAYER_TWO_INTERRUPT_OFFSET: usize = 9;

impl AyFile {
    /// Initializes memory and cpu registers, creates a player routine and loads song data into memory.
    /// Provide `song_index` of the desired song from this file to be played.
    ///
    /// # Panics
    /// * If `song_index` is larger or equal to the number of contained songs.
    ///   You may get the number of songs by invoking `.songs.len()` method.
    /// * If special player is required. See [AyMeta].
    /// * If a capacity of provided memory is less than 64kb.
    pub fn initialize_player<C, M>(&self, cpu: &mut C, mem: &mut M, song_index: usize)
    where C: Cpu, M: ZxMemory
    {
        if self.meta.special_player {
            panic!("can't initialize file with a special player");
        }
        let song = &self.songs[song_index];
        debug!("loading song: ({}) {}", song_index, song.name.to_str_lossy());
        let rawmem = mem.mem_mut();
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
        // debug_memory(0x0000, &rawmem[0x0000..player.len()]);
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

/****************************************************************************/
/*                        OM NOM NOM NOM NOM NOM NOM                        */
/****************************************************************************/
#[derive(Clone, Debug, PartialEq)]
struct VerboseBytesError<'a> {
  errors: Vec<(&'a [u8], VerboseBytesErrorKind)>,
}

trait UnwrapErr<E> {
    fn unwrap(self) -> E;
}

impl<E> UnwrapErr<E> for Err<E> {
    fn unwrap(self) -> E {
        match self {
            Err::Failure(e)|Err::Error(e) => e,
            Err::Incomplete(..) => panic!("can't unwrap an incomplete error")
        }
    }
}

impl<'a> VerboseBytesError<'a> {
    fn describe(&self, input: &'a[u8]) -> String {
        let mut res = String::new();
        for (i, (subs, kind)) in self.errors.iter().enumerate() {
            let offset = input.offset(subs);
            match kind {
                VerboseBytesErrorKind::Context(s) => writeln!(&mut res,
                    "{i}: at byte {offset} of {len}, {context}",
                    i = i,
                    context = s,
                    len = input.len(),
                    offset = offset
                ),
                VerboseBytesErrorKind::Nom(e) => writeln!(&mut res,
                    "{i}: at byte {offset} of {len}, in {nom_err:?}",
                    i = i,
                    len = input.len(),
                    offset = offset,
                    nom_err = e
                ),
            }.unwrap()
        }
        res
    }
}

#[derive(Clone, Debug, PartialEq)]
/// error context for `VerboseBytesError`
enum VerboseBytesErrorKind {
  /// static string added by the `context` function
  Context(&'static str),
  /// error kind given by various nom parsers
  Nom(ErrorKind),
}

impl<'a> ParseError<&'a[u8]> for VerboseBytesError<'a> {
  fn from_error_kind(input: &'a[u8], kind: ErrorKind) -> Self {
    VerboseBytesError {
      errors: vec![(input, VerboseBytesErrorKind::Nom(kind))],
    }
  }

  fn append(input: &'a[u8], kind: ErrorKind, mut other: Self) -> Self {
    other.errors.push((input, VerboseBytesErrorKind::Nom(kind)));
    other
  }

  fn add_context(input: &'a[u8], ctx: &'static str, mut other: Self) -> Self {
    other.errors.push((input, VerboseBytesErrorKind::Context(ctx)));
    other
  }
}

fn c_string<'a, E: ParseError<&'a [u8]>>()
    -> impl Fn(&'a[u8]) -> IResult<&'a[u8], &'a[u8], E>
{
    move |input| {
        match memchr(0, input) {
            Some(offs) => Ok((&input[offs..], &input[..offs])),
            None => Err(Err::Error(E::from_error_kind(input, ErrorKind::Eof)))
        }
    }
}

fn parse_at<'a, T: 'a, E: ParseError<&'a[u8]>, F>(
        offset: usize,
        f: F
    ) -> impl FnOnce(&'a[u8]) -> Result<T, Err<E>>
    where F: FnOnce(&'a[u8])->Result<T, Err<E>>
{
    move |input| {
        input.get(offset..).ok_or_else(|| Err::Failure(
                    E::from_error_kind(input, ErrorKind::Eof)
        )).and_then(f)
    }
}

fn fail_err<I: Clone, O, E: ParseError<I>, F>(
        f: F
    ) -> impl Fn(I) -> IResult<I, O, E> 
    where F: Fn(I) -> IResult<I, O, E>
{
    move |input: I| {
        f(input).map_err(|e| e.into_failure())
    }
}

trait IntoFailure<E> {
    fn into_failure(self) -> Self;
}

impl<E> IntoFailure<E> for Err<E> {
    fn into_failure(self) -> Self {
        if let Err::Error(e) = self {
            Err::Failure(e)
        }
        else {
            self
        }
    }
}

fn parse_ay_raw<'a, E: Clone + ParseError<&'a [u8]>>(
        raw: &'a [u8]
    ) -> Result<(AyMeta, Box<[AySong]>), Err<E>>
{

    let ay_blob = |offset: usize, len: usize| {
        if offset+len > raw.len() {
            debug!("truncating data off: {} len: {} end: {} > {}", offset, len, offset+len, raw.len());
            raw.get(offset..)
        }
        else {
            raw.get(offset..offset+len)
        }.map(|blob| {
            AyBlob::new(&blob)
        }).ok_or_else(|| {
            let (inp, context) = (&raw[raw.len()..], "data offset exceeding file size");
            Err::Failure(E::add_context(inp, context,
                E::from_error_kind(inp, ErrorKind::Eof)
            ))
        })
    };

    let ay_string = |ctx, offset: Option<usize>| {
        offset.map(|offset|
            parse_at(offset,  context(ctx, c_string()))( raw )
        ).unwrap_or_else(||
            Ok( (raw, b"") )
        ).map(|(_,s)| AyString::new(s))
    };

    let maybe_offs_to = |inp| {
        let offset = isize::try_from(raw.offset(inp)).unwrap();
        let (inp, offs) = map(be_u16, NonZeroU16::new)(inp)?;
        let res = offs.map(|offs| {
            usize::try_from(offset + offs.get() as isize).unwrap()
        });
        Ok((inp, res))
    };

    let offset_to = |inp| {
        let (inp, offs) = maybe_offs_to(inp)?;
        match offs {
            None => Err(Err::Error(E::add_context(inp, "offset",
                        E::from_error_kind(inp, ErrorKind::NonEmpty)
                    ))),
            Some(offs) => Ok((inp, offs))
        }
    };

    let parse_address = |inp| {
        let (inp, address) = context("address", fail_err(be_u16))(inp)?;
        if address == 0 {
            Err(Err::Error(E::from_error_kind(inp, ErrorKind::Complete)))
        }
        else {
            let (inp, (len, offset)) = fail_err(tuple(
                (context("data length", be_u16), offset_to)
            ))( inp )?;
            Ok((inp, AySongBlock {
                address, data: ay_blob(offset, len.into())?
            }))
        }
    };

    let parse_song_data = |data_offs| {
        parse_at(data_offs, 
            context("song data", 
                map(many1(parse_address), |vec| vec.into_boxed_slice()))
        )( raw ).map(|(_,s)| s)
    };

    let parse_song_info = |info_offs, name: AyString| {
        let (_, (chan_a, chan_b, chan_c, noise,
        song_duration, fade_duration,
        hi_reg, lo_reg,
        init_offs, data_offs)) = parse_at(info_offs, context("missing song metadata", tuple(
            (be_u8, be_u8, be_u8, be_u8,
             be_u16, be_u16,
             be_u8, be_u8,
             offset_to, offset_to))
        ))( raw )?;

        let (_, (stack, init, interrupt)) = parse_at(init_offs, context("play routines",
            tuple((context("stack", be_u16), be_u16, be_u16)))
        )( raw )?;
        let blocks = parse_song_data(data_offs)?;
        Ok(AySong {
            name, chan_a, chan_b, chan_c, noise,
            song_duration, fade_duration,
            hi_reg, lo_reg,
            stack, init, interrupt,
            blocks
        })
    };

    let parse_song = |inp| {
        let (inp, (name_offs, info_offs)) = tuple((maybe_offs_to, offset_to))(inp)?;
        let name = ay_string("song name", name_offs)?;
        let song = parse_song_info(info_offs, name)?;
        Ok((inp, song))
    };

    let parse_songs = |songs_offs, num| {
        parse_at(songs_offs, |inp| {
            let mut it = iterator(inp, parse_song);
            let songs = it.take(num).collect::<Box<[_]>>();
            let (_,_) = it.finish()?;
            Ok(songs)
        })( raw )
    };


    let (_, (
        _tag1, _tag2, file_version, player_version,
        special_player_offs, author_offs, misc_offs,
        num_of_songs, first_song,
        songs_offs)
    ) = context("missing header parts",
            tuple((
                context("tag ZXAY", tag(b"ZXAY")),
                context("tag ZXAY", tag(b"EMUL")),
                context("file version", be_u8),
                context("player version", be_u8),
                context("spec. player", maybe_offs_to),
                context("author offs.", maybe_offs_to),
                context("misc. offs.", maybe_offs_to),
                context("num. of songs", be_u8),
                context("firs song index", be_u8),
                context("songs offs.", offset_to)
            ))
    )( raw )?;
    let special_player = special_player_offs.is_some();
    let author = ay_string("author", author_offs)?;
    let misc = ay_string("misc", misc_offs)?;
    let songs = parse_songs(songs_offs, num_of_songs as usize + 1)?;
    let meta = AyMeta {
        file_version, player_version,
        special_player, author, misc, first_song
    };

    Ok((meta, songs))
}

/// The type of the error returned by the *AY* file parser.
#[derive(Clone, Debug, PartialEq)]
pub struct AyParseError {
    /// Data parsed.
    pub data: Box<[u8]>,
    /// *AY* file parser backtrace and error messages.
    pub description: String,
}

impl std::error::Error for AyParseError {}
impl fmt::Display for AyParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.description)
    }
}

impl From<(Box<[u8]>, String)> for AyParseError {
    fn from((data, description): (Box<[u8]>, String)) -> Self {
        AyParseError { data, description }
    }
}


/// Parses given `data` and returns a pinned [AyFile] owning `data`.
/// On error returns the `data` wrapped in [AyParseError].
pub fn parse_ay<B: Into<Box<[u8]>>>(
        data: B
    ) -> Result<PinAyFile, AyParseError>
{
    let raw: Box<_> = data.into();
    let res = parse_ay_raw::<VerboseBytesError>(&raw)
                        .map_err(|e|
                            e.unwrap().describe(&raw)
                        );

    let (meta, songs) = match res {
        Ok(ok) => ok,
        Err(err) => return Err(AyParseError::from((raw, err)))
    };

    Ok(Box::pin(AyFile {
        raw, meta, songs,
        _pin: PhantomPinned 
    }))
}

/// Reads data from `rd`, parses data and returns a pinned [AyFile] owning `data` on success.
/// When there was a parse error returns `Err` with [AyParseError] wrapped in [io::Error]
/// with [io::ErrorKind::InvalidData]. To get to the inner [AyParseError] you need either
/// to downcast it yourself or use one of convenient [IoErrorExt] methods.
pub fn read_ay<R: io::Read>(
        mut rd: R
    ) -> io::Result<PinAyFile>
{
    let mut data = Vec::new();
    rd.read_to_end(&mut data)?;
    parse_ay(data).map_err(|e|
        io::Error::new(io::ErrorKind::InvalidData, e)
    )
}

/// A trait with helpers for extracting [AyParseError] from [io::Error].
pub trait IoErrorExt: Sized {
    fn is_ay_parse(&self) -> bool {
        self.ay_parse_ref().is_some()
    }
    fn into_ay_parse(self) -> Option<Box<AyParseError>>;
    fn ay_parse_ref(&self) -> Option<&AyParseError>;
    fn ay_parse_mut(&mut self) -> Option<&mut AyParseError>;
    fn into_ay_parse_data(self) -> Option<Box<[u8]>> {
        self.into_ay_parse().map(|ay| ay.data)
    }
}

impl IoErrorExt for io::Error {
    fn into_ay_parse(self) -> Option<Box<AyParseError>> {
        if let Some(inner) = self.into_inner() {
            if let Ok(ay_err) = inner.downcast::<AyParseError>() {
                return Some(ay_err)
            }
        }
        None
    }
    fn ay_parse_ref(&self) -> Option<&AyParseError> {
        if let Some(inner) = self.get_ref() {
            if let Some(ay_err) = inner.downcast_ref::<AyParseError>() {
                return Some(ay_err)
            }
        }
        None
    }
    fn ay_parse_mut(&mut self) -> Option<&mut AyParseError> {
        if let Some(inner) = self.get_mut() {
            if let Some(ay_err) = inner.downcast_mut::<AyParseError>() {
                return Some(ay_err)
            }
        }
        None
    }
}
/*
Kudos to Sergey Bulba.

There is but one bug in Bulba's specification, all offsets are U16 not I16.
E.g. ProjectAY/Spectrum/Demos/SMC1.AY

struct AYFileHeader {
    // file_id:          [u8;4], // b"ZXAY"
    // type_id:          [u8;4], // b"EMUL"
    file_version:     u8,
    player_version:   u8,
    special_player_offs:  AYOffset, // u16
    author_offs:          AYOffset, // u16
    misc_offs:            AYOffset, // u16
    num_of_songs:     u8,
    first_song:       u8,
    songs_info_offs:  AYOffset<[Song]>, // u16
}

struct Song {
    song_name_offs:        AYOffset, // u16
    song_data_offs:        AYOffset<[SongInfo]>, // u16
}

struct SongInfo {
    chan_a: u8,
    chan_b: u8,
    chan_c: u8,
    noise: u8,
    song_duration: AYWord, // u16
    fade_duration: AYWord, // u16
    hi_reg: u8,
    lo_reg: u8,
    points_offs:    AYOffset<Pointers>, // u16,
    addresses_offs: AYOffset<[AYWord]>, // u16, // SongData
}

struct Pointers {
    stack: AYWord,
    init: AYWord,
    interrupt: AYWord
}

struct SongData {
    address: AYWord,
    length:  AYWord,
    offset:  AYOffset,
}
*/
