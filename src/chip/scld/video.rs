use core::iter::{self, Empty};
// use core::ops::Range;
use std::vec::Drain;

// #[cfg(feature = "snapshot")]
// use serde::{Serialize, Deserialize};
use crate::memory::PagedMemory8k;
use crate::clock::{VideoTs, VideoTsData2, VideoTsData6};
use crate::video::{
    RendererPlus, ColorPalette, PaletteChange, BorderSize, BorderColor, PixelBuffer, Palette,
    VideoFrame, Video,
    // CellCoords, MAX_BORDER_SIZE,
    frame_cache::{
        pixel_address_coords, color_address_coords
    }
};
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
        let mut palette = ColorPalette::default();
        self.create_renderer(border_size, &mut palette)
            .render_pixels::<B, P, V>(buffer, pitch)
    }

    fn current_video_ts(&self) -> VideoTs {
        self.ula.current_video_ts()
    }
}

impl<M, D, X, V> Scld<M, D, X, V>
    where M: PagedMemory8k,
          V: VideoFrame
{
    #[inline]
    pub(super) fn update_frame_cache(&mut self, addr: u16, ts: VideoTs) {
        match addr {
            0x4000..=0x57FF if self.mem_paged & 4 == 0 => {
                let coords = pixel_address_coords(addr);
                self.ula.frame_cache.update_frame_pixels(&self.ula.memory, coords, addr, ts);
            }
            0x5800..=0x5AFF if self.mem_paged & 4 == 0 => {
                let coords = color_address_coords(addr);
                self.ula.frame_cache.update_frame_colors(&self.ula.memory, coords, addr, ts);
            }
            0x6000..=0x77FF if self.mem_paged & 8 == 0 => {
                let coords = pixel_address_coords(addr);
                self.sec_frame_cache.update_frame_pixels(&self.ula.memory, coords, addr, ts);
            }
            0x7800..=0x7AFF if self.mem_paged & 8 == 0 => {
                let coords = color_address_coords(addr);
                self.sec_frame_cache.update_frame_colors(&self.ula.memory, coords, addr, ts);
            }
            _ => {}
        }
    }

    fn create_renderer<'a, 'r>(
            &'a mut self,
            border_size: BorderSize,
            palette: &'r mut ColorPalette
        ) -> RendererPlus<'r, ScldFrameProducer<'a, V, Drain<'a, VideoTsData2>>,
                          Drain<'a, VideoTsData6>,
                          Empty<PaletteChange>>
    {
        let render_mode = self.beg_render_mode();
        let screen0 = &self.ula.memory.screen_ref(0).unwrap();
        let screen1 = &self.ula.memory.screen_ref(1).unwrap();
        let frame_image_producer = ScldFrameProducer::new(
            SourceMode::from(self.beg_ctrl_flags),
            screen0, &self.ula.frame_cache,
            screen1, &self.sec_frame_cache,
            self.source_changes.drain(..));
        // print!("render: {} {:?}", screen_bank, screen.as_ptr());
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
