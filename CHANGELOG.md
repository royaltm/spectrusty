v0.3.1
* spectrusty-audio: cpal bumped to 0.13.1, fixes compilation on 32-bit machines.
* bumped nom, bitvec and rand.

v0.3.0
* RendererPlus: Added support for the hi-res modes with color and grayscale palette.
* spectrusty-formats: scr: Added format support for the hi-res screen with a color palette.

v0.2.1

* spectrusty-peripherals: Fixed deserializing of `JoystickSelect` from owned strings; `TryFromStrJoystickSelectError` inner public property type changed to `Cow<str>`.
* spectrusty-utils: Fixed compilation without default features.

v0.2.0

* Decouple `BusDevice` timestamps from `ControlUnit` implementations. Now timestamps must implement `From<VFrameTs<_>>` instead of being exactly the same. This enables usage of a common timestamp type for devices shared between different chipset implementations.
* Removed unnecessary `Debug` and `Default` constraints on the timestamp type from definitions of NamedBusDevice, AyIoPort, NullDevice.
* `ControlUnit` implementations, while executing single instructions invoke `BusDevice::update_timestamp`.
* Removed unnecessary conditions on chipset implementations.
* (BREAKING) `TimestampOps` trait now used in peripherals implementations replaces `FrameTimestamp`. Former methods of `FrameTimestamp` moved to `VFrameTs` struct implementation. A new constant `VFrameTs<V>::EOF`.
* (BREAKING) Redefined an `eof_timestamp` argument to BusDevice::next_frame.
* (BREAKING) `UlaPlusInner::video_render_data_view` return type is now `VideoRenderDataView`.
* (BREAKING) `VFrameTs<V>` serializes to and deserializes from `FTs`. This enables compatibility of the snapshots between different timestamp implementations. Not backward compatible.
* (fix, BREAKING) peripherals: types definitions in bus::ay::serial128 do not expose invariant parameter `T` but properly bind it to the `BusDevice` implementations of `D`.

* examples: zxspectrum-common: generic bus device timestamp types; Bus device timestamps are now `FTs`.
* examples: sdl2-zxspectrum, web-zxspectrum: adapted to changes in zxspectrum-common.

* Changes suggested by Clippy.
