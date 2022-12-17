/*
    Copyright (C) 2020-2022  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
use core::mem;
use core::slice;
use core::convert::{TryFrom, TryInto};
use core::num::NonZeroU32;
use std::io::{ErrorKind, Error, Write, Seek, SeekFrom, Result};
use super::pulse::PulseDecodeWriter;
use super::{Header, checksum};

const LEN_PREFIX_SIZE: u64 = mem::size_of::<u16>() as u64;

/// A tool for writing *TAP* file chunks to byte streams.
///
/// Data can be written in one of 3 ways:
///
/// * Writing tap chunk data at once with [TapChunkWriter::write_header] or [TapChunkWriter::write_chunk].
/// * Writing tap chunk data with multiple writes via [TapChunkWriter::begin] and then [TapChunkWriteTran].
/// * Writing tap chunks from decoded *TAPE* pulse iterators with [TapChunkWriter::write_pulses_as_tap_chunks].
pub struct TapChunkWriter<W> {
    chunk_head: u64,
    mpwr: PulseDecodeWriter<W>
}

/// A [TapChunkWriter] transaction holder. Created by [TapChunkWriter::begin].
///
/// Write data with its [Write] implementation methods.
/// Then call [TapChunkWriteTran::commit].
pub struct TapChunkWriteTran<'a, W: Write + Seek> {
    /// Checksum byte updated with each write.
    pub checksum: u8,
    nchunks: usize,
    uncommitted: u16,
    writer: &'a mut TapChunkWriter<W>
}

impl<W> TapChunkWriter<W> {
    /// Returns the underlying pulse decode writer.
    pub fn into_inner(self) -> PulseDecodeWriter<W> {
        self.mpwr
    }
    /// Returns a mutable reference to the inner pulse decode writer.
    pub fn get_mut(&mut self) -> &mut PulseDecodeWriter<W> {
        &mut self.mpwr
    }
    /// Returns a shared reference to the inner pulse decode writer.
    pub fn get_ref(&self) -> &PulseDecodeWriter<W> {
        &self.mpwr
    }
}

impl<'a, W: Write + Seek> Drop for TapChunkWriteTran<'a, W> {
    /// Rollbacks if uncommitted. In this instance moves the cursor back to where it was before the
    /// transaction has started.
    fn drop(&mut self) {
        if self.uncommitted != 0 {
            let chunk_head = self.writer.chunk_head.checked_add(LEN_PREFIX_SIZE).unwrap();
            let _ = self.writer.mpwr.get_mut().seek(SeekFrom::Start(chunk_head));
        }
    }
}

impl<'a, W: Write + Seek> Write for TapChunkWriteTran<'a, W> {
    /// Appends data to the current chunk.
    /// 
    /// Any number of writes should be followed by [TapChunkWriteTran::commit].
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        let _: u16 = (self.uncommitted as usize).checked_add(buf.len()).unwrap()
                    .try_into().map_err(|e| Error::new(ErrorKind::WriteZero, e))?;
        let written = self.writer.mpwr.get_mut().write(buf)?;
        self.checksum ^= checksum(&buf[..written]);
        self.uncommitted += written as u16;
        Ok(written)
    }

    fn flush(&mut self) -> Result<()> {
        self.writer.flush()
    }
}

impl<'a, W> TapChunkWriteTran<'a, W>
    where W: Write + Seek
{
    /// Commits a *TAP* chunk.
    ///
    /// If `with_checksum` is `true` additionally writes a checksum byte of data written so far.
    ///
    /// Returns number of *TAP* chunks written including the call to `begin`.
    pub fn commit(mut self, with_checksum: bool) -> Result<usize> {
        let mut nchunks = self.nchunks;
        let mut uncommitted = self.uncommitted;
        if with_checksum {
            if let Some(size) = uncommitted.checked_add(1) {
                uncommitted = size;
                let checksum = self.checksum;
                self.write_all(slice::from_ref(&checksum))?;
            }
            else {
                return Err(Error::new(ErrorKind::WriteZero, "chunk is larger than the maximum allowed size"))
            }
        }
        if let Some(size) = NonZeroU32::new(uncommitted.into()) {
            self.writer.commit_chunk(size)?;
            nchunks += 1;
        }
        Ok(nchunks)
    }
}

impl<W> TapChunkWriter<W>
    where W: Write + Seek
{
    /// Returns a new instance of `TapChunkWriter` with the given writer on success.
    ///
    /// The stream cursor should be positioned where the next chunk will be written.
    ///
    /// This method does not write any data, but moves the stream cursor to make room for
    /// the next block's length indicator.
    pub fn try_new(wr: W) -> Result<Self> {
        let mut mpwr = PulseDecodeWriter::new(wr);
        let chunk_start = mpwr.get_mut().seek(SeekFrom::Current(LEN_PREFIX_SIZE as i64))?;
        let chunk_head = chunk_start.checked_sub(LEN_PREFIX_SIZE).unwrap();
        Ok(TapChunkWriter { chunk_head, mpwr })
    }
    /// Flushes the underlying writer, ensuring that all intermediately buffered
    /// contents reach their destination (invokes [Write::flush]).
    pub fn flush(&mut self) -> Result<()> {
        self.mpwr.get_mut().flush()
    }
    /// Forces pending pulse decode data transfer to [end][PulseDecodeWriter::end].
    ///
    /// Returns the number of *TAP* chunks written.
    pub fn end_pulse_chunk(&mut self) -> Result<usize> {
        if let Some(size) = self.mpwr.end()? {
            self.commit_chunk(size)?;
            Ok(1)
        }
        else {
            Ok(0)
        }
    }
    /// Writes a provided header as a *TAP* chunk.
    ///
    /// Flushes internal [mic pulse writer][PulseDecodeWriter::end] before proceeding with writing the header.
    ///
    /// Returns the number of *TAP* chunks written.
    pub fn write_header(&mut self, header: &Header) -> Result<usize> {
        self.write_chunk(header.to_tap_chunk())
    }
    /// Writes provided data as a *TAP* chunk.
    ///
    /// Flushes internal [mic pulse writer][PulseDecodeWriter::end] before proceeding with writing the data.
    ///
    /// Returns the number of *TAP* chunks written.
    pub fn write_chunk<D: AsRef<[u8]>>(&mut self, chunk: D) -> Result<usize> {
        let data = chunk.as_ref();
        let size = u16::try_from(data.len()).map_err(|_|
                    Error::new(ErrorKind::InvalidData, "TAP chunk too large."))?;
        let nchunks = self.end_pulse_chunk()?;
        let wr = self.mpwr.get_mut();
        let chunk_head = wr.seek(SeekFrom::Start(self.chunk_head))?;
        debug_assert_eq!(chunk_head, self.chunk_head);
        wr.write_all(&size.to_le_bytes())?;
        wr.write_all(data)?;
        let chunk_start = wr.seek(SeekFrom::Current(LEN_PREFIX_SIZE as i64))?;
        self.chunk_head = chunk_start.checked_sub(LEN_PREFIX_SIZE).unwrap();
        Ok(nchunks + 1)
    }
    /// Creates a transaction allowing for multiple data writes to the same *TAP* chunk.
    ///
    /// Flushes internal [mic pulse writer][PulseDecodeWriter::end].
    ///
    /// Returns a transaction holder, which can be used to write data to the current chunk.
    pub fn begin(&mut self) -> Result<TapChunkWriteTran<'_, W>> {
        let nchunks = self.end_pulse_chunk()?;
        Ok(TapChunkWriteTran { checksum: 0, nchunks, uncommitted: 0, writer: self })
    }
    /// Interprets pulse intervals from the provided iterator as bytes and writes them
    /// to the underlying writer as *TAP* chunks.
    ///
    /// See [PulseDecodeWriter].
    ///
    /// Returns the number of *TAP* chunks written.
    pub fn write_pulses_as_tap_chunks<I>(&mut self, mut iter: I) -> Result<usize>
        where I: Iterator<Item=NonZeroU32>
    {
        let mut chunks = 0;
        loop {
            match self.mpwr.write_decoded_pulses(iter.by_ref())? {
                None => return Ok(chunks),
                Some(size)  => {
                    chunks += 1;
                    self.commit_chunk(size)?;
                }
            }
        }
    }

    fn commit_chunk(&mut self, size: NonZeroU32) -> Result<()> {
        let size = u16::try_from(size.get()).map_err(|_|
                    Error::new(ErrorKind::InvalidData, "TAP chunk too large."))?;
        // println!("flush chunk: {} at {}", size, self.chunk_head);
        let wr = self.mpwr.get_mut();
        let chunk_head = wr.seek(SeekFrom::Start(self.chunk_head))?;
        debug_assert_eq!(chunk_head, self.chunk_head);
        wr.write_all(&size.to_le_bytes())?;
        self.chunk_head = chunk_head.checked_add(LEN_PREFIX_SIZE + size as u64).unwrap();
        let pos_cur = self.chunk_head.checked_add(LEN_PREFIX_SIZE).unwrap();
        let pos_next = wr.seek(SeekFrom::Start(pos_cur))?;
        debug_assert_eq!(pos_next, pos_cur);
        Ok(())
    }
}
