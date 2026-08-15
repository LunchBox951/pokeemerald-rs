//! The scripted Route 103 rival battle's own choice of trainer -- which of
//! the six `TRAINER_*_ROUTE_103_*` ids a playthrough fights (issue #237,
//! ladders to S-6 and gates I-5).
//!
//! Sibling of [`crate::flow::first_battle`], and split from it the same way
//! issue #221 split that module from [`crate::flow::wild_encounter`]: the
//! battle *rules* live in the `battle` crate
//! ([`battle::Battle::new_trainer`] and [`battle::trainer`]'s module docs).
//! The *construction* — `CreateNPCTrainerParty`'s seeded personalities and
//! fixed IVs, plus the headless one-turn-per-frame driver — used to live
//! here too, but issue #264 (I-5 follow-up) needed the exact same
//! construction for Route 103's *sight* trainers (Daisy, Andrew, Miguel,
//! Rhett, Marcos, Isabelle, Pete, and the deferred Amy/Liv double), not just
//! this module's own scripted rival, so it moved to the shared
//! [`crate::flow::npc_trainer_battle`] `(oop-boundaries)`: this module now
//! re-exports [`start_route103_rival_battle`]/[`advance_route103_rival_battle`]/
//! [`trainer_party_personalities`]/[`RivalBattleError`] as thin,
//! Route-103-flavoured names over that module's real definitions, so nothing
//! about this module's own public API or its existing tests had to change.
//! See [`crate::flow::npc_trainer_battle`]'s own module docs for
//! `CreateNPCTrainerParty`'s odd personality seed, the RNG stream, and the
//! held-item-party fail-closed gap.
//!
//! # Reachability: wired from real play since issue #248
//!
//! Issue #237 scoped the rules, the construction and the driver, and *not*
//! the overworld reachability; issue #248 wired that on top.
//! `overworld_phase::route103_rival_trigger` recognizes the rival's
//! `Route103_EventScript_Rival` object event on an A-press
//! (`is_rival_trigger`), and its `begin_route103_rival_battle` is this
//! module's production caller. What that trigger covers and what it still
//! defers is recorded on the `data/maps/Route103#Route103_EventScript_Rival`
//! ledger entry: the rival-approach cutscene and `RivalEnd`'s seven
//! post-battle script lines remain unmodelled. Route 103's *other* object
//! events — the sight-cone trainers `TrainerApproachPlayer` finds, not the
//! rival's own interaction script — are a separate reachability path,
//! `overworld_phase::sight_trainer_trigger` (issue #264), recorded on its own
//! `data/maps/Route103#Route103_SightTrainers` and
//! `src/trainer_see.c#CheckForTrainersWantingBattle` ledger entries.
//!
//! Which of the six `TRAINER_*_ROUTE_103_*` rivals a playthrough fights is
//! decided by that caller too: [`Rival::for_gender`] reads the saved
//! `player_gender` (always the *opposite* protagonist), and
//! [`PlayerStarter::from_species`] maps the party lead's own species —
//! `VAR_STARTER_MON` itself is still not modelled, so the lead's real
//! species is the honest stand-in. [`route103_rival_for`] exposes the
//! *table* — the mapping upstream's `Route103_EventScript_*` scripts
//! encode — independently of that derivation.

use assets::trainers::TrainerId;
use assets::SpeciesId;
use battle::{Battle, BattleOutcome, BattlePokemon};
use engine::rng::Rng;

use super::npc_trainer_battle;

/// Which starter the *player* chose — the axis the Route 103 rival's own
/// party is picked along (upstream `VAR_STARTER_MON`, read by
/// `Route103_EventScript_Rival*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlayerStarter {
    /// `SPECIES_TREECKO` (`VAR_STARTER_MON == 0`).
    Treecko,
    /// `SPECIES_TORCHIC` (`VAR_STARTER_MON == 1`).
    Torchic,
    /// `SPECIES_MUDKIP` (`VAR_STARTER_MON == 2`).
    Mudkip,
}

impl PlayerStarter {
    /// The honest species -> [`PlayerStarter`] mapping issue #248 (I-5)
    /// needs to reach [`route103_rival_for`] from an actual battle-facing
    /// lead, since `VAR_STARTER_MON` itself is not modelled (module docs'
    /// "Explicitly out of scope" section: no starter-select UI exists, so
    /// nothing ever writes it). Every production lead this is asked about
    /// is `crate::new_game::PROVISIONAL_STARTER_SPECIES` (Treecko, the
    /// stand-in for the un-ported Birch-bag handout), but the mapping
    /// covers the real three starters, not just that one, so it stays
    /// correct if a future slice ever lets the mon in slot 0 differ.
    ///
    /// `None` for any other species -- unreachable in production (the
    /// provisional starter is always one of the three), but a real `None`
    /// rather than a guessed starter for, say, a test lead built around an
    /// arbitrary species.
    #[must_use]
    pub const fn from_species(species: SpeciesId) -> Option<Self> {
        match species.0 {
            277 => Some(Self::Treecko), // SPECIES_TREECKO
            280 => Some(Self::Torchic), // SPECIES_TORCHIC
            283 => Some(Self::Mudkip),  // SPECIES_MUDKIP
            _ => None,
        }
    }
}

/// Which rival the player faces — the other axis, upstream's player-gender
/// check (`MALE`/`FEMALE`; the player and the rival are always opposite).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Rival {
    /// A female player's rival: `TRAINER_BRENDAN_ROUTE_103_*`.
    Brendan,
    /// A male player's rival: `TRAINER_MAY_ROUTE_103_*`.
    May,
}

impl Rival {
    /// The rival a player of `gender` faces (upstream: `Route103_EventScript_Rival`'s
    /// own `checkplayergender` — module docs; `data/maps/Route103/scripts.inc:19-21`)
    /// — always the *opposite* protagonist.
    ///
    /// `None` for [`engine::save::PlayerGender::Other`]: upstream's
    /// `checkplayergender` copies the raw `playerGender` byte into
    /// `VAR_RESULT` and the two `goto_if_eq`s (`MALE`/`FEMALE`) simply fall
    /// through to `end` for any other byte (`src/scrcmd.c:2014-2018`), so a
    /// save with an out-of-range gender byte starts no battle at all —
    /// the same no-op `crate::new_game::apply_truck_intro_flags` already
    /// documents for its own `checkplayergender` branch. Unreachable in
    /// production: [`crate::new_game::DEFAULT_PLAYER_GENDER`] is always
    /// `Male` or `Female`, and nothing else writes the field.
    #[must_use]
    pub const fn for_gender(gender: engine::save::PlayerGender) -> Option<Self> {
        match gender {
            engine::save::PlayerGender::Male => Some(Self::May),
            engine::save::PlayerGender::Female => Some(Self::Brendan),
            engine::save::PlayerGender::Other(_) => None,
        }
    }
}

/// The six `TRAINER_*_ROUTE_103_*` ids
/// (`pokeemerald/include/constants/opponents.h:524`-`:539`).
///
/// The suffix names the **player's** starter, not the rival's: the rival's
/// own party is the type-advantaged answer to it
/// (`src/data/trainer_parties.h:6784`, `:6828`, `:6872`, `:6916`, `:6960`,
/// `:7004` — Mudkip ⇒ a level-5 Treecko, Treecko ⇒ Torchic, Torchic ⇒
/// Mudkip), which is exactly the pairing this function encodes.
#[must_use]
pub const fn route103_rival_for(rival: Rival, starter: PlayerStarter) -> TrainerId {
    match (rival, starter) {
        (Rival::Brendan, PlayerStarter::Mudkip) => TrainerId(520),
        (Rival::Brendan, PlayerStarter::Treecko) => TrainerId(523),
        (Rival::Brendan, PlayerStarter::Torchic) => TrainerId(526),
        (Rival::May, PlayerStarter::Mudkip) => TrainerId(529),
        (Rival::May, PlayerStarter::Treecko) => TrainerId(532),
        (Rival::May, PlayerStarter::Torchic) => TrainerId(535),
    }
}

/// [`crate::flow::npc_trainer_battle::NpcTrainerBattleError`], aliased under
/// this module's original name (issue #264 review: the construction this
/// type reports on moved to that shared module, but nothing about this
/// module's own public API should shift under existing callers/tests). Used
/// as [`start_route103_rival_battle`]'s own error type below -- unlike
/// [`npc_trainer_battle::trainer_party_personalities`] (this module's tests
/// now import that directly off the module it actually lives in, since a
/// same-named pass-through here would have no production caller of its own
/// and so nothing to keep it alive outside `#[cfg(test)]`), this alias stays
/// because [`start_route103_rival_battle`] genuinely is.
pub type RivalBattleError = crate::flow::npc_trainer_battle::NpcTrainerBattleError;

/// Build `trainer`'s whole party and start the `BATTLE_TYPE_TRAINER` battle
/// around it (module docs, "RNG stream") -- a thin, Route-103-flavoured name
/// for [`npc_trainer_battle::start_npc_trainer_battle`], the shared
/// construction every NPC trainer battle in this crate now goes through
/// (issue #264 review).
///
/// # Errors
///
/// See [`npc_trainer_battle::start_npc_trainer_battle`].
pub fn start_route103_rival_battle(
    player_lead: BattlePokemon,
    trainer: TrainerId,
    rng: &mut Rng,
) -> Result<Battle, RivalBattleError> {
    npc_trainer_battle::start_npc_trainer_battle(player_lead, trainer, rng)
}

/// Play one turn of the in-progress rival battle in `slot`, headlessly --
/// a thin, Route-103-flavoured name for
/// [`npc_trainer_battle::advance_npc_trainer_battle`] (see that function's
/// own docs for the driver's policy and what a `None` return means).
pub fn advance_route103_rival_battle(
    slot: &mut Option<Battle>,
    lead: &mut Option<BattlePokemon>,
    rng: &mut Rng,
) -> Option<BattleOutcome> {
    npc_trainer_battle::advance_npc_trainer_battle(slot, lead, rng)
}

#[cfg(test)]
mod tests;
