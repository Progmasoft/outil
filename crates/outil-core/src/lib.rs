// SPDX-FileCopyrightText: 2026 Progmasoft <support@progmasoft.com>
// SPDX-License-Identifier: EPL-2.0

//! Outil: build automation. A project describes its tasks in `Outil.json5`
//! or `Outil.yml`; Outil reads that file and runs the tasks.

#![forbid(unsafe_code)]

pub mod model;
pub mod options;
pub mod run;
pub mod template;
pub mod value;
pub mod yaml;

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use model::File;
use options::Options;
use run::{Context, RunError};

/// The name of the JSON5 file.
pub const JSON5_FILE: &str = "Outil.json5";
/// The name of the YAML file.
pub const YAML_FILE: &str = "Outil.yml";

/// Finds the Outil file of a directory or of the nearest directory above it.
///
/// # Errors
///
/// No file, or both files in one directory.
pub fn find_file(start: &Path) -> Result<PathBuf, String> {
    for dir in start.ancestors() {
        let json5 = dir.join(JSON5_FILE);
        let yaml = dir.join(YAML_FILE);
        match (json5.is_file(), yaml.is_file()) {
            (true, true) => {
                return Err(format!(
                    "{} holds both {JSON5_FILE} and {YAML_FILE}; a directory holds one",
                    dir.display()
                ));
            }
            (true, false) => return Ok(json5),
            (false, true) => return Ok(yaml),
            (false, false) => {}
        }
    }
    Err(format!(
        "no {JSON5_FILE} or {YAML_FILE} in {} or above it",
        start.display()
    ))
}

/// Reads and checks an Outil file.
///
/// # Errors
///
/// The file cannot be read, is not JSON5 or YAML, or is not a valid Outil
/// file.
pub fn load(path: &Path) -> Result<File, String> {
    let text =
        std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let is_yaml = path.file_name().is_some_and(|name| name == YAML_FILE);
    let value = if is_yaml {
        yaml::parse_yaml(&text)
    } else {
        value::parse_json5(&text)
    }
    .map_err(|error| format!("{}: {error}", path.display()))?;
    model::read_file(&value).map_err(|error| format!("{}: {error}", path.display()))
}

/// The tasks `--list` shows: those with a `desc` that are not internal.
#[must_use]
pub fn listing(file: &File) -> String {
    let shown: Vec<_> = file
        .tasks
        .iter()
        .filter(|(_, task)| task.desc.is_some() && !task.internal)
        .collect();
    let width = shown.iter().map(|(name, _)| name.len()).max().unwrap_or(0);
    let mut text = String::new();
    for (name, task) in shown {
        let desc = task.desc.as_deref().unwrap_or_default();
        // Writing to a String cannot fail.
        let _ = writeln!(text, "* {name:width$}  {desc}");
    }
    text
}

/// Runs tasks of a file in order; none runs `default`.
///
/// # Errors
///
/// An internal task named, or the first task that fails.
pub fn run_tasks(
    file: &File,
    path: PathBuf,
    tasks: &[String],
    options: &Options,
    working_dir: PathBuf,
) -> Result<(), RunError> {
    let mut tasks = tasks.to_vec();
    if tasks.is_empty() {
        tasks.push("default".to_owned());
    }
    for name in &tasks {
        let internal = file.task(name).is_some_and(|(_, task)| task.internal);
        if internal {
            return Err(RunError::Invalid(format!(
                "task `{name}` is internal; it cannot be called from the command line"
            )));
        }
    }
    let context = Context {
        file,
        path,
        options,
        working_dir,
    };
    for name in &tasks {
        run::run_task(&context, name, &[], &[])?;
    }
    Ok(())
}
