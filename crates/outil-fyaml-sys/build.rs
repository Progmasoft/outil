// SPDX-FileCopyrightText: 2026 Progmasoft <support@progmasoft.com>
// SPDX-License-Identifier: EPL-2.0

//! Builds libfyaml from the source in `libfyaml/` with `CMake`, as a static
//! library, and writes the bindings of its C API with bindgen.

use std::path::PathBuf;

fn main() {
    let source = PathBuf::from("libfyaml");
    println!("cargo:rerun-if-changed=libfyaml");
    println!("cargo:rerun-if-changed=wrapper.h");

    // Only the library: no tools, no tests, no network, and none of the
    // subsystems that need a compiler extension or libclang at run time.
    let build = cmake::Config::new(&source)
        .define("BUILD_SHARED_LIBS", "OFF")
        .define("BUILD_TESTING", "OFF")
        .define("ENABLE_NETWORK", "OFF")
        .define("ENABLE_REFLECTION", "OFF")
        .define("ENABLE_GENERIC", "OFF")
        .define("ENABLE_LIBCLANG", "OFF")
        .define("ENABLE_PYTHON_BINDINGS", "OFF")
        .define("LIBFYAML_ENABLE_TAGS", "OFF")
        .define("ENABLE_PORTABLE_TARGET", "ON")
        .build_target("fyaml_static")
        .build();
    let build_dir = build.join("build");
    println!("cargo:rustc-link-search=native={}", build_dir.display());
    for profile in ["Release", "Debug", "RelWithDebInfo", "MinSizeRel"] {
        println!(
            "cargo:rustc-link-search=native={}",
            build_dir.join(profile).display()
        );
    }
    println!("cargo:rustc-link-lib=static=fyaml_static");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo:rustc-link-lib=dylib=pthread");
    }

    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        .clang_arg(format!("-I{}", source.join("include").display()))
        .clang_arg("-std=gnu11")
        // The bindings need the types and the declarations, not the bodies of
        // the atomic helpers; without this, bindgen depends on the
        // <stdatomic.h> of whatever libclang it finds, and some lack C11's.
        // The library itself is built by CMake with its own atomics.
        .clang_arg("-D__STDC_NO_ATOMICS__=1")
        .allowlist_function("fy_.*")
        .allowlist_type("fy_.*")
        .allowlist_var("FY.*")
        .rustified_enum("fy_node_type")
        .rustified_enum("fy_node_style")
        .rustified_enum("fy_error_type")
        .bitfield_enum("fy_parse_cfg_flags")
        .layout_tests(false)
        .generate_comments(false)
        .generate()
        .expect("bindgen could not read the libfyaml headers");
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("Cargo sets OUT_DIR"));
    bindings
        .write_to_file(out.join("bindings.rs"))
        .expect("the bindings could not be written");
}
