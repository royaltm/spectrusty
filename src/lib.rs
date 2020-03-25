/*! # SPECTRUSTY
```text
+-----------+------------------+-----------+------------+
| component |       ULA        |    BUS    |    CPU     |
+-----------+------------------+-----------+------------+
|   trait   |    UlaCommon     | BusDevice |    Cpu     |
|           +------------------+-----------+------------+
| interface |  UlaAudioFrame   | 
+-----------+------------------+
```
* [UlaCommon][chip::ula::UlaCommon] 
* [UlaAudioFrame][chip::ula::UlaAudioFrame]
* [BusDevice][chip::BusDevice]
* [Cpu][z80emu::Cpu]

## Implementations

* [Ula][chip::ula::Ula]`<M, B, X, V>`
* [Ula128][chip::ula128::Ula128]`<B, X>`

### Generics

* `B`: [BusDevice][chip::BusDevice]
* `X`: [MemoryExtension][memory::MemoryExtension]
* `M`: [ZxMemory][memory::ZxMemory]
* `V`: [VideoFrame][video::VideoFrame]
*/
#![allow(dead_code)]
#![allow(unused_imports)]

#[macro_use]
extern crate bitflags;

pub use z80emu;

pub mod audio;
pub mod bus;
pub mod chip;
pub mod clock;
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
