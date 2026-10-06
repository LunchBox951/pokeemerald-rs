//! Re-launches `xtask` under Cargo's release profile (F-3, V-1; issue #1878).
//!
//! The `cargo xtask` alias forwards everything after `--` to this binary, so
//! an application-level `--release` cannot choose the profile the binary was
//! compiled with. When it is requested of a non-release binary, the dispatcher
//! builds and runs a release `xtask` through an outer Cargo invocation.

use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::process::Command;

use crate::XtaskError;

/// Whether this binary was compiled with Cargo's `release` profile.
///
/// Compile-time evidence recorded by `build.rs`, not a runtime marker a
/// caller could set to claim a debug binary is a release one.
pub(crate) fn built_for_release() -> bool {
    env!("XTASK_BUILD_PROFILE") == "release"
}

/// The Cargo executable to launch: `$CARGO` if set, else `cargo` on `PATH`.
pub(crate) fn cargo_executable() -> OsString {
    std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"))
}

/// The `xtask` features this binary was compiled with, so the release child
/// runs the same subcommands rather than a different feature set.
pub(crate) fn enabled_features() -> Vec<&'static str> {
    let mut features = Vec::new();
    if cfg!(feature = "scenes") {
        features.push("scenes");
    }
    if cfg!(feature = "smoke") {
        features.push("smoke");
    }
    if cfg!(feature = "record-snapshot") {
        features.push("record-snapshot");
    }
    if cfg!(feature = "scenario") {
        features.push("scenario");
    }
    if cfg!(feature = "rom-drift") {
        features.push("rom-drift");
    }
    features
}

/// Builds the outer `cargo run --release` command for `e2e --suite smoke`.
///
/// The child keeps the application `--release` flag; its own build script
/// reports `release`, so it runs smoke directly instead of launching again.
pub(crate) fn smoke_release_command(cargo: &OsStr, repo_root: &Path, features: &[&str]) -> Command {
    let mut command = Command::new(cargo);
    command
        .current_dir(repo_root)
        .args(["run", "--release", "--locked", "--manifest-path"])
        .arg(repo_root.join("Cargo.toml"))
        .args(["-p", "xtask", "--no-default-features"]);
    if !features.is_empty() {
        command.arg("--features").arg(features.join(","));
    }
    command.args(["--", "e2e", "--suite", "smoke", "--release"]);
    command
}

/// Runs `command`, mapping spawn failure and any unsuccessful exit to an error.
pub(crate) fn launch(command: &mut Command) -> Result<(), XtaskError> {
    launch_with(command, Command::status)
}

fn launch_with<F>(command: &mut Command, run: F) -> Result<(), XtaskError>
where
    F: FnOnce(&mut Command) -> std::io::Result<std::process::ExitStatus>,
{
    match run(command) {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(XtaskError::E2eReleaseFailed(format!(
            "release child exited unsuccessfully ({status})"
        ))),
        Err(err) => Err(XtaskError::E2eReleaseFailed(format!(
            "could not launch `{}`: {err}",
            command.get_program().to_string_lossy()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_has_exact_argv() {
        let cmd = smoke_release_command(OsStr::new("cargo"), Path::new("/r"), &["scenes", "smoke"]);
        let argv: Vec<&OsStr> = cmd.get_args().collect();
        let expected = [
            "run",
            "--release",
            "--locked",
            "--manifest-path",
            "/r/Cargo.toml",
            "-p",
            "xtask",
            "--no-default-features",
            "--features",
            "scenes,smoke",
            "--",
            "e2e",
            "--suite",
            "smoke",
            "--release",
        ];
        assert_eq!(argv, expected.map(OsStr::new));
        assert_eq!(cmd.get_program(), "cargo");
        assert_eq!(cmd.get_current_dir(), Some(Path::new("/r")));
    }

    #[test]
    fn empty_features_omit_flag() {
        let cmd = smoke_release_command(OsStr::new("cargo"), Path::new("/r"), &[]);
        assert!(!cmd.get_args().any(|arg| arg == "--features"));
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_root_is_preserved() {
        use std::os::unix::ffi::OsStrExt;
        let root = Path::new(OsStr::from_bytes(b"/r\xff"));
        let cmd = smoke_release_command(OsStr::new("cargo"), root, &[]);
        assert_eq!(cmd.get_current_dir(), Some(root));
        assert!(cmd
            .get_args()
            .any(|arg| arg.as_bytes() == b"/r\xff/Cargo.toml"));
    }

    #[test]
    fn spawn_failure_is_an_error() {
        let mut command = Command::new("cargo");
        let err =
            launch_with(&mut command, |_| Err(std::io::Error::other("no such file"))).unwrap_err();
        assert!(matches!(err, XtaskError::E2eReleaseFailed(_)));
    }

    #[cfg(unix)]
    #[test]
    fn exit_status_decides_success() {
        use std::os::unix::process::ExitStatusExt;
        use std::process::ExitStatus;
        let mut command = Command::new("cargo");
        let err = launch_with(&mut command, |_| Ok(ExitStatus::from_raw(1 << 8))).unwrap_err();
        assert!(matches!(err, XtaskError::E2eReleaseFailed(_)));
        launch_with(&mut command, |_| Ok(ExitStatus::from_raw(0))).unwrap();
    }
}
