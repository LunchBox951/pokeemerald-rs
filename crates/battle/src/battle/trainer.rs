//! Trainer party construction, validation, replacement, and victory rewards.
//!
//! Trainer construction receives a fixed personality and scales each party
//! entry's IV byte, so it consumes RNG only while rolling non-shiny
//! original-trainer IDs. Unsupported parties are screened before construction
//! to leave the shared RNG stream unchanged.

use assets::trainers::{AiFlags, TrainerClass, TrainerData, TrainerId, TrainerParty, TrainerTable};
use assets::{MoveId, SpeciesId};

use crate::damage::BattleRng;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::{BattlePokemon, Ivs};

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

const TRAINER_CLASS_HIKER: TrainerClass = TrainerClass(0x02);
const TRAINER_CLASS_TEAM_AQUA: TrainerClass = TrainerClass(0x03);
const TRAINER_CLASS_PKMN_BREEDER: TrainerClass = TrainerClass(0x04);
const TRAINER_CLASS_COOLTRAINER: TrainerClass = TrainerClass(0x05);
const TRAINER_CLASS_BIRD_KEEPER: TrainerClass = TrainerClass(0x06);
const TRAINER_CLASS_COLLECTOR: TrainerClass = TrainerClass(0x07);
const TRAINER_CLASS_SWIMMER_M: TrainerClass = TrainerClass(0x08);
const TRAINER_CLASS_TEAM_MAGMA: TrainerClass = TrainerClass(0x09);
const TRAINER_CLASS_EXPERT: TrainerClass = TrainerClass(0x0a);
const TRAINER_CLASS_AQUA_ADMIN: TrainerClass = TrainerClass(0x0b);
const TRAINER_CLASS_BLACK_BELT: TrainerClass = TrainerClass(0x0c);
const TRAINER_CLASS_AQUA_LEADER: TrainerClass = TrainerClass(0x0d);
const TRAINER_CLASS_HEX_MANIAC: TrainerClass = TrainerClass(0x0e);
const TRAINER_CLASS_AROMA_LADY: TrainerClass = TrainerClass(0x0f);
const TRAINER_CLASS_RUIN_MANIAC: TrainerClass = TrainerClass(0x10);
const TRAINER_CLASS_INTERVIEWER: TrainerClass = TrainerClass(0x11);
const TRAINER_CLASS_TUBER_F: TrainerClass = TrainerClass(0x12);
const TRAINER_CLASS_TUBER_M: TrainerClass = TrainerClass(0x13);
const TRAINER_CLASS_LADY: TrainerClass = TrainerClass(0x14);
const TRAINER_CLASS_BEAUTY: TrainerClass = TrainerClass(0x15);
const TRAINER_CLASS_RICH_BOY: TrainerClass = TrainerClass(0x16);
const TRAINER_CLASS_POKEMANIAC: TrainerClass = TrainerClass(0x17);
const TRAINER_CLASS_GUITARIST: TrainerClass = TrainerClass(0x18);
const TRAINER_CLASS_KINDLER: TrainerClass = TrainerClass(0x19);
const TRAINER_CLASS_CAMPER: TrainerClass = TrainerClass(0x1a);
const TRAINER_CLASS_PICNICKER: TrainerClass = TrainerClass(0x1b);
const TRAINER_CLASS_BUG_MANIAC: TrainerClass = TrainerClass(0x1c);
const TRAINER_CLASS_PSYCHIC: TrainerClass = TrainerClass(0x1d);
const TRAINER_CLASS_GENTLEMAN: TrainerClass = TrainerClass(0x1e);
const TRAINER_CLASS_ELITE_FOUR: TrainerClass = TrainerClass(0x1f);
const TRAINER_CLASS_LEADER: TrainerClass = TrainerClass(0x20);
const TRAINER_CLASS_SCHOOL_KID: TrainerClass = TrainerClass(0x21);
const TRAINER_CLASS_SR_AND_JR: TrainerClass = TrainerClass(0x22);
const TRAINER_CLASS_WINSTRATE: TrainerClass = TrainerClass(0x23);
const TRAINER_CLASS_POKEFAN: TrainerClass = TrainerClass(0x24);
const TRAINER_CLASS_YOUNGSTER: TrainerClass = TrainerClass(0x25);
const TRAINER_CLASS_CHAMPION: TrainerClass = TrainerClass(0x26);
const TRAINER_CLASS_FISHERMAN: TrainerClass = TrainerClass(0x27);
const TRAINER_CLASS_TRIATHLETE: TrainerClass = TrainerClass(0x28);
const TRAINER_CLASS_DRAGON_TAMER: TrainerClass = TrainerClass(0x29);
const TRAINER_CLASS_NINJA_BOY: TrainerClass = TrainerClass(0x2a);
const TRAINER_CLASS_BATTLE_GIRL: TrainerClass = TrainerClass(0x2b);
const TRAINER_CLASS_PARASOL_LADY: TrainerClass = TrainerClass(0x2c);
const TRAINER_CLASS_SWIMMER_F: TrainerClass = TrainerClass(0x2d);
const TRAINER_CLASS_TWINS: TrainerClass = TrainerClass(0x2e);
const TRAINER_CLASS_SAILOR: TrainerClass = TrainerClass(0x2f);
const TRAINER_CLASS_MAGMA_ADMIN: TrainerClass = TrainerClass(0x31);
const TRAINER_CLASS_RIVAL: TrainerClass = TrainerClass(0x32);
const TRAINER_CLASS_BUG_CATCHER: TrainerClass = TrainerClass(0x33);
const TRAINER_CLASS_PKMN_RANGER: TrainerClass = TrainerClass(0x34);
const TRAINER_CLASS_MAGMA_LEADER: TrainerClass = TrainerClass(0x35);
const TRAINER_CLASS_LASS: TrainerClass = TrainerClass(0x36);
const TRAINER_CLASS_YOUNG_COUPLE: TrainerClass = TrainerClass(0x37);
const TRAINER_CLASS_OLD_COUPLE: TrainerClass = TrainerClass(0x38);
const TRAINER_CLASS_SIS_AND_BRO: TrainerClass = TrainerClass(0x39);

const TRAINER_MONEY_TABLE: [(TrainerClass, u32); 55] = [
    (TRAINER_CLASS_TEAM_AQUA, 5),
    (TRAINER_CLASS_AQUA_ADMIN, 10),
    (TRAINER_CLASS_AQUA_LEADER, 20),
    (TRAINER_CLASS_AROMA_LADY, 10),
    (TRAINER_CLASS_RUIN_MANIAC, 15),
    (TRAINER_CLASS_INTERVIEWER, 12),
    (TRAINER_CLASS_TUBER_F, 1),
    (TRAINER_CLASS_TUBER_M, 1),
    (TRAINER_CLASS_SIS_AND_BRO, 3),
    (TRAINER_CLASS_COOLTRAINER, 12),
    (TRAINER_CLASS_HEX_MANIAC, 6),
    (TRAINER_CLASS_LADY, 50),
    (TRAINER_CLASS_BEAUTY, 20),
    (TRAINER_CLASS_RICH_BOY, 50),
    (TRAINER_CLASS_POKEMANIAC, 15),
    (TRAINER_CLASS_SWIMMER_M, 2),
    (TRAINER_CLASS_BLACK_BELT, 8),
    (TRAINER_CLASS_GUITARIST, 8),
    (TRAINER_CLASS_KINDLER, 8),
    (TRAINER_CLASS_CAMPER, 4),
    (TRAINER_CLASS_OLD_COUPLE, 10),
    (TRAINER_CLASS_BUG_MANIAC, 15),
    (TRAINER_CLASS_PSYCHIC, 6),
    (TRAINER_CLASS_GENTLEMAN, 20),
    (TRAINER_CLASS_ELITE_FOUR, 25),
    (TRAINER_CLASS_LEADER, 25),
    (TRAINER_CLASS_SCHOOL_KID, 5),
    (TRAINER_CLASS_SR_AND_JR, 4),
    (TRAINER_CLASS_POKEFAN, 20),
    (TRAINER_CLASS_EXPERT, 10),
    (TRAINER_CLASS_YOUNGSTER, 4),
    (TRAINER_CLASS_CHAMPION, 50),
    (TRAINER_CLASS_FISHERMAN, 10),
    (TRAINER_CLASS_TRIATHLETE, 10),
    (TRAINER_CLASS_DRAGON_TAMER, 12),
    (TRAINER_CLASS_BIRD_KEEPER, 8),
    (TRAINER_CLASS_NINJA_BOY, 3),
    (TRAINER_CLASS_BATTLE_GIRL, 6),
    (TRAINER_CLASS_PARASOL_LADY, 10),
    (TRAINER_CLASS_SWIMMER_F, 2),
    (TRAINER_CLASS_PICNICKER, 4),
    (TRAINER_CLASS_TWINS, 3),
    (TRAINER_CLASS_SAILOR, 8),
    (TRAINER_CLASS_COLLECTOR, 15),
    (TRAINER_CLASS_RIVAL, 15),
    (TRAINER_CLASS_PKMN_BREEDER, 10),
    (TRAINER_CLASS_PKMN_RANGER, 12),
    (TRAINER_CLASS_TEAM_MAGMA, 5),
    (TRAINER_CLASS_MAGMA_ADMIN, 10),
    (TRAINER_CLASS_MAGMA_LEADER, 20),
    (TRAINER_CLASS_LASS, 4),
    (TRAINER_CLASS_BUG_CATCHER, 4),
    (TRAINER_CLASS_HIKER, 10),
    (TRAINER_CLASS_YOUNG_COUPLE, 8),
    (TRAINER_CLASS_WINSTRATE, 10),
];

/// The prize multiplier for a trainer class absent from the money table.
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

    /// Removes and returns the next non-fainted member in party order.
    ///
    /// This models `OpponentHandleChoosePokemon`'s fallback scan, not its type-match
    /// preference. Current reachable encounters cannot create a bench: rivals have
    /// one member; Route 103's single-battle sight trainers fail before construction,
    /// Miguel on held-item decoding and the other six on move preflight.
    pub(crate) fn send_out_next(&mut self) -> Option<BattlePokemon> {
        while !self.bench.is_empty() {
            let mon = self.bench.remove(0);
            if !mon.is_fainted() {
                return Some(mon);
            }
        }
        None
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
        DEFAULT_MONEY_VALUE, SHINY_ODDS, TRAINER_CLASS_CHAMPION, TRAINER_CLASS_RIVAL,
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
        assert_eq!(data.class, TRAINER_CLASS_RIVAL);
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
        assert_eq!(money_value_for_class(TRAINER_CLASS_CHAMPION), 50);
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
