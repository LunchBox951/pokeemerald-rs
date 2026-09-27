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
//! one that would load as a corrupt pack.
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
//! Syncing the destination persists the pack's name *inside* it, never the
//! destination's own name in the level above. So a directory this run had
//! to create is one more entry starting life only in the page cache — and
//! it is the entry the pack hangs from, which a crash would take the whole
//! import with. Every level created is therefore made durable through its
//! parent, outermost first, on the same best-effort terms. The first import
//! on a machine is the one that creates the data directory, so this is the
//! ordinary path rather than a corner of it.
//!
//! Two destinations are refused outright before anything is built.
//!
//! The first is a destination that already exists as a directory --
//! checked before the ROM is read or a temporary file is built (see
//! [`Dest::name_is_directory`]).
//!
//! The second is the ROM being imported. `$POKEEMERALD_PACK` can name any
//! path, including the file the player passed to `--import-rom`, and the
//! rename would then drop the pack on top of their cartridge image. That
//! refusal is a device and inode comparison against the destination
//! directory's own handle, so a hard link or a symlink spelling of the ROM
//! is still the ROM.
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
//! On Unix that is a descriptor, and the directory-type check, the ROM
//! check, the temporary file's exclusive creation, the write, and the
//! publishing rename each name their file by basename against it
//! (`openat`/`renameat`, through `rustix`; `std` exposes them on no
//! platform). Redirecting a component
//! after the open moves nothing, because nothing after the open looks at a
//! component again. What is left trusted is the final name inside that one
//! directory, and exclusive creation covers the write: a link planted
//! there is a refused import, not a write through it. An account that can
//! write the directory itself can still swap entries between creation and
//! the rename — that account can equally replace the published pack
//! outright, so the guarantee held here is only that no write ever lands
//! *through* a planted link, never that a hostile directory yields a
//! trustworthy pack.
//!
//! Off Unix there is no such descriptor — `rustix` is Unix-only — so the
//! destination is still addressed by path and the window above is still
//! open there. On Windows, `$POKEEMERALD_PACK` is trusted to name a path
//! only the player controls. The default destination, their own user-data
//! directory, is one.
//!
//! The directory levels [`create_directories`] has to make on the way to
//! the destination are pinned the same way, on Unix, and for the same
//! reason: a failed import's cleanup runs an arbitrary interval after they
//! were created, long enough for another account to rename one away and
//! put an unrelated directory at the same name. [`undo_created_directories`]
//! removes a level only relative to the parent handle it was created
//! through, and only once that parent's own lookup of the name still
//! identifies the directory this run made (`fstat`'s device and inode,
//! checked immediately before `unlinkat`). That narrows the race to the gap
//! between the check and the removal itself, which nothing portable closes
//! further; anything caught earlier is refused, and a name that resolves to
//! something else at check time is left standing as harmless litter. Off
//! Unix, created levels are still addressed by path, like the destination
//! itself.
//!
//! That identity comes from a lookup (`statat`) right after `mkdirat`
//! succeeds, not a descriptor `mkdirat` never hands back
//! ([`dest::open_created_directory_at`] owns the reopen that follows,
//! including why it refuses a symlink there). The reopened descriptor's own
//! identity must match that lookup before the descent continues -- a
//! mismatch means the name was swapped for a replacement in the gap between
//! the two calls, so this run stops there instead of creating the next
//! level inside it, leaving the replacement unrecorded and untouched. Only
//! a level whose reopen and identity check both agree gets its descriptor
//! held open until cleanup runs, pinning its inode so a later directory
//! reusing the same freed inode number is not mistaken for this one; a
//! level whose reopen failed is recorded with no pin, so cleanup leaves it
//! standing rather than trust identity alone, and a level whose reopened
//! identity did not match is not recorded at all.
//!
//! One gap stays open regardless: a directory can be swapped for another
//! between `mkdirat`'s own success and the `statat` that reads its
//! identity, and nothing catches that one, because no portable call creates
//! a directory and hands back a descriptor to what it made in the same
//! step. A replacement landed there is recorded as if this run had made it,
//! and cleanup could later remove it.
//!
//! Pinning costs one descriptor per level, so an unusually deep chain of
//! missing levels could exhaust the process's descriptor table before it
//! runs out of levels to create. Past [`MAX_PINNED_LEVELS`], only the
//! innermost that many stay pinned; the rest still descend through a
//! transient reopen but keep no descriptor for cleanup to verify by, so
//! cleanup leaves them standing -- litter, on the same terms as any level
//! it cannot confirm.

mod dest;

use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::io;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use rom_import::{ImportError, ImportedPack, OneLinePath};

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
            OneLinePath(&self.pack_path)
        )
    }
}

/// Why `--import-rom` failed.
///
/// Concrete per-crate enum `(oop-boundaries)`; no `anyhow`. Every message
/// is one line, because the binary prints it on one terminal row. The paths
/// they quote come from `$POKEEMERALD_PACK` and from `--import-rom`, so they
/// render through [`OneLinePath`] to keep that row intact.
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
    Import {
        /// The importer's own diagnosis.
        source: ImportError,
        /// The temporary file the import had already created when it
        /// failed, if any. `None` before the ROM's own open succeeds --
        /// nothing has been created yet at that point (module docs:
        /// "everything from here on names files inside this one handle").
        temp_path: Option<PathBuf>,
        /// Whether the cleanup that followed actually removed
        /// `temp_path` -- also true when there was nothing to remove.
        /// [`Dest::discard`] swallows its own failure so it cannot
        /// displace this diagnosis, which means a partial file can still
        /// be there.
        temp_removed: bool,
    },
    /// The resolved destination names no file to publish.
    ///
    /// A `$POKEEMERALD_PACK` ending in `..` (or naming a filesystem root)
    /// has no final component, so there is no name to rename the finished
    /// pack onto. One ending in a separator — `…/pack/`, `…/pack/.` — names
    /// a directory, which is not a file to publish either. Refused before
    /// anything is written; substituting a default name, or the name in
    /// front of the separator, would publish somewhere the player did not
    /// ask for.
    DestinationNamesNoFile {
        /// The destination that names no file.
        pack_path: PathBuf,
    },
    /// The resolved destination already exists, and it is a directory.
    ///
    /// Unlike [`Self::DestinationNamesNoFile`], the path itself names a
    /// file — no trailing separator, a real final component — but that
    /// name is already occupied by a directory. Refused before anything
    /// is written; see [`Dest::name_is_directory`] for how and why.
    DestinationIsDirectory {
        /// The destination occupied by an existing directory.
        pack_path: PathBuf,
    },
    /// The resolved destination *is* the ROM being imported.
    ///
    /// Publishing renames the finished pack over the destination, so a
    /// `$POKEEMERALD_PACK` pointing at the file passed to `--import-rom`
    /// would replace the player's cartridge image with a pack. Refused
    /// before anything is written.
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
        /// Whether the cleanup that followed actually removed
        /// `temp_path` -- also true when nothing was ever created (temp-
        /// file creation itself is what failed). [`Dest::discard`]
        /// swallows its own failure so it cannot displace this
        /// diagnosis, which means a partial file can still be there.
        temp_removed: bool,
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
        /// Whether the cleanup that followed actually removed
        /// `temp_path`. [`Dest::discard`] swallows its own failure so it
        /// cannot displace this diagnosis, which means the file can still
        /// be there — and it holds a *finished* pack, so the player is
        /// owed the truth about it rather than told it is gone.
        temp_removed: bool,
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
                write!(f, "could not create `{}`: {source}", OneLinePath(path))
            }
            Self::OpenDirFailed { path, source } => {
                write!(f, "could not open `{}`: {source}", OneLinePath(path))
            }
            Self::Import {
                source,
                temp_path,
                temp_removed,
            } => {
                write!(f, "{source}")?;
                match temp_path {
                    Some(temp_path) if !*temp_removed => write!(
                        f,
                        " (the partial file `{}` could not be removed and is still there)",
                        OneLinePath(temp_path)
                    ),
                    _ => Ok(()),
                }
            }
            Self::DestinationNamesNoFile { pack_path } => write!(
                f,
                "cannot write the asset pack to `{}`: the path names no file — point `{}` at a \
                 file",
                OneLinePath(pack_path),
                pack_format::PACK_PATH_ENV
            ),
            Self::DestinationIsDirectory { pack_path } => write!(
                f,
                "cannot write the asset pack to `{}`: a directory is already there — point `{}` \
                 at a file",
                OneLinePath(pack_path),
                pack_format::PACK_PATH_ENV
            ),
            Self::DestinationIsSource { rom_path } => write!(
                f,
                "refusing to write the asset pack over the source ROM `{}`: point `{}` at a \
                 different file, or unset it to use the default location",
                OneLinePath(rom_path),
                pack_format::PACK_PATH_ENV
            ),
            Self::TempFileFailed {
                temp_path,
                source,
                temp_removed,
            } => {
                write!(
                    f,
                    "could not build the asset pack in `{}`: {source}",
                    OneLinePath(temp_path)
                )?;
                if *temp_removed {
                    Ok(())
                } else {
                    write!(
                        f,
                        " (that file could not be removed either and is still there)"
                    )
                }
            }
            Self::PublishFailed {
                temp_path,
                pack_path,
                source,
                temp_removed,
            } => {
                write!(
                    f,
                    "could not publish the finished pack to `{}`: {source}",
                    OneLinePath(pack_path)
                )?;
                if *temp_removed {
                    write!(
                        f,
                        " (the temporary file `{}` was removed)",
                        OneLinePath(temp_path)
                    )
                } else {
                    // The pack itself is finished and synced, so this is not
                    // just litter to apologize for -- it is the import's
                    // whole product, and naming it is what lets the player
                    // move it into place or delete it themselves.
                    write!(
                        f,
                        " (the finished pack is still at `{}`; move it to the destination or \
                         delete it yourself)",
                        OneLinePath(temp_path)
                    )
                }
            }
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
            Self::Import { source, .. } => Some(source),
            Self::NoDestination
            | Self::DestinationNamesNoFile { .. }
            | Self::DestinationIsDirectory { .. }
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
/// [`ImportRomError::DestinationIsDirectory`] if a directory already
/// exists there, [`ImportRomError::DestinationIsSource`] if that location
/// is the ROM itself, [`ImportRomError::Import`] if the ROM is not the
/// supported build or the import otherwise fails,
/// [`ImportRomError::TempFileFailed`] if the pack cannot be built in its
/// temporary file, and [`ImportRomError::PublishFailed`] if the finished
/// pack cannot be moved into place.
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
    let rom = fs::File::open(rom_path).map_err(|source| ImportRomError::Import {
        source: ImportError::ReadFailed {
            path: rom_path.to_path_buf(),
            source,
        },
        temp_path: None,
        temp_removed: true,
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
    // runs rather than after it succeeds. The levels this run made answer
    // both of the questions that follow: which entries a successful run has
    // to leave durable, and which directories a failed one takes back.
    let created = match create_directories(&dir) {
        Ok(created) => created,
        Err((created, source)) => {
            // The creation can fail after making outer levels; take those
            // back too, not just the levels of a fully created destination.
            undo_created_directories(&created);
            return Err(ImportRomError::CreateDirFailed {
                path: dir.clone(),
                source,
            });
        }
    };
    sync_created_directories(&created);

    // Everything from here on names files inside this one handle. A
    // directory component redirected after this open is a component
    // nothing looks at again.
    let dest = match Dest::open(&dir) {
        Ok(dest) => dest,
        Err(source) => {
            undo_created_directories(&created);
            return Err(ImportRomError::OpenDirFailed {
                path: dir.clone(),
                source,
            });
        }
    };

    if let Err(err) = refuse_existing_directory(&dest, name, pack_path) {
        undo_created_directories(&created);
        return Err(err);
    }

    // `$POKEEMERALD_PACK` can name the file the player passed to
    // `--import-rom`, and it is the *publishing rename* that would drop the
    // pack on their cartridge image: the temporary file never shares the
    // ROM's name, so no guard on that name can see this. Refuse before
    // building a pack that has nowhere safe to go.
    if dest.is_same_file_as(name, &rom, rom_path) {
        undo_created_directories(&created);
        return Err(ImportRomError::DestinationIsSource {
            rom_path: rom_path.to_path_buf(),
        });
    }

    let temp_name = dest.temp_name();
    // Exclusive, and before the import runs: every byte is written through
    // this one handle, so nothing that happens during the import can make
    // the write land through another file. The directory entry itself is
    // only as trustworthy as the directory (see the module docs). A name
    // already taken fails here having created nothing, which is what
    // leaves that file to whoever does own it.
    let mut file = match dest.create_new(&temp_name) {
        Ok(file) => file,
        Err(source) => {
            undo_created_directories(&created);
            return Err(ImportRomError::TempFileFailed {
                temp_path: dir.join(&temp_name),
                source,
                temp_removed: true,
            });
        }
    };

    let pack = match import(&rom, rom_path) {
        Ok(pack) => pack,
        Err(source) => {
            drop(file);
            let temp_removed = dest.discard(&temp_name);
            undo_created_directories(&created);
            return Err(ImportRomError::Import {
                source,
                temp_path: Some(dir.join(&temp_name)),
                temp_removed,
            });
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
        let temp_removed = dest.discard(&temp_name);
        undo_created_directories(&created);
        return Err(ImportRomError::TempFileFailed {
            temp_path: dir.join(&temp_name),
            source,
            temp_removed,
        });
    }
    drop(file);

    dest.publish(&temp_name, name).map_err(|source| {
        let temp_removed = dest.discard(&temp_name);
        undo_created_directories(&created);
        ImportRomError::PublishFailed {
            temp_removed,
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
/// cannot open.
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

/// Refuse `name` if it already exists inside `dest` as a directory.
///
/// See [`Dest::name_is_directory`] for what counts and why.
fn refuse_existing_directory(
    dest: &Dest,
    name: &OsStr,
    pack_path: &Path,
) -> Result<(), ImportRomError> {
    if dest.name_is_directory(name) {
        Err(ImportRomError::DestinationIsDirectory {
            pack_path: pack_path.to_path_buf(),
        })
    } else {
        Ok(())
    }
}

/// The levels `dir` is missing, outermost first: the ones
/// [`create_directories`] has to try. Empty for a `dir` that is already a
/// directory, which is a run with nothing to create.
///
/// A component that exists but is *not* a directory is listed like a
/// missing one. Creating it then fails on it, and that failure carries the
/// levels already made, so distinguishing the two here would buy nothing.
fn directories_to_create(dir: &Path) -> Vec<PathBuf> {
    let mut missing = Vec::new();
    let mut current = Some(dir);
    while let Some(path) = current.filter(|path| !path.as_os_str().is_empty()) {
        if path.is_dir() {
            break;
        }
        missing.push(path.to_path_buf());
        current = path.parent();
    }
    missing.reverse();
    missing
}

/// A directory level [`create_directories`] made, and what
/// [`sync_created_directories`] and [`undo_created_directories`] use to find
/// it again. See the module docs for why cleanup needs the parent handle,
/// basename, identity, and pinned descriptor kept here on Unix; off Unix
/// there is no descriptor to pin (`rustix` is Unix-only), so this stays the
/// path alone, re-resolved each time.
#[derive(Debug)]
#[cfg(unix)]
struct CreatedDirectory {
    /// Where this level was created. Kept so both platform arms expose the
    /// same field for `tests::created_paths` to compare; removal never
    /// re-resolves it, which is the whole point.
    #[allow(
        dead_code,
        reason = "read only by tests::created_paths, not by production Unix code"
    )]
    path: PathBuf,
    /// The directory this level was created in. Shared, not duplicated,
    /// with the level above's own [`CreatedDirectory::own`] when that level
    /// was created by this run too, so a deep chain costs one descriptor per
    /// level rather than two. `None` past [`MAX_PINNED_LEVELS`]: this level
    /// was never given a descriptor to verify by, so cleanup skips it
    /// without stopping the levels above it.
    parent: Option<std::rc::Rc<std::os::fd::OwnedFd>>,
    /// This level's own basename inside `parent`.
    name: std::ffi::OsString,
    /// This level's own device and inode, captured right after it was
    /// created. Kept as `rustix`'s own platform-native [`rustix::fs::Stat`],
    /// not normalized into a fixed-width type: `st_dev`'s width and
    /// signedness differ across Unixes this ships to (`i32` on macOS, `u64`
    /// on Linux).
    identity: rustix::fs::Stat,
    /// This level itself, held open until cleanup runs; see the module docs
    /// for what the pin captures and why. `None` when the reopen right
    /// after `mkdirat` failed, or this level sits past [`MAX_PINNED_LEVELS`];
    /// either way, cleanup leaves it standing.
    own: Option<std::rc::Rc<std::os::fd::OwnedFd>>,
}

/// [`CreatedDirectory`]'s off-Unix shape: see its own docs for why.
#[derive(Debug)]
#[cfg(not(unix))]
struct CreatedDirectory {
    /// Where this level was created.
    path: PathBuf,
}

/// Whether `a` and `b` name the same file: the same device and inode,
/// `create_directories_with_hooks`'s and [`undo_created_directories`]'s
/// shared test for "is this still the directory this run made".
#[cfg(unix)]
fn same_directory_identity(a: &rustix::fs::Stat, b: &rustix::fs::Stat) -> bool {
    a.st_dev == b.st_dev && a.st_ino == b.st_ino
}

/// The most levels [`create_directories_with_hooks`] keeps a descriptor
/// pinned for at once; see the module docs for why. A destination this many
/// levels deep or shallower is unaffected -- every level still pins, exactly
/// as before this existed. Real fd pressure can still force fewer than this
/// many pinned; see [`shed_outermost_pinned_level`].
#[cfg(unix)]
const MAX_PINNED_LEVELS: usize = 32;

/// Whether `error` is the OS refusing one more descriptor (`EMFILE` or
/// `ENFILE`) -- the two [`shed_outermost_pinned_level`] exists to answer by
/// giving one back rather than failing the whole create.
#[cfg(unix)]
fn is_out_of_descriptors(error: rustix::io::Errno) -> bool {
    error == rustix::io::Errno::MFILE || error == rustix::io::Errno::NFILE
}

/// [`is_out_of_descriptors`] for an already-converted [`io::Error`], the
/// shape a reopen's own seam hands back.
#[cfg(unix)]
fn is_out_of_descriptors_io(error: &io::Error) -> bool {
    rustix::io::Errno::from_io_error(error).is_some_and(is_out_of_descriptors)
}

/// Releases the outermost level still holding a pinned descriptor (`own`,
/// `parent`, or both), if any, so a caller that just hit
/// [`is_out_of_descriptors`] can retry with one fewer descriptor in use.
///
/// `pin_from` is a cursor: everything before it already holds nothing, so
/// this walks forward from it, one record at a time, until it finds one
/// that still does, clears both fields there, and leaves the cursor just
/// past it. Repeated calls therefore visit every level in turn rather than
/// skipping every other one.
///
/// Clearing a record's own `own` does not by itself close its descriptor --
/// the level just inside it still needs that same descriptor as its own
/// `parent` -- but a call landing on *that* inner record clears its
/// `parent` too, and by then nothing before it holds a reference either.
/// Each call fully releases the descriptor the call before it only halved.
#[cfg(unix)]
fn shed_outermost_pinned_level(created: &mut [CreatedDirectory], pin_from: &mut usize) -> bool {
    while let Some(level) = created.get_mut(*pin_from) {
        if level.own.is_none() && level.parent.is_none() {
            *pin_from += 1;
            continue;
        }
        level.own = None;
        level.parent = None;
        *pin_from += 1;
        return true;
    }
    false
}

/// Create the levels `dir` is missing, outermost first, and answer with the
/// ones this run made — the levels [`sync_created_directories`] persists
/// and [`undo_created_directories`] may take back.
///
/// Ownership is what the create reports, never what a look beforehand
/// predicted. [`fs::create_dir_all`] says only whether the destination
/// exists afterwards, so pairing it with an earlier [`directories_to_create`]
/// claims levels another process created in between — and a failed import
/// would then remove a directory that process is about to write into.
/// Creating one level at a time asks the question of the syscall instead:
/// an existing level is somebody else's, and only a create that succeeded
/// is recorded.
///
/// A failure hands back the levels made before it, which are this run's to
/// take back like any other.
///
/// On Unix, every level is both created and recorded by descriptor: the
/// parent of the outermost missing level is the one path this resolves --
/// it already exists, which is why [`directories_to_create`]'s own walk
/// stopped there -- and every level after it chains through
/// [`dest::open_directory_at`] or [`dest::open_created_directory_at`]
/// instead of a path re-resolved past that first parent.
///
/// [`dest::open_traversal_directory`] documents, per platform, whether
/// opening a directory to hold as `mkdirat`'s target needs more than the
/// write and search a plain `mkdir` already needed. A level this run
/// creates is unaffected either way: it is made at the ordinary default
/// mode, not reopened read-restricted.
#[cfg(unix)]
fn create_directories(
    dir: &Path,
) -> Result<Vec<CreatedDirectory>, (Vec<CreatedDirectory>, io::Error)> {
    create_directories_with_hooks(dir, &mut || {}, &mut || None)
}

/// What reopening a level [`create_directories_with_hooks`] just made turned
/// up, checked against its `statat` identity; see the module docs for why.
#[cfg(unix)]
enum ReopenedLevel {
    /// Matches `identity`; safe to descend into.
    Pinned(std::rc::Rc<std::os::fd::OwnedFd>),
    /// The reopen itself failed; recorded with no pin.
    Unpinned(io::Error),
    /// The reopen's own identity did not match, or could not be read.
    /// Either way, not provably the level `mkdirat` made -- left unrecorded.
    Unverified(io::Error),
}

/// Reopens the level [`create_directories_with_hooks`] just created at
/// `name` in `parent` and checks it against `identity`. `before_reopen` is
/// the test seam [`create_directories_with_hooks`] documents.
///
/// A reopen refused with [`is_out_of_descriptors`] retries after
/// [`shed_outermost_pinned_level`] frees one, as long as there is one left
/// to free; only once none is does the refusal stand.
#[cfg(unix)]
fn reopen_created_level(
    created: &mut [CreatedDirectory],
    pin_from: &mut usize,
    parent: &std::rc::Rc<std::os::fd::OwnedFd>,
    name: &OsStr,
    identity: &rustix::fs::Stat,
    before_reopen: &mut dyn FnMut() -> Option<io::Error>,
) -> ReopenedLevel {
    let descend = loop {
        let attempt = match before_reopen() {
            Some(err) => Err(err),
            None => dest::open_created_directory_at(parent, name).map(std::rc::Rc::new),
        };
        match attempt {
            Err(source)
                if is_out_of_descriptors_io(&source)
                    && shed_outermost_pinned_level(created, pin_from) => {}
            result => break result,
        }
    };
    let descend = match descend {
        Ok(fd) => fd,
        Err(source) => return ReopenedLevel::Unpinned(source),
    };
    match rustix::fs::fstat(&*descend) {
        Ok(reopened) if same_directory_identity(&reopened, identity) => {
            ReopenedLevel::Pinned(descend)
        }
        Ok(_) => ReopenedLevel::Unverified(io::Error::other(
            "directory level was replaced before it could be reopened",
        )),
        Err(source) => ReopenedLevel::Unverified(source.into()),
    }
}

/// Finishes a level [`create_directories_with_hooks`] just `mkdirat`ed at
/// `name`, inside `parent_fd`: captures its identity (retrying through
/// [`shed_outermost_pinned_level`] on [`is_out_of_descriptors`], like the
/// `mkdirat` before it), reopens and records it through
/// [`reopen_created_level`], and answers with the descriptor the next level
/// descends through.
#[cfg(unix)]
fn create_missing_level(
    created: &mut Vec<CreatedDirectory>,
    pin_from: &mut usize,
    parent_fd: std::rc::Rc<std::os::fd::OwnedFd>,
    path: PathBuf,
    name: std::ffi::OsString,
    before_reopen: &mut dyn FnMut() -> Option<io::Error>,
) -> Result<std::rc::Rc<std::os::fd::OwnedFd>, io::Error> {
    // The identity comes from a lookup, not from a descriptor `mkdirat`
    // never hands back -- and it is captured whether or not the reopen just
    // below succeeds, so a mundane failure there (too many open files)
    // cannot un-record a level that really was made and leave it stuck
    // forever. `mkdirat` just said this name is a fresh directory, so
    // anything other than one sitting there the instant this looks again is
    // somebody else's swap; refused the same way, without following it as a
    // symlink might.
    let found = loop {
        match rustix::fs::statat(&*parent_fd, &name, rustix::fs::AtFlags::SYMLINK_NOFOLLOW) {
            Err(err)
                if is_out_of_descriptors(err) && shed_outermost_pinned_level(created, pin_from) => {
            }
            result => break result,
        }
    };
    let identity = match found {
        Ok(found)
            if rustix::fs::FileType::from_raw_mode(found.st_mode)
                == rustix::fs::FileType::Directory =>
        {
            found
        }
        Ok(_) => {
            return Err(io::Error::other(
                "directory level was replaced during creation",
            ));
        }
        Err(source) => return Err(source.into()),
    };
    match reopen_created_level(
        created,
        pin_from,
        &parent_fd,
        &name,
        &identity,
        before_reopen,
    ) {
        ReopenedLevel::Pinned(descend) => {
            created.push(CreatedDirectory {
                path,
                parent: Some(parent_fd),
                name,
                identity,
                own: Some(std::rc::Rc::clone(&descend)),
            });
            // This level just made the pinned window one longer than
            // `MAX_PINNED_LEVELS`; shed the outermost to bring it back --
            // one call always suffices, since nothing but this push could
            // have grown the window since the last time it was checked.
            if created.len() - *pin_from > MAX_PINNED_LEVELS {
                shed_outermost_pinned_level(created, pin_from);
            }
            Ok(descend)
        }
        // A real reopen failure, not a budget choice: kept on the same
        // terms as before `MAX_PINNED_LEVELS` existed, so cleanup still
        // stops here rather than past it.
        ReopenedLevel::Unpinned(source) => {
            created.push(CreatedDirectory {
                path,
                parent: Some(parent_fd),
                name,
                identity,
                own: None,
            });
            Err(source)
        }
        ReopenedLevel::Unverified(source) => Err(source),
    }
}

/// [`create_directories`]'s body on Unix, with two seams a test injects
/// and production leaves as no-ops:
///
/// - `before_dotdot` runs the instant before a `..` level is resolved --
///   after every level ahead of it is made and pinned, the one point a
///   swap can land that matters.
/// - `before_reopen` runs the instant before the reopen issued right after a
///   successful `mkdirat`. `Some` replaces that reopen with the given
///   failure instead of running it; `None` defers to the real reopen.
#[cfg(unix)]
fn create_directories_with_hooks(
    dir: &Path,
    before_dotdot: &mut dyn FnMut(),
    before_reopen: &mut dyn FnMut() -> Option<io::Error>,
) -> Result<Vec<CreatedDirectory>, (Vec<CreatedDirectory>, io::Error)> {
    let levels = directories_to_create(dir);
    let Some(first) = levels.first() else {
        return Ok(Vec::new());
    };
    // A bare relative name has no parent, and `""` is not a directory any OS
    // accepts, so it is the current directory — the same rule
    // [`pack_directory`] resolves the destination with.
    let start = first
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut parent_fd = match dest::open_traversal_directory(start) {
        Ok(fd) => std::rc::Rc::new(fd),
        Err(source) => return Err((Vec::new(), source)),
    };

    // Only the innermost `MAX_PINNED_LEVELS` get a descriptor pinned; see
    // the module docs. Counted against `created`, the levels this run
    // actually makes, never `levels` itself: a lexical `..` in `levels`
    // creates nothing, and pairing the budget with the wrong count would
    // pin too few of a destination that fits comfortably within it.
    let mut pin_from = 0;
    let mut created = Vec::new();
    for level in levels {
        // `directories_to_create`'s lexical walk can produce a `..` level
        // (`Path::file_name` is `None` for one) that names no new component
        // to make. It resolves relative to the descent itself (`..` from
        // `parent_fd`, which already stands where this loop last made or
        // found a level) rather than by reopening `level`'s full path,
        // which would re-walk -- and so trust again -- every component
        // already pinned.
        let Some(name) = level.file_name().map(std::ffi::OsStr::to_os_string) else {
            before_dotdot();
            parent_fd = match dest::open_directory_at(&parent_fd, OsStr::new("..")) {
                Ok(fd) => std::rc::Rc::new(fd),
                Err(source) => return Err((created, source)),
            };
            continue;
        };
        let mkdir_result = loop {
            match rustix::fs::mkdirat(
                &*parent_fd,
                &name,
                rustix::fs::Mode::RWXU | rustix::fs::Mode::RWXG | rustix::fs::Mode::RWXO,
            ) {
                Err(err)
                    if is_out_of_descriptors(err)
                        && shed_outermost_pinned_level(&mut created, &mut pin_from) => {}
                result => break result,
            }
        };
        match mkdir_result {
            Ok(()) => {
                match create_missing_level(
                    &mut created,
                    &mut pin_from,
                    parent_fd,
                    level,
                    name,
                    before_reopen,
                ) {
                    Ok(descend) => parent_fd = descend,
                    Err(source) => return Err((created, source)),
                }
            }
            // A level that stands as a directory now is no failure, whoever
            // made it — `create_dir_all`'s own rule. It is simply not this
            // run's to record, though anything nested under it still has to
            // be created through it. A final symlink to a directory counts,
            // as it does for `Path::is_dir`.
            Err(mkdir_err) => {
                let already_a_directory =
                    rustix::fs::statat(&*parent_fd, &name, rustix::fs::AtFlags::empty()).is_ok_and(
                        |stat| {
                            rustix::fs::FileType::from_raw_mode(stat.st_mode)
                                == rustix::fs::FileType::Directory
                        },
                    );
                if already_a_directory {
                    match dest::open_directory_at(&parent_fd, &name) {
                        Ok(fd) => parent_fd = std::rc::Rc::new(fd),
                        // The open's own failure is the real diagnosis now;
                        // the `mkdir` collision only ever proved the name
                        // was taken, which it still is.
                        Err(source) => return Err((created, source)),
                    }
                } else {
                    return Err((created, mkdir_err.into()));
                }
            }
        }
    }
    Ok(created)
}

/// [`create_directories`]'s off-Unix arm: no descriptor to pin, so each
/// level is [`fs::create_dir`] addressed by path.
#[cfg(not(unix))]
fn create_directories(
    dir: &Path,
) -> Result<Vec<CreatedDirectory>, (Vec<CreatedDirectory>, io::Error)> {
    let mut created = Vec::new();
    for level in directories_to_create(dir) {
        match fs::create_dir(&level) {
            Ok(()) => created.push(CreatedDirectory { path: level }),
            // A level that stands as a directory now is no failure, whoever
            // made it — `create_dir_all`'s own rule. It is simply not this
            // run's to record.
            Err(_) if level.is_dir() => {}
            Err(source) => return Err((created, source)),
        }
    }
    Ok(created)
}

/// Get the entries [`create_directories`] just wrote onto the disk.
///
/// A new directory is a *name in the level above it*, so the parent is what
/// has to be synced for it — the directory's own sync would only persist
/// what is inside it. Outermost first, so a crash part-way through leaves a
/// prefix of the chain rather than a deep directory hanging from a name
/// that never reached the disk.
///
/// Best-effort throughout: a weaker durability guarantee is not something
/// to fail a finished import over -- including `dir.parent` itself being a
/// traversal-only descriptor nothing can `fsync` directly
/// ([`dest::reopen_for_sync`]), or, past [`MAX_PINNED_LEVELS`], not being
/// pinned at all.
#[cfg(unix)]
fn sync_created_directories(created: &[CreatedDirectory]) {
    for dir in created {
        let Some(parent) = dir.parent.as_deref() else {
            continue;
        };
        if let Ok(real) = dest::reopen_for_sync(parent) {
            let _ = rustix::fs::fsync(&real);
        }
    }
}

/// [`sync_created_directories`]'s off-Unix arm, through
/// [`dest::sync_directory`] since no descriptor is pinned to sync directly.
#[cfg(not(unix))]
fn sync_created_directories(created: &[CreatedDirectory]) {
    for dir in created {
        // A bare relative name has no parent, and `""` is not a directory
        // any OS accepts, so it is the current directory — the same rule
        // [`pack_directory`] resolves the destination with.
        let parent = dir
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        dest::sync_directory(parent);
    }
}

/// Remove the directories this run created, innermost first.
///
/// A failed import should leave the filesystem as it found it: an empty
/// `pokeemerald-rs` directory in the user's data directory is litter that
/// looks like a half-installed game, and a destination reached through
/// several missing levels would leave a whole chain of them. Innermost
/// first, because a directory only comes away once what it holds is gone.
///
/// Non-recursive on purpose, so it can only ever remove a directory this
/// run created and left empty. The first refusal ends the walk: a level
/// that will not go is one the level above it is not empty of either, and
/// a directory something else has since been put in is no longer this
/// run's to take.
///
/// On Unix, "this run created" is asked of the pinned parent, not of the
/// path: `dir.name` is looked up in `dir.parent`, without following a
/// final symlink, and compared against the identity `create_directories`
/// captured and against the level's own held descriptor's `fstat`; a level
/// with no held descriptor is never removed. Only a match at that lookup
/// is removed, by name, through `unlinkat`. The module docs own why the
/// held descriptor makes that comparison trustworthy, why `unlinkat`
/// re-resolves the name regardless, and how narrow the remaining gap is.
///
/// A level past [`MAX_PINNED_LEVELS`] has no `dir.parent` to look anything
/// up in at all, so it is skipped rather than stopping the walk: unlike a
/// lookup that comes back wrong, sitting outside the pin budget is this
/// run's own choice, not a sign the levels further out are suspect too.
/// Nothing above such a level can be removed either unless it is genuinely
/// empty, since `unlinkat` still fails for real on one that is not.
#[cfg(unix)]
fn undo_created_directories(created: &[CreatedDirectory]) {
    for dir in created.iter().rev() {
        let Some(parent) = dir.parent.as_deref() else {
            continue;
        };
        match rustix::fs::statat(parent, &dir.name, rustix::fs::AtFlags::SYMLINK_NOFOLLOW) {
            Ok(stat)
                if same_directory_identity(&stat, &dir.identity)
                    && dir.own.as_ref().is_some_and(|own| {
                        rustix::fs::fstat(&**own)
                            .is_ok_and(|held| same_directory_identity(&stat, &held))
                    }) =>
            {
                match rustix::fs::unlinkat(parent, &dir.name, rustix::fs::AtFlags::REMOVEDIR) {
                    Ok(()) => {}
                    // Already gone is already taken care of.
                    Err(err) if err == rustix::io::Errno::NOENT => {}
                    // Refused for real (it holds something): the levels
                    // above hold it too and are not this run's to take back.
                    Err(_) => break,
                }
            }
            // Gone already -- nothing to remove here, and the level above is
            // unaffected by that.
            Err(err) if err == rustix::io::Errno::NOENT => {}
            // Some other entry sits at the name now, or it could not be
            // examined at all: not this run's directory either way, so it
            // is left alone, and the levels above are left too.
            _ => break,
        }
    }
}

/// [`undo_created_directories`]'s off-Unix arm: no identity to check, so
/// each recorded level is removed by path.
#[cfg(not(unix))]
fn undo_created_directories(created: &[CreatedDirectory]) {
    for dir in created.iter().rev() {
        if fs::remove_dir(&dir.path).is_err() {
            // A partial `create_dir_all` never made this level -- it is
            // missing, unnameable, or the non-directory component it tripped
            // on -- and the outer levels it did create still get taken back.
            // A level that still stands as a directory refused removal for
            // real (it holds something), so the levels above hold it too and
            // are not this run's to take back.
            match fs::symlink_metadata(&dir.path) {
                Ok(meta) if meta.is_dir() => break,
                _ => {}
            }
        }
    }
}

/// The fixed leading part of every temporary pack name.
///
/// The dot keeps the file out of a casual directory listing while it
/// exists; the rest says who left it there, on the rare occasion a crash
/// between the create and the publish leaves one behind.
const TEMP_PREFIX: &str = ".pokeemerald-rs-import";

#[cfg(test)]
mod tests;
