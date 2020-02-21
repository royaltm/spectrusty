use core::slice;
use core::num::NonZeroU32;
use core::convert::TryFrom;
use std::io::{ErrorKind, Error, Read, Seek, SeekFrom, Result, Take};
use crate::formats::ear_mic::{EarPulseIter, consts::PAUSE_PULSE_LENGTH};
use super::{Header, TapChunkInfo, HEAD_BLOCK_FLAG, DATA_BLOCK_FLAG, HEADER_SIZE, checksum, try_checksum};

/// Implements a [Reader][Read] of *TAP* chunks data.
///
/// Implements reader that reads only up to the size of the current *TAP* chunk.
#[derive(Debug)]
pub struct TapChunkReader<R> {
    next_pos: u64,
    chunk_index: u32,
    inner: Take<R>,
}

/// Implements an iterator of [TapChunkInfo] over the [TapChunkReader].
#[derive(Debug)]
pub struct TapReadInfoIter<'a, R> {
    inner: &'a mut TapChunkReader<R>,
}

/// Implements an iterator of T-state pulse intervals over the [TapChunkReader].
/// See also: [EarPulseIter].
#[derive(Debug)]
pub struct TapEarPulseIter<R> {
    ep_iter: EarPulseIter<TapChunkReader<R>>
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
            HEAD_BLOCK_FLAG => {
                let mut header: [u8; HEADER_SIZE - 1] = Default::default();
                rd.read_exact(&mut header)?;
                if rd.limit() != 0 {
                    return Err(Error::new(ErrorKind::InvalidData, "Not a proper TAP header: invalid length"));
                }
                if checksum(&header) != flag {
                    return Err(Error::new(ErrorKind::InvalidData, "Not a proper TAP header: invalid checksum"));
                }
                Ok(TapChunkInfo::Head(Header::try_from(&header[..HEADER_SIZE - 2])?))
            }
            DATA_BLOCK_FLAG => {
                let checksum = try_checksum(rd.by_ref().bytes())? ^ flag;
                if rd.limit() != 0 {
                    return Err(Error::new(ErrorKind::InvalidData, "Not a proper TAP header: invalid length"));
                }
                Ok(TapChunkInfo::Data{ length: limit as u16 - 2, checksum })
            }
            flag => {
                // TODO: check length and read to eof
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

impl<R: Read> TapChunkReader<R> {
    pub fn chunk_no(&self) -> u32 {
        self.chunk_index
    }
    /// Returns this chunk's remaining bytes limit to be read.
    pub fn chunk_limit(&self) -> u16 {
        self.inner.limit() as u16
    }
}

impl<R: Read + Seek> TapChunkReader<R> {
    /// Forwards the inner reader to the position of a next *TAP* chunk.
    /// Returns `Ok(None)` if end of file has been reached.
    /// On success returns `Ok(size)` in bytes of the next *TAP* chunk
    /// and limits the inner [Take] reader to that size.
    pub fn next_chunk(&mut self) -> Result<Option<u16>> {
        let rd = self.inner.get_mut();
        if self.next_pos != rd.seek(SeekFrom::Start(self.next_pos))? {
            return Err(Error::new(ErrorKind::UnexpectedEof, "stream unexpectedly ended"));
        }

        let mut size: [u8; 2] = Default::default();
        loop {
            match rd.read(&mut size) {
                Ok(0) => {
                    self.inner.set_limit(0);
                    return Ok(None)
                },
                Ok(2) => break,
                Ok(_) => return Err(Error::new(ErrorKind::UnexpectedEof, "failed to fill whole buffer")),
                Err(ref e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            };
        }
        let size = u16::from_le_bytes(size);
        self.chunk_index += 1;
        self.inner.set_limit(size as u64);
        self.next_pos += size as u64 + 2;
        Ok(Some(size))
    }

    /// Forwards the inner reader to the position of a next `skip` + 1 *TAP* chunks.
    /// Returns `Ok(None)` if end of file has been reached.
    /// On success returns `Ok(size)` in bytes of the next *TAP* chunk
    /// and limits the inner [Take] reader to that size.
    pub fn skip_chunks(&mut self, skip: usize) -> Result<Option<u16>> {
        for _ in 0..skip {
            if self.next_chunk()?.is_none() {
                return Ok(None)
            }
        }
        self.next_chunk()
    }

    /// Repositions the inner reader to the start of a file and sets inner limit to 0.
    /// To read the first chunk you need to call [TapChunkReader::next_chunk] first.
    pub fn rewind(&mut self) {
        self.inner.set_limit(0);
        self.chunk_index = 0;
        self.next_pos = 0;
    }
}

impl<R: Read> Read for TapChunkReader<R> {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        self.inner.read(buf)
    }

    #[inline]
    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> Result<usize> {
        self.inner.read_to_end(buf)
    }
}

impl<R: Read + Seek> From<R> for TapChunkReader<R> {
    fn from(mut rd: R) -> Self {
        let next_pos = rd.seek(SeekFrom::Current(0)).unwrap();
        let inner = rd.take(0);
        TapChunkReader { next_pos, chunk_index: 0, inner }
    }
}

impl<'a, R> From<&'a mut TapChunkReader<R>> for TapReadInfoIter<'a, R>
{
    #[inline]
    fn from(inner: &'a mut TapChunkReader<R>) -> Self {
        TapReadInfoIter { inner }
    }
}

impl<'a, R: Read + Seek> Iterator for TapReadInfoIter<'a, R> {
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

impl<R: Read + Seek> From<TapChunkReader<R>> for TapEarPulseIter<R> {
    fn from(rd: TapChunkReader<R>) -> Self {
        TapEarPulseIter::from(EarPulseIter::new(rd))
    }
}

impl<R: Read + Seek> From<EarPulseIter<TapChunkReader<R>>> for TapEarPulseIter<R> {
    fn from(ep_iter: EarPulseIter<TapChunkReader<R>>) -> Self {
        TapEarPulseIter { ep_iter }        
    }
}

impl<R> TapEarPulseIter<R>
    where R: Read + Seek
{
    pub fn chunk_no(&self) -> u32 {
        self.ep_iter.get_ref().chunk_no()
    }

    pub fn rewind(&mut self) {
        self.ep_iter.get_mut().rewind();
        self.ep_iter.reset();
    }

    pub fn next_chunk(&mut self) -> Result<Option<u16>> {
        let res = self.ep_iter.get_mut().next_chunk()?;
        self.ep_iter.reset();
        Ok(res)
    }

    pub fn skip_chunks(&mut self, skip: usize) -> Result<Option<u16>> {
        let res = self.ep_iter.get_mut().skip_chunks(skip)?;
        self.ep_iter.reset();
        Ok(res)        
    }

}

impl<R> TapEarPulseIter<R> {
    pub fn into_inner(self) -> EarPulseIter<TapChunkReader<R>> {
        self.ep_iter
    }

    pub fn get_mut(&mut self) -> &mut EarPulseIter<TapChunkReader<R>> {
        &mut self.ep_iter
    }

    pub fn get_ref(&self) -> &EarPulseIter<TapChunkReader<R>> {
        &self.ep_iter
    }
}

impl<R: Read + Seek> Iterator for TapEarPulseIter<R> {
    type Item = NonZeroU32;

    fn next(&mut self) -> Option<Self::Item> {
        match self.ep_iter.next() {
            pulse @ Some(_) => pulse,
            None => {
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
        }
    }
}
