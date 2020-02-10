//! Band-limited steps with adjustable high-pass and low-pass filtering, for efficient band-limited synthesis.
//!
//! For original implementation refer to the web site:
//!
//! http://www.slack.net/~ant/bl-synth
//!
//! TODO: re-implement the more efficient integer based blip buffer, this implementation is just enough to get going
#![allow(unused_imports)]
use core::marker::PhantomData;
use core::{iter::{Skip, StepBy}, slice::Iter};
use core::num::NonZeroUsize;
use core::cell::Cell;
use core::ops::{AddAssign, Add};
use core::convert::TryFrom;
use super::{SampleDelta, Blep, sample::{IntoSample, FromSample, MulNorm}};

const PI2: f64 = core::f64::consts::PI * 2.0;
/// number of phase offsets to sample band-limited step at
const PHASE_COUNT: usize = 32;
/// number of samples in each final band-limited step
const STEP_WIDTH: usize = 24;

pub trait BandLimOpt {
    /// lower values filter more high frequency
    const LOW_PASS: f64 = 0.999;
    /// lower values filter more low frequency
    const HIGH_PASS: f32 = 0.999;
}

pub struct BandLimHiFi;
impl BandLimOpt for BandLimHiFi {}

pub struct BandLimLowTreb;
impl BandLimOpt for BandLimLowTreb {
    const LOW_PASS: f64 = 0.899;
}

pub struct BandLimLowBass;
impl BandLimOpt for BandLimLowBass {
    const HIGH_PASS: f32 = 0.899;
}

pub struct BandLimNarrow;
impl BandLimOpt for BandLimNarrow {
    const LOW_PASS: f64 = 0.899;
    const HIGH_PASS: f32 = 0.899;
}

pub struct BandLimited<T, O=BandLimHiFi> {
    steps: [[T; STEP_WIDTH]; PHASE_COUNT],
    diffs: Vec<T>,
    channels: NonZeroUsize,
    frame_time: f64,
    start_time: f64,
    sums: Box<[(T, Cell<Option<T>>)]>,
    last_nsamples: Option<usize>,
    _options: PhantomData<O>
}

impl<T: Copy + Default, O> BandLimited<T, O> {
    pub fn reset(&mut self) {
        for d in self.diffs.iter_mut() { *d = T::default(); }
        for s in self.sums.iter_mut() { *s = (T::default(), Cell::default()); }
        self.last_nsamples = None;
        self.start_time = 0.0;
    }

    /// time unit is one sample (1.0 = 1 sample): T-States / CPU_HZ * sample_rate
    /// `frame_time` should specify how many samples (including a fraction) fit in the average audio frame
    /// `margin_time` specifies how much (a maximum number of samples) a single audio frame can exceed the average frame size
    pub fn set_frame_time(&mut self, frame_time: f64, margin_time: f64) {
        let max_samples = (frame_time + margin_time).ceil() as usize;
        let required_len = (max_samples + STEP_WIDTH + 1) * self.channels.get();
        // eprintln!("max_samples {} required_len {} frame_time {}", max_samples, required_len, frame_time);
        if self.diffs.len() != required_len {
            self.diffs.resize(required_len, T::default());
            self.diffs.shrink_to_fit();
        }
        self.frame_time = frame_time;
    }

    pub fn is_frame_ended(&mut self) -> bool {
        self.last_nsamples.is_some()
    }

    // time_end should be > than the max time added via add_step
    pub fn end_frame(&mut self, time_end: f64) -> usize {
        if self.last_nsamples.is_none() {
            let samples = (time_end - self.start_time).trunc();
            let num_samples = samples as usize;
            self.last_nsamples = Some(num_samples);
            num_samples
        }
        else {
            panic!("BandLimited frame already over");
        }
    }

    #[inline]
    fn prepare_next_frame(&mut self, num_samples: usize) {
        let channels = self.channels.get();
        let chans_nsamples = num_samples*channels;
        let chans_step_width = STEP_WIDTH*channels;
        self.diffs.copy_within(chans_nsamples..chans_nsamples+chans_step_width, 0);
        for p in self.diffs[chans_step_width..chans_nsamples+chans_step_width].iter_mut() {
            *p = T::default();
        }
    }
}

impl<T,O> BandLimited<T,O>
where T: Copy + Default + AddAssign + MulNorm + FromSample<f32>,
      O: BandLimOpt,
      // f32: FromSample<T>,
{
    /// Panics if channels is 0.
    pub fn new(channels: usize) -> Self {
        let channels = NonZeroUsize::new(channels).expect("BandLimited: channels should be 1 or more");
        // Generate master band-limited step by adding sine components of a square wave
        let mut steps = [[T::default();STEP_WIDTH];PHASE_COUNT];
        const MASTER_SIZE: usize = STEP_WIDTH * PHASE_COUNT;
        let mut master = [0.5f64;MASTER_SIZE];
        let mut gain: f64 = 0.5 / 0.777; // adjust normal square wave's amplitude of ~0.777 to 0.5
        const SINE_SIZE: usize = 256 * PHASE_COUNT + 2;
        let max_harmonic: usize = SINE_SIZE / 2 / PHASE_COUNT;
        for h in (1..=max_harmonic).step_by(2) {
            let amplitude: f64 = gain / h as f64;
            let to_angle: f64 = PI2 / SINE_SIZE as f64 * h as f64;
            // println!("h: {} amp: {} ang: {}", h, amplitude, to_angle);

            for (i, m) in master.iter_mut().enumerate() {
                *m += ( (i as isize - MASTER_SIZE as isize / 2) as f64 * to_angle ).sin() * amplitude;
            }
            gain *= O::LOW_PASS;
        }
        // Sample master step at several phases
        for (phase, step) in steps.iter_mut().enumerate() {
            let mut error: f64 = 1.0;
            let mut prev: f64 = 0.0;
            for (i, s) in step.iter_mut().enumerate() {
                let cur: f64 = master[i * PHASE_COUNT + (PHASE_COUNT - 1 - phase)];
                let delta: f64 = cur - prev;
                error -= delta;
                prev = cur;
                *s = T::from_sample(delta as f32);
            }
            // each delta should total 1.0
            // println!("PHASE: {} sum: {}", phase, step.iter().copied().fold(0.0, |acc, x| acc + f32::from_sample(x)));
            // println!("{:?}", &step[STEP_WIDTH / 2 - 1..STEP_WIDTH / 2 + 2]);
            // let max = step.iter().max().unwrap();
            // let index = step.iter().position(|x| x == max).unwrap();
            // println!("PHASE: {} {} {}  err: {}", phase, max, index, T::from_sample(error as f32));
            step[STEP_WIDTH / 2    ] += T::from_sample((error * 0.5) as f32);
            if phase < 16 {
                step[STEP_WIDTH / 2 - 1] += T::from_sample((error * 0.5) as f32);
            }
            else {
                step[STEP_WIDTH / 2 + 1] += T::from_sample((error * 0.5) as f32);
            }
            // println!("{:?} {}", &step[STEP_WIDTH / 2 - 1..STEP_WIDTH / 2], step.iter().max().unwrap());
            // println!("PHASE: {} sum: {}", phase, step.iter().copied().fold(0.0, |acc, x| acc + f32::from_sample(x)));
            // for (i, m) in step.iter().enumerate() {
            //     println!("{}: {}", i, "@".repeat((50.0 + f32::from_sample(*m) * 100.0).round() as usize));
            //     // println!("{}", (*m * 32768.0).trunc() as i16);
            // }
        }

        BandLimited {
            steps,
            diffs: Vec::new(),
            channels,
            frame_time: 0.0,
            start_time: 0.0,
            sums: vec![(T::default(), Cell::default()); channels.get()].into_boxed_slice(),
            last_nsamples: None,
            _options: PhantomData
        }
    }

    pub fn next_frame(&mut self) {
        let num_samples = self.last_nsamples.take().expect("BandLimited frame not ended");
        self.start_time += num_samples as f64 - self.frame_time;
        for (channel, (sum_tgt, sum_end)) in self.sums.iter_mut().enumerate() {
            *sum_tgt = match sum_end.take() {
                Some(sum) => sum,
                None => {
                    let channels = self.channels.get();
                    let mut sum = *sum_tgt;
                    for diff in self.diffs[..num_samples*channels].iter().skip(channel).step_by(channels) {
                        sum = sum.saturating_add(*diff)
                                 .mul_norm(T::from_sample(O::HIGH_PASS));
                    }
                    sum
                }
            };
        }
        self.prepare_next_frame(num_samples);
    }
}

impl<T,O> BandLimited<T,O>
where T: Copy + MulNorm + FromSample<f32>,
      O: BandLimOpt
{
    // pub fn sum_iter<'a>(&'a mut self, channel: usize) -> BandLimitedSumIter<'a, StepBy<Skip<Iter<'a, f32>>>> {
    // pub fn sum_iter<'a>(&'a mut self, channel: usize) -> BandLimitedSumIter<'a, impl Iterator<Item=&'a f32>> {
    pub fn sum_iter<'a, S: 'a>(&'a self, channel: usize) -> impl Iterator<Item=S> + ExactSizeIterator + 'a
    where T: IntoSample<S>
    {
        let channels = self.channels.get();
        if channel >= channels {
            panic!("Invalid channel: {}, should match: 0..{}", channel, channels);
        }
        let num_samples = self.last_nsamples.expect("BandLimited frame not ended");
        let diffs = self.diffs[..num_samples*channels].iter().skip(channel).step_by(channels);
        BandLimitedSumIter {
            diffs,
            sum_end: &self.sums[channel].1,
            sum: self.sums[channel].0,
            _output: PhantomData::<S>,
            _options: PhantomData::<O>,
        }
    }
}

struct BandLimitedSumIter<'a, T: Copy + MulNorm + IntoSample<S> + FromSample<f32>,
                              O: BandLimOpt,
                              I: Iterator<Item=&'a T>,
                              S> {
    diffs: I,
    sum_end: &'a Cell<Option<T>>,
    sum: T,
    _output: PhantomData<S>,
    _options: PhantomData<O>,
}

impl<'a, T, O, I, S> Drop for BandLimitedSumIter<'a, T, O, I, S>
where I: Iterator<Item=&'a T>,
      O: BandLimOpt,
      T: Copy + MulNorm + IntoSample<S> + FromSample<f32>
{
    fn drop(&mut self) {
        if self.sum_end.get().is_none() {
            for _ in self.into_iter() {}
            self.sum_end.set(Some(self.sum))
        }
    }
}

impl<'a, T, O, I, S> std::iter::ExactSizeIterator for BandLimitedSumIter<'a, T, O, I, S>
where I: Iterator<Item=&'a T> + ExactSizeIterator,
      T: Copy + MulNorm + IntoSample<S> + FromSample<f32>,
      O: BandLimOpt
{
    fn len(&self) -> usize {
        self.diffs.len()
    }
}

impl<'a, T, O, I, S> Iterator for BandLimitedSumIter<'a, T, O, I, S>
where I: Iterator<Item=&'a T>,
      T: Copy + MulNorm + IntoSample<S> + FromSample<f32>,
      O: BandLimOpt
{
    type Item = S;

    fn next(&mut self) -> Option<S> {
        self.diffs.next().map(|&delta| {
            let sum = self.sum.saturating_add(delta);
            self.sum = sum.mul_norm(T::from_sample(O::HIGH_PASS));
            sum.into_sample()
        })
    }
}

impl<T,O> Blep for BandLimited<T,O>
where T: Copy + Default + SampleDelta + MulNorm
{
    type SampleDelta = T;
    type SampleTime = f64;

    #[inline]
    fn ensure_frame_time(&mut self, frame_time: f64, margin_time: f64) {
        self.set_frame_time(frame_time, margin_time);
    }

    #[inline]
    fn end_frame(&mut self, time: Self::SampleTime) -> usize {
        self.end_frame(time)
    }

    #[inline]
    fn add_step(&mut self, channel: usize, time: f64, delta: T) {
        let time = time - self.start_time;
        let channels = self.channels.get();
        debug_assert!(channel < channels);
        let index = time.trunc() as usize * channels + channel;
        let phase = ((time.fract() * PHASE_COUNT as f64).trunc() as usize).rem_euclid(PHASE_COUNT);
        for (dp, phase) in self.diffs[index..index + STEP_WIDTH*channels]
                          .iter_mut().step_by(channels)
                          .zip(self.steps[phase].iter()) {
            *dp = dp.saturating_add(phase.mul_norm(delta));
        }
    }    
}
