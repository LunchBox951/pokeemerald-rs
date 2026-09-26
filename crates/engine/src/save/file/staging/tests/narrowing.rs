use std::path::Path;

use super::super::{create_new_exclusive, MAX_COMPONENT_LEN, WIDEST_HEX_DIGITS};
use super::shared::area;
use crate::save::file::tests::{saved_store, TempDir};
use crate::save::file::SaveFile;
use crate::save::store::FLASH_IMAGE_LEN;

/// Every save path whose own name is valid gets a valid staging sibling:
/// the unique suffix is fixed-width, and a basename at the component limit
/// is cut to make room for it rather than pushed over.
#[test]
fn a_basename_at_the_component_limit_still_gets_a_valid_staging_sibling() {
    let file = SaveFile::at(Path::new(&"s".repeat(MAX_COMPONENT_LEN)));

    let sibling = area(&file).first_name();
    let component = sibling.file_name().unwrap().to_str().unwrap();
    assert_eq!(component.len(), MAX_COMPONENT_LEN, "{component}");
    let suffix_at = component.len() - ".tmp.".len() - WIDEST_HEX_DIGITS;
    assert!(component[..suffix_at].bytes().all(|byte| byte == b's'));
    assert!(component[suffix_at..].starts_with(".tmp."));
    assert!(component[suffix_at + 5..]
        .bytes()
        .all(|byte| byte.is_ascii_hexdigit()));

    assert_ne!(
        area(&file).first_name(),
        area(&file).first_name(),
        "two stagings in the same process must not share a name"
    );
}

/// A staging candidate is always a sibling of the save path, never the save
/// path itself. A save whose own basename already has the `.tmp.<hex>`
/// shape the generator renders can otherwise be drawn exactly -- a save
/// named `.tmp.a`, once the shrink chain has reached an empty stem and a
/// single hex digit, is one of only sixteen names the generator can produce
/// there -- and `create_new` succeeds on a destination no save occupies yet,
/// so the image would be written in place instead of staged: visible while
/// half-written, and a partial file left behind by a crash where the rename
/// is supposed to publish a whole one.
#[test]
fn a_staging_candidate_is_never_the_save_path_itself() {
    let path = Path::new("/saves/.tmp.a");
    let file = SaveFile::at(path);

    let mut drawn = std::collections::BTreeSet::new();
    for _ in 0..4_096 {
        let candidate = area(&file).first_name_under(0, 1);
        assert_ne!(
            candidate, *path,
            "a staging candidate must never be the save path itself"
        );
        drawn.insert(candidate);
    }

    assert_eq!(
        drawn.len(),
        15,
        "escaping the save path must cost only that one name, not narrow the \
         floor further: {drawn:?}"
    );
}

/// A case-insensitive volume treats `.tmp.a` and `.TMP.A` as one entry, so
/// the walk must also skip a candidate that differs from the save path only
/// by ASCII case; otherwise `create_new` opens the destination itself there.
#[test]
fn a_staging_candidate_never_aliases_the_save_path_by_ascii_case() {
    let path = Path::new("/saves/.TMP.A");
    let file = SaveFile::at(path);

    let mut drawn = std::collections::BTreeSet::new();
    for _ in 0..4_096 {
        let candidate = area(&file).first_name_under(0, 1);
        let name = candidate.file_name().expect("candidate has a file name");
        assert!(
            !name.as_encoded_bytes().eq_ignore_ascii_case(b".TMP.A"),
            "a staging candidate must not alias the save path by case: {candidate:?}"
        );
        drawn.insert(candidate);
    }

    assert_eq!(
        drawn.len(),
        15,
        "escaping the case alias must cost only that one name: {drawn:?}"
    );
}

/// The shortest save basename whose staging sibling would exceed the
/// component limit must still be writable: the stem is cut to make room for
/// the fixed-width suffix rather than the sibling being refused.
#[test]
fn a_basename_the_fixed_width_suffix_pushes_over_the_limit_still_writes() {
    let dir = TempDir::new("longname");
    let basename_len = MAX_COMPONENT_LEN - ".tmp.".len() - WIDEST_HEX_DIGITS + 1;
    assert!(
        basename_len + ".tmp.".len() + WIDEST_HEX_DIGITS > MAX_COMPONENT_LEN,
        "the suffix must actually carry this basename over the limit, or nothing is cut"
    );
    let file = SaveFile::at(dir.join(&"s".repeat(basename_len)));
    let (store, _, _) = saved_store();

    file.write(&store).unwrap();
    assert!(file.exists());
}

/// The longest basename `parent` accepts as a save file, found by growing
/// one byte at a time until the host refuses it -- pinpointing the exact
/// boundary rather than assuming a constant for it.
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

/// A save path at the host's real, unpredictable whole-path ceiling --
/// discovered by probing rather than assumed from a constant, since a
/// symlinked temp root (macOS's `/var/folders` -> `/private/var`) can make
/// the kernel's resolved length longer than the one this process measures --
/// must still be writable, even when the directory leaves less than the
/// widest suffix's own 15 bytes of room. Neither the first-guess candidate
/// nor an empty stem carrying that widest suffix fits there -- asserted
/// below, so the scenario is reached rather than merely approached -- so the
/// write can only succeed by narrowing the suffix too.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn a_save_path_near_the_hosts_real_path_limit_still_writes() {
    let dir = TempDir::new("path-limit");

    // Coarse phase: nest maximally sized directories until the host refuses
    // one, to approach its real ceiling quickly without assuming a specific
    // number for it.
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

    // Fine phase: narrow further, one directory at a time, until fewer than
    // 15 bytes of headroom remain -- less than even the fixed-width
    // `.tmp.<hex>` suffix needs on its own, so an empty stem is not enough
    // and the hex component must narrow too -- but still at least 10, so a
    // one-digit `.tmp.<hex>` suffix (6 bytes) has room to land in. Nesting a
    // directory `headroom - 10` bytes long leaves exactly 9 bytes of
    // headroom behind it (one byte of every step goes to the new
    // separator), so this converges in a single pass.
    let mut headroom = longest_valid_basename(&parent);
    while headroom >= 15 {
        let nested_len = (headroom - 10).min(MAX_COMPONENT_LEN);
        let nested = parent.join("d".repeat(nested_len));
        std::fs::create_dir(&nested)
            .expect("nesting further to tighten the remaining headroom must succeed");
        parent = nested;
        headroom = longest_valid_basename(&parent);
    }
    assert!(
        (6..15).contains(&headroom),
        "test setup must leave room for at least the one-digit `.tmp.<hex>` suffix (6 \
         bytes) but less than the full-width one (15 bytes): {headroom} bytes"
    );

    let save_path = parent.join("s");
    let file = SaveFile::at(&save_path);
    std::fs::File::create(&save_path)
        .expect("a save path this close to the host's real limit must itself be a valid path");
    std::fs::remove_file(&save_path).unwrap();

    // Neither the first-guess candidate nor an empty stem carrying the
    // widest suffix fits here; this is exactly the scenario under test.
    let naive_candidate = area(&file).first_name();
    assert!(
        std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&naive_candidate)
            .is_err_and(|err| err.kind() == std::io::ErrorKind::InvalidFilename),
        "test setup must actually exceed the host's real limit, not merely approach it"
    );
    let empty_stem_candidate = area(&file).first_name_under(0, WIDEST_HEX_DIGITS);
    assert!(
        std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&empty_stem_candidate)
            .is_err_and(|err| err.kind() == std::io::ErrorKind::InvalidFilename),
        "test setup must exceed even the fixed-width suffix's own floor, not just the \
         first-guess candidate, or this would not exercise the hex-digit shrink"
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

/// A host whose real per-component limit sits well under
/// [`MAX_COMPONENT_LEN`]'s first guess -- eCryptfs caps a
/// component at 143 bytes, not 255 -- must still get a valid staging
/// sibling: the first-guess candidate is refused outright, and that must
/// shrink the stem and retry rather than propagate immediately. Pinned
/// through an injected `open` rather than a real eCryptfs mount, so this
/// holds on every platform this crate builds for, not just whichever one
/// happens to have such a filesystem mounted.
#[test]
fn a_host_component_limit_below_the_first_guess_still_gets_a_staging_sibling() {
    const HOST_NAME_MAX: usize = 143;
    // Long enough that the 15-byte suffix carries it past the host's real
    // limit, and short enough that `MAX_COMPONENT_LEN`'s 255-byte first
    // guess does not truncate it at all, so the very first candidate is
    // refused outright.
    const BASENAME_LEN: usize = 131;
    const { assert!(BASENAME_LEN + 15 > HOST_NAME_MAX) };
    const { assert!(BASENAME_LEN < MAX_COMPONENT_LEN - 15) };

    let dir = TempDir::new("host-component-limit");
    let path = dir.join(&"s".repeat(BASENAME_LEN));
    let file = SaveFile::at(&path);

    let refuse_long_components = |candidate: &Path| -> std::io::Result<std::fs::File> {
        let component_len = candidate
            .file_name()
            .map_or(0, |name| name.as_encoded_bytes().len());
        if component_len > HOST_NAME_MAX {
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
        final_component_len <= HOST_NAME_MAX,
        "the staged sibling must respect the host's real limit: {final_component_len} bytes"
    );
    assert!(
        final_component_len < BASENAME_LEN + 15,
        "the stem must actually have been shortened from the first-guess candidate, not \
         merely have succeeded by chance: {final_component_len} bytes"
    );

    let staged_path = staged.path.clone();
    drop(staged);
    std::fs::remove_file(staged_path).unwrap();
}

/// A directory with less room than the widest `.tmp.<hex>` suffix needs must
/// still get a staging sibling: an empty stem alone is not enough once the
/// suffix itself no longer fits, so the retries must narrow
/// [`WIDEST_HEX_DIGITS`] too. Pinned through an injected
/// `open` that refuses anything longer than the save path plus 6 bytes --
/// tighter than even an empty stem's full-width suffix allows -- so only
/// narrowing the hex component can satisfy it.
#[test]
fn a_directory_too_tight_for_the_fixed_width_suffix_still_gets_a_staging_sibling() {
    let dir = TempDir::new("hex-digit-shrink");
    let path = dir.join("s");
    let file = SaveFile::at(&path);

    let save_path_len = path.as_os_str().as_encoded_bytes().len();
    let injected_limit = save_path_len + 6;

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
        final_component_len <= original_basename_len + 6,
        "the staged sibling's component must be at most 6 bytes over the save name: \
         {final_component_len} bytes vs a {original_basename_len}-byte save name"
    );

    let staged_path = staged.path.clone();
    drop(staged);
    std::fs::remove_file(staged_path).unwrap();
}
