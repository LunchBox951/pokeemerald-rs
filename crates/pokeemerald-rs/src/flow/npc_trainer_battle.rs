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
//! Both held-item party shapes carry their items onto the built [`battle::BattlePokemon`],
//! as `CreateNPCTrainerParty` writes `MON_DATA_HELD_ITEM` after `CreateMon`
//! (`battle_main.c:2035-2065`). The battle crate then refuses an item whose effect it does
//! not model ([`battle::ensure_held_item_playable`]) with the rest of the pre-flight, before
//! the first draw.

use assets::items::ItemId;
use assets::trainers::{TrainerData, TrainerId, TrainerParty};
use assets::{MoveId, SpeciesNames};
use battle::{Battle, BattleError, BattleEvent, BattleOutcome, BattlePokemon, Dex};
use engine::rng::Rng;

use super::battle_finalize::finalize_battle_turn;
use super::first_usable_move::take_first_usable_move_turn;
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
}

impl core::fmt::Display for NpcTrainerBattleError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Battle(error) => write!(f, "{error}"),
            Self::UnnamedCharacter { name, character } => write!(
                f,
                "name `{name}` contains `{character}`, which has no Gen-3 charmap byte"
            ),
        }
    }
}

impl std::error::Error for NpcTrainerBattleError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Battle(error) => Some(error),
            Self::UnnamedCharacter { .. } => None,
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
    held_item: ItemId,
}

fn real_moves(moves: [MoveId; 4]) -> Vec<MoveId> {
    moves
        .iter()
        .copied()
        .filter(|move_id| *move_id != battle::MOVE_NONE)
        .collect()
}

fn party_entries(party: TrainerParty) -> Vec<PartyEntry> {
    match party {
        TrainerParty::NoItemDefaultMoves(party) => party
            .iter()
            .map(|mon| PartyEntry {
                species: mon.species,
                level: mon.lvl,
                unscaled_iv: mon.iv,
                custom_moves: None,
                held_item: ItemId::NONE,
            })
            .collect(),
        TrainerParty::NoItemCustomMoves(party) => party
            .iter()
            .map(|mon| PartyEntry {
                species: mon.species,
                level: mon.lvl,
                unscaled_iv: mon.iv,
                custom_moves: Some(real_moves(mon.moves)),
                held_item: ItemId::NONE,
            })
            .collect(),
        TrainerParty::ItemDefaultMoves(party) => party
            .iter()
            .map(|mon| PartyEntry {
                species: mon.species,
                level: mon.lvl,
                unscaled_iv: mon.iv,
                custom_moves: None,
                held_item: mon.held_item,
            })
            .collect(),
        TrainerParty::ItemCustomMoves(party) => party
            .iter()
            .map(|mon| PartyEntry {
                species: mon.species,
                level: mon.lvl,
                unscaled_iv: mon.iv,
                custom_moves: Some(real_moves(mon.moves)),
                held_item: mon.held_item,
            })
            .collect(),
    }
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
/// Returns [`NpcTrainerBattleError::UnnamedCharacter`] when a name cannot be encoded, or a
/// battle data error.
#[cfg(test)]
pub fn trainer_party_personalities(id: TrainerId) -> Result<Vec<u32>, NpcTrainerBattleError> {
    let trainer = battle::trainer_data(id)?;
    party_personalities(trainer, &party_entries(trainer.party))
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
    let entries = party_entries(data.party);
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
        .zip(&personalities)
        .map(|((entry, moves), &personality)| battle::TrainerPartyMon {
            species: entry.species,
            level: entry.level,
            moves,
            personality,
            held_item: entry.held_item,
        })
        .collect();
    battle::ensure_trainer_party_startable(&dex, &player_lead, trainer, &specs)?;
    battle::ensure_participant_admissible(&player_lead)?;
    battle::ensure_held_item_playable(player_lead.held_item())?;

    let mut party = Vec::with_capacity(entries.len());
    for ((entry, personality), moves) in entries.iter().zip(personalities).zip(movesets) {
        party.push(
            battle::build_trainer_pokemon(
                &dex,
                entry.species,
                entry.level,
                battle::fixed_ivs(entry.unscaled_iv),
                personality,
                moves,
                &mut SharedRng::new(rng),
            )?
            .with_held_item(entry.held_item),
        );
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
/// With no battle menu, the driver selects the first usable move slot (see
/// [`take_first_usable_move_turn`]). It answers pending move replacement prompts, credits
/// any reward events, and writes back the player's lead when the battle ends. `None` can
/// mean no active battle, an ongoing battle, or a failed turn; callers inspect
/// `battle_slot` to distinguish an active battle.
pub fn advance_npc_trainer_battle(
    battle_slot: &mut Option<Battle>,
    player_lead: &mut Option<BattlePokemon>,
    money: &mut u32,
    rng: &mut Rng,
) -> Option<BattleOutcome> {
    let battle = battle_slot.as_mut()?;
    let turn_failed = match take_first_usable_move_turn(battle, rng) {
        Ok(events) => {
            credit_reward_events(money, events);
            false
        }
        Err(error) => {
            eprintln!("npc trainer battle: turn failed ({error:?}) -- ending the battle");
            true
        }
    };
    credit_reward_events(money, settle_move_learn_prompts(battle, rng));
    finalize_battle_turn(battle_slot, turn_failed, player_lead)
}

#[cfg(test)]
mod tests {
    use super::*;
    use assets::SpeciesId;

    const TREECKO: SpeciesId = SpeciesId(277);
    const SLASH: MoveId = MoveId::SLASH;
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

    #[test]
    fn statused_limber_lead_is_refused_before_any_ot_id_draw() {
        let mut lead = BattlePokemon::new(
            &Dex::new(),
            SpeciesId(53),
            TEST_LEVEL,
            battle::fixed_ivs(TEST_PARTY_IV),
            TEST_PERSONALITY,
            vec![MoveId::TACKLE],
        )
        .expect("Persian/Tackle is a valid pairing");
        assert_eq!(lead.ability(), assets::AbilityId::LIMBER, "setup: Limber");
        lead.set_status1(battle::Status1::Paralysed);

        let mut rng = Rng::new(1);
        let before = rng.state();
        let result = start_npc_trainer_battle(lead, TrainerId::MAY_ROUTE_103_MUDKIP, &mut rng);
        assert_eq!(
            result.err(),
            Some(NpcTrainerBattleError::Battle(
                BattleError::UnportedAbilityInteraction(assets::AbilityId::LIMBER)
            )),
        );
        assert_eq!(
            rng.state(),
            before,
            "the statused-entrant refusal must precede every OT-ID draw"
        );
    }

    fn lead_of(species: SpeciesId) -> BattlePokemon {
        BattlePokemon::new(
            &Dex::new(),
            species,
            TEST_LEVEL,
            battle::fixed_ivs(TEST_PARTY_IV),
            TEST_PERSONALITY,
            vec![MoveId::TACKLE],
        )
        .expect("a valid lead")
    }

    #[test]
    fn two_ability_lead_is_refused_before_any_ot_id_draw() {
        const VOLTORB: SpeciesId = SpeciesId(100);
        for slot in [0, 1] {
            let lead = lead_of(VOLTORB).with_ability_slot(slot);
            let mut rng = Rng::new(1);
            let before = rng.state();
            for _ in 0..2 {
                let result = start_npc_trainer_battle(
                    lead.clone(),
                    TrainerId::MAY_ROUTE_103_MUDKIP,
                    &mut rng,
                );
                assert_eq!(
                    result.err(),
                    Some(NpcTrainerBattleError::Battle(
                        BattleError::AmbiguousPlayerAbility(VOLTORB)
                    )),
                );
                assert_eq!(rng.state(), before, "the refusal must precede every draw");
            }
        }
    }

    #[test]
    fn single_ability_lead_starts_the_battle() {
        let mut rng = Rng::new(1);
        let result =
            start_npc_trainer_battle(lead_of(TREECKO), TrainerId::MAY_ROUTE_103_MUDKIP, &mut rng);
        assert!(result.is_ok(), "{:?}", result.err());
    }

    /// A Lady holding a Nugget, with a Full Restore in her bag.
    const CINDY_WITH_FULL_RESTORE: TrainerId = TrainerId(114);
    const NUGGET_THEN_EMPTY_PARTY: TrainerId = TrainerId(694);
    const ORAN_BERRY_PARTY: TrainerId = TrainerId(292);

    fn treecko_lead() -> BattlePokemon {
        lead_of(TREECKO)
    }

    #[test]
    fn every_party_shape_maps_its_item_and_moves() {
        const NO_ITEM_DEFAULT: [assets::TrainerMonNoItemDefaultMoves; 1] =
            [assets::TrainerMonNoItemDefaultMoves {
                iv: 1,
                lvl: 5,
                species: TREECKO,
            }];
        const NO_ITEM_CUSTOM: [assets::TrainerMonNoItemCustomMoves; 1] =
            [assets::TrainerMonNoItemCustomMoves {
                iv: 2,
                lvl: 6,
                species: TREECKO,
                moves: [
                    SLASH,
                    battle::MOVE_NONE,
                    battle::MOVE_NONE,
                    battle::MOVE_NONE,
                ],
            }];
        const ITEM_DEFAULT: [assets::TrainerMonItemDefaultMoves; 1] =
            [assets::TrainerMonItemDefaultMoves {
                iv: 3,
                lvl: 7,
                species: TREECKO,
                held_item: ItemId::NUGGET,
            }];
        const ITEM_CUSTOM: [assets::TrainerMonItemCustomMoves; 1] =
            [assets::TrainerMonItemCustomMoves {
                iv: 4,
                lvl: 8,
                species: TREECKO,
                held_item: ItemId::ORAN_BERRY,
                moves: [SLASH, MoveId::TACKLE, battle::MOVE_NONE, battle::MOVE_NONE],
            }];
        let entry = |unscaled_iv, level, custom_moves, held_item| PartyEntry {
            species: TREECKO,
            level,
            unscaled_iv,
            custom_moves,
            held_item,
        };

        assert_eq!(
            party_entries(TrainerParty::NoItemDefaultMoves(&NO_ITEM_DEFAULT)),
            [entry(1, 5, None, ItemId::NONE)]
        );
        assert_eq!(
            party_entries(TrainerParty::NoItemCustomMoves(&NO_ITEM_CUSTOM)),
            [entry(2, 6, Some(vec![SLASH]), ItemId::NONE)]
        );
        assert_eq!(
            party_entries(TrainerParty::ItemDefaultMoves(&ITEM_DEFAULT)),
            [entry(3, 7, None, ItemId::NUGGET)]
        );
        assert_eq!(
            party_entries(TrainerParty::ItemCustomMoves(&ITEM_CUSTOM)),
            [entry(
                4,
                8,
                Some(vec![SLASH, MoveId::TACKLE]),
                ItemId::ORAN_BERRY
            )]
        );
    }

    #[test]
    fn a_held_item_party_still_has_its_personalities() {
        for id in [
            CINDY_WITH_FULL_RESTORE,
            NUGGET_THEN_EMPTY_PARTY,
            ORAN_BERRY_PARTY,
        ] {
            let personalities =
                trainer_party_personalities(id).expect("personalities are item-blind");
            assert_eq!(
                personalities.len(),
                party_entries(battle::trainer_data(id).unwrap().party).len()
            );
        }
    }

    #[test]
    fn an_effect_free_held_item_is_carried_onto_the_built_party() {
        let mut rng = Rng::new(1);
        let battle = start_npc_trainer_battle(treecko_lead(), NUGGET_THEN_EMPTY_PARTY, &mut rng)
            .expect("a Nugget holder with a bare reserve constructs");
        let bench = battle.trainer().expect("a trainer battle").bench();
        assert_eq!(battle.enemy().held_item(), ItemId::NUGGET);
        assert_eq!(
            bench
                .iter()
                .map(BattlePokemon::held_item)
                .collect::<Vec<_>>(),
            [ItemId::NONE]
        );
    }

    #[test]
    fn a_berry_holding_lead_is_refused_before_any_ot_id_draw() {
        let mut rng = Rng::new(1);
        let before = rng.state();
        let lead = treecko_lead().with_held_item(ItemId::ORAN_BERRY);
        let result = start_npc_trainer_battle(lead, NUGGET_THEN_EMPTY_PARTY, &mut rng);
        assert_eq!(
            result.err(),
            Some(NpcTrainerBattleError::Battle(
                BattleError::UnsupportedHeldItem(ItemId::ORAN_BERRY)
            ))
        );
        assert_eq!(
            rng.state(),
            before,
            "the lead's item is screened before every draw"
        );
    }

    #[test]
    fn a_trainer_with_a_bag_item_is_refused_before_any_ot_id_draw() {
        let mut rng = Rng::new(1);
        let before = rng.state();
        let result = start_npc_trainer_battle(treecko_lead(), CINDY_WITH_FULL_RESTORE, &mut rng);
        assert_eq!(
            result.err(),
            Some(NpcTrainerBattleError::Battle(
                BattleError::UnsupportedTrainerBagItem(ItemId::FULL_RESTORE)
            )),
            "her party is playable; only her bag is not"
        );
        assert_eq!(rng.state(), before, "the bag screen precedes every draw");
    }

    #[test]
    fn a_berry_party_is_refused_before_any_ot_id_draw() {
        let mut rng = Rng::new(1);
        let before = rng.state();
        let result = start_npc_trainer_battle(treecko_lead(), ORAN_BERRY_PARTY, &mut rng);
        assert_eq!(
            result.err(),
            Some(NpcTrainerBattleError::Battle(
                BattleError::UnsupportedHeldItem(ItemId::ORAN_BERRY)
            ))
        );
        assert_eq!(rng.state(), before, "the item screen precedes every draw");
    }
}
