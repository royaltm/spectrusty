SPECTRUSTY
==========

![SPECTRUSTY img][S P E C T R U S T Y]

[![Crate][Crate img]][Crate Link]
[![Docs][Docs img]][Docs Link]
[![Build Status][Build img]][Build Link]
[![Minimum rustc version][rustc version img]][rustc version link]
[![License][License img]][License Link]

What's this & Why?
------------------

There are plenty of [ZX Spectrum] emulators, a large portion of them are open-source, some very advanced, most are done just for fun. But except some examples of Z80 CPU emulators, each of the ones I've seen is a complete program, with code that is not easy to build upon or port to another system. Most that I found is written in C, a lot of them in Java, and even some in javascript.

The notable example is [Free Unix Spectrum Emulator][Fuse] written in C with pleasantly readable code. The components of the emulator are nicely separated, with high cohesion but unfortunately, here and there sprinkled with a vast range of global variables. However, I can imagine that with sufficient patience and skill you can build successful projects upon such a foundation.

**SPECTRUSTY** was built from scratch, partly for fun, but also as a Rust abstraction testing facility. I needed some theme to dive deep into the Rust OO model for educational purposes, and have chosen [ZX Spectrum] because it's such a simple machine thoroughly reverse-engineered and documented. It was also the first computer that I've owned. From the start, the goal was not to build a complete emulator program, but rather a library, upon which you can relatively easy build emulators that suit your particular needs.

In this library, every component is a separate entity that doesn't know anything about the world and realizes its purpose via trait interfaces. Most of the structs are defined with generic parameters that can be substituted by the complementary components which complete or extend their base functionality.

To create your emulator with **SPECTRUSTY** first, you need to choose the components you need, then choose the way to glue them together, and then realize their functions via trait methods.


Caveats
-------

Because of the heavy use of polymorphism and static dispatching the separate code is being created due to monomorphisation for every set of spectrum chipset, device set, and the CPU. This may result in a rather large output code if you want all these sets to be available at run time. On the other hand, the produced code is quite efficient.


How To
------

Because the best way to learn new things is by example, I invite you to see the [tutorial].

Several examples of what can be built with **SPECTRUSTY** can be also found in the [examples](examples) directory, provided as separate crates.


Contribution and feedback
-------------------------

Because of ZX Spectrum's popularity, there is a lot of technical information available from various sources. On the other hand, the certainty of the information has been deluded because some of the resources are contradicting each other. Unfortunately, my access to the real hardware is very limited - I'm a proud owner of a single [TC2048], that is still working today. If you'd find something that is being emulated incorrectly, please don't hesitate to submit an issue.


Features
--------

Below you'll find the featured facilities of the library and its roadmap.

### CPU

The Zilog's Z80 CPU emulation is performed via a separate [z80emu] crate that can emulate `NMOS`, `CMOS`, and `BM1` variants of the CPU.

### Core chipset models

The video and contention timings for the Sinclair/Amstrad machines are thoroughly tested and at least match those of the [Fuse] emulator. The SCLD implementation has no separate timings implemented yet, so for now it can borrow the timings from Sinclair models.

* [x] - Ferranti ULA (Uncommitted Logic Array) for Sinclair ZX Spectrum 16k/48k (PAL and NTSC timings).
* [x] - Ferranti ULA 128 for Sinclair ZX Spectrum 128k/+2.
* [x] - Amstrad Gate Array for ZX Spectrum +2A/+3.
* [x] - (partial) SCLD by NCR Corporation for Timex TC2048/TC2068/TS2068 series (uses Ferrant ULA's contention at the moment).
* [x] - ULAplus screen and color modes (including grayscale) as an enhancement wrapper for other chipsets.
* [ ] - Chloe for ZX Spectrum SE (272kB RAM).
* [ ] - Russian Pentagon and Scorpion models.
* [ ] - Slovak's Didaktik series.
* [ ] - Polish Elwro 800 Junior (if I ever find some serious specs).

### Emulated chipsets' original bugs and features

* [x] - "Slow" memory contention.
* [x] - Partial port decoding as on original machines.
* [x] - Floating bus when read an unused port on Ferranti ULA chipsets.
* [x] - Keyboard issue (2/3/none/scld) on ULA port read - determines the EAR IN bit value when there is no tape signal.
* [x] - EAR/MIC output allows for sound emulation from both sources.
* [x] - Ferranti ULA "snow" bug interference - when the register I is set to a contended page (see Robocop 3 on 128k model or Vectron). However the exact timings of the RFSH influence on the interference is not currently known, so it is an approximation. The bug emulation also does not hang Spectrum as on real machines.
* [x] - Early/late timing schemes.
* [x] - I/O port contention matches that of the [Fuse] emulator. Most of the emulators assume port read/write is performed at the last [IO cycle] - `T4`, however the `IO_IORQ` signal of Z80 is active between `T1` and `T4` and [Fuse] port read/write is timestamped at `T1`. This is especially important for exact emulation of border effects.
* [x] - Reads from port 0x7ffd cause a crash on ULA 128 as its HAL10H8 chip does not distinguish between reads and writes to this port, resulting in a floating data bus being used to set the paging registers.

### Emulated peripherals

* [x] - Tape signal interface as a pulse iterator (input and output).
* [x] - AY-3-8910 sound processor and generic I/O A and B ports (for 128k / Melodik / FullerBox / Timex).
* [x] - 128k RS-232 port as an AY-3-8910 port attachement.
* [x] - [128k keypad] as an AY-3-8910 port attachement.
* [x] - +2A/+3 centronics port.
* [x] - Interface 1 ROM extension.
* [x] - Interface 1 - microdrives (including IN 0 bug).
* [x] - Interface 1 - RS232.
* [x] - Interface 1 - ZX-NET (with the real time UDP packet encapsulation!).
* [x] - Joysticks: Kempston, Fuller, Sinclair, Cursor.
* [x] - Kempston mouse.
* [ ] - +3 floppy disk drive
* [ ] - other floppy drive systems (TR-DOS, DiSCIPLE, +D, FDD 3000, ...)

### File formats

* [x] - .SNA format loader/saver
* [x] - .Z80 v1/2/3 format loader/saver
* [ ] - .SLT
* [ ] - .SZX
* [x] - .TAP format reader/writer, pulse encoder/decoder
* [ ] - .TZX format reader/writer, pulse encoder/decoder
* [ ] - .PZX format reader/writer, pulse encoder/decoder
* [ ] - .RZX format reader/writer, recorder/player
* [x] - .MDR microdrive format reader/writer, filesystem browser
* [x] - .SCR format loader/saver
* [ ] - .ZXP format loader/saver
* [x] - .AY player format parser


Rust Version Requirements
-------------------------

`spectrusty` requires Rustc version 1.36 or greater due to the usage of some macro features and API that was introduced
or stabilized in this version.


Copyright
---------

The **SPECTRUSTY** library is available under the terms of the GNU LGPL version 3 or later.

[Examples](examples/), [tests](tests/), and [benchmarks](benches/) are available under the terms of the GNU GPL version 3 or later.

`ROM` files found in the [resources](resources/roms) directory are made available under different [terms](resources/README-copyright.md).

[SPECTRUSTY img]: resources/spectrusty.png
[Crate Link]: https://crates.io/crates/spectrusty
[Crate img]: https://img.shields.io/crates/v/spectrusty.svg
[Docs Link]: https://docs.rs/spectrusty
[Docs img]: https://docs.rs/spectrusty/badge.svg
[Build Link]: https://travis-ci.org/royaltm/spectrusty
[Build img]: https://travis-ci.org/royaltm/spectrusty.svg?branch=master
[rustc version link]: https://github.com/royaltm/spectrusty#rust-version-requirements
[rustc version img]: https://img.shields.io/badge/rustc-1.36+-lightgray.svg
[License Link]: https://www.gnu.org/licenses/#LGPL
[License img]: https://img.shields.io/crates/l/spectrusty
[TC2048]: https://en.wikipedia.org/wiki/Timex_Computer_2048
[ZX Spectrum]: https://en.wikipedia.org/wiki/ZX_Spectrum
[Fuse]: http://fuse-emulator.sourceforge.net/
[z80emu]: https://github.com/royaltm/rust-z80emu
[IO cycle]: https://docs.rs/z80emu/0.6.0/z80emu/host/cycles/index.html#inputoutput
[tutorial]: https://royaltm.github.io/spectrusty-tutorial
[128k keypad]: http://www.fruitcake.plus.com/Sinclair/Spectrum128/Keypad/Spectrum128Keypad.htm