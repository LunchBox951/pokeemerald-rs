//! Stages a flash image beside its save path, ready to be renamed over it.
//!
//! [`SaveFile::write`](super::SaveFile::write) publishes by rename, so the
//! image must first exist whole and synced at a sibling entry in the same
//! directory. This module owns that entry, from the name it takes through
//! the cleanup that abandons it.

use std::path::{Path, PathBuf};

/// A first guess at the per-component limit most POSIX and Windows
/// filesystems share, in bytes -- not a promise every filesystem keeps:
/// eCryptfs caps a component at 143, and a symlinked temp root can make a
/// host's resolved path longer than the one this process measures.
/// [`StagingArea::stage_narrowing_until_accepted`], not this constant, is
/// what keeps the sibling valid.
pub(super) const MAX_COMPONENT_LEN: usize = 255;

/// Suffix width the first staging attempt uses.
pub(super) const WIDEST_HEX_DIGITS: usize = 10;

/// Suffix width the narrowing retries stop at.
const NARROWEST_HEX_DIGITS: usize = 1;

/// The widest suffix a walk exhausts rather than samples. The narrowing
/// retries halve the width, so a host's real limits can only land staging on
/// ten, five, two, or one hex digits; two renders 256 names, few enough that
/// entries a crashed process never swept could hold them all, and few enough
/// to try in bounded work. Five renders over a million, more than any
/// directory holds and more than a walk should attempt: a width that wide is
/// there to be unguessable, not to be exhausted.
const WIDEST_EXHAUSTIBLE_HEX_DIGITS: usize = 2;

/// The names a walk at `hex_digits` offers before reporting the namespace
/// exhausted: every name that width renders, or, past
/// [`WIDEST_EXHAUSTIBLE_HEX_DIGITS`], as many as that width would.
fn names_offered(hex_digits: usize) -> usize {
    1_usize << (4 * hex_digits.min(WIDEST_EXHAUSTIBLE_HEX_DIGITS))
}

/// Opens `path` for writing and fails if anything already holds that name,
/// refusing an existing file, directory, or symlink instead of following or
/// truncating it.
///
/// On Windows the returned handle shares nothing, so until it is dropped no
/// other opener -- in this process or any other -- can open, delete, or
/// rename that entry: the name cannot be made to mean a different file
/// while the handle lives. Windows counts the promoting rename among the
/// things it bars, so the handle has to be given up first; see
/// [`StagedSave::release_hold`].
pub(super) fn create_new_exclusive(path: &Path) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;

        options.share_mode(0);
    }
    options.open(path)
}

/// Stages `bytes` at the first of `names` that `create_new` finds free,
/// reporting the last collision once they have all turned out to be taken.
pub(super) fn stage_at_first_free_name(
    names: impl IntoIterator<Item = PathBuf>,
    create_new: impl Fn(&Path) -> std::io::Result<std::fs::File>,
    bytes: &[u8],
) -> std::io::Result<StagedSave> {
    let mut last_collision = None;
    for path in names {
        match fill_new_file(&create_new, &path, bytes) {
            Ok(staged) => return Ok(staged),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                last_collision = Some(err);
            }
            Err(err) => return Err(err),
        }
    }
    Err(last_collision.unwrap_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "exhausted staging attempts",
        )
    }))
}

/// Writes and syncs `bytes` into a `path` `create_new` has just claimed,
/// cleaning up through [`StagedSave::remove_after`] on any failure past
/// that open.
fn fill_new_file(
    create_new: impl Fn(&Path) -> std::io::Result<std::fs::File>,
    path: &Path,
    bytes: &[u8],
) -> std::io::Result<StagedSave> {
    use std::io::Write as _;

    let file = create_new(path)?;
    // Captured before the write so a read failure here (never observed off
    // an object this call just opened) is reported the same way regardless
    // of what the write below would have done, and so the identity
    // `StagedSave::remove_after` binds its handle-bound delete to
    // ([`remove_through_verified_handle`]) is on hand for the whole of this
    // staged file's life, including after `release_hold` empties the hold.
    //
    // A read failure here has no identity to bind a handle-based delete to,
    // and removing `path` by name would reopen exactly the TOCTOU this
    // cleanup exists to close (an ancestor could already be retargeted with
    // no live hold yet to have stopped it) -- so the file is left in place
    // and the failure is surfaced rather than guessed at.
    #[cfg(windows)]
    let identity = match WindowsFileIdentity::of(&file) {
        Ok(identity) => identity,
        Err(unreadable) => {
            drop(file);
            return Err(std::io::Error::new(
                unreadable.kind(),
                format!(
                    "the staging file at {} was created but its identity could not be read \
                     to bind a safe removal to it, so it was left in place: {unreadable}",
                    path.display()
                ),
            ));
        }
    };
    let result = (|| {
        let mut writer = std::io::BufWriter::new(&file);
        writer.write_all(bytes)?;
        writer.flush()?;
        file.sync_all()
    })();
    #[cfg(not(windows))]
    let hold = file;
    #[cfg(windows)]
    let hold = WindowsHold {
        file: Some(file),
        identity,
    };
    let mut staged = StagedSave {
        path: path.to_path_buf(),
        hold,
    };
    match result {
        Ok(()) => Ok(staged),
        Err(source) => Err(staged.remove_after(source)),
    }
}

/// The sibling namespace beside one save path.
#[derive(Debug, Clone, Copy)]
pub(super) struct StagingArea<'a> {
    save_path: &'a Path,
}

impl<'a> StagingArea<'a> {
    /// The staging area for a save at `save_path`.
    pub(super) fn beside(save_path: &'a Path) -> Self {
        Self { save_path }
    }

    /// Stages `bytes` beside the save, under a name this host accepts and no
    /// one else holds.
    pub(super) fn stage(&self, bytes: &[u8]) -> std::io::Result<StagedSave> {
        self.stage_narrowing_until_accepted(create_new_exclusive, bytes)
    }

    /// As [`Self::stage`], claiming each name through `create_new` so a test
    /// can stand in a host that refuses names this one accepts.
    ///
    /// A host that refuses a name outright (`InvalidFilename`) rather than
    /// finding it taken (`AlreadyExists`) is reporting a limit
    /// [`MAX_COMPONENT_LEN`] guessed wrong, so the name gets shorter and the
    /// walk restarts: first halving the stem down to nothing, then halving
    /// the suffix down to [`NARROWEST_HEX_DIGITS`]. Only a refusal of that
    /// floor propagates.
    pub(super) fn stage_narrowing_until_accepted(
        &self,
        create_new: impl Fn(&Path) -> std::io::Result<std::fs::File>,
        bytes: &[u8],
    ) -> std::io::Result<StagedSave> {
        let mut stem_cap = first_guess_stem_cap();
        let mut hex_digits = WIDEST_HEX_DIGITS;
        loop {
            match stage_at_first_free_name(self.names(stem_cap, hex_digits), &create_new, bytes) {
                Err(err) if err.kind() == std::io::ErrorKind::InvalidFilename && stem_cap > 0 => {
                    stem_cap /= 2;
                }
                Err(err)
                    if err.kind() == std::io::ErrorKind::InvalidFilename
                        && hex_digits > NARROWEST_HEX_DIGITS =>
                {
                    hex_digits = (hex_digits / 2).max(NARROWEST_HEX_DIGITS);
                }
                result => return result,
            }
        }
    }

    /// The names one width offers, in the order they are tried: the save's
    /// basename cut to at most `max_stem_len` bytes on a char boundary,
    /// carrying a `.tmp.<hex>` suffix `hex_digits` wide.
    ///
    /// The suffix is drawn once and then stepped, never redrawn, so no name
    /// is offered twice, and the walk runs [`names_offered`] names long: at a
    /// width the shrink chain narrows to, that is the width's entire
    /// namespace -- sixteen names at [`NARROWEST_HEX_DIGITS`], 256 one rung
    /// above it -- and independent draws, or a walk cut to some other width's
    /// count, would report the namespace exhausted while a free name sat
    /// untried.
    ///
    /// A name equal to the save path is skipped. `create_new` succeeds on a
    /// destination no save occupies yet, so staging there would write the
    /// image in place: visible while half-written, and left partial by a
    /// crash where the rename is supposed to publish a whole file.
    fn names(&self, max_stem_len: usize, hex_digits: usize) -> impl Iterator<Item = PathBuf> + '_ {
        let mut stem = self
            .save_path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        if stem.len() > max_stem_len {
            let mut cut = max_stem_len;
            while !stem.is_char_boundary(cut) {
                cut -= 1;
            }
            stem.truncate(cut);
        }
        let mask = unique_value_mask(hex_digits);
        let mut value = unique_value(hex_digits);
        std::iter::repeat_with(move || loop {
            let candidate = self
                .save_path
                .with_file_name(format!("{stem}.tmp.{value:0hex_digits$x}"));
            value = value.wrapping_add(1) & mask;
            if !self.aliases_save_path(&candidate) {
                return candidate;
            }
        })
        .take(names_offered(hex_digits))
    }

    /// Whether `candidate` names the save path itself on any supported file
    /// system. The candidate's stem is copied from the save's own basename
    /// and its suffix is ASCII, so ASCII case folding covers every alias a
    /// case-insensitive volume can add.
    fn aliases_save_path(&self, candidate: &Path) -> bool {
        match (candidate.file_name(), self.save_path.file_name()) {
            (Some(candidate), Some(save)) => candidate
                .as_encoded_bytes()
                .eq_ignore_ascii_case(save.as_encoded_bytes()),
            _ => candidate == self.save_path,
        }
    }

    /// The first name of a fresh walk under these widths. A test convenience
    /// over [`Self::names`]; staging hands the whole walk to
    /// [`stage_at_first_free_name`].
    #[cfg(test)]
    pub(super) fn first_name_under(&self, max_stem_len: usize, hex_digits: usize) -> PathBuf {
        self.names(max_stem_len, hex_digits)
            .next()
            .expect("every width offers at least one name")
    }

    /// The first name of a fresh walk at the widths [`Self::stage`] tries
    /// first. A test convenience, as [`Self::first_name_under`].
    #[cfg(test)]
    pub(super) fn first_name(&self) -> PathBuf {
        self.first_name_under(first_guess_stem_cap(), WIDEST_HEX_DIGITS)
    }
}

/// Whatever [`MAX_COMPONENT_LEN`] leaves once the widest suffix is
/// subtracted, so the very first attempt fits the common case.
fn first_guess_stem_cap() -> usize {
    let suffix_len = ".tmp.".len() + WIDEST_HEX_DIGITS;
    MAX_COMPONENT_LEN.saturating_sub(suffix_len)
}

/// `std`-only entropy folded into one value that fits `width` hex digits:
/// process id, clock nanoseconds, and a fresh `RandomState` key, which alone
/// already differs between two calls at the same nanosecond. `create_new` is
/// what keeps two stagings from colliding; this width only makes the name
/// unguessable, so narrowing it costs unpredictability, not exclusivity.
fn unique_value(width: usize) -> u64 {
    use std::hash::{BuildHasher, Hasher};

    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u32(std::process::id());
    hasher.write_u128(nanos);
    hasher.finish() & unique_value_mask(width)
}

/// Every value `width` hex digits can render, and no other. `width` is
/// [`WIDEST_HEX_DIGITS`] or a halving of it, so it is never wide enough to
/// overflow the shift.
fn unique_value_mask(width: usize) -> u64 {
    (1_u64 << (4 * width)) - 1
}

/// The handle that wrote the staged image, kept open until the image is
/// promoted or abandoned: while it lives the file cannot be freed, so
/// nothing that takes its name can inherit its identity and pass for it. A
/// rename is indifferent to it, so nothing has to give it up early.
#[cfg(not(windows))]
type Hold = std::fs::File;

/// The handle that wrote the staged image, kept open until the image is
/// promoted or abandoned, alongside the identity [`StagedSave::remove_after`]
/// binds its handle-bound delete to once that handle is gone (issue #1132).
/// What the open handle forbids while it lives is documented at
/// [`create_new_exclusive`]; that includes this process, which is why `file`
/// is an `Option`: [`StagedSave::release_hold`] empties it when the name has
/// to be given up, but `identity` outlives it.
#[cfg(windows)]
#[derive(Debug)]
struct WindowsHold {
    file: Option<std::fs::File>,
    identity: WindowsFileIdentity,
}

#[cfg(windows)]
type Hold = WindowsHold;

/// Ends `hold` where the platform needs it ended: nothing, since the hold
/// stays for the life of the [`StagedSave`] (see [`Hold`]).
#[cfg(not(windows))]
fn release(_hold: &mut Hold) {}

/// Ends `hold` where the platform needs it ended: drops the handle so the
/// staging name can be renamed or deleted again (see [`Hold`]). `identity`
/// is left in place for a cleanup that runs after this.
#[cfg(windows)]
fn release(hold: &mut Hold) {
    drop(hold.file.take());
}

/// The identity `GetFileInformationByHandle` reads off an open handle:
/// volume serial number and file index, the one per-object identity Windows
/// exposes to a handle regardless of which path was used to open it (`std`'s
/// equivalent, `MetadataExt::volume_serial_number`/`file_index`, is still
/// gated on the unstable `windows_by_handle`, rust-lang/rust#63010).
///
/// Comparing this after opening `path` fresh, rather than trusting the live
/// hold the way [`is_the_held_file`] does before [`StagedSave::release_hold`]
/// runs, is what lets [`remove_through_verified_handle`] delete through the
/// very handle it just verified instead of re-resolving `path` a second time
/// for the delete itself (issue #1132; approval trail in the dependency
/// ledger, `README.md`).
#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WindowsFileIdentity {
    volume_serial_number: u32,
    file_index: u64,
}

#[cfg(windows)]
impl WindowsFileIdentity {
    /// Reads the identity of the object `handle` refers to.
    fn of(handle: &std::fs::File) -> std::io::Result<Self> {
        use std::os::windows::io::AsRawHandle as _;
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
        };

        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        // SAFETY: `handle` stays open for this call, and `info` is a correctly-sized out parameter the API fills in place.
        let ok = unsafe { GetFileInformationByHandle(handle.as_raw_handle(), &raw mut info) };
        if ok == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Self {
            volume_serial_number: info.dwVolumeSerialNumber,
            file_index: (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
        })
    }
}

/// Opens `path` for [`remove_through_verified_handle`]'s identity check and
/// delete: `DELETE | FILE_READ_ATTRIBUTES` access, no sharing, and
/// `FILE_FLAG_OPEN_REPARSE_POINT` so a reparse point planted at the final
/// component is not followed underneath the check.
#[cfg(windows)]
fn open_for_verified_delete(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::windows::fs::OpenOptionsExt as _;
    use windows_sys::Win32::Storage::FileSystem::{
        DELETE, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
    };

    std::fs::OpenOptions::new()
        .access_mode(DELETE | FILE_READ_ATTRIBUTES)
        .share_mode(0)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

/// Marks the object `handle` refers to for deletion once every handle to it
/// closes, through `handle` itself rather than a second lookup of its path.
#[cfg(windows)]
fn delete_through_handle(handle: &std::fs::File) -> std::io::Result<()> {
    use std::os::windows::io::AsRawHandle as _;
    use windows_sys::Win32::Storage::FileSystem::{
        FileDispositionInfo, SetFileInformationByHandle, FILE_DISPOSITION_INFO,
    };

    let info = FILE_DISPOSITION_INFO { DeleteFile: true };
    let size = u32::try_from(std::mem::size_of::<FILE_DISPOSITION_INFO>())
        .expect("FILE_DISPOSITION_INFO is far smaller than u32::MAX");
    // SAFETY: `handle` has DELETE access, `FileDispositionInfo` matches `info`'s type, and `size` is that type's exact size.
    let ok = unsafe {
        SetFileInformationByHandle(
            handle.as_raw_handle(),
            FileDispositionInfo,
            std::ptr::addr_of!(info).cast(),
            size,
        )
    };
    if ok == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Removes the file `identity` names if `path` still reaches it, deleting
/// through the same handle the identity check opens so no re-lookup of
/// `path` occurs between the check and the delete. A path that now reaches a
/// different object -- or nothing at all -- is left alone.
#[cfg(windows)]
fn remove_through_verified_handle(
    path: &Path,
    identity: WindowsFileIdentity,
) -> std::io::Result<()> {
    remove_through_verified_handle_with(path, identity, |_handle| {})
}

/// [`remove_through_verified_handle`] with a hook run between the identity
/// check and the delete, so a test can land a retarget in exactly that
/// window without a real race: a test's only other way to reach it would be
/// two threads racing the OS, which no cheap seam makes deterministic.
#[cfg(windows)]
fn remove_through_verified_handle_with(
    path: &Path,
    identity: WindowsFileIdentity,
    before_delete: impl FnOnce(&std::fs::File),
) -> std::io::Result<()> {
    let handle = match open_for_verified_delete(path) {
        Ok(handle) => handle,
        Err(missing) if missing.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(open_err) => return Err(open_err),
    };
    if WindowsFileIdentity::of(&handle)? != identity {
        return Ok(());
    }
    before_delete(&handle);
    delete_through_handle(&handle)
}

/// Folds `cleanup_err` into `source`, keeping `source`'s `ErrorKind` (mirrors
/// [`StagedSave::remove_after`]'s own fold) and naming `path` as the
/// abandoned staging file left behind.
#[cfg(windows)]
fn fold_cleanup_error(
    source: &std::io::Error,
    cleanup_err: &std::io::Error,
    path: &Path,
) -> std::io::Error {
    std::io::Error::new(
        source.kind(),
        format!(
            "{source}; additionally failed to remove the abandoned staging file {}: {cleanup_err}",
            path.display()
        ),
    )
}

/// Whether `found` describes the very file `hold` holds open: same device
/// and inode.
#[cfg(unix)]
fn is_the_held_file(hold: &Hold, found: &std::fs::Metadata) -> std::io::Result<bool> {
    use std::os::unix::fs::MetadataExt as _;

    let staged = hold.metadata()?;
    Ok((staged.dev(), staged.ino()) == (found.dev(), found.ino()))
}

/// Whether `found` describes the very file `hold` holds open. Off unix
/// there is no identity to read back -- the Windows file index sits behind
/// the unstable `windows_by_handle` feature -- so the answer rests on what
/// the hold forbids (see [`create_new_exclusive`]) rather than on what a
/// reading shows.
///
/// On Windows that leaves [`StagedSave::still_ours`]'s regular-file test as
/// the whole remaining question -- while the hold lives. Once
/// [`StagedSave::release_hold`] empties it, [`StagedSave::remove_after`]
/// no longer goes through `still_ours` at all off Unix; it deletes through
/// [`remove_through_verified_handle`] instead, bound to the identity
/// captured when the staged file was created. On any other non-unix host
/// the hold is an ordinary handle, and a regular file that replaced the
/// entry would go undetected.
#[cfg(not(unix))]
#[expect(
    clippy::unnecessary_wraps,
    reason = "one signature for both platforms; only the unix arm can fail to read an identity"
)]
fn is_the_held_file(_hold: &Hold, _found: &std::fs::Metadata) -> std::io::Result<bool> {
    Ok(true)
}

/// A staged flash image and the hold that keeps the staging name its own.
#[derive(Debug)]
pub(super) struct StagedSave {
    pub(super) path: PathBuf,
    hold: Hold,
}

impl StagedSave {
    /// Whether the staging path still names this staged file, rather than a
    /// symlink, directory, or other entry that took its name.
    ///
    /// A reading that failed is neither answer, and surfaces rather than
    /// passing for "replaced".
    pub(super) fn still_ours(&self) -> std::io::Result<bool> {
        let found = std::fs::symlink_metadata(&self.path)?;
        Ok(found.file_type().is_file() && is_the_held_file(&self.hold, &found)?)
    }

    /// Gives up the hold, so that the rename which promotes the staged
    /// image -- or the unlink which abandons it -- can take its name.
    ///
    /// Call this only after the last [`Self::still_ours`] whose answer the
    /// caller relies on: off unix that answer rests on the hold (see
    /// [`is_the_held_file`]). What is left once the hold ends is the window
    /// [`SaveFile::write_with`](super::SaveFile::write_with) documents.
    pub(super) fn release_hold(&mut self) {
        release(&mut self.hold);
    }
}

#[cfg(unix)]
impl StagedSave {
    /// Removes this staged file after `source`, folding a cleanup failure into
    /// the returned error rather than swallowing it -- otherwise a caller who
    /// only sees `source` would never learn a staging file was left behind.
    /// An entry that replaced it belongs to whoever put it there and is left
    /// where it is; ownership that could not be read at all leaves the same
    /// file behind, and is reported the same way.
    ///
    /// The check-then-act bound documented at the rename in
    /// [`SaveFile::write_with`](super::SaveFile::write_with) applies to this
    /// unlink too: the check above is by pathname, and the unlink below
    /// re-resolves `self.path` a second time.
    pub(super) fn remove_after(&mut self, source: std::io::Error) -> std::io::Error {
        let left_behind = match self.still_ours() {
            Ok(false) => return source,
            Ok(true) => {
                self.release_hold();
                std::fs::remove_file(&self.path).err()
            }
            Err(unreadable) => Some(unreadable),
        };
        let Some(cleanup_source) = left_behind else {
            return source;
        };
        std::io::Error::new(
            source.kind(),
            format!(
                "{source}; additionally failed to remove the abandoned staging file {}: {cleanup_source}",
                self.path.display()
            ),
        )
    }
}

#[cfg(windows)]
impl StagedSave {
    /// [`remove_after`](Self::remove_after)'s handle-bound counterpart to the
    /// Unix arm: releases the hold (a live one would refuse the reopen below
    /// as a sharing violation) and deletes through
    /// [`remove_through_verified_handle`], bound to the identity
    /// [`fill_new_file`] captured when this staged file was created rather
    /// than to whatever `self.path` resolves to now. That handle-bound
    /// delete never performs a second lookup of `self.path` for a
    /// retargeted ancestor -- or a swap the departed hold no longer blocks
    /// -- to land in. The release-to-rename window
    /// [`SaveFile::write_with`](super::SaveFile::write_with) documents is
    /// unrelated: it bounds the *rename*, not this cleanup unlink.
    pub(super) fn remove_after(&mut self, source: std::io::Error) -> std::io::Error {
        self.release_hold();
        match remove_through_verified_handle(&self.path, self.hold.identity) {
            Ok(()) => source,
            Err(cleanup_err) => fold_cleanup_error(&source, &cleanup_err, &self.path),
        }
    }
}

#[cfg(test)]
mod tests;
