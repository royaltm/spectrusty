use super::color::PixelRgb;

/// The trait for putting pixels into byte buffers.
pub trait PixelBuffer {
    /// Specifies how many bytes a single pixel occupies.
    const PIXEL_BYTES: usize;
    /// Puts bytes from a `pixel` into the provided `buffer` using a provided writer.
    fn put_pixel<'a, I: Iterator<Item = &'a mut u8>>(writer: &mut I, pixel: PixelRgb);
    fn put_pixels<'a, I: Iterator<Item = &'a mut u8>>(writer: &mut I, pixel: PixelRgb, count: usize);
}

/// A [PixelBuffer] tool for a RGB24 buffer (3 bytes/pixel: red, green, blue).
pub struct PixelBufRGB24;

impl PixelBuffer for PixelBufRGB24 {
    const PIXEL_BYTES: usize = 3;

    #[inline]
    fn put_pixel<'a, I>(writer: &mut I, pixel: PixelRgb)
        where I: Iterator<Item = &'a mut u8>
    {
        for (color, ptr) in pixel.iter_rgb_values().zip(writer) {
            *ptr = color.to_color_u8clamped();
        }
    }

    #[inline]
    fn put_pixels<'a, I>(writer: &mut I, pixel: PixelRgb, count: usize)
        where I: Iterator<Item = &'a mut u8>
    {
        for _ in 0..count {
            for color in pixel.iter_rgb_values() {
                if let Some(ptr) = writer.next() {
                    *ptr = color.to_color_u8clamped();
                }
                else {
                    return;
                }
            }
        }
    }
}

/// A [PixelBuffer] tool for a RGBA8 buffer (4 bytes/pixel: red, green, blue, alpha).
pub struct PixelBufRGBA8;

impl PixelBuffer for PixelBufRGBA8 {
    const PIXEL_BYTES: usize = 4;

    #[inline]
    fn put_pixel<'a, I>(writer: &mut I, pixel: PixelRgb)
        where I: Iterator<Item = &'a mut u8>
    {
        for (color, ptr) in pixel.iter_rgba_values(1.0).zip(writer) {
            *ptr = color.to_color_u8clamped();
        }
    }

    #[inline]
    fn put_pixels<'a, I>(writer: &mut I, pixel: PixelRgb, count: usize)
        where I: Iterator<Item = &'a mut u8>
    {
        for _ in 0..count {
            for color in pixel.iter_rgba_values(1.0) {
                if let Some(ptr) = writer.next() {
                    *ptr = color.to_color_u8clamped();
                }
                else {
                    return;
                }
            }
        }
    }
}

/// Provides a method of converting color part from a `f32` type to a `u8`.
pub trait ToColor8 {
    fn to_color_u8clamped(&self) -> u8;
}

impl ToColor8 for f32 {
    #[inline]
    fn to_color_u8clamped(&self) -> u8 { (self.abs().min(1.0) * 255.0) as u8 }
}