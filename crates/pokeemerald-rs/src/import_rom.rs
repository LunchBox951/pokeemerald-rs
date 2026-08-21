//! `--import-rom`: turn the player's own ROM into the runtime asset pack
//! (S-4, Discussion #71 policy C, issue #122).
//!
//! `rom_import` owns reading the cartridge and assembling the pack. This
//! module owns the two things only the shipped binary can decide: *where*
//! the pack goes, and how it gets there without a half-written file ever
//! being visible.
//!
//! # Where the pack goes
//!
//! Same precedence [`pack_format::default_pack_path`] reads it back with,
//! so an import and the next run agree:
//!
//! 1. `$POKEEMERALD_PACK` ([`pack_format::PACK_PATH_ENV`]), if set and
//!    non-empty. An explicit path wins outright.
//! 2. [`pack_format::user_pack_path`], the OS user-data directory. The
//!    directory is created if it does not exist.
//!
//! There is no third rung. The read side falls back to a portable install
//! and then to the build machine's checkout, and writing to either would
//! put a pack somewhere the player cannot find or is not allowed to touch.
//!
//! # Why the rename
//!
//! The pack is written to a temporary file *in the destination directory*
//! and renamed into place only after the import succeeds. A same-directory
//! rename is atomic on every OS this project targets, so an interrupted or
//! failed import leaves the previous pack intact rather than a truncated
//! one that would load as a corrupt pack `(no-silent-failure)`.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use rom_import::{ImportError, ImportReport};

/// What a successful import produced.
///
/// Its [`Display`](fmt::Display) is the exact one-line summary the binary
/// prints on success.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportOutcome {
    /// Where the pack was written.
    pack_path: PathBuf,
    /// How many entries it holds.
    entry_count: usize,
    /// How large it is, in bytes.
    pack_bytes: usize,
}

impl ImportOutcome {
    /// Where the pack was written.
    #[must_use]
    pub fn pack_path(&self) -> &Path {
        &self.pack_path
    }

    /// How many entries the pack holds.
    #[must_use]
    pub const fn entry_count(&self) -> usize {
        self.entry_count
    }

    /// How large the pack is, in bytes.
    #[must_use]
    pub const fn pack_bytes(&self) -> usize {
        self.pack_bytes
    }
}

impl fmt::Display for ImportOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "imported {} entries ({} bytes) to {}",
            self.entry_count,
            self.pack_bytes,
            self.pack_path.display()
        )
    }
}

/// Why `--import-rom` failed.
///
/// Concrete per-crate enum `(oop-boundaries)`; no `anyhow`. Every message
/// is one line, because the binary prints it on one terminal row.
#[derive(Debug)]
pub enum ImportRomError {
    /// No destination could be resolved: `$POKEEMERALD_PACK` is unset and
    /// the OS user-data directory could not be determined either (a
    /// scrubbed environment with no `HOME`/`APPDATA`).
    NoDestination,
    /// The destination directory could not be created.
    CreateDirFailed {
        /// The directory that could not be created.
        path: PathBuf,
        /// The underlying I/O failure.
        source: io::Error,
    },
    /// The importer itself failed. Carries [`ImportError`] whole, so the
    /// message the player sees is the importer's own typed diagnosis: the
    /// wrong ROM, a truncated file, or an asset the profile's addresses do
    /// not reach.
    Import(ImportError),
    /// The pack was built, but moving it from its temporary file to the
    /// destination failed.
    PublishFailed {
        /// The temporary file that holds the finished pack.
        temp_path: PathBuf,
        /// The destination it could not be moved to.
        pack_path: PathBuf,
        /// The underlying I/O failure.
        source: io::Error,
    },
}

impl fmt::Display for ImportRomError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDestination => write!(
                f,
                "cannot tell where to write the asset pack: no user data directory, and \
                 `{}` is not set",
                pack_format::PACK_PATH_ENV
            ),
            Self::CreateDirFailed { path, source } => {
                write!(f, "could not create `{}`: {source}", path.display())
            }
            Self::Import(source) => write!(f, "{source}"),
            Self::PublishFailed {
                temp_path,
                pack_path,
                source,
            } => write!(
                f,
                "could not publish the finished pack to `{}`: {source} (the temporary file `{}` was removed)",
                pack_path.display(),
                temp_path.display()
            ),
        }
    }
}

impl std::error::Error for ImportRomError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        // Every variant is spelled out, so a future source-carrying variant
        // fails to compile here instead of reporting an empty cause chain.
        match self {
            Self::CreateDirFailed { source, .. } | Self::PublishFailed { source, .. } => {
                Some(source)
            }
            Self::Import(source) => Some(source),
            Self::NoDestination => None,
        }
    }
}

/// Import the ROM at `rom_path` into the runtime asset pack.
///
/// Resolves the destination (see the module docs), creates its directory,
/// and writes the pack atomically.
///
/// # Errors
///
/// [`ImportRomError::NoDestination`] if no pack location can be resolved,
/// [`ImportRomError::CreateDirFailed`] if its directory cannot be created,
/// [`ImportRomError::Import`] if the ROM is not the supported build or the
/// import otherwise fails, and [`ImportRomError::PublishFailed`] if the
/// finished pack cannot be moved into place.
pub fn import_rom(rom_path: &Path) -> Result<ImportOutcome, ImportRomError> {
    let pack_path = destination()?;
    import_to(rom_path, &pack_path)
}

/// The pack's destination for this run. See the module docs for the order.
fn destination() -> Result<PathBuf, ImportRomError> {
    if let Some(value) = std::env::var_os(pack_format::PACK_PATH_ENV) {
        if !value.is_empty() {
            return Ok(PathBuf::from(value));
        }
    }
    pack_format::user_pack_path().ok_or(ImportRomError::NoDestination)
}

/// [`import_rom`]'s destination-agnostic core, so tests write into a
/// temporary directory instead of the developer's real data directory.
fn import_to(rom_path: &Path, pack_path: &Path) -> Result<ImportOutcome, ImportRomError> {
    import_to_with(rom_path, pack_path, rom_import::import)
}

/// [`import_to`] with the importer injected, so the write path is testable
/// on both outcomes without a real ROM (`pack_format::path`'s pure-core
/// precedent).
fn import_to_with(
    rom_path: &Path,
    pack_path: &Path,
    import: impl FnOnce(&Path, &Path) -> Result<ImportReport, ImportError>,
) -> Result<ImportOutcome, ImportRomError> {
    let dir = pack_directory(pack_path);
    // The temporary file has to sit in the destination directory for the
    // rename to be atomic, so the directory is created before the import
    // runs rather than after it succeeds. `existed` is what lets a failed
    // run put that back.
    let existed = dir.is_dir();
    fs::create_dir_all(&dir).map_err(|source| ImportRomError::CreateDirFailed {
        path: dir.clone(),
        source,
    })?;

    let temp_path = temp_path(pack_path, &dir);
    let report = match import(rom_path, &temp_path) {
        Ok(report) => report,
        Err(source) => {
            // A failed import may have written part of a pack. Nothing
            // downstream should ever see it, and the file has no other
            // owner, so drop it here.
            let _ = fs::remove_file(&temp_path);
            undo_created_dir(&dir, existed);
            return Err(ImportRomError::Import(source));
        }
    };

    fs::rename(&temp_path, pack_path).map_err(|source| {
        let _ = fs::remove_file(&temp_path);
        undo_created_dir(&dir, existed);
        ImportRomError::PublishFailed {
            temp_path: temp_path.clone(),
            pack_path: pack_path.to_path_buf(),
            source,
        }
    })?;

    Ok(ImportOutcome {
        pack_path: pack_path.to_path_buf(),
        entry_count: report.entry_count(),
        pack_bytes: report.pack_bytes(),
    })
}

/// The directory the pack file lives in.
///
/// A bare file name has no parent, and a parent of `""` is not a directory
/// any OS accepts, so both become the current directory.
fn pack_directory(pack_path: &Path) -> PathBuf {
    match pack_path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

/// Remove the destination directory this run created, if it created one.
///
/// A failed import should leave the filesystem as it found it: an empty
/// `pokeemerald-rs` directory in the user's data directory is litter that
/// looks like a half-installed game. Non-recursive on purpose, so it can
/// only ever remove a directory this run created and left empty; anything
/// else fails harmlessly.
fn undo_created_dir(dir: &Path, existed: bool) {
    if !existed {
        let _ = fs::remove_dir(dir);
    }
}

/// The temporary file the pack is built in, beside its destination.
///
/// The process id keeps two concurrent imports from writing the same
/// temporary file; the leading dot keeps it out of a casual directory
/// listing while it exists.
fn temp_path(pack_path: &Path, dir: &Path) -> PathBuf {
    let name = pack_path.file_name().map_or_else(
        || "pokeemerald.pack".to_owned(),
        |n| n.to_string_lossy().into_owned(),
    );
    dir.join(format!(".{name}.{}.tmp", std::process::id()))
}

#[cfg(test)]
mod tests;
