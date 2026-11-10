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
    /// Set for a name the plant created and then failed on, before it could
    /// write a sentinel. It is removed only if empty, so a populated
    /// replacement swapped in meanwhile is reported and kept.
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
        Self::entry_planted(path, None)
    }

    /// `planted` is the sentinel content the plant wrote. A directory that
    /// does not hold it is a replacement swapped in after the plant: it is
    /// recorded without an identity, so it is reported and never removed.
    fn entry_planted(path: &Path, planted: Option<&str>) -> PlantedEntry {
        let swapped = planted.is_some_and(|planted| sentinel_of(path).as_deref() != Some(planted));
        let identity = identity_of(path).ok().flatten().filter(|_| !swapped);
        let is_dir = identity.as_ref().is_some_and(|identity| identity.is_dir);
        #[cfg(windows)]
        let handle = identity.as_ref().and_then(|identity| pin(path, identity));
        #[cfg(not(windows))]
        let handle: Option<std::fs::File> = None;
        PlantedEntry {
            path: path.to_path_buf(),
            identity,
            handle,
            sentinel: if is_dir { sentinel_of(path) } else { None },
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
                        // Only an empty directory (bar a half-written
                        // sentinel): a populated replacement is not adopted.
                        let _ = std::fs::remove_file(path.join(SENTINEL));
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
fn plant_all(
    prefix: &str,
    names: &[PathBuf],
    plant: impl Fn(&Path) -> io::Result<Option<String>>,
    on_wait: &dyn Fn(),
) -> Planted {
    let mut planted = Planted(Vec::new());
    let deadline = Instant::now() + PLANT_WAIT;
    for path in names {
        loop {
            let absent =
                std::fs::symlink_metadata(path).is_err_and(|e| e.kind() == io::ErrorKind::NotFound);
            match plant(path) {
                Ok(sentinel) => {
                    planted
                        .0
                        .push(Planted::entry_planted(path, sentinel.as_deref()));
                    break;
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists && Instant::now() < deadline => {
                    on_wait();
                    std::thread::sleep(Duration::from_millis(2));
                }
                Err(e) => {
                    // A name free before the plant and taken after it is the
                    // guard's even though the plant failed; record it for cleanup.
                    if absent
                        && e.kind() != io::ErrorKind::AlreadyExists
                        && std::fs::symlink_metadata(path).is_ok()
                    {
                        let mut entry = Planted::entry(path);
                        entry.created_by_failed_plant = true;
                        planted.0.push(entry);
                    }
                    panic!("{prefix}: cannot plant {}: {e}", path.display());
                }
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

/// `PRECIOUS` plus a suffix no other planting shares: process, a counter and
/// the clock.
fn fresh_sentinel() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!(
        "{PRECIOUS}-{}-{}-{nanos}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

/// Writes the sentinel and returns its content.
fn populate(dir: &Path) -> io::Result<String> {
    let content = fresh_sentinel();
    std::fs::write(dir.join(SENTINEL), &content)?;
    Ok(content)
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
        |path| {
            std::fs::create_dir(path)?;
            populate(path).map(Some)
        },
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
        populate(victim.path()).unwrap();
        let links = plant_all(
            prefix,
            &names,
            |path| std::os::unix::fs::symlink(victim.path(), path).map(|()| None),
            on_wait,
        );
        fixture();
        let removed: Vec<_> = links
            .0
            .iter()
            .filter(|e| std::fs::read_link(&e.path).ok().as_deref() != Some(victim.path()))
            .map(|e| e.path.clone())
            .collect();
        let victim_untouched = untouched(victim.path());
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

    #[cfg(unix)]
    #[test]
    fn planted_drop_reports_a_failed_removal() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!(
            "pokeemerald-rs-fixture-guard-undeletable-{}",
            std::process::id()
        ));
        std::fs::create_dir(&dir).unwrap();
        populate(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
        let planted = Planted(vec![Planted::entry(&dir)]);
        let reported = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(planted)));
        let left_behind = dir.exists();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).ok();
        let _ = std::fs::remove_dir_all(&dir);
        if !left_behind {
            eprintln!("skipped: the process ignores directory modes (root), so 0555 is removable");
            return;
        }
        assert!(
            reported.is_err(),
            "Planted::drop left {} behind without reporting it",
            dir.display()
        );
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

    /// The replacement taking the freed inode is the case a bare identity
    /// compare cannot tell apart.
    #[cfg(unix)]
    #[test]
    fn planted_drop_leaves_a_replacement_that_recycled_the_inode() {
        use std::os::unix::fs::MetadataExt as _;
        let path = std::env::temp_dir().join(format!(
            "pokeemerald-rs-fixture-guard-recycled-{}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        populate(&path).unwrap();
        let planted = Planted(vec![Planted::entry(&path)]);
        let before = std::fs::symlink_metadata(&path).unwrap().ino();
        std::fs::remove_dir_all(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        populate(&path).unwrap();
        let after = std::fs::symlink_metadata(&path).unwrap().ino();
        eprintln!("planted inode {before}, replacement inode {after}");
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(planted)));
        let survived = untouched(&path);
        let _ = std::fs::remove_dir_all(&path);
        assert!(
            survived,
            "Planted::drop deleted an unrelated directory at {}",
            path.display()
        );
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
        let planted = Planted(vec![Planted::entry_planted(&path, Some(&planted_sentinel))]);
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
                |p| {
                    std::fs::create_dir(p)?;
                    Err::<Option<String>, _>(io::Error::other("injected population failure"))
                },
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
