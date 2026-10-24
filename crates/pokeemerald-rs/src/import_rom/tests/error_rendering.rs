//! How `ImportRomError` and related diagnostics render for the user.

#[cfg(not(unix))]
use std::fs;
use std::path::PathBuf;

use rom_import::ImportError;

use super::super::dest::not_a_directory_error;
#[cfg(not(unix))]
use super::super::dest::Dest;
use super::super::{ImportOutcome, ImportRomError};
#[cfg(not(unix))]
use super::support::TempDir;

#[test]
fn the_import_error_renders_the_importers_own_message() {
    let err = ImportRomError::Import {
        source: ImportError::EmptyPack,
        temp_path: None,
        temp_removed: true,
    };
    assert_eq!(err.to_string(), ImportError::EmptyPack.to_string());
    assert!(err.to_string().contains("no pack was written"));
}

#[test]
fn the_outcome_renders_the_one_line_summary() {
    let outcome = ImportOutcome {
        pack_path: PathBuf::from("/data/pokeemerald.pack"),
        entry_count: 1234,
        pack_bytes: 5678,
    };
    assert_eq!(
        outcome.to_string(),
        "imported 1234 entries (5678 bytes) to /data/pokeemerald.pack"
    );
}

/// Every message here is one terminal row (module docs on
/// [`ImportRomError`]), and the paths they quote are user-supplied:
/// `--import-rom`'s argument and `$POKEEMERALD_PACK`. Control bytes in
/// either must be escaped before interpolation, which is what routing them
/// through `OneLinePath` (`crates/rom-import/src/one_line.rs`) buys.
#[test]
fn a_destination_carrying_control_bytes_stays_one_printable_row() {
    let rendered = ImportRomError::DestinationIsDirectory {
        pack_path: PathBuf::from("/data/\npokeemerald.pack\u{1b}]0;x\u{7}"),
    }
    .to_string();
    assert!(
        !rendered.chars().any(char::is_control),
        "the diagnosis must stay one printable row: {rendered:?}"
    );

    let summary = ImportOutcome {
        pack_path: PathBuf::from("/data/\npokeemerald.pack"),
        entry_count: 1,
        pack_bytes: 2,
    }
    .to_string();
    assert!(
        !summary.chars().any(char::is_control),
        "the summary must stay one printable row: {summary:?}"
    );
}

/// Every path-bearing [`ImportRomError`] variant escapes control bytes the
/// same way as [`a_destination_carrying_control_bytes_stays_one_printable_row`]
/// above, not just [`ImportRomError::DestinationIsDirectory`]: a newline or
/// an ESC reaching any of them from `$POKEEMERALD_PACK` or the
/// `--import-rom` argument must not reach the terminal, and the escaped
/// spelling must still be legible rather than silently dropped.
#[test]
fn every_path_bearing_variant_escapes_control_bytes_and_stays_legible() {
    let hostile = PathBuf::from("one\ntwo\u{1b}[2Kthree");
    let escaped = r"one\ntwo\u{1b}[2Kthree";
    let cases = [
        ImportRomError::CreateDirFailed {
            path: hostile.clone(),
            source: std::io::Error::from(std::io::ErrorKind::PermissionDenied),
        },
        ImportRomError::OpenDirFailed {
            path: hostile.clone(),
            source: std::io::Error::from(std::io::ErrorKind::PermissionDenied),
        },
        ImportRomError::Import {
            source: ImportError::EmptyPack,
            temp_path: Some(hostile.clone()),
            temp_removed: false,
        },
        ImportRomError::DestinationNamesNoFile {
            pack_path: hostile.clone(),
        },
        ImportRomError::DestinationIsDirectory {
            pack_path: hostile.clone(),
        },
        ImportRomError::DestinationIsSource {
            rom_path: hostile.clone(),
        },
        ImportRomError::TempFileFailed {
            temp_path: hostile.clone(),
            source: std::io::Error::from(std::io::ErrorKind::PermissionDenied),
            temp_removed: false,
        },
        ImportRomError::PublishFailed {
            temp_path: hostile.clone(),
            pack_path: PathBuf::from("/data/pokeemerald.pack"),
            source: std::io::Error::from(std::io::ErrorKind::PermissionDenied),
            temp_removed: true,
        },
        ImportRomError::PublishFailed {
            temp_path: PathBuf::from("/data/temp.tmp"),
            pack_path: hostile.clone(),
            source: std::io::Error::from(std::io::ErrorKind::PermissionDenied),
            temp_removed: false,
        },
    ];
    for case in cases {
        let text = case.to_string();
        assert!(!text.contains('\n'), "{text:?}");
        assert!(!text.contains('\u{1b}'), "{text:?}");
        assert!(text.contains(escaped), "escaped path missing from {text:?}");
    }
}

/// A hostile destination appears exactly once, escaped, in
/// [`ImportRomError::OpenDirFailed`]'s rendering of
/// [`not_a_directory_error`].
#[test]
fn a_non_directory_open_error_names_the_destination_once() {
    let hostile = PathBuf::from("one\ntwo\u{1b}[2Kthree");
    let rendered = ImportRomError::OpenDirFailed {
        path: hostile,
        source: not_a_directory_error(),
    }
    .to_string();
    assert_eq!(
        rendered,
        r"could not open `one\ntwo\u{1b}[2Kthree`: not a directory"
    );
    assert!(
        !rendered.chars().any(char::is_control),
        "the diagnosis must stay one printable row: {rendered:?}"
    );
}

/// The non-Unix [`Dest::open`] itself keeps `dir` out of the failure it
/// reports, not just [`not_a_directory_error`] built independently.
#[cfg(not(unix))]
#[test]
fn the_non_unix_open_of_a_non_directory_does_not_name_it() {
    let dir = TempDir::new("open-non-directory");
    let path = dir.join("pokeemerald.pack");
    fs::write(&path, b"a file, not a directory").expect("a file to open");

    let source = Dest::open(&path).err().expect("a file is not a directory");

    assert_eq!(source.to_string(), not_a_directory_error().to_string());
    assert!(
        !source.to_string().contains("pokeemerald"),
        "the inner error must not carry the path: {source}"
    );
}

/// An ordinary path -- a Windows one included, whose separators are
/// backslashes and whose player might have quoted a space in it -- still
/// prints as the literal string a player can copy back into a shell:
/// `OneLinePath` does not double `\` or `"`.
#[test]
fn an_ordinary_windows_path_prints_the_literal_a_player_can_copy_back() {
    let windows = PathBuf::from(r#"C:\Users\me\my "roms"\pokeemerald.pack"#);
    let rendered = ImportRomError::DestinationIsDirectory { pack_path: windows }.to_string();
    assert!(
        rendered.contains(r#"C:\Users\me\my "roms"\pokeemerald.pack"#),
        "{rendered}"
    );
}

/// The line separators `U+2028`/`U+2029` and the `Bidi_Control`
/// characters are outside `char::is_control`, so the control-byte tests
/// above cannot see them, in the success summary or the errors.
#[test]
fn a_separator_or_bidi_control_in_a_path_is_escaped_too() {
    let hostile = PathBuf::from(
        "a\u{2028}b\u{2029}c\u{202e}d\u{202a}e\u{202b}f\u{202c}g\u{202d}h\u{2066}i\u{2067}j\u{2068}k\u{2069}l\u{200e}m\u{200f}n\u{61c}o",
    );
    let escaped = r"a\u{2028}b\u{2029}c\u{202e}d\u{202a}e\u{202b}f\u{202c}g\u{202d}h\u{2066}i\u{2067}j\u{2068}k\u{2069}l\u{200e}m\u{200f}n\u{61c}o";
    let rendered = ImportRomError::DestinationIsDirectory {
        pack_path: hostile.clone(),
    }
    .to_string();
    assert!(rendered.contains(escaped), "{rendered:?}");
    let summary = ImportOutcome {
        pack_path: hostile,
        entry_count: 1,
        pack_bytes: 2,
    }
    .to_string();
    assert!(summary.contains(escaped), "{summary:?}");
}

#[test]
fn a_refused_directory_destination_says_which_variable_to_change() {
    let rendered = ImportRomError::DestinationIsDirectory {
        pack_path: PathBuf::from("/data/pokeemerald.pack"),
    }
    .to_string();
    assert!(rendered.contains("/data/pokeemerald.pack"), "{rendered}");
    assert!(rendered.contains(pack_format::PACK_PATH_ENV), "{rendered}");
    assert!(!rendered.contains('\n'), "{rendered}");
}

#[test]
fn a_refused_no_file_destination_says_which_variable_to_change() {
    let rendered = ImportRomError::DestinationNamesNoFile {
        pack_path: PathBuf::from("/data/.."),
    }
    .to_string();
    assert!(rendered.contains("/data/.."), "{rendered}");
    assert!(rendered.contains(pack_format::PACK_PATH_ENV), "{rendered}");
    assert!(!rendered.contains('\n'), "{rendered}");
}

#[test]
fn a_refused_same_file_destination_says_which_variable_to_change() {
    let rendered = ImportRomError::DestinationIsSource {
        rom_path: PathBuf::from("/roms/emerald.gba"),
    }
    .to_string();
    assert!(rendered.contains("/roms/emerald.gba"), "{rendered}");
    assert!(rendered.contains(pack_format::PACK_PATH_ENV), "{rendered}");
    assert!(!rendered.contains('\n'), "{rendered}");
}

#[test]
fn a_missing_destination_says_which_variable_to_set() {
    let rendered = ImportRomError::NoDestination.to_string();
    assert!(rendered.contains(pack_format::PACK_PATH_ENV), "{rendered}");
}
