// SPDX-FileCopyrightText: 2026 Progmasoft <support@progmasoft.com>
// SPDX-License-Identifier: EPL-2.0

//! Running tasks: their dependencies at the same time, then their commands
//! in order, each through the shell: `cmd.exe` on Windows, `sh` elsewhere.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command as Process, Stdio};

use crate::model::{Command, File, Task, TaskCall, Variable};
use crate::options::{Flag, Options};
use crate::template::{Scope, render};
use crate::value::Value;

/// Why a run failed.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RunError {
    /// The file or a call is not valid where it is used.
    #[error("{0}")]
    Invalid(String),
    /// A command failed; Outil exits with its code.
    #[error("task `{task}`: `{command}` exited with {code}")]
    Failed {
        /// The task.
        task: String,
        /// The command, as run.
        command: String,
        /// Its exit code.
        code: i32,
    },
}

/// What every task of one call of Outil shares.
pub struct Context<'a> {
    /// The file.
    pub file: &'a File,
    /// The path of the file.
    pub path: PathBuf,
    /// The options of the call.
    pub options: &'a Options,
    /// The directory Outil was started in.
    pub working_dir: PathBuf,
}

impl Context<'_> {
    fn file_dir(&self) -> PathBuf {
        self.path
            .parent()
            .map_or_else(PathBuf::new, Path::to_path_buf)
    }
}

/// The variables a task sees, from the strongest: the special variables, the
/// command line, the calls that lead to it, the task's, the file's, and the
/// environment.
struct TaskScope<'a> {
    context: &'a Context<'a>,
    special: Vec<(String, Value)>,
    layers: Vec<&'a [(String, Variable)]>,
}

impl TaskScope<'_> {
    fn find(&self, name: &str) -> Option<&Variable> {
        self.layers
            .iter()
            .find_map(|layer| layer.iter().find(|(known, _)| known == name))
            .map(|(_, variable)| variable)
    }

    fn resolve(&self, name: &str, depth: usize) -> Option<Value> {
        if let Some((_, value)) = self.special.iter().find(|(known, _)| known == name) {
            return Some(value.clone());
        }
        if let Some((_, value)) = self
            .context
            .options
            .vars
            .iter()
            .find(|(known, _)| known == name)
        {
            return Some(Value::String(value.clone()));
        }
        match self.find(name) {
            // A chain of references longer than this is a loop.
            Some(Variable::Reference(target)) if depth < 64 => self.resolve(target, depth + 1),
            Some(Variable::Reference(_)) => None,
            Some(Variable::Static(value)) => {
                Some(render_value(value, self).unwrap_or_else(|_| value.clone()))
            }
            Some(Variable::Shell(command)) => {
                let command = render(command, self).ok()?;
                capture(&command, &self.context.file_dir()).ok()
            }
            None => std::env::var(name).ok().map(Value::String),
        }
    }
}

impl Scope for TaskScope<'_> {
    fn lookup(&self, name: &str) -> Option<Value> {
        self.resolve(name, 0)
    }
}

/// Writes the templates of every string inside a value.
fn render_value(value: &Value, scope: &dyn Scope) -> Result<Value, String> {
    Ok(match value {
        Value::String(text) => Value::String(render(text, scope)?),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| render_value(item, scope))
                .collect::<Result<_, _>>()?,
        ),
        Value::Object(entries) => Value::Object(
            entries
                .iter()
                .map(|(key, item)| Ok((key.clone(), render_value(item, scope)?)))
                .collect::<Result<_, String>>()?,
        ),
        other => other.clone(),
    })
}

/// The name of the shell, for messages.
const SHELL: &str = if cfg!(windows) { "cmd.exe" } else { "sh" };

/// The shell of the commands: `cmd.exe /C` on Windows, `sh -c` elsewhere.
#[cfg(windows)]
fn shell(command: &str) -> Process {
    use std::os::windows::process::CommandExt;

    let mut process = Process::new("cmd.exe");
    // cmd.exe reads its command line itself; the command goes to it as
    // written, without the quoting of other programs' arguments.
    process
        .arg("/D")
        .arg("/S")
        .arg("/C")
        .raw_arg(format!("\"{command}\""));
    process
}

/// The shell of the commands: `cmd.exe /C` on Windows, `sh -c` elsewhere.
#[cfg(not(windows))]
fn shell(command: &str) -> Process {
    let mut process = Process::new("sh");
    process.arg("-c").arg(command);
    process
}

/// The output of a command, without its last newline.
fn capture(command: &str, dir: &Path) -> Result<Value, String> {
    let output = shell(command)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .output()
        .map_err(|error| format!("cannot start {SHELL}: {error}"))?;
    if !output.status.success() {
        return Err(format!("`{command}` failed"));
    }
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    if text.ends_with('\n') {
        text.pop();
        if text.ends_with('\r') {
            text.pop();
        }
    }
    Ok(Value::String(text))
}

fn bool_value(flag: bool) -> Value {
    Value::Bool(flag)
}

fn path_value(path: &Path) -> Value {
    Value::String(path.display().to_string())
}

/// Runs a task called by name, with the variables of its calls.
///
/// # Errors
///
/// A task that does not exist or is internal, a loop of dependencies, a
/// variable that is required but not set, a precondition or a command that
/// fails.
pub fn run_task(
    context: &Context<'_>,
    name: &str,
    calls: &[&[(String, Variable)]],
    stack: &[String],
) -> Result<(), RunError> {
    let (name, task) = context
        .file
        .task(name)
        .ok_or_else(|| RunError::Invalid(format!("no task `{name}`")))?;
    if stack.iter().any(|known| known == name) {
        let mut chain = stack.to_vec();
        chain.push(name.to_owned());
        return Err(RunError::Invalid(format!(
            "a loop of tasks: {}",
            chain.join(" -> ")
        )));
    }
    let mut stack = stack.to_vec();
    stack.push(name.to_owned());

    let scope = task_scope(context, name, task, calls);
    run_deps(context, task, &scope, calls, &stack)?;
    check_requires(name, task, &scope)?;
    let dir = task_dir(context, task, &scope)?;
    let env = environment(context, task, &scope)?;
    check_preconditions(name, task, &scope, &dir, &env)?;
    let label = task
        .label
        .as_deref()
        .map_or_else(|| Ok(name.to_owned()), |label| render(label, &scope))
        .map_err(RunError::Invalid)?;
    let place = Place {
        name,
        label: &label,
        dir: &dir,
        env: &env,
    };
    for command in &task.cmds {
        match command {
            Command::Shell {
                cmd,
                silent,
                ignore_error,
            } => {
                run_shell(
                    context,
                    task,
                    &scope,
                    &place,
                    cmd,
                    *silent || task.silent,
                    *ignore_error,
                )?;
            }
            Command::Call(call) => run_call(context, call, &scope, calls, &stack)?,
        }
    }
    Ok(())
}

/// Where the commands of a task run.
struct Place<'a> {
    name: &'a str,
    label: &'a str,
    dir: &'a Path,
    env: &'a [(String, String)],
}

/// Runs one command of a task, and stops the task when it fails.
fn run_shell(
    context: &Context<'_>,
    task: &Task,
    scope: &TaskScope<'_>,
    place: &Place<'_>,
    cmd: &str,
    silent: bool,
    ignore_error: bool,
) -> Result<(), RunError> {
    let cmd = render(cmd, scope)
        .map_err(|error| RunError::Invalid(format!("task `{}`: {error}", place.name)))?;
    let quiet = silent || context.file.silent || context.options.has(Flag::Silent);
    let code = execute(place.label, &cmd, quiet, place.dir, place.env)?;
    if code != 0 && !ignore_error && !task.ignore_error {
        return Err(RunError::Failed {
            task: place.label.to_owned(),
            command: cmd,
            code,
        });
    }
    Ok(())
}

fn task_scope<'a>(
    context: &'a Context<'a>,
    name: &str,
    task: &'a Task,
    calls: &[&'a [(String, Variable)]],
) -> TaskScope<'a> {
    let options = context.options;
    let special = vec![
        ("TASK".to_owned(), Value::String(name.to_owned())),
        ("CLI_ARGS".to_owned(), Value::String(options.args.join(" "))),
        ("CLI_FORCE".to_owned(), bool_value(options.has(Flag::Force))),
        (
            "CLI_SILENT".to_owned(),
            bool_value(options.has(Flag::Silent)),
        ),
        (
            "CLI_VERBOSE".to_owned(),
            bool_value(options.has(Flag::Verbose)),
        ),
        (
            "CLI_OFFLINE".to_owned(),
            bool_value(options.has(Flag::Offline)),
        ),
        ("ROOT_FILE".to_owned(), path_value(&context.path)),
        ("ROOT_DIR".to_owned(), path_value(&context.file_dir())),
        ("FILE".to_owned(), path_value(&context.path)),
        ("FILE_DIR".to_owned(), path_value(&context.file_dir())),
        (
            "USER_WORKING_DIR".to_owned(),
            path_value(&context.working_dir),
        ),
        (
            "OUTIL_VERSION".to_owned(),
            Value::String(env!("CARGO_PKG_VERSION").to_owned()),
        ),
    ];
    let mut layers: Vec<&[(String, Variable)]> = calls.iter().rev().copied().collect();
    layers.push(&task.vars);
    layers.push(&context.file.vars);
    TaskScope {
        context,
        special,
        layers,
    }
}

fn run_deps(
    context: &Context<'_>,
    task: &Task,
    scope: &TaskScope<'_>,
    calls: &[&[(String, Variable)]],
    stack: &[String],
) -> Result<(), RunError> {
    // Dependencies start at the same time; the first error is reported.
    std::thread::scope(|threads| {
        let handles: Vec<_> = task
            .deps
            .iter()
            .map(|dep| threads.spawn(move || run_call(context, dep, scope, calls, stack)))
            .collect();
        let mut first = Ok(());
        for handle in handles {
            let result = handle
                .join()
                .unwrap_or_else(|_| Err(RunError::Invalid("a dependency stopped".to_owned())));
            if first.is_ok() {
                first = result;
            }
        }
        first
    })
}

fn run_call(
    context: &Context<'_>,
    call: &TaskCall,
    scope: &TaskScope<'_>,
    calls: &[&[(String, Variable)]],
    stack: &[String],
) -> Result<(), RunError> {
    let called = render(&call.task, scope).map_err(RunError::Invalid)?;
    // The variables of the call are written in the scope of the caller.
    let vars: Vec<(String, Variable)> = call
        .vars
        .iter()
        .map(|(key, variable)| match variable {
            Variable::Static(value) => {
                render_value(value, scope).map(|value| (key.clone(), Variable::Static(value)))
            }
            other => Ok((key.clone(), other.clone())),
        })
        .collect::<Result<_, _>>()
        .map_err(RunError::Invalid)?;
    let mut chain = calls.to_vec();
    chain.push(&vars);
    run_task(context, &called, &chain, stack)
}

fn check_requires(name: &str, task: &Task, scope: &TaskScope<'_>) -> Result<(), RunError> {
    for requirement in &task.requires {
        let value = scope.lookup(&requirement.name).ok_or_else(|| {
            RunError::Invalid(format!(
                "task `{name}` needs the variable {}",
                requirement.name
            ))
        })?;
        let text = value.to_text();
        if !requirement.allowed.is_empty() && !requirement.allowed.contains(&text) {
            return Err(RunError::Invalid(format!(
                "task `{name}`: {} is `{text}`, not one of {}",
                requirement.name,
                requirement.allowed.join(", ")
            )));
        }
    }
    Ok(())
}

fn task_dir(
    context: &Context<'_>,
    task: &Task,
    scope: &TaskScope<'_>,
) -> Result<PathBuf, RunError> {
    let base = context.file_dir();
    match &task.dir {
        Some(dir) => Ok(base.join(render(dir, scope).map_err(RunError::Invalid)?)),
        None => Ok(base),
    }
}

fn environment(
    context: &Context<'_>,
    task: &Task,
    scope: &TaskScope<'_>,
) -> Result<Vec<(String, String)>, RunError> {
    let mut env = Vec::new();
    for (name, variable) in context.file.env.iter().chain(&task.env) {
        let value = match variable {
            Variable::Static(value) => render_value(value, scope).map_err(RunError::Invalid)?,
            Variable::Reference(target) => scope
                .lookup(target)
                .ok_or_else(|| RunError::Invalid(format!("env {name}: `{target}` is not set")))?,
            Variable::Shell(command) => {
                let command = render(command, scope).map_err(RunError::Invalid)?;
                capture(&command, &context.file_dir()).map_err(RunError::Invalid)?
            }
        };
        env.push((name.clone(), value.to_text()));
    }
    Ok(env)
}

fn check_preconditions(
    name: &str,
    task: &Task,
    scope: &TaskScope<'_>,
    dir: &Path,
    env: &[(String, String)],
) -> Result<(), RunError> {
    for precondition in &task.preconditions {
        let command = render(&precondition.sh, scope).map_err(RunError::Invalid)?;
        let status = shell(&command)
            .current_dir(dir)
            .envs(env.iter().map(|(key, value)| (key, value)))
            .stdout(Stdio::null())
            .status()
            .map_err(|error| RunError::Invalid(format!("cannot start {SHELL}: {error}")))?;
        if !status.success() {
            let message = match &precondition.msg {
                Some(msg) => render(msg, scope).map_err(RunError::Invalid)?,
                None => format!("`{command}` failed"),
            };
            return Err(RunError::Invalid(format!("task `{name}`: {message}")));
        }
    }
    Ok(())
}

/// Runs a command and returns its exit code.
fn execute(
    label: &str,
    command: &str,
    quiet: bool,
    dir: &Path,
    env: &[(String, String)],
) -> Result<i32, RunError> {
    if !quiet {
        let mut stderr = std::io::stderr().lock();
        // A closed standard error does not stop the task.
        let _ = writeln!(stderr, "[{label}] {command}");
    }
    let status = shell(command)
        .current_dir(dir)
        .envs(env.iter().map(|(key, value)| (key, value)))
        .status()
        .map_err(|error| {
            RunError::Invalid(format!(
                "cannot start {SHELL} in {}: {error}",
                dir.display()
            ))
        })?;
    // A command stopped by a signal has no code; it counts as 1.
    Ok(status.code().unwrap_or(1))
}
