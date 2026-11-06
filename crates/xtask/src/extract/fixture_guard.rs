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

/// Entries the guard created, removed on drop whatever the fixture did.
struct Planted(Vec<PathBuf>);

impl Drop for Planted {
    fn drop(&mut self) {
        for entry in &self.0 {
            if entry.is_symlink() {
                let _ = std::fs::remove_file(entry);
            } else {
                let _ = std::fs::remove_dir_all(entry);
            }
        }
    }
}

/// Every name planted for one fixture: the predictable `{prefix}-{pid}`,
/// `{prefix}-{pid}-{line}` for a fixture that appended `line!()` (every line
/// up to `line_limit` outside the `OwnedDir` range), and the first
/// `OwnedDir` candidates `{prefix}-{pid}-{n}`.
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

/// Plants an entry with `plant` at every name. A name held by a concurrent
/// run of the same fixture is waited for, so the guard owns every name
/// before the fixture runs; one still held after `PLANT_WAIT`, or any other
/// failure, panics.
fn plant_all(prefix: &str, names: &[PathBuf], plant: impl Fn(&Path) -> io::Result<()>) -> Planted {
    let mut planted = Planted(Vec::new());
    let deadline = Instant::now() + PLANT_WAIT;
    for path in names {
        loop {
            match plant(path) {
                Ok(()) => break,
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists && Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(2));
                }
                Err(e) => panic!("{prefix}: cannot plant {}: {e}", path.display()),
            }
        }
        planted.0.push(path.clone());
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

fn populate(dir: &Path) -> io::Result<()> {
    std::fs::write(dir.join(SENTINEL), PRECIOUS)
}

fn untouched(dir: &Path) -> bool {
    listing(dir).as_deref() == Some(&[SENTINEL.to_owned()][..])
        && std::fs::read_to_string(dir.join(SENTINEL)).ok().as_deref() == Some(PRECIOUS)
}

/// Runs `fixture`, the real test function, with something planted at each
/// former and candidate name for `prefix`, and fails if it touched them.
///
/// Pass one plants populated directories: deleting by path, or writing into
/// what is already at the name, disturbs them. Pass two (Unix) plants
/// symlinks to a victim directory: writing through one adds files to the
/// victim, and unlinking one makes the link vanish.
///
/// `line_limit` is the highest line a fixture that appended `line!()` to its
/// former name could have been on; pass the line count of its file.
pub(crate) fn assert_fixture_ignores_planted_names(
    prefix: &str,
    line_limit: Option<usize>,
    fixture: fn(),
) {
    let names = names(prefix, line_limit);

    let dirs = plant_all(prefix, &names, |path| {
        std::fs::create_dir(path)?;
        populate(path)
    });
    fixture();
    let disturbed: Vec<_> = dirs.0.iter().filter(|p| !untouched(p)).cloned().collect();
    drop(dirs);
    assert!(
        disturbed.is_empty(),
        "{prefix}: fixture disturbed populated directories it never created: {disturbed:?}"
    );

    #[cfg(unix)]
    {
        let victim = OwnedDir::create("pokeemerald-rs-fixture-guard-victim").unwrap();
        populate(victim.path()).unwrap();
        let links = plant_all(prefix, &names, |path| {
            std::os::unix::fs::symlink(victim.path(), path)
        });
        fixture();
        let removed: Vec<_> = links
            .0
            .iter()
            .filter(|p| std::fs::read_link(p).ok().as_deref() != Some(victim.path()))
            .cloned()
            .collect();
        let victim_untouched = untouched(victim.path());
        drop(links);
        assert!(
            removed.is_empty(),
            "{prefix}: fixture removed or replaced symlinks it never created: {removed:?}"
        );
        assert!(
            victim_untouched,
            "{prefix}: fixture wrote through a planted symlink"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::owned_fixture::OwnedDir;

    const PREFIX: &str = "pokeemerald-rs-fixture-guard-sibling";

    /// Joins the thread on drop, so a panic unwinding past it still waits.
    struct Join(Option<std::thread::JoinHandle<()>>);
    impl Drop for Join {
        fn drop(&mut self) {
            self.0.take().unwrap().join().unwrap();
        }
    }

    /// Stands in for a fixture reverted to a predictable path: it reuses
    /// the first `OwnedDir` candidate name, deleting and rewriting it.
    fn fixture_reusing_candidate_zero() {
        let dir = std::env::temp_dir().join(format!("{PREFIX}-{}-0", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join("scratch"), "x").unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A concurrent run of the same fixture holds candidate 0 while the guard
    /// plants. The guard must wait for it and still plant candidate 0, so the
    /// reverted fixture's reuse of that name is caught.
    #[test]
    #[should_panic(expected = "disturbed populated directories")]
    fn guard_catches_a_fixture_that_reuses_candidate_zero_while_a_sibling_held_it_at_planting() {
        let (claimed, claimed_rx) = std::sync::mpsc::channel();
        let sibling = std::thread::spawn(move || {
            let held = OwnedDir::create(PREFIX).unwrap();
            claimed.send(()).unwrap();
            std::thread::sleep(Duration::from_millis(150));
            drop(held);
        });
        claimed_rx.recv().unwrap();
        let _join = Join(Some(sibling));
        assert_fixture_ignores_planted_names(PREFIX, None, fixture_reusing_candidate_zero);
    }
}
