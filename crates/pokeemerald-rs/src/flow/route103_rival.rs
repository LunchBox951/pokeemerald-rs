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
//! [`crate::flow::npc_trainer_battle`] `(oop-boundaries)`. From issue #264
//! until issue #347, this module kept thin
//! `start_route103_rival_battle`/`advance_route103_rival_battle`
//! pass-throughs and a `RivalBattleError` alias over that module's real
//! `start_npc_trainer_battle`/`advance_npc_trainer_battle`/
//! `NpcTrainerBattleError` -- `trainer_party_personalities` made the same
//! move outright with no wrapper even then, so its one test caller already
//! imported it from `npc_trainer_battle` directly. Issue #347 retired the
//! remaining three: none carried any Route-103-specific behaviour, and the
//! wrapper shape is exactly what let `overworld_phase::route103_rival_trigger`
//! call the shared constructor with no fainted-lead screen of its own while
//! `overworld_phase::sight_trainer_trigger`'s own caller-side screen looked
//! like the load-bearing one instead of the drift risk it actually was (see
//! [`crate::flow::npc_trainer_battle`]'s own module docs, "The fainted-lead
//! check used to live on the caller side instead"). Both this module's own
//! trigger and its tests now call
//! [`crate::flow::npc_trainer_battle::start_npc_trainer_battle`]/
//! [`crate::flow::npc_trainer_battle::advance_npc_trainer_battle`] directly.
//! See that module's own docs for `CreateNPCTrainerParty`'s odd personality
//! seed, the RNG stream, and the held-item-party fail-closed gap.
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
//! [`PlayerStarter::from_var`] reads the real `VAR_STARTER_MON`
//! (`overworld_phase::first_battle_conclusion` now writes it, issue #251,
//! retiring the species-derived stand-in [`PlayerStarter::from_species`]
//! used to be for -- that method's own doc comment records the narrower
//! role it keeps). [`route103_rival_for`] exposes the *table* — the mapping
//! upstream's `Route103_EventScript_*` scripts encode — independently of
//! that derivation.

use assets::trainers::TrainerId;
use assets::SpeciesId;

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
    /// The species -> [`PlayerStarter`] mapping issue #248 (I-5) used to
    /// reach [`route103_rival_for`] from the party lead's own species,
    /// before `VAR_STARTER_MON` was modelled. Issue #251 gives the var a
    /// real write (`overworld_phase::first_battle_conclusion`), so
    /// `overworld_phase::route103_rival_trigger::OverworldPhase::begin_route103_rival_battle`
    /// now reads that var through [`Self::from_var`] instead of re-deriving
    /// a starter from whatever mon happens to be in the lead slot -- the
    /// species-derived stand-in this method was for is retired there.
    ///
    /// This method survives for the one honest use that remains: the
    /// *write* side. `first_battle_conclusion::OverworldPhase::conclude_first_battle`
    /// still has no `ChooseStarter` UI to read a real choice from, so it
    /// derives the value it writes into `VAR_STARTER_MON` from
    /// [`crate::new_game::PROVISIONAL_STARTER_SPECIES`] via this same
    /// mapping -- the mirror-image use of [`Self::var_value`], not
    /// [`Self::from_var`]'s.
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

    /// The inverse of [`Self::var_value`]: what a real `VAR_STARTER_MON`
    /// reading `value` names (`include/constants/vars.h:53`'s own comment,
    /// `// 0=Treecko, 1=Torchic, 2=Mudkip`) --
    /// `route103_rival_trigger::OverworldPhase::begin_route103_rival_battle`'s
    /// own read of the var this crate now actually writes
    /// (`overworld_phase::first_battle_conclusion`, issue #251).
    ///
    /// `None` for any other value -- unreachable against a save this crate
    /// itself ever wrote (`Self::var_value`'s own range), but a save file is
    /// still external data, so a corrupted or hand-edited var fails closed
    /// here rather than aliasing to an arbitrary starter.
    #[must_use]
    pub const fn from_var(value: u16) -> Option<Self> {
        match value {
            0 => Some(Self::Treecko),
            1 => Some(Self::Torchic),
            2 => Some(Self::Mudkip),
            _ => None,
        }
    }

    /// `VAR_STARTER_MON`'s own numeric encoding for `self`
    /// (`include/constants/vars.h:53`'s comment; [`Self::from_var`]'s
    /// inverse) -- what `overworld_phase::first_battle_conclusion` writes
    /// into the var once it knows which [`PlayerStarter`] the provisional
    /// species names.
    #[must_use]
    pub const fn var_value(self) -> u16 {
        match self {
            Self::Treecko => 0,
            Self::Torchic => 1,
            Self::Mudkip => 2,
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

#[cfg(test)]
mod tests;
