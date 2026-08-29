//! New-game identity, save blocks, spawn, and skipped truck-exit state.
//!
//! The flow starts directly in Brendan's bedroom with a fixed name, gender, and
//! starter. It applies the skipped truck exit's gender-specific heal location,
//! flags, and variables so the houses match a normal playthrough.
//! Berry-tree initialization remains unmodeled because the save blocks have no
//! typed berry-tree state. Other unmodeled save fields retain their defaults.

use engine::overworld::{Direction, TilePos};
use engine::rng::Rng;
use engine::save::{Coords16, PlayerGender, SaveBlock1, SaveBlock2, WarpData};
use engine::text;

use crate::overworld::PlayerCharacter;

/// Fixed player name for the direct-start flow.
pub const DEFAULT_PLAYER_NAME: &str = "STU";

/// Fixed player gender for the direct-start flow.
pub const DEFAULT_PLAYER_GENDER: PlayerGender = PlayerGender::Male;

/// Overworld character paired with [`DEFAULT_PLAYER_GENDER`].
pub const DEFAULT_PLAYER_CHARACTER: PlayerCharacter = PlayerCharacter::Brendan;

/// Money in a fresh save.
pub const STARTING_MONEY: u32 = 3000;

const TREECKO_SPECIES: assets::SpeciesId = assets::SpeciesId(277);

/// Fixed Treecko choice for the direct-start flow.
pub const PROVISIONAL_STARTER_SPECIES: assets::SpeciesId = TREECKO_SPECIES;

/// Level of the provisional starter.
pub const PROVISIONAL_STARTER_LEVEL: u8 = 5;

const PROVISIONAL_STARTER_PERSONALITY: u32 = 0;

/// Builds the direct-start flow's battle-ready lead.
///
/// Construction consumes no RNG draws because those belong to the omitted
/// handout sequence. The lead remains runtime state because the save model has
/// no typed party-Pokémon encoding.
///
/// # Panics
///
/// Panics if the generated dex lacks the provisional species or its learnset.
#[must_use]
pub fn provisional_starter() -> battle::BattlePokemon {
    let moves = battle::initial_moveset(PROVISIONAL_STARTER_SPECIES, PROVISIONAL_STARTER_LEVEL);
    battle::BattlePokemon::new(
        &battle::Dex::new(),
        PROVISIONAL_STARTER_SPECIES,
        PROVISIONAL_STARTER_LEVEL,
        battle::Ivs::default(),
        PROVISIONAL_STARTER_PERSONALITY,
        moves,
    )
    .expect("the provisional starter's species and level-up moves are in the dex")
}

/// Bedroom stair tile where the intro hands control to the overworld.
pub const SPAWN_POSITION: TilePos = (7, 1);

/// Elevation of [`SPAWN_POSITION`].
pub const SPAWN_ELEVATION: u8 = 0;

/// Direction faced after the port's direct bedroom spawn.
pub const SPAWN_FACING: Direction = Direction::South;

/// Bedroom map where the intro hands control to the overworld.
pub const SPAWN_MAP_ID: assets::MapId = assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F");

/// Map-group index of [`SPAWN_MAP_ID`] in the generated header table.
pub const SPAWN_MAP_GROUP: i8 = 1;

/// Map index of [`SPAWN_MAP_ID`] within [`SPAWN_MAP_GROUP`].
pub const SPAWN_MAP_NUM: i8 = 1;

/// Shared map group of the two bedroom heal locations.
pub const DEFAULT_HEAL_LOCATION_MAP_GROUP: i8 = SPAWN_MAP_GROUP;

/// Map index of Brendan's bedroom heal location.
pub const DEFAULT_HEAL_LOCATION_MALE_MAP_NUM: i8 = SPAWN_MAP_NUM;

/// Map index of May's bedroom heal location.
pub const DEFAULT_HEAL_LOCATION_FEMALE_MAP_NUM: i8 = 3;

/// Horizontal coordinate shared by the bedroom heal locations.
pub const DEFAULT_HEAL_LOCATION_X: i16 = 4;

/// Vertical coordinate shared by the bedroom heal locations.
pub const DEFAULT_HEAL_LOCATION_Y: i16 = 2;

const NO_WARP_EVENT: i8 = -1;

/// Returns the heal location selected by the skipped truck-exit script.
///
/// An unrecognized gender returns [`WarpData::default`] because upstream's
/// gender check has no fallback branch
/// (`pokeemerald/data/maps/InsideOfTruck/scripts.inc:19-22`).
#[must_use]
pub fn default_last_heal_location(gender: PlayerGender) -> WarpData {
    let map_num = match gender {
        PlayerGender::Male => DEFAULT_HEAL_LOCATION_MALE_MAP_NUM,
        PlayerGender::Female => DEFAULT_HEAL_LOCATION_FEMALE_MAP_NUM,
        PlayerGender::Other(_) => return WarpData::default(),
    };
    WarpData {
        map_group: DEFAULT_HEAL_LOCATION_MAP_GROUP,
        map_num,
        warp_id: NO_WARP_EVENT,
        x: DEFAULT_HEAL_LOCATION_X,
        y: DEFAULT_HEAL_LOCATION_Y,
    }
}

/// Builds fresh save blocks for every modeled new-game field.
///
/// The RNG's pre-draw state supplies the trainer ID's low half and one draw
/// supplies its high half, preserving `SeedRngAndSetTrainerId` followed by
/// `InitPlayerTrainerId` (`pokeemerald/src/main.c:208-219`,
/// `pokeemerald/src/new_game.c:84-88`).
///
/// # Panics
///
/// Panics if the static spawn coordinates or generated new-game IDs exceed the
/// save model's supported ranges.
#[must_use]
pub fn init_save_blocks(rng: &mut Rng) -> (SaveBlock1, SaveBlock2) {
    let trainer_id_low = rng.state();
    let block2 = SaveBlock2 {
        player_name: encode_default_player_name(),
        player_gender: DEFAULT_PLAYER_GENDER,
        player_trainer_id: trainer_id_bytes(trainer_id_low, rng),
        encryption_key: 0,
    };

    let spawn = WarpData {
        map_group: SPAWN_MAP_GROUP,
        map_num: SPAWN_MAP_NUM,
        warp_id: NO_WARP_EVENT,
        x: i16::try_from(SPAWN_POSITION.0).expect("SPAWN_POSITION x fits the save format"),
        y: i16::try_from(SPAWN_POSITION.1).expect("SPAWN_POSITION y fits the save format"),
    };
    let mut block1 = SaveBlock1 {
        pos: Coords16 {
            x: spawn.x,
            y: spawn.y,
        },
        location: spawn,
        money: STARTING_MONEY,
        last_heal_location: default_last_heal_location(block2.player_gender),
        ..SaveBlock1::default()
    };

    for &flag in assets::RESET_MAP_FLAGS {
        block1
            .event_data
            .flag_set(flag)
            .expect("RESET_MAP_FLAGS ids are all ordinary flag ids");
    }

    apply_truck_intro_flags(&mut block1, block2.player_gender);

    (block1, block2)
}

fn apply_truck_intro_flags(block1: &mut SaveBlock1, gender: PlayerGender) {
    let (flags, vars) = match gender {
        PlayerGender::Male => (
            assets::TRUCK_INTRO_FLAGS_MALE,
            assets::TRUCK_INTRO_VARS_MALE,
        ),
        PlayerGender::Female => (
            assets::TRUCK_INTRO_FLAGS_FEMALE,
            assets::TRUCK_INTRO_VARS_FEMALE,
        ),
        PlayerGender::Other(_) => return,
    };
    for &flag in flags {
        block1
            .event_data
            .flag_set(flag)
            .expect("truck-intro ids are all ordinary flag ids");
    }
    for &(var, value) in vars {
        block1
            .event_data
            .var_set(var, value)
            .expect("truck-intro var ids are all ordinary var ids");
    }
}

/// Deterministic seed for new-game runtime state.
pub const NEW_GAME_RNG_SEED: u32 = 0;

/// Builds fresh save blocks without exposing the continuing RNG stream.
#[must_use]
pub fn init_save_blocks_for_new_game() -> (SaveBlock1, SaveBlock2) {
    let mut rng = Rng::new(NEW_GAME_RNG_SEED);
    init_save_blocks(&mut rng)
}

fn encode_default_player_name() -> [u8; engine::save::block::PLAYER_NAME_BUF_LEN] {
    const GEN_3_END_OF_STRING: u8 = 0xFF;

    let encoded =
        text::encode_str(DEFAULT_PLAYER_NAME).expect("DEFAULT_PLAYER_NAME is Gen-3 encodable");
    let mut buf = [GEN_3_END_OF_STRING; engine::save::block::PLAYER_NAME_BUF_LEN];
    assert!(
        encoded.len() <= buf.len(),
        "DEFAULT_PLAYER_NAME must fit PLAYER_NAME_BUF_LEN"
    );
    buf[..encoded.len()].copy_from_slice(&encoded);
    buf
}

fn trainer_id_bytes(
    pre_draw_state: u32,
    rng: &mut Rng,
) -> [u8; engine::save::block::TRAINER_ID_LENGTH] {
    let high_half = rng.next_u16();
    let low_half = pre_draw_state & u32::from(u16::MAX);
    let trainer_id = (u32::from(high_half) << u16::BITS) | low_half;
    trainer_id.to_le_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIGURED_SEED_TRAINER_ID: [u8; 4] = [0x00, 0x00, 0x00, 0x00];
    const CONFIGURED_SEED_STATE_AFTER_TRAINER_ID: u32 = 0x0000_6073;
    const NONDEGENERATE_TRAINER_ID_SEED: u32 = 0x1234;
    const NONDEGENERATE_TRAINER_ID: [u8; 4] = [0x34, 0x12, 0xCB, 0x4D];
    const PETALBURG_MAP_POSITION: (i8, i8) = (0, 0);
    const UNRECOGNIZED_GENDER_BYTE: u8 = 0xFF;
    const RESET_MAP_FLAG_COUNT: usize = 159;
    const TRUCK_EXIT_BRANCH_FLAG_COUNT: usize = 5;

    #[test]
    fn default_player_name_is_stu_and_fits_the_save_buffer() {
        let encoded_name_with_terminator = text::encode_str(DEFAULT_PLAYER_NAME).unwrap();
        assert!(encoded_name_with_terminator.len() <= engine::save::block::PLAYER_NAME_BUF_LEN);
        assert_eq!(DEFAULT_PLAYER_NAME, "STU");
    }

    #[test]
    fn provisional_starter_is_a_deterministic_fightable_treecko() {
        let starter = provisional_starter();
        assert_eq!(starter.species(), PROVISIONAL_STARTER_SPECIES);
        assert_eq!(starter.species(), TREECKO_SPECIES);
        assert_eq!(starter.level(), PROVISIONAL_STARTER_LEVEL);
        assert!(!starter.is_fainted());
        assert_eq!(starter.current_hp(), starter.stats().max_hp);
        assert!(
            !starter.moves().is_empty(),
            "GiveBoxMonInitialMoveset gives a level-5 Treecko at least one move"
        );
        assert_eq!(starter.stages(), battle::StatStages::default());
        assert_eq!(starter, provisional_starter());
    }

    #[test]
    fn init_save_blocks_matches_new_game_init_data_for_modeled_fields() {
        let mut rng = Rng::new(0);
        let (block1, block2) = init_save_blocks(&mut rng);

        assert_eq!(block2.encryption_key, 0);
        assert_eq!(block2.player_gender, PlayerGender::Male);
        assert_eq!(text::decode_to_string(&block2.player_name).unwrap(), "STU");

        assert_eq!(block1.money, STARTING_MONEY);
        assert_eq!(block1.player_party_count, 0);
        assert_eq!(block1.player_party, SaveBlock1::default().player_party);
        assert_eq!(block1.bag, engine::save::Bag::default());

        assert_eq!(block1.pos.x, 7);
        assert_eq!(block1.pos.y, 1);
        assert_eq!(block1.location.warp_id, NO_WARP_EVENT);
        assert_eq!(block1.location.map_group, SPAWN_MAP_GROUP);
        assert_eq!(block1.location.map_num, SPAWN_MAP_NUM);
    }

    #[test]
    fn trainer_id_uses_the_configured_seed_and_one_rng_draw() {
        let mut rng = Rng::new(NEW_GAME_RNG_SEED);
        let (_, block2) = init_save_blocks(&mut rng);
        assert_eq!(block2.player_trainer_id, CONFIGURED_SEED_TRAINER_ID);
        assert_eq!(rng.state(), CONFIGURED_SEED_STATE_AFTER_TRAINER_ID);
    }

    #[test]
    fn trainer_id_halves_are_not_transposed_for_a_nondegenerate_seed() {
        let mut rng = Rng::new(NONDEGENERATE_TRAINER_ID_SEED);
        let (_, block2) = init_save_blocks(&mut rng);
        assert_eq!(block2.player_trainer_id, NONDEGENERATE_TRAINER_ID);
    }

    #[test]
    fn spawn_location_matches_the_bedrooms_map_header_position() {
        let header = assets::MapHeaderTable::new()
            .header(SPAWN_MAP_ID)
            .expect("SPAWN_MAP_ID must resolve in the generated map-header table");
        assert_eq!(i8::try_from(header.group).unwrap(), SPAWN_MAP_GROUP);
        assert_eq!(i8::try_from(header.num).unwrap(), SPAWN_MAP_NUM);

        let mut rng = Rng::new(0);
        let (block1, _) = init_save_blocks(&mut rng);
        assert_eq!(block1.location.map_group, SPAWN_MAP_GROUP);
        assert_eq!(block1.location.map_num, SPAWN_MAP_NUM);

        assert_ne!(
            (block1.location.map_group, block1.location.map_num),
            PETALBURG_MAP_POSITION
        );
    }

    #[test]
    fn init_save_blocks_for_new_game_uses_the_fixed_seed() {
        let mut rng = Rng::new(NEW_GAME_RNG_SEED);
        let expected = init_save_blocks(&mut rng);
        let actual = init_save_blocks_for_new_game();
        assert_eq!(actual.0.money, expected.0.money);
        assert_eq!(actual.1.player_trainer_id, expected.1.player_trainer_id);
    }

    #[test]
    fn trainer_id_is_deterministic_from_seed_and_one_rng_draw() {
        let mut rng_a = Rng::new(42);
        let mut rng_b = Rng::new(42);
        let (_, block2_a) = init_save_blocks(&mut rng_a);
        let (_, block2_b) = init_save_blocks(&mut rng_b);
        assert_eq!(block2_a.player_trainer_id, block2_b.player_trainer_id);

        let mut rng_c = Rng::new(43);
        let (_, block2_c) = init_save_blocks(&mut rng_c);
        assert_ne!(block2_a.player_trainer_id, block2_c.player_trainer_id);
    }

    #[test]
    fn spawn_constants_match_the_bedroom_warp_chain() {
        assert_eq!(SPAWN_POSITION, (7, 1));
        assert_eq!(SPAWN_ELEVATION, 0);
        assert_eq!(SPAWN_FACING, Direction::South);
        assert_eq!(SPAWN_MAP_ID.0, "MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F");
    }

    #[test]
    fn default_heal_locations_match_the_generated_map_header_positions() {
        let brendans = assets::MapHeaderTable::new()
            .header(assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F"))
            .expect("must resolve in the generated map-header table");
        assert_eq!(
            i8::try_from(brendans.group).unwrap(),
            DEFAULT_HEAL_LOCATION_MAP_GROUP
        );
        assert_eq!(
            i8::try_from(brendans.num).unwrap(),
            DEFAULT_HEAL_LOCATION_MALE_MAP_NUM
        );

        let mays = assets::MapHeaderTable::new()
            .header(assets::MapId("MAP_LITTLEROOT_TOWN_MAYS_HOUSE_2F"))
            .expect("must resolve in the generated map-header table");
        assert_eq!(
            i8::try_from(mays.group).unwrap(),
            DEFAULT_HEAL_LOCATION_MAP_GROUP
        );
        assert_eq!(
            i8::try_from(mays.num).unwrap(),
            DEFAULT_HEAL_LOCATION_FEMALE_MAP_NUM
        );
    }

    #[test]
    fn default_last_heal_location_matches_the_gender_pairing() {
        assert_eq!(
            default_last_heal_location(PlayerGender::Male),
            WarpData {
                map_group: DEFAULT_HEAL_LOCATION_MAP_GROUP,
                map_num: DEFAULT_HEAL_LOCATION_MALE_MAP_NUM,
                warp_id: NO_WARP_EVENT,
                x: DEFAULT_HEAL_LOCATION_X,
                y: DEFAULT_HEAL_LOCATION_Y,
            }
        );
        assert_eq!(
            default_last_heal_location(PlayerGender::Female),
            WarpData {
                map_group: DEFAULT_HEAL_LOCATION_MAP_GROUP,
                map_num: DEFAULT_HEAL_LOCATION_FEMALE_MAP_NUM,
                warp_id: NO_WARP_EVENT,
                x: DEFAULT_HEAL_LOCATION_X,
                y: DEFAULT_HEAL_LOCATION_Y,
            }
        );
        assert_eq!(
            default_last_heal_location(PlayerGender::Other(UNRECOGNIZED_GENDER_BYTE)),
            WarpData::default(),
            "upstream's own no-op: checkplayergender falls through to a bare `end`"
        );
    }

    #[test]
    fn fresh_save_has_a_real_last_heal_location() {
        let mut rng = Rng::new(0);
        let (block1, block2) = init_save_blocks(&mut rng);
        assert_eq!(block2.player_gender, PlayerGender::Male);
        assert_eq!(
            block1.last_heal_location,
            default_last_heal_location(PlayerGender::Male)
        );
        assert_ne!(
            (
                block1.last_heal_location.map_group,
                block1.last_heal_location.map_num
            ),
            PETALBURG_MAP_POSITION,
            "must not be Petalburg City's own position"
        );
    }

    #[test]
    fn fresh_save_hides_the_rival_bedroom_object_event() {
        let mut rng = Rng::new(0);
        let (block1, _) = init_save_blocks(&mut rng);
        assert!(block1
            .event_data
            .flag_get(assets::FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_RIVAL_BEDROOM)
            .unwrap());
        let events = assets::MapEventsTable::new()
            .resolve(SPAWN_MAP_ID)
            .expect("spawn map has generated events");
        assert!(
            events
                .object_events
                .iter()
                .any(|obj| obj.flag == "FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_RIVAL_BEDROOM"),
            "the bedroom must have an object event hidden by the reset-map flag"
        );
    }

    #[test]
    fn fresh_save_applies_every_reset_map_flag() {
        let mut rng = Rng::new(0);
        let (block1, _) = init_save_blocks(&mut rng);
        for &flag in assets::RESET_MAP_FLAGS {
            assert!(
                block1.event_data.flag_get(flag).unwrap(),
                "flag {flag:#X} from RESET_MAP_FLAGS must be set on a fresh save"
            );
        }
        let set_bits: usize = block1
            .event_data
            .flag_bytes()
            .iter()
            .map(|b| b.count_ones() as usize)
            .sum();
        assert_eq!(
            set_bits,
            assets::RESET_MAP_FLAGS.len() + assets::TRUCK_INTRO_FLAGS_MALE.len(),
            "a fresh save sets exactly the reset script's flags plus the \
             male truck-intro branch's"
        );
        assert_eq!(
            set_bits,
            RESET_MAP_FLAG_COUNT + TRUCK_EXIT_BRANCH_FLAG_COUNT
        );
        assert!(assets::RESET_MAP_FLAGS
            .iter()
            .all(|&f| f < engine::event_data::FLAGS_COUNT));
    }

    #[test]
    fn truck_intro_ids_are_all_settable() {
        let mut data = engine::event_data::EventData::new();
        for ids in [
            assets::TRUCK_INTRO_FLAGS_MALE,
            assets::TRUCK_INTRO_FLAGS_FEMALE,
        ] {
            for &flag in ids {
                assert!(data.flag_set(flag).is_ok(), "{flag:#X} must be settable");
            }
        }
        for vars in [
            assets::TRUCK_INTRO_VARS_MALE,
            assets::TRUCK_INTRO_VARS_FEMALE,
        ] {
            for &(var, value) in vars {
                assert!(
                    data.var_set(var, value).is_ok(),
                    "{var:#X} must be settable"
                );
            }
        }
    }

    #[test]
    fn fresh_save_applies_only_its_gender_truck_exit_branch() {
        for (gender, mine, theirs, vars) in [
            (
                PlayerGender::Male,
                assets::TRUCK_INTRO_FLAGS_MALE,
                assets::TRUCK_INTRO_FLAGS_FEMALE,
                assets::TRUCK_INTRO_VARS_MALE,
            ),
            (
                PlayerGender::Female,
                assets::TRUCK_INTRO_FLAGS_FEMALE,
                assets::TRUCK_INTRO_FLAGS_MALE,
                assets::TRUCK_INTRO_VARS_FEMALE,
            ),
        ] {
            let mut block1 = SaveBlock1::default();
            apply_truck_intro_flags(&mut block1, gender);

            for &flag in mine {
                assert!(
                    block1.event_data.flag_get(flag).unwrap(),
                    "{gender:?}: {flag:#X} must be set"
                );
            }
            for &flag in theirs {
                assert!(
                    !block1.event_data.flag_get(flag).unwrap(),
                    "{gender:?}: {flag:#X} belongs to the other gender's branch"
                );
            }
            for &(var, value) in vars {
                assert_eq!(
                    block1.event_data.var_get(var).unwrap(),
                    value,
                    "{gender:?}: {var:#X}"
                );
            }
        }
    }

    #[test]
    fn unrecognized_gender_applies_no_truck_exit_branch() {
        let mut block1 = SaveBlock1::default();
        apply_truck_intro_flags(&mut block1, PlayerGender::Other(UNRECOGNIZED_GENDER_BYTE));
        let set_bits: u32 = block1
            .event_data
            .flag_bytes()
            .iter()
            .map(|b| b.count_ones())
            .sum();
        assert_eq!(set_bits, 0, "upstream's script has no else branch");
    }

    #[test]
    fn a_fresh_save_puts_each_familys_object_events_in_the_right_house() {
        struct Houses {
            own_2f: &'static str,
            own_1f: &'static str,
            rivals_1f: &'static str,
        }
        let brendans = Houses {
            own_2f: "MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F",
            own_1f: "MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F",
            rivals_1f: "MAP_LITTLEROOT_TOWN_MAYS_HOUSE_1F",
        };
        let mays = Houses {
            own_2f: "MAP_LITTLEROOT_TOWN_MAYS_HOUSE_2F",
            own_1f: "MAP_LITTLEROOT_TOWN_MAYS_HOUSE_1F",
            rivals_1f: "MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F",
        };

        let table = assets::MapEventsTable::new();
        let find_object = |map: &'static str, graphics_id: &str| -> &'static assets::ObjectEvent {
            table
                .resolve(assets::MapId(map))
                .unwrap_or_else(|_| panic!("{map} must resolve"))
                .object_events
                .iter()
                .find(|object| object.graphics_id == graphics_id)
                .unwrap_or_else(|| panic!("{map} must declare a {graphics_id}"))
        };

        for (gender, houses) in [
            (PlayerGender::Male, &brendans),
            (PlayerGender::Female, &mays),
        ] {
            let mut block1 = SaveBlock1::default();
            for &flag in assets::RESET_MAP_FLAGS {
                block1.event_data.flag_set(flag).unwrap();
            }
            apply_truck_intro_flags(&mut block1, gender);
            let data = &block1.event_data;
            let is_visible = |object| engine::overworld::object_event_is_visible(object, data);

            let rivals_poke_ball = find_object(houses.own_2f, "OBJ_EVENT_GFX_ITEM_BALL");
            let players_mother = find_object(houses.own_1f, "OBJ_EVENT_GFX_MOM");
            let rivals_mother = find_object(houses.own_1f, "OBJ_EVENT_GFX_WOMAN_4");
            let rivals_sibling = find_object(houses.own_1f, "OBJ_EVENT_GFX_NINJA_BOY");
            let neighbor_mother = find_object(houses.rivals_1f, "OBJ_EVENT_GFX_WOMAN_4");
            let players_mother_next_door = find_object(houses.rivals_1f, "OBJ_EVENT_GFX_MOM");

            assert!(
                !is_visible(rivals_poke_ball),
                "{gender:?}: the rival's Poké Ball must not be spawned in \
                 the player's own bedroom ({})",
                houses.own_2f
            );

            assert!(
                is_visible(players_mother),
                "{gender:?}: the player's own mother must be home",
            );
            assert!(
                !is_visible(rivals_mother),
                "{gender:?}: the rival's mother must not be duplicated into \
                 the player's own house ({})",
                houses.own_1f
            );
            assert!(
                !is_visible(rivals_sibling),
                "{gender:?}: the rival's sibling must not be in the \
                 player's own house ({})",
                houses.own_1f
            );

            assert!(
                is_visible(neighbor_mother),
                "{gender:?}: the rival's mother belongs in the rival's house ({})",
                houses.rivals_1f
            );
            assert!(
                !is_visible(players_mother_next_door),
                "{gender:?}: the player's own mother must not also be in the \
                 rival's house ({})",
                houses.rivals_1f
            );
        }
    }

    #[test]
    fn the_truck_intro_vars_stay_below_moms_script_guards() {
        const LITTLEROOT_INTRO_STATE: u16 = 0x4092;
        const LITTLEROOT_HOUSES_STATE_MAY: u16 = 0x4082;
        const LITTLEROOT_HOUSES_STATE_BRENDAN: u16 = 0x408C;
        const MOM_DID_YOU_MEET_BIRCH_STATE: u16 = 7;
        const MOM_DONT_PUSH_YOURSELF_STATE: u16 = 4;
        for vars in [
            assets::TRUCK_INTRO_VARS_MALE,
            assets::TRUCK_INTRO_VARS_FEMALE,
        ] {
            for &(var, value) in vars {
                match var {
                    LITTLEROOT_INTRO_STATE => {
                        assert_ne!(value, MOM_DID_YOU_MEET_BIRCH_STATE);
                    }
                    LITTLEROOT_HOUSES_STATE_MAY | LITTLEROOT_HOUSES_STATE_BRENDAN => {
                        assert_ne!(value, MOM_DONT_PUSH_YOURSELF_STATE);
                    }
                    other => panic!("unexpected truck-intro var {other:#X}"),
                }
            }
        }
    }
}
