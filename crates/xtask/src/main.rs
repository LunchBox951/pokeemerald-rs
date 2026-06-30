//! `xtask` — the project's task-automation entry point (F-3).
//!
//! A hand-rolled dev runner (std only, no `clap`/`anyhow` — `minimal-deps`).
//! This is a SKELETON: every subcommand parses, dispatches, and reports that it
//! is not implemented yet. Real `extract` / `record-snapshot` / `scenario` /
//! `e2e` behaviour, and wiring `e2e` into CI (V-1), are out of scope here.
//!
//! Run via the workspace alias: `cargo xtask <subcommand>`.

use std::error::Error;
use std::fmt;
use std::process::ExitCode;

/// Usage text shown on stderr for any parse error.
const USAGE: &str = "\
usage: cargo xtask <command>

commands:
  extract            extract data/assets from the upstream reference
  record-snapshot    record a golden snapshot for regression tests
  scenario           run a scripted gameplay scenario
  e2e --suite <s>    run the end-to-end suite; <s> is smoke | full | soak";

/// Errors produced while parsing an `xtask` invocation.
///
/// Concrete per-crate enum (`oop-boundaries`); no `anyhow`.
#[derive(Debug)]
pub enum XtaskError {
    /// No subcommand, or an unrecognised one. Carries the offending input
    /// (empty string when no subcommand was given).
    UnknownCommand(String),
    /// `e2e` was given a `--suite` value that is not `smoke`, `full`, or `soak`.
    InvalidSuite(String),
    /// `e2e --suite` was present but no value followed it.
    MissingSuiteValue,
    /// A subcommand received an argument it does not accept. Carries the
    /// offending token.
    UnexpectedArg(String),
}

impl fmt::Display for XtaskError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownCommand(cmd) if cmd.is_empty() => {
                writeln!(f, "error: no subcommand given")?;
            }
            Self::UnknownCommand(cmd) => {
                writeln!(f, "error: unknown subcommand `{cmd}`")?;
            }
            Self::InvalidSuite(suite) => {
                writeln!(f, "error: unknown e2e suite `{suite}`")?;
            }
            Self::MissingSuiteValue => {
                writeln!(f, "error: `e2e --suite` requires a value")?;
            }
            Self::UnexpectedArg(arg) => {
                writeln!(f, "error: unexpected argument `{arg}`")?;
            }
        }
        write!(f, "{USAGE}")
    }
}

impl Error for XtaskError {}

/// The `e2e` test suite selected via `--suite`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Suite {
    /// Fast confidence check.
    Smoke,
    /// Full regression run.
    Full,
    /// Long-running stability run.
    Soak,
}

impl Suite {
    /// Parse a suite name.
    ///
    /// # Errors
    ///
    /// Returns [`XtaskError::InvalidSuite`] if `value` is not a known suite.
    pub fn parse(value: &str) -> Result<Self, XtaskError> {
        match value {
            "smoke" => Ok(Self::Smoke),
            "full" => Ok(Self::Full),
            "soak" => Ok(Self::Soak),
            other => Err(XtaskError::InvalidSuite(other.to_owned())),
        }
    }
}

/// A parsed, validated `xtask` invocation.
#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    /// `extract`
    Extract,
    /// `record-snapshot`
    RecordSnapshot,
    /// `scenario`
    Scenario,
    /// `e2e --suite <suite>`
    E2e(Suite),
}

/// Parse the post-program-name arguments into a [`Command`].
///
/// `args` is the slice *after* the program name (i.e. what `cargo xtask`
/// forwards). The first element is the subcommand.
///
/// # Errors
///
/// Returns [`XtaskError::UnknownCommand`] for an empty or unrecognised
/// subcommand, [`XtaskError::UnexpectedArg`] when a subcommand that takes no
/// arguments is given one (or `e2e` is given a stray token),
/// [`XtaskError::MissingSuiteValue`] if `e2e --suite` has no value, and
/// [`XtaskError::InvalidSuite`] for an unknown suite name.
pub fn parse(args: &[String]) -> Result<Command, XtaskError> {
    let Some(subcommand) = args.first() else {
        return Err(XtaskError::UnknownCommand(String::new()));
    };
    let rest = &args[1..];

    match subcommand.as_str() {
        "extract" => no_args(rest).map(|()| Command::Extract),
        "record-snapshot" => no_args(rest).map(|()| Command::RecordSnapshot),
        "scenario" => no_args(rest).map(|()| Command::Scenario),
        "e2e" => parse_e2e(rest).map(Command::E2e),
        other => Err(XtaskError::UnknownCommand(other.to_owned())),
    }
}

/// Reject any trailing arguments for a subcommand that accepts none.
///
/// # Errors
///
/// Returns [`XtaskError::UnexpectedArg`] carrying the first stray token.
fn no_args(rest: &[String]) -> Result<(), XtaskError> {
    match rest.first() {
        None => Ok(()),
        Some(arg) => Err(XtaskError::UnexpectedArg(arg.clone())),
    }
}

/// Parse the arguments following the `e2e` subcommand: `--suite <value>`.
///
/// Requires the exact form `--suite <value>` with no leading or trailing
/// tokens; anything else is an [`XtaskError::UnexpectedArg`].
fn parse_e2e(rest: &[String]) -> Result<Suite, XtaskError> {
    let Some(flag) = rest.first() else {
        return Err(XtaskError::MissingSuiteValue);
    };
    if flag != "--suite" {
        return Err(XtaskError::UnexpectedArg(flag.clone()));
    }
    let value = rest.get(1).ok_or(XtaskError::MissingSuiteValue)?;
    if let Some(extra) = rest.get(2) {
        return Err(XtaskError::UnexpectedArg(extra.clone()));
    }
    Suite::parse(value)
}

/// Dispatch a parsed command, printing its (not-yet-implemented) status.
fn dispatch(cmd: &Command) {
    match cmd {
        Command::Extract => println!("xtask extract: not implemented yet"),
        Command::RecordSnapshot => {
            println!("xtask record-snapshot: not implemented yet");
        }
        Command::Scenario => println!("xtask scenario: not implemented yet"),
        Command::E2e(suite) => {
            println!("xtask e2e ({suite:?}): not implemented yet");
        }
    }
}

/// Parse and dispatch a single invocation.
///
/// Kept separate from `main` so tests can drive it without spawning a process.
///
/// # Errors
///
/// Propagates any [`XtaskError`] from [`parse`].
pub fn run(args: &[String]) -> Result<(), XtaskError> {
    let cmd = parse(args)?;
    dispatch(&cmd);
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{parse, run, Command, Suite, XtaskError};

    fn args(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn parse_extract_routes() {
        assert_eq!(parse(&args(&["extract"])).unwrap(), Command::Extract);
    }

    #[test]
    fn parse_record_snapshot_routes() {
        assert_eq!(
            parse(&args(&["record-snapshot"])).unwrap(),
            Command::RecordSnapshot
        );
    }

    #[test]
    fn parse_scenario_routes() {
        assert_eq!(parse(&args(&["scenario"])).unwrap(), Command::Scenario);
    }

    #[test]
    fn parse_e2e_smoke() {
        assert_eq!(
            parse(&args(&["e2e", "--suite", "smoke"])).unwrap(),
            Command::E2e(Suite::Smoke)
        );
    }

    #[test]
    fn parse_e2e_full() {
        assert_eq!(
            parse(&args(&["e2e", "--suite", "full"])).unwrap(),
            Command::E2e(Suite::Full)
        );
    }

    #[test]
    fn parse_e2e_soak() {
        assert_eq!(
            parse(&args(&["e2e", "--suite", "soak"])).unwrap(),
            Command::E2e(Suite::Soak)
        );
    }

    #[test]
    fn parse_e2e_invalid_suite() {
        let err = parse(&args(&["e2e", "--suite", "bogus"])).unwrap_err();
        assert!(matches!(err, XtaskError::InvalidSuite(s) if s == "bogus"));
    }

    #[test]
    fn parse_e2e_missing_suite_value() {
        let err = parse(&args(&["e2e", "--suite"])).unwrap_err();
        assert!(matches!(err, XtaskError::MissingSuiteValue));
    }

    #[test]
    fn parse_e2e_requires_suite_flag() {
        let err = parse(&args(&["e2e"])).unwrap_err();
        assert!(matches!(err, XtaskError::MissingSuiteValue));
    }

    #[test]
    fn parse_rejects_trailing_args() {
        let err = parse(&args(&["extract", "foo"])).unwrap_err();
        assert!(matches!(err, XtaskError::UnexpectedArg(s) if s == "foo"));
        let err = parse(&args(&["scenario", "x", "y"])).unwrap_err();
        assert!(matches!(err, XtaskError::UnexpectedArg(s) if s == "x"));
    }

    #[test]
    fn parse_e2e_rejects_leading_token() {
        let err = parse(&args(&["e2e", "junk", "--suite", "smoke"])).unwrap_err();
        assert!(matches!(err, XtaskError::UnexpectedArg(s) if s == "junk"));
    }

    #[test]
    fn parse_e2e_rejects_trailing_token() {
        let err = parse(&args(&["e2e", "--suite", "smoke", "extra"])).unwrap_err();
        assert!(matches!(err, XtaskError::UnexpectedArg(s) if s == "extra"));
    }

    #[test]
    fn parse_unknown_command() {
        let err = parse(&args(&["frobnicate"])).unwrap_err();
        assert!(matches!(err, XtaskError::UnknownCommand(s) if s == "frobnicate"));
    }

    #[test]
    fn parse_empty() {
        let err = parse(&args(&[])).unwrap_err();
        assert!(matches!(err, XtaskError::UnknownCommand(s) if s.is_empty()));
    }

    #[test]
    fn suite_parse_roundtrip() {
        assert_eq!(Suite::parse("smoke").unwrap(), Suite::Smoke);
        assert_eq!(Suite::parse("full").unwrap(), Suite::Full);
        assert_eq!(Suite::parse("soak").unwrap(), Suite::Soak);
        assert!(matches!(
            Suite::parse("nope").unwrap_err(),
            XtaskError::InvalidSuite(_)
        ));
    }

    #[test]
    fn run_ok_for_recognised() {
        assert!(run(&args(&["extract"])).is_ok());
        assert!(run(&args(&["e2e", "--suite", "smoke"])).is_ok());
    }

    #[test]
    fn run_err_for_unknown() {
        assert!(run(&args(&["frobnicate"])).is_err());
    }

    #[test]
    fn display_includes_usage() {
        let rendered = XtaskError::UnknownCommand("x".to_owned()).to_string();
        assert!(rendered.contains("usage: cargo xtask"));
        assert!(rendered.contains("extract"));
        assert!(rendered.contains("e2e --suite"));
    }
}
