use std::sync::OnceLock;

use pokeemerald_rs::main_menu::MainMenuItem;
use pokeemerald_rs::{AppButtons, AppState};

use super::{expand_segments, ScenarioFrame, Segment, WALK_FRAMES_PER_TILE};

#[cfg(test)]
const TITLE_AND_MENU_FRAMES: usize = 2;
const INTRO_HANDOFF_FRAMES: usize = 1;
const BUTTON_EDGE_FRAMES: usize = 1;
const WELCOME_FIRST_PROMPT_RUN_INDEX: usize = 0;
const MAIN_SPEECH_FIRST_SCROLL_PROMPT_RUN_INDEX: usize = 8;
const B_CONFIRM_RUN_INDICES: &[usize] = &[
    WELCOME_FIRST_PROMPT_RUN_INDEX,
    MAIN_SPEECH_FIRST_SCROLL_PROMPT_RUN_INDEX,
];

const STEP_OFF_BEDROOM_STAIR_WARP_TILES: usize = 1;
const REENTER_BEDROOM_STAIR_WARP_TILES: usize = 1;
const HOUSE_HALL_TO_FRONT_DOOR_TILES: usize = 6;
const CLEAR_TOWN_DOOR_LANDING_TILES: usize = 1;
const TOWN_EAST_TO_ROUTE_COLUMN_TILES: usize = 6;
const TOWN_NORTH_TO_ROUTE_EDGE_TILES: usize = 9;
const CROSS_ROUTE_EDGE_TO_RESCUE_TRIGGER_TILES: usize = 1;
const ROUTE_TRIGGER_LANDING_FRAMES: usize = 1;
const FIRST_BATTLE_DRIVER_BUTTONS: AppButtons = AppButtons::RIGHT;
const REAL_PACK_FIRST_BATTLE_FRAMES_AFTER_LANDING: usize = 2;
const FIRST_BATTLE_CONCLUDING_FRAMES: usize = 1;
const FINAL_RELEASE_FRAMES: usize = 1;

#[cfg(test)]
const ROUTE_WALK_TILES: usize = STEP_OFF_BEDROOM_STAIR_WARP_TILES
    + REENTER_BEDROOM_STAIR_WARP_TILES
    + HOUSE_HALL_TO_FRONT_DOOR_TILES
    + CLEAR_TOWN_DOOR_LANDING_TILES
    + TOWN_EAST_TO_ROUTE_COLUMN_TILES
    + TOWN_NORTH_TO_ROUTE_EDGE_TILES
    + CROSS_ROUTE_EDGE_TO_RESCUE_TRIGGER_TILES;
#[cfg(test)]
const FIRST_BATTLE_STATE_FRAMES: usize =
    ROUTE_TRIGGER_LANDING_FRAMES + REAL_PACK_FIRST_BATTLE_FRAMES_AFTER_LANDING;

#[derive(Debug, Clone, Copy)]
enum ScenarioBlock {
    Segment(Segment),
    IntroTraversal,
}

const fn segment(buttons: AppButtons, count: usize, expected: AppState) -> Segment {
    Segment {
        buttons,
        count,
        expected,
    }
}

const fn held(buttons: AppButtons, count: usize, expected: AppState) -> ScenarioBlock {
    ScenarioBlock::Segment(segment(buttons, count, expected))
}

const fn walk(direction: AppButtons, tiles: usize, expected: AppState) -> ScenarioBlock {
    held(direction, WALK_FRAMES_PER_TILE * tiles, expected)
}

const SEGMENTS: &[ScenarioBlock] = &[
    held(
        AppButtons::START,
        BUTTON_EDGE_FRAMES,
        AppState::MainMenu(MainMenuItem::NewGame),
    ),
    held(AppButtons::A, BUTTON_EDGE_FRAMES, AppState::Intro),
    ScenarioBlock::IntroTraversal,
    walk(
        AppButtons::DOWN,
        STEP_OFF_BEDROOM_STAIR_WARP_TILES,
        AppState::Overworld,
    ),
    walk(
        AppButtons::UP,
        REENTER_BEDROOM_STAIR_WARP_TILES,
        AppState::Overworld,
    ),
    walk(
        AppButtons::DOWN,
        HOUSE_HALL_TO_FRONT_DOOR_TILES,
        AppState::Overworld,
    ),
    walk(
        AppButtons::DOWN,
        CLEAR_TOWN_DOOR_LANDING_TILES,
        AppState::Overworld,
    ),
    walk(
        AppButtons::RIGHT,
        TOWN_EAST_TO_ROUTE_COLUMN_TILES,
        AppState::Overworld,
    ),
    walk(
        AppButtons::UP,
        TOWN_NORTH_TO_ROUTE_EDGE_TILES,
        AppState::Overworld,
    ),
    held(
        AppButtons::UP,
        WALK_FRAMES_PER_TILE * CROSS_ROUTE_EDGE_TO_RESCUE_TRIGGER_TILES
            - ROUTE_TRIGGER_LANDING_FRAMES,
        AppState::Overworld,
    ),
    held(
        AppButtons::UP,
        ROUTE_TRIGGER_LANDING_FRAMES,
        AppState::FirstBattle,
    ),
    held(
        FIRST_BATTLE_DRIVER_BUTTONS,
        REAL_PACK_FIRST_BATTLE_FRAMES_AFTER_LANDING,
        AppState::FirstBattle,
    ),
    held(
        FIRST_BATTLE_DRIVER_BUTTONS,
        FIRST_BATTLE_CONCLUDING_FRAMES,
        AppState::Overworld,
    ),
    held(AppButtons::NONE, FINAL_RELEASE_FRAMES, AppState::Overworld),
];

fn intro_confirm_button(run_index: usize) -> AppButtons {
    if B_CONFIRM_RUN_INDICES.contains(&run_index) {
        AppButtons::B
    } else {
        AppButtons::A
    }
}

fn append_intro_traversal(segments: &mut Vec<Segment>) {
    let (handoff_run, intro_runs) = pokeemerald_rs::intro::TRAVERSAL_RUNS
        .split_last()
        .expect("the intro traversal includes its overworld handoff");

    for (run_index, run) in intro_runs.iter().enumerate() {
        segments.push(segment(
            AppButtons::NONE,
            run.frames as usize,
            AppState::Intro,
        ));
        if run.confirm_after {
            segments.push(segment(
                intro_confirm_button(run_index),
                BUTTON_EDGE_FRAMES,
                AppState::Intro,
            ));
        }
    }

    assert!(
        !handoff_run.confirm_after,
        "the intro handoff needs no input"
    );
    let intro_frames = (handoff_run.frames as usize)
        .checked_sub(INTRO_HANDOFF_FRAMES)
        .expect("the intro handoff run includes its transition frame");
    segments.push(segment(AppButtons::NONE, intro_frames, AppState::Intro));
    segments.push(segment(
        AppButtons::NONE,
        INTRO_HANDOFF_FRAMES,
        AppState::Overworld,
    ));
}

fn scenario_segments() -> Vec<Segment> {
    let mut segments = Vec::new();
    for block in SEGMENTS {
        match block {
            ScenarioBlock::Segment(segment) => segments.push(*segment),
            ScenarioBlock::IntroTraversal => append_intro_traversal(&mut segments),
        }
    }
    segments
}

pub(super) fn frames() -> &'static [ScenarioFrame] {
    static FRAMES: OnceLock<Vec<ScenarioFrame>> = OnceLock::new();
    FRAMES.get_or_init(|| expand_segments(&scenario_segments()))
}

#[cfg(test)]
mod tests {
    use crate::scenario::{spec, WALK_FRAMES_PER_TILE};
    use crate::ScenarioName;
    use pokeemerald_rs::main_menu::MainMenuItem;
    use pokeemerald_rs::{AppButtons, AppState};

    use super::{
        B_CONFIRM_RUN_INDICES, FINAL_RELEASE_FRAMES, FIRST_BATTLE_CONCLUDING_FRAMES,
        FIRST_BATTLE_STATE_FRAMES, REAL_PACK_FIRST_BATTLE_FRAMES_AFTER_LANDING, ROUTE_WALK_TILES,
        TITLE_AND_MENU_FRAMES,
    };

    #[test]
    fn script_has_the_expected_route_and_state_shape() {
        let frames = spec(ScenarioName::BootToFirstFight).frames;

        let expected_total = TITLE_AND_MENU_FRAMES
            + pokeemerald_rs::intro::TRAVERSAL_FRAMES
            + WALK_FRAMES_PER_TILE * ROUTE_WALK_TILES
            + REAL_PACK_FIRST_BATTLE_FRAMES_AFTER_LANDING
            + FIRST_BATTLE_CONCLUDING_FRAMES
            + FINAL_RELEASE_FRAMES;
        assert_eq!(frames.len(), expected_total);

        assert_eq!(frames[0].buttons, AppButtons::START);
        assert_eq!(
            frames[0].expected,
            AppState::MainMenu(MainMenuItem::NewGame)
        );
        assert_eq!(frames[1].buttons, AppButtons::A);
        assert_eq!(frames[1].expected, AppState::Intro);

        let intro_start = TITLE_AND_MENU_FRAMES;
        let intro_end = intro_start + pokeemerald_rs::intro::TRAVERSAL_FRAMES;
        for (offset, frame) in frames[intro_start..intro_end - 1].iter().enumerate() {
            assert_eq!(
                frame.expected,
                AppState::Intro,
                "intro frame {offset} (script index {}) must still be Intro",
                intro_start + offset
            );
        }
        assert_eq!(
            frames[intro_end - 1].expected,
            AppState::Overworld,
            "the intro handoff frame must enter the overworld"
        );
        let b_presses_in_intro = frames[intro_start..intro_end]
            .iter()
            .filter(|frame| frame.buttons == AppButtons::B)
            .count();
        assert_eq!(
            b_presses_in_intro,
            B_CONFIRM_RUN_INDICES.len(),
            "both B confirmations must reach the app"
        );

        let first_battle_frames = frames
            .iter()
            .filter(|frame| frame.expected == AppState::FirstBattle)
            .count();
        assert_eq!(first_battle_frames, FIRST_BATTLE_STATE_FRAMES);

        let last = frames.last().expect("the script is non-empty");
        assert_eq!(last.expected, AppState::Overworld);
        assert_eq!(last.buttons, AppButtons::NONE);
    }

    #[test]
    fn intro_segments_follow_the_engine_traversal_runs() {
        let frames = spec(ScenarioName::BootToFirstFight).frames;
        let mut frame_index = TITLE_AND_MENU_FRAMES;

        for (run_index, run) in pokeemerald_rs::intro::TRAVERSAL_RUNS.iter().enumerate() {
            for frame_in_run in 0..run.frames as usize {
                assert_eq!(
                    frames[frame_index + frame_in_run].buttons,
                    AppButtons::NONE,
                    "run {run_index}: frame {frame_in_run} of {} must be released",
                    run.frames
                );
            }
            frame_index += run.frames as usize;

            if run.confirm_after {
                let buttons = frames[frame_index].buttons;
                assert!(
                    buttons == AppButtons::A || buttons == AppButtons::B,
                    "run {run_index} ends on a wait: script index {frame_index} must press A or B, \
                     not {buttons:?}"
                );
                frame_index += 1;
            } else {
                assert_ne!(
                    frames.get(frame_index).map(|frame| frame.buttons),
                    Some(AppButtons::A),
                    "run {run_index} needs no confirmation"
                );
            }
        }

        assert_eq!(
            frame_index,
            TITLE_AND_MENU_FRAMES + pokeemerald_rs::intro::TRAVERSAL_FRAMES
        );
    }

    #[test]
    #[cfg(feature = "scenario")]
    #[ignore = "needs a local pack produced by `cargo xtask extract`"]
    fn real_pack_boot_to_first_fight_passes_and_reaches_every_milestone_in_order() {
        let _pack = crate::extract::REAL_PACK_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let report = crate::scenario::run(ScenarioName::BootToFirstFight)
            .expect("boot-to-first-fight should pass against the real pack");
        assert_eq!(
            report.frames_run,
            spec(ScenarioName::BootToFirstFight).frames.len()
        );
        assert_eq!(
            report.milestones,
            vec![
                AppState::Title,
                AppState::MainMenu(MainMenuItem::NewGame),
                AppState::Intro,
                AppState::Overworld,
                AppState::FirstBattle,
                AppState::Overworld,
            ]
        );
        assert_eq!(
            report.first_battle_outcome,
            Some(pokeemerald_rs::BattleOutcome::PlayerWon),
            "the scenario must prove a terminal win, not only an emptied battle slot"
        );
    }
}
