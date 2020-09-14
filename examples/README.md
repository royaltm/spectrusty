SPECTRUSTY
==========

Examples
--------

This directory contains example programs for the SPECTRUSTY library.

_NOTE_: Each of these examples is a separate crate and should be compiled and run from within the example's directory.

_COPYRIGHT_: The following example programs are covered by the GNU GPL-3.0 or later, see [COPYING](../COPYING). The `COPYING.LESSER` does _not_ apply to these files.

### [CPAL Audio](audio/)

Demonstrates how to implement audio functions provided by the library using native CPAL bindings.


### [SDL2 ZX Spectrum ](sdl2-zxspectrum/)

An example emulator program using [SDL2](https://www.libsdl.org/index.php) for interfacing native audio and video.


### [Web AY Player](web-ay-player/)

WebAssembly example of ZX Spectrum [`.ay`](https://worldofspectrum.org/projectay/gdmusic.htm) file format player.


### [Web ZX Spectrum](web-zxspectrum/)

WebAssembly example of a complete ZX Spectrum emulator with UI aware of both desktop and mobile.


### [ZX Spectrum common](zxspectrum-common/)

An example ZX Spectrum emulator library that organizes concrete models and device interfaces.

This library provides a very opinionated, high-level API on top of SPECTRUSTY.

Commonly used by both [Web](web-zxspectrum/) and [SDL2](sdl2-zxspectrum/) example emulators.
