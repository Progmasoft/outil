// SPDX-FileCopyrightText: 2026 Progmasoft <support@progmasoft.com>
// SPDX-License-Identifier: EPL-2.0

//! A safe layer over `outil-fyaml-sys`: a YAML document read by libfyaml,
//! as a tree of nodes that own their text and know where they start.
//!
//! The tree is copied out of libfyaml while the document is alive, so no
//! pointer of libfyaml outlives [`parse`]. Aliases are kept as aliases and
//! never expanded, and duplicate keys are kept with their positions, so the
//! caller decides about both.

#![deny(unsafe_op_in_unsafe_fn, clippy::undocumented_unsafe_blocks)]

use std::ffi::CStr;
use std::os::raw::{c_char, c_void};
use std::ptr;

use outil_fyaml_sys as sys;

/// Where a node or an error starts: its line and its column, both from 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mark {
    /// The line, from 1.
    pub line: usize,
    /// The column, from 1.
    pub column: usize,
}

/// How a scalar is written, which decides how it is typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarStyle {
    /// Without quotes: its text may be a bool, a number or null.
    Plain,
    /// In single quotes: always a string.
    SingleQuoted,
    /// In double quotes: always a string.
    DoubleQuoted,
    /// A `|` block: always a string.
    Literal,
    /// A `>` block: always a string.
    Folded,
}

/// A node of a YAML document.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    /// A scalar.
    Scalar {
        /// Its text, with escapes and folding applied.
        text: String,
        /// How it is written.
        style: ScalarStyle,
        /// Its tag, such as `!!str`, when it has one.
        tag: Option<String>,
        /// Where it starts.
        mark: Mark,
    },
    /// A sequence.
    Sequence {
        /// Its items.
        items: Vec<Node>,
        /// Where it starts.
        mark: Mark,
    },
    /// A mapping, its pairs in the order written, duplicates included.
    Mapping {
        /// Its keys and values.
        pairs: Vec<(Node, Node)>,
        /// Where it starts.
        mark: Mark,
    },
    /// An alias, `*name`, which is not expanded.
    Alias {
        /// The name of the anchor.
        name: String,
        /// Where it starts.
        mark: Mark,
    },
}

impl Node {
    /// Where the node starts.
    #[must_use]
    pub fn mark(&self) -> Mark {
        match self {
            Node::Scalar { mark, .. }
            | Node::Sequence { mark, .. }
            | Node::Mapping { mark, .. }
            | Node::Alias { mark, .. } => *mark,
        }
    }
}

/// A document libfyaml could not read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{}", match .mark { Some(mark) => format!("line {} column {}: {}", mark.line, mark.column, .message), None => .message.clone() })]
pub struct Error {
    /// What is wrong, in libfyaml's words.
    pub message: String,
    /// Where, when libfyaml says.
    pub mark: Option<Mark>,
}

/// Discards the text libfyaml would print; the errors are collected instead.
extern "C" fn discard(
    _diag: *mut sys::fy_diag,
    _user: *mut c_void,
    _text: *const c_char,
    _length: usize,
) {
}

/// A diagnostic object that collects errors and prints nothing.
struct Diagnostics(*mut sys::fy_diag);

impl Diagnostics {
    fn new() -> Option<Self> {
        // SAFETY: fy_diag_cfg is plain data; fy_diag_cfg_default fills it, and
        // fy_diag_create copies what it needs from it.
        unsafe {
            let mut config: sys::fy_diag_cfg = std::mem::zeroed();
            sys::fy_diag_cfg_default(&raw mut config);
            config.fp = ptr::null_mut();
            config.output_fn = Some(discard);
            let diag = sys::fy_diag_create(&raw const config);
            if diag.is_null() {
                return None;
            }
            sys::fy_diag_set_collect_errors(diag, true);
            Some(Diagnostics(diag))
        }
    }

    /// The first error collected.
    fn first_error(&self) -> Option<Error> {
        let mut iterator: *mut c_void = ptr::null_mut();
        // SAFETY: the diagnostic object is alive; the error and its message
        // belong to it and are copied before it goes.
        unsafe {
            let error = sys::fy_diag_errors_iterate(self.0, &raw mut iterator);
            if error.is_null() {
                return None;
            }
            let error = &*error;
            let message = if error.msg.is_null() {
                "the YAML is malformed".to_owned()
            } else {
                CStr::from_ptr(error.msg).to_string_lossy().into_owned()
            };
            let mark = (error.line >= 0 && error.column >= 0).then(|| Mark {
                line: usize::try_from(error.line).unwrap_or(0) + 1,
                column: usize::try_from(error.column).unwrap_or(0) + 1,
            });
            Some(Error { message, mark })
        }
    }
}

impl Drop for Diagnostics {
    fn drop(&mut self) {
        // SAFETY: the object was created by fy_diag_create and is released once.
        unsafe { sys::fy_diag_destroy(self.0) }
    }
}

/// A document of libfyaml, destroyed when it goes.
struct Document(*mut sys::fy_document);

impl Drop for Document {
    fn drop(&mut self) {
        // SAFETY: the document was built by libfyaml and is destroyed once.
        unsafe { sys::fy_document_destroy(self.0) }
    }
}

/// The text of a libfyaml string of a length.
///
/// # Safety
///
/// `text` points to `length` bytes, or is null.
unsafe fn text_of(text: *const c_char, length: usize) -> String {
    if text.is_null() {
        return String::new();
    }
    // SAFETY: the caller gives a pointer to `length` bytes.
    let bytes = unsafe { std::slice::from_raw_parts(text.cast::<u8>(), length) };
    String::from_utf8_lossy(bytes).into_owned()
}

/// Where a node starts.
///
/// # Safety
///
/// `node` is a node of a live document.
unsafe fn mark_of(node: *mut sys::fy_node) -> Mark {
    // SAFETY: the node is alive; its token and the mark belong to the document.
    unsafe {
        let token = sys::fy_node_get_start_token(node);
        if token.is_null() {
            return Mark::default();
        }
        // The style mark starts at the quote or the indicator of the node.
        let mark = sys::fy_token_style_start_mark(token);
        if mark.is_null() {
            return Mark::default();
        }
        Mark {
            line: usize::try_from((*mark).line).unwrap_or(0) + 1,
            column: usize::try_from((*mark).column).unwrap_or(0) + 1,
        }
    }
}

fn style_of(style: sys::fy_node_style) -> ScalarStyle {
    match style {
        sys::fy_node_style::FYNS_SINGLE_QUOTED => ScalarStyle::SingleQuoted,
        sys::fy_node_style::FYNS_DOUBLE_QUOTED => ScalarStyle::DoubleQuoted,
        sys::fy_node_style::FYNS_LITERAL => ScalarStyle::Literal,
        sys::fy_node_style::FYNS_FOLDED => ScalarStyle::Folded,
        _ => ScalarStyle::Plain,
    }
}

/// How deep a document may nest; a deeper one is refused rather than
/// followed into a stack overflow.
const MAX_DEPTH: usize = 256;

/// Copies a node of a live document into the tree.
///
/// # Safety
///
/// `node` is a node of a live document.
unsafe fn copy(node: *mut sys::fy_node, depth: usize) -> Result<Node, Error> {
    // SAFETY: the caller's node is alive; every pointer taken from it belongs
    // to the same document and is read before the document goes.
    unsafe {
        let mark = mark_of(node);
        if depth > MAX_DEPTH {
            return Err(Error {
                message: format!("the document nests deeper than {MAX_DEPTH} levels"),
                mark: Some(mark),
            });
        }
        match sys::fy_node_get_type(node) {
            sys::fy_node_type::FYNT_SCALAR => {
                let mut length = 0;
                let text = sys::fy_node_get_scalar(node, &raw mut length);
                let text = text_of(text, length);
                let style = sys::fy_node_get_style(node);
                if style == sys::fy_node_style::FYNS_ALIAS {
                    return Ok(Node::Alias { name: text, mark });
                }
                let mut tag_length = 0;
                let tag = sys::fy_node_get_tag(node, &raw mut tag_length);
                let tag = (!tag.is_null()).then(|| text_of(tag, tag_length));
                Ok(Node::Scalar {
                    text,
                    style: style_of(style),
                    tag,
                    mark,
                })
            }
            sys::fy_node_type::FYNT_SEQUENCE => {
                let mut items = Vec::new();
                let mut iterator: *mut c_void = ptr::null_mut();
                loop {
                    let item = sys::fy_node_sequence_iterate(node, &raw mut iterator);
                    if item.is_null() {
                        break;
                    }
                    items.push(copy(item, depth + 1)?);
                }
                Ok(Node::Sequence { items, mark })
            }
            sys::fy_node_type::FYNT_MAPPING => {
                let mut pairs = Vec::new();
                let mut iterator: *mut c_void = ptr::null_mut();
                loop {
                    let pair = sys::fy_node_mapping_iterate(node, &raw mut iterator);
                    if pair.is_null() {
                        break;
                    }
                    let key = sys::fy_node_pair_key(pair);
                    let value = sys::fy_node_pair_value(pair);
                    let key = if key.is_null() {
                        null_scalar(mark)
                    } else {
                        copy(key, depth + 1)?
                    };
                    let value = if value.is_null() {
                        null_scalar(mark)
                    } else {
                        copy(value, depth + 1)?
                    };
                    pairs.push((key, value));
                }
                Ok(Node::Mapping { pairs, mark })
            }
        }
    }
}

/// An empty plain scalar, which YAML reads as null.
fn null_scalar(mark: Mark) -> Node {
    Node::Scalar {
        text: String::new(),
        style: ScalarStyle::Plain,
        tag: None,
        mark,
    }
}

/// Reads the first document of a YAML text.
///
/// libfyaml reads YAML 1.2 and keeps duplicate keys, aliases and anchors as
/// written; nothing is expanded.
///
/// # Errors
///
/// The text is not YAML; the first error of libfyaml, with its line and
/// column.
pub fn parse(text: &str) -> Result<Option<Node>, Error> {
    let diagnostics = Diagnostics::new().ok_or_else(|| Error {
        message: "libfyaml could not start".to_owned(),
        mark: None,
    })?;
    let flags = sys::fy_parse_cfg_flags::FYPCF_QUIET
        | sys::fy_parse_cfg_flags::FYPCF_COLLECT_DIAG
        | sys::fy_parse_cfg_flags::FYPCF_ALLOW_DUPLICATE_KEYS
        | sys::fy_parse_cfg_flags::FYPCF_DEFAULT_VERSION_1_2;
    let config = sys::fy_parse_cfg {
        search_path: ptr::null(),
        flags,
        userdata: ptr::null_mut(),
        diag: diagnostics.0,
    };
    // SAFETY: the configuration and the text outlive the call; the document
    // copies the text it needs, and is destroyed by its guard.
    let document = unsafe {
        sys::fy_document_build_from_string(&raw const config, text.as_ptr().cast(), text.len())
    };
    if document.is_null() {
        // A text of only comments or white space has no document.
        return match diagnostics.first_error() {
            Some(error) => Err(error),
            None => Ok(None),
        };
    }
    let document = Document(document);
    // SAFETY: the document is alive until its guard goes, after the copy.
    unsafe {
        let root = sys::fy_document_root(document.0);
        if root.is_null() {
            return Ok(None);
        }
        copy(root, 0).map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scalar(node: &Node) -> (&str, ScalarStyle) {
        match node {
            Node::Scalar { text, style, .. } => (text, *style),
            other => panic!("not a scalar: {other:?}"),
        }
    }

    #[test]
    fn reads_a_mapping_with_marks_and_styles() {
        let root = parse("tasks:\n  build:\n    cmds:\n      - make\n      - 'make check'\n")
            .unwrap()
            .unwrap();
        let Node::Mapping { pairs, .. } = &root else {
            panic!("{root:?}")
        };
        assert_eq!(scalar(&pairs[0].0), ("tasks", ScalarStyle::Plain));
        let Node::Mapping { pairs: tasks, .. } = &pairs[0].1 else {
            panic!()
        };
        assert_eq!(tasks[0].0.mark(), Mark { line: 2, column: 3 });
        let Node::Mapping { pairs: build, .. } = &tasks[0].1 else {
            panic!()
        };
        let Node::Sequence { items, .. } = &build[0].1 else {
            panic!()
        };
        assert_eq!(scalar(&items[1]), ("make check", ScalarStyle::SingleQuoted));
        assert_eq!(items[1].mark(), Mark { line: 5, column: 9 });
    }

    #[test]
    fn keeps_duplicate_keys_and_aliases() {
        let root = parse("a: &x 1\na: *x\n").unwrap().unwrap();
        let Node::Mapping { pairs, .. } = root else {
            panic!()
        };
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[1].0.mark().line, 2);
        assert!(matches!(&pairs[1].1, Node::Alias { name, .. } if name == "x"));
    }

    #[test]
    fn reads_blocks_and_tags() {
        let root = parse("a: |\n  one\n  two\nb: !!str 5\nc:\n")
            .unwrap()
            .unwrap();
        let Node::Mapping { pairs, .. } = root else {
            panic!()
        };
        assert_eq!(scalar(&pairs[0].1), ("one\ntwo\n", ScalarStyle::Literal));
        assert!(matches!(&pairs[1].1, Node::Scalar { tag: Some(tag), .. } if tag.ends_with("str")));
        assert_eq!(scalar(&pairs[2].1).0, "");
    }

    #[test]
    fn reports_errors_with_their_position() {
        let error = parse("a: [1, 2\nb: c\n").unwrap_err();
        assert!(error.mark.is_some(), "{error}");
        assert_ne!(error.message, "");
        assert_eq!(parse("").unwrap(), None);
        assert_eq!(parse("# only a comment\n").unwrap(), None);
    }

    #[test]
    fn refuses_a_document_that_nests_too_deep() {
        let deep = format!("{}{}", "[".repeat(300), "]".repeat(300));
        let error = parse(&deep).unwrap_err();
        assert!(
            error.message.contains("nests deeper") || error.mark.is_some(),
            "{error}"
        );
    }
}
