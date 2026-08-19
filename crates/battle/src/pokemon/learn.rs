//! Level-up move learning, including the four-known-moves decision (S-6,
//! issues #252 and #304): `MonTryLearningNewMove` and the
//! `GiveMoveToMon`/`SetMonMoveSlot` writes behind it
//! (`pokeemerald/src/pokemon.c:2934`-`:3044`, `:2973`-`:2977`).
//!
//! Split out of [`super`] as its own concept `(oop-boundaries)`: the walk is
//! the one part of a level-up that can *stop halfway* and wait for the
//! player, so it carries a small state machine the rest of
//! [`BattlePokemon`] does not need to know about.
//!
//! # The walk, and where it pauses
//!
//! [`BattlePokemon::apply_experience`] hands every level crossed by one
//! award to this module, in ascending order. Each of that level's
//! [`assets::LevelUpLearnsets`] entries is offered to `GiveMoveToBoxMon`'s
//! three outcomes (`:2939`-`:2955`), reproduced here `(no-verbatim)`:
//!
//! - `MON_ALREADY_KNOWS_MOVE` — skipped, costing no slot (`:2951`-`:2952`);
//! - an empty slot — learned into it at the move's own base PP
//!   (`:2945`-`:2949`);
//! - `MON_HAS_MAX_MOVES` — the walk **pauses** and returns a
//!   [`PendingMoveLearn`].
//!
//! That third outcome is upstream's `Cmd_handlelearnnewmove` reaching
//! `BattleScript_AskToLearnMove`'s yes/no box
//! (`src/battle_script_commands.c:5368`-`:5370`): the game stops and asks.
//! Answering is [`BattlePokemon::resolve_move_learn`], and the answer is a
//! [`MoveLearnDecision`] — decline, or name the slot to forget. Either way
//! the walk **resumes** from exactly where it stopped, which is what
//! `sLearningMoveTableID` does for upstream's own
//! `BattleScript_TryLearnMoveLoop` (`:3021`-`:3040`): declining continues to
//! the next eligible entry rather than abandoning the rest of the level-up.
//!
//! Pausing rather than deciding is the whole point: this crate has no
//! message layer and no summary screen, so it must not answer a question
//! upstream asks the player. The token travels to whatever layer *can* ask
//! ([`crate::battle::Battle::pending_move_learn`] forwards it out of a
//! battle), and until it is answered no further turn may be taken.
//!
//! # Replacement clears the slot's PP Ups
//!
//! `Cmd_yesnoboxlearnmove` runs `RemoveMonPPBonus` immediately before
//! `SetMonMoveSlot` (`src/battle_script_commands.c:5479`-`:5480`), so the
//! forgotten move's PP Ups are forgotten with it and the incoming move
//! starts at its own base PP with no bonus. [`super::PpBonuses::cleared`] is
//! that write; skipping it would silently gift the new move up to three PP
//! Ups the player never spent.
//!
//! [`BattlePokemon`]: super::BattlePokemon

use assets::{LevelUpLearnsets, MoveId};

use super::{BattlePokemon, MoveSlot, MAX_MON_MOVES};
use crate::dex::Dex;
use crate::error::BattleError;

/// A level-up move that needs a player decision before it can be learned:
/// the mon already knows [`MAX_MON_MOVES`] moves, so a slot has to be given
/// up (`MON_HAS_MAX_MOVES`, `pokeemerald/src/pokemon.c:2954`).
///
/// The token also carries the walk's resume position — upstream's
/// `sLearningMoveTableID` (`src/pokemon.c:3021`), a file-static there — so
/// answering it continues the same level-up rather than restarting it. It is
/// therefore only meaningful to the mon that produced it; handing one to a
/// different [`BattlePokemon`] resumes *that* mon's learnset at an unrelated
/// position. Owners keep the two together
/// ([`crate::battle::Battle::pending_move_learn`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PendingMoveLearn {
    /// `gMoveToLearn` (`src/pokemon.c:3037`).
    move_id: MoveId,
    /// The level whose learnset entries were being walked when the prompt
    /// came up.
    level: u8,
    /// The learnset index to resume from — the entry *after* the one that
    /// raised this prompt.
    next_entry: usize,
    /// The highest level this walk covers: the level the mon reached.
    last_level: u8,
}

impl PendingMoveLearn {
    /// The move the player is being asked about — upstream's `gMoveToLearn`.
    #[must_use]
    pub const fn move_id(&self) -> MoveId {
        self.move_id
    }

    /// The level whose learnset offered this move.
    #[must_use]
    pub const fn level(&self) -> u8 {
        self.level
    }
}

/// The player's answer to a [`PendingMoveLearn`].
///
/// The two outcomes of upstream's yes/no box plus its summary screen
/// (`Cmd_yesnoboxlearnmove`, `src/battle_script_commands.c:5394`-`:5497`):
/// NO — or `GetMoveSlotToReplace` returning `MAX_MON_MOVES`, the player
/// backing out of the move list — is [`MoveLearnDecision::Decline`], and a
/// chosen slot is [`MoveLearnDecision::Replace`]. Both resume the walk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MoveLearnDecision {
    /// Keep the current moveset. The move is not learned, and the walk
    /// continues to the next eligible learnset entry
    /// (`BattleScript_TryLearnMoveLoop`).
    Decline,
    /// Forget the move in this slot and learn the new one in its place —
    /// `RemoveMonPPBonus` + `SetMonMoveSlot`
    /// (`src/battle_script_commands.c:5479`-`:5480`).
    Replace(usize),
}

/// What a [`MoveLearnDecision::Replace`] actually did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LearnedMove {
    /// The move that was learned.
    pub move_id: MoveId,
    /// The move it replaced.
    pub forgotten: MoveId,
    /// The slot both occupied.
    pub slot: usize,
}

/// The result of answering one [`PendingMoveLearn`]:
/// [`BattlePokemon::resolve_move_learn`]'s report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MoveLearnResolution {
    /// The replacement that happened, or `None` for a decline.
    pub learned: Option<LearnedMove>,
    /// The *next* prompt the resumed walk stopped at, if it hit another
    /// full-moveset entry before the last crossed level ran out. A single
    /// award can raise several prompts in a row — a multi-level jump past
    /// two learnset entries with no slots free asks twice.
    pub next: Option<PendingMoveLearn>,
}

impl BattlePokemon {
    /// Walk `from_level..=last_level`'s learnset entries, starting at index
    /// `from_entry` within `from_level`, teaching what fits and stopping at
    /// the first entry that needs a player decision.
    ///
    /// Shared by [`BattlePokemon::apply_experience`] (which starts a fresh
    /// walk) and [`BattlePokemon::resolve_move_learn`] (which resumes a
    /// paused one), because upstream's `MonTryLearningNewMove` is likewise
    /// one function reached both with `firstMove = TRUE` and with `FALSE`
    /// (`src/pokemon.c:3014`-`:3044`).
    ///
    /// Teaching is **unscreened**, exactly as upstream teaches: whatever
    /// move id the learnset names is learned, including one whose effect
    /// this crate cannot execute yet — a level-6 Treecko learns Absorb even
    /// though `EFFECT_ABSORB` has no resolver ([`crate::hit`]'s module
    /// docs). Every caller applies this to a mon on the *player's* side, and
    /// the player's moveset is the one this crate deliberately does not
    /// screen: [`crate::battle::Battle::new`] documents that only the wild
    /// moveset is checked up front, while a player slot is validated per
    /// turn, at selection, ahead of the turn's first RNG draw. An
    /// unexecutable taught move therefore sits in the moveset exactly like
    /// an unexecutable hand-picked one and is refused when it is *picked*,
    /// recoverably.
    pub(super) fn walk_learnset(
        &mut self,
        dex: &Dex,
        from_level: u8,
        from_entry: usize,
        last_level: u8,
    ) -> Option<PendingMoveLearn> {
        let learnset = LevelUpLearnsets::new().get(self.species)?;
        for level in from_level..=last_level {
            let start = if level == from_level { from_entry } else { 0 };
            for (index, entry) in learnset.iter().enumerate().skip(start) {
                if entry.level != level {
                    continue;
                }
                let move_id = entry.move_id;
                if self.moves.iter().any(|slot| slot.move_id == move_id) {
                    continue; // MON_ALREADY_KNOWS_MOVE -- no slot spent.
                }
                if self.moves.len() >= MAX_MON_MOVES {
                    // MON_HAS_MAX_MOVES: stop and ask (module docs).
                    return Some(PendingMoveLearn {
                        move_id,
                        level,
                        next_entry: index + 1,
                        last_level,
                    });
                }
                // Upstream indexes `gBattleMoves[move]` for the starting PP
                // with no lookup that can fail; the learnset table only ever
                // names real move ids, so this is total in practice. Skipping
                // beats panicking if a future data extraction disagrees.
                if let Ok(mv) = dex.move_data(move_id) {
                    self.moves.push(MoveSlot { move_id, pp: mv.pp });
                }
            }
        }
        None
    }

    /// Answer a [`PendingMoveLearn`] and resume the level-up walk it paused
    /// — upstream's `Cmd_yesnoboxlearnmove` outcome
    /// (`src/battle_script_commands.c:5455`-`:5497`) followed by the
    /// `BattleScript_TryLearnMoveLoop` jump back into
    /// `Cmd_handlelearnnewmove`.
    ///
    /// A [`MoveLearnDecision::Replace`] performs both upstream writes, in
    /// upstream's order: `RemoveMonPPBonus` clears that slot's PP Ups
    /// (`:5479`), then `SetMonMoveSlot` writes the new move with the move's
    /// own base PP (`:5480`, `src/pokemon.c:2973`-`:2977`). Nothing else
    /// about the slot survives — that is the point of clearing the bonus:
    /// PP Ups belong to the move that was spent on, not to the slot.
    ///
    /// HM moves are **not** exempted here. Upstream refuses to overwrite one
    /// (`IsHMMove2`, `:5471`-`:5475`, printing
    /// `STRINGID_HMMOVESCANTBEFORGOTTEN` and re-opening the list), but no
    /// path in this port can put an HM in a slot: there is no TM/HM teaching,
    /// and no level-up learnset entry is an HM move. Modelling a refusal with
    /// no reachable input would be a guard against nothing; it is recorded on
    /// the ledger instead.
    ///
    /// # Errors
    ///
    /// [`BattleError::InvalidMoveSlot`] if [`MoveLearnDecision::Replace`]
    /// names a slot this mon does not have — a caller bug, since a prompt
    /// only exists when all [`MAX_MON_MOVES`] slots are filled. Nothing is
    /// mutated in that case. [`BattleError::UnknownMove`] if the pending
    /// move is not in `dex` (unreachable: the walk read it from the
    /// extracted learnset).
    pub fn resolve_move_learn(
        &mut self,
        dex: &Dex,
        pending: PendingMoveLearn,
        decision: MoveLearnDecision,
    ) -> Result<MoveLearnResolution, BattleError> {
        let learned = match decision {
            MoveLearnDecision::Decline => None,
            MoveLearnDecision::Replace(slot) => {
                // Both lookups happen before the first write, so a rejected
                // decision leaves the moveset exactly as it found it.
                let forgotten = self
                    .moves
                    .get(slot)
                    .ok_or(BattleError::InvalidMoveSlot(slot))?
                    .move_id;
                let pp = dex.move_data(pending.move_id)?.pp;
                self.pp_bonuses = self.pp_bonuses.cleared(slot);
                self.moves[slot] = MoveSlot {
                    move_id: pending.move_id,
                    pp,
                };
                Some(LearnedMove {
                    move_id: pending.move_id,
                    forgotten,
                    slot,
                })
            }
        };
        let next = self.walk_learnset(dex, pending.level, pending.next_entry, pending.last_level);
        Ok(MoveLearnResolution { learned, next })
    }
}

#[cfg(test)]
mod tests {
    use super::{MoveLearnDecision, MAX_MON_MOVES};
    use crate::dex::Dex;
    use crate::error::BattleError;
    use crate::pokemon::{BattlePokemon, Ivs, PpBonuses};
    use assets::{MoveId, SpeciesId};

    const TORCHIC: SpeciesId = SpeciesId(280);
    const SCRATCH: MoveId = MoveId(10);
    const TACKLE: MoveId = MoveId(33);
    const LEER: MoveId = MoveId(43);
    const GROWL: MoveId = MoveId(45);
    /// `MOVE_PECK` — Torchic's level-16 learnset entry.
    const PECK: MoveId = MoveId(64);
    /// `MOVE_SAND_ATTACK` — Torchic's level-19 entry.
    const SAND_ATTACK: MoveId = MoveId(28);

    fn full_torchic(dex: &Dex, level: u8) -> BattlePokemon {
        BattlePokemon::new(
            dex,
            TORCHIC,
            level,
            Ivs::default(),
            0,
            vec![SCRATCH, GROWL, TACKLE, LEER],
        )
        .expect("a four-move Torchic is representable")
    }

    fn experience_to(dex: &Dex, mon: &BattlePokemon, level: u8) -> u32 {
        let growth_rate = dex.species(mon.species()).unwrap().growth_rate;
        assets::experience_for_level(growth_rate, level).unwrap() - mon.experience()
    }

    /// The defect this module exists to fix: a full moveset no longer
    /// silently declines, it asks.
    #[test]
    fn a_full_moveset_pauses_for_a_decision_instead_of_declining() {
        let dex = Dex::new();
        let mut mon = full_torchic(&dex, 15);
        let award = experience_to(&dex, &mon, 16);

        let pending = mon
            .apply_experience(&dex, award)
            .expect("level 16's Peck has no free slot, so the walk must pause");

        assert_eq!(pending.move_id(), PECK);
        assert_eq!(pending.level(), 16);
        assert_eq!(mon.level(), 16, "the level still rises while we ask");
        assert_eq!(
            mon.moves().iter().map(|s| s.move_id).collect::<Vec<_>>(),
            vec![SCRATCH, GROWL, TACKLE, LEER],
            "nothing changes until the decision is made"
        );
    }

    #[test]
    fn declining_leaves_the_moveset_alone() {
        let dex = Dex::new();
        let mut mon = full_torchic(&dex, 15);
        let award = experience_to(&dex, &mon, 16);
        let pending = mon.apply_experience(&dex, award).unwrap();
        let before = mon.moves().to_vec();

        let resolution = mon
            .resolve_move_learn(&dex, pending, MoveLearnDecision::Decline)
            .unwrap();

        assert!(resolution.learned.is_none());
        assert!(resolution.next.is_none(), "no further level was crossed");
        assert_eq!(mon.moves(), before);
    }

    /// `RemoveMonPPBonus` + `SetMonMoveSlot`: the slot takes the new move at
    /// its own base PP, and the forgotten move's PP Ups go with it.
    #[test]
    fn replacing_a_slot_clears_that_slots_pp_ups() {
        let dex = Dex::new();
        // Three PP Ups on slot 1 (Growl), one on slot 0 (Scratch).
        let bonuses = PpBonuses::from_bits(0b0000_1101);
        let mut mon = full_torchic(&dex, 15)
            .with_pp_bonuses(&dex, bonuses)
            .unwrap();
        let award = experience_to(&dex, &mon, 16);
        let pending = mon.apply_experience(&dex, award).unwrap();

        let resolution = mon
            .resolve_move_learn(&dex, pending, MoveLearnDecision::Replace(1))
            .unwrap();

        let learned = resolution.learned.expect("slot 1 was replaced");
        assert_eq!(learned.move_id, PECK);
        assert_eq!(learned.forgotten, GROWL);
        assert_eq!(learned.slot, 1);
        assert_eq!(
            mon.moves().iter().map(|s| s.move_id).collect::<Vec<_>>(),
            vec![SCRATCH, PECK, TACKLE, LEER]
        );
        assert_eq!(mon.pp_bonuses().get(1), 0, "the slot's PP Ups are gone");
        assert_eq!(
            mon.pp_bonuses().get(0),
            1,
            "every other slot keeps its own PP Ups"
        );
        assert_eq!(
            mon.moves()[1].pp,
            dex.move_data(PECK).unwrap().pp,
            "SetMonMoveSlot writes the move's own base PP, bonus-free"
        );
        assert_eq!(
            mon.max_pp(&dex, 1).unwrap(),
            dex.move_data(PECK).unwrap().pp
        );
    }

    /// The walk resumes where it paused: a multi-level jump asks once per
    /// entry it cannot fit, in ascending order.
    #[test]
    fn a_resumed_walk_asks_again_for_the_next_entry_it_cannot_fit() {
        let dex = Dex::new();
        let mut mon = full_torchic(&dex, 15);
        let award = experience_to(&dex, &mon, 19);

        let first = mon.apply_experience(&dex, award).unwrap();
        assert_eq!(first.move_id(), PECK);

        let resolution = mon
            .resolve_move_learn(&dex, first, MoveLearnDecision::Decline)
            .unwrap();
        let second = resolution
            .next
            .expect("level 19's Sand Attack still has nowhere to go");
        assert_eq!(second.move_id(), SAND_ATTACK);
        assert_eq!(second.level(), 19);

        let resolution = mon
            .resolve_move_learn(&dex, second, MoveLearnDecision::Replace(3))
            .unwrap();
        assert!(resolution.next.is_none(), "the crossed range is exhausted");
        assert_eq!(
            mon.moves().iter().map(|s| s.move_id).collect::<Vec<_>>(),
            vec![SCRATCH, GROWL, TACKLE, SAND_ATTACK],
            "Peck was declined and Sand Attack took the chosen slot"
        );
    }

    /// The same-level case `sLearningMoveTableID` exists for
    /// (`pokemon.c:3019-3022`): Wynaut learns four moves at level 15, so a
    /// full-moveset Wynaut crossing 14 -> 15 must ask once per entry, each
    /// resume re-entering the *same* level at the next table index rather
    /// than restarting it (which would re-offer the first entry forever).
    #[test]
    fn a_resumed_walk_advances_through_same_level_entries() {
        const WYNAUT: SpeciesId = SpeciesId(360);
        // Wynaut's level-15 block, in table order (`assets`'s
        // `LEARNSET_WYNAUT`): Counter, Mirror Coat, Safeguard, Destiny Bond.
        const LEVEL_15_BLOCK: [MoveId; 4] = [MoveId(68), MoveId(243), MoveId(219), MoveId(194)];

        let dex = Dex::new();
        let mut mon = BattlePokemon::new(
            &dex,
            WYNAUT,
            14,
            Ivs::default(),
            0,
            vec![SCRATCH, GROWL, TACKLE, LEER],
        )
        .expect("a four-move Wynaut is representable");
        let award = experience_to(&dex, &mon, 15);

        let mut pending = mon.apply_experience(&dex, award);
        let mut offered = Vec::new();
        while let Some(prompt) = pending {
            assert_eq!(prompt.level(), 15, "every offer sits on one level");
            offered.push(prompt.move_id());
            pending = mon
                .resolve_move_learn(&dex, prompt, MoveLearnDecision::Decline)
                .unwrap()
                .next;
        }
        assert_eq!(
            offered,
            LEVEL_15_BLOCK.to_vec(),
            "each decline resumes at the next same-level entry, once each"
        );
    }

    /// A move that lands in an *empty* slot never prompts — the walk only
    /// stops for `MON_HAS_MAX_MOVES`.
    #[test]
    fn a_free_slot_learns_without_asking() {
        let dex = Dex::new();
        let mut mon =
            BattlePokemon::new(&dex, TORCHIC, 15, Ivs::default(), 0, vec![SCRATCH]).unwrap();
        let award = experience_to(&dex, &mon, 16);

        assert!(mon.apply_experience(&dex, award).is_none());
        assert_eq!(
            mon.moves().iter().map(|s| s.move_id).collect::<Vec<_>>(),
            vec![SCRATCH, PECK]
        );
    }

    #[test]
    fn replacing_a_slot_the_mon_does_not_have_is_refused_without_mutating() {
        let dex = Dex::new();
        let mut mon = full_torchic(&dex, 15);
        let award = experience_to(&dex, &mon, 16);
        let pending = mon.apply_experience(&dex, award).unwrap();
        let before = mon.moves().to_vec();

        assert_eq!(
            mon.resolve_move_learn(&dex, pending, MoveLearnDecision::Replace(MAX_MON_MOVES)),
            Err(BattleError::InvalidMoveSlot(MAX_MON_MOVES))
        );
        assert_eq!(mon.moves(), before);
        assert_eq!(
            mon.resolve_move_learn(&dex, pending, MoveLearnDecision::Decline)
                .unwrap()
                .learned,
            None,
            "the same prompt can still be answered afterwards"
        );
    }
}
