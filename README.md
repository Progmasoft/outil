<!-- SPDX-FileCopyrightText: 2026 Progmasoft <support@progmasoft.com> -->
<!-- SPDX-License-Identifier: EPL-2.0 -->

# Outil

Outil is a tool for build automation. A project describes its tasks in one file, `Outil.json5` or `Outil.yml`, in the
directory it belongs to; Outil reads that file and runs the tasks. A task names its inputs (`srcs`) and its outputs
(`gens`), and the values of the file can use templates written `${{ VARIABLE }}`. The two files describe the same model,
with the same keys and values; a directory holds one of them, never both.

Outil is written in Rust, edition 2024. It reads `Outil.json5` first; `Outil.yml` follows, read through the C API of
libfyaml. Their schemas are published on `schemastore.progmasoft.com`.

No code is written yet.

## Installing

Releases are published on `storage.progmasoft.com/outil/<version>/`: archives for macOS (Apple silicon and Intel) and
Windows (x64 and ARM64), each beside its SHA-256 and BLAKE3 sums and its CycloneDX SBOM, and install scripts
(`install.zsh`, `Install-Outil.ps1`, `install.cmd`). On Linux, Outil installs from Progmasoft's package repositories.

## License

[EPL-2.0](LICENSE.txt). Progmasoft's own code is also covered by the [Progmasoft Patent Grant 1.1](PATENTS.md).
Participation follows the [Code of Conduct](CODE_OF_CONDUCT.md).
