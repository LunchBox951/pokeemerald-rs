//! The lead-owner compatibility boundary over `OverworldPhase`'s
//! `party_lead`, `party_lead_slot`, and `lead_hp_hidden_by_load` triad.
//!
//! The three fields are one fact -- the battle-facing lead, the saved slot it
//! was decoded from, and the HP the load clamp hid from it -- and upstream
//! never synchronises them as unrelated values
//! (`pokeemerald/src/pokemon.c:2823-2895`). This interface is the only
//! surface adopters should touch: it returns the battler, never the slot or
//! the offset, and every operation that moves one field moves the others with
//! it. It carries the existing behaviour unchanged, so the storage can later
//! become a single aggregate without changing any caller.

use battle::{BattleError, BattlePokemon, Dex};

use super::OverworldPhase;
use crate::party;

/// The slot a lead-less phase reports; never read while no lead is present.
const NO_LEAD_SLOT: usize = 0;

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "compatibility boundary awaiting its flow adopters"
    )
)]
impl OverworldPhase {
    /// Battler observation: the lead in battle-ready form, if any.
    pub(super) fn lead_battler(&self) -> Option<&BattlePokemon> {
        self.party_lead.as_ref()
    }

    /// Battler mutation. Exposes the battler only, so the slot and hidden-HP
    /// offset cannot be edited independently of it.
    pub(super) fn lead_battler_mut(&mut self) -> Option<&mut BattlePokemon> {
        self.party_lead.as_mut()
    }

    /// Lend: moves the battler out for a battle. The selected slot and the
    /// hidden-HP offset stay with the owner, so [`Self::restore_lead_battler`]
    /// merges back into the same record.
    pub(super) fn take_lead_battler(&mut self) -> Option<BattlePokemon> {
        self.party_lead.take()
    }

    /// Restore: returns the battler a battle borrowed, keeping the retained
    /// slot and offset.
    ///
    /// # Panics
    ///
    /// As [`party::LoadedLead::restore_battler`] does: panics if the battler
    /// was not lent out, or if a saved party (nonzero stored count) backs the
    /// lead and `battler` is not the mon in the selected slot (personality
    /// and original trainer id). An unsaved lead has no record to match; the
    /// next flush files it fresh.
    pub(super) fn restore_lead_battler(&mut self, battler: BattlePokemon) {
        assert!(
            self.party_lead.is_none(),
            "the loaded lead's battler was never lent out"
        );
        let record = &self.save1.player_party[self.party_lead_slot].box_data;
        assert!(
            self.save1.player_party_count == 0
                || (battler.personality() == record.personality()
                    && battler.original_trainer_id() == record.ot_id()),
            "a different party member cannot take the loaded lead's place"
        );
        self.party_lead = Some(battler);
    }

    /// Installs a lead with no saved backing: slot 0, no hidden HP, and no
    /// retained-undecodable diagnostic. Writes nothing to the save.
    pub(super) fn install_fresh_lead(&mut self, battler: BattlePokemon) {
        self.party_lead = Some(battler);
        self.party_lead_slot = NO_LEAD_SLOT;
        self.lead_hp_hidden_by_load = 0;
        self.undecodable_lead_retained = false;
    }

    /// Save flush: merges the live battler into its selected slot (the party
    /// half of `SavePlayerParty`). Without a battler, the party records and
    /// count are untouched; a zero count with a battler becomes one.
    pub(super) fn flush_lead_to_save(&mut self) {
        let slot = self.party_lead_slot;
        if let Some(lead) = &self.party_lead {
            self.save1.player_party[slot] = party::merge_into_save_pokemon(
                &Dex::new(),
                lead,
                &self.save1.player_party[slot],
                &mut self.lead_hp_hidden_by_load,
            );
            if self.save1.player_party_count == 0 {
                self.save1.player_party_count = 1;
            }
        }
    }

    /// Load and selection: rebuilds the lead from the saved party
    /// ([`party::select_active_battler`]), measuring what the load hid.
    pub(super) fn load_lead_from_save(&mut self) {
        self.copy_party_and_objects_from_save();
    }

    /// Whole-lead healing (`HealPlayerParty`, `script_pokemon_util.c:30-58`):
    /// clears the selected record's status, fills its HP, heals the battler,
    /// re-measures the offset, and merges. A lead beyond a nonzero stored
    /// count is merged without healing. On a PP-heal error the saved
    /// plaintext repair stays and nothing is merged. No battler is a no-op.
    pub(super) fn heal_whole_lead(&mut self, dex: &Dex) -> Result<(), BattleError> {
        let stored_count =
            usize::from(self.save1.player_party_count).min(self.save1.player_party.len());
        let slot = self.party_lead_slot;
        let Some(lead) = self.party_lead.as_mut() else {
            return Ok(());
        };
        if stored_count == 0 || slot < stored_count {
            self.save1.player_party[slot].status = 0;
            self.save1.player_party[slot].hp = self.save1.player_party[slot].max_hp;
            lead.heal(dex)?;
        }
        self.lead_hp_hidden_by_load =
            party::hp_hidden_by_load(dex, &self.save1.player_party[slot], lead);
        self.save1.player_party[slot] = party::merge_into_save_pokemon(
            dex,
            lead,
            &self.save1.player_party[slot],
            &mut self.lead_hp_hidden_by_load,
        );
        Ok(())
    }

    /// Reselection: re-scans the saved party for the active battler. Skipped
    /// at a zero stored count; a same-slot result keeps the live battler;
    /// failure keeps the previous selection and is logged under `context`.
    pub(super) fn reselect_lead_from_save(&mut self, dex: &Dex, context: &str) {
        if self.save1.player_party_count == 0 {
            return;
        }
        match party::select_active_battler(dex, &self.save1.player_party) {
            Ok((slot, mon)) => {
                if slot != self.party_lead_slot || self.party_lead.is_none() {
                    self.lead_hp_hidden_by_load =
                        party::hp_hidden_by_load(dex, &self.save1.player_party[slot], &mon);
                    self.party_lead = Some(mon);
                    self.party_lead_slot = slot;
                    self.undecodable_lead_retained = false;
                }
            }
            Err(err) => {
                eprintln!("{context}: {err} -- keeping the previously selected slot");
            }
        }
    }

    /// Whether the live lead is the battler backed by saved slot `slot`:
    /// the coupled form of "battler present and slot matches", for skipping
    /// the live lead while dormant slots are walked.
    pub(super) fn owns_lead_slot(&self, slot: usize) -> bool {
        self.party_lead.is_some() && self.party_lead_slot == slot
    }
}
