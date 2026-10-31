use std::fs;
use std::path::PathBuf;

use super::super::tests::TempDir;
#[cfg(unix)]
use super::super::tests::{
    fill_descriptor_table_leaving, in_descriptor_pressure_child, run_under_descriptor_pressure,
};
use super::*;

mod creation;
#[cfg(unix)]
mod durability;
#[cfg(unix)]
mod quarantine;
#[cfg(unix)]
mod races;
#[cfg(unix)]
mod report;
#[cfg(unix)]
mod restore;
#[cfg(unix)]
mod unexamined;

#[cfg(unix)]
fn entry_names(dir: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[cfg(unix)]
const ROLLBACK_STDERR_CHILD: &str = "POKEEMERALD_IMPORT_ROLLBACK_STDERR_CHILD";

/// Whether this process is the child [`rerun_alone_for_stderr`] spawned.
#[cfg(unix)]
fn in_rollback_stderr_child() -> bool {
    std::env::var_os(ROLLBACK_STDERR_CHILD).is_some()
}

/// Reruns the test at `test_path` alone in a child, fails unless it passes
/// there, and returns the child's stderr, where rollback warns.
#[cfg(unix)]
fn rerun_alone_for_stderr(test_path: &str) -> String {
    let exe = std::env::current_exe().unwrap();
    let output = std::process::Command::new(exe)
        .args([test_path, "--exact", "--nocapture"])
        .env(ROLLBACK_STDERR_CHILD, "1")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        stdout.contains("1 passed"),
        "child failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    stderr
}

/// The path a `<lead><path><tail>` line in `stderr` names.
#[cfg(unix)]
fn warned_path<'a>(stderr: &'a str, lead: &str, tail: &str) -> &'a str {
    stderr
        .lines()
        .find_map(|line| {
            let rest = line.strip_prefix(lead)?;
            rest.find(tail).map(|end| &rest[..end])
        })
        .unwrap_or_else(|| panic!("no {lead:?}...{tail:?} line; stderr:\n{stderr}"))
}

/// Asserts `warned` is the path the child printed on its `marker` line,
/// as recorded or made canonical: Linux reports it through `/proc`, which
/// resolves symlinks, elsewhere it is the recorded path, which may run
/// through one (macOS's `/var`).
#[cfg(unix)]
fn assert_names_marked_path(stderr: &str, warned: &str, marker: &str) {
    let line = stderr
        .lines()
        .find_map(|line| line.strip_prefix(marker))
        .unwrap_or_else(|| panic!("no {marker:?} line; stderr:\n{stderr}"));
    let (recorded, canonical) = line.split_once(" | ").expect("recorded | canonical");
    assert!(
        warned == recorded || warned == canonical,
        "warned {warned:?}, expected {recorded:?} or {canonical:?}"
    );
}

/// Prints `path` after `marker` as recorded and canonical, for
/// [`assert_names_marked_path`] in the parent.
#[cfg(unix)]
fn mark_path(marker: &str, path: &std::path::Path) {
    eprintln!(
        "{marker}{} | {}",
        path.display(),
        fs::canonicalize(path).unwrap().display()
    );
}
