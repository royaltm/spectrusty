//! **TAP** format related utilities for sweetening the handling of **TAP** files.
use core::convert::TryFrom;
use core::num::NonZeroU32;
use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::fs::{OpenOptions, File};
use std::io::{Read, Write, Result, Seek, SeekFrom};

use spectrusty_formats::tap::*;

#[derive(Clone, Copy, Debug, PartialEq)]
enum TapState {
    Idle,
    Playing,
    Recording
}

/// An enum with two variants, one for reading and the other for writing to the same [TAP] file.
///
/// The `F` can be anything that implements: [Read] + [Write] + [Seek].
///
/// [TAP]: spectrusty_formats::tap
pub enum Tap<F> {
    Reader(TapChunkPulseIter<F>),
    Writer(TapChunkWriter<F>)
}

/// The struct that emulates a simple tape recorder.
pub struct Tape<F> {
    /// `true` if the tape is playing, depending on the [Tap] variant it may indicate tape playback or recording.
    /// `false` then the tape has stopped.
    pub running: bool,
    /// `Some(tap)` indicates the tape casette is inserted, `None` it is ejected.
    pub tap: Option<Tap<F>>
}

/// A specific [TapCabinet] for workng directly with [fs::File]s.
pub type TapFileCabinet = TapCabinet<File, PathBuf>;

/// A slightly more advanced [Tap] organizer that allows handling an arbitrary number of emulated tape casettes.
///
/// The `F` can be anything that implements: [Read] + [Write] + [Seek].
/// `S` is the type of the tape metadata of any kind.
pub struct TapCabinet<F, S> {
    taps: Vec<(Tap<F>, S)>,
    current: usize,
    state: TapState
}

impl TapFileCabinet {
    pub fn add_file<P: AsRef<Path>>(&mut self, name: P) -> Result<()> {
        let file = OpenOptions::new().read(true).write(true).open(name.as_ref())?;
        self.add(file, name.as_ref().to_path_buf());
        Ok(())
    }

    pub fn create_file<P: AsRef<Path>>(&mut self, name: P) -> Result<()> {
        let file = OpenOptions::new().create_new(true).read(true).write(true).open(name.as_ref())?;
        self.taps.push((
            Tap::Writer(TapChunkWriter::try_new(file)?),
            name.as_ref().to_path_buf()
        ));
        Ok(())
    }

    pub fn current_tap_path(&self) -> Option<&Path> {
        self.current_tap_meta().map(|path| path.as_path())
    }

    pub fn current_tap_file_name(&self) -> Option<Cow<str>> {
        self.current_tap_meta().and_then(|path| 
            path.as_path().file_name()
        )
        .map(|n| n.to_string_lossy())
    }
}

impl<F, S> TapCabinet<F, S>
    where F: Write + Read + Seek
{
    pub fn new() -> Self {
        TapCabinet {
            taps: Vec::new(),
            current: 0,
            state: TapState::Idle
        }
    }

    pub fn add(&mut self, file: F, meta: S) {
        self.taps.push((
            Tap::Reader(TapChunkPulseIter::from(TapChunkReader::from(file))),
            meta
        ));
    }

    pub fn remove_current_tap(&mut self) -> Option<(F, S)> {
        if self.is_idle() && !self.taps.is_empty() {
            let res = match self.taps.remove(self.current) {
                (Tap::Reader(iter), meta) => {
                    Some((iter.into_inner().into_inner().into_inner(), meta))
                }
                (Tap::Writer(writer), meta) => {
                    Some((writer.into_inner().into_inner(), meta))
                }
            };
            self.current = self.current.min(self.taps.len().saturating_sub(1));
            return res
        }
        None
    }

    pub fn is_playing(&self) -> bool {
        TapState::Playing == self.state
    }

    pub fn is_recording(&self) -> bool {
        TapState::Recording == self.state
    }

    pub fn is_idle(&self) -> bool {
        TapState::Idle == self.state
    }

    pub fn ear_in_pulse_iter<'a>(&'a mut self) -> Option<impl Iterator<Item=NonZeroU32> + 'a> {
        if let TapState::Playing = self.state {
            if let Some((Tap::Reader(iter),_)) = self.taps.get_mut(self.current) {
                return Some(iter)
            }
        }
        None
    }

    pub fn mic_out_pulse_writer(&mut self) -> Option<&mut TapChunkWriter<impl Write + Seek>> {
        if let TapState::Recording = self.state {
            if let Some((Tap::Writer(writer),_)) = self.taps.get_mut(self.current) {
                return Some(writer)
            }
        }
        None
    }

    pub fn len(&self) -> usize {
        self.taps.len()
    }

    pub fn iter_meta(&self) -> impl Iterator<Item=&S> {
        self.taps.iter().map(|(_,m)| m)
    }

    pub fn current_tap_index(&self) -> usize {
        self.current
    }

    pub fn current_tap_meta(&self) -> Option<&S> {
        self.taps.get(self.current).map(|(_,meta)| meta)
    }

    pub fn current_tap_info(&mut self) -> Result<Option<TapReadInfoIter<TapChunkReaderMut<'_, F>>>> {
        self.current_tap_reader_mut().map(|res|
            res.map(|reader| TapReadInfoIter::from(reader) )
        )
    }

    pub fn current_tap_reader_mut(&mut self) -> Result<Option<TapChunkReaderMut<'_, F>>> {
        self.taps.get_mut(self.current).map(|(tap, _)| {
            tap.try_reader_mut()
        }).transpose()
    }

    pub fn rewind_tap(&mut self) -> Result<bool> {
        self.with_tap_reader(|iter| {
            iter.rewind();
            Ok(true)
        })
    }

    pub fn next_tap_chunk(&mut self) -> Result<Option<NonZeroU32>> {
        self.with_tap_reader(|iter| {
            iter.next_chunk()?;
            return Ok(NonZeroU32::new(iter.chunk_no()))
        })
    }

    pub fn prev_tap_chunk(&mut self) -> Result<Option<NonZeroU32>> {
        self.with_tap_reader(|iter| {
            if let Some(no) = NonZeroU32::new(iter.chunk_no()) {
                iter.rewind();
                if let Some(ntgt) = no.get().checked_sub(2) {
                    iter.skip_chunks(ntgt)?;
                }
            }
            return Ok(NonZeroU32::new(iter.chunk_no()))
        })
    }

    pub fn skip_tap_chunks(&mut self, skip: u32) -> Result<Option<NonZeroU32>> {
        self.with_tap_reader(|iter| {
            iter.skip_chunks(skip)?;
            return Ok(NonZeroU32::new(iter.chunk_no()))
        })
    }

    pub fn current_tap_chunk_no(&mut self) -> Result<Option<NonZeroU32>> {
        self.with_tap_reader(|iter| {
            Ok(NonZeroU32::new(iter.chunk_no()))
        })
    }

    pub fn select_first_tap(&mut self) -> bool {
        if self.is_idle() {
            self.current = 0;
            return true;
        }
        false
    }

    pub fn select_last_tap(&mut self) -> bool {
        if self.is_idle() {
            self.current = self.taps.len().saturating_sub(1);
            return true;
        }
        false
    }

    pub fn select_prev_tap(&mut self) -> bool {
        if self.is_idle() {
            self.current = self.current.saturating_sub(1);
            return true;
        }
        false
    }

    pub fn select_next_tap(&mut self) -> bool {
        if self.is_idle() {
            self.current = (self.current + 1).min(self.taps.len().saturating_sub(1));
            return true;
        }
        false
    }

    pub fn toggle_record(&mut self) -> Result<bool> {
        if !self.is_playing() {
            let taps = &mut self.taps;
            let current = self.current;
            match taps.get_mut(current) {
                Some((Tap::Reader(..), _)) => {
                    self.make_writer()?;
                    self.state = TapState::Recording;
                    return Ok(true)
                }
                Some((Tap::Writer(ref mut writer), _)) => {
                    self.state = match self.state {
                        TapState::Recording => {
                            writer.flush()?;
                            TapState::Idle
                        },
                        _ => TapState::Recording
                    };
                    return Ok(true)
                }
                None => {}
            }
        }
        Ok(false)
    }

    pub fn toggle_play(&mut self) -> Result<bool> {
        if !self.is_recording() {
            let taps = &mut self.taps;
            let current = self.current;
            match taps.get(current) {
                Some((Tap::Writer(..), _)) => {
                    self.make_reader()?;
                    self.state = TapState::Playing;
                    return Ok(true)
                }
                Some((Tap::Reader(..), _)) => {
                    self.state = match self.state {
                        TapState::Playing => TapState::Idle,
                        _ => TapState::Playing
                    };
                    return Ok(true)
                }
                None => {}
            }
        }
        Ok(false)
    }

    pub fn stop(&mut self) -> bool {
        match self.state {
            TapState::Idle => false,
            TapState::Playing|TapState::Recording => {
                self.state = TapState::Idle;
                true
            }
        }
    }

    fn with_tap_reader<'a, C, B>(&'a mut self, f: C) -> Result<B>
        where C: FnOnce(&'a mut TapChunkPulseIter<F>) -> Result<B>,
              B: Default
    {
        if !self.taps.is_empty() {
            if self.is_idle() {
                self.ensure_reader()?;
                if let (Tap::Reader(ref mut iter), _) = &mut self.taps[self.current] {
                    return f(iter)
                }
            }
        }
        Ok(B::default())
    }

    fn ensure_reader(&mut self) -> Result<()> {
        if let Some((Tap::Writer(..), _)) = self.taps.get(self.current) {
            self.make_reader()
        }
        else {
            Ok(())
        }
    }

    fn make_writer(&mut self) -> Result<()> {
        let taps = &mut self.taps;
        if let (reader @ Tap::Reader(..), meta) = taps.swap_remove(self.current) {
            let writer = reader.try_into_writer()?;
            taps.push((writer, meta));
            let last_index = taps.len() - 1;
            taps.swap(self.current, last_index);
        }
        else {
            unreachable!()
        }
        Ok(())
    }

    fn make_reader(&mut self) -> Result<()> {
        let taps = &mut self.taps;
        if let (writer @ Tap::Writer(..), meta) = taps.swap_remove(self.current) {
            let reader = writer.try_into_reader()?;
            taps.push((reader, meta));
            let last_index = taps.len() - 1;
            taps.swap(self.current, last_index);
        }
        else {
            unreachable!()
        }
        Ok(())
    }
}

impl<F> Default for Tape<F> {
    fn default() -> Self {
        Tape { running: false, tap: None }
    }
}

impl<F: Write + Read + Seek> Tap<F> {
    /// Returns a [Tap::Reader] variant.
    pub fn new_reader(file: F) -> Self {
        let reader = TapChunkReader::from(file);
        let pulse_iter = TapChunkPulseIter::from(reader);
        Tap::Reader(pulse_iter)
    }

    /// Returns a [Tap::Writer] variant on success.
    pub fn try_new_writer(file: F) -> Result<Self> {
        let writer = TapChunkWriter::try_new(file)?;
        Ok(Tap::Writer(writer))
    }

    /// Returns a mutable reference to the pulse iterator if the current variant of `self` is [Tap::Reader].
    pub fn reader_mut(&mut self) -> Option<&mut TapChunkPulseIter<F>> {
        match self {
            Tap::Reader(reader) => Some(reader),
            _ => None
        }
    }

    /// Returns a mutable reference to the tap chunk writer if the current variant of `self` is [Tap::Writer].
    pub fn writer_mut(&mut self) -> Option<&mut TapChunkWriter<F>> {
        match self {
            Tap::Writer(writer) => Some(writer),
            _ => None
        }
    }

    /// Returns a reference to the pulse iterator if the current variant of `self` is [Tap::Reader].
    pub fn reader_ref(&self) -> Option<&TapChunkPulseIter<F>> {
        match self {
            Tap::Reader(reader) => Some(reader),
            _ => None
        }
    }

    /// Returns a reference to the tap chunk writer if the current variant of `self` is [Tap::Writer].
    pub fn writer_ref(&self) -> Option<&TapChunkWriter<F>> {
        match self {
            Tap::Writer(writer) => Some(writer),
            _ => None
        }
    }

    /// Returns `true` if `self` is a [Tap::Reader].
    pub fn is_reader(&self) -> bool {
        match self {
            Tap::Reader(..) => true,
            _ => false
        }
    }

    /// Returns `true` if `self` is a [Tap::Writer].
    pub fn is_writer(&self) -> bool {
        match self {
            Tap::Writer(..) => true,
            _ => false
        }
    }

    /// Transforms the provided [Tap] into the [Tap::Reader] on success.
    ///
    /// The cursor position of the reader is set to the beginning of a file.
    ///
    /// If `self` is a [Tap::Writer] this method ensures that the chunk being currently written is
    /// comitted thus ensuring the integrity of the TAP file and also calls the [Writer::flush] on
    /// the file before transforming it.
    pub fn try_into_reader(self) -> Result<Self> {
        let mut file = self.try_into_file()?;
        file.seek(SeekFrom::Start(0))?;
        Ok(Self::new_reader(file))
    }

    /// Transforms the provided [Tap] into the [Tap::Writer] on success.
    ///
    /// The cursor position of the writer is set to the end of a file.
    ///
    /// If `self` is already a [Tap::Writer] this method ensures that the chunk being currently written
    /// is comitted thus ensuring the integrity of the TAP file and also calls the [Writer::flush] on
    /// the file before transforming it.
    pub fn try_into_writer(self) -> Result<Self> {
        let mut file = self.try_into_file()?;
        file.seek(SeekFrom::End(0))?;
        Self::try_new_writer(file)
    }

    /// Returns the unwrapped file on success.
    ///
    /// If `self` is a [Tap::Writer] this method ensures that the chunk being currently written is
    /// comitted thus ensuring the integrity of the TAP file and also calls the [Writer::flush] on
    /// the file before returning it.
    pub fn try_into_file(self) -> Result<F> {
        Ok(match self {
            Tap::Reader(reader) => {
                let file: F = reader.into_inner().into_inner().into_inner();
                file
            },
            Tap::Writer(mut writer) => {
                writer.end_pulse_chunk()?;
                writer.flush()?;
                let file: F = writer.into_inner().into_inner();
                file
            }
        })
    }

    /// Returns a clone of [TapChunkReader] with a mutable reference to the file
    /// under the guard that ensures the position of the underlying file is set back
    /// to where it was before this method was called when the guard goes out of scope.
    pub fn try_reader_mut(&mut self) -> Result<TapChunkReaderMut<'_, F>> {
        match self {
            Tap::Reader(reader) => reader.as_mut().try_clone_mut(),
            Tap::Writer(writer) => {
                let file_mut = writer.get_mut().get_mut();
                let reader = TapChunkReader::from(file_mut);
                TapChunkReaderMut::try_from(reader)
            }
        }
    }

    /// Conditionally rewinds a tape if its variant is [Tap::Reader]. In this instance returns `true`.
    /// Otherwise returns `false`.
    pub fn rewind(&mut self) -> bool {
        if let Some(reader) = self.reader_mut() {
            reader.rewind();
            true
        }
        else {
            false
        }
    }

    /// Conditionally forwards a tape to the next chunk if its variant is [Tap::Reader]. In this
    /// instance returns `Ok(Some(was_next_chunk))`. Otherwise returns `Ok(None)`.
    pub fn forward_chunk(&mut self) -> Result<Option<bool>> {
        self.reader_mut().map(|rd| rd.forward_chunk()).transpose()
    }

    /// Conditionally rewinds a tape to the previous chunk if its variant is [Tap::Reader]. In this
    /// instance returns `Ok(Some(chunk_no))`. Otherwise returns `Ok(None)`.
    pub fn rewind_prev_chunk(&mut self) -> Result<Option<u32>> {
        self.reader_mut().map(|rd| rd.rewind_prev_chunk()).transpose()
    }

    /// Conditionally rewinds a tape to the beginning of the current chunk if its variant is
    /// [Tap::Reader]. In this instance returns `Ok(Some(chunk_no))`. Otherwise returns `Ok(None)`.
    pub fn rewind_chunk(&mut self) -> Result<Option<u32>> {
        self.reader_mut().map(|rd| rd.rewind_chunk()).transpose()
    }

    /// Conditionally rewinds or forwards a tape to the nth chunk if its variant is [Tap::Reader].
    /// In this instance returns `Ok(Some(was_a_chunk))`. Otherwise returns `Ok(None)`.
    pub fn rewind_nth_chunk(&mut self, chunk_no: u32) -> Result<Option<bool>> {
        self.reader_mut().map(|rd| rd.rewind_nth_chunk(chunk_no)).transpose()
    }
}

impl<F: Write + Read + Seek> Tape<F> {
    /// Returns a new instance of [Tape] with the tape file inserted.
    pub fn new_with_tape(file: F) -> Self {
        let tap = Tap::new_reader(file);
        Tape { running: false, tap: Some(tap) }
    }

    /// Inserts the tape file as a [Tap::Reader].
    /// Returns the previously inserted [Tap] instance.
    pub fn insert_as_reader(&mut self, file: F) -> Option<Tap<F>> {
        let tap = Tap::new_reader(file);
        self.tap.replace(tap)
    }

    /// Tries to insert the tape file as a [Tap::Writer].
    /// Returns the previously inserted [Tap] instance.
    pub fn try_insert_as_writer(&mut self, file: F) -> Result<Option<Tap<F>>> {
        let tap = Tap::try_new_writer(file)?;
        Ok(self.tap.replace(tap))
    }

    /// Ejects and returns the previously inserted [Tap] instance.
    pub fn eject(&mut self) -> Option<Tap<F>> {
        self.running = false;
        self.tap.take()
    }

    /// Transforms the inserted [Tap] into the [Tap::Reader] on success.
    ///
    /// Returns `Ok(true)` if the inserted [Tap] was a [Tap::Writer]. In this instance the cursor
    /// position of the reader is set to the beginning of a file and this method ensures that
    /// the chunk being currently written is comitted thus ensuring the integrity of the TAP
    /// file and also calls the [Writer::flush] on the file before transforming it.
    ///
    /// If the inserted [Tap] is already a [Tap::Reader] or if there is no [Tap] inserted returns
    /// `Ok(false)`.
    pub fn make_reader(&mut self) -> Result<bool> {
        if let Some(tap) = self.tap.as_ref() {
            if tap.is_writer() {
                let tap = self.tap.take().unwrap().try_into_reader()?;
                self.tap = Some(tap);
                return Ok(true)
            }
        }
        Ok(false)
    }

    /// Transforms the inserted [Tap] into the [Tap::Writer] on success.
    ///
    /// Returns `Ok(true)` if the inserted [Tap] was a [Tap::Reader]. In this instance the cursor
    /// position of the reader is set to the end of a file.
    ///
    /// If the inserted [Tap] is already a [Tap::Writer] or if there is no [Tap] inserted returns
    /// `Ok(false)`.
    pub fn make_writer(&mut self) -> Result<bool> {
        if let Some(tap) = self.tap.as_ref() {
            if tap.is_reader() {
                let tap = self.tap.take().unwrap().try_into_writer()?;
                self.tap = Some(tap);
                return Ok(true)
            }
        }
        Ok(false)
    }

    /// Returns `true` if there is no [Tap] inserted, otherwise returns `false`.
    pub fn is_ejected(&self) -> bool {
        self.tap.is_none()
    }

    /// Returns `true` if there is a [Tap] inserted, otherwise returns `false`.
    pub fn is_inserted(&self) -> bool {
        self.tap.is_some()
    }

    /// Returns `true` if there is some [Tap] inserted and [Tape::running] is `true`, otherwise returns `false`.
    pub fn is_running(&self) -> bool {
        self.running && self.tap.is_some()
    }

    /// Returns `true` if there is a [Tap::Reader] variant inserted and [Tape::running] is `true`,
    /// otherwise returns `false`.
    pub fn is_playing(&self) -> bool {
        if self.running {
            if let Some(tap) = self.tap.as_ref() {
                return tap.is_reader()
            }
        }
        false
    }

    /// Returns `true` if there is a [Tap::Writer] variant inserted and [Tape::running] is `true`,
    /// otherwise returns `false`.
    pub fn is_recording(&self) -> bool {
        if self.running {
            if let Some(tap) = self.tap.as_ref() {
                return tap.is_writer()
            }
        }
        false
    }

    /// Returns a mutable reference to the pulse iterator if the current variant of [Tape::tap] is [Tap::Reader].
    pub fn reader_mut(&mut self) -> Option<&mut TapChunkPulseIter<F>> {
        self.tap.as_mut().and_then(|tap| tap.reader_mut())
    }

    /// Returns a mutable reference to the tap chunk writer if the current variant of [Tape::tap] is [Tap::Writer].
    pub fn writer_mut(&mut self) -> Option<&mut TapChunkWriter<F>> {
        self.tap.as_mut().and_then(|tap| tap.writer_mut())
    }

    /// Returns a reference to the pulse iterator if the current variant of [Tape::tap] is [Tap::Reader].
    pub fn reader_ref(&self) -> Option<&TapChunkPulseIter<F>> {
        self.tap.as_ref().and_then(|tap| tap.reader_ref())
    }

    /// Returns a reference to the tap chunk writer if the current variant of [Tape::tap] is [Tap::Writer].
    pub fn writer_ref(&self) -> Option<&TapChunkWriter<F>> {
        self.tap.as_ref().and_then(|tap| tap.writer_ref())
    }

    /// Returns a mutable reference to the pulse iterator if there is a [Tap::Reader] variant inserted
    /// and [Tape::running] is `true`, otherwise returns `None`.
    pub fn playing_tap(&mut self) -> Option<&mut TapChunkPulseIter<F>> {
        if self.running {
            return self.tap.as_mut().and_then(|tap| tap.reader_mut())
        }
        None
    }

    /// Returns a mutable reference to the tap chunk writer if there is a [Tap::Writer] variant inserted
    /// and [Tape::running] is `true`, otherwise returns `None`.
    pub fn recording_tap(&mut self) -> Option<&mut TapChunkWriter<F>> {
        if self.running {
            return self.tap.as_mut().and_then(|tap| tap.writer_mut())
        }
        None
    }

    /// Returns a clone of [TapChunkReader] with a mutable reference to the file
    /// under the guard that ensures the position of the underlying file is set back
    /// to where it was before this method was called when the guard goes out of scope.
    pub fn try_reader_mut(&mut self) -> Result<Option<TapChunkReaderMut<'_, F>>> {
        self.tap.as_mut().map(|tap| tap.try_reader_mut()).transpose()
    }

    /// Sets [Tape::running] to `true` and ensures the inserted variant is a [Tap::Reader].
    ///
    /// Returns `Ok(true)` if the state of `self` changes.
    pub fn play(&mut self) -> Result<bool> {
        let running = self.running;
        self.running = true;
        Ok(self.make_reader()? || !running)
    }

    /// Sets [Tape::running] to `true` and ensures the inserted variant is a [Tap::Writer].
    ///
    /// Returns `Ok(true)` if the state of `self` changes.
    pub fn record(&mut self) -> Result<bool> {
        let running = self.running;
        self.running = true;
        Ok(self.make_writer()? || !running)
    }

    /// Sets [Tape::running] to `false`.
    pub fn stop(&mut self) {
        self.running = false;
    }

    /// Conditionally rewinds a tape if it's inserted and its variant is [Tap::Reader].
    /// In this instance returns `true`. Otherwise returns `false`.
    pub fn rewind(&mut self) -> bool {
        self.tap.as_mut().map(|rd| rd.rewind()).filter(|b| *b).is_some()
    }

    /// Conditionally forwards a tape to the next chunk if it's inserted and its variant
    /// is [Tap::Reader]. In this instance returns `Ok(Some(was_next_chunk))`. Otherwise returns
    /// `Ok(None)`.
    pub fn forward_chunk(&mut self) -> Result<Option<bool>> {
        self.reader_mut().map(|rd| rd.forward_chunk()).transpose()
    }

    /// Conditionally rewinds a tape to the previous chunk if it's inserted and its variant
    /// is [Tap::Reader]. In this instance returns `Ok(Some(chunk_no))`. Otherwise returns `Ok(None)`.
    pub fn rewind_prev_chunk(&mut self) -> Result<Option<u32>> {
        self.reader_mut().map(|rd| rd.rewind_prev_chunk()).transpose()
    }

    /// Conditionally rewinds a tape to the beginning of the current chunk if it's inserted and its
    /// variant is [Tap::Reader]. In this instance returns `Ok(Some(chunk_no))`. Otherwise returns
    /// `Ok(None)`.
    pub fn rewind_chunk(&mut self) -> Result<Option<u32>> {
        self.reader_mut().map(|rd| rd.rewind_chunk()).transpose()
    }

    /// Conditionally rewinds or forwards a tape to the nth chunk if it's inserted and its
    /// variant is [Tap::Reader]. In this instance returns `Ok(Some(was_a_chunk))`. Otherwise
    /// returns `Ok(None)`.
    pub fn rewind_nth_chunk(&mut self, chunk_no: u32) -> Result<Option<bool>> {
        self.reader_mut().map(|rd| rd.rewind_nth_chunk(chunk_no)).transpose()
    }
}
