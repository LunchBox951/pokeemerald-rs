//! `sight_trainer_trigger` (I-5) and
//! `sight_trainer_approach` (S-5): the per-frame
//! `TRAINER_TYPE_NORMAL` sight-cone check, the approach cutscene it starts,
//! the battle handoff at the end of that, and the defeated-flag/win/loss
//! posture. A sibling test module, the same per-area split
//! `route103_rival_tests` uses (module docs there).
//!
//! Mirrors `route103_rival_tests`' own "real events over a synthetic grid"
//! split: a fabricated flat open room paired with the *real* `MAP_ROUTE103`
//! id, so `assets::MapEventsTable::resolve` hands back the real sight
//! trainers' own object events (positions, facings, ranges) without needing
//! a local pack. The room is sized to comfortably contain every
//! elevation-3 sight trainer at once (Rhett, Marcos, Daisy, Liv, Amy,
//! Andrew, Miguel); Isabelle and Pete are elevation 1 and so never trigger
//! against this fixture's uniform elevation-3 ground. `real_parties` raises
//! the one cell Pete's player stands on to elevation 1
//! (`synthetic_scene_with_cell_elevation`), which is all his one-tile approach
//! needs; `engine::overworld::trainer_sight`'s own test suite pins the
//! elevation-compatibility rule generically.
//!
//! # The stand-in party (`seed_battle`)
//!
//! `begin_sight_trainer_approach_if_seen`'s own docs ("Refusals cost
//! nothing, forever") record an emergent gap: most real Route 103 sight
//! trainers' default, level-up-derived movesets still include a move this
//! battle engine does not yet implement or score, so
//! [`crate::flow::npc_trainer_battle::start_npc_trainer_battle`] fails for
//! them (pinned generically by
//! `sight_trainer_trigger::tests::refused_sight_trainers_real_parties_fail_to_construct_for_exactly_these_reasons`).
//! Andrew's, Rhett's, Marcos's and Pete's real parties do construct, and
//! `real_parties` drives each from its own cone to its conclusion. The
//! `trigger` tests that need a refusal use Isabelle's id (her party stops at
//! Rollout) in place of Rhett's at Rhett's cone through
//! `synthetic_sight_trainer`, so the geometry stays Rhett's while the refused
//! party is real.
//!
//! The win/loss/defeated-flag *driver* half
//! ([`OverworldPhase::advance_sight_trainer_battle_frame`][advance]) is a
//! different concern -- it runs identically regardless of *which* trainer's
//! party constructed -- and leaving it untested would hide real bugs in this
//! module's own glue (the flag id, the white-out call, the outcome channel)
//! behind an unrelated `battle`-crate move-coverage gap. So
//! [`support::seed_battle`] seeds
//! [`OverworldPhase::active_battle`][active]'s `SightTrainer` variant
//! directly: a real battle, built through the real
//! `start_npc_trainer_battle`/`advance_npc_trainer_battle` path, against one
//! of the six Route 103 *rivals* (proven constructible by
//! `route103_rival::tests::all_six_rivals_construct_and_play_to_a_terminal_outcome`)
//! as a stand-in party, while the `ActiveBattle::SightTrainer` variant
//! carries Rhett's own real `TrainerId` -- so the *defeated-flag* half is
//! pinned honestly (the
//! real id the flag ends up keyed to) even though the *party* is borrowed.
//!
//! [advance]: super::OverworldPhase::advance_sight_trainer_battle_frame
//! [active]: super::OverworldPhase::active_battle
//!
//! # Layout
//!
//! - `support`: the shared fixtures (Route 103 phase, stand-in battle and
//!   approach seeds).
//! - `real_parties`: the four fighting trainers, cone to conclusion.
//! - `trigger`: cone geometry, refusals, honest cuts, real-pack terrain.
//! - `battle`: frame ownership, win/loss outcomes, the defeated flag, abort.
//! - `approach_timing`: the approach's tile timing, handoff and icon frames.
//! - `approach_handoff`: the intro speech and the handoff into the battle.

mod approach_handoff;
mod approach_timing;
mod battle;
mod real_parties;
mod support;
mod trigger;
