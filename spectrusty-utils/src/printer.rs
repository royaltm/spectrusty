use std::io;

mod epson_gfx;
mod image_spooler;

pub use epson_gfx::*;
pub use image_spooler::*;

/// A trait for dot matrix printer spoolers that can produce monochromatic images.
///
/// This trait defines an interface for the emulator program to be able to produce buffered images.
pub trait DotMatrixGfx {
    /// Returns `true` if the printer is currently capturing data. Returns `false` otherwise.
    fn is_spooling(&self) -> bool;
    /// Returns an approximate number of graphic lines already buffered.
    fn lines_buffered(&self) -> usize;
    /// Clears the buffered image data.
    fn clear(&mut self);
    /// Returns `true` if no image has been buffered. Otherwise returns `false`
    fn is_empty(&self) -> bool {
        self.lines_buffered() == 0
    }
    /// Renders already buffered image data as an SVG image written to the provided `target`.
    /// Returns `Ok(true)` if an image has been rendered. If there was no image data spooled, returns `Ok(false)`.
    fn write_svg_dot_gfx_lines(&self, description: &str, target: &mut dyn io::Write) -> io::Result<bool>;
    /// Renders already buffered image data as a monochrome 8-bit grascale image data.
    /// Returns `Some(width, height)` if an image has been rendered. If there was no image data spooled, returns `None`.
    ///
    /// You may use `image` crate to render an actual image in some popular format:
    ///
    /// ```text
    /// let mut buf: Vec<u8> = Vec::new();
    /// if let Some((width, height)) = printer.write_gfx_data(&mut buf) {
    ///     let img = image::ImageBuffer::<image::Luma<u8>, _>::from_vec(width, height, buf);
    ///     img.save("printed.png").unwrap();
    ///     // or alternatively
    ///     image::save_buffer("printed.png", &buf, width, height, image::ColorType::L8).unwrap();
    /// }
    /// ```
    fn write_gfx_data(&mut self, target: &mut Vec<u8>) -> Option<(u32, u32)>;
}
