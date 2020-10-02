/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
use crate::clock::{Ts, VFrameTs, VideoTs, VideoTsData2};
use crate::video::{
    VideoFrame,
    frame_cache::PlusVidFrameDataIterator
};
use crate::memory::ScreenArray;
use crate::chip::{
    ula::frame_cache::UlaFrameCache,
    ula128::frame_cache::vc_to_line,
    scld::frame_cache::{
        SourceMode, ScldFrameRef, ScldFrameProducer
    }
};

/// Implements a [PlusVidFrameDataIterator] for ULAplus based Spectrum models.
pub struct PlusFrameProducer<'a, V, IM, IS> {
    scld_frame_prod: ScldFrameProducer<'a, V, IM>,
    shadow_frame: ScldFrameRef<'a, V>,
    screen_changes: IS,
    swap_at: VFrameTs<V>,
}

impl<'a, V, IM, IS> PlusVidFrameDataIterator for PlusFrameProducer<'a, V, IM, IS>
    where V: VideoFrame,
          IM: Iterator<Item=VideoTsData2>,
          IS: Iterator<Item=VideoTs>
{
    fn next_line(&mut self) {
        self.scld_frame_prod.next_line();
        self.swap_at.vc -= 1;
    }
}

impl<'a, V, IM, IS> PlusFrameProducer<'a, V, IM, IS>
    where V: VideoFrame,
          IM: Iterator<Item=VideoTsData2>,
          IS: Iterator<Item=VideoTs>
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
            swap_screens: bool,
            source: SourceMode,
            screen0: &'a ScreenArray,
            frame_cache0: &'a UlaFrameCache<V>,
            screen1: &'a ScreenArray,
            frame_cache1: &'a UlaFrameCache<V>,
            screen_shadow0: &'a ScreenArray,
            frame_cache_shadow0: &'a UlaFrameCache<V>,
            screen_shadow1: &'a ScreenArray,
            frame_cache_shadow1: &'a UlaFrameCache<V>,
            mut screen_changes: IS,
            source_changes: IM
        ) -> Self
    {
        let swap_at = screen_changes.next()
                      .map(|ts| vc_to_line::<V>(ts, 0))
                      .unwrap_or_else(VFrameTs::max_value);
        let mut scld_frame_prod = ScldFrameProducer::new(
            source,
            screen0, frame_cache0,
            screen1, frame_cache1,
            source_changes
        );
        let mut shadow_frame = ScldFrameRef::new(
            screen_shadow0, frame_cache_shadow0,
            screen_shadow1, frame_cache_shadow1,
        );
        if swap_screens {
            scld_frame_prod.swap_frame(&mut shadow_frame);
        }
        PlusFrameProducer {
            scld_frame_prod,
            shadow_frame,
            screen_changes,
            swap_at
        }
    }

    fn swap_frames(&mut self) {
        self.scld_frame_prod.swap_frame(&mut self.shadow_frame);
    }
}

impl<'a, V, IM, IS> Iterator for PlusFrameProducer<'a, V, IM, IS>
    where V: VideoFrame,
          IM: Iterator<Item=VideoTsData2>,
          IS: Iterator<Item=VideoTs>
{
    type Item = (u8, u8, Ts);

    fn next(&mut self) -> Option<Self::Item> {
        while self.swap_at.vc <= 0 {
            if self.swap_at.vc < 0 || self.swap_at.hc < self.scld_frame_prod.peek_hts() {
                self.swap_frames();
                self.swap_at = self.screen_changes.next()
                            .map(|ts| vc_to_line::<V>(ts, self.scld_frame_prod.line() as Ts))
                            .unwrap_or_else(VFrameTs::max_value)
            }
            else {
                break;
            }
        }
        self.scld_frame_prod.next()
    }
}
