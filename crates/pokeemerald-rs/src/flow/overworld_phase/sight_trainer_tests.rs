//! `sight_trainer_trigger` (issue #264, I-5 follow-up) and
//! `sight_trainer_approach` (S-5, issue #300): the per-frame
//! `TRAINER_TYPE_NORMAL` sight-cone check, the approach cutscene it starts,
//! the battle handoff at the end of that, and the defeated-flag/win/loss
//! posture. New sibling test module, the same per-area split
//! `route103_rival_tests` already uses (module docs there).
//!
//! Mirrors `route103_rival_tests`' own "real events over a synthetic grid"
//! split: a fabricated flat open room paired with the *real* `MAP_ROUTE103`
//! id, so `assets::MapEventsTable::resolve` hands back the real sight
//! trainers' own object events (positions, facings, ranges) without needing
//! a local pack. The room is sized to comfortably contain every
//! elevation-3 sight trainer at once (Rhett, Marcos, Daisy, Liv, Amy,
//! Andrew, Miguel); Isabelle and Pete are elevation 1 and so never trigger
//! against this fixture's uniform elevation-3 ground (an honest fixture
//! limitation, not a claim they are untestable -- `engine::overworld::trainer_sight`'s
//! own test suite already pins the elevation-compatibility rule generically).
//!
//! # The stand-in party (`seed_battle`)
//!
//! `begin_sight_trainer_approach_if_seen`'s own docs ("Refusals cost
//! nothing, forever") record an emergent gap
//! discovered while writing this file: every real Route 103 sight trainer's
//! own default, level-up-derived moveset currently includes at least one
//! move this battle engine does not yet implement, so
//! [`crate::flow::npc_trainer_battle::start_npc_trainer_battle`] fails for
//! all nine today (pinned generically by
//! `sight_trainer_trigger::tests::every_sight_trainers_real_party_fails_to_construct_for_exactly_these_reasons`).
//! That is a genuine, current fact about this port, not a testing
//! inconvenience to work around invisibly -- so the tests above that only
//! need the trigger/geometry/refusal half
//! (`standing_in_a_real_trainers_cone_attempts_the_real_handoff_which_currently_fails_to_construct`
//! and its siblings) exercise Rhett's own real, currently-failing
//! construction attempt directly, and pin that it fails.
//!
//! The win/loss/defeated-flag *driver* half
//! ([`OverworldPhase::advance_sight_trainer_battle_frame`]) is a different
//! concern -- it runs identically regardless of *which* trainer's party
//! constructed -- and leaving it untested would hide real bugs in this
//! module's own glue (the flag id, the white-out call, the outcome channel)
//! behind an unrelated `battle`-crate move-coverage gap. So [`seed_battle`]
//! below seeds [`OverworldPhase::active_battle`]'s `SightTrainer` variant
//! directly: a real battle, built through the real
//! `start_npc_trainer_battle`/`advance_npc_trainer_battle` path, against one
//! of the six Route 103 *rivals* (proven constructible by
//! `route103_rival::tests::all_six_rivals_construct_and_play_to_a_terminal_outcome`)
//! as a stand-in party, while the `ActiveBattle::SightTrainer` variant
//! carries Rhett's own real `TrainerId` -- so the *defeated-flag* half is
//! pinned honestly (the
//! real id the flag ends up keyed to) even though the *party* is borrowed.
//! Once a future move-coverage slice lets a real sight trainer construct,
//! the two halves should be merged back into one real end-to-end test.
//!
//! # Layout
//!
//! - `support`: the shared fixtures (Route 103 phase, stand-in battle and
//!   approach seeds).
//! - `trigger`: cone geometry, refusals, honest cuts, real-pack terrain.
//! - `battle`: frame ownership, win/loss outcomes, the defeated flag, abort.
//! - `approach_timing`: the approach's tile timing, handoff and icon frames.
//! - `approach_handoff`: the intro speech and the handoff into the battle.

mod approach_handoff;
mod approach_timing;
mod battle;
mod support;
mod trigger;
