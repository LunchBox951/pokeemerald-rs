//! The lead owner: `OverworldPhase`'s one [`party::LoadedLead`], behind the
//! interface every flow adopter uses.
//!
//! The battle-facing lead, the saved slot it was decoded from, the backing
//! record, and the HP the load clamp hid from it are one fact, and upstream
//! never synchronises them as unrelated values
//! (`pokeemerald/src/pokemon.c:2823-2895`). [`party::LoadedLead`] owns all of
//! them, so none can move without the others. The phase files the owned record
//! into the save only at [`party::LoadedLead::slot`]; the interface returns
//! the battler, never the slot or the offset.

use battle::{BattleError, BattlePokemon, Dex};

use super::OverworldPhase;
use crate::party::{self, LoadedLead};

impl OverworldPhase {
    /// The lead whose battler is home: absent when no lead is loaded or a
    /// battle has the battler out on loan.
    fn resident_lead(&self) -> Option<&LoadedLead> {
        self.lead.as_ref().filter(|lead| !lead.is_lent())
    }

    fn resident_lead_mut(&mut self) -> Option<&mut LoadedLead> {
        self.lead.as_mut().filter(|lead| !lead.is_lent())
    }

    /// Battler observation: the lead in battle-ready form, if any.
    pub(super) fn lead_battler(&self) -> Option<&BattlePokemon> {
        self.resident_lead().map(LoadedLead::battler)
    }

    /// Battler mutation. Exposes the battler only, so the slot, record, and
    /// hidden-HP offset cannot be edited independently of it.
    pub(super) fn lead_battler_mut(&mut self) -> Option<&mut BattlePokemon> {
        self.resident_lead_mut().map(LoadedLead::battler_mut)
    }

    /// The lead classified against the save owner for a battle start:
    /// `IsTradedMon` (`pokeemerald/src/pokemon.c:6602-6626`) compares the
    /// backing record's OT id and OT name with the owner's. The record backs
    /// the lead only if its personality and OT id match the battler's, so a
    /// lead installed without a saved record is left as is.
    fn classified_for_battle(&self, lead: BattlePokemon) -> BattlePokemon {
        let owner_id = u32::from_le_bytes(self.save2.player_trainer_id);
        let traded = self
            .lead
            .as_ref()
            .map(LoadedLead::record)
            .is_some_and(|record| {
                record.box_data.personality() == lead.personality()
                    && record.box_data.ot_id() == lead.original_trainer_id()
                    && !party::has_unstamped_owner_name(record, owner_id)
                    && party::is_traded_record(record, owner_id, &self.save2.player_name)
            });
        let traded = traded || lead.is_traded_recipient();
        lead.with_traded_recipient(traded)
    }

    /// A copy of the resident lead for a battle start, classified against the
    /// save owner (see [`Self::take_lead_for_battle`]).
    pub(super) fn lead_for_battle(&self) -> Option<BattlePokemon> {
        let lead = self.lead_battler()?.clone();
        Some(self.classified_for_battle(lead))
    }

    /// [`Self::take_lead_battler`] for a battle start: the lent battler is
    /// classified against the save owner, so the traded-recipient experience
    /// bonus reaches every overworld battle.
    ///
    /// # Panics
    ///
    /// As [`Self::take_lead_battler`] does.
    pub(super) fn take_lead_for_battle(&mut self) -> Option<BattlePokemon> {
        let lead = self.take_lead_battler()?;
        Some(self.classified_for_battle(lead))
    }

    /// Lend: moves the battler out for a battle. The lead keeps its slot,
    /// record, and hidden-HP offset, so [`Self::restore_lead_battler`] merges
    /// back into the same record. `None` when no lead is loaded.
    ///
    /// # Panics
    ///
    /// As [`party::LoadedLead::take_battler`] does: panics if the battler is
    /// already lent out.
    pub(super) fn take_lead_battler(&mut self) -> Option<BattlePokemon> {
        self.lead.as_mut().map(LoadedLead::take_battler)
    }

    /// Restore: returns the battler a battle borrowed to the lead that lent it.
    ///
    /// # Panics
    ///
    /// As [`party::LoadedLead::restore_battler`] does: panics if the battler
    /// was not lent out by [`Self::take_lead_battler`] (an absent lead that
    /// was never loaded, such as a zero-count or undecodable save, does not
    /// count), or if `battler` is not the mon that was lent (personality and
    /// original trainer id of the owned record).
    pub(super) fn restore_lead_battler(&mut self, battler: BattlePokemon) {
        let Some(lead) = self.lead.as_mut() else {
            panic!("the loaded lead's battler was never lent out");
        };
        lead.restore_battler(battler);
    }

    /// Installs a lead with no saved backing: slot 0, no hidden HP, and no
    /// retained-undecodable diagnostic. Its backing record is the fresh
    /// `to_save_pokemon` encoding of the battler, held by the lead and filed
    /// only by [`Self::flush_lead_to_save`]; the save is untouched here.
    pub(super) fn install_fresh_lead(&mut self, battler: BattlePokemon) {
        let dex = Dex::new();
        let record = party::to_save_pokemon(&dex, &battler);
        self.lead = Some(LoadedLead::created(&dex, battler, record));
        self.undecodable_lead_retained = false;
    }

    /// Save flush: merges the live battler into its selected slot (the party
    /// half of `SavePlayerParty`). Without a battler, the party records and
    /// count are untouched; a zero count with a battler becomes one.
    pub(super) fn flush_lead_to_save(&mut self) {
        let Some(lead) = self.lead.as_mut().filter(|lead| !lead.is_lent()) else {
            return;
        };
        self.save1.player_party[lead.slot()] = lead.merge_and_save(&Dex::new());
        if self.save1.player_party_count == 0 {
            self.save1.player_party_count = 1;
        }
    }

    /// Load and selection (the party half of `CopyPartyAndObjectsFromSave`,
    /// `LoadPlayerParty`, `src/load_save.c:160-205`): rebuilds the lead from
    /// the saved party through [`party::LoadedLead::load`], measuring what the
    /// load hid. Reads the saved party and count; writes no saved bytes.
    ///
    /// The lead is empty when the stored party count is zero (the port's
    /// deliberate difference from `SetBattlePartyIds`'s count-blind scan in
    /// `src/battle_controllers.c`) or when no slot decodes into a usable
    /// battler. The second case is logged and sets
    /// [`OverworldPhase::undecodable_lead_retained`]; no replacement starter
    /// is fabricated, because that would hand the player a Pokemon they did
    /// not save. The object-event half lives in [`OverworldPhase::from_saved`],
    /// because the player's facing must be known before the player state is
    /// built.
    pub(super) fn load_lead_from_save(&mut self) {
        if self.save1.player_party_count == 0 {
            self.set_no_party_lead(false);
            return;
        }
        // `SetBattlePartyIds` scans every party slot, not only the stored count.
        match LoadedLead::load(&Dex::new(), &self.save1.player_party) {
            Ok(lead) => {
                self.lead = Some(lead);
                self.undecodable_lead_retained = false;
            }
            Err(err) => {
                eprintln!(
                    "continue: {err} -- slot 0's record and stored party count are \
                     retained; resuming with an empty party"
                );
                // A decode failure is this port's limit, not proof the bytes
                // are junk, so the next SAVE must not erase them.
                self.set_no_party_lead(true);
            }
        }
    }

    fn set_no_party_lead(&mut self, undecodable_lead_retained: bool) {
        self.lead = None;
        self.undecodable_lead_retained = undecodable_lead_retained;
    }

    /// Whole-lead healing (`HealPlayerParty`, `script_pokemon_util.c:30-58`)
    /// through [`party::LoadedLead::heal_whole_lead`], filing the owned record
    /// at its slot. A lead beyond a nonzero stored count is merged without
    /// healing. On a PP-heal error the record keeps its status and HP repair
    /// and is filed without a merge. No battler is a no-op.
    pub(super) fn heal_whole_lead(&mut self, dex: &Dex) -> Result<(), BattleError> {
        let player_party_count = self.save1.player_party_count;
        let Some(lead) = self.lead.as_mut().filter(|lead| !lead.is_lent()) else {
            return Ok(());
        };
        let healed = lead.heal_whole_lead(dex, player_party_count);
        self.save1.player_party[lead.slot()] = *lead.record();
        healed
    }

    /// Reselection: re-scans the saved party for the active battler. Skipped
    /// at a zero stored count; a same-slot result keeps the live battler;
    /// failure keeps the previous selection and is logged under `context`.
    pub(super) fn reselect_lead_from_save(&mut self, dex: &Dex, context: &str) {
        if self.save1.player_party_count == 0 {
            return;
        }
        match LoadedLead::load(dex, &self.save1.player_party) {
            Ok(selected) => {
                let kept = self
                    .resident_lead()
                    .is_some_and(|lead| lead.slot() == selected.slot());
                if !kept {
                    self.lead = Some(selected);
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
        self.resident_lead().is_some_and(|lead| lead.slot() == slot)
    }
}
