//! A level made at its final name, where no no-replace rename can claim it:
//! on Unix targets without one, and through the `EINVAL` seam in `acquire`
//! where the filesystem refuses it. Nothing is claimed, but the entry is
//! synced and the owner's access the umask took is restored.

use super::acquisition::create_as;
#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
use super::acquisition::private_entries;
use super::*;

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
