use std::iter::Peekable;
use crate::clock::{VideoTs, Ts, VideoTsData3};
use crate::video::{
    BorderSize, PixelBuffer, Palette, VideoFrame,
    frame_cache::VideoFrameDataIterator
};

const PIXEL_LINES: usize = 192;
const FLASH_MASK : u8 = 0b1000_0000;
const BRIGHT_MASK: u8 = 0b0100_0000;
const INK_MASK   : u8 = 0b0000_0111;
const PAPER_MASK : u8 = 0b0011_1000;

/// Implements a method to render an image of a video frame for classic ZX Spectrum lo-res modes.
#[derive(Debug)]
pub struct Renderer<VD, BI> {
    /// A border color value 0..=7 at the beginning of the frame.
    pub border: u8,
    /// An iterator of ink/paper pixels and attributes by line.
    pub frame_image_producer: VD,
    /// Changes to border color registered with timestamps.
    pub border_changes: BI,
    /// Determines the size of the rendered screen.
    pub border_size: BorderSize,
    /// Flash state.
    pub invert_flash: bool
}

impl<VD, BI> Renderer<VD, BI>
    where VD: VideoFrameDataIterator,
          BI: Iterator<Item=VideoTsData3>,
{
    #[inline(always)]
    pub fn render_pixels<'a, B: PixelBuffer<'a>, P: Palette<Pixel=B::Pixel>, C: VideoFrame>(
            self,
            buffer: &'a mut [u8],
            pitch: usize
        )
    {
        let Renderer {
            mut frame_image_producer,
            mut border,
            border_changes,
            border_size,
            invert_flash
        } = self;

        let mut border_changes = border_changes.peekable();
        let border_top = C::border_top_vsl_iter(border_size);
        let border_bot = C::border_bot_vsl_iter(border_size);
        let mut line_chunks_vc = buffer.chunks_mut(pitch)
                                       .zip(border_top.start..border_bot.end);
        // render top border
        for (rgb_line, vc) in line_chunks_vc.by_ref().take(border_top.len()) {
            Self::render_border_line::<B, P, C>(rgb_line, &mut border_changes, &mut border, vc, border_size);
        }
        // render ink/paper area with left and right border
        for (rgb_line, vc) in line_chunks_vc.by_ref().take(PIXEL_LINES) {
            Self::render_ink_paper_line::<B, P, C>(rgb_line, &mut frame_image_producer,
                    &mut border_changes, &mut border, invert_flash, vc, border_size);
            frame_image_producer.next_line();
        }
        // render bottom border
        for (rgb_line, vc) in line_chunks_vc {
            Self::render_border_line::<B, P, C>(rgb_line, &mut border_changes, &mut border, vc, border_size);
        }
    }

    fn render_border_line<'a, B: PixelBuffer<'a>, P: Palette<Pixel=B::Pixel>, C: VideoFrame>(
            rgb_line: &'a mut [u8],
            border_changes: &mut Peekable<BI>,
            border: &mut u8,
            vc: Ts,
            border_size: BorderSize
        )
    {
        let mut line_buffer = B::from_line(rgb_line);
        let mut brd_pixel = P::get_pixel(*border);
        let mut ts = VideoTs::new(vc, 0);
        for hts in C::border_whole_line_hts_iter(border_size) {
            ts.hc = hts;
            Self::render_border_pixels::<B,P>(&mut line_buffer, border_changes, &mut brd_pixel, border, ts);
        }
    }

    #[inline]
    fn render_border_pixels<'a, B: PixelBuffer<'a>, P: Palette<Pixel=B::Pixel>>(
            buffer: &mut B,
            border_peek: &mut Peekable<BI>,
            brd_pixel: &mut B::Pixel,
            border: &mut u8,
            ts: VideoTs
        )
    {
            while let Some(tsc) = border_peek.peek().map(|&t| VideoTs::from(t)) {
                if tsc < ts {
                    let brd = border_peek.next().unwrap().into_data();
                    *border = brd;
                    *brd_pixel = P::get_pixel(brd);
                }
                else {
                    break;
                }
            }
            buffer.put_pixels(*brd_pixel, 8);
    }

    #[allow(clippy::too_many_arguments)]
    #[inline(always)]
    fn render_ink_paper_line<'a, B: PixelBuffer<'a>, P: Palette<Pixel=B::Pixel>, C: VideoFrame>(
                rgb_line: &'a mut [u8],
                ink_attr_iter: &mut VD,
                border_changes: &mut Peekable<BI>,
                border: &mut u8,
                invert_flash: bool,
                vc: Ts,
                border_size: BorderSize
            )
    {
        let mut line_buffer = B::from_line(rgb_line);
        let mut brd_pixel = P::get_pixel(*border);
        // left border
        let mut ts = VideoTs::new(vc, 0);
        for hts in C::border_left_hts_iter(border_size) {
            ts.hc = hts;
            Self::render_border_pixels::<B,P>(&mut line_buffer, border_changes, &mut brd_pixel, border, ts);
        }
        // ink/paper pixels
        for (ink, attr) in ink_attr_iter {
            put_8pixels_ink_attr::<B,P>(&mut line_buffer, ink, attr, invert_flash);
        }
        // right border
        for hts in C::border_right_hts_iter(border_size) {
            ts.hc = hts;
            Self::render_border_pixels::<B,P>(&mut line_buffer, border_changes, &mut brd_pixel, border, ts);
        }
    }
}

#[inline(always)]
fn put_8pixels_ink_attr<'a, B: PixelBuffer<'a>, P: Palette<Pixel=B::Pixel>>(
        buffer: &mut B,
        ink: u8,
        attr: u8,
        invert_flash: bool
    )
{
    let mut pixels = if invert_flash && (attr & FLASH_MASK) != 0  { !ink } else { ink };
    let ink_color = if (attr & BRIGHT_MASK) != 0 { attr & INK_MASK | 8 } else { attr & INK_MASK };
    let paper_color = (attr & (BRIGHT_MASK|PAPER_MASK)) >> 3;
    for _ in 0..8 {
        pixels = pixels.rotate_left(1);
        let color = if pixels & 1 != 0 { ink_color } else { paper_color };
        buffer.put_pixel( P::get_pixel(color) );
    }
}
