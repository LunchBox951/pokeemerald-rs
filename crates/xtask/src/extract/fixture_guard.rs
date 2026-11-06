//! Regression guard for extractor fixtures that claim their directory with
//! [`OwnedDir`](crate::extract::owned_fixture::OwnedDir).
//!
//! It runs the real fixture function in-process with something planted at
//! every name a predictable-path fixture could use, and fails if the fixture
//! deleted, overwrote or wrote through anything it did not create.

use std::io;
use std::path::{Path, PathBuf};

use crate::extract::owned_fixture::OwnedDir;

const SENTINEL: &str = "sentinel";
const PRECIOUS: &str = "precious";

/// `OwnedDir` tries suffixes `0..ATTEMPTS`.
const ATTEMPTS: usize = 64;
/// The suffixes the guard occupies; the rest stay free so the fixture can
/// still claim one.
const OCCUPIED_CANDIDATES: usize = 2;

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

/// The names planted for one fixture.
struct Names {
    /// Names only a predictable-path fixture uses: `{prefix}-{pid}` and, for
    /// a fixture that appended `line!()`, `{prefix}-{pid}-{line}` for every
    /// line up to the limit outside the `OwnedDir` range. Nothing else
    /// creates these, so each must be planted.
    predictable: Vec<PathBuf>,
    /// The first names `OwnedDir` tries. A sibling run of the same fixture
    /// may hold one, so only some must be planted.
    candidates: Vec<PathBuf>,
}

fn names(prefix: &str, line_limit: Option<usize>) -> Names {
    let base = std::env::temp_dir();
    let pid = std::process::id();
    let mut predictable = vec![base.join(format!("{prefix}-{pid}"))];
    if let Some(limit) = line_limit {
        predictable
            .extend((ATTEMPTS..=limit).map(|line| base.join(format!("{prefix}-{pid}-{line}"))));
    }
    let candidates = (0..OCCUPIED_CANDIDATES)
        .map(|n| base.join(format!("{prefix}-{pid}-{n}")))
        .collect();
    Names {
        predictable,
        candidates,
    }
}

/// Plants an entry with `plant` at each name. Panics when a predictable name
/// cannot be planted, or when no candidate name can.
fn plant_all(prefix: &str, names: &Names, plant: impl Fn(&Path) -> io::Result<()>) -> Planted {
    let mut planted = Planted(Vec::new());
    for path in &names.predictable {
        plant(path).unwrap_or_else(|e| panic!("{prefix}: cannot plant {}: {e}", path.display()));
        planted.0.push(path.clone());
    }
    let before = planted.0.len();
    planted.0.extend(
        names
            .candidates
            .iter()
            .filter(|p| plant(p).is_ok())
            .cloned(),
    );
    assert!(
        planted.0.len() > before,
        "{prefix}: cannot plant any OwnedDir candidate name"
    );
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

fn populate(dir: &Path) {
    std::fs::write(dir.join(SENTINEL), PRECIOUS).unwrap();
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
        std::fs::write(path.join(SENTINEL), PRECIOUS)
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
        populate(victim.path());
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
