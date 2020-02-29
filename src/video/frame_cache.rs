pub struct PixelPack8 {
    pub ink: u8,
    pub attr: u8
}

pub trait FrameImageProducer: Iterator<Item=PixelPack8> {
    fn next_line(&mut self);
}
