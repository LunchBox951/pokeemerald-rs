//! Where the pack lives at runtime.
//!
//! The compile-time checkout path (`env!("CARGO_MANIFEST_DIR")`) is the
//! *build machine's*, right for a developer running `cargo test` and wrong
//! for every distributed binary. The shipped ROM importer (Policy C) makes
//! the pack a user-facing path, so resolution owns the rungs a shipped
//! binary needs and falls back to the checkout last.
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

/// Build-selected channel; ordinary source builds use `dev`.
pub const RELEASE_CHANNEL: &str = env!("POKEEMERALD_BUILD_CHANNEL");

/// Shared save and pack directory below the OS user-data directory.
pub const APP_DATA_SUBDIRECTORY: &str = env!("POKEEMERALD_APP_DATA_SUBDIRECTORY");

/// The pack's file name inside [`APP_DATA_SUBDIRECTORY`].
const PACK_FILE_NAME: &str = "pokeemerald.pack";

/// The family for the host this binary was built for.
const HOST_RULE: HostFamily = HostFamily::host();

/// The OS user-data directory, or `None` when every root it could be built
/// from is unset, empty, or not an absolute path (a daemon with a scrubbed
/// environment, say, or a relative `$HOME` that would make the pack's
/// location depend on the launch directory).
///
/// - Linux and other Unix: `$XDG_DATA_HOME` if absolute, else
///   `$HOME/.local/share` if `$HOME` is absolute.
/// - macOS: `$HOME/Library/Application Support` if `$HOME` is absolute.
/// - Windows: `%APPDATA%` if absolute, else
///   `%USERPROFILE%\AppData\Roaming` if `%USERPROFILE%` is absolute.
///
/// Resolved by [`data_dir_for`], the same function the save file uses: a
/// player's pack and a player's save belong under one per-user directory.
///
/// Hand-rolled from [`mod@std::env`] rather than taken from a crate
/// `(minimal-deps)`: three rules is less code than a dependency review.
#[must_use]
pub fn user_data_dir() -> Option<PathBuf> {
    data_dir_for(HOST_RULE, std_env)
}

/// Where the ROM importer writes by default:
/// [`user_data_dir`]/[`APP_DATA_SUBDIRECTORY`]/`pokeemerald.pack`.
///
/// Returned whether or not the file exists; the importer needs the path
/// before it has written anything there.
#[must_use]
pub fn user_pack_path() -> Option<PathBuf> {
    user_data_dir().map(|dir| dir.join(APP_DATA_SUBDIRECTORY).join(PACK_FILE_NAME))
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
/// reaching past it for another one.
///
/// Rung 4 always yields a path in a `dev` build, so a `dev` build's
/// resolution never fails; the caller's own "no pack extracted yet"
/// diagnostic covers a path that does not exist there. Channel builds
/// return their user path even when absent, and fall back to a channel
/// directory beside the executable when there is no user directory — never
/// a different channel's pack or the build machine's checkout.
///
/// # Panics
///
/// A non-`dev` build panics when no override is set, no user-data
/// directory is known, and the running executable's own directory cannot
/// be determined: the only candidate left would be relative to the
/// process's current directory, the hazard [`data_dir_for`]
/// already refuses for a relative `$XDG_DATA_HOME`.
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
/// [`NotFound`](std::io::ErrorKind::NotFound) and
/// [`NotADirectory`](std::io::ErrorKind::NotADirectory) advance: both prove
/// no pack can be at the candidate, the former because nothing is there and
/// the latter because a parent component is a regular file. Anything else
/// at the candidate that is not a regular file counts as missing too: a
/// directory named `pokeemerald.pack` is not a pack, and the next rung is a
/// better answer than a read error on it.
fn probe(path: &Path) -> Probe {
    match path.metadata() {
        Ok(meta) if meta.is_file() => Probe::Found,
        Ok(_) => Probe::Missing,
        Err(err)
            if matches!(
                err.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
            ) =>
        {
            Probe::Missing
        }
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
///
/// # Panics
///
/// When [`default_pack_path`] does; its `# Panics` owns the condition.
fn resolve(
    env: &impl Fn(&str) -> Option<OsString>,
    exe_dir: Option<&Path>,
    probe: &impl Fn(&Path) -> Probe,
    rule: HostFamily,
) -> PathBuf {
    if let Some(value) = env(PACK_PATH_ENV) {
        if !value.is_empty() {
            return PathBuf::from(value);
        }
    }
    if let Some(dir) = data_dir_for(rule, env) {
        let candidate = dir.join(APP_DATA_SUBDIRECTORY).join(PACK_FILE_NAME);
        if RELEASE_CHANNEL != "dev" || probe(&candidate) != Probe::Missing {
            return candidate;
        }
    }
    if RELEASE_CHANNEL != "dev" {
        return exe_dir
            .expect(
                "a release build must not fall back through the process's current directory \
                 when neither a user-data directory nor an executable directory is known",
            )
            .join(APP_DATA_SUBDIRECTORY)
            .join(PACK_FILE_NAME);
    }
    if let Some(dir) = exe_dir {
        let candidate = dir.join(OUTPUT_RELATIVE_PATH);
        if probe(&candidate) != Probe::Missing {
            return candidate;
        }
    }
    repo_pack_path()
}

/// Host convention used to resolve a per-user data directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostFamily {
    /// `%APPDATA%` if absolute, else `%USERPROFILE%\AppData\Roaming` if
    /// `%USERPROFILE%` is absolute.
    Windows,
    /// `$HOME/Library/Application Support` if `$HOME` is absolute.
    MacOs,
    /// The XDG Base Directory Specification: `$XDG_DATA_HOME` when
    /// absolute, else `$HOME/.local/share` if `$HOME` is absolute.
    Xdg,
}

impl HostFamily {
    /// The family this binary was compiled for.
    #[must_use]
    pub const fn host() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Xdg
        }
    }
}

/// Resolves `family`'s per-user data directory through `env`.
///
/// This is the one resolver both the pack and the save file use, so a
/// player's pack and save always land under one directory. The result is the
/// OS data directory, not the application subdirectory.
///
/// - [`HostFamily::Xdg`]: `$XDG_DATA_HOME`, else `$HOME/.local/share`.
/// - [`HostFamily::MacOs`]: `$HOME/Library/Application Support`.
/// - [`HostFamily::Windows`]: `%APPDATA%`, else
///   `%USERPROFILE%\AppData\Roaming`.
///
/// Only unset, empty, or non-absolute roots are ignored; `None` when no
/// eligible root remains. Absoluteness follows `family` (POSIX or Windows
/// path rules read from the bytes), independently of the platform running
/// this binary. There are no filesystem probes, no canonicalisation, and no
/// further rejection of absolute roots.
#[must_use]
pub fn data_dir_for(family: HostFamily, env: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let absolute_root = |name: &str, is_absolute: fn(&OsStr) -> bool| {
        env(name)
            .filter(|value| is_absolute(value))
            .map(PathBuf::from)
    };
    match family {
        HostFamily::Windows => absolute_root("APPDATA", is_absolute_windows_path).or_else(|| {
            absolute_root("USERPROFILE", is_absolute_windows_path)
                .map(|home| home.join("AppData").join("Roaming"))
        }),
        HostFamily::MacOs => absolute_root("HOME", is_absolute_xdg_path)
            .map(|home| home.join("Library").join("Application Support")),
        HostFamily::Xdg => absolute_root("XDG_DATA_HOME", is_absolute_xdg_path).or_else(|| {
            absolute_root("HOME", is_absolute_xdg_path)
                .map(|home| home.join(".local").join("share"))
        }),
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
/// [`Path::is_absolute`] because
/// the rule is POSIX's, so it must not change shape when [`data_dir_for`] is
/// driven with [`HostFamily::Xdg`] on a Windows host in a test.
fn is_absolute_xdg_path(path: &OsStr) -> bool {
    path.as_encoded_bytes().starts_with(b"/")
}

/// Whether `path` is absolute under Windows path rules (a drive letter
/// followed by a separator, a UNC root naming a server and share, either
/// bare or behind a verbatim `\\?\` or device `\\.\` prefix, or any other
/// named NT namespace root behind such a prefix), independently of the platform
/// running this binary. Drive-relative (`C:foo`) and root-relative (`\foo`)
/// forms depend on the current directory or drive, so they are rejected like
/// any other relative path.
fn is_absolute_windows_path(path: &OsStr) -> bool {
    let bytes = path.as_encoded_bytes();
    let is_separator = |byte: u8| byte == b'/' || byte == b'\\';
    let drive_absolute = matches!(
        bytes,
        [letter, b':', separator, ..] if letter.is_ascii_alphabetic() && is_separator(*separator)
    );
    let unc = match bytes {
        [first, second, rest @ ..] if is_separator(*first) && is_separator(*second) => {
            let mut components = rest.split(|byte| is_separator(*byte));
            match components.next() {
                // A verbatim (`\\?\`) or device (`\\.\`) prefix is not a
                // server. What follows is an NT namespace root (`C:`,
                // `Volume{GUID}`, `GLOBALROOT`, `BootPartition`, ...), which
                // never depends on the current directory, so any named root
                // followed by a separator is absolute. `UNC` is the one root
                // that is itself incomplete without a server and share.
                Some(b"?" | b".") => match components.next() {
                    Some(unc) if unc.eq_ignore_ascii_case(b"UNC") => matches!(
                        (components.next(), components.next()),
                        (Some(server), Some(share)) if !server.is_empty() && !share.is_empty()
                    ),
                    Some(root) => !root.is_empty() && components.next().is_some(),
                    None => false,
                },
                Some(server) => matches!(
                    components.next(),
                    Some(share) if !server.is_empty() && !share.is_empty()
                ),
                None => false,
            }
        }
        _ => false,
    };
    drive_absolute || unc
}

/// The checkout's own pack: `<repo root>/`[`OUTPUT_RELATIVE_PATH`], where
/// `cargo xtask extract` writes. This crate's manifest directory is always
/// `<repo root>/crates/pack-format`, so two levels up is the repo root.
///
/// Resolved at compile time, which is exactly why it is [`default_pack_path`]'s last
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
