// cargo +nightly bench --bench synth -- --nocapture
#![feature(test)]
extern crate test;
use core::ops::AddAssign;
use test::{black_box, Bencher, stats::Summary};

use zxspecemu::clock::*;
use zxspecemu::audio::sample::*;
use zxspecemu::audio::*;
use zxspecemu::audio::synth::*;

pub const SAMPLE_RATE: u32 = 48000;
pub const FRAME_TSTATES: FTs = 70908;
pub const CPU_HZ: u32 = 3_546_900;

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
    let time_rate = f64::time_rate(SAMPLE_RATE, CPU_HZ);
    let frame_time = time_rate.at_timestamp(FRAME_TSTATES);
    blep.ensure_frame_time(frame_time, time_rate.at_timestamp(1));
    let Summary { median, .. } = ben.bench(|ben| {
        let mut time = 0;
        let mut sample_buf: Vec<S> = Vec::new();
        let mut delta = T::max_pos_amplitude().mul_norm(T::from_sample(0.5));
        ben.iter(|| {
            for _ in 0..50 {
                while time < FRAME_TSTATES {
                    delta = -delta;
                    blep.add_step(0, time_rate.at_timestamp(time), delta);
                    time += 32;
                }
                time -= FRAME_TSTATES;
                let nsamples = blep.end_frame(time_rate.at_timestamp(FRAME_TSTATES));
                sample_buf.resize(nsamples, S::center());
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
