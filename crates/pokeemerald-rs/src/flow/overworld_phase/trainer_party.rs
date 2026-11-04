//! The saved party behind a trainer battle's lead: the Route 103 rival and the sight
//! trainers hand over the active member and every eligible saved reserve, and a finished
//! fight files each member that entered back into its own saved slot.
//!
//! The roster, the per-slot owners and the reconciliation are the wild flow's
//! ([`OverworldPhase::wild_reserve_owners`], [`OverworldPhase::file_battle_party`]), so a
//! reserve merges under the same secure-record and hidden-HP rules and members that never
//! enter, eggs and undecodable records keep their saved bytes. What a trainer adds is the
//! pre-flight against the trainer's own party for each reserve
//! ([`npc_trainer_battle::ensure_reserve_can_enter`]), so a reserve the battle crate would
//! refuse after the party's OT-id draws is left out beforehand instead.
//!
//! A [`battle::BattleOutcome::PlayerLost`] ends in a white-out only when no healthy saved
//! member was left out of the roster: the battle reports the loss once its roster has
//! fainted, and upstream's loss is the whole party's HP total reaching zero
//! (`Cmd_checkteamslost`, `pokeemerald/src/battle_script_commands.c:3534-3564`).

use assets::trainers::TrainerId;
use battle::{BattleOutcome, BattlePokemon, Dex};

use crate::flow::battle_finalize::PartyBattleEnd;
use crate::flow::npc_trainer_battle;
use crate::party::LoadedLead;

use super::OverworldPhase;

/// The saved members beside a trainer battle's lead.
///
/// `owners` holds one owner per [`battle::Battle::player_members`] position after the
/// first, so position `i + 1` is `owners[i]`'s saved slot. A member that never enters keeps
/// the owner's untouched record.
///
/// `unplayable_member_remains` is set when a saved member that upstream counts toward the
/// party's HP total could not join the roster, so the battle's own exhaustion is not the
/// party's.
#[derive(Debug, Default)]
pub(in crate::flow) struct PartyReserves {
    owners: Vec<LoadedLead>,
    unplayable_member_remains: bool,
}

/// A finished trainer battle's party once filed into the save.
#[derive(Debug)]
pub(super) struct FiledParty {
    entered_owners: Vec<LoadedLead>,
    files_party: bool,
    unplayable_member_remains: bool,
}

impl OverworldPhase {
    /// The saved members other than the lead that can be sent out against `trainer`, with
    /// the battlers lent for the battle in the same order. A reserve the trainer's party
    /// cannot be fought with is left out and keeps its saved bytes.
    pub(super) fn trainer_party_reserves(
        &self,
        trainer: TrainerId,
    ) -> (Vec<BattlePokemon>, PartyReserves) {
        let dex = Dex::new();
        let (candidates, mut unplayable_member_remains) = self.wild_reserve_owners(&dex);
        let mut owners = Vec::new();
        let mut lent = Vec::new();
        for mut owner in candidates {
            if let Err(error) =
                npc_trainer_battle::ensure_reserve_can_enter(trainer, owner.battler())
            {
                eprintln!(
                    "trainer battle: slot {} can't enter the fight ({error}) -- left out",
                    owner.slot()
                );
                unplayable_member_remains = true;
                continue;
            }
            // Classified against its own record like the lead, so its award is boosted too.
            let record = *owner.record();
            let battler = owner.take_battler();
            lent.push(self.classified_against_record(Some(&record), battler));
            owners.push(owner);
        }
        (
            lent,
            PartyReserves {
                owners,
                unplayable_member_remains,
            },
        )
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
        let PartyReserves {
            owners,
            unplayable_member_remains,
        } = reserves;
        let files_party = !owners.is_empty() || unplayable_member_remains;
        let entered_owners =
            self.file_battle_party(&Dex::new(), members, entered, owners, files_party);
        FiledParty {
            entered_owners,
            files_party,
            unplayable_member_remains,
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
            unplayable_member_remains,
        } = filed;
        if outcome == Some(BattleOutcome::PlayerLost) && !unplayable_member_remains {
            self.white_out();
        } else if files_party {
            self.select_lead_after_wild_battle(&Dex::new(), entered_owners);
        }
    }
}
