//! Decides whether an entry at a save or lock path is a plain file this
//! module may open, and opens it so a swap after that decision cannot
//! change the answer.
//!
//! [`SaveFile::read`](super::SaveFile::read) and the lock slot share this
//! one policy: a pre-open check for a fast, typed refusal, open flags that
//! neither follow a symlink nor block on a FIFO, and a check of the opened
//! handle itself.

use std::path::Path;

/// What is wrong with an entry [`refuse_an_unusable_entry`] inspected, for
/// the caller to fold into whichever [`SaveFileError`](super::SaveFileError) variant fits its own
/// path (a save path or a lock slot).
pub(super) enum UnusableEntry {
    /// The entry could not be inspected at all.
    Inspect(std::io::Error),
    /// The entry is a symlink instead of naming the file it opens.
    IsAlias,
    /// The entry exists and is not a symlink, but is not a plain file
    /// either -- a directory, socket, device, FIFO, or another kind of
    /// Windows reparse point.
    NotAPlainFile,
}

/// Refuses a symlinked or non-plain-file entry before anything opens it,
/// since the open would follow the link or block on a FIFO. A missing entry
/// is fine: the caller's own open reports that in its own way.
///
/// This is the one policy [`SaveFile::read`](super::SaveFile::read) and the lock slot's
/// `open_lock_slot` share for "what counts as a plain file here", so the two
/// can never quietly disagree.
///
/// # Errors
///
/// [`UnusableEntry::IsAlias`] if `path` is a symlink; [`UnusableEntry::NotAPlainFile`]
/// if it is anything else that is not a plain file; [`UnusableEntry::Inspect`]
/// if it could not be inspected.
pub(super) fn refuse_an_unusable_entry(path: &Path) -> Result<(), UnusableEntry> {
    let entry = match std::fs::symlink_metadata(path) {
        Ok(entry) => entry,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => return Err(UnusableEntry::Inspect(source)),
    };
    refuse_an_unusable_kind(&entry)
}

/// The one classification both [`refuse_an_unusable_entry`] and
/// [`refuse_an_unusable_open`] apply to what they inspected.
///
/// On Windows `std` calls only a name-surrogate reparse point (a symlink or
/// junction) a symlink, so a cloud-files placeholder, an app-execution
/// alias, or a deduplicated file reports as a plain file. Opened with
/// [`refuse_unusable_opens`]'s flags, such an entry would be read as its
/// raw reparse object, not the data its filter serves, so any reparse point
/// that is not a symlink is refused too.
fn refuse_an_unusable_kind(metadata: &std::fs::Metadata) -> Result<(), UnusableEntry> {
    if metadata.is_symlink() {
        Err(UnusableEntry::IsAlias)
    } else if metadata.is_file() && !is_reparse_point(metadata) {
        Ok(())
    } else {
        Err(UnusableEntry::NotAPlainFile)
    }
}

/// Whether `metadata` carries Windows's reparse-point attribute; never true
/// elsewhere.
fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        metadata.file_attributes() & open_flags::FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        let _ = metadata;
        false
    }
}

/// `open(2)`/`CreateFileW` flag and errno values `std` has no portable name
/// for, needed so the open itself -- not just the [`refuse_an_unusable_entry`]
/// check before it -- refuses a symlinked final component and never blocks
/// on a FIFO's other end.
#[cfg(unix)]
mod open_flags {
    // Linux's generic `<asm-generic/fcntl.h>` `O_NOFOLLOW`/`O_LARGEFILE`
    // pair, which x86's `<bits/fcntl-linux.h>` and riscv64's `<asm/fcntl.h>`
    // (which does not override it) both keep unchanged: `O_NOFOLLOW` is
    // `0x20000` and `O_LARGEFILE` -- unused here -- is `0x8000`.
    #[cfg(all(
        target_os = "linux",
        any(target_arch = "x86", target_arch = "x86_64", target_arch = "riscv64")
    ))]
    pub(super) const O_NOFOLLOW: i32 = 0x0002_0000;
    // Linux arm's and aarch64's `<asm/fcntl.h>` swap that pair: `O_NOFOLLOW`
    // is `0x8000` and `O_LARGEFILE` is `0x20000`. Using the generic value
    // here would ask for `O_LARGEFILE`, a 64-bit-build no-op, instead of
    // `O_NOFOLLOW`, and let a symlink swapped in after the pre-open check
    // through.
    #[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
    pub(super) const O_NOFOLLOW: i32 = 0x0000_8000;
    // No Linux arch this crate builds for overrides `<asm-generic/fcntl.h>`'s
    // `O_NONBLOCK` or `<asm-generic/errno.h>`'s `ELOOP`.
    #[cfg(target_os = "linux")]
    pub(super) const O_NONBLOCK: i32 = 0x0000_0800;
    #[cfg(target_os = "linux")]
    pub(super) const ELOOP: i32 = 40;

    // aarch64's `O_NOFOLLOW` is the one value here that CI, which builds
    // only for x86_64, never compiles; a cross `cargo check --target
    // aarch64-unknown-linux-gnu -p engine` catches a wrong value moved here,
    // and this pins the value itself once that check runs.
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    const _: () = assert!(O_NOFOLLOW == 0x0000_8000);

    // macOS's and the BSDs' shared `<sys/fcntl.h>` and `<sys/errno.h>`.
    #[cfg(not(target_os = "linux"))]
    pub(super) const O_NOFOLLOW: i32 = 0x0000_0100;
    #[cfg(not(target_os = "linux"))]
    pub(super) const O_NONBLOCK: i32 = 0x0000_0004;
    #[cfg(not(target_os = "linux"))]
    pub(super) const ELOOP: i32 = 62;
}

/// Windows's `<winbase.h>` and `<winnt.h>`.
#[cfg(windows)]
mod open_flags {
    /// Opens a reparse point (a symlink, among others) itself rather than
    /// following it.
    pub(super) const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    /// Marks an entry as a reparse point of any tag.
    pub(super) const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
}

/// Adds the flags that close the window between [`refuse_an_unusable_entry`]
/// and the open it precedes: an entry swapped for a symlink in that window
/// can no longer be followed, and a FIFO swapped in cannot block the open.
pub(super) fn refuse_unusable_opens(options: &mut std::fs::OpenOptions) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags(open_flags::O_NOFOLLOW | open_flags::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        options.custom_flags(open_flags::FILE_FLAG_OPEN_REPARSE_POINT);
    }
}

/// Whether `error` is [`refuse_unusable_opens`]'s flags refusing to follow a
/// symlinked final component.
pub(super) fn open_refused_a_symlink(error: &std::io::Error) -> bool {
    #[cfg(unix)]
    {
        error.raw_os_error() == Some(open_flags::ELOOP)
    }
    #[cfg(not(unix))]
    {
        // Windows opens the reparse point itself instead of failing the
        // open; `refuse_an_unusable_open` catches it from the handle.
        let _ = error;
        false
    }
}

/// Refuses an opened handle that is not a plain file after all: the one
/// case [`refuse_unusable_opens`]'s flags cannot themselves refuse on
/// Windows (a reparse point, opened rather than followed), or anything
/// [`refuse_an_unusable_entry`] could have missed because the entry changed
/// between that check and this open.
pub(super) fn refuse_an_unusable_open(file: &std::fs::File) -> Result<(), UnusableEntry> {
    let opened = file.metadata().map_err(UnusableEntry::Inspect)?;
    refuse_an_unusable_kind(&opened)
}
