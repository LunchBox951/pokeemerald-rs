//! Regression harness for fixtures that claim an [`OwnedDir`].

use std::path::{Path, PathBuf};

use super::owned_fixture::OwnedDir;

/// Removes a planted symlink, never what it points to.
#[cfg(unix)]
struct PlantedLink(PathBuf);

#[cfg(unix)]
impl Drop for PlantedLink {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Removes the populated directory planted at the fixture's former path.
struct LegacyDir(PathBuf);

impl Drop for LegacyDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn assert_only_sentinel(dir: &Path) {
    let names: Vec<_> = std::fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("{} was removed: {error}", dir.display()))
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names, ["sentinel"], "{} was modified", dir.display());
    assert_eq!(std::fs::read(dir.join("sentinel")).unwrap(), b"precious");
}

/// Runs `claim` while the first names under `prefix` are taken: a populated
/// directory and, on Unix, a symlink to another populated directory. `legacy`
/// is the fixed path the fixture used before it claimed a directory; it is
/// populated too. The claimed directory must be a different path, and writing
/// to it and dropping it must leave every occupant and the symlink target as
/// planted.
///
/// `prefix` must be used by no other test, so no sibling claims a name between
/// the planting and the claim.
pub(super) fn assert_claim_steps_around_occupied_names(
    prefix: &str,
    legacy: &Path,
    claim: impl FnOnce() -> OwnedDir,
) {
    std::fs::create_dir(legacy)
        .unwrap_or_else(|error| panic!("{} is already taken: {error}", legacy.display()));
    let legacy = LegacyDir(legacy.to_path_buf());
    std::fs::write(legacy.0.join("sentinel"), "precious").unwrap();
    let populated = OwnedDir::create(prefix).unwrap();
    std::fs::write(populated.path().join("sentinel"), "precious").unwrap();
    let victim = OwnedDir::create(&format!("{prefix}-victim")).unwrap();
    std::fs::write(victim.path().join("sentinel"), "precious").unwrap();
    #[cfg(unix)]
    let link = {
        let slot = OwnedDir::create(prefix).unwrap();
        let link = slot.path().to_path_buf();
        drop(slot);
        std::os::unix::fs::symlink(victim.path(), &link).unwrap();
        PlantedLink(link)
    };

    let claimed = claim();
    let claimed_path = claimed.path().to_path_buf();
    assert!(
        claimed_path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with(prefix)),
        "{} does not carry the fixture prefix {prefix}",
        claimed_path.display()
    );
    assert_ne!(claimed_path, populated.path());
    assert_ne!(claimed_path, legacy.0);
    #[cfg(unix)]
    assert_ne!(claimed_path, link.0);
    drop(claimed);

    assert!(
        !claimed_path.exists(),
        "the claimed tree is removed on drop"
    );
    assert_only_sentinel(populated.path());
    assert_only_sentinel(&legacy.0);
    assert_only_sentinel(victim.path());
    #[cfg(unix)]
    assert!(link.0.is_symlink(), "{} was replaced", link.0.display());
}
