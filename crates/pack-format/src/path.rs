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

use std::ffi::{OsStr, OsString};
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
    /// Linux and other Unix: `$XDG_DATA_HOME` if absolute, else
    /// `$HOME/.local/share`.
    Xdg,
    /// macOS: `$HOME/Library/Application Support`.
    MacOs,
    /// Windows: `%APPDATA%`, else `%USERPROFILE%\AppData\Roaming`.
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
/// - Linux and other Unix: `$XDG_DATA_HOME` if absolute, else
///   `$HOME/.local/share`.
/// - macOS: `$HOME/Library/Application Support`.
/// - Windows: `%APPDATA%`, else `%USERPROFILE%\AppData\Roaming`.
///
/// Deliberately the same three rules `engine::save::file::data_dir_for`
/// resolves the save file with, down to the fallbacks: a player's pack and
/// a player's save belong under one per-user directory, so the two
/// resolvers disagreeing about where that is would split them across the
/// disk in exactly the environments the fallbacks exist for.
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
/// "If that file exists" in rungs 2 and 3 means *known* not to exist. A
/// candidate that cannot be examined at all — an unsearchable directory
/// component, say — stops resolution and is returned, so the loader's error
/// names the pack the player actually installed instead of silently
/// reaching past it for another one. See [`Probe`].
///
/// Rung 4 always yields a path, so this never fails; the caller's own
/// "no pack extracted yet" diagnostic covers a path that does not exist.
#[must_use]
pub fn default_pack_path() -> PathBuf {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf));
    resolve(&std_env, exe_dir.as_deref(), &probe, HOST_RULE)
}

/// What a look at a candidate pack path found.
///
/// Three answers, not two, because "there is no pack here" and "I was not
/// allowed to look" are different facts and only the first should send
/// resolution to the next rung.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Probe {
    /// A pack file is there.
    Found,
    /// Nothing is there. The next rung is the right place to look.
    Missing,
    /// The candidate could not be examined — a directory component that
    /// cannot be searched, most often. Whether a pack is there is unknown.
    Unreadable,
}

/// [`Path::is_file`], but keeping the distinction that method throws away.
///
/// `is_file` folds every error into `false`, so a user pack sitting behind
/// an unsearchable directory reads as absent and resolution walks on to a
/// portable install or the compile-time checkout path. The player then gets
/// either a *different* pack loaded silently or a "no pack" message naming
/// a path they have never heard of, when the honest answer is that their
/// own installed pack could not be reached.
///
/// Only [`NotFound`](std::io::ErrorKind::NotFound) advances. Anything at
/// the candidate that is not a regular file counts as missing too: a
/// directory named `pokeemerald.pack` is not a pack, and the next rung is a
/// better answer than a read error on it.
fn probe(path: &Path) -> Probe {
    match path.metadata() {
        Ok(meta) if meta.is_file() => Probe::Found,
        Ok(_) => Probe::Missing,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Probe::Missing,
        Err(_) => Probe::Unreadable,
    }
}

/// [`std::env::var_os`] as a plain function, so [`resolve`] can be handed a
/// fake environment in tests without either side naming a closure type.
fn std_env(key: &str) -> Option<OsString> {
    std::env::var_os(key)
}

/// [`default_pack_path`]'s pure core: see it for the resolution order.
///
/// `env` reads environment variables, `exe_dir` is the running
/// executable's directory (`None` when the OS will not say), `probe` looks
/// at a candidate file, and `rule` selects the user-data-directory
/// convention.
///
/// A rung is skipped only on [`Probe::Missing`]. [`Probe::Unreadable`]
/// *stops* here and hands the candidate back: the pack may well be there,
/// and letting `AssetPack::load` fail on the path the player actually
/// installed to is the only way they learn it was a permission problem
/// rather than a missing file.
fn resolve(
    env: &impl Fn(&str) -> Option<OsString>,
    exe_dir: Option<&Path>,
    probe: &impl Fn(&Path) -> Probe,
    rule: DataDirRule,
) -> PathBuf {
    if let Some(value) = env(PACK_PATH_ENV) {
        if !value.is_empty() {
            return PathBuf::from(value);
        }
    }
    if let Some(dir) = data_dir(env, rule) {
        let candidate = dir.join(APP_DIR).join(PACK_FILE_NAME);
        if probe(&candidate) != Probe::Missing {
            return candidate;
        }
    }
    if let Some(dir) = exe_dir {
        let candidate = dir.join(OUTPUT_RELATIVE_PATH);
        if probe(&candidate) != Probe::Missing {
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
            .filter(|dir| is_absolute_xdg_path(dir.as_os_str()))
            .or_else(|| non_empty("HOME").map(|home| home.join(".local").join("share"))),
        DataDirRule::MacOs => {
            non_empty("HOME").map(|home| home.join("Library").join("Application Support"))
        }
        DataDirRule::Windows => non_empty("APPDATA")
            .or_else(|| non_empty("USERPROFILE").map(|home| home.join("AppData").join("Roaming"))),
    }
}

/// Whether `path` is absolute under the XDG Base Directory Specification's
/// POSIX path rules, independently of the platform running this binary.
///
/// The specification requires a relative `$XDG_DATA_HOME` to be *ignored*,
/// not resolved: honouring one would let the process's current directory
/// pick the pack, so `pokeemerald-rs` launched from an untrusted directory
/// with `XDG_DATA_HOME=data` would load `data/pokeemerald-rs/pokeemerald.pack`
/// from it instead of the player's own. Asks the bytes rather than
/// [`Path::is_absolute`] for the same reason
/// `engine::save::file::is_absolute_xdg_path` does: the rule is POSIX's, so
/// it must not change shape when [`data_dir`] is driven with
/// [`DataDirRule::Xdg`] on a Windows host in a test.
fn is_absolute_xdg_path(path: &OsStr) -> bool {
    path.as_encoded_bytes().starts_with(b"/")
}

/// The checkout's own pack: `<repo root>/`[`OUTPUT_RELATIVE_PATH`], where
/// `cargo xtask extract` writes. This crate's manifest directory is always
/// `<repo root>/crates/pack-format`, so two levels up is the repo root.
///
/// Resolved at compile time, which is exactly why it is [`resolve`]'s last
/// rung: it names the machine that built the binary, not the one running
/// it.
///
/// Public because a *checkout-validation* gate must ask for it by name
/// rather than through [`default_pack_path`]. That resolver answers "where
/// does a running game find its pack", and its earlier rungs are the two
/// destinations `--import-rom` writes to, so a gate resolving through it
/// would validate whichever pack the developer happens to have installed
/// instead of the one `cargo xtask extract` just produced — an extractor
/// regression passing against an older user pack, or a stale user pack
/// failing a checkout that is fine `(test-ratchet)`. `xtask::extract::run`
/// and `rom-import`'s equivalence gate already compute this same path
/// privately for that reason.
#[must_use]
pub fn repo_pack_path() -> PathBuf {
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
