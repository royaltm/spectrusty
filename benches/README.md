SPECTRUSTY
==========

Benchmarks
----------

_COPYRIGHT_: The benchmark programs are covered by the GNU GPL-3.0 or later, see [COPYING](../COPYING). The `COPYING.LESSER` does _not_ apply to these files.

### [Boot](boot.rs)

Measures the emulated CPU performance while executing boot sequences of ZX Spectrum 16k and 48k.

Run with:

```
cargo +nightly bench --bench boot -- --nocapture
```

### [Synth](synth.rs)

Measures performance of a Bandwidth-Limited Pulse Buffer implementation by rendering audio frames.

Run with:

```
cargo +nightly bench --bench synth --features=audio -- --nocapture
```

### [Video](video.rs)

Measures performance of rendering the ULA video frames.

Run with:

```
cargo +nightly bench --bench video -- --nocapture
```


### [Video128](video128.rs)

Measures performance of rendering the ULA 128 video frames.

Run with:

```
cargo +nightly bench --bench video128 -- --nocapture
```

### [VideoPlus](video_plus.rs)

Measures performance of rendering the ULAPlus video frames.

Run with:

```
cargo +nightly bench --bench video_plus -- --nocapture
```
