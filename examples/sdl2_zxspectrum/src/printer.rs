use image;
use zxspecemu::bus::zxprinter::{PIXEL_LINE_WIDTH, Spooler};

#[derive(Clone, Default)]
pub struct ImageSpooler {
    counter: u16,
    spooling: bool,
    buf: Vec<u8>
}

impl ImageSpooler {
    pub fn is_spooling(&self) -> bool {
        self.spooling
    }

    pub fn save_image(&mut self) {
        let height: u32 = (self.buf.len() as u32 + PIXEL_LINE_WIDTH - 1) / PIXEL_LINE_WIDTH;
        if height == 0 {
            return;
        }
        let counter = self.counter;
        self.counter = counter.wrapping_add(1);
        self.buf.resize((PIXEL_LINE_WIDTH * height) as usize, u8::max_value());
        let name = format!("printer_out_{:04}.png", counter);
        image::save_buffer(name, &self.buf, PIXEL_LINE_WIDTH, height, image::ColorType::L8).unwrap();
    }

    pub fn clear(&mut self) {
        self.buf.clear();
    }
}

impl Spooler for ImageSpooler {
    fn motor_on(&mut self) {
        self.spooling = true;
    }

    fn push_line(&mut self, line: &[u8]) {
        self.buf.reserve(PIXEL_LINE_WIDTH as usize);
        for mut b in line.iter().copied() {
            for _ in 0..8 {
                self.buf.push(if b & 0x80 == 0x80 { u8::min_value() } else { u8::max_value() });
                b = b.rotate_left(1);
            }
        }
        println!("line spooled; {}", self.buf.len() / PIXEL_LINE_WIDTH as usize);
    }

    fn motor_off(&mut self) {
        self.spooling = false;
        println!("end of print");
    }
}
