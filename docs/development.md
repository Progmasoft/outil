<!-- SPDX-FileCopyrightText: 2026 Progmasoft <support@progmasoft.com> -->
<!-- SPDX-License-Identifier: EPL-2.0 -->

# Development

Outil needs Rust (`rust-toolchain.toml` names the version), and CMake, a C compiler and libclang for libfyaml.

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo doc --workspace --no-deps
```

The end-to-end tests in `crates/outil/tests/` run the `outil` command on files in directories of their own, under
Cargo's `CARGO_TARGET_TMPDIR`. CI runs the tests on Linux and macOS, and rustfmt, Clippy (pedantic) and rustdoc as
errors.
