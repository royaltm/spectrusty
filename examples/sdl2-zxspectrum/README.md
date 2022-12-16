SPECTRUSTY
==========

SDL2 ZX Spectrum
----------------

ZX Spectrum emulator as a native application with hardware bindings via [SDL2] library.

This is an example of how to built a complete emulator program using SPECTRUSTY, including printers, serial and parallel ports and also [ZX Interface I] functions.

However, to enable `ZX Interface I` in the emulator, you need to obtain a copy of its shadow ROM [if1-2.rom](https://sinclair.wiki.zxnet.co.uk/wiki/ROM_images) first.


Usage
-----

The program has no GUI. Instead it interacts with the user via the keyboard and mouse, if the Kempston Mouse emulation is enabled.

When program's window shows up, press `F1` for the pop-up help.


Special features
----------------

### Printers & ports

The RS-232 and Centronics ports output is redirected to program's standard output which is a console output by default.
The RS-232 input is provided from the program's standard input which is a console keyboard by default.

Graphics printed via one of these ports, using the `COPY` or `COPY EXT` Basic commands, is being captured and can be saved as an `SVG` file, by pressing `F3`.

RS-232 is available in models: `128k`, `+2`, `+2A`, `++2` and `+2B`, but also in any other model with attached `ZX Interface I`.

Centronics port is available in models: `+2A` and `+2B`.

Please refer to Spectrum's operating manual on how to use those ports from the emulated computers.

[ZX Printer] spooler, if the printer is enabled and supported by the emulated model, will collect printed images, which may be saved at any later time as both a `PNG` and an `SVG` files, by pressing `F3`.

### Data files

`TAP`, `SCR` files and `MDR` cartridge files can be added or snapshot files can be loaded by dragging & dropping them on the program's window.

Only one `TAP` file can be inserted at the same time. The previous file will be ejected if another one has been added.

Adding `SCR` file will just render it into model's active screen memory, possibly adjusting the current screen mode.

[ZX Microdrive]'s `MDR` cartridges can be used only if `ZX Interface I` is enabled in the emulator.

Up to 7 `MDR` cardridges can be added in read-only mode. One empty `MDR` cardridge in R/W mode is always being present when Microdrives are active.

Before exiting or loading another snapshot file, the program saves the current state into the `JSON` snapshot file, that can be restored later by providing a path to the file in the command line arguments or by dropping it on the program's window. If the first Microdrive cartridge has been modified it will also be saved as an `MDR` file.

When recording tape data, it's being written directly to the `TAP` file.


Building
--------

To build any Rust program linked against [SDL2] please consult [this page](https://github.com/Rust-SDL2/rust-sdl2#requirements).

By default it compiles using `"bundled"` feature of the [sdl2](https://crates.io/crates/sdl2) crate.
For this to succeed you'd need several additional [development tools](https://github.com/Rust-SDL2/rust-sdl2#bundled-feature).

```
cargo build --release
```

If this is not a feasible option try to compile without default features:

```
cargo build --release --no-default-features
```

In this instance follow guides on how to install SDL2 files in your system first.

In both scenarios the compiled binary will link dynamically with SDL2. If instead you'd like to link SDL2 statically please use the `"static-link"` feature.

With `"bundled"`:

```
cargo build --release --features=static-link
```

or without:

```
cargo build --release --no-default-features --features=static-link
```


### Features

* `bundled` - Use bundled SDL2 build.
* `static-link` - Link SDL2 statically.
* `use-pkgconfig` - Use pkgconfig to find SDL2 libraries.
* `compact` - enable `boxed_frame_cache`, `boxed_devices` and `universal_dev_ts` features, renders smaller code.
* `universal_dev_ts` - Clock counters have the same base type across all models, thus reducing the compiled binary size at the cost of a small performance penalty.
* `boxed_frame_cache` - Chipset implementations will have significantly reduced struct sizes at the cost of a minimal performance penalty.
* `boxed_devices` - Struct and output code size sizes reduced even further.

__NOTE__: Without `boxed_frame_cache` feature enabled you might hit stack overflow on the main thread.


Running
-------

After building, the program can be found in the `../../target/release/` directory.
You may copy it as long as you make sure SDL2 library file is also in `PATH`, unless statically linked.

```
sdl2-zxspectrum --help

SDL2_SPECTRUSTY_Example
Rafał Michalski
SPECTRUSTY: a desktop SDL2 example emulator.

USAGE:
    sdl2-zxspectrum.exe [FLAGS] [OPTIONS] [FILES]...

FLAGS:
        --fuller     Inserts Fuller AY-3-8910 device
    -h, --help       Prints help information
        --ay         Inserts Melodik AY-3-8910 device
        --mouse      Inserts Kempston Mouse device
    -p, --printer    Inserts ZX Printer device
    -V, --version    Prints version information

OPTIONS:
        --audio <audio>              Audio latency
    -b, --border <border>            Initial border size
    -c, --cpu <cpu>                  Select CPU type
    -i, --interface1 <interface1>    Installs ZX Interface 1 (ROM path required)
    -j, --joystick <joystick>        Selects joystick at startup
    -m, --model <model>              Selects emulated model
        --bind <zxnet_bind>          Bind address of ZX-NET UDP socket
        --conn <zxnet_conn>          Connect address of ZX-NET UDP socket

ARGS:
    <FILES>...    Sets the input file(s) to load at startup
```

By default the `ZX Spectrum +2B` model is being selected.

If a `TAP` file is specified as `FILES`, the emulator will attempt to automatically load data from the tape, unless a snapshot file is also specified as another argument.

When a `Z80` snapshot is specified, that requires ZX Interface 1 to be attached, make sure to also provide a path to the IF 1 ROM file via `-i` or `--interface1` option.

Options, except `--model` and `--interface1`, provided together with a snapshot file argument, will overwrite any settings loaded from that snapshot.


#### Examples

To run ZX Spectrum 48k model with Melodik AY-3-8910 extension, Kempston joystick with a tape file, try:

```
../../target/release/sdl2-zxspectrum -m 48k --ay -j k path/to/file.tap
```

How about `ZX Spectrum 128k` with ZX Interface I listening on port 30000 for ZX-NET packets:

```
../../target/release/sdl2-zxspectrum -m 128k -i path/to/if1-2.rom --bind 0.0.0.0:30000
```

to connect another instance of spectrum, this time 48k model, via ZX-NET over UDP run:

```
../../target/release/sdl2-zxspectrum -m 48k -i path/to/if1-2.rom --conn 127.0.0.1:30000
```


[SDL2]: https://www.libsdl.org/index.php
[ZX Interface I]: https://pl.wikipedia.org/wiki/ZX_Interface_1
[ZX Printer]: https://en.wikipedia.org/wiki/ZX_Printer
[ZX Microdrive]: https://en.wikipedia.org/wiki/ZX_Microdrive