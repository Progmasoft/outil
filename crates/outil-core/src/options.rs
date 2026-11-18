// SPDX-FileCopyrightText: 2026 Progmasoft <support@progmasoft.com>
// SPDX-License-Identifier: EPL-2.0

//! What a call of Outil passes to every task: its variables, its arguments
//! and its flags.

/// A flag of a call, which `CLI_<NAME>` reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flag {
    /// Run the tasks even when they are up to date.
    Force,
    /// Do not print the commands.
    Silent,
    /// Print what Outil does.
    Verbose,
    /// Use no network.
    Offline,
}

/// The options of a call.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Options {
    /// `NAME=value` variables, the strongest of all.
    pub vars: Vec<(String, String)>,
    /// The arguments after `--`, in `CLI_ARGS`.
    pub args: Vec<String>,
    /// The flags given, each once.
    pub flags: Vec<Flag>,
}

impl Options {
    /// Whether a flag was given.
    #[must_use]
    pub fn has(&self, flag: Flag) -> bool {
        self.flags.contains(&flag)
    }

    /// Adds a flag once.
    pub fn set(&mut self, flag: Flag) {
        if !self.has(flag) {
            self.flags.push(flag);
        }
    }
}
