//! Where the pack lives at runtime.
//!
//! `AssetPack::default_path` used to derive the pack's location from
//! `env!("CARGO_MANIFEST_DIR")`, the *build machine's* checkout. That is
//! right for a developer running `cargo test` and wrong for every
//! distributed binary, which would look for the pack on a CI runner's disk.
//! Policy C (the shipped ROM importer) makes that a real user-facing path,
//! so resolution moves here and gains the rungs a shipped binary needs.
//!
//! Resolution is pure: [`resolve`] takes the environment, the executable's
//! directory, and an existence predicate as arguments, so every rung and
//! every OS convention is unit-testable on one host without touching the
//! real environment.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::layout::OUTPUT_RELATIVE_PATH;

/// The environment variable that overrides pack resolution outright. Holds
/// a path to the pack *file*, not its directory.
pub const PACK_PATH_ENV: &str = "POKEEMERALD_PACK";

/// This project's directory inside the OS user-data directory.
const APP_DIR: &str = "pokeemerald-rs";

/// The pack's file name inside [`APP_DIR`].
const PACK_FILE_NAME: &str = "pokeemerald.pack";

/// Which OS convention names the per-user data directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DataDirRule {
    /// Linux and other Unix: `$XDG_DATA_HOME`, else `$HOME/.local/share`.
    Xdg,
    /// macOS: `$HOME/Library/Application Support`.
    MacOs,
    /// Windows: `%APPDATA%`.
    Windows,
}

/// The rule for the host this binary was built for.
const HOST_RULE: DataDirRule = if cfg!(windows) {
    DataDirRule::Windows
} else if cfg!(target_os = "macos") {
    DataDirRule::MacOs
} else {
    DataDirRule::Xdg
};

/// The OS user-data directory, or `None` when the variables it is built
/// from are unset (a daemon with a scrubbed environment, say).
///
/// - Linux and other Unix: `$XDG_DATA_HOME`, else `$HOME/.local/share`.
/// - macOS: `$HOME/Library/Application Support`.
/// - Windows: `%APPDATA%`.
///
/// Hand-rolled from [`mod@std::env`] rather than taken from a crate
/// `(minimal-deps)`: three rules is less code than a dependency review.
#[must_use]
pub fn user_data_dir() -> Option<PathBuf> {
    data_dir(&std_env, HOST_RULE)
}

/// Where the ROM importer writes by default:
/// [`user_data_dir`]`/pokeemerald-rs/pokeemerald.pack`.
///
/// Returned whether or not the file exists; the importer needs the path
/// before it has written anything there.
#[must_use]
pub fn user_pack_path() -> Option<PathBuf> {
    user_data_dir().map(|dir| dir.join(APP_DIR).join(PACK_FILE_NAME))
}

/// The pack's location for this run. First match wins:
///
/// 1. `$POKEEMERALD_PACK`, if set and non-empty. An explicit override,
///    honoured even if the file is absent, so a typo reports a missing pack
///    at that path instead of silently loading another one.
/// 2. [`user_pack_path`], if that file exists. Where the shipped ROM
///    importer writes.
/// 3. `<directory of the running executable>/assets-pack/pokeemerald.pack`,
///    if it exists. Portable installs that carry the pack beside the binary.
/// 4. `<this crate's repo root>/assets-pack/pokeemerald.pack`. The
///    compile-time developer path, which keeps `cargo test` working in a
///    checkout with nothing configured.
///
/// Rung 4 always yields a path, so this never fails; the caller's own
/// "no pack extracted yet" diagnostic covers a path that does not exist.
#[must_use]
pub fn default_pack_path() -> PathBuf {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf));
    resolve(
        &std_env,
        exe_dir.as_deref(),
        &|path| path.is_file(),
        HOST_RULE,
    )
}

/// [`std::env::var_os`] as a plain function, so [`resolve`] can be handed a
/// fake environment in tests without either side naming a closure type.
fn std_env(key: &str) -> Option<OsString> {
    std::env::var_os(key)
}

/// [`default_pack_path`]'s pure core: see it for the resolution order.
///
/// `env` reads environment variables, `exe_dir` is the running
/// executable's directory (`None` when the OS will not say), `exists`
/// answers whether a candidate file is there, and `rule` selects the
/// user-data-directory convention.
fn resolve(
    env: &impl Fn(&str) -> Option<OsString>,
    exe_dir: Option<&Path>,
    exists: &impl Fn(&Path) -> bool,
    rule: DataDirRule,
) -> PathBuf {
    if let Some(value) = env(PACK_PATH_ENV) {
        if !value.is_empty() {
            return PathBuf::from(value);
        }
    }
    if let Some(dir) = data_dir(env, rule) {
        let candidate = dir.join(APP_DIR).join(PACK_FILE_NAME);
        if exists(&candidate) {
            return candidate;
        }
    }
    if let Some(dir) = exe_dir {
        let candidate = dir.join(OUTPUT_RELATIVE_PATH);
        if exists(&candidate) {
            return candidate;
        }
    }
    repo_pack_path()
}

/// [`user_data_dir`]'s pure core.
fn data_dir(env: &impl Fn(&str) -> Option<OsString>, rule: DataDirRule) -> Option<PathBuf> {
    let non_empty = |key: &str| {
        env(key)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    match rule {
        DataDirRule::Xdg => non_empty("XDG_DATA_HOME")
            .or_else(|| non_empty("HOME").map(|home| home.join(".local").join("share"))),
        DataDirRule::MacOs => {
            non_empty("HOME").map(|home| home.join("Library").join("Application Support"))
        }
        DataDirRule::Windows => non_empty("APPDATA"),
    }
}

/// The developer path: this crate's manifest directory is always
/// `<repo root>/crates/pack-format`, so two levels up is the repo root.
///
/// Resolved at compile time, which is exactly why it is the last rung: it
/// names the machine that built the binary, not the one running it.
fn repo_pack_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map_or_else(
            || PathBuf::from(OUTPUT_RELATIVE_PATH),
            |root| root.join(OUTPUT_RELATIVE_PATH),
        )
}

#[cfg(test)]
mod tests;
