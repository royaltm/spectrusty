use std::borrow::Cow;
use std::path::{Path, PathBuf};
use core::num::NonZeroU32;
use std::fs::{OpenOptions, File};
use std::io::{Read, Write, Result, Error, Seek, SeekFrom, ErrorKind};

use crate::formats::tap::{*, pulse::*};

#[derive(Clone, Copy, Debug, PartialEq)]
enum TapState {
    Idle,
    Playing,
    Recording
}

enum TapFile<F> {
    Reader(TapChunkPulseIter<F>),
    Writer(TapChunkWriter<F>)
}

pub type TapFileCabinet = TapCabinet<File, PathBuf>;

pub struct TapCabinet<F, S> {
    taps: Vec<(TapFile<F>, S)>,
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
            TapFile::Writer(TapChunkWriter::try_new(file)?),
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
            TapFile::Reader(TapChunkPulseIter::from(TapChunkReader::from(file))),
            meta
        ));
    }

    pub fn remove_current_tap(&mut self) -> Option<(F, S)> {
        if self.is_idle() && !self.taps.is_empty() {
            let res = match self.taps.remove(self.current) {
                (TapFile::Reader(iter), meta) => {
                    Some((iter.into_inner().into_inner().into_inner(), meta))
                }
                (TapFile::Writer(writer), meta) => {
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
            if let Some((TapFile::Reader(iter),_)) = self.taps.get_mut(self.current) {
                return Some(iter)
            }
        }
        None
    }

    pub fn mic_out_pulse_writer(&mut self) -> Option<&mut TapChunkWriter<impl Write + Seek>> {
        if let TapState::Recording = self.state {
            if let Some((TapFile::Writer(writer),_)) = self.taps.get_mut(self.current) {
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

    pub fn current_tap_info<'a>(&'a mut self) -> Result<Option<TapReadInfoIter<'a, F>>> {
        self.with_tap_reader(|iter| {
            iter.rewind();
            let info_iter = TapReadInfoIter::from(iter.get_mut().get_mut());
            Ok(Some(info_iter))
        })
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
                if let Some(ntgt) = (no.get() as usize).checked_sub(2) {
                    iter.skip_chunks(ntgt)?;
                }
            }
            return Ok(NonZeroU32::new(iter.chunk_no()))
        })
    }

    pub fn skip_tap_chunks(&mut self, skip: usize) -> Result<Option<NonZeroU32>> {
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
                Some((TapFile::Reader(..), _)) => {
                    self.make_writer()?;
                    self.state = TapState::Recording;
                    return Ok(true)
                }
                Some((TapFile::Writer(ref mut writer), _)) => {
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
                Some((TapFile::Writer(..), _)) => {
                    self.make_reader()?;
                    self.state = TapState::Playing;
                    return Ok(true)
                }
                Some((TapFile::Reader(..), _)) => {
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
        where C: FnOnce(&'a mut TapChunkPulseIter<F>) -> Result<B>, B: Default
    {
        if !self.taps.is_empty() {
            if self.is_idle() {
                self.ensure_reader()?;
                if let (TapFile::Reader(ref mut iter), _) = &mut self.taps[self.current] {
                    return f(iter)
                }
            }
        }
        Ok(B::default())
    }

    fn ensure_reader(&mut self) -> Result<()> {
        if let Some((TapFile::Writer(..), _)) = self.taps.get(self.current) {
            self.make_reader()
        }
        else {
            Ok(())
        }
    }

    fn make_writer(&mut self) -> Result<()> {
        let taps = &mut self.taps;
        if let (TapFile::Reader(iter), meta) = taps.swap_remove(self.current) {
            let mut file: F = iter.into_inner().into_inner().into_inner();
            file.seek(SeekFrom::End(0))?;
            taps.push((
                TapFile::Writer(
                    TapChunkWriter::try_new(file)?),
                meta
            ));
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
        if let (TapFile::Writer(mut writer), meta) = taps.swap_remove(self.current) {
            writer.flush()?;
            let mut file: F = writer.into_inner().into_inner();
            file.seek(SeekFrom::Start(0))?;
            taps.push((
                TapFile::Reader(
                    TapChunkPulseIter::from(TapChunkReader::from(file))),
                meta
            ));
            let last_index = taps.len() - 1;
            taps.swap(self.current, last_index);
        }
        else {
            unreachable!()
        }
        Ok(())
    }
}
