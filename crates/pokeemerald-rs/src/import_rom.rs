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
//! The temporary file is `sync_all`ed before that rename, on the same
//! reasoning `engine`'s save writer spells out for save images: the rename
//! is what survives the process dying, and the sync before it is what keeps
//! a *power* loss from publishing a name whose bytes never reached the
//! disk. The destination directory is synced after the rename, best-effort,
//! so a completed import is not silently undone by the same crash — but not
//! every platform lets a directory be synced, so its failure is ignored
//! rather than reported over an import that otherwise finished.
//!
//! The one destination that is refused outright is the ROM being imported.
//! `$POKEEMERALD_PACK` can name any path, including the file the player
//! passed to `--import-rom`, and the rename would then drop the pack on
//! top of their cartridge image. That refusal is a device and inode
//! comparison against the destination directory's own handle, so a hard
//! link or a symlink spelling of the ROM is still the ROM.
//!
//! The ROM is pinned for the same reason the destination is. That refusal
//! is only as good as the two files it compares, and asking a *path* what
//! file it names answers about the moment of asking: an account that can
//! redirect a component of the ROM's path can let the comparison see a
//! harmless file and the import read the one sitting at the destination,
//! and the pack is then published over the very ROM it was built from. So
//! `--import-rom`'s file is opened once, before the comparison, and the
//! same descriptor answers both questions — `fstat` for the identity, and
//! the bytes for the pack ([`rom_import::import_pack_from_file`]). The
//! destination pin means the attacker cannot move where the pack lands;
//! the source pin means they cannot change what it was built from after
//! the guard has passed on it.
//!
//! # Why the directory is pinned
//!
//! A path is not a handle. Let `$POKEEMERALD_PACK` run through a directory
//! component another account can modify, and a check that reads the path
//! answers about the directory that component pointed at *then*: the
//! account redirects it while the pack is being built, a temporary file
//! created by path and a rename issued by path each resolve the path
//! again, and the pack lands wherever the component now points — on the
//! cartridge image itself, if the destination's file name is the ROM's.
//!
//! So the destination directory is opened once and held ([`dest::Dest`]).
//! On Unix that is a descriptor, and the ROM check, the temporary file's
//! exclusive creation, the write, and the publishing rename each name
//! their file by basename against it (`openat`/`renameat`, through
//! `rustix`; `std` exposes them on no platform). Redirecting a component
//! after the open moves nothing, because nothing after the open looks at a
//! component again. What is left trusted is the final name inside that one
//! directory, and exclusive creation covers it: a link planted there is a
//! refused import, not a write through it.
//!
//! Off Unix there is no such descriptor — `rustix` is Unix-only — so the
//! destination is still addressed by path and the window above is still
//! open there. On Windows, `$POKEEMERALD_PACK` is trusted to name a path
//! only the player controls. The default destination, their own user-data
//! directory, is one.

mod dest;

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs;
use std::io;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use rom_import::{ImportError, ImportedPack};

use dest::Dest;

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
    /// The destination directory could not be opened.
    ///
    /// The import writes through a handle on that directory rather than
    /// through its path (see the module docs), so failing to open it stops
    /// the import instead of falling back to the path.
    OpenDirFailed {
        /// The directory that could not be opened.
        path: PathBuf,
        /// The underlying I/O failure.
        source: io::Error,
    },
    /// The importer itself failed. Carries [`ImportError`] whole, so the
    /// message the player sees is the importer's own typed diagnosis: the
    /// wrong ROM, a truncated file, or an asset the profile's addresses do
    /// not reach.
    Import(ImportError),
    /// The resolved destination names no file to publish.
    ///
    /// A `$POKEEMERALD_PACK` ending in `..` (or naming a filesystem root)
    /// has no final component, so there is no name to rename the finished
    /// pack onto. One ending in a separator — `…/pack/`, `…/pack/.` — names
    /// a directory, which is not a file to publish either. Refused before
    /// anything is written; substituting a default name, or the name in
    /// front of the separator, would publish somewhere the player did not
    /// ask for `(no-silent-failure)`.
    DestinationNamesNoFile {
        /// The destination that names no file.
        pack_path: PathBuf,
    },
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
    /// The temporary file the pack is built in could not be created, or
    /// the finished pack could not be written into it.
    ///
    /// Creation is exclusive, so a name already taken lands here as
    /// [`io::ErrorKind::AlreadyExists`] — and nothing was created, so the
    /// file that holds the name is left exactly as it was found.
    TempFileFailed {
        /// The temporary file that could not be written.
        temp_path: PathBuf,
        /// The underlying I/O failure.
        source: io::Error,
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
            Self::OpenDirFailed { path, source } => {
                write!(f, "could not open `{}`: {source}", path.display())
            }
            Self::Import(source) => write!(f, "{source}"),
            Self::DestinationNamesNoFile { pack_path } => write!(
                f,
                "cannot write the asset pack to `{}`: the path names no file — point `{}` at a \
                 file",
                pack_path.display(),
                pack_format::PACK_PATH_ENV
            ),
            Self::DestinationIsSource { rom_path } => write!(
                f,
                "refusing to write the asset pack over the source ROM `{}`: point `{}` at a \
                 different file, or unset it to use the default location",
                rom_path.display(),
                pack_format::PACK_PATH_ENV
            ),
            Self::TempFileFailed { temp_path, source } => write!(
                f,
                "could not build the asset pack in `{}`: {source}",
                temp_path.display()
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
            Self::CreateDirFailed { source, .. }
            | Self::OpenDirFailed { source, .. }
            | Self::TempFileFailed { source, .. }
            | Self::PublishFailed { source, .. } => Some(source),
            Self::Import(source) => Some(source),
            Self::NoDestination
            | Self::DestinationNamesNoFile { .. }
            | Self::DestinationIsSource { .. } => None,
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
/// [`ImportRomError::CreateDirFailed`] or [`ImportRomError::OpenDirFailed`]
/// if its directory cannot be created or opened,
/// [`ImportRomError::DestinationNamesNoFile`] if it names no file,
/// [`ImportRomError::DestinationIsSource`] if that location is the ROM
/// itself, [`ImportRomError::Import`] if the ROM is not the supported build
/// or the import otherwise fails, [`ImportRomError::TempFileFailed`] if the
/// pack cannot be built in its temporary file, and
/// [`ImportRomError::PublishFailed`] if the finished pack cannot be moved
/// into place.
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
    import_to_with(rom_path, pack_path, rom_import::import_pack_from_file)
}

/// [`import_to`] with the importer injected, so the write path is testable
/// on both outcomes without a real ROM (`pack_format::path`'s pure-core
/// precedent).
///
/// The importer only builds bytes. Creating the file they go in and
/// publishing it are this module's, because both have to happen against
/// the destination directory's own handle rather than its path — see the
/// module docs.
fn import_to_with(
    rom_path: &Path,
    pack_path: &Path,
    import: impl FnOnce(&fs::File, &Path) -> Result<ImportedPack, ImportError>,
) -> Result<ImportOutcome, ImportRomError> {
    // The ROM is opened once, before anything else looks at it, and every
    // question the import asks about it is asked of this handle: whether it
    // is the file the pack would be published over, and what its bytes are.
    // See the module docs — a path answers about whichever file it named
    // when it was asked, and the source path is not this run's to trust.
    let rom = fs::File::open(rom_path).map_err(|source| {
        ImportRomError::Import(ImportError::ReadFailed {
            path: rom_path.to_path_buf(),
            source,
        })
    })?;

    let dir = pack_directory(pack_path);
    // Refused before anything is created: with no final component, or with
    // the destination spelled as a directory, there is no name to publish
    // onto that the player's own path leads back to.
    let Some(name) = pack_name(pack_path) else {
        return Err(ImportRomError::DestinationNamesNoFile {
            pack_path: pack_path.to_path_buf(),
        });
    };
    // The temporary file has to sit in the destination directory for the
    // rename to be atomic, so the directory is created before the import
    // runs rather than after it succeeds. `existed` is what lets a failed
    // run put that back.
    let existed = dir.is_dir();
    fs::create_dir_all(&dir).map_err(|source| ImportRomError::CreateDirFailed {
        path: dir.clone(),
        source,
    })?;

    // Everything from here on names files inside this one handle. A
    // directory component redirected after this open is a component
    // nothing looks at again.
    let dest = match Dest::open(&dir) {
        Ok(dest) => dest,
        Err(source) => {
            undo_created_dir(&dir, existed);
            return Err(ImportRomError::OpenDirFailed {
                path: dir.clone(),
                source,
            });
        }
    };

    // `$POKEEMERALD_PACK` can name the file the player passed to
    // `--import-rom`, and it is the *publishing rename* that would drop the
    // pack on their cartridge image: the temporary file never shares the
    // ROM's name, so no guard on that name can see this. Refuse before
    // building a pack that has nowhere safe to go.
    if dest.is_same_file_as(name, &rom, rom_path) {
        undo_created_dir(&dir, existed);
        return Err(ImportRomError::DestinationIsSource {
            rom_path: rom_path.to_path_buf(),
        });
    }

    let temp_name = temp_name();
    // Exclusive, and before the import runs: the file the pack goes in is
    // this run's own from the moment it exists, so nothing that happens
    // during the import can substitute another one for it. A name already
    // taken fails here having created nothing, which is what leaves that
    // file to whoever does own it.
    let mut file = match dest.create_new(&temp_name) {
        Ok(file) => file,
        Err(source) => {
            undo_created_dir(&dir, existed);
            return Err(ImportRomError::TempFileFailed {
                temp_path: dir.join(&temp_name),
                source,
            });
        }
    };

    let pack = match import(&rom, rom_path) {
        Ok(pack) => pack,
        Err(source) => {
            drop(file);
            dest.discard(&temp_name);
            undo_created_dir(&dir, existed);
            return Err(ImportRomError::Import(source));
        }
    };

    // A write that dies part-way leaves a prefix of a pack, and the next
    // run would pick a different name and leave this one behind, so it goes
    // with the failure. The handle is dropped before the removal because
    // Windows refuses to unlink a file that is still open.
    //
    // `sync_all` is part of the write, not an optimization after it: the
    // rename below publishes a *name*, and a power loss that lands the
    // rename without the bytes would replace a pack that worked with a
    // truncated one. Reported like any other write failure, because an
    // import that cannot get the pack onto the disk has not succeeded.
    if let Err(source) = file
        .write_all(pack.bytes())
        .and_then(|()| file.flush())
        .and_then(|()| file.sync_all())
    {
        drop(file);
        dest.discard(&temp_name);
        undo_created_dir(&dir, existed);
        return Err(ImportRomError::TempFileFailed {
            temp_path: dir.join(&temp_name),
            source,
        });
    }
    drop(file);

    dest.publish(&temp_name, name).map_err(|source| {
        dest.discard(&temp_name);
        undo_created_dir(&dir, existed);
        ImportRomError::PublishFailed {
            temp_path: dir.join(&temp_name),
            pack_path: pack_path.to_path_buf(),
            source,
        }
    })?;

    Ok(ImportOutcome {
        pack_path: pack_path.to_path_buf(),
        entry_count: pack.entry_count(),
        pack_bytes: pack.bytes().len(),
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

/// The pack file's own name inside [`pack_directory`].
///
/// Every filesystem operation the import performs names its file this way,
/// relative to the pinned directory, so the name has to survive on its own.
/// It stays an [`OsStr`] end to end: the name the player's path spells is
/// the name published, byte for byte, even off UTF-8. A path with no final
/// component (a bare `..`, a filesystem root) names no file to publish and
/// is `None` — the caller refuses it rather than inventing a name. So does
/// one spelled as a directory, for the reason [`names_a_directory`] gives.
fn pack_name(pack_path: &Path) -> Option<&OsStr> {
    if names_a_directory(pack_path) {
        return None;
    }
    pack_path.file_name()
}

/// Whether `pack_path` is spelled as a directory rather than as a file.
///
/// [`Path::file_name`] answers about *components*, and a trailing separator
/// is not one: `…/pokeemerald.pack/` and `…/pokeemerald.pack/.` both hand
/// back `pokeemerald.pack`. Publishing under that name writes a file the
/// player's own path cannot reach — every OS resolves the separator they
/// typed, and a regular file behind one is `ENOTDIR` — while replacing
/// whatever already held the name. The loader reads `$POKEEMERALD_PACK`
/// back exactly as it was set ([`pack_format::default_pack_path`]'s first
/// rung), so the import would report success over a pack the next run
/// cannot open `(no-silent-failure)`.
///
/// Separators and `.` are ASCII, and [`OsStr::to_string_lossy`] leaves
/// ASCII bytes alone, so a destination that is not UTF-8 reads correctly
/// here too.
fn names_a_directory(pack_path: &Path) -> bool {
    let text = pack_path.as_os_str().to_string_lossy();
    let mut tail = text.chars().rev();
    match tail.next() {
        // A trailing `.` is a directory spelling only after a separator:
        // `pokeemerald.pack.` is a file name that happens to end in one.
        Some('.') => tail.next().is_some_and(std::path::is_separator),
        Some(last) => std::path::is_separator(last),
        None => false,
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

/// The fixed leading part of every temporary pack name.
///
/// The dot keeps the file out of a casual directory listing while it
/// exists; the rest says who left it there, on the rare occasion a crash
/// between the create and the publish leaves one behind.
const TEMP_PREFIX: &str = ".pokeemerald-rs-import";

/// The name of the temporary file the pack is built in, beside the pack.
///
/// The file is created exclusively ([`Dest::create_new`]), so the name has
/// one job: be one nothing else already
/// holds. A process id alone is not that. It repeats across PID
/// namespaces sharing one mounted directory, it is recycled after a kill
/// that left a stale temporary file behind, and `/proc` hands it to
/// anyone on the machine — so in a pack directory another account can
/// write to, `.pokeemerald-rs-import.<pid>.tmp` is a name an attacker can
/// pre-create as a link to a file of the player's. The clock's
/// nanoseconds and a per-process counter go in with it: no pre-created
/// name matches one, and covering a second of them is a billion files.
///
/// A collision that happens anyway is a refused import naming the path,
/// never a write through someone else's link, and the next run picks a
/// different name.
///
/// What is deliberately *not* in it is the pack's own name. A 240-byte
/// basename is valid on every filesystem this ships to, and prefixing a
/// temporary name with the whole of it pushed past the 255-byte limit for
/// one component: `ENAMETOOLONG` on a name the player never typed, leaving
/// a perfectly valid destination impossible to import to. A fixed prefix
/// and three numbers is bounded whatever the pack is called, and the
/// destination is not what makes the name unique anyway.
fn temp_name() -> OsString {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);

    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    OsString::from(format!(
        "{TEMP_PREFIX}.{}.{nanos:x}.{sequence:x}.tmp",
        std::process::id()
    ))
}

#[cfg(test)]
mod tests;
