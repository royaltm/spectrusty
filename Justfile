name := 'spectrusty'
benchmark_names := 'boot video video128 video_plus'
llvm_profdata_exe := replace(clean(`rustc --print target-libdir` / ".." / "bin" / "llvm-profdata"),'\','/')
target := replace_regex(trim_end_match(`rustup default`, ' (default)'), '^[^-]+-', '')
optimizations := '-Zno-parallel-llvm -Ccodegen-units=1'

# run all benchmarks
bench:
    cargo +nightly bench --bench boot --features=boxed_frame_cache -- --nocapture
    for bench in {{benchmark_names}}; do \
        cargo +nightly bench --bench $bench -- --nocapture; \
    done
    cargo +nightly bench --bench synth --features=audio -- --nocapture

# run all benchmarks MIR optimized (rustc nightly)
bench-mir: rustcwrap
    RUSTFLAGS="{{optimizations}}" RUSTC_WRAPPER="./rustcwrap" \
        cargo +nightly-{{target}} bench --jobs=1 --target="{{target}}" --features=boxed_frame_cache --bench boot -- --nocapture
    RUSTFLAGS="{{optimizations}}" RUSTC_WRAPPER="./rustcwrap" \
        cargo +nightly-{{target}} bench --target="{{target}}" --bench boot -- --nocapture

# build all examples
examples:
    cargo build -p audio --bins --release
    just examples/web-ay-player/webpack
    just examples/web-zxspectrum/webpack
    cargo build -p sdl2-zxspectrum --release

# sdl2-zxspectrum example - MIR optimized (rustc nightly)
example-mir: rustcwrap
    RUSTFLAGS="{{optimizations}}" RUSTC_WRAPPER="./rustcwrap" \
        cargo +nightly-{{target}} build --target="{{target}}" -p sdl2-zxspectrum --release

# generate sdl2-zxspectrum example profile (rustc nightly)
example-profgen args="":
    set -euxo pipefail
    # rustup component add llvm-tools-preview
    rm -rf tmp/pgo-data
    RUSTFLAGS="-Cprofile-generate=tmp/pgo-data" cargo +nightly-{{target}} run --target="{{target}}" \
                -p sdl2-zxspectrum --release -- {{args}}
    {{llvm_profdata_exe}} merge -o tmp/pgo-data/merged.profdata tmp/pgo-data

# run sdl2-zxspectrum with profile-driven optimizations (rustc nightly)
example-prof args="":
    RUSTFLAGS="-Cllvm-args=-pgo-warn-missing-function -Cprofile-use={{justfile_directory()}}/tmp/pgo-data/merged.profdata" \
        cargo +nightly-{{target}} run --target="{{target}}" -p sdl2-zxspectrum --release -- {{args}}

# build rustcwrap for MIR builds
rustcwrap:
    rustc rustcwrap.rs -o rustcwrap.exe

# build all docs
doc:
    cargo +nightly doc -p zxspectrum-common --all-features

# run all tests
test:
    cargo test --no-default-features -- --nocapture
    cargo test --no-default-features -- --ignored --nocapture
    cargo test -- --nocapture
    cargo test -- --ignored --nocapture
    cargo test --features=boxed_frame_cache -- --nocapture
    cargo test --features=boxed_frame_cache -- --ignored --nocapture
    cargo build -p zxspectrum-common
    cargo test -p zxspectrum-common -- --nocapture
    cargo build --no-default-features -p zxspectrum-common
    cargo test --no-default-features -p zxspectrum-common -- --nocapture
    cargo build -p audio --bins
    cargo test -p audio -- --nocapture
    cargo test -p sdl2-zxspectrum -- --nocapture

# clean all build artefacts
clean:
    rm -rf tmp/pgo-data
    cargo clean
    just examples/web-zxspectrum/clean
    just examples/web-ay-player/clean

# run clippy tests
clippy:
    touch src/lib.rs
    cargo clippy -- -D warnings
    cargo clippy --no-default-features -- -D warnings
    for directory in spectrusty-*; do \
        cd $directory && \
        touch src/lib.rs && \
        cargo clippy --no-default-features -- -D warnings && \
        cd ..; \
    done
    cd spectrusty-audio && \
        touch src/lib.rs && \
        cargo clippy --features=sdl2 -- -D warnings && \
        touch src/lib.rs && \
        cargo clippy --features=cpal -- -D warnings && \
        cd ..
    cd spectrusty-utils && \
        touch src/lib.rs && \
        cargo clippy --features=sdl2 -- -D warnings && \
        touch src/lib.rs && \
        cargo clippy --features=minifb -- -D warnings && \
        touch src/lib.rs && \
        cargo clippy --features=winit -- -D warnings && \
        touch src/lib.rs && \
        cargo clippy --features=web-sys -- -D warnings && \
        cd ..
