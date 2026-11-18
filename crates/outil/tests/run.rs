// SPDX-FileCopyrightText: 2026 Progmasoft <support@progmasoft.com>
// SPDX-License-Identifier: EPL-2.0

//! The `outil` command, run on Outil files in directories of their own.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A directory of one test, removed when the test ends.
struct Project {
    dir: PathBuf,
}

impl Project {
    fn new(json5: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "outil-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Outil.json5"), json5).unwrap();
        Project { dir }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    fn run(&self, arguments: &[&str]) -> Output {
        run_in(&self.dir, arguments)
    }
}

/// Runs `outil` in a directory.
fn run_in(dir: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_outil"))
        .args(arguments)
        .current_dir(dir)
        .env_remove("OUTIL_TEST_FROM_ENV")
        .output()
        .unwrap()
}

impl Drop for Project {
    fn drop(&mut self) {
        // Only the directory this test made.
        if self.dir.starts_with(std::env::temp_dir()) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn runs_default_after_its_dependencies() {
    let project = Project::new(
        r#"{
          vars: { GREETING: "hello" },
          tasks: {
            default: { deps: ["a", "b"], cmds: ["echo ${{ GREETING }} from ${{ TASK }}"] },
            a: "echo a",
            b: "echo b",
          },
        }"#,
    );
    let output = project.run(&[]);
    assert!(output.status.success(), "{}", stderr(&output));
    let lines: Vec<String> = stdout(&output).lines().map(str::to_owned).collect();
    assert_eq!(lines.len(), 3);
    assert!(lines[..2].contains(&"a".to_owned()) && lines[..2].contains(&"b".to_owned()));
    assert_eq!(lines[2], "hello from default");
    assert!(stderr(&output).contains("[default] echo hello from default"));
}

#[test]
fn variables_follow_their_precedence() {
    let project = Project::new(
        r#"{
          vars: { MODE: "file", LEVEL: "file", FROM_SH: { sh: "printf 'from sh\n'" }, ALIAS: { ref: "MODE" } },
          tasks: {
            show: {
              vars: { LEVEL: "task" },
              cmds: ["echo ${{ MODE }} ${{ LEVEL }} ${{ FROM_SH }} ${{ ALIAS }}"],
            },
            caller: { cmds: [{ task: "show", vars: { LEVEL: "call" } }] },
          },
        }"#,
    );
    assert_eq!(stdout(&project.run(&["show"])), "file task from sh file\n");
    assert_eq!(
        stdout(&project.run(&["caller"])),
        "file call from sh file\n"
    );
    assert_eq!(
        stdout(&project.run(&["caller", "LEVEL=cli", "MODE=cli"])),
        "cli cli from sh cli\n"
    );
}

#[test]
fn a_failing_command_stops_the_task_with_its_code() {
    let project = Project::new(
        r#"{
          tasks: {
            fails: { cmds: ["exit 3", "echo never"] },
            tolerant: { cmds: [{ cmd: "exit 4", ignore_error: true }, "echo after"] },
          },
        }"#,
    );
    let failed = project.run(&["fails"]);
    assert_eq!(failed.status.code(), Some(3));
    assert!(!stdout(&failed).contains("never"));
    assert!(stderr(&failed).contains("task `fails`: `exit 3` exited with 3"));
    assert_eq!(stdout(&project.run(&["tolerant", "-s"])), "after\n");
}

#[test]
fn checks_requirements_and_preconditions() {
    let project = Project::new(
        r#"{
          tasks: {
            deploy: {
              requires: { vars: [{ name: "ENV", enum: ["staging", "production"] }] },
              preconditions: [{ sh: "test -f ready", msg: "ready is missing" }],
              cmds: ["echo deploying to ${{ ENV }}"],
            },
          },
        }"#,
    );
    assert!(stderr(&project.run(&["deploy"])).contains("needs the variable ENV"));
    assert!(
        stderr(&project.run(&["deploy", "ENV=moon"])).contains("not one of staging, production")
    );
    assert!(stderr(&project.run(&["deploy", "ENV=staging"])).contains("ready is missing"));
    std::fs::write(project.path("ready"), "").unwrap();
    assert_eq!(
        stdout(&project.run(&["deploy", "ENV=staging", "--silent"])),
        "deploying to staging\n"
    );
}

#[test]
fn refuses_loops_internal_tasks_and_unknown_tasks() {
    let project = Project::new(
        r#"{
          tasks: {
            a: { deps: ["b"] },
            b: { deps: ["a"] },
            hidden: { internal: true, cmd: "echo no" },
          },
        }"#,
    );
    assert!(stderr(&project.run(&["a"])).contains("a loop of tasks: a -> b -> a"));
    assert!(stderr(&project.run(&["hidden"])).contains("is internal"));
    assert!(stderr(&project.run(&["nope"])).contains("no task `nope`"));
}

#[test]
fn lists_and_summarizes() {
    let project = Project::new(
        r#"{
          tasks: {
            build: { desc: "Compile the program", summary: "Compiles all of src/.", cmd: "true" },
            "build-docs": { desc: "Write the documentation", cmd: "true" },
            quiet: "true",
            hidden: { desc: "Not listed", internal: true, cmd: "true" },
          },
        }"#,
    );
    assert_eq!(
        stdout(&project.run(&["--list"])),
        "* build       Compile the program\n* build-docs  Write the documentation\n"
    );
    assert_eq!(
        stdout(&project.run(&["--summary", "build"])),
        "Compiles all of src/.\n"
    );
}

#[test]
fn finds_the_file_above_and_runs_in_its_directory() {
    let project = Project::new(r#"{ tasks: { default: "pwd" } }"#);
    let sub = project.path("sub");
    std::fs::create_dir_all(&sub).unwrap();
    let output = run_in(&sub, &[]);
    assert!(output.status.success(), "{}", stderr(&output));
    let printed = PathBuf::from(stdout(&output).trim());
    assert_eq!(
        printed.canonicalize().unwrap(),
        project.dir.canonicalize().unwrap()
    );
}

#[test]
fn refuses_two_files_and_names_what_is_not_read_yet() {
    let project = Project::new("{}");
    std::fs::write(project.path("Outil.yml"), "tasks: {}\n").unwrap();
    assert!(stderr(&project.run(&[])).contains("holds both Outil.json5 and Outil.yml"));
    let srcs = Project::new(r#"{ tasks: { a: { srcs: ["*.c"], cmd: "true" } } }"#);
    assert!(stderr(&srcs.run(&["a"])).contains("`srcs` is part of the format but not carried out"));
}

#[test]
fn reports_json5_errors_with_their_lines() {
    let project = Project::new("{\n  tasks: {\n    a: 'x',\n    a: 'y',\n  },\n}\n");
    let output = project.run(&["a"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("the key `a` is written twice in one object, on lines 3 and 4"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn passes_arguments_flags_and_environment() {
    let project = Project::new(
        r#"{
          env: { FROM_FILE: "file-${{ TASK }}" },
          tasks: {
            show: {
              env: { FROM_TASK: "task" },
              cmds: ["echo $FROM_FILE $FROM_TASK '${{ CLI_ARGS }}' ${{ CLI_FORCE }} ${{ CLI_SILENT }}"],
            },
          },
        }"#,
    );
    assert_eq!(
        stdout(&project.run(&["show", "-s", "--force", "--", "a", "b"])),
        "file-show task a b true true\n"
    );
}
