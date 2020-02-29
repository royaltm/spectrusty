#![allow(dead_code,unused_variables)]
use std::iter::Peekable;
use crate::clock::{VideoTs, Ts, VideoTsData3};
use crate::video::{*, frame_cache::*};
// 0, 0, 0, 21, 21, 201, 202, 33, 33, 203, 38, 203, 44, 203, 44, 47, 204, 204, 205, 205, 53, 205, 205, 205,
// 0, 0, 0, 27, 27, 251, 252, 41, 41, 252, 47, 252, 55, 253, 55, 59, 254, 254, 255, 255, 65, 255, 255, 255
#[allow(clippy::unreadable_literal,clippy::excessive_precision)]
pub static PALETTE: [(f32,f32,f32);16] = [
    (0.0,0.0,0.0),
    (0.08235294117647059,0.08235294117647059,0.788235294117647),
    (0.792156862745098,0.12941176470588237,0.12941176470588237),
    (0.796078431372549,0.14901960784313725,0.796078431372549),
    (0.17254901960784313,0.796078431372549,0.17254901960784313),
    (0.1843137254901961,0.8,0.8),
    (0.803921568627451,0.803921568627451,0.20784313725490197),
    (0.803921568627451,0.803921568627451,0.803921568627451),
    (0.0,0.0,0.0),
    (0.10588235294117647,0.10588235294117647,0.984313725490196),
    (0.9882352941176471,0.1607843137254902,0.1607843137254902),
    (0.9882352941176471,0.1843137254901961,0.9882352941176471),
    (0.21568627450980393,0.9921568627450981,0.21568627450980393),
    (0.23137254901960785,0.996078431372549,0.996078431372549),
    (1.0,1.0,0.2549019607843137),
    (1.0,1.0,1.0)];

const PIXEL_LINES: usize = 192;
const FLASH_MASK : u8 = 0b1000_0000;
const BRIGHT_MASK: u8 = 0b0100_0000;
const INK_MASK   : u8 = 0b0000_0111;
const PAPER_MASK : u8 = 0b0011_1000;

#[derive(Debug)]
pub struct Renderer<FIP, BI> {
    /// A border color value 0..=7 at the beginning of the frame.
    pub border: usize,
    /// An ink/paper and attributes producer.
    pub frame_image_producer: FIP,
    /// Changes to border color registered at the specified t-state.
    pub border_changes: BI,
    /// Determines the size of rendered screen.
    pub border_size: BorderSize,
    /// Flash state.
    pub invert_flash: bool
}

impl<FIP, BI> Renderer<FIP, BI>
    where FIP: FrameImageProducer,
          BI: Iterator<Item=VideoTsData3>,
{
    #[inline(always)]
    pub fn render_pixels<B: PixelBuffer, C: VideoFrame>(self, buffer: &mut [u8], pitch: usize) {
        let Renderer {
            mut frame_image_producer,
            mut border,
            border_changes,
            border_size,
            invert_flash
        } = self;

        let border_top = C::border_top_vsl_iter(border_size);
        let mut border_changes = border_changes.peekable();
        let mut line_chunks_vc = buffer.chunks_mut(pitch)
                                    .zip(border_top.start..C::border_bot_vsl_iter(border_size).end);

        for (rgb_line, vc) in line_chunks_vc.by_ref().take(border_top.len()) {
            Self::render_border_line::<B, C>(rgb_line, &mut border_changes, &mut border, vc, border_size);
        }

        for (rgb_line, vc) in line_chunks_vc.by_ref().take(PIXEL_LINES) {
            Self::render_pixel_line::<B, C>(rgb_line, &mut frame_image_producer,
                    &mut border_changes, &mut border, invert_flash, vc, border_size);
            frame_image_producer.next_line();
        }

        for (rgb_line, vc) in line_chunks_vc {
            Self::render_border_line::<B, C>(rgb_line, &mut border_changes, &mut border, vc, border_size);
        }
    }

    fn render_border_line<B, C>(rgb_line: &mut [u8], border_changes: &mut Peekable<BI>, border: &mut usize, vc: Ts, border_size: BorderSize)
    where B: PixelBuffer, C: VideoFrame
    {
        let mut writer = rgb_line.iter_mut();
        let mut brd_pixel = PixelRgb::from_palette(*border, &PALETTE);
        let mut ts = VideoTs::new(vc, 0);
        for hts in C::border_whole_line_hts_iter(border_size) {
            ts.hc = hts;
            Self::render_border_pixels::<B,_>(&mut writer, border_changes, &mut brd_pixel, border, ts);
        }
    }

    #[inline]
    fn render_border_pixels<'w, B, I>(writer: &mut I, border_peek: &mut Peekable<BI>, brd_pixel: &mut PixelRgb, border: &mut usize, ts: VideoTs)
    where B: PixelBuffer, I: Iterator<Item = &'w mut u8>
    {
            while let Some(tsc) = border_peek.peek().map(|&t| VideoTs::from(t)) {
                if tsc < ts {
                    let brd = border_peek.next().unwrap().into_data();
                    *border = brd as usize;
                    *brd_pixel = PixelRgb::from_palette(*border, &PALETTE);
                }
                else {
                    break;
                }
            }
            B::put_pixels(writer, *brd_pixel, 8);
    }

    #[allow(clippy::too_many_arguments)]
    #[inline(always)]
    fn render_pixel_line<'a, B: PixelBuffer, C: VideoFrame>(
                rgb_line: &mut [u8],
                line_iter: &mut FIP,
                border_changes: &mut Peekable<BI>,
                border: &mut usize,
                invert_flash: bool,
                vc: Ts,
                border_size: BorderSize
            )
    {
        let mut writer = rgb_line.iter_mut();
        let mut brd_pixel = PixelRgb::from_palette(*border, &PALETTE);
        let mut ts = VideoTs::new(vc, 0);

        for hts in C::border_left_hts_iter(border_size) {
            ts.hc = hts;
            Self::render_border_pixels::<B,_>(&mut writer, border_changes, &mut brd_pixel, border, ts);
        }

        for PixelPack8 { ink, attr } in line_iter {
            put_8pixels_ink_attr::<B,_>(&mut writer, ink, attr, invert_flash);
        }

        let mut ts = VideoTs::new(vc, 0);

        for hts in C::border_right_hts_iter(border_size) {
            ts.hc = hts;
            Self::render_border_pixels::<B,_>(&mut writer, border_changes, &mut brd_pixel, border, ts);
        }
    }
}

#[inline(always)]
fn put_8pixels_ink_attr<'a, B, I>(writer: &mut I, ink: u8, attr: u8, invert_flash: bool)
where B: PixelBuffer, I: Iterator<Item = &'a mut u8>
{
    let mut pixels = if invert_flash && (attr & FLASH_MASK) != 0  { !ink } else { ink };
    let ink_color = if (attr & BRIGHT_MASK) != 0 { attr & INK_MASK | 8 } else { attr & INK_MASK } as usize;
    let paper_color = ((attr & (BRIGHT_MASK|PAPER_MASK)) >> 3) as usize;
    for _ in 0..8 {
        pixels = pixels.rotate_left(1);
        let color = if pixels & 1 != 0 { ink_color } else { paper_color };
        let pixel = PixelRgb::from_palette(color, &PALETTE);
        B::put_pixel(writer, pixel);
    }
}
