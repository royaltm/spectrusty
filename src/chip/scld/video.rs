use core::iter::{self, Empty};
use std::vec::Drain;

use crate::memory::PagedMemory8k;
use crate::clock::{VideoTs, VideoTsData2, VideoTsData6, VFrameTsCounter};
use crate::video::{
    RendererPlus, UlaPlusPalette, PaletteChange, BorderSize, BorderColor, PixelBuffer, Palette,
    VideoFrame, Video,
    frame_cache::{
        pixel_address_coords, color_address_coords
    }
};
use crate::chip::ula::UlaMemoryContention;
use super::frame_cache::{
    SourceMode, ScldFrameProducer
};
use super::Scld;

impl<M, D, X, V> Video for Scld<M, D, X, V>
    where M: PagedMemory8k,
          V: VideoFrame
{
    const PIXEL_DENSITY: u32 = 2;
    type VideoFrame = V;
    type Contention = UlaMemoryContention;

    #[inline]
    fn border_color(&self) -> BorderColor {
        self.ula.border_color()
    }

    fn set_border_color(&mut self, border: BorderColor) {
        self.change_border_color(border, self.ula.current_video_ts())
    }

    fn render_video_frame<'a, B: PixelBuffer<'a>, P: Palette<Pixel=B::Pixel>>(
            &mut self,
            buffer: &'a mut [u8],
            pitch: usize,
            border_size: BorderSize
        )
    {
        let mut palette = UlaPlusPalette::default();
        self.create_renderer(border_size, &mut palette)
            .render_pixels::<B, P, V>(buffer, pitch)
    }

    fn current_video_ts(&self) -> VideoTs {
        self.ula.current_video_ts()
    }

    fn current_video_clock(&self) -> VFrameTsCounter<Self::VideoFrame, Self::Contention> {
        self.ula.current_video_clock()
    }

    fn set_video_ts(&mut self, vts: VideoTs) {
        self.ula.set_video_ts(vts);
    }
}


impl<M, D, X, V> Scld<M, D, X, V>
    where M: PagedMemory8k,
          V: VideoFrame
{
    #[inline]
    pub(super) fn update_frame_cache(&mut self, addr: u16, ts: VideoTs) {
        let frame_cache = match addr {
            0x4000..=0x5AFF if self.mem_paged & 4 == 0 => &mut self.ula.frame_cache,
            0x6000..=0x7AFF if self.mem_paged & 8 == 0 => &mut self.sec_frame_cache,
            _ => return
        };

        if addr & 0x1800 != 0x1800 {
            let coords = pixel_address_coords(addr);
            frame_cache.update_frame_pixels(&self.ula.memory, coords, addr, ts);
        }
        else {
            let coords = color_address_coords(addr);
            frame_cache.update_frame_colors(&self.ula.memory, coords, addr, ts);
        }
    }

    fn create_renderer<'a, 'r>(
            &'a mut self,
            border_size: BorderSize,
            palette: &'r mut UlaPlusPalette
        ) -> RendererPlus<'r, ScldFrameProducer<'a, V, Drain<'a, VideoTsData2>>,
                          Drain<'a, VideoTsData6>,
                          Empty<PaletteChange>>
    {
        let render_mode = self.beg_render_mode();
        let screen0 = self.ula.memory.screen_ref(0).unwrap();
        let screen1 = self.ula.memory.screen_ref(1).unwrap();
        let frame_image_producer = ScldFrameProducer::new(
            SourceMode::from_scld_flags(self.beg_ctrl_flags),
            screen0, &self.ula.frame_cache,
            screen1, &self.sec_frame_cache,
            self.source_changes.drain(..));

        RendererPlus {
            render_mode,
            palette,
            frame_image_producer,
            mode_changes: self.mode_changes.drain(..),
            palette_changes: iter::empty(),
            border_size,
            invert_flash: self.ula.frames.0 & 16 != 0
        }
    }
}
