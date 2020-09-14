/*
    zxspectrum-common: High-level ZX Spectrum emulator library example.
    Copyright (C) 2020  Rafal Michalski

    For the full copyright notice, see the lib.rs file.
*/
use spectrusty::z80emu::Cpu;
use spectrusty::chip::{ControlUnit, FrameState};
use spectrusty::video::{Palette, PixelBuffer, BorderColor, Video, VideoFrame};
use crate::config::InterlaceMode;
use super::spectrum::ZxSpectrum;

/// A helper trait for querying and controling certain video aspects of the emulated machine.
///
/// Can be used in a dynamic context.
pub trait VideoControl {
    /// A rendered screen pixel size (horizontal, vertical) measured in real pixels being rendered,
    /// depending on the border size selection and interlace mode.
    fn render_size_pixels(&self) -> (u32, u32);
    /// A target screen pixel size (horizontal, vertical) measured in square pixels being presented
    /// depending on the border size selection and interlace mode.
    fn target_size_pixels(&self) -> (u32, u32);
    /// Returns the current border color.
    fn border_color(&self) -> BorderColor;
    /// Override the border area with the given color.
    fn set_border_color(&mut self, border: BorderColor);
}

/// A helper trait for rendering vide frames of the emulated machine.
///
/// Can be used in a dynamic context.
pub trait VideoBuffer<'a, B: PixelBuffer<'a>, P: Palette<Pixel=B::Pixel>> {
    /// Renders video frame into the provided pixel `buffer`, resizing it when necessary.
    /// Returns pixel size: `(horizontal, vertical)` of the rendered area.
    fn render_video_frame(
        &mut self,
        buffer: &'a mut Vec<u8>) -> (u32, u32);
}

impl<C, U, F> VideoControl for ZxSpectrum<C, U, F>
    where C: Cpu,
          U: Video
{
    fn render_size_pixels(&self) -> (u32, u32) {
        let (w, mut h) = U::render_size_pixels(self.state.border_size);
        if self.state.interlace.is_enabled() {
            h *= 2;
        }
        (w, h)
    }

    fn target_size_pixels(&self) -> (u32, u32) {
        let (w, h) = U::VideoFrame::screen_size_pixels(self.state.border_size);
        let factor = U::pixel_density()
                     .max(if self.state.interlace.is_enabled() { 2 } else { 1 });
        (w*factor, h*factor)
    }

    #[inline]
    fn border_color(&self) -> BorderColor {
        self.ula.border_color()
    }

    #[inline]
    fn set_border_color(&mut self, border: BorderColor) {
        self.ula.set_border_color(border)
    }
}

impl<'a, B, P, C, U, F> VideoBuffer<'a, B, P> for ZxSpectrum<C, U, F>
    where B: PixelBuffer<'a>,
          P: Palette<Pixel=B::Pixel>,
          C: Cpu,
          U: Video + ControlUnit + FrameState
{
    #[inline]
    fn render_video_frame(
        &mut self,
        buffer: &'a mut Vec<u8>) -> (u32, u32)
    {
        let (width, height) = U::render_size_pixels(self.state.border_size);
        let (factor, offset) = match self.state.interlace {
            InterlaceMode::Disabled => (1, 0),
            InterlaceMode::OddFramesFirst => (
                2, self.ula.current_frame() as u32 & 1
            ),
            InterlaceMode::EvenFramesFirst => (
                2, self.ula.current_frame() as u32 & 1 ^ 1
            ),
        };
        let row_stride = (width * factor) as usize * B::pixel_stride();
        let row_offset = (width * offset) as usize * B::pixel_stride();
        let pixel_size = row_stride * height as usize;
        if buffer.len() != pixel_size {
            buffer.resize(pixel_size, u8::default());
        }
        let pixel_buffer = &mut buffer[row_offset..];
        self.ula.render_video_frame::<B, P>(pixel_buffer, row_stride, self.state.border_size);
        (width, height * factor)
    }
}
