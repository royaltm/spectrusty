use std::vec::Drain;

use crate::memory::ZxMemory;
use crate::clock::{VideoTs, VideoTsData2, VideoTsData6, VFrameTsCounter};
use crate::video::{
    RendererPlus, PaletteChange, BorderSize, BorderColor, PixelBuffer, Palette, Video,
    frame_cache::{
        pixel_address_coords, color_address_coords
    }
};
use crate::chip::ControlUnit;
use super::frame_cache::{
    PlusFrameProducer
};
use super::{UlaPlus, UlaPlusInner};

impl<U> Video for UlaPlus<U>
    where U: for<'a> UlaPlusInner<'a> + ControlUnit
{
    const PIXEL_DENSITY: u32 = 2;

    type VideoFrame = U::VideoFrame;
    type Contention = U::Contention;

    #[inline]
    fn border_color(&self) -> BorderColor {
        self.ula.border_color()
    }

    fn set_border_color(&mut self, border: BorderColor) {
        self.change_border_color(border, self.ula.current_video_ts())
    }

    fn render_video_frame<'b, B: PixelBuffer<'b>, P: Palette<Pixel=B::Pixel>>(
            &mut self,
            buffer: &'b mut [u8],
            pitch: usize,
            border_size: BorderSize
        )
    {
        let renderer = self.create_renderer(border_size);
        renderer.render_pixels::<B, P, U::VideoFrame>(buffer, pitch);
    }

    fn current_video_ts(&self) -> VideoTs {
        self.ula.current_video_ts()
    }

    fn visible_screen_bank(&self) -> usize {
        (self.cur_source_mode.is_shadow_bank() ^ self.ula.cur_screen_shadow()).into()
    }

    fn current_video_clock(&self) -> VFrameTsCounter<Self::VideoFrame, Self::Contention> {
        self.ula.current_video_clock()
    }

    fn set_video_ts(&mut self, vts: VideoTs) {
        self.ula.set_video_ts(vts);
    }
}

impl<'a, U> UlaPlus<U>
    where U: UlaPlusInner<'a> + ControlUnit
{
    #[inline]
    pub(super) fn update_frame_cache(&mut self, addr: u16, ts: VideoTs) {
        let (frame_cache, memory_ref) = match addr {
            0x4000..=0x5AFF => match self.ula.page1_screen0_shadow_bank() {
                Some(false) => self.ula.frame_cache_mut_mem_ref(),
                Some(true) => self.ula.shadow_frame_cache_mut_mem_ref(),
                None => return
            }
            0x6000..=0x7AFF => match self.ula.page1_screen1_shadow_bank() {
                Some(false) => (&mut self.sec_frame_cache, self.ula.memory_ref()),
                Some(true) => (&mut self.shadow_sec_frame_cache, self.ula.memory_ref()),
                None => return
            }
            0xC000..=0xDAFF => match self.ula.page3_screen0_shadow_bank() {
                Some(false) => self.ula.frame_cache_mut_mem_ref(),
                Some(true) => self.ula.shadow_frame_cache_mut_mem_ref(),
                None => return
            }
            0xE000..=0xFAFF => match self.ula.page3_screen1_shadow_bank() {
                Some(false) => (&mut self.sec_frame_cache, self.ula.memory_ref()),
                Some(true) => (&mut self.shadow_sec_frame_cache, self.ula.memory_ref()),
                None => return
            }
            _ => return
        };

        if addr & 0x1800 != 0x1800 {
            let coords = pixel_address_coords(addr);
            frame_cache.update_frame_pixels(memory_ref, coords, addr, ts);
        }
        else {
            let coords = color_address_coords(addr);
            frame_cache.update_frame_colors(memory_ref, coords, addr, ts);
        }
    }

    // #[inline(always)]
    // pub(super) fn update_snow_interference(&mut self, ts: VideoTs, ir: u16) {
    //     if self.ula.is_snow_interference(ir) {
    //         if let Some(coords) = U::VideoFrame::snow_interference_coords(ts) {
    //             let (screen0, frame_cache0,
    //                  screen1, frame_cache1
    //             ) = if self.ula.cur_screen_shadow() ^ self.cur_source_mode.is_shadow_bank() {
    //                 let (frame_cache0, memory) = self.ula.shadow_frame_cache_mut_mem_ref();
    //                 (memory.screen_ref(1).unwrap(), frame_cache0,
    //                  memory.screen_ref(3).unwrap(), self.shadow_sec_frame_cache)
    //             }
    //             else {
    //                 let (frame_cache0, memory) = self.ula.frame_cache_mut_mem_ref();
    //                 (memory.screen_ref(0).unwrap(), frame_cache0,
    //                  memory.screen_ref(2).unwrap(), self.sec_frame_cache)
    //             };
    //             frame_cache0.apply_snow_interference(screen0, coords, ir as u8);
    //             frame_cache1.apply_snow_interference(screen1, coords, ir as u8);
    //         }
    //     }
    // }

    fn create_renderer(
            &'a mut self,
            border_size: BorderSize,
        ) -> RendererPlus<'a,
                PlusFrameProducer<'a,
                    U::VideoFrame,
                    Drain<'a, VideoTsData2>,
                    <U as UlaPlusInner<'a>>::ScreenSwapIter>,
                Drain<'a, VideoTsData6>,
                Drain<'a, PaletteChange>>
    {
        let invert_flash = self.ula.current_frame() & 16 != 0;
        let render_mode = self.beg_render_mode;
        let source_mode = self.beg_source_mode;
        let swap_screens = source_mode.is_shadow_bank() ^ self.ula.beg_screen_shadow();
        let (screen_changes,
            memory,
            frame_cache0,
            frame_cache_shadow0
            ) = self.ula.video_render_data_view();
        let (s0p0, s0p1, s1p0, s1p1) = match U::Memory::SCR_BANKS_MAX {
            3 => (0, 1, 2, 3),
            1 => (0, 0, 1, 1),
            _ => panic!("unexpected number of screen banks")
        };
        let screen0        = memory.screen_ref(s0p0).unwrap();
        let screen_shadow0 = memory.screen_ref(s0p1).unwrap();
        let screen1        = memory.screen_ref(s1p0).unwrap();
        let screen_shadow1 = memory.screen_ref(s1p1).unwrap();
        let palette = &mut self.beg_palette;
        let frame_cache1 = &mut self.sec_frame_cache;
        let frame_cache_shadow1 = &mut self.shadow_sec_frame_cache;
        let frame_image_producer = PlusFrameProducer::new(
            swap_screens,
            source_mode,
            screen0, frame_cache0,
            screen1, frame_cache1,
            screen_shadow0, frame_cache_shadow0,
            screen_shadow1, frame_cache_shadow1,
            screen_changes,
            self.source_changes.drain(..));

        RendererPlus {
            render_mode,
            palette,
            frame_image_producer,
            mode_changes: self.mode_changes.drain(..),
            palette_changes: self.palette_changes.drain(..),
            border_size,
            invert_flash
        }
    }
}
