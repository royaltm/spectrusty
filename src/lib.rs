#![allow(dead_code)]

#[macro_use]
extern crate bitflags;

pub mod audio;
pub mod bus;
pub mod chip;
pub mod clock;
pub mod io;
pub mod memory;
pub mod formats;
pub mod video;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
