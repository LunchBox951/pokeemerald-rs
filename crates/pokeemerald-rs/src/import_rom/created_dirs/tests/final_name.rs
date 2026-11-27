//! A level made at its final name, where no no-replace rename can claim it:
//! on Unix targets without one, and through the `EINVAL` seam in `acquire`
//! where the filesystem refuses it. Nothing is claimed, but the entry is
//! synced and the owner's access the umask took is restored.

#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
use super::acquisition::private_entries;
use super::acquisition::{create_as, walk_through_a_read_only_parent};
use super::*;

/// Makes the walk on this thread take the final-name path: through the
/// `EINVAL` seam where the platform has a no-replace rename, and as it does
/// anyway everywhere else.
#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
#[must_use]
fn at_final_name() -> super::acquire::tests_seam::Refusing {
    super::acquire::tests_seam::refuse_noreplace(rustix::io::Errno::INVAL)
}

/// What [`at_final_name`] holds where there is no seam to install.
#[cfg(not(any(target_os = "linux", target_os = "android", target_vendor = "apple")))]
struct AlreadyAtFinalName;

/// [`at_final_name`] where every level is made at its final name already.
#[cfg(not(any(target_os = "linux", target_os = "android", target_vendor = "apple")))]
#[must_use]
fn at_final_name() -> AlreadyAtFinalName {
    AlreadyAtFinalName
}

// Only where every level is made at its final name: where the platform has a
// no-replace rename, the private claim refuses such a level before the rename
// the seam refuses is reached (`acquisition::a_level_it_cannot_vouch_for_*`).
#[cfg(not(any(target_os = "linux", target_os = "android", target_vendor = "apple")))]
#[test]
fn a_level_of_another_user_made_at_its_final_name_is_walked_through() {
    // Nothing made at a final name is vouched for, so a level the walk is
    // told belongs to another user is neither refused nor recorded: both
    // levels are made and walked through, and nothing is claimed.
    let dir = TempDir::new("final-name-other-owner");
    let target = dir.join("a").join("b");
    let owner = rustix::process::geteuid().as_raw();
    let mut after_mkdir = 0;

    let creation = create_as(&target, owner.wrapping_add(1), &mut || after_mkdir += 1)
        .map_err(|(_, source)| source)
        .expect("both levels are made and walked through");

    assert!(target.is_dir());
    assert!(creation.created.is_empty(), "a level was recorded");
    assert!(after_mkdir >= 2, "the walk stopped before the second level");
}

#[test]
fn an_existing_level_under_a_read_only_parent_is_walked_through_at_final_name() {
    let _fallback = at_final_name();
    walk_through_a_read_only_parent("final-name-existing-under-read-only", &[]);
}

#[cfg(not(any(target_os = "linux", target_os = "android", target_vendor = "apple")))]
#[test]
fn a_level_is_made_at_its_final_name_where_no_rename_can_refuse_to_replace() {
    // The fallback contract: the directory is made at the name itself, so
    // the seam sees it there and nothing is made privately. Nothing can tell
    // it from a same-user replacement put there in the gap, so neither level
    // is recorded as this run's.
    let dir = TempDir::new("final-name");
    let first = dir.join("a");
    let target = first.join("b");
    let owner = rustix::process::geteuid().as_raw();
    let mut seen = Vec::new();

    let creation = create_as(&target, owner, &mut || {
        seen.push(entry_names(&dir.path));
    })
    .expect("both levels are created");

    assert!(target.is_dir());
    assert!(
        creation.created.is_empty(),
        "a level made at its final name was claimed"
    );
    assert_eq!(seen[0], ["a"], "the level is already at its final name");
}

#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
#[test]
fn a_missing_level_is_created_where_noreplace_rename_is_unsupported() {
    // On a filesystem that cannot refuse to replace (Linux NFS answers
    // `EINVAL`), the private-name claim cannot finish. Creating the level
    // must still work, at its final name, and leave no private directory;
    // nothing there is claimed, since a same-user replacement in the gap
    // could not be told apart.
    // Every no-replace rename on this thread answers `EINVAL`, through the
    // seam in `acquire`, so this runs wherever the platform renames at all.
    let dir = TempDir::new("noreplace-einval");
    let _refusing = super::acquire::tests_seam::refuse_noreplace(rustix::io::Errno::INVAL);
    let target = dir.join("a").join("b");
    let owner = rustix::process::geteuid().as_raw();
    let result = create_as(&target, owner, &mut || {});

    let creation = result
        .map_err(|(_, source)| source)
        .expect("creating a missing level failed");
    assert!(
        creation.created.is_empty(),
        "a level made at its final name was claimed"
    );
    assert!(target.is_dir());
    assert_eq!(private_entries(&dir.path), Vec::<String>::new());
    assert_eq!(private_entries(&dir.join("a")), Vec::<String>::new());
    // One refused claim per level, and the cleanup after each: a pass
    // without them would not have exercised the fallback.
    assert!(
        super::acquire::tests_seam::refusals() >= 2,
        "the fallback ran for {} renames, expected at least one per level",
        super::acquire::tests_seam::refusals()
    );
}

#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
#[test]
fn a_replacement_put_at_a_levels_name_after_the_fallback_mkdir_is_not_claimed() {
    // Where the no-replace rename is refused (`EINVAL`, as on NFS), the level
    // is made at its final name. A same-user writer moves it away right after
    // `mkdirat` and puts its own directory there: that replacement must not
    // be recorded as this run's, so rollback leaves it standing.
    let dir = TempDir::new("fallback-swap");
    let _refusing = super::acquire::tests_seam::refuse_noreplace(rustix::io::Errno::INVAL);
    let first = dir.join("a");
    let target = first.join("b");
    let moved = dir.join("a-original");
    let owner = rustix::process::geteuid().as_raw();
    let mut swapped = false;
    let result = create_as(&target, owner, &mut || {
        if !swapped && first.exists() {
            swapped = true;
            fs::rename(&first, &moved).unwrap();
            fs::create_dir(&first).unwrap();
        }
    });
    assert!(
        swapped,
        "the fallback never made the level at its final name"
    );
    let replacement_ino = rustix::fs::stat(&first).unwrap().st_ino;
    let created = match result {
        Ok(creation) => creation.created,
        Err((created, _)) => created,
    };
    let adopted = created
        .iter()
        .any(|level| level.identity.st_ino == replacement_ino);
    super::undo_created_directories(&created);
    assert!(
        !adopted,
        "the replacement (ino {replacement_ino}) was recorded as a created level; after rollback it exists: {}",
        first.exists()
    );
    assert!(first.is_dir(), "rollback removed the replacement");
}

#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
#[test]
fn a_level_made_at_its_final_name_has_its_entry_synced() {
    // A level made at its final name is not recorded, so no later pass syncs
    // its entry: its parent must be synced when it is made, or a crash after
    // a successful import can lose the new directory chain.
    let dir = TempDir::new("fallback-sync");
    let _refusing = super::acquire::tests_seam::refuse_noreplace(rustix::io::Errno::INVAL);
    let first = dir.join("a");
    let target = first.join("b");
    let owner = rustix::process::geteuid().as_raw();
    let mut synced = Vec::new();
    let creation = super::create_directories_pinned_after_mkdir(
        &target,
        super::MAX_PINNED_LEVELS,
        &mut || {},
        &mut || None,
        &mut || None,
        &mut |parent| {
            let real = dest::reopen_for_sync(parent)?;
            synced.push(rustix::fs::fstat(&real)?.st_ino);
            Ok(real)
        },
        owner,
        &mut || {},
    )
    .map_err(|(_, source)| source)
    .expect("both levels are created");
    assert!(creation.created.is_empty(), "nothing is claimed");
    assert!(target.is_dir());
    for parent in [&dir.path, &first] {
        let ino = rustix::fs::stat(parent).unwrap().st_ino;
        assert!(
            synced.contains(&ino),
            "{parent:?}, which a level was made in, was never synced"
        );
    }
}

/// Set in the child [`a_level_made_at_its_final_name_keeps_owner_access`]
/// re-runs itself as, so only that child changes its own umask.
#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
const FALLBACK_UMASK_CHILD: &str = "POKEEMERALD_IMPORT_FALLBACK_UMASK_CHILD";

#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
#[test]
fn a_level_made_at_its_final_name_keeps_owner_access() {
    // Under umask 0277 `mkdirat(0777)` leaves 0500, which the next level
    // cannot be made in. A level made at its final name is not claimed, but
    // it is still this run's `mkdirat`, so the owner's access is restored
    // before the walk descends, as for a claimed level. The umask is
    // process-wide, so the body runs alone in a child.
    use std::os::unix::fs::PermissionsExt as _;

    const TEST: &str = "import_rom::created_dirs::tests::final_name::a_level_made_at_its_final_name_keeps_owner_access";
    if std::env::var_os(FALLBACK_UMASK_CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([TEST, "--exact", "--nocapture"])
            .env(FALLBACK_UMASK_CHILD, "1")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("1 passed"),
            "child status {:?}\nstdout:\n{stdout}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let dir = TempDir::new("fallback-umask");
    let _refusing = super::acquire::tests_seam::refuse_noreplace(rustix::io::Errno::INVAL);
    let first = dir.join("a");
    let target = first.join("b");
    let owner = rustix::process::geteuid().as_raw();
    let previous = rustix::process::umask(rustix::fs::Mode::from_raw_mode(0o277));
    let result = create_as(&target, owner, &mut || {});
    rustix::process::umask(previous);
    let creation = result
        .map_err(|(_, source)| source)
        .expect("the nested levels are created under umask 0277");
    assert!(creation.created.is_empty(), "nothing is claimed");
    assert!(
        super::acquire::tests_seam::refusals() >= 2,
        "the fallback did not run"
    );
    for level in [&first, &target] {
        let mode = fs::metadata(level).unwrap().permissions().mode() & 0o7777;
        assert_eq!(mode, 0o700, "{level:?} lost its owner's access");
    }
}

#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
#[test]
fn a_symlink_put_at_a_level_made_at_its_final_name_is_not_followed() {
    // `mkdirat` cannot make a symlink, so one at the name of a level just
    // made there was swapped in since. Following it would create the next
    // level, and in the end the pack, inside a directory someone else chose.
    let dir = TempDir::new("fallback-symlink");
    let _refusing = super::acquire::tests_seam::refuse_noreplace(rustix::io::Errno::INVAL);
    let first = dir.join("a");
    let target = first.join("b");
    let moved = dir.join("a-original");
    let elsewhere = dir.join("elsewhere");
    fs::create_dir(&elsewhere).unwrap();
    let owner = rustix::process::geteuid().as_raw();
    let mut swapped = false;
    let result = create_as(&target, owner, &mut || {
        if !swapped && first.is_dir() && !first.is_symlink() {
            swapped = true;
            fs::rename(&first, &moved).unwrap();
            std::os::unix::fs::symlink(&elsewhere, &first).unwrap();
        }
    });
    assert!(
        swapped,
        "the fallback never made the level at its final name"
    );
    assert!(
        !elsewhere.join("b").exists(),
        "a level was made through the symlink"
    );
    assert!(result.is_err(), "a symlink at a level just made was walked");
}

/// The entries of this process's open descriptors whose target lies under
/// `dir` and names a private `.import-` directory.
#[cfg(target_os = "linux")]
fn private_descriptors_under(dir: &std::path::Path) -> Vec<String> {
    fs::read_dir("/proc/self/fd")
        .unwrap()
        .filter_map(|entry| fs::read_link(entry.ok()?.path()).ok())
        .map(|target| target.to_string_lossy().into_owned())
        .filter(|target| target.starts_with(&*dir.to_string_lossy()) && target.contains(".import-"))
        .collect()
}

#[cfg(target_os = "linux")]
#[test]
fn the_private_directory_is_released_before_a_level_is_made_at_its_final_name() {
    // Where the no-replace rename is refused, the private directory is
    // discarded and the level made at its final name. Its descriptor is of
    // no further use, and holding it costs the parent sync after the final
    // `mkdirat` a slot it may need when no pin is left to shed.
    let dir = TempDir::new("fallback-held-released");
    let _refusing = super::acquire::tests_seam::refuse_noreplace(rustix::io::Errno::INVAL);
    let target = dir.join("a");
    let owner = rustix::process::geteuid().as_raw();
    let mut held_at_sync = Vec::new();
    super::create_directories_pinned_after_mkdir(
        &target,
        super::MAX_PINNED_LEVELS,
        &mut || {},
        &mut || None,
        &mut || None,
        &mut |parent| {
            held_at_sync.extend(private_descriptors_under(&dir.path));
            dest::reopen_for_sync(parent)
        },
        owner,
        &mut || {},
    )
    .map_err(|(_, source)| source)
    .expect("the level is created");
    assert!(target.is_dir());
    assert_eq!(
        held_at_sync,
        Vec::<String>::new(),
        "the discarded private directory was still held at the parent sync"
    );
}
