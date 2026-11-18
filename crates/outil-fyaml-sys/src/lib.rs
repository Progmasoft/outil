// SPDX-FileCopyrightText: 2026 Progmasoft <support@progmasoft.com>
// SPDX-License-Identifier: EPL-2.0

//! The C API of libfyaml, which Outil reads `Outil.yml` with.
//!
//! libfyaml 1.0.0-rc2 (MIT, pantoniou/libfyaml) is built from the source in
//! `libfyaml/` by `CMake` as a static library, and bindgen writes these
//! bindings from its headers at build time. Only the core API is bound: the
//! `fy_` functions and types and the `FY` constants. The safe layer is
//! `outil-fyaml-rs`.

/// The code bindgen writes; it is generated, so the lints of hand-written
/// code do not apply to it.
#[allow(
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    missing_docs,
    clippy::all,
    clippy::pedantic
)]
mod bindings {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

pub use bindings::*;

/// The version of libfyaml these bindings are built from.
pub const LIBFYAML_VERSION: &str = "1.0.0-rc2";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_document_through_the_c_api() {
        let text = "a: 1\n";
        // SAFETY: the text outlives the document, which is destroyed here.
        unsafe {
            let document =
                fy_document_build_from_string(std::ptr::null(), text.as_ptr().cast(), text.len());
            assert!(!document.is_null());
            let root = fy_document_root(document);
            assert_eq!(fy_node_get_type(root), fy_node_type::FYNT_MAPPING);
            assert_eq!(fy_node_mapping_item_count(root), 1);
            fy_document_destroy(document);
        }
    }
}
