//! Wild-battle handling for [`OverworldPhase`]: screening a map's encounter
//! table, starting a battle from a fired encounter, and advancing it one frame
//! at a time. The turn engine and the RNG adapter it shares with the overworld
//! live in [`crate::flow::wild_encounter`].

use crate::flow::battle_finalize::PartyBattleEnd;
use crate::flow::lead_screen::ensure_lead_can_battle;
use crate::flow::wild_encounter;
use crate::party::{LoadedLead, PartyError};
use battle::Dex;

use super::{ActiveBattle, OverworldPhase};

/// A wild battle and the saved party members beside its lead.
///
/// The lead's own [`LoadedLead`] stays in the phase, lent for the battle; `reserves` owns
/// the other members the battle may send out, one per [`battle::Battle::player_members`]
/// position after the first, so position `i + 1` is `reserves[i]`'s saved slot. A member
/// that never enters keeps the owner's untouched record.
///
/// `unplayable_member_remains` is set when a saved member that upstream counts toward the
/// party's HP total could not join the roster, so the battle's own exhaustion is not the
/// party's.
#[derive(Debug)]
pub(in crate::flow) struct WildBattle {
    pub(in crate::flow) battle: battle::Battle,
    reserves: Vec<LoadedLead>,
    unplayable_member_remains: bool,
}

#[cfg(test)]
impl WildBattle {
    /// A battle for a party that is only its lead.
    pub(in crate::flow) fn without_reserves(battle: battle::Battle) -> Self {
        Self {
            battle,
            reserves: Vec::new(),
            unplayable_member_remains: false,
        }
    }
}

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

    /// Gates the encounter roll, since `Battle::new` refuses an inadmissible lead, or one
    /// holding an item with a battle effect, only after `CreateWildMon`'s draws; a missing lead passes to `begin_wild_battle`.
    pub(in crate::flow) fn wild_lead_admissible(&mut self) -> bool {
        let refusal = self
            .lead_battler()
            .and_then(|lead| ensure_lead_can_battle(lead).err());
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
    /// the party's members return to their owners through
    /// [`OverworldPhase::complete_wild_battle`] before any white-out runs.
    ///
    /// A [`battle::BattleOutcome::PlayerLost`] outcome, which the battle reports
    /// only once every member of its roster has fainted, runs
    /// [`OverworldPhase::white_out`] in the same frame, so no frame observes a
    /// fainted party. Every ending, a loss
    /// included, then re-initialises the field tileset animations, as returning
    /// to the field does.
    pub(super) fn advance_wild_battle_frame(&mut self, wild: WildBattle) -> Option<ActiveBattle> {
        let WildBattle {
            battle,
            reserves,
            unplayable_member_remains,
        } = wild;
        let mut slot = Some(battle);
        let Some(end) = wild_encounter::advance_wild_battle(&mut slot, &mut self.rng) else {
            return slot.map(|battle| {
                ActiveBattle::Wild(WildBattle {
                    battle,
                    reserves,
                    unplayable_member_remains,
                })
            });
        };
        self.complete_wild_battle(end, reserves, unplayable_member_remains);
        self.reinit_field_tileset_animations();
        None
    }

    /// Files a finished wild battle's party back into the save and consumes its outcome.
    ///
    /// Position 0 returns to the lent lead; each reserve that was sent out returns to its
    /// own owner and is merged into its own saved slot, so members never sent out, eggs and
    /// undecodable records keep their saved bytes. With reserves, or a member left out of
    /// the roster, the lead is filed as well and the field lead is re-selected
    /// ([`Self::select_lead_after_wild_battle`]), since the member that ends the battle
    /// active need not be the one that began it. A lone lead is only handed back.
    ///
    /// The battle reports `PlayerLost` once its roster has fainted, which is the party's
    /// defeat (`Cmd_checkteamslost`, `src/battle_script_commands.c:3534-3564`;
    /// `CB2_EndWildBattle`, `src/battle_setup.c:602-616`) unless a healthy member was left
    /// out of the roster. Then upstream's HP total is not zero, so nothing whites out.
    fn complete_wild_battle(
        &mut self,
        end: PartyBattleEnd,
        reserves: Vec<LoadedLead>,
        unplayable_member_remains: bool,
    ) {
        let dex = Dex::new();
        let PartyBattleEnd {
            outcome,
            members,
            entered,
        } = end;
        let mut members = members.into_iter();
        let lead = members
            .next()
            .expect("a battle always has an active member");
        self.restore_lead_battler(lead);
        let files_party = !reserves.is_empty() || unplayable_member_remains;
        let mut entered_owners = Vec::new();
        for (member, mut owner) in members.zip(reserves).take(entered.saturating_sub(1)) {
            owner.restore_battler(member);
            self.save1.player_party[owner.slot()] = owner.merge_and_save(&dex);
            entered_owners.push(owner);
        }
        if files_party {
            self.flush_lead_to_save();
        }
        if let Some(outcome) = outcome {
            eprintln!("wild battle: ended -- {outcome:?}");
        }
        let party_defeated =
            outcome == Some(battle::BattleOutcome::PlayerLost) && !unplayable_member_remains;
        if party_defeated {
            self.white_out();
        } else if files_party {
            self.select_lead_after_wild_battle(&dex, entered_owners);
        }
    }

    /// The field lead once a wild battle has filed its party: the first usable saved slot,
    /// as [`Self::reselect_lead_from_save`] picks. A slot held by one of the `entered`
    /// owners keeps that owner, whose live battler carries the state a save record does not
    /// (status, and the hidden-HP offset it has rebased); another slot is decoded from its
    /// record, which the battle never changed. A failed selection keeps the previous lead.
    fn select_lead_after_wild_battle(&mut self, dex: &Dex, mut entered: Vec<LoadedLead>) {
        let selected = match LoadedLead::load(dex, &self.save1.player_party) {
            Ok(selected) => selected,
            Err(error) => {
                eprintln!("wild battle: {error} -- keeping the previously selected slot");
                return;
            }
        };
        if self.owns_lead_slot(selected.slot()) {
            return;
        }
        let owner = entered
            .iter()
            .position(|owner| owner.slot() == selected.slot())
            .map_or(selected, |index| entered.swap_remove(index));
        self.lead = Some(owner);
        self.undecodable_lead_retained = false;
    }

    /// The saved members other than the lead that may be sent out, in slot order, each with
    /// its own owner, and whether a member that counts toward upstream's party HP total was
    /// left out. Every slot is inspected, not only the stored count's prefix
    /// (`SetBattlePartyIds` scans all six, `pokeemerald/src/battle_controllers.c:591-606`),
    /// and an unsaved party (count 0) has no reserves. Left out, and so retained untouched:
    /// eggs and Bad Eggs, records that are empty, fainted members, and the members that
    /// have HP but cannot join: records that do not decode, and members the battle engine
    /// cannot play yet ([`ensure_lead_can_battle`]).
    fn wild_reserve_owners(&self, dex: &Dex) -> (Vec<LoadedLead>, bool) {
        let stored = usize::from(self.save1.player_party_count).min(self.save1.player_party.len());
        let Some(lead_slot) = self.lead.as_ref().map(LoadedLead::slot) else {
            return (Vec::new(), false);
        };
        if stored == 0 {
            return (Vec::new(), false);
        }
        let mut owners = Vec::new();
        let mut unplayable_member_remains = false;
        for (slot, record) in self.save1.player_party.iter().enumerate() {
            if slot == lead_slot {
                continue;
            }
            let owner = match LoadedLead::load_slot(dex, slot, record) {
                Ok(owner) => owner,
                Err(PartyError::Egg) => continue,
                Err(error) => {
                    if record.box_data.has_species() && record.hp != 0 {
                        eprintln!("wild encounter: slot {slot}'s record {error} -- left out");
                        unplayable_member_remains = true;
                    }
                    continue;
                }
            };
            if owner.battler().is_fainted() {
                continue;
            }
            if let Err(error) = ensure_lead_can_battle(owner.battler()) {
                eprintln!(
                    "wild encounter: slot {slot} can't enter a battle ({error:?}) -- left out"
                );
                unplayable_member_remains = true;
                continue;
            }
            owners.push(owner);
        }
        (owners, unplayable_member_remains)
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
        // `CreateWildMon` consults party slot 0, which is not the battle lead when it is
        // fainted; an unsaved party (count 0) has the live lead as its whole party.
        let stored = usize::from(self.save1.player_party_count).min(self.save1.player_party.len());
        let dex = Dex::new();
        let (synchronize_nature, cute_charm_gender) = if stored == 0 {
            (
                (lead.ability() == assets::AbilityId::SYNCHRONIZE).then(|| lead.nature()),
                crate::party::live_lead_cute_charm_wild_gender(&dex, &lead),
            )
        } else {
            let saved_party = &self.save1.player_party[..stored];
            (
                crate::party::slot_zero_synchronize_nature(&dex, saved_party),
                crate::party::slot_zero_cute_charm_wild_gender(&dex, saved_party),
            )
        };
        let (mut reserves, unplayable_member_remains) = self.wild_reserve_owners(&dex);
        let player_reserves = reserves.iter_mut().map(LoadedLead::take_battler).collect();
        // The start consumes its copies; the lent original returns on rejection.
        match wild_encounter::start_wild_battle_with_lead_effects(
            lead.clone(),
            player_reserves,
            synchronize_nature,
            cute_charm_gender,
            encounter,
            player_trainer_id,
            &mut self.rng,
        ) {
            Ok(battle) => {
                self.active_battle = Some(ActiveBattle::Wild(WildBattle {
                    battle,
                    reserves,
                    unplayable_member_remains,
                }));
            }
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
