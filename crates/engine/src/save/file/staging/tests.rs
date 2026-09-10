//! Pins the staged sibling's name, its exclusive creation, the retries a
//! host's real limits force, and the cleanup that abandons it.

use std::path::Path;

use super::{
    create_new_exclusive, stage_at_first_free_name, StagingArea, MAX_COMPONENT_LEN,
    WIDEST_HEX_DIGITS,
};
use crate::save::block::SaveBlock1;
use crate::save::file::tests::{guessable_pid_staging_path, saved_store, sibling_path, TempDir};
use crate::save::file::{SaveFile, SAVE_FILE_NAME};
use crate::save::store::FLASH_IMAGE_LEN;

// Only the platform-gated tests below reach these, so their imports carry the
// same gate: an import a host compiles out every use of is a `dead_code`
// warning, and warnings are denied.
#[cfg(target_os = "linux")]
use super::fill_new_file;
#[cfg(unix)]
use crate::save::file::SaveFileError;

/// The staging area beside `file`'s save path.
fn area(file: &SaveFile) -> StagingArea<'_> {
    StagingArea::beside(file.path())
}

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

    std::fs::remove_file(&staged.path).unwrap();
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

    std::fs::remove_file(&staged.path).unwrap();
}

/// Whether `dir`'s filesystem will hold an entry whose name is not valid
/// UTF-8. Not every one will: APFS and HFS+ validate the bytes of every
/// name a syscall hands them and refuse `sa\xFFv` outright with `EILSEQ`
/// ("Illegal byte sequence"), so on macOS the save path the test below
/// needs cannot be brought into existence at all and there is nothing
/// there to assert about. Probed through the very operation that test
/// depends on -- the rename that publishes the staged image onto such a
/// name -- rather than assumed from `target_os`, so a host that does
/// accept one keeps the coverage.
#[cfg(unix)]
fn host_accepts_a_non_utf8_filename(dir: &Path) -> bool {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt as _;

    let valid = dir.join("utf8-name-probe");
    if std::fs::write(&valid, b"probe").is_err() {
        return false;
    }
    let raw = dir.join(OsString::from_vec(vec![b'p', 0xFF, b'e']));
    let accepted = std::fs::rename(&valid, &raw).is_ok();
    drop(std::fs::remove_file(if accepted { &raw } else { &valid }));
    accepted
}

/// A save path whose basename is invalid UTF-8 must still be writable, and
/// the sibling it stages under must land beside it, in the same directory:
/// `staging_path_with_caps` renders that basename through `to_string_lossy`
/// and a char-boundary truncation, which is only exercised by a non-UTF-8
/// input.
#[cfg(unix)]
#[test]
fn a_non_utf8_basename_stages_and_writes_in_the_same_directory() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt as _;

    let dir = TempDir::new("non-utf8-basename");
    if !host_accepts_a_non_utf8_filename(&dir.path) {
        return;
    }
    let path = dir
        .path
        .join(OsString::from_vec(vec![b's', b'a', 0xFF, b'v']));
    assert!(path.file_name().unwrap().to_str().is_none());
    let file = SaveFile::at(&path);
    let (store, _, _) = saved_store();

    let staged_parent = std::cell::RefCell::new(None);
    file.write_with(
        &store,
        SaveFile::sync_directory_best_effort,
        |bytes| area(&file).stage_narrowing_until_accepted(create_new_exclusive, bytes),
        |staged| *staged_parent.borrow_mut() = staged.parent().map(Path::to_path_buf),
    )
    .expect("a non-UTF-8 basename must still be writable");

    assert_eq!(
        staged_parent.into_inner(),
        Some(dir.path.clone()),
        "the staged sibling must be created beside the save path, in the same directory"
    );
    assert!(file.exists());

    let reloaded = file.read().unwrap().expect("the save must be readable");
    assert_eq!(reloaded.flash_image(), store.flash_image());
}

/// A narrowed staging namespace is walked without repeats, so occupancy
/// alone can never report exhaustion while a free name is still there. The
/// one-hex-digit floor holds only sixteen names; eight independent draws
/// over sixteen revisit names already found taken, and with fifteen held
/// they give up about three times in five with the survivor untried.
/// Pinned through an injected `open` that refuses anything longer than the
/// save path plus five bytes, so the shrink chain has nowhere to go but
/// that floor.
#[test]
fn a_narrowed_staging_namespace_is_walked_to_its_last_free_name() {
    const FREE_DIGIT: char = 'd';

    let dir = TempDir::new("staging-namespace-exhaustion");
    let path = dir.join("s");
    let file = SaveFile::at(&path);

    // Every name the floor can render but one, so only a walk that tries
    // each of the sixteen once is certain to reach the survivor.
    for digit in "0123456789abcdef".chars() {
        if digit != FREE_DIGIT {
            std::fs::write(dir.join(&format!(".tmp.{digit}")), b"someone else's file").unwrap();
        }
    }

    let injected_limit = path.as_os_str().as_encoded_bytes().len() + 5;
    let refuse_long_paths = |candidate: &Path| -> std::io::Result<std::fs::File> {
        if candidate.as_os_str().as_encoded_bytes().len() > injected_limit {
            return Err(std::io::Error::from(std::io::ErrorKind::InvalidFilename));
        }
        create_new_exclusive(candidate)
    };

    let staged = area(&file)
        .stage_narrowing_until_accepted(refuse_long_paths, &vec![0u8; FLASH_IMAGE_LEN])
        .expect(
            "a narrowed namespace holding one free name must be walked to it, not \
             reported exhausted",
        );

    assert_eq!(
        staged.path,
        dir.join(&format!(".tmp.{FREE_DIGIT}")),
        "the one name left free at the floor is the only one staging could have taken"
    );
}

/// The value a two-digit staging suffix renders. The stem is already empty
/// wherever the shrink chain has narrowed the suffix this far, so the whole
/// component is the suffix.
fn two_digit_suffix_of(candidate: &Path) -> u8 {
    let component = candidate
        .file_name()
        .and_then(|name| name.to_str())
        .expect("a staging candidate always has a UTF-8 name here");
    let hex = component
        .strip_prefix(".tmp.")
        .unwrap_or_else(|| panic!("a narrowed staging candidate is all suffix: {component}"));
    u8::from_str_radix(hex, 16)
        .unwrap_or_else(|err| panic!("{component} must render two hex digits: {err}"))
}

/// Each width the shrink chain lands on is walked to the end of its own
/// namespace, not to the count the narrowest width happens to hold. Two hex
/// digits render 256 names; a walk cut to sixteen of them reports the
/// namespace exhausted with 240 untried, so a start that lands on a run of
/// entries a crashed process never swept fails a write that had free names
/// in reach.
///
/// The run is seeded from inside the injected `open`, on the sixteen
/// consecutive names the walk actually starts from, so however the start was
/// drawn the seventeenth attempt is the first that can succeed.
#[test]
fn a_two_digit_staging_namespace_is_walked_past_its_first_sixteen_names() {
    const OCCUPIED_RUN: u8 = 16;

    let dir = TempDir::new("staging-two-digit-namespace");
    let path = dir.join("s");
    let file = SaveFile::at(&path);

    // Room for an empty stem and a two-digit suffix and no more, so the
    // shrink chain stops one rung above the floor rather than on it.
    let two_digit_limit = path
        .with_file_name(".tmp.00")
        .as_os_str()
        .as_encoded_bytes()
        .len();
    let start = std::cell::Cell::new(None);
    let occupy_the_start_of_the_walk = |candidate: &Path| -> std::io::Result<std::fs::File> {
        if candidate.as_os_str().as_encoded_bytes().len() > two_digit_limit {
            return Err(std::io::Error::from(std::io::ErrorKind::InvalidFilename));
        }
        if start.get().is_none() {
            let origin = two_digit_suffix_of(candidate);
            start.set(Some(origin));
            for step in 0..OCCUPIED_RUN {
                let taken = dir.join(&format!(".tmp.{:02x}", origin.wrapping_add(step)));
                std::fs::write(&taken, b"someone else's file").unwrap();
            }
        }
        create_new_exclusive(candidate)
    };

    let staged = area(&file)
        .stage_narrowing_until_accepted(occupy_the_start_of_the_walk, &vec![0u8; FLASH_IMAGE_LEN])
        .expect(
            "a two-digit namespace with 240 names free must be walked past the sixteen \
             taken ones, not reported exhausted",
        );

    let origin = start
        .get()
        .expect("the shrink chain must have reached the two-digit width");
    assert_eq!(
        staged.path,
        dir.join(&format!(".tmp.{:02x}", origin.wrapping_add(OCCUPIED_RUN))),
        "the walk must take the first free name past the occupied run"
    );
}

/// A non-UTF-8 basename's lossy rendering can be longer than its raw bytes
/// -- each invalid byte becomes a three-byte replacement character -- so the
/// first-guess candidate built from it can be longer than the raw basename
/// would need. The shrink chain must still bring it under an injected limit
/// the raw basename alone would have satisfied.
#[cfg(unix)]
#[test]
fn a_non_utf8_basename_whose_lossy_form_is_longer_still_respects_an_injected_limit() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt as _;

    // A limit the lossy candidate (15-byte inflated stem plus the 15-byte
    // suffix) exceeds, but that the 5-byte raw basename plus the suffix
    // would not have -- isolating the lossy-inflation scenario specifically.
    const INJECTED_LIMIT: usize = 20;

    // Five invalid bytes, each rendered as a three-byte U+FFFD replacement
    // character, so the lossy stem (15 bytes) is three times longer than
    // the raw basename (5 bytes) it was built from.
    let raw_basename = vec![0xFFu8; 5];
    assert!(std::str::from_utf8(&raw_basename).is_err());
    assert!(raw_basename.len() + 15 <= INJECTED_LIMIT);

    let dir = TempDir::new("non-utf8-lossy-limit");
    let path = dir.path.join(OsString::from_vec(raw_basename.clone()));
    let file = SaveFile::at(&path);

    let refuse_long_components = |candidate: &Path| -> std::io::Result<std::fs::File> {
        let component_len = candidate
            .file_name()
            .map_or(0, |name| name.as_encoded_bytes().len());
        if component_len > INJECTED_LIMIT {
            return Err(std::io::Error::from(std::io::ErrorKind::InvalidFilename));
        }
        create_new_exclusive(candidate)
    };

    let staged = area(&file)
        .stage_narrowing_until_accepted(refuse_long_components, &vec![0u8; FLASH_IMAGE_LEN])
        .expect("a lossy-inflated non-UTF-8 stem must still be satisfiable by shrinking");

    let final_component_len = staged
        .path
        .file_name()
        .map_or(0, |name| name.as_encoded_bytes().len());
    assert!(
        final_component_len <= INJECTED_LIMIT,
        "the staged sibling must respect the injected limit even when the lossy \
         rendering of a non-UTF-8 basename is longer than its raw bytes: {final_component_len} bytes"
    );
    assert!(
        final_component_len < raw_basename.len() + 15 + 15,
        "the stem must actually have been shortened from the lossy first-guess \
         candidate, not merely have succeeded by chance: {final_component_len} bytes"
    );

    std::fs::remove_file(&staged.path).unwrap();
}

#[test]
fn a_staging_name_collision_is_retried_onto_a_fresh_name() {
    let dir = TempDir::new("staging-retry");
    let path = dir.join(SAVE_FILE_NAME);
    let file = SaveFile::at(&path);
    let (store, _, _) = saved_store();

    // The first attempt lands on an occupied name and the second on a free
    // one, so only a retry that carries on past the collision can write.
    let occupied = sibling_path(&path, ".tmp.occupied");
    std::fs::write(&occupied, b"someone else's file").unwrap();
    let fresh = sibling_path(&path, ".tmp.fresh");
    let mut attempts = 0;
    file.write_with(
        &store,
        SaveFile::sync_directory_best_effort,
        |bytes| {
            stage_at_first_free_name(
                std::iter::repeat_with(|| {
                    attempts += 1;
                    if attempts == 1 {
                        occupied.clone()
                    } else {
                        fresh.clone()
                    }
                })
                .take(2),
                create_new_exclusive,
                bytes,
            )
        },
        |_| {},
    )
    .expect("a staging-name collision must be retried onto a fresh name");

    assert_eq!(
        attempts, 2,
        "the collision must cost exactly one extra attempt"
    );
    assert_eq!(
        std::fs::read(&occupied).unwrap(),
        b"someone else's file",
        "a colliding staging attempt must leave the file already at that name alone"
    );
    assert!(
        !fresh.exists(),
        "the retried staged file must be renamed into place, not left on disk"
    );
    let reloaded = file.read().unwrap().expect("the save must be readable");
    assert_eq!(reloaded.flash_image(), store.flash_image());
}

/// Set in the re-executed child of
/// [`a_staged_write_that_fails_after_creating_its_file_removes_it`].
#[cfg(target_os = "linux")]
const STAGING_WRITE_FAILURE_CHILD: &str = "POKEEMERALD_RS_STAGING_WRITE_FAILURE_CHILD";

#[cfg(target_os = "linux")]
#[test]
fn a_staged_write_that_fails_after_creating_its_file_removes_it() {
    // No in-process API can fail a write to a freshly created file, so the
    // child re-executes this test under a file-size limit that the flash
    // image exceeds, with SIGXFSZ ignored so the failure surfaces as EFBIG.
    if std::env::var_os(STAGING_WRITE_FAILURE_CHILD).is_some() {
        let dir = TempDir::new("staging-write-failure");
        let staged = dir.join("staged.tmp");
        let err = fill_new_file(create_new_exclusive, &staged, &vec![0u8; FLASH_IMAGE_LEN])
            .expect_err("a staged write over the file-size limit cannot succeed");
        assert_eq!(err.kind(), std::io::ErrorKind::FileTooLarge, "{err:?}");
        assert!(
            !staged.exists(),
            "a staged file whose write failed must be cleaned up, not left beside the save"
        );
        return;
    }

    let exe = std::env::current_exe().expect("the test binary must be locatable");
    let output = std::process::Command::new("bash")
        .arg("-c")
        .arg(r#"trap "" XFSZ; ulimit -f 1; exec "$0" "$1" --exact --nocapture"#)
        .arg(&exe)
        .arg("save::file::staging::tests::a_staged_write_that_fails_after_creating_its_file_removes_it")
        .env(STAGING_WRITE_FAILURE_CHILD, "1")
        .output()
        .expect("the child test process must be spawnable");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("1 passed"),
        "child status {:?}\nstdout:\n{stdout}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(unix)]
#[test]
fn a_symlink_planted_at_the_guessable_pid_staging_name_is_not_followed() {
    let dir = TempDir::new("staging-symlink");
    let path = dir.join(SAVE_FILE_NAME);
    let bystander = dir.join("bystander");
    std::fs::write(&bystander, b"not a save file").unwrap();

    // A symlink here targets the one name an attacker without this process's
    // clock or entropy could still guess; the real staging name never
    // matches it, and `create_new` refuses a symlink regardless.
    std::os::unix::fs::symlink(&bystander, guessable_pid_staging_path(&path)).unwrap();

    let file = SaveFile::at(&path);
    let (store, _, _) = saved_store();
    file.write(&store)
        .expect("planting a decoy symlink must not fail the write");

    assert_eq!(
        std::fs::read(&bystander).unwrap(),
        b"not a save file",
        "a symlink planted at a staging name must never redirect the flash \
         image onto the file it points at"
    );
    assert!(
        !std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_symlink()),
        "the save path must hold a save image, not a planted symlink"
    );
    let reloaded = file.read().unwrap().expect("the save must be readable");
    assert_eq!(reloaded.flash_image(), store.flash_image());
}

#[cfg(unix)]
#[test]
fn a_symlink_at_the_staging_path_the_write_actually_uses_is_refused() {
    let dir = TempDir::new("staging-symlink-real-name");
    let path = dir.join(SAVE_FILE_NAME);
    let bystander = dir.join("bystander");
    std::fs::write(&bystander, b"not a save file").unwrap();

    // The symlink sits on the very name the staging attempt opens, so only
    // the exclusive `create_new` open can keep the flash image off its target.
    let staging = sibling_path(&path, ".tmp.planted");
    std::os::unix::fs::symlink(&bystander, &staging).unwrap();

    let file = SaveFile::at(&path);
    let (store, _, _) = saved_store();
    let err = file
        .write_with(
            &store,
            SaveFile::sync_directory_best_effort,
            |bytes| {
                stage_at_first_free_name(
                    std::iter::once(staging.clone()),
                    create_new_exclusive,
                    bytes,
                )
            },
            |_| {},
        )
        .expect_err("staging onto a planted symlink must never succeed");

    assert!(
        matches!(err, SaveFileError::Write { .. }),
        "a refused staged write must surface as a write failure: {err:?}"
    );
    assert_eq!(
        std::fs::read(&bystander).unwrap(),
        b"not a save file",
        "the exclusive open must not follow the symlink onto its target"
    );
    assert!(
        std::fs::symlink_metadata(&staging)
            .expect("the planted symlink must survive")
            .file_type()
            .is_symlink(),
        "a refused staging attempt must not delete the path another caller created"
    );
    assert!(
        !path.exists(),
        "a write that never got staged must not create the save file"
    );
}

#[cfg(unix)]
#[test]
fn cleaning_up_a_failed_staged_write_leaves_the_entry_that_replaced_it_alone() {
    let dir = TempDir::new("staging-cleanup-ownership");
    let bystander = dir.join("bystander");
    std::fs::write(&bystander, b"not a save file").unwrap();

    let staging = dir.join("staged.tmp");
    let staged = stage_at_first_free_name(
        std::iter::once(staging.clone()),
        create_new_exclusive,
        &vec![0u8; FLASH_IMAGE_LEN],
    )
    .expect("the exclusive staging write must succeed");
    std::fs::remove_file(&staging).unwrap();
    std::os::unix::fs::symlink(&bystander, &staging).unwrap();

    let err = staged.remove_after(std::io::Error::other("the rename failed"));

    assert_eq!(
        err.kind(),
        std::io::ErrorKind::Other,
        "cleanup that removed nothing must surface the failure it was given: {err:?}"
    );
    assert!(
        std::fs::symlink_metadata(&staging)
            .expect("the planted symlink must survive")
            .file_type()
            .is_symlink(),
        "cleanup must never remove an entry that replaced this call's staging file"
    );
    assert_eq!(
        std::fs::read(&bystander).unwrap(),
        b"not a save file",
        "cleanup must not reach the symlink's target either"
    );
}

/// A directory the ownership check can see before the unlink runs is never a
/// regular file, so it is caught by [`StagedSave::still_ours`] and no unlink
/// is even attempted -- `remove_after` only ever calls `std::fs::remove_file`
/// on an entry the check just confirmed is still this call's own staged
/// file. Only a peer that lands the swap in the narrow gap between that
/// check and the unlink -- the same accepted bound the rename window above
/// documents -- could make the unlink itself observe a directory there, and
/// no cheap seam exists to force that exact interleaving deterministically.
#[cfg(unix)]
#[test]
fn a_directory_that_replaces_the_staging_entry_survives_cleanup() {
    let dir = TempDir::new("staging-cleanup-directory-swap");

    let staging = dir.join("staged.tmp");
    let staged = stage_at_first_free_name(
        std::iter::once(staging.clone()),
        create_new_exclusive,
        &vec![0u8; FLASH_IMAGE_LEN],
    )
    .expect("the exclusive staging write must succeed");
    std::fs::remove_file(&staging).unwrap();
    std::fs::create_dir(&staging).unwrap();

    let err = staged.remove_after(std::io::Error::other("the rename failed"));

    assert_eq!(
        err.kind(),
        std::io::ErrorKind::Other,
        "a directory is never a regular file, so cleanup must never attempt to unlink \
         it and must surface the failure it was given unmodified: {err:?}"
    );
    assert!(
        std::fs::symlink_metadata(&staging)
            .expect("the planted directory must survive")
            .file_type()
            .is_dir(),
        "cleanup must never remove a directory that replaced this call's staging file -- \
         std::fs::remove_file cannot unlink one"
    );
}

#[test]
fn two_save_files_on_one_path_never_share_a_staging_name() {
    let dir = TempDir::new("same-path-staging-names");
    let path = dir.join(SAVE_FILE_NAME);

    // Two independent `SaveFile` values on the identical path, not clones of
    // one -- each must derive its own staging name. Observed directly: a
    // race between two writers proves nothing when either can finish staging
    // before the other one even starts.
    let first = SaveFile::at(&path);
    let second = SaveFile::at(&path);
    let first_staging = area(&first).first_name();
    let second_staging = area(&second).first_name();
    let guessable_name = guessable_pid_staging_path(&path);

    assert_ne!(
        first_staging, second_staging,
        "two SaveFile values on the same path must never derive the same staging name"
    );
    assert_ne!(
        first_staging, guessable_name,
        "the staging name must not be the guessable, symlink-plantable <save>.tmp.<pid> form"
    );
    assert_ne!(
        second_staging, guessable_name,
        "the staging name must not be the guessable, symlink-plantable <save>.tmp.<pid> form"
    );

    // Distinct staging names also mean two writers on one path can be
    // sequenced without either clobbering the other's in-flight staging file.
    let (first_store, _, block2) = saved_store();
    first
        .write(&first_store)
        .expect("the first writer must succeed");

    let mut second_store = first_store.clone();
    second_store.save(
        &SaveBlock1 {
            money: 222_222,
            ..SaveBlock1::default()
        },
        &block2,
    );
    second
        .write(&second_store)
        .expect("the second writer must succeed");

    let reloaded = second.read().unwrap().expect("the save must be readable");
    assert_eq!(reloaded.flash_image(), second_store.flash_image());
}
