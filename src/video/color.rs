//! Contains a [PixelRgb].
use derive_more::*;
use std::ptr;

/// A struct representing one or more pixels in the linear RGB color space.
#[derive(Debug, Copy, Clone, Default, PartialEq,
    Neg, Add, Sub, Mul, Div, Rem, AddAssign, SubAssign, MulAssign, DivAssign, RemAssign)]
pub struct PixelRgb {
    pub r: f32,
    pub g: f32,
    pub b: f32
}

impl PixelRgb {
    /// Creates an instance of [PixelRgb] from RGB color components.
    #[inline]
    pub fn new(r: f32, g: f32, b: f32) -> PixelRgb {
        PixelRgb {r, g, b}
    }
}

/// An iterator of [PixelRgb] color components.
///
/// The iterator is being created with a [PixelRgb::iter_rgb_values] method.
///
/// If a "use-simd" feature is enabled the iterator provides a color component values for all 8 pixels.
/// In this instance the order will be `[r0, g0, b0, r1, g1, b1, ..., r7, g7, b7]`.
#[derive(Clone)]
pub struct RgbIter {
    rgb: [f32; RgbIter::LEN],
    offs: usize
}

impl RgbIter {
    /// The number of components of each pixel.
    const LEN: usize = 3;
}

/// An iterator of [PixelRgb] color components plus an alpha component.
///
/// The iterator is being created with a [PixelRgb::iter_rgba_values] method.
#[derive(Clone)]
pub struct RgbaIter {
    rgba: [f32; RgbaIter::LEN],
    offs: usize
}

impl RgbaIter {
    /// The number of components of each pixel.
    const LEN: usize = 4;
}


macro_rules! rgb_iterator_impl {
    ($name:ident, $prop:ident) => {
        impl ExactSizeIterator for $name {
            #[inline]
            fn len(&self) -> usize { $name::LEN - self.offs }
        }

        impl Iterator for $name {
            type Item = f32;

            #[inline]
            fn next(&mut self) -> Option<f32> {
                match self.offs {
                    offs if offs < $name::LEN => {
                        self.offs += 1;
                        Some(unsafe { ptr::read(self.$prop.as_ptr().add(offs)) })
                    },
                    _ => None
                }
            }

            #[inline]
            fn size_hint(&self) -> (usize, Option<usize>) {
                let len = self.len();
                (len, Some(len))
            }
        }
    }
}

rgb_iterator_impl!(RgbIter, rgb);
rgb_iterator_impl!(RgbaIter, rgba);

impl PixelRgb {
    /// Creates a [RgbIter] from this instance of [PixelRgb].
    #[inline]
    pub fn iter_rgb_values(self) -> RgbIter {
        let PixelRgb { r, g, b } = self;
        let rgb = [r, g, b];
        RgbIter { rgb, offs: 0 }
    }

    /// Creates a [RgbaIter] from this instance of [PixelRgb].
    #[inline]
    pub fn iter_rgba_values(self, alpha: f32) -> RgbaIter {
        let PixelRgb { r, g, b } = self;
        let rgba = [r, g, b, alpha];
        RgbaIter { rgba, offs: 0 }
    }

    #[inline]
    pub fn from_palette(color: usize, palette: &[(f32,f32,f32)]) -> PixelRgb {
        let (r, g, b) = palette[color];
        PixelRgb { r, g, b }
    }

    /// Creates an instance of a [PixelRgb] from HSV color components.
    ///
    /// `hue` should be in the range: `[0, 2)` and will be normalized.
    /// `sat` and `val` should be in the range: `[0, 1]` and won't be normalized.
    #[allow(clippy::many_single_char_names)]
    #[inline]
    pub fn from_hsv(hue: f32, sat: f32, val: f32) -> PixelRgb {
        let c = val * sat;
        let h = (hue - ((hue / 2.0).floor() * 2.0)) * 3.0;
        let x = c * (1.0 - (h % 2.0 - 1.0).abs());
        let m = val - c;

        let (r, g, b) = {
            if h < 1.0 {
                ( c+m, x+m, m   )
            } else if h < 2.0 {
                ( x+m, c+m, m   )
            } else if h < 3.0 {
                ( m  , c+m, x+m )
            } else if h < 4.0 {
                ( m  , x+m, c+m )
            } else if h < 5.0 {
                ( x+m, m  , c+m )
            } else {
                ( c+m, m  , x+m )
            }
        };
        PixelRgb { r, g, b }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        assert_eq!(PixelRgb::from_hsv(0.0, 0.0, 0.0), PixelRgb { r: 0.0, g: 0.0, b: 0.0, });
        assert_eq!(PixelRgb::from_hsv(0.0, 0.0, 1.0), PixelRgb { r: 1.0, g: 1.0, b: 1.0, });
        assert_eq!(PixelRgb::from_hsv(0.0, 1.0, 1.0), PixelRgb { r: 1.0, g: 0.0, b: 0.0, });
        assert_eq!(PixelRgb::from_hsv(2.0, 1.0, 1.0), PixelRgb { r: 1.0, g: 0.0, b: 0.0, });
        assert_eq!(PixelRgb::from_hsv(1.0, 1.0, 1.0), PixelRgb { r: 0.0, g: 1.0, b: 1.0, });
        assert_eq!(PixelRgb::from_hsv(1.0, 1.0, 2.0), PixelRgb { r: 0.0, g: 2.0, b: 2.0, });
        assert_eq!(PixelRgb::from_hsv(1.0, 0.5, 0.5), PixelRgb { r: 0.25, g: 0.5, b: 0.5, });
        assert_eq!(PixelRgb::from_hsv(-1.0, 1.0, 1.0), PixelRgb { r: 0.0, g: 1.0, b: 1.0, });
        assert_eq!(PixelRgb::from_hsv(-0.5, 1.0, 1.0), PixelRgb::from_hsv(1.5, 1.0, 1.0));
    }

    #[test]
    fn iterator_works() {
        let pixel = PixelRgb::new(0.0, 0.5, 1.0);
        let rgb: Vec<f32> = pixel.iter_rgb_values().collect();
        assert_eq!(rgb, vec![0.0, 0.5, 1.0]);
        let rgba: Vec<f32> = pixel.iter_rgba_values(0.25).collect();
        assert_eq!(rgba, vec![0.0, 0.5, 1.0, 0.25]);
    }
}