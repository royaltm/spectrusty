/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    SPECTRUSTY is free software: you can redistribute it and/or modify it under
    the terms of the GNU Lesser General Public License (LGPL) as published
    by the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    SPECTRUSTY is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU Lesser General Public License for more details.

    You should have received a copy of the GNU Lesser General Public License
    along with this program.  If not, see <https://www.gnu.org/licenses/>.

    Author contact information: see Cargo.toml file, section [package.authors].
*/
/*! # SPECTRUSTY

**S P E C T R U S T Y**
<svg viewBox="0 0 350 100" width="3em" height="1em" style="padding-top:4px" xmlns="http://www.w3.org/2000/svg">
  <polygon points="0,100 100,0 150,0 50,100" fill="#D80000" stroke="#D80000"/>
  <polygon points="50,100 150,0 200,0 100,100" fill="#D8D800" stroke="#D8D800"/>
  <polygon points="100,100 200,0 250,0 150,100" fill="#00D800" stroke="#00D800"/>
  <polygon points="150,100 250,0 300,0 200,100" fill="#00D8D8" stroke="#00D8D8"/>
</svg>
is a library for building emulators based on various ZX Spectrum computer models and clones.

Components of the library can also be used to implement chiptune players, format converters, and more.

Because of its vast scope, **SPECTRUSTY** has been split into several dependent crates and one extra crate
with complementary utilities. The additional crates can be used separately or through this library.

* `spectrusty` - this library.
* [spectrusty-core] - defines basic traits and structs.
* [spectrusty-audio] - tools for synthesizing and playing audio samples.
* [spectrusty-formats] - file formats and related utilities.
* [spectrusty-peripherals] - emulators of various peripheral devices.
* [z80emu] - Zilog's Z80 CPU family emulator, re-exported as `z80emu`.

The separate crate [spectrusty-utils], provides additional utilities, like TAPe
organizers, tape ROM auto-loader, printer utilities, and keyboard helpers for various platforms.

Several features control which components will be included:

* `"audio"` - includes [spectrusty-audio] which is re-exported as [audio].
* `"formats"` - includes [spectrusty-formats] which is re-exported as [formats].
* `"peripherals"` - includes [spectrusty-peripherals] which is re-exported as [peripherals].

Additional features:

* "snapshot" - enables serde serializers and deserializers, so custom snapshots can be created with ease in
  elastic formats.
* "compression" - enables gzip compression/decompression of memory chunks stored in snapshots.
* "sdl2" - enables audio implementation for [SDL2] hardware abstraction layer.
* "cpal" - enables audio implementation for [cpal] native audio library.

The default features are:

```text
default = ["formats", "peripherals", "snapshot", "compression"]
```

## Traits

Implemented by ZX Spectrum's core chipset emulators.

Responsible for code execution, keyboard input, video, and accessing peripheral devices:

| trait | function |
|-------|----------|
| [UlaCommon][chip::UlaCommon]       | A grouping trait that includes all of the traits listed below in this table |
| [ControlUnit][chip::ControlUnit]   | Code execution, reset/nmi, access to [BusDevice] peripherals |
| [FrameState][chip::FrameState]     | Provides access to the frame and T-state counters |
| [MemoryAccess][chip::MemoryAccess] | Provides access to onboard memory [ZxMemory] and memory extensions [MemoryExtension] |
| [Video][video::Video]              | Rendering video frame into pixel buffer, border-color control |
| [KeyboardInterface][peripherals::KeyboardInterface] | Keyboard input control |
| [EarIn][chip::EarIn]               | EAR line input access |
| [MicOut][chip::MicOut]             | MIC line output access |
| [UlaControl][chip::UlaControl]     | Accessors for specialized ULA functionality |

Audio output:

| trait | function |
|-------|----------|
| [UlaAudioFrame][audio::UlaAudioFrame] | A grouping trait that includes all of the traits listed below in this table |
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
| [VideoFrame] | Helps driving video rendering, provides arithmetic and conversion tools for [VideoTs] timestamps |
| [MemoryContention][clock::MemoryContention] | A trait that helps establish if an address is being contended |
| [AmpLevels][audio::AmpLevels] | Converts digital audio levels to sample amplitudes |

Implemented by other components:

| trait | implemented by | function |
|-------|----------------|----------|
| [Cpu][z80emu::Cpu] | Z80 CPU | Central processing unit |
| [ZxMemory] | System memory | An access to memory banks, pages, screens, attaching external ROM's |
| [PagedMemory16k][memory::PagedMemory16k] | Memory group | Groups memory implementations with 16k paging capability |
| [PagedMemory8k][memory::PagedMemory8k] | Memory group | Groups memory implementations with 8k paging capability |
| [MemoryExtension] | Memory extensions | Installs program counter traps to switch in and out external banks of memory |
| [BusDevice] | I/O peripheral devices | Establishes address and data BUS communication between CPU and peripherals |
| [Blep] | Bandwidth-Limited Pulse Buffer | An intermediate amplitude differences buffer for rendering square-waves |

## Structs

These are the most commonly used:

| struct | function |
|--------|----------|
| [VideoTs] | A video T-state timestamp, that consist of two components: a scan line number and a horizontal T-state |
| [VFrameTs][clock::VFrameTs]`<V>` | A [VideoFrame] aware video T-state timestamp, for timestamp calculations |
| [VFrameTsCounter][clock::VFrameTsCounter]`<V, C>` | Counts T-states and handles the CPU clock contention |
| [Ula][chip::ula::Ula]`<M, B, X, V>` | A chipset for emulating ZX Spectrum 16/48k PAL/NTSC |
| [Ula128][chip::ula128::Ula128]`<B, X>` | A chipset for emulating ZX Spectrum 128k/+2 |
| [Ula3][chip::ula3::Ula3]`<B, X>` | A chipset for emulating ZX Spectrum +2A/+3 |
| [Scld][chip::scld::Scld]`<M, B, X, V>` | A chipset for emulating TC2048 / TC2068 / TS2068 |
| [UlaPlus][chip::plus::UlaPlus]`<U>` | A wrapper chipset enhancer for emulating ULAplus graphic modes |

### Generic parameters

* `M`: A system memory that implements [ZxMemory] trait.
* `B`: A peripheral device that implements [BusDevice].
* `X`: A memory extension that implements [MemoryExtension].
* `V`: A unit struct that implements [VideoFrame].
* `C`: A unit struct that implements [MemoryContention][clock::MemoryContention].
* `U`: An underlying chipset implementing [UlaPlusInner][chip::plus::UlaPlusInner].

[spectrusty-core]: spectrusty_core
[spectrusty-audio]: spectrusty_audio
[spectrusty-formats]: spectrusty_formats
[spectrusty-peripherals]: spectrusty_peripherals
[spectrusty-utils]: ../spectrusty_utils
[SDL2]: https://crates.io/crates/sdl2
[cpal]: https://crates.io/crates/cpal
[BusDevice]: bus::BusDevice
[Blep]: audio::Blep
[ZxMemory]: memory::ZxMemory
[MemoryExtension]: memory::MemoryExtension
[VideoFrame]: video::VideoFrame
[VideoTs]: clock::VideoTs
*/
pub use spectrusty_core::z80emu;
pub use spectrusty_core::clock;
#[cfg(feature = "formats")] pub use spectrusty_formats as formats;

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
