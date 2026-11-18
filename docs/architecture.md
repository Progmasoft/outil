<!-- SPDX-FileCopyrightText: 2026 Progmasoft <support@progmasoft.com> -->
<!-- SPDX-License-Identifier: EPL-2.0 -->

# Architecture

## Crates

| Crate | What it is |
| --- | --- |
| `outil` | The command: its command line, read by hand, and `main` |
| `outil-core` | The Outil file, its model and templates, and running its tasks |
| `outil-fyaml-sys` | libfyaml 1.0.0-rc2, vendored in `libfyaml/`, built by CMake, bound by bindgen |
| `outil-fyaml-rs` | A safe layer over `outil-fyaml-sys`: a document as owned nodes with their positions |

The two libfyaml crates are published to Progmasoft's Cargo registry; `outil` and `outil-core` use them by path in
this workspace.

## From a file to tasks

1. `outil-core` finds `Outil.json5` or `Outil.yml` in the directory or above it; both in one directory is an error.
2. The text becomes `Value`s: JSON5 through the json5 crate (`value.rs`), YAML through `outil-fyaml-rs` (`yaml.rs`),
   which types scalars by the core schema of YAML 1.2 and refuses aliases. Both refuse a key written twice, naming
   both lines.
3. `model.rs` reads the values into a `File` of tasks, variables and commands, refusing unknown keys and naming the
   keys of the format this version does not carry out.
4. `run.rs` runs a task: its dependencies at the same time, then its checks, then its commands in order through the
   shell, `cmd.exe` on Windows and `sh` elsewhere. `template.rs` writes the `${{ }}` templates of each string.
