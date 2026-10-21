//! Experience progression for one battler: awarding experience, the
//! level-from-experience walk, and reconciling saved experience.
//!
//! The learnset side of a level-up lives in [`super::learn`]; the stat
//! recompute a crossed level triggers lives in [`super::stats`].

use assets::experience_for_level;

use super::stats::{compute_stats, effective_battle_base_stats};
use super::{BattlePokemon, PendingMoveLearn, MAX_LEVEL};
use crate::dex::Dex;
use crate::error::BattleError;

impl BattlePokemon {
    /// Applies earned experience one level threshold at a time, capping both
    /// level and total experience at [`MAX_LEVEL`]. Each crossed level
    /// recalculates stats, preserves damage taken, and teaches that level's
    /// complete learnset without filtering unsupported moves — a move this
    /// crate cannot execute yet is still learned, exactly like upstream. A full moveset pauses the
    /// award and returns a [`PendingMoveLearn`]; the caller must pass its
    /// decision to [`BattlePokemon::resolve_move_learn`] before applying more
    /// experience. [`BattlePokemon::evs`] is not changed here —
    /// [`BattlePokemon::gain_evs`] already folds a KO's award in before this
    /// runs; friendship is still not part of this battle model
    /// and does not change.
    ///
    /// # Errors
    ///
    /// Returns [`BattleError::MoveLearnPending`] without mutation when a move
    /// decision is already pending — a second walk would overwrite the open
    /// question and drop its unconsumed remainder.
    #[must_use = "a full moveset pauses the level-up walk for a player \
                  decision the mon now carries \
                  (`BattlePokemon::pending_move_learn`); ignoring the \
                  report means never asking the player"]
    pub fn apply_experience(
        &mut self,
        dex: &Dex,
        amount: u32,
    ) -> Result<Option<PendingMoveLearn>, BattleError> {
        if let Some(pending) = self.pending_move_learn {
            return Err(BattleError::MoveLearnPending(pending.move_id()));
        }
        self.pending_move_learn = self.advance_experience(dex, amount);
        Ok(self.pending_move_learn)
    }

    /// Reconciles saved experience without teaching crossed-level moves.
    ///
    /// Experience is clamped between the current value and the level-100
    /// threshold. The level can rise to match it but never decreases.
    pub fn reconcile_saved_experience(&mut self, total: u32) {
        let max_experience =
            experience_for_level(self.base_stats.growth_rate, MAX_LEVEL).unwrap_or(u32::MAX);
        self.experience = self.experience.max(total.min(max_experience));
        self.raise_level_to_experience();
    }

    /// Raises the level (and stats, preserving damage taken) to match the
    /// current experience total, returning `Some((old_level, new_level))`
    /// when at least one threshold was crossed. Shared by the in-battle
    /// award ([`BattlePokemon::apply_experience`], which then teaches the
    /// crossed learnset moves) and the save decoder
    /// ([`BattlePokemon::reconcile_saved_experience`], which must not).
    ///
    /// The stat recompute is [`compute_stats`] at `0` EVs for max HP and
    /// every non-Deoxys stat; Deoxys's five Speed Forme stats then take the
    /// current EVs through [`BattlePokemon::apply_speed_forme_evs`]. See the
    /// [`super::evs`] module docs for why the rest of [`BattlePokemon::stats`] must
    /// stay `0`-EV even here. A crossed level
    /// also snapshots [`BattlePokemon::evs_at_last_level_up`] -- see that
    /// field's own doc for what reads it and why.
    pub(super) fn raise_level_to_experience(&mut self) -> Option<(u8, u8)> {
        let mut new_level = self.level;
        while new_level < MAX_LEVEL {
            let next_level = new_level + 1;
            let threshold =
                experience_for_level(self.base_stats.growth_rate, next_level).unwrap_or(u32::MAX);
            if self.experience < threshold {
                break;
            }
            new_level = next_level;
        }
        if new_level == self.level {
            return None;
        }

        let old_level = self.level;
        let old_max_hp = self.stats.max_hp;
        self.level = new_level;
        self.stats = compute_stats(
            self.species,
            &effective_battle_base_stats(self.species, &self.base_stats),
            self.level,
            self.nature,
            self.ivs,
        );
        self.apply_speed_forme_evs();
        self.current_hp = self
            .current_hp
            .saturating_add(self.stats.max_hp.saturating_sub(old_max_hp));
        self.evs_at_last_level_up = self.evs;
        Some((old_level, new_level))
    }
}
