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
//!
//! The one destination that is refused outright is the ROM being imported.
//! `$POKEEMERALD_PACK` can name any path, including the file the player
//! passed to `--import-rom`, and the rename would then drop the pack on
//! top of their cartridge image. [`rom_import::overwrites_rom`] answers
//! that before the directory is created.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

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
    /// The resolved destination *is* the ROM being imported.
    ///
    /// Publishing renames the finished pack over the destination, so a
    /// `$POKEEMERALD_PACK` pointing at the file passed to `--import-rom`
    /// would replace the player's cartridge image with a pack. Refused
    /// before anything is written `(no-silent-failure)`.
    DestinationIsSource {
        /// The ROM that would have been replaced.
        rom_path: PathBuf,
    },
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
            Self::DestinationIsSource { rom_path } => write!(
                f,
                "refusing to write the asset pack over the source ROM `{}`: point `{}` at a \
                 different file, or unset it to use the default location",
                rom_path.display(),
                pack_format::PACK_PATH_ENV
            ),
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
            Self::NoDestination | Self::DestinationIsSource { .. } => None,
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
/// [`ImportRomError::DestinationIsSource`] if that location is the ROM
/// itself, [`ImportRomError::CreateDirFailed`] if its directory cannot be
/// created,
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
    // The temporary file never shares the ROM's name, so `rom_import`'s own
    // guard cannot see this one: it is the *rename* that would drop the pack
    // on top of the cartridge image. Refuse before the directory is touched,
    // rather than build a pack that has nowhere safe to go.
    if rom_import::overwrites_rom(rom_path, pack_path) {
        return Err(ImportRomError::DestinationIsSource {
            rom_path: rom_path.to_path_buf(),
        });
    }

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
            // downstream should ever see it, and the file is this run's
            // own, so drop it here. The exception is an import that failed
            // *because* the name was already taken: what sits there is
            // then somebody else's, and this run does not delete files it
            // did not create.
            if !name_was_taken(&source) {
                let _ = fs::remove_file(&temp_path);
            }
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

/// Whether an import failed because its destination name was taken.
///
/// The importer creates the temporary file exclusively, so this is the one
/// failure whose path names a file this run did not create: a leftover
/// from a killed run, or a link planted by another account in a writable
/// pack directory. Neither is this run's to remove.
fn name_was_taken(error: &ImportError) -> bool {
    matches!(
        error,
        ImportError::WriteFailed { source, .. } if source.kind() == io::ErrorKind::AlreadyExists
    )
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
/// The importer creates this file exclusively (`rom_import`'s
/// `write_new`), so the name has one job: be one nothing else already
/// holds. A process id alone is not that. It repeats across PID
/// namespaces sharing one mounted directory, it is recycled after a kill
/// that left a stale temporary file behind, and `/proc` hands it to
/// anyone on the machine — so in a pack directory another account can
/// write to, `.pokeemerald.pack.<pid>.tmp` is a name an attacker can
/// pre-create as a link to a file of the player's. The clock's
/// nanoseconds and a per-process counter go in with it: no pre-created
/// name matches one, and covering a second of them is a billion files.
///
/// A collision that happens anyway is a refused import naming the path,
/// never a write through someone else's link, and the next run picks a
/// different name. The leading dot keeps the file out of a casual
/// directory listing while it exists.
fn temp_path(pack_path: &Path, dir: &Path) -> PathBuf {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);

    let name = pack_path.file_name().map_or_else(
        || "pokeemerald.pack".to_owned(),
        |n| n.to_string_lossy().into_owned(),
    );
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    dir.join(format!(
        ".{name}.{}.{nanos:x}.{sequence:x}.tmp",
        std::process::id()
    ))
}

#[cfg(test)]
mod tests;
