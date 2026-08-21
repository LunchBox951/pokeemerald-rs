//! `boot-to-first-fight`'s script (I-7, issue #245): title -> the no-save
//! main menu -> Birch's intro (skipped) -> the protagonist's own bedroom ->
//! down the stairs -> through the house -> out onto Littleroot Town ->
//! north onto Route 101's real rescue coord-event trigger tile ->
//! `BATTLE_TYPE_FIRST_BATTLE` -> a concluded battle.
//!
//! Split out of `super` (`crate::scenario`) into its own file purely to
//! keep that module under the `oop-boundaries` size guideline -- this is
//! still one concept with the rest of the runner, just one specific
//! scenario's data rather than the shared driver/spec machinery
//! ([`super::Segment`]/[`super::expand_segments`]/[`super::ScenarioSpec`])
//! every scenario shares.
//!
//! # Milestones and the NPC-dialog limitation
//!
//! [`AppState`] is the only vocabulary a [`super::ScenarioDriver`] has, and
//! its own doc comment was written with exactly this scenario in mind
//! ("lets I-7's `boot-to-first-fight` scenario prove it reached the
//! scripted fight rather than merely Route 101"). Issue #245's narrative
//! scope names six milestones -- title reached, save menu answered,
//! bedroom entered, first NPC dialog, Route 101 rescue trigger, first
//! battle started, battle concluded -- but [`AppState`] only
//! distinguishes five of those: `Title`, `MainMenu`, `Intro`, `Overworld`
//! (which "bedroom entered", "first NPC dialog", and "Route 101 rescue
//! trigger" all fold into -- none of them changes the coarse flow state),
//! `FirstBattle`, and `Overworld` again on the concluding frame. The
//! honest result, asserted by this module's own tests: `[Title,
//! MainMenu(NewGame), Intro, Overworld, FirstBattle, Overworld]`.
//!
//! **No NPC-dialog milestone.** A deliberate scope split, not a missing
//! subsystem: the NPC dialog engine exists and is live in the exact flow
//! this scenario drives (`pokeemerald_rs::overworld::dialog`'s `NpcDialog`,
//! held as `OverworldPhase`'s `dialog` field and opened by the A-press
//! interaction path -- issue #161), but [`AppState`]'s five-variant
//! vocabulary has no dialog variant and no `App` accessor exposes
//! dialog-open to a [`super::ScenarioDriver`] yet. "First NPC interaction"
//! is I-3's acceptance criterion (`docs/acceptance/v1.md`), not I-7's, and
//! keeps its own committed real-pack coverage
//! (`flow/overworld_phase`'s
//! `walking_downstairs_and_talking_to_mom_opens_and_closes_her_dialog`), so
//! omitting the milestone here narrows this scenario to I-7's own criterion
//! text rather than leaving Mom's dialog unguarded. When an I-3 slice wants
//! the milestone asserted end-to-end, the retained-outcome accessor pattern
//! below (`App::first_battle_outcome`) is the template for exposing it.
//!
//! The concluding frame's `AppState::Overworld` still says only that the
//! battle slot emptied, but the runner now pairs it with the retained
//! `pokeemerald_rs::BattleOutcome`. This scenario requires that channel to
//! be populated on the `FirstBattle` -> `Overworld` edge, so the identical
//! state transition produced by an aborted battle fails closed.
//!
//! # Issue #251: the concluding frame now stands in Birch's lab, unbudgeted
//!
//! `pokeemerald_rs::flow::overworld_phase::first_battle_conclusion::OverworldPhase::conclude_first_battle`
//! now runs on the very same frame the battle empties its slot -- healing
//! the party, writing the Birch's-bag vars, and warping the player to
//! `MAP_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB` -- so by the time this
//! script's own final segments run, the player already stands in the lab,
//! not on Route 101's grass. `AppState` carries no map identity at all, so
//! this changes nothing about the script's own shape (`SEGMENTS`, the frame
//! budget below) -- confirmed by re-running this scenario against the real
//! pack while authoring issue #251, unchanged.
//!
//! This script is **not** extended to walk the newly-reachable lab
//! interior. `Route101_EventScript_BirchsBag`'s unmodelled dressing --
//! Birch's thank-you dialog (`scripts.inc:231`) and the gender-conditional
//! bedroom-hide calls (`:240-241`), both of which upstream runs *before*
//! the warp at `:242` -- has no script-engine counterpart yet
//! (`first_battle_conclusion`'s own module docs, "What's deliberately
//! deferred"), so there is no honestly-modelled next milestone to walk
//! toward; extending the script here would only prove the lab's own empty
//! floor is walkable, which is not this scenario's acceptance criterion.
//!
//! # The route, tile by tile
//!
//! Traced empirically against the real pack while authoring this scenario
//! (this module's own real-pack ignored test is the standing proof it
//! stays true): the bedroom's spawn tile
//! (`pokeemerald_rs::new_game::SPAWN_POSITION`) is the stair warp itself,
//! so the walk steps off it and back on to fire it; six tiles down
//! `MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F`'s own front hall reach its door
//! warp; one more tile clears the landing's fencing on Littleroot Town's
//! side; six tiles east and nine north reach the map's own last interior
//! row at the real walkable `x` column
//! (`pokeemerald_rs::flow::overworld_phase::connections_tests::walking_off_littlerootss_north_edge_crosses_into_route_101_and_back`'s
//! own doc comment already pins `x` in `{10, 11}`); the tenth north tile
//! crosses Littleroot's own north edge (`offset: 0` both ways) and lands
//! exactly on Route 101's real rescue coord-event trigger
//! (`pokeemerald_rs::flow::overworld_phase::first_battle_trigger`'s module
//! docs).

use std::sync::OnceLock;

use pokeemerald_rs::main_menu::MainMenuItem;
use pokeemerald_rs::{AppButtons, AppState};

use super::{expand_segments, ScenarioFrame, Segment, WALK_FRAMES_PER_TILE};

const SEGMENTS: &[Segment] = &[
    // Title -> the no-save main menu, same first frame as `BOOT_TO_MAIN_MENU`.
    Segment {
        buttons: AppButtons::START,
        count: 1,
        expected: AppState::MainMenu(MainMenuItem::NewGame),
    },
    // Confirm NEW GAME -> Birch's intro
    // (`pokeemerald_rs::flow::advance_scene`'s `MainMenu` arm,
    // `MainMenuAction::NewGame`).
    Segment {
        buttons: AppButtons::A,
        count: 1,
        expected: AppState::Intro,
    },
    // Skip the whole intro in one press
    // (`pokeemerald_rs::intro::IntroScene::tick`'s `skip_pressed` arm) --
    // lands in the bedroom the same frame, facing
    // `pokeemerald_rs::new_game::SPAWN_FACING` (south).
    Segment {
        buttons: AppButtons::B,
        count: 1,
        expected: AppState::Overworld,
    },
    // The spawn tile is the stair warp itself, so arriving there triggers
    // nothing -- only *walking onto* it does. Step off it south, then
    // back north: the second step's own landing frame fires the warp to
    // `MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F`, arriving at `(8, 2)`.
    Segment {
        buttons: AppButtons::DOWN,
        count: WALK_FRAMES_PER_TILE,
        expected: AppState::Overworld,
    },
    Segment {
        buttons: AppButtons::UP,
        count: WALK_FRAMES_PER_TILE,
        expected: AppState::Overworld,
    },
    // Straight down the stairwell hall to the front door: six tiles from
    // `(8, 2)` to the door's own warp tile at `(8, 8)`, which warps to
    // `MAP_LITTLEROOT_TOWN` at `(5, 8)` mid-hold.
    Segment {
        buttons: AppButtons::DOWN,
        count: WALK_FRAMES_PER_TILE * 6,
        expected: AppState::Overworld,
    },
    // The door's own Littleroot landing is fenced on every side but
    // south; one more tile clears it before turning.
    Segment {
        buttons: AppButtons::DOWN,
        count: WALK_FRAMES_PER_TILE,
        expected: AppState::Overworld,
    },
    // East to `x = 11`, inside the walkable `{10, 11}` column.
    Segment {
        buttons: AppButtons::RIGHT,
        count: WALK_FRAMES_PER_TILE * 6,
        expected: AppState::Overworld,
    },
    // North to the map's own last interior row, `y = 0`: nine tiles.
    Segment {
        buttons: AppButtons::UP,
        count: WALK_FRAMES_PER_TILE * 9,
        expected: AppState::Overworld,
    },
    // The tenth tile crosses Littleroot's own north edge into Route 101
    // and lands exactly on `(11, 19)`, one of the two real rescue
    // coord-event trigger tiles. `AppState`'s own doc comment: "the fight
    // is scheduled at the end of the `App::step` call whose movement
    // triggered it ... so that landing frame already reports ...
    // `AppState::FirstBattle`" -- so this crossing's own walk animation
    // (fifteen frames, still `Overworld`) is split from its own
    // sixteenth, landing frame (already `FirstBattle`).
    Segment {
        buttons: AppButtons::UP,
        count: WALK_FRAMES_PER_TILE - 1,
        expected: AppState::Overworld,
    },
    Segment {
        buttons: AppButtons::UP,
        count: 1,
        expected: AppState::FirstBattle,
    },
    // `pokeemerald_rs::flow::first_battle::advance_first_battle` always
    // chooses the lead's first move slot (that module's own docs):
    // against `pokeemerald_rs::new_game::provisional_starter`'s Treecko
    // and the scripted level-2 Zigzagoon opponent, the fight resolves in
    // exactly three `FirstBattle` frames -- the trigger's own landing
    // frame, plus the two driven here.
    //
    // That `3` is **empirically pinned, not derived**. It was measured by
    // running this scenario against the real pack, and it reproduces only
    // because the whole run is deterministic: one fixed script off
    // `pokeemerald_rs::new_game::NEW_GAME_RNG_SEED`, so the battle always
    // begins at one exact position in the new-game RNG stream. No test
    // anywhere asserts this three-frame count -- the closest,
    // `pokeemerald_rs::flow::overworld_phase::first_battle_trigger_tests::real_pack_crossing_into_route_101_lands_on_the_rescue_trigger_and_starts_the_battle`,
    // drives an unbudgeted up-to-500-frame loop and never checks how many
    // frames it took -- so no test elsewhere can stand in as the source of
    // this number.
    //
    // It is deliberately a hard budget. Any change to the battle driver's
    // per-turn RNG draw counts or damage rolls shifts the fight's length,
    // and the scenario fails closed on one of these frames with a
    // `super::ScenarioError::Milestone` mismatch (`FirstBattle` where
    // `Overworld` was expected, or the reverse) rather than silently
    // passing. When that happens, re-derive the budget by running the
    // scenario itself -- `cargo run -p xtask --features scenario --
    // scenario --name boot-to-first-fight` names the mismatching frame --
    // and adjust the counts below; the battle driver's own draw counts are
    // the moving part, nothing on the walk above.
    //
    // Which button is held here doesn't matter --
    // `OverworldPhase::advance_first_battle_frame` owns the whole frame
    // once a first battle is running -- `RIGHT` only mirrors the choice
    // made by
    // `pokeemerald_rs::flow::overworld_phase::first_battle_trigger_tests::real_pack_crossing_into_route_101_lands_on_the_rescue_trigger_and_starts_the_battle`,
    // which drives its own (unbudgeted, up-to-500-frame) loop the same way.
    Segment {
        buttons: AppButtons::RIGHT,
        count: 2,
        expected: AppState::FirstBattle,
    },
    // The turn that ends the battle also clears the battle slot the same
    // frame (`AppState`'s own doc comment: "the concluding frame reports
    // `AppState::Overworld` again").
    Segment {
        buttons: AppButtons::RIGHT,
        count: 1,
        expected: AppState::Overworld,
    },
    // Release everything on one final frame and prove the overworld stays
    // put, the same convention `super::BOOT_TO_MAIN_MENU` ends on ("prove
    // the menu remains stable; otherwise a later scenario could inherit a
    // held key and miss the next newly-pressed edge").
    Segment {
        buttons: AppButtons::NONE,
        count: 1,
        expected: AppState::Overworld,
    },
];

/// [`SEGMENTS`], expanded once and cached: [`super::spec`] is called on
/// every [`super::run`]/test invocation, and re-flattening a
/// several-hundred-frame script on every call would be wasted work for a
/// script that never changes at runtime.
pub(super) fn frames() -> &'static [ScenarioFrame] {
    static FRAMES: OnceLock<Vec<ScenarioFrame>> = OnceLock::new();
    FRAMES.get_or_init(|| expand_segments(SEGMENTS))
}

#[cfg(test)]
mod tests {
    use crate::scenario::{spec, WALK_FRAMES_PER_TILE};
    use crate::ScenarioName;
    use pokeemerald_rs::main_menu::MainMenuItem;
    use pokeemerald_rs::{AppButtons, AppState};

    /// Pack-free shape assertions on the authored script itself: the total
    /// frame count [`super::SEGMENTS`] adds up to, the opening title ->
    /// menu -> intro -> overworld handoff, and that exactly one landing
    /// frame plus two driven turns report `FirstBattle` before the
    /// concluding frame drops back to `Overworld` -- the same three-frame
    /// battle budget [`super::SEGMENTS`]' own doc comment pins
    /// empirically. Guards the script's own self-consistency without a
    /// pack; the real-pack ignored test below is the actual behavioural
    /// proof.
    #[test]
    fn boot_to_first_fight_script_has_the_expected_shape() {
        let frames = spec(ScenarioName::BootToFirstFight).frames;

        let expected_total = 3 // start, confirm, skip
            + WALK_FRAMES_PER_TILE * 2 // off the stairs, back onto them
            + WALK_FRAMES_PER_TILE * 6 // down to the front door
            + WALK_FRAMES_PER_TILE // clear the door's fencing
            + WALK_FRAMES_PER_TILE * 6 // east to the walkable column
            + WALK_FRAMES_PER_TILE * 10 // north to, and across, the edge
            + 3 // the battle: two driven turns plus the concluding frame
            + 1; // the trailing button-release frame
        assert_eq!(frames.len(), expected_total);

        assert_eq!(frames[0].buttons, AppButtons::START);
        assert_eq!(
            frames[0].expected,
            AppState::MainMenu(MainMenuItem::NewGame)
        );
        assert_eq!(frames[1].buttons, AppButtons::A);
        assert_eq!(frames[1].expected, AppState::Intro);
        assert_eq!(frames[2].buttons, AppButtons::B);
        assert_eq!(frames[2].expected, AppState::Overworld);

        let first_battle_frames = frames
            .iter()
            .filter(|frame| frame.expected == AppState::FirstBattle)
            .count();
        assert_eq!(
            first_battle_frames, 3,
            "the trigger's landing frame plus two more driven turns"
        );
        let last = frames.last().expect("the script is non-empty");
        assert_eq!(
            last.expected,
            AppState::Overworld,
            "the battle's concluding frame must report Overworld again"
        );
        assert_eq!(
            last.buttons,
            AppButtons::NONE,
            "the script ends on a released frame, like BOOT_TO_MAIN_MENU"
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
            "the scenario must prove a real terminal resolution, not only an emptied slot"
        );
    }
}
