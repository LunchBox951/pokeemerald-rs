//! The saved party behind a trainer battle's lead: the Route 103 rival and the sight
//! trainers hand over the active member and every healthy saved reserve, and a finished
//! fight files each member that entered back into its own saved slot.
//!
//! The roster, the per-slot owners and the reconciliation are the wild flow's
//! ([`OverworldPhase::saved_reserve_owners`], [`OverworldPhase::file_battle_party`]), so a
//! reserve merges under the same secure-record and hidden-HP rules and members that never
//! enter, eggs and undecodable records keep their saved bytes. What a trainer adds is the
//! pre-flight against the trainer's own party for each reserve
//! ([`npc_trainer_battle::ensure_reserve_can_enter`]).
//!
//! A trainer battle is refused, before any draw, while a saved member that upstream counts
//! toward the party's HP total cannot join the roster
//! ([`NpcTrainerBattleError::UnplayablePartyMember`]). A started battle's roster is
//! therefore every member with HP, so its [`battle::BattleOutcome::PlayerLost`] is the
//! whole party's HP total reaching zero (`Cmd_checkteamslost`,
//! `pokeemerald/src/battle_script_commands.c:3534-3564`) and always ends in the white-out.

use assets::trainers::TrainerId;
use battle::{BattleOutcome, BattlePokemon, Dex};

use crate::flow::battle_finalize::PartyBattleEnd;
use crate::flow::npc_trainer_battle::{self, NpcTrainerBattleError};
use crate::party::LoadedLead;

use super::OverworldPhase;

/// The saved members beside a trainer battle's lead.
///
/// `owners` holds one owner per [`battle::Battle::player_members`] position after the
/// first, so position `i + 1` is `owners[i]`'s saved slot. A member that never enters keeps
/// the owner's untouched record.
#[derive(Debug, Default)]
pub(in crate::flow) struct PartyReserves {
    owners: Vec<LoadedLead>,
}

/// A finished trainer battle's party once filed into the save.
#[derive(Debug)]
pub(super) struct FiledParty {
    entered_owners: Vec<LoadedLead>,
    files_party: bool,
}

impl OverworldPhase {
    /// Every healthy saved member other than the lead, as the reserves of a fight with
    /// `trainer`, with the battlers lent for the battle in the same order. Draws nothing
    /// and logs nothing.
    ///
    /// # Errors
    ///
    /// [`NpcTrainerBattleError::UnplayablePartyMember`] when a saved member that counts
    /// toward the party's HP total cannot join the roster: one
    /// [`OverworldPhase::saved_reserve_owners`] reports, or one the trainer's party cannot
    /// be fought with ([`npc_trainer_battle::ensure_reserve_can_enter`]).
    pub(super) fn trainer_party_reserves(
        &self,
        trainer: TrainerId,
    ) -> Result<(Vec<BattlePokemon>, PartyReserves), NpcTrainerBattleError> {
        let dex = Dex::new();
        let (candidates, unplayable) = self.saved_reserve_owners(&dex);
        if let Some(reason) = unplayable.into_iter().next() {
            return Err(NpcTrainerBattleError::UnplayablePartyMember(reason));
        }
        let mut owners = Vec::new();
        let mut lent = Vec::new();
        for mut owner in candidates {
            npc_trainer_battle::ensure_reserve_can_enter(trainer, owner.battler()).map_err(
                |error| {
                    NpcTrainerBattleError::UnplayablePartyMember(format!(
                        "slot {} can't enter the fight ({error})",
                        owner.slot()
                    ))
                },
            )?;
            // Classified against its own record like the lead, so its award is boosted too.
            let record = *owner.record();
            let battler = owner.take_battler();
            lent.push(self.classified_against_record(Some(&record), battler));
            owners.push(owner);
        }
        Ok((lent, PartyReserves { owners }))
    }

    /// Files a finished trainer battle's party: the lead returns to its owner and each
    /// reserve that entered is merged into its own saved slot. Runs before any flag or
    /// white-out, which read the filed party.
    pub(super) fn file_trainer_party(
        &mut self,
        end: PartyBattleEnd,
        reserves: PartyReserves,
    ) -> FiledParty {
        let PartyBattleEnd {
            members, entered, ..
        } = end;
        let PartyReserves { owners } = reserves;
        let files_party = !owners.is_empty();
        // The lead is filed into its saved slot whenever a party is saved, so a lone lead's
        // experience and PP reach the save like an entered reserve's. An unsaved party
        // (count 0) has no slot to file into.
        let flushes_lead = files_party || self.save1.player_party_count != 0;
        let entered_owners =
            self.file_battle_party(&Dex::new(), members, entered, owners, flushes_lead);
        FiledParty {
            entered_owners,
            files_party,
        }
    }

    /// The party's consequence once its fight has ended: the white-out of a defeated party,
    /// otherwise the field lead re-selected when the party was filed, since the member that
    /// ends the battle active need not be the one that began it.
    pub(super) fn settle_trainer_party(
        &mut self,
        outcome: Option<BattleOutcome>,
        filed: FiledParty,
    ) {
        let FiledParty {
            entered_owners,
            files_party,
        } = filed;
        if outcome == Some(BattleOutcome::PlayerLost) {
            self.white_out();
        } else if files_party {
            self.select_lead_after_wild_battle(&Dex::new(), entered_owners);
        }
    }
}
