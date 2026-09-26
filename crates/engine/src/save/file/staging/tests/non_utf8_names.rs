#[cfg(unix)]
use std::path::Path;

#[cfg(unix)]
use super::super::create_new_exclusive;
#[cfg(unix)]
use super::shared::area;
#[cfg(unix)]
use crate::save::file::tests::{saved_store, TempDir};
#[cfg(unix)]
use crate::save::file::SaveFile;
#[cfg(unix)]
use crate::save::store::FLASH_IMAGE_LEN;

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

    let staged_path = staged.path.clone();
    drop(staged);
    std::fs::remove_file(staged_path).unwrap();
}
