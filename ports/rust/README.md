# Rust standard-library port for MakOS

Custom `aarch64-unknown-makos` target. Distinct `os=makos`, `family=unix`,
`env=musl`; never aliases Linux. Firefox requires genuine Rust `std`, so
mapping Gecko crates to a Linux target is rejected.

Current target specification is bootstrap work. Build with official nightly
plus `rust-src` and MakOS libc/C++ sysroot. Runtime claim requires executing
the probe inside MakOS; successful host-side cross compilation alone is not
runtime proof.

`prepare-toolchain.sh` clones official nightly locally, then routes Rust
`std`'s Unix backend through its Linux/musl implementation while keeping
`target_os="makos"`. `prepare-libc.sh` does the equivalent for Rust's `libc`
crate. No Linux target triple or host runtime enters target output.

```sh
rustup toolchain install nightly --component rust-src
ports/rust/build-std.sh
```

Current host proof: real static AArch64 ELF, nonzero `_start`, Rust `std`,
unwind runtime, and `std::thread` linked. Execution remains blocked until
MakOS ELF argv/auxv loading, TLS, clone, futex, and thread-exit contracts land.
