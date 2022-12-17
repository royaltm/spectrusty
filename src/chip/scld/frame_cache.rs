/*
    Copyright (C) 2020-2022  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
use core::fmt;
use core::convert::TryFrom;

use bitflags::bitflags;

use crate::clock::{Ts, VFrameTs, VideoTsData2};
use crate::chip::{
    ScldCtrlFlags, UlaPlusRegFlags,
    ula::{
        frame_cache::{UlaFrameCache, UlaFrameLineIter}
    }
};
use crate::memory::ScreenArray;
use crate::video::{
    VideoFrame, 
    frame_cache::{
        COLUMNS, PIXEL_LINES,
        ink_line_from, attr_line_from,
        PlusVidFrameDataIterator
    }
};

#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};

bitflags! {
    /// Flags determining the screen data source for INK/PAPER masks and attributes.
    ///
    /// ```text
    /// source
    /// attr  sec.  mode      ink_mask  attr
    ///    0    0   screen 0  screen 0  screen 0 attr. (8 lines)
    ///    0    1   screen 1  screen 1  screen 1 attr. (8 lines)
    ///    1    0   combined  screen 0  screen 1
    ///    1    1   combined  screen 1  screen 1
    /// ```
    #[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "snapshot", serde(try_from = "u8", into = "u8"))]
    #[derive(Default)]
    pub struct SourceMode: u8 {
        /// Determines the source of INK/PAPER masks.
        const SECOND_SCREEN = 0b001;
        /// Determines the source of attributes together with [SourceMode::SECOND_SCREEN].
        const ATTR_HI_COLOR = 0b010;
        const SOURCE_MASK   = 0b011;
        /// Determines the screen bank the data is read from.
        const SHADOW_BANK   = 0b100;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TryFromU8SourceModeError(pub u8);

/// A reference to primary and secondary screen data with relevant frame caches.
pub struct ScldFrameRef<'a, V> {
    /// primary screen data
    pub screen0: &'a ScreenArray,
    /// secondary screen data
    pub screen1: &'a ScreenArray,
    /// primary screen frame cache
    pub frame_cache0: &'a UlaFrameCache<V>,
    /// secondary screen frame cache
    pub frame_cache1: &'a UlaFrameCache<V>,
}

/// Implements a [PlusVidFrameDataIterator] for SCLD based Timex models.
pub struct ScldFrameProducer<'a, V, I> {
    frame_ref: ScldFrameRef<'a, V>,
    line: usize,
    source: SourceMode,
    next_source: SourceMode,
    next_change_at: VFrameTs<V>,
    source_changes: I,
    line_iter: UlaFrameLineIter<'a>
}

const EMPTY_FRAME_LINE: &(u32, [u8;COLUMNS]) = &(0, [0u8;COLUMNS]);
const EMPTY_LINE_ITER: UlaFrameLineIter<'_> = UlaFrameLineIter {
    column: 0,
    ink_line: &EMPTY_FRAME_LINE.1,
    attr_line: &EMPTY_FRAME_LINE.1,
    frame_pixels: EMPTY_FRAME_LINE,
    frame_colors: EMPTY_FRAME_LINE,
    frame_colors_coarse: EMPTY_FRAME_LINE
};

impl SourceMode {
    #[inline]
    pub fn from_scld_flags(flags: ScldCtrlFlags) -> SourceMode {
        SourceMode::from_bits_truncate(
            (flags & ScldCtrlFlags::SCREEN_SOURCE_MASK).bits()
        )
    }
    #[inline]
    pub fn from_plus_reg_flags(flags: UlaPlusRegFlags) -> SourceMode {
        /*
            ula +   source mode
            0 0 0 -> 0 0 0      screen 0
            0 0 1 -> 0 0 1      screen 1
            0 1 0 -> 0 1 0      hi-color
            0 1 1 -> 1 1 0 *    hi-color (shadow)
            1 0 0 -> 1 0 0      screen 0 (shadow)
            1 0 1 -> 1 0 1      screen 1 (shadow)
            1 1 0 -> 0 1 0 *    hi-res
            1 1 1 -> 1 1 0 *    hi-res   (shadow)
        */
        match (flags & UlaPlusRegFlags::SCREEN_MODE_MASK).bits() {
            0b110 => SourceMode::ATTR_HI_COLOR, // 0b010
            0b011|
            0b111 => SourceMode::SHADOW_BANK|SourceMode::ATTR_HI_COLOR, // 0b110
            bits  => SourceMode::from_bits_truncate(bits)
        }
    }
    #[inline]
    pub fn is_second_screen(self) -> bool {
        self.intersects(SourceMode::SECOND_SCREEN)
    }
    #[inline]
    pub fn is_hi_color(self) -> bool {
        self.intersects(SourceMode::ATTR_HI_COLOR)
    }
    #[inline]
    pub fn is_shadow_bank(self) -> bool {
        self.intersects(SourceMode::SHADOW_BANK)
    }
}

impl From<SourceMode> for u8 {
    #[inline]
    fn from(mode: SourceMode) -> u8 {
        mode.bits()
    }
}

impl From<VideoTsData2> for SourceMode {
    #[inline]
    fn from(vtsm: VideoTsData2) -> Self {
        SourceMode::from_bits_truncate(vtsm.into_data())
    }
}

impl std::error::Error for TryFromU8SourceModeError {}

impl fmt::Display for TryFromU8SourceModeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "converted integer (0x{:x}) contains extraneous bits for `SourceMode`", self.0)
    }
}

impl TryFrom<u8> for SourceMode {
    type Error = TryFromU8SourceModeError;
    fn try_from(mode: u8) -> Result<Self, Self::Error> {
        SourceMode::from_bits(mode).ok_or(TryFromU8SourceModeError(mode))
    }
}

impl<'a, V> ScldFrameRef<'a, V> {
    pub fn new(
            screen0: &'a ScreenArray,
            frame_cache0: &'a UlaFrameCache<V>,
            screen1: &'a ScreenArray,
            frame_cache1: &'a UlaFrameCache<V>
        ) -> Self
    {
        ScldFrameRef { screen0, screen1, frame_cache0, frame_cache1 }
    }
}

impl<'a, V, I> ScldFrameProducer<'a, V, I>
    where V: VideoFrame,
          I: Iterator<Item=VideoTsData2>
{
    pub fn new(
            source: SourceMode,
            screen0: &'a ScreenArray,
            frame_cache0: &'a UlaFrameCache<V>,
            screen1: &'a ScreenArray,
            frame_cache1: &'a UlaFrameCache<V>,
            mut source_changes: I
        ) -> Self
    {
        let line = 0;
        let line_iter = EMPTY_LINE_ITER;
        let (next_change_at, next_source) = source_changes.next()
                    .map(|tsm| vtm_next::<V>(tsm, line as Ts))
                    .unwrap_or_else(||
                        ( VFrameTs::max_value(), source )
                    );
        let mut prod = ScldFrameProducer {
            frame_ref: ScldFrameRef::new(screen0, frame_cache0, screen1, frame_cache1),
            line,
            next_source,
            next_change_at,
            source_changes,
            source,
            line_iter
        };
        prod.update_iter();
        prod
    }

    pub fn swap_frame(&mut self, frame_ref: &mut ScldFrameRef<'a, V>) {
        core::mem::swap(&mut self.frame_ref, frame_ref);
        self.update_iter();
    }

    #[inline(always)]
    pub fn peek_hts(&mut self) -> Ts {
        col_hts(self.line_iter.column as i16)
    }

    #[inline(always)]
    pub fn column(&mut self) -> usize {
        self.line_iter.column
    }

    #[inline(always)]
    pub fn line(&mut self) -> usize {
        self.line
    }

    pub fn update_iter(&mut self) {
        let line = self.line;
        let source = self.source;

        if source.is_second_screen() {
            self.line_iter.ink_line = ink_line_from(line, self.frame_ref.screen1);
            self.line_iter.frame_pixels = &self.frame_ref.frame_cache1.frame_pixels[line];
        }
        else {
            self.line_iter.ink_line = ink_line_from(line, self.frame_ref.screen0);
            self.line_iter.frame_pixels = &self.frame_ref.frame_cache0.frame_pixels[line];
        }

        if source.is_hi_color() {
            self.line_iter.attr_line = ink_line_from(line, self.frame_ref.screen1);
            self.line_iter.frame_colors = &self.frame_ref.frame_cache1.frame_pixels[line];
            self.line_iter.frame_colors_coarse = EMPTY_FRAME_LINE;
        }
        else if source.is_second_screen() {
            self.line_iter.attr_line = attr_line_from(line, self.frame_ref.screen1);
            self.line_iter.frame_colors = &self.frame_ref.frame_cache1.frame_colors[line];
            self.line_iter.frame_colors_coarse = &self.frame_ref.frame_cache1.frame_colors_coarse[line >> 3];
        }
        else {
            self.line_iter.attr_line = attr_line_from(line, self.frame_ref.screen0);
            self.line_iter.frame_colors = &self.frame_ref.frame_cache0.frame_colors[line];
            self.line_iter.frame_colors_coarse = &self.frame_ref.frame_cache0.frame_colors_coarse[line >> 3];
        }
    }
}

impl<'a, V, I> PlusVidFrameDataIterator for ScldFrameProducer<'a, V, I>
    where V: VideoFrame,
          I: Iterator<Item=VideoTsData2>
{
    fn next_line(&mut self) {
        let line = self.line + 1;
        if line < PIXEL_LINES {
            self.line = line;
            self.update_iter();
            self.line_iter.column = 0;
            self.next_change_at.vc -= 1;
        }
    }
}

impl<'a, V, I> Iterator for ScldFrameProducer<'a, V, I>
    where V: VideoFrame,
          I: Iterator<Item=VideoTsData2>
{
    type Item = (u8, u8, Ts);

    fn next(&mut self) -> Option<Self::Item> {
        let htc = col_hts(self.line_iter.column as i16);
        while self.next_change_at.vc <= 0 {
            if self.next_change_at.vc < 0 || self.next_change_at.hc < htc {
                self.source = self.next_source;
                self.update_iter();
                if let Some((next_at, next_source)) = self.source_changes.next()
                            .map(|ts| vtm_next::<V>(ts, self.line as Ts)) {
                    self.next_change_at = next_at;
                    self.next_source = next_source;
                }
                else {
                    self.next_change_at = VFrameTs::max_value();
                    break;
                }
            }
            else {
                break;
            }
        }
        self.line_iter.next()
            .map(|(ink, attr)|
                (ink, attr, htc)
            )
    }
}

#[inline(always)]
fn vtm_next<V: VideoFrame>(vtm: VideoTsData2, line: Ts) -> (VFrameTs<V>, SourceMode) {
    let (mut vts, data): (VFrameTs<V>, u8) = vtm.into();
    vts.vc -= V::VSL_PIXELS.start + line;
    (vts, SourceMode::from_bits_truncate(data))
}

#[inline]
fn col_hts(x: i16) -> i16 {
    ((x >> 1) << 3) + ((x & 1) << 1) + 3
}
