// SPDX-FileCopyrightText: 2026 Progmasoft <support@progmasoft.com>
// SPDX-License-Identifier: EPL-2.0

//! Templates: `${{ NAME }}`, `${{ NAME.KEY }}` and `${{ FUNCTION() }}` in a
//! string; `$${{` writes `${{` itself.

use crate::value::Value;

/// Where a template finds the value of a name.
pub trait Scope {
    /// The value of a variable, if it is set.
    fn lookup(&self, name: &str) -> Option<Value>;
}

/// The operating system, as `OS()` writes it.
#[must_use]
pub fn os_name() -> &'static str {
    match std::env::consts::OS {
        "macos" => "darwin",
        other => other,
    }
}

/// The architecture, as `ARCH()` writes it.
#[must_use]
pub fn arch_name() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        "x86" => "386",
        other => other,
    }
}

fn call(function: &str) -> Result<String, String> {
    match function {
        "OS" => Ok(os_name().to_owned()),
        "ARCH" => Ok(arch_name().to_owned()),
        "numCPU" => Ok(std::thread::available_parallelism()
            .map_or(1, std::num::NonZeroUsize::get)
            .to_string()),
        "exeExt" => Ok(if cfg!(windows) { ".exe" } else { "" }.to_owned()),
        "joinPath" | "relPath" | "fromSlash" | "toSlash" | "splitLines" | "catLines"
        | "splitArgs" | "shellQuote" | "toJson" | "fromJson" | "toYaml" | "fromYaml" => {
            Err(format!(
                "{function}() is part of the format but not carried out by Outil {} yet",
                env!("CARGO_PKG_VERSION")
            ))
        }
        _ => Err(format!("no function {function}()")),
    }
}

fn evaluate(expression: &str, scope: &dyn Scope) -> Result<String, String> {
    if let Some(function) = expression.strip_suffix("()") {
        return call(function);
    }
    let mut parts = expression.split('.');
    let name = parts.next().unwrap_or_default();
    let mut value = scope
        .lookup(name)
        .ok_or_else(|| format!("the variable `{name}` is not set"))?;
    for key in parts {
        value = value
            .get(key)
            .cloned()
            .ok_or_else(|| format!("`{expression}`: `{key}` is not a key of it"))?;
    }
    Ok(value.to_text())
}

/// Writes the templates of a text with the values of a scope.
///
/// # Errors
///
/// A template that is not closed, a variable that is not set, a key that is
/// not there, or a function that does not exist.
pub fn render(text: &str, scope: &dyn Scope) -> Result<String, String> {
    let mut output = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("${{") {
        // `$${{` writes `${{`.
        if rest[..start].ends_with('$') {
            output.push_str(&rest[..start - 1]);
            output.push_str("${{");
            rest = &rest[start + 3..];
            continue;
        }
        output.push_str(&rest[..start]);
        let after = &rest[start + 3..];
        let end = after
            .find("}}")
            .ok_or_else(|| format!("a template is not closed: `{}`", &rest[start..]))?;
        output.push_str(&evaluate(after[..end].trim(), scope)?);
        rest = &after[end + 2..];
    }
    output.push_str(rest);
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Values(Vec<(&'static str, Value)>);

    impl Scope for Values {
        fn lookup(&self, name: &str) -> Option<Value> {
            self.0
                .iter()
                .find(|(known, _)| *known == name)
                .map(|(_, value)| value.clone())
        }
    }

    fn scope() -> Values {
        Values(vec![
            ("BUILD_DIR", Value::String("build".into())),
            ("JOBS", Value::Integer(8)),
            (
                "FILES",
                Value::Array(vec![
                    Value::String("a.c".into()),
                    Value::String("b.c".into()),
                ]),
            ),
            (
                "CONFIG",
                Value::Object(vec![("port".into(), Value::Integer(8080))]),
            ),
        ])
    }

    #[test]
    fn writes_variables_keys_and_functions() {
        let text = "${{ BUILD_DIR }}/app -j${{JOBS}} ${{ FILES }} :${{ CONFIG.port }} ${{ OS() }}";
        assert_eq!(
            render(text, &scope()).unwrap(),
            format!("build/app -j8 a.c b.c :8080 {}", os_name())
        );
    }

    #[test]
    fn escapes_with_a_dollar() {
        assert_eq!(
            render("echo $${{ BUILD_DIR }}", &scope()).unwrap(),
            "echo ${{ BUILD_DIR }}"
        );
        assert_eq!(render("$HOME ${HOME}", &scope()).unwrap(), "$HOME ${HOME}");
    }

    #[test]
    fn refuses_what_it_cannot_write() {
        let errors = [
            ("${{ NOPE }}", "`NOPE` is not set"),
            ("${{ BUILD_DIR", "not closed"),
            ("${{ CONFIG.host }}", "`host`"),
            ("${{ toJson() }}", "not carried out"),
            ("${{ nope() }}", "no function"),
        ];
        for (text, expected) in errors {
            let error = render(text, &scope()).unwrap_err();
            assert!(error.contains(expected), "{text}: {error}");
        }
    }
}
