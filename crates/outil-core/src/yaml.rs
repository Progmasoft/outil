// SPDX-FileCopyrightText: 2026 Progmasoft <support@progmasoft.com>
// SPDX-License-Identifier: EPL-2.0

//! `Outil.yml`: its YAML, read by libfyaml through outil-fyaml-rs, turned
//! into the values of an Outil file.
//!
//! Scalars are typed by the core schema of YAML 1.2: a plain scalar may be
//! null, a bool or a number, and a quoted or block scalar is a string. Aliases
//! are refused, so that no file can make Outil expand a value again and
//! again; a key written twice is refused with both of its lines.

use outil_fyaml_rs::{Mark, Node, ScalarStyle};

use crate::value::Value;

fn at(mark: Mark, message: impl std::fmt::Display) -> String {
    format!("line {} column {}: {message}", mark.line, mark.column)
}

/// The value of a plain scalar by the core schema of YAML 1.2.
fn plain(text: &str, mark: Mark) -> Result<Value, String> {
    match text {
        "" | "~" | "null" | "Null" | "NULL" => return Ok(Value::Null),
        "true" | "True" | "TRUE" => return Ok(Value::Bool(true)),
        "false" | "False" | "FALSE" => return Ok(Value::Bool(false)),
        ".inf" | ".Inf" | ".INF" | "+.inf" | "+.Inf" | "+.INF" | "-.inf" | "-.Inf" | "-.INF"
        | ".nan" | ".NaN" | ".NAN" => {
            return Err(at(
                mark,
                "infinities and NaN are not values of an Outil file",
            ));
        }
        _ => {}
    }
    match integer(text) {
        Integer::Fits(value) => return Ok(Value::Integer(value)),
        Integer::TooLarge => return Err(at(mark, format!("{text} is too large"))),
        Integer::NotOne => {}
    }
    if is_float(text) {
        if let Ok(value) = text.parse::<f64>() {
            if value.is_finite() {
                return Ok(Value::Float(value));
            }
        }
        return Err(at(mark, format!("{text} is too large")));
    }
    Ok(Value::String(text.to_owned()))
}

/// What a text is as an integer of the core schema.
enum Integer {
    /// It is not written as an integer.
    NotOne,
    /// It is one, and fits.
    Fits(i64),
    /// It is one, and does not fit 64 bits.
    TooLarge,
}

/// The integer a text is: `[-+]?[0-9]+`, `0o[0-7]+` or `0x[0-9a-fA-F]+`.
fn integer(text: &str) -> Integer {
    let digits = |body: &str, valid: fn(char) -> bool| !body.is_empty() && body.chars().all(valid);
    let (body, radix, valid): (&str, u32, fn(char) -> bool) =
        if let Some(body) = text.strip_prefix("0o") {
            (body, 8, |c| ('0'..='7').contains(&c))
        } else if let Some(body) = text.strip_prefix("0x") {
            (body, 16, |c| c.is_ascii_hexdigit())
        } else {
            (text, 10, |c| c.is_ascii_digit())
        };
    let unsigned = if radix == 10 {
        body.strip_prefix(['-', '+']).unwrap_or(body)
    } else {
        body
    };
    if !digits(unsigned, valid) {
        return Integer::NotOne;
    }
    match i64::from_str_radix(body, radix) {
        Ok(value) => Integer::Fits(value),
        Err(_) => Integer::TooLarge,
    }
}

/// Whether a text is a float of the core schema:
/// `[-+]?(\.[0-9]+|[0-9]+(\.[0-9]*)?)([eE][-+]?[0-9]+)?`.
fn is_float(text: &str) -> bool {
    let body = text.strip_prefix(['-', '+']).unwrap_or(text);
    let (mantissa, exponent) = match body.find(['e', 'E']) {
        Some(index) => (&body[..index], Some(&body[index + 1..])),
        None => (body, None),
    };
    let all_digits = |part: &str| part.chars().all(|c| c.is_ascii_digit());
    let mantissa_ok = match mantissa.split_once('.') {
        Some((whole, fraction)) => {
            all_digits(whole) && all_digits(fraction) && !(whole.is_empty() && fraction.is_empty())
        }
        None => !mantissa.is_empty() && all_digits(mantissa),
    };
    let exponent_ok = exponent.is_none_or(|part| {
        let digits = part.strip_prefix(['-', '+']).unwrap_or(part);
        !digits.is_empty() && all_digits(digits)
    });
    mantissa_ok && exponent_ok
}

fn scalar(text: &str, style: ScalarStyle, tag: Option<&str>, mark: Mark) -> Result<Value, String> {
    match tag {
        None => {}
        Some(tag) if tag == "!!str" || tag == "tag:yaml.org,2002:str" => {
            return Ok(Value::String(text.to_owned()));
        }
        Some(tag) => {
            return Err(at(
                mark,
                format!("the tag {tag} is not part of an Outil file"),
            ));
        }
    }
    if style == ScalarStyle::Plain {
        plain(text, mark)
    } else {
        Ok(Value::String(text.to_owned()))
    }
}

fn key_of(node: &Node) -> Result<String, String> {
    match node {
        Node::Scalar {
            text,
            style,
            tag,
            mark,
        } => match scalar(text, *style, tag.as_deref(), *mark)? {
            Value::Null => Err(at(*mark, "a key is empty")),
            value => Ok(value.to_text()),
        },
        other => Err(at(other.mark(), "a key is a scalar, not a list or a map")),
    }
}

fn mapping(pairs: &[(Node, Node)], depth: usize) -> Result<Value, String> {
    let mut entries: Vec<(String, Value)> = Vec::new();
    let mut lines: Vec<(String, usize)> = Vec::new();
    for (key, value) in pairs {
        let name = key_of(key)?;
        if let Some((_, first)) = lines.iter().find(|(known, _)| *known == name) {
            return Err(format!(
                "the key `{name}` is written twice in one map, on lines {first} and {}",
                key.mark().line
            ));
        }
        lines.push((name.clone(), key.mark().line));
        let value = convert(value, depth + 1)?;
        // null stands for a key that is left out.
        if value != Value::Null {
            entries.push((name, value));
        }
    }
    Ok(Value::Object(entries))
}

fn convert(node: &Node, depth: usize) -> Result<Value, String> {
    match node {
        Node::Scalar {
            text,
            style,
            tag,
            mark,
        } => scalar(text, *style, tag.as_deref(), *mark),
        Node::Sequence { items, .. } => items
            .iter()
            .map(|item| match convert(item, depth + 1)? {
                Value::Null => Err(at(item.mark(), "a null item of a list is an error")),
                value => Ok(value),
            })
            .collect::<Result<_, _>>()
            .map(Value::Array),
        Node::Mapping { pairs, .. } => mapping(pairs, depth),
        Node::Alias { name, mark } => Err(at(
            *mark,
            format!("the alias *{name} is not part of an Outil file; write the value out"),
        )),
    }
}

/// Reads the text of an `Outil.yml`.
///
/// # Errors
///
/// The YAML is malformed, or has an alias, a tag, a key written twice, a key
/// that is not a scalar, or a value no Outil file has.
pub fn parse_yaml(text: &str) -> Result<Value, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    match outil_fyaml_rs::parse(text).map_err(|error| error.to_string())? {
        Some(root) => convert(&root, 0),
        None => Ok(Value::Object(Vec::new())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn types_plain_scalars_by_the_core_schema() {
        let value = parse_yaml(
            "a: true\nb: 8\nc: 0x1F\nd: 0.5\ne: 1e3\nf: hello\ng: '8'\nh: \"true\"\ni: ~\nj: -3\nk: 0o17\n",
        )
        .unwrap();
        assert_eq!(value.get("a"), Some(&Value::Bool(true)));
        assert_eq!(value.get("b"), Some(&Value::Integer(8)));
        assert_eq!(value.get("c"), Some(&Value::Integer(31)));
        assert_eq!(value.get("d"), Some(&Value::Float(0.5)));
        assert_eq!(value.get("e"), Some(&Value::Float(1000.0)));
        assert_eq!(value.get("f"), Some(&Value::String("hello".into())));
        assert_eq!(value.get("g"), Some(&Value::String("8".into())));
        assert_eq!(value.get("h"), Some(&Value::String("true".into())));
        assert_eq!(value.get("i"), None);
        assert_eq!(value.get("j"), Some(&Value::Integer(-3)));
        assert_eq!(value.get("k"), Some(&Value::Integer(15)));
    }

    #[test]
    fn keeps_the_order_and_blocks() {
        let value = parse_yaml("z: 1\na: |\n  line one\n  line two\n").unwrap();
        let Value::Object(entries) = &value else {
            panic!()
        };
        assert_eq!(entries[0].0, "z");
        assert_eq!(
            value.get("a"),
            Some(&Value::String("line one\nline two\n".into()))
        );
    }

    #[test]
    fn refuses_what_an_outil_file_does_not_have() {
        let cases = [
            ("a: &x [1]\nb: *x\n", "the alias *x"),
            (
                "a: 1\nb: 2\na: 3\n",
                "`a` is written twice in one map, on lines 1 and 3",
            ),
            ("a: .inf\n", "infinities"),
            ("a: !!binary eA==\n", "the tag"),
            ("[1]: x\n", "a key is a scalar"),
            ("a: [1, ~]\n", "a null item"),
            ("a: 99999999999999999999\n", "too large"),
            ("a: [1\n", "line"),
        ];
        for (text, expected) in cases {
            let error = parse_yaml(text).unwrap_err();
            assert!(error.contains(expected), "{text:?}: {error}");
        }
    }

    #[test]
    fn a_tag_of_str_keeps_the_text() {
        assert_eq!(
            parse_yaml("a: !!str 5\n").unwrap().get("a"),
            Some(&Value::String("5".into()))
        );
    }

    #[test]
    fn an_empty_file_is_an_empty_map() {
        assert_eq!(
            parse_yaml("# nothing\n").unwrap(),
            Value::Object(Vec::new())
        );
    }
}
