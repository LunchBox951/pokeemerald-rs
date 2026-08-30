use std::fmt;

use pokeemerald_rs::main_menu::MainMenuItem;
use pokeemerald_rs::{App, AppButtons, AppState, BattleOutcome};

use crate::ScenarioName;

mod boot_to_first_fight;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScenarioFrame {
    buttons: AppButtons,
    expected: AppState,
}

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
    ScenarioFrame {
        buttons: AppButtons::NONE,
        expected: AppState::MainMenu(MainMenuItem::NewGame),
    },
];

// Mirrors `engine::overworld::WALK_FRAMES_PER_TILE` without adding a direct dependency.
const WALK_FRAMES_PER_TILE: usize = 16;

#[derive(Debug, Clone, Copy)]
struct Segment {
    buttons: AppButtons,
    count: usize,
    expected: AppState,
}

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

/// A scenario setup or execution failure. Every `frame` field below is a
/// zero-based index into the scenario's frames, not a one-based frame number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScenarioError {
    /// App creation failed.
    #[cfg(feature = "scenario")]
    Start(String),
    /// The initial app state did not match the scenario.
    InitialState {
        expected: AppState,
        actual: AppState,
    },
    /// Applying a frame's held buttons failed.
    Input { frame: usize, reason: String },
    /// Advancing one app frame failed.
    Step { frame: usize, reason: String },
    /// The app stopped before all scenario frames ran.
    UnexpectedStop { frame: usize },
    /// A frame ended in an unexpected app state.
    Milestone {
        frame: usize,
        expected: AppState,
        actual: AppState,
    },
    /// The first battle ended without a terminal outcome.
    FirstBattleEndedWithoutOutcome { frame: usize },
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

/// Results of a completed scenario.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// Completed frames.
    pub frames_run: usize,
    /// Visited states in order, including the initial state and excluding consecutive duplicates.
    pub milestones: Vec<AppState>,
    /// Retained terminal outcome of the scripted first battle, if one completed.
    pub first_battle_outcome: Option<BattleOutcome>,
}

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

/// Runs a named scenario against the default pack through the production headless app.
///
/// # Errors
///
/// Returns [`ScenarioError`] if app creation or a scripted frame fails.
#[cfg(feature = "scenario")]
pub fn run(name: ScenarioName) -> Result<Report, ScenarioError> {
    let mut app =
        App::new_headless_real().map_err(|error| ScenarioError::Start(error.to_string()))?;
    run_with_driver(spec(name), &mut app)
}

fn run_with_driver(
    spec: ScenarioSpec,
    driver: &mut impl ScenarioDriver,
) -> Result<Report, ScenarioError> {
    let actual_initial_state = driver.state();
    if actual_initial_state != spec.initial {
        return Err(ScenarioError::InitialState {
            expected: spec.initial,
            actual: actual_initial_state,
        });
    }

    let mut milestones = vec![actual_initial_state];
    let mut previous_state = actual_initial_state;
    for (frame, expected_frame) in spec.frames.iter().enumerate() {
        driver
            .set_buttons(expected_frame.buttons)
            .map_err(|reason| ScenarioError::Input { frame, reason })?;
        let should_continue = driver
            .step()
            .map_err(|reason| ScenarioError::Step { frame, reason })?;
        if !should_continue {
            return Err(ScenarioError::UnexpectedStop { frame });
        }

        let actual_state = driver.state();
        if actual_state != expected_frame.expected {
            return Err(ScenarioError::Milestone {
                frame,
                expected: expected_frame.expected,
                actual: actual_state,
            });
        }
        let first_battle_ended = spec.requires_first_battle_outcome
            && previous_state == AppState::FirstBattle
            && actual_state != AppState::FirstBattle;
        if first_battle_ended && driver.first_battle_outcome().is_none() {
            return Err(ScenarioError::FirstBattleEndedWithoutOutcome { frame });
        }
        if milestones.last() != Some(&actual_state) {
            milestones.push(actual_state);
        }
        previous_state = actual_state;
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

    struct FakeFrame {
        expected_buttons: AppButtons,
        next_state: AppState,
        should_continue: bool,
    }

    struct FakeDriver {
        current_state: AppState,
        held_buttons: AppButtons,
        frames: VecDeque<FakeFrame>,
        first_battle_outcome: Option<BattleOutcome>,
    }

    impl FakeDriver {
        fn boot_to_main_menu() -> Self {
            Self {
                current_state: AppState::Title,
                held_buttons: AppButtons::NONE,
                first_battle_outcome: None,
                frames: VecDeque::from([
                    FakeFrame {
                        expected_buttons: AppButtons::START,
                        next_state: AppState::MainMenu(MainMenuItem::NewGame),
                        should_continue: true,
                    },
                    FakeFrame {
                        expected_buttons: AppButtons::NONE,
                        next_state: AppState::MainMenu(MainMenuItem::NewGame),
                        should_continue: true,
                    },
                ]),
            }
        }
    }

    impl ScenarioDriver for FakeDriver {
        fn state(&self) -> AppState {
            self.current_state
        }

        fn first_battle_outcome(&self) -> Option<BattleOutcome> {
            self.first_battle_outcome
        }

        fn set_buttons(&mut self, buttons: AppButtons) -> Result<(), String> {
            self.held_buttons = buttons;
            Ok(())
        }

        fn step(&mut self) -> Result<bool, String> {
            let Some(frame) = self.frames.pop_front() else {
                return Err("script advanced beyond the fake's frames".to_owned());
            };
            if self.held_buttons != frame.expected_buttons {
                return Err(format!(
                    "expected held bits {:04x}, got {:04x}",
                    frame.expected_buttons.bits(),
                    self.held_buttons.bits()
                ));
            }
            self.current_state = frame.next_state;
            Ok(frame.should_continue)
        }
    }

    #[test]
    fn boot_to_main_menu_drives_press_release_and_ordered_milestones() {
        let mut driver = FakeDriver::boot_to_main_menu();
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
        let mut driver = FakeDriver::boot_to_main_menu();
        driver.current_state = AppState::SyntheticBoot;
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
        let mut driver = FakeDriver::boot_to_main_menu();
        driver.frames[0].next_state = AppState::Title;
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
        let mut driver = FakeDriver::boot_to_main_menu();
        driver.frames[0].should_continue = false;
        let error = run_with_driver(spec(ScenarioName::BootToMainMenu), &mut driver)
            .expect_err("a null backend must never stop");
        assert_eq!(error, ScenarioError::UnexpectedStop { frame: 0 });
    }

    #[test]
    fn a_required_first_battle_rejects_an_aborted_transition() {
        let mut driver = FakeDriver {
            current_state: AppState::FirstBattle,
            held_buttons: AppButtons::NONE,
            frames: VecDeque::from([FakeFrame {
                expected_buttons: AppButtons::NONE,
                next_state: AppState::Overworld,
                should_continue: true,
            }]),
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
