//! Exclusively claimed scratch directories for the crate's unit tests.

use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;

/// How many names a claim tries before it gives up.
const CLAIM_ATTEMPTS: u32 = 1_000;

/// A fresh directory this process created, cleaned up on drop.
///
/// A predictable name is only safe once `create_dir` has made it:
/// `create_dir` fails on any existing entry (a populated directory, a file,
/// a symlink even when dangling), so the name is retried rather than
/// reused, and no candidate is ever deleted or written through before it is
/// claimed. On unix the directory is created `0700`.
///
/// Cleanup never deletes by the directory's name alone, because a name can
/// be swapped between a check and the delete. Every path a test derives
/// through [`ScratchDir::join`] is recorded by its first component; drop
/// removes only those entries (a recorded directory recursively, so anything
/// inside it goes too) and then calls the non-recursive `remove_dir`, which
/// refuses a non-empty directory. A stranger that swaps itself in at the
/// claimed name therefore survives, and so does any unrecorded top-level
/// entry (the directory is then left behind rather than guessed at).
///
/// Before any of that, drop checks the name still holds the directory the
/// claim created, against an identity recorded at claim time, and touches
/// nothing when it does not: a replacement directory keeps its entries even
/// when one of them carries a recorded name.
///
/// - Unix: the identity is the directory's device and inode number, read
///   without following a link. The root is then removed by the
///   non-recursive `remove_dir`.
/// - Windows: the identity is the `win_fs::WindowsFileIdentity` (volume
///   serial and file ID) of a handle opened without following a reparse
///   point. Drop reopens the name the same way, compares, and keeps that
///   handle open across the removals, so the claimed directory's file ID
///   cannot be recycled meanwhile; the root is then deleted through that
///   verified handle (a delete the file system refuses for a non-empty
///   directory), never by name. A volume whose file IDs are not stable
///   identities (FAT, exFAT: `win_fs::volume_supports_open_by_file_id`
///   answers no) yields no identity, and neither does a failed open or
///   query, at claim or at drop; drop then removes nothing and leaves the
///   directory behind rather than delete unverified.
/// - Any other platform: drop fails closed, removing nothing.
///
/// The residual on both platforms is a same-user swap that recreates the
/// exact recorded names between the identity check and the removals, which
/// go by pathname.
pub(crate) struct ScratchDir {
    path: PathBuf,
    /// The claimed directory's identity, so cleanup can tell it from
    /// whatever occupies the name afterwards.
    #[cfg(unix)]
    identity: (u64, u64),
    /// As on unix; `None` when no trustworthy identity could be read, which
    /// leaves everything in place at drop.
    #[cfg(windows)]
    identity: Option<win_fs::WindowsFileIdentity>,
    /// First path component of everything handed out by `join`.
    entries: Mutex<Vec<OsString>>,
}

impl ScratchDir {
    /// Claim a fresh `{prefix}-{pid}-{attempt}` directory under the OS
    /// temporary directory.
    pub(crate) fn new(prefix: &str) -> Self {
        Self::new_in(&std::env::temp_dir(), prefix)
    }

    /// Claim a fresh directory under `root`. Panics rather than reusing a
    /// name, so a hostile or crowded `root` fails the test instead of the
    /// fixture.
    pub(crate) fn new_in(root: &Path, prefix: &str) -> Self {
        let pid = std::process::id();
        for attempt in 0..CLAIM_ATTEMPTS {
            let path = root.join(format!("{prefix}-{pid}-{attempt}"));
            match create_private_dir(&path) {
                Ok(()) => return Self::owning(path),
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(err) => panic!("a writable scratch directory: {err}"),
            }
        }
        panic!("no unused scratch directory name for {prefix} under pid {pid}");
    }

    fn owning(path: PathBuf) -> Self {
        Self {
            #[cfg(unix)]
            identity: identity_of(&path).expect("the directory just created"),
            #[cfg(windows)]
            identity: open_verifiable(&path).map(|(_handle, identity)| identity),
            path,
            entries: Mutex::new(Vec::new()),
        }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// A path inside the directory. Its first component is recorded so drop
    /// can remove it.
    pub(crate) fn join(&self, name: impl AsRef<Path>) -> PathBuf {
        let name = name.as_ref();
        if let Some(Component::Normal(first)) = name.components().next() {
            let mut entries = self.entries.lock().expect("the entry list");
            if !entries.iter().any(|known| known == first) {
                entries.push(first.to_os_string());
            }
        }
        self.path.join(name)
    }
}

#[cfg(unix)]
fn create_private_dir(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt as _;
    std::fs::DirBuilder::new().mode(0o700).create(path)
}

#[cfg(not(unix))]
fn create_private_dir(path: &Path) -> std::io::Result<()> {
    std::fs::create_dir(path)
}

/// Device and inode of the directory itself (never a link's target), or
/// `None` when the name holds anything else.
#[cfg(unix)]
fn identity_of(path: &Path) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt as _;
    let meta = std::fs::symlink_metadata(path).ok()?;
    meta.is_dir().then(|| (meta.dev(), meta.ino()))
}

/// Opens whatever `path` names, without following a reparse point, and
/// reads its identity off that handle. `None` when the open or a query
/// fails, or when the volume's file IDs are not stable identities (a
/// replacement could then reuse the claimed ID), so the caller never
/// deletes on an identity it cannot trust.
#[cfg(windows)]
fn open_verifiable(path: &Path) -> Option<(std::fs::File, win_fs::WindowsFileIdentity)> {
    let handle = win_fs::open_for_verified_delete(path).ok()?;
    if !win_fs::volume_supports_open_by_file_id(&handle).ok()? {
        return None;
    }
    let identity = win_fs::WindowsFileIdentity::of(&handle).ok()?;
    Some((handle, identity))
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        #[cfg(unix)]
        if identity_of(&self.path) != Some(self.identity) {
            return;
        }
        // Held to the end of drop: it pins the verified directory's file ID
        // and is the handle the root is deleted through.
        #[cfg(windows)]
        let root = {
            let Some(claimed) = self.identity else {
                return;
            };
            match open_verifiable(&self.path) {
                Some((root, found)) if found == claimed => root,
                _ => return,
            }
        };
        #[cfg(any(unix, windows))]
        {
            let entries = std::mem::take(&mut *self.entries.lock().expect("the entry list"));
            for name in entries {
                let entry = self.path.join(name);
                // `symlink_metadata` so a link is unlinked, never followed.
                let _ = match std::fs::symlink_metadata(&entry) {
                    Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(&entry),
                    Ok(_) => std::fs::remove_file(&entry),
                    Err(err) => Err(err),
                };
            }
            // Non-recursive: a non-empty directory is left alone.
            #[cfg(unix)]
            let _ = std::fs::remove_dir(&self.path);
            // Through the verified handle, not the name; the file system
            // refuses the delete for a non-empty directory.
            #[cfg(windows)]
            let _ = win_fs::delete_through_handle(&root);
        }
    }
}

/// Plants a populated stranger at the first predictable name a claim with
/// `prefix` would try under the OS temporary directory, and (on unix) a
/// symlink to a populated victim at the second. `legacy_names` are further
/// names, each a single component under that directory, where the fixture's
/// unfixed predecessor would have put its directory; a populated stranger
/// is planted at each too, so the regression collides with the old
/// delete-then-create fixture as well as the exclusive claim (an unfixed
/// fixture would delete the stranger and the sentinel check below fails).
/// A name already planted above is skipped. Runs `claim`, which must
/// make a fixture whose claimed directory is named `{prefix}-{pid}-{n}` and
/// return it with a path inside it obtained through the fixture's recorded
/// `join`. Asserts the claim lands on neither planted name, that writing
/// through it and dropping it leaves the stranger, the victim, and the link
/// as they were, and that the fixture removed its own directory.
///
/// The planted entries are real shared-temp names, so each is removed by
/// name (never recursively) on the way out, even when an assertion fails.
pub(crate) fn assert_claim_spares_planted_occupants<G>(
    prefix: &str,
    legacy_names: &[String],
    claim: impl FnOnce() -> (G, std::path::PathBuf),
) {
    struct Planted(Vec<std::path::PathBuf>);
    impl Drop for Planted {
        fn drop(&mut self) {
            // Reverse order: contents before the directory that holds them.
            for path in self.0.iter().rev() {
                let _ = std::fs::remove_file(path).or_else(|_| std::fs::remove_dir(path));
            }
        }
    }

    let temp = std::env::temp_dir();
    let pid = std::process::id();
    let mut planted = Planted(Vec::new());
    let mut strangers = Vec::new();

    let stranger = temp.join(format!("{prefix}-{pid}-0"));
    std::fs::create_dir(&stranger).expect("a free predictable name for the stranger");
    planted.0.push(stranger.clone());
    planted.0.push(stranger.join("sentinel"));
    std::fs::write(stranger.join("sentinel"), b"not yours").expect("the stranger's sentinel");
    strangers.push(stranger.clone());

    #[cfg(unix)]
    let (link, victim) = {
        let victim = temp.join(format!("{prefix}-victim-{pid}"));
        std::fs::create_dir(&victim).expect("a free name for the link's victim");
        planted.0.push(victim.clone());
        planted.0.push(victim.join("sentinel"));
        std::fs::write(victim.join("sentinel"), b"not yours").expect("the victim's sentinel");
        let link = temp.join(format!("{prefix}-{pid}-1"));
        std::os::unix::fs::symlink(&victim, &link).expect("the planted link");
        planted.0.push(link.clone());
        (link, victim)
    };

    for name in legacy_names {
        let legacy = temp.join(name);
        if legacy.symlink_metadata().is_ok() {
            continue;
        }
        std::fs::create_dir(&legacy).expect("a free name for the legacy candidate");
        planted.0.push(legacy.clone());
        planted.0.push(legacy.join("sentinel"));
        std::fs::write(legacy.join("sentinel"), b"not yours").expect("the legacy sentinel");
        strangers.push(legacy);
    }

    let (guard, child) = claim();
    let claimed = child
        .parent()
        .expect("a path inside the claim")
        .to_path_buf();
    assert!(
        !strangers.contains(&claimed),
        "the claim reused a stranger's name"
    );
    #[cfg(unix)]
    assert_ne!(claimed, link, "the claim reused the planted link's name");
    std::fs::write(&child, b"mine").expect("the claim is writable");

    #[cfg(unix)]
    assert_eq!(
        std::fs::read_dir(&victim).unwrap().count(),
        1,
        "the claim wrote through the planted link"
    );
    drop(guard);

    for stranger in &strangers {
        assert_eq!(
            std::fs::read(stranger.join("sentinel")).unwrap(),
            b"not yours",
            "dropping the claim reached the stranger {}",
            stranger.display()
        );
    }
    #[cfg(unix)]
    {
        assert_eq!(
            std::fs::read(victim.join("sentinel")).unwrap(),
            b"not yours",
            "dropping the claim reached the link's victim"
        );
        assert!(
            std::fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink(),
            "the planted link was replaced"
        );
    }
    #[cfg(any(unix, windows))]
    assert!(!claimed.exists(), "the claim left its own directory behind");
}

#[cfg(test)]
mod tests {
    use super::ScratchDir;

    /// The name the first claim under `root` would take for `prefix`.
    /// Recorded in `root`, so the test's own cleanup removes whatever it
    /// plants there.
    fn first_candidate(root: &ScratchDir, prefix: &str) -> std::path::PathBuf {
        root.join(format!("{prefix}-{}-0", std::process::id()))
    }

    #[test]
    fn a_claim_leaves_an_occupied_candidate_and_its_contents_alone() {
        let root = ScratchDir::new("scratch-occupied-root");
        let occupied = first_candidate(&root, "probe");
        std::fs::create_dir(&occupied).expect("the stranger's directory");
        std::fs::write(occupied.join("sentinel"), b"not yours").expect("the sentinel");

        let claimed = ScratchDir::new_in(root.path(), "probe");
        assert_ne!(claimed.path(), occupied);
        assert!(std::fs::read_dir(claimed.path()).unwrap().next().is_none());
        drop(claimed);
        assert_eq!(
            std::fs::read(occupied.join("sentinel")).unwrap(),
            b"not yours",
            "claiming and dropping must not reach the stranger's directory"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_claim_does_not_write_through_a_planted_symlink() {
        let root = ScratchDir::new("scratch-symlink-root");
        let victim = root.join("victim");
        std::fs::create_dir(&victim).expect("the victim directory");
        std::fs::write(victim.join("sentinel"), b"not yours").expect("the sentinel");
        let planted = first_candidate(&root, "probe");
        std::os::unix::fs::symlink(&victim, &planted).expect("the planted link");

        let claimed = ScratchDir::new_in(root.path(), "probe");
        assert_ne!(claimed.path(), planted);
        std::fs::write(claimed.join("scratch"), b"mine").expect("the claim is writable");
        drop(claimed);

        assert_eq!(
            std::fs::read(victim.join("sentinel")).unwrap(),
            b"not yours"
        );
        assert_eq!(
            std::fs::read_dir(&victim).unwrap().count(),
            1,
            "nothing was written through the link"
        );
        assert!(std::fs::symlink_metadata(&planted).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn a_dangling_planted_symlink_is_not_claimed() {
        let root = ScratchDir::new("scratch-dangling-root");
        let missing = root.join("missing-target");
        let planted = first_candidate(&root, "probe");
        std::os::unix::fs::symlink(&missing, &planted).expect("the dangling link");

        let claimed = ScratchDir::new_in(root.path(), "probe");
        assert_ne!(claimed.path(), planted);
        assert!(!missing.exists(), "the link's target was created");
    }

    #[cfg(unix)]
    #[test]
    fn a_claim_is_private_to_its_owner() {
        use std::os::unix::fs::PermissionsExt as _;
        let claimed = ScratchDir::new("scratch-mode");
        let mode = std::fs::metadata(claimed.path())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o700);
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn dropping_a_claim_spares_a_stranger_that_took_its_name() {
        let root = ScratchDir::new("scratch-replaced-root");
        let claimed = ScratchDir::new_in(root.path(), "probe");
        let path = root.join(
            claimed
                .path()
                .file_name()
                .and_then(|name| name.to_str())
                .expect("a utf-8 claim name"),
        );
        // Moved, not removed: a freed inode number (or Windows file ID) can
        // be handed straight to the stranger, which would make the two
        // indistinguishable.
        let moved = root.join("moved-claim");
        std::fs::rename(&path, &moved).expect("the claim is moved behind its back");
        std::fs::create_dir(&path).expect("a stranger takes the name");
        std::fs::write(path.join("sentinel"), b"not yours").expect("the sentinel");

        drop(claimed);
        assert_eq!(std::fs::read(path.join("sentinel")).unwrap(), b"not yours");
    }

    // Unix and Windows both: each verifies the root's identity before it
    // deletes a recorded name.
    #[cfg(any(unix, windows))]
    #[test]
    fn dropping_a_claim_spares_a_stranger_holding_a_recorded_name() {
        let root = ScratchDir::new("scratch-replaced-recorded-root");
        let claimed = ScratchDir::new_in(root.path(), "probe");
        let _ = claimed.join("sentinel");
        let path = root.join(
            claimed
                .path()
                .file_name()
                .and_then(|name| name.to_str())
                .expect("a utf-8 claim name"),
        );
        let moved = root.join("moved-claim");
        std::fs::rename(&path, &moved).expect("the claim is moved behind its back");
        std::fs::create_dir(&path).expect("a stranger takes the name");
        std::fs::write(path.join("sentinel"), b"not yours").expect("the sentinel");

        drop(claimed);
        assert_eq!(std::fs::read(path.join("sentinel")).unwrap(), b"not yours");
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn drop_removes_what_the_fixture_recorded_and_the_directory() {
        let root = ScratchDir::new("scratch-cleanup-root");
        let claimed = ScratchDir::new_in(root.path(), "probe");
        let path = claimed.path().to_path_buf();
        std::fs::write(claimed.join("emerald.gba"), b"rom").unwrap();
        std::fs::create_dir(claimed.join("nested")).unwrap();
        std::fs::write(claimed.join("nested").join("inner"), b"x").unwrap();
        drop(claimed);
        assert!(!path.exists());
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn drop_leaves_an_unrecorded_entry_and_its_directory_in_place() {
        let root = ScratchDir::new("scratch-unrecorded-root");
        let claimed = ScratchDir::new_in(root.path(), "probe");
        // Recorded in `root`, whose own drop sweeps up the kept directory.
        let path = root.join(
            claimed
                .path()
                .file_name()
                .and_then(|name| name.to_str())
                .expect("a utf-8 claim name"),
        );
        std::fs::write(path.join("not-via-join"), b"kept").unwrap();
        drop(claimed);
        assert_eq!(std::fs::read(path.join("not-via-join")).unwrap(), b"kept");
    }
}
