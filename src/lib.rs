#![allow(dead_code)]
#![allow(unused_imports)]

#[macro_use]
extern crate bitflags;

pub use z80emu;

pub mod audio;
pub mod bus;
pub mod chip;
pub mod clock;
pub mod io;
pub mod memory;
pub mod formats;
pub mod video;
pub mod peripherals;
pub mod utils;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
