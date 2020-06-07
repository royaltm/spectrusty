/*! # SPECTRUSTY

Here is a cheat-sheet of trait interfaces and structs that are the building blocks of SPECTRUSTY emulator.

## Traits

Implemented by ZX Spectrum's core chipset emulators.

Responsible for code execution, keyboard input, video and accessing peripheral devices:

| trait | function |
|-------|----------|
| [UlaCommon][chip::ula::UlaCommon]  | A grouping trait that includes all of the traits listed below in this table |
| [ControlUnit][chip::ControlUnit]   | Code execution, reset/nmi, frame/T-state counter, [BusDevice] peripherals access|
| [MemoryAccess][chip::MemoryAccess] | An access to onboard memory [ZxMemory] and memory extensions [MemoryExtension] |
| [Video][video::Video]              | Rendering video frame into pixel buffer, border color control |
| [KeyboardInterface][peripherals::KeyboardInterface] | Keyboard input control |
| [EarIn][chip::EarIn]               | EAR line input access |
| [MicOut][chip::MicOut]             | MIC line output access |

Audio output:

| trait | function |
|-------|----------|
| [UlaAudioFrame][chip::ula::UlaAudioFrame] | A grouping trait that includes all of the traits listed below in this table |
| [AudioFrame][audio::AudioFrame] | A helper trait for setting up [Blep] and ending audio frames |
| [EarMicOutAudioFrame][audio::EarMicOutAudioFrame] | Adds [Blep] steps from EAR/MIC output lines data |
| [EarInAudioFrame][audio::EarInAudioFrame] | Adds [Blep] steps from EAR/MIC output lines data |
| [AyAudioFrame][peripherals::ay::audio::AyAudioFrame] | Renders [Blep] steps from AY-3-891x sound processor | 

Emulated computer configurations:

| trait | function |
|-------|----------|
| [HostConfig][chip::HostConfig] | Defines CPU clock frequency and a video frame duration |

Associated traits implemented by special unit structs for driving audio and video rendering and memory contention:

| trait | function |
|-------|----------|
| [VideoFrame][video::VideoFrame] | Helps driving video rendering, provides arithmetic and conversion tools for [VideoTs] timestamps |
| [MemoryContention][clock::MemoryContention] | A trait that helps establish if an address is being contended |
| [AmpLevels][audio::AmpLevels] | Converts digital audio levels to sample amplitudes |

Implemented by other components:

| trait | implemented by | function |
|-------|----------------|----------|
| [Cpu][z80emu::Cpu] | Z80 CPU | Central processing unit |
| [ZxMemory] | System memory | An access to memory banks, pages, screens, attaching external ROM's |
| [MemoryExtension] | Memory extensions | Installs memory TRAPS to switch in and out external banks of memory |
| [BusDevice] | I/O peripheral devices | Establishes address and data BUS communication between CPU and peripherals |
| [Blep] | Bandwidth-Limited Pulse Buffer | An intermediate amplitude differences buffer for rendering square-waves |

## Structs

| struct | function |
|--------|----------|
| [VideoTs] | A video T-state timestamp, that consist of two components: a scan line number and a horizontal T-state |
| [VFrameTsCounter][clock::VFrameTsCounter]`<V, C>` | Counts T-states and handles the CPU clock contention |
| [Ula][chip::ula::Ula]`<M, B, X, V>` | A chipset for emulating ZX Spectrum 16/48k PAL/NTSC |
| [Ula128][chip::ula128::Ula128]`<B, X>` | A chipset for emulating ZX Spectrum 128k/+2 |
| [Ula3][chip::ula3::Ula3]`<B, X>` | A chipset for emulating ZX Spectrum +2A/+3 |

### Generic parameters

* `M`: A system memory that implements [ZxMemory] trait.
* `B`: A peripheral device that implements [BusDevice].
* `X`: A memory extension that implements [MemoryExtension].
* `V`: A unit struct that implements [VideoFrame][video::VideoFrame].
* `C`: A unit struct that implements [MemoryContention][clock::MemoryContention].

[BusDevice]: bus::BusDevice
[Blep]: audio::Blep
[ZxMemory]: memory::ZxMemory
[MemoryExtension]: memory::MemoryExtension
[VideoTs]: clock::VideoTs
*/
pub use spectrusty_core::z80emu;
pub use spectrusty_core::clock;
#[cfg(feature = "formats")] pub use spectrusty_formats as formats;
#[cfg(feature = "utils")] pub use spectrusty_utils as utils;

pub mod audio;
pub mod bus;
pub mod chip;
pub mod memory;
pub mod peripherals;
pub mod video;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
