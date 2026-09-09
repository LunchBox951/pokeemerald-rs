use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::{
    data_dir_for, default_save_path_from, HostFamily, SaveFile, SaveFileError, SAVE_DIR_NAME,
    SAVE_FILE_NAME, SAVE_PATH_ENV,
};
use crate::save::block::{SaveBlock1, SaveBlock2};
use crate::save::store::{SaveStatus, SaveStore, FLASH_IMAGE_LEN};

fn env_of<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
    move |name| {
        pairs
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| OsString::from(*value))
    }
}

fn expected_sibling_path(save_path: &Path, suffix: impl AsRef<std::ffi::OsStr>) -> PathBuf {
    let mut path = save_path.as_os_str().to_os_string();
    path.push(suffix);
    PathBuf::from(path)
}

fn expected_staging_path(save_path: &Path) -> PathBuf {
    expected_sibling_path(save_path, format!(".tmp.{}", std::process::id()))
}

/// Every save path whose own name is valid gets a valid staging sibling:
/// the unique suffix is fixed-width, and a basename at the component limit
/// is cut to make room for it rather than pushed over.
#[test]
fn a_basename_at_the_component_limit_still_gets_a_valid_staging_sibling() {
    let file = SaveFile::at(Path::new(&"s".repeat(SaveFile::MAX_COMPONENT_LEN)));

    let staging = file.staging_path();
    let component = staging.file_name().unwrap().to_str().unwrap();
    assert_eq!(component.len(), SaveFile::MAX_COMPONENT_LEN, "{component}");
    let suffix_at = component.len() - ".tmp.".len() - SaveFile::UNIQUE_COMPONENT_HEX_DIGITS;
    assert!(component[..suffix_at].bytes().all(|byte| byte == b's'));
    assert!(component[suffix_at..].starts_with(".tmp."));
    assert!(component[suffix_at + 5..]
        .bytes()
        .all(|byte| byte.is_ascii_hexdigit()));

    assert_ne!(
        file.staging_path(),
        file.staging_path(),
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
        let candidate = file.staging_path_with_caps(0, 1);
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
        let candidate = file.staging_path_with_caps(0, 1);
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

/// The longest save basename whose `.tmp.<pid>` staging sibling fit within
/// the filesystem's 255-byte component limit must still be writable: the
/// fixed-width suffix may not push a previously valid basename over.
#[test]
fn a_basename_that_fit_the_former_staging_suffix_still_writes() {
    let dir = TempDir::new("longname");
    let pid_digits = std::process::id().to_string().len();
    let basename_len = SaveFile::MAX_COMPONENT_LEN - ".tmp.".len() - pid_digits;
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
    (1..=SaveFile::MAX_COMPONENT_LEN)
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
/// fixed-width suffix's own 15 bytes of room. Neither
/// [`SaveFile::staging_path`]'s first-guess candidate nor the old stem-only
/// shrink chain's floor -- an empty stem with the full-width hex suffix --
/// fits there (proving the scenario is real, not merely approached);
/// [`SaveFile::write`] must still succeed by also narrowing the hex
/// component, which is asserted here by the write's own success rather than
/// by inspecting which candidate ultimately won.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn a_save_path_near_the_hosts_real_path_limit_still_writes() {
    let dir = TempDir::new("path-limit");

    // Coarse phase: nest maximally sized directories until the host refuses
    // one, to approach its real ceiling quickly without assuming a specific
    // number for it.
    let mut parent = dir.path.clone();
    loop {
        let candidate = parent.join("d".repeat(SaveFile::MAX_COMPONENT_LEN));
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
        let nested_len = (headroom - 10).min(SaveFile::MAX_COMPONENT_LEN);
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

    // Neither the naive first-guess candidate nor the old stem-only shrink
    // chain's floor -- an empty stem with the full-width hex suffix -- fits
    // here; this is exactly the scenario under test.
    let naive_candidate = file.staging_path();
    assert!(
        std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&naive_candidate)
            .is_err_and(|err| err.kind() == std::io::ErrorKind::InvalidFilename),
        "test setup must actually exceed the host's real limit, not merely approach it"
    );
    let empty_stem_candidate =
        file.staging_path_with_caps(0, SaveFile::UNIQUE_COMPONENT_HEX_DIGITS);
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
/// [`SaveFile::MAX_COMPONENT_LEN`]'s first guess -- eCryptfs caps a
/// component at 143 bytes, not 255 -- must still get a valid staging
/// sibling: the first-guess candidate is refused outright, and that must
/// shrink the stem and retry rather than propagate immediately. Pinned
/// through an injected `open` rather than a real eCryptfs mount, so this
/// holds on every platform this crate builds for, not just whichever one
/// happens to have such a filesystem mounted.
#[test]
fn a_host_component_limit_below_the_first_guess_still_gets_a_staging_sibling() {
    const HOST_NAME_MAX: usize = 143;
    // Long enough that a former, short `.tmp.<pid>` suffix still fit under
    // the host's real limit, but the current fixed 15-byte suffix does not
    // -- and short enough that `MAX_COMPONENT_LEN`'s 255-byte first guess
    // does not truncate it at all, so the very first candidate is refused.
    const BASENAME_LEN: usize = 131;
    let pid_digits = std::process::id().to_string().len();
    assert!(BASENAME_LEN + ".tmp.".len() + pid_digits <= HOST_NAME_MAX);
    const { assert!(BASENAME_LEN + 15 > HOST_NAME_MAX) };
    const { assert!(BASENAME_LEN < SaveFile::MAX_COMPONENT_LEN - 15) };

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
        SaveFile::real_open(candidate)
    };

    let staged = file
        .stage_shrinking_on_invalid_filename(refuse_long_components, &vec![0u8; FLASH_IMAGE_LEN])
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

/// A directory that leaves room for the former, short `.tmp.<pid>` name but
/// not for the current fixed-width `.tmp.<hex>` suffix must still get a
/// staging sibling: an empty stem alone is not enough when the suffix
/// itself no longer fits, so the shrink chain must narrow
/// [`SaveFile::UNIQUE_COMPONENT_HEX_DIGITS`] too. Pinned through an injected
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
        SaveFile::real_open(candidate)
    };

    let staged = file
        .stage_shrinking_on_invalid_filename(refuse_long_paths, &vec![0u8; FLASH_IMAGE_LEN])
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
        |bytes| file.stage_shrinking_on_invalid_filename(SaveFile::real_open, bytes),
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
        SaveFile::real_open(candidate)
    };

    let staged = file
        .stage_shrinking_on_invalid_filename(refuse_long_paths, &vec![0u8; FLASH_IMAGE_LEN])
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
        SaveFile::real_open(candidate)
    };

    let staged = file
        .stage_shrinking_on_invalid_filename(refuse_long_components, &vec![0u8; FLASH_IMAGE_LEN])
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

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "pokeemerald-rs-save-file-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        drop(std::fs::remove_dir_all(&path));
        std::fs::create_dir_all(&path).expect("scratch directory must be creatable");
        Self { path }
    }

    fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.path));
    }
}

#[test]
fn windows_data_dir_prefers_appdata_then_falls_back_to_the_user_profile() {
    let with_appdata = env_of(&[("APPDATA", "C:/Users/May/AppData/Roaming")]);
    assert_eq!(
        data_dir_for(HostFamily::Windows, with_appdata),
        Some(PathBuf::from("C:/Users/May/AppData/Roaming"))
    );

    let profile_only = env_of(&[("USERPROFILE", "C:/Users/May")]);
    assert_eq!(
        data_dir_for(HostFamily::Windows, profile_only),
        Some(
            PathBuf::from("C:/Users/May")
                .join("AppData")
                .join("Roaming")
        )
    );

    assert_eq!(data_dir_for(HostFamily::Windows, env_of(&[])), None);
}

#[test]
fn macos_data_dir_is_the_application_support_directory() {
    assert_eq!(
        data_dir_for(HostFamily::MacOs, env_of(&[("HOME", "/Users/may")])),
        Some(
            PathBuf::from("/Users/may")
                .join("Library")
                .join("Application Support")
        )
    );
    assert_eq!(data_dir_for(HostFamily::MacOs, env_of(&[])), None);
}

#[test]
fn xdg_data_dir_prefers_an_absolute_xdg_data_home() {
    let absolute = env_of(&[("XDG_DATA_HOME", "/srv/data"), ("HOME", "/home/may")]);
    assert_eq!(
        data_dir_for(HostFamily::Xdg, absolute),
        Some(PathBuf::from("/srv/data"))
    );
}

#[test]
fn xdg_data_dir_ignores_a_relative_xdg_data_home_and_uses_home() {
    let relative = env_of(&[("XDG_DATA_HOME", "data"), ("HOME", "/home/may")]);
    assert_eq!(
        data_dir_for(HostFamily::Xdg, relative),
        Some(PathBuf::from("/home/may").join(".local").join("share"))
    );
    assert_eq!(data_dir_for(HostFamily::Xdg, env_of(&[])), None);
}

#[test]
fn an_empty_environment_variable_counts_as_unset() {
    let empty = env_of(&[("HOME", ""), ("XDG_DATA_HOME", "")]);
    assert_eq!(data_dir_for(HostFamily::Xdg, empty), None);
}

#[test]
fn the_save_path_override_wins_over_every_data_directory() {
    let env = env_of(&[(SAVE_PATH_ENV, "/tmp/scratch.sav"), ("HOME", "/home/may")]);
    assert_eq!(
        default_save_path_from(HostFamily::Xdg, env).unwrap(),
        PathBuf::from("/tmp/scratch.sav")
    );
}

#[test]
fn without_an_override_the_save_path_is_the_named_file_under_the_data_directory() {
    let env = env_of(&[("HOME", "/home/may")]);
    assert_eq!(
        default_save_path_from(HostFamily::Xdg, env).unwrap(),
        PathBuf::from("/home/may")
            .join(".local")
            .join("share")
            .join(SAVE_DIR_NAME)
            .join(SAVE_FILE_NAME)
    );
}

#[test]
fn an_empty_override_falls_through_to_the_data_directory() {
    let env = env_of(&[(SAVE_PATH_ENV, ""), ("HOME", "/home/may")]);
    assert!(default_save_path_from(HostFamily::Xdg, env)
        .unwrap()
        .starts_with("/home/may"));
}

#[test]
fn no_data_directory_is_a_named_error_not_a_guessed_path() {
    let err = default_save_path_from(HostFamily::Xdg, env_of(&[])).unwrap_err();
    assert!(matches!(err, SaveFileError::NoDataDirectory));
    assert!(
        err.to_string().contains(SAVE_PATH_ENV),
        "the diagnostic must name the override that fixes it: {err}"
    );
}

fn saved_store() -> (SaveStore, SaveBlock1, SaveBlock2) {
    let block2 = SaveBlock2 {
        encryption_key: 0x1234_5678,
        ..SaveBlock2::default()
    };
    let block1 = SaveBlock1 {
        money: 4321,
        ..SaveBlock1::default()
    };
    let mut store = SaveStore::new();
    store.save(&block1, &block2);
    (store, block1, block2)
}

#[test]
fn reading_a_path_with_no_file_reports_no_save_rather_than_an_error() {
    let dir = TempDir::new("missing");
    let file = SaveFile::at(dir.join(SAVE_FILE_NAME));
    assert!(!file.exists());
    assert!(file.read().unwrap().is_none());
}

#[test]
fn a_written_image_reads_back_byte_identical() {
    let dir = TempDir::new("roundtrip");
    let file = SaveFile::at(dir.join(SAVE_FILE_NAME));
    let (store, _, _) = saved_store();

    file.write(&store).unwrap();
    assert!(file.exists());

    let reloaded = file.read().unwrap().expect("the file was just written");
    assert_eq!(reloaded.flash_image(), store.flash_image());
}

#[test]
fn a_written_save_reloads_through_the_stores_own_validation() {
    let dir = TempDir::new("reload");
    let file = SaveFile::at(dir.join(SAVE_FILE_NAME));
    let (store, block1, block2) = saved_store();
    file.write(&store).unwrap();

    let mut reloaded = file.read().unwrap().expect("the file was just written");
    let outcome = reloaded.load();

    assert_eq!(outcome.status, SaveStatus::Ok);
    assert_eq!(outcome.block2.encryption_key, block2.encryption_key);
    assert_eq!(outcome.block1.money, block1.money);
    assert_eq!(reloaded.save_counter(), store.save_counter());
    assert_eq!(reloaded.last_written_sector(), store.last_written_sector());
}

#[test]
fn writing_creates_the_parent_directory() {
    let dir = TempDir::new("mkdir");
    let file = SaveFile::at(dir.join("nested").join("deeper").join(SAVE_FILE_NAME));
    let (store, _, _) = saved_store();

    file.write(&store).unwrap();
    assert!(file.exists());
}

#[test]
fn writing_leaves_no_temporary_file_behind() {
    let dir = TempDir::new("atomic");
    let path = dir.join(SAVE_FILE_NAME);
    let file = SaveFile::at(&path);
    let (store, _, _) = saved_store();

    file.write(&store).unwrap();

    let leftover_staging_names: Vec<_> = std::fs::read_dir(&dir.path)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.file_name())
        .filter(|name| {
            name.to_string_lossy()
                .starts_with(&format!("{SAVE_FILE_NAME}.tmp."))
        })
        .collect();
    assert!(
        leftover_staging_names.is_empty(),
        "the staged temporary must be renamed away, not left on disk: {leftover_staging_names:?}"
    );
}

#[test]
fn a_write_that_cannot_be_staged_leaves_the_previous_save_byte_identical() {
    let dir = TempDir::new("staging");
    let path = dir.join(SAVE_FILE_NAME);
    let file = SaveFile::at(&path);

    let (first, _, block2) = saved_store();
    file.write(&first).unwrap();
    let original = std::fs::read(&path).expect("the first save must be readable");

    // Every staging attempt is pointed at the same pre-existing directory, so
    // every attempt -- and the retry bound -- collides, forcing the failure
    // this test exercises without depending on the real staging name's shape.
    let unstageable = expected_staging_path(&path);
    std::fs::create_dir_all(&unstageable).unwrap();

    let mut second = first.clone();
    second.save(
        &SaveBlock1 {
            money: 777_777,
            ..SaveBlock1::default()
        },
        &block2,
    );
    let err = file
        .write_with(
            &second,
            SaveFile::sync_directory_best_effort,
            |bytes| SaveFile::stage(|| unstageable.clone(), SaveFile::real_open, bytes),
            |_| {},
        )
        .expect_err("staging into a directory cannot succeed");
    assert!(
        matches!(err, SaveFileError::Write { .. }),
        "a failed staged write must surface as a write failure: {err:?}"
    );
    assert_eq!(
        std::fs::read(&path).expect("the previous save must still be there"),
        original,
        "a write that never got staged must not touch the image already on \
         disk -- writing straight to the destination would lose both \
         rotating slots at once"
    );
}

#[test]
fn a_staging_name_collision_is_retried_onto_a_fresh_name() {
    let dir = TempDir::new("staging-retry");
    let path = dir.join(SAVE_FILE_NAME);
    let file = SaveFile::at(&path);
    let (store, _, _) = saved_store();

    // The first attempt lands on an occupied name and the second on a free
    // one, so only a retry that carries on past the collision can write.
    let occupied = expected_sibling_path(&path, ".tmp.occupied");
    std::fs::write(&occupied, b"someone else's file").unwrap();
    let fresh = expected_sibling_path(&path, ".tmp.fresh");
    let mut attempts = 0;
    file.write_with(
        &store,
        SaveFile::sync_directory_best_effort,
        |bytes| {
            SaveFile::stage(
                || {
                    attempts += 1;
                    if attempts == 1 {
                        occupied.clone()
                    } else {
                        fresh.clone()
                    }
                },
                SaveFile::real_open,
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

#[test]
fn a_staged_write_that_cannot_be_renamed_removes_its_staged_file() {
    let dir = TempDir::new("staging-rename-failure");
    let path = dir.join(SAVE_FILE_NAME);
    // A directory at the save path stages fine but can never be renamed onto,
    // so the failure lands after the staged file exists.
    std::fs::create_dir_all(&path).unwrap();
    let file = SaveFile::at(&path);
    let (store, _, _) = saved_store();

    let staged = expected_sibling_path(&path, ".tmp.staged");
    let err = file
        .write_with(
            &store,
            SaveFile::sync_directory_best_effort,
            |bytes| SaveFile::stage(|| staged.clone(), SaveFile::real_open, bytes),
            |_| {},
        )
        .expect_err("renaming onto a directory cannot succeed");

    assert!(
        matches!(err, SaveFileError::Write { .. }),
        "a failed rename must surface as a write failure: {err:?}"
    );
    assert!(
        !staged.exists(),
        "a staged file whose rename failed must be cleaned up, not left beside the save"
    );
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
        let err = SaveFile::write_and_sync(&staged, &vec![0u8; FLASH_IMAGE_LEN])
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
        .arg("save::file::tests::a_staged_write_that_fails_after_creating_its_file_removes_it")
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
fn a_symlink_planted_at_the_old_deterministic_staging_name_is_not_followed() {
    let dir = TempDir::new("staging-symlink");
    let path = dir.join(SAVE_FILE_NAME);
    let bystander = dir.join("bystander");
    std::fs::write(&bystander, b"not a save file").unwrap();

    // A symlink here targets the one deterministic name an attacker without
    // this process's clock or entropy could still guess; the real staging
    // name never matches it, and `create_new` refuses a symlink regardless.
    std::os::unix::fs::symlink(&bystander, expected_staging_path(&path)).unwrap();

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
    let staging = expected_sibling_path(&path, ".tmp.planted");
    std::os::unix::fs::symlink(&bystander, &staging).unwrap();

    let file = SaveFile::at(&path);
    let (store, _, _) = saved_store();
    let err = file
        .write_with(
            &store,
            SaveFile::sync_directory_best_effort,
            |bytes| SaveFile::stage(|| staging.clone(), SaveFile::real_open, bytes),
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
fn a_staging_file_replaced_before_the_rename_is_never_promoted() {
    let dir = TempDir::new("staging-replaced-before-rename");
    let path = dir.join(SAVE_FILE_NAME);
    let bystander = dir.join("bystander");
    std::fs::write(&bystander, b"not a save file").unwrap();

    let file = SaveFile::at(&path);
    let (first, _, block2) = saved_store();
    file.write(&first).unwrap();
    let original = std::fs::read(&path).expect("the first save must be readable");

    let mut second = first.clone();
    second.save(
        &SaveBlock1 {
            money: 424_242,
            ..SaveBlock1::default()
        },
        &block2,
    );

    // The window the exclusive create alone cannot defend: the staged file is
    // unlinked and a symlink takes its name after the create proved the name
    // was free, so only an ownership check standing between the write and the
    // rename can keep the replacement off the save path.
    let staging = expected_sibling_path(&path, ".tmp.replaced");
    let err = file
        .write_with(
            &second,
            SaveFile::sync_directory_best_effort,
            |bytes| SaveFile::stage(|| staging.clone(), SaveFile::real_open, bytes),
            |staged| {
                std::fs::remove_file(staged).unwrap();
                std::os::unix::fs::symlink(&bystander, staged).unwrap();
            },
        )
        .expect_err("a write that lost its staged file must never report success");

    assert!(
        matches!(&err, SaveFileError::Write { source, .. }
            if source.kind() == std::io::ErrorKind::InvalidData),
        "a replaced staging file must fail closed, not pass for the staged image: {err:?}"
    );
    assert!(
        !std::fs::symlink_metadata(&path)
            .expect("the previous save must still be there")
            .file_type()
            .is_symlink(),
        "the save path must never become the planted symlink"
    );
    assert_eq!(
        std::fs::read(&path).unwrap(),
        original,
        "a refused promotion must leave the image already on disk untouched"
    );
    assert_eq!(
        std::fs::read(&bystander).unwrap(),
        b"not a save file",
        "a refused promotion must not reach the symlink's target"
    );
    assert!(
        std::fs::symlink_metadata(&staging)
            .expect("the planted symlink must survive")
            .file_type()
            .is_symlink(),
        "a refused promotion must not delete the entry that replaced ours"
    );
}

#[cfg(unix)]
#[test]
fn a_staging_file_swapped_for_another_regular_file_is_never_promoted() {
    let dir = TempDir::new("staging-swapped-before-rename");
    let path = dir.join(SAVE_FILE_NAME);

    let file = SaveFile::at(&path);
    let (first, _, block2) = saved_store();
    file.write(&first).unwrap();
    let original = std::fs::read(&path).expect("the first save must be readable");

    let mut second = first.clone();
    second.save(
        &SaveBlock1 {
            money: 515_151,
            ..SaveBlock1::default()
        },
        &block2,
    );

    // A regular file walks straight past the "not a symlink, not a
    // directory" test, so nothing but the staged handle's own device and
    // inode can tell this impostor from the image this call wrote.
    let staging = expected_sibling_path(&path, ".tmp.swapped");
    let err = file
        .write_with(
            &second,
            SaveFile::sync_directory_best_effort,
            |bytes| SaveFile::stage(|| staging.clone(), SaveFile::real_open, bytes),
            |staged| {
                std::fs::remove_file(staged).unwrap();
                std::fs::write(staged, b"someone else's file").unwrap();
            },
        )
        .expect_err("a write that lost its staged file must never report success");

    assert!(
        matches!(&err, SaveFileError::Write { source, .. }
            if source.kind() == std::io::ErrorKind::InvalidData),
        "a swapped staging file must fail closed, not pass for the staged image: {err:?}"
    );
    assert_eq!(
        std::fs::read(&path).unwrap(),
        original,
        "a refused promotion must leave the image already on disk untouched"
    );
    assert_eq!(
        std::fs::read(&staging).unwrap(),
        b"someone else's file",
        "a refused promotion must not delete the entry that replaced ours"
    );
}

#[cfg(unix)]
#[test]
fn cleaning_up_a_failed_staged_write_leaves_the_entry_that_replaced_it_alone() {
    let dir = TempDir::new("staging-cleanup-ownership");
    let bystander = dir.join("bystander");
    std::fs::write(&bystander, b"not a save file").unwrap();

    let staging = dir.join("staged.tmp");
    let staged = SaveFile::stage(
        || staging.clone(),
        SaveFile::real_open,
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
/// regular file, so it is caught by [`StagedSave::still_named`] and no unlink
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
    let staged = SaveFile::stage(
        || staging.clone(),
        SaveFile::real_open,
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
    let first_staging = first.staging_path();
    let second_staging = second.staging_path();
    let old_deterministic_name = expected_staging_path(&path);

    assert_ne!(
        first_staging, second_staging,
        "two SaveFile values on the same path must never derive the same staging name"
    );
    assert_ne!(
        first_staging, old_deterministic_name,
        "the staging name must not be the old, symlink-plantable <save>.tmp.<pid> form"
    );
    assert_ne!(
        second_staging, old_deterministic_name,
        "the staging name must not be the old, symlink-plantable <save>.tmp.<pid> form"
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

#[test]
fn overwriting_an_existing_save_replaces_it_whole() {
    let dir = TempDir::new("overwrite");
    let file = SaveFile::at(dir.join(SAVE_FILE_NAME));
    let (mut store, _, block2) = saved_store();
    file.write(&store).unwrap();

    let second = SaveBlock1 {
        money: 999_999,
        ..SaveBlock1::default()
    };
    store.save(&second, &block2);
    file.write(&store).unwrap();

    let mut reloaded = file.read().unwrap().unwrap();
    let outcome = reloaded.load();
    assert_eq!(outcome.status, SaveStatus::Ok);
    assert_eq!(outcome.block1.money, 999_999);
}

#[test]
fn a_file_of_the_wrong_length_is_rejected_by_length_not_silently_padded() {
    let dir = TempDir::new("truncated");
    let path = dir.join(SAVE_FILE_NAME);
    std::fs::write(&path, vec![0u8; FLASH_IMAGE_LEN - 1]).unwrap();

    let err = SaveFile::at(&path).read().unwrap_err();
    match err {
        SaveFileError::BadLength { expected, got, .. } => {
            assert_eq!(expected, FLASH_IMAGE_LEN);
            assert_eq!(got, FLASH_IMAGE_LEN - 1);
        }
        other => panic!("expected a length rejection, got {other:?}"),
    }
}

#[test]
fn an_oversized_file_is_rejected_after_a_bounded_read() {
    let dir = TempDir::new("oversized");
    let path = dir.join(SAVE_FILE_NAME);
    std::fs::write(&path, vec![0u8; FLASH_IMAGE_LEN + 4096]).unwrap();

    let err = SaveFile::at(&path).read().unwrap_err();
    match err {
        SaveFileError::BadLength { expected, got, .. } => {
            assert_eq!(expected, FLASH_IMAGE_LEN);
            assert_eq!(got, FLASH_IMAGE_LEN + 1);
        }
        other => panic!("expected a length rejection, got {other:?}"),
    }
}

#[test]
fn reading_a_directory_in_the_files_place_is_an_io_error_not_a_panic() {
    let dir = TempDir::new("isdir");
    let path = dir.join(SAVE_FILE_NAME);
    std::fs::create_dir_all(&path).unwrap();

    let file = SaveFile::at(&path);
    assert!(!file.exists(), "a directory is not a save file");
    assert!(
        matches!(file.read(), Err(SaveFileError::Read { .. })),
        "reading a directory must surface as a read failure"
    );
}

/// The container of every level from the filesystem root down to `target`,
/// outermost first, computed independently of [`SaveFile::ancestor_chain`]
/// and [`SaveFile::directory_containing`] via [`Path::ancestors`].
fn expected_ancestor_containers(target: &Path) -> Vec<PathBuf> {
    let mut containers: Vec<PathBuf> = target
        .ancestors()
        .filter_map(|level| level.parent().map(Path::to_path_buf))
        .collect();
    containers.reverse();
    containers
}

#[test]
fn ancestor_chain_lists_every_level_outermost_first() {
    let dir = TempDir::new("ancestor-chain");
    let target = dir.join("one").join("two");

    let chain = SaveFile::ancestor_chain(&target);
    let mut expected: Vec<PathBuf> = target.ancestors().map(Path::to_path_buf).collect();
    expected.reverse();
    assert_eq!(
        chain, expected,
        "the chain must list every level from the filesystem root to the target, \
         outermost first, with nothing skipped or reordered"
    );
}

#[test]
fn locking_a_fresh_multi_level_root_syncs_the_whole_ancestor_chain() {
    let dir = TempDir::new("sync-created");
    let target = dir.join("one").join("two");
    let file = SaveFile::at(target.join(SAVE_FILE_NAME));

    let synced = std::cell::RefCell::new(Vec::new());
    let guard = file
        .lock_with(|path| synced.borrow_mut().push(path.to_path_buf()))
        .expect("a fresh multi-level root must be lockable");
    drop(guard);

    assert!(target.is_dir());
    assert_eq!(
        synced.into_inner(),
        expected_ancestor_containers(&target),
        "on a first save, every level's directory entry in its own container must be \
         synced, outermost first, and nothing else -- otherwise a created directory's \
         entry can be unsynced and vanish after a power loss, or an unrelated \
         directory can be synced unintentionally"
    );
}

#[test]
fn a_first_save_under_an_absolute_path_never_syncs_the_working_directory() {
    let dir = TempDir::new("absolute-no-cwd-sync");
    let file = SaveFile::at(dir.join(SAVE_FILE_NAME));
    assert!(dir.path.is_absolute(), "the temp root must be absolute");

    let synced = std::cell::RefCell::new(Vec::new());
    let guard = file
        .lock_with(|path| synced.borrow_mut().push(path.to_path_buf()))
        .expect("a fresh absolute path must be lockable");
    drop(guard);

    assert!(
        !synced.into_inner().contains(&PathBuf::from(".")),
        "a first save under an absolute path must never sync the process's working \
         directory -- the filesystem root has no container to record its own entry in"
    );
}

#[test]
fn directory_containing_has_no_entry_to_record_for_a_level_that_already_exists() {
    for level in [Path::new("/"), Path::new("."), Path::new("..")] {
        assert_eq!(
            SaveFile::directory_containing(level),
            None,
            "{level:?} always exists already, so it has no directory entry that \
             `create_dir_all` could have made and no container to synchronise"
        );
    }
}

#[test]
fn directory_containing_returns_the_parent_for_a_level_create_dir_all_can_make() {
    assert_eq!(
        SaveFile::directory_containing(Path::new("/tmp")),
        Some(Path::new("/"))
    );
    assert_eq!(
        SaveFile::directory_containing(Path::new("sub")),
        Some(Path::new(".")),
        "a bare relative name's entry lives in the working directory"
    );
    assert_eq!(
        SaveFile::directory_containing(Path::new("../saves")),
        Some(Path::new("..")),
        "the entry this level actually adds lives in its literal parent, not in \
         the working directory the whole relative path is resolved against"
    );
    assert_eq!(
        SaveFile::directory_containing(Path::new("./saves")),
        Some(Path::new(".")),
    );
}

#[test]
fn a_locker_that_wins_the_race_syncs_ancestors_an_earlier_contender_left_unsynced() {
    let dir = TempDir::new("race");
    let target = dir.join("one").join("two");
    let path = target.join(SAVE_FILE_NAME);

    // An earlier contender created the hierarchy but was pre-empted before
    // it could sync or lock -- its directories now exist on disk with
    // nobody yet having synced them.
    let first = SaveFile::at(&path);
    first.create_parent_directory().unwrap();

    let second = SaveFile::at(&path);
    let synced = std::cell::RefCell::new(Vec::new());
    let guard = second
        .lock_with(|p| synced.borrow_mut().push(p.to_path_buf()))
        .unwrap();
    drop(guard);

    assert_eq!(
        synced.into_inner(),
        expected_ancestor_containers(&target),
        "a locker must sync every ancestor's container of a first save regardless of \
         who created it on disk, and nothing else -- otherwise an earlier contender's \
         unsynced work can be reported as a successful save"
    );
}

#[test]
fn locking_after_a_successful_first_save_syncs_nothing_more() {
    let dir = TempDir::new("sync-after-first-save");
    let file = SaveFile::at(dir.join("one").join("two").join(SAVE_FILE_NAME));
    let (store, _, _) = saved_store();

    let guard = file.lock().unwrap();
    file.write(&store).unwrap();
    drop(guard);

    let synced = std::cell::RefCell::new(Vec::new());
    let guard = file
        .lock_with(|path| synced.borrow_mut().push(path.to_path_buf()))
        .unwrap();
    drop(guard);

    assert!(
        synced.into_inner().is_empty(),
        "once a save file exists, this is no longer a first save, so ancestors must \
         not be resynced on every subsequent lock"
    );
}

#[test]
fn locking_synchronises_ancestors_only_once_the_lock_is_held() {
    let dir = TempDir::new("sync-order");
    let path = dir.join("nested").join("deeper").join(SAVE_FILE_NAME);
    let file = SaveFile::at(&path);

    let synced_while_locked = std::cell::Cell::new(false);
    let guard = file
        .lock_with(|_ancestor_parent| {
            let probe = std::fs::OpenOptions::new()
                .write(true)
                .open(expected_sibling_path(&path, ".lock"))
                .expect("the lock file must already exist while ancestors are synced");
            synced_while_locked.set(matches!(
                probe.try_lock(),
                Err(std::fs::TryLockError::WouldBlock)
            ));
        })
        .expect("locking must create the missing hierarchy");
    drop(guard);

    assert!(
        synced_while_locked.get(),
        "ancestors must be synced only after this call holds the exclusive lock -- \
         syncing before locking would let a second, concurrent locker report a \
         successful save before either locker had made them durable"
    );
}

#[test]
fn locking_before_any_directory_exists_creates_the_whole_hierarchy() {
    let dir = TempDir::new("lock-mkdir");
    let path = dir.join("nested").join("deeper").join(SAVE_FILE_NAME);
    let file = SaveFile::at(&path);

    let guard = file
        .lock()
        .expect("locking must create the missing hierarchy");
    assert!(path.parent().unwrap().is_dir());
    assert!(expected_sibling_path(&path, ".lock").exists());

    let (store, _, _) = saved_store();
    file.write(&store).unwrap();
    drop(guard);

    let reloaded = file.read().unwrap().expect("the file was just written");
    assert_eq!(reloaded.flash_image(), store.flash_image());
}

#[test]
fn the_save_lock_excludes_a_second_locker_until_dropped() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let dir = TempDir::new("lock");
    let path = dir.join(SAVE_FILE_NAME);
    let file = SaveFile::at(&path);
    let first_lock_released = Arc::new(AtomicBool::new(false));

    let guard = file.lock().expect("first lock must succeed");

    let probe = std::fs::OpenOptions::new()
        .write(true)
        .open(expected_sibling_path(&path, ".lock"))
        .expect("the lock file exists while the guard is held");
    match probe.try_lock() {
        Err(std::fs::TryLockError::WouldBlock) => {}
        other => panic!("the held lock must exclude a second locker, got {other:?}"),
    }
    drop(probe);

    let contender = {
        let first_lock_released = Arc::clone(&first_lock_released);
        let file = SaveFile::at(&path);
        std::thread::spawn(move || {
            let _guard = file.lock().expect("second lock must eventually succeed");
            first_lock_released.load(Ordering::SeqCst)
        })
    };
    // This gives the contender a chance to block; the nonblocking probe above proves exclusion.
    std::thread::yield_now();
    first_lock_released.store(true, Ordering::SeqCst);
    drop(guard);
    assert!(
        contender.join().expect("contender must not panic"),
        "the second lock() returned while the first guard was still held"
    );
}

/// A bare relative save-file name, unique to this process and thread, that
/// removes itself on drop -- never touching the working directory every thread shares.
struct BareRelativeSave {
    name: PathBuf,
}

impl BareRelativeSave {
    fn unique(label: &str) -> Self {
        Self {
            name: PathBuf::from(format!(
                "pokeemerald-rs-save-file-{label}-{}-{:?}.sav",
                std::process::id(),
                std::thread::current().id()
            )),
        }
    }
}

impl Drop for BareRelativeSave {
    fn drop(&mut self) {
        drop(std::fs::remove_file(&self.name));
    }
}

#[test]
fn a_bare_relative_save_path_syncs_the_working_directory_after_the_rename() {
    let bare = BareRelativeSave::unique("write");
    let file = SaveFile::at(bare.name.clone());
    let (store, _, _) = saved_store();

    let synced = std::cell::RefCell::new(Vec::new());
    file.write_with(
        &store,
        |path| synced.borrow_mut().push(path.to_path_buf()),
        |bytes| SaveFile::stage(|| file.staging_path(), SaveFile::real_open, bytes),
        |_| {},
    )
    .expect("writing a bare relative save path must succeed");

    assert_eq!(
        synced.into_inner(),
        vec![PathBuf::from(".")],
        "a bare relative save path's directory entry lives in the working directory, and \
         the rename must best-effort sync it exactly once"
    );
}
