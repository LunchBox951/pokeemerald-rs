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

#[cfg(unix)]
const INVALID_UTF8_BYTE: u8 = 0xFF;

/// APFS and HFS+ reject non-UTF-8 names with `EILSEQ`; probe the rename that
/// publishes the save rather than assuming support from `target_os`.
#[cfg(unix)]
fn host_accepts_a_non_utf8_filename(dir: &Path) -> bool {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt as _;

    let valid = dir.join("utf8-name-probe");
    if std::fs::write(&valid, b"probe").is_err() {
        return false;
    }
    let raw = dir.join(OsString::from_vec(vec![b'p', INVALID_UTF8_BYTE, b'e']));
    let accepted = std::fs::rename(&valid, &raw).is_ok();
    drop(std::fs::remove_file(if accepted { &raw } else { &valid }));
    accepted
}

#[cfg(unix)]
#[test]
fn a_non_utf8_basename_stages_and_writes_in_the_same_directory() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt as _;

    let dir = TempDir::new("non-utf8-basename");
    if !host_accepts_a_non_utf8_filename(&dir.path) {
        return;
    }
    let path = dir.path.join(OsString::from_vec(vec![
        b's',
        b'a',
        INVALID_UTF8_BYTE,
        b'v',
    ]));
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

/// A raw basename can fit a component limit while its lossy staging name
/// exceeds it; shrinking must still satisfy the limit.
#[cfg(unix)]
#[test]
fn a_non_utf8_basename_whose_lossy_form_is_longer_still_respects_an_injected_limit() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt as _;

    const RAW_BASENAME_LEN: usize = 5;
    const WIDEST_SUFFIX_LEN: usize = 15;
    const LOSSY_STEM_LEN: usize = RAW_BASENAME_LEN * '\u{FFFD}'.len_utf8();
    const INJECTED_LIMIT: usize = RAW_BASENAME_LEN + WIDEST_SUFFIX_LEN;

    let raw_basename = vec![INVALID_UTF8_BYTE; RAW_BASENAME_LEN];
    assert!(std::str::from_utf8(&raw_basename).is_err());
    assert!(raw_basename.len() + WIDEST_SUFFIX_LEN <= INJECTED_LIMIT);

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
        final_component_len < raw_basename.len() + LOSSY_STEM_LEN + WIDEST_SUFFIX_LEN,
        "the stem must actually have been shortened from the lossy first-guess \
         candidate, not merely have succeeded by chance: {final_component_len} bytes"
    );

    let staged_path = staged.path.clone();
    drop(staged);
    std::fs::remove_file(staged_path).unwrap();
}
