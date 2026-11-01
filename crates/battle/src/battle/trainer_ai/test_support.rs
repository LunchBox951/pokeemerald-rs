//! Fixtures shared by the trainer AI test modules.

use crate::dex::Dex;
use crate::pokemon::BattlePokemon;
use crate::pokemon::Ivs;
use crate::pokemon::MAX_MON_MOVES;
use crate::script_rng::SequenceRng;
use assets::trainers::AiFlags;
use assets::MoveId;
use assets::SpeciesId;

pub(super) const ROUTE_103_LEVEL: u8 = 5;
pub(super) const DEFAULT_PERSONALITY: u32 = 0;
pub(super) const SECOND_TURN: u8 = 1;
pub(super) const HUGE_POWER_ABILITY_SLOT: u8 = 1;
pub(super) const GUTS_ABILITY_SLOT: u8 = 1;
pub(super) const HUSTLE_ABILITY_SLOT: u8 = 1;
const MAXIMUM_SIMULATED_DAMAGE_DRAW: u16 = 0;
pub(super) const SELECT_FIRST_TIED_MOVE: u16 = 0;
pub(super) const SELECT_SECOND_TIED_MOVE: u16 = 1;
pub(super) const MAXIMUM_DAMAGE_ROLL: u16 = 0;

pub(super) const TREECKO: SpeciesId = SpeciesId(277);
pub(super) const TORCHIC: SpeciesId = SpeciesId(280);
pub(super) const MUDKIP: SpeciesId = SpeciesId(283);
pub(super) const MARILL: SpeciesId = SpeciesId(183);
pub(super) const SQUIRTLE: SpeciesId = SpeciesId(7);
/// `SPECIES_MAKUHITA`: Guts in ability slot 1 (Thick Fat is slot 0).
pub(super) const MAKUHITA: SpeciesId = SpeciesId(335);
/// `SPECIES_MILOTIC`: Marvel Scale in its primary (and only) ability slot.
pub(super) const MILOTIC: SpeciesId = SpeciesId(329);
/// `SPECIES_DELIBIRD`: Vital Spirit in slot 0, Hustle in slot 1 -- unlike
/// a single-ability species, a same-species non-Hustle control is
/// reachable through `with_ability_slot`.
pub(super) const DELIBIRD: SpeciesId = SpeciesId(225);
/// `SPECIES_SHEDINJA`: Wonder Guard in its only ability slot.
pub(super) const SHEDINJA: SpeciesId = SpeciesId(303);
/// `SPECIES_VOLTORB`: Soundproof in slot 0 (Static is slot 1).
pub(super) const VOLTORB: SpeciesId = SpeciesId(100);
/// `SPECIES_PINSIR`: Hyper Cutter is its only ability.
pub(super) const PINSIR: SpeciesId = SpeciesId(127);
/// `SPECIES_REGIROCK`: Clear Body is its only ability.
pub(super) const REGIROCK: SpeciesId = SpeciesId(401);
/// `SPECIES_MR_MIME`: Soundproof is its only ability.
pub(super) const MR_MIME: SpeciesId = SpeciesId(122);
/// `SPECIES_GASTLY`: Levitate in its only ability slot.
pub(super) const GASTLY: SpeciesId = SpeciesId(92);
/// `SPECIES_TRAPINCH`: Hyper Cutter in slot 0, Arena Trap (a trapping
/// ability) in slot 1.
pub(super) const TRAPINCH: SpeciesId = SpeciesId(332);

pub(super) const POUND: MoveId = MoveId::POUND;
pub(super) const SCRATCH: MoveId = MoveId::SCRATCH;
pub(super) const WING_ATTACK: MoveId = MoveId::WING_ATTACK;
pub(super) const TACKLE: MoveId = MoveId::TACKLE;
pub(super) const LEER: MoveId = MoveId::LEER;
pub(super) const GROWL: MoveId = MoveId::GROWL;
pub(super) const EMBER: MoveId = MoveId::EMBER;
pub(super) const WATER_GUN: MoveId = MoveId::WATER_GUN;
pub(super) const EARTHQUAKE: MoveId = MoveId::EARTHQUAKE;
/// `MOVE_HYPER_VOICE`: absent from the AI's own Soundproof list
/// (`data/battle_ai_scripts.s:95-103`) despite belonging to execution's
/// `SOUND_MOVES` (`crates/battle/src/stat_change.rs:83-94`).
pub(super) const HYPER_VOICE: MoveId = MoveId::HYPER_VOICE;

pub(super) const SEISMIC_TOSS: MoveId = MoveId::SEISMIC_TOSS;
pub(super) const SONIC_BOOM: MoveId = MoveId::SONIC_BOOM;
/// Upstream draws one setup value per move slot, then one tie-break value
/// (`src/battle_ai_script_commands.c:322-342,445`).
pub(super) const FLAGLESS_DRAWS: usize = MAX_MON_MOVES + 1;

pub(super) fn route_103_flags() -> AiFlags {
    AiFlags::CHECK_BAD_MOVE
        .union(AiFlags::TRY_TO_FAINT)
        .union(AiFlags::CHECK_VIABILITY)
}

pub(super) fn first_turn_setup_flags() -> AiFlags {
    AiFlags::CHECK_BAD_MOVE
        .union(AiFlags::TRY_TO_FAINT)
        .union(AiFlags::SETUP_FIRST_TURN)
}

pub(super) fn pokemon(species: SpeciesId, moves: Vec<MoveId>) -> BattlePokemon {
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

pub(super) fn spend_move(pokemon: &mut BattlePokemon, slot: usize) {
    while pokemon.moves()[slot].pp > 0 {
        pokemon.deduct_pp(slot).unwrap();
    }
}

pub(super) fn rng_with_maximum_simulated_damage(
    subsequent_draws: impl IntoIterator<Item = u16>,
) -> SequenceRng {
    SequenceRng::new(
        [MAXIMUM_SIMULATED_DAMAGE_DRAW; MAX_MON_MOVES]
            .into_iter()
            .chain(subsequent_draws),
    )
}
