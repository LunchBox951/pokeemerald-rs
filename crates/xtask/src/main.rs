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
//! the headless boot-shell run it drives. `extract` (S-4, F-3) is also
//! real: see [`crate::extract::run`] for the local asset-pack pipeline it
//! drives (Discussion #71 policy A, issue #81). `record-snapshot` (F-3,
//! V-4, issue #226) is also real: see [`crate::record_snapshot::run`] for
//! the deterministic scene capture it drives, and `docs/snapshots.md` for
//! the capture format and the blessing workflow that consumes it.
//! `scenario` (F-3, issue #233) is also real: its named input scripts drive
//! the production [`pokeemerald_rs::App`] frame loop. Only the `e2e`
//! `full`/`soak` suites remain stubs.
//!
//! `mod e2e`, `mod record_snapshot`, and `mod scenario` need the
//! workspace-local `pokeemerald-rs` (and, for `record_snapshot`, `assets`)
//! dependency to drive a real scene, so all sit behind the shared `scenes`
//! cargo feature
//! (`Cargo.toml`) — a default `cargo build -p xtask` (every other
//! subcommand) stays dependency-free, matching pre-PR `xtask`. Without
//! `scenes`, each subcommand still fails *closed* —
//! [`XtaskError::SmokeUnavailable`] / [`XtaskError::RecordSnapshotUnavailable`] /
//! [`XtaskError::ScenarioUnavailable`],
//! not a silent no-op — telling the caller which feature to rebuild with.
//! `mod extract` needs no such gate: it depends on nothing beyond `std`, so
//! it is always compiled in.
//!
//! The two `#[cfg]`s are deliberately *asymmetric*. `mod e2e` is gated on
//! `smoke` (the caller-facing feature that names its subcommand), because
//! `e2e::run_smoke` boots a real windowless app that CI runs as its own
//! separate step. `mod record_snapshot` is gated on `scenes` — the shared
//! *implied* feature — one rung wider than the `record-snapshot` feature
//! that names its subcommand: `record_snapshot`'s tests are pure
//! synthetic-pack unit tests (`crate::record_snapshot::tests`, no real pack,
//! no window, no upstream checkout), and gating the module on
//! `record-snapshot` would have left them compiled out of every command CI
//! actually runs — invisible to every merge gate. Gating on `scenes` means
//! CI's existing `cargo test -p xtask --features smoke` leg compiles and
//! runs them, while `--features record-snapshot` (which implies `scenes`)
//! still builds the subcommand for a developer.
//!
//! Run via the workspace alias: `cargo xtask <subcommand>` for
//! feature-free subcommands (`extract`); a gated subcommand
//! needs the full `cargo run` form so the feature flag lands before the
//! `--`, e.g. `cargo run -p xtask --features smoke -- e2e --suite smoke`
//! or `cargo run -p xtask --features record-snapshot -- record-snapshot
//! --scene title`, or `cargo run -p xtask --features scenario -- scenario
//! --name boot-to-main-menu`.

use std::error::Error;
use std::fmt;
use std::process::ExitCode;

#[cfg(feature = "smoke")]
mod e2e;
mod extract;
#[cfg(feature = "scenes")]
mod record_snapshot;
#[cfg(any(feature = "scenario", all(test, feature = "scenes")))]
mod scenario;

/// Usage text shown on stderr for any parse error.
const USAGE: &str = "\
usage: cargo xtask <command>

commands:
  extract            extract data/assets from the upstream reference
  record-snapshot --scene <name>
                     record a deterministic frame capture; <name> is
                     title | main-menu-new-game | main-menu-option
  scenario --name <name>
                     run a scripted gameplay scenario; <name> is
                     boot-to-main-menu | boot-to-first-fight
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
    /// `record-snapshot` was given a `--scene` value that is not one of
    /// [`Scene`]'s known names.
    InvalidScene(String),
    /// `record-snapshot --scene` was present but no value followed it, or
    /// `record-snapshot` was given no `--scene` at all.
    MissingSceneValue,
    /// `scenario` was given a `--name` value that is not one of
    /// [`ScenarioName`]'s known names.
    InvalidScenario(String),
    /// `scenario --name` was present but no value followed it, or
    /// `scenario` was given no `--name` at all.
    MissingScenarioName,
    /// A subcommand received an argument it does not accept. Carries the
    /// offending token.
    UnexpectedArg(String),
    /// A recognised subcommand whose behaviour is not implemented yet. Carries
    /// the command's name. Returned so stub subcommands fail *closed* (non-zero
    /// exit) rather than silently reporting success — the `RELEASE.md` gate
    /// commands must never be satisfiable by a no-op `(gated-by-default)`
    /// `(test-ratchet)`.
    NotImplemented(&'static str),
    /// `extract` ran but failed. Carries [`extract::ExtractError`]'s
    /// rendered message (missing upstream checkout, a malformed source
    /// file, or a write failure — see that type for the exact cases).
    ExtractFailed(String),
    /// `e2e --suite smoke` ran but did not report a clean boot. Carries
    /// [`e2e::E2eError`]'s rendered message.
    SmokeFailed(String),
    /// `e2e --suite smoke` was requested, but this `xtask` binary was built
    /// without the `smoke` feature, so `mod e2e` (and its `pokeemerald-rs`
    /// dependency) was not compiled in. Fails *closed* rather than silently
    /// reporting success `(gated-by-default)` `(test-ratchet)`.
    SmokeUnavailable,
    /// `record-snapshot` ran but failed. Carries
    /// [`record_snapshot::RecordSnapshotError`]'s rendered message (missing
    /// local pack, a scene that failed to build, or a write failure — see
    /// that type for the exact cases).
    RecordSnapshotFailed(String),
    /// `record-snapshot` was requested, but this `xtask` binary was built
    /// without the `record-snapshot` feature, so `mod record_snapshot` (and
    /// its `pokeemerald-rs`/`assets` dependencies) was not compiled in.
    /// Fails *closed* rather than silently reporting success
    /// `(gated-by-default)` `(test-ratchet)`.
    RecordSnapshotUnavailable,
    /// `scenario` ran but its real headless app failed to start, accept an
    /// input frame, advance, or reach an expected milestone.
    ScenarioFailed(String),
    /// `scenario` was requested without the caller-facing `scenario`
    /// feature. Fails closed even when another feature happens to compile
    /// the shared scene dependencies.
    ScenarioUnavailable,
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
            Self::InvalidScene(scene) => {
                writeln!(f, "error: unknown record-snapshot scene `{scene}`")?;
            }
            Self::MissingSceneValue => {
                writeln!(f, "error: `record-snapshot --scene` requires a value")?;
            }
            Self::InvalidScenario(name) => {
                writeln!(f, "error: unknown scenario `{name}`")?;
            }
            Self::MissingScenarioName => {
                writeln!(f, "error: `scenario --name` requires a value")?;
            }
            Self::UnexpectedArg(arg) => {
                writeln!(f, "error: unexpected argument `{arg}`")?;
            }
            // A not-implemented status is a runtime failure, not a usage error,
            // so it gets no USAGE tail.
            Self::NotImplemented(what) => {
                return write!(f, "error: `{what}` is not implemented yet");
            }
            // Likewise an extract failure: runtime/behavioural, not a
            // malformed invocation.
            Self::ExtractFailed(reason) => {
                return write!(f, "error: `extract` failed: {reason}");
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
            // Likewise a record-snapshot failure: runtime/behavioural, not
            // a malformed invocation.
            Self::RecordSnapshotFailed(reason) => {
                return write!(f, "error: `record-snapshot` failed: {reason}");
            }
            // Likewise: a missing feature is a build-configuration problem,
            // not a malformed invocation, so it gets no USAGE tail either.
            Self::RecordSnapshotUnavailable => {
                return write!(
                    f,
                    "error: `record-snapshot` requires the `record-snapshot` feature: \
                     rebuild with `cargo run -p xtask --features record-snapshot -- \
                     record-snapshot --scene <name>`"
                );
            }
            Self::ScenarioFailed(reason) => {
                return write!(f, "error: `scenario` failed: {reason}");
            }
            Self::ScenarioUnavailable => {
                return write!(
                    f,
                    "error: `scenario` requires the `scenario` feature: rebuild with \
                     `cargo run -p xtask --features scenario -- scenario --name <name>`"
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

/// The scene `record-snapshot --scene <name>` captures (F-3, V-4).
///
/// Defined here at the crate root, not inside the feature-gated
/// [`record_snapshot`] module, so [`parse`] can route (and reject unknown)
/// scene names even in a build without the `record-snapshot` feature —
/// mirrors [`Suite`] living outside the feature-gated [`e2e`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scene {
    /// The real title screen (`pokeemerald_rs::title`), captured at frame
    /// 16 — a fixed frame inside the *visible* half of the "Press Start"
    /// blink, so the capture witnesses the banner (see
    /// `record_snapshot::TITLE_FRAME_INDEX` for the full rationale).
    Title,
    /// The no-save main menu (`pokeemerald_rs::main_menu`, issue #216) with
    /// its default selection, `NEW GAME`.
    MainMenuNewGame,
    /// The no-save main menu with `OPTION` selected (one `DPAD_DOWN` press
    /// from the default selection).
    MainMenuOption,
}

impl Scene {
    /// Parse a scene name.
    ///
    /// # Errors
    ///
    /// Returns [`XtaskError::InvalidScene`] if `value` is not a known scene.
    pub fn parse(value: &str) -> Result<Self, XtaskError> {
        match value {
            "title" => Ok(Self::Title),
            "main-menu-new-game" => Ok(Self::MainMenuNewGame),
            "main-menu-option" => Ok(Self::MainMenuOption),
            other => Err(XtaskError::InvalidScene(other.to_owned())),
        }
    }

    /// This scene's canonical name — the exact string [`Self::parse`]
    /// accepts back, and the stem `record_snapshot::run_with_paths` uses
    /// for its `<name>.rgb`/`<name>.meta` output files.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::MainMenuNewGame => "main-menu-new-game",
            Self::MainMenuOption => "main-menu-option",
        }
    }
}

/// A named scripted gameplay scenario (F-3, issue #233).
///
/// This type remains outside the feature-gated runner so unknown names are
/// rejected before the build-configuration error, matching [`Scene`]'s
/// fail-closed split.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScenarioName {
    /// Boot the real title screen and press Start into the no-save main menu.
    BootToMainMenu,
    /// Boot to the title, start a new game, walk the protagonist's room and
    /// Route 101, trigger `BATTLE_TYPE_FIRST_BATTLE`, and play it to a
    /// terminal outcome (I-7, issue #245).
    BootToFirstFight,
}

impl ScenarioName {
    /// Parse a scenario's canonical name.
    ///
    /// # Errors
    ///
    /// Returns [`XtaskError::InvalidScenario`] for an unknown name.
    pub fn parse(value: &str) -> Result<Self, XtaskError> {
        match value {
            "boot-to-main-menu" => Ok(Self::BootToMainMenu),
            "boot-to-first-fight" => Ok(Self::BootToFirstFight),
            other => Err(XtaskError::InvalidScenario(other.to_owned())),
        }
    }

    /// The exact name accepted by [`Self::parse`].
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::BootToMainMenu => "boot-to-main-menu",
            Self::BootToFirstFight => "boot-to-first-fight",
        }
    }
}

/// A parsed, validated `xtask` invocation.
#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    /// `extract`
    Extract,
    /// `record-snapshot --scene <name>`
    RecordSnapshot {
        /// The scene to capture.
        scene: Scene,
    },
    /// `scenario --name <name>`
    Scenario {
        /// The scripted run to execute.
        name: ScenarioName,
    },
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
        "record-snapshot" => {
            parse_record_snapshot(rest).map(|scene| Command::RecordSnapshot { scene })
        }
        "scenario" => parse_scenario(rest).map(|name| Command::Scenario { name }),
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

/// Parse the arguments following the `record-snapshot` subcommand:
/// `--scene <value>`, required.
///
/// # Errors
///
/// Returns [`XtaskError::MissingSceneValue`] if `--scene` is missing or has
/// no value, [`XtaskError::InvalidScene`] for an unknown scene name, and
/// [`XtaskError::UnexpectedArg`] for any other token (including a
/// duplicate `--scene`).
fn parse_record_snapshot(rest: &[String]) -> Result<Scene, XtaskError> {
    let mut scene: Option<Scene> = None;
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "--scene" if scene.is_none() => {
                let value = rest.get(i + 1).ok_or(XtaskError::MissingSceneValue)?;
                scene = Some(Scene::parse(value)?);
                i += 2;
            }
            other => return Err(XtaskError::UnexpectedArg(other.to_owned())),
        }
    }
    scene.ok_or(XtaskError::MissingSceneValue)
}

/// Parse `scenario --name <value>`, with exactly one required name.
fn parse_scenario(rest: &[String]) -> Result<ScenarioName, XtaskError> {
    let mut name: Option<ScenarioName> = None;
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "--name" if name.is_none() => {
                let value = rest.get(i + 1).ok_or(XtaskError::MissingScenarioName)?;
                name = Some(ScenarioName::parse(value)?);
                i += 2;
            }
            other => return Err(XtaskError::UnexpectedArg(other.to_owned())),
        }
    }
    name.ok_or(XtaskError::MissingScenarioName)
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
/// `extract` is real (S-4, F-3): see [`extract::run`] for the local
/// asset-pack pipeline it drives. `e2e --suite smoke` is also real (F-3,
/// V-1) when built with `--features smoke`: see [`e2e::run_smoke`].
/// `record-snapshot` is also real (F-3, V-4) when built with `--features
/// record-snapshot` (or anything else implying `scenes` — crate docs):
/// see [`record_snapshot::run`]. Without the matching
/// feature, each still fails *closed* —
/// [`XtaskError::SmokeUnavailable`] / [`XtaskError::RecordSnapshotUnavailable`] /
/// [`XtaskError::ScenarioUnavailable`] — rather than silently no-opping.
/// The `e2e` `full`/`soak` suites remain stubs: rather than
/// exiting 0, each returns [`XtaskError::NotImplemented`] so the process
/// fails *closed* (non-zero exit). The `RELEASE.md` promotion gates run
/// these exact commands, so a stub that reported success would satisfy a
/// gate with zero validation `(gated-by-default)` `(test-ratchet)`.
///
/// # Errors
///
/// Returns [`XtaskError::NotImplemented`] for each still-stubbed suite,
/// [`XtaskError::ExtractFailed`] if `extract` ran but
/// failed, [`XtaskError::SmokeUnavailable`] if `e2e --suite smoke` was
/// requested but this binary was built without the `smoke` feature,
/// [`XtaskError::SmokeFailed`] if `e2e --suite smoke` ran but did not report
/// a clean boot, [`XtaskError::RecordSnapshotUnavailable`] if
/// `record-snapshot` was requested but this binary was built without the
/// `record-snapshot` feature, or [`XtaskError::RecordSnapshotFailed`] if
/// `record-snapshot` ran but failed, [`XtaskError::ScenarioUnavailable`] if
/// `scenario` was requested without its feature, or
/// [`XtaskError::ScenarioFailed`] if the script did not complete.
fn dispatch(cmd: &Command) -> Result<(), XtaskError> {
    match cmd {
        Command::Extract => {
            let report =
                extract::run().map_err(|err| XtaskError::ExtractFailed(err.to_string()))?;
            println!(
                "extracted {} asset(s), {} bytes, to {}",
                report.entry_count,
                report.pack_size,
                report.output_path.display()
            );
            Ok(())
        }
        #[cfg(feature = "scenes")]
        Command::RecordSnapshot { scene } => {
            let report = record_snapshot::run(*scene)
                .map_err(|err| XtaskError::RecordSnapshotFailed(err.to_string()))?;
            println!(
                "recorded `{}` snapshot ({} byte(s)) to {} (+ {}); rgb hash {}, pack hash {}, git sha {}",
                scene.name(),
                report.payload_len,
                report.rgb_path.display(),
                report.meta_path.display(),
                report.rgb_hash,
                report.pack_hash,
                report.git_sha,
            );
            Ok(())
        }
        #[cfg(not(feature = "scenes"))]
        Command::RecordSnapshot { .. } => Err(XtaskError::RecordSnapshotUnavailable),
        #[cfg(feature = "scenario")]
        Command::Scenario { name } => {
            let report =
                scenario::run(*name).map_err(|err| XtaskError::ScenarioFailed(err.to_string()))?;
            println!(
                "scenario `{}` passed: {} frame(s), milestones {:?}",
                name.name(),
                report.frames_run,
                report.milestones,
            );
            Ok(())
        }
        #[cfg(not(feature = "scenario"))]
        Command::Scenario { .. } => Err(XtaskError::ScenarioUnavailable),
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
    use super::{extract, parse, run, Command, ScenarioName, Scene, Suite, XtaskError, USAGE};

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
            parse(&args(&["record-snapshot", "--scene", "title"])).unwrap(),
            Command::RecordSnapshot {
                scene: Scene::Title
            }
        );
        assert_eq!(
            parse(&args(&["record-snapshot", "--scene", "main-menu-new-game"])).unwrap(),
            Command::RecordSnapshot {
                scene: Scene::MainMenuNewGame
            }
        );
        assert_eq!(
            parse(&args(&["record-snapshot", "--scene", "main-menu-option"])).unwrap(),
            Command::RecordSnapshot {
                scene: Scene::MainMenuOption
            }
        );
    }

    #[test]
    fn parse_record_snapshot_requires_scene() {
        let err = parse(&args(&["record-snapshot"])).unwrap_err();
        assert!(matches!(err, XtaskError::MissingSceneValue));
    }

    #[test]
    fn parse_record_snapshot_missing_scene_value() {
        let err = parse(&args(&["record-snapshot", "--scene"])).unwrap_err();
        assert!(matches!(err, XtaskError::MissingSceneValue));
    }

    #[test]
    fn parse_record_snapshot_invalid_scene() {
        let err = parse(&args(&["record-snapshot", "--scene", "bogus"])).unwrap_err();
        assert!(matches!(err, XtaskError::InvalidScene(s) if s == "bogus"));
    }

    #[test]
    fn parse_record_snapshot_rejects_trailing_args() {
        let err = parse(&args(&["record-snapshot", "--scene", "title", "extra"])).unwrap_err();
        assert!(matches!(err, XtaskError::UnexpectedArg(s) if s == "extra"));
    }

    #[test]
    fn parse_record_snapshot_rejects_duplicate_scene() {
        let err = parse(&args(&[
            "record-snapshot",
            "--scene",
            "title",
            "--scene",
            "title",
        ]))
        .unwrap_err();
        assert!(matches!(err, XtaskError::UnexpectedArg(s) if s == "--scene"));
    }

    #[test]
    fn scene_parse_roundtrip() {
        assert_eq!(Scene::parse("title").unwrap(), Scene::Title);
        assert_eq!(
            Scene::parse("main-menu-new-game").unwrap(),
            Scene::MainMenuNewGame
        );
        assert_eq!(
            Scene::parse("main-menu-option").unwrap(),
            Scene::MainMenuOption
        );
        assert!(matches!(
            Scene::parse("nope").unwrap_err(),
            XtaskError::InvalidScene(_)
        ));
    }

    #[test]
    fn scene_name_round_trips_through_parse() {
        for scene in [Scene::Title, Scene::MainMenuNewGame, Scene::MainMenuOption] {
            assert_eq!(Scene::parse(scene.name()).unwrap(), scene);
        }
    }

    #[test]
    fn parse_scenario_routes() {
        assert_eq!(
            parse(&args(&["scenario", "--name", "boot-to-main-menu"])).unwrap(),
            Command::Scenario {
                name: ScenarioName::BootToMainMenu
            }
        );
    }

    #[test]
    fn parse_scenario_requires_name() {
        assert!(matches!(
            parse(&args(&["scenario"])).unwrap_err(),
            XtaskError::MissingScenarioName
        ));
        assert!(matches!(
            parse(&args(&["scenario", "--name"])).unwrap_err(),
            XtaskError::MissingScenarioName
        ));
    }

    #[test]
    fn parse_scenario_rejects_unknown_and_duplicate_names() {
        assert!(matches!(
            parse(&args(&["scenario", "--name", "bogus"])).unwrap_err(),
            XtaskError::InvalidScenario(name) if name == "bogus"
        ));
        assert!(matches!(
            parse(&args(&[
                "scenario",
                "--name",
                "boot-to-main-menu",
                "--name",
                "boot-to-main-menu"
            ]))
            .unwrap_err(),
            XtaskError::UnexpectedArg(arg) if arg == "--name"
        ));
    }

    #[test]
    fn every_scenario_name_round_trips_and_appears_in_usage() {
        const ALL: &[ScenarioName] =
            &[ScenarioName::BootToMainMenu, ScenarioName::BootToFirstFight];
        for name in ALL {
            // Compile-forced completeness: adding a `ScenarioName` variant
            // breaks this match, steering the author here to extend `ALL`
            // -- which then drags along `parse` (whose string catch-all
            // the compiler cannot check) and USAGE's hardcoded name list.
            match name {
                ScenarioName::BootToMainMenu | ScenarioName::BootToFirstFight => {}
            }
            assert_eq!(ScenarioName::parse(name.name()).unwrap(), *name);
            assert!(
                USAGE.contains(name.name()),
                "USAGE must list `{}`",
                name.name()
            );
        }
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
        // `(gated-by-default)` `(test-ratchet)`. `extract`, `e2e --suite
        // smoke`, and `record-snapshot` are deliberately absent here: all
        // four are no longer stubs (S-4/F-3, F-3/V-1, F-3/V-4, and F-3
        // issue #233 respectively) and each has dedicated coverage —
        // `extract` via `extract_dispatch_fails_closed_without_local_checkout`/
        // `extract_dispatch_succeeds_with_local_checkout` below, `e2e
        // --suite smoke` via `crate::e2e::tests::smoke_suite_boots_cleanly_headless`
        // (with the feature) and `e2e_smoke_without_feature_fails_closed`
        // below (without it), `record-snapshot` via
        // `record_snapshot_without_feature_fails_closed` below (without the
        // feature) and `crate::record_snapshot::tests` (with it), and
        // `scenario` via the unavailable check below (without its feature),
        // the two `scenario_dispatch_*` wiring tests below (with it), plus
        // `crate::scenario::tests` under `--features smoke`/`scenario`.
        assert!(matches!(
            run(&args(&["e2e", "--suite", "full", "--release"])).unwrap_err(),
            XtaskError::NotImplemented("e2e")
        ));
        assert!(matches!(
            run(&args(&["e2e", "--suite", "soak", "--release"])).unwrap_err(),
            XtaskError::NotImplemented("e2e")
        ));
    }

    #[test]
    fn extract_dispatch_fails_closed_without_local_checkout() {
        // In an environment with no `pokeemerald/` checkout (e.g. CI, or a
        // fresh clone before `./init.sh`), `extract` must fail loudly, not
        // silently report success `(gated-by-default)`. Checked read-only
        // (`extract::upstream_present`) rather than by calling
        // `extract::run()` and inspecting the result, so this test never
        // triggers a real (slow, disk-writing) extraction as a side effect
        // in a dev environment that *does* have a checkout -- that path is
        // covered instead by `extract_dispatch_succeeds_with_local_checkout`.
        if extract::upstream_present() {
            return;
        }
        let err = run(&args(&["extract"])).unwrap_err();
        assert!(matches!(err, XtaskError::ExtractFailed(_)));
        assert!(err.to_string().contains("init.sh"));
    }

    #[test]
    #[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
    fn extract_dispatch_succeeds_with_local_checkout() {
        // Rewrites the real pack -- exclude concurrent real-pack readers
        // (see `extract::REAL_PACK_LOCK`).
        let _pack = extract::REAL_PACK_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        run(&args(&["extract"])).expect("extract should succeed against a real checkout");
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

    // Identical reasoning to `e2e_smoke_without_feature_fails_closed`, for
    // `record-snapshot`: in a build with neither `scenes`-implying feature,
    // `mod record_snapshot` is not compiled in, so this must fail *closed*
    // rather than silently no-opping `(gated-by-default)` `(test-ratchet)`.
    // Gated on `scenes`, not `record-snapshot`, to match the module's own
    // `#[cfg]` (crate docs' "asymmetric" note): under `--features smoke` the
    // subcommand *is* compiled in and dispatch really runs it, so asserting
    // `RecordSnapshotUnavailable` there would be asserting the opposite of
    // the truth. The determinism/metadata behaviour with the module present
    // lives in `crate::record_snapshot::tests` instead.
    #[test]
    #[cfg(not(feature = "scenes"))]
    fn record_snapshot_without_feature_fails_closed() {
        let err = run(&args(&["record-snapshot", "--scene", "title"])).unwrap_err();
        assert!(matches!(err, XtaskError::RecordSnapshotUnavailable));
    }

    #[test]
    #[cfg(not(feature = "scenario"))]
    fn scenario_without_feature_fails_closed_after_validating_the_name() {
        let err = run(&args(&["scenario", "--name", "boot-to-main-menu"])).unwrap_err();
        assert!(matches!(err, XtaskError::ScenarioUnavailable));

        let invalid = run(&args(&["scenario", "--name", "bogus"])).unwrap_err();
        assert!(matches!(invalid, XtaskError::InvalidScenario(name) if name == "bogus"));
    }

    // The other half of `scenario`'s wiring proof, mirroring
    // `record_snapshot_dispatch_fails_closed_without_a_pack` below: with
    // the feature-enabled arm compiled in, dispatch must really reach
    // `scenario::run` -- an arm that swallowed the error (or returned
    // `Ok(())` without running anything) would let `cargo xtask scenario`
    // report success with zero validation `(gated-by-default)`
    // `(test-ratchet)`. Checked through the pack-missing failure path so a
    // pack-free `--features scenario` test run exercises it, and skips
    // read-only on a box whose real pack would make the run succeed --
    // that full path is `scenario_dispatch_succeeds_against_the_real_pack`
    // below.
    #[test]
    #[cfg(feature = "scenario")]
    fn scenario_dispatch_fails_closed_without_a_pack() {
        // Hold the real-pack lock across the existence probe *and* the
        // run, exactly as the record-snapshot twin does: under
        // `--include-ignored`, `extract_dispatch_succeeds_with_local_
        // checkout` could otherwise create the pack between the two and
        // turn the expected error into a success.
        let _pack = extract::REAL_PACK_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if assets::pack::AssetPack::default_path().exists() {
            return;
        }
        let err = run(&args(&["scenario", "--name", "boot-to-main-menu"])).unwrap_err();
        assert!(matches!(err, XtaskError::ScenarioFailed(_)));
        assert!(err.to_string().contains("pack"));
    }

    // The success half of the same wiring proof, run by CI's ignored
    // real-checkout xtask leg (`--features record-snapshot,scenario --
    // --ignored`, pack present): the one automated invocation that drives
    // the feature-enabled `Command::Scenario` arm end to end.
    // `crate::scenario`'s own ignored real-pack test proves the runner but
    // calls `scenario::run` directly, bypassing dispatch.
    #[test]
    #[cfg(feature = "scenario")]
    #[ignore = "needs a local pack produced by `cargo xtask extract`"]
    fn scenario_dispatch_succeeds_against_the_real_pack() {
        let _pack = extract::REAL_PACK_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        run(&args(&["scenario", "--name", "boot-to-main-menu"]))
            .expect("boot-to-main-menu should pass through dispatch against the real pack");
    }

    // The other half of the wiring proof, mirroring
    // `extract_dispatch_fails_closed_without_local_checkout`: with the
    // module compiled in, dispatch must really reach
    // `record_snapshot::run` -- a stub arm that reported success would
    // satisfy a gate with zero validation `(gated-by-default)`
    // `(test-ratchet)`. Checked through the pack-missing error path so
    // this runs (and fails a no-op mutation) in pack-free CI, and skips
    // read-only on a dev box whose real pack would make `run` succeed --
    // that full path is `record_snapshot`'s own ignored real-pack test.
    #[test]
    #[cfg(feature = "scenes")]
    fn record_snapshot_dispatch_fails_closed_without_a_pack() {
        // Hold the real-pack lock across the existence probe *and* the
        // run: under `--include-ignored`, `extract_dispatch_succeeds_with_
        // local_checkout` could otherwise create the pack between the two
        // and turn the expected error into a success.
        let _pack = extract::REAL_PACK_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if assets::pack::AssetPack::default_path().exists() {
            return;
        }
        let err = run(&args(&["record-snapshot", "--scene", "title"])).unwrap_err();
        assert!(matches!(err, XtaskError::RecordSnapshotFailed(_)));
        assert!(err.to_string().contains("pack"));
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
    fn record_snapshot_unavailable_display_names_the_feature_and_has_no_usage_tail() {
        let rendered = XtaskError::RecordSnapshotUnavailable.to_string();
        assert!(rendered.contains("--features record-snapshot"));
        assert!(!rendered.contains("usage: cargo xtask"));
    }

    #[test]
    fn record_snapshot_failed_display_carries_the_reason_and_has_no_usage_tail() {
        let rendered = XtaskError::RecordSnapshotFailed("boom".to_owned()).to_string();
        assert!(rendered.contains("record-snapshot"));
        assert!(rendered.contains("boom"));
        assert!(!rendered.contains("usage: cargo xtask"));
    }

    #[test]
    fn scenario_unavailable_display_names_the_feature_and_has_no_usage_tail() {
        let rendered = XtaskError::ScenarioUnavailable.to_string();
        assert!(rendered.contains("--features scenario"));
        assert!(!rendered.contains("usage: cargo xtask"));
    }

    #[test]
    fn scenario_failed_display_carries_the_reason_and_has_no_usage_tail() {
        let rendered = XtaskError::ScenarioFailed("boom".to_owned()).to_string();
        assert!(rendered.contains("scenario"));
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
