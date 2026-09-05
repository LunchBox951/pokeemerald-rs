//! Level-up move learning and full-moveset replacement decisions.

use assets::{experience_for_level, is_hm_move, LevelUpLearnsets, MoveId};

use super::{BattlePokemon, MoveSlot, MAX_LEVEL, MAX_MON_MOVES};
use crate::dex::Dex;
use crate::error::BattleError;

/// A level-up move awaiting a player decision because all move slots are full.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PendingMoveLearn {
    offered_move: MoveId,
    offered_at_level: u8,
    resume_at_entry: usize,
    unapplied_experience: u32,
}

impl PendingMoveLearn {
    /// Returns the move offered for learning.
    #[must_use]
    pub const fn move_id(&self) -> MoveId {
        self.offered_move
    }

    /// Returns the level at which the move was offered.
    #[must_use]
    pub const fn level(&self) -> u8 {
        self.offered_at_level
    }
}

/// The player's answer to a pending move-learning prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MoveLearnDecision {
    /// Keep the current moveset and resume applying the experience award.
    Decline,
    /// Replace the move in this slot and resume applying the experience award.
    Replace(usize),
}

/// A move replacement completed while resolving a learning prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LearnedMove {
    /// Learned move.
    pub move_id: MoveId,
    /// Forgotten move.
    pub forgotten: MoveId,
    /// Replaced slot.
    pub slot: usize,
}

/// Result of resolving one move-learning prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MoveLearnResolution {
    /// Completed replacement, or `None` when the move was declined.
    pub learned: Option<LearnedMove>,
    /// Next move awaiting a decision while the experience award resumes.
    pub next: Option<PendingMoveLearn>,
}

impl BattlePokemon {
    pub(super) fn advance_experience(
        &mut self,
        dex: &Dex,
        mut unapplied_experience: u32,
    ) -> Option<PendingMoveLearn> {
        let max_experience =
            experience_for_level(self.base_stats.growth_rate, MAX_LEVEL).unwrap_or(u32::MAX);
        loop {
            if self.level >= MAX_LEVEL {
                self.experience = self
                    .experience
                    .saturating_add(unapplied_experience)
                    .min(max_experience);
                return None;
            }
            let next_level_experience =
                experience_for_level(self.base_stats.growth_rate, self.level + 1)
                    .unwrap_or(u32::MAX);
            let experience_after_award = self.experience.saturating_add(unapplied_experience);
            if experience_after_award < next_level_experience {
                self.experience = experience_after_award;
                return None;
            }
            unapplied_experience = experience_after_award - next_level_experience;
            self.experience = next_level_experience;
            self.raise_level_to_experience();
            if let Some(pending) =
                self.process_level_learnset(dex, self.level, 0, unapplied_experience)
            {
                return Some(pending);
            }
        }
    }

    fn process_level_learnset(
        &mut self,
        dex: &Dex,
        level: u8,
        start_at_entry: usize,
        unapplied_experience: u32,
    ) -> Option<PendingMoveLearn> {
        let learnset = LevelUpLearnsets::new().get(self.species)?;
        for (learnset_index, entry) in learnset.iter().enumerate().skip(start_at_entry) {
            if entry.level() != level {
                continue;
            }
            let move_id = entry.move_id();
            let already_known = self.moves.iter().any(|slot| slot.move_id == move_id);
            if already_known {
                continue;
            }
            if self.moves.len() >= MAX_MON_MOVES {
                return Some(PendingMoveLearn {
                    offered_move: move_id,
                    offered_at_level: level,
                    resume_at_entry: learnset_index + 1,
                    unapplied_experience,
                });
            }
            // Upstream trusts learnset IDs and indexes `gBattleMoves` directly
            // (`src/pokemon.c:2948`); ignore invalid extracted IDs instead of panicking.
            let Ok(move_data) = dex.move_data(move_id) else {
                continue;
            };
            self.moves.push(MoveSlot {
                move_id,
                pp: move_data.pp,
            });
        }
        None
    }

    /// Resolves the current learning prompt and resumes the experience award.
    ///
    /// Replacing a move clears the chosen slot's PP Ups and gives the learned
    /// move its base PP. Declining preserves the moveset.
    ///
    /// # Errors
    ///
    /// Returns [`BattleError::NoMoveLearnPending`] when no decision is pending,
    /// [`BattleError::InvalidMoveSlot`] for a missing replacement slot,
    /// [`BattleError::HmMoveCantBeForgotten`] for an HM replacement, or
    /// [`BattleError::UnknownMove`] when `dex` lacks the offered move. Errors
    /// leave the Pokémon and prompt unchanged.
    pub fn resolve_move_learn(
        &mut self,
        dex: &Dex,
        decision: MoveLearnDecision,
    ) -> Result<MoveLearnResolution, BattleError> {
        let pending = self
            .pending_move_learn
            .ok_or(BattleError::NoMoveLearnPending)?;
        let learned = match decision {
            MoveLearnDecision::Decline => None,
            MoveLearnDecision::Replace(slot) => {
                let forgotten_move = self
                    .moves
                    .get(slot)
                    .ok_or(BattleError::InvalidMoveSlot(slot))?
                    .move_id;
                if is_hm_move(forgotten_move) {
                    return Err(BattleError::HmMoveCantBeForgotten(forgotten_move));
                }
                let replacement_pp = dex.move_data(pending.offered_move)?.pp;
                self.pp_bonuses = self.pp_bonuses.cleared(slot);
                self.moves[slot] = MoveSlot {
                    move_id: pending.offered_move,
                    pp: replacement_pp,
                };
                Some(LearnedMove {
                    move_id: pending.offered_move,
                    forgotten: forgotten_move,
                    slot,
                })
            }
        };
        let next = self
            .process_level_learnset(
                dex,
                pending.offered_at_level,
                pending.resume_at_entry,
                pending.unapplied_experience,
            )
            .or_else(|| self.advance_experience(dex, pending.unapplied_experience));
        self.pending_move_learn = next;
        Ok(MoveLearnResolution { learned, next })
    }

    /// Returns the move-learning prompt currently blocking experience advancement.
    #[must_use]
    pub const fn pending_move_learn(&self) -> Option<PendingMoveLearn> {
        self.pending_move_learn
    }
}

#[cfg(test)]
mod tests {
    use super::{MoveLearnDecision, MAX_MON_MOVES};
    use crate::dex::Dex;
    use crate::error::BattleError;
    use crate::pokemon::{BattlePokemon, Ivs, PpBonuses};
    use assets::{MoveId, SpeciesId};

    const TEST_PERSONALITY: u32 = 0;
    const TORCHIC: SpeciesId = SpeciesId(280);
    const WYNAUT: SpeciesId = SpeciesId(360);
    const SCRATCH: MoveId = MoveId(10);
    const SAND_ATTACK: MoveId = MoveId(28);
    const TACKLE: MoveId = MoveId(33);
    const LEER: MoveId = MoveId(43);
    const GROWL: MoveId = MoveId(45);
    const SURF: MoveId = MoveId(57);
    const PECK: MoveId = MoveId(64);
    const COUNTER: MoveId = MoveId(68);
    const DESTINY_BOND: MoveId = MoveId(194);
    const SAFEGUARD: MoveId = MoveId(219);
    const MIRROR_COAT: MoveId = MoveId(243);

    const BEFORE_PECK_LEVEL: u8 = 15;
    const PECK_LEVEL: u8 = 16;
    const SAND_ATTACK_LEVEL: u8 = 19;
    const BEFORE_WYNAUT_MULTI_MOVE_LEVEL: u8 = 14;
    const WYNAUT_MULTI_MOVE_LEVEL: u8 = 15;
    const WYNAUT_MULTI_LEVEL_MOVES: [MoveId; 4] = [COUNTER, MIRROR_COAT, SAFEGUARD, DESTINY_BOND];

    const SCRATCH_SLOT: usize = 0;
    const GROWL_SLOT: usize = 1;
    const SURF_SLOT: usize = 1;
    const LEER_SLOT: usize = 3;

    const ADDITIONAL_EXPERIENCE: u32 = 50;

    const SCRATCH_ONE_GROWL_THREE_PP_UPS: PpBonuses = PpBonuses::from_bits(0b0000_1101);
    const NONZERO_PP_BONUSES: PpBonuses = PpBonuses::from_bits(0b0000_0110);

    fn torchic_with_full_moveset(dex: &Dex, level: u8) -> BattlePokemon {
        BattlePokemon::new(
            dex,
            TORCHIC,
            level,
            Ivs::default(),
            TEST_PERSONALITY,
            vec![SCRATCH, GROWL, TACKLE, LEER],
        )
        .expect("a four-move Torchic is representable")
    }

    fn experience_needed_for_level(dex: &Dex, mon: &BattlePokemon, level: u8) -> u32 {
        let growth_rate = dex.species(mon.species()).unwrap().growth_rate;
        assets::experience_for_level(growth_rate, level).unwrap() - mon.experience()
    }

    fn known_moves(mon: &BattlePokemon) -> Vec<MoveId> {
        mon.moves().iter().map(|slot| slot.move_id).collect()
    }

    #[test]
    fn a_full_moveset_requires_a_decision_before_learning() {
        let dex = Dex::new();
        let mut mon = torchic_with_full_moveset(&dex, BEFORE_PECK_LEVEL);
        let award = experience_needed_for_level(&dex, &mon, PECK_LEVEL);

        let pending = mon
            .apply_experience(&dex, award)
            .unwrap()
            .expect("level 16's Peck has no free slot, so the walk must pause");

        assert_eq!(pending.move_id(), PECK);
        assert_eq!(pending.level(), PECK_LEVEL);
        assert_eq!(
            mon.level(),
            PECK_LEVEL,
            "the level still rises while we ask"
        );
        assert_eq!(
            known_moves(&mon),
            vec![SCRATCH, GROWL, TACKLE, LEER],
            "nothing changes until the decision is made"
        );
    }

    #[test]
    fn a_second_award_with_an_open_prompt_is_refused_unmutated() {
        let dex = Dex::new();
        let mut mon = torchic_with_full_moveset(&dex, BEFORE_PECK_LEVEL);
        let award = experience_needed_for_level(&dex, &mon, PECK_LEVEL);
        let pending = mon.apply_experience(&dex, award).unwrap().unwrap();

        assert_eq!(
            mon.apply_experience(&dex, ADDITIONAL_EXPERIENCE),
            Err(BattleError::MoveLearnPending(PECK))
        );
        assert_eq!(
            mon.pending_move_learn(),
            Some(pending),
            "the open prompt survives the refused call untouched"
        );
        assert_eq!(
            mon.level(),
            PECK_LEVEL,
            "no part of the second award landed"
        );
    }

    #[test]
    fn declining_leaves_the_moveset_alone() {
        let dex = Dex::new();
        let mut mon = torchic_with_full_moveset(&dex, BEFORE_PECK_LEVEL);
        let award = experience_needed_for_level(&dex, &mon, PECK_LEVEL);
        let _ = mon.apply_experience(&dex, award).unwrap().unwrap();
        let before = mon.moves().to_vec();

        let resolution = mon
            .resolve_move_learn(&dex, MoveLearnDecision::Decline)
            .unwrap();

        assert!(resolution.learned.is_none());
        assert!(resolution.next.is_none(), "no further level was crossed");
        assert_eq!(mon.moves(), before);
    }

    #[test]
    fn replacing_a_slot_clears_that_slots_pp_ups() {
        let dex = Dex::new();
        let mut mon = torchic_with_full_moveset(&dex, BEFORE_PECK_LEVEL)
            .with_pp_bonuses(&dex, SCRATCH_ONE_GROWL_THREE_PP_UPS)
            .unwrap();
        let award = experience_needed_for_level(&dex, &mon, PECK_LEVEL);
        let _ = mon.apply_experience(&dex, award).unwrap().unwrap();

        let resolution = mon
            .resolve_move_learn(&dex, MoveLearnDecision::Replace(GROWL_SLOT))
            .unwrap();

        let learned = resolution.learned.expect("Growl's slot was replaced");
        assert_eq!(learned.move_id, PECK);
        assert_eq!(learned.forgotten, GROWL);
        assert_eq!(learned.slot, GROWL_SLOT);
        assert_eq!(known_moves(&mon), vec![SCRATCH, PECK, TACKLE, LEER]);
        assert_eq!(
            mon.pp_bonuses().get(GROWL_SLOT),
            0,
            "the slot's PP Ups are gone"
        );
        assert_eq!(
            mon.pp_bonuses().get(SCRATCH_SLOT),
            1,
            "every other slot keeps its own PP Ups"
        );
        assert_eq!(
            mon.moves()[GROWL_SLOT].pp,
            dex.move_data(PECK).unwrap().pp,
            "the learned move starts at its base PP"
        );
        assert_eq!(
            mon.max_pp(&dex, GROWL_SLOT).unwrap(),
            dex.move_data(PECK).unwrap().pp
        );
    }

    #[test]
    fn a_resumed_walk_asks_again_for_the_next_entry_it_cannot_fit() {
        let dex = Dex::new();
        let mut mon = torchic_with_full_moveset(&dex, BEFORE_PECK_LEVEL);
        let award = experience_needed_for_level(&dex, &mon, SAND_ATTACK_LEVEL);
        let growth_rate = dex.species(mon.species()).unwrap().growth_rate;

        let first = mon.apply_experience(&dex, award).unwrap().unwrap();
        assert_eq!(first.move_id(), PECK);
        assert_eq!(first.level(), PECK_LEVEL);
        assert_eq!(
            mon.level(),
            PECK_LEVEL,
            "the award pauses at the prompted level; the rest is unconsumed"
        );
        assert_eq!(
            mon.experience(),
            assets::experience_for_level(growth_rate, PECK_LEVEL).unwrap(),
            "experience stops at the prompted level's threshold"
        );
        assert_eq!(
            mon.stats().max_hp,
            torchic_with_full_moveset(&dex, PECK_LEVEL).stats().max_hp,
            "stats are the prompted level's, not the final level's"
        );

        let resolution = mon
            .resolve_move_learn(&dex, MoveLearnDecision::Decline)
            .unwrap();
        let second = resolution
            .next
            .expect("level 19's Sand Attack still has nowhere to go");
        assert_eq!(second.move_id(), SAND_ATTACK);
        assert_eq!(second.level(), SAND_ATTACK_LEVEL);
        assert_eq!(
            mon.level(),
            SAND_ATTACK_LEVEL,
            "the answer released the rest of the award"
        );

        let resolution = mon
            .resolve_move_learn(&dex, MoveLearnDecision::Replace(LEER_SLOT))
            .unwrap();
        assert!(resolution.next.is_none(), "the award is fully spent");
        assert_eq!(mon.level(), SAND_ATTACK_LEVEL);
        assert_eq!(
            mon.experience(),
            assets::experience_for_level(growth_rate, SAND_ATTACK_LEVEL).unwrap()
        );
        assert_eq!(
            known_moves(&mon),
            vec![SCRATCH, GROWL, TACKLE, SAND_ATTACK],
            "Peck was declined and Sand Attack took the chosen slot"
        );
    }

    #[test]
    fn a_resumed_walk_advances_through_same_level_entries() {
        let dex = Dex::new();
        let mut mon = BattlePokemon::new(
            &dex,
            WYNAUT,
            BEFORE_WYNAUT_MULTI_MOVE_LEVEL,
            Ivs::default(),
            TEST_PERSONALITY,
            vec![SCRATCH, GROWL, TACKLE, LEER],
        )
        .expect("a four-move Wynaut is representable");
        let award = experience_needed_for_level(&dex, &mon, WYNAUT_MULTI_MOVE_LEVEL);

        let mut pending = mon.apply_experience(&dex, award).unwrap();
        let mut offered = Vec::new();
        while let Some(prompt) = pending {
            assert_eq!(
                prompt.level(),
                WYNAUT_MULTI_MOVE_LEVEL,
                "every offer sits on one level"
            );
            offered.push(prompt.move_id());
            pending = mon
                .resolve_move_learn(&dex, MoveLearnDecision::Decline)
                .unwrap()
                .next;
        }
        assert_eq!(
            offered,
            WYNAUT_MULTI_LEVEL_MOVES.to_vec(),
            "each decline resumes at the next same-level entry, once each"
        );
    }

    #[test]
    fn a_free_slot_learns_without_asking() {
        let dex = Dex::new();
        let mut mon = BattlePokemon::new(
            &dex,
            TORCHIC,
            BEFORE_PECK_LEVEL,
            Ivs::default(),
            TEST_PERSONALITY,
            vec![SCRATCH],
        )
        .unwrap();
        let award = experience_needed_for_level(&dex, &mon, PECK_LEVEL);

        assert!(mon.apply_experience(&dex, award).unwrap().is_none());
        assert_eq!(known_moves(&mon), vec![SCRATCH, PECK]);
    }

    #[test]
    fn replacing_a_slot_the_mon_does_not_have_is_refused_without_mutating() {
        let dex = Dex::new();
        let mut mon = torchic_with_full_moveset(&dex, BEFORE_PECK_LEVEL);
        let award = experience_needed_for_level(&dex, &mon, PECK_LEVEL);
        let _ = mon.apply_experience(&dex, award).unwrap().unwrap();
        let before = mon.moves().to_vec();

        assert_eq!(
            mon.resolve_move_learn(&dex, MoveLearnDecision::Replace(MAX_MON_MOVES)),
            Err(BattleError::InvalidMoveSlot(MAX_MON_MOVES))
        );
        assert_eq!(mon.moves(), before);
        assert_eq!(
            mon.resolve_move_learn(&dex, MoveLearnDecision::Decline)
                .unwrap()
                .learned,
            None,
            "the same prompt can still be answered afterwards"
        );
    }

    #[test]
    fn replacing_an_hm_slot_is_refused_and_the_prompt_stays_answerable() {
        let dex = Dex::new();

        let mut mon = BattlePokemon::new(
            &dex,
            TORCHIC,
            BEFORE_PECK_LEVEL,
            Ivs::default(),
            TEST_PERSONALITY,
            vec![SCRATCH, SURF, TACKLE, LEER],
        )
        .unwrap()
        .with_pp_bonuses(&dex, NONZERO_PP_BONUSES)
        .unwrap();
        let award = experience_needed_for_level(&dex, &mon, PECK_LEVEL);
        let pending = mon.apply_experience(&dex, award).unwrap().unwrap();
        assert_eq!(pending.move_id(), PECK);
        let before = mon.moves().to_vec();

        assert_eq!(
            mon.resolve_move_learn(&dex, MoveLearnDecision::Replace(SURF_SLOT)),
            Err(BattleError::HmMoveCantBeForgotten(SURF)),
            "Surf cannot be forgotten"
        );
        assert_eq!(mon.moves(), before, "the refusal writes nothing");
        assert_eq!(
            mon.pp_bonuses(),
            NONZERO_PP_BONUSES,
            "the refusal preserves PP Ups"
        );

        let resolution = mon
            .resolve_move_learn(&dex, MoveLearnDecision::Replace(SCRATCH_SLOT))
            .unwrap();
        let learned = resolution.learned.expect("Scratch is replaceable");
        assert_eq!(learned.forgotten, SCRATCH);
        assert_eq!(known_moves(&mon), vec![PECK, SURF, TACKLE, LEER]);
    }

    #[test]
    fn an_hm_refusal_can_still_be_followed_by_a_decline() {
        let dex = Dex::new();

        let mut mon = BattlePokemon::new(
            &dex,
            TORCHIC,
            BEFORE_PECK_LEVEL,
            Ivs::default(),
            TEST_PERSONALITY,
            vec![SCRATCH, SURF, TACKLE, LEER],
        )
        .unwrap();
        let award = experience_needed_for_level(&dex, &mon, PECK_LEVEL);
        let _ = mon.apply_experience(&dex, award).unwrap().unwrap();
        let before = mon.moves().to_vec();

        assert_eq!(
            mon.resolve_move_learn(&dex, MoveLearnDecision::Replace(SURF_SLOT)),
            Err(BattleError::HmMoveCantBeForgotten(SURF))
        );
        let resolution = mon
            .resolve_move_learn(&dex, MoveLearnDecision::Decline)
            .unwrap();
        assert!(resolution.learned.is_none());
        assert!(resolution.next.is_none());
        assert_eq!(mon.moves(), before);
    }
}
