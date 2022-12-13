/*
    video128: benchmark program for the SPECTRUSTY library.
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
// cargo +nightly bench --bench video128 -- --nocapture
#![feature(test)]
extern crate test;
use test::{black_box, Bencher, stats::Summary};

use spectrusty::clock::*;
use spectrusty::video::{*, pixel::*};
use spectrusty::chip::ula::frame_cache::*;
use spectrusty::chip::ula128::{*, frame_cache::*};
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
    let border_changes = Vec::new();
    let frame_cache0 = Default::default();
    let frame_cache1 = Default::default();
    let screen_changes = Vec::new();
    bench_video::<Ula128VidFrame>(ben, border_changes, frame_cache0, frame_cache1, screen_changes);
}

#[bench]
fn bench_frame_full_coarse(ben: &mut Bencher) {
    let border_changes = Vec::new();
    let mut frame_cache0: UlaFrameCache<Ula128VidFrame> = Default::default();
    let mut frame_cache1: UlaFrameCache<Ula128VidFrame> = Default::default();
    fill_frame_cache(&mut frame_cache0, true);
    fill_frame_cache(&mut frame_cache1, true);
    let screen_changes = Vec::new();
    bench_video::<Ula128VidFrame>(ben, border_changes, frame_cache0, frame_cache1, screen_changes);
}

#[bench]
fn bench_frame_full(ben: &mut Bencher) {
    let border_changes = Vec::new();
    let mut frame_cache0: UlaFrameCache<Ula128VidFrame> = Default::default();
    let mut frame_cache1: UlaFrameCache<Ula128VidFrame> = Default::default();
    fill_frame_cache(&mut frame_cache0, false);
    fill_frame_cache(&mut frame_cache1, false);
    let screen_changes = Vec::new();
    bench_video::<Ula128VidFrame>(ben, border_changes, frame_cache0, frame_cache1, screen_changes);
}

#[bench]
fn bench_border_intensive(ben: &mut Bencher) {
    let mut border_changes = Vec::new();
    let frame_cache0 = Default::default();
    let frame_cache1 = Default::default();
    fill_border_changes(&mut border_changes, 21, thread_rng().gen_range(0..21));
    let screen_changes = Vec::new();
    bench_video::<Ula128VidFrame>(ben, border_changes, frame_cache0, frame_cache1, screen_changes);
}

#[bench]
fn bench_border_rare(ben: &mut Bencher) {
    let mut border_changes = Vec::new();
    let frame_cache0 = Default::default();
    let frame_cache1 = Default::default();
    fill_border_changes(&mut border_changes, 11000, thread_rng().gen_range(0..11000));
    let screen_changes = Vec::new();
    bench_video::<Ula128VidFrame>(ben, border_changes, frame_cache0, frame_cache1, screen_changes);
}

#[bench]
fn bench_screen_changes_intensive(ben: &mut Bencher) {
    let border_changes = Vec::new();
    let frame_cache0 = Default::default();
    let frame_cache1 = Default::default();
    let mut screen_changes = Vec::new();
    fill_screen_changes(&mut screen_changes, 21, thread_rng().gen_range(0..21));
    bench_video::<Ula128VidFrame>(ben, border_changes, frame_cache0, frame_cache1, screen_changes);
}

#[bench]
fn bench_screen_changes_rare(ben: &mut Bencher) {
    let border_changes = Vec::new();
    let frame_cache0 = Default::default();
    let frame_cache1 = Default::default();
    let mut screen_changes = Vec::new();
    fill_screen_changes(&mut screen_changes, 11000, thread_rng().gen_range(0..11000));
    bench_video::<Ula128VidFrame>(ben, border_changes, frame_cache0, frame_cache1, screen_changes);
}

#[bench]
fn bench_border_changes_screen_changes_intensive(ben: &mut Bencher) {
    let mut border_changes = Vec::new();
    fill_border_changes(&mut border_changes, 42, thread_rng().gen_range(0..42));
    let frame_cache0 = Default::default();
    let frame_cache1 = Default::default();
    let mut screen_changes = Vec::new();
    fill_screen_changes(&mut screen_changes, 42, thread_rng().gen_range(0..42));
    bench_video::<Ula128VidFrame>(ben, border_changes, frame_cache0, frame_cache1, screen_changes);
}

#[bench]
fn bench_border_changes_screen_changes_rare(ben: &mut Bencher) {
    let mut border_changes = Vec::new();
    fill_border_changes(&mut border_changes, 7000, thread_rng().gen_range(0..7000));
    let frame_cache0 = Default::default();
    let frame_cache1 = Default::default();
    let mut screen_changes = Vec::new();
    fill_screen_changes(&mut screen_changes, 11000, thread_rng().gen_range(0..11000));
    bench_video::<Ula128VidFrame>(ben, border_changes, frame_cache0, frame_cache1, screen_changes);
}

fn fill_border_changes(border_changes: &mut Vec<VideoTsData3>, step: usize, start: usize) {
    let mut color = 0;
    for ts in (0..Ula128VidFrame::FRAME_TSTATES_COUNT).skip(start).step_by(step) {
        let vts = VFrameTs::<Ula128VidFrame>::from_tstates(ts);
        border_changes.push((vts, color).into());
        color = (color + 1) & 7;
    }
}

fn fill_screen_changes(border_changes: &mut Vec<VideoTs>, step: usize, start: usize) {
    let mut color = 0;
    for ts in (0..Ula128VidFrame::FRAME_TSTATES_COUNT).skip(start).step_by(step) {
        let vts = VFrameTs::<Ula128VidFrame>::from_tstates(ts);
        border_changes.push(vts.into());
        color = (color + 1) & 7;
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
        border_changes: Vec<VideoTsData3>,
        frame_cache0: UlaFrameCache<V>,
        frame_cache1: UlaFrameCache<V>,
        screen_changes: Vec<VideoTs>
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
        let mut shadow_screen = false;
        ben.iter(|| {
            shadow_screen = !shadow_screen;
            for _ in 0..50 {
                invert_flash = !invert_flash;
                let producer = Ula128FrameProducer::new(shadow_screen, &screen0, &screen1, &frame_cache0, &frame_cache1,
                                screen_changes.iter().copied());
                let renderer = Renderer {
                    border: BorderColor::WHITE,
                    frame_image_producer: producer,
                    border_changes: border_changes.iter().copied(),
                    border_size: BorderSize::Full,
                    invert_flash
                };
                renderer.render_pixels::<PixelBuf,SpectrumPal,V>(&mut buffer, pitch);
                black_box(&mut buffer);
            }
        });
        Ok(())
    }).unwrap().unwrap();
    let time = median / 1.0e9;
    eprintln!("frames / s: {:.0}, median time: {} s", 50.0/time, time);
}
