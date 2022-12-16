SPECTRUSTY
==========

ZX Spectrum common
------------------

This package contains an example ZX Spectrum emulator library that organizes concrete models and interfaces for easy device access.

This library provides a very opinionated, high-level emulator API on top of SPECTRUSTY.


### Models

The computer models defined in this example use the following [ROMs](../../resources/):

* `ZX Spectrum 16k` - [48.rom]
* `ZX Spectrum 48k` - [48.rom]
* `ZX Spectrum NTSC` - [48.rom]
* `ZX Spectrum 128k` - [128-0.rom], [128-1.rom]
* `ZX Spectrum +2` - [plus2-0.rom], [plus2-1.rom]
* `ZX Spectrum +2A` - [plus3-0.rom], [plus3-1.rom], [plus3-2.rom], [plus3-3.rom]
* `Timex TC2048` - [tc2048.rom]
* `ZX Spectrum 48k+ (ULAplus)` - [48.rom]
* `ZX Spectrum ++2 (ULAplus)` - [plus2-0.rom], [plus2-1.rom]
* `ZX Spectrum +2B (ULAplus)` - [plus2B.rom], [48.rom], [BBCBasic.rom], [opense.rom]


### Features

* `boxed_devices` - Devices of the chipset models are boxed thus reducing chipset struct size at the cost of a small performance penalty.
* `universal_dev_ts` - Clock counters have the same base (linear) type across all models, thus significantly reducing the compiled binary size at the cost of a small performance penalty.

Both features are enabled by default and can be opt out with `--no-default-features`:


### Documentation

To build documentation, run from this directory:

```
cargo +nightly doc
```

[48.rom]: ../../resources/48.rom
[128-0.rom]: ../../resources/128-0.rom
[128-1.rom]: ../../resources/128-1.rom
[plus2-0.rom]: ../../resources/plus2-0.rom
[plus2-1.rom]: ../../resources/plus2-1.rom
[plus3-0.rom]: ../../resources/plus3-0.rom
[plus3-1.rom]: ../../resources/plus3-1.rom
[plus3-2.rom]: ../../resources/plus3-2.rom
[plus3-3.rom]: ../../resources/plus3-3.rom
[tc2048.rom]: ../../resources/tc2048.rom
[plus2B.rom]: ../../resources/plus2B.rom
[BBCBasic.rom]: ../../resources/BBCBasic.rom
[opense.rom]: ../../resources/opense.rom