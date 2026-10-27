//! Trainer party construction, validation, replacement, and victory rewards.
//!
//! Trainer construction receives a fixed personality and scales each party
//! entry's IV byte, so it consumes RNG only while rolling non-shiny
//! original-trainer IDs. Callers run [`ensure_trainer_party_startable`] before
//! [`build_trainer_pokemon`], as `flow::npc_trainer_battle` does: construction
//! draws OT IDs before `Battle::new_trainer` can refuse an unexecutable move.

use assets::items::ItemId;
use assets::trainers::{AiFlags, TrainerClass, TrainerData, TrainerId, TrainerParty, TrainerTable};
use assets::{MoveId, SpeciesId};

use crate::damage::BattleRng;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::{BattlePokemon, Ivs};

mod suitability;

/// The maximum IV assigned to one stat.
pub const MAX_PER_STAT_IVS: u16 = 31;

/// The exclusive upper bound for a shiny value.
pub const SHINY_ODDS: u16 = 8;

/// Scales a trainer party's IV byte and assigns the result to every stat.
#[must_use]
pub fn fixed_ivs(individual_value: u8) -> Ivs {
    let value = u8::try_from(u16::from(individual_value) * MAX_PER_STAT_IVS / u16::from(u8::MAX))
        .unwrap_or(u8::MAX);
    Ivs {
        hp: value,
        attack: value,
        defense: value,
        speed: value,
        sp_attack: value,
        sp_defense: value,
    }
}

const fn xor_fold_halves(value: u32) -> u16 {
    let bytes = value.to_le_bytes();
    u16::from_le_bytes([bytes[0], bytes[1]]) ^ u16::from_le_bytes([bytes[2], bytes[3]])
}

/// Returns the xor-folded shiny value for an original-trainer ID and personality.
#[must_use]
pub const fn shiny_value(ot_id: u32, personality: u32) -> u16 {
    xor_fold_halves(ot_id) ^ xor_fold_halves(personality)
}

/// Draws original-trainer IDs until the resulting Pokémon is not shiny.
///
/// Each attempt consumes one [`BattleRng::next_u32`] value.
#[must_use]
pub fn roll_non_shiny_ot_id(personality: u32, rng: &mut impl BattleRng) -> u32 {
    loop {
        let ot_id = rng.next_u32();
        if shiny_value(ot_id, personality) >= SHINY_ODDS {
            return ot_id;
        }
    }
}

/// Builds one trainer party member with fixed IVs and personality.
///
/// # Errors
///
/// Returns any error from [`BattlePokemon::new`] (species, level, moves, IVs)
/// before consuming RNG.
pub fn build_trainer_pokemon(
    dex: &Dex,
    species: SpeciesId,
    level: u8,
    fixed_iv: Ivs,
    personality: u32,
    moves: Vec<MoveId>,
    rng: &mut impl BattleRng,
) -> Result<BattlePokemon, BattleError> {
    let pokemon = BattlePokemon::new(dex, species, level, fixed_iv, personality, moves)?;
    let ot_id = roll_non_shiny_ot_id(personality, rng);
    Ok(pokemon.with_original_trainer_id(ot_id))
}

/// `gTrainerMoneyTable` in upstream order (`pokeemerald/src/battle_main.c:474-531`).
const TRAINER_MONEY_TABLE: [(TrainerClass, u32); 55] = [
    (TrainerClass::TEAM_AQUA, 5),
    (TrainerClass::AQUA_ADMIN, 10),
    (TrainerClass::AQUA_LEADER, 20),
    (TrainerClass::AROMA_LADY, 10),
    (TrainerClass::RUIN_MANIAC, 15),
    (TrainerClass::INTERVIEWER, 12),
    (TrainerClass::TUBER_F, 1),
    (TrainerClass::TUBER_M, 1),
    (TrainerClass::SIS_AND_BRO, 3),
    (TrainerClass::COOLTRAINER, 12),
    (TrainerClass::HEX_MANIAC, 6),
    (TrainerClass::LADY, 50),
    (TrainerClass::BEAUTY, 20),
    (TrainerClass::RICH_BOY, 50),
    (TrainerClass::POKEMANIAC, 15),
    (TrainerClass::SWIMMER_M, 2),
    (TrainerClass::BLACK_BELT, 8),
    (TrainerClass::GUITARIST, 8),
    (TrainerClass::KINDLER, 8),
    (TrainerClass::CAMPER, 4),
    (TrainerClass::OLD_COUPLE, 10),
    (TrainerClass::BUG_MANIAC, 15),
    (TrainerClass::PSYCHIC, 6),
    (TrainerClass::GENTLEMAN, 20),
    (TrainerClass::ELITE_FOUR, 25),
    (TrainerClass::LEADER, 25),
    (TrainerClass::SCHOOL_KID, 5),
    (TrainerClass::SR_AND_JR, 4),
    (TrainerClass::POKEFAN, 20),
    (TrainerClass::EXPERT, 10),
    (TrainerClass::YOUNGSTER, 4),
    (TrainerClass::CHAMPION, 50),
    (TrainerClass::FISHERMAN, 10),
    (TrainerClass::TRIATHLETE, 10),
    (TrainerClass::DRAGON_TAMER, 12),
    (TrainerClass::BIRD_KEEPER, 8),
    (TrainerClass::NINJA_BOY, 3),
    (TrainerClass::BATTLE_GIRL, 6),
    (TrainerClass::PARASOL_LADY, 10),
    (TrainerClass::SWIMMER_F, 2),
    (TrainerClass::PICNICKER, 4),
    (TrainerClass::TWINS, 3),
    (TrainerClass::SAILOR, 8),
    (TrainerClass::COLLECTOR, 15),
    (TrainerClass::RIVAL, 15),
    (TrainerClass::PKMN_BREEDER, 10),
    (TrainerClass::PKMN_RANGER, 12),
    (TrainerClass::TEAM_MAGMA, 5),
    (TrainerClass::MAGMA_ADMIN, 10),
    (TrainerClass::MAGMA_LEADER, 20),
    (TrainerClass::LASS, 4),
    (TrainerClass::BUG_CATCHER, 4),
    (TrainerClass::HIKER, 10),
    (TrainerClass::YOUNG_COUPLE, 8),
    (TrainerClass::WINSTRATE, 10),
];

/// The prize multiplier for a trainer class absent from the money table:
/// `gTrainerMoneyTable`'s `{0xFF, 5}` sentinel row (`battle_main.c:530`).
pub const DEFAULT_MONEY_VALUE: u32 = 5;

const BASE_PRIZE_MULTIPLIER: u32 = 4;

/// Returns the prize multiplier for a trainer class.
#[must_use]
pub fn money_value_for_class(class: TrainerClass) -> u32 {
    TRAINER_MONEY_TABLE
        .iter()
        .find(|(candidate, _)| *candidate == class)
        .map_or(DEFAULT_MONEY_VALUE, |(_, value)| *value)
}

#[must_use]
fn last_mon_level(party: TrainerParty) -> Option<u8> {
    match party {
        TrainerParty::NoItemDefaultMoves(p) => p.last().map(|m| m.lvl),
        TrainerParty::NoItemCustomMoves(p) => p.last().map(|m| m.lvl),
        TrainerParty::ItemDefaultMoves(p) => p.last().map(|m| m.lvl),
        TrainerParty::ItemCustomMoves(p) => p.last().map(|m| m.lvl),
    }
}

/// Returns the single-battle prize money for defeating a trainer.
///
/// `GetTrainerMoneyToGive` (`pokeemerald/src/battle_script_commands.c:5578`):
/// `4 * lastMonLevel * classValue` with `moneyMultiplier` fixed at `1`.
/// Empty parties pay nothing. Held-item and double-battle multipliers are not
/// inputs because those battle modes are not modeled.
#[must_use]
pub fn trainer_money(trainer: &TrainerData) -> u32 {
    let Some(level) = last_mon_level(trainer.party) else {
        return 0;
    };
    BASE_PRIZE_MULTIPLIER * u32::from(level) * money_value_for_class(trainer.class)
}

/// State owned by a trainer opponent for the duration of a battle.
///
/// The party keeps its original slot order for the whole battle, as
/// upstream's `gEnemyParty` does while `gBattlerPartyIndexes` names the
/// active member. [`Battle`](crate::battle::Battle) owns the active member as
/// `Battle::enemy`; this context owns every other party record, fainted ones
/// included, and the active member's original slot.
#[derive(Debug, Clone)]
pub struct TrainerContext {
    id: TrainerId,
    class: TrainerClass,
    ai_flags: AiFlags,
    money: u32,
    /// Every party member but the active one, in original slot order.
    non_active: Vec<BattlePokemon>,
    /// The original party slot of the active member.
    active_slot: usize,
}

impl TrainerContext {
    /// Builds the context for a party whose slot 0 is the active member and
    /// whose remaining members are `bench` in original slot order.
    #[must_use]
    pub(crate) fn new(id: TrainerId, data: &TrainerData, bench: Vec<BattlePokemon>) -> Self {
        Self {
            id,
            class: data.class,
            ai_flags: data.ai_flags,
            money: trainer_money(data),
            non_active: bench,
            active_slot: 0,
        }
    }

    /// Returns the opponent's trainer ID.
    #[must_use]
    pub const fn id(&self) -> TrainerId {
        self.id
    }

    /// Returns the opponent's trainer class.
    #[must_use]
    pub const fn class(&self) -> TrainerClass {
        self.class
    }

    /// Returns the flags that select the opponent's AI scoring rules.
    #[must_use]
    pub const fn ai_flags(&self) -> AiFlags {
        self.ai_flags
    }

    /// Returns the prize money paid for defeating the opponent.
    #[must_use]
    pub const fn money(&self) -> u32 {
        self.money
    }

    /// Returns the original party slot of the active member.
    #[must_use]
    pub const fn active_slot(&self) -> usize {
        self.active_slot
    }

    /// Returns the number of party members behind the active one that can
    /// still fight. A fainted member keeps its slot but is not counted.
    #[must_use]
    pub fn bench_len(&self) -> usize {
        self.non_active
            .iter()
            .filter(|mon| !mon.is_fainted())
            .count()
    }

    /// Returns every party member but the active one, fainted ones included,
    /// in original slot order. Use [`TrainerContext::bench_len`] for the
    /// number that can still fight.
    #[must_use]
    pub fn bench(&self) -> &[BattlePokemon] {
        &self.non_active
    }

    /// Returns each non-active party member with its original party slot, in
    /// slot order.
    pub fn candidates(&self) -> impl Iterator<Item = (usize, &BattlePokemon)> {
        self.non_active
            .iter()
            .enumerate()
            .map(|(index, mon)| (self.original_slot(index), mon))
    }

    /// The original party slot of `non_active[index]`.
    const fn original_slot(&self, index: usize) -> usize {
        if index < self.active_slot {
            index
        } else {
            index + 1
        }
    }

    /// The `non_active` index holding original `slot`, or `None` for the
    /// active slot and slots past the party.
    fn non_active_index(&self, slot: usize) -> Option<usize> {
        match slot.cmp(&self.active_slot) {
            std::cmp::Ordering::Equal => None,
            std::cmp::Ordering::Less => Some(slot),
            std::cmp::Ordering::Greater => Some(slot - 1),
        }
        .filter(|&index| index < self.non_active.len())
    }

    /// Returns the member in original party `slot` when it is a non-active,
    /// non-fainted member, the only kind a switch may name.
    pub(crate) fn switch_target(&self, slot: usize) -> Result<&BattlePokemon, BattleError> {
        self.non_active_index(slot)
            .map(|index| &self.non_active[index])
            .filter(|mon| !mon.is_fainted())
            .ok_or(BattleError::InvalidTrainerSwitch(slot))
    }

    /// Selects the original slot of the most suitable non-fainted member for
    /// a forced post-faint send-out: `GetMostSuitableMonToSwitchInto`'s type
    /// and most-damage passes, then the first healthy non-active slot in
    /// party order. `fainted` is the battler being replaced and
    /// `resolving_move` is the move whose resolution fainted it (the turn
    /// engine's own last-executed move; upstream's `gCurrentMove` is exactly
    /// this stale value at the point a forced replacement runs). See the
    /// ledger's `GetMostSuitableMonToSwitchInto` and
    /// `OpponentHandleChoosePokemon` entries for the full upstream mapping.
    ///
    /// # Errors
    ///
    /// Returns an error if a bench member's move is missing from `dex` --
    /// never in practice, since trainer-party validation already screens
    /// every move (see [`ensure_trainer_party_startable`]).
    pub(crate) fn select_replacement(
        &self,
        dex: &Dex,
        fainted: &BattlePokemon,
        resolving_move: MoveId,
        player: &BattlePokemon,
    ) -> Result<Option<usize>, BattleError> {
        if let Some(slot) = self.most_suitable_by_type(dex, player)? {
            return Ok(Some(slot));
        }
        if let Some(slot) = self.most_suitable_by_damage(dex, fainted, resolving_move, player)? {
            return Ok(Some(slot));
        }
        Ok(self
            .candidates()
            .find(|(_, mon)| !mon.is_fainted())
            .map(|(slot, _)| slot))
    }

    /// Swaps `active` with the member in original party `slot`: the outgoing
    /// member returns to its own slot and the named member becomes active.
    /// Nothing changes when `slot` is not a healthy non-active member.
    ///
    /// # Errors
    ///
    /// Returns [`BattleError::InvalidTrainerSwitch`] for the active slot, a
    /// slot past the party, or a fainted member.
    pub(crate) fn switch_active(
        &mut self,
        active: &mut BattlePokemon,
        slot: usize,
    ) -> Result<(), BattleError> {
        self.switch_target(slot)?;
        let index = self
            .non_active_index(slot)
            .expect("switch_target accepted the slot");
        let incoming = self.non_active.remove(index);
        let outgoing = std::mem::replace(active, incoming);
        // The outgoing member takes the position its own original slot holds
        // among the members still behind the new active one.
        let outgoing_index = if self.active_slot < slot {
            self.active_slot
        } else {
            self.active_slot - 1
        };
        self.non_active.insert(outgoing_index, outgoing);
        self.active_slot = slot;
        Ok(())
    }
}

/// Looks up a trainer in the extracted trainer table.
///
/// # Errors
///
/// Returns [`BattleError::UnknownTrainer`] when the ID is outside the table.
pub fn trainer_data(trainer: TrainerId) -> Result<&'static TrainerData, BattleError> {
    TrainerTable::new()
        .get(trainer)
        .ok_or(BattleError::UnknownTrainer(trainer))
}

pub(crate) fn ensure_move_playable(
    dex: &Dex,
    move_id: MoveId,
    ai_flags: AiFlags,
) -> Result<(), BattleError> {
    crate::battle::ensure_executable(dex, move_id)?;
    super::trainer_ai::ensure_scoreable(dex, move_id, ai_flags)
}

/// Inputs needed to validate a prospective trainer party member.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrainerPartyMon<'a> {
    /// The member's species.
    pub species: SpeciesId,
    /// The member's level.
    pub level: u8,
    /// The moves the member will use in battle.
    pub moves: &'a [MoveId],
    /// The fixed personality [`build_trainer_pokemon`] will receive; it selects the
    /// ability slot the admission screen checks.
    pub personality: u32,
    /// The item the member will hold ([`ItemId::NONE`] for none); a held item
    /// the battle cannot honour is refused before any draw.
    pub held_item: ItemId,
}

/// Validates that a trainer party can be built and played without consuming RNG.
///
/// Empty-party validation runs before trainer lookup. Non-empty parties then
/// undergo member validation, AI-flag validation, and move validation.
///
/// # Errors
///
/// Returns [`BattleError::EmptyTrainerParty`] for an empty party,
/// [`BattleError::UnknownTrainer`] for an unknown trainer, or the first
/// member, AI-flag, or move validation error, or
/// [`BattleError::AmbiguousPlayerAbility`] when `player_lead` has two ability
/// slots, its selected ability is not a trapping one, and the trainer scores
/// with `CheckBadMove` (the only script that reads the target's ability), or
/// [`BattleError::UnportedAbilityInteraction`] when a member's personality selects an
/// always-refused ability or a member's PP-positive move would be refused by
/// [`secondary::ensure_admissible`](crate::secondary::ensure_admissible)
/// against `player_lead` (for example Serene Grace with a Confuse Hit move).
/// [`BattleError::UnsupportedHeldItem`] when a member's held item has an
/// effect no pipeline models, or [`BattleError::UnsupportedTrainerBagItem`]
/// when the trainer's bag holds any item.
/// Only the lead is screened; reserves are screened by `Battle::new_trainer`.
pub fn ensure_trainer_party_startable(
    dex: &Dex,
    player_lead: &BattlePokemon,
    trainer: TrainerId,
    party: &[TrainerPartyMon<'_>],
) -> Result<(), BattleError> {
    if party.is_empty() {
        return Err(BattleError::EmptyTrainerParty(trainer));
    }
    let data = trainer_data(trainer)?;
    super::held_item::ensure_trainer_bag_unused(&data.items)?;
    for mon in party {
        BattlePokemon::validate(dex, mon.species, mon.level, mon.moves)?;
        super::admission::ensure_selected_ability_admissible(dex, mon.species, mon.personality)?;
    }
    super::trainer_ai::ensure_supported_flags(data.ai_flags)?;
    for mon in party {
        // `BattlePokemon::new` draws no RNG; the OT-ID roll is the only draw
        // `build_trainer_pokemon` adds, and the screen never reads it. IVs do
        // not affect the ability or any secondary-effect screen.
        let attacker = BattlePokemon::new(
            dex,
            mon.species,
            mon.level,
            fixed_ivs(0),
            mon.personality,
            mon.moves.to_vec(),
        )?;
        for slot in attacker.moves() {
            ensure_move_playable(dex, slot.move_id, data.ai_flags)?;
            if slot.pp > 0 {
                super::secondary::ensure_admissible(dex, slot.move_id, &attacker, player_lead)?;
            }
        }
        super::held_item::ensure_held_item_playable(mon.held_item)?;
    }
    if data.ai_flags.contains(AiFlags::CHECK_BAD_MOVE) {
        super::trainer_ai::ensure_target_ability_unambiguous(player_lead)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        build_trainer_pokemon, ensure_trainer_party_startable, fixed_ivs, money_value_for_class,
        roll_non_shiny_ot_id, shiny_value, trainer_data, trainer_money, TrainerPartyMon,
        DEFAULT_MONEY_VALUE, SHINY_ODDS,
    };
    use crate::dex::Dex;
    use crate::script_rng::SequenceRng;
    use assets::items::ItemId;
    use assets::trainers::{TrainerClass, TrainerId};
    use assets::{MoveId, SpeciesId};

    const MAY_ROUTE_103_MUDKIP: TrainerId = TrainerId(529);
    const UNKNOWN_TRAINER: TrainerId = TrainerId(60_000);
    const TREECKO: SpeciesId = SpeciesId(277);
    const POUND: MoveId = MoveId::POUND;
    const HARDEN: MoveId = MoveId::HARDEN;
    const ROUTE_103_RIVAL_LEVEL: u8 = 5;
    const INVALID_LEVEL: u8 = 0;
    const NON_SHINY_OT_ID: u32 = 0x0000_00FF;
    const TRAINER_CLASS_COOLTRAINER_2: TrainerClass = TrainerClass(0x30);
    const TRAINER_CLASS_RS_PROTAG: TrainerClass = TrainerClass(0x41);

    fn random32_draws(value: u32) -> [u16; 2] {
        let bytes = value.to_le_bytes();
        [
            u16::from_le_bytes([bytes[0], bytes[1]]),
            u16::from_le_bytes([bytes[2], bytes[3]]),
        ]
    }

    #[test]
    fn fixed_ivs_scales_the_full_byte_range_and_truncates() {
        assert_eq!(fixed_ivs(0).as_array(), [0; 6]);
        assert_eq!(fixed_ivs(255).as_array(), [31; 6]);
        assert_eq!(fixed_ivs(100).as_array(), [12; 6]);
    }

    #[test]
    fn the_ot_id_loop_redraws_only_while_the_result_would_be_shiny() {
        const PERSONALITY: u32 = 0;
        const SHINY_OT_ID: u32 = 0x0000_0001;

        assert!(shiny_value(SHINY_OT_ID, PERSONALITY) < SHINY_ODDS);
        assert!(shiny_value(NON_SHINY_OT_ID, PERSONALITY) >= SHINY_ODDS);

        let draws = random32_draws(SHINY_OT_ID)
            .into_iter()
            .chain(random32_draws(NON_SHINY_OT_ID));
        let mut rng = SequenceRng::new(draws);
        assert_eq!(roll_non_shiny_ot_id(PERSONALITY, &mut rng), NON_SHINY_OT_ID);
        assert_eq!(rng.draws(), 4, "two Random32 draws: one rejected, one kept");
    }

    #[test]
    fn a_trainer_mon_draws_only_its_ot_id() {
        const PERSONALITY: u32 = 0x1234_5678;

        let dex = Dex::new();
        let mut rng = SequenceRng::new(random32_draws(NON_SHINY_OT_ID));
        let mon = build_trainer_pokemon(
            &dex,
            TREECKO,
            ROUTE_103_RIVAL_LEVEL,
            fixed_ivs(0),
            PERSONALITY,
            vec![POUND],
            &mut rng,
        )
        .expect("Treecko/Pound are dex-resident");
        assert_eq!(
            rng.draws(),
            2,
            "personality and IVs are fixed; only the OT id draws"
        );
        assert_eq!(mon.personality(), PERSONALITY);
        assert_eq!(mon.ivs().as_array(), [0; 6]);
        assert_eq!(mon.original_trainer_id(), NON_SHINY_OT_ID);
    }

    #[test]
    fn a_rejected_request_draws_nothing_at_all() {
        let dex = Dex::new();
        let mut rng = SequenceRng::new([]);
        let error = build_trainer_pokemon(
            &dex,
            TREECKO,
            INVALID_LEVEL,
            fixed_ivs(0),
            0,
            vec![POUND],
            &mut rng,
        )
        .unwrap_err();
        assert_eq!(
            error,
            crate::error::BattleError::InvalidLevel(INVALID_LEVEL)
        );
        assert_eq!(rng.draws(), 0, "validation runs ahead of the OT-id loop");
    }

    #[test]
    fn an_invalid_iv_draws_nothing_at_all() {
        let dex = Dex::new();
        let mut rng = SequenceRng::new([]);
        let mut ivs = fixed_ivs(0);
        ivs.speed = crate::pokemon::MAX_IV + 1;
        let error = build_trainer_pokemon(
            &dex,
            TREECKO,
            ROUTE_103_RIVAL_LEVEL,
            ivs,
            0,
            vec![POUND],
            &mut rng,
        )
        .unwrap_err();
        assert_eq!(
            error,
            crate::error::BattleError::InvalidIv(crate::pokemon::MAX_IV + 1)
        );
        assert_eq!(rng.draws(), 0, "IV validation runs ahead of the OT-id loop");
    }

    #[test]
    fn the_route_103_rival_pays_the_rival_class_prize_money() {
        let data = trainer_data(MAY_ROUTE_103_MUDKIP).expect("a real TRAINER_* id");
        assert_eq!(data.class, TrainerClass::RIVAL);
        assert_eq!(money_value_for_class(data.class), 15);
        assert_eq!(trainer_money(data), 300);
    }

    #[test]
    fn unlisted_classes_use_the_default_money_value() {
        assert_eq!(
            money_value_for_class(TRAINER_CLASS_COOLTRAINER_2),
            DEFAULT_MONEY_VALUE
        );
        assert_eq!(
            money_value_for_class(TRAINER_CLASS_RS_PROTAG),
            DEFAULT_MONEY_VALUE
        );
        assert_eq!(money_value_for_class(TrainerClass::CHAMPION), 50);
    }

    const VOLTORB: assets::SpeciesId = assets::SpeciesId(100);

    fn lead(species: assets::SpeciesId) -> crate::pokemon::BattlePokemon {
        crate::pokemon::BattlePokemon::new(
            &Dex::new(),
            species,
            ROUTE_103_RIVAL_LEVEL,
            fixed_ivs(0),
            0,
            vec![POUND],
        )
        .expect("a valid lead")
    }

    /// An even personality selects slot 0, as every NPC trainer personality does; an odd one
    /// selects slot 1 when the species has one.
    const SLOT_0: u32 = 0x88;
    const SLOT_1: u32 = 0x89;

    #[test]
    fn the_pre_flight_refuses_the_always_refused_slot_construction_selects() {
        let dex = Dex::new();
        let moves = [MoveId::TACKLE];
        for (species, personality, ability) in [
            (SpeciesId::SLAKOTH, SLOT_0, assets::AbilityId::TRUANT),
            (SpeciesId::SLAKING, SLOT_1, assets::AbilityId::TRUANT),
            (SpeciesId::TAUROS, SLOT_1, assets::AbilityId::INTIMIDATE),
            (SpeciesId::GYARADOS, SLOT_0, assets::AbilityId::INTIMIDATE),
            (SpeciesId::GRANBULL, SLOT_0, assets::AbilityId::INTIMIDATE),
            (SpeciesId::STANTLER, SLOT_0, assets::AbilityId::INTIMIDATE),
            (SpeciesId::HITMONTOP, SLOT_0, assets::AbilityId::INTIMIDATE),
            (SpeciesId::MIGHTYENA, SLOT_0, assets::AbilityId::INTIMIDATE),
            (SpeciesId::MASQUERAIN, SLOT_0, assets::AbilityId::INTIMIDATE),
            (SpeciesId::SALAMENCE, SLOT_0, assets::AbilityId::INTIMIDATE),
            (SpeciesId::EKANS, SLOT_0, assets::AbilityId::INTIMIDATE),
            (SpeciesId::SNUBBULL, SLOT_0, assets::AbilityId::INTIMIDATE),
            (SpeciesId::MAWILE, SLOT_1, assets::AbilityId::INTIMIDATE),
            (SpeciesId::NINJASK, SLOT_0, assets::AbilityId::SPEED_BOOST),
            (SpeciesId::YANMA, SLOT_0, assets::AbilityId::SPEED_BOOST),
            (SpeciesId::KECLEON, SLOT_0, assets::AbilityId::COLOR_CHANGE),
            (SpeciesId::KECLEON, SLOT_1, assets::AbilityId::COLOR_CHANGE),
            (SpeciesId::PORYGON, SLOT_0, assets::AbilityId::TRACE),
            (SpeciesId::PORYGON2, SLOT_1, assets::AbilityId::TRACE),
            (SpeciesId::RALTS, SLOT_1, assets::AbilityId::TRACE),
            (SpeciesId::KIRLIA, SLOT_1, assets::AbilityId::TRACE),
            (SpeciesId::GARDEVOIR, SLOT_1, assets::AbilityId::TRACE),
            (SpeciesId::PARAS, SLOT_0, assets::AbilityId::EFFECT_SPORE),
            (SpeciesId::PARASECT, SLOT_1, assets::AbilityId::EFFECT_SPORE),
            (
                SpeciesId::SHROOMISH,
                SLOT_0,
                assets::AbilityId::EFFECT_SPORE,
            ),
            (SpeciesId::BRELOOM, SLOT_1, assets::AbilityId::EFFECT_SPORE),
            (SpeciesId::KYOGRE, SLOT_0, assets::AbilityId::DRIZZLE),
            (SpeciesId::GROUDON, SLOT_0, assets::AbilityId::DROUGHT),
            (SpeciesId::TYRANITAR, SLOT_0, assets::AbilityId::SAND_STREAM),
        ] {
            let refused = TrainerPartyMon {
                species,
                level: 20,
                moves: &moves,
                personality,
                held_item: ItemId::NONE,
            };
            let ok = TrainerPartyMon {
                species: SpeciesId::MUDKIP,
                level: 20,
                moves: &moves,
                personality: SLOT_0,
                held_item: ItemId::NONE,
            };
            for party in [[refused, ok], [ok, refused]] {
                assert_eq!(
                    ensure_trainer_party_startable(
                        &dex,
                        &lead(SpeciesId::MUDKIP),
                        MAY_ROUTE_103_MUDKIP,
                        &party,
                    ),
                    Err(crate::error::BattleError::UnportedAbilityInteraction(
                        ability
                    )),
                    "{species:?} with personality {personality:#x}",
                );
            }
        }
    }

    #[test]
    fn the_pre_flight_admits_a_mixed_pool_species_whose_selected_slot_is_supported() {
        let dex = Dex::new();
        let moves = [MoveId::TACKLE];
        for (species, personality) in [
            (SpeciesId::EKANS, SLOT_1),
            (SpeciesId::MAWILE, SLOT_0),
            (SpeciesId::YANMA, SLOT_1),
            (SpeciesId::RALTS, SLOT_0),
            (SpeciesId::KIRLIA, SLOT_0),
            (SpeciesId::GARDEVOIR, SLOT_0),
        ] {
            assert!(ensure_trainer_party_startable(
                &dex,
                &lead(SpeciesId::MUDKIP),
                MAY_ROUTE_103_MUDKIP,
                &[TrainerPartyMon {
                    species,
                    level: 20,
                    moves: &moves,
                    personality,
                    held_item: ItemId::NONE,
                }],
            )
            .is_ok());
        }
    }

    #[test]
    fn an_admitted_party_member_builds_with_the_ability_the_pre_flight_screened() {
        let dex = Dex::new();
        for (species, personality, ability) in [
            (SpeciesId::EKANS, SLOT_1, assets::AbilityId::SHED_SKIN),
            (SpeciesId::MAWILE, SLOT_0, assets::AbilityId::HYPER_CUTTER),
        ] {
            let mut rng = SequenceRng::new(random32_draws(NON_SHINY_OT_ID));
            let built = build_trainer_pokemon(
                &dex,
                species,
                20,
                fixed_ivs(0),
                personality,
                vec![MoveId::TACKLE],
                &mut rng,
            )
            .unwrap();
            assert_eq!(built.ability(), ability);
        }
    }

    #[test]
    fn ensure_trainer_party_startable_accepts_a_real_constructible_party() {
        let dex = Dex::new();
        let moves = [POUND];
        assert_eq!(
            ensure_trainer_party_startable(
                &dex,
                &lead(TREECKO),
                MAY_ROUTE_103_MUDKIP,
                &[TrainerPartyMon {
                    species: TREECKO,
                    level: ROUTE_103_RIVAL_LEVEL,
                    moves: &moves,
                    personality: 0,
                    held_item: ItemId::NONE,
                }],
            ),
            Ok(())
        );
    }

    #[test]
    fn the_pre_flight_refuses_a_later_members_unmodelled_item_and_admits_an_effect_free_one() {
        let dex = Dex::new();
        let moves = [POUND];
        let member = |held_item| TrainerPartyMon {
            species: TREECKO,
            level: ROUTE_103_RIVAL_LEVEL,
            moves: &moves,
            personality: 0,
            held_item,
        };
        let screen = |party: &[TrainerPartyMon<'_>]| {
            ensure_trainer_party_startable(&dex, &lead(TREECKO), MAY_ROUTE_103_MUDKIP, party)
        };

        assert_eq!(
            screen(&[member(ItemId::NUGGET), member(ItemId::NONE)]),
            Ok(())
        );
        assert_eq!(
            screen(&[member(ItemId::NONE), member(ItemId::ORAN_BERRY)]),
            Err(crate::error::BattleError::UnsupportedHeldItem(
                ItemId::ORAN_BERRY
            ))
        );
    }

    #[test]
    fn the_pre_flight_reports_what_the_real_handoff_would_but_without_the_draws() {
        let dex = Dex::new();
        let moves = [HARDEN];
        let screened = ensure_trainer_party_startable(
            &dex,
            &lead(TREECKO),
            MAY_ROUTE_103_MUDKIP,
            &[TrainerPartyMon {
                species: TREECKO,
                level: ROUTE_103_RIVAL_LEVEL,
                moves: &moves,
                personality: 0,
                held_item: ItemId::NONE,
            }],
        )
        .expect_err("Harden is not executable by this turn engine");

        let mut rng = SequenceRng::new(random32_draws(NON_SHINY_OT_ID));
        let enemy = build_trainer_pokemon(
            &dex,
            TREECKO,
            ROUTE_103_RIVAL_LEVEL,
            fixed_ivs(0),
            0,
            moves.to_vec(),
            &mut rng,
        )
        .expect("Treecko/Harden is a valid pairing -- only the turn engine refuses it");
        assert_eq!(
            rng.draws(),
            2,
            "the leak: draws are spent before the screen"
        );
        let player = crate::pokemon::BattlePokemon::new(
            &Dex::new(),
            TREECKO,
            ROUTE_103_RIVAL_LEVEL,
            fixed_ivs(0),
            0,
            vec![POUND],
        )
        .expect("Treecko/Pound is a valid pairing");
        let handoff = crate::battle::Battle::new_trainer(
            Dex::new(),
            player,
            MAY_ROUTE_103_MUDKIP,
            vec![enemy],
            &mut rng,
        )
        .expect_err("the battle refuses the same moveset");
        assert_eq!(screened, handoff);
    }

    #[test]
    fn the_pre_flight_refuses_a_serene_grace_confuse_hit_member_before_any_draw() {
        let dex = Dex::new();
        let moves = [MoveId::PSYBEAM];
        let party = [TrainerPartyMon {
            species: SpeciesId::DUNSPARCE,
            level: ROUTE_103_RIVAL_LEVEL,
            moves: &moves,
            personality: SLOT_0,
            held_item: ItemId::NONE,
        }];
        let enemy = crate::pokemon::BattlePokemon::new(
            &dex,
            SpeciesId::DUNSPARCE,
            ROUTE_103_RIVAL_LEVEL,
            fixed_ivs(0),
            SLOT_0,
            moves.to_vec(),
        )
        .unwrap();
        assert_eq!(enemy.ability(), assets::AbilityId::SERENE_GRACE);
        let refusal = Err(crate::error::BattleError::UnportedAbilityInteraction(
            assets::AbilityId::SERENE_GRACE,
        ));
        assert_eq!(
            ensure_trainer_party_startable(&dex, &lead(TREECKO), MAY_ROUTE_103_MUDKIP, &party),
            refusal
        );
        let mut rng = SequenceRng::new([]);
        assert_eq!(
            crate::battle::Battle::new_trainer(
                Dex::new(),
                lead(TREECKO),
                MAY_ROUTE_103_MUDKIP,
                vec![enemy],
                &mut rng,
            )
            .map(|_| ()),
            refusal
        );
        assert_eq!(rng.draws(), 0);
    }

    #[test]
    fn the_pre_flight_rejects_an_empty_party_and_an_unknown_trainer() {
        let dex = Dex::new();
        assert_eq!(
            ensure_trainer_party_startable(&dex, &lead(TREECKO), MAY_ROUTE_103_MUDKIP, &[]),
            Err(crate::error::BattleError::EmptyTrainerParty(
                MAY_ROUTE_103_MUDKIP
            ))
        );
        let moves = [POUND];
        assert_eq!(
            ensure_trainer_party_startable(
                &dex,
                &lead(TREECKO),
                UNKNOWN_TRAINER,
                &[TrainerPartyMon {
                    species: TREECKO,
                    level: ROUTE_103_RIVAL_LEVEL,
                    moves: &moves,
                    personality: 0,
                    held_item: ItemId::NONE,
                }],
            ),
            Err(crate::error::BattleError::UnknownTrainer(UNKNOWN_TRAINER))
        );
    }

    #[test]
    fn a_flagless_trainer_admits_a_two_ability_lead() {
        let dex = Dex::new();
        let moves = [POUND];
        let party = [TrainerPartyMon {
            species: TREECKO,
            level: ROUTE_103_RIVAL_LEVEL,
            moves: &moves,
            personality: 0,
            held_item: ItemId::NONE,
        }];
        assert_eq!(
            trainer_data(TrainerId::JANI).unwrap().ai_flags,
            assets::trainers::AiFlags::NONE
        );
        let voltorb = lead(VOLTORB);
        assert_eq!(
            ensure_trainer_party_startable(&dex, &voltorb, TrainerId::JANI, &party),
            Ok(())
        );
        let mut rng = SequenceRng::new([0; 16]);
        assert!(crate::battle::Battle::new_trainer(
            Dex::new(),
            voltorb,
            TrainerId::JANI,
            vec![lead(TREECKO)],
            &mut rng,
        )
        .is_ok());
    }

    #[test]
    fn a_flagless_trainer_admits_executable_fixed_damage_moves() {
        let dex = Dex::new();
        let moves = [MoveId::SEISMIC_TOSS, MoveId::SONIC_BOOM];
        let party = [TrainerPartyMon {
            species: TREECKO,
            level: ROUTE_103_RIVAL_LEVEL,
            moves: &moves,
            personality: 0,
            held_item: ItemId::NONE,
        }];
        assert_eq!(
            ensure_trainer_party_startable(&dex, &lead(VOLTORB), TrainerId::JANI, &party),
            Ok(())
        );
        let enemy = crate::pokemon::BattlePokemon::new(
            &dex,
            TREECKO,
            ROUTE_103_RIVAL_LEVEL,
            crate::Ivs::default(),
            0,
            moves.to_vec(),
        )
        .expect("a valid fixed-damage party member");
        let mut rng = SequenceRng::new([0; 16]);
        assert!(crate::battle::Battle::new_trainer(
            Dex::new(),
            lead(VOLTORB),
            TrainerId::JANI,
            vec![enemy],
            &mut rng,
        )
        .is_ok());
    }

    #[test]
    fn the_pre_flight_refuses_a_two_ability_lead_before_any_draw() {
        let dex = Dex::new();
        let moves = [POUND];
        let party = [TrainerPartyMon {
            species: TREECKO,
            level: ROUTE_103_RIVAL_LEVEL,
            moves: &moves,
            personality: 0,
            held_item: ItemId::NONE,
        }];
        for slot in [0, 1] {
            let voltorb = lead(VOLTORB).with_ability_slot(slot);
            let mut rng = SequenceRng::new([]);
            assert_eq!(
                ensure_trainer_party_startable(&dex, &voltorb, MAY_ROUTE_103_MUDKIP, &party),
                Err(crate::error::BattleError::AmbiguousPlayerAbility(VOLTORB))
            );
            let refused = crate::battle::Battle::new_trainer(
                Dex::new(),
                voltorb,
                MAY_ROUTE_103_MUDKIP,
                vec![lead(TREECKO)],
                &mut rng,
            );
            assert!(matches!(
                refused,
                Err(crate::error::BattleError::AmbiguousPlayerAbility(VOLTORB))
            ));
            assert_eq!(rng.draws(), 0);
            rng.assert_exhausted();
        }
    }

    /// `HandleTurnActionSelectionState` diverts a battler whose moves are all
    /// unusable to Struggle before the AI chooses a move
    /// (`battle_main.c:4183`-`:4191`), so `Cmd_get_ability` never runs for an
    /// all-spent trainer and a two-ability lead stays playable. A spent lead
    /// with a PP-positive bench member still reaches scoring after the switch.
    #[test]
    fn an_all_spent_trainer_admits_a_two_ability_lead_but_a_scoring_bench_does_not() {
        fn spent(species: assets::SpeciesId) -> crate::pokemon::BattlePokemon {
            let mut mon = lead(species);
            while mon.moves()[0].pp > 0 {
                mon.deduct_pp(0).unwrap();
            }
            mon
        }

        let mut rng = SequenceRng::new([0; 16]);
        let admitted = crate::battle::Battle::new_trainer(
            Dex::new(),
            lead(VOLTORB),
            MAY_ROUTE_103_MUDKIP,
            vec![spent(TREECKO), spent(TREECKO)],
            &mut rng,
        );
        assert!(admitted.is_ok(), "{:?}", admitted.err());

        let mut rng = SequenceRng::new([]);
        let refused = crate::battle::Battle::new_trainer(
            Dex::new(),
            lead(VOLTORB),
            MAY_ROUTE_103_MUDKIP,
            vec![spent(TREECKO), lead(TREECKO)],
            &mut rng,
        );
        assert!(matches!(
            refused,
            Err(crate::error::BattleError::AmbiguousPlayerAbility(VOLTORB))
        ));
        assert_eq!(rng.draws(), 0);
    }

    #[test]
    fn an_out_of_range_trainer_id_is_rejected_rather_than_panicking() {
        assert_eq!(
            trainer_data(UNKNOWN_TRAINER).unwrap_err(),
            crate::error::BattleError::UnknownTrainer(UNKNOWN_TRAINER)
        );
    }
}
