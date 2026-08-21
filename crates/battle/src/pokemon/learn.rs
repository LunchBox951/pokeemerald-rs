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
//! [`BattlePokemon::apply_experience`] spends one award **one level
//! threshold at a time**, the way upstream's own loop does: the controller
//! caps each write at the *next* level's total and hands the leftover back
//! (`Task_GiveExpToMon`, `src/battle_controller_player.c:1154`-`:1181`),
//! `Cmd_getexp`'s case 4 runs that level's `BattleScript_LevelUp`, and case
//! 5 loops back with the remainder until none is left
//! (`src/battle_script_commands.c:3505`-`:3509`). Each level reached this
//! way has its [`assets::LevelUpLearnsets`] entries offered to
//! `GiveMoveToBoxMon`'s three outcomes (`src/pokemon.c:2939`-`:2955`),
//! reproduced here `(no-verbatim)`:
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
//! Because the award is spent level by level, the pause holds the mon *at*
//! the level whose learnset asked — the rest of the award is still
//! unconsumed, carried on the token, exactly as upstream's leftover sits in
//! `gBattleMoveDamage` while the level-up script runs. A prompt saying
//! "level 16" therefore never shows a level-17 mon with prematurely raised
//! stats. Answering is [`BattlePokemon::resolve_move_learn`], and the
//! answer is a [`MoveLearnDecision`] — decline, or name the slot to forget.
//! Either way the walk **resumes** from exactly where it stopped, which is
//! what `sLearningMoveTableID` does for upstream's own
//! `BattleScript_TryLearnMoveLoop` (`:3021`-`:3040`): declining continues to
//! the next eligible entry rather than abandoning the rest of the level-up,
//! and once the paused level's entries run out the *remaining* award is
//! spent the same way, level by level, raising further prompts in order.
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
//! # An HM move refuses to be forgotten
//!
//! Before those two writes, `Cmd_yesnoboxlearnmove` checks the slot being
//! given up with `IsHMMove2` (`src/battle_script_commands.c:5468`-`:5472`,
//! over the eight-field-move `sHMMoves` list, `src/pokemon.c:2108`) and
//! refuses: it prints `STRINGID_HMMOVESCANTBEFORGOTTEN` and reopens the
//! move list rather than overwriting. No *learnset* entry is an HM move,
//! but a slot can still hold one — [`BattlePokemon::new`] accepts any real
//! moveset, and a loaded save's mon legitimately knows Cut or Surf — so
//! [`BattlePokemon::resolve_move_learn`] models the refusal:
//! [`BattleError::HmMoveCantBeForgotten`], mutating nothing and leaving the
//! prompt answerable again ([`assets::is_hm_move`] is `IsHMMove2`).
//!
//! [`BattlePokemon`]: super::BattlePokemon

use assets::{experience_for_level, is_hm_move, LevelUpLearnsets, MoveId};

use super::{BattlePokemon, MoveSlot, MAX_LEVEL, MAX_MON_MOVES};
use crate::dex::Dex;
use crate::error::BattleError;

/// A level-up move that needs a player decision before it can be learned:
/// the mon already knows [`MAX_MON_MOVES`] moves, so a slot has to be given
/// up (`MON_HAS_MAX_MOVES`, `pokeemerald/src/pokemon.c:2954`).
///
/// The token also carries the walk's resume position — upstream's
/// `sLearningMoveTableID` (`src/pokemon.c:3021`), a file-static there —
/// *and* the still-unconsumed remainder of the experience award that
/// crossed this level (upstream's leftover `gBattleMoveDamage`,
/// `src/battle_script_commands.c:3463`, `:3505`-`:3507`), so answering it
/// continues the same level-up rather than restarting or truncating it. It
/// is therefore only meaningful to the mon that produced it, which is why
/// the mon *owns* it ([`BattlePokemon::pending_move_learn`]) and
/// [`BattlePokemon::resolve_move_learn`] takes only the answer: a copy of
/// this type is a report to read, never a credential to hand back in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PendingMoveLearn {
    /// `gMoveToLearn` (`src/pokemon.c:3037`).
    move_id: MoveId,
    /// The level whose learnset entries were being walked when the prompt
    /// came up — also the mon's level *right now*: the rest of the award is
    /// not applied until the walk resumes.
    level: u8,
    /// The learnset index to resume from — the entry *after* the one that
    /// raised this prompt.
    next_entry: usize,
    /// The part of the experience award not yet consumed when the walk
    /// paused: everything above this level's own threshold.
    remaining_exp: u32,
}

impl PendingMoveLearn {
    /// The move the player is being asked about — upstream's `gMoveToLearn`.
    #[must_use]
    pub const fn move_id(&self) -> MoveId {
        self.move_id
    }

    /// The level whose learnset offered this move. The mon *is* at this
    /// level while the prompt is open — a multi-level award pauses at each
    /// prompted level with the remainder unconsumed.
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
    /// The *next* prompt the resumed level-up stopped at, if the rest of
    /// the paused level's entries — or a later level reached by the award's
    /// unconsumed remainder — hit another full-moveset entry. A single
    /// award can raise several prompts in a row, each at its own level.
    pub next: Option<PendingMoveLearn>,
}

impl BattlePokemon {
    /// Spend `remaining` experience one level threshold at a time —
    /// upstream's cap-at-`nextLvlExp` controller write plus `Cmd_getexp`'s
    /// case-5 loop back (`src/battle_controller_player.c:1154`-`:1181`,
    /// `src/battle_script_commands.c:3505`-`:3509`) — walking each newly
    /// reached level's learnset entries and stopping at the first one that
    /// needs a player decision, with whatever is left of the award still on
    /// the token.
    ///
    /// Shared by [`BattlePokemon::apply_experience`] (which starts a fresh
    /// award) and [`BattlePokemon::resolve_move_learn`] (which continues a
    /// paused one once the prompted level's own entries run out), because
    /// upstream likewise reaches the same case-3 state on both paths.
    pub(super) fn advance_experience(
        &mut self,
        dex: &Dex,
        mut remaining: u32,
    ) -> Option<PendingMoveLearn> {
        let max_experience =
            experience_for_level(self.base_stats.growth_rate, MAX_LEVEL).unwrap_or(u32::MAX);
        loop {
            if self.level >= MAX_LEVEL {
                self.experience = self
                    .experience
                    .saturating_add(remaining)
                    .min(max_experience);
                return None;
            }
            let threshold = experience_for_level(self.base_stats.growth_rate, self.level + 1)
                .unwrap_or(u32::MAX);
            let total = self.experience.saturating_add(remaining);
            if total < threshold {
                self.experience = total;
                return None;
            }
            // Consume exactly up to the threshold; the leftover rides along
            // (upstream's `gainedExp -= nextLvlExp - currExp`,
            // `battle_controller_player.c:1174`).
            remaining = total - threshold;
            self.experience = threshold;
            // Exactly one level: the total sits on the next level's floor.
            self.raise_level_to_experience();
            if let Some(pending) = self.walk_level_learnset(dex, self.level, 0, remaining) {
                return Some(pending);
            }
        }
    }

    /// Walk one `level`'s learnset entries, starting at index `from_entry`,
    /// teaching what fits and stopping at the first entry that needs a
    /// player decision — `MonTryLearningNewMove`'s per-level loop
    /// (`src/pokemon.c:3014`-`:3044`), reached both with `firstMove = TRUE`
    /// ([`BattlePokemon::advance_experience`], a level just crossed) and
    /// with `FALSE` ([`BattlePokemon::resolve_move_learn`], resuming after
    /// an answer). `remaining_exp` is the award's unconsumed remainder,
    /// carried onto any [`PendingMoveLearn`] this walk raises.
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
    pub(super) fn walk_level_learnset(
        &mut self,
        dex: &Dex,
        level: u8,
        from_entry: usize,
        remaining_exp: u32,
    ) -> Option<PendingMoveLearn> {
        let learnset = LevelUpLearnsets::new().get(self.species)?;
        for (index, entry) in learnset.iter().enumerate().skip(from_entry) {
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
                    remaining_exp,
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
        None
    }

    /// Answer a [`PendingMoveLearn`] and resume the level-up it paused —
    /// upstream's `Cmd_yesnoboxlearnmove` outcome
    /// (`src/battle_script_commands.c:5455`-`:5497`) followed by the
    /// `BattleScript_TryLearnMoveLoop` jump back into
    /// `Cmd_handlelearnnewmove`, and then — once this level's entries run
    /// out — `Cmd_getexp`'s case-5 loop spending the rest of the award
    /// ([`BattlePokemon::advance_experience`]).
    ///
    /// A [`MoveLearnDecision::Replace`] performs both upstream writes, in
    /// upstream's order: `RemoveMonPPBonus` clears that slot's PP Ups
    /// (`:5479`), then `SetMonMoveSlot` writes the new move with the move's
    /// own base PP (`:5480`, `src/pokemon.c:2973`-`:2977`). Nothing else
    /// about the slot survives — that is the point of clearing the bonus:
    /// PP Ups belong to the move that was spent on, not to the slot.
    ///
    /// # Errors
    ///
    /// Every error leaves the mon unmutated and the prompt still
    /// answerable — the caller simply asks again:
    ///
    /// - [`BattleError::NoMoveLearnPending`] if no prompt is open
    ///   ([`BattlePokemon::pending_move_learn`]).
    /// - [`BattleError::HmMoveCantBeForgotten`] if
    ///   [`MoveLearnDecision::Replace`] names a slot holding an HM move —
    ///   upstream's `IsHMMove2` refusal (`:5468`-`:5472`, printing
    ///   `STRINGID_HMMOVESCANTBEFORGOTTEN` and reopening the move list).
    ///   No learnset entry is an HM, but [`BattlePokemon::new`] accepts any
    ///   real moveset, so a loaded save's Cut or Surf can sit in a slot.
    /// - [`BattleError::InvalidMoveSlot`] if the named slot does not exist
    ///   — a caller bug, since a prompt only exists when all
    ///   [`MAX_MON_MOVES`] slots are filled.
    /// - [`BattleError::UnknownMove`] if the pending move is not in `dex`
    ///   (unreachable: the walk read it from the extracted learnset).
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
                // Every check happens before the first write, so a rejected
                // decision leaves the moveset exactly as it found it.
                let forgotten = self
                    .moves
                    .get(slot)
                    .ok_or(BattleError::InvalidMoveSlot(slot))?
                    .move_id;
                if is_hm_move(forgotten) {
                    return Err(BattleError::HmMoveCantBeForgotten(forgotten));
                }
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
        // Finish the paused level's own entries first, then spend the rest
        // of the award level by level — either can raise the next prompt.
        let next = self
            .walk_level_learnset(
                dex,
                pending.level,
                pending.next_entry,
                pending.remaining_exp,
            )
            .or_else(|| self.advance_experience(dex, pending.remaining_exp));
        self.pending_move_learn = next;
        Ok(MoveLearnResolution { learned, next })
    }

    /// The paused level-up prompt waiting on
    /// [`BattlePokemon::resolve_move_learn`], if any — the question itself
    /// lives on the mon (upstream's `gMoveToLearn`/`sLearningMoveTableID`
    /// are flow state, not values the answer carries back in), so a stale
    /// copy of an earlier prompt cannot be replayed and one mon's prompt
    /// cannot be answered on another.
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

    const TORCHIC: SpeciesId = SpeciesId(280);
    const SCRATCH: MoveId = MoveId(10);
    const TACKLE: MoveId = MoveId(33);
    const LEER: MoveId = MoveId(43);
    const GROWL: MoveId = MoveId(45);
    /// `MOVE_SURF` — HM03's field move (`sHMMoves`, `pokemon.c:2108`).
    const SURF: MoveId = MoveId(57);
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
        let _ = mon.apply_experience(&dex, award).unwrap();
        let before = mon.moves().to_vec();

        let resolution = mon
            .resolve_move_learn(&dex, MoveLearnDecision::Decline)
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
        let _ = mon.apply_experience(&dex, award).unwrap();

        let resolution = mon
            .resolve_move_learn(&dex, MoveLearnDecision::Replace(1))
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
    /// entry it cannot fit, in ascending order — and the award is consumed
    /// only up to each prompted level, so the mon *is* the level the prompt
    /// names while the question is open (`Task_GiveExpToMon`'s cap at
    /// `nextLvlExp`, `battle_controller_player.c:1168`-`:1174`).
    #[test]
    fn a_resumed_walk_asks_again_for_the_next_entry_it_cannot_fit() {
        let dex = Dex::new();
        let mut mon = full_torchic(&dex, 15);
        let award = experience_to(&dex, &mon, 19);
        let growth_rate = dex.species(mon.species()).unwrap().growth_rate;

        let first = mon.apply_experience(&dex, award).unwrap();
        assert_eq!(first.move_id(), PECK);
        assert_eq!(first.level(), 16);
        assert_eq!(
            mon.level(),
            16,
            "the award pauses at the prompted level; the rest is unconsumed"
        );
        assert_eq!(
            mon.experience(),
            assets::experience_for_level(growth_rate, 16).unwrap(),
            "consumed exactly up to the next threshold (Task_GiveExpToMon)"
        );
        assert_eq!(
            mon.stats().max_hp,
            full_torchic(&dex, 16).stats().max_hp,
            "stats are the prompted level's, not the final level's"
        );

        let resolution = mon
            .resolve_move_learn(&dex, MoveLearnDecision::Decline)
            .unwrap();
        let second = resolution
            .next
            .expect("level 19's Sand Attack still has nowhere to go");
        assert_eq!(second.move_id(), SAND_ATTACK);
        assert_eq!(second.level(), 19);
        assert_eq!(mon.level(), 19, "the answer released the rest of the award");

        let resolution = mon
            .resolve_move_learn(&dex, MoveLearnDecision::Replace(3))
            .unwrap();
        assert!(resolution.next.is_none(), "the award is fully spent");
        assert_eq!(mon.level(), 19);
        assert_eq!(
            mon.experience(),
            assets::experience_for_level(growth_rate, 19).unwrap()
        );
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
                .resolve_move_learn(&dex, MoveLearnDecision::Decline)
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
        let _ = mon.apply_experience(&dex, award).unwrap();
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

    /// `IsHMMove2`'s refusal (`battle_script_commands.c:5468`-`:5472`): a
    /// slot holding an HM move — reachable through the unrestricted
    /// constructor and the save decoder, never through a learnset — cannot
    /// be forgotten. Nothing mutates, and the *same* prompt is still
    /// answerable: with a different slot, or by declining.
    #[test]
    fn replacing_an_hm_slot_is_refused_and_the_prompt_stays_answerable() {
        let dex = Dex::new();
        let bonuses = PpBonuses::from_bits(0b0000_0110);
        let mut mon = BattlePokemon::new(
            &dex,
            TORCHIC,
            15,
            Ivs::default(),
            0,
            vec![SCRATCH, SURF, TACKLE, LEER],
        )
        .unwrap()
        .with_pp_bonuses(&dex, bonuses)
        .unwrap();
        let award = experience_to(&dex, &mon, 16);
        let pending = mon.apply_experience(&dex, award).unwrap();
        assert_eq!(pending.move_id(), PECK);
        let before = mon.moves().to_vec();

        assert_eq!(
            mon.resolve_move_learn(&dex, MoveLearnDecision::Replace(1)),
            Err(BattleError::HmMoveCantBeForgotten(SURF)),
            "slot 1 holds Surf, which upstream refuses to overwrite"
        );
        assert_eq!(mon.moves(), before, "the refusal writes nothing");
        assert_eq!(
            mon.pp_bonuses(),
            bonuses,
            "RemoveMonPPBonus never ran either"
        );

        // The walk did not advance: the same prompt takes a corrected
        // answer, exactly like upstream reopening the move list.
        let resolution = mon
            .resolve_move_learn(&dex, MoveLearnDecision::Replace(0))
            .unwrap();
        let learned = resolution.learned.expect("slot 0 holds no HM");
        assert_eq!(learned.forgotten, SCRATCH);
        assert_eq!(
            mon.moves().iter().map(|s| s.move_id).collect::<Vec<_>>(),
            vec![PECK, SURF, TACKLE, LEER]
        );
    }

    /// The other way out of the refusal: declining the same prompt.
    #[test]
    fn an_hm_refusal_can_still_be_followed_by_a_decline() {
        let dex = Dex::new();
        let mut mon = BattlePokemon::new(
            &dex,
            TORCHIC,
            15,
            Ivs::default(),
            0,
            vec![SCRATCH, SURF, TACKLE, LEER],
        )
        .unwrap();
        let award = experience_to(&dex, &mon, 16);
        let _ = mon.apply_experience(&dex, award).unwrap();
        let before = mon.moves().to_vec();

        assert_eq!(
            mon.resolve_move_learn(&dex, MoveLearnDecision::Replace(1)),
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
