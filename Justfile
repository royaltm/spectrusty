name := 'spectrusty'

# run all benchmarks
bench:
    cargo +nightly bench --bench boot -- --nocapture
    cargo +nightly bench --bench synth --features=audio -- --nocapture
    cargo +nightly bench --bench video -- --nocapture
    cargo +nightly bench --bench video128 -- --nocapture
    cargo +nightly bench --bench video_plus -- --nocapture

# build all examples
examples:
    cargo build -p audio --bins --release
    just examples/web-ay-player/webpack
    just examples/web-zxspectrum/webpack
    cargo build -p sdl2-zxspectrum --release

# build all docs
doc:
    cargo +nightly doc -p zxspectrum-common --all-features

# run all tests
test:
    cargo test -- --nocapture
    cargo test -- --ignored --nocapture
    cargo build -p zxspectrum-common
    cargo test -p zxspectrum-common -- --nocapture
    cargo build -p audio --bins
    cargo test -p audio -- --nocapture
