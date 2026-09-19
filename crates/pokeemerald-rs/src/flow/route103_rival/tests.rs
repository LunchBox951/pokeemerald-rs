use assets::trainers::{TrainerClass, TrainerId, TrainerParty, TrainerTable};
use assets::{MoveId, SpeciesId};
use battle::{trainer_data, trainer_money, BattleOutcome, BattlePokemon, Dex, Ivs};
use engine::rng::Rng;

use super::{route103_rival_for, PlayerStarter, Rival};
use crate::flow::npc_trainer_battle::{
    advance_npc_trainer_battle, start_npc_trainer_battle, trainer_party_personalities,
    NpcTrainerBattleError,
};

const POUND: MoveId = MoveId(1);
const TACKLE: MoveId = MoveId(33);
const LEER: MoveId = MoveId(43);
const SLASH: MoveId = MoveId(163);
const FEMALE_TRAINER_PERSONALITY_BASE: u32 = 0x78;
const MALE_TRAINER_PERSONALITY_BASE: u32 = 0x88;
const NAME_HASH_PERSONALITY_SHIFT: u32 = 8;
const PLAYER_PERSONALITY: u32 = 0;
const RIVAL_LEVEL: u8 = 5;
const RIVAL_UNSCALED_IV: u8 = 0;
const ROUTE_103_RIVAL_REWARD: u32 = 300;
const DOMINANT_PLAYER_LEVEL: u8 = 50;
const MAX_BATTLE_TURNS: usize = 32;
const HEADLESS_MOVE_SLOT: usize = 0;
const INVALID_STARTER_VALUE: u16 = 3;

#[derive(Clone, Copy)]
struct RivalMatchup {
    rival: Rival,
    player_starter: PlayerStarter,
    player_species: SpeciesId,
    rival_species: SpeciesId,
}

const RIVAL_MATCHUPS: [RivalMatchup; 6] = [
    RivalMatchup {
        rival: Rival::Brendan,
        player_starter: PlayerStarter::Mudkip,
        player_species: SpeciesId::MUDKIP,
        rival_species: SpeciesId::TREECKO,
    },
    RivalMatchup {
        rival: Rival::Brendan,
        player_starter: PlayerStarter::Treecko,
        player_species: SpeciesId::TREECKO,
        rival_species: SpeciesId::TORCHIC,
    },
    RivalMatchup {
        rival: Rival::Brendan,
        player_starter: PlayerStarter::Torchic,
        player_species: SpeciesId::TORCHIC,
        rival_species: SpeciesId::MUDKIP,
    },
    RivalMatchup {
        rival: Rival::May,
        player_starter: PlayerStarter::Mudkip,
        player_species: SpeciesId::MUDKIP,
        rival_species: SpeciesId::TREECKO,
    },
    RivalMatchup {
        rival: Rival::May,
        player_starter: PlayerStarter::Treecko,
        player_species: SpeciesId::TREECKO,
        rival_species: SpeciesId::TORCHIC,
    },
    RivalMatchup {
        rival: Rival::May,
        player_starter: PlayerStarter::Torchic,
        player_species: SpeciesId::TORCHIC,
        rival_species: SpeciesId::MUDKIP,
    },
];

const STARTER_VAR_ENCODINGS: [(u16, PlayerStarter); 3] = [
    (0, PlayerStarter::Treecko),
    (1, PlayerStarter::Torchic),
    (2, PlayerStarter::Mudkip),
];

fn player_mon(species: SpeciesId, level: u8, moves: Vec<MoveId>) -> BattlePokemon {
    let max_ivs = Ivs {
        hp: battle::MAX_IV,
        attack: battle::MAX_IV,
        defense: battle::MAX_IV,
        speed: battle::MAX_IV,
        sp_attack: battle::MAX_IV,
        sp_defense: battle::MAX_IV,
    };
    BattlePokemon::new(
        &Dex::new(),
        species,
        level,
        max_ivs,
        PLAYER_PERSONALITY,
        moves,
    )
    .expect("player mon must use known species and moves")
}

fn encoded_name_sum(name: &str) -> u32 {
    name.chars()
        .map(|character| {
            u32::from(engine::text::char_to_byte(character).expect("name must use known glyphs"))
        })
        .sum()
}

#[test]
fn every_route_103_rival_fields_the_type_advantaged_level_5_starter() {
    let table = TrainerTable::new();
    for matchup in RIVAL_MATCHUPS {
        let id = route103_rival_for(matchup.rival, matchup.player_starter);
        let data = table.trainer(id).expect("Route 103 rival must exist");
        let TrainerParty::NoItemDefaultMoves(party) = data.party else {
            panic!("{id:?} must use a no-item, level-derived party");
        };
        assert_eq!(party.len(), 1, "{id:?} must field exactly one mon");
        assert_eq!(party[0].species, matchup.rival_species, "{id:?}");
        assert_eq!(party[0].lvl, RIVAL_LEVEL, "{id:?}");
        assert_eq!(party[0].iv, RIVAL_UNSCALED_IV, "{id:?}");
        assert_eq!(data.class, TrainerClass::RIVAL, "{id:?}");
    }
}

#[test]
fn the_seeded_personality_matches_the_hand_computed_upstream_formula() {
    let may_id = route103_rival_for(Rival::May, PlayerStarter::Mudkip);
    let may_hash = encoded_name_sum("MAYTREECKO");
    let expected_may_personality = FEMALE_TRAINER_PERSONALITY_BASE
        .wrapping_add(may_hash.wrapping_shl(NAME_HASH_PERSONALITY_SHIFT));
    assert_eq!(
        trainer_party_personalities(may_id).unwrap(),
        vec![expected_may_personality]
    );

    let brendan_id = route103_rival_for(Rival::Brendan, PlayerStarter::Treecko);
    let brendan_hash = encoded_name_sum("BRENDANTORCHIC");
    let expected_brendan_personality = MALE_TRAINER_PERSONALITY_BASE
        .wrapping_add(brendan_hash.wrapping_shl(NAME_HASH_PERSONALITY_SHIFT));
    assert_eq!(
        trainer_party_personalities(brendan_id).unwrap(),
        vec![expected_brendan_personality]
    );
}

#[test]
fn the_name_hash_uses_charmap_bytes_and_not_ascii() {
    let id = route103_rival_for(Rival::May, PlayerStarter::Mudkip);
    let ascii_hash: u32 = "MAYTREECKO".chars().map(u32::from).sum();
    let charmap_hash = encoded_name_sum("MAYTREECKO");
    let personality_from_charmap = FEMALE_TRAINER_PERSONALITY_BASE
        .wrapping_add(charmap_hash.wrapping_shl(NAME_HASH_PERSONALITY_SHIFT));
    let personality_from_ascii = FEMALE_TRAINER_PERSONALITY_BASE
        .wrapping_add(ascii_hash.wrapping_shl(NAME_HASH_PERSONALITY_SHIFT));

    assert_ne!(ascii_hash, charmap_hash);
    assert_eq!(
        trainer_party_personalities(id).unwrap(),
        vec![personality_from_charmap]
    );
    assert_ne!(
        trainer_party_personalities(id).unwrap(),
        vec![personality_from_ascii]
    );
}

#[test]
fn the_name_hash_accumulates_across_a_multi_mon_party() {
    let table = TrainerTable::new();
    let id = (1..assets::trainers::TRAINERS_COUNT)
        .map(|index| TrainerId(u16::try_from(index).unwrap()))
        .find(|id| {
            matches!(
                table.trainer(*id).unwrap().party,
                TrainerParty::NoItemDefaultMoves(party) if party.len() == 2
            )
        })
        .expect("trainer table must contain a two-mon default-moves party");
    let data = table.trainer(id).unwrap();
    let TrainerParty::NoItemDefaultMoves(party) = data.party else {
        unreachable!()
    };
    let species_names = assets::SpeciesNames::new();
    let personality_base = if data.encounter_music.is_female {
        FEMALE_TRAINER_PERSONALITY_BASE
    } else {
        MALE_TRAINER_PERSONALITY_BASE
    };
    let first_hash = encoded_name_sum(data.name)
        + encoded_name_sum(species_names.name(party[0].species).unwrap());
    let second_hash = first_hash
        + encoded_name_sum(data.name)
        + encoded_name_sum(species_names.name(party[1].species).unwrap());
    let expected_personalities = vec![
        personality_base.wrapping_add(first_hash.wrapping_shl(NAME_HASH_PERSONALITY_SHIFT)),
        personality_base.wrapping_add(second_hash.wrapping_shl(NAME_HASH_PERSONALITY_SHIFT)),
    ];

    assert_eq!(
        trainer_party_personalities(id).unwrap(),
        expected_personalities
    );
}

#[test]
fn every_trainer_and_species_name_has_a_charmap_encoding() {
    for data in TrainerTable::new().iter() {
        for character in data.name.chars() {
            assert!(
                engine::text::char_to_byte(character).is_some(),
                "trainer name `{}` has an unencodable `{character}`",
                data.name
            );
        }
    }
    let names = assets::SpeciesNames::new();
    for index in 0..names.len() {
        let name = names
            .name(SpeciesId(u16::try_from(index).unwrap()))
            .unwrap();
        for character in name.chars() {
            assert!(
                engine::text::char_to_byte(character).is_some(),
                "species name `{name}` has an unencodable `{character}`"
            );
        }
    }
}

#[test]
fn a_held_item_party_is_refused_rather_than_stripped() {
    let table = TrainerTable::new();
    let id = (1..assets::trainers::TRAINERS_COUNT)
        .map(|index| TrainerId(u16::try_from(index).unwrap()))
        .find(|id| {
            matches!(
                table.trainer(*id).unwrap().party,
                TrainerParty::ItemDefaultMoves(_) | TrainerParty::ItemCustomMoves(_)
            )
        })
        .expect("trainer table must contain a held-item party");
    assert_eq!(
        trainer_party_personalities(id).unwrap_err(),
        NpcTrainerBattleError::HeldItemParty(id)
    );
}

#[test]
fn starting_the_rival_battle_builds_the_seeded_level_5_treecko() {
    let id = route103_rival_for(Rival::May, PlayerStarter::Mudkip);
    let mut rng = Rng::new(1);
    let lead = player_mon(SpeciesId::MUDKIP, RIVAL_LEVEL, vec![TACKLE]);
    let battle = start_npc_trainer_battle(lead, id, &mut rng).expect("battle must start");

    assert_eq!(battle.enemy().species(), SpeciesId::TREECKO);
    assert_eq!(battle.enemy().level(), RIVAL_LEVEL);
    assert_eq!(battle.enemy().ivs().as_array(), [0; 6]);
    assert_eq!(
        battle.enemy().personality(),
        trainer_party_personalities(id).unwrap()[0]
    );
    let known_moves: Vec<MoveId> = battle
        .enemy()
        .moves()
        .iter()
        .map(|move_slot| move_slot.move_id)
        .collect();
    assert_eq!(known_moves, vec![POUND, LEER]);
    let trainer = battle.trainer().expect("trainer context must be present");
    assert_eq!(trainer.id(), id);
    assert_eq!(trainer.money(), ROUTE_103_RIVAL_REWARD);
}

#[test]
fn construction_draws_only_the_ot_id_then_the_turn_number() {
    const SEED: u32 = 7;

    let id = route103_rival_for(Rival::May, PlayerStarter::Mudkip);
    let mut rng = Rng::new(SEED);
    let lead = player_mon(SpeciesId::MUDKIP, RIVAL_LEVEL, vec![TACKLE]);
    let battle = start_npc_trainer_battle(lead, id, &mut rng).expect("battle must start");

    let mut reference_rng = Rng::new(SEED);
    let original_trainer_id = reference_rng.next_u32();
    assert!(
        battle::shiny_value(original_trainer_id, battle.enemy().personality())
            >= battle::SHINY_ODDS,
        "the selected seed must not require an OT-ID redraw"
    );
    assert_eq!(battle.enemy().original_trainer_id(), original_trainer_id);
    let _initial_turn_number = reference_rng.next_u16();
    assert_ne!(
        battle.player().effective_speed(),
        battle.enemy().effective_speed(),
        "the selected matchup must not require a speed-tie draw"
    );
    assert_eq!(rng.state(), reference_rng.state());
}

#[test]
fn all_six_rivals_construct_and_play_to_a_terminal_outcome() {
    for matchup in RIVAL_MATCHUPS {
        let id = route103_rival_for(matchup.rival, matchup.player_starter);
        let mut rng = Rng::new(11);
        let lead = player_mon(matchup.player_species, DOMINANT_PLAYER_LEVEL, vec![SLASH]);
        let mut slot = Some(
            start_npc_trainer_battle(lead, id, &mut rng)
                .unwrap_or_else(|error| panic!("{id:?} must construct: {error}")),
        );
        let mut written_back = None;
        let mut money = 0;
        let mut outcome = None;
        for _turn in 0..MAX_BATTLE_TURNS {
            outcome =
                advance_npc_trainer_battle(&mut slot, &mut written_back, &mut money, &mut rng);
            if outcome.is_some() {
                break;
            }
        }
        assert_eq!(
            outcome,
            Some(BattleOutcome::PlayerWon),
            "{id:?} must reach a terminal outcome"
        );
        assert!(slot.is_none(), "a finished battle must empty its slot");
        assert!(
            written_back.is_some(),
            "the player's mon must be written back"
        );
        assert_eq!(
            money,
            trainer_money(trainer_data(id).unwrap()),
            "{id:?} must credit its trainer reward"
        );
    }
}

#[test]
fn the_driver_never_attempts_to_run() {
    let id = route103_rival_for(Rival::May, PlayerStarter::Mudkip);
    let mut rng = Rng::new(3);
    let lead = player_mon(SpeciesId::MUDKIP, RIVAL_LEVEL, vec![TACKLE]);
    let mut slot = Some(start_npc_trainer_battle(lead, id, &mut rng).unwrap());
    let enemy_hp_before = slot.as_ref().unwrap().enemy().current_hp();

    let mut written_back = None;
    let mut money = 0;
    let outcome = advance_npc_trainer_battle(&mut slot, &mut written_back, &mut money, &mut rng);
    assert_eq!(outcome, None);
    assert!(slot.is_some());
    let battle = slot.as_ref().unwrap();
    assert!(
        battle.enemy().current_hp() < enemy_hp_before
            || battle.player().current_hp() < battle.player().stats().max_hp,
        "the turn must deal damage"
    );
    assert_eq!(battle.turn_counter(), 0);
}

#[test]
fn a_lead_with_no_pp_in_slot_zero_ends_the_battle_and_is_still_written_back() {
    let id = route103_rival_for(Rival::May, PlayerStarter::Mudkip);
    let mut rng = Rng::new(5);
    let mut lead = player_mon(SpeciesId::MUDKIP, RIVAL_LEVEL, vec![TACKLE]);
    for _remaining_pp in 0..lead.moves()[HEADLESS_MOVE_SLOT].pp {
        lead.deduct_pp(HEADLESS_MOVE_SLOT).unwrap();
    }
    let mut slot = Some(start_npc_trainer_battle(lead, id, &mut rng).unwrap());
    let mut written_back = None;
    let mut money = 0;

    let outcome = advance_npc_trainer_battle(&mut slot, &mut written_back, &mut money, &mut rng);
    assert_eq!(outcome, None);
    assert!(slot.is_none());
    let mon = written_back.expect("the player's mon must be written back");
    assert_eq!(mon.moves()[HEADLESS_MOVE_SLOT].pp, 0);
}

#[test]
fn player_starter_from_species_covers_exactly_the_three_starters() {
    assert_eq!(
        PlayerStarter::from_species(SpeciesId::TREECKO),
        Some(PlayerStarter::Treecko)
    );
    assert_eq!(
        PlayerStarter::from_species(SpeciesId::TORCHIC),
        Some(PlayerStarter::Torchic)
    );
    assert_eq!(
        PlayerStarter::from_species(SpeciesId::MUDKIP),
        Some(PlayerStarter::Mudkip)
    );
    assert_eq!(PlayerStarter::from_species(SpeciesId::ZIGZAGOON), None);
}

#[test]
fn player_starter_var_value_and_from_var_round_trip_the_real_encoding() {
    for (value, starter) in STARTER_VAR_ENCODINGS {
        assert_eq!(PlayerStarter::from_var(value), Some(starter));
        assert_eq!(starter.var_value(), value);
    }
    assert_eq!(PlayerStarter::from_var(INVALID_STARTER_VALUE), None);
}

#[test]
fn rival_for_gender_is_always_the_opposite_protagonist() {
    use engine::save::PlayerGender;

    assert_eq!(Rival::for_gender(PlayerGender::Male), Some(Rival::May));
    assert_eq!(
        Rival::for_gender(PlayerGender::Female),
        Some(Rival::Brendan)
    );
    assert_eq!(Rival::for_gender(PlayerGender::Other(u8::MAX)), None);
}

#[test]
fn advancing_an_empty_slot_does_nothing() {
    let mut rng = Rng::new(1);
    let mut slot = None;
    let mut lead = None;
    let mut money = crate::new_game::STARTING_MONEY;
    assert_eq!(
        advance_npc_trainer_battle(&mut slot, &mut lead, &mut money, &mut rng),
        None
    );
    assert!(lead.is_none());
    assert_eq!(money, crate::new_game::STARTING_MONEY);
}
