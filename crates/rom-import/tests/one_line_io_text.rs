//! The one-line contract against I/O error text and profile names.

use std::error::Error;
use std::io;
use std::path::PathBuf;

use rom_import::{ImportError, PartialFile};

#[test]
fn io_error_text_and_profile_name_stay_one_line() {
    let cases = [
        ImportError::ReadFailed {
            path: PathBuf::from("rom.gba"),
            source: io::Error::other("first\nsecond\u{1b}[2K"),
        },
        ImportError::WriteFailed {
            path: PathBuf::from("out.pack"),
            partial: PartialFile::Unreadable(io::Error::other("why\nnext")),
            source: io::Error::other("first\nsecond"),
        },
        ImportError::WriteFailed {
            path: PathBuf::from("out.pack"),
            partial: PartialFile::RemovalFailed(io::Error::other("why\u{1b}[2Knext\r")),
            source: io::Error::other("first"),
        },
        ImportError::ProfileHeaderMismatch {
            profile: "name\nnext\u{1b}[2K",
        },
    ];
    let bad: Vec<String> = cases
        .iter()
        .map(ToString::to_string)
        .filter(|text| text.contains(['\n', '\r', '\u{1b}']))
        .collect();
    assert!(
        bad.is_empty(),
        "multi-line or control-bearing diagnostics: {bad:#?}"
    );
}

#[test]
fn escaped_causes_stay_legible() {
    let read = ImportError::ReadFailed {
        path: PathBuf::from("rom.gba"),
        source: io::Error::other("first\nsecond\u{1b}[2K"),
    };
    assert_eq!(
        read.to_string(),
        r"could not read ROM `rom.gba`: first\nsecond\u{1b}[2K"
    );

    let write = ImportError::WriteFailed {
        path: PathBuf::from("out.pack"),
        partial: PartialFile::RemovalFailed(io::Error::other("why\nnext")),
        source: io::Error::other("first\tsecond"),
    };
    assert_eq!(
        write.to_string(),
        r"could not write `out.pack`: first\tsecond (a partial file remains there and could not be removed: why\nnext; retry with a fresh destination name)"
    );
}

#[test]
fn escaping_the_text_leaves_the_io_error_source_whole() {
    let read = ImportError::ReadFailed {
        path: PathBuf::from("rom.gba"),
        source: io::Error::other("first\nsecond"),
    };
    let source = read.source().expect("ReadFailed carries its cause");
    assert_eq!(source.to_string(), "first\nsecond");

    let write = ImportError::WriteFailed {
        path: PathBuf::from("out.pack"),
        partial: PartialFile::Gone,
        source: io::Error::from_raw_os_error(28),
    };
    let io_source = write
        .source()
        .and_then(|source| source.downcast_ref::<io::Error>())
        .expect("WriteFailed carries the io::Error itself");
    assert_eq!(io_source.raw_os_error(), Some(28));
}
