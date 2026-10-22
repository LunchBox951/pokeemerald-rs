//! Wild-battle handling for [`OverworldPhase`]: screening a map's encounter
//! table, starting a battle from a fired encounter, and advancing it one frame
//! at a time. The turn engine and the RNG adapter it shares with the overworld
//! live in [`crate::flow::wild_encounter`].

use crate::flow::wild_encounter;

use super::{ActiveBattle, OverworldPhase};

impl OverworldPhase {
    /// Whether the current map's land table only rolls species the battle
    /// engine can fight ([`wild_encounter::map_wild_table_fightable`]). `false`
    /// disables the encounter roll before any RNG draw. The answer is memoised
    /// against [`OverworldPhase::map_id`], so a map change recomputes it.
    pub(in crate::flow) fn wild_table_fightable(&mut self) -> bool {
        match self.wild_table_screen {
            Some((map, fightable)) if map == self.map_id => fightable,
            _ => {
                let fightable = wild_encounter::map_wild_table_fightable(self.map_id);
                self.wild_table_screen = Some((self.map_id, fightable));
                fightable
            }
        }
    }

    /// Gates the encounter roll, since `Battle::new` refuses an inadmissible lead
    /// only after `CreateWildMon`'s draws; a missing lead passes to `begin_wild_battle`.
    pub(in crate::flow) fn wild_lead_admissible(&mut self) -> bool {
        let refusal = self
            .party_lead
            .as_ref()
            .and_then(|lead| battle::ensure_participant_admissible(lead).err());
        if let Some(error) = refusal {
            if self.wild_lead_refusal != Some(error) {
                eprintln!(
                    "wild encounters: disabled while the lead can't enter a battle ({error:?})"
                );
            }
        }
        self.wild_lead_refusal = refusal;
        refusal.is_none()
    }

    /// Advances a wild battle taken out of [`OverworldPhase::active_battle`] by
    /// one frame. Returns the battle while it continues, `None` once it has
    /// ended or aborted. The turn is [`wild_encounter::advance_wild_battle`]'s;
    /// the battle's mon returns to the lead owner through
    /// [`OverworldPhase::restore_lead_battler`] before any white-out runs.
    ///
    /// A [`battle::BattleOutcome::PlayerLost`] outcome runs
    /// [`OverworldPhase::white_out`] in the same frame, so no frame observes a
    /// fainted party. Every ending, a loss included, then re-initialises the
    /// field tileset animations, as returning to the field does.
    pub(super) fn advance_wild_battle_frame(
        &mut self,
        battle: battle::Battle,
    ) -> Option<ActiveBattle> {
        let mut slot = Some(battle);
        let mut returned_lead = None;
        let outcome =
            wild_encounter::advance_wild_battle(&mut slot, &mut returned_lead, &mut self.rng);
        if let Some(lead) = returned_lead {
            self.restore_lead_battler(lead);
        }
        if let Some(outcome) = outcome {
            eprintln!("wild battle: ended -- {outcome:?}");
            if outcome == battle::BattleOutcome::PlayerLost {
                self.white_out();
            }
        }
        if slot.is_none() {
            self.reinit_field_tileset_animations();
        }
        slot.map(ActiveBattle::Wild)
    }

    /// Starts a wild battle from a fired encounter, taking the party lead into
    /// it. Nothing starts, and the lead stays in place, when the encounter is
    /// `None`, when no lead exists (a bare [`OverworldPhase::new`] phase), or
    /// when [`wild_encounter::start_wild_battle`] rejects the battle; the
    /// last two log and drop the encounter. The roll's own RNG draws are
    /// already consumed; the no-lead path draws nothing more, while a
    /// rejected start may have drawn while building the opponent.
    ///
    /// A lead that is fainted because the whole saved party is unusable is
    /// not screened here; [`battle::Battle::new`] refuses it with
    /// `FaintedBattler`, which this method logs and drops like any other
    /// rejection. A party with a usable member never reaches this state,
    /// because [`crate::party::select_active_battler`] picks that member as
    /// lead (`SetBattlePartyIds`, `pokeemerald/src/battle_controllers.c:585-606`).
    pub(super) fn begin_wild_battle(
        &mut self,
        encounter: Option<engine::overworld::WildEncounter>,
    ) {
        let Some(encounter) = encounter else {
            return;
        };
        eprintln!(
            "wild encounter: slot {} -- species {:?} at level {}",
            encounter.slot, encounter.species, encounter.level
        );
        let Some(lead) = self.take_lead_battler() else {
            eprintln!("wild encounter: no party mon yet -- no battle to start");
            return;
        };
        let player_trainer_id = u32::from_le_bytes(self.save2.player_trainer_id);
        // The start consumes its copy; the lent original returns on rejection.
        match wild_encounter::start_wild_battle(
            lead.clone(),
            encounter,
            player_trainer_id,
            &mut self.rng,
        ) {
            Ok(battle) => self.active_battle = Some(ActiveBattle::Wild(battle)),
            Err(error) => {
                eprintln!("wild encounter: can't start a battle ({error:?})");
                self.restore_lead_battler(lead);
            }
        }
    }

    /// A first-battle landing never carries a wild encounter, so at most one
    /// battle starts.
    pub(super) fn begin_step_battle(
        &mut self,
        first_battle_triggered: bool,
        encounter: Option<engine::overworld::WildEncounter>,
    ) {
        if first_battle_triggered {
            self.begin_first_battle();
        } else {
            self.begin_wild_battle(encounter);
        }
    }
}
