// SPDX-FileCopyrightText: 2026 Progmasoft <support@progmasoft.com>
// SPDX-License-Identifier: EPL-2.0

//! The `outil` command: reads its command line and runs the tasks of the
//! Outil file through outil-core.

#![forbid(unsafe_code)]

mod cli;

use std::io::Write;

use cli::Action;
use outil_core::run::RunError;

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(main_with(&arguments));
}

/// Runs Outil with the arguments after the program name.
///
/// The result is the exit code; messages go to standard error.
fn main_with(arguments: &[String]) -> i32 {
    let mut stderr = std::io::stderr();
    let invocation = match cli::parse(arguments) {
        Ok(invocation) => invocation,
        Err(error) => {
            let _ = writeln!(stderr, "outil: {error}");
            return 2;
        }
    };
    match invocation.action {
        Action::Help => {
            print!("{}", cli::HELP);
            return 0;
        }
        Action::Version => {
            println!("outil {}", env!("CARGO_PKG_VERSION"));
            return 0;
        }
        _ => {}
    }
    let working_dir = std::env::current_dir().unwrap_or_default();
    let start = invocation
        .dir
        .as_ref()
        .map_or_else(|| working_dir.clone(), |dir| working_dir.join(dir));
    let loaded = outil_core::find_file(&start)
        .and_then(|path| outil_core::load(&path).map(|file| (path, file)));
    let (path, file) = match loaded {
        Ok(found) => found,
        Err(error) => {
            let _ = writeln!(stderr, "outil: {error}");
            return 1;
        }
    };
    match &invocation.action {
        Action::List => {
            print!("{}", outil_core::listing(&file));
            0
        }
        Action::Summary(name) => {
            let Some((_, task)) = file.task(name) else {
                let _ = writeln!(stderr, "outil: no task `{name}`");
                return 1;
            };
            println!(
                "{}",
                task.summary
                    .as_deref()
                    .or(task.desc.as_deref())
                    .unwrap_or_default()
            );
            0
        }
        _ => match outil_core::run_tasks(
            &file,
            path,
            &invocation.tasks,
            &invocation.options,
            working_dir,
        ) {
            Ok(()) => 0,
            Err(error) => {
                let _ = writeln!(stderr, "outil: {error}");
                match error {
                    RunError::Failed { code, .. } => code,
                    RunError::Invalid(_) => 1,
                }
            }
        },
    }
}
