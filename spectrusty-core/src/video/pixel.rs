/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
//! Building blocks for rendering pixel surfaces.
use core::slice::IterMut;

/// A trait for providing a way for placing pixels into byte buffers.
pub trait PixelBuffer<'a> {
    /// Specifies the type used for pixels.
    type Pixel: Copy;
    /// Should return a new instance of `PixelBuffer` implementation from the mutable slice of bytes representing
    /// a single line of pixels of the target buffer.
    fn from_line(line_buffer: &'a mut [u8]) -> Self;
    /// Puts the next `pixel` into the line buffer and increases an internal cursor position by a single pixel.
    ///
    /// If the internal buffer boundaries would be overflown, this method must not panic, but instead it
    /// should just return without writing anything to the underyling buffer.
    fn put_pixel(&mut self, pixel: Self::Pixel);
    /// Puts `count` number of `pixel` copies into the line buffer and increases an internal cursor position
    /// accordingly.
    ///
    /// If the internal buffer boundaries would be overflown, this method must not panic, but instead it
    /// should just return without writing anything to the underyling buffer.
    #[inline]
    fn put_pixels(&mut self, pixel: Self::Pixel, count: usize) {
        for _ in 0..count {
            self.put_pixel(pixel);
        }
    }
    /// Returns the size of a single pixel in bytes.
    #[inline]
    fn pixel_stride() -> usize {
        core::mem::size_of::<Self::Pixel>()
    }
}

/// A trait used for obtaining pixel colors.
pub trait Palette {
    /// Specifies the type used for pixels.
    type Pixel: Copy;
    /// Should return one of ZX Spectrum pixel colors:
    /// ```text
    /// index color   index color
    ///   0 - black     8 - bright black (same as black)
    ///   1 - blue      9 - bright blue
    ///   2 - red      10 - bright red
    ///   3 - magenta  11 - bright magenta
    ///   4 - green    12 - bright green
    ///   5 - cyan     13 - bright cyan
    ///   6 - yellow   14 - bright yellow
    ///   7 - white    15 - bright white
    /// ```
    #[inline]
    fn get_pixel(index: u8) -> Self::Pixel {
        Self::get_pixel_grb8(index_to_grb(index))
    }
    /// Should return a grayscale pixel with the intensity of one of the ZX Spectrum colors: [0, 15].
    ///
    /// See [Palette::get_pixel] for color values.
    fn get_pixel_gray(index: u8) -> Self::Pixel;
    /// Should return one of [ULAplus](https://faqwiki.zxnet.co.uk/wiki/ULAplus#GRB_palette_entries) colors.
    fn get_pixel_grb8(g3r3b2: u8) -> Self::Pixel;
    /// Should return a grayscale pixel (0 - black, 255 - full intensity white).
    fn get_pixel_gray8(value: u8) -> Self::Pixel;
}

/// A [PixelBuffer] tool for placing pixels into byte buffers using 3 `u8` element arrays of color channels
/// (3 bytes per pixel).
pub struct PixelBufA24<'a> {
    iter: IterMut<'a, [u8;3]>
}

/// A [PixelBuffer] tool for placing pixels into byte buffers using 4 `u8` element arrays of color channels
/// (4 bytes per pixel).
pub struct PixelBufA32<'a> {
    iter: IterMut<'a, [u8;4]>
}

/// A [PixelBuffer] tool for placing pixels into byte buffers using `u32` packed color channels.
pub struct PixelBufP32<'a> {
    iter: IterMut<'a, u32>
}

/// A [PixelBuffer] tool for placing pixels into byte buffers using `u16` packed color channels.
pub struct PixelBufP16<'a> {
    iter: IterMut<'a, u16>
}

/// A [PixelBuffer] tool for placing pixels into byte buffers using `u8` packed color channels.
pub struct PixelBufP8<'a> {
    iter: IterMut<'a, u8>
}

/// A color ZX Spectrum [Palette] implementation to be used with [PixelBufA24].
pub struct SpectrumPalRGB24;

/// A color ZX Spectrum [Palette] implementation to be used with [PixelBufA32].
pub struct SpectrumPalRGBA32;

/// A color ZX Spectrum [Palette] implementation to be used with [PixelBufA32].
pub struct SpectrumPalARGB32;

/// A color ZX Spectrum [Palette] implementation to be used with [PixelBufP32].
pub struct SpectrumPalA8R8G8B8;

/// A color ZX Spectrum [Palette] implementation to be used with [PixelBufP32].
pub struct SpectrumPalR8G8B8A8;

/// A color ZX Spectrum [Palette] implementation to be used with [PixelBufP16].
pub struct SpectrumPalR5G6B5;

/// A color ZX Spectrum [Palette] implementation to be used with [PixelBufP8].
pub struct SpectrumPalR3G3B2;

/// A grayscale ZX Spectrum [Palette] implementation to be used with [PixelBufA24].
pub struct GrayscalePalRGB24;

/// A grayscale ZX Spectrum [Palette] implementation to be used with [PixelBufA32].
pub struct GrayscalePalRGBA32;

/// A grayscale ZX Spectrum [Palette] implementation to be used with [PixelBufA32].
pub struct GrayscalePalARGB32;

/// A grayscale ZX Spectrum [Palette] implementation to be used with [PixelBufP32].
pub struct GrayscalePalA8R8G8B8;

/// A grayscale ZX Spectrum [Palette] implementation to be used with [PixelBufP32].
pub struct GrayscalePalR8G8B8A8;

/// A grayscale ZX Spectrum [Palette] implementation to be used with [PixelBufP16].
pub struct GrayscalePalR5G6B5;

/// A grayscale ZX Spectrum [Palette] implementation to be used with [PixelBufP8].
pub struct GrayscalePalR3G3B2;

#[inline]
fn index_to_grb(index: u8) -> u8 {
    match index & 15 {
         0 => 0b000_000_00,
         1 => 0b000_000_10,
         2 => 0b000_101_00,
         3 => 0b000_101_10,
         4 => 0b101_000_00,
         5 => 0b101_000_10,
         6 => 0b101_101_00,
         7 => 0b101_101_10,
         8 => 0b000_000_00,
         9 => 0b000_000_11,
        10 => 0b000_111_00,
        11 => 0b000_111_11,
        12 => 0b111_000_00,
        13 => 0b111_000_11,
        14 => 0b111_111_00,
        15 => 0b111_111_11,
        _ => unsafe { core::hint::unreachable_unchecked() }
    }
}

macro_rules! impl_pixel_buffer {
    ($pixel_buf:ty, $pixel:ty) => {
        impl<'a> PixelBuffer<'a> for $pixel_buf {
            type Pixel = $pixel;

            fn from_line(line_buffer: &'a mut [u8]) -> Self {
                let (_, pixels, _) = unsafe { line_buffer.align_to_mut::<Self::Pixel>() };
                let iter = pixels.iter_mut();
                Self { iter }
            }

            #[inline]
            fn put_pixel(&mut self, pixel: Self::Pixel) {
                if let Some(dest) = self.iter.next() {
                    *dest = pixel;
                }
            }

            #[inline]
            fn put_pixels(&mut self, pixel: Self::Pixel, count: usize) {
                for dest in self.iter.by_ref().take(count) {
                    *dest = pixel;
                }
            }
        }
    };
}

impl_pixel_buffer!(PixelBufA24<'a>, [u8;3]);
impl_pixel_buffer!(PixelBufA32<'a>, [u8;4]);
impl_pixel_buffer!(PixelBufP32<'a>, u32);
impl_pixel_buffer!(PixelBufP16<'a>, u16);
impl_pixel_buffer!(PixelBufP8<'a>,  u8);

macro_rules! impl_palette {
    ($palette:ty, $pixel:ty) => {
        impl Palette for $palette {
            type Pixel = $pixel;

            #[inline(always)]
            fn get_pixel_grb8(g3r3b2: u8) -> Self::Pixel {
                Self::pixel_grb(g3r3b2)
            }
            #[inline(always)]
            fn get_pixel_gray(index: u8) -> Self::Pixel {
                Self::pixel_gray_grb(index_to_grb(index))
            }
            #[inline(always)]
            fn get_pixel_gray8(value: u8) -> Self::Pixel {
                Self::pixel_gray_intensity(value)
            }
        }        
    };
}

macro_rules! impl_pixel_grb_const {
    ($palette:ty, $pixel:ty, $grayscale:ident) => {
        impl $palette {
            #[inline(always)]
            fn pixel_grb(g3r3b2: u8) -> $pixel {
                Self::COLORS_GRB[g3r3b2 as usize]
            }
            #[inline(always)]
            fn pixel_gray_grb(i: u8) -> $pixel {
                Self::pixel_gray_intensity($grayscale[i as usize])
            }
        }
    };
}

macro_rules! impl_pixel_grb_gray {
    ($palette:ty, $pixel:ty, $grayscale:ident) => {
        impl $palette {
            #[inline(always)]
            fn pixel_grb(g3r3b2: u8) -> $pixel {
                Self::pixel_gray_grb(g3r3b2)
            }
            #[inline(always)]
            fn pixel_gray_grb(i: u8) -> $pixel {
                Self::pixel_gray_intensity($grayscale[i as usize])
            }
        }
    };
}

const ALPHA_MAX: u8 = u8::max_value();

const fn grb_2r(grb: u8) -> u8 {
    ((grb >> 3) & 3) | (grb & 0b11100) | ((grb & 0b11100) << 3)
}

const fn grb_2g(grb: u8) -> u8 {
    ((grb >> 6) & 3) | ((grb >> 3) & 0b11100) | (grb & 0b11100000)
}

const fn grb_2b(grb: u8) -> u8 {
    (grb & 3) | ((grb << 3) & 0b11000) | (((grb << 2) | (grb << 1)) & 0b100) |
    ((grb << 6) & 0b11000000) | (((grb << 5) | (grb << 4)) & 0b100000)
}

#[inline(always)]
const fn pack_8888(a: u8, b: u8, c: u8, d: u8) -> u32 {
    ((a as u32) << 24) | ((b as u32) << 16) | ((c as u32) << 8) | (d as u32)
}

#[inline(always)]
const fn pack_565(a: u8, b: u8, c: u8) -> u16 {
    (((a as u16) >> 3) << 11) | (((b as u16) >> 2) << 5) | ((c as u16) >> 3)
}

#[inline(always)]
const fn pack_332(a: u8, b: u8, c: u8) -> u8 {
    ((a >> 5) << 5) | ((b >> 5) << 2) | (c >> 6)
}

const fn grayscale(r: u8, g: u8, b: u8) -> u8 {
    ((13933 * r as u32 + 46871 * g as u32 + 4732 * b as u32) >> 16) as u8
}

macro_rules! impl_palette_plus_formats {
    ([$($grb:expr),*]) => {
        const GRAYSCALE: [u8; 256] = [$(grayscale(grb_2r($grb), grb_2g($grb), grb_2b($grb))),*];

        impl SpectrumPalRGB24 {
            const COLORS_GRB: [[u8;3]; 256] = [
                $([grb_2r($grb), grb_2g($grb), grb_2b($grb)]),*
            ];
            #[inline(always)]
            fn pixel_gray_intensity(v: u8) -> [u8;3] { [v, v, v] }
        }
        impl_pixel_grb_const!(SpectrumPalRGB24, [u8;3], GRAYSCALE);

        impl SpectrumPalRGBA32 {
            const COLORS_GRB: [[u8;4]; 256] = [
                $([grb_2r($grb), grb_2g($grb), grb_2b($grb), ALPHA_MAX]),*
            ];
            #[inline(always)]
            fn pixel_gray_intensity(v: u8) -> [u8;4] { [v, v, v, ALPHA_MAX] }
        }
        impl_pixel_grb_const!(SpectrumPalRGBA32, [u8;4], GRAYSCALE);

        impl SpectrumPalARGB32 {
            const COLORS_GRB: [[u8;4]; 256] = [
                $([ALPHA_MAX, grb_2r($grb), grb_2g($grb), grb_2b($grb)]),*
            ];
            #[inline(always)]
            fn pixel_gray_intensity(v: u8) -> [u8;4] { [ALPHA_MAX, v, v, v] }
        }
        impl_pixel_grb_const!(SpectrumPalARGB32, [u8;4], GRAYSCALE);

        impl SpectrumPalA8R8G8B8 {
            const COLORS_GRB: [u32; 256] = [
                $(pack_8888(ALPHA_MAX, grb_2r($grb), grb_2g($grb), grb_2b($grb))),*
            ];
            #[inline(always)]
            fn pixel_gray_intensity(v: u8) -> u32 { pack_8888(ALPHA_MAX, v, v, v) }
        }
        impl_pixel_grb_const!(SpectrumPalA8R8G8B8, u32, GRAYSCALE);

        impl SpectrumPalR8G8B8A8 {
            const COLORS_GRB: [u32; 256] = [
                $(pack_8888(grb_2r($grb), grb_2g($grb), grb_2b($grb), ALPHA_MAX)),*
            ];
            #[inline(always)]
            fn pixel_gray_intensity(v: u8) -> u32 { pack_8888(v, v, v, ALPHA_MAX) }
        }
        impl_pixel_grb_const!(SpectrumPalR8G8B8A8, u32, GRAYSCALE);

        impl SpectrumPalR5G6B5 {
            const COLORS_GRB: [u16; 256] = [
                $(pack_565(grb_2r($grb), grb_2g($grb), grb_2b($grb))),*
            ];
            #[inline(always)]
            fn pixel_gray_intensity(v: u8) -> u16 { pack_565(v, v, v) }
        }
        impl_pixel_grb_const!(SpectrumPalR5G6B5, u16, GRAYSCALE);

        impl SpectrumPalR3G3B2 {
            const COLORS_GRB: [u8; 256] = [
                $(pack_332(grb_2r($grb), grb_2g($grb), grb_2b($grb))),*
            ];
            #[inline(always)]
            fn pixel_gray_intensity(v: u8) -> u8 { pack_332(v, v, v) }
        }
        impl_pixel_grb_const!(SpectrumPalR3G3B2, u8, GRAYSCALE);

        impl GrayscalePalRGB24 {
            #[inline(always)]
            fn pixel_gray_intensity(v: u8) -> [u8;3] { [v, v, v] }
        }
        impl_pixel_grb_gray!(GrayscalePalRGB24, [u8;3], GRAYSCALE);

        impl GrayscalePalRGBA32 {
            #[inline(always)]
            fn pixel_gray_intensity(v: u8) -> [u8;4] { [v, v, v, ALPHA_MAX] }
        }
        impl_pixel_grb_gray!(GrayscalePalRGBA32, [u8;4], GRAYSCALE);

        impl GrayscalePalARGB32 {
            #[inline(always)]
            fn pixel_gray_intensity(v: u8) -> [u8;4] { [ALPHA_MAX, v, v, v] }
        }
        impl_pixel_grb_gray!(GrayscalePalARGB32, [u8;4], GRAYSCALE);

        impl GrayscalePalA8R8G8B8 {
            #[inline(always)]
            fn pixel_gray_intensity(v: u8) -> u32 { pack_8888(ALPHA_MAX, v, v, v) }
        }
        impl_pixel_grb_gray!(GrayscalePalA8R8G8B8, u32, GRAYSCALE);

        impl GrayscalePalR8G8B8A8 {
            #[inline(always)]
            fn pixel_gray_intensity(v: u8) -> u32 { pack_8888(v, v, v, ALPHA_MAX) }
        }
        impl_pixel_grb_gray!(GrayscalePalR8G8B8A8, u32, GRAYSCALE);

        impl GrayscalePalR5G6B5 {
            #[inline(always)]
            fn pixel_gray_intensity(v: u8) -> u16 { pack_565(v, v, v) }
        }
        impl_pixel_grb_gray!(GrayscalePalR5G6B5, u16, GRAYSCALE);

        impl GrayscalePalR3G3B2 {
            #[inline(always)]
            fn pixel_gray_intensity(v: u8) -> u8 { pack_332(v, v, v) }
        }
        impl_pixel_grb_gray!(GrayscalePalR3G3B2, u8, GRAYSCALE);
    };
}

// let colors = [
//     (  0,  0,  0),
//     ( 21, 21,201),
//     (202, 33, 33),
//     (203, 38,203),
//     ( 44,203, 44),
//     ( 47,204,204),
//     (205,205, 53),
//     (205,205,205),
//     (  0,  0,  0),
//     ( 27, 27,251),
//     (252, 41, 41),
//     (252, 47,252),
//     ( 55,253, 55),
//     ( 59,254,254),
//     (255,255, 65),
//     (255,255,255)
// ];

// constant eval iterators where are you...
impl_palette_plus_formats!([0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,38,39,40,41,42,43,44,45,46,47,48,49,50,51,52,53,54,55,56,57,58,59,60,61,62,63,64,65,66,67,68,69,70,71,72,73,74,75,76,77,78,79,80,81,82,83,84,85,86,87,88,89,90,91,92,93,94,95,96,97,98,99,100,101,102,103,104,105,106,107,108,109,110,111,112,113,114,115,116,117,118,119,120,121,122,123,124,125,126,127,128,129,130,131,132,133,134,135,136,137,138,139,140,141,142,143,144,145,146,147,148,149,150,151,152,153,154,155,156,157,158,159,160,161,162,163,164,165,166,167,168,169,170,171,172,173,174,175,176,177,178,179,180,181,182,183,184,185,186,187,188,189,190,191,192,193,194,195,196,197,198,199,200,201,202,203,204,205,206,207,208,209,210,211,212,213,214,215,216,217,218,219,220,221,222,223,224,225,226,227,228,229,230,231,232,233,234,235,236,237,238,239,240,241,242,243,244,245,246,247,248,249,250,251,252,253,254,255]);
impl_palette!(SpectrumPalRGB24,     [u8;3]);
impl_palette!(SpectrumPalRGBA32,    [u8;4]);
impl_palette!(SpectrumPalARGB32,    [u8;4]);
impl_palette!(SpectrumPalA8R8G8B8,  u32);
impl_palette!(SpectrumPalR8G8B8A8,  u32);
impl_palette!(SpectrumPalR5G6B5,    u16);
impl_palette!(SpectrumPalR3G3B2,    u8);
impl_palette!(GrayscalePalRGB24,    [u8;3]);
impl_palette!(GrayscalePalRGBA32,   [u8;4]);
impl_palette!(GrayscalePalARGB32,   [u8;4]);
impl_palette!(GrayscalePalA8R8G8B8, u32);
impl_palette!(GrayscalePalR8G8B8A8, u32);
impl_palette!(GrayscalePalR5G6B5,   u16);
impl_palette!(GrayscalePalR3G3B2,   u8);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_palette_works() {
        for i in (0..=255).step_by(16) {
            assert_eq!(SpectrumPalRGB24::get_pixel(i + 0),  [         0,          0,          0]);
            assert_eq!(SpectrumPalRGB24::get_pixel(i + 1),  [         0,          0, 0b10110110]);
            assert_eq!(SpectrumPalRGB24::get_pixel(i + 2),  [0b10110110,          0,          0]);
            assert_eq!(SpectrumPalRGB24::get_pixel(i + 3),  [0b10110110,          0, 0b10110110]);
            assert_eq!(SpectrumPalRGB24::get_pixel(i + 4),  [         0, 0b10110110,          0]);
            assert_eq!(SpectrumPalRGB24::get_pixel(i + 5),  [         0, 0b10110110, 0b10110110]);
            assert_eq!(SpectrumPalRGB24::get_pixel(i + 6),  [0b10110110, 0b10110110,          0]);
            assert_eq!(SpectrumPalRGB24::get_pixel(i + 7),  [0b10110110, 0b10110110, 0b10110110]);
            assert_eq!(SpectrumPalRGB24::get_pixel(i + 8),  [         0,          0,          0]);
            assert_eq!(SpectrumPalRGB24::get_pixel(i + 9),  [         0,          0, 0b11111111]);
            assert_eq!(SpectrumPalRGB24::get_pixel(i + 10), [0b11111111,          0,          0]);
            assert_eq!(SpectrumPalRGB24::get_pixel(i + 11), [0b11111111,          0, 0b11111111]);
            assert_eq!(SpectrumPalRGB24::get_pixel(i + 12), [         0, 0b11111111,          0]);
            assert_eq!(SpectrumPalRGB24::get_pixel(i + 13), [         0, 0b11111111, 0b11111111]);
            assert_eq!(SpectrumPalRGB24::get_pixel(i + 14), [0b11111111, 0b11111111,          0]);
            assert_eq!(SpectrumPalRGB24::get_pixel(i + 15), [0b11111111, 0b11111111, 0b11111111]);

            assert_eq!(GrayscalePalRGB24::get_pixel(i + 0),  [  0,  0,  0]);
            assert_eq!(GrayscalePalRGB24::get_pixel(i + 1),  [ 13, 13, 13]);
            assert_eq!(GrayscalePalRGB24::get_pixel(i + 2),  [ 38, 38, 38]);
            assert_eq!(GrayscalePalRGB24::get_pixel(i + 3),  [ 51, 51, 51]);
            assert_eq!(GrayscalePalRGB24::get_pixel(i + 4),  [130,130,130]);
            assert_eq!(GrayscalePalRGB24::get_pixel(i + 5),  [143,143,143]);
            assert_eq!(GrayscalePalRGB24::get_pixel(i + 6),  [168,168,168]);
            assert_eq!(GrayscalePalRGB24::get_pixel(i + 7),  [182,182,182]);
            assert_eq!(GrayscalePalRGB24::get_pixel(i + 8),  [  0,  0,  0]);
            assert_eq!(GrayscalePalRGB24::get_pixel(i + 9),  [ 18, 18, 18]);
            assert_eq!(GrayscalePalRGB24::get_pixel(i + 10), [ 54, 54, 54]);
            assert_eq!(GrayscalePalRGB24::get_pixel(i + 11), [ 72, 72, 72]);
            assert_eq!(GrayscalePalRGB24::get_pixel(i + 12), [182,182,182]);
            assert_eq!(GrayscalePalRGB24::get_pixel(i + 13), [200,200,200]);
            assert_eq!(GrayscalePalRGB24::get_pixel(i + 14), [236,236,236]);
            assert_eq!(GrayscalePalRGB24::get_pixel(i + 15), [255,255,255]);
        }
    }

    #[test]
    fn pixel_palette_grb_works() {
        assert_eq!(SpectrumPalRGB24::get_pixel_grb8(0), [0, 0, 0]);
        assert_eq!(SpectrumPalRGB24::get_pixel_grb8(0b000_001_00), [0b00100100, 0, 0]);
        assert_eq!(SpectrumPalRGB24::get_pixel_grb8(0b001_010_00), [0b01001001, 0b00100100, 0]);
        assert_eq!(SpectrumPalRGB24::get_pixel_grb8(1), [0, 0, 0b01101101]);
        assert_eq!(SpectrumPalRGB24::get_pixel_grb8(0b000_101_01), [0b10110110, 0, 0b01101101]);
        assert_eq!(SpectrumPalRGB24::get_pixel_grb8(0b110_111_01), [0b11111111, 0b11011011, 0b01101101]);
        assert_eq!(SpectrumPalRGB24::get_pixel_grb8(2), [0, 0, 0b10110110]);
        assert_eq!(SpectrumPalRGB24::get_pixel_grb8(0b011_011_10), [0b01101101, 0b01101101, 0b10110110]);
        assert_eq!(SpectrumPalRGB24::get_pixel_grb8(3), [0, 0, 0b11111111]);
        assert_eq!(SpectrumPalRGB24::get_pixel_grb8(255), [255, 255, 255]);
        assert_eq!(SpectrumPalRGBA32::get_pixel_grb8(0), [0, 0, 0, 255]);
        assert_eq!(SpectrumPalRGBA32::get_pixel_grb8(0b111_011_01), [0b01101101, 0b11111111, 0b01101101, 255]);
        assert_eq!(SpectrumPalRGBA32::get_pixel_grb8(255), [255, 255, 255, 255]);
        assert_eq!(SpectrumPalARGB32::get_pixel_grb8(0), [255, 0, 0, 0]);
        assert_eq!(SpectrumPalARGB32::get_pixel_grb8(0b111_011_01), [255, 0b01101101, 0b11111111, 0b01101101]);
        assert_eq!(SpectrumPalARGB32::get_pixel_grb8(255), [255, 255, 255, 255]);
        assert_eq!(SpectrumPalR8G8B8A8::get_pixel_grb8(0), 255);
        assert_eq!(SpectrumPalR8G8B8A8::get_pixel_grb8(0b111_011_01), 0b01101101_11111111_01101101_11111111);
        assert_eq!(SpectrumPalR8G8B8A8::get_pixel_grb8(255), 0xffff_ffff);
        assert_eq!(SpectrumPalA8R8G8B8::get_pixel_grb8(0), 0xff00_0000);
        assert_eq!(SpectrumPalA8R8G8B8::get_pixel_grb8(0b111_011_01), 0b11111111_01101101_11111111_01101101);
        assert_eq!(SpectrumPalA8R8G8B8::get_pixel_grb8(255), 0xffff_ffff);
        assert_eq!(SpectrumPalR5G6B5::get_pixel_grb8(0), 0);
        assert_eq!(SpectrumPalR5G6B5::get_pixel_grb8(0b111_011_01), 0b01101_111111_01101);
        assert_eq!(SpectrumPalR5G6B5::get_pixel_grb8(255), 0xffff);
        assert_eq!(SpectrumPalR3G3B2::get_pixel_grb8(0), 0);
        assert_eq!(SpectrumPalR3G3B2::get_pixel_grb8(0b111_011_01), 0b011_111_01);
        assert_eq!(SpectrumPalR3G3B2::get_pixel_grb8(255), 0xff);
    }

    #[test]
    fn pixel_palette_gray_works() {
        let grayscale_u16 = |v| (((v as u16) >> 3) << 11)|(((v as u16) >> 2) << 5)|((v as u16) >> 3);
        let grayscale_u8 = |v| ((v >> 5) << 5)|((v >> 5) << 2)|(v >> 6);
        for i in 0..=255u8 {
            let v = GRAYSCALE[i as usize];
            assert_eq!(SpectrumPalRGB24::get_pixel_gray8(i), [i, i, i]);
            assert_eq!(GrayscalePalRGB24::get_pixel_gray8(i), [i, i, i]);
            assert_eq!(GrayscalePalRGB24::get_pixel_grb8(i), [v, v, v]);
            assert_eq!(SpectrumPalRGBA32::get_pixel_gray8(i), [i, i, i, 255]);
            assert_eq!(GrayscalePalRGBA32::get_pixel_gray8(i), [i, i, i, 255]);
            assert_eq!(GrayscalePalRGBA32::get_pixel_grb8(i), [v, v, v, 255]);
            assert_eq!(SpectrumPalARGB32::get_pixel_gray8(i), [255, i, i, i]);
            assert_eq!(GrayscalePalARGB32::get_pixel_gray8(i), [255, i, i, i]);
            assert_eq!(GrayscalePalARGB32::get_pixel_grb8(i), [255, v, v, v]);
            assert_eq!(SpectrumPalA8R8G8B8::get_pixel_gray8(i), u32::from_be_bytes([255, i, i, i]));
            assert_eq!(GrayscalePalA8R8G8B8::get_pixel_gray8(i), u32::from_be_bytes([255, i, i, i]));
            assert_eq!(GrayscalePalA8R8G8B8::get_pixel_grb8(i), u32::from_be_bytes([255, v, v, v]));
            assert_eq!(SpectrumPalR8G8B8A8::get_pixel_gray8(i), u32::from_be_bytes([i, i, i, 255]));
            assert_eq!(GrayscalePalR8G8B8A8::get_pixel_gray8(i), u32::from_be_bytes([i, i, i, 255]));
            assert_eq!(GrayscalePalR8G8B8A8::get_pixel_grb8(i), u32::from_be_bytes([v, v, v, 255]));
            assert_eq!(SpectrumPalR5G6B5::get_pixel_gray8(i), grayscale_u16(i));
            assert_eq!(GrayscalePalR5G6B5::get_pixel_gray8(i), grayscale_u16(i));
            assert_eq!(GrayscalePalR5G6B5::get_pixel_grb8(i), grayscale_u16(v));
            assert_eq!(SpectrumPalR3G3B2::get_pixel_gray8(i), grayscale_u8(i));
            assert_eq!(GrayscalePalR3G3B2::get_pixel_gray8(i), grayscale_u8(i));
            assert_eq!(GrayscalePalR3G3B2::get_pixel_grb8(i), grayscale_u8(v));
        }
    }
}
