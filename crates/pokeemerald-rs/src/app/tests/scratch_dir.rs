//! A scratch directory a test claims exclusively and removes only if it still owns it.

/// A scratch directory's identity, read from the handle the guard keeps open
/// so [`Drop`] can tell the directory it created from anything later placed
/// at the same path. Holding that handle also pins the inode, so removing the
/// directory and creating another cannot hand the replacement the same
/// identity.
#[cfg(unix)]
#[derive(PartialEq, Eq)]
struct DirIdentity(u64, u64);

#[cfg(unix)]
impl DirIdentity {
    fn of(meta: &std::fs::Metadata) -> Self {
        use std::os::unix::fs::MetadataExt as _;
        Self(meta.dev(), meta.ino())
    }
}

/// Windows reads the volume serial number and file ID off the open handle
/// (`win-fs` owns that query).
#[cfg(windows)]
type DirIdentity = win_fs::WindowsFileIdentity;

/// No supported identity: [`Drop`] leaves the directory in place rather than
/// risk removing one it cannot prove it owns.
#[cfg(not(any(unix, windows)))]
struct DirIdentity;

/// A scratch directory this test created exclusively and so owns. The path
/// is claimed with `create_dir` (which fails on an existing entry), an
/// occupied candidate is skipped rather than reused, and `Drop` removes only
/// the directory this guard created.
pub(super) struct ScratchDir {
    path: std::path::PathBuf,
    identity: DirIdentity,
    /// Kept open until `Drop` has finished: it pins the inode the identity
    /// names and is the handle Linux cleanup removes the contents through.
    pin: Option<std::fs::File>,
}

impl ScratchDir {
    pub(super) fn new(prefix: &str) -> Self {
        Self::claim_in(&std::env::temp_dir(), prefix)
    }

    pub(super) fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// Claims the first unoccupied `<prefix>-<pid>-<n>` under `parent`. The
    /// directory is created `0700` on Unix, so no other user can place an
    /// entry in it.
    pub(super) fn claim_in(parent: &std::path::Path, prefix: &str) -> Self {
        for n in 0..u32::MAX {
            let candidate = parent.join(format!("{prefix}-{}-{n}", std::process::id()));
            #[cfg(unix)]
            let builder = {
                use std::os::unix::fs::DirBuilderExt as _;
                let mut builder = std::fs::DirBuilder::new();
                builder.mode(0o700);
                builder
            };
            #[cfg(not(unix))]
            let builder = std::fs::DirBuilder::new();
            match builder.create(&candidate) {
                Ok(()) => return Self::pin(candidate),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => panic!("cannot create scratch dir {}: {e}", candidate.display()),
            }
        }
        panic!("no unoccupied scratch dir candidate for {prefix}");
    }

    /// Opens the directory `create_dir` just made, keeps the handle and
    /// records its identity. The directory must still be the empty real
    /// directory just created; anything else means it was replaced between
    /// `create_dir` and this open, so the guard refuses to own it. That
    /// window is the one this guard cannot close: `std` has no
    /// `mkdir`-returning-a-handle, and a same-user process that swaps an
    /// empty directory in right there is indistinguishable from ours.
    fn pin(path: std::path::PathBuf) -> Self {
        #[cfg(not(windows))]
        let pin = std::fs::File::open(&path);
        #[cfg(windows)]
        let pin = win_fs::open_for_verified_delete(&path);
        let pin = pin.unwrap_or_else(|e| {
            let _ = std::fs::remove_dir(&path);
            panic!("cannot pin scratch dir {}: {e}", path.display())
        });
        #[cfg(unix)]
        let identity = {
            let found = DirIdentity::of(&pin.metadata().expect("the scratch dir has metadata"));
            let named = std::fs::symlink_metadata(&path)
                .ok()
                .filter(|meta| meta.file_type().is_dir())
                .map(|meta| DirIdentity::of(&meta));
            assert!(
                named.as_ref() == Some(&found),
                "scratch dir {} was replaced right after it was created",
                path.display()
            );
            found
        };
        #[cfg(windows)]
        let identity =
            win_fs::WindowsFileIdentity::of(&pin).expect("the scratch dir has a file identity");
        #[cfg(not(any(unix, windows)))]
        let identity = DirIdentity;
        assert!(
            std::fs::read_dir(&path).is_ok_and(|mut entries| entries.next().is_none()),
            "scratch dir {} was not empty right after it was created",
            path.display()
        );
        Self {
            path,
            identity,
            pin: Some(pin),
        }
    }

    /// Removes the contents of the directory the retained handle names, even
    /// if that directory has since been renamed or removed. `/proc/self/fd/N`
    /// resolves to the pinned inode itself, so no pathname lookup can be
    /// redirected to a replacement; each child is removed without following
    /// links.
    #[cfg(any(target_os = "linux", target_os = "android"))]
    fn clear_pinned_contents(pin: &std::fs::File) {
        use std::os::fd::AsRawFd as _;
        let root = std::path::PathBuf::from(format!("/proc/self/fd/{}", pin.as_raw_fd()));
        let Ok(entries) = std::fs::read_dir(&root) else {
            return;
        };
        for entry in entries.flatten() {
            let child = root.join(entry.file_name());
            let _ = match std::fs::symlink_metadata(&child) {
                Ok(meta) if meta.file_type().is_dir() => std::fs::remove_dir_all(&child),
                _ => std::fs::remove_file(&child),
            };
        }
    }

    /// Whether the path still names the object the handle was opened on.
    #[cfg(unix)]
    fn path_is_still_claimed(&self) -> bool {
        std::fs::symlink_metadata(&self.path)
            .is_ok_and(|meta| meta.file_type().is_dir() && DirIdentity::of(&meta) == self.identity)
    }
}

impl Drop for ScratchDir {
    /// Removes the directory this guard created and what is inside it, and
    /// nothing else.
    ///
    /// On Linux the contents go through `/proc/self/fd/N` of the retained
    /// handle, which names the claimed inode whatever the path now points
    /// at. A directory itself cannot be removed through a descriptor with
    /// `std`, so it is then removed by path with `remove_dir` -- which
    /// refuses a directory that still holds anything -- and only while the
    /// path still has the recorded device and inode. A replacement is never
    /// emptied, and the residual is a same-user process that swaps an empty
    /// directory in between that check and the `remove_dir`, which loses
    /// only an empty directory.
    ///
    /// Other Unix targets have no way to reach the contents through the
    /// handle, so they re-check the identity immediately before
    /// `remove_dir_all` on the path; a same-user process that swaps the path
    /// inside that window still has the swapped tree removed. Windows
    /// verifies the identity through a fresh `win-fs` handle held open across
    /// the removal, with the same residual for the path-based `remove_dir_all`.
    /// A target with no readable identity removes nothing.
    fn drop(&mut self) {
        #[cfg(any(target_os = "linux", target_os = "android"))]
        if let Some(pin) = self.pin.as_ref() {
            Self::clear_pinned_contents(pin);
            if self.path_is_still_claimed() {
                let _ = std::fs::remove_dir(&self.path);
            }
        }
        #[cfg(all(unix, not(any(target_os = "linux", target_os = "android"))))]
        if self.path_is_still_claimed() {
            let _ = std::fs::remove_dir_all(&self.path);
        }
        #[cfg(windows)]
        if let Ok(handle) = win_fs::open_for_verified_delete(&self.path) {
            if win_fs::WindowsFileIdentity::of(&handle).ok() == Some(self.identity) {
                let _ = std::fs::remove_dir_all(&self.path);
            }
        }
        // Close the pinned handle last, so a platform that defers a delete
        // until the last handle closes has finished with it.
        drop(self.pin.take());
    }
}

/// `ScratchDir` skips an occupied candidate, leaves its contents intact, and
/// on drop removes only the directory it created.
#[test]
fn scratch_dir_skips_occupied_candidates_and_removes_only_its_own() {
    let outer = ScratchDir::new("pokeemerald-scratch-guard");
    let occupied = outer.path().join(format!("probe-{}-0", std::process::id()));
    std::fs::create_dir(&occupied).unwrap();
    let sentinel = occupied.join("unrelated.txt");
    std::fs::write(&sentinel, b"not yours").unwrap();

    let owned_path = {
        let owned = ScratchDir::claim_in(outer.path(), "probe");
        assert_ne!(owned.path(), occupied, "an occupied candidate was reused");
        assert!(owned.path().is_dir());
        owned.path().to_path_buf()
    };

    assert!(!owned_path.exists(), "the owned directory was not removed");
    assert!(sentinel.exists(), "the occupied directory was disturbed");
}

/// The claimed directory is removed and a populated one planted at the same
/// path before the guard drops: the replacement is left alone.
#[cfg(unix)]
#[test]
fn scratch_dir_drop_spares_a_replacement_it_did_not_create() {
    let outer = ScratchDir::new("pokeemerald-scratch-replaced");
    let owned = ScratchDir::claim_in(outer.path(), "probe");
    let path = owned.path().to_path_buf();
    std::fs::remove_dir(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    let sentinel = path.join("unrelated.txt");
    std::fs::write(&sentinel, b"not yours").unwrap();

    drop(owned);

    assert_eq!(
        std::fs::read(&sentinel).ok().as_deref(),
        Some(&b"not yours"[..]),
        "drop deleted a replacement it never created"
    );
}

/// The claimed directory is renamed aside and a populated one planted in its
/// place: the replacement survives on every platform.
#[test]
fn scratch_dir_drop_spares_a_replacement_planted_after_a_rename() {
    let outer = ScratchDir::new("pokeemerald-scratch-renamed");
    let owned = ScratchDir::claim_in(outer.path(), "probe");
    let path = owned.path().to_path_buf();
    let aside = outer.path().join("aside");
    std::fs::rename(&path, &aside).unwrap();
    std::fs::create_dir(&path).unwrap();
    let sentinel = path.join("unrelated.txt");
    std::fs::write(&sentinel, b"not yours").unwrap();

    drop(owned);

    assert_eq!(
        std::fs::read(&sentinel).ok().as_deref(),
        Some(&b"not yours"[..]),
        "drop deleted a replacement it never created"
    );
}

/// A symlink swapped in at the claimed path is neither followed nor removed.
#[cfg(unix)]
#[test]
fn scratch_dir_drop_does_not_follow_a_symlink_swapped_in_after_the_claim() {
    let outer = ScratchDir::new("pokeemerald-scratch-link");
    let victim = ScratchDir::claim_in(outer.path(), "victim");
    let sentinel = victim.path().join("unrelated.txt");
    std::fs::write(&sentinel, b"not yours").unwrap();
    let owned = ScratchDir::claim_in(outer.path(), "probe");
    let path = owned.path().to_path_buf();
    std::fs::remove_dir(&path).unwrap();
    std::os::unix::fs::symlink(victim.path(), &path).unwrap();

    drop(owned);

    assert_eq!(
        std::fs::read(&sentinel).ok().as_deref(),
        Some(&b"not yours"[..]),
        "drop followed a symlink into a directory it never created"
    );
    assert!(std::fs::symlink_metadata(&path).unwrap().is_symlink());
}

/// With the path untouched, drop still removes the whole claimed tree.
#[test]
fn scratch_dir_drop_removes_the_whole_claimed_tree() {
    let outer = ScratchDir::new("pokeemerald-scratch-tree");
    let owned = ScratchDir::claim_in(outer.path(), "probe");
    let path = owned.path().to_path_buf();
    std::fs::create_dir_all(path.join("a").join("b")).unwrap();
    std::fs::write(path.join("a").join("b").join("f.bin"), b"mine").unwrap();
    std::fs::write(path.join("top.bin"), b"mine").unwrap();

    drop(owned);

    assert!(!path.exists(), "the claimed tree was not removed");
}

/// The claimed directory is private to its owner.
#[cfg(unix)]
#[test]
fn scratch_dir_is_private_to_its_owner() {
    use std::os::unix::fs::PermissionsExt as _;
    let owned = ScratchDir::new("pokeemerald-scratch-mode");
    let mode = std::fs::metadata(owned.path())
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o077, 0, "mode {mode:o}");
}
