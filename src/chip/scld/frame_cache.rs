use bitflags::bitflags;
use crate::clock::{Ts, VideoTs, VideoTsData2};
use crate::chip::{
    ScldCtrlFlags,
    ula::{
        frame_cache::{UlaFrameCache, UlaFrameLineIter}
    }
};
use crate::video::{
    VideoFrame, 
    frame_cache::{
        COLUMNS, PIXEL_LINES,
        ink_line_from, attr_line_from,
        PlusVidFrameDataIterator
    }
};
/*
    mode source
    bit2 bit1 bit0   descr     ink  attr
       0    0    0   screen 0  p0   a0   s0.fp  s0.fc/fcc
       0    0    1   screen 1  p1   a1   s1.fp  s1.fc/fcc
       0    1    0   hi-color  p0   p1   s0.fp  s1.fp/dummy
       0    1    1   hi-color  p1   p1   s1.fp  s1.fp/dummy
       1    0    0   hi-res    p0   a0
       1    0    1   hi-res    p1   a1
       1    1    0   hi-res    p0   p1
       1    1    1   hi-res    p1   p1

  Bits 0-2: Screen mode. 000=screen 0, 001=screen 1, 010=hi-colour, 110=hi-res (bank 5)
                         100=screen 0, 101=screen 1, 011=hi-colour, 111=hi-res (bank 7)
    mode source
    bit2 bit1 bit0   descr     ink  attr bank
       0    0    0   screen 0  p0   a0   5    s0.fp  s0.fc/fcc 
       0    0    1   screen 1  p1   a1   5    s2.fp  s2.fc/fcc
       0    1    0   hi-color  p0   p1   5    s0.fp  s2.fp/dummy
       0    1    1   hi-color  p0   p1   7    s1.fp  s3.fp/dummy
       1    0    0   hi-res    p0   a0   7    s1.fp  s1.fc/fcc 
       1    0    1   hi-res    p1   a1   7    s3.fp  s3.fc/fcc 
       1    1    0   hi-res    p0   p1   5    s0.fp  s2.fp/dummy
       1    1    1   hi-res    p0   p1   7    s1.fp  s3.fp/dummy
*/
bitflags! {
    /// Flags determining the source mode for ink masks and attributes.
    ///
    /// ```text
    /// source
    /// attr shad.  mode      ink_mask  attr
    ///    0    0   screen 0  screen 0  screen attr. 0 (8 lines)
    ///    0    1   screen 1  screen 1  screen attr. 1 (8 lines)
    ///    1    0   combined  screen 0  screen 1
    ///    1    1   combined  screen 1  screen 1
    /// ```
    #[derive(Default)]
    pub struct SourceMode: u8 {
        const SHADOW_SCREEN = 0b01;
        const ATTR_HI_COLOR = 0b10;
    }
}

pub struct ScldFrameRef<'a, V> {
    /// primary screen data
    pub screen0: &'a[u8],
    /// secondary screen data
    pub screen1: &'a[u8],
    /// primary screen frame cache
    pub frame_cache0: &'a UlaFrameCache<V>,
    /// secondary screen frame cache
    pub frame_cache1: &'a UlaFrameCache<V>,
}

pub struct ScldFrameProducer<'a, V, I> {
    frame_ref: ScldFrameRef<'a, V>,
    line: usize,
    source: SourceMode,
    next_source: SourceMode,
    next_change_at: VideoTs,
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
    pub fn is_shadow(self) -> bool {
        self.intersects(SourceMode::SHADOW_SCREEN)
    }
    #[inline]
    pub fn is_hi_color(self) -> bool {
        self.intersects(SourceMode::ATTR_HI_COLOR)
    }
}

impl From<u8> for SourceMode {
    #[inline]
    fn from(mode: u8) -> Self {
        SourceMode::from_bits_truncate(mode)
    }
}

impl From<VideoTsData2> for SourceMode {
    #[inline]
    fn from(vtsm: VideoTsData2) -> Self {
        SourceMode::from_bits_truncate(vtsm.into_data())
    }
}

impl From<ScldCtrlFlags> for SourceMode {
    fn from(flags: ScldCtrlFlags) -> SourceMode {
        SourceMode::from(
            (flags & ScldCtrlFlags::SCREEN_SOURCE_MASK).bits()
        )
    }
}

impl<'a, V> ScldFrameRef<'a, V> {
    pub fn new(
            screen0: &'a[u8],
            frame_cache0: &'a UlaFrameCache<V>,
            screen1: &'a[u8],
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
            screen0: &'a[u8],
            frame_cache0: &'a UlaFrameCache<V>,
            screen1: &'a[u8],
            frame_cache1: &'a UlaFrameCache<V>,
            mut source_changes: I
        ) -> Self
    {
        let line = 0;
        let line_iter = EMPTY_LINE_ITER;
        let (next_change_at, next_source) = source_changes.next()
                    .map(|tsm| vtm_next::<V>(tsm, line as Ts))
                    .unwrap_or_else(||
                        ( V::vts_max(), source )
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

        if source.is_shadow() {
            self.line_iter.ink_line = ink_line_from(line, &self.frame_ref.screen1);
            self.line_iter.frame_pixels = &self.frame_ref.frame_cache1.frame_pixels[line];
        }
        else {
            self.line_iter.ink_line = ink_line_from(line, &self.frame_ref.screen0);
            self.line_iter.frame_pixels = &self.frame_ref.frame_cache0.frame_pixels[line];
        }

        if source.is_hi_color() {
            self.line_iter.attr_line = ink_line_from(line, &self.frame_ref.screen1);
            self.line_iter.frame_colors = &self.frame_ref.frame_cache1.frame_pixels[line];
            self.line_iter.frame_colors_coarse = EMPTY_FRAME_LINE;
        }
        else if source.is_shadow() {
            self.line_iter.attr_line = attr_line_from(line, &self.frame_ref.screen1);
            self.line_iter.frame_colors = &self.frame_ref.frame_cache1.frame_colors[line];
            self.line_iter.frame_colors_coarse = &self.frame_ref.frame_cache1.frame_colors_coarse[line >> 3];
        }
        else {
            self.line_iter.attr_line = attr_line_from(line, &self.frame_ref.screen0);
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
                    self.next_change_at = V::vts_max();
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
fn vtm_next<V: VideoFrame>(vtm: VideoTsData2, line: Ts) -> (VideoTs, SourceMode) {
    let (mut vts, data) = vtm.into();
    vts.vc -= V::VSL_PIXELS.start + line;
    (vts, data.into())
}

#[inline]
fn col_hts(x: i16) -> i16 {
    ((x >> 1) << 3) + ((x & 1) << 1) + 3
}
