use crate::clock::FTs;

pub trait AudioSample: Copy + Default {
    #[inline(always)]
    fn zero() -> Self {
        Self::default()
    }
}

pub trait FromSample<S> {
    fn from_sample(other: S) -> Self;
}

pub trait IntoSample<S> {
    fn into_sample(self) -> S;
}

pub trait SampleTime: Copy {
    fn time_rate(sample_rate: u32, ts_per_second: u32) -> Self;
    fn at_timestamp(self, timestamp: FTs) -> Self;
}

pub trait SampleDelta: Copy {
    /// The difference between `after` and `before`.
    fn sample_delta(self, after: Self) -> Option<Self>;
}

pub trait MulNorm {
    fn saturating_add(self, other: Self) -> Self;
    fn mul_norm(self, other: Self) -> Self;
}

impl SampleTime for f64 {
    #[inline]
    fn at_timestamp(self, timestamp: FTs) -> Self {
        self * timestamp as f64
    }
    #[inline]
    fn time_rate(sample_rate: u32, ts_per_second: u32) -> Self {
        sample_rate as f64 / ts_per_second as f64
    }
}

impl SampleDelta for f32 {
    #[inline]
    fn sample_delta(self, after: f32) -> Option<f32> {
        let delta = after - self;
        if delta.abs() > core::f32::EPSILON {
            Some(delta)
        }
        else {
            None
        }
    }
}

impl SampleDelta for i16 {
    #[inline]
    fn sample_delta(self, after: i16) -> Option<i16> {
        let delta = after - self;
        if delta != 0 {
            Some(delta)
        }
        else {
            None
        }
    }
}

impl AudioSample for f32 {}
impl AudioSample for i16 {}
impl AudioSample for i8 {}
impl AudioSample for u16 {
    #[inline(always)]
    fn zero() -> Self {
        0x8000
    }    
}
impl AudioSample for u8 {
    #[inline(always)]
    fn zero() -> Self {
        0x80
    }
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
                    other as $ft / -(<$int>::min_value() as $ft)
                } else {
                    other as $ft / <$int>::max_value() as $ft
                }
            }
        }

        impl FromSample<$ft> for $int {
            #[inline]
            fn from_sample(other: $ft) -> $int {
                if other >= 0.0 {
                    (other * <$int>::max_value() as $ft) as $int
                } else {
                    (-other * <$int>::min_value() as $ft) as $int
                }
            }
        }

        impl FromSample<$uint> for $ft {
            #[inline]
            fn from_sample(other: $uint) -> $ft {
                <$ft>::from_sample(<$int>::from_sample(other))
            }
        }

        impl FromSample<$uint> for $int {
            #[inline]
            fn from_sample(other: $uint) -> $int {
                other.wrapping_sub(<$int>::min_value() as $uint) as $int
            }
        }

        impl FromSample<$int> for $uint {
            #[inline]
            fn from_sample(other: $int) -> $uint {
                other.wrapping_sub(<$int>::min_value()) as $uint
            }
        }

        impl FromSample<$ft> for $uint {
            #[inline]
            fn from_sample(other: $ft) -> $uint {
                <$uint>::from_sample(<$int>::from_sample(other))
            }
        }

    };
}

impl_from_sample!(i8, u8, f32);
impl_from_sample!(i16, u16, f32);

impl MulNorm for f32 {
    fn saturating_add(self, other: f32) -> f32 {
        let res = self + other;
        if res > 1.0 {
            1.0
        }
        else if res < -1.0 {
            -1.0
        }
        else {
            res
        }
    }

    fn mul_norm(self, other: f32) -> f32 {
        self * other
    }
}

impl MulNorm for i16 {
    fn saturating_add(self, other: i16) -> i16 {
        self.saturating_add(other)
    }

    fn mul_norm(self, other: i16) -> i16 {
        ((self as i32 * other as i32) >> 15) as i16
    }
}

impl MulNorm for i32 {
    fn saturating_add(self, other: i32) -> i32 {
        self.saturating_add(other)
    }

    fn mul_norm(self, other: i32) -> i32 {
        ((self as i64 * other as i64) >> 31) as i32
    }
}

#[cfg(test)]
mod test {
    use super::*;

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
}
