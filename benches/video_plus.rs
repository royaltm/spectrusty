/*
    video_plus: benchmark program for the SPECTRUSTY library.
    Copyright (C) 2020  Rafal Michalski

    This program is free software: you can redistribute it and/or modify
    it under the terms of the GNU General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    This program is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU General Public License for more details.

    You should have received a copy of the GNU General Public License
    along with this program.  If not, see <https://www.gnu.org/licenses/>.

    Author contact information: see Cargo.toml file, section [package.authors].
*/
// cargo +nightly bench --bench video_plus -- --nocapture
#![feature(test)]
extern crate test;
use test::{black_box, Bencher, stats::Summary};

use spectrusty::clock::*;
use spectrusty::video::{*, pixel::*};
use spectrusty::chip::ula::{*, frame_cache::UlaFrameCache};
use spectrusty::chip::scld::frame_cache::*;
use spectrusty::memory::{ScreenArray, SCREEN_SIZE};
use rand::prelude::*;

type PixelBuf<'a> = PixelBufA24<'a>;
type SpectrumPal = SpectrumPalRGB24;
// type PixelBuf<'a> = PixelBufA32<'a>;
// type SpectrumPal = SpectrumPalRGBA32;
// type PixelBuf<'a> = PixelBufP32<'a>;
// type SpectrumPal = SpectrumPalR8G8B8A8;
// type PixelBuf<'a> = PixelBufP8<'a>;
// type SpectrumPal = SpectrumPalR3G3B2;
// type PixelBuf<'a> = PixelBufP16<'a>;
// type SpectrumPal = SpectrumPalR5G6B5;

#[bench]
fn bench_empty(ben: &mut Bencher) {
    let mode_changes = Vec::new();
    let palette_changes = Vec::new();
    let source_changes = Vec::new();
    let frame_cache0 = Default::default();
    let frame_cache1 = Default::default();
    bench_video::<UlaVideoFrame>(ben, mode_changes, palette_changes, source_changes, frame_cache0, frame_cache1);
}

#[bench]
fn bench_frame_full_coarse(ben: &mut Bencher) {
    let mode_changes = Vec::new();
    let palette_changes = Vec::new();
    let source_changes = Vec::new();
    let mut frame_cache0: UlaFrameCache<UlaVideoFrame> = Default::default();
    let mut frame_cache1: UlaFrameCache<UlaVideoFrame> = Default::default();
    fill_frame_cache(&mut frame_cache0, true);
    fill_frame_cache(&mut frame_cache1, true);
    bench_video::<UlaVideoFrame>(ben, mode_changes, palette_changes, source_changes, frame_cache0, frame_cache1);
}

#[bench]
fn bench_frame_full(ben: &mut Bencher) {
    let mode_changes = Vec::new();
    let palette_changes = Vec::new();
    let source_changes = Vec::new();
    let mut frame_cache0: UlaFrameCache<UlaVideoFrame> = Default::default();
    let mut frame_cache1: UlaFrameCache<UlaVideoFrame> = Default::default();
    fill_frame_cache(&mut frame_cache0, false);
    fill_frame_cache(&mut frame_cache1, false);
    bench_video::<UlaVideoFrame>(ben, mode_changes, palette_changes, source_changes, frame_cache0, frame_cache1);
}

#[bench]
fn bench_mode_intensive(ben: &mut Bencher) {
    let mut mode_changes = Vec::new();
    let palette_changes = Vec::new();
    let source_changes = Vec::new();
    let frame_cache0 = Default::default();
    let frame_cache1 = Default::default();
    fill_mode_changes(&mut mode_changes, 21, thread_rng().gen_range(0..21));
    bench_video::<UlaVideoFrame>(ben, mode_changes, palette_changes, source_changes, frame_cache0, frame_cache1);
}

#[bench]
fn bench_mode_source_intensive(ben: &mut Bencher) {
    let mut mode_changes = Vec::new();
    let palette_changes = Vec::new();
    let mut source_changes = Vec::new();
    let frame_cache0 = Default::default();
    let frame_cache1 = Default::default();
    fill_mode_changes(&mut mode_changes, 42, thread_rng().gen_range(0..42));
    fill_source_changes(&mut source_changes, 42, thread_rng().gen_range(0..42));
    bench_video::<UlaVideoFrame>(ben, mode_changes, palette_changes, source_changes, frame_cache0, frame_cache1);
}

#[bench]
fn bench_mode_palette_intensive(ben: &mut Bencher) {
    let mut mode_changes = Vec::new();
    let mut palette_changes = Vec::new();
    let source_changes = Vec::new();
    let frame_cache0 = Default::default();
    let frame_cache1 = Default::default();
    fill_mode_changes(&mut mode_changes, 42, thread_rng().gen_range(0..42));
    fill_palette_changes(&mut palette_changes, 42, thread_rng().gen_range(0..42));
    bench_video::<UlaVideoFrame>(ben, mode_changes, palette_changes, source_changes, frame_cache0, frame_cache1);
}

#[bench]
fn bench_all_intensive(ben: &mut Bencher) {
    let mut mode_changes = Vec::new();
    let mut palette_changes = Vec::new();
    let mut source_changes = Vec::new();
    let frame_cache0 = Default::default();
    let frame_cache1 = Default::default();
    fill_mode_changes(&mut mode_changes, 63, thread_rng().gen_range(0..63));
    fill_palette_changes(&mut palette_changes, 63, thread_rng().gen_range(0..63));
    fill_source_changes(&mut source_changes, 63, thread_rng().gen_range(0..63));
    bench_video::<UlaVideoFrame>(ben, mode_changes, palette_changes, source_changes, frame_cache0, frame_cache1);
}

fn fill_mode_changes(mode_changes: &mut Vec<VideoTsData6>, step: usize, start: usize) {
    let mut color = 0;
    let mut rng = thread_rng();
    for ts in (0..UlaVideoFrame::FRAME_TSTATES_COUNT).skip(start).step_by(step) {
        let vts = VFrameTs::<UlaVideoFrame>::from_tstates(ts);
        let mode = match rng.gen_range(0..8) {
            0 => (RenderMode::HI_RESOLUTION).bits()|color,
            1 => (RenderMode::HI_RESOLUTION|RenderMode::GRAYSCALE).bits()|color,
            2 => (RenderMode::PALETTE).bits()|color,
            3 => (RenderMode::PALETTE|RenderMode::GRAYSCALE).bits()|color,
            4 => (RenderMode::GRAYSCALE).bits()|color,
            _ => color
        };
        mode_changes.push((vts, mode).into());
        color = (color + 1) & 7;
    }
}

fn fill_palette_changes(palette_changes: &mut Vec<PaletteChange>, step: usize, start: usize) {
    let mut index = 0;
    let mut rng = thread_rng();
    for ts in (0..UlaVideoFrame::FRAME_TSTATES_COUNT).skip(start).step_by(step) {
        let vts = VFrameTs::<UlaVideoFrame>::from_tstates(ts);
        let color: u8 = rng.gen();
        palette_changes.push((vts.into(), index, color).into());
        index = (index + 1) & 63;
    }
}

fn fill_source_changes(source_changes: &mut Vec<VideoTsData2>, step: usize, start: usize) {
    let mut rng = thread_rng();
    for ts in (0..UlaVideoFrame::FRAME_TSTATES_COUNT).skip(start).step_by(step) {
        let vts = VFrameTs::<UlaVideoFrame>::from_tstates(ts);
        let source: u8 = rng.gen_range(0..3);
        source_changes.push((vts, source).into());
    }
}

fn fill_frame_cache<V: VideoFrame>(frame_cache: &mut UlaFrameCache<V>, coarse: bool) {
    for p in frame_cache.frame_pixels.iter_mut() {
        p.0 = !0;
        for p1 in p.1.iter_mut() {
            *p1 = random();
        }
    }
    if coarse {
        for p in frame_cache.frame_colors_coarse.iter_mut() {
            p.0 = !0;
            for p1 in p.1.iter_mut() {
                *p1 = random();
            }
        }
    }
    else {
        for p in frame_cache.frame_colors.iter_mut() {
            p.0 = !0;
            for p1 in p.1.iter_mut() {
                *p1 = random();
            }
        }
    }
}

fn bench_video<V>(
        ben: &mut Bencher,
        mode_changes: Vec<VideoTsData6>,
        palette_changes: Vec<PaletteChange>,
        source_changes: Vec<VideoTsData2>,
        frame_cache0: UlaFrameCache<V>,
        frame_cache1: UlaFrameCache<V>,
    )
    where V: VideoFrame
{
    let Summary { median, .. } = ben.bench(|ben| {
        let mut screen0: ScreenArray = [0u8;SCREEN_SIZE as usize];
        thread_rng().fill(&mut screen0[..]);
        let mut screen1: ScreenArray = [0u8;SCREEN_SIZE as usize];
        thread_rng().fill(&mut screen1[..]);
        let (w, h) = V::screen_size_pixels(BorderSize::Full);
        let pitch = w as usize * PixelBuf::pixel_stride();
        let mut buffer = vec![0u8;pitch * h as usize];
        let mut invert_flash = false;
        ben.iter(|| {
            for _ in 0..50 {
                invert_flash = !invert_flash;
                let mut palette = UlaPlusPalette::default();
                let frame_image_producer = ScldFrameProducer::new(
                    SourceMode::empty(),
                    &screen0, &frame_cache0,
                    &screen1, &frame_cache1,
                    source_changes.iter().copied()
                );
                let renderer = RendererPlus {
                    render_mode: RenderMode::empty(),
                    palette: &mut palette,
                    frame_image_producer,
                    mode_changes: mode_changes.iter().copied(),
                    palette_changes: palette_changes.iter().copied(),
                    border_size: BorderSize::Full,
                    invert_flash
                };
                renderer.render_pixels::<PixelBuf, SpectrumPal, V>(&mut buffer, pitch);
                black_box(&mut buffer);
            }
        });
        Ok(())
    }).unwrap().unwrap();
    let time = median / 1.0e9;
    eprintln!("frames / s: {:.0}, median time: {} s", 50.0/time, time);
}
