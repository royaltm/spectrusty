SPECTRUSTY
==========

Web AY Player
-------------

ZX Spectrum [AY] file format audio player example as a Web application.

This is an example of how to built a WebAssembly chiptune player using SPECTRUSTY.

This demo program plays [AY] and [SNA] files (in ZX Spectrum mode) loaded from your disk.


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

[AY]: https://worldofspectrum.org/projectay/gdmusic.htm
[SNA]: https://worldofspectrum.org/faq/reference/formats.htm#SNA