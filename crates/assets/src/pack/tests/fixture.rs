#[cfg(unix)]
use super::shared::pack_fixture_dir;
use super::shared::write_pack;

const PRECIOUS: &[u8] = b"precious";

#[test]
fn a_fixture_removes_its_file_and_its_directory() {
    let fixture = write_pack("fixture-cleanup", &[1, 2, 3]);
    let path = fixture.to_path_buf();
    let dir = path.parent().unwrap().to_path_buf();
    let written = std::fs::read(&path).unwrap();
    drop(fixture);
    assert_eq!(written, [1, 2, 3]);
    assert!(!path.exists(), "the fixture left its file behind");
    assert!(!dir.exists(), "the fixture left its directory behind");
}

#[cfg(unix)]
#[test]
fn a_fixture_directory_is_private_to_its_owner() {
    use std::os::unix::fs::PermissionsExt as _;
    let fixture = write_pack("fixture-private", &[1]);
    let mode = std::fs::metadata(fixture.parent().unwrap())
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o077, 0, "the fixture directory is open to others");
}

#[test]
fn cleanup_keeps_a_stranger_in_the_fixture_directory() {
    let fixture = write_pack("fixture-stranger", &[1]);
    let dir = fixture.parent().unwrap().to_path_buf();
    let stranger = dir.join("stranger.bin");
    std::fs::write(&stranger, PRECIOUS).unwrap();
    drop(fixture);
    let kept = std::fs::read(&stranger).ok();
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(
        kept.as_deref(),
        Some(PRECIOUS),
        "cleanup removed a stranger"
    );
}

// Unix only: Windows refuses a new file at the name of an open, deleted one.
#[cfg(unix)]
#[test]
fn cleanup_spares_a_replacement_regular_file() {
    let fixture = write_pack("fixture-replaced", &[1, 2, 3]);
    let path = fixture.to_path_buf();
    let dir = path.parent().unwrap().to_path_buf();
    // The fixture's handle stays open, so the freed name cannot hand the
    // replacement the original inode number.
    std::fs::remove_file(&path).unwrap();
    std::fs::write(&path, PRECIOUS).unwrap();
    drop(fixture);
    let kept = std::fs::read(&path).ok();
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(
        kept.as_deref(),
        Some(PRECIOUS),
        "cleanup removed a replacement"
    );
}

#[cfg(unix)]
#[test]
fn a_replacement_symlink_is_neither_written_through_nor_unlinked() {
    let target = write_pack("fixture-replaced-link-target", PRECIOUS);
    let fixture = write_pack("fixture-replaced-link", &[1, 2, 3]);
    let path = fixture.to_path_buf();
    let dir = path.parent().unwrap().to_path_buf();
    std::fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink(&*target, &path).unwrap();
    drop(fixture);
    let link_survives = std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink());
    let target_after = std::fs::read(&*target).ok();
    std::fs::remove_dir_all(&dir).unwrap();
    assert!(link_survives, "cleanup unlinked a replacement symlink");
    assert_eq!(
        target_after.as_deref(),
        Some(PRECIOUS),
        "the replacement symlink's target was changed"
    );
}

#[cfg(unix)]
#[test]
fn write_pack_does_not_follow_a_preplanted_symlink() {
    let victim = write_pack("fixture-preplanted-victim", PRECIOUS);
    let victim_dir = victim.parent().unwrap().to_path_buf();
    let link = pack_fixture_dir("fixture-preplanted", 0);
    std::os::unix::fs::symlink(&victim_dir, &link).unwrap();

    let fixture = write_pack("fixture-preplanted", &[1, 2, 3]);

    let victim_after = std::fs::read(&*victim).unwrap();
    let victim_dir_entries = std::fs::read_dir(&victim_dir).unwrap().count();
    let link_survives = std::fs::symlink_metadata(&link).is_ok_and(|m| m.file_type().is_symlink());
    let distinct = fixture.parent() != Some(link.as_path());
    let written = std::fs::read(&*fixture).unwrap();
    drop(fixture);
    std::fs::remove_file(&link).unwrap();

    assert_eq!(victim_after, PRECIOUS, "the symlink target was modified");
    assert_eq!(victim_dir_entries, 1, "a file was written through the link");
    assert!(
        link_survives && distinct,
        "the fixture reused the link's name"
    );
    assert_eq!(written, [1, 2, 3], "fixture bytes differ");
}
