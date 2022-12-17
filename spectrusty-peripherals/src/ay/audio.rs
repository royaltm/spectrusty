/*
    Copyright (C) 2020-2022  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
//! The emulation of the AY-3-8910/8912/8913 sound generator.
use core::num::NonZeroU16;
use core::marker::PhantomData;

#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};

use super::{AyRegister, AyRegChange};
use spectrusty_core::audio::*;

/// Internal clock divisor.
pub const INTERNAL_CLOCK_DIVISOR: FTs = 16;
/// Cpu clock ratio.
pub const HOST_CLOCK_RATIO: FTs = 2;

/// Amplitude levels for AY-3-891x.
///
/// These levels are closest to the specs.
///
/// Original comment from [game-music-emu]:
/// ```text
///    // With channels tied together and 1K resistor to ground (as datasheet recommends),
///    // output nearly matches logarithmic curve as claimed. Approx. 1.5 dB per step.
/// ```
/// [AyAmps] struct implements `AMPS` for [AmpLevels]. See also [FUSE_AMPS].
///
/// [game-music-emu]: https://bitbucket.org/mpyne/game-music-emu/src/013d4676c689dc49f363f99dcfb8b88f22278236/gme/Ay_Apu.cpp#lines-32
#[allow(clippy::approx_constant,clippy::excessive_precision)]
pub const AMPS: [f32;16] = [0.000_000, 0.007_813, 0.011_049, 0.015_625,
                            0.022_097, 0.031_250, 0.044_194, 0.062_500,
                            0.088_388, 0.125_000, 0.176_777, 0.250_000,
                            0.353_553, 0.500_000, 0.707_107, 1.000_000];

pub const AMPS_I32: [i32;16] = [0x0000_0000, 0x0100_0431, 0x016a_0db9, 0x01ff_ffff,
                                0x02d4_1313, 0x03ff_ffff, 0x05a8_2627, 0x07ff_ffff,
                                0x0b50_4c4f, 0x0fff_ffff, 0x16a0_a0ff, 0x1fff_ffff,
                                0x2d41_397f, 0x3fff_ffff, 0x5a82_7b7f, 0x7fff_ffff];

pub const AMPS_I16: [i16;16] = [0x0000, 0x0100, 0x016a, 0x01ff,
                                0x02d4, 0x03ff, 0x05a8, 0x07ff,
                                0x0b50, 0x0fff, 0x16a0, 0x1fff,
                                0x2d40, 0x3fff, 0x5a81, 0x7fff];

/// These AY-3-891x amplitude levels are being used in the ["Free Unix Spectrum Emulator"] emulator.
///
/// The original comment below:
/// ```text
///  /* AY output doesn't match the claimed levels; these levels are based
///   * on the measurements posted to comp.sys.sinclair in Dec 2001 by
///   * Matthew Westcott, adjusted as I described in a followup to his post,
///   * then scaled to 0..0xffff.
///   */
/// ```
/// These are more linear than [AMPS].
/// [AyFuseAmps] struct implements `FUSE_AMPS` for [AmpLevels].
///
/// ["Free Unix Spectrum Emulator"]: http://fuse-emulator.sourceforge.net/
#[allow(clippy::unreadable_literal,clippy::excessive_precision)]
pub const FUSE_AMPS: [f32;16] = [0.000000000, 0.0137483785, 0.020462349, 0.029053178,
                                 0.042343784, 0.0618448150, 0.084718090, 0.136903940,
                                 0.169131000, 0.2646677500, 0.352712300, 0.449942770,
                                 0.570382240, 0.6872816000, 0.848172700, 1.000000000];

pub const FUSE_AMPS_I16: [i16;16] = [0x0000, 0x01c2, 0x029e, 0x03b8,
                                     0x056b, 0x07ea, 0x0ad8, 0x1186,
                                     0x15a6, 0x21e0, 0x2d25, 0x3997,
                                     0x4902, 0x57f8, 0x6c90, 0x7fff];

/// This may be used to calculate other levels, but I'd discourage from using it in the player
/// as it uses expensive float calculations.
pub struct LogAmpLevels16<T>(PhantomData<T>);
impl<T: Copy + FromSample<f32>> AmpLevels<T> for LogAmpLevels16<T> {
    fn amp_level(level: u32) -> T {
        // as proposed by https://www.dr-lex.be/info-stuff/volumecontrols.html
        const A: f32 = 3.1623e-3;
        const B: f32 = 5.757;
        let y: f32 = match level & 0xF {
            0  => 0.0,
            15 => 1.0,
            l => {
                let x = l as f32 / 15.0;
                A * (B * x).exp()
            }
        };
        T::from_sample(y)
    }
}

/// A struct implementing [AmpLevels] for Ay-3-891x sound chip. See also [AMPS].
pub struct AyAmps<T>(PhantomData<T>);
/// A struct implementing alternative [AmpLevels] for Ay-3-891x sound chip. See also [FUSE_AMPS].
pub struct AyFuseAmps<T>(PhantomData<T>);

macro_rules! impl_ay_amp_levels {
    ($([$name:ident, $ty:ty, $amps:ident]),*) => { $(
        impl AmpLevels<$ty> for $name<$ty> {
            #[inline(always)]
            fn amp_level(level: u32) -> $ty {
                $amps[(level & 15) as usize]
            }
        }
    )* };
}
impl_ay_amp_levels!(
    [AyAmps, f32, AMPS], [AyAmps, i32, AMPS_I32], [AyAmps, i16, AMPS_I16],
    [AyFuseAmps, f32, FUSE_AMPS], [AyFuseAmps, i16, FUSE_AMPS_I16]);

/// A trait for interfacing controllers to render square-wave audio pulses from an AY-3-891x emulator.
pub trait AyAudioFrame<B: Blep> {
    /// Renders square-wave pulses via [Blep] interface.
    ///
    /// Provide [AmpLevels] that can handle `level` values from 0 to 15 (4-bits).
    /// `channels` - target [Blep] audio channels for `[A, B, C]` AY-3-891x channels.
    fn render_ay_audio_frame<V: AmpLevels<B::SampleDelta>>(
        &mut self,
        blep: &mut B,
        channels: [usize; 3]
    );
}

/// Implements AY-3-8910/8912/8913 programmable sound generator.
///
/// For the implementation of I/O ports see [crate::ay].
#[derive(Default, Clone, Debug)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "snapshot", serde(rename_all = "camelCase"))]
pub struct Ay3_891xAudio {
    current_ts: FTs,
    last_levels: [u8; 3],
    amp_levels: [AmpLevel; 3],
    env_control: EnvelopeControl,
    noise_control: NoiseControl,
    tone_control: [ToneControl; 3],
    mixer: Mixer,
}

/// A type for AY-3-891x amplitude level register values.
#[derive(Default, Clone, Copy, Debug)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
struct AmpLevel(u8);

impl AmpLevel {
    #[inline]
    pub fn set(&mut self, level: u8) {
        self.0 = level & 0x1F;
    }
    #[inline]
    pub fn is_env_control(self) -> bool {
        self.0 & 0x10 != 0
    }
}

/// A type for AY-3-891x mixer controller register values.
#[derive(Default, Clone, Copy, Debug)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
struct Mixer(u8);

impl Mixer {
    #[inline]
    pub fn has_tone(self) -> bool {
        self.0 & 1 == 0
    }
    #[inline]
    pub fn has_noise(self) -> bool {
        self.0 & 8 == 0
    }
    #[inline]
    pub fn next_chan(&mut self) {
        self.0 >>= 1
    }
}

// TODO: make bitflags
pub const ENV_SHAPE_CONT_MASK:   u8 = 0b0000_1000;
pub const ENV_SHAPE_ATTACK_MASK: u8 = 0b0000_0100;
pub const ENV_SHAPE_ALT_MASK:    u8 = 0b0000_0010;
pub const ENV_SHAPE_HOLD_MASK:   u8 = 0b0000_0001;
const ENV_LEVEL_REV_MASK:    u8 = 0b1000_0000;
const ENV_LEVEL_MOD_MASK:    u8 = 0b0100_0000;
const ENV_LEVEL_MASK:        u8 = 0x0F;
const ENV_CYCLE_MASK:        u8 = 0xF0;

/// A type implementing AY-3-891x volume envelope progression.
#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
struct EnvelopeControl {
    period: u16,
    tick: u16,
    // c c c c CT AT AL HO
    cycle: u8,
    // RV MD 0 0 v v v v
    level: u8
}

impl Default for EnvelopeControl {
    fn default() -> Self {
        EnvelopeControl { period: 1, tick: 0, cycle: 0, level: 0 }
    }
}

impl EnvelopeControl {
    #[inline]
    fn set_shape(&mut self, shape: u8) {
        self.tick = 0;
        self.cycle = shape & !ENV_CYCLE_MASK;
        self.level = if shape & ENV_SHAPE_ATTACK_MASK != 0 {
            ENV_LEVEL_MOD_MASK
        }
        else {
            ENV_LEVEL_MOD_MASK|ENV_LEVEL_REV_MASK|ENV_LEVEL_MASK
        }
    }
    #[inline]
    fn set_period_fine(&mut self, perlo: u8) {
        self.set_period(self.period & 0xFF00 | perlo as u16)
    }

    #[inline]
    fn set_period_coarse(&mut self, perhi: u8) {
        self.set_period(u16::from_le_bytes([self.period as u8, perhi]))
    }
    #[inline]
    fn set_period(&mut self, mut period: u16) {
        if period == 0 { period = 1 }
        self.period = period;
        if self.tick >= period {
            self.tick %= period;
        }
    }
    #[inline]
    fn get_level(&self) -> u8 {
        self.level & ENV_LEVEL_MASK
    }
    #[inline]
    fn get_shape(&self) -> u8 {
        self.cycle & !ENV_CYCLE_MASK
    }
    #[inline]
    fn update_level(&mut self) -> u8 {
        let EnvelopeControl { period, mut tick, mut level, .. } = *self;
        if tick >= period {
            tick -= period;

            if level & ENV_LEVEL_MOD_MASK != 0 {
                level = (level & !ENV_LEVEL_MASK) | (
                    if level & ENV_LEVEL_REV_MASK == 0 {
                        level.wrapping_add(1)
                    }
                    else {
                        level.wrapping_sub(1)
                    }
                & ENV_LEVEL_MASK);

                let cycle = self.cycle.wrapping_add(0x10); // 16 times
                if cycle & ENV_CYCLE_MASK == 0 {
                    if cycle & ENV_SHAPE_CONT_MASK == 0 {
                        level = 0;
                    }
                    else if cycle & ENV_SHAPE_HOLD_MASK != 0 {
                        if cycle & ENV_SHAPE_ALT_MASK == 0 {
                            level ^= ENV_LEVEL_MOD_MASK|ENV_LEVEL_MASK;
                        }
                        else {
                            level ^= ENV_LEVEL_MOD_MASK;
                        }
                    }
                    else if cycle & ENV_SHAPE_ALT_MASK != 0 {
                        level ^= ENV_LEVEL_REV_MASK|ENV_LEVEL_MASK;
                    }
                }
                self.level = level;
                self.cycle = cycle;
            }
        }
        self.tick = tick.wrapping_add(1);
        level & ENV_LEVEL_MASK
    }
}

const NOISE_PERIOD_MASK: u8 = 0x1F;

/// A type implementing AY-3-891x noise progression.
#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
struct NoiseControl {
    rng: i32,
    period: u8,
    tick: u8,
    low: bool,
}

impl Default for NoiseControl {
    fn default() -> Self {
        NoiseControl { rng: 1, period: 0, tick: 0, low: false }
    }
}

impl NoiseControl {
    #[inline]
    fn set_period(&mut self, mut period: u8) {
        period &= NOISE_PERIOD_MASK;
        if period == 0 { period = 1 }
        self.period = period;
        if self.tick >= period {
            self.tick %= period;
        }
    }

    #[inline]
    fn update_is_low(&mut self) -> bool {
        let NoiseControl { mut rng, period, mut tick, mut low } = *self;
        if tick >= period {
            tick -= period;

            if (rng + 1) & 2 != 0 {
                low = !low;
                self.low = low;
            }
            rng = (-(rng & 1) & 0x12000) ^ (rng >> 1);
            self.rng = rng;
        }
        self.tick = tick.wrapping_add(1);
        low
    }
}

const TONE_GEN_MIN_THRESHOLD: u16 = 5;
const TONE_PERIOD_MASK: u16 = 0xFFF;

/// A type implementing AY-3-891x tone progression.
#[derive(Default, Clone, Copy, Debug)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
struct ToneControl {
    period: u16,
    tick: u16,
    low: bool
}


impl ToneControl {
    #[inline]
    fn set_period_fine(&mut self, perlo: u8) {
        self.set_period(self.period & 0xFF00 | perlo as u16)
    }

    #[inline]
    fn set_period_coarse(&mut self, perhi: u8) {
        self.set_period(u16::from_le_bytes([self.period as u8, perhi]))
    }

    #[inline]
    fn set_period(&mut self, mut period: u16) {
        period &= TONE_PERIOD_MASK;
        if period == 0 { period = 1 }
        self.period = period;
        if self.tick >= period*2 {
            self.tick %= period*2;
        }
    }

    #[inline]
    fn update_is_low(&mut self) -> bool {
        let ToneControl { period, mut tick, mut low } = *self;
        if period < TONE_GEN_MIN_THRESHOLD {
            low = false;
        }
        else if tick >= period {
            tick -= period;
            low = !low;
            self.low = low;
        }
        self.tick = tick.wrapping_add(2);
        low
    }
}

/// A type implementing timestamp iterator.
#[derive(Clone, Copy, Debug)]
struct Ticker {
    current: FTs,
    end_ts: FTs
}

impl Ticker {
    const CLOCK_INCREASE: FTs = HOST_CLOCK_RATIO * INTERNAL_CLOCK_DIVISOR;
    fn new(current: FTs, end_ts: FTs) -> Self {
        Ticker { current, end_ts }
    }
}

impl Iterator for Ticker {
    type Item = FTs;
    fn next(&mut self) -> Option<FTs> {
        let res = self.current;
        if res < self.end_ts {
            self.current = res + Self::CLOCK_INCREASE;
            Some(res)
        }
        else {
            None
        }
    }
}

/// Use the [Default] trait to create instances of this struct.
impl Ay3_891xAudio {
    /// Resets the internal state to the one initialized with.
    pub fn reset(&mut self) {
        *self = Default::default()
    }
    /// Converts a tone frequency given in Hz to a closest 16-bit tone period register value.
    ///
    /// `clock_hz` AY-3-891x clock frequency in Hz. In ZX Spectrum it equals to CPU_HZ / 2.
    /// Amstrad CPC has PSG clocked at 1 MHz. Atari ST at 2 MHz.
    ///
    /// Returns `None` if the result can't be properly represented by 16-bit unsigned integer or if
    /// the result is `0`.
    #[allow(clippy::float_cmp)]
    pub fn freq_to_tone_period(clock_hz: f32, hz: f32) -> Option<NonZeroU16> {
        let ftp = (clock_hz / (INTERNAL_CLOCK_DIVISOR as f32 * hz)).round();
        let utp = ftp as u16;
        if utp as f32 != ftp {
            None
        }
        else {
            NonZeroU16::new(utp)
        }
    }
    /// Converts a 16-bit tone period register value to a tone frequency in Hz.
    ///
    /// `clock_hz` AY-3-891x clock frequency in Hz. In ZX Spectrum it equals to CPU_HZ / 2.
    /// Amstrad CPC has PSG clocked at 1 MHz. Atari ST at 2 MHz.
    pub fn tone_period_to_freq(clock_hz: f32, tp: u16) -> f32 {
        clock_hz / (tp as f32 * INTERNAL_CLOCK_DIVISOR as f32)
    }
    /// Creates an iterator of tone periods for the AY-3-891x chip.
    ///
    /// `min_octave` A minimal octave number, 0-based (0 is the minimum).
    /// `max_octave` A maximal octave number, 0-based (7 is the maximum).
    /// `note_freqs` An array of tone frequencies (in Hz) in the 5th octave (0-based: 4).
    ///  To generate frequencies you may want to use audio::music::equal_tempered_scale_note_freqs.
    /// `clock_hz` The AY-3-891x clock frequency in Hz. Usually, it's CPU_HZ / 2.
    ///
    /// # Panics
    /// Panics if any period can't be expressed by 16-bit unsigned integer.
    pub fn tone_periods<I>(
                clock_hz: f32,
                min_octave: i32,
                max_octave: i32,
                note_freqs: I
            ) -> impl IntoIterator<Item=u16>
        where I: Clone + IntoIterator<Item=f32>
    {
        (min_octave..=max_octave).flat_map(move |octave| {
          note_freqs.clone().into_iter().map(move |hz| {
            let hz = hz * (2.0f32).powi(octave - 4);
            Self::freq_to_tone_period(clock_hz, hz)
                 .expect("frequency out of range")
                 .get()
          })
        })
    }
    /// Renders square-wave audio pulses via the [Blep] interface while mutating the internal state.
    ///
    /// The internal state is being altered every [INTERNAL_CLOCK_DIVISOR] * [HOST_CLOCK_RATIO] Cpu
    /// clock cycles until `end_ts` is reached. The internal cycle counter is then decremented by the
    /// value of `frame_tstates` before returning from this method.
    ///
    /// Provide [AmpLevels] that can handle `level` values from 0 to 15 (4-bits).
    ///
    /// * `changes` should be ordered by `time` and recorded only with `time` < `end_ts`
    ///   otherwise, some register changes may be lost - the iterator will be drained anyway.
    /// * `end_ts` should be a value of an end of frame T-state counter value.
    /// * `frame_tstates` should be a duration of a single frame in T-states.
    /// * `channels` - indicate [Blep] audio channels for `[A, B, C]` AY channels.
    pub fn render_audio<V,I,A>(&mut self,
                changes: I,
                blep: &mut A,
                end_ts: FTs,
                frame_tstates: FTs,
                chans: [usize; 3]
            )
        where V: AmpLevels<A::SampleDelta>,
              I: IntoIterator<Item=AyRegChange>,
              A: Blep
    {
        let mut change_iter = changes.into_iter().peekable();
        let mut ticker = Ticker::new(self.current_ts, end_ts);
        let mut tone_levels: [u8; 3] = self.last_levels;
        let mut vol_levels: [A::SampleDelta;3] = Default::default();

        for (level, tgt_amp) in tone_levels.iter().copied()
                                .zip(vol_levels.iter_mut()) {
            *tgt_amp = V::amp_level(level.into());
        }
        for tick in &mut ticker {
            while let Some(change) = change_iter.peek() {
                if change.time <= tick {
                    let AyRegChange { reg, val, .. } = change_iter.next().unwrap();
                    self.update_register(reg, val);
                }
                else {
                    break
                }
            }


            let env_level = self.env_control.update_level();
            let noise_low = self.noise_control.update_is_low();
            let mut mixer = self.mixer;
            for ((level, tone_control), tgt_lvl) in self.amp_levels.iter()
                                                    .zip(self.tone_control.iter_mut())
                                                        .zip(tone_levels.iter_mut()) {
                *tgt_lvl = if (mixer.has_tone() && tone_control.update_is_low()) ||
                   (mixer.has_noise() && noise_low) {
                    0
                }
                else if level.is_env_control() {
                    env_level
                }
                else {
                    level.0
                };
                mixer.next_chan();
            }

            for (chan, (level, last_vol)) in chans.iter().copied()
                                                  .zip(tone_levels.iter().copied()
                                                  .zip(vol_levels.iter_mut())) {
                let vol = V::amp_level(level.into());
                if let Some(delta) = last_vol.sample_delta(vol) {
                    blep.add_step(chan, tick, delta);
                    *last_vol = vol;
                }
            }

        }
        for AyRegChange { reg, val, .. } in change_iter {
            self.update_register(reg, val);
        }

        self.current_ts = ticker.current - frame_tstates;
        self.last_levels = tone_levels;
    }
    /// Updates the value of one of the sound generator registers for the indicated `reg` register,
    /// with the value given in `val`.
    ///
    /// This method can be used to instantly set the state of the sound generator without the need
    /// to generate audio pulses.
    #[inline]
    pub fn update_register(&mut self, reg: AyRegister, val: u8) {
        use AyRegister::*;
        match reg {
            ToneFineA|ToneFineB|ToneFineC => {
                self.tone_control[usize::from(reg) >> 1].set_period_fine(val)
            }
            ToneCoarseA|ToneCoarseB|ToneCoarseC => {
                self.tone_control[usize::from(reg) >> 1].set_period_coarse(val)
            }
            NoisePeriod => {
                self.noise_control.set_period(val)
            }
            MixerControl => {
                self.mixer = Mixer(val)
            }
            AmpLevelA|AmpLevelB|AmpLevelC => {
                self.amp_levels[usize::from(reg) - 8].set(val)
            }
            EnvPerFine => {
                self.env_control.set_period_fine(val)
            }
            EnvPerCoarse => {
                self.env_control.set_period_coarse(val)
            }
            EnvShape => {
                self.env_control.set_shape(val)
            }
            _ => ()
        }
    }
    /// Returns the current tone periods of each channel.
    ///
    /// The period is in the range: [1, 4095].
    #[inline]
    pub fn get_tone_periods(&self) -> [u16;3] {
        let mut periods = [0;3];
        for (tone, tgt) in self.tone_control.iter().zip(periods.iter_mut()) {
            *tgt = tone.period;
        }
        periods
    }
    /// Returns the current amplitude level of each channel.
    ///
    /// If the channel volume register's envelope bit is set, it returns the current envelope
    /// level for that channel.
    ///
    /// The levels are in the range: [0, 15].
    #[inline]
    pub fn get_amp_levels(&self) -> [u8;3] {
        let mut amps = [0;3];
        for (level, tgt) in self.amp_levels.iter().zip(amps.iter_mut()) {
            *tgt = if level.is_env_control() {
                self.env_control.get_level()
            }
            else {
                level.0
            };
        }
        amps
    }
    /// Returns the current noise pitch.
    ///
    /// The pitch is in the range: [0, 31].
    #[inline]
    pub fn get_noise_pitch(&self) -> u8 {
        self.noise_control.period
    }
    /// Returns the current value of the mixer register.
    ///
    /// ```text
    /// t - tone bit: 0 tone enabled, 1 disabled
    /// n - noise bit: 0 noise enabled, 1 disabled
    ///
    /// b7 b6 b5 b4 b3 b2 b1 b0 bit
    /// -  -  n  n  n  t  t  t  value
    /// -  -  C  B  A  C  B  A  channel
    /// ```
    #[inline]
    pub fn get_mixer(&self) -> u8 {
        self.mixer.0
    }
    /// Returns the current level of the envelope generator.
    #[inline]
    pub fn get_envelope_level(&self) -> u8 {
        self.env_control.get_level()
    }
    /// Returns the envelope shape.
    #[inline]
    pub fn get_envelope_shape(&self) -> u8 {
        self.env_control.get_shape()
    }
    /// Returns the envelope period.
    ///
    /// The period is in the range: [1, 65535].
    #[inline]
    pub fn get_envelope_period(&self) -> u16 {
        self.env_control.period
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ay_3_889x_tone_periods() {
        use spectrusty_audio::music::*;
        let clock_hz = 3_546_900.0/2.0f32;
        let mut notes: Vec<u16> = Vec::new();
        assert_eq!(252, Ay3_891xAudio::freq_to_tone_period(clock_hz, 440.0).unwrap().get());
        assert_eq!(5, Ay3_891xAudio::freq_to_tone_period(clock_hz, 24000.0).unwrap().get());
        assert_eq!(439.84375, Ay3_891xAudio::tone_period_to_freq(clock_hz, 252));
        assert_eq!(22168.125, Ay3_891xAudio::tone_period_to_freq(clock_hz, 5));
        notes.extend(Ay3_891xAudio::tone_periods(clock_hz, 0, 7, equal_tempered_scale_note_freqs(440.0, 0, 12)));
        assert_eq!(
            vec![4031, 3804, 3591, 3389, 3199, 3020, 2850, 2690, 2539, 2397, 2262, 2135,
                 2015, 1902, 1795, 1695, 1600, 1510, 1425, 1345, 1270, 1198, 1131, 1068,
                 1008,  951,  898,  847,  800,  755,  713,  673,  635,  599,  566,  534,
                  504,  476,  449,  424,  400,  377,  356,  336,  317,  300,  283,  267,
                  252,  238,  224,  212,  200,  189,  178,  168,  159,  150,  141,  133,
                  126,  119,  112,  106,  100,   94,   89,   84,   79,   75,   71,   67,
                   63,   59,   56,   53,   50,   47,   45,   42,   40,   37,   35,   33,
                   31,   30,   28,   26,   25,   24,   22,   21,   20,   19,   18,   17], notes);
    }

    #[test]
    fn ay_3_889x_env_works() {
        // println!("Ay3_891xAudio {:?}", core::mem::size_of::<Ay3_891xAudio>());
        let mut ay = Ay3_891xAudio::default();

        for shape in [0, ENV_SHAPE_ALT_MASK,
                         ENV_SHAPE_HOLD_MASK,
                         ENV_SHAPE_ALT_MASK|ENV_SHAPE_HOLD_MASK,
                         ENV_SHAPE_CONT_MASK|ENV_SHAPE_HOLD_MASK].iter().copied() {
            ay.env_control.set_shape(shape);
            assert_eq!(ay.env_control.tick, 0);
            assert_eq!(ay.env_control.cycle, shape);
            assert_eq!(ay.env_control.level, ENV_LEVEL_REV_MASK|ENV_LEVEL_MOD_MASK|ENV_LEVEL_MASK);
            ay.env_control.set_period(0);
            assert_eq!(ay.env_control.period, 1);
            for exp_level in (0..=15).rev() {
                assert_eq!(ay.env_control.update_level(), exp_level);
                assert_eq!(ay.env_control.tick, 1);
            }
            for _ in 0..100 {
                assert_eq!(ay.env_control.tick, 1);
                assert_eq!(ay.env_control.update_level(), 0);
            }
        }

        for shape in [0, ENV_SHAPE_ALT_MASK,
                         ENV_SHAPE_HOLD_MASK,
                         ENV_SHAPE_ALT_MASK|ENV_SHAPE_HOLD_MASK,
                         ENV_SHAPE_CONT_MASK|ENV_SHAPE_ATTACK_MASK|ENV_SHAPE_ALT_MASK|ENV_SHAPE_HOLD_MASK
                         ].iter().copied() {
            ay.env_control.set_shape(shape|ENV_SHAPE_ATTACK_MASK);
            assert_eq!(ay.env_control.tick, 0);
            assert_eq!(ay.env_control.cycle, shape|ENV_SHAPE_ATTACK_MASK);
            assert_eq!(ay.env_control.level, ENV_LEVEL_MOD_MASK);
            ay.env_control.set_period(0);
            assert_eq!(ay.env_control.period, 1);
            for exp_level in 0..=15 {
                assert_eq!(ay.env_control.update_level(), exp_level);
                assert_eq!(ay.env_control.level, ENV_LEVEL_MOD_MASK|exp_level);
                assert_eq!(ay.env_control.tick, 1);
            }
            for _ in 0..100 {
                assert_eq!(ay.env_control.tick, 1);
                assert_eq!(ay.env_control.update_level(), 0);
                assert_eq!(ay.env_control.level, 0);
            }
        }

        ay.env_control.set_shape(ENV_SHAPE_CONT_MASK);
        assert_eq!(ay.env_control.tick, 0);
        assert_eq!(ay.env_control.cycle, ENV_SHAPE_CONT_MASK);
        assert_eq!(ay.env_control.level, ENV_LEVEL_REV_MASK|ENV_LEVEL_MOD_MASK|ENV_LEVEL_MASK);
        ay.env_control.set_period(0);
        assert_eq!(ay.env_control.period, 1);
        for _ in 0..10 {
            for exp_level in (0..=15).rev() {
                assert_eq!(ay.env_control.update_level(), exp_level);
                assert_eq!(ay.env_control.level, ENV_LEVEL_REV_MASK|ENV_LEVEL_MOD_MASK|exp_level);
                assert_eq!(ay.env_control.tick, 1);
            }
        }

        ay.env_control.set_shape(ENV_SHAPE_CONT_MASK|ENV_SHAPE_ALT_MASK);
        assert_eq!(ay.env_control.tick, 0);
        assert_eq!(ay.env_control.cycle, ENV_SHAPE_CONT_MASK|ENV_SHAPE_ALT_MASK);
        assert_eq!(ay.env_control.level, ENV_LEVEL_REV_MASK|ENV_LEVEL_MOD_MASK|ENV_LEVEL_MASK);
        ay.env_control.set_period(0);
        assert_eq!(ay.env_control.period, 1);
        for _ in 0..10 {
            for exp_level in (0..=15).rev() {
                assert_eq!(ay.env_control.update_level(), exp_level);
                assert_eq!(ay.env_control.level, ENV_LEVEL_REV_MASK|ENV_LEVEL_MOD_MASK|exp_level);
                assert_eq!(ay.env_control.tick, 1);
            }
            for exp_level in 0..=15 {
                assert_eq!(ay.env_control.update_level(), exp_level);
                assert_eq!(ay.env_control.level, ENV_LEVEL_MOD_MASK|exp_level);
                assert_eq!(ay.env_control.tick, 1);
            }
        }

        ay.env_control.set_shape(ENV_SHAPE_CONT_MASK|ENV_SHAPE_ALT_MASK|ENV_SHAPE_HOLD_MASK);
        assert_eq!(ay.env_control.tick, 0);
        assert_eq!(ay.env_control.cycle, ENV_SHAPE_CONT_MASK|ENV_SHAPE_ALT_MASK|ENV_SHAPE_HOLD_MASK);
        assert_eq!(ay.env_control.level, ENV_LEVEL_REV_MASK|ENV_LEVEL_MOD_MASK|ENV_LEVEL_MASK);
        ay.env_control.set_period(0);
        assert_eq!(ay.env_control.period, 1);
        for exp_level in (0..=15).rev() {
            assert_eq!(ay.env_control.update_level(), exp_level);
            assert_eq!(ay.env_control.level, ENV_LEVEL_REV_MASK|ENV_LEVEL_MOD_MASK|exp_level);
            assert_eq!(ay.env_control.tick, 1);
        }
        for _ in 0..100 {
            assert_eq!(ay.env_control.tick, 1);
            assert_eq!(ay.env_control.update_level(), 15);
            assert_eq!(ay.env_control.level, ENV_LEVEL_REV_MASK|15);
        }

        ay.env_control.set_shape(ENV_SHAPE_CONT_MASK|ENV_SHAPE_ATTACK_MASK);
        assert_eq!(ay.env_control.tick, 0);
        assert_eq!(ay.env_control.cycle, ENV_SHAPE_CONT_MASK|ENV_SHAPE_ATTACK_MASK);
        assert_eq!(ay.env_control.level, ENV_LEVEL_MOD_MASK);
        ay.env_control.set_period(0);
        assert_eq!(ay.env_control.period, 1);
        for _ in 0..10 {
            for exp_level in 0..=15 {
                assert_eq!(ay.env_control.update_level(), exp_level);
                assert_eq!(ay.env_control.level, ENV_LEVEL_MOD_MASK|exp_level);
                assert_eq!(ay.env_control.tick, 1);
            }
        }

        ay.env_control.set_shape(ENV_SHAPE_CONT_MASK|ENV_SHAPE_ATTACK_MASK|ENV_SHAPE_HOLD_MASK);
        assert_eq!(ay.env_control.tick, 0);
        assert_eq!(ay.env_control.cycle, ENV_SHAPE_CONT_MASK|ENV_SHAPE_ATTACK_MASK|ENV_SHAPE_HOLD_MASK);
        assert_eq!(ay.env_control.level, ENV_LEVEL_MOD_MASK);
        ay.env_control.set_period(0);
        assert_eq!(ay.env_control.period, 1);
        for exp_level in 0..=15 {
            assert_eq!(ay.env_control.update_level(), exp_level);
            assert_eq!(ay.env_control.level, ENV_LEVEL_MOD_MASK|exp_level);
            assert_eq!(ay.env_control.tick, 1);
        }
        for _ in 0..100 {
            assert_eq!(ay.env_control.tick, 1);
            assert_eq!(ay.env_control.update_level(), 15);
            assert_eq!(ay.env_control.level, 15);
        }

        ay.env_control.set_shape(ENV_SHAPE_CONT_MASK|ENV_SHAPE_ATTACK_MASK|ENV_SHAPE_ALT_MASK);
        assert_eq!(ay.env_control.tick, 0);
        assert_eq!(ay.env_control.cycle, ENV_SHAPE_CONT_MASK|ENV_SHAPE_ATTACK_MASK|ENV_SHAPE_ALT_MASK);
        assert_eq!(ay.env_control.level, ENV_LEVEL_MOD_MASK);
        ay.env_control.set_period(0);
        assert_eq!(ay.env_control.period, 1);
        for _ in 0..10 {
            for exp_level in 0..=15 {
                assert_eq!(ay.env_control.update_level(), exp_level);
                assert_eq!(ay.env_control.level, ENV_LEVEL_MOD_MASK|exp_level);
                assert_eq!(ay.env_control.tick, 1);
            }
            for exp_level in (0..=15).rev() {
                assert_eq!(ay.env_control.update_level(), exp_level);
                assert_eq!(ay.env_control.level, ENV_LEVEL_REV_MASK|ENV_LEVEL_MOD_MASK|exp_level);
                assert_eq!(ay.env_control.tick, 1);
            }
        }
    }
}
