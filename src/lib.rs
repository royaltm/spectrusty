#![allow(dead_code)]
#![allow(unused_imports)]

#[macro_use]
extern crate bitflags;

// #[macro_use]
// extern crate serde_derive;

pub mod audio;
pub mod cpu_debug;
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
