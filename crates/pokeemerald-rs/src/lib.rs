//! `pokeemerald-rs` library surface (I-1 slice 1: boot shell; I-2, issue
//! #109: real title screen): the owned [`App`] type the binary crate's thin
//! `main` (`src/main.rs`) delegates to, plus the modules it is built from.
//!
//! Split into a library plus a thin binary so `xtask`'s headless `e2e
//! --suite smoke` run (F-3, V-1) can construct and drive [`App`] in-process
//! via [`App::new_headless`]/[`App::step`], without spawning the compiled
//! binary `(oop-boundaries)`.
//!
//! [`title`] decodes the real title screen from the local asset pack
//! (`assets::pack`, populated by `cargo xtask extract`); [`App::new`] uses
//! it, falling back to no scene at all (a clean, no-panic diagnostic) when
//! no pack has been extracted yet -- see [`title`]'s module docs.
//!
//! [`overworld`] (I-3, issue #126) composes the map viewport + player OBJ
//! presentation lane over the `engine` overworld runtime (S-5, PR #120) --
//! see its module docs.
//!
//! [`main_menu`], [`intro`], and [`new_game`] (I-3, issue #149) are the
//! connective tissue between the title screen and the overworld: [`App`]'s
//! real (windowed) path now drives title -> main menu -> intro -> overworld
//! as one state machine (see `app`'s module docs' "Game flow" section).
//! [`App::new_headless_real`], [`App::set_headless_buttons`], and
//! [`App::state`] expose that same state machine to deterministic xtask
//! scenarios without opening a separate transition path (F-3, issue #233).
//! [`new_game`] holds the pure new-game state (spawn position, default
//! player identity, fresh [`engine::save`] blocks) both [`main_menu`] and
//! [`intro`] ultimately hand off to.
//!
//! The overworld phase also owns this run's single `Random()` stream and
//! rolls a wild encounter on every completed step (I-4, issue #169); the
//! handoff from a rolled species/level to a real `battle::Battle` lives in
//! `flow::wild_encounter`, headless for now — there is no battle scene yet.
//!
//! [`game_save`](crate::game_save) closes the loop (I-6, issues
//! #214/#232): the overworld's live `SaveBlock1`/`SaveBlock2` are written
//! to a real save file when the player saves, and read back at boot so the
//! main menu can offer `CONTINUE` — upstream's
//! `LoadGameSave`/`TrySavingData` pair, over `engine::save`'s existing
//! sector serialization. The write is reached the way upstream reaches it:
//! `START` in the overworld opens the field
//! [`start_menu`](crate::start_menu), whose `SAVE` action runs
//! `src/start_menu.c`'s confirm/overwrite chain. [`party`](crate::party)
//! is the `gPlayerParty` <-> `SaveBlock1::playerParty` encoder that save
//! and continue are bracketed by, so a continued session fights with the
//! mon that was saved.
//!
//! [`start_first_battle`]/[`advance_first_battle`] (`flow::first_battle`,
//! issue #221) are the scripted `BATTLE_TYPE_FIRST_BATTLE` Zigzagoon fight's
//! own construction and headless driver — see that module's docs for the
//! full account. Since issue #231 they have a real production caller:
//! Route 101's rescue coord events (`Route101_EventScript_StartBirchRescue`,
//! tiles (10,19)/(11,19), gated on `VAR_ROUTE101_STATE`), recognized by
//! `flow::overworld_phase`'s `first_battle_trigger` on the same
//! `OverworldPhase::step` path `App`'s game-flow state machine drives — so
//! walking Route 101's grass in real play reaches the fight. Still not
//! modelled around it: the rescue cutscene, Birch's bag and the
//! starter-choose UI, and the `B_TRANSITION_BLUR` intro (the trigger
//! module's docs carry the full deferral list). The crate-root re-exports
//! back that hookup for `xtask`'s `boot-to-first-fight` scenario (I-7,
//! issue #245), which drives [`App`] through this exact chain -- title,
//! new game, the protagonist's room, Route 101, the scripted fight, and the
//! frame that concluded battle empties the slot on -- headlessly. That
//! scenario reads coarse [`AppState`] milestones only, so it shows the
//! fight ran and ended, not what it resolved to -- and no test pins that
//! outcome either: `flow::overworld_phase`'s own tests drive the fight to
//! conclusion through the real per-frame driver, then assert the frozen
//! overworld and the lead handed back, never the `BattleOutcome` itself.
//!
//! [`music`] (S-3, issue #185) bridges the asset pack's song/voicegroup/
//! sample entries into the `audio` crate's sequencer and owns the
//! frame-driven [`music::MusicPlayer`] `App::step` ticks while the title
//! screen is showing -- see that module's docs for the resolution shape and
//! Discussion #227's owner decision on why playback is frame-driven rather
//! than a background thread.

pub mod app;
mod flow;
pub mod frame;
mod game_save;
pub mod intro;
pub mod main_menu;
pub mod music;
pub mod new_game;
pub mod overworld;
mod party;
pub mod scene;
mod start_menu;
mod textbox;
pub mod title;

pub use app::{App, AppState};
pub use flow::first_battle::{
    advance_first_battle, start_first_battle, FIRST_BATTLE_OPPONENT_LEVEL,
    FIRST_BATTLE_OPPONENT_SPECIES,
};
pub use platform::Buttons as AppButtons;

/// Real-pack pinning tests for extraction pipelines that don't yet have a
/// runtime consumer of their own in this crate (currently just
/// `xtask::extract::voicegroups`, S-4 issue #182) -- see that module's own
/// docs for why these live here rather than in `crates/xtask`/`crates/assets`
/// directly: when they were placed, this was the one crate whose `#[ignore]`d
/// real-pack tests CI ran (`.github/workflows/ci.yml` now gates the
/// `pokeemerald-rs`, `assets`, and `xtask` ignored lanes), and it already
/// depends on `assets` to decode the pack's typed entries.
#[cfg(test)]
mod voicegroup_pack_tests;
