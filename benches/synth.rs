/*
    synth: benchmark program for the SPECTRUSTY library.
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
// cargo +nightly bench --bench synth --features=audio -- --nocapture
#![feature(test)]
extern crate test;
use core::ops::AddAssign;
use test::{black_box, Bencher, stats::Summary};

use spectrusty::clock::*;
use spectrusty::audio::*;
use spectrusty::audio::synth::*;

pub const SAMPLE_RATE: u32 = 48000;
pub const FRAME_TSTATES: FTs = 70908;
pub const CPU_HZ: f64 = 3_546_900.0;

#[bench]
fn bench_blep_f32_to_f32(ben: &mut Bencher) {
    bench_blep::<f32,f32>(ben);
}

#[bench]
fn bench_blep_f32_to_i16(ben: &mut Bencher) {
    bench_blep::<f32,i16>(ben);
}

#[bench]
fn bench_blep_f32_to_u16(ben: &mut Bencher) {
    bench_blep::<f32,u16>(ben);
}

#[bench]
fn bench_blep_i16_to_i16(ben: &mut Bencher) {
    bench_blep::<i16,i16>(ben);
}

#[bench]
fn bench_blep_i16_to_u16(ben: &mut Bencher) {
    bench_blep::<i16,u16>(ben);
}

#[bench]
fn bench_blep_i16_to_f32(ben: &mut Bencher) {
    bench_blep::<i16,f32>(ben);
}

fn bench_blep<T, S>(ben: &mut Bencher)
where T: Copy + Default + AddAssign + MulNorm + FromSample<f32> + SampleDelta + AudioSample + std::ops::Neg<Output=T>,
      S: FromSample<T> + AudioSample
{
    let mut blep = BandLimited::<T>::new(1);
    blep.ensure_frame_time(SAMPLE_RATE, CPU_HZ, FRAME_TSTATES, 0);
    let Summary { median, .. } = ben.bench(|ben| {
        let mut time = 0;
        let mut sample_buf: Vec<S> = Vec::new();
        let mut delta = T::max_pos_amplitude().mul_norm(T::from_sample(0.5));
        ben.iter(|| {
            for _ in 0..50 {
                while time < FRAME_TSTATES {
                    delta = -delta;
                    blep.add_step(0, time, delta);
                    time += 32;
                }
                time -= FRAME_TSTATES;
                let nsamples = Blep::end_frame(&mut blep, FRAME_TSTATES);
                sample_buf.resize(nsamples, S::silence());
                for (sample, pb) in blep.sum_iter(0).zip(sample_buf.iter_mut()) {
                    *pb = sample;
                }
                black_box(&mut sample_buf);
                blep.next_frame();
            }
        });
    }).unwrap();
    let time = median / 1.0e9;
    eprintln!("frames / s: {:.0}, median time: {} s", 50.0/time, time);
}
