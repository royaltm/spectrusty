v0.2.0

* `TimestampOps` trait now used in peripherals implementations replaces deprecated `FrameTimestamp`. Former methods of `FrameTimestamp` moved to `VFrameTs` struct implementation. A new constant `VFrameTs<V>::EOF`.
* Decouple `BusDevice` timestamps from `ControlUnit` implementations. Now timestamps must implement `From<VFrameTs<_>>` instead of being exactly the same. This enables usage of a common timestamp type for devices shared between different chipset implementations.
* Removed unnecessary `Debug` and `Default` constraints on the timestamp type from definitions of NamedBusDevice, AyIoPort, NullDevice.
* `ControlUnit` implementations, while executing single instructions invoke `BusDevice::update_timestamp`.
* Removed unnecessary conditions on chipset implementations.
* (fix, BREAKING) peripherals: types definitions in bus::ay::serial128 do not expose invariant parameter `T` but properly bind it to the `BusDevice` implementations of `D`.
* (BREAKING) Redefined an `eof_timestamp` argument to BusDevice::next_frame.
* (BREAKING) `UlaPlusInner::video_render_data_view` return type is now `VideoRenderDataView`.

* examples: zxspectrum-common: generic bus device timestamp types; Bus device timestamps are now `FTs`.
* examples: sdl2-zxspectrum, web-zxspectrum: adapted to changes in zxspectrum-common.

* Changes suggested by Clippy.
