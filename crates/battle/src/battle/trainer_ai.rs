//! Trainer AI admission and move scoring.
//!
//! Route 103 trainers use four scoring scripts over three move effects. The
//! admission functions reject other flags and effects before a battle starts
//! because unmodelled scoring branches can consume different RNG draws.

use assets::trainers::AiFlags;
use assets::{MoveEffect, MoveId, Type, TypeChart};

use super::opponent_ai::{selectable_slot, EnemyAction};
use crate::damage::{apply_stab, apply_type_effectiveness, base_damage, has_stab, BattleRng};
use crate::damage::{DamageInput, MoveCategory, Weather};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::{BattlePokemon, MAX_MON_MOVES};
use crate::stat_change::{EFFECT_ATTACK_DOWN, EFFECT_DEFENSE_DOWN};
use crate::stat_stage::StatStage;

const PERCENT_SCALE: u32 = 100;
const AI_RANDOM_ROLL_MODULUS: u16 = 256;
const SIMULATED_DAMAGE_VARIANCE: u16 = 16;
const MINIMUM_DAMAGE: u32 = 1;
const MINIMUM_DAMAGING_MOVE_POWER: u8 = 2;

const INITIAL_MOVE_SCORE: i8 = 100;
const UNUSABLE_MOVE_SCORE: i8 = 0;
const STRONGLY_DISCOURAGE: i8 = -10;
const SLIGHTLY_DISCOURAGE: i8 = -1;
const DISCOURAGE: i8 = -2;
const ENCOURAGE: i8 = 2;
const STRONGLY_ENCOURAGE: i8 = 4;

const QUADRUPLE_EFFECTIVENESS_BONUS_THRESHOLD: u16 = 80;
const STAT_DROP_DISCOURAGEMENT_THRESHOLD: u16 = 50;
const FIRST_TURN_SETUP_BONUS_THRESHOLD: u16 = 80;
const ATTACK_DROP_USER_HP_THRESHOLD: u32 = 90;
const DEFENSE_DROP_USER_HP_THRESHOLD: u32 = 70;
const STAT_DROP_TARGET_HP_THRESHOLD: u32 = 70;
const HEAVILY_LOWERED_STAT_STAGE: i8 = -3;
const FIRST_TURN: u8 = 0;

const AI_EFFECTIVENESS_NEUTRAL: u32 = 40;
const AI_EFFECTIVENESS_QUADRUPLE: u32 = 160;
const AI_EFFECTIVENESS_QUADRUPLE_WITH_STAB: u32 = 240;

/// The plain damaging move effect used by Pound, Scratch, and Tackle.
pub const EFFECT_HIT: MoveEffect = MoveEffect(0);

const SUPPORTED_AI_FLAGS: u32 = AiFlags::CHECK_BAD_MOVE.bits()
    | AiFlags::TRY_TO_FAINT.bits()
    | AiFlags::CHECK_VIABILITY.bits()
    | AiFlags::SETUP_FIRST_TURN.bits();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScoreableEffect {
    Hit,
    AttackDown,
    DefenseDown,
}

impl ScoreableEffect {
    const fn from_raw(effect: MoveEffect) -> Option<Self> {
        if effect.0 == EFFECT_HIT.0 {
            Some(Self::Hit)
        } else if effect.0 == EFFECT_ATTACK_DOWN.0 {
            Some(Self::AttackDown)
        } else if effect.0 == EFFECT_DEFENSE_DOWN.0 {
            Some(Self::DefenseDown)
        } else {
            None
        }
    }

    const fn is_stat_drop(self) -> bool {
        matches!(self, Self::AttackDown | Self::DefenseDown)
    }
}

#[must_use]
pub(crate) const fn is_scoreable_effect(effect: MoveEffect) -> bool {
    ScoreableEffect::from_raw(effect).is_some()
}

fn scoreable_effect(dex: &Dex, move_id: MoveId) -> Result<ScoreableEffect, BattleError> {
    ScoreableEffect::from_raw(dex.move_data(move_id)?.effect)
        .ok_or(BattleError::UnscoreableMoveEffect(move_id))
}

/// Rejects a move whose effect the trainer AI cannot score.
///
/// # Errors
///
/// Returns [`BattleError::UnknownMove`] or
/// [`BattleError::UnscoreableMoveEffect`].
pub(crate) fn ensure_scoreable(dex: &Dex, move_id: MoveId) -> Result<(), BattleError> {
    if is_scoreable_effect(dex.move_data(move_id)?.effect) {
        Ok(())
    } else {
        Err(BattleError::UnscoreableMoveEffect(move_id))
    }
}

/// Rejects trainer AI flags without a modelled scoring script.
///
/// # Errors
///
/// Returns [`BattleError::UnsupportedAiFlags`] containing only the unsupported
/// bits.
pub(crate) const fn ensure_supported_flags(flags: AiFlags) -> Result<(), BattleError> {
    let unsupported_bits = flags.bits() & !SUPPORTED_AI_FLAGS;
    if unsupported_bits == 0 {
        Ok(())
    } else {
        Err(BattleError::UnsupportedAiFlags(AiFlags(unsupported_bits)))
    }
}

struct MoveScores {
    values: [i8; MAX_MON_MOVES],
    simulated_damage_percent: [u32; MAX_MON_MOVES],
}

impl MoveScores {
    fn initialize(enemy: &BattlePokemon, rng: &mut impl BattleRng) -> Self {
        let mut values = [UNUSABLE_MOVE_SCORE; MAX_MON_MOVES];
        for (slot, score) in values.iter_mut().enumerate() {
            if move_slot_can_be_scored(enemy, slot) {
                *score = INITIAL_MOVE_SCORE;
            }
        }

        let mut simulated_damage_percent = [0; MAX_MON_MOVES];
        for percent in &mut simulated_damage_percent {
            *percent = PERCENT_SCALE - u32::from(rng.next_u16() % SIMULATED_DAMAGE_VARIANCE);
        }

        Self {
            values,
            simulated_damage_percent,
        }
    }

    /// Emerald adds score bytes as wrapping `i8` values before flooring a
    /// negative result at zero (`src/battle_ai_script_commands.c:703`).
    fn adjust(&mut self, slot: usize, delta: i8) {
        self.values[slot] = self.values[slot]
            .wrapping_add(delta)
            .max(UNUSABLE_MOVE_SCORE);
    }

    fn discard(&mut self, slot: usize) {
        self.values[slot] = UNUSABLE_MOVE_SCORE;
    }
}

#[derive(Debug, Clone, Copy)]
enum ScoringScript {
    CheckBadMove,
    TryToFaint,
    CheckViability,
    SetupFirstTurn,
}

const SCORING_SCRIPTS_IN_FLAG_ORDER: [(AiFlags, ScoringScript); 4] = [
    (AiFlags::CHECK_BAD_MOVE, ScoringScript::CheckBadMove),
    (AiFlags::TRY_TO_FAINT, ScoringScript::TryToFaint),
    (AiFlags::CHECK_VIABILITY, ScoringScript::CheckViability),
    (AiFlags::SETUP_FIRST_TURN, ScoringScript::SetupFirstTurn),
];

/// Chooses a trainer opponent's action after validating its AI flags.
///
/// Scoring consumes four simulated-damage draws, runs enabled scripts in
/// ascending flag order, and consumes one final tie-break draw. A trainer with
/// no usable move returns Struggle without drawing.
///
/// # Errors
///
/// Returns an unsupported flag, move, or move-effect error if the caller did
/// not run the trainer admission checks first.
pub(crate) fn choose_trainer_action(
    dex: &Dex,
    enemy: &BattlePokemon,
    player: &BattlePokemon,
    ai_flags: AiFlags,
    turn_counter: u8,
    rng: &mut impl BattleRng,
) -> Result<EnemyAction, BattleError> {
    ensure_supported_flags(ai_flags)?;

    if all_known_moves_are_spent(enemy) {
        return Ok(EnemyAction::Struggle);
    }

    let mut scores = MoveScores::initialize(enemy, rng);
    for (flag, script) in SCORING_SCRIPTS_IN_FLAG_ORDER {
        if !ai_flags.contains(flag) {
            continue;
        }

        for slot in 0..MAX_MON_MOVES {
            if !move_slot_can_be_scored(enemy, slot) {
                scores.discard(slot);
                continue;
            }

            let move_id = enemy.moves()[slot].move_id;
            match script {
                ScoringScript::CheckBadMove => {
                    score_bad_move(dex, &mut scores, slot, move_id, enemy, player)?;
                }
                ScoringScript::TryToFaint => {
                    score_try_to_faint(dex, &mut scores, slot, move_id, enemy, player, rng)?;
                }
                ScoringScript::CheckViability => {
                    score_viability(dex, &mut scores, slot, move_id, enemy, player, rng)?;
                }
                ScoringScript::SetupFirstTurn => {
                    score_first_turn_setup(dex, &mut scores, slot, move_id, turn_counter, rng)?;
                }
            }
        }
    }

    let selected_slot = select_highest_scoring_move(enemy, scores.values, rng);
    Ok(EnemyAction::Move(selected_slot))
}

fn all_known_moves_are_spent(pokemon: &BattlePokemon) -> bool {
    pokemon.moves().iter().all(|slot| slot.pp == 0)
}

fn move_slot_can_be_scored(pokemon: &BattlePokemon, slot: usize) -> bool {
    selectable_slot(pokemon.move_at(slot)) && pokemon.moves()[slot].pp > 0
}

/// Emerald admits slot zero before checking occupancy and always draws for the
/// final tie-break (`src/battle_ai_script_commands.c:423`-`:445`).
fn select_highest_scoring_move(
    enemy: &BattlePokemon,
    scores: [i8; MAX_MON_MOVES],
    rng: &mut impl BattleRng,
) -> usize {
    let mut highest_score = scores[0];
    let mut highest_scoring_slots = vec![0];

    for (slot, score) in scores.iter().copied().enumerate().skip(1) {
        if !selectable_slot(enemy.move_at(slot)) {
            continue;
        }
        if score == highest_score {
            highest_scoring_slots.push(slot);
        }
        if score > highest_score {
            highest_score = score;
            highest_scoring_slots.clear();
            highest_scoring_slots.push(slot);
        }
    }

    let selected = usize::from(rng.next_u16()) % highest_scoring_slots.len();
    highest_scoring_slots[selected]
}

fn current_hp_percent(pokemon: &BattlePokemon) -> u32 {
    PERCENT_SCALE * pokemon.current_hp() / pokemon.stats().max_hp
}

fn random_roll_is_at_least(rng: &mut impl BattleRng, threshold: u16) -> bool {
    rng.next_u16() % AI_RANDOM_ROLL_MODULUS >= threshold
}

/// Preserves Emerald's dual non-immunity glitch: its AI type command folds
/// rows without carrying an immunity flag, so a later non-immune row can lift
/// zero damage back to one (`src/battle_ai_script_commands.c:1515`-`:1556`).
fn fold_ai_type_effectiveness(damage: u32, move_type: Type, defender_types: [Type; 2]) -> u32 {
    let defender_has_two_types = defender_types[1] != defender_types[0];
    let mut damage = damage;

    for &(attacker_type, defender_type, effectiveness) in TypeChart::rows() {
        if attacker_type != move_type {
            continue;
        }
        let applies_to_defender = defender_type == defender_types[0]
            || (defender_has_two_types && defender_type == defender_types[1]);
        if applies_to_defender {
            damage = apply_type_effectiveness(damage, effectiveness);
        }
    }

    damage
}

fn ai_type_effectiveness(
    dex: &Dex,
    move_id: MoveId,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
) -> Result<Option<u32>, BattleError> {
    let Some(move_type) = dex.move_data(move_id)?.move_type.battle_type() else {
        return Ok(None);
    };
    let neutral_with_stab = apply_stab(
        AI_EFFECTIVENESS_NEUTRAL,
        has_stab(attacker.types(), move_id, move_type),
    );
    Ok(Some(fold_ai_type_effectiveness(
        neutral_with_stab,
        move_type,
        defender.types(),
    )))
}

fn is_quadruple_effective(
    dex: &Dex,
    move_id: MoveId,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
) -> Result<bool, BattleError> {
    let Some(effectiveness) = ai_type_effectiveness(dex, move_id, attacker, defender)? else {
        return Ok(false);
    };
    Ok(matches!(
        effectiveness,
        AI_EFFECTIVENESS_QUADRUPLE | AI_EFFECTIVENESS_QUADRUPLE_WITH_STAB
    ))
}

fn has_no_effect(
    dex: &Dex,
    move_id: MoveId,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
) -> Result<bool, BattleError> {
    Ok(ai_type_effectiveness(dex, move_id, attacker, defender)? == Some(0))
}

fn estimated_damage(
    dex: &Dex,
    move_id: MoveId,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
    simulated_damage_percent: u32,
) -> Result<u32, BattleError> {
    let move_data = dex.move_data(move_id)?;
    let Some(move_type) = move_data.move_type.battle_type() else {
        return Ok(MINIMUM_DAMAGE);
    };
    let category = MoveCategory::for_type(move_type);
    let (attack_stat, attack_stage) = attacker.attacking_stat(category);
    let attack_stat = crate::ability::huge_power_attack(attacker.ability(), category, attack_stat);
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
        attacker_pinch_boost: crate::ability::pinch_boosts_power(
            attacker.ability(),
            move_type,
            attacker.current_hp(),
            attacker.stats().max_hp,
        ),
    };
    let damage = base_damage(&input);
    let damage = apply_stab(damage, has_stab(attacker.types(), move_id, move_type));
    let damage = fold_ai_type_effectiveness(damage, move_type, defender.types());
    Ok((damage * simulated_damage_percent / PERCENT_SCALE).max(MINIMUM_DAMAGE))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PowerComparison {
    NotComparable,
    WeakerThanAnotherMove,
    Strongest,
}

fn compare_move_power(
    dex: &Dex,
    scores: &MoveScores,
    considered_slot: usize,
    move_id: MoveId,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
) -> Result<PowerComparison, BattleError> {
    if dex.move_data(move_id)?.power < MINIMUM_DAMAGING_MOVE_POWER {
        return Ok(PowerComparison::NotComparable);
    }

    let mut damage_by_slot = [0; MAX_MON_MOVES];
    for (slot, damage) in damage_by_slot.iter_mut().enumerate() {
        let Some(candidate_move) = attacker.move_at(slot) else {
            continue;
        };
        let candidate_data = dex.move_data(candidate_move)?;
        if candidate_data.power < MINIMUM_DAMAGING_MOVE_POWER {
            continue;
        }
        *damage = estimated_damage(
            dex,
            candidate_move,
            attacker,
            defender,
            scores.simulated_damage_percent[slot],
        )?;
    }

    if damage_by_slot
        .iter()
        .any(|damage| *damage > damage_by_slot[considered_slot])
    {
        Ok(PowerComparison::WeakerThanAnotherMove)
    } else {
        Ok(PowerComparison::Strongest)
    }
}

fn score_bad_move(
    dex: &Dex,
    scores: &mut MoveScores,
    slot: usize,
    move_id: MoveId,
    user: &BattlePokemon,
    target: &BattlePokemon,
) -> Result<(), BattleError> {
    let effect = scoreable_effect(dex, move_id)?;
    let is_ineffective = match effect {
        ScoreableEffect::Hit => has_no_effect(dex, move_id, user, target)?,
        ScoreableEffect::AttackDown => target.stages().attack == StatStage::MIN,
        ScoreableEffect::DefenseDown => target.stages().defense == StatStage::MIN,
    };
    if is_ineffective {
        scores.adjust(slot, STRONGLY_DISCOURAGE);
    }
    Ok(())
}

fn score_try_to_faint(
    dex: &Dex,
    scores: &mut MoveScores,
    slot: usize,
    move_id: MoveId,
    user: &BattlePokemon,
    target: &BattlePokemon,
    rng: &mut impl BattleRng,
) -> Result<(), BattleError> {
    let move_data = dex.move_data(move_id)?;
    if move_data.power >= MINIMUM_DAMAGING_MOVE_POWER {
        let damage = estimated_damage(
            dex,
            move_id,
            user,
            target,
            scores.simulated_damage_percent[slot],
        )?;
        if target.current_hp() <= damage {
            scores.adjust(slot, STRONGLY_ENCOURAGE);
            return Ok(());
        }
    }

    if compare_move_power(dex, scores, slot, move_id, user, target)?
        == PowerComparison::WeakerThanAnotherMove
    {
        scores.adjust(slot, SLIGHTLY_DISCOURAGE);
        return Ok(());
    }

    if is_quadruple_effective(dex, move_id, user, target)?
        && random_roll_is_at_least(rng, QUADRUPLE_EFFECTIVENESS_BONUS_THRESHOLD)
    {
        scores.adjust(slot, ENCOURAGE);
    }
    Ok(())
}

fn score_viability(
    dex: &Dex,
    scores: &mut MoveScores,
    slot: usize,
    move_id: MoveId,
    user: &BattlePokemon,
    target: &BattlePokemon,
    rng: &mut impl BattleRng,
) -> Result<(), BattleError> {
    match scoreable_effect(dex, move_id)? {
        ScoreableEffect::Hit => {}
        ScoreableEffect::AttackDown => {
            score_attack_drop_viability(scores, slot, user, target, rng);
        }
        ScoreableEffect::DefenseDown => {
            score_defense_drop_viability(scores, slot, user, target, rng);
        }
    }
    Ok(())
}

/// Exact six-entry `AI_CV_AttackDown_PhysicalTypeList`. Emerald omits Flying,
/// Poison, and Ghost (`data/battle_ai_scripts.s:1115`-`:1121`).
const PHYSICAL_TYPE_LIST: [Type; 6] = [
    Type::Normal,
    Type::Fighting,
    Type::Ground,
    Type::Rock,
    Type::Bug,
    Type::Steel,
];

fn has_physical_type(pokemon: &BattlePokemon) -> bool {
    pokemon
        .types()
        .iter()
        .any(|pokemon_type| PHYSICAL_TYPE_LIST.contains(pokemon_type))
}

fn score_attack_drop_viability(
    scores: &mut MoveScores,
    slot: usize,
    user: &BattlePokemon,
    target: &BattlePokemon,
    rng: &mut impl BattleRng,
) {
    let attack_stage = target.stages().attack;
    if attack_stage != StatStage::NEUTRAL {
        scores.adjust(slot, SLIGHTLY_DISCOURAGE);
        if current_hp_percent(user) <= ATTACK_DROP_USER_HP_THRESHOLD {
            scores.adjust(slot, SLIGHTLY_DISCOURAGE);
        }
        if attack_stage.offset() <= HEAVILY_LOWERED_STAT_STAGE
            && random_roll_is_at_least(rng, STAT_DROP_DISCOURAGEMENT_THRESHOLD)
        {
            scores.adjust(slot, DISCOURAGE);
        }
    }

    if current_hp_percent(target) <= STAT_DROP_TARGET_HP_THRESHOLD {
        scores.adjust(slot, DISCOURAGE);
    }
    if !has_physical_type(target)
        && random_roll_is_at_least(rng, STAT_DROP_DISCOURAGEMENT_THRESHOLD)
    {
        scores.adjust(slot, DISCOURAGE);
    }
}

fn score_defense_drop_viability(
    scores: &mut MoveScores,
    slot: usize,
    user: &BattlePokemon,
    target: &BattlePokemon,
    rng: &mut impl BattleRng,
) {
    let user_is_healthy = current_hp_percent(user) >= DEFENSE_DROP_USER_HP_THRESHOLD;
    let defense_is_not_heavily_lowered =
        target.stages().defense.offset() > HEAVILY_LOWERED_STAT_STAGE;
    let skip_discouragement_roll = user_is_healthy && defense_is_not_heavily_lowered;

    if !skip_discouragement_roll && random_roll_is_at_least(rng, STAT_DROP_DISCOURAGEMENT_THRESHOLD)
    {
        scores.adjust(slot, DISCOURAGE);
    }
    if current_hp_percent(target) <= STAT_DROP_TARGET_HP_THRESHOLD {
        scores.adjust(slot, DISCOURAGE);
    }
}

fn score_first_turn_setup(
    dex: &Dex,
    scores: &mut MoveScores,
    slot: usize,
    move_id: MoveId,
    turn_counter: u8,
    rng: &mut impl BattleRng,
) -> Result<(), BattleError> {
    if turn_counter != FIRST_TURN {
        return Ok(());
    }
    if !scoreable_effect(dex, move_id)?.is_stat_drop() {
        return Ok(());
    }
    if random_roll_is_at_least(rng, FIRST_TURN_SETUP_BONUS_THRESHOLD) {
        scores.adjust(slot, ENCOURAGE);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        choose_trainer_action, ensure_scoreable, ensure_supported_flags, estimated_damage,
        is_scoreable_effect, EFFECT_HIT, FIRST_TURN, FIRST_TURN_SETUP_BONUS_THRESHOLD,
        PERCENT_SCALE, STAT_DROP_DISCOURAGEMENT_THRESHOLD,
    };
    use crate::battle::opponent_ai::EnemyAction;
    use crate::dex::Dex;
    use crate::error::BattleError;
    use crate::pokemon::{BattlePokemon, Ivs, MAX_MON_MOVES};
    use crate::script_rng::SequenceRng;
    use assets::trainers::AiFlags;
    use assets::{MoveId, SpeciesId};

    const ROUTE_103_LEVEL: u8 = 5;
    const DEFAULT_PERSONALITY: u32 = 0;
    const SECOND_TURN: u8 = 1;
    const HUGE_POWER_ABILITY_SLOT: u8 = 1;
    const MAXIMUM_SIMULATED_DAMAGE_DRAW: u16 = 0;
    const SELECT_FIRST_TIED_MOVE: u16 = 0;
    const SELECT_SECOND_TIED_MOVE: u16 = 1;
    const MAXIMUM_DAMAGE_ROLL: u16 = 0;

    const TREECKO: SpeciesId = SpeciesId(277);
    const TORCHIC: SpeciesId = SpeciesId(280);
    const MUDKIP: SpeciesId = SpeciesId(283);
    const MARILL: SpeciesId = SpeciesId(183);
    const SQUIRTLE: SpeciesId = SpeciesId(7);

    const POUND: MoveId = MoveId(1);
    const SCRATCH: MoveId = MoveId(10);
    const TACKLE: MoveId = MoveId(33);
    const LEER: MoveId = MoveId(43);
    const GROWL: MoveId = MoveId(45);
    const EMBER: MoveId = MoveId(52);

    fn route_103_flags() -> AiFlags {
        AiFlags::CHECK_BAD_MOVE
            .union(AiFlags::TRY_TO_FAINT)
            .union(AiFlags::CHECK_VIABILITY)
    }

    fn first_turn_setup_flags() -> AiFlags {
        AiFlags::CHECK_BAD_MOVE
            .union(AiFlags::TRY_TO_FAINT)
            .union(AiFlags::SETUP_FIRST_TURN)
    }

    fn pokemon(species: SpeciesId, moves: Vec<MoveId>) -> BattlePokemon {
        BattlePokemon::new(
            &Dex::new(),
            species,
            ROUTE_103_LEVEL,
            Ivs::default(),
            DEFAULT_PERSONALITY,
            moves,
        )
        .expect("test Pokemon must be dex-resident")
    }

    fn spend_move(pokemon: &mut BattlePokemon, slot: usize) {
        while pokemon.moves()[slot].pp > 0 {
            pokemon.deduct_pp(slot).unwrap();
        }
    }

    fn rng_with_maximum_simulated_damage(
        subsequent_draws: impl IntoIterator<Item = u16>,
    ) -> SequenceRng {
        SequenceRng::new(
            [MAXIMUM_SIMULATED_DAMAGE_DRAW; MAX_MON_MOVES]
                .into_iter()
                .chain(subsequent_draws),
        )
    }

    #[test]
    fn route_103_move_effects_are_scoreable_and_ember_is_not() {
        let dex = Dex::new();

        assert!(is_scoreable_effect(EFFECT_HIT));
        assert!(ensure_scoreable(&dex, POUND).is_ok());
        assert!(ensure_scoreable(&dex, GROWL).is_ok());
        assert!(ensure_scoreable(&dex, LEER).is_ok());
        assert_eq!(
            ensure_scoreable(&dex, EMBER),
            Err(BattleError::UnscoreableMoveEffect(EMBER))
        );
    }

    #[test]
    fn only_the_four_lowest_ai_script_bits_are_supported() {
        assert!(ensure_supported_flags(route_103_flags()).is_ok());
        assert!(ensure_supported_flags(first_turn_setup_flags()).is_ok());
        assert_eq!(
            ensure_supported_flags(AiFlags::CHECK_BAD_MOVE.union(AiFlags::RISKY)),
            Err(BattleError::UnsupportedAiFlags(AiFlags::RISKY))
        );
    }

    #[test]
    fn every_spent_move_forces_struggle_without_drawing() {
        let mut enemy = pokemon(TORCHIC, vec![SCRATCH, GROWL]);
        for slot in 0..enemy.moves().len() {
            spend_move(&mut enemy, slot);
        }
        let player = pokemon(TREECKO, vec![POUND]);
        let mut rng = SequenceRng::new([]);

        assert_eq!(
            choose_trainer_action(
                &Dex::new(),
                &enemy,
                &player,
                route_103_flags(),
                FIRST_TURN,
                &mut rng,
            )
            .unwrap(),
            EnemyAction::Struggle
        );
        assert_eq!(rng.draws(), 0);
    }

    #[test]
    fn opening_tackle_and_growl_cost_setup_viability_and_tie_break_draws() {
        let enemy = pokemon(MUDKIP, vec![TACKLE, GROWL]);
        let player = pokemon(TORCHIC, vec![SCRATCH]);
        let mut rng = rng_with_maximum_simulated_damage([
            STAT_DROP_DISCOURAGEMENT_THRESHOLD,
            SELECT_FIRST_TIED_MOVE,
        ]);

        let action = choose_trainer_action(
            &Dex::new(),
            &enemy,
            &player,
            route_103_flags(),
            FIRST_TURN,
            &mut rng,
        )
        .unwrap();

        assert_eq!(action, EnemyAction::Move(0));
        assert_eq!(rng.draws(), MAX_MON_MOVES + 2);
    }

    #[test]
    fn opening_leer_skips_the_viability_roll_and_ties_with_pound() {
        let enemy = pokemon(TREECKO, vec![POUND, LEER]);
        let player = pokemon(MUDKIP, vec![TACKLE]);
        let mut choose_pound = rng_with_maximum_simulated_damage([SELECT_FIRST_TIED_MOVE]);
        let mut choose_leer = rng_with_maximum_simulated_damage([SELECT_SECOND_TIED_MOVE]);

        assert_eq!(
            choose_trainer_action(
                &Dex::new(),
                &enemy,
                &player,
                route_103_flags(),
                FIRST_TURN,
                &mut choose_pound,
            )
            .unwrap(),
            EnemyAction::Move(0)
        );
        assert_eq!(choose_pound.draws(), MAX_MON_MOVES + 1);
        assert_eq!(
            choose_trainer_action(
                &Dex::new(),
                &enemy,
                &player,
                route_103_flags(),
                FIRST_TURN,
                &mut choose_leer,
            )
            .unwrap(),
            EnemyAction::Move(1)
        );
        assert_eq!(choose_leer.draws(), MAX_MON_MOVES + 1);
    }

    #[test]
    fn a_finishable_target_makes_tackle_outscore_growl() {
        let dex = Dex::new();
        let enemy = pokemon(MUDKIP, vec![TACKLE, GROWL]);
        let mut player = pokemon(TORCHIC, vec![SCRATCH]);
        let estimate = estimated_damage(&dex, TACKLE, &enemy, &player, PERCENT_SCALE).unwrap();
        player.apply_damage(player.stats().max_hp - estimate);
        assert_eq!(player.current_hp(), estimate);

        let mut rng = rng_with_maximum_simulated_damage([
            STAT_DROP_DISCOURAGEMENT_THRESHOLD,
            SELECT_FIRST_TIED_MOVE,
        ]);

        assert_eq!(
            choose_trainer_action(
                &dex,
                &enemy,
                &player,
                route_103_flags(),
                FIRST_TURN,
                &mut rng,
            )
            .unwrap(),
            EnemyAction::Move(0)
        );
    }

    #[test]
    fn setup_first_turn_scores_stat_moves_only_on_turn_one() {
        let enemy = pokemon(TREECKO, vec![POUND, LEER]);
        let player = pokemon(MUDKIP, vec![TACKLE]);
        let mut first_turn_rng = rng_with_maximum_simulated_damage([
            FIRST_TURN_SETUP_BONUS_THRESHOLD,
            SELECT_FIRST_TIED_MOVE,
        ]);

        assert_eq!(
            choose_trainer_action(
                &Dex::new(),
                &enemy,
                &player,
                first_turn_setup_flags(),
                FIRST_TURN,
                &mut first_turn_rng,
            )
            .unwrap(),
            EnemyAction::Move(1)
        );
        assert_eq!(first_turn_rng.draws(), MAX_MON_MOVES + 2);

        let mut second_turn_rng = rng_with_maximum_simulated_damage([SELECT_FIRST_TIED_MOVE]);
        assert_eq!(
            choose_trainer_action(
                &Dex::new(),
                &enemy,
                &player,
                first_turn_setup_flags(),
                SECOND_TURN,
                &mut second_turn_rng,
            )
            .unwrap(),
            EnemyAction::Move(0)
        );
        assert_eq!(second_turn_rng.draws(), MAX_MON_MOVES + 1);
    }

    #[test]
    fn a_spent_move_is_neither_scored_nor_selected() {
        let mut enemy = pokemon(MUDKIP, vec![TACKLE, GROWL]);
        spend_move(&mut enemy, 0);
        let player = pokemon(TORCHIC, vec![SCRATCH]);
        let mut rng = rng_with_maximum_simulated_damage([
            STAT_DROP_DISCOURAGEMENT_THRESHOLD,
            SELECT_FIRST_TIED_MOVE,
        ]);

        assert_eq!(
            choose_trainer_action(
                &Dex::new(),
                &enemy,
                &player,
                route_103_flags(),
                FIRST_TURN,
                &mut rng,
            )
            .unwrap(),
            EnemyAction::Move(1)
        );
        assert_eq!(rng.draws(), MAX_MON_MOVES + 2);
    }

    #[test]
    fn huge_power_estimated_damage_matches_the_real_damage_step() {
        let dex = Dex::new();
        let attacker = BattlePokemon::new(
            &dex,
            MARILL,
            ROUTE_103_LEVEL,
            Ivs::default(),
            DEFAULT_PERSONALITY,
            vec![TACKLE],
        )
        .unwrap()
        .with_ability_slot(HUGE_POWER_ABILITY_SLOT);
        assert_eq!(attacker.ability(), crate::ability::HUGE_POWER);
        let defender = pokemon(SQUIRTLE, vec![TACKLE]);

        let critical_hits_suppressed = true;
        let mut no_draws = SequenceRng::new([]);
        let damage_before_roll = crate::hit::damage_before_roll(
            &dex,
            TACKLE,
            &attacker,
            &defender,
            critical_hits_suppressed,
            &mut no_draws,
        )
        .unwrap();
        let mut best_damage_roll = SequenceRng::new([MAXIMUM_DAMAGE_ROLL]);
        let real_damage =
            crate::damage::apply_damage_roll(damage_before_roll.damage, &mut best_damage_roll);
        let estimated_damage =
            estimated_damage(&dex, TACKLE, &attacker, &defender, PERCENT_SCALE).unwrap();

        assert_eq!(estimated_damage, real_damage);
    }
}
