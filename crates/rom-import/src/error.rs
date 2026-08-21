//! The importer's error type.
//!
//! A concrete per-crate enum `(oop-boundaries)`, no `anyhow` in a library
//! crate. Every message is a single user-facing line: the importer runs from
//! a CLI a player invokes on their own ROM, so "what went wrong and what do I
//! do now" has to survive being printed on one terminal row.
//!
//! Messages never echo ROM content. The only ROM-derived bytes any variant
//! renders are the 12-byte ASCII header title and the 4-byte game code, both
//! cartridge identity rather than game data.

use std::fmt;
use std::io;
use std::path::PathBuf;

use crate::profile::EMERALD_US_REV0;
use crate::sha1::Digest;

/// Why a ROM's GBA cartridge header was rejected.
///
/// No variant carries bytes. A fault means the file is not a GBA cartridge
/// image at all, and naming the offending value adds nothing a user can act
/// on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderFault {
    /// The file is shorter than the 192-byte cartridge header.
    Truncated,
    /// The fixed `0x96` byte at `0xB2` is something else.
    FixedByte,
    /// The complement check over `0xA0..=0xBC` disagrees with the byte at
    /// `0xBD`.
    Checksum,
    /// The 12-byte title is not printable ASCII.
    NonAsciiTitle,
    /// The 4-byte game code is not printable ASCII.
    NonAsciiGameCode,
}

impl fmt::Display for HeaderFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::Truncated => "the file is shorter than a cartridge header",
            Self::FixedByte => "the fixed header byte at 0xB2 is wrong",
            Self::Checksum => "the header checksum does not match",
            Self::NonAsciiTitle => "the header title is not ASCII",
            Self::NonAsciiGameCode => "the header game code is not ASCII",
        };
        f.write_str(text)
    }
}

/// Why an LZ77 stream could not be decompressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lz77Fault {
    /// The first byte is not the BIOS type `0x10`.
    WrongType,
    /// A flag, back-reference, or literal byte was needed past the end of
    /// the input.
    UnexpectedEnd,
    /// A back-reference pointed further back than the output produced so
    /// far. Carries the displacement and the bytes produced.
    BackReferenceTooFar {
        /// The back-reference displacement, in bytes.
        displacement: usize,
        /// How many bytes had been produced when it was hit.
        produced: usize,
    },
    /// The stream decoded to a different length than the caller expected.
    SizeMismatch {
        /// The length the caller asked for.
        expected: usize,
        /// The length the stream produced.
        actual: usize,
    },
}

impl fmt::Display for Lz77Fault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongType => f.write_str("not an LZ77 type 0x10 stream"),
            Self::UnexpectedEnd => f.write_str("the stream ends mid-token"),
            Self::BackReferenceTooFar {
                displacement,
                produced,
            } => write!(
                f,
                "a back-reference reaches {displacement} bytes back with only {produced} produced"
            ),
            Self::SizeMismatch { expected, actual } => {
                write!(f, "decoded {actual} bytes, expected {expected}")
            }
        }
    }
}

/// Anything that can stop a ROM import.
#[derive(Debug)]
pub enum ImportError {
    /// The ROM file could not be opened or read.
    ReadFailed {
        /// The path the importer tried to read.
        path: PathBuf,
        /// The underlying I/O failure.
        source: io::Error,
    },
    /// The file is not exactly [`ROM_SIZE`](crate::ROM_SIZE) bytes.
    WrongSize {
        /// The size seen, in bytes.
        actual: u64,
    },
    /// The file is the right size but carries no valid cartridge header.
    BadHeader(HeaderFault),
    /// The ROM is a GBA cartridge, but not one this importer knows.
    UnsupportedRevision {
        /// The whole-file SHA-1.
        sha1: Digest,
        /// The header's 4-byte game code.
        game_code: [u8; 4],
        /// The header's version byte.
        version: u8,
    },
    /// A profile matched by hash, but the ROM's header disagreed with it.
    ///
    /// A SHA-1 collision is not a serious possibility here; in practice this
    /// means the profile table itself is wrong.
    ProfileHeaderMismatch {
        /// The profile's name.
        profile: &'static str,
    },
    /// A pointer read out of the ROM does not address the cartridge window.
    PointerOutOfRange {
        /// The ROM offset the pointer was read from.
        at: usize,
        /// The raw 32-bit value.
        raw: u32,
    },
    /// A read ran past the end of the ROM.
    Truncated {
        /// The ROM offset the read started at.
        at: usize,
        /// How many bytes were wanted.
        len: usize,
    },
    /// A table index was past the table's end.
    IndexOutOfRange {
        /// The table's name, for the message.
        table: &'static str,
        /// The index asked for.
        index: usize,
        /// How many entries the table has.
        count: usize,
    },
    /// An LZ77 stream in the ROM could not be decompressed.
    Lz77 {
        /// The ROM offset the stream starts at.
        at: usize,
        /// What went wrong.
        fault: Lz77Fault,
    },
    /// A decoded length was past what the format allows.
    Length {
        /// What was being measured, for the message.
        what: &'static str,
        /// The value decoded.
        value: usize,
        /// The largest value the format allows.
        max: usize,
    },
    /// The asset pack could not be assembled.
    PackWrite(pack_format::PackWriteError),
    /// The asset pack could not be written to disk.
    WriteFailed {
        /// The path the importer tried to write.
        path: PathBuf,
        /// The underlying I/O failure.
        source: io::Error,
    },
    /// The import ran but this build has no domain readers, so there is
    /// nothing to put in a pack.
    ///
    /// The importer fails *closed* rather than writing an empty pack: a
    /// zero-entry pack would look like a successful import to every
    /// downstream check `(gated-by-default)` `(test-ratchet)`.
    NoDomains,
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReadFailed { path, source } => {
                write!(f, "could not read ROM `{}`: {source}", path.display())
            }
            Self::WrongSize { actual } => write!(
                f,
                "ROM is {actual} bytes; a Pokemon Emerald ROM is exactly {} bytes",
                crate::ROM_SIZE
            ),
            Self::BadHeader(fault) => write!(f, "not a GBA ROM: {fault}"),
            Self::UnsupportedRevision {
                sha1,
                game_code,
                version,
            } => {
                f.write_str("unsupported ROM (game code ")?;
                write_ascii(f, game_code)?;
                write!(
                    f,
                    ", version {version}, SHA-1 {sha1}); the only supported ROM is {} with SHA-1 {}",
                    EMERALD_US_REV0.name, EMERALD_US_REV0.sha1
                )
            }
            Self::ProfileHeaderMismatch { profile } => write!(
                f,
                "ROM matches profile `{profile}` by SHA-1 but its header game code or version disagrees"
            ),
            Self::PointerOutOfRange { at, raw } => write!(
                f,
                "pointer {raw:#010x} at ROM offset {at:#08x} is outside the cartridge window"
            ),
            Self::Truncated { at, len } => write!(
                f,
                "a {len}-byte read at ROM offset {at:#08x} runs past the end of the ROM"
            ),
            Self::IndexOutOfRange {
                table,
                index,
                count,
            } => write!(
                f,
                "index {index} is past the end of table `{table}` ({count} entries)"
            ),
            Self::Lz77 { at, fault } => {
                write!(f, "LZ77 stream at ROM offset {at:#08x}: {fault}")
            }
            Self::Length { what, value, max } => {
                write!(f, "{what} is {value}, over the maximum of {max}")
            }
            Self::PackWrite(source) => write!(f, "could not assemble the asset pack: {source}"),
            Self::WriteFailed { path, source } => {
                write!(f, "could not write `{}`: {source}", path.display())
            }
            Self::NoDomains => f.write_str(
                "ROM import is not implemented yet: this build has no domain readers, so no pack was written",
            ),
        }
    }
}

impl std::error::Error for ImportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ReadFailed { source, .. } | Self::WriteFailed { source, .. } => Some(source),
            Self::PackWrite(source) => Some(source),
            _ => None,
        }
    }
}

impl From<pack_format::PackWriteError> for ImportError {
    fn from(source: pack_format::PackWriteError) -> Self {
        Self::PackWrite(source)
    }
}

/// Render a header ASCII field, substituting `?` for anything unprintable.
///
/// Header fields are validated as printable ASCII before a
/// [`ImportError::UnsupportedRevision`] is built, so the substitution is
/// belt-and-braces: it keeps a malformed byte from reaching the terminal as
/// a control character.
fn write_ascii(f: &mut fmt::Formatter<'_>, bytes: &[u8]) -> fmt::Result {
    for &byte in bytes {
        let c = if byte.is_ascii_graphic() || byte == b' ' {
            char::from(byte)
        } else {
            '?'
        };
        f.write_str(c.encode_utf8(&mut [0u8; 4]))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{HeaderFault, ImportError, Lz77Fault};

    #[test]
    fn unsupported_revision_names_the_one_supported_rom() {
        let err = ImportError::UnsupportedRevision {
            sha1: crate::Digest::from_hex("0123456789abcdef0123456789abcdef01234567"),
            game_code: *b"BPRE",
            version: 1,
        };
        let text = err.to_string();
        assert_eq!(
            text,
            "unsupported ROM (game code BPRE, version 1, SHA-1 \
             0123456789abcdef0123456789abcdef01234567); the only supported ROM is \
             Pokemon Emerald (US) rev 0 with SHA-1 \
             f3ae088181bf583e55daf962a92bb46f4f1d07b7"
        );
        assert!(!text.contains('\n'), "messages stay on one line");
    }

    #[test]
    fn unprintable_game_code_is_masked() {
        let err = ImportError::UnsupportedRevision {
            sha1: crate::Digest::from_bytes([0; 20]),
            game_code: [0x00, 0x1b, b'E', b'E'],
            version: 0,
        };
        assert!(err.to_string().contains("game code ??EE"));
    }

    #[test]
    fn messages_are_single_line() {
        let cases = [
            ImportError::WrongSize { actual: 12 },
            ImportError::BadHeader(HeaderFault::Checksum),
            ImportError::ProfileHeaderMismatch { profile: "x" },
            ImportError::PointerOutOfRange {
                at: 0x10,
                raw: 0x0000_0000,
            },
            ImportError::Truncated {
                at: 0x00ff_fff0,
                len: 64,
            },
            ImportError::IndexOutOfRange {
                table: "species",
                index: 9,
                count: 4,
            },
            ImportError::Lz77 {
                at: 0x20,
                fault: Lz77Fault::WrongType,
            },
            ImportError::Length {
                what: "sprite size",
                value: 9,
                max: 8,
            },
            ImportError::PackWrite(pack_format::PackWriteError::DuplicateId("a".into())),
            ImportError::NoDomains,
        ];
        for case in cases {
            let text = case.to_string();
            assert!(!text.is_empty());
            assert!(!text.contains('\n'), "{text}");
        }
    }

    #[test]
    fn wrong_size_names_the_expected_size() {
        assert_eq!(
            ImportError::WrongSize { actual: 1 }.to_string(),
            "ROM is 1 bytes; a Pokemon Emerald ROM is exactly 16777216 bytes"
        );
    }

    #[test]
    fn faults_render() {
        assert_eq!(
            HeaderFault::FixedByte.to_string(),
            "the fixed header byte at 0xB2 is wrong"
        );
        assert_eq!(
            Lz77Fault::BackReferenceTooFar {
                displacement: 4,
                produced: 2
            }
            .to_string(),
            "a back-reference reaches 4 bytes back with only 2 produced"
        );
        assert_eq!(
            Lz77Fault::SizeMismatch {
                expected: 4,
                actual: 5
            }
            .to_string(),
            "decoded 5 bytes, expected 4"
        );
    }

    #[test]
    fn pack_write_errors_convert() {
        let err: ImportError = pack_format::PackWriteError::InvalidId(String::new()).into();
        assert!(matches!(err, ImportError::PackWrite(_)));
    }
}
