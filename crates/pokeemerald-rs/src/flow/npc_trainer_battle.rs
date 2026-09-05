//! Constructs NPC trainer parties and drives their headless battles.
//!
//! # Personality construction
//!
//! Upstream's `CreateNPCTrainerParty` carries one wrapping name-byte sum across the whole
//! party (`battle_main.c:1960-2073`). Each member adds the trainer name again and then its
//! species name before the sum becomes part of its personality. The names use the Gen III
//! character encoding, not ASCII.
//!
//! # RNG stream
//!
//! [`start_npc_trainer_battle`] uses the overworld's shared generator. Fixed personalities
//! and IVs consume no draws. Each constructed party member draws a non-shiny OT ID before
//! [`battle::Battle::new_trainer`] draws its initial turn state. Trainer battles consume no
//! wild held-item draw because upstream's `SetWildMonHeldItem` excludes them
//! (`pokemon.c:6678-6682`).
//!
//! # Nothing is built before the whole party is screened
//!
//! Every rejection occurs before the first RNG draw. This keeps repeated sight-cone checks
//! from changing the shared stream while a trainer party remains unsupported.
//!
//! # Held-item parties
//!
//! [`battle::BattlePokemon`] cannot represent held items. This module rejects either
//! held-item party shape instead of silently discarding its items.

use assets::trainers::{TrainerData, TrainerId, TrainerParty};
use assets::{MoveId, SpeciesNames};
use battle::{Battle, BattleError, BattleEvent, BattleOutcome, BattlePokemon, Dex, PlayerAction};
use engine::rng::Rng;

use super::battle_finalize::finalize_battle_turn;
use super::move_learn::settle_move_learn_prompts;
use super::wild_encounter::SharedRng;

/// Maximum wallet balance.
pub const MAX_MONEY: u32 = 999_999;

fn credit_money(money: &mut u32, amount: u32) {
    *money = money.saturating_add(amount).min(MAX_MONEY);
}

/// Why an NPC trainer battle could not be constructed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NpcTrainerBattleError {
    /// The party or battle violates a battle-engine requirement.
    Battle(BattleError),
    /// A trainer or species name has no Gen III encoding for its character.
    UnnamedCharacter { name: &'static str, character: char },
    /// The trainer's party carries held items, which are not modelled.
    HeldItemParty(TrainerId),
}

impl core::fmt::Display for NpcTrainerBattleError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Battle(error) => write!(f, "{error}"),
            Self::UnnamedCharacter { name, character } => write!(
                f,
                "name `{name}` contains `{character}`, which has no Gen-3 charmap byte"
            ),
            Self::HeldItemParty(id) => write!(
                f,
                "trainer `{}` fields held items, which are not modelled",
                id.0
            ),
        }
    }
}

impl std::error::Error for NpcTrainerBattleError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Battle(error) => Some(error),
            Self::UnnamedCharacter { .. } | Self::HeldItemParty(_) => None,
        }
    }
}

impl From<BattleError> for NpcTrainerBattleError {
    fn from(error: BattleError) -> Self {
        Self::Battle(error)
    }
}

const DOUBLE_BATTLE_PERSONALITY_BASE: u32 = 0x80;
const FEMALE_TRAINER_PERSONALITY_BASE: u32 = 0x78;
const MALE_TRAINER_PERSONALITY_BASE: u32 = 0x88;
const NAME_HASH_PERSONALITY_SHIFT: u32 = 8;
const HEADLESS_PLAYER_MOVE_SLOT: usize = 0;

#[must_use]
fn personality_base(trainer: &TrainerData) -> u32 {
    if trainer.double_battle {
        DOUBLE_BATTLE_PERSONALITY_BASE
    } else if trainer.encounter_music.is_female {
        FEMALE_TRAINER_PERSONALITY_BASE
    } else {
        MALE_TRAINER_PERSONALITY_BASE
    }
}

fn add_encoded_name_to_hash(
    hash: &mut u32,
    name: &'static str,
) -> Result<(), NpcTrainerBattleError> {
    for character in name.chars() {
        let byte = engine::text::char_to_byte(character)
            .ok_or(NpcTrainerBattleError::UnnamedCharacter { name, character })?;
        *hash = hash.wrapping_add(u32::from(byte));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PartyEntry {
    species: assets::SpeciesId,
    level: u8,
    unscaled_iv: u8,
    custom_moves: Option<Vec<MoveId>>,
}

fn party_entries(
    id: TrainerId,
    trainer: &TrainerData,
) -> Result<Vec<PartyEntry>, NpcTrainerBattleError> {
    let entries = match trainer.party {
        TrainerParty::NoItemDefaultMoves(party) => party
            .iter()
            .map(|mon| PartyEntry {
                species: mon.species,
                level: mon.lvl,
                unscaled_iv: mon.iv,
                custom_moves: None,
            })
            .collect(),
        TrainerParty::NoItemCustomMoves(party) => party
            .iter()
            .map(|mon| PartyEntry {
                species: mon.species,
                level: mon.lvl,
                unscaled_iv: mon.iv,
                custom_moves: Some(
                    mon.moves
                        .iter()
                        .copied()
                        .filter(|move_id| *move_id != battle::MOVE_NONE)
                        .collect(),
                ),
            })
            .collect(),
        TrainerParty::ItemDefaultMoves(_) | TrainerParty::ItemCustomMoves(_) => {
            return Err(NpcTrainerBattleError::HeldItemParty(id))
        }
    };
    Ok(entries)
}

fn party_personalities(
    trainer: &TrainerData,
    entries: &[PartyEntry],
) -> Result<Vec<u32>, NpcTrainerBattleError> {
    let names = SpeciesNames::new();
    let base = personality_base(trainer);

    let mut name_hash: u32 = 0;
    let mut personalities = Vec::with_capacity(entries.len());
    for entry in entries {
        add_encoded_name_to_hash(&mut name_hash, trainer.name)?;
        let species_name = names.name(entry.species).map_err(|_| {
            NpcTrainerBattleError::Battle(BattleError::UnknownSpecies(entry.species))
        })?;
        add_encoded_name_to_hash(&mut name_hash, species_name)?;
        personalities.push(base.wrapping_add(name_hash.wrapping_shl(NAME_HASH_PERSONALITY_SHIFT)));
    }
    Ok(personalities)
}

/// Computes each party member's fixed personality without consuming RNG.
///
/// The wrapping Gen III name-byte hash carries across party members as described in the
/// module contract.
///
/// # Errors
///
/// Returns [`NpcTrainerBattleError::UnnamedCharacter`] when a name cannot be encoded,
/// [`NpcTrainerBattleError::HeldItemParty`] for a held-item party, or a battle data error.
#[cfg(test)]
pub fn trainer_party_personalities(id: TrainerId) -> Result<Vec<u32>, NpcTrainerBattleError> {
    let trainer = battle::trainer_data(id)?;
    let entries = party_entries(id, trainer)?;
    party_personalities(trainer, &entries)
}

/// Builds a trainer's party and starts its battle on the shared RNG stream.
///
/// All validation finishes before party construction, so every error leaves `rng`
/// unchanged. Successful construction draws each party member's OT ID before battle
/// initialization.
///
/// # Errors
///
/// Returns [`NpcTrainerBattleError`] when the lead, trainer data, or party cannot start a
/// supported battle.
pub fn start_npc_trainer_battle(
    player_lead: BattlePokemon,
    trainer: TrainerId,
    rng: &mut Rng,
) -> Result<Battle, NpcTrainerBattleError> {
    if player_lead.is_fainted() {
        return Err(BattleError::FaintedBattler(true).into());
    }
    let dex = Dex::new();
    let data = battle::trainer_data(trainer)?;
    let entries = party_entries(trainer, data)?;
    let personalities = party_personalities(data, &entries)?;

    let movesets: Vec<Vec<MoveId>> = entries
        .iter()
        .map(|entry| {
            entry
                .custom_moves
                .clone()
                .unwrap_or_else(|| battle::initial_moveset(entry.species, entry.level))
        })
        .collect();
    let specs: Vec<battle::TrainerPartyMon<'_>> = entries
        .iter()
        .zip(&movesets)
        .map(|(entry, moves)| battle::TrainerPartyMon {
            species: entry.species,
            level: entry.level,
            moves,
        })
        .collect();
    battle::ensure_trainer_party_startable(&dex, trainer, &specs)?;

    let mut party = Vec::with_capacity(entries.len());
    for ((entry, personality), moves) in entries.iter().zip(personalities).zip(movesets) {
        party.push(battle::build_trainer_pokemon(
            &dex,
            entry.species,
            entry.level,
            battle::fixed_ivs(entry.unscaled_iv),
            personality,
            moves,
            &mut SharedRng::new(rng),
        )?);
    }

    Ok(Battle::new_trainer(
        dex,
        player_lead,
        trainer,
        party,
        &mut SharedRng::new(rng),
    )?)
}

fn credit_reward_events(money: &mut u32, events: impl IntoIterator<Item = BattleEvent>) {
    for event in events {
        if let BattleEvent::MoneyGained(amount) = event {
            credit_money(money, amount);
        }
    }
}

/// Advances a headless trainer battle by one turn and settles its resulting state.
///
/// With no battle menu, the driver selects the first move slot. It answers pending move
/// replacement prompts, credits any reward events, and writes back the player's lead when
/// the battle ends. `None` can mean no active battle, an ongoing battle, or a failed turn;
/// callers inspect `battle_slot` to distinguish an active battle.
pub fn advance_npc_trainer_battle(
    battle_slot: &mut Option<Battle>,
    player_lead: &mut Option<BattlePokemon>,
    money: &mut u32,
    rng: &mut Rng,
) -> Option<BattleOutcome> {
    let battle = battle_slot.as_mut()?;
    let player_action = PlayerAction::UseMove(HEADLESS_PLAYER_MOVE_SLOT);
    let turn_failed = match battle.take_turn(player_action, &mut SharedRng::new(rng)) {
        Ok(events) => {
            credit_reward_events(money, events);
            false
        }
        Err(error) => {
            eprintln!("npc trainer battle: turn failed ({error:?}) -- ending the battle");
            true
        }
    };
    credit_reward_events(money, settle_move_learn_prompts(battle));
    finalize_battle_turn(battle_slot, turn_failed, player_lead)
}

#[cfg(test)]
mod tests {
    use super::*;
    use assets::SpeciesId;

    const TREECKO: SpeciesId = SpeciesId(277);
    const SLASH: MoveId = MoveId(163);
    const TEST_LEVEL: u8 = 50;
    const TEST_PARTY_IV: u8 = 31;
    const TEST_PERSONALITY: u32 = 0;
    const UNKNOWN_TRAINER: TrainerId = TrainerId(u16::MAX);

    #[test]
    fn fainted_player_lead_is_rejected_before_trainer_lookup_or_rng_draw() {
        let mut lead = BattlePokemon::new(
            &Dex::new(),
            TREECKO,
            TEST_LEVEL,
            battle::fixed_ivs(TEST_PARTY_IV),
            TEST_PERSONALITY,
            vec![SLASH],
        )
        .expect("Treecko/Slash is a valid pairing");
        lead.apply_damage(u32::MAX);
        assert!(lead.is_fainted(), "setup: the lead really is fainted");

        let mut rng = Rng::new(1);
        let before = rng.state();
        let result = start_npc_trainer_battle(lead, UNKNOWN_TRAINER, &mut rng);
        assert_eq!(
            result.err(),
            Some(NpcTrainerBattleError::Battle(BattleError::FaintedBattler(
                true
            ))),
            "the lead must be rejected before the unknown trainer is looked up"
        );
        assert_eq!(
            rng.state(),
            before,
            "a refused construction must draw nothing at all"
        );
    }
}
