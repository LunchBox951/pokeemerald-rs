use std::path::Path;

use super::super::{create_new_exclusive, MAX_COMPONENT_LEN, WIDEST_HEX_DIGITS};
use super::shared::area;
use crate::save::file::tests::{saved_store, TempDir};
use crate::save::file::SaveFile;
use crate::save::store::FLASH_IMAGE_LEN;

const STAGING_MARKER: &str = ".tmp.";
const FULL_SUFFIX_LEN: usize = STAGING_MARKER.len() + WIDEST_HEX_DIGITS;
const ONE_HEX_DIGIT: usize = 1;
const ONE_DIGIT_SUFFIX_LEN: usize = STAGING_MARKER.len() + ONE_HEX_DIGIT;
#[cfg(any(target_os = "linux", target_os = "macos"))]
const TARGET_HEADROOM: usize = 9;
const ONE_DIGIT_NAMES: usize = 16;
const DRAWS_EXHAUSTING_ONE_DIGIT_NAMES: usize = 4_096;

#[test]
fn a_basename_at_the_component_limit_still_gets_a_valid_staging_sibling() {
    let file = SaveFile::at(Path::new(&"s".repeat(MAX_COMPONENT_LEN)));

    let sibling = area(&file).first_name();
    let component = sibling.file_name().unwrap().to_str().unwrap();
    assert_eq!(component.len(), MAX_COMPONENT_LEN, "{component}");
    let suffix_at = component.len() - FULL_SUFFIX_LEN;
    assert!(component[..suffix_at].bytes().all(|byte| byte == b's'));
    assert!(component[suffix_at..].starts_with(STAGING_MARKER));
    assert!(component[suffix_at + STAGING_MARKER.len()..]
        .bytes()
        .all(|byte| byte.is_ascii_hexdigit()));

    assert_ne!(
        area(&file).first_name(),
        area(&file).first_name(),
        "two stagings in the same process must not share a name"
    );
}

/// A save named like a generated `.tmp.<hex>` name must not be drawn as its own
/// staging candidate: `create_new` would succeed and write the image in place.
#[test]
fn a_staging_candidate_is_never_the_save_path_itself() {
    let path = Path::new("/saves/.tmp.a");
    let file = SaveFile::at(path);

    let mut drawn = std::collections::BTreeSet::new();
    for _ in 0..DRAWS_EXHAUSTING_ONE_DIGIT_NAMES {
        let candidate = area(&file).first_name_under(0, ONE_HEX_DIGIT);
        assert_ne!(
            candidate, *path,
            "a staging candidate must never be the save path itself"
        );
        drawn.insert(candidate);
    }

    assert_eq!(
        drawn.len(),
        ONE_DIGIT_NAMES - 1,
        "escaping the save path must cost only that one name, not narrow the \
         floor further: {drawn:?}"
    );
}

/// Case-insensitive volumes treat `.tmp.a` and `.TMP.A` as one entry.
#[test]
fn a_staging_candidate_never_aliases_the_save_path_by_ascii_case() {
    let path = Path::new("/saves/.TMP.A");
    let file = SaveFile::at(path);

    let mut drawn = std::collections::BTreeSet::new();
    for _ in 0..DRAWS_EXHAUSTING_ONE_DIGIT_NAMES {
        let candidate = area(&file).first_name_under(0, ONE_HEX_DIGIT);
        let name = candidate.file_name().expect("candidate has a file name");
        assert!(
            !name.as_encoded_bytes().eq_ignore_ascii_case(b".TMP.A"),
            "a staging candidate must not alias the save path by case: {candidate:?}"
        );
        drawn.insert(candidate);
    }

    assert_eq!(
        drawn.len(),
        ONE_DIGIT_NAMES - 1,
        "escaping the case alias must cost only that one name: {drawn:?}"
    );
}

#[test]
fn a_basename_the_fixed_width_suffix_pushes_over_the_limit_still_writes() {
    let dir = TempDir::new("longname");
    let basename_len = MAX_COMPONENT_LEN - FULL_SUFFIX_LEN + 1;
    assert!(
        basename_len + FULL_SUFFIX_LEN > MAX_COMPONENT_LEN,
        "the suffix must carry this basename over the limit"
    );
    let file = SaveFile::at(dir.join(&"s".repeat(basename_len)));
    let (store, _, _) = saved_store();

    file.write(&store).unwrap();
    assert!(file.exists());
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn longest_valid_basename(parent: &Path) -> usize {
    (1..=MAX_COMPONENT_LEN)
        .take_while(|&len| {
            let candidate = parent.join("s".repeat(len));
            match std::fs::File::create(&candidate) {
                Ok(_) => {
                    std::fs::remove_file(&candidate).unwrap();
                    true
                }
                Err(err) if err.kind() == std::io::ErrorKind::InvalidFilename => false,
                Err(err) => {
                    panic!("unexpected error probing the host's real path limit: {err:?}")
                }
            }
        })
        .last()
        .unwrap_or(0)
}

/// Probes the host limit: a symlinked temp root (macOS `/var/folders`) resolves
/// longer than the path measured here.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn a_save_path_near_the_hosts_real_path_limit_still_writes() {
    let dir = TempDir::new("path-limit");

    let mut parent = dir.path.clone();
    loop {
        let candidate = parent.join("d".repeat(MAX_COMPONENT_LEN));
        match std::fs::create_dir(&candidate) {
            Ok(()) => parent = candidate,
            Err(err) if err.kind() == std::io::ErrorKind::InvalidFilename => break,
            Err(err) => {
                panic!("unexpected error nesting toward the host's real path limit: {err:?}")
            }
        }
    }

    // The `- 1` is the new directory's separator byte.
    let mut headroom = longest_valid_basename(&parent);
    while headroom >= FULL_SUFFIX_LEN {
        let nested_len = (headroom - TARGET_HEADROOM - 1).min(MAX_COMPONENT_LEN);
        let nested = parent.join("d".repeat(nested_len));
        std::fs::create_dir(&nested)
            .expect("nesting further to tighten the remaining headroom must succeed");
        parent = nested;
        headroom = longest_valid_basename(&parent);
    }
    assert!(
        (ONE_DIGIT_SUFFIX_LEN..FULL_SUFFIX_LEN).contains(&headroom),
        "test setup must leave room for the one-digit suffix but not the full one: {headroom} bytes"
    );

    let save_path = parent.join("s");
    let file = SaveFile::at(&save_path);
    std::fs::File::create(&save_path)
        .expect("a save path this close to the host's real limit must itself be a valid path");
    std::fs::remove_file(&save_path).unwrap();

    let naive_candidate = area(&file).first_name();
    assert!(
        std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&naive_candidate)
            .is_err_and(|err| err.kind() == std::io::ErrorKind::InvalidFilename),
        "the first-guess candidate must exceed the host limit"
    );
    let empty_stem_candidate = area(&file).first_name_under(0, WIDEST_HEX_DIGITS);
    assert!(
        std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&empty_stem_candidate)
            .is_err_and(|err| err.kind() == std::io::ErrorKind::InvalidFilename),
        "an empty stem with the full suffix must exceed the host limit"
    );

    let (store, _, _) = saved_store();
    file.write(&store).expect(
        "a save path at the host's real path limit must still be writable, by narrowing \
         the hex component once the stem cannot shrink any further",
    );
    assert!(file.exists());

    let reloaded = file.read().unwrap().expect("the save must be readable");
    assert_eq!(reloaded.flash_image(), store.flash_image());
}

/// Injected `open`, so the host limit holds on every platform.
#[test]
fn a_host_component_limit_below_the_first_guess_still_gets_a_staging_sibling() {
    const ECRYPTFS_NAME_MAX: usize = 143;
    const BASENAME_LEN: usize = 131;
    const { assert!(BASENAME_LEN + FULL_SUFFIX_LEN > ECRYPTFS_NAME_MAX) };
    const { assert!(BASENAME_LEN < MAX_COMPONENT_LEN - FULL_SUFFIX_LEN) };

    let dir = TempDir::new("host-component-limit");
    let path = dir.join(&"s".repeat(BASENAME_LEN));
    let file = SaveFile::at(&path);

    let refuse_long_components = |candidate: &Path| -> std::io::Result<std::fs::File> {
        let component_len = candidate
            .file_name()
            .map_or(0, |name| name.as_encoded_bytes().len());
        if component_len > ECRYPTFS_NAME_MAX {
            return Err(std::io::Error::from(std::io::ErrorKind::InvalidFilename));
        }
        create_new_exclusive(candidate)
    };

    let staged = area(&file)
        .stage_narrowing_until_accepted(refuse_long_components, &vec![0u8; FLASH_IMAGE_LEN])
        .expect(
            "a host component limit under the first guess must still be satisfiable by \
             shrinking the stem and retrying",
        );

    let final_component_len = staged
        .path
        .file_name()
        .map_or(0, |name| name.as_encoded_bytes().len());
    assert!(
        final_component_len <= ECRYPTFS_NAME_MAX,
        "the staged sibling must respect the host's real limit: {final_component_len} bytes"
    );
    assert!(
        final_component_len < BASENAME_LEN + FULL_SUFFIX_LEN,
        "the stem must have been shortened below the first-guess length: {final_component_len} bytes"
    );

    let staged_path = staged.path.clone();
    drop(staged);
    std::fs::remove_file(staged_path).unwrap();
}

/// Injected `open` refuses anything longer than the save path plus the
/// one-digit suffix, which only a narrowed hex component can satisfy.
#[test]
fn a_directory_too_tight_for_the_fixed_width_suffix_still_gets_a_staging_sibling() {
    let dir = TempDir::new("hex-digit-shrink");
    let path = dir.join("s");
    let file = SaveFile::at(&path);

    let save_path_len = path.as_os_str().as_encoded_bytes().len();
    let injected_limit = save_path_len + ONE_DIGIT_SUFFIX_LEN;

    let refuse_long_paths = |candidate: &Path| -> std::io::Result<std::fs::File> {
        if candidate.as_os_str().as_encoded_bytes().len() > injected_limit {
            return Err(std::io::Error::from(std::io::ErrorKind::InvalidFilename));
        }
        create_new_exclusive(candidate)
    };

    let staged = area(&file)
        .stage_narrowing_until_accepted(refuse_long_paths, &vec![0u8; FLASH_IMAGE_LEN])
        .expect(
            "a directory too tight for even the fixed-width suffix must still be \
             satisfiable by narrowing the hex component too",
        );

    let original_basename_len = path
        .file_name()
        .map_or(0, |name| name.as_encoded_bytes().len());
    let final_component_len = staged
        .path
        .file_name()
        .map_or(0, |name| name.as_encoded_bytes().len());
    assert!(
        final_component_len <= original_basename_len + ONE_DIGIT_SUFFIX_LEN,
        "the staged sibling's component must be at most the one-digit suffix over the save name: \
         {final_component_len} bytes vs a {original_basename_len}-byte save name"
    );

    let staged_path = staged.path.clone();
    drop(staged);
    std::fs::remove_file(staged_path).unwrap();
}
