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
//! [`new_game`] holds the pure new-game state (spawn position, default
//! player identity, fresh [`engine::save`] blocks) both [`main_menu`] and
//! [`intro`] ultimately hand off to.
//!
//! The overworld phase also owns this run's single `Random()` stream and
//! rolls a wild encounter on every completed step (I-4, issue #169); the
//! handoff from a rolled species/level to a real `battle::Battle` lives in
//! `flow::wild_encounter`, headless for now — there is no battle scene yet.

pub mod app;
mod flow;
pub mod frame;
pub mod intro;
pub mod main_menu;
pub mod new_game;
pub mod overworld;
pub mod scene;
mod textbox;
pub mod title;

pub use app::App;

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
