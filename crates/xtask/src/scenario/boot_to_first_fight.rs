//! `boot-to-first-fight`'s script (I-7, issue #245): title -> the no-save
//! main menu -> Birch's intro, read page by page (issue #393 deleted the
//! pre-1.0 whole-intro B-skip this script used to take a shortcut through)
//! -> the protagonist's own bedroom -> down the stairs -> through the
//! house -> out onto Littleroot Town -> north onto Route 101's real rescue
//! coord-event trigger tile -> `BATTLE_TYPE_FIRST_BATTLE` -> a concluded
//! battle.
//!
//! # The intro traversal (issue #393)
//!
//! [`SEGMENTS`]' own intro block reads every one of Birch's eight speech
//! pages exactly as a player would: release-then-press frames at every
//! internal `\p`/`\l` wait (twenty-four of them across the eight pages --
//! `pokeemerald_rs::intro::speech`'s own module docs), never a held button,
//! so none of `engine::text::render::Printer`'s held-A/B print speed-up
//! (issue #393's other half) ever engages -- every `NONE`-held wait below
//! is therefore the *exact* number of frames `TextSpeed::Mid` takes to
//! print up to that wait, not a padded guess. Two of the twenty-four
//! presses use B instead of A (one `\p`, one `\l`), proving both buttons
//! reach the game through the real `App`/[`super::ScenarioDriver`] path,
//! not just [`pokeemerald_rs::intro::IntroScene`]'s own headless tests.
//!
//! These counts were measured, not derived by hand: a scratch harness drove
//! the real `pokeemerald_rs::intro::speech::pages()` token streams through
//! `engine::text::render::Printer` at `TextSpeed::Mid` (`IntroScene::from_pack`'s
//! own default) with a synthetic glyph sheet -- the same fixture shape
//! `crate::intro::tests` already uses, and pixel-content-independent, since
//! only real advance-width/frame-timing metadata (not sheet pixels) affects
//! *when* a wait is reached. Re-derive by temporarily adding an equivalent
//! harness if `pokeemerald_rs::intro::speech`'s authored pages ever change;
//! see [`super::ScenarioError::Milestone`]'s own doc comment on this
//! script's general "fails closed, re-derive the budget" philosophy.
//!
//! Every page's own last-token-before-`Token::End` shape (`...{P}` --
//! `pokeemerald_rs::intro::speech`'s "every_question_page_waits_for_a_press_before_advancing"
//! test) means the four ticks after a page's *final* prompt is confirmed
//! are identical across every page: three reveal-delay-drain ticks the
//! `\p` reloaded, then the tick that actually consumes `Token::End`. For
//! pages 0-6 that fourth tick only fires `IntroScene::advance_page` (still
//! `AppState::Intro`); for page 7 (`ARE_YOU_READY`, the last page) that
//! same fourth tick is the one where `IntroStatus::Finished` hands off to
//! the overworld *within* that frame's own `App::step` --
//! `pokeemerald_rs::flow::advance_scene`'s `Intro` arm transitions
//! `AppScene` the instant it sees `Finished`, so [`super::run`]'s own
//! post-step state read (`scenario.rs`'s `run_with_driver`) already
//! reports `AppState::Overworld` on that exact frame, not one frame later
//! -- hence the split `3` (`Intro`) `+ 1` (`Overworld`) at the very end of
//! the intro block below, instead of a uniform `4`.
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
    // Read Birch's entire eight-page speech (module docs' "The intro
    // traversal" section) -- release-then-press at every internal `\p`/`\l`
    // wait, landing in the bedroom on the final page's own terminator tick,
    // facing `pokeemerald_rs::new_game::SPAWN_FACING` (south).
    // --- Birch speech page 0: WELCOME ---
    Segment {
        buttons: AppButtons::NONE,
        count: 121,
        expected: AppState::Intro,
    },
    Segment {
        buttons: AppButtons::B,
        count: 1,
        expected: AppState::Intro,
    }, // page 0 prompt 1 (CLEAR)
    Segment {
        buttons: AppButtons::NONE,
        count: 132,
        expected: AppState::Intro,
    },
    Segment {
        buttons: AppButtons::A,
        count: 1,
        expected: AppState::Intro,
    }, // page 0 prompt 2 (CLEAR)
    Segment {
        buttons: AppButtons::NONE,
        count: 72,
        expected: AppState::Intro,
    },
    Segment {
        buttons: AppButtons::A,
        count: 1,
        expected: AppState::Intro,
    }, // page 0 prompt 3 (CLEAR)
    Segment {
        buttons: AppButtons::NONE,
        count: 179,
        expected: AppState::Intro,
    },
    Segment {
        buttons: AppButtons::A,
        count: 1,
        expected: AppState::Intro,
    }, // page 0 prompt 4 (CLEAR)
    Segment {
        buttons: AppButtons::NONE,
        count: 4,
        expected: AppState::Intro,
    }, // page 0 -> page 1 (reveal-delay drain then the terminator)
    // --- Birch speech page 1: THIS_IS_A_POKEMON ---
    Segment {
        buttons: AppButtons::NONE,
        count: 230,
        expected: AppState::Intro,
    },
    Segment {
        buttons: AppButtons::A,
        count: 1,
        expected: AppState::Intro,
    }, // page 1 prompt 1 (CLEAR)
    Segment {
        buttons: AppButtons::NONE,
        count: 4,
        expected: AppState::Intro,
    }, // page 1 -> page 2 (reveal-delay drain then the terminator)
    // --- Birch speech page 2: MAIN_SPEECH ---
    Segment {
        buttons: AppButtons::NONE,
        count: 244,
        expected: AppState::Intro,
    },
    Segment {
        buttons: AppButtons::A,
        count: 1,
        expected: AppState::Intro,
    }, // page 2 prompt 1 (CLEAR)
    Segment {
        buttons: AppButtons::NONE,
        count: 279,
        expected: AppState::Intro,
    },
    Segment {
        buttons: AppButtons::B,
        count: 1,
        expected: AppState::Intro,
    }, // page 2 prompt 2 (SCROLL)
    Segment {
        buttons: AppButtons::NONE,
        count: 9,
        expected: AppState::Intro,
    }, // scroll-animation drain, no input needed
    Segment {
        buttons: AppButtons::NONE,
        count: 140,
        expected: AppState::Intro,
    },
    Segment {
        buttons: AppButtons::A,
        count: 1,
        expected: AppState::Intro,
    }, // page 2 prompt 3 (CLEAR)
    Segment {
        buttons: AppButtons::NONE,
        count: 235,
        expected: AppState::Intro,
    },
    Segment {
        buttons: AppButtons::A,
        count: 1,
        expected: AppState::Intro,
    }, // page 2 prompt 4 (CLEAR)
    Segment {
        buttons: AppButtons::NONE,
        count: 267,
        expected: AppState::Intro,
    },
    Segment {
        buttons: AppButtons::A,
        count: 1,
        expected: AppState::Intro,
    }, // page 2 prompt 5 (CLEAR)
    Segment {
        buttons: AppButtons::NONE,
        count: 235,
        expected: AppState::Intro,
    },
    Segment {
        buttons: AppButtons::A,
        count: 1,
        expected: AppState::Intro,
    }, // page 2 prompt 6 (CLEAR)
    Segment {
        buttons: AppButtons::NONE,
        count: 247,
        expected: AppState::Intro,
    },
    Segment {
        buttons: AppButtons::A,
        count: 1,
        expected: AppState::Intro,
    }, // page 2 prompt 7 (SCROLL)
    Segment {
        buttons: AppButtons::NONE,
        count: 9,
        expected: AppState::Intro,
    }, // scroll-animation drain, no input needed
    Segment {
        buttons: AppButtons::NONE,
        count: 72,
        expected: AppState::Intro,
    },
    Segment {
        buttons: AppButtons::A,
        count: 1,
        expected: AppState::Intro,
    }, // page 2 prompt 8 (CLEAR)
    Segment {
        buttons: AppButtons::NONE,
        count: 4,
        expected: AppState::Intro,
    }, // page 2 -> page 3 (reveal-delay drain then the terminator)
    // --- Birch speech page 3: AND_YOU_ARE ---
    Segment {
        buttons: AppButtons::NONE,
        count: 49,
        expected: AppState::Intro,
    },
    Segment {
        buttons: AppButtons::A,
        count: 1,
        expected: AppState::Intro,
    }, // page 3 prompt 1 (CLEAR)
    Segment {
        buttons: AppButtons::NONE,
        count: 4,
        expected: AppState::Intro,
    }, // page 3 -> page 4 (reveal-delay drain then the terminator)
    // --- Birch speech page 4: WHATS_YOUR_NAME ---
    Segment {
        buttons: AppButtons::NONE,
        count: 112,
        expected: AppState::Intro,
    },
    Segment {
        buttons: AppButtons::A,
        count: 1,
        expected: AppState::Intro,
    }, // page 4 prompt 1 (CLEAR)
    Segment {
        buttons: AppButtons::NONE,
        count: 4,
        expected: AppState::Intro,
    }, // page 4 -> page 5 (reveal-delay drain then the terminator)
    // --- Birch speech page 5: so_its_player ---
    Segment {
        buttons: AppButtons::NONE,
        count: 49,
        expected: AppState::Intro,
    },
    Segment {
        buttons: AppButtons::A,
        count: 1,
        expected: AppState::Intro,
    }, // page 5 prompt 1 (CLEAR)
    Segment {
        buttons: AppButtons::NONE,
        count: 4,
        expected: AppState::Intro,
    }, // page 5 -> page 6 (reveal-delay drain then the terminator)
    // --- Birch speech page 6: youre_player ---
    Segment {
        buttons: AppButtons::NONE,
        count: 37,
        expected: AppState::Intro,
    },
    Segment {
        buttons: AppButtons::A,
        count: 1,
        expected: AppState::Intro,
    }, // page 6 prompt 1 (CLEAR)
    Segment {
        buttons: AppButtons::NONE,
        count: 215,
        expected: AppState::Intro,
    },
    Segment {
        buttons: AppButtons::A,
        count: 1,
        expected: AppState::Intro,
    }, // page 6 prompt 2 (SCROLL)
    Segment {
        buttons: AppButtons::NONE,
        count: 9,
        expected: AppState::Intro,
    }, // scroll-animation drain, no input needed
    Segment {
        buttons: AppButtons::NONE,
        count: 56,
        expected: AppState::Intro,
    },
    Segment {
        buttons: AppButtons::A,
        count: 1,
        expected: AppState::Intro,
    }, // page 6 prompt 3 (CLEAR)
    Segment {
        buttons: AppButtons::NONE,
        count: 4,
        expected: AppState::Intro,
    }, // page 6 -> page 7 (reveal-delay drain then the terminator)
    // --- Birch speech page 7: ARE_YOU_READY ---
    Segment {
        buttons: AppButtons::NONE,
        count: 101,
        expected: AppState::Intro,
    },
    Segment {
        buttons: AppButtons::A,
        count: 1,
        expected: AppState::Intro,
    }, // page 7 prompt 1 (CLEAR)
    Segment {
        buttons: AppButtons::NONE,
        count: 175,
        expected: AppState::Intro,
    },
    Segment {
        buttons: AppButtons::A,
        count: 1,
        expected: AppState::Intro,
    }, // page 7 prompt 2 (CLEAR)
    Segment {
        buttons: AppButtons::NONE,
        count: 251,
        expected: AppState::Intro,
    },
    Segment {
        buttons: AppButtons::A,
        count: 1,
        expected: AppState::Intro,
    }, // page 7 prompt 3 (SCROLL)
    Segment {
        buttons: AppButtons::NONE,
        count: 9,
        expected: AppState::Intro,
    }, // scroll-animation drain, no input needed
    Segment {
        buttons: AppButtons::NONE,
        count: 136,
        expected: AppState::Intro,
    },
    Segment {
        buttons: AppButtons::A,
        count: 1,
        expected: AppState::Intro,
    }, // page 7 prompt 4 (CLEAR)
    Segment {
        buttons: AppButtons::NONE,
        count: 263,
        expected: AppState::Intro,
    },
    Segment {
        buttons: AppButtons::A,
        count: 1,
        expected: AppState::Intro,
    }, // page 7 prompt 5 (CLEAR)
    Segment {
        buttons: AppButtons::NONE,
        count: 3,
        expected: AppState::Intro,
    }, // trailing reveal-delay drain -- still Intro
    Segment {
        buttons: AppButtons::NONE,
        count: 1,
        expected: AppState::Overworld,
    }, // the terminator tick: IntroStatus::Finished hands off to the overworld
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

    /// The intro traversal's own total frame count (module docs' "The
    /// intro traversal" section): twenty-four release-then-press pairs (two
    /// of them B, the rest A), two nine-frame scroll drains, and the final
    /// page's split `3 + 1` terminator tail -- summed here from the exact
    /// same measured per-prompt counts [`super::SEGMENTS`]' own intro block
    /// is built from, so a future re-measurement only has to update one
    /// side of this comparison for `boot_to_first_fight_script_has_the_expected_shape`
    /// to catch a drift.
    const INTRO_TRAVERSAL_FRAMES: usize = 121 + 1 // page 0 prompt 1 (B)
        + 132 + 1 // page 0 prompt 2
        + 72 + 1 // page 0 prompt 3
        + 179 + 1 // page 0 prompt 4
        + 4 // page 0 -> page 1
        + 230 + 1 // page 1 prompt 1
        + 4 // page 1 -> page 2
        + 244 + 1 // page 2 prompt 1
        + 279 + 1 + 9 // page 2 prompt 2 (B, scroll)
        + 140 + 1 // page 2 prompt 3
        + 235 + 1 // page 2 prompt 4
        + 267 + 1 // page 2 prompt 5
        + 235 + 1 // page 2 prompt 6
        + 247 + 1 + 9 // page 2 prompt 7 (scroll)
        + 72 + 1 // page 2 prompt 8
        + 4 // page 2 -> page 3
        + 49 + 1 // page 3 prompt 1
        + 4 // page 3 -> page 4
        + 112 + 1 // page 4 prompt 1
        + 4 // page 4 -> page 5
        + 49 + 1 // page 5 prompt 1
        + 4 // page 5 -> page 6
        + 37 + 1 // page 6 prompt 1
        + 215 + 1 + 9 // page 6 prompt 2 (scroll)
        + 56 + 1 // page 6 prompt 3
        + 4 // page 6 -> page 7
        + 101 + 1 // page 7 prompt 1
        + 175 + 1 // page 7 prompt 2
        + 251 + 1 + 9 // page 7 prompt 3 (scroll)
        + 136 + 1 // page 7 prompt 4
        + 263 + 1 // page 7 prompt 5
        + 3 + 1; // trailing reveal-delay drain, then the Overworld handoff

    /// Pack-free shape assertions on the authored script itself: the total
    /// frame count [`super::SEGMENTS`] adds up to, the opening title ->
    /// menu -> intro pages -> overworld handoff, and that exactly one
    /// landing frame plus two driven turns report `FirstBattle` before the
    /// concluding frame drops back to `Overworld` -- the same three-frame
    /// battle budget [`super::SEGMENTS`]' own doc comment pins
    /// empirically. Guards the script's own self-consistency without a
    /// pack; the real-pack ignored test below is the actual behavioural
    /// proof.
    #[test]
    fn boot_to_first_fight_script_has_the_expected_shape() {
        let frames = spec(ScenarioName::BootToFirstFight).frames;

        let expected_total = 2 // start, confirm
            + INTRO_TRAVERSAL_FRAMES
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

        // Issue #393: the intro no longer finishes in one B press -- every
        // frame of the whole traversal but its very last must stay
        // `Intro`, and B must appear at least once (proving it reaches the
        // real `App`, not just `IntroScene`'s own headless tests).
        let intro_start = 2;
        let intro_end = intro_start + INTRO_TRAVERSAL_FRAMES;
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
            "the intro's own terminator tick must hand off to the overworld"
        );
        let b_presses_in_intro = frames[intro_start..intro_end]
            .iter()
            .filter(|frame| frame.buttons == AppButtons::B)
            .count();
        assert_eq!(
            b_presses_in_intro, 2,
            "B must advance a page exactly like A -- issue #393's own point"
        );

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
