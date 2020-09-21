![S P E C T R U S T Y][SPECTRUSTY img]

SPECTRUSTY is a [Rust] library for building highly customizable emulators of several ZX Spectrum computer models and clones.

The [Rust] language allows you to easily compile your programs to [WebAssembly], and make your applications available on a thousand different devices. One such a compiled example of a complete emulator program is available [here](web-zxspectrum/) and runs pretty well on mobile devices too.

As an emulator builder, here you find the main core components, such as:

* [Z80](//crates.io/crates/z80emu) CPU - An emulator of Central Processing Unit in a few variations: NMOS, CMOS, and BM1 (dubbed "flavours").
* Sinclair Ferranti ULA (Uncommitted Logic Array) - 16k/48k and 128k version of ZX Spectrum core chipset emulator.
* Amstrad Gate Array ULA (or AGA) - +3/+2A version of ZX Spectrum core chipset emulator.
* ULAplus core chipset extension as a wrapper of any of the above chipsets.
* SCLD by NCR Corporation for Timex TC2048/TC2068/TS2068 series core chipset emulator.

as well as peripheral device components, file format utilities, and more.

See this [full list](https://github.com/royaltm/spectrusty/#Features) of available features.

Core chipset components drive the emulation and give the ability to render video and audio output, created as side effects of the emulated frames. They are also responsible for organizing and interfacing computer memory and I/O of peripheral devices.

You build an emulator around one of the chipset implementations by providing the proper specializations in place of its generic type parameters. To run the emulation, you need an instance of a CPU emulator.

The interaction between the components, as well as the way of interfacing them from your emulator, is realized via the trait system. Most of the traits are defined in the [spectrusty-core] crate and are separated by the context of the provided facility. Hence it's possible to build your own components or extend the existing ones by implementing those traits yourself. This however might require some low-level knowledge of the inner workings of SPECTRUSTY, but shouldn't be very hard. It would be easier though if any kind of [specialization](https://github.com/rust-lang/rust/issues/31844) was already stabilized in Rust

If you want to see a step by step introduction on how to build your Spectrum emulator, please see the [tutorial]. Otherwise, if you like to jump straight into the deep end, here are [examples] of fully functional emulator programs and also an example of a [high-level] emulator library, providing a selection of a few packaged computer models with a bow on top.

[SPECTRUSTY img]: spectrusty.png
[Rust]: https://www.rust-lang.org/
[WebAssembly]: https://webassembly.org/
[spectrusty-core]: https://crates.io/crates/spectrusty-core
[tutorial]: https://royaltm.github.io/spectrusty-tutorial/
[examples]: https://github.com/royaltm/spectrusty/tree/master/examples
[high-level]: https://github.com/royaltm/spectrusty/tree/master/examples/zxspectrum-common