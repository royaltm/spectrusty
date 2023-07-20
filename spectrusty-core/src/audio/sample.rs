/*
    Copyright (C) 2020-2023  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
//! Various traits for types being used as audio samples.

/// Provides various methods to primitive types being used as audio samples.
pub trait AudioSample: Copy + Send + Default + 'static {
    /// Creates a silent sample value (with zero amplitude). Useful for filling buffers.
    #[inline(always)]
    fn silence() -> Self {
        Self::default()
    }
    fn max_pos_amplitude() -> Self;
    fn max_neg_amplitude() -> Self;
}

/// For converting samples between types.
pub trait FromSample<S> {
    /// Converts to Self a sample from the `other`.
    fn from_sample(other: S) -> Self;
}

/// For converting samples between types.
pub trait IntoSample<S> {
    /// Convert to `S` a sample type from `self`.
    fn into_sample(self) -> S;
}

/// This trait is being used to calculate sample amplitude differences (∆).
pub trait SampleDelta: Copy + Default {
    /// Returns the difference (if any) between `after` and `self` (before).
    fn sample_delta(self, after: Self) -> Option<Self>;
}

/// This trait is being used for scaling sample amplitudes.
pub trait MulNorm {
    /// Saturating addition. Computes self + other, saturating at the normalized bounds instead of overflowing.
    ///
    /// Float samples operates in range: [-1.0, 1.0], integer: [min, max]
    fn saturating_add(self, other: Self) -> Self;
    /// Multiplies `self` with the `other` in the normalized sample amplitude range.
    ///
    /// Float samples operates in range: [-1.0, 1.0], integer: [min, max]
    fn mul_norm(self, other: Self) -> Self;
}

macro_rules! impl_sample_delta_float {
    ($ty:ty) => {
        impl SampleDelta for $ty {
            #[inline]
            fn sample_delta(self, after: $ty) -> Option<$ty> {
                let delta = after - self;
                if delta.abs() > <$ty>::EPSILON {
                    Some(delta)
                }
                else {
                    None
                }
            }
        }
    }
}

impl_sample_delta_float!(f32);
impl_sample_delta_float!(f64);

macro_rules! impl_sample_delta_int {
    ($ty:ty) => {
        impl SampleDelta for $ty {
            #[inline]
            fn sample_delta(self, after: $ty) -> Option<$ty> {
                let delta = after - self;
                if delta != 0 {
                    Some(delta)
                }
                else {
                    None
                }
            }
        }
    };
}

impl_sample_delta_int!(i8);
impl_sample_delta_int!(i16);
impl_sample_delta_int!(i32);
impl_sample_delta_int!(i64);

impl AudioSample for f64 {
    #[inline(always)] fn max_pos_amplitude() -> Self {  1.0 }
    #[inline(always)] fn max_neg_amplitude() -> Self { -1.0 }
}
impl AudioSample for f32 {
    #[inline(always)] fn max_pos_amplitude() -> Self {  1.0 }
    #[inline(always)] fn max_neg_amplitude() -> Self { -1.0 }
}
impl AudioSample for i64 {
    #[inline(always)] fn max_pos_amplitude() -> Self { i64::MAX }
    #[inline(always)] fn max_neg_amplitude() -> Self { i64::MIN }
}
impl AudioSample for i32 {
    #[inline(always)] fn max_pos_amplitude() -> Self { i32::MAX }
    #[inline(always)] fn max_neg_amplitude() -> Self { i32::MIN }
}
impl AudioSample for i16 {
    #[inline(always)] fn max_pos_amplitude() -> Self { i16::MAX }
    #[inline(always)] fn max_neg_amplitude() -> Self { i16::MIN }
}
impl AudioSample for i8 {
    #[inline(always)] fn max_pos_amplitude() -> Self { i8::MAX }
    #[inline(always)] fn max_neg_amplitude() -> Self { i8::MIN }
}
impl AudioSample for u64 {
    #[inline(always)]
    fn silence() -> Self {
        1 << 63
    }
    #[inline(always)] fn max_pos_amplitude() -> Self { u64::MAX }
    #[inline(always)] fn max_neg_amplitude() -> Self { 0 }
}
impl AudioSample for u32 {
    #[inline(always)]
    fn silence() -> Self {
        1 << 31
    }
    #[inline(always)] fn max_pos_amplitude() -> Self { u32::MAX }
    #[inline(always)] fn max_neg_amplitude() -> Self { 0 }
}
impl AudioSample for u16 {
    #[inline(always)]
    fn silence() -> Self {
        0x8000
    }    
    #[inline(always)] fn max_pos_amplitude() -> Self { u16::MAX }
    #[inline(always)] fn max_neg_amplitude() -> Self { 0 }
}
impl AudioSample for u8 {
    #[inline(always)]
    fn silence() -> Self {
        0x80
    }
    #[inline(always)] fn max_pos_amplitude() -> Self { u8::MAX }
    #[inline(always)] fn max_neg_amplitude() -> Self { 0 }
}

impl<S: FromSample<T>, T> IntoSample<S> for T {
    #[inline]
    fn into_sample(self) -> S {
        S::from_sample(self)
    }    
}

impl<T: AudioSample> FromSample<T> for T {
    #[inline(always)]
    fn from_sample(other: T) -> T {
        other
    }
}

macro_rules! impl_from_sample {
    ($int:ty, $uint:ty, $ft:ty) => {
        impl FromSample<$int> for $ft {
            #[inline]
            fn from_sample(other: $int) -> $ft {
                if other < 0 {
                    other as $ft / -(<$int>::MIN as $ft)
                } else {
                    other as $ft / <$int>::MAX as $ft
                }
            }
        }

        impl FromSample<$ft> for $int {
            #[inline]
            fn from_sample(other: $ft) -> $int {
                if other >= 0.0 {
                    (other * <$int>::MAX as $ft) as $int
                } else {
                    (-other * <$int>::MIN as $ft) as $int
                }
            }
        }

        impl FromSample<$uint> for $ft {
            #[inline]
            fn from_sample(other: $uint) -> $ft {
                <$ft>::from_sample(<$int>::from_sample(other))
            }
        }

        impl FromSample<$ft> for $uint {
            #[inline]
            fn from_sample(other: $ft) -> $uint {
                <$uint>::from_sample(<$int>::from_sample(other))
            }
        }
    };

    ($int:ty, $uint:ty) => {
        impl FromSample<$uint> for $int {
            #[inline]
            fn from_sample(other: $uint) -> $int {
                other.wrapping_sub(<$int>::MIN as $uint) as $int
            }
        }

        impl FromSample<$int> for $uint {
            #[inline]
            fn from_sample(other: $int) -> $uint {
                other.wrapping_sub(<$int>::MIN) as $uint
            }
        }
    };
}

macro_rules! impl_from_sample_lossy {
    ($int:ty, $uint:ty, $ft:ty) => {
        impl FromSample<$int> for $ft {
            #[inline]
            fn from_sample(other: $int) -> $ft {
                const BITS: u32 = <$int>::BITS - <$ft>::MANTISSA_DIGITS - 1;
                if other < 0 {
                    (other >> BITS) as $ft / -((<$int>::MIN >> BITS) as $ft)
                } else {
                    (other >> BITS) as $ft / (<$int>::MAX >> BITS) as $ft
                }
            }
        }

        impl FromSample<$ft> for $int {
            #[inline]
            fn from_sample(other: $ft) -> $int {
                const BITS: u32 = <$int>::BITS - <$ft>::MANTISSA_DIGITS - 1;
                (if other >= 0.0 {
                    (other * (<$int>::MAX >> BITS) as $ft) as $int
                } else {
                    (-other * (<$int>::MIN >> BITS) as $ft) as $int
                }) << BITS
            }
        }

        impl FromSample<$uint> for $ft {
            #[inline]
            fn from_sample(other: $uint) -> $ft {
                <$ft>::from_sample(<$int>::from_sample(other))
            }
        }

        impl FromSample<$ft> for $uint {
            #[inline]
            fn from_sample(other: $ft) -> $uint {
                <$uint>::from_sample(<$int>::from_sample(other))
            }
        }
    };

    ($lint:ty, $luint:ty, $hint:ty, $huint:ty) => {
        impl FromSample<$lint> for $hint {
            #[inline]
            fn from_sample(other: $lint) -> $hint {
                (other as $hint) << (<$hint>::BITS - <$lint>::BITS)
            }
        }

        impl FromSample<$luint> for $huint {
            #[inline]
            fn from_sample(other: $luint) -> $huint {
                (other as $huint) << (<$huint>::BITS - <$luint>::BITS)
            }
        }

        impl FromSample<$lint> for $huint {
            #[inline]
            fn from_sample(other: $lint) -> $huint {
                <$huint>::from_sample(<$luint>::from_sample(other))
            }
        }

        impl FromSample<$luint> for $hint {
            #[inline]
            fn from_sample(other: $luint) -> $hint {
                <$hint>::from_sample(<$lint>::from_sample(other))
            }
        }

        impl FromSample<$hint> for $lint {
            #[inline]
            fn from_sample(other: $hint) -> $lint {
                (other >> (<$hint>::BITS - <$lint>::BITS)) as $lint
            }
        }

        impl FromSample<$huint> for $luint {
            #[inline]
            fn from_sample(other: $huint) -> $luint {
                (other >> (<$huint>::BITS - <$luint>::BITS)) as $luint
            }
        }

        impl FromSample<$hint> for $luint {
            #[inline]
            fn from_sample(other: $hint) -> $luint {
                <$luint>::from_sample(<$huint>::from_sample(other))
            }
        }

        impl FromSample<$huint> for $lint {
            #[inline]
            fn from_sample(other: $huint) -> $lint {
                <$lint>::from_sample(<$hint>::from_sample(other))
            }
        }
    };
}

impl_from_sample!(i8, u8);
impl_from_sample!(i16, u16);
impl_from_sample!(i32, u32);
impl_from_sample!(i64, u64);

impl_from_sample!(i8, u8, f32);
impl_from_sample!(i16, u16, f32);
impl_from_sample_lossy!(i32, u32, f32);
impl_from_sample_lossy!(i64, u64, f32);

impl_from_sample!(i8, u8, f64);
impl_from_sample!(i16, u16, f64);
impl_from_sample!(i32, u32, f64);
impl_from_sample_lossy!(i64, u64, f64);

impl_from_sample_lossy!(i32, u32, i64, u64);
impl_from_sample_lossy!(i16, u16, i64, u64);
impl_from_sample_lossy!(i8, u8, i64, u64);
impl_from_sample_lossy!(i16, u16, i32, u32);
impl_from_sample_lossy!(i8, u8, i32, u32);
impl_from_sample_lossy!(i8, u8, i16, u16);

impl FromSample<f32> for f64 {
    #[inline]
    fn from_sample(other: f32) -> f64 {
        other as f64
    }
}

impl FromSample<f64> for f32 {
    #[inline]
    fn from_sample(other: f64) -> f32 {
        other as f32
    }
}

macro_rules! impl_mul_norm_float {
    ($ty:ty) => {
        impl MulNorm for $ty {
            #[inline]
            fn saturating_add(self, other: $ty) -> $ty {
                (self + other).clamp(-1.0, 1.0)
            }

            #[inline]
            fn mul_norm(self, other: $ty) -> $ty {
                self * other
            }
        }
    };
}

impl_mul_norm_float!(f32);
impl_mul_norm_float!(f64);

macro_rules! impl_mul_norm_int {
    ($ty:ty, $up:ty) => {
        impl MulNorm for $ty {
            #[inline]
            fn saturating_add(self, other: $ty) -> $ty {
                self.saturating_add(other)
            }

            #[inline]
            fn mul_norm(self, other: $ty) -> $ty {
                const BITS: u32 = (<$up>::BITS - <$ty>::BITS) - 1;
                ((self as $up * other as $up) >> BITS) as $ty
            }
        }
    };
}

impl_mul_norm_int!(i8, i16);
impl_mul_norm_int!(i16, i32);
impl_mul_norm_int!(i32, i64);

macro_rules! impl_mul_norm_uint {
    ($ty:ty, $int:ty) => {
        impl MulNorm for $ty {
            #[inline]
            fn saturating_add(self, other: $ty) -> $ty {
                (<$int>::from_sample(self).saturating_add(<$int>::from_sample(other))).into_sample()
            }

            #[inline]
            fn mul_norm(self, other: $ty) -> $ty {
                <$int>::from_sample(self).mul_norm(<$int>::from_sample(other)).into_sample()
            }
        }
    };
}

impl_mul_norm_uint!(u8, i8);
impl_mul_norm_uint!(u16, i16);
impl_mul_norm_uint!(u32, i32);

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn mul_norm_float() {
        for (a, b, c) in vec![(0f32, 0f32, 0f32), (1.0, 1.0, 1.0), (-1.0, -1.0, 1.0), (-1.0, 1.0, -1.0), (0.5, 0.5, 0.25), (0.5, -0.5, -0.25)] {
            assert_eq!(a.mul_norm(b), c);
        }
    }

    #[test]
    fn mul_norm_int() {
        // the exception for the case when -1.0 * -1.0 = -1.0 is irrelevant,
        // mul_norm is performed with either phase step which is never -1.0 or with a positive number,
        for (a, b, c) in vec![(0i8, 0i8, 0i8), (127, 127, 126), (-128, -128, -128), (-128, 127, -127), (64, 64, 32), (64, -64, -32)] {
            assert_eq!(a.mul_norm(b), c);
        }
    }

    #[test]
    fn mul_norm_uint() {
        // the exception for the case when -1.0 * -1.0 = -1.0 is irrelevant,
        // mul_norm is performed with either phase step which is never -1.0 or with a positive number,
        for (a, b, c) in vec![(128u8, 128u8, 128u8), (255, 255, 254), (0, 0, 0), (0, 255, 1), (128+64, 128+64, 128+32), (128+64, 128-64, 128-32)] {
            assert_eq!(a.mul_norm(b), c);
        }
    }

    #[test]
    fn i16_from_i16() {
        for (f, t) in vec![(0i16, 0i16), (123, 123), (-456, -456), (32767, 32767), (-32768, -32768)] {
            assert_eq!(i16::from_sample(f), t);
            assert_eq!(IntoSample::<i16>::into_sample(f), t);
        }
    }

    #[test]
    fn i16_from_u16() {
        for (f, t) in vec![(32768u16, 0i16), (32769, 1), (16384, -16384), (65535, 32767), (0, -32768)] {
            assert_eq!(i16::from_sample(f), t);
            assert_eq!(IntoSample::<i16>::into_sample(f), t);
        }
    }

    #[test]
    fn i16_from_f32() {
        for (f, t) in vec![(0.0f32, 0i16), (1.0/32767.0, 1), (-0.5, -16384), (1.0, 32767), (-1.0, -32768)] {
            assert_eq!(i16::from_sample(f), t);
            assert_eq!(IntoSample::<i16>::into_sample(f), t);
        }
    }

    #[test]
    fn u16_from_i16() {
        for (f, t) in vec![(0i16, 32768u16), (1, 32769), (-16384, 16384), (32767, 65535), (-32768, 0)] {
            assert_eq!(u16::from_sample(f), t);
            assert_eq!(IntoSample::<u16>::into_sample(f), t);
        }
    }

    #[test]
    fn u16_from_u16() {
        for (f, t) in vec![(32768u16, 32768u16), (32769, 32769), (16384, 16384), (65535, 65535), (0, 0)] {
            assert_eq!(u16::from_sample(f), t);
            assert_eq!(IntoSample::<u16>::into_sample(f), t);
        }
    }

    #[test]
    fn u16_from_f32() {
        for (f, t) in vec![(0.0f32, 32768u16), (1.0/32767.0, 32769), (-0.5, 16384), (1.0, 65535), (-1.0, 0)] {
            assert_eq!(u16::from_sample(f), t);
            assert_eq!(IntoSample::<u16>::into_sample(f), t);
        }
    }

    #[test]
    fn f32_from_i16() {
        for (f, t) in vec![(0i16, 0.0f32), (1, 1.0/32767.0), (-16384, -0.5), (32767, 1.0), (-32768, -1.0)] {
            assert_eq!(f32::from_sample(f), t);
            assert_eq!(IntoSample::<f32>::into_sample(f), t);
        }
    }

    #[test]
    fn f32_from_u16() {
        for (f, t) in vec![(32768u16, 0.0f32), (32769, 1.0/32767.0), (16384, -0.5), (65535, 1.0), (0, -1.0)] {
            assert_eq!(f32::from_sample(f), t);
            assert_eq!(IntoSample::<f32>::into_sample(f), t);
        }
    }

    #[test]
    fn f32_from_f32() {
        for (f, t) in vec![(0.0f32, 0.0f32), (-0.5, -0.5), (0.5, 0.5), (1.0, 1.0), (-1.0, -1.0)] {
            assert_eq!(f32::from_sample(f), t);
            assert_eq!(IntoSample::<f32>::into_sample(f), t);
        }
    }

    #[test]
    fn f64_from_f32() {
        for (f, t) in vec![(0.0f32, 0.0f64), (-0.5, -0.5), (0.5, 0.5), (1.0, 1.0), (-1.0, -1.0)] {
            assert_eq!(f64::from_sample(f), t);
            assert_eq!(IntoSample::<f64>::into_sample(f), t);
            assert_eq!(f32::from_sample(t), f);
            assert_eq!(IntoSample::<f32>::into_sample(t), f);
        }
    }

    #[test]
    fn i64_from_i32() {
        for (f, t) in vec![(0i32, 0i64), (1, 0x0000000100000000), (-1, -4294967296), (0x7fffffff, 0x7fffffff00000000), (-0x80000000, -0x8000000000000000)] {
            assert_eq!(i64::from_sample(f), t);
            assert_eq!(IntoSample::<i64>::into_sample(f), t);
            assert_eq!(i32::from_sample(t), f);
            assert_eq!(IntoSample::<i32>::into_sample(t), f);
        }
    }

    #[test]
    fn u64_from_u32() {
        for (f, t) in vec![(0x80000000u32, 0x8000000000000000u64), (0x80000001u32, 0x8000000100000000), (0xFFFFFFFF, 0xFFFFFFFF00000000), (0x7fffffff, 0x7fffffff00000000), (0, 0)] {
            assert_eq!(u64::from_sample(f), t);
            assert_eq!(IntoSample::<u64>::into_sample(f), t);
            assert_eq!(u32::from_sample(t), f);
            assert_eq!(IntoSample::<u32>::into_sample(t), f);
        }
    }

    #[test]
    fn i64_from_u32() {
        for (f, t) in vec![(0x80000000u32, 0i64), (0x80000001u32, 0x0000000100000000), (0x7fffffffu32, -4294967296), (0xffffffff, 0x7fffffff00000000), (0, -0x8000000000000000)] {
            assert_eq!(i64::from_sample(f), t);
            assert_eq!(IntoSample::<i64>::into_sample(f), t);
            assert_eq!(u32::from_sample(t), f);
            assert_eq!(IntoSample::<u32>::into_sample(t), f);
        }
    }

    #[test]
    fn u64_from_i32() {
        for (f, t) in vec![(0i32, 0x8000000000000000u64), (1, 0x8000000100000000), (0x7FFFFFFF, 0xFFFFFFFF00000000), (0x7fffffff, 0xffffffff00000000), (-0x80000000, 0)] {
            assert_eq!(u64::from_sample(f), t);
            assert_eq!(IntoSample::<u64>::into_sample(f), t);
            assert_eq!(i32::from_sample(t), f);
            assert_eq!(IntoSample::<i32>::into_sample(t), f);
        }
    }
}
