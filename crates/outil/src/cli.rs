// SPDX-FileCopyrightText: 2026 Progmasoft <support@progmasoft.com>
// SPDX-License-Identifier: EPL-2.0

//! The command line, read by hand into a typed value.

use std::path::PathBuf;

use outil_core::model::is_variable_name;
use outil_core::options::{Flag, Options};

/// What Outil is asked to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Run tasks; none runs `default`.
    Run,
    /// `--list`, `-l`: list the tasks that have a `desc`.
    List,
    /// `--summary <task>`: show the summary of a task.
    Summary(String),
    /// `--help`, `-h`.
    Help,
    /// `--version`.
    Version,
}

/// A command line, read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    /// What to do.
    pub action: Action,
    /// The tasks to run, in order.
    pub tasks: Vec<String>,
    /// `--dir`, `-d`: where to look for the Outil file.
    pub dir: Option<PathBuf>,
    /// The variables, arguments and flags every task sees.
    pub options: Options,
}

/// The text of `outil --help`.
pub const HELP: &str = "\
Usage: outil [options] [task...] [NAME=value...] [-- arguments...]

Runs the tasks of Outil.json5, or the task `default` when none is named.

Options:
  -l, --list             List the tasks that have a description
      --summary <task>   Show the summary of a task
  -d, --dir <dir>        Look for the Outil file from <dir> instead of here
  -f, --force            Run the tasks even when they are up to date
  -s, --silent           Do not print the commands before they run
  -v, --verbose          Print what Outil does
      --offline          Use no network
  -h, --help             Show this text
      --version          Show the version of Outil

NAME=value sets a variable for every task; the arguments after -- are
CLI_ARGS.
";

/// Reads the arguments after the program name.
///
/// # Errors
///
/// An unknown option, an option without its value, or two actions.
pub fn parse(arguments: &[String]) -> Result<Invocation, String> {
    let mut invocation = Invocation {
        action: Action::Run,
        tasks: Vec::new(),
        dir: None,
        options: Options::default(),
    };
    let mut index = 0;
    while index < arguments.len() {
        let argument = &arguments[index];
        index += 1;
        if argument == "--" {
            invocation.options.args = arguments[index..].to_vec();
            break;
        }
        if argument.starts_with('-') && argument.len() > 1 {
            index = option(&mut invocation, argument, arguments, index)?;
            continue;
        }
        match argument.split_once('=') {
            Some((name, value)) if is_variable_name(name) => {
                invocation
                    .options
                    .vars
                    .push((name.to_owned(), value.to_owned()));
            }
            _ => invocation.tasks.push(argument.clone()),
        }
    }
    Ok(invocation)
}

/// Sets the action, refusing a second one.
fn set_action(invocation: &mut Invocation, action: Action) -> Result<(), String> {
    if invocation.action != Action::Run {
        return Err("give only one of --list, --summary, --help and --version".to_owned());
    }
    invocation.action = action;
    Ok(())
}

fn set_flag(invocation: &mut Invocation, flag: Flag) {
    invocation.options.set(flag);
}

/// Reads one option; returns the index of the next argument.
fn option(
    invocation: &mut Invocation,
    argument: &str,
    arguments: &[String],
    index: usize,
) -> Result<usize, String> {
    let value = |name: &str| {
        arguments
            .get(index)
            .cloned()
            .ok_or_else(|| format!("{name} needs a value"))
    };
    match argument {
        "-l" | "--list" => set_action(invocation, Action::List)?,
        "--summary" => {
            let task = value("--summary")?;
            set_action(invocation, Action::Summary(task))?;
            return Ok(index + 1);
        }
        "-d" | "--dir" => {
            invocation.dir = Some(PathBuf::from(value(argument)?));
            return Ok(index + 1);
        }
        "-f" | "--force" => set_flag(invocation, Flag::Force),
        "-s" | "--silent" => set_flag(invocation, Flag::Silent),
        "-v" | "--verbose" => set_flag(invocation, Flag::Verbose),
        "--offline" => set_flag(invocation, Flag::Offline),
        "-h" | "--help" => set_action(invocation, Action::Help)?,
        "--version" => set_action(invocation, Action::Version)?,
        _ => return Err(format!("no option {argument}; outil --help lists them")),
    }
    Ok(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(arguments: &[&str]) -> Result<Invocation, String> {
        parse(
            &arguments
                .iter()
                .map(|&argument| argument.to_owned())
                .collect::<Vec<_>>(),
        )
    }

    #[test]
    fn reads_tasks_variables_and_arguments() {
        let invocation = read(&["build", "MODE=release", "-s", "test", "--", "-x", "y=1"]).unwrap();
        assert_eq!(invocation.tasks, ["build", "test"]);
        assert_eq!(
            invocation.options.vars,
            [("MODE".to_owned(), "release".to_owned())]
        );
        assert_eq!(invocation.options.args, ["-x", "y=1"]);
        assert!(invocation.options.has(Flag::Silent));
        assert_eq!(invocation.action, Action::Run);
    }

    #[test]
    fn reads_actions_and_values() {
        assert_eq!(read(&["--list"]).unwrap().action, Action::List);
        assert_eq!(
            read(&["--summary", "build"]).unwrap().action,
            Action::Summary("build".into())
        );
        assert_eq!(
            read(&["-d", "sub", "x"]).unwrap().dir,
            Some(PathBuf::from("sub"))
        );
        assert!(read(&["--summary"]).unwrap_err().contains("needs a value"));
        assert!(
            read(&["--list", "--help"])
                .unwrap_err()
                .contains("only one")
        );
        assert!(read(&["--frob"]).unwrap_err().contains("no option --frob"));
    }

    #[test]
    fn a_word_with_an_invalid_name_before_equals_is_a_task() {
        assert_eq!(read(&["a-b=c"]).unwrap().tasks, ["a-b=c"]);
    }
}
