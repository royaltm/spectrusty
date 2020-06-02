use core::fmt;
use core::ops::{Deref, DerefMut};
use core::marker::PhantomData;
/*
    frame producer: ([bank5/7,] screen0+ink0/screen1+ink1/screen0+screen1)
    border changes (16 colors) 0-7 applies palette rules, 8-15 no palette (grayscale applies)
    palette changes (lo/hi-res, on/off, mono/color, entry 0..=63 value: u8)
    bit0: palette on/off
    bit1: grayscale on/off
    bit2: hires on/off
*/
use std::iter::Peekable;
use bitflags::bitflags;
use crate::clock::{VideoTs, Ts, VideoTsData6};
// use crate::chip::{TimexCtrlFlags};
use crate::video::{
    BorderSize, PixelBuffer, Palette, VideoFrame,
    frame_cache::{PIXEL_LINES, PlusVidFrameDataIterator},
};
use super::render_pixels::{
    FLASH_MASK,
    BRIGHT_MASK,
    INK_MASK,
    PAPER_MASK,
};

const CLUT_MASK: u8 = 0b1100_0000;

/// The type used for ULAplus GRB/grayscale palette.
pub struct ColorPalette(pub [u8;64]);

bitflags! {
    /// The render mode control flags.
    ///
    /// ```text
    /// g p h b b b
    /// 0 0 0 b b b - (b)order, lo-res, palette off
    /// 1 0 0 b b b - (b)order, lo-res, palette off/grayscale on
    /// 0 1 0 b b b - (b)order, lo-res, palette on
    /// 1 1 0 b b b - (b)order, lo-res, palette on/grayscale on
    /// 0 0 1 i i i - (i)nk, hi-res
    /// 1 0 1 i i i - (i)nk, hi-res/grascale on
    /// 0 1 1 - unknown mode
    /// 1 1 1 - unknown mode
    /// ```
    #[derive(Default)]
    pub struct RenderMode: u8 {
        const MODE_MASK     = 0b111000;
        const COLOR_MASK    = 0b000111;
        const HI_RESOLUTION = 0b001000;
        const PALETTE       = 0b010000;
        const GRAYSCALE     = 0b100000;
        const GRAY_PALETTE  = 0b110000;
        const GRAY_HI_RES   = 0b101000;
        const INDEX_MODE    = 0b000000;
    }
}

/// The type used to record changes of ULAPlus palette entries.
#[derive(Debug,Default,Copy,Clone,PartialEq,Eq,PartialOrd,Ord)]
pub struct PaletteChange {
    /// The video timestamp when the change should be applied.
    pub vts: VideoTs,
    /// The index of palette entry to change: [0, 63].
    pub index: u8,
    /// A value to set the palette entry to.
    pub color: u8
}

impl Default for ColorPalette {
    fn default() -> Self {
        ColorPalette([0u8;64])
    }
}

impl Deref for ColorPalette {
    type Target = [u8;64];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for ColorPalette {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl fmt::Debug for ColorPalette {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.0.iter()).finish()
    }
}

impl RenderMode {
    /// Returns the border or ink bits ORed with hi resolution bit.
    ///
    /// The returned value is in range: [0, 15] and represents the border color: [0, 7]
    /// or a high resolution ink color: [8, 15].
    #[inline]
    pub fn color_index(self) -> u8 {
        (self & (RenderMode::HI_RESOLUTION|RenderMode::COLOR_MASK)).bits()
    }
    #[inline]
    pub fn is_hi_res(self) -> bool {
        self.intersects(RenderMode::HI_RESOLUTION)
    }
    #[inline]
    pub fn is_grayscale(self) -> bool {
        self.intersects(RenderMode::GRAYSCALE)
    }
    #[inline]
    pub fn is_palette(self) -> bool {
        self.intersects(RenderMode::PALETTE)
    }
}

impl From<u8> for RenderMode {
    #[inline]
    fn from(mode: u8) -> Self {
        RenderMode::from_bits_truncate(mode)
    }
}

impl From<VideoTsData6> for RenderMode {
    #[inline]
    fn from(vtsr: VideoTsData6) -> Self {
        RenderMode::from_bits_truncate(vtsr.into_data())
    }
}

// impl From<TimexCtrlFlags> for RenderMode {
//     fn from(flags: TimexCtrlFlags) -> RenderMode {
//         RenderMode::from(
//             (flags & (TimexCtrlFlags::SCREEN_HI_RES)).bits()
//         )
//     }
// }

impl From<&PaletteChange> for VideoTs {
    fn from(vtsp: &PaletteChange) -> Self {
        vtsp.vts
    }
}

impl From<(VideoTs, u8, u8)> for PaletteChange {
    fn from((vts, index, color): (VideoTs, u8, u8)) -> Self {
        PaletteChange::new(vts, index, color)
    }
}

impl PaletteChange {
    #[inline]
    pub fn new(vts: VideoTs, index: u8, color: u8) -> Self {
        PaletteChange { vts, index: index & 63, color }
    }
    /// Updates the color palette and returns an updated index: [0, 63].
    #[inline]
    pub fn update_palette(self, palette: &mut ColorPalette) -> u8 {
        let PaletteChange { mut index, color, .. } = self;
        index &= 63;
        palette[index as usize] = color;
        index
    }
}

/// Implements a method to render the double resolution image of a video frame for ULAplus/SCLD modes.
#[derive(Debug)]
pub struct RendererPlus<'r, VD, MI, PI> {
    /// A render mode at the beginning of the frame (includes the border color).
    pub render_mode: RenderMode,
    /// A palette is being modified during the rendering of pixels.
    pub palette: &'r mut ColorPalette,
    /// An iterator of ink/paper pixels and attributes by line.
    pub frame_image_producer: VD,
    /// Changes to render mode and border color.
    pub mode_changes: MI,
    /// Changes to the palette.
    pub palette_changes: PI,
    /// Determines the size of the rendered screen.
    pub border_size: BorderSize,
    /// Flash state.
    pub invert_flash: bool
}

struct Worker<'r, 'a,
                  MI: Iterator<Item=VideoTsData6>,
                  PI: Iterator<Item=PaletteChange>,
                  B: PixelBuffer<'a>,
                  P: Palette<Pixel=B::Pixel>,
                  V: VideoFrame>
{
    border_pixel: B::Pixel,
    hi_res_pixel: B::Pixel,
    render_mode: RenderMode,
    palette: &'r mut ColorPalette,
    mode_changes: Peekable<MI>,
    palette_changes: Peekable<PI>,
    border_size: BorderSize,
    invert_flash: bool,
    _palette: PhantomData<P>,
    _vframe: PhantomData<V>,
}

impl<'r, VD, MI, PI> RendererPlus<'r, VD, MI, PI>
    where VD: PlusVidFrameDataIterator,
          MI: Iterator<Item=VideoTsData6>,
          PI: Iterator<Item=PaletteChange>
{
    #[inline(never)]
    pub fn render_pixels<'a, B: PixelBuffer<'a>, P: Palette<Pixel=B::Pixel>, V: VideoFrame>(
            self,
            buffer: &'a mut [u8],
            pitch: usize
        )
        where 'r: 'a
    {
        let RendererPlus {
            mut frame_image_producer,
            render_mode,
            palette,
            mode_changes,
            palette_changes,
            border_size,
            invert_flash
        } = self;

        let border_pixel = get_border_pixel::<P>(render_mode, palette);
        let hi_res_pixel = get_hi_res_ink_pixel::<P>(render_mode);
        let mode_changes = mode_changes.peekable();
        let palette_changes = palette_changes.peekable();
        let border_top = V::border_top_vsl_iter(border_size);
        let border_bot = V::border_bot_vsl_iter(border_size);
        let mut line_chunks_vc = buffer.chunks_mut(pitch)
                                       .zip(border_top.start..border_bot.end);

        let mut worker: Worker<MI, PI, B, P, V> = Worker {
            border_pixel,
            hi_res_pixel,
            render_mode,
            palette,
            mode_changes,
            palette_changes,
            border_size,
            invert_flash,
            _palette: PhantomData,
            _vframe: PhantomData
        };

        // render top border
        for (rgb_line, vc) in line_chunks_vc.by_ref().take(border_top.len()) {
            worker.render_border_line(rgb_line, vc);
        }
        // render ink/paper area with left and right border
        for (rgb_line, vc) in line_chunks_vc.by_ref().take(PIXEL_LINES) {
            worker.render_ink_paper_line(rgb_line, &mut frame_image_producer, vc);
            frame_image_producer.next_line();
        }
        // render bottom border
        for (rgb_line, vc) in line_chunks_vc {
            worker.render_border_line(rgb_line, vc);
        }
        // drain palette changes
        for vts_pal in worker.palette_changes {
            vts_pal.update_palette(worker.palette);
        }
    }
}

impl<'r, 'a, MI, PI, B, P, V> Worker<'r, 'a, MI, PI, B, P, V>
    where MI: Iterator<Item=VideoTsData6>,
          PI: Iterator<Item=PaletteChange>,
          B: PixelBuffer<'a>,
          P: Palette<Pixel=B::Pixel>,
          V: VideoFrame
{
    #[inline(always)]
    fn consume_mode_changes(&mut self, ts: VideoTs) {
        while let Some(tsc) = self.mode_changes.peek().map(|t| VideoTs::from(t)) {
            if tsc < ts {
                self.render_mode = self.mode_changes.next().unwrap().into();
                self.border_pixel = get_border_pixel::<P>(self.render_mode, self.palette);
                if self.render_mode.is_hi_res() {
                    self.hi_res_pixel = get_hi_res_ink_pixel::<P>(self.render_mode);
                }                
            }
            else {
                break;
            }
        }
    }

    #[inline(always)]
    fn consume_palette_changes(&mut self, ts: VideoTs) {
        while let Some(tsc) = self.palette_changes.peek().map(|t| VideoTs::from(t)) {
            if tsc < ts {
                let vts_pal = self.palette_changes.next().unwrap();
                if is_border_palette_index( vts_pal.update_palette(self.palette) ) {
                    self.border_pixel = get_border_pixel::<P>(self.render_mode, self.palette);
                }
            }
            else {
                break;
            }
        }
    }

    #[inline(never)]
    fn render_border_line(&mut self, rgb_line: &'a mut [u8], vc: Ts) {
        let mut line_buffer = B::from_line(rgb_line);
        let mut ts = VideoTs::new(vc, V::HTS_RANGE.start);
        for hts in V::border_whole_line_hts_iter(self.border_size) {
            ts.hc = hts;
            self.render_border_pixels(&mut line_buffer, ts);
        }
    }

    #[inline(always)]
    fn render_border_pixels(&mut self, line_buffer: &mut B, ts: VideoTs) {
        self.consume_mode_changes(ts);
        if self.render_mode.is_palette() {
            self.consume_palette_changes(ts);
        }
        line_buffer.put_pixels(self.border_pixel, 8);
    }

    #[inline(never)]
    fn render_ink_paper_line<VD: PlusVidFrameDataIterator>(
            &mut self,
            rgb_line: &'a mut [u8],
            frame_image_producer: &mut VD,
            vc: Ts
        )
    {
        let mut line_buffer = B::from_line(rgb_line);
        // left border
        let mut ts = VideoTs::new(vc, V::HTS_RANGE.start);
        for hts in V::border_left_hts_iter(self.border_size) {
            ts.hc = hts;
            self.render_border_pixels(&mut line_buffer, ts);
        }
        // ink/paper pixels
        for (mut ink_mask, attr, hts) in frame_image_producer {
            ts.hc = hts;
            self.consume_mode_changes(ts);

            if self.render_mode.is_hi_res() {
                Self::put_8pixels_hires(&mut line_buffer, ink_mask, attr, self.hi_res_pixel, self.border_pixel);
            }
            else {
                let (ink, paper) = if self.render_mode.is_palette() {
                    self.consume_palette_changes(ts);
                    let (ink, paper) = attr_to_palette_color(attr, &self.palette);
                    if self.render_mode.is_grayscale() {
                        (P::get_pixel_gray8(ink), P::get_pixel_gray8(paper))
                    }
                    else {
                        (P::get_pixel_grb8(ink), P::get_pixel_grb8(paper))
                    }
                }
                else {
                    if self.invert_flash && (attr & FLASH_MASK) != 0  {
                        ink_mask = !ink_mask
                    };
                    let (ink, paper) = attr_to_ink_paper(attr);
                    if self.render_mode.is_grayscale() {
                        (P::get_pixel_gray4(ink), P::get_pixel_gray4(paper))
                    }
                    else {
                        (P::get_pixel(ink), P::get_pixel(paper))
                    }
                };
                Self::put_8pixels_lores(&mut line_buffer, ink_mask, ink, paper);
            }
        }
        // right border
        for hts in V::border_right_hts_iter(self.border_size) {
            ts.hc = hts;
            self.render_border_pixels(&mut line_buffer, ts);
        }
    }

    #[inline(always)]
    fn put_8pixels_hires(buffer: &mut B, ink_mask0: u8, ink_mask1: u8, ink: B::Pixel, paper: B::Pixel) {
        let mut ink_mask = u16::from_le_bytes([ink_mask1, ink_mask0]);
        for _ in 0..16 {
            ink_mask = ink_mask.rotate_left(1);
            let color = if ink_mask & 1 != 0 { ink } else { paper };
            buffer.put_pixel(color);
        }
    }

    #[inline(always)]
    fn put_8pixels_lores(buffer: &mut B, mut ink_mask: u8, ink: B::Pixel, paper: B::Pixel) {
        for _ in 0..8 {
            ink_mask = ink_mask.rotate_left(1);
            let color = if ink_mask & 1 != 0 { ink } else { paper };
            buffer.put_pixels(color, 2);
        }
    }
}

#[inline(always)]
fn attr_to_ink_paper(attr: u8) -> (u8, u8) {
    let ink = if (attr & BRIGHT_MASK) != 0 { attr & INK_MASK | 8 } else { attr & INK_MASK };
    let paper = (attr & (BRIGHT_MASK|PAPER_MASK)) >> 3;
    (ink, paper)
}

#[inline(always)]
fn attr_to_palette_color(attr: u8, palette: &ColorPalette) -> (u8, u8) {
    let clut = (attr & CLUT_MASK) >> 2;
    let ink_index = clut | (attr & INK_MASK);
    let ink_grb = palette[ink_index as usize];
    let paper_index = clut | ((attr & PAPER_MASK) >> 3) | 8;
    let paper_grb = palette[paper_index as usize];
    (ink_grb, paper_grb)
}

#[inline(always)]
fn get_border_pixel<P: Palette>(render_mode: RenderMode, palette: &ColorPalette) -> P::Pixel {
    let border = render_mode.color_index();
    match render_mode & RenderMode::MODE_MASK {
        RenderMode::PALETTE => P::get_pixel_grb8( palette[(8|border) as usize] ),
        RenderMode::GRAY_PALETTE => P::get_pixel_gray8( palette[(8|border) as usize] ),
        RenderMode::GRAY_HI_RES => P::get_pixel_gray4( border ^ INK_MASK ),
        RenderMode::HI_RESOLUTION => P::get_pixel( border ^ INK_MASK ),
        RenderMode::GRAYSCALE => P::get_pixel_gray4( border ),
        RenderMode::INDEX_MODE => P::get_pixel(border),
        _ => unimplemented!("Unimplemented render mode (palette + hi-res)")
    }
}

#[inline(always)]
fn get_hi_res_ink_pixel<P: Palette>(render_mode: RenderMode) -> P::Pixel {
    let color = render_mode.color_index();
    if render_mode.is_grayscale() {
        P::get_pixel_gray4( color )
    }
    else {
        P::get_pixel( color )
    }
}

#[inline(always)]
fn is_border_palette_index(index: u8) -> bool {
    index & !7 == 8
}
