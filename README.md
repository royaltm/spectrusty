SPECTRUSTY
==========

What's this & Why?
------------------

There are plenty of [ZX Spectrum] emulators, a large portion of them are open-source, some very advanced, most are done just for fun. But except some examples of Z80 CPU emulators, every one of them is a complete program, with code that is not easy to build upon or port to another system. Most that I found are in C, a lot of them in Java, and even some in javascript.

The notable example is a ZX Spectrum [Fuse] emulator, written in C, with pleasantly readable code. Nevertheless, the components of the emulator are nicely separated, with high cohesion but unfortunately also high coupling, here and there sprinkled with a vast range of global variables. However, I can imagine that with sufficient patience and skill you can build upon this foundation.

**SPECTRUSTY** was partly created for fun, but also as a Rust abstraction testing facility. Because Spectrum is so simple, and it was the first computer that I put my hands on, I needed a good reason to dive deep in the Rust OO model for purely educational purposes. From the start, the goal was not to build a complete emulator program, but rather a library upon which you can relatively easily build emulators that suit your particular needs.

In this library, every component is a separate entity. Most of them defined with generic parameters, that can be completed and complemented with other components using, and perhaps slightly abusing, the Rust trait-based OO model. The interaction between the library's components is realized via Rust's trait system.


Caveats
-------

Because of the heavy use of static dispatching and due to monomorphisation the separate code is being created for every set of spectrum chipset, device set, and the CPU. This may result in a rather large output code if you want all these sets to be available at run time. On the other hand, the produced code is quite efficient.


How To
------

Because the best way to learn new things is through practice, so instead of explaining the core concept behind the library, I'll just point you to the [tutorial] that I'm finishing at the moment.


Examples
--------

Several examples of what can be build with **SPECTRUSTY** can be found in the [examples](examples) directory as separate crates.


Contribution and feedback
-------------------------

Because of ZX Spectrum's popularity, there is a lot of technical information available from various sources. On the other hand, because of the same reason, the certainty of the information has been deluded, because some of the resources are contradicting each other. Unfortunately, my access to the real hardware is very limited - I'm a proud owner of a single TC2048, that is still working today. If you'd find something that is being emulated in the wrong way, please don't hesitate to submit an issue.


Features and the roadmap
------------------------

### CPU

The Zilog's Z80 emulation is performed via a separate [z80emu] crate which can emulate `NMOS`/`CMOS`/`BM1` version of the CPU.

### Core chipset models

The video and contention timings for the Sinclair/Amstrad machines are thoroughly tested and match those of the [Fuse] emulator. The SCLD implementation has no separate timings implemented yet, so for now it can borrow the timings from Sinclair models.

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


Copyright
---------

The **SPECTRUSTY** library is available under the terms of the GNU LGPL version 3 or later.

[Examples](examples/), [tests](tests/) and [benchmarks](benches/) are available under the terms of the GNU GPL version 3 or later.

`ROM` files found in the [resources](resources/roms) directory are made available under different [terms](resources/README-copyright.md).

[ZX Spectrum]: https://en.wikipedia.org/wiki/ZX_Spectrum
[Fuse]: http://fuse-emulator.sourceforge.net/
[z80emu]: https://github.com/royaltm/rust-z80emu
[IO cycle]: https://docs.rs/z80emu/0.3.0/z80emu/host/cycles/index.html#inputoutput
[tutorial]: https://royaltm.github.io/spectrusty-tutorial
[audio]: https://github.com/royaltm/spectrusty/examples/audio
[sdl2-spectrum]: examples/sdl2-spectrum
[web-spectrum]: examples/web-spectrum
[web-ay-player]: examples/web-ay-player
[spectrum-common]: examples/spectrum-common
[128k keypad]: http://www.fruitcake.plus.com/Sinclair/Spectrum128/Keypad/Spectrum128Keypad.htm