SPECTRUSTY
==========

Web ZX Spectrum
---------------

ZX Spectrum emulator as a Web application.

This is an example of how to built a usable emulator program using SPECTRUSTY.

Web UI in this example can be used with most modern desktop and mobile browsers.

This program does not emulates printers, serial and parallel ports nor functions of ZX Interface I, such as Microdrives, or networking.

For the example implementation of those features in a complete emulator program, please see [SDL2 ZX Spectrum](../sdl2-zxspectrum) instead.


### Prerequisites

In addition to Rust compiler the following tools are needed:

* [node.js + npm](https://nodejs.org/en/)

Install local dependencies, run from this directory:

```
npm install
```


### Development

To run it locally in development mode:

```
npm run serve
```

Open `http://localhost:8080` in the web browser.


### Distribution

To build distribution files in a `dist` folder:

```
NODE_ENV=production npx webpack
```


### Features

* `universal_dev_ts` - Clock counters have the same base type across all models, thus reducing the compiled binary size at the cost of a small performance penalty.
* `boxed_frame_cache` - Chipset implementations will have significantly reduced struct sizes at the cost of a minimal performance penalty.
* `boxed_devices` - Struct and output code size sizes reduced even further.

```
SPECTRUSTY_FEATURES="universal_dev_ts,boxed_devices" NODE_ENV=production npx webpack
```
