use super::{grant_starter, GrantError, Starter, STARTER_LEVEL};
use crate::party::{from_save_pokemon, LoadedLead};
use battle::{BattleRng, Dex};
use engine::save::{PlayerGender, Pokemon, SaveBlock1, SaveBlock2, SaveStatus, SaveStore};

const ROUTE_101: u8 = 16;
const OT_NAME: [u8; 8] = [0xBB, 0xC2, 0xC7, 0xC8, 0xC9, 0xCA, 0xCB, 0xFF];
const TRAINER_ID: [u8; 4] = [0x39, 0x30, 0x0A, 0x1B];

/// Plays back fixed 16-bit draws and records how many were taken.
struct Scripted {
    draws: Vec<u16>,
    taken: usize,
}

impl BattleRng for Scripted {
    fn next_u16(&mut self) -> u16 {
        let draw = self.draws[self.taken];
        self.taken += 1;
        draw
    }
}

// Personality 0xBEEF_1234 (low first), then asymmetric IV words whose top bit
// (ignored by upstream's 15-bit unpack) is set.
const DRAWS: [u16; 5] = [
    0x1234,
    0xBEEF,
    0x8000 | 3 | (7 << 5) | (11 << 10),
    0x8000 | 9 | (13 << 5) | (30 << 10),
    0xDEAD,
];

fn identity(gender: PlayerGender) -> SaveBlock2 {
    SaveBlock2 {
        player_name: OT_NAME,
        player_gender: gender,
        player_trainer_id: TRAINER_ID,
        encryption_key: 0x5A5A_1234,
        ..SaveBlock2::default()
    }
}

fn grant(starter: Starter, gender: PlayerGender) -> (LoadedLead, SaveBlock1, Scripted) {
    let mut rng = Scripted {
        draws: DRAWS.to_vec(),
        taken: 0,
    };
    let mut block = SaveBlock1::default();
    let lead = grant_starter(
        &Dex::new(),
        &mut rng,
        starter,
        ROUTE_101,
        &identity(gender),
        &mut block,
    )
    .expect("fresh party accepts the starter");
    (lead, block, rng)
}

fn misc(record: &Pokemon) -> [u8; 12] {
    record.box_data.substructures().unwrap().misc
}

#[test]
fn grant_matches_create_mon_for_every_starter_and_gender() {
    let expected_moves = [
        (Starter::Treecko, [(1, 35), (43, 30)]),
        (Starter::Torchic, [(10, 35), (45, 40)]),
        (Starter::Mudkip, [(33, 35), (45, 40)]),
    ];
    for (starter, moves) in expected_moves {
        for (gender, bit) in [(PlayerGender::Male, 0u16), (PlayerGender::Female, 1)] {
            let (lead, block, rng) = grant(starter, gender);
            let record = lead.record();
            let boxed = &record.box_data;

            assert_eq!(rng.taken, 4, "four draws; the fifth stays unspent");
            assert_eq!(boxed.personality(), 0xBEEF_1234);
            assert_eq!(boxed.ot_id(), 0x1B0A_3039);
            assert_eq!(boxed.to_bytes()[20..27], OT_NAME[..7]);
            assert_eq!(record.level, STARTER_LEVEL);

            let ivs = lead.battler().ivs();
            assert_eq!(
                [
                    ivs.hp,
                    ivs.attack,
                    ivs.defense,
                    ivs.speed,
                    ivs.sp_attack,
                    ivs.sp_defense
                ],
                [3, 7, 11, 9, 13, 30]
            );

            let sub = boxed.substructures().unwrap();
            assert_eq!(sub.growth[0..2], starter.species().0.to_le_bytes());
            assert_eq!(sub.growth[4..8], 135u32.to_le_bytes());
            assert_eq!(sub.growth[9], 70, "base friendship");
            let origins = u16::from_le_bytes([sub.misc[2], sub.misc[3]]);
            assert_eq!(origins, 0x2185 | (bit << 15));
            assert_eq!(sub.misc[1], ROUTE_101);
            assert_eq!(sub.misc[4..8], {
                let word = 3u32 | (7 << 5) | (11 << 10) | (9 << 15) | (13 << 20) | (30 << 25);
                word.to_le_bytes()
            });
            let ids = [
                u16::from_le_bytes([sub.attacks[0], sub.attacks[1]]),
                u16::from_le_bytes([sub.attacks[2], sub.attacks[3]]),
                u16::from_le_bytes([sub.attacks[4], sub.attacks[5]]),
            ];
            assert_eq!(ids[0], moves[0].0);
            assert_eq!(ids[1], moves[1].0);
            assert_eq!(ids[2], 0);
            assert_eq!(sub.attacks[8], moves[0].1);
            assert_eq!(sub.attacks[9], moves[1].1);
            assert_eq!(sub.attacks[10..12], [0, 0]);
            assert_eq!(sub.evs_and_condition, [0; 12]);
            assert_eq!(record.hp, record.max_hp);
            assert_eq!(record.mail, u8::MAX);

            assert_eq!(block.player_party_count, 1);
            assert_eq!(block.player_party[0], *record);
            assert_eq!(
                block.player_party[1..],
                SaveBlock1::default().player_party[1..]
            );
        }
    }
}

#[test]
fn runtime_member_equals_the_member_reloaded_from_its_record() {
    for starter in Starter::ALL {
        let (lead, _, _) = grant(starter, PlayerGender::Female);
        let reloaded = from_save_pokemon(&Dex::new(), lead.record()).unwrap();
        assert_eq!(&reloaded, lead.battler());
        assert_eq!(lead.hidden_hp_offset(), 0);
    }
}

#[test]
fn draws_follow_create_mon_order_on_the_real_generator() {
    struct Real(engine::rng::Rng);
    impl BattleRng for Real {
        fn next_u16(&mut self) -> u16 {
            self.0.next_u16()
        }
    }
    let mut real = Real(engine::rng::Rng::new(0x1357));
    let mut direct = engine::rng::Rng::new(0x1357);
    let low = direct.next_u16();
    let high = direct.next_u16();
    let _ivs = (direct.next_u16(), direct.next_u16());

    let mut block = SaveBlock1::default();
    let lead = grant_starter(
        &Dex::new(),
        &mut real,
        Starter::Mudkip,
        ROUTE_101,
        &identity(PlayerGender::Male),
        &mut block,
    )
    .unwrap();
    assert_eq!(
        lead.battler().personality(),
        u32::from(low) | (u32::from(high) << 16)
    );
    assert_eq!(real.0.next_u16(), direct.next_u16());
}

#[test]
fn refused_grants_leave_rng_and_save_untouched() {
    let mut block = SaveBlock1 {
        player_party_count: 1,
        ..SaveBlock1::default()
    };
    let before = block.clone().to_bytes(0);
    let mut rng = Scripted {
        draws: DRAWS.to_vec(),
        taken: 0,
    };
    let result = grant_starter(
        &Dex::new(),
        &mut rng,
        Starter::Treecko,
        ROUTE_101,
        &identity(PlayerGender::Male),
        &mut block,
    );
    assert_eq!(result.err(), Some(GrantError::PartyNotEmpty { count: 1 }));
    assert_eq!(rng.taken, 0);
    assert_eq!(block.to_bytes(0), before);
}

#[test]
fn met_level_survives_a_level_up_and_merge() {
    let dex = Dex::new();
    let (mut lead, _, _) = grant(Starter::Torchic, PlayerGender::Male);
    let growth_rate = dex.species(lead.battler().species()).unwrap().growth_rate;
    let level_seven = assets::experience_for_level(growth_rate, 7).unwrap();
    let award = level_seven - lead.battler().experience();
    assert!(
        lead.battler_mut()
            .apply_experience(&dex, award)
            .expect("no move-learn prompt is pending")
            .is_none(),
        "two of the four slots are free, so no move-learn prompt opens"
    );
    assert_eq!(lead.battler().level(), 7, "fixture sanity: the level moved");
    let merged = lead.merge_and_save(&dex);
    assert_eq!(merged.level, 7);
    assert_eq!(
        u16::from_le_bytes([misc(&merged)[2], misc(&merged)[3]]) & 0x7F,
        5
    );
    assert_eq!(merged.box_data.to_bytes()[20..27], OT_NAME[..7]);
    assert_eq!(misc(&merged)[1], ROUTE_101);
}

#[test]
fn every_choice_survives_save_and_load_with_shared_state_unchanged() {
    for starter in Starter::ALL {
        for gender in [PlayerGender::Male, PlayerGender::Female] {
            let mut block1 = SaveBlock1 {
                money: 3000,
                pos: engine::save::Coords16 { x: 7, y: 9 },
                ..SaveBlock1::default()
            };
            block1.location.map_num = 16;
            block1.last_heal_location.map_group = 3;
            block1.bag.items[0] = engine::save::ItemSlot {
                item_id: 13,
                quantity: 5,
            };
            block1.event_data.flag_set(0x0801).unwrap();
            block1.event_data.var_set(0x4001, 0x1234).unwrap();
            let mut expected = block1.clone();
            let block2 = identity(gender);

            let mut rng = Scripted {
                draws: DRAWS.to_vec(),
                taken: 0,
            };
            let lead = grant_starter(
                &Dex::new(),
                &mut rng,
                starter,
                ROUTE_101,
                &block2,
                &mut block1,
            )
            .unwrap();

            let mut store = SaveStore::new();
            store.save(&block1, &block2);
            let mut fresh = SaveStore::from_flash_image(store.flash_image()).unwrap();
            let loaded = fresh.load();
            assert_eq!(loaded.status, SaveStatus::Ok);
            assert_eq!(loaded.block2, block2);

            expected.player_party_count = 1;
            expected.player_party[0] = *lead.record();
            let key = block2.encryption_key;
            assert_eq!(loaded.block1.to_bytes(key), expected.to_bytes(key));
            assert_eq!(loaded.block1.player_party[0], *lead.record());

            let reloaded = LoadedLead::load(&Dex::new(), &loaded.block1.player_party[..1]).unwrap();
            assert_eq!(reloaded.battler(), lead.battler());
            assert_eq!(reloaded.record(), lead.record());
        }
    }
}
