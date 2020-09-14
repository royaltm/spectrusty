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
NODE_ENV=production npm run build
```
