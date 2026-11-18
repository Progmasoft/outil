// SPDX-FileCopyrightText: 2026 Progmasoft <support@progmasoft.com>
// SPDX-License-Identifier: EPL-2.0

//! The model of an Outil file, read from its values and checked.

use crate::value::Value;

/// A variable of `vars` or `env`.
#[derive(Debug, Clone, PartialEq)]
pub enum Variable {
    /// A value: a string, a bool, a number, a list, or a `{ map }`.
    Static(Value),
    /// `{ sh: <command> }`: the output of the command, without its last
    /// newline.
    Shell(String),
    /// `{ ref: NAME }`: the value of another variable.
    Reference(String),
}

/// A call of a task, from `deps` or `cmds`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TaskCall {
    /// The task called.
    pub task: String,
    /// The variables of the call.
    pub vars: Vec<(String, Variable)>,
    /// Whether its commands are not printed.
    pub silent: bool,
}

/// An item of `cmds`.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// A command run by the shell.
    Shell {
        /// The command.
        cmd: String,
        /// Whether it is not printed before it runs.
        silent: bool,
        /// Whether its failure does not stop the task.
        ignore_error: bool,
    },
    /// A call of another task.
    Call(TaskCall),
}

/// An item of `preconditions`.
#[derive(Debug, Clone, PartialEq)]
pub struct Precondition {
    /// The command that must succeed.
    pub sh: String,
    /// The message when it fails.
    pub msg: Option<String>,
}

/// A variable `requires` asks for.
#[derive(Debug, Clone, PartialEq)]
pub struct Requirement {
    /// Its name.
    pub name: String,
    /// The values it may take; any when empty.
    pub allowed: Vec<String>,
}

/// A task.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Task {
    /// What it runs, in order.
    pub cmds: Vec<Command>,
    /// The tasks that run before it, at the same time as each other.
    pub deps: Vec<TaskCall>,
    /// The line `outil --list` shows.
    pub desc: Option<String>,
    /// The text `outil --summary` shows.
    pub summary: Option<String>,
    /// The name the output uses in place of the task's.
    pub label: Option<String>,
    /// Other names of the task.
    pub aliases: Vec<String>,
    /// Its variables.
    pub vars: Vec<(String, Variable)>,
    /// The environment variables of its commands.
    pub env: Vec<(String, Variable)>,
    /// The directory its commands run in.
    pub dir: Option<String>,
    /// Whether its commands are not printed.
    pub silent: bool,
    /// Whether it cannot be called from the command line.
    pub internal: bool,
    /// Whether a failing command does not stop it.
    pub ignore_error: bool,
    /// Checks that must pass before it runs.
    pub preconditions: Vec<Precondition>,
    /// The variables that must be set.
    pub requires: Vec<Requirement>,
}

/// An Outil file.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct File {
    /// The tasks, in the order written.
    pub tasks: Vec<(String, Task)>,
    /// The variables of every task.
    pub vars: Vec<(String, Variable)>,
    /// The environment variables of every command.
    pub env: Vec<(String, Variable)>,
    /// Whether commands are not printed.
    pub silent: bool,
}

impl File {
    /// The task with a name or an alias.
    #[must_use]
    pub fn task(&self, name: &str) -> Option<(&str, &Task)> {
        self.tasks
            .iter()
            .find(|(known, task)| known == name || task.aliases.iter().any(|alias| alias == name))
            .map(|(known, task)| (known.as_str(), task))
    }
}

/// The keys of the format that this version of Outil does not carry out yet.
/// A file that uses one is refused, so that nothing is silently skipped.
const NOT_YET: &[&str] = &[
    "dotenv",
    "includes",
    "output",
    "method",
    "run",
    "interval",
    "set",
    "shopt",
    "srcs",
    "gens",
    "status",
    "prompt",
    "platforms",
    "interactive",
    "prefix",
    "watch",
    "defer",
    "for",
];

type Checked<T> = Result<T, String>;

fn unknown(place: &str, key: &str) -> String {
    if NOT_YET.contains(&key) {
        format!(
            "{place}: `{key}` is part of the format but not carried out by Outil {} yet",
            env!("CARGO_PKG_VERSION")
        )
    } else {
        format!("{place}: unknown key `{key}`")
    }
}

fn entries<'a>(place: &str, value: &'a Value) -> Checked<&'a [(String, Value)]> {
    match value {
        Value::Object(entries) => Ok(entries),
        other => Err(format!("{place}: expected a map, found {}", other.kind())),
    }
}

fn string(place: &str, value: &Value) -> Checked<String> {
    match value {
        Value::String(text) => Ok(text.clone()),
        other => Err(format!(
            "{place}: expected a string, found {}",
            other.kind()
        )),
    }
}

fn boolean(place: &str, value: &Value) -> Checked<bool> {
    match value {
        Value::Bool(flag) => Ok(*flag),
        other => Err(format!(
            "{place}: expected true or false, found {}",
            other.kind()
        )),
    }
}

fn strings(place: &str, value: &Value) -> Checked<Vec<String>> {
    match value {
        Value::Array(items) => items
            .iter()
            .enumerate()
            .map(|(index, item)| string(&format!("{place}[{index}]"), item))
            .collect(),
        other => Err(format!(
            "{place}: expected a list of strings, found {}",
            other.kind()
        )),
    }
}

/// Whether a name is a task name: letters, digits, `_`, `-` and `.`.
#[must_use]
pub fn is_task_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "_-.".contains(character))
}

/// Whether a name is a variable name: a letter or `_`, then letters, digits
/// and `_`.
#[must_use]
pub fn is_variable_name(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn variable(place: &str, value: &Value) -> Checked<Variable> {
    let Value::Object(entries) = value else {
        return Ok(Variable::Static(value.clone()));
    };
    match entries.as_slice() {
        [(key, Value::Object(map))] if key == "map" => {
            Ok(Variable::Static(Value::Object(map.clone())))
        }
        [(key, command)] if key == "sh" => Ok(Variable::Shell(string(place, command)?)),
        [(key, name)] if key == "ref" => Ok(Variable::Reference(string(place, name)?)),
        _ => Err(format!(
            "{place}: a variable that is a map is {{ map }}, {{ sh }} or {{ ref }}"
        )),
    }
}

fn variables(place: &str, value: &Value) -> Checked<Vec<(String, Variable)>> {
    entries(place, value)?
        .iter()
        .map(|(name, item)| {
            if !is_variable_name(name) {
                return Err(format!("{place}: `{name}` is not a variable name"));
            }
            Ok((name.clone(), variable(&format!("{place}.{name}"), item)?))
        })
        .collect()
}

fn call(place: &str, value: &Value) -> Checked<TaskCall> {
    if let Value::String(task) = value {
        return Ok(TaskCall {
            task: task.clone(),
            ..TaskCall::default()
        });
    }
    let mut result = TaskCall::default();
    for (key, item) in entries(place, value)? {
        let inner = format!("{place}.{key}");
        match key.as_str() {
            "task" => result.task = string(&inner, item)?,
            "vars" => result.vars = variables(&inner, item)?,
            "silent" => result.silent = boolean(&inner, item)?,
            _ => return Err(unknown(place, key)),
        }
    }
    if result.task.is_empty() {
        return Err(format!("{place}: a call names no task"));
    }
    Ok(result)
}

fn command(place: &str, value: &Value) -> Checked<Command> {
    if let Value::String(cmd) = value {
        return Ok(Command::Shell {
            cmd: cmd.clone(),
            silent: false,
            ignore_error: false,
        });
    }
    let fields = entries(place, value)?;
    if fields.iter().any(|(key, _)| key == "task") {
        return Ok(Command::Call(call(place, value)?));
    }
    let (mut cmd, mut silent, mut ignore_error) = (None, false, false);
    for (key, item) in fields {
        let inner = format!("{place}.{key}");
        match key.as_str() {
            "cmd" => cmd = Some(string(&inner, item)?),
            "silent" => silent = boolean(&inner, item)?,
            "ignore_error" => ignore_error = boolean(&inner, item)?,
            _ => return Err(unknown(place, key)),
        }
    }
    let cmd = cmd.ok_or_else(|| format!("{place}: a command has no `cmd` or `task`"))?;
    Ok(Command::Shell {
        cmd,
        silent,
        ignore_error,
    })
}

fn list<T>(place: &str, value: &Value, read: fn(&str, &Value) -> Checked<T>) -> Checked<Vec<T>> {
    match value {
        Value::Array(items) => items
            .iter()
            .enumerate()
            .map(|(index, item)| read(&format!("{place}[{index}]"), item))
            .collect(),
        other => Err(format!("{place}: expected a list, found {}", other.kind())),
    }
}

fn precondition(place: &str, value: &Value) -> Checked<Precondition> {
    if let Value::String(sh) = value {
        return Ok(Precondition {
            sh: sh.clone(),
            msg: None,
        });
    }
    let (mut sh, mut msg) = (None, None);
    for (key, item) in entries(place, value)? {
        match key.as_str() {
            "sh" => sh = Some(string(place, item)?),
            "msg" => msg = Some(string(place, item)?),
            _ => return Err(unknown(place, key)),
        }
    }
    let sh = sh.ok_or_else(|| format!("{place}: a precondition has no `sh`"))?;
    Ok(Precondition { sh, msg })
}

fn requirement(place: &str, value: &Value) -> Checked<Requirement> {
    if let Value::String(name) = value {
        return Ok(Requirement {
            name: name.clone(),
            allowed: Vec::new(),
        });
    }
    let (mut name, mut allowed) = (None, Vec::new());
    for (key, item) in entries(place, value)? {
        match key.as_str() {
            "name" => name = Some(string(place, item)?),
            "enum" => allowed = strings(place, item)?,
            _ => return Err(unknown(place, key)),
        }
    }
    let name = name.ok_or_else(|| format!("{place}: a requirement has no `name`"))?;
    Ok(Requirement { name, allowed })
}

fn requires(place: &str, value: &Value) -> Checked<Vec<Requirement>> {
    let fields = entries(place, value)?;
    match fields {
        [(key, vars)] if key == "vars" => list(&format!("{place}.vars"), vars, requirement),
        _ => Err(format!("{place}: requires holds only `vars`")),
    }
}

fn task_field(task: &mut Task, place: &str, key: &str, item: &Value) -> Checked<()> {
    let inner = format!("{place}.{key}");
    match key {
        "cmds" => task.cmds = list(&inner, item, command)?,
        "cmd" => task.cmds = vec![command(&inner, item)?],
        "deps" => task.deps = list(&inner, item, call)?,
        "desc" => task.desc = Some(string(&inner, item)?),
        "summary" => task.summary = Some(string(&inner, item)?),
        "label" => task.label = Some(string(&inner, item)?),
        "aliases" => task.aliases = strings(&inner, item)?,
        "vars" => task.vars = variables(&inner, item)?,
        "env" => task.env = variables(&inner, item)?,
        "dir" => task.dir = Some(string(&inner, item)?),
        "silent" => task.silent = boolean(&inner, item)?,
        "internal" => task.internal = boolean(&inner, item)?,
        "ignore_error" => task.ignore_error = boolean(&inner, item)?,
        "preconditions" => task.preconditions = list(&inner, item, precondition)?,
        "requires" => task.requires = requires(&inner, item)?,
        _ => return Err(unknown(place, key)),
    }
    Ok(())
}

fn task(place: &str, value: &Value) -> Checked<Task> {
    match value {
        // A task that is only commands.
        Value::String(_) => Ok(Task {
            cmds: vec![command(place, value)?],
            ..Task::default()
        }),
        Value::Array(_) => Ok(Task {
            cmds: list(place, value, command)?,
            ..Task::default()
        }),
        _ => {
            let mut result = Task::default();
            for (key, item) in entries(place, value)? {
                task_field(&mut result, place, key, item)?;
            }
            Ok(result)
        }
    }
}

/// Reads the model of a file from its values.
///
/// # Errors
///
/// A message that names the place of the first invalid value.
pub fn read_file(value: &Value) -> Checked<File> {
    let mut file = File::default();
    for (key, item) in entries("the file", value)? {
        match key.as_str() {
            "$schema" => {
                string("$schema", item)?;
            }
            "vars" => file.vars = variables("vars", item)?,
            "env" => file.env = variables("env", item)?,
            "silent" => file.silent = boolean("silent", item)?,
            "tasks" => {
                for (name, body) in entries("tasks", item)? {
                    if !is_task_name(name) {
                        return Err(format!("tasks: `{name}` is not a task name"));
                    }
                    file.tasks
                        .push((name.clone(), task(&format!("tasks.{name}"), body)?));
                }
            }
            _ => return Err(unknown("the file", key)),
        }
    }
    check_names(&file)?;
    Ok(file)
}

/// Checks that every name and alias is one task, and every call names one.
fn check_names(file: &File) -> Checked<()> {
    let mut names: Vec<&str> = Vec::new();
    for (name, task) in &file.tasks {
        for alias in std::iter::once(name).chain(&task.aliases) {
            if names.contains(&alias.as_str()) {
                return Err(format!("tasks: `{alias}` names two tasks"));
            }
            names.push(alias);
        }
    }
    for (name, task) in &file.tasks {
        let calls = task
            .deps
            .iter()
            .chain(task.cmds.iter().filter_map(|command| match command {
                Command::Call(call) => Some(call),
                Command::Shell { .. } => None,
            }));
        for called in calls {
            if !called.task.contains("${{") && file.task(&called.task).is_none() {
                return Err(format!("tasks.{name}: no task `{}`", called.task));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::parse_json5;

    fn read(text: &str) -> Checked<File> {
        read_file(&parse_json5(text).unwrap())
    }

    #[test]
    fn reads_tasks_in_every_short_form() {
        let file = read(
            r#"{
              $schema: "https://schemastore.progmasoft.com/json5/outil-Outil.json",
              vars: { BUILD_DIR: "build", COMMIT: { sh: "git rev-parse HEAD" }, ALL: { ref: "BUILD_DIR" } },
              tasks: {
                default: { deps: ["build"] },
                build: { desc: "Compile", cmds: ["cc main.c", { cmd: "strip a.out", silent: true }, { task: "clean" }] },
                clean: "rm -rf build",
                both: ["a", "b"],
              },
            }"#,
        )
        .unwrap();
        assert_eq!(file.tasks.len(), 4);
        assert_eq!(file.vars[1].1, Variable::Shell("git rev-parse HEAD".into()));
        assert_eq!(file.vars[2].1, Variable::Reference("BUILD_DIR".into()));
        assert_eq!(file.task("both").unwrap().1.cmds.len(), 2);
        assert!(matches!(
            file.task("build").unwrap().1.cmds[2],
            Command::Call(_)
        ));
    }

    #[test]
    fn refuses_what_it_does_not_know() {
        assert!(
            read("{ colors: true }")
                .unwrap_err()
                .contains("unknown key `colors`")
        );
        assert!(
            read("{ tasks: { a: { srcs: [] } } }")
                .unwrap_err()
                .contains("not carried out")
        );
        assert!(
            read("{ tasks: { 'a b': 'x' } }")
                .unwrap_err()
                .contains("not a task name")
        );
        assert!(
            read("{ tasks: { a: { deps: ['b'] } } }")
                .unwrap_err()
                .contains("no task `b`")
        );
        assert!(
            read("{ vars: { X: { other: 1 } } }")
                .unwrap_err()
                .contains("{ map }")
        );
        assert!(
            read("{ tasks: { a: { aliases: ['b'] }, b: 'x' } }")
                .unwrap_err()
                .contains("names two tasks")
        );
    }

    #[test]
    fn finds_tasks_by_alias() {
        let file = read("{ tasks: { build: { aliases: ['b'], cmd: 'make' } } }").unwrap();
        assert_eq!(file.task("b").unwrap().0, "build");
    }
}
