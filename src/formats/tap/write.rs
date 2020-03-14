use core::slice;
use core::convert::{TryFrom, TryInto};
use core::num::{NonZeroU16, NonZeroU32};
use std::io::{ErrorKind, Error, Write, Seek, SeekFrom, Result};
use crate::formats::ear_mic::MicPulseWriter;
use super::{Header, checksum};

/// A tool for writing *TAP* file chunks.
pub struct TapChunkWriter<W> {
    chunk_start: Option<u64>,
    mpwr: MicPulseWriter<W>
}

/// A [TapChunkWriter] transaction holder. Created by [TapChunkWriter::begin].
pub struct TapChunkWriteTran<'a, W: Write + Seek> {
    pub checksum: u8,
    nchunks: usize,
    uncommitted: u16,
    writer: &'a mut TapChunkWriter<W>
}

impl<W> From<W> for TapChunkWriter<W> where W: Write + Seek {
    fn from(wr: W) -> Self {
        TapChunkWriter::new(wr)
    }
}

impl<W> TapChunkWriter<W> {
    /// Returns the underlying writer.
    pub fn into_inner(self) -> MicPulseWriter<W> {
        self.mpwr
    }
    /// Returns a mutable reference to the inner writer.
    pub fn get_mut(&mut self) -> &mut MicPulseWriter<W> {
        &mut self.mpwr
    }
    /// Returns a shared reference to the inner writer.
    pub fn get_ref(&self) -> &MicPulseWriter<W> {
        &self.mpwr
    }
}

impl<'a, W: Write + Seek> Drop for TapChunkWriteTran<'a, W> {
    /// Rollbacks if uncommitted. In this instance moves the cursor back to where it was before the
    /// transaction has started.
    fn drop(&mut self) {
        if let Some(start) = self.writer.chunk_start.take() {
            self.writer.mpwr.get_mut().seek(SeekFrom::Start(start)).unwrap();
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
        self.writer.mpwr.get_mut().flush()
    }
}

impl<'a, W> TapChunkWriteTran<'a, W>
    where W: Write + Seek
{
    /// Commits a *TAP* chunk.
    ///
    /// If `with_checksum` is `true` additionally writes a checksum of bytes written so far.
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
            self.writer.flush_chunk(size)?;
            nchunks += 1;
        }
        Ok(nchunks)
    }
}

impl<W> TapChunkWriter<W>
    where W: Write + Seek
{
    /// Returns a new instance of `TapChunkWriter` wrapped around the given writer.
    pub fn new(wr: W) -> Self {
        let mpwr = MicPulseWriter::new(wr);
        TapChunkWriter { chunk_start: None, mpwr }
    }
    /// Flushes internal [mic pulse writer][MicPulseWriter::flush].
    ///
    /// Returns a number of *TAP* chunks written.
    pub fn flush(&mut self) -> Result<usize> {
        if let Some(size) = self.mpwr.flush()? {
            self.flush_chunk(size)?;
            Ok(1)
        }
        else {
            Ok(0)
        }
    }
    /// Writes a provided header as a *TAP* chunk.
    ///
    /// Flushes internal [mic pulse writer][MicPulseWriter::flush] before proceeding with writing the header.
    /// Returns a number of *TAP* chunks written.
    pub fn write_header(&mut self, header: &Header) -> Result<usize> {
        self.write_chunk(header.to_tap_chunk())
    }
    /// Writes provided data as a *TAP* chunk.
    ///
    /// Flushes internal [mic pulse writer][MicPulseWriter::flush] before proceeding with writing the data.
    /// Returns a number of *TAP* chunks written.
    pub fn write_chunk<D: AsRef<[u8]>>(&mut self, chunk: D) -> Result<usize> {
        let data = chunk.as_ref();
        let size = u16::try_from(data.len()).map_err(|_|
                    Error::new(ErrorKind::InvalidData, "TAP chunk too large."))?;
        let nchunks = self.flush()?;
        let head_was_written = self.ensure_tap_head(size)?;
        self.mpwr.get_mut().write_all(data)?;
        if !head_was_written {
            if let Some(size) = NonZeroU32::new(size.into()) {
                self.flush_chunk(size)?;
            }
        }
        self.chunk_start = None;
        Ok(nchunks + 1)
    }
    /// Creates a transaction allowing for multiple data writes to the same *TAP* chunk.
    ///
    /// Flushes internal [mic pulse writer][MicPulseWriter::flush].
    ///
    /// Returns a transaction holder, use it to write data to the current chunk.
    pub fn begin(&mut self) -> Result<TapChunkWriteTran<'_, W>> {
        let nchunks = self.flush()?;
        self.ensure_tap_head(0)?;
        Ok(TapChunkWriteTran { checksum: 0, nchunks, uncommitted: 0, writer: self })
    }
    /// Interprets pulses from the provided frame pulse iterator as *TAPE* bytes and writes them
    /// to the underlying writer as *TAP* chunks.
    ///
    /// Returns a number of *TAP* chunks written.
    pub fn write_pulses_as_tap_chunks<I>(&mut self, iter: &mut I) -> Result<usize>
        where I: Iterator<Item=NonZeroU32>
    {
        let mut chunks = 0;
        self.ensure_tap_head(0)?;
        loop {
            match self.mpwr.write_pulses_as_bytes(iter)? {
                None => return Ok(chunks),
                Some(size)  => {
                    chunks += 1;
                    let chunk_start = self.flush_chunk(size)?;
                    // println!("new head at: {}", chunk_start);
                    self.mpwr.get_mut().write_all(&[0, 0])?;
                    self.chunk_start = Some(chunk_start);
                }
            }
        }
    }

    fn flush_chunk(&mut self, size: NonZeroU32) -> Result<u64> {
        let size = u16::try_from(size.get()).map_err(|_|
                    Error::new(ErrorKind::InvalidData, "TAP chunk too large."))?;
        let start = self.chunk_start.expect("there should be a header");
        // println!("flush chunk: {} at {}", size, start);
        self.chunk_start = None;
        let wr = self.mpwr.get_mut();
        let pos_cur = wr.seek(SeekFrom::Current(0))?;
        wr.seek(SeekFrom::Start(start))?;
        wr.write_all(&size.to_le_bytes())?;
        wr.seek(SeekFrom::Start(pos_cur))?;
        Ok(pos_cur)
    }

    fn ensure_tap_head(&mut self, size: u16) -> Result<bool> {
        if self.chunk_start.is_none() {
            let wr = self.mpwr.get_mut();
            let chunk_start = wr.seek(SeekFrom::Current(0))?;
            // println!("tap head at: {}", chunk_start);
            wr.write_all(&size.to_le_bytes())?;
            self.chunk_start = Some(chunk_start);
            Ok(true)
        }
        else {
            Ok(false)
        } 
    }
}
