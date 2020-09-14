SPECTRUSTY
==========

CPAL Audio
----------

This package contains programs demonstrating how to use audio functions of the SPECTRUSTY library with the [cpal](https://crates.io/crates/cpal) audio native bindings.

#### [audio_tap_cpal](src/bin/audio_tap_cpal.rs)

Loads a `.tap` file provided in the command line and plays it via the default audio stream output, imitating the ZX Spectrum ROM TAPE data saving routines.

The synthesized sound data is also being saved to the `tap_output.wav` file.

Usage:

```
cargo run --bin audio_tap_cpal -- path/to/file.tap
```


#### [audio_ay_cpal](src/bin/audio_ay_cpal.rs)

Example of direct synthesis of the audio output of the [AY-3-8910](https://pl.wikipedia.org/wiki/General_Instrument_AY-3-8910) Programmable Sound Generator.

Usage:

```
cargo run --bin audio_ay_cpal
```


#### [ay_player_cpal](src/bin/ay_player_cpal.rs)

Command-line example of a ZX Spectrum [`.ay`](https://worldofspectrum.org/projectay/gdmusic.htm) file format player.

The synthesized sound data is also being saved to the `tap_output.wav` file.

Usage:

```
cargo run --release --bin ay_player_cpal -- path/to/file.ay [song index]
```


### Building

To build all, run from this directory:

```
cargo build --bins
```

or in release mode:

```
cargo build --bins --release
```
