/*
    Copyright (C) 2020-2022  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
use core::marker::PhantomData;
use std::iter::Peekable;
use crate::clock::{VideoTs, Ts, VideoTsData3};
use crate::video::{
    BorderColor, BorderSize, PixelBuffer, Palette, VideoFrame,
    frame_cache::{PIXEL_LINES, VideoFrameDataIterator}
};

pub const FLASH_MASK : u8 = 0b1000_0000;
pub const BRIGHT_MASK: u8 = 0b0100_0000;
pub const INK_MASK   : u8 = 0b0000_0111;
pub const PAPER_MASK : u8 = 0b0011_1000;

/// Implements a method to render an image of a video frame for classic ZX Spectrum low resolution modes.
#[derive(Debug)]
pub struct Renderer<VD, BI> {
    /// A border color at the beginning of the frame.
    pub border: BorderColor,
    /// An iterator of ink/paper pixels and attributes by line.
    pub frame_image_producer: VD,
    /// Changes to border color registered with timestamps.
    pub border_changes: BI,
    /// Determines the size of the rendered screen.
    pub border_size: BorderSize,
    /// Flash state.
    pub invert_flash: bool
}

struct Worker<'a, VD,
                  BI: Iterator<Item=VideoTsData3>,
                  B: PixelBuffer<'a>,
                  P: Palette<Pixel=B::Pixel>,
                  V: VideoFrame>
{
    border_pixel: B::Pixel,
    frame_image_producer: VD,
    border_changes: Peekable<BI>,
    border_size: BorderSize,
    invert_flash: bool,
    _palette: PhantomData<P>,
    _vframe: PhantomData<V>,
}

impl<VD, BI> Renderer<VD, BI>
    where VD: VideoFrameDataIterator,
          BI: Iterator<Item=VideoTsData3>,
{
    #[inline(never)]
    pub fn render_pixels<'a, B: PixelBuffer<'a>, P: Palette<Pixel=B::Pixel>, V: VideoFrame>(
            self,
            buffer: &'a mut [u8],
            pitch: usize
        )
    {
        let Renderer {
            border,
            frame_image_producer,
            border_changes,
            border_size,
            invert_flash
        } = self;

        let border_pixel = P::get_pixel(border.into());
        let border_changes = border_changes.peekable();
        let border_top = V::border_top_vsl_iter(border_size);
        let border_bot = V::border_bot_vsl_iter(border_size);
        let mut line_chunks_vc = buffer.chunks_mut(pitch)
                                       .zip(border_top.start..border_bot.end);
        let mut worker: Worker<VD, BI, B, P, V> = Worker {
            border_pixel,
            frame_image_producer,
            border_changes,
            border_size,
            invert_flash,
            _palette: PhantomData,
            _vframe: PhantomData,
        };

        // render top border
        for (rgb_line, vc) in line_chunks_vc.by_ref().take(border_top.len()) {
            worker.render_border_line(rgb_line, vc);
        }
        // render ink/paper area with left and right border
        for (rgb_line, vc) in line_chunks_vc.by_ref().take(PIXEL_LINES) {
            worker.render_ink_paper_line(rgb_line, vc);
            worker.frame_image_producer.next_line();
        }
        // render bottom border
        for (rgb_line, vc) in line_chunks_vc {
            worker.render_border_line(rgb_line, vc);
        }
    }
}

impl<'a, VD, BI, B, P, V> Worker<'a, VD, BI, B, P, V>
    where P: Palette,
          VD: VideoFrameDataIterator,
          BI: Iterator<Item=VideoTsData3>,
          B: PixelBuffer<'a>,
          P: Palette<Pixel=B::Pixel>,
          V: VideoFrame
{
    #[inline(never)]
    fn render_border_line(
            &mut self,
            rgb_line: &'a mut [u8],
            vc: Ts
        )
    {
        let mut line_buffer = B::from_line(rgb_line);
        let mut ts = VideoTs::new(vc, V::HTS_RANGE.start);
        for hts in V::border_whole_line_hts_iter(self.border_size) {
            ts.hc = hts;
            self.render_border_pixels(&mut line_buffer, ts);
        }
    }

    #[inline(always)]
    fn render_border_pixels(&mut self, line_buffer: &mut B, ts: VideoTs) {
            while let Some(tsc) = self.border_changes.peek().map(|&t| VideoTs::from(t)) {
                if tsc < ts {
                    let border = self.border_changes.next().unwrap().into_data();
                    self.border_pixel = P::get_pixel(border);
                }
                else {
                    break;
                }
            }
            line_buffer.put_pixels(self.border_pixel, 8);
    }

    #[inline(never)]
    fn render_ink_paper_line(&mut self, rgb_line: &'a mut [u8], vc: Ts) {
        let mut line_buffer = B::from_line(rgb_line);
        // left border
        let mut ts = VideoTs::new(vc, V::HTS_RANGE.start);
        for hts in V::border_left_hts_iter(self.border_size) {
            ts.hc = hts;
            self.render_border_pixels(&mut line_buffer, ts);
        }
        // ink/paper pixels
        for (ink_mask, attr) in self.frame_image_producer.by_ref() {
            Self::put_8pixels_ink_attr(&mut line_buffer, ink_mask, attr, self.invert_flash);
        }
        // right border
        for hts in V::border_right_hts_iter(self.border_size) {
            ts.hc = hts;
            self.render_border_pixels(&mut line_buffer, ts);
        }
    }

    #[inline(never)]
    fn put_8pixels_ink_attr(buffer: &mut B, mut ink_mask: u8, attr: u8, invert_flash: bool) {
        if invert_flash && (attr & FLASH_MASK) != 0  {
            ink_mask = !ink_mask;
        };
        let ink_color = if (attr & BRIGHT_MASK) != 0 { attr & INK_MASK | 8 } else { attr & INK_MASK };
        let paper_color = (attr & (BRIGHT_MASK|PAPER_MASK)) >> 3;
        for _ in 0..8 {
            ink_mask = ink_mask.rotate_left(1);
            let color = if ink_mask & 1 != 0 { ink_color } else { paper_color };
            buffer.put_pixel( P::get_pixel(color) );
        }
    }
}
