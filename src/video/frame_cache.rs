/// A trait for data iterators being used to render an image of a video frame.
///
/// This emulates the Spectrum's ULA reading two bytes of video data to compose a single cell
/// consisting of 8 (or 16 in hi-res) pixels.
///
/// In low resolution mode the first byte returned by the iterator is interpreted as INK/PAPER bit
/// selector and the second as a color attribute.
/// In hi-res mode both bytes are being used to render 16 monochrome pixels.
pub trait VideoFrameDataIterator: Iterator<Item=(u8, u8)> {
    /// Forwards the iterator to the next video line.
    fn next_line(&mut self);
}
