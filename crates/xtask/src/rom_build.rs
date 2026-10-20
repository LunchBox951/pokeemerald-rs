//! `rom-build`: materialise the pinned upstream checkout in isolated
//! temporary storage, run its own build, and report the compiled outputs.
//!
//! Developer-only. The upstream `Makefile` links the ELF with `-Map`, then its
//! `rom` rule runs `objcopy -O binary` to emit the cartridge image. This module
//! drives that build in a scratch clone so the reference tree is never written,
//! and fails closed on an unverifiable pin, a missing tool, a failed build,
//! missing or empty outputs, or an image other than the one the pinned
//! `rom.sha1` names (the check upstream's `compare` goal runs). The image is returned in memory as a
//! [`CompiledArtifact`]; it is never written into the repository.

mod artifact;
mod checkout;
#[cfg(test)]
mod tests;

use std::collections::VecDeque;
use std::ffi::OsString;
use std::fmt;
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};

pub use artifact::{CompiledArtifact, Provenance};

use crate::gen_rom_profile::SymbolMap;

/// The cartridge image the upstream `rom` rule writes at the tree root.
const ROM_FILE: &str = "pokeemerald.gba";
/// The linker map the upstream ELF rule writes at the tree root.
const MAP_FILE: &str = "pokeemerald.map";
/// The pinned tree's checksum of the original cartridge image, which the
/// upstream `compare` goal checks the built image against (`Makefile:221-227`).
const CHECKSUM_FILE: &str = "rom.sha1";
/// A GBA cartridge address space is 32 MiB.
const MAX_ROM_BYTES: usize = 32 * 1024 * 1024;
/// A linker map for the full upstream build is a few megabytes; one past
/// this bound is a runaway build, not a map worth parsing.
const MAX_MAP_BYTES: u64 = 64 * 1024 * 1024;
/// Tools the upstream build resolves from `PATH`.
const REQUIRED_TOOLS: [&str; 6] = [
    "make",
    "gcc",
    "perl",
    "arm-none-eabi-as",
    "arm-none-eabi-ld",
    "arm-none-eabi-objcopy",
];
/// Environment the build inherits; everything else is dropped. `OS` is how
/// the upstream `Makefile` and its tool makefiles detect Windows to append
/// `.exe` to tool paths (`Makefile:48-51`).
const INHERITED_ENV: [&str; 5] = ["PATH", "HOME", "DEVKITARM", "DEVKITPRO", "OS"];
/// Name prefixes the build also inherits. Upstream's NixOS shell
/// (`INSTALL.md:353-358`) passes include, library, and `pkg-config` search
/// paths to its compiler and `pkg-config` wrappers only through `NIX_*` and
/// `PKG_CONFIG_*` variables, and the host tools need them to find libpng
/// (`tools/gbagfx/Makefile:4,7`).
const INHERITED_ENV_PREFIXES: [&str; 2] = ["NIX_", "PKG_CONFIG_"];
/// Names the directory holding an installed `agbcc`.
const AGBCC_ENV: &str = "POKEEMERALD_AGBCC";
/// Where the upstream `Makefile` looks for `agbcc`, relative to the tree.
const AGBCC_DEST: &str = "tools/agbcc";
/// How much of a failed build's stderr the error carries.
const STDERR_TAIL_BYTES: usize = 2000;

/// What [`build`] runs against.
#[derive(Debug, Clone)]
pub struct BuildConfig {
    /// The local reference checkout; only read.
    pub reference: PathBuf,
    /// The full pinned revision the reference must be at.
    pub pinned_revision: String,
    /// Directory under which the scratch build tree is created.
    pub scratch_root: PathBuf,
    /// The program that runs the upstream build.
    pub make: OsString,
    /// Programs that must resolve before any work starts.
    pub required_tools: Vec<OsString>,
    /// A developer's installed `agbcc` tree (`bin/`, `lib/`, `include/`),
    /// which the upstream build expects at `tools/agbcc` but which is
    /// untracked and git-ignored upstream.
    pub agbcc_source: PathBuf,
    /// Parallel build jobs.
    pub jobs: usize,
}

impl BuildConfig {
    /// The configuration for this repository's `pokeemerald/` reference.
    ///
    /// # Errors
    ///
    /// [`RomBuildError::PinUnreadable`] if `init.sh` names no pin, or
    /// [`RomBuildError::MissingTool`] if `POKEEMERALD_AGBCC` is unset.
    pub fn for_repo() -> Result<Self, RomBuildError> {
        let root = crate::extract::repo_root();
        let agbcc_source = std::env::var_os(AGBCC_ENV)
            .map(PathBuf::from)
            .ok_or_else(|| RomBuildError::MissingTool(format!("agbcc (set {AGBCC_ENV})")))?;
        Ok(Self {
            reference: root.join("pokeemerald"),
            pinned_revision: checkout::read_pin(&root.join("init.sh"))?,
            scratch_root: std::env::temp_dir(),
            make: OsString::from("make"),
            required_tools: REQUIRED_TOOLS.iter().map(OsString::from).collect(),
            agbcc_source,
            jobs: std::thread::available_parallelism().map_or(1, usize::from),
        })
    }
}

/// Why a build did not produce a [`CompiledArtifact`].
#[derive(Debug)]
pub enum RomBuildError {
    /// `init.sh` could not be read or names no pin.
    PinUnreadable { path: PathBuf, reason: String },
    /// The reference checkout directory does not exist.
    MissingReference(PathBuf),
    /// The reference cannot be shown to be a git checkout of the pin.
    Unverifiable { path: PathBuf, reason: String },
    /// A checkout is not at the pinned revision.
    PinMismatch {
        path: PathBuf,
        expected: String,
        actual: String,
    },
    /// A checkout has modified, untracked, or ignored files.
    DirtyTree { path: PathBuf, status: String },
    /// Tracked files carry assume-unchanged or skip-worktree index bits, which
    /// hide edits from `git status`.
    HiddenIndexState { path: PathBuf, entries: String },
    /// A required program is not on `PATH`.
    MissingTool(String),
    /// A git command failed.
    Git(String),
    /// Scratch storage could not be created.
    Scratch(String),
    /// The pinned tree could not be materialised.
    Materialize(String),
    /// The upstream build ran and failed, or could not be started.
    BuildFailed { reason: String },
    /// The build succeeded but did not emit an expected output.
    MissingOutput(String),
    /// An emitted output is unusable.
    InvalidOutput { file: &'static str, reason: String },
    /// The built image is not the one the pinned tree's `rom.sha1` names.
    ChecksumMismatch { expected: String, actual: String },
}

impl fmt::Display for RomBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PinUnreadable { path, reason } => {
                write!(
                    f,
                    "cannot read the pinned revision from {}: {reason}",
                    path.display()
                )
            }
            Self::MissingReference(path) => {
                write!(f, "{} is missing; run ./init.sh first", path.display())
            }
            Self::Unverifiable { path, reason } => write!(
                f,
                "{} cannot be verified against the pin ({reason}); restore it with \
                 `rm -rf -- <dir> && ./init.sh`",
                path.display()
            ),
            Self::PinMismatch {
                path,
                expected,
                actual,
            } => write!(
                f,
                "{} is at {actual}, but the pin is {expected}",
                path.display()
            ),
            Self::DirtyTree { path, status } => {
                write!(
                    f,
                    "{} is not a clean pinned tree:\n{status}",
                    path.display()
                )
            }
            Self::HiddenIndexState { path, entries } => write!(
                f,
                "{} has index-hidden paths (assume-unchanged or skip-worktree), so its \
                 edits cannot be ruled out:\n{entries}",
                path.display()
            ),
            Self::MissingTool(tool) => write!(
                f,
                "required tool `{tool}` was not found on PATH; install the upstream build \
                 toolchain (arm-none-eabi binutils, a host C compiler, make, perl)"
            ),
            Self::Git(reason) => write!(f, "git failed: {reason}"),
            Self::Scratch(reason) => write!(f, "cannot create scratch storage: {reason}"),
            Self::Materialize(reason) => {
                write!(f, "cannot materialise the pinned tree: {reason}")
            }
            Self::BuildFailed { reason } => write!(f, "the upstream build failed: {reason}"),
            Self::MissingOutput(file) => {
                write!(f, "the build succeeded but emitted no `{file}`")
            }
            Self::InvalidOutput { file, reason } => {
                write!(f, "the build's `{file}` is unusable: {reason}")
            }
            Self::ChecksumMismatch { expected, actual } => write!(
                f,
                "the built `{ROM_FILE}` has SHA-1 {actual}, but the pinned `{CHECKSUM_FILE}` \
                 names {expected}; the toolchain does not reproduce the pinned image"
            ),
        }
    }
}

impl std::error::Error for RomBuildError {}

/// A scratch directory removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn create(root: &Path) -> Result<Self, RomBuildError> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let path = root.join(format!(
            "pokeemerald-rs-rom-build-{}-{nanos}",
            std::process::id()
        ));
        let builder = &mut std::fs::DirBuilder::new();
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(builder, 0o700);
        builder
            .create(&path)
            .map_err(|err| RomBuildError::Scratch(format!("{}: {err}", path.display())))?;
        Ok(Self(path))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        // Best effort: a leftover temp directory must not mask the result.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Builds with the repository's own configuration.
///
/// # Errors
///
/// See [`build`] and [`BuildConfig::for_repo`].
pub fn run() -> Result<CompiledArtifact, RomBuildError> {
    build(&BuildConfig::for_repo()?)
}

/// Whether the `agbcc` install lies within the reference checkout, where its
/// untracked files would fail the clean-tree verification. Paths are compared
/// canonically so symlinks and `..` cannot hide containment.
fn agbcc_inside_reference(config: &BuildConfig) -> bool {
    let canon = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    canon(&config.agbcc_source).starts_with(canon(&config.reference))
}

/// Verifies the reference, builds the pinned tree in scratch storage, and
/// returns its compiled outputs.
///
/// # Errors
///
/// A [`RomBuildError`] naming the first failed stage; nothing is returned
/// from a partial build.
pub fn build(config: &BuildConfig) -> Result<CompiledArtifact, RomBuildError> {
    if agbcc_inside_reference(config) {
        return Err(RomBuildError::MissingTool(format!(
            "agbcc ({} is inside the reference checkout {}; install it elsewhere)",
            config.agbcc_source.display(),
            config.reference.display()
        )));
    }
    checkout::verify_reference(&config.reference, &config.pinned_revision)?;
    if !config.agbcc_source.join("bin").is_dir() {
        return Err(RomBuildError::MissingTool(format!(
            "agbcc (no `bin/` under {})",
            config.agbcc_source.display()
        )));
    }
    let search = search_path();
    let suffixes = executable_suffixes();
    for tool in &config.required_tools {
        if !on_path(tool, &search, &suffixes) {
            return Err(RomBuildError::MissingTool(
                tool.to_string_lossy().into_owned(),
            ));
        }
    }
    let scratch = Scratch::create(&config.scratch_root)?;
    let tree = scratch.0.join("pokeemerald");
    checkout::materialize(&config.reference, &config.pinned_revision, &tree)?;
    // Read before anything else runs in the tree, so the build cannot rewrite
    // the checksum it is checked against.
    let expected = pinned_checksum(&tree)?;
    copy_dir(&config.agbcc_source, &tree.join(AGBCC_DEST))
        .map_err(|err| RomBuildError::Materialize(format!("cannot provision agbcc: {err}")))?;
    let build_command = run_make(config, &tree)?;
    let rom = read_rom(&tree)?;
    verify_checksum(&expected, &rom)?;
    let symbols = read_map(&tree)?;
    Ok(CompiledArtifact::new(
        Provenance {
            pinned_revision: config.pinned_revision.clone(),
            build_command,
        },
        rom,
        symbols,
    ))
}

/// The directories the upstream build resolves tools from: its `Makefile`
/// prepends `$DEVKITARM/bin` to `PATH` when that directory exists.
fn search_path() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(kit) = std::env::var_os("DEVKITARM") {
        let bin = Path::new(&kit).join("bin");
        if bin.is_dir() {
            dirs.push(bin);
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    dirs
}

/// Filename suffixes the platform adds when resolving a program name: none on
/// Unix, `PATHEXT` (falling back to `.exe`) on Windows.
fn executable_suffixes() -> Vec<String> {
    if !cfg!(windows) {
        return Vec::new();
    }
    let pathext = std::env::var("PATHEXT").unwrap_or_default();
    let mut suffixes: Vec<String> = pathext
        .split(';')
        .filter(|ext| !ext.is_empty())
        .map(str::to_owned)
        .collect();
    if suffixes.is_empty() {
        suffixes.push(".exe".to_owned());
    }
    suffixes
}

/// Whether `tool` names a file, directly or in one of `search`, bare or with
/// one of `suffixes` appended.
fn on_path(tool: &OsString, search: &[PathBuf], suffixes: &[String]) -> bool {
    let exists = |base: &Path| {
        base.is_file()
            || suffixes.iter().any(|ext| {
                let mut named = base.as_os_str().to_owned();
                named.push(ext);
                Path::new(&named).is_file()
            })
    };
    let candidate = Path::new(tool);
    if candidate.components().count() > 1 {
        return exists(candidate);
    }
    search.iter().any(|dir| exists(&dir.join(candidate)))
}

/// Recursively copies `from` into a new directory `to`.
fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// The environment the build inherits: each [`INHERITED_ENV`] variable
/// `lookup` has a value for, then each of `vars` whose name starts with one of
/// [`INHERITED_ENV_PREFIXES`].
fn inherited_env(
    lookup: impl Fn(&str) -> Option<OsString>,
    vars: impl IntoIterator<Item = (OsString, OsString)>,
) -> Vec<(OsString, OsString)> {
    let named = INHERITED_ENV
        .iter()
        .filter_map(|&name| lookup(name).map(|value| (OsString::from(name), value)));
    let prefixed = vars.into_iter().filter(|(name, _)| {
        name.to_str().is_some_and(|name| {
            INHERITED_ENV_PREFIXES
                .iter()
                .any(|prefix| name.starts_with(prefix))
        })
    });
    named.chain(prefixed).collect()
}

/// Runs the upstream build in `tree` with a scrubbed environment and returns
/// the command line it ran.
fn run_make(config: &BuildConfig, tree: &Path) -> Result<String, RomBuildError> {
    let jobs = format!("-j{}", config.jobs.max(1));
    let mut command = Command::new(&config.make);
    command
        .arg(&jobs)
        .current_dir(tree)
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    command.envs(inherited_env(
        |name| std::env::var_os(name),
        std::env::vars_os(),
    ));
    let rendered = format!("{} {jobs}", config.make.to_string_lossy());
    let mut child = command.spawn().map_err(|err| {
        if err.kind() == ErrorKind::NotFound {
            RomBuildError::MissingTool(config.make.to_string_lossy().into_owned())
        } else {
            RomBuildError::BuildFailed {
                reason: format!("could not start `{rendered}`: {err}"),
            }
        }
    })?;
    let tail = Arc::new(Mutex::new(VecDeque::new()));
    let (done_tx, done_rx) = mpsc::channel();
    if let Some(stream) = child.stderr.take() {
        let tail = Arc::clone(&tail);
        std::thread::spawn(move || {
            let _ = done_tx.send(drain_tail(stream, &tail));
        });
    } else {
        drop(done_tx);
    }
    let status = child.wait().map_err(|err| RomBuildError::BuildFailed {
        reason: format!("could not wait for `{rendered}`: {err}"),
    })?;
    // A process make started can inherit stderr and hold the pipe open after
    // make exits, so the drain is given STDERR_GRACE to reach end of stream
    // and is then left behind with whatever tail it has read.
    if let Ok(Err(err)) = done_rx.recv_timeout(STDERR_GRACE) {
        return Err(RomBuildError::BuildFailed {
            reason: format!("could not read the stderr of `{rendered}`: {err}"),
        });
    }
    let stderr_bytes: Vec<u8> = lock_tail(&tail).iter().copied().collect();
    if status.success() {
        return Ok(rendered);
    }
    let stderr = String::from_utf8_lossy(&stderr_bytes);
    let tail_start = stderr
        .char_indices()
        .rev()
        .nth(STDERR_TAIL_BYTES)
        .map_or(0, |(i, _)| i);
    Err(RomBuildError::BuildFailed {
        reason: format!(
            "`{rendered}` exited with {}: {}",
            status,
            &stderr[tail_start..]
        ),
    })
}

/// How many bytes of stderr are kept: enough for the last
/// [`STDERR_TAIL_BYTES`] + 1 characters even when every one is four UTF-8
/// bytes and the first kept one is cut, so a build that floods stderr cannot
/// exhaust memory.
const STDERR_KEEP_BYTES: usize = 4 * (STDERR_TAIL_BYTES + 2);
/// How long a failed or finished build's stderr may stay open after make
/// exits before the tail read so far is reported.
const STDERR_GRACE: std::time::Duration = std::time::Duration::from_secs(2);

fn lock_tail(tail: &Mutex<VecDeque<u8>>) -> std::sync::MutexGuard<'_, VecDeque<u8>> {
    tail.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Reads `stream` to its end into `tail`, keeping only its last
/// [`STDERR_KEEP_BYTES`].
fn drain_tail(mut stream: impl Read, tail: &Mutex<VecDeque<u8>>) -> std::io::Result<()> {
    let mut chunk = [0_u8; 8192];
    loop {
        let read = match stream.read(&mut chunk) {
            Ok(0) => return Ok(()),
            Ok(read) => read,
            Err(err) if err.kind() == ErrorKind::Interrupted => continue,
            Err(err) => return Err(err),
        };
        let mut tail = lock_tail(tail);
        tail.extend(&chunk[..read]);
        let excess = tail.len().saturating_sub(STDERR_KEEP_BYTES);
        tail.drain(..excess);
    }
}

/// `O_NONBLOCK` where its value is fixed for the target (Linux on the listed
/// architectures, and the BSD family), so opening a FIFO swapped in after
/// [`open_output`]'s check cannot block. It has no effect on reads from a
/// regular file. Elsewhere the open is plain.
#[cfg(unix)]
const O_NONBLOCK: Option<i32> = if cfg!(all(
    any(target_os = "linux", target_os = "android"),
    any(
        target_arch = "x86",
        target_arch = "x86_64",
        target_arch = "arm",
        target_arch = "aarch64",
        target_arch = "riscv64",
        target_arch = "powerpc64",
        target_arch = "s390x"
    )
)) {
    Some(0o4000)
} else if cfg!(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "dragonfly"
)) {
    Some(0x0004)
} else {
    None
};

/// Opens the build output `name` only if it is a regular file, checked before
/// opening (a FIFO with no writer would block the open, a device would never
/// end) and again on the opened handle, so a path swapped in between is
/// refused rather than read.
fn open_output(tree: &Path, name: &'static str) -> Result<std::fs::File, RomBuildError> {
    let invalid = |reason: String| RomBuildError::InvalidOutput { file: name, reason };
    let path = tree.join(name);
    let before = match std::fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == ErrorKind::NotFound => {
            return Err(RomBuildError::MissingOutput(name.to_owned()));
        }
        Err(err) => return Err(invalid(err.to_string())),
    };
    if !before.file_type().is_file() {
        return Err(invalid(format!(
            "not a regular file ({:?})",
            before.file_type()
        )));
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    if let Some(flag) = O_NONBLOCK {
        std::os::unix::fs::OpenOptionsExt::custom_flags(&mut options, flag);
    }
    let file = options
        .open(&path)
        .map_err(|err| invalid(err.to_string()))?;
    let after = file.metadata().map_err(|err| invalid(err.to_string()))?;
    #[cfg(unix)]
    let same = {
        use std::os::unix::fs::MetadataExt;
        after.dev() == before.dev() && after.ino() == before.ino()
    };
    #[cfg(not(unix))]
    let same = true;
    if !after.is_file() || !same {
        return Err(invalid("replaced while it was being opened".to_owned()));
    }
    Ok(file)
}

/// Reads the built image, refusing one larger than a cartridge without
/// reading past that bound.
fn read_rom(tree: &Path) -> Result<Vec<u8>, RomBuildError> {
    let invalid = |reason: String| RomBuildError::InvalidOutput {
        file: ROM_FILE,
        reason,
    };
    let file = open_output(tree, ROM_FILE)?;
    let len = file
        .metadata()
        .map_err(|err| invalid(err.to_string()))?
        .len();
    if len > MAX_ROM_BYTES as u64 {
        return Err(invalid(implausible(len)));
    }
    let rom = read_bounded(file, MAX_ROM_BYTES as u64).map_err(|err| invalid(err.to_string()))?;
    if rom.is_empty() || rom.len() > MAX_ROM_BYTES {
        return Err(invalid(implausible(rom.len() as u64)));
    }
    Ok(rom)
}

/// At most one byte more than `limit` from `reader`, so an oversized or
/// still-growing output is detected without being read whole.
fn read_bounded(reader: impl Read, limit: u64) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take(limit + 1).read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Reads and parses the linker map through one handle, refusing one past
/// [`MAX_MAP_BYTES`] by its size and by a read that stops one byte past the
/// bound, so a process still writing the map cannot grow it past the check.
fn read_map(tree: &Path) -> Result<SymbolMap, RomBuildError> {
    let invalid = |reason: String| RomBuildError::InvalidOutput {
        file: MAP_FILE,
        reason,
    };
    let file = open_output(tree, MAP_FILE)?;
    let metadata = file.metadata().map_err(|err| invalid(err.to_string()))?;
    let past_bound = |len: u64| {
        invalid(format!(
            "{len} bytes is past the {MAX_MAP_BYTES}-byte bound for a linker map"
        ))
    };
    if metadata.len() > MAX_MAP_BYTES {
        return Err(past_bound(metadata.len()));
    }
    let bytes = read_bounded(file, MAX_MAP_BYTES).map_err(|err| invalid(err.to_string()))?;
    if bytes.len() as u64 > MAX_MAP_BYTES {
        return Err(past_bound(bytes.len() as u64));
    }
    parse_map(bytes)
}

/// Parses map text as [`SymbolMap::load`] does, requiring one symbol.
fn parse_map(bytes: Vec<u8>) -> Result<SymbolMap, RomBuildError> {
    let invalid = |reason: String| RomBuildError::InvalidOutput {
        file: MAP_FILE,
        reason,
    };
    let text = String::from_utf8(bytes).map_err(|err| invalid(err.to_string()))?;
    let symbols = SymbolMap::parse(&text);
    if symbols.is_empty() {
        return Err(invalid("no `0x<address> <symbol>` lines found".to_owned()));
    }
    Ok(symbols)
}

fn implausible(len: u64) -> String {
    format!("{len} bytes is not a plausible cartridge image")
}

/// The digest the materialised pinned tree's `rom.sha1` names for the image,
/// the value upstream's `compare` goal checks against (`Makefile:221-227`).
fn pinned_checksum(tree: &Path) -> Result<String, RomBuildError> {
    let listing = std::fs::read_to_string(tree.join(CHECKSUM_FILE)).map_err(|err| {
        RomBuildError::Materialize(format!("cannot read `{CHECKSUM_FILE}`: {err}"))
    })?;
    listing
        .lines()
        .find_map(|line| {
            let (digest, name) = line.split_once(char::is_whitespace)?;
            (name.trim_start().trim_start_matches('*') == ROM_FILE)
                .then(|| digest.to_ascii_lowercase())
        })
        .ok_or_else(|| {
            RomBuildError::Materialize(format!("`{CHECKSUM_FILE}` names no `{ROM_FILE}`"))
        })
}

/// Checks `rom` against the `expected` digest, so only the pinned image is
/// reported.
fn verify_checksum(expected: &str, rom: &[u8]) -> Result<(), RomBuildError> {
    let actual = rom_import::sha1(rom).to_string();
    if actual == expected {
        Ok(())
    } else {
        Err(RomBuildError::ChecksumMismatch {
            expected: expected.to_owned(),
            actual,
        })
    }
}
