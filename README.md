<!-- SPDX-FileCopyrightText: 2026 Progmasoft <support@progmasoft.com> -->
<!-- SPDX-License-Identifier: EPL-2.0 -->

# Outil

Outil is a tool for build automation. A project describes its tasks in one file, `Outil.json5` or `Outil.yml`, in the
directory it belongs to; Outil reads that file and runs the tasks. A task names its inputs (`srcs`) and its outputs
(`gens`), and the values of the file can use templates written `${{ VARIABLE }}`. The two files describe the same model,
with the same keys and values; a directory holds one of them, never both.

Outil is written in Rust, edition 2024. It reads `Outil.json5` first; `Outil.yml` follows, read through the C API of
libfyaml. Their schemas are published on `schemastore.progmasoft.com`.

The repository is a Cargo workspace of four crates:

| Crate | What it is |
| --- | --- |
| `outil` | the `outil` command: its command line, read by hand |
| `outil-core` | the Outil file, its model and templates, and running its tasks |
| `outil-fyaml-sys` | the C API of libfyaml (MIT), built from source and bound by bindgen |
| `outil-fyaml-rs` | a safe layer over `outil-fyaml-sys`, for reading `Outil.yml` |

Outil reads `Outil.json5` or `Outil.yml` and runs its tasks: their dependencies at the same time, then their commands
in order through the shell, `cmd.exe` on Windows and `sh` elsewhere, with `vars`, `env`, `requires`, `preconditions`,
task calls, aliases, internal tasks and `${{ }}` templates. A key of the format that this version does not carry out
yet, such as `srcs` or `includes`, is refused by name, so that nothing is skipped without a word.

`Outil.yml` is read by libfyaml 1.0.0-rc2, built from the source in `crates/outil-fyaml-sys/libfyaml/` by CMake and
bound by bindgen, so building Outil needs CMake, a C compiler and libclang. Its scalars are typed by the core schema
of YAML 1.2; aliases are refused, so no file can make Outil expand a value again and again, and a key written twice
is refused with both of its lines. `outil-fyaml-sys` and `outil-fyaml-rs` are published to Progmasoft's Cargo
registry, `sparse+https://rustreg.progmasoft.com/index/`.

```sh
cargo test --workspace
cargo run -p outil -- --list
```

## Installing

Releases are published on `storage.progmasoft.com/outil/<version>/`: archives for macOS (Apple silicon and Intel) and
Windows (x64 and ARM64), each beside its SHA-256 and BLAKE3 sums and its CycloneDX SBOM, and install scripts
(`install.zsh`, `Install-Outil.ps1`, `install.cmd`). On Linux, Outil installs from Progmasoft's package repositories.

## License

[EPL-2.0](LICENSE.txt). Progmasoft's own code is also covered by the [Progmasoft Patent Grant 1.1](PATENTS.md).
Participation follows the [Code of Conduct](CODE_OF_CONDUCT.md).
