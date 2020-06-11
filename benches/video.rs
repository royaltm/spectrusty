// cargo +nightly bench --bench video -- --nocapture
#![feature(test)]
extern crate test;
use test::{black_box, Bencher, stats::Summary};

use spectrusty::clock::*;
use spectrusty::video::{*, pixel::*};
use spectrusty::chip::ula::{*, frame_cache::*};
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
    let frame_cache = Default::default();
    bench_video::<UlaVideoFrame>(ben, border_changes, frame_cache);
}

#[bench]
fn bench_frame_full_coarse(ben: &mut Bencher) {
    let border_changes = Vec::new();
    let mut frame_cache: UlaFrameCache<UlaVideoFrame> = Default::default();
    fill_frame_cache(&mut frame_cache, true);
    bench_video::<UlaVideoFrame>(ben, border_changes, frame_cache);
}

#[bench]
fn bench_frame_full(ben: &mut Bencher) {
    let border_changes = Vec::new();
    let mut frame_cache: UlaFrameCache<UlaVideoFrame> = Default::default();
    fill_frame_cache(&mut frame_cache, false);
    bench_video::<UlaVideoFrame>(ben, border_changes, frame_cache);
}

#[bench]
fn bench_border_intensive(ben: &mut Bencher) {
    let mut border_changes = Vec::new();
    let frame_cache = Default::default();
    fill_border_changes(&mut border_changes, 21, thread_rng().gen_range(0, 21));
    bench_video::<UlaVideoFrame>(ben, border_changes, frame_cache);
}

#[bench]
fn bench_border_rare(ben: &mut Bencher) {
    let mut border_changes = Vec::new();
    let frame_cache = Default::default();
    fill_border_changes(&mut border_changes, 11000, thread_rng().gen_range(0, 11000));
    bench_video::<UlaVideoFrame>(ben, border_changes, frame_cache);
}

fn fill_border_changes(border_changes: &mut Vec<VideoTsData3>, step: usize, start: usize) {
    let mut color = 0;
    for ts in (0..UlaVideoFrame::FRAME_TSTATES_COUNT).skip(start).step_by(step) {
        let vts = UlaVideoFrame::tstates_to_vts(ts);
        border_changes.push((vts, color).into());
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
        frame_cache: UlaFrameCache<V>,
    )
    where V: VideoFrame
{
    let Summary { median, .. } = ben.bench(|ben| {
        let mut screen: ScreenArray = [0u8;SCREEN_SIZE as usize];
        thread_rng().fill(&mut screen[..]);
        let (w, h) = V::screen_size_pixels(BorderSize::Full);
        let pitch = w as usize * PixelBuf::pixel_stride();
        let mut buffer = vec![0u8;pitch * h as usize];
        let mut invert_flash = false;
        ben.iter(|| {
            for _ in 0..50 {
                invert_flash = !invert_flash;
                let renderer = Renderer {
                    border: BorderColor::WHITE,
                    frame_image_producer: UlaFrameProducer::new(&screen, &frame_cache),
                    border_changes: border_changes.iter().copied(),
                    border_size: BorderSize::Full,
                    invert_flash
                };
                renderer.render_pixels::<PixelBuf,SpectrumPal,V>(&mut buffer, pitch);
                black_box(&mut buffer);
            }
        });
    }).unwrap();
    let time = median / 1.0e9;
    eprintln!("frames / s: {:.0}, median time: {} s", 50.0/time, time);
}
