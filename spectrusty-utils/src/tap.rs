/*
    Copyright (C) 2020-2022  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
//! **TAP** format related utilities for sweetening the handling of **TAP** files.
use core::fmt;
use core::convert::TryFrom;
use std::io::{Read, Write, Result, Seek, SeekFrom};

use spectrusty::formats::tap::*;

pub mod romload;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TapState {
    Idle,
    Playing,
    Recording
}

/// An enum with two variants, one for reading and the other for writing to the same [TAP] file.
///
/// The `F` can be anything that implements: [Read] + [Write] + [Seek].
///
/// [TAP]: spectrusty::formats::tap
pub enum Tap<F> {
    Reader(TapChunkPulseIter<F>),
    Writer(TapChunkWriter<F>)
}

/// The struct that emulates a simple tape recorder.
#[derive(Debug)]
pub struct Tape<F> {
    /// `true` if the tape is playing, depending on the [Tap] variant it may indicate tape playback or recording.
    /// `false` then the tape has stopped.
    pub running: bool,
    /// `Some(tap)` indicates the tape cassette is inserted, `None` - there is no tape.
    pub tap: Option<Tap<F>>
}

impl<F> fmt::Debug for Tap<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Tap::Reader(..) => "Tap::Reader(..)".fmt(f),
            Tap::Writer(..) => "Tap::Writer(..)".fmt(f)
        }
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
        matches!(self, Tap::Reader(..))
    }

    /// Returns `true` if `self` is a [Tap::Writer].
    pub fn is_writer(&self) -> bool {
        matches!(self, Tap::Writer(..))
    }

    /// Transforms the provided [Tap] into the [Tap::Reader] on success.
    ///
    /// The cursor position of the reader is set to the beginning of a file.
    ///
    /// If `self` is a [Tap::Writer] this method ensures that the chunk being currently written is
    /// comitted thus ensuring the integrity of the TAP file and also calls the [Write::flush] on
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
    /// is comitted thus ensuring the integrity of the TAP file and also calls the [Write::flush] on
    /// the file before transforming it.
    pub fn try_into_writer(self) -> Result<Self> {
        let mut file = self.try_into_file()?;
        file.seek(SeekFrom::End(0))?;
        Self::try_new_writer(file)
    }

    /// Returns the unwrapped file on success.
    ///
    /// If `self` is a [Tap::Writer] this method ensures that the chunk being currently written is
    /// comitted thus ensuring the integrity of the TAP file and also calls the [Write::flush] on
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
    /// Returns `Ok(true)` if the inserted [Tap] was a [Tap::Writer]. In this instance, the cursor
    /// position of the reader is set to the beginning of a file and this method ensures that
    /// the chunk being currently written is comitted thus ensuring the integrity of the TAP
    /// file and also calls the [Write::flush] on the file before transforming it.
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

    /// Returns `true` if no [Tap] is inserted or [Tape::running] is `false`, otherwise returns `true`.
    pub fn is_idle(&self) -> bool {
        !self.is_running()
    }

    /// Returns the current status of the inserted tape as an enum.
    pub fn tap_state(&self) -> TapState {
        if self.running {
            return match self.tap.as_ref() {
                Some(Tap::Reader(..)) => TapState::Playing,
                Some(Tap::Writer(..)) => TapState::Recording,
                _ => TapState::Idle
            }
        }
        TapState::Idle
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
    pub fn playing_reader_mut(&mut self) -> Option<&mut TapChunkPulseIter<F>> {
        if self.running {
            return self.reader_mut();
        }
        None
    }

    /// Returns a mutable reference to the tap chunk writer if there is a [Tap::Writer] variant inserted
    /// and [Tape::running] is `true`, otherwise returns `None`.
    pub fn recording_writer_mut(&mut self) -> Option<&mut TapChunkWriter<F>> {
        if self.running {
            return self.writer_mut();
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
