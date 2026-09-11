//! Trainer party construction, validation, replacement, and victory rewards.
//!
//! Trainer construction receives a fixed personality and scales each party
//! entry's IV byte, so it consumes RNG only while rolling non-shiny
//! original-trainer IDs. Callers run [`ensure_trainer_party_startable`] before
//! [`build_trainer_pokemon`], as `flow::npc_trainer_battle` does: construction
//! draws OT IDs before `Battle::new_trainer` can refuse an unexecutable move.

use assets::trainers::{AiFlags, TrainerClass, TrainerData, TrainerId, TrainerParty, TrainerTable};
use assets::{Effectiveness, MoveId, SpeciesId, Type, TypeChart};

use crate::ability::{huge_power_attack, pinch_boosts_power};
use crate::damage::{
    apply_dual_type_effectiveness, apply_stab, base_damage, has_stab, BattleRng, DamageInput,
    MoveCategory, Weather,
};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::{BattlePokemon, Ivs};

/// The neutral type-effectiveness baseline (`TYPE_MUL_NORMAL`).
const NEUTRAL_TYPE_SCORE: u32 = 10;

/// `gBattleMoves[move].power`'s OHKO sentinel (`GUILLOTINE`, `HORN_DRILL`,
/// `FISSURE`, `SHEER_COLD`), the one power value upstream's most-damage pass
/// excludes (`pokeemerald/src/battle_ai_switch_items.c:776`).
const OHKO_POWER_SENTINEL: u8 = 1;

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
/// Returns any error from [`BattlePokemon::validate`] before consuming RNG.
pub fn build_trainer_pokemon(
    dex: &Dex,
    species: SpeciesId,
    level: u8,
    fixed_iv: Ivs,
    personality: u32,
    moves: Vec<MoveId>,
    rng: &mut impl BattleRng,
) -> Result<BattlePokemon, BattleError> {
    BattlePokemon::validate(dex, species, level, &moves)?;
    let ot_id = roll_non_shiny_ot_id(personality, rng);
    Ok(
        BattlePokemon::new(dex, species, level, fixed_iv, personality, moves)?
            .with_original_trainer_id(ot_id),
    )
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
#[derive(Debug, Clone)]
pub struct TrainerContext {
    id: TrainerId,
    class: TrainerClass,
    ai_flags: AiFlags,
    money: u32,
    bench: Vec<BattlePokemon>,
}

impl TrainerContext {
    #[must_use]
    pub(crate) fn new(id: TrainerId, data: &TrainerData, bench: Vec<BattlePokemon>) -> Self {
        Self {
            id,
            class: data.class,
            ai_flags: data.ai_flags,
            money: trainer_money(data),
            bench,
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

    /// Returns the number of party members remaining behind the active one.
    #[must_use]
    pub fn bench_len(&self) -> usize {
        self.bench.len()
    }

    /// Returns the remaining party members in send-out order.
    #[must_use]
    pub fn bench(&self) -> &[BattlePokemon] {
        &self.bench
    }

    /// Removes and returns the most suitable non-fainted member for a forced
    /// post-faint send-out: `GetMostSuitableMonToSwitchInto`'s type and
    /// most-damage passes, then party order. `fainted` is the battler being
    /// replaced. See the ledger's `GetMostSuitableMonToSwitchInto` and
    /// `OpponentHandleChoosePokemon` entries for the full upstream mapping
    /// and its one documented divergence (issue #1040).
    ///
    /// # Errors
    ///
    /// Returns an error if a bench member's move is missing from `dex` --
    /// never in practice, since trainer-party validation already screens
    /// every move (see [`ensure_trainer_party_startable`]).
    pub(crate) fn send_out_next(
        &mut self,
        dex: &Dex,
        fainted: &BattlePokemon,
        player: &BattlePokemon,
    ) -> Result<Option<BattlePokemon>, BattleError> {
        if let Some(index) = self.most_suitable_by_type(dex, player)? {
            return Ok(Some(self.bench.remove(index)));
        }
        if let Some(index) = self.most_suitable_by_damage(dex, fainted, player)? {
            return Ok(Some(self.bench.remove(index)));
        }
        Ok(self.send_out_first_healthy())
    }

    /// Removes and returns the next non-fainted member in party order.
    fn send_out_first_healthy(&mut self) -> Option<BattlePokemon> {
        while !self.bench.is_empty() {
            let mon = self.bench.remove(0);
            if !mon.is_fainted() {
                return Some(mon);
            }
        }
        None
    }

    /// The healthy bench member whose typing takes the most damage from
    /// `player` and knows a super-effective move back, retrying the
    /// next-worst typing otherwise.
    fn most_suitable_by_type(
        &self,
        dex: &Dex,
        player: &BattlePokemon,
    ) -> Result<Option<usize>, BattleError> {
        let mut invalid = vec![false; self.bench.len()];
        loop {
            let mut best_index = None;
            let mut best_score = 0;
            for (index, candidate) in self.bench.iter().enumerate() {
                if invalid[index] || candidate.is_fainted() {
                    continue;
                }
                let score = typing_suitability_score(player, candidate);
                if best_score < score {
                    best_score = score;
                    best_index = Some(index);
                }
            }
            let Some(index) = best_index else {
                return Ok(None);
            };
            if has_super_effective_move(dex, &self.bench[index], player)? {
                return Ok(Some(index));
            }
            invalid[index] = true;
        }
    }

    /// The healthy bench member whose own move deals `fainted`'s most damage
    /// to `player`. Matches upstream's use of `fainted` -- the battler
    /// actually being replaced -- as the attacker for stats, ability, and
    /// STAB (`pokeemerald/src/battle_ai_switch_items.c:772`-`:779`,
    /// `battle_script_commands.c:1306`-`:1311`, `:1547`-`:1552`); only each
    /// candidate's move (not upstream's stale one) supplies power and type,
    /// per [`send_out_next`]'s documented divergence.
    fn most_suitable_by_damage(
        &self,
        dex: &Dex,
        fainted: &BattlePokemon,
        player: &BattlePokemon,
    ) -> Result<Option<usize>, BattleError> {
        let mut best_index = None;
        let mut best_damage = 0;
        for (index, candidate) in self.bench.iter().enumerate() {
            if candidate.is_fainted() {
                continue;
            }
            for slot in candidate.moves() {
                let Some(damage) = damaging_move_damage(dex, slot.move_id, fainted, player)? else {
                    continue;
                };
                if best_damage < damage {
                    best_damage = damage;
                    best_index = Some(index);
                }
            }
        }
        Ok(best_index)
    }
}

/// Scores `candidate`'s typing against `player`'s, applying `player`'s two
/// type slots even when they repeat (`ModulateByTypeEffectiveness`'s two
/// unguarded calls in `pokeemerald/src/battle_ai_switch_items.c:709`-`:710`
/// square the effect for a single-typed `player`). Plain truncating
/// multiplication with no floor, unlike [`apply_dual_type_effectiveness`]:
/// upstream's suitability score (unlike real damage) has no minimum-one rule
/// (`pokeemerald/src/battle_ai_switch_items.c:605`-`:623`).
fn typing_suitability_score(player: &BattlePokemon, candidate: &BattlePokemon) -> u32 {
    let candidate_types = candidate.types();
    let player_types = player.types();
    let score = apply_raw_type_multiplier(NEUTRAL_TYPE_SCORE, player_types[0], candidate_types);
    apply_raw_type_multiplier(score, player_types[1], candidate_types)
}

/// Applies every [`TypeChart`] row matching `attacking_type` against each of
/// `defending_types`' distinct slots, truncating after each row with no
/// floor (`ModulateByTypeEffectiveness`, `pokeemerald/src/battle_ai_switch_items.c:605`-`:627`).
fn apply_raw_type_multiplier(score: u32, attacking_type: Type, defending_types: [Type; 2]) -> u32 {
    let mut score = score;
    let distinct = defending_types[1] != defending_types[0];
    for &(atk, def, effectiveness) in TypeChart::rows() {
        if atk != attacking_type {
            continue;
        }
        if def == defending_types[0] {
            score = raw_multiply(score, effectiveness);
        }
        if distinct && def == defending_types[1] {
            score = raw_multiply(score, effectiveness);
        }
    }
    score
}

fn raw_multiply(score: u32, effectiveness: Effectiveness) -> u32 {
    score * u32::from(effectiveness.multiplier_x10()) / 10
}

/// Whether `candidate` knows a damaging move super effective against
/// `player`.
fn has_super_effective_move(
    dex: &Dex,
    candidate: &BattlePokemon,
    player: &BattlePokemon,
) -> Result<bool, BattleError> {
    for slot in candidate.moves() {
        let move_data = dex.move_data(slot.move_id)?;
        if move_data.power == 0 {
            continue;
        }
        let Some(move_type) = move_data.move_type.battle_type() else {
            continue;
        };
        let effectiveness =
            apply_dual_type_effectiveness(NEUTRAL_TYPE_SCORE, move_type, player.types());
        if effectiveness > NEUTRAL_TYPE_SCORE {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Returns the damage `move_id` would deal from `attacker` to `defender`, or
/// `None` for the [`OHKO_POWER_SENTINEL`] upstream excludes here or a
/// `???`-typed move this crate cannot run type math on. A power-0 status
/// move is not excluded, matching upstream: its base-damage formula adds a
/// flat `+2` regardless of power. Screens, weather, burn, and a random roll
/// are not modelled: a switch-in candidate carries none of the first three,
/// and this selector never draws RNG upstream.
fn damaging_move_damage(
    dex: &Dex,
    move_id: MoveId,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
) -> Result<Option<u32>, BattleError> {
    let move_data = dex.move_data(move_id)?;
    if move_data.power == OHKO_POWER_SENTINEL {
        return Ok(None);
    }
    let Some(move_type) = move_data.move_type.battle_type() else {
        return Ok(None);
    };
    let category = MoveCategory::for_type(move_type);
    let (attack_stat, attack_stage) = attacker.attacking_stat(category);
    let attack_stat = huge_power_attack(attacker.ability(), category, attack_stat);
    let (defense_stat, defense_stage) = defender.defending_stat(category);
    let input = DamageInput {
        attacker_level: attacker.level(),
        power: u32::from(move_data.power),
        move_type,
        attack_stat,
        attack_stage,
        defense_stat,
        defense_stage,
        attacker_burned: false,
        reflect: false,
        light_screen: false,
        weather: Weather::None,
        is_solar_beam: false,
        attacker_pinch_boost: pinch_boosts_power(
            attacker.ability(),
            move_type,
            attacker.current_hp(),
            attacker.stats().max_hp,
        ),
    };
    let damage = base_damage(&input);
    let damage = apply_stab(damage, has_stab(attacker.types(), move_id, move_type));
    Ok(Some(apply_dual_type_effectiveness(
        damage,
        move_type,
        defender.types(),
    )))
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

pub(crate) fn ensure_move_playable(dex: &Dex, move_id: MoveId) -> Result<(), BattleError> {
    crate::battle::ensure_executable(dex, move_id)?;
    super::trainer_ai::ensure_scoreable(dex, move_id)
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
/// member, AI-flag, or move validation error.
pub fn ensure_trainer_party_startable(
    dex: &Dex,
    trainer: TrainerId,
    party: &[TrainerPartyMon<'_>],
) -> Result<(), BattleError> {
    if party.is_empty() {
        return Err(BattleError::EmptyTrainerParty(trainer));
    }
    let data = trainer_data(trainer)?;
    for mon in party {
        BattlePokemon::validate(dex, mon.species, mon.level, mon.moves)?;
    }
    super::trainer_ai::ensure_supported_flags(data.ai_flags)?;
    for mon in party {
        for move_id in mon.moves {
            ensure_move_playable(dex, *move_id)?;
        }
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
    use assets::trainers::{TrainerClass, TrainerId};
    use assets::{MoveId, SpeciesId};

    const MAY_ROUTE_103_MUDKIP: TrainerId = TrainerId(529);
    const UNKNOWN_TRAINER: TrainerId = TrainerId(60_000);
    const TREECKO: SpeciesId = SpeciesId(277);
    const POUND: MoveId = MoveId(1);
    const HARDEN: MoveId = MoveId(106);
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

    #[test]
    fn ensure_trainer_party_startable_accepts_a_real_constructible_party() {
        let dex = Dex::new();
        let moves = [POUND];
        assert_eq!(
            ensure_trainer_party_startable(
                &dex,
                MAY_ROUTE_103_MUDKIP,
                &[TrainerPartyMon {
                    species: TREECKO,
                    level: ROUTE_103_RIVAL_LEVEL,
                    moves: &moves,
                }],
            ),
            Ok(())
        );
    }

    #[test]
    fn the_pre_flight_reports_what_the_real_handoff_would_but_without_the_draws() {
        let dex = Dex::new();
        let moves = [HARDEN];
        let screened = ensure_trainer_party_startable(
            &dex,
            MAY_ROUTE_103_MUDKIP,
            &[TrainerPartyMon {
                species: TREECKO,
                level: ROUTE_103_RIVAL_LEVEL,
                moves: &moves,
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
    fn the_pre_flight_rejects_an_empty_party_and_an_unknown_trainer() {
        let dex = Dex::new();
        assert_eq!(
            ensure_trainer_party_startable(&dex, MAY_ROUTE_103_MUDKIP, &[]),
            Err(crate::error::BattleError::EmptyTrainerParty(
                MAY_ROUTE_103_MUDKIP
            ))
        );
        let moves = [POUND];
        assert_eq!(
            ensure_trainer_party_startable(
                &dex,
                UNKNOWN_TRAINER,
                &[TrainerPartyMon {
                    species: TREECKO,
                    level: ROUTE_103_RIVAL_LEVEL,
                    moves: &moves,
                }],
            ),
            Err(crate::error::BattleError::UnknownTrainer(UNKNOWN_TRAINER))
        );
    }

    #[test]
    fn an_out_of_range_trainer_id_is_rejected_rather_than_panicking() {
        assert_eq!(
            trainer_data(UNKNOWN_TRAINER).unwrap_err(),
            crate::error::BattleError::UnknownTrainer(UNKNOWN_TRAINER)
        );
    }
}
