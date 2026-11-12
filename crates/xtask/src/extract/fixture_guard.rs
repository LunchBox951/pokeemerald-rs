//! Regression guard for extractor fixtures that claim their directory with
//! [`OwnedDir`](crate::extract::owned_fixture::OwnedDir).
//!
//! It runs the real fixture function in-process with something planted at
//! every name a predictable-path fixture could use, and fails if the fixture
//! deleted, overwrote or wrote through anything it did not create.

use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[cfg(unix)]
use crate::extract::owned_fixture::OwnedDir;

const SENTINEL: &str = "sentinel";
const PRECIOUS: &str = "precious";

/// `OwnedDir` tries suffixes `0..ATTEMPTS`.
const ATTEMPTS: usize = 64;
/// The suffixes the guard occupies; the rest stay free so the fixture can
/// still claim one.
const OCCUPIED_CANDIDATES: usize = 2;
/// How long planting waits for a name a concurrent run of the same fixture
/// holds. The fixture test runs once per process and holds its name for
/// milliseconds, so a name still occupied after this is a leftover.
const PLANT_WAIT: Duration = Duration::from_secs(10);

/// Unix: device and inode of the entry itself (a symlink is not followed).
#[cfg(unix)]
type Token = (u64, u64);

/// Windows: the volume and file ID read off a handle opened without
/// following a reparse point.
#[cfg(windows)]
type Token = win_fs::WindowsFileIdentity;

/// Any other target has no identity this guard can read, so it removes
/// nothing there and reports every planted entry.
#[cfg(not(any(unix, windows)))]
type Token = ();

#[derive(Debug, PartialEq, Eq)]
struct Identity {
    token: Token,
    /// An unpinned symlink's token can be recycled, so the kind bounds what a
    /// recycled match can remove.
    is_dir: bool,
}

fn identity_of(path: &Path) -> io::Result<Option<Identity>> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let is_dir = metadata.file_type().is_dir();
    #[cfg(unix)]
    let token = {
        use std::os::unix::fs::MetadataExt as _;
        (metadata.dev(), metadata.ino())
    };
    #[cfg(windows)]
    let token = win_fs::open_for_verified_delete(path)
        .and_then(|handle| win_fs::WindowsFileIdentity::of(&handle))?;
    #[cfg(not(any(unix, windows)))]
    let token: Token = return Err(io::ErrorKind::Unsupported.into());
    #[cfg(any(unix, windows))]
    Ok(Some(Identity { token, is_dir }))
}

/// Windows: opens the directory `identity` was read from and keeps the handle,
/// if it is still that directory, so a file ID is not recycled while the guard
/// holds it.
///
/// Unix does not pin: one descriptor per planted directory would exhaust a
/// 256-descriptor soft limit (the macOS default) on the voicegroup fixtures'
/// hundreds of names. A directory there is instead proved ours by the unique
/// content of its sentinel file; see [`PlantedEntry::sentinel`].
#[cfg(windows)]
fn pin(path: &Path, identity: &Identity) -> Option<std::fs::File> {
    if !identity.is_dir {
        return None;
    }
    let handle = win_fs::open_for_verified_delete(path).ok()?;
    (win_fs::WindowsFileIdentity::of(&handle).ok()? == identity.token).then_some(handle)
}

struct PlantedEntry {
    path: PathBuf,
    /// `None` when the identity could not be read at planting: the entry can
    /// then not be told from a replacement and is never removed.
    identity: Option<Identity>,
    /// Windows: held open on a planted directory so a replacement cannot
    /// recycle its identity. Never set on Unix, where it would cost a
    /// descriptor per directory.
    handle: Option<std::fs::File>,
    /// The content `populate` wrote to the directory's sentinel file, unique
    /// to this planting. A directory is removed only if it holds the handle or
    /// still has this content, so a replacement that recycled the inode (and
    /// so passes the identity compare) is left alone unless it also copied the
    /// sentinel. A symlink has none; a directory with neither is never removed.
    sentinel: Option<String>,
    /// Set for a name the plant created and then failed to populate. Its
    /// `identity` was read right after the guard created it, so the entry is
    /// touched only while that device and inode are still at the path, and
    /// then with `remove_dir` alone (which refuses a directory holding
    /// anything). For such an entry `sentinel` is the content the failed
    /// write was to leave, never read from disk.
    created_by_failed_plant: bool,
}

impl PlantedEntry {
    /// Whether the directory at `path` is the one planted, beyond its identity.
    fn proves_ownership(&self, path: &Path) -> bool {
        self.handle.is_some()
            || self.created_by_failed_plant
            || self
                .sentinel
                .as_deref()
                .is_some_and(|expected| sentinel_of(path).as_deref() == Some(expected))
    }
}

struct Planted(Vec<PlantedEntry>);

impl Planted {
    fn entry(path: &Path) -> PlantedEntry {
        Self::entry_planted(path, None, None)
    }

    /// A name whose creation identity could not be read: it can never be told
    /// from a replacement, so it is reported and never removed.
    fn entry_unproven(path: &Path) -> PlantedEntry {
        PlantedEntry {
            path: path.to_path_buf(),
            identity: None,
            handle: None,
            sentinel: None,
            created_by_failed_plant: false,
        }
    }

    /// A name the guard created and failed to populate. `created` is the
    /// identity read right after the creation; a racer's replacement carries
    /// another one and is left alone. `intended` is the sentinel content the
    /// failed write was to leave.
    fn entry_failed(path: &Path, created: Option<Identity>, intended: String) -> PlantedEntry {
        PlantedEntry {
            path: path.to_path_buf(),
            identity: created,
            handle: None,
            sentinel: Some(intended),
            created_by_failed_plant: true,
        }
    }

    /// `planted` is the sentinel content the plant wrote and `created` the
    /// identity read right after the guard created the name (`None` reads it
    /// from the path now, for an entry whose creation was not observed). A
    /// directory that does not hold the sentinel, or whose identity is not
    /// `created`, is a replacement swapped in after the plant: it is recorded
    /// without an identity, so it is reported and never removed.
    fn entry_planted(
        path: &Path,
        planted: Option<&str>,
        created: Option<&Identity>,
    ) -> PlantedEntry {
        let current = identity_of(path).ok().flatten();
        let swapped = planted.is_some_and(|planted| sentinel_of(path).as_deref() != Some(planted))
            || created.is_some_and(|created| current.as_ref() != Some(created));
        let identity = current.filter(|_| !swapped);
        let is_dir = identity.as_ref().is_some_and(|identity| identity.is_dir);
        #[cfg(windows)]
        let handle = identity.as_ref().and_then(|identity| pin(path, identity));
        #[cfg(not(windows))]
        let handle: Option<std::fs::File> = None;
        PlantedEntry {
            path: path.to_path_buf(),
            identity,
            handle,
            sentinel: if is_dir {
                planted.map(str::to_owned).or_else(|| sentinel_of(path))
            } else {
                None
            },
            created_by_failed_plant: false,
        }
    }
}

impl Drop for Planted {
    /// Removes an entry only while its path still names the one the guard
    /// planted (the rule of `OwnedDir`), and reports any left behind: by
    /// panicking, or by printing when the thread is already unwinding, so a
    /// leftover is never silent.
    fn drop(&mut self) {
        let mut failures = Vec::new();
        for entry in &mut self.0 {
            let path = &entry.path;
            match identity_of(path) {
                Ok(None) => {}
                Err(error) => failures.push(format!("{}: {error}", path.display())),
                Ok(Some(current))
                    if Some(&current) != entry.identity.as_ref()
                        || (current.is_dir && !entry.proves_ownership(path)) =>
                {
                    failures.push(format!(
                        "{}: not the entry the guard planted, left alone",
                        path.display()
                    ));
                }
                Ok(Some(current)) => {
                    // The directory cannot be removed while a handle is open
                    // on Windows.
                    drop(entry.handle.take());
                    let removed = if entry.created_by_failed_plant {
                        // The identity matched, so this is the directory the
                        // guard created unless a racer's reused its inode.
                        // Only a sentinel holding exactly the content this
                        // plant was writing is deleted. An empty or partial
                        // one cannot be told from a racer's, so it is kept
                        // and the directory is reported instead.
                        remove_own_sentinel(path, entry.sentinel.as_deref());
                        std::fs::remove_dir(path)
                    } else if current.is_dir {
                        std::fs::remove_dir_all(path)
                    } else {
                        std::fs::remove_file(path)
                    };
                    if let Err(error) = removed {
                        if error.kind() != io::ErrorKind::NotFound {
                            failures.push(format!("{}: {error}", path.display()));
                        }
                    }
                }
            }
        }
        if std::thread::panicking() {
            if !failures.is_empty() {
                eprintln!("removing planted entries failed: {failures:?}");
            }
        } else {
            assert!(
                failures.is_empty(),
                "removing planted entries failed: {failures:?}"
            );
        }
    }
}

/// Deletes `dir`'s sentinel if it is a regular file holding exactly
/// `intended`, the unique content only this plant could have written. Any
/// other content, empty or partial included, may be a racer's and is kept.
fn remove_own_sentinel(dir: &Path, intended: Option<&str>) {
    let Some(intended) = intended else { return };
    let sentinel = dir.join(SENTINEL);
    let is_file = std::fs::symlink_metadata(&sentinel).is_ok_and(|m| m.file_type().is_file());
    let ours =
        is_file && std::fs::read_to_string(&sentinel).is_ok_and(|content| content == intended);
    if ours {
        let _ = std::fs::remove_file(&sentinel);
    }
}

/// Every name a predictable-path fixture could have used, plus the first
/// `OwnedDir` candidates.
fn names(prefix: &str, line_limit: Option<usize>) -> Vec<PathBuf> {
    let base = std::env::temp_dir();
    let pid = std::process::id();
    let mut names = vec![base.join(format!("{prefix}-{pid}"))];
    names.extend((0..OCCUPIED_CANDIDATES).map(|n| base.join(format!("{prefix}-{pid}-{n}"))));
    if let Some(limit) = line_limit {
        names.extend((ATTEMPTS..=limit).map(|line| base.join(format!("{prefix}-{pid}-{line}"))));
    }
    names
}

/// A name held by a concurrent run of the same fixture is waited for, so the
/// guard owns every name before the fixture runs. `on_wait` runs each time a
/// name is found held.
///
/// `create` must claim the name exclusively (failing with `AlreadyExists`
/// when it is taken). Ownership starts when it succeeds: the identity is read
/// then, so a failed `populate` removes only the entry still carrying that
/// identity. Absence before the plant and a failed plant prove nothing. The
/// `&str` `populate` gets is the sentinel content to write.
fn plant_all(
    prefix: &str,
    names: &[PathBuf],
    create: impl Fn(&Path) -> io::Result<()>,
    populate: impl Fn(&Path, &str) -> io::Result<Option<String>>,
    on_wait: &dyn Fn(),
) -> Planted {
    let mut planted = Planted(Vec::new());
    let deadline = Instant::now() + PLANT_WAIT;
    for path in names {
        loop {
            match create(path) {
                Ok(()) => {
                    let created = identity_of(path).ok().flatten();
                    let intended = fresh_sentinel(path);
                    match populate(path, &intended) {
                        Ok(sentinel) => planted.0.push(match created.as_ref() {
                            Some(created) => {
                                Planted::entry_planted(path, sentinel.as_deref(), Some(created))
                            }
                            None => Planted::entry_unproven(path),
                        }),
                        Err(e) => {
                            planted
                                .0
                                .push(Planted::entry_failed(path, created, intended));
                            panic!("{prefix}: cannot plant {}: {e}", path.display());
                        }
                    }
                    break;
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists && Instant::now() < deadline => {
                    on_wait();
                    std::thread::sleep(Duration::from_millis(2));
                }
                Err(e) => panic!("{prefix}: cannot plant {}: {e}", path.display()),
            }
        }
    }
    planted
}

/// The sorted entry names in `dir`, or `None` when it cannot be listed.
fn listing(dir: &Path) -> Option<Vec<String>> {
    let mut names = std::fs::read_dir(dir)
        .ok()?
        .map(|e| e.ok()?.file_name().into_string().ok())
        .collect::<Option<Vec<_>>>()?;
    names.sort();
    Some(names)
}

/// `PRECIOUS` plus a suffix no other planting shares: the process, the clock
/// and the directory, so no state is shared between plantings.
fn fresh_sentinel(dir: &Path) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!(
        "{PRECIOUS}-{}-{nanos}-{}",
        std::process::id(),
        dir.display()
    )
}

/// Writes the sentinel and returns its content.
fn populate(dir: &Path) -> io::Result<String> {
    let content = fresh_sentinel(dir);
    write_sentinel(dir, &content)?;
    Ok(content)
}

fn write_sentinel(dir: &Path, content: &str) -> io::Result<()> {
    std::fs::write(dir.join(SENTINEL), content)
}

fn sentinel_of(dir: &Path) -> Option<String> {
    std::fs::read_to_string(dir.join(SENTINEL)).ok()
}

/// `dir` holds only the sentinel, with `expected` content or, when it is
/// `None`, any content `populate` writes.
fn untouched_as(dir: &Path, expected: Option<&str>) -> bool {
    listing(dir).as_deref() == Some(&[SENTINEL.to_owned()][..])
        && sentinel_of(dir).is_some_and(|content| match expected {
            Some(expected) => content == expected,
            None => content.starts_with(PRECIOUS),
        })
}

fn untouched(dir: &Path) -> bool {
    untouched_as(dir, None)
}

/// Runs `fixture`, the real test function, with something planted at each
/// former and candidate name for `prefix`, and fails if it touched them.
///
/// A second pass (Unix) plants symlinks, which see a write-through or an
/// unlink that leaves every populated directory intact.
///
/// `line_limit` is the highest line a fixture that appended `line!()` to its
/// former name could have been on; pass the line count of its file.
pub(crate) fn assert_fixture_ignores_planted_names(
    prefix: &str,
    line_limit: Option<usize>,
    fixture: fn(),
) {
    assert_fixture_ignores_planted_names_observing(prefix, line_limit, fixture, &|| {});
}

/// [`assert_fixture_ignores_planted_names`] with `on_wait`, so a test can
/// release a holder on an observed planting attempt instead of a timer.
fn assert_fixture_ignores_planted_names_observing(
    prefix: &str,
    line_limit: Option<usize>,
    fixture: fn(),
    on_wait: &dyn Fn(),
) {
    let names = names(prefix, line_limit);

    let dirs = plant_all(
        prefix,
        &names,
        |path| std::fs::create_dir(path),
        |path, content| write_sentinel(path, content).map(|()| Some(content.to_owned())),
        on_wait,
    );
    fixture();
    let disturbed: Vec<_> = dirs
        .0
        .iter()
        .filter(|e| !untouched_as(&e.path, e.sentinel.as_deref()))
        .map(|e| e.path.clone())
        .collect();
    assert!(
        disturbed.is_empty(),
        "{prefix}: fixture disturbed populated directories it never created: {disturbed:?}"
    );
    drop(dirs);

    #[cfg(unix)]
    {
        let victim = OwnedDir::create("pokeemerald-rs-fixture-guard-victim").unwrap();
        let victim_sentinel = populate(victim.path()).unwrap();
        let links = plant_all(
            prefix,
            &names,
            |path| std::os::unix::fs::symlink(victim.path(), path),
            |_, _| Ok(None),
            on_wait,
        );
        fixture();
        let removed: Vec<_> = links
            .0
            .iter()
            .filter(|e| std::fs::read_link(&e.path).ok().as_deref() != Some(victim.path()))
            .map(|e| e.path.clone())
            .collect();
        let victim_untouched = untouched_as(victim.path(), Some(&victim_sentinel));
        assert!(
            removed.is_empty(),
            "{prefix}: fixture removed or replaced symlinks it never created: {removed:?}"
        );
        assert!(
            victim_untouched,
            "{prefix}: fixture wrote through a planted symlink"
        );
        drop(links);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::owned_fixture::OwnedDir;

    const PREFIX: &str = "pokeemerald-rs-fixture-guard-sibling";

    /// Joins the sibling even when a panic unwinds past it.
    struct Sibling {
        release: std::sync::mpsc::Sender<()>,
        thread: Option<std::thread::JoinHandle<()>>,
    }
    impl Drop for Sibling {
        fn drop(&mut self) {
            let _ = self.release.send(());
            self.thread.take().unwrap().join().unwrap();
        }
    }

    fn fixture_reusing_candidate_zero() {
        let dir = std::env::temp_dir().join(format!("{PREFIX}-{}-0", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join("scratch"), "x").unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The sibling frees the name only after the guard reports meeting it
    /// held, so a guard that skipped occupied names fails on any schedule.
    #[test]
    #[should_panic(expected = "disturbed populated directories")]
    fn guard_catches_a_fixture_that_reuses_candidate_zero_while_a_sibling_held_it_at_planting() {
        let (claimed, claimed_rx) = std::sync::mpsc::channel();
        let (release, release_rx) = std::sync::mpsc::channel::<()>();
        let thread = std::thread::spawn(move || {
            let held = OwnedDir::create(PREFIX).unwrap();
            claimed.send(()).unwrap();
            release_rx.recv().unwrap();
            drop(held);
        });
        claimed_rx.recv().unwrap();
        let wait_report = release.clone();
        let _sibling = Sibling {
            release,
            thread: Some(thread),
        };
        assert_fixture_ignores_planted_names_observing(
            PREFIX,
            None,
            fixture_reusing_candidate_zero,
            &|| {
                let _ = wait_report.send(());
            },
        );
    }

    /// A directory the guard planted but cannot remove must be reported. A
    /// failed plant removes with `remove_dir`, which refuses a directory that
    /// holds anything, whoever runs the test (directory modes do not stop root).
    #[test]
    fn planted_drop_reports_a_failed_removal() {
        let dir = std::env::temp_dir().join(format!(
            "pokeemerald-rs-fixture-guard-unremovable-{}",
            std::process::id()
        ));
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join("racer"), "theirs").unwrap();
        let mut entry = Planted::entry(&dir);
        entry.created_by_failed_plant = true;
        let planted = Planted(vec![entry]);
        let reported = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(planted)));
        let left_behind = dir.join("racer").exists();
        let _ = std::fs::remove_dir_all(&dir);
        let payload = reported.expect_err("Planted::drop left the directory without reporting it");
        let message = payload
            .downcast_ref::<String>()
            .cloned()
            .unwrap_or_default();
        assert!(
            left_behind,
            "Planted::drop deleted a directory it did not populate"
        );
        assert!(
            message.contains("removing planted entries failed")
                && message.contains(&dir.display().to_string()),
            "unexpected report: {message}"
        );
    }

    /// The plant creates its directory, a racer swaps in its own directory
    /// holding a sentinel before `populate` writes, and the write fails. The
    /// racer's sentinel must survive cleanup.
    #[test]
    fn a_replacement_swapped_in_during_a_failed_plant_keeps_its_sentinel() {
        let path = std::env::temp_dir().join(format!(
            "pokeemerald-rs-fixture-guard-failed-plant-swap-{}",
            std::process::id()
        ));
        let names = vec![path.clone()];
        let result = std::panic::catch_unwind(|| {
            plant_all(
                "failed-plant-swap",
                &names,
                |p| std::fs::create_dir(p),
                |p, _| {
                    std::fs::remove_dir(p).unwrap();
                    std::fs::create_dir(p).unwrap();
                    std::fs::write(p.join(SENTINEL), "theirs").unwrap();
                    Err::<Option<String>, _>(io::Error::from(io::ErrorKind::PermissionDenied))
                },
                &|| {},
            )
        });
        let theirs = sentinel_of(&path);
        let _ = std::fs::remove_dir_all(&path);
        assert!(result.is_err(), "plant_all did not panic");
        assert_eq!(
            theirs.as_deref(),
            Some("theirs"),
            "cleanup deleted the racer's sentinel"
        );
    }

    /// Same race with an empty sentinel: the racer's directory is
    /// indistinguishable from a half-written plant by content, so it is the
    /// identity captured at creation that must keep the guard off it.
    #[test]
    fn a_replacement_with_an_empty_sentinel_swapped_in_during_a_failed_plant_survives() {
        let path = std::env::temp_dir().join(format!(
            "pokeemerald-rs-fixture-guard-failed-plant-empty-swap-{}",
            std::process::id()
        ));
        let names = vec![path.clone()];
        let result = std::panic::catch_unwind(|| {
            plant_all(
                "failed-plant-empty-swap",
                &names,
                |p| std::fs::create_dir(p),
                |p, _| {
                    std::fs::remove_dir(p).unwrap();
                    std::fs::create_dir(p).unwrap();
                    std::fs::write(p.join(SENTINEL), "").unwrap();
                    Err::<Option<String>, _>(io::Error::from(io::ErrorKind::PermissionDenied))
                },
                &|| {},
            )
        });
        let survived = path.is_dir() && sentinel_of(&path).as_deref() == Some("");
        let _ = std::fs::remove_dir_all(&path);
        assert!(result.is_err(), "plant_all did not panic");
        assert!(
            survived,
            "cleanup deleted the racer's directory or its empty sentinel"
        );
    }

    /// The guard's own sentinel, written in full before the plant failed, is
    /// cleaned up with the directory it created.
    #[test]
    fn plant_all_removes_its_own_sentinel_after_a_failed_plant() {
        let path = std::env::temp_dir().join(format!(
            "pokeemerald-rs-fixture-guard-own-sentinel-{}",
            std::process::id()
        ));
        let names = vec![path.clone()];
        let result = std::panic::catch_unwind(|| {
            plant_all(
                "own-sentinel",
                &names,
                |p| std::fs::create_dir(p),
                |p, content| {
                    std::fs::write(p.join(SENTINEL), content).unwrap();
                    Err::<Option<String>, _>(io::Error::other("injected late failure"))
                },
                &|| {},
            )
        });
        let left_behind = path.exists();
        let _ = std::fs::remove_dir_all(&path);
        assert!(result.is_err(), "plant_all did not panic");
        assert!(!left_behind, "plant_all left its own sentinel");
    }

    #[cfg(unix)]
    #[test]
    fn planted_drop_leaves_an_unrelated_replacement() {
        let path = std::env::temp_dir().join(format!(
            "pokeemerald-rs-fixture-guard-replaced-{}",
            std::process::id()
        ));
        let aside = path.with_extension("aside");
        std::fs::create_dir(&path).unwrap();
        populate(&path).unwrap();
        let planted = Planted(vec![Planted::entry(&path)]);
        std::fs::rename(&path, &aside).unwrap();
        std::fs::create_dir(&path).unwrap();
        populate(&path).unwrap();
        let reported = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(planted)));
        let survived = untouched(&path);
        let _ = std::fs::remove_dir_all(&path);
        let _ = std::fs::remove_dir_all(&aside);
        assert!(
            survived,
            "Planted::drop deleted an unrelated directory at {}",
            path.display()
        );
        assert!(
            reported.is_err(),
            "Planted::drop skipped {} without reporting it",
            path.display()
        );
    }

    /// A replacement that recycled the planted inode passes the identity
    /// compare; only the sentinel tells it apart. The test keeps the directory
    /// and its identity fixed and changes only the sentinel, so it reaches that
    /// check on any filesystem.
    #[test]
    fn planted_drop_leaves_a_directory_whose_identity_matches_but_sentinel_differs() {
        let path = std::env::temp_dir().join(format!(
            "pokeemerald-rs-fixture-guard-recycled-{}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        populate(&path).unwrap();
        let planted = Planted(vec![Planted::entry(&path)]);
        std::fs::write(path.join(SENTINEL), "precious-theirs").unwrap();
        let reported = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(planted)));
        let survived = sentinel_of(&path).as_deref() == Some("precious-theirs");
        let _ = std::fs::remove_dir_all(&path);
        assert!(
            survived,
            "Planted::drop deleted a directory with another sentinel at {}",
            path.display()
        );
        assert!(reported.is_err(), "the kept directory went unreported");
    }

    /// The identity read right after creation, not one read later from the
    /// path, decides whether a directory is the one planted: a replacement
    /// that already holds the planted sentinel is still not adopted.
    #[test]
    fn a_replacement_that_copied_the_sentinel_does_not_match_the_creation_identity() {
        let path = std::env::temp_dir().join(format!(
            "pokeemerald-rs-fixture-guard-copied-sentinel-{}",
            std::process::id()
        ));
        let aside = path.with_extension("aside");
        std::fs::create_dir(&path).unwrap();
        let content = populate(&path).unwrap();
        let created = identity_of(&path).unwrap().unwrap();
        std::fs::rename(&path, &aside).unwrap();
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join(SENTINEL), &content).unwrap();
        let planted = Planted(vec![Planted::entry_planted(
            &path,
            Some(&content),
            Some(&created),
        )]);
        let reported = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(planted)));
        let survived = untouched_as(&path, Some(&content));
        let _ = std::fs::remove_dir_all(&path);
        let _ = std::fs::remove_dir_all(&aside);
        assert!(survived, "the replacement was adopted and deleted");
        assert!(reported.is_err(), "the replacement went unreported");
    }

    /// A populated directory swapped in between the plant and the guard's
    /// record of it must be reported and kept, not adopted and deleted.
    #[test]
    fn a_replacement_swapped_in_before_the_record_is_not_adopted() {
        let path = std::env::temp_dir().join(format!(
            "pokeemerald-rs-fixture-guard-swapped-{}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        let planted_sentinel = populate(&path).unwrap();
        std::fs::remove_dir_all(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        populate(&path).unwrap();
        let planted = Planted(vec![Planted::entry_planted(
            &path,
            Some(&planted_sentinel),
            None,
        )]);
        let reported = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(planted)));
        let survived = untouched(&path);
        let _ = std::fs::remove_dir_all(&path);
        assert!(survived, "the swapped-in directory was adopted and deleted");
        assert!(
            reported.is_err(),
            "the swapped-in directory went unreported"
        );
    }

    #[test]
    fn plant_all_removes_a_directory_whose_population_failed() {
        let path = std::env::temp_dir().join(format!(
            "pokeemerald-rs-fixture-guard-populate-fails-{}",
            std::process::id()
        ));
        let names = vec![path.clone()];
        let result = std::panic::catch_unwind(|| {
            plant_all(
                "populate-fails",
                &names,
                |p| std::fs::create_dir(p),
                |_, _| Err::<Option<String>, _>(io::Error::other("injected population failure")),
                &|| {},
            )
        });
        let left_behind = path.exists();
        let _ = std::fs::remove_dir_all(&path);
        assert!(
            result.is_err(),
            "plant_all did not panic on population failure"
        );
        assert!(
            !left_behind,
            "plant_all left {} behind after population failed",
            path.display()
        );
    }

    #[cfg(unix)]
    const SYMLINK_PREFIX: &str = "pokeemerald-rs-fixture-guard-symlink-only";

    #[cfg(unix)]
    fn fixture_unlinking_only_a_symlink() {
        let path = std::env::temp_dir().join(format!("{SYMLINK_PREFIX}-{}-0", std::process::id()));
        if path.is_symlink() {
            std::fs::remove_file(&path).unwrap();
        }
    }

    /// Only the symlink pass can see this fixture: the directory pass finds
    /// every populated directory intact.
    #[cfg(unix)]
    #[test]
    #[should_panic(expected = "removed or replaced symlinks")]
    fn guard_catches_a_symlink_only_disturbance_with_directories_intact() {
        assert_fixture_ignores_planted_names(
            SYMLINK_PREFIX,
            None,
            fixture_unlinking_only_a_symlink,
        );
    }

    #[cfg(unix)]
    const WRITE_THROUGH_PREFIX: &str = "pokeemerald-rs-fixture-guard-write-through";

    #[cfg(unix)]
    fn fixture_writing_through_only_a_symlink() {
        let path =
            std::env::temp_dir().join(format!("{WRITE_THROUGH_PREFIX}-{}-0", std::process::id()));
        if path.is_symlink() {
            std::fs::write(path.join("scratch"), "x").unwrap();
        }
    }

    #[cfg(unix)]
    #[test]
    #[should_panic(expected = "wrote through a planted symlink")]
    fn guard_catches_a_write_through_a_symlink_with_directories_intact() {
        assert_fixture_ignores_planted_names(
            WRITE_THROUGH_PREFIX,
            None,
            fixture_writing_through_only_a_symlink,
        );
    }

    /// A directory-pinning descriptor per planted name exhausted the 256
    /// descriptor soft limit macOS sets by default: the voicegroup fixtures
    /// plant hundreds of names, so planting failed with EMFILE.
    #[cfg(unix)]
    mod descriptor_limit {
        use super::*;

        const CHILD_ENV: &str = "FIXTURE_GUARD_DESCRIPTOR_LIMIT_CHILD";
        const PANIC_PREFIX: &str = "pokeemerald-rs-fixture-guard-descriptor-limit";
        const VOICEGROUP_TEST: &str = "extract::voicegroups::tests::\
            voicegroup_fixtures_ignore_everything_planted_at_their_former_and_candidate_names";
        const CHILD_TEST: &str =
            "extract::fixture_guard::tests::descriptor_limit::panicking_fixture_child";
        /// More names than the soft limit has descriptors.
        const LINE_LIMIT: usize = 700;

        fn fixture_that_panics() {
            panic!("fixture panicked");
        }

        /// Runs only in the child `run_under_the_limit` starts, which has the
        /// descriptor limit set.
        #[test]
        fn panicking_fixture_child() {
            if std::env::var_os(CHILD_ENV).is_none() {
                return;
            }
            assert!(names(PANIC_PREFIX, Some(LINE_LIMIT)).len() > 256);
            let payload = std::panic::catch_unwind(|| {
                assert_fixture_ignores_planted_names(
                    PANIC_PREFIX,
                    Some(LINE_LIMIT),
                    fixture_that_panics,
                );
            })
            .expect_err("the fixture's panic must propagate");
            let message = payload
                .downcast_ref::<&str>()
                .map(|m| (*m).to_owned())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_default();
            assert!(
                message.contains("fixture panicked"),
                "planting failed before the fixture ran: {message}"
            );
        }

        /// Runs `test` alone in a child whose soft descriptor limit is 256, and
        /// returns the child's pid and output.
        fn run_under_the_limit(test: &str) -> (u32, std::process::Output) {
            let child = std::process::Command::new("sh")
                .args(["-c", "ulimit -Sn 256 && exec \"$0\" \"$@\""])
                .arg(std::env::current_exe().unwrap())
                .args(["--exact", test, "--test-threads=1"])
                .env(CHILD_ENV, "1")
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            let pid = child.id();
            (pid, child.wait_with_output().unwrap())
        }

        fn leftovers(prefix: &str, pid: u32) -> Vec<String> {
            let own = format!("-{pid}");
            std::fs::read_dir(std::env::temp_dir())
                .unwrap()
                .filter_map(|e| e.ok()?.file_name().into_string().ok())
                .filter(|name| {
                    name.starts_with(prefix)
                        && (name.ends_with(&own) || name.contains(&format!("{own}-")))
                })
                .collect()
        }

        fn assert_passed(what: &str, output: &std::process::Output) {
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(
                output.status.success() && stdout.contains("1 passed"),
                "{what} under a 256-descriptor soft limit: {}\n{stdout}\n{}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            );
        }

        #[test]
        fn the_voicegroup_fixtures_are_planted_under_a_256_descriptor_soft_limit() {
            let (pid, output) = run_under_the_limit(VOICEGROUP_TEST);
            let left = leftovers("pokeemerald-rs-voicegroup-", pid);
            assert_passed("the voicegroup guard", &output);
            assert!(left.is_empty(), "planted directories left behind: {left:?}");
        }

        #[test]
        fn a_panicking_fixture_leaves_no_planted_directory_under_a_256_descriptor_soft_limit() {
            let (pid, output) = run_under_the_limit(CHILD_TEST);
            let left = leftovers(PANIC_PREFIX, pid);
            assert_passed("the panicking fixture", &output);
            assert!(left.is_empty(), "planted directories left behind: {left:?}");
        }
    }
}
