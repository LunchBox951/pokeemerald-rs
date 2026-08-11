//! Named scripted headless runs through the real [`pokeemerald_rs::App`]
//! (F-3, issue #233).
//!
//! A [`ScenarioSpec`] is plain Rust data: an expected initial
//! [`AppState`](pokeemerald_rs::AppState), followed by per-frame held-button
//! sets and expected states. [`run`] constructs the same real game flow as
//! the windowed binary via [`App::new_headless_real`], injects each held set
//! into the null platform backend, and advances only through [`App::step`].
//! The runner never calls `flow` directly, sleeps, reads wall time, or owns a
//! second transition implementation.
//!
//! The driver trait is private test infrastructure. Pack-free tests use it
//! to pin script ordering and failure behavior under CI's existing
//! `--features smoke` leg; the production implementation is [`App`] itself.
//! The ignored proving test holds `extract::REAL_PACK_LOCK` while reading the
//! one developer-local pack, composing safely with every other real-pack
//! xtask test.

use std::fmt;

use pokeemerald_rs::main_menu::MainMenuItem;
use pokeemerald_rs::{App, AppButtons, AppState, BattleOutcome};

use crate::ScenarioName;

/// `boot-to-first-fight` (I-7, issue #245) -- split into its own file to
/// keep this module under the `oop-boundaries` size guideline; see that
/// module's own docs for the scenario itself.
mod boot_to_first_fight;

/// One exact frame of a scenario: which buttons are held while the app
/// pumps input, and which state must be active after that frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScenarioFrame {
    buttons: AppButtons,
    expected: AppState,
}

/// An in-repository scripted run. All slices are static so adding a
/// scenario requires no parser, data-file format, or dependency
/// (`minimal-deps`).
#[derive(Debug, Clone, Copy)]
struct ScenarioSpec {
    initial: AppState,
    frames: &'static [ScenarioFrame],
    requires_first_battle_outcome: bool,
}

const BOOT_TO_MAIN_MENU: [ScenarioFrame; 2] = [
    ScenarioFrame {
        buttons: AppButtons::START,
        expected: AppState::MainMenu(MainMenuItem::NewGame),
    },
    // Release Start on the next frame and prove the menu remains stable;
    // otherwise a later scenario could inherit a held key and miss the next
    // newly-pressed edge.
    ScenarioFrame {
        buttons: AppButtons::NONE,
        expected: AppState::MainMenu(MainMenuItem::NewGame),
    },
];

/// `engine::overworld::WALK_FRAMES_PER_TILE`, restated rather than
/// imported: `xtask` only pulls in `pokeemerald-rs` (behind the `scenes`
/// feature, `crate` root docs' "asymmetric" note), not `engine` directly --
/// adding that edge just for one constant would widen the dependency seam
/// this crate deliberately keeps narrow. Sixteen rendered frames per tile
/// crossing, matching every citation of the same constant in
/// `pokeemerald_rs::flow::overworld_phase`'s own real-pack tests.
const WALK_FRAMES_PER_TILE: usize = 16;

/// One run of `count` consecutive frames holding `buttons`, all expecting
/// `expected` -- [`expand_segments`]'s compact authoring unit, so a
/// multi-hundred-frame walk doesn't need one literal [`ScenarioFrame`] per
/// frame in this file (`oop-boundaries`).
///
/// A held direction only pays a facing-turn frame when the player is at
/// rest facing a different way
/// (`engine::overworld::player::PlayerState::step`'s own `running !=
/// RunningState::Moving` guard on its turn branch): as long as input never
/// lets go of a direction between two tiles, changing direction costs
/// nothing extra -- the new direction just steers the very next tile.
/// [`boot_to_first_fight`]'s own script never releases a direction
/// mid-walk, so every one of its walking segments runs at exactly
/// [`WALK_FRAMES_PER_TILE`] frames per tile, turn included, verified
/// empirically against the real pack while authoring that scenario (see
/// that constant's own doc comment).
#[derive(Debug, Clone, Copy)]
struct Segment {
    buttons: AppButtons,
    count: usize,
    expected: AppState,
}

/// Flatten [`Segment`]s into the frame-per-frame script [`ScenarioSpec`]
/// needs.
fn expand_segments(segments: &[Segment]) -> Vec<ScenarioFrame> {
    let mut frames = Vec::with_capacity(segments.iter().map(|segment| segment.count).sum());
    for segment in segments {
        frames.extend(std::iter::repeat_n(
            ScenarioFrame {
                buttons: segment.buttons,
                expected: segment.expected,
            },
            segment.count,
        ));
    }
    frames
}

/// Look up a parsed scenario's definition. Exhaustive over
/// [`ScenarioName`], so a new public name cannot silently lack a script.
fn spec(name: ScenarioName) -> ScenarioSpec {
    match name {
        ScenarioName::BootToMainMenu => ScenarioSpec {
            initial: AppState::Title,
            frames: &BOOT_TO_MAIN_MENU,
            requires_first_battle_outcome: false,
        },
        ScenarioName::BootToFirstFight => ScenarioSpec {
            initial: AppState::Title,
            frames: boot_to_first_fight::frames(),
            requires_first_battle_outcome: true,
        },
    }
}

/// Why a named scenario did not complete exactly as specified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScenarioError {
    /// Loading the real app/pack failed before the first milestone.
    #[cfg(feature = "scenario")]
    Start(String),
    /// The app did not begin in the scenario's required state.
    InitialState {
        /// Required state.
        expected: AppState,
        /// State the app actually reported.
        actual: AppState,
    },
    /// Supplying one frame's held-button set failed.
    Input {
        /// Zero-based frame within the scenario.
        frame: usize,
        /// Underlying app/platform diagnostic.
        reason: String,
    },
    /// Advancing the production frame loop failed.
    Step {
        /// Zero-based frame within the scenario.
        frame: usize,
        /// Underlying app/platform diagnostic.
        reason: String,
    },
    /// A headless app unexpectedly reported a window-style stop.
    UnexpectedStop {
        /// Zero-based frame within the scenario.
        frame: usize,
    },
    /// A frame completed but reached the wrong flow state.
    Milestone {
        /// Zero-based frame within the scenario.
        frame: usize,
        /// Required post-frame state.
        expected: AppState,
        /// Actual post-frame state.
        actual: AppState,
    },
    /// A required scripted first battle left its active state without the
    /// app retaining a terminal battle outcome.
    FirstBattleEndedWithoutOutcome {
        /// Zero-based frame on which the active battle slot emptied.
        frame: usize,
    },
}

impl fmt::Display for ScenarioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(feature = "scenario")]
            Self::Start(reason) => write!(f, "real headless app failed to start: {reason}"),
            Self::InitialState { expected, actual } => write!(
                f,
                "initial milestone mismatch: expected {expected:?}, reached {actual:?}"
            ),
            Self::Input { frame, reason } => {
                write!(f, "frame {frame} input injection failed: {reason}")
            }
            Self::Step { frame, reason } => {
                write!(f, "frame {frame} app step failed: {reason}")
            }
            Self::UnexpectedStop { frame } => {
                write!(f, "frame {frame} reported an unexpected stop")
            }
            Self::Milestone {
                frame,
                expected,
                actual,
            } => write!(
                f,
                "frame {frame} milestone mismatch: expected {expected:?}, reached {actual:?}"
            ),
            Self::FirstBattleEndedWithoutOutcome { frame } => write!(
                f,
                "frame {frame} ended the scripted first battle without a terminal outcome"
            ),
        }
    }
}

impl std::error::Error for ScenarioError {}

/// A successful scenario summary, returned for CLI output and test
/// assertions without re-running the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// Number of [`App::step`] calls completed.
    pub frames_run: usize,
    /// Distinct expected states reached in order, including the initial
    /// state and suppressing consecutive duplicates.
    pub milestones: Vec<AppState>,
    /// The terminal result retained by the app's scripted first-battle
    /// channel, if this run completed one.
    pub first_battle_outcome: Option<BattleOutcome>,
}

/// The minimal owned-driver boundary the runner needs. [`App`] is the only
/// production implementation; fakes exist only in this module's tests.
trait ScenarioDriver {
    fn state(&self) -> AppState;
    fn first_battle_outcome(&self) -> Option<BattleOutcome>;
    fn set_buttons(&mut self, buttons: AppButtons) -> Result<(), String>;
    fn step(&mut self) -> Result<bool, String>;
}

impl ScenarioDriver for App {
    fn state(&self) -> AppState {
        self.state()
    }

    fn first_battle_outcome(&self) -> Option<BattleOutcome> {
        self.first_battle_outcome()
    }

    fn set_buttons(&mut self, buttons: AppButtons) -> Result<(), String> {
        self.set_headless_buttons(buttons)
            .map_err(|error| error.to_string())
    }

    fn step(&mut self) -> Result<bool, String> {
        self.step().map_err(|error| error.to_string())
    }
}

/// Run `name` against the default local pack through the real headless app.
///
/// # Errors
///
/// Returns [`ScenarioError`] if app construction, input injection, a frame
/// step, or any expected state milestone fails.
#[cfg(feature = "scenario")]
pub fn run(name: ScenarioName) -> Result<Report, ScenarioError> {
    let mut app =
        App::new_headless_real().map_err(|error| ScenarioError::Start(error.to_string()))?;
    run_with_driver(spec(name), &mut app)
}

/// Execute one definition against an owned driver. Kept separate from
/// [`run`] so pack-free tests can cover the state assertions themselves.
fn run_with_driver(
    spec: ScenarioSpec,
    driver: &mut impl ScenarioDriver,
) -> Result<Report, ScenarioError> {
    let initial = driver.state();
    if initial != spec.initial {
        return Err(ScenarioError::InitialState {
            expected: spec.initial,
            actual: initial,
        });
    }

    let mut milestones = vec![initial];
    let mut previous = initial;
    for (frame, scripted) in spec.frames.iter().enumerate() {
        driver
            .set_buttons(scripted.buttons)
            .map_err(|reason| ScenarioError::Input { frame, reason })?;
        let keep_going = driver
            .step()
            .map_err(|reason| ScenarioError::Step { frame, reason })?;
        if !keep_going {
            return Err(ScenarioError::UnexpectedStop { frame });
        }

        let actual = driver.state();
        if actual != scripted.expected {
            return Err(ScenarioError::Milestone {
                frame,
                expected: scripted.expected,
                actual,
            });
        }
        if spec.requires_first_battle_outcome
            && previous == AppState::FirstBattle
            && actual != AppState::FirstBattle
            && driver.first_battle_outcome().is_none()
        {
            return Err(ScenarioError::FirstBattleEndedWithoutOutcome { frame });
        }
        if milestones.last() != Some(&actual) {
            milestones.push(actual);
        }
        previous = actual;
    }

    Ok(Report {
        frames_run: spec.frames.len(),
        milestones,
        first_battle_outcome: driver.first_battle_outcome(),
    })
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::{
        run_with_driver, spec, ScenarioDriver, ScenarioError, ScenarioFrame, ScenarioSpec,
    };
    use crate::ScenarioName;
    use pokeemerald_rs::main_menu::MainMenuItem;
    use pokeemerald_rs::{AppButtons, AppState, BattleOutcome};

    struct FakeDriver {
        state: AppState,
        pending: AppButtons,
        frames: VecDeque<(AppButtons, AppState, bool)>,
        first_battle_outcome: Option<BattleOutcome>,
    }

    impl FakeDriver {
        fn healthy() -> Self {
            Self {
                state: AppState::Title,
                pending: AppButtons::NONE,
                first_battle_outcome: None,
                frames: VecDeque::from([
                    (
                        AppButtons::START,
                        AppState::MainMenu(MainMenuItem::NewGame),
                        true,
                    ),
                    (
                        AppButtons::NONE,
                        AppState::MainMenu(MainMenuItem::NewGame),
                        true,
                    ),
                ]),
            }
        }
    }

    impl ScenarioDriver for FakeDriver {
        fn state(&self) -> AppState {
            self.state
        }

        fn first_battle_outcome(&self) -> Option<BattleOutcome> {
            self.first_battle_outcome
        }

        fn set_buttons(&mut self, buttons: AppButtons) -> Result<(), String> {
            self.pending = buttons;
            Ok(())
        }

        fn step(&mut self) -> Result<bool, String> {
            let Some((expected_buttons, next, keep_going)) = self.frames.pop_front() else {
                return Err("script advanced beyond the fake's frames".to_owned());
            };
            if self.pending != expected_buttons {
                return Err(format!(
                    "expected held bits {:04x}, got {:04x}",
                    expected_buttons.bits(),
                    self.pending.bits()
                ));
            }
            self.state = next;
            Ok(keep_going)
        }
    }

    #[test]
    fn boot_to_main_menu_drives_press_release_and_ordered_milestones() {
        let mut driver = FakeDriver::healthy();
        let report = run_with_driver(spec(ScenarioName::BootToMainMenu), &mut driver)
            .expect("the proving scenario should pass");

        assert_eq!(report.frames_run, 2);
        assert_eq!(report.first_battle_outcome, None);
        assert_eq!(
            report.milestones,
            vec![AppState::Title, AppState::MainMenu(MainMenuItem::NewGame)]
        );
        assert!(driver.frames.is_empty(), "every scripted frame must run");
    }

    #[test]
    fn a_wrong_initial_state_fails_before_input() {
        let mut driver = FakeDriver::healthy();
        driver.state = AppState::SyntheticBoot;
        let error = run_with_driver(spec(ScenarioName::BootToMainMenu), &mut driver)
            .expect_err("the title milestone is required");
        assert!(matches!(
            error,
            ScenarioError::InitialState {
                expected: AppState::Title,
                actual: AppState::SyntheticBoot
            }
        ));
        assert_eq!(
            driver.frames.len(),
            2,
            "no frame may run after the mismatch"
        );
    }

    #[test]
    fn a_wrong_post_frame_milestone_fails_closed() {
        let mut driver = FakeDriver::healthy();
        driver.frames[0].1 = AppState::Title;
        let error = run_with_driver(spec(ScenarioName::BootToMainMenu), &mut driver)
            .expect_err("the menu milestone is required");
        assert!(matches!(
            error,
            ScenarioError::Milestone {
                frame: 0,
                expected: AppState::MainMenu(MainMenuItem::NewGame),
                actual: AppState::Title
            }
        ));
    }

    #[test]
    fn an_unexpected_stop_fails_before_accepting_the_milestone() {
        let mut driver = FakeDriver::healthy();
        driver.frames[0].2 = false;
        let error = run_with_driver(spec(ScenarioName::BootToMainMenu), &mut driver)
            .expect_err("a null backend must never stop");
        assert_eq!(error, ScenarioError::UnexpectedStop { frame: 0 });
    }

    #[test]
    fn a_required_first_battle_rejects_an_aborted_transition() {
        let mut driver = FakeDriver {
            state: AppState::FirstBattle,
            pending: AppButtons::NONE,
            frames: VecDeque::from([(AppButtons::NONE, AppState::Overworld, true)]),
            first_battle_outcome: None,
        };
        let scenario = ScenarioSpec {
            initial: AppState::FirstBattle,
            frames: &[ScenarioFrame {
                buttons: AppButtons::NONE,
                expected: AppState::Overworld,
            }],
            requires_first_battle_outcome: true,
        };

        let error = run_with_driver(scenario, &mut driver)
            .expect_err("an emptied first-battle slot needs a terminal outcome");
        assert_eq!(
            error,
            ScenarioError::FirstBattleEndedWithoutOutcome { frame: 0 }
        );
    }

    #[test]
    #[cfg(feature = "scenario")]
    #[ignore = "needs a local pack produced by `cargo xtask extract`"]
    fn real_pack_boot_to_main_menu_passes_and_reaches_every_milestone_in_order() {
        let _pack = crate::extract::REAL_PACK_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let report = super::run(ScenarioName::BootToMainMenu)
            .expect("boot-to-main-menu should pass against the real pack");
        assert_eq!(report.frames_run, 2);
        assert_eq!(report.first_battle_outcome, None);
        assert_eq!(
            report.milestones,
            vec![AppState::Title, AppState::MainMenu(MainMenuItem::NewGame)]
        );
    }

    // `boot-to-first-fight` (I-7, issue #245) itself lives in
    // `boot_to_first_fight`, including its own tests -- this module keeps
    // only the shared runner's tests (`oop-boundaries`).

    use super::{expand_segments, Segment};

    #[test]
    fn expand_segments_flattens_each_run_in_order() {
        let segments = [
            Segment {
                buttons: AppButtons::UP,
                count: 3,
                expected: AppState::Overworld,
            },
            Segment {
                buttons: AppButtons::A,
                count: 1,
                expected: AppState::FirstBattle,
            },
        ];
        let frames = expand_segments(&segments);
        assert_eq!(frames.len(), 4);
        assert!(frames[..3]
            .iter()
            .all(|frame| frame.buttons == AppButtons::UP && frame.expected == AppState::Overworld));
        assert_eq!(frames[3].buttons, AppButtons::A);
        assert_eq!(frames[3].expected, AppState::FirstBattle);
    }
}
