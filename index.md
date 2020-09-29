![S P E C T R U S T Y][SPECTRUSTY img]

SPECTRUSTY is a [Rust] library for building highly customizable emulators of several ZX Spectrum computer models and clones. Perhaps even [more](https://royaltm.github.io/rust-ym-file-parser/).

The [Rust] language allows you to easily compile your programs to [WebAssembly], and make your applications available on a thousand different devices. Check out this [WebAssembly emulator example](web-zxspectrum/). It runs pretty well on mobile devices too.

Here you'll find:

* [Z80](//crates.io/crates/z80emu) CPU - An emulator of the Central Processing Unit in a few variations: NMOS, CMOS, and BM1 (dubbed "flavours").
* Sinclair Ferranti ULA (Uncommitted Logic Array) - [16k/48k][Ula] and [128k][Ula128] version of ZX Spectrum core chipset emulator.
* Amstrad Gate Array ULA (or AGA) - [+3/+2A][Ula3] version of ZX Spectrum core chipset emulator.
* [ULAplus] core chipset extension as a wrapper of any of the above chipsets.
* [SCLD] by NCR Corporation for Timex TC2048/TC2068/TS2068 series core chipset emulator.

as well as [peripheral] device components, file [format] utilities, and [more][spectrusty-utils].

See the [full list](https://github.com/royaltm/spectrusty/#Features) of available features.

If you want to see a step by step introduction on how to build your emulator using SPECTRUSTY, see the [tutorial].

Otherwise, if you like to jump straight into the deep end, here are [examples] of fully functional emulator programs. There is also an example of a [high-level] emulator library, providing a selection of a few packaged computer models with a bow on top.

<script>var clicky_site_ids = clicky_site_ids || []; clicky_site_ids.push(101270192);</script>
<script async src="//static.getclicky.com/js"></script>

[SPECTRUSTY img]: spectrusty.png
[Rust]: https://www.rust-lang.org/
[WebAssembly]: https://webassembly.org/
[spectrusty-core]: https://crates.io/crates/spectrusty-core
[tutorial]: https://royaltm.github.io/spectrusty-tutorial/
[examples]: https://github.com/royaltm/spectrusty/tree/master/examples
[high-level]: https://github.com/royaltm/spectrusty/tree/master/examples/zxspectrum-common
[format]: https://docs.rs/spectrusty-formats/
[peripheral]: https://docs.rs/spectrusty/*/spectrusty/peripherals/index.html
[spectrusty-utils]: https://docs.rs/spectrusty-utils/
[Ula]: https://docs.rs/spectrusty/*/spectrusty/chip/ula/struct.Ula.html
[Ula128]: https://docs.rs/spectrusty/*/spectrusty/chip/ula128/struct.Ula128.html
[Ula3]: https://docs.rs/spectrusty/*/spectrusty/chip/ula3/struct.Ula3.html
[ULAplus]: https://docs.rs/spectrusty/*/spectrusty/chip/plus/struct.UlaPlus.html
[SCLD]: https://docs.rs/spectrusty/*/spectrusty/chip/scld/struct.Scld.html