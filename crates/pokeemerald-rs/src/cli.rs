//! The shipped binary's command line (S-4, Discussion #71 policy C, issue
//! #122).
//!
//! Hand-rolled and std-only. `xtask`'s `Command` enum
//! (`crates/xtask/src/main.rs`) is the precedent: one [`parse`] function,
//! one concrete error enum, no `clap` `(minimal-deps)`. The surface is
//! deliberately tiny, because a player runs this binary with no arguments
//! at all; `--import-rom` is the one thing they type once.
//!
//! Parsing is pure: [`parse`] takes the arguments as a slice and returns a
//! [`Command`], so every branch is unit-tested without spawning a process.

use std::error::Error;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::PathBuf;

/// The `--import-rom` flag's long form.
const IMPORT_ROM: &str = "--import-rom";

/// The `--import-rom=<path>` inline form's prefix.
const IMPORT_ROM_EQ: &str = "--import-rom=";

/// Usage text: printed by `--help`, and appended to any [`CliError`].
pub const USAGE: &str = "\
usage: pokeemerald-rs [options]

With no options, plays the game from the installed asset pack.

options:
  --import-rom <path>  Read your own Pokemon Emerald (US) ROM, write the
                       asset pack, and exit. `--import-rom=<path>` works too.
  -h, --help           Print this message and exit.";

/// A parsed, validated invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// No options: run the game.
    Play,
    /// `--import-rom <path>`: import that ROM, then exit.
    ImportRom {
        /// The ROM image to read.
        path: PathBuf,
    },
    /// `--help` or `-h`: print [`USAGE`], then exit successfully.
    Help,
}

/// Why an invocation could not be parsed.
///
/// Concrete per-crate enum `(oop-boundaries)`; no `anyhow`. Every message
/// is one line, with [`USAGE`] appended, so a mistyped flag shows both what
/// was wrong and what was accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliError {
    /// `--import-rom` was given without a path, or with an empty one.
    MissingRomPath,
    /// An argument this binary does not accept. Carries the token.
    UnexpectedArg(String),
    /// `--import-rom` was given more than once. One run writes one pack, so
    /// a second path would silently lose to the first.
    DuplicateImportRom,
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingRomPath => {
                writeln!(f, "`{IMPORT_ROM}` requires a path to a ROM file")?;
            }
            Self::UnexpectedArg(arg) => {
                writeln!(f, "unexpected argument `{arg}`")?;
            }
            Self::DuplicateImportRom => {
                writeln!(f, "`{IMPORT_ROM}` was given more than once")?;
            }
        }
        write!(f, "{USAGE}")
    }
}

impl Error for CliError {}

/// Parse the post-program-name arguments into a [`Command`].
///
/// `args` is what [`std::env::args_os`] yields after the program name. An
/// empty slice is [`Command::Play`], the case every player hits. Paths are
/// bytes on Linux, so the ROM path stays an `OsStr`; only flag tokens need
/// to be UTF-8.
///
/// Arguments are read left to right. `--help` wins wherever it is read as
/// a flag; the token after `--import-rom` is always a path.
///
/// # Errors
///
/// Returns [`CliError::MissingRomPath`] when `--import-rom` carries no
/// path, [`CliError::DuplicateImportRom`] when it is repeated, and
/// [`CliError::UnexpectedArg`] for anything else.
pub fn parse(args: &[OsString]) -> Result<Command, CliError> {
    let mut rom: Option<PathBuf> = None;
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_os_str();
        let unexpected = || CliError::UnexpectedArg(arg.to_string_lossy().into_owned());
        match arg.to_str() {
            Some("--help" | "-h") => return Ok(Command::Help),
            Some(IMPORT_ROM) => {
                let value = args.get(index + 1).ok_or(CliError::MissingRomPath)?;
                rom = Some(take_rom_path(rom.as_ref(), value)?);
                index += 2;
            }
            Some(text) => {
                let value = text.strip_prefix(IMPORT_ROM_EQ).ok_or_else(unexpected)?;
                rom = Some(take_rom_path(rom.as_ref(), OsStr::new(value))?);
                index += 1;
            }
            None => return Err(unexpected()),
        }
    }
    Ok(rom.map_or(Command::Play, |path| Command::ImportRom { path }))
}

/// Accept one `--import-rom` value, rejecting a repeat or an empty path.
///
/// An empty path is a missing path, not a path to the current directory: a
/// shell that expanded an unset variable produced it, and importing "" can
/// only fail later with a worse message.
fn take_rom_path(seen: Option<&PathBuf>, value: &OsStr) -> Result<PathBuf, CliError> {
    if seen.is_some() {
        return Err(CliError::DuplicateImportRom);
    }
    if value.is_empty() {
        return Err(CliError::MissingRomPath);
    }
    Ok(PathBuf::from(value))
}

#[cfg(test)]
mod tests {
    use super::{parse, CliError, Command, USAGE};
    use std::ffi::OsString;
    use std::path::PathBuf;

    fn args(parts: &[&str]) -> Vec<OsString> {
        parts.iter().map(OsString::from).collect()
    }

    #[cfg(unix)]
    #[test]
    fn a_non_utf8_rom_path_is_accepted_and_a_non_utf8_flag_is_rejected() {
        use std::os::unix::ffi::OsStringExt;
        let path = OsString::from_vec(b"/roms/\xe9merald.gba".to_vec());
        let parsed = parse(&[OsString::from("--import-rom"), path.clone()]).unwrap();
        assert_eq!(
            parsed,
            Command::ImportRom {
                path: PathBuf::from(path)
            }
        );
        let flag = OsString::from_vec(b"--\xff".to_vec());
        assert!(matches!(
            parse(&[flag]).unwrap_err(),
            CliError::UnexpectedArg(_)
        ));
    }

    #[test]
    fn no_arguments_plays() {
        assert_eq!(parse(&[]).unwrap(), Command::Play);
    }

    #[test]
    fn separate_import_rom_value_parses() {
        assert_eq!(
            parse(&args(&["--import-rom", "/roms/emerald.gba"])).unwrap(),
            Command::ImportRom {
                path: PathBuf::from("/roms/emerald.gba")
            }
        );
    }

    #[test]
    fn inline_import_rom_value_parses() {
        assert_eq!(
            parse(&args(&["--import-rom=/roms/emerald.gba"])).unwrap(),
            Command::ImportRom {
                path: PathBuf::from("/roms/emerald.gba")
            }
        );
    }

    #[test]
    fn a_path_with_an_equals_sign_survives_the_inline_form() {
        // Only the first `=` separates flag from value, so a ROM in a
        // directory with an `=` in its name still imports.
        assert_eq!(
            parse(&args(&["--import-rom=/roms/a=b/emerald.gba"])).unwrap(),
            Command::ImportRom {
                path: PathBuf::from("/roms/a=b/emerald.gba")
            }
        );
    }

    #[test]
    fn both_help_spellings_parse() {
        assert_eq!(parse(&args(&["--help"])).unwrap(), Command::Help);
        assert_eq!(parse(&args(&["-h"])).unwrap(), Command::Help);
    }

    #[test]
    fn help_wins_over_a_later_import() {
        assert_eq!(
            parse(&args(&["--help", "--import-rom", "/roms/emerald.gba"])).unwrap(),
            Command::Help
        );
    }

    #[test]
    fn import_rom_without_a_value_is_rejected() {
        assert_eq!(
            parse(&args(&["--import-rom"])).unwrap_err(),
            CliError::MissingRomPath
        );
    }

    #[test]
    fn an_empty_rom_path_is_rejected_in_both_forms() {
        assert_eq!(
            parse(&args(&["--import-rom", ""])).unwrap_err(),
            CliError::MissingRomPath
        );
        assert_eq!(
            parse(&args(&["--import-rom="])).unwrap_err(),
            CliError::MissingRomPath
        );
    }

    #[test]
    fn a_repeated_import_rom_is_rejected() {
        assert_eq!(
            parse(&args(&["--import-rom", "/a.gba", "--import-rom", "/b.gba"])).unwrap_err(),
            CliError::DuplicateImportRom
        );
        assert_eq!(
            parse(&args(&["--import-rom=/a.gba", "--import-rom=/b.gba"])).unwrap_err(),
            CliError::DuplicateImportRom
        );
        // Mixing the two spellings is still a repeat.
        assert_eq!(
            parse(&args(&["--import-rom", "/a.gba", "--import-rom=/b.gba"])).unwrap_err(),
            CliError::DuplicateImportRom
        );
    }

    #[test]
    fn an_unknown_flag_is_rejected() {
        assert_eq!(
            parse(&args(&["--extract"])).unwrap_err(),
            CliError::UnexpectedArg("--extract".to_owned())
        );
    }

    #[test]
    fn a_bare_positional_argument_is_rejected() {
        // The ROM path needs its flag: a bare path is far more likely to be
        // a typo than an import request.
        assert_eq!(
            parse(&args(&["/roms/emerald.gba"])).unwrap_err(),
            CliError::UnexpectedArg("/roms/emerald.gba".to_owned())
        );
    }

    #[test]
    fn a_stray_argument_after_a_valid_import_is_rejected() {
        assert_eq!(
            parse(&args(&["--import-rom", "/a.gba", "extra"])).unwrap_err(),
            CliError::UnexpectedArg("extra".to_owned())
        );
    }

    #[test]
    fn every_error_appends_the_usage_text() {
        for err in [
            CliError::MissingRomPath,
            CliError::UnexpectedArg("--nope".to_owned()),
            CliError::DuplicateImportRom,
        ] {
            let rendered = err.to_string();
            assert!(rendered.ends_with(USAGE), "no usage tail: {rendered}");
            // One line of diagnosis, then the usage block.
            assert!(rendered.starts_with('`') || rendered.starts_with("unexpected"));
        }
    }

    #[test]
    fn the_usage_text_names_the_import_flag() {
        assert!(USAGE.contains("--import-rom <path>"));
        assert!(USAGE.contains("-h, --help"));
    }
}
