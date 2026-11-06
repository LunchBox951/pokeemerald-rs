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
//! Where real parties stand today (pinned by
//! `sight_trainer_trigger::tests::refused_sight_trainers_real_parties_fail_to_construct_for_exactly_these_reasons`
//! and that module's `real_party_outcome` tests): six of the Route 103
//! sight trainers' real parties construct through
//! [`crate::flow::npc_trainer_battle::start_npc_trainer_battle`] and play to
//! a win -- Daisy, Andrew, Rhett, Marcos, Pete and Isabelle. Only Miguel
//! (his Skitty's Cute Charm ability) and Amy & Liv (a double battle; their
//! party also stops on Helping Hand) are still refused, and
//! `begin_sight_trainer_approach_if_seen`'s own docs ("Refusals cost
//! nothing, forever") cover what that costs. `real_parties` drives four of
//! the six (Andrew, Rhett, Marcos, Pete) from their own cones to their
//! conclusions with no borrowed party; Daisy's and Isabelle's construction
//! and win are pinned in `sight_trainer_trigger` but not driven from their
//! cones here.
//!
//! The stand-in below is therefore a choice for determinism and isolation,
//! not a workaround for an unavailable production path. A borrowed party
//! keeps these tests off any one trainer's moveset and AI scoring, so a
//! `battle`-crate change to Rhett's or Andrew's moves cannot move a timing,
//! flag or RNG-draw pin that is about this module's own glue. Through
//! `synthetic_sight_trainer` it also makes a false-positive cone observable
//! as a started approach whichever single-battle trainer the scan wrongly
//! selects, including the refused Miguel, whose real party would mask it.
//! Amy and Liv are not covered that way: the scan sets double battles aside
//! before selection, so the stand-in is never reached for them. The `trigger` tests that need a refusal at Rhett's geometry
//! borrow, through the same field, the id of a party that stops at Taillow's
//! Headbutt (`TRAINER_REFUSED_FOR_MOVE`), because Rhett's own party constructs;
//! the real refusals (Miguel, Amy) have their own cone tests.
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
//! - `real_parties`: four of the six constructible trainers, cone to
//!   conclusion.
//! - `live_selection`: selection from the live object-event collection.
//! - `trigger`: cone geometry, refusals, honest cuts, real-pack terrain.
//! - `battle`: frame ownership, win/loss outcomes, the defeated flag, abort.
//! - `approach_timing`: the approach's tile timing, handoff and icon frames.
//! - `approach_handoff`: the intro speech and the handoff into the battle.

mod approach_handoff;
mod approach_timing;
mod battle;
mod live_selection;
mod real_parties;
mod support;
mod trigger;
