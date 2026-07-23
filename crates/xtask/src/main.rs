//! `xtask` — the project's task-automation entry point (F-3).
//!
//! A hand-rolled dev runner (std only, no `clap`/`anyhow` for the CLI itself
//! — `minimal-deps`; `crate::e2e`'s only dependency is the workspace-local
//! `pokeemerald-rs` crate under test, and it is optional — see below). Every
//! subcommand parses and dispatches; a recognised-but-unimplemented
//! subcommand fails *closed* — returning [`XtaskError::NotImplemented`]
//! (non-zero exit) rather than exiting 0 — so the `RELEASE.md` gate commands
//! (`e2e --suite …`) can never be satisfied by a no-op stub
//! `(gated-by-default)`.
//!
//! `e2e --suite smoke` (F-3, V-1) is real: see [`crate::e2e::run_smoke`] for
//! the headless boot-shell run it drives. `extract` / `record-snapshot` /
//! `scenario`, and the `e2e` `full`/`soak` suites, remain stubs.
//!
//! `mod e2e` and its `pokeemerald-rs` dependency are gated behind the
//! `smoke` cargo feature: a default `cargo build -p xtask` (every other
//! subcommand) stays dependency-free, matching pre-PR `xtask`. Without the
//! feature, `e2e --suite smoke` still fails *closed* — [`XtaskError::SmokeUnavailable`],
//! not a silent no-op — telling the caller to rebuild with `--features
//! smoke`.
//!
//! Run via the workspace alias: `cargo xtask <subcommand>` (add `--features
//! smoke` for `e2e --suite smoke`, e.g.
//! `cargo run -p xtask --features smoke -- e2e --suite smoke`).

use std::error::Error;
use std::fmt;
use std::process::ExitCode;

#[cfg(feature = "smoke")]
mod e2e;

/// Usage text shown on stderr for any parse error.
const USAGE: &str = "\
usage: cargo xtask <command>

commands:
  extract            extract data/assets from the upstream reference
  record-snapshot    record a golden snapshot for regression tests
  scenario           run a scripted gameplay scenario
  e2e --suite <s> [--release]
                     run the end-to-end suite; <s> is smoke | full | soak";

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
    /// A recognised subcommand whose behaviour is not implemented yet. Carries
    /// the command's name. Returned so stub subcommands fail *closed* (non-zero
    /// exit) rather than silently reporting success — the `RELEASE.md` gate
    /// commands must never be satisfiable by a no-op `(gated-by-default)`
    /// `(test-ratchet)`.
    NotImplemented(&'static str),
    /// `e2e --suite smoke` ran but did not report a clean boot. Carries
    /// [`e2e::E2eError`]'s rendered message.
    SmokeFailed(String),
    /// `e2e --suite smoke` was requested, but this `xtask` binary was built
    /// without the `smoke` feature, so `mod e2e` (and its `pokeemerald-rs`
    /// dependency) was not compiled in. Fails *closed* rather than silently
    /// reporting success `(gated-by-default)` `(test-ratchet)`.
    SmokeUnavailable,
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
            // A not-implemented status is a runtime failure, not a usage error,
            // so it gets no USAGE tail.
            Self::NotImplemented(what) => {
                return write!(f, "error: `{what}` is not implemented yet");
            }
            // Likewise a smoke-run failure: it's a runtime/behavioural
            // failure, not a malformed invocation.
            Self::SmokeFailed(reason) => {
                return write!(f, "error: `e2e --suite smoke` failed: {reason}");
            }
            // Likewise: a missing feature is a build-configuration problem,
            // not a malformed invocation, so it gets no USAGE tail either.
            Self::SmokeUnavailable => {
                return write!(
                    f,
                    "error: `e2e --suite smoke` requires the `smoke` feature: rebuild with \
                     `cargo run -p xtask --features smoke -- e2e --suite smoke`"
                );
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
    /// `e2e --suite <suite> [--release]`
    E2e {
        /// The selected test suite.
        suite: Suite,
        /// Whether `--release` was requested (release-mode gates: V-2/V-3).
        release: bool,
    },
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
/// arguments is given one (or `e2e` is given a stray/duplicate token),
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
        "e2e" => parse_e2e(rest).map(|(suite, release)| Command::E2e { suite, release }),
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

/// Parse the arguments following the `e2e` subcommand:
/// `--suite <value> [--release]`.
///
/// `--suite <value>` is required; `--release` is an optional release-mode flag
/// (the V-2/V-3 gate commands in `RELEASE.md`). The two may appear in either
/// order, but any other token — or a repeated/stray argument — is an
/// [`XtaskError::UnexpectedArg`]. Returns the suite and whether release mode
/// was requested.
fn parse_e2e(rest: &[String]) -> Result<(Suite, bool), XtaskError> {
    let mut suite: Option<Suite> = None;
    let mut release = false;
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "--suite" if suite.is_none() => {
                let value = rest.get(i + 1).ok_or(XtaskError::MissingSuiteValue)?;
                suite = Some(Suite::parse(value)?);
                i += 2;
            }
            "--release" if !release => {
                release = true;
                i += 1;
            }
            other => return Err(XtaskError::UnexpectedArg(other.to_owned())),
        }
    }
    let suite = suite.ok_or(XtaskError::MissingSuiteValue)?;
    Ok((suite, release))
}

/// Dispatch a parsed command.
///
/// `e2e --suite smoke` is real (F-3, V-1) when built with `--features
/// smoke`: see [`e2e::run_smoke`]. Without that feature, the same command
/// still fails *closed* — [`XtaskError::SmokeUnavailable`] — rather than
/// silently no-opping. Every other subcommand — `extract`,
/// `record-snapshot`, `scenario`, and the `e2e` `full`/`soak` suites —
/// remains a stub: rather than exiting 0, each returns
/// [`XtaskError::NotImplemented`] so the process fails *closed* (non-zero
/// exit). The `RELEASE.md` promotion gates run these exact commands, so a
/// stub that reported success would satisfy a gate with zero validation
/// `(gated-by-default)` `(test-ratchet)`.
///
/// # Errors
///
/// Returns [`XtaskError::NotImplemented`] for every still-stubbed
/// subcommand/suite, [`XtaskError::SmokeUnavailable`] if `e2e --suite smoke`
/// was requested but this binary was built without the `smoke` feature, or
/// [`XtaskError::SmokeFailed`] if `e2e --suite smoke` ran but did not report
/// a clean boot.
fn dispatch(cmd: &Command) -> Result<(), XtaskError> {
    match cmd {
        Command::Extract => Err(XtaskError::NotImplemented("extract")),
        Command::RecordSnapshot => Err(XtaskError::NotImplemented("record-snapshot")),
        Command::Scenario => Err(XtaskError::NotImplemented("scenario")),
        #[cfg(feature = "smoke")]
        Command::E2e {
            suite: Suite::Smoke,
            ..
        } => e2e::run_smoke().map_err(|err| XtaskError::SmokeFailed(err.to_string())),
        #[cfg(not(feature = "smoke"))]
        Command::E2e {
            suite: Suite::Smoke,
            ..
        } => Err(XtaskError::SmokeUnavailable),
        Command::E2e { .. } => Err(XtaskError::NotImplemented("e2e")),
    }
}

/// Parse and dispatch a single invocation.
///
/// Kept separate from `main` so tests can drive it without spawning a process.
///
/// # Errors
///
/// Propagates any [`XtaskError`] from [`parse`], and (until the subcommands are
/// implemented) [`XtaskError::NotImplemented`] from [`dispatch`].
pub fn run(args: &[String]) -> Result<(), XtaskError> {
    let cmd = parse(args)?;
    dispatch(&cmd)
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
            Command::E2e {
                suite: Suite::Smoke,
                release: false
            }
        );
    }

    #[test]
    fn parse_e2e_full() {
        assert_eq!(
            parse(&args(&["e2e", "--suite", "full"])).unwrap(),
            Command::E2e {
                suite: Suite::Full,
                release: false
            }
        );
    }

    #[test]
    fn parse_e2e_soak() {
        assert_eq!(
            parse(&args(&["e2e", "--suite", "soak"])).unwrap(),
            Command::E2e {
                suite: Suite::Soak,
                release: false
            }
        );
    }

    #[test]
    fn parse_e2e_full_release() {
        // RELEASE.md V-2 gate: `cargo xtask e2e --suite full --release`.
        assert_eq!(
            parse(&args(&["e2e", "--suite", "full", "--release"])).unwrap(),
            Command::E2e {
                suite: Suite::Full,
                release: true
            }
        );
    }

    #[test]
    fn parse_e2e_soak_release() {
        // RELEASE.md V-3 gate: `cargo xtask e2e --suite soak --release`.
        assert_eq!(
            parse(&args(&["e2e", "--suite", "soak", "--release"])).unwrap(),
            Command::E2e {
                suite: Suite::Soak,
                release: true
            }
        );
    }

    #[test]
    fn parse_e2e_release_before_suite() {
        assert_eq!(
            parse(&args(&["e2e", "--release", "--suite", "smoke"])).unwrap(),
            Command::E2e {
                suite: Suite::Smoke,
                release: true
            }
        );
    }

    #[test]
    fn parse_e2e_rejects_duplicate_release() {
        let err = parse(&args(&["e2e", "--suite", "full", "--release", "--release"])).unwrap_err();
        assert!(matches!(err, XtaskError::UnexpectedArg(s) if s == "--release"));
    }

    #[test]
    fn parse_e2e_rejects_unknown_flag() {
        let err = parse(&args(&["e2e", "--suite", "smoke", "--bogus"])).unwrap_err();
        assert!(matches!(err, XtaskError::UnexpectedArg(s) if s == "--bogus"));
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
    fn run_stub_commands_fail_closed() {
        // Recognised-but-unimplemented subcommands must NOT report success:
        // RELEASE.md wires `xtask e2e --suite …` in as promotion gates, so a
        // stub exiting 0 would satisfy a gate with zero validation
        // `(gated-by-default)` `(test-ratchet)`. `e2e --suite smoke` is
        // deliberately absent here: with `--features smoke` it is no longer
        // a stub (F-3, V-1) and is covered directly by
        // `crate::e2e::tests::smoke_suite_boots_cleanly_headless` (the same
        // headless boot, kept there rather than duplicated here; the
        // dispatch routing itself is exercised by the CI smoke job's
        // `cargo run` step); without the feature it fails closed a different
        // way (see `e2e_smoke_without_feature_fails_closed` below), not as a
        // `NotImplemented` stub.
        assert!(matches!(
            run(&args(&["extract"])).unwrap_err(),
            XtaskError::NotImplemented("extract")
        ));
        assert!(matches!(
            run(&args(&["e2e", "--suite", "full", "--release"])).unwrap_err(),
            XtaskError::NotImplemented("e2e")
        ));
        assert!(matches!(
            run(&args(&["e2e", "--suite", "soak", "--release"])).unwrap_err(),
            XtaskError::NotImplemented("e2e")
        ));
    }

    // Without `--features smoke`, `mod e2e` is not compiled in at all, so
    // this binary must fail *closed* rather than silently no-opping
    // `(gated-by-default)` `(test-ratchet)`. `cargo test --workspace` (the
    // required CI job) builds without the feature, so this is exactly the
    // path that job exercises for `e2e --suite smoke` — it must NOT run the
    // real smoke boot (that stays confined to the non-required `smoke` CI
    // job building with `--features smoke`; see
    // `crate::e2e::tests::smoke_suite_boots_cleanly_headless`).
    #[test]
    #[cfg(not(feature = "smoke"))]
    fn e2e_smoke_without_feature_fails_closed() {
        let err = run(&args(&["e2e", "--suite", "smoke"])).unwrap_err();
        assert!(matches!(err, XtaskError::SmokeUnavailable));
    }

    #[test]
    fn smoke_unavailable_display_names_the_feature_and_has_no_usage_tail() {
        let rendered = XtaskError::SmokeUnavailable.to_string();
        assert!(rendered.contains("--features smoke"));
        assert!(!rendered.contains("usage: cargo xtask"));
    }

    #[test]
    fn not_implemented_display_has_no_usage_tail() {
        let rendered = XtaskError::NotImplemented("e2e").to_string();
        assert!(rendered.contains("not implemented"));
        assert!(!rendered.contains("usage: cargo xtask"));
    }

    #[test]
    fn smoke_failed_display_carries_the_reason_and_has_no_usage_tail() {
        let rendered = XtaskError::SmokeFailed("boom".to_owned()).to_string();
        assert!(rendered.contains("e2e --suite smoke"));
        assert!(rendered.contains("boom"));
        assert!(!rendered.contains("usage: cargo xtask"));
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
