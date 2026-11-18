// SPDX-FileCopyrightText: 2026 Progmasoft <support@progmasoft.com>
// SPDX-License-Identifier: EPL-2.0

//! The values of an Outil file, read from JSON5 before they become the model.

use std::fmt;

use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};

/// A value of an Outil file: what JSON5 and YAML have in common.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// `null`: a key left out.
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// A whole number.
    Integer(i64),
    /// A number with a fraction.
    Float(f64),
    /// A string.
    String(String),
    /// An array.
    Array(Vec<Value>),
    /// An object, its keys in the order written.
    Object(Vec<(String, Value)>),
}

impl Value {
    /// The name of the kind of the value, for messages.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Value::Null => "null",
            Value::Bool(_) => "a bool",
            Value::Integer(_) | Value::Float(_) => "a number",
            Value::String(_) => "a string",
            Value::Array(_) => "a list",
            Value::Object(_) => "a map",
        }
    }

    /// The value as text, as a template writes it: a number in its shortest
    /// decimal form, a bool as `true` or `false`, a list joined by spaces.
    #[must_use]
    pub fn to_text(&self) -> String {
        match self {
            Value::Null => String::new(),
            Value::Bool(value) => value.to_string(),
            Value::Integer(value) => value.to_string(),
            Value::Float(value) => format_float(*value),
            Value::String(value) => value.clone(),
            Value::Array(items) => items
                .iter()
                .map(Value::to_text)
                .collect::<Vec<_>>()
                .join(" "),
            Value::Object(entries) => entries
                .iter()
                .map(|(key, value)| format!("{key}={}", value.to_text()))
                .collect::<Vec<_>>()
                .join(" "),
        }
    }

    /// The entry of an object with a key.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(entries) => entries
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value),
            _ => None,
        }
    }
}

/// The shortest decimal form of a float that reads back as the same float.
fn format_float(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        #[allow(clippy::cast_possible_truncation)]
        return (value as i64).to_string();
    }
    value.to_string()
}

impl<'de> Deserialize<'de> for Value {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(ValueVisitor)
    }
}

struct ValueVisitor;

impl<'de> Visitor<'de> for ValueVisitor {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a JSON5 value")
    }

    fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_none<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        Value::deserialize(deserializer)
    }

    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Value, E> {
        Ok(Value::Bool(value))
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Value, E> {
        Ok(Value::Integer(value))
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Value, E> {
        i64::try_from(value)
            .map(Value::Integer)
            .map_err(|_| E::custom(format!("{value} is too large")))
    }

    fn visit_i128<E: de::Error>(self, value: i128) -> Result<Value, E> {
        i64::try_from(value)
            .map(Value::Integer)
            .map_err(|_| E::custom(format!("{value} is too large")))
    }

    fn visit_u128<E: de::Error>(self, value: u128) -> Result<Value, E> {
        i64::try_from(value)
            .map(Value::Integer)
            .map_err(|_| E::custom(format!("{value} is too large")))
    }

    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Value, E> {
        if value.is_finite() {
            Ok(Value::Float(value))
        } else {
            Err(E::custom(
                "Infinity and NaN are not values of an Outil file",
            ))
        }
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Value, E> {
        Ok(Value::String(value.to_owned()))
    }

    fn visit_string<E: de::Error>(self, value: String) -> Result<Value, E> {
        Ok(Value::String(value))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Value, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = sequence.next_element::<Value>()? {
            if item == Value::Null {
                return Err(de::Error::custom("a null item of a list is an error"));
            }
            items.push(item);
        }
        Ok(Value::Array(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut entries: Vec<(String, Value)> = Vec::new();
        while let Some(key) = map.next_key::<String>()? {
            if entries.iter().any(|(known, _)| *known == key) {
                return Err(de::Error::custom(format!("{DUPLICATE}{key}")));
            }
            let value = map.next_value::<Value>()?;
            // null stands for a key that is left out.
            if value != Value::Null {
                entries.push((key, value));
            }
        }
        Ok(Value::Object(entries))
    }
}

/// Reads a JSON5 document; a byte order mark at its start is skipped.
///
/// # Errors
///
/// The message of json5, with the line and column it gives.
pub fn parse_json5(text: &str) -> Result<Value, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    json5::from_str::<Value>(text).map_err(|error| {
        let message = error.to_string();
        match message.strip_prefix(DUPLICATE) {
            Some(rest) => {
                // json5 places the error at the object; the lines of the
                // key are found in the text.
                let key = rest.split(" at line ").next().unwrap_or(rest);
                let object = error.position().map_or(0, |position| {
                    offset_of(text, position.line, position.column)
                });
                let lines = key_lines(text, object, key);
                let lines: Vec<String> = lines.iter().map(|line| (line + 1).to_string()).collect();
                format!(
                    "the key `{key}` is written twice in one object, on lines {}",
                    lines.join(" and ")
                )
            }
            None => message,
        }
    })
}

/// The start of the message of a repeated key, which `parse_json5` rewrites.
const DUPLICATE: &str = "\u{0}duplicate key\u{0}";

/// The byte offset of a line and a column, both from 0, as json5 counts them.
fn offset_of(text: &str, line: usize, column: usize) -> usize {
    let mut current = 0;
    for (offset, character) in text.char_indices() {
        if current == line {
            return text[offset..]
                .char_indices()
                .nth(column)
                .map_or(text.len(), |(inner, _)| offset + inner);
        }
        if character == '\n' {
            current += 1;
        }
    }
    text.len()
}

/// The lines, from 0, on which an object that starts at an offset has a key.
fn key_lines(text: &str, object: usize, key: &str) -> Vec<usize> {
    let bytes = text.as_bytes();
    let (mut index, mut depth, mut lines) = (object, 0usize, Vec::new());
    let line_at = |offset: usize| text[..offset].matches('\n').count();
    while index < bytes.len() {
        match bytes[index] {
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                index = text[index..]
                    .find('\n')
                    .map_or(bytes.len(), |end| index + end);
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                index = text[index + 2..]
                    .find("*/")
                    .map_or(bytes.len(), |end| index + end + 4);
            }
            quote @ (b'"' | b'\'') => {
                let start = index;
                index += 1;
                while index < bytes.len() && bytes[index] != quote {
                    index += if bytes[index] == b'\\' { 2 } else { 1 };
                }
                index += 1;
                let content = &text[start + 1..(index - 1).min(text.len())];
                if depth == 1 && content == key && is_key_end(text, index) {
                    lines.push(line_at(start));
                }
                continue;
            }
            b'{' | b'[' => depth += 1,
            b'}' | b']' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            character
                if depth == 1
                    && (character.is_ascii_alphabetic()
                        || character == b'_'
                        || character == b'$') =>
            {
                let start = index;
                while index < bytes.len()
                    && (bytes[index].is_ascii_alphanumeric()
                        || bytes[index] == b'_'
                        || bytes[index] == b'$')
                {
                    index += 1;
                }
                if &text[start..index] == key && is_key_end(text, index) {
                    lines.push(line_at(start));
                }
                continue;
            }
            _ => {}
        }
        index += 1;
    }
    lines
}

/// Whether a key ends at an offset: a `:` follows, after white space.
fn is_key_end(text: &str, offset: usize) -> bool {
    text.get(offset..)
        .is_some_and(|rest| rest.trim_start().starts_with(':'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_json5_in_order() {
        let value = parse_json5("{ b: 1, a: 'x', c: [true, 0x1F, .5], }").unwrap();
        let Value::Object(entries) = value else {
            panic!("not an object")
        };
        let keys: Vec<_> = entries.iter().map(|(key, _)| key.as_str()).collect();
        assert_eq!(keys, ["b", "a", "c"]);
        assert_eq!(
            entries[2].1,
            Value::Array(vec![
                Value::Bool(true),
                Value::Integer(31),
                Value::Float(0.5)
            ])
        );
    }

    #[test]
    fn refuses_a_key_written_twice() {
        let error = parse_json5("{\n  a: 1,\n  a: 2,\n}").unwrap_err();
        assert_eq!(
            error,
            "the key `a` is written twice in one object, on lines 2 and 3"
        );
        let nested = parse_json5(
            "{\n  tasks: {\n    // a: 0\n    'a': 'x',\n    b: { a: 1 },\n    a: 'y',\n  },\n}",
        )
        .unwrap_err();
        assert_eq!(
            nested,
            "the key `a` is written twice in one object, on lines 4 and 6"
        );
    }

    #[test]
    fn null_leaves_a_key_out_but_not_an_item() {
        assert_eq!(parse_json5("{ a: null }").unwrap(), Value::Object(vec![]));
        assert!(parse_json5("[1, null]").is_err());
    }

    #[test]
    fn refuses_infinity_and_nan() {
        assert!(parse_json5("{ a: Infinity }").is_err());
        assert!(parse_json5("{ a: NaN }").is_err());
    }

    #[test]
    fn skips_a_byte_order_mark() {
        assert_eq!(parse_json5("\u{feff}{}").unwrap(), Value::Object(vec![]));
    }

    #[test]
    fn writes_numbers_in_their_shortest_form() {
        assert_eq!(Value::Integer(31).to_text(), "31");
        assert_eq!(Value::Float(5.0).to_text(), "5");
        assert_eq!(Value::Float(0.25).to_text(), "0.25");
    }
}
