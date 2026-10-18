//! `rom-build`: materialise the pinned upstream checkout in isolated
//! temporary storage, run its own build, and report the compiled outputs.
//!
//! Developer-only. The upstream `Makefile` links the ELF with `-Map`, then its
//! `rom` rule runs `objcopy -O binary` to emit the cartridge image. This module
//! drives that build in a scratch clone so the reference tree is never written,
//! and fails closed on an unverifiable pin, a missing tool, a failed build, or
//! missing or empty outputs. The image is returned in memory as a
//! [`CompiledArtifact`]; it is never written into the repository.

mod artifact;
mod checkout;
#[cfg(test)]
mod tests;

use std::ffi::OsString;
use std::fmt;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub use artifact::{CompiledArtifact, Provenance};

use crate::gen_rom_profile::SymbolMap;

/// The cartridge image the upstream `rom` rule writes at the tree root.
const ROM_FILE: &str = "pokeemerald.gba";
/// The linker map the upstream ELF rule writes at the tree root.
const MAP_FILE: &str = "pokeemerald.map";
/// A GBA cartridge address space is 32 MiB.
const MAX_ROM_BYTES: usize = 32 * 1024 * 1024;
/// Tools the upstream build resolves from `PATH`.
const REQUIRED_TOOLS: [&str; 6] = [
    "make",
    "gcc",
    "perl",
    "arm-none-eabi-as",
    "arm-none-eabi-ld",
    "arm-none-eabi-objcopy",
];
/// Environment the build inherits; everything else is dropped.
const INHERITED_ENV: [&str; 4] = ["PATH", "HOME", "DEVKITARM", "DEVKITPRO"];
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
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
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

/// Verifies the reference, builds the pinned tree in scratch storage, and
/// returns its compiled outputs.
///
/// # Errors
///
/// A [`RomBuildError`] naming the first failed stage; nothing is returned
/// from a partial build.
pub fn build(config: &BuildConfig) -> Result<CompiledArtifact, RomBuildError> {
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
    copy_dir(&config.agbcc_source, &tree.join(AGBCC_DEST))
        .map_err(|err| RomBuildError::Materialize(format!("cannot provision agbcc: {err}")))?;
    let build_command = run_make(config, &tree)?;
    let rom = read_rom(&tree)?;
    let map_path = tree.join(MAP_FILE);
    if !map_path.is_file() {
        return Err(RomBuildError::MissingOutput(MAP_FILE.to_owned()));
    }
    let symbols = SymbolMap::load(&map_path).map_err(|err| RomBuildError::InvalidOutput {
        file: MAP_FILE,
        reason: err.to_string(),
    })?;
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

/// Runs the upstream build in `tree` with a scrubbed environment and returns
/// the command line it ran.
fn run_make(config: &BuildConfig, tree: &Path) -> Result<String, RomBuildError> {
    let jobs = format!("-j{}", config.jobs.max(1));
    let mut command = Command::new(&config.make);
    command
        .arg(&jobs)
        .current_dir(tree)
        .env_clear()
        .stdin(Stdio::null());
    for name in INHERITED_ENV {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    let rendered = format!("{} {jobs}", config.make.to_string_lossy());
    let output = command.output().map_err(|err| {
        if err.kind() == ErrorKind::NotFound {
            RomBuildError::MissingTool(config.make.to_string_lossy().into_owned())
        } else {
            RomBuildError::BuildFailed {
                reason: format!("could not start `{rendered}`: {err}"),
            }
        }
    })?;
    if output.status.success() {
        return Ok(rendered);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let tail_start = stderr
        .char_indices()
        .rev()
        .nth(STDERR_TAIL_BYTES)
        .map_or(0, |(i, _)| i);
    Err(RomBuildError::BuildFailed {
        reason: format!(
            "`{rendered}` exited with {}: {}",
            output.status,
            &stderr[tail_start..]
        ),
    })
}

fn read_rom(tree: &Path) -> Result<Vec<u8>, RomBuildError> {
    let rom = match std::fs::read(tree.join(ROM_FILE)) {
        Ok(rom) => rom,
        Err(err) if err.kind() == ErrorKind::NotFound => {
            return Err(RomBuildError::MissingOutput(ROM_FILE.to_owned()));
        }
        Err(err) => {
            return Err(RomBuildError::InvalidOutput {
                file: ROM_FILE,
                reason: err.to_string(),
            });
        }
    };
    if rom.is_empty() || rom.len() > MAX_ROM_BYTES {
        return Err(RomBuildError::InvalidOutput {
            file: ROM_FILE,
            reason: format!("{} bytes is not a plausible cartridge image", rom.len()),
        });
    }
    Ok(rom)
}
