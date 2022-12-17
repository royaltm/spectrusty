name := 'spectrusty'
benchmark_names := 'boot video video128 video_plus'
llvm_profdata_exe := replace(clean(`rustc --print target-libdir` / ".." / "bin" / "llvm-profdata"),'\','/')
target := replace_regex(trim_end_match(`rustup default`, ' (default)'), '^[^-]+-', '')
optimizations := '-Zno-parallel-llvm -Ccodegen-units=1'
features := env_var_or_default('SPECTRUSTY_FEATURES', "bundled,compact")

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
    cd examples/sdl2-zxspectrum && cargo build --release --no-default-features --features={{features}}

# run sdl2-zxspectrum example
run args="":
    cd examples/sdl2-zxspectrum && \
        cargo run -p sdl2-zxspectrum --release --no-default-features --features={{features}} -- {{args}}

# run sdl2-zxspectrum example - MIR optimized (rustc nightly)
run-mir args="": rustcwrap
    cd examples/sdl2-zxspectrum && \
        RUSTFLAGS="{{optimizations}}" RUSTC_WRAPPER="./rustcwrap" \
            cargo +nightly-{{target}} run --target="{{target}}" --release \
                --no-default-features --features={{features}} -- {{args}}

# run sdl2-zxspectrum profile generate (rustc nightly)
run-profgen args="":
    set -euxo pipefail
    # rustup component add llvm-tools-preview
    rm -rf tmp/pgo-data
    cd examples/sdl2-zxspectrum && \
        RUSTFLAGS="-Cprofile-generate=tmp/pgo-data" \
            cargo +nightly-{{target}} run --target="{{target}}" \
                --release --no-default-features --features={{features}} -- {{args}}
    {{llvm_profdata_exe}} merge -o tmp/pgo-data/merged.profdata tmp/pgo-data

# run sdl2-zxspectrum with profile-driven optimizations (rustc nightly)
run-prof args="":
    cd examples/sdl2-zxspectrum && \
    RUSTFLAGS="-Cllvm-args=-pgo-warn-missing-function -Cprofile-use={{justfile_directory()}}/tmp/pgo-data/merged.profdata" \
        cargo +nightly-{{target}} run --target="{{target}}" --release --no-default-features --features={{features}} -- {{args}}

# build rustcwrap for MIR builds
rustcwrap:
    rustc rustcwrap.rs -o rustcwrap.exe

# build all docs
doc:
    cargo +nightly doc -p zxspectrum-common --all-features

# run all tests
test-all: test test-examples

# run library tests
test:
    set -euxo pipefail
    cargo test --no-default-features -- --nocapture
    cargo test --no-default-features -- --ignored --nocapture
    cargo test -- --nocapture
    cargo test -- --ignored --nocapture
    cargo test --features=boxed_frame_cache -- --nocapture
    cargo test --features=boxed_frame_cache -- --ignored --nocapture
    cargo test --no-default-features --features=boxed_frame_cache -- --nocapture
    cargo test --no-default-features --features=boxed_frame_cache -- --ignored --nocapture

# test examples
test-examples:
    set -euxo pipefail
    cargo build -p audio --bins
    cargo test -p audio -- --nocapture
    cd examples/zxspectrum-common && cargo build
    cd examples/zxspectrum-common && cargo test -- --nocapture
    cd examples/zxspectrum-common && cargo build --no-default-features
    cd examples/zxspectrum-common && cargo test --no-default-features -- --nocapture
    cd examples/sdl2-zxspectrum && cargo build
    cd examples/sdl2-zxspectrum && cargo test -- --nocapture
    cd examples/sdl2-zxspectrum && cargo build --no-default-features --features=bundled
    cd examples/sdl2-zxspectrum && cargo test --no-default-features --features=bundled -- --nocapture
    cd examples/sdl2-zxspectrum && cargo build --no-default-features --features=bundled,compact
    cd examples/sdl2-zxspectrum && cargo test --no-default-features --features=bundled,compact -- --nocapture

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
