/*
    Copyright (C) 2020-2022  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
use core::ops::{Deref, DerefMut};
use core::mem::ManuallyDrop;
use core::slice;
use core::num::NonZeroU32;
use core::convert::{TryInto, TryFrom};
use std::io::{ErrorKind, Error, Read, Seek, SeekFrom, Result, Take};

use crate::ReadExactEx;
use super::pulse::{ReadEncPulseIter, consts::PAUSE_PULSE_LENGTH};
use super::{Header, TapChunkInfo, HEAD_BLOCK_FLAG, DATA_BLOCK_FLAG, HEADER_SIZE, checksum, try_checksum};

/// Implements a [Reader][Read] of *TAP* chunks data.
///
/// Implements reader that reads only up to the size of the current *TAP* chunk.
#[derive(Debug)]
pub struct TapChunkReader<R> {
    /// The `checksum` is being updated when reading via [Read] methods from a [TapChunkReader].
    pub checksum: u8,
    next_pos: u64,
    chunk_index: u32,
    inner: Take<R>,
}

/// A guard returned by [TapChunkReader::try_clone_mut].
///
/// This struct dereferences to [TapChunkReader].
#[derive(Debug)]
pub struct TapChunkReaderMut<'a, R: Seek> {
    reader: TapChunkReader<&'a mut R>,
    original_pos: u64
}

/// Implements an iterator of [TapChunkInfo] instances from any mutable reference to [TapChunkReader]
/// including anything (like smart pointers) that dereferences to it.
#[derive(Debug)]
pub struct TapReadInfoIter<TR> {
    inner: TR,
}

/// Implements an iterator of T-state pulse intervals over the [TapChunkReader].
/// See also: [ReadEncPulseIter].
#[derive(Debug)]
pub struct TapChunkPulseIter<R> {
    /// Determines if a next chunk should be processed after the previous one ends.
    ///
    /// * `false` to start iterating over pulses of the next chunk a [TapChunkRead::next_chunk]
    ///   method should be called first. Until then the [Iterator::next] will return `None`.
    /// * `true` the next chunk will be processed automatically and a single pulse of the interval
    ///   of [PAUSE_PULSE_LENGTH] T-states is emitted before lead pulses of the next chunk.
    pub auto_next: bool,
    ep_iter: ReadEncPulseIter<TapChunkReader<R>>
}

/// A trait with tools implemented by tap chunk readers.
pub trait TapChunkRead {
    /// Returns this chunk's number.
    ///
    /// The first chunk's number is 1. If this method returns 0 the cursor is at the beginning of a file,
    /// before the first chunk.
    fn chunk_no(&self) -> u32;
    /// Returns this chunk's remaining bytes to be read.
    fn chunk_limit(&self) -> u16;
    /// Repositions the inner reader to the start of a file and sets the inner limit to 0.
    /// To read the first chunk you need to call [TapChunkRead::next_chunk] first.
    fn rewind(&mut self);
    /// Forwards the inner reader to the position of the next *TAP* chunk.
    ///
    /// Returns `Ok(None)` if the end of the file has been reached.
    ///
    /// On success returns `Ok(size)` in bytes of the next *TAP* chunk
    /// and limits the inner [Take] reader to that size.
    fn next_chunk(&mut self) -> Result<Option<u16>>;
    /// Forwards the inner reader to the position of a next `skip` + 1 *TAP* chunks.
    /// Returns `Ok(None)` if the end of the file has been reached.
    /// On success returns `Ok(size)` in bytes of the next *TAP* chunk
    /// and limits the inner [Take] reader to that size.
    ///
    /// `skip_chunks(0)` acts the same as [TapChunkRead::next_chunk].
    fn skip_chunks(&mut self, skip: u32) -> Result<Option<u16>> {
        for _ in 0..skip {
            if self.next_chunk()?.is_none() {
                return Ok(None)
            }
        }
        self.next_chunk()
    }
    /// Rewinds or forwards the tape to the nth chunk. Returns `Ok(true)` if the nth chunk exists.
    /// Otherwise returns `Ok(false)` and leaves the seek position at the end of the tape.
    fn rewind_nth_chunk(&mut self, chunk_no: u32) -> Result<bool> {
        let current_no = self.chunk_no();
        let res = if chunk_no > current_no {
            self.skip_chunks(chunk_no - current_no - 1)?.is_some()
        }
        else {
            if current_no != 0 {
                self.rewind();
            }
            if chunk_no != 0 {
                self.skip_chunks(chunk_no - 1)?.is_some()
            }
            else {
                true
            }
        };
        Ok(res)
    }
    /// Forwards the tape to the next chunk. Returns `Ok(true)` if the forwarded to chunk exists.
    /// Otherwise returns `Ok(false)` and leaves the seek position at the end of the tape.
    fn forward_chunk(&mut self) -> Result<bool> {
        Ok(self.next_chunk()?.is_some())
    }
    /// Rewinds the tape to the beginning of the previous chunk. Returns `Ok(chunk_no)`.
    fn rewind_prev_chunk(&mut self) -> Result<u32> {
        if let Some(no) = NonZeroU32::new(self.chunk_no()) {
            self.rewind();
            if let Some(ntgt) = no.get().checked_sub(2) {
                self.skip_chunks(ntgt)?;
            }
        }
        Ok(self.chunk_no())
    }
    /// Rewinds the tape to the beginning of the current chunk. Returns `Ok(chunk_no)`.
    fn rewind_chunk(&mut self) -> Result<u32> {
        if let Some(no) = NonZeroU32::new(self.chunk_no()) {
            self.rewind();
            self.skip_chunks(no.get() - 1)?;
        }
        Ok(self.chunk_no())
    }
}

impl<R: Read> TryFrom<&'_ mut Take<R>> for TapChunkInfo {
    type Error = Error;

    #[inline]
    fn try_from(rd: &mut Take<R>) -> Result<Self> {
        let limit = match rd.limit() {
            0 => {
                return Ok(TapChunkInfo::Empty)
            }
            limit if limit > u16::max_value().into() => {
                return Err(Error::new(ErrorKind::InvalidData, "Not a proper TAP chunk: too large"));
            }
            limit => limit
        };
        let mut flag: u8 = 0;
        rd.read_exact(slice::from_mut(&mut flag))?;
        if limit == 1 {
            return Ok(TapChunkInfo::Unknown { size: 1, flag })
        }
        match flag {
            HEAD_BLOCK_FLAG if limit == HEADER_SIZE as u64 => {
                let mut header: [u8; HEADER_SIZE - 1] = Default::default();
                rd.read_exact(&mut header)?;
                if checksum(header) != flag {
                    Ok(TapChunkInfo::Unknown { size: limit as u16, flag })
                }
                else {
                    Header::try_from(&header[..HEADER_SIZE - 2])
                    .map(TapChunkInfo::Head)
                    .or(Ok(TapChunkInfo::Unknown { size: limit as u16, flag }))
                }
            }
            DATA_BLOCK_FLAG => {
                let checksum = try_checksum(rd.by_ref().bytes())? ^ flag;
                if rd.limit() != 0 {
                    return Err(Error::new(ErrorKind::InvalidData, "Not a proper TAP block: invalid length"));
                }
                Ok(TapChunkInfo::Data{ length: limit as u16 - 2, checksum })
            }
            flag => {
                // TODO: perhaps check length and read to eof
                Ok(TapChunkInfo::Unknown { size: limit as u16, flag })
            }
        }
    }    
}

impl<R> TapChunkReader<R> {
    /// Returns the wrapped reader.
    pub fn into_inner(self) -> R {
        self.inner.into_inner()
    }
    /// Returns a reference to the chunk's [Take] reader.
    pub fn get_ref(&self) -> &Take<R> {
        &self.inner
    }

    /// Returns a mutable reference to the chunk's [Take] reader.
    pub fn get_mut(&mut self) -> &mut Take<R> {
        &mut self.inner
    }
}

impl<R: Read + Seek> TapChunkReader<R> {
    /// Creates a new instance of [TapChunkReader] from the reader with an assumption that the next
    /// two bytes read from it will form the next chunk header.
    ///
    /// `chunk_no` should be the chunk number of the previous chunk.
    pub fn try_from_current(mut rd: R, chunk_no: u32) -> Result<Self> {
        let next_pos = rd.seek(SeekFrom::Current(0))?;
        let inner = rd.take(0);
        Ok(TapChunkReader { next_pos, chunk_index: chunk_no, checksum: 0, inner })
    }

    /// Creates a clone of self but with a mutable reference to the underlying reader.
    ///
    /// Returns a guard that, when dropped, will try to restore the original position
    /// of the reader. However to check if it succeeded it's better to use [TapChunkReaderMut::done]
    /// method directly on the guard which returns a result from the seek operation.
    pub fn try_clone_mut(&mut self) -> Result<TapChunkReaderMut<'_, R>> {
        let limit = self.inner.limit();
        let inner = self.inner.get_mut().take(limit);
        TapChunkReader {
            checksum: self.checksum,
            next_pos: self.next_pos,
            chunk_index: self.chunk_index,
            inner
        }.try_into()
    }
}

impl<R: Read + Seek> TapChunkRead for TapChunkReader<R> {
    fn chunk_no(&self) -> u32 {
        self.chunk_index
    }

    fn chunk_limit(&self) -> u16 {
        self.inner.limit() as u16
    }

    fn rewind(&mut self) {
        self.inner.set_limit(0);
        self.checksum = 0;
        self.chunk_index = 0;
        self.next_pos = 0;
    }

    /// Also clears [TapChunkReader::checksum].
    fn next_chunk(&mut self) -> Result<Option<u16>> {
        let rd = self.inner.get_mut();
        if self.next_pos != rd.seek(SeekFrom::Start(self.next_pos))? {
            return Err(Error::new(ErrorKind::UnexpectedEof, "stream unexpectedly ended"));
        }

        let mut size: [u8; 2] = Default::default();
        if !rd.read_exact_or_none(&mut size)? {
            self.inner.set_limit(0);
            return Ok(None)
        }
        let size = u16::from_le_bytes(size);
        self.chunk_index += 1;
        self.checksum = 0;
        self.inner.set_limit(size as u64);
        self.next_pos += size as u64 + 2;
        Ok(Some(size))
    }
}

impl<R: Read> Read for TapChunkReader<R> {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        match self.inner.read(buf) {
            Ok(size) => {
                self.checksum ^= checksum(&buf[..size]);
                Ok(size)
            }
            e => e
        }
    }

    #[inline]
    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> Result<usize> {
        match self.inner.read_to_end(buf) {
            Ok(size) => {
                self.checksum ^= checksum(&buf[buf.len() - size..]);
                Ok(size)
            }
            e => e
        }
    }
}

impl<R: Read + Seek> From<R> for TapChunkReader<R> {
    fn from(rd: R) -> Self {
        let inner = rd.take(0);
        TapChunkReader { next_pos: 0, chunk_index: 0, checksum: 0, inner }
    }
}

impl<'a, R: Seek> TapChunkReaderMut<'a, R> {
    /// Tries to restore the original reader position before dropping self.
    pub fn done(self) -> Result<u64> {
        let pos = self.original_pos;
        let mut nodrop = ManuallyDrop::new(self);
        nodrop.reader.inner.get_mut().seek(SeekFrom::Start(pos))
    }
}

impl<'a, R: Seek> Drop for TapChunkReaderMut<'a, R> {
    fn drop(&mut self) {
        let pos = self.original_pos;
        let _ = self.reader.inner.get_mut().seek(SeekFrom::Start(pos));
    }
}

impl<'a, R: Seek> TryFrom<TapChunkReader<&'a mut R>> for TapChunkReaderMut<'a, R> {
    type Error = Error;

    fn try_from(mut reader: TapChunkReader<&'a mut R>) -> Result<Self> {
        let original_pos = reader.inner.get_mut().seek(SeekFrom::Current(0))?;
        Ok(TapChunkReaderMut { reader, original_pos })
    }
}

impl<'a, R: Seek> Deref for TapChunkReaderMut<'a, R> {
    type Target = TapChunkReader<&'a mut R>;
    fn deref(&self) -> &Self::Target {
        &self.reader
    }
}

impl<'a, R: Seek> DerefMut for TapChunkReaderMut<'a, R> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.reader
    }
}

impl<T, R> From<T> for TapReadInfoIter<T>
    where T: Deref<Target=TapChunkReader<R>> + DerefMut,
          R: Read + Seek
{
    #[inline]
    fn from(inner: T) -> Self {
        TapReadInfoIter { inner }
    }
}

impl<T, R> Iterator for TapReadInfoIter<T>
    where T: Deref<Target=TapChunkReader<R>> + DerefMut,
          R: Read + Seek
{
    type Item = Result<TapChunkInfo>;

    fn next(&mut self) -> Option<Self::Item> {
        let info = match self.inner.next_chunk() {
            Ok(None) => return None,
            Ok(_) => TapChunkInfo::try_from(self.inner.get_mut()),
            Err(e) => return Some(Err(e))
        };
        Some(info)
    }
}

impl<T, R> AsRef<TapChunkReader<R>> for TapReadInfoIter<T>
    where T: Deref<Target=TapChunkReader<R>>
{
    fn as_ref(&self) -> &TapChunkReader<R> {
        &self.inner
    }
}

impl<T, R> AsMut<TapChunkReader<R>> for TapReadInfoIter<T>
    where T: Deref<Target=TapChunkReader<R>> + DerefMut
{
    fn as_mut(&mut self) -> &mut TapChunkReader<R> {
        &mut self.inner
    }
}

impl<R: Read + Seek> From<TapChunkReader<R>> for TapChunkPulseIter<R> {
    fn from(rd: TapChunkReader<R>) -> Self {
        TapChunkPulseIter::from(ReadEncPulseIter::new(rd))
    }
}

impl<R: Read + Seek> From<ReadEncPulseIter<TapChunkReader<R>>> for TapChunkPulseIter<R> {
    fn from(ep_iter: ReadEncPulseIter<TapChunkReader<R>>) -> Self {
        TapChunkPulseIter { auto_next: true, ep_iter }
    }
}

impl<R> TapChunkPulseIter<R> {
    /// Returns the wrapped iterator.
    pub fn into_inner(self) -> ReadEncPulseIter<TapChunkReader<R>> {
        self.ep_iter
    }
    /// Returns a reference to the inner iterator.
    pub fn get_ref(&self) -> &ReadEncPulseIter<TapChunkReader<R>> {
        &self.ep_iter
    }
    /// Returns a mutable reference to the inner iterator.
    pub fn get_mut(&mut self) -> &mut ReadEncPulseIter<TapChunkReader<R>> {
        &mut self.ep_iter
    }
}

impl<R> TapChunkPulseIter<R>
    where R: Read + Seek
{
    /// Returns `true` if there are no more pulses to emit or there
    /// was an error while reading bytes.
    ///
    /// If [TapChunkPulseIter::auto_next] is `false` returns `true` after processing each chunk.
    /// Otherwise returns `true` after all chunks have been processed.
    pub fn is_done(&self) -> bool {
        self.ep_iter.is_done()
    }
}

impl<R: Read + Seek> TapChunkRead for TapChunkPulseIter<R> {
    fn chunk_no(&self) -> u32 {
        self.ep_iter.get_ref().chunk_no()
    }

    fn chunk_limit(&self) -> u16 {
        self.ep_iter.get_ref().chunk_limit()
    }

    /// Invokes underlying [TapChunkReader::rewind] and [resets][ReadEncPulseIter::reset] the internal
    /// pulse iterator. Returns the result from [TapChunkReader::rewind].
    fn rewind(&mut self) {
        self.ep_iter.get_mut().rewind();
        self.ep_iter.reset();
    }

    /// Invokes underlying [TapChunkReader::next_chunk] and [resets][ReadEncPulseIter::reset] the internal
    /// pulse iterator. Returns the result from [TapChunkReader::next_chunk].
    fn next_chunk(&mut self) -> Result<Option<u16>> {
        let res = self.ep_iter.get_mut().next_chunk()?;
        self.ep_iter.reset();
        Ok(res)
    }

    /// Invokes underlying [TapChunkReader::skip_chunks] and [resets][ReadEncPulseIter::reset] the internal
    /// pulse iterator. Returns the result from [TapChunkReader::skip_chunks].
    fn skip_chunks(&mut self, skip: u32) -> Result<Option<u16>> {
        let res = self.ep_iter.get_mut().skip_chunks(skip)?;
        self.ep_iter.reset();
        Ok(res)        
    }
}

impl<R> AsRef<TapChunkReader<R>> for TapChunkPulseIter<R> {
    fn as_ref(&self) -> &TapChunkReader<R> {
        self.ep_iter.get_ref()
    }
}

impl<R> AsMut<TapChunkReader<R>> for TapChunkPulseIter<R> {
    fn as_mut(&mut self) -> &mut TapChunkReader<R> {
        self.ep_iter.get_mut()
    }
}

impl<R: Read + Seek> Iterator for TapChunkPulseIter<R> {
    type Item = NonZeroU32;

    fn next(&mut self) -> Option<Self::Item> {
        match self.ep_iter.next() {
            pulse @ Some(_) => pulse,
            None if self.auto_next => {
                if self.ep_iter.err().is_some() ||
                        self.ep_iter.get_mut().next_chunk().is_err() {
                    return None;
                }
                self.ep_iter.reset();
                if self.ep_iter.is_done() {
                    None
                }
                else {
                    Some(PAUSE_PULSE_LENGTH)
                }
            }
            None => None
        }
    }
}
