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
* [BusDevice][bus::BusDevice]
* [Cpu][z80emu::Cpu]

## Implementations

* [Ula][chip::ula::Ula]`<M, B, X, V>`
* [Ula128][chip::ula128::Ula128]`<B, X>`

### Generics

* `B`: [BusDevice][bus::BusDevice]
* `X`: [MemoryExtension][memory::MemoryExtension]
* `M`: [ZxMemory][memory::ZxMemory]
* `V`: [VideoFrame][video::VideoFrame]
*/
pub use spectrusty_core::z80emu;
#[cfg(feature = "peripherals")] pub use spectrusty_peripherals as peripherals;
#[cfg(feature = "formats")] pub use spectrusty_formats as formats;
#[cfg(feature = "utils")] pub use spectrusty_utils as utils;

#[macro_use] extern crate bitflags;

pub mod audio;
pub mod bus;
pub mod chip;
pub mod clock;
pub mod memory;
pub mod video;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
