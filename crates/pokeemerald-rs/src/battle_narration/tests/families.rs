//! One test per message family, driving [`Narrator`] with events and expanding
//! the requests against upstream template text.

use assets::items::ItemId;
use assets::{AbilityId, Effectiveness, MoveId};
use battle::{BattleEvent, BattleOutcome, ChangedStat, StatStage, Status1};

use super::fixtures::{
    battler, narrator, render, request, scene, upstream_fixture, Fixture, RATTATA, SPECIALS,
    ZIGZAGOON,
};
use crate::battle_narration::{
    expand, AssetNames, BattleKind, NarrationError, NarrationRequest, Narrator, Side, TemplateId,
};

fn lines(requests: &[NarrationRequest]) -> Vec<String> {
    let fixture = upstream_fixture();
    requests
        .iter()
        .map(|request| render(&expand(&fixture.table(), &AssetNames, request).unwrap()))
        // The game's apostrophe glyph decodes to a typographic quote.
        .map(|line| line.replace('’', "'"))
        .collect()
}

fn narrate(narrator: &mut Narrator, events: &[BattleEvent]) -> Vec<NarrationRequest> {
    events
        .iter()
        .flat_map(|event| narrator.requests(&AssetNames, event).unwrap())
        .collect()
}

#[test]
fn a_wild_battle_opens_by_naming_the_wild_pokemon() {
    let narrator = narrator(BattleKind::Wild);

    assert_eq!(
        lines(&[narrator.introduction()]),
        ["Wild RATTATA appeared!{CLEAR}"]
    );
    assert_eq!(narrator.initial_send_out(Side::Opponent), None);
    assert_eq!(
        lines(&[narrator.initial_send_out(Side::Player).unwrap()]),
        ["Go! TORCHIC!"]
    );
}

#[test]
fn a_trainer_battle_opens_with_the_challenge_and_the_trainers_send_out() {
    let narrator = narrator(BattleKind::Trainer);

    assert_eq!(
        lines(&[
            narrator.introduction(),
            narrator.initial_send_out(Side::Opponent).unwrap()
        ]),
        [
            "YOUNGSTER BEN\nwould like to battle!{CLEAR}",
            "YOUNGSTER BEN sent\nout RATTATA!"
        ]
    );
}

#[test]
fn a_used_move_is_announced_ahead_of_its_result_with_the_users_prefix() {
    let mut narrator = narrator(BattleKind::Trainer);
    let events = [
        BattleEvent::UsedMove {
            by_player: false,
            move_id: MoveId::TACKLE,
        },
        BattleEvent::Missed {
            by_player: false,
            move_id: MoveId::TACKLE,
        },
        BattleEvent::UsedMove {
            by_player: true,
            move_id: MoveId::EMBER,
        },
    ];

    assert_eq!(
        lines(&narrate(&mut narrator, &events)),
        [
            "Foe RATTATA used\nTACKLE!",
            "Foe RATTATA's\nattack missed!",
            "TORCHIC used\nEMBER!"
        ]
    );
}

#[test]
fn a_sleeping_user_prints_fast_asleep_in_place_of_its_move_and_wakes_ahead_of_it() {
    let mut narrator = narrator(BattleKind::Trainer);
    let events = [
        BattleEvent::FastAsleep {
            by_player: false,
            move_id: MoveId::TACKLE,
        },
        BattleEvent::WokeUp { by_player: true },
        BattleEvent::UsedMove {
            by_player: true,
            move_id: MoveId::EMBER,
        },
    ];

    assert_eq!(
        lines(&narrate(&mut narrator, &events)),
        [
            "Foe RATTATA is fast\nasleep.",
            "TORCHIC woke up!",
            "TORCHIC used\nEMBER!"
        ]
    );
}

#[test]
fn a_frozen_user_prints_frozen_solid_and_a_thaw_names_the_move_that_caused_it() {
    let mut narrator = narrator(BattleKind::Trainer);
    let events = [
        BattleEvent::FrozenSolid {
            by_player: false,
            move_id: MoveId::TACKLE,
        },
        BattleEvent::Thawed { by_player: true },
        BattleEvent::ThawedByMove {
            by_player: true,
            move_id: MoveId::EMBER,
        },
    ];

    assert_eq!(
        lines(&narrate(&mut narrator, &events)),
        [
            "Foe RATTATA is\nfrozen solid!",
            "TORCHIC was\ndefrosted!",
            "TORCHIC was\ndefrosted by EMBER!"
        ]
    );
}

/// `STRINGID_PKMNWASBURNED` names the effect battler and
/// `STRINGID_PKMNWASDEFROSTED` the defender, which is the move's target in
/// both (`battle_message.c:94,101`).
#[test]
fn a_burn_and_a_fire_move_defrost_name_the_target_not_the_user() {
    let mut narrator = narrator(BattleKind::Trainer);
    let events = [
        BattleEvent::Burned {
            by_player: true,
            move_id: MoveId::EMBER,
        },
        BattleEvent::TargetDefrosted {
            by_player: true,
            move_id: MoveId::EMBER,
        },
        BattleEvent::Burned {
            by_player: false,
            move_id: MoveId::TACKLE,
        },
        BattleEvent::TargetDefrosted {
            by_player: false,
            move_id: MoveId::TACKLE,
        },
    ];

    assert_eq!(
        lines(&narrate(&mut narrator, &events)),
        [
            "Foe RATTATA was burned!",
            "Foe RATTATA was\ndefrosted!",
            "TORCHIC was burned!",
            "TORCHIC was\ndefrosted!"
        ]
    );
}

#[test]
fn a_loafing_user_prints_loafing_around_in_place_of_its_move() {
    let mut narrator = narrator(BattleKind::Trainer);
    let events = [
        BattleEvent::LoafingAround {
            by_player: false,
            move_id: MoveId::TACKLE,
        },
        BattleEvent::UsedMove {
            by_player: true,
            move_id: MoveId::EMBER,
        },
    ];

    assert_eq!(
        lines(&narrate(&mut narrator, &events)),
        ["Foe RATTATA is\nloafing around!", "TORCHIC used\nEMBER!"]
    );
}

#[test]
fn a_stat_change_expands_its_nested_stat_and_degree_buffers() {
    let mut narrator = narrator(BattleKind::Wild);
    let fell = BattleEvent::StatFell {
        by_player: true,
        move_id: MoveId::GROWL,
        stat: ChangedStat::Attack,
        new_stage: StatStage::new(-2).unwrap(),
        magnitude: 2,
    };

    assert_eq!(
        lines(&narrate(&mut narrator, &[fell])),
        ["Wild RATTATA's ATTACK\nharshly fell!"]
    );
}

#[test]
fn a_critical_hit_prints_but_a_plain_hit_prints_nothing() {
    let mut narrator = narrator(BattleKind::Wild);
    let hit = |is_critical| BattleEvent::Hit {
        by_player: true,
        move_id: MoveId::TACKLE,
        damage: 5,
        is_critical,
    };

    assert_eq!(
        lines(&narrate(&mut narrator, &[hit(true)])),
        ["A critical hit!"]
    );
    assert!(narrate(&mut narrator, &[hit(false)]).is_empty());
}

#[test]
fn an_ability_blocking_a_move_names_the_holder_the_ability_and_the_move() {
    let mut narrator = narrator(BattleKind::Wild);
    let blocked = BattleEvent::SoundproofProtected {
        by_player: false,
        move_id: MoveId::HYPER_VOICE,
    };

    assert_eq!(
        lines(&narrate(&mut narrator, &[blocked])),
        ["TORCHIC's SOUNDPROOF\nblocks HYPER VOICE!"]
    );
}

#[test]
fn a_status_passed_on_by_an_ability_names_its_holder_and_its_victim() {
    let mut narrator = narrator(BattleKind::Wild);
    let by_target = BattleEvent::ParalyzedByStatic {
        by_player: true,
        move_id: MoveId::TACKLE,
    };
    let by_attacker = BattleEvent::ParalyzedByAttackerSynchronize {
        by_player: true,
        move_id: MoveId::TACKLE,
    };
    let blocked = BattleEvent::SynchronizeLimberProtected {
        by_player: true,
        move_id: MoveId::TACKLE,
    };

    assert_eq!(
        lines(&narrate(&mut narrator, &[by_target, by_attacker, blocked])),
        [
            "Wild RATTATA's STATIC\nparalyzed TORCHIC!",
            "TORCHIC's SYNCHRONIZE\nparalyzed Wild RATTATA!",
            "TORCHIC's LIMBER\nprevents Wild RATTATA's SYNCHRONIZE from working!"
        ]
    );
}

/// `ABILITY_EFFECT_SPORE` statuses the attacker through `SetMoveEffect`'s
/// by-ability strings (`src/battle_util.c:2782`-`:2804`,
/// `src/battle_script_commands.c:2491`-`:2499`), with the holder as the
/// scripting battler.
#[test]
fn effect_spore_names_its_holder_and_the_attacker_it_statused() {
    let mut scene = scene(BattleKind::Wild);
    scene.opponent = battler("PARAS", Some(AbilityId::EFFECT_SPORE));
    let mut narrator = Narrator::new(scene);
    let (by_player, move_id) = (true, MoveId::TACKLE);

    assert_eq!(
        lines(&narrate(
            &mut narrator,
            &[
                BattleEvent::AsleepByEffectSpore { by_player, move_id },
                BattleEvent::PoisonedByEffectSpore { by_player, move_id },
                BattleEvent::ParalyzedByEffectSpore { by_player, move_id },
            ]
        )),
        [
            "Wild PARAS's EFFECT SPORE\nmade TORCHIC sleep!",
            "Wild PARAS's EFFECT SPORE\npoisoned TORCHIC!",
            "Wild PARAS's EFFECT SPORE\nparalyzed TORCHIC!",
        ]
    );
}

/// `BattleScript_BurnTurnDmg` and `BattleScript_PoisonTurnDmg` name the
/// battler taking the tick as the attacker (`data/battle_scripts_1.s:3736`-`:3753`).
#[test]
fn end_turn_poison_and_burn_name_the_battler_they_hurt() {
    let mut narrator = narrator(BattleKind::Wild);

    assert_eq!(
        lines(&narrate(
            &mut narrator,
            &[
                BattleEvent::HurtByBurn {
                    by_player: true,
                    damage: 1,
                },
                BattleEvent::HurtByPoison {
                    by_player: false,
                    damage: 1,
                },
            ]
        )),
        [
            "TORCHIC is hurt\nby its burn!",
            "Wild RATTATA is hurt\nby poison!"
        ]
    );
}

/// `gLeechSeedStringIds` prints its set entry for the seeded target
/// (`data/battle_scripts_1.s:1164`-`:1170`) and its drain entry once for the
/// seeded battler, as the attacker, after both HP updates (`:3265`-`:3284`);
/// the seeder's share prints nothing. `BattleScript_ItemHealHP_RemoveItem`
/// names the holder as the scripting battler and its item as the last used one.
#[test]
fn leech_seed_and_oran_berry_print_their_own_messages() {
    let mut narrator = narrator(BattleKind::Wild);

    assert_eq!(
        lines(&narrate(
            &mut narrator,
            &[
                BattleEvent::Seeded {
                    by_player: true,
                    move_id: MoveId::LEECH_SEED,
                },
                BattleEvent::SappedByLeechSeed {
                    by_player: false,
                    damage: 1,
                },
                BattleEvent::LeechSeedHealed {
                    by_player: true,
                    healed: 1,
                },
                BattleEvent::ItemRestoredHealth {
                    by_player: true,
                    item: ItemId::ORAN_BERRY,
                    healed: 10,
                },
            ]
        )),
        [
            "Wild RATTATA was seeded!",
            "Wild RATTATA's health is\nsapped by LEECH SEED!",
            "TORCHIC's ORAN BERRY\nrestored health!",
        ]
    );
}

/// `BattleScript_EffectLeechSeed`'s `accuracycheck` jumps to
/// `BattleScript_DoLeechSeed` on a miss (`data/battle_scripts_1.s:1163`), and
/// `Cmd_setseeded` picks `B_MSG_LEECH_SEED_MISS` for that miss and for an
/// already-seeded target alike (`src/battle_script_commands.c:6720`-`:6724`),
/// so either failure names the target (`gLeechSeedStringIds`,
/// `src/battle_message.c:963`) rather than the attacker's generic miss.
#[test]
fn a_missed_or_repeated_leech_seed_names_the_target_that_evaded_it() {
    let mut narrator = narrator(BattleKind::Wild);

    assert_eq!(
        lines(&narrate(
            &mut narrator,
            &[
                BattleEvent::LeechSeedEvaded {
                    by_player: true,
                    move_id: MoveId::LEECH_SEED,
                },
                BattleEvent::LeechSeedEvaded {
                    by_player: false,
                    move_id: MoveId::LEECH_SEED,
                },
            ]
        )),
        [
            "Wild RATTATA evaded\nthe attack!",
            "TORCHIC evaded\nthe attack!",
        ]
    );
}

#[test]
fn a_replacement_is_named_after_the_faint_that_precedes_it() {
    let mut narrator = narrator(BattleKind::Trainer);
    let events = [
        BattleEvent::Fainted { by_player: false },
        BattleEvent::TrainerSentOut {
            species: ZIGZAGOON,
            bench_remaining: 0,
        },
        BattleEvent::Missed {
            by_player: false,
            move_id: MoveId::TACKLE,
        },
    ];
    let requests = narrate(&mut narrator, &events);

    assert_eq!(
        lines(&requests),
        [
            "Foe RATTATA\nfainted!{CLEAR}",
            "YOUNGSTER BEN sent\nout ZIGZAGOON!",
            "Foe ZIGZAGOON's\nattack missed!"
        ]
    );
}

#[test]
fn the_players_replacement_wording_follows_the_health_scale() {
    let spoken = |hp_scale| {
        let mut scene = scene(BattleKind::Wild);
        scene.hp_scale = hp_scale;
        let mut narrator = Narrator::new(scene);
        let event = BattleEvent::PlayerSentOut {
            species: RATTATA,
            reserves_remaining: 1,
        };
        lines(&narrate(&mut narrator, &[event]))
    };

    assert_eq!(spoken(0), ["Go! RATTATA!"]);
    assert_eq!(spoken(1), ["Do it! RATTATA!"]);
    assert_eq!(spoken(2), ["Go for it, RATTATA!"]);
    assert_eq!(spoken(3), ["Your foe's weak!\nGet 'em, RATTATA!"]);
}

#[test]
fn experience_and_move_learning_print_in_upstream_order() {
    let mut narrator = narrator(BattleKind::Wild);
    let events = [
        BattleEvent::ExpGained {
            amount: 1234,
            boosted: false,
        },
        BattleEvent::MoveLearnPrompt {
            move_id: MoveId::EMBER,
        },
        BattleEvent::MoveReplaced {
            learned: MoveId::EMBER,
            forgotten: MoveId::SCRATCH,
            slot: 0,
        },
        BattleEvent::MoveLearnDeclined {
            move_id: MoveId::EMBER,
        },
    ];

    assert_eq!(
        lines(&narrate(&mut narrator, &events)),
        [
            "TORCHIC gained\n1234 EXP. Points!{CLEAR}",
            "TORCHIC is trying to\nlearn EMBER.{CLEAR}",
            "But, TORCHIC can't learn\nmore than four moves.{CLEAR}",
            "Delete a move to make\nroom for EMBER?",
            "1, 2, and... Poof!{CLEAR}",
            "TORCHIC forgot\nSCRATCH.{CLEAR}",
            "And...{CLEAR}",
            "TORCHIC learned\nEMBER!{CLEAR}",
            // `decode_to_string` spells `{PAUSE 32}` as its control code.
            "{CTRL:0x08}Stop learning\nEMBER?",
            "TORCHIC did not learn\nEMBER.{CLEAR}",
        ]
    );
}

#[test]
fn a_trainer_victory_prints_the_defeat_the_trainers_words_then_the_prize() {
    let mut narrator = narrator(BattleKind::Trainer);
    let events = [
        BattleEvent::MoneyGained(120),
        BattleEvent::Ended(BattleOutcome::PlayerWon),
    ];

    assert_eq!(
        lines(&narrate(&mut narrator, &events)),
        [
            "Player defeated\nYOUNGSTER BEN!{CLEAR}",
            "Good battle!{CTRL:0x09}",
            "BRENDAN got ¥120\nfor winning!{CLEAR}"
        ]
    );
}

#[test]
fn a_loss_prints_both_whiteout_messages_and_a_wild_win_prints_nothing() {
    let mut narrator = narrator(BattleKind::Wild);

    assert_eq!(
        lines(&narrate(
            &mut narrator,
            &[BattleEvent::Ended(BattleOutcome::PlayerLost)]
        )),
        [
            "BRENDAN is out of\nusable POKéMON!{CLEAR}",
            "BRENDAN whited out!"
        ]
    );
    assert!(narrate(
        &mut narrator,
        &[BattleEvent::Ended(BattleOutcome::PlayerWon)]
    )
    .is_empty());
}

#[test]
fn running_and_a_fleeing_wild_pokemon_print_their_own_lines() {
    let mut narrator = narrator(BattleKind::Wild);
    let events = [
        BattleEvent::RunAttempt {
            by_player: true,
            success: false,
        },
        BattleEvent::RunAttempt {
            by_player: true,
            success: true,
        },
        BattleEvent::Ended(BattleOutcome::PlayerRan),
        BattleEvent::WildFled,
        BattleEvent::Ended(BattleOutcome::WildFled),
    ];

    assert_eq!(
        lines(&narrate(&mut narrator, &events)),
        [
            "Can't escape!{CLEAR}",
            "Got away safely!{CLEAR}",
            "Wild RATTATA fled!"
        ]
    );
}

#[test]
fn a_cured_status_that_names_no_status_is_refused() {
    let mut narrator = narrator(BattleKind::Wild);
    let event = BattleEvent::ShedSkinCured {
        by_player: true,
        status: Status1::Healthy,
    };

    assert_eq!(
        narrator.requests(&AssetNames, &event),
        Err(NarrationError::MissingContext("cured status"))
    );
}

/// Shed Skin names the cured status through `STATUS1_POISON` for either
/// poison (`pokeemerald/src/battle_util.c:2620-2638`), so a bad poison cure
/// narrates exactly as an ordinary one, whatever its stored counter.
#[test]
fn a_shed_skin_bad_poison_cure_narrates_as_poison() {
    for by_player in [true, false] {
        let expected = narrator(BattleKind::Wild).requests(
            &AssetNames,
            &BattleEvent::ShedSkinCured {
                by_player,
                status: Status1::Poisoned,
            },
        );
        assert!(expected.is_ok(), "{expected:?}");
        for counter in [0, 5, 15] {
            let toxic = narrator(BattleKind::Wild).requests(
                &AssetNames,
                &BattleEvent::ShedSkinCured {
                    by_player,
                    status: Status1::BadPoisoned { counter },
                },
            );
            assert_eq!(toxic, expected, "counter {counter}, by_player {by_player}");
        }
    }
}

#[test]
fn a_request_keeps_the_names_it_was_made_with() {
    let mut narrator = narrator(BattleKind::Wild);
    let before = narrate(
        &mut narrator,
        &[BattleEvent::Missed {
            by_player: false,
            move_id: MoveId::TACKLE,
        }],
    );
    narrate(
        &mut narrator,
        &[BattleEvent::TrainerSentOut {
            species: ZIGZAGOON,
            bench_remaining: 0,
        }],
    );

    assert_eq!(lines(&before), ["Wild RATTATA's\nattack missed!"]);
    assert_eq!(
        request(TemplateId::Table(23), before[0].context.clone()),
        before[0]
    );
    assert_eq!(narrator.scene().opponent.name, "ZIGZAGOON");
    assert_eq!(narrator.scene().opponent.ability, None);
}

#[test]
fn a_staged_replacement_supplies_its_nickname_and_ability_to_later_requests() {
    let mut narrator = narrator(BattleKind::Wild);
    narrator.stage_replacement(Side::Player, battler("ACE", Some(AbilityId::WATER_ABSORB)));
    let events = [
        BattleEvent::PlayerSentOut {
            species: RATTATA,
            reserves_remaining: 0,
        },
        BattleEvent::AbilityHealed {
            by_player: false,
            move_id: MoveId::TACKLE,
            healed: 3,
            ability: AbilityId::WATER_ABSORB,
        },
    ];
    let mut requests = narrate(&mut narrator, &events);
    requests[1].template = TemplateId::Table(197);
    let fixture = Fixture::new(
        &[(
            197,
            "{B_DEF_NAME_WITH_PREFIX} restored HP\nusing its {B_DEF_ABILITY}!",
        )],
        SPECIALS,
    );

    let printed: Vec<String> = requests
        .iter()
        .map(|request| render(&expand(&fixture.table(), &AssetNames, request).unwrap()))
        .collect();

    assert_eq!(
        printed,
        ["Go! ACE!", "ACE restored HP\nusing its WATER ABSORB!"]
    );
}

#[test]
fn a_hit_prints_its_critical_line_before_its_effectiveness_line() {
    let mut narrator = narrator(BattleKind::Wild);
    let events = [
        BattleEvent::Hit {
            by_player: true,
            move_id: MoveId::TACKLE,
            damage: 9,
            is_critical: true,
        },
        BattleEvent::TypeEffectiveness {
            by_player: true,
            effectiveness: Effectiveness::SuperEffective,
        },
        BattleEvent::TypeEffectiveness {
            by_player: true,
            effectiveness: Effectiveness::NotVeryEffective,
        },
    ];

    assert_eq!(
        lines(&narrate(&mut narrator, &events)),
        [
            "A critical hit!",
            "It's super effective!",
            "It's not very effective…"
        ]
    );
}

#[test]
fn a_run_away_escape_names_the_fleeing_battler_and_its_ability() {
    let mut scene = scene(BattleKind::Wild);
    scene.player = battler("TORCHIC", Some(AbilityId::RUN_AWAY));
    let mut narrator = Narrator::new(scene);
    let escape = BattleEvent::RunAttempt {
        by_player: true,
        success: true,
    };

    assert_eq!(
        lines(&narrate(&mut narrator, &[escape])),
        ["TORCHIC fled\nusing RUN AWAY!{CLEAR}"]
    );
}

#[test]
fn a_full_hp_water_absorb_activation_says_the_move_was_made_useless() {
    let mut scene = scene(BattleKind::Wild);
    scene.opponent = battler("VAPOREON", Some(AbilityId::WATER_ABSORB));
    let mut narrator = Narrator::new(scene);
    let useless = BattleEvent::AbsorbMadeMoveUseless {
        by_player: true,
        move_id: MoveId::WATER_GUN,
        ability: AbilityId::WATER_ABSORB,
    };

    assert_eq!(
        lines(&narrate(&mut narrator, &[useless])),
        ["Wild VAPOREON's WATER ABSORB\nmade WATER GUN useless!"]
    );
}

#[test]
fn an_unstaged_send_out_still_names_the_absorbing_ability() {
    let mut narrator = narrator(BattleKind::Trainer);
    let sent_out = BattleEvent::TrainerSentOut {
        species: ZIGZAGOON,
        bench_remaining: 0,
    };
    let absorbed = [
        BattleEvent::AbsorbMadeMoveUseless {
            by_player: true,
            move_id: MoveId::WATER_GUN,
            ability: AbilityId::WATER_ABSORB,
        },
        BattleEvent::AbilityHealed {
            by_player: true,
            move_id: MoveId::THUNDER_SHOCK,
            healed: 3,
            ability: AbilityId::VOLT_ABSORB,
        },
    ];

    narrate(&mut narrator, &[sent_out]);
    assert_eq!(narrator.scene().opponent.ability, None);
    assert_eq!(
        lines(&narrate(&mut narrator, &absorbed)),
        [
            "Foe ZIGZAGOON's WATER ABSORB\nmade WATER GUN useless!",
            "Foe ZIGZAGOON restored HP\nusing its VOLT ABSORB!",
        ]
    );
}

#[test]
fn replacement_wording_reflects_opponent_health_at_send_out() {
    let mut narrator = narrator(BattleKind::Wild);
    let sent_out = BattleEvent::PlayerSentOut {
        species: RATTATA,
        reserves_remaining: 1,
    };

    narrator.observe_opponent_health(30, 30);
    assert_eq!(
        lines(&narrate(&mut narrator, &[sent_out])),
        ["Go! RATTATA!"]
    );
    narrator.observe_opponent_health(2, 30);
    assert_eq!(
        lines(&narrate(&mut narrator, &[sent_out])),
        ["Your foe's weak!\nGet 'em, RATTATA!"]
    );
}

#[test]
fn opponent_health_maps_to_upstreams_hp_thresholds() {
    let scale = |current, max| {
        let mut narrator = narrator(BattleKind::Wild);
        narrator.observe_opponent_health(current, max);
        narrator.scene().hp_scale
    };

    assert_eq!(scale(100, 100), 0);
    assert_eq!(scale(70, 100), 0);
    assert_eq!(scale(69, 100), 1);
    assert_eq!(scale(40, 100), 1);
    assert_eq!(scale(39, 100), 2);
    assert_eq!(scale(10, 100), 2);
    assert_eq!(scale(9, 100), 3);
    assert_eq!(scale(1, 1000), 3);
    assert_eq!(scale(0, 100), 0);
}

#[test]
fn declining_a_move_asks_stop_learning_before_did_not_learn() {
    let mut narrator = narrator(BattleKind::Wild);
    let requests = narrate(
        &mut narrator,
        &[BattleEvent::MoveLearnDeclined {
            move_id: MoveId::EMBER,
        }],
    );
    let templates: Vec<TemplateId> = requests.iter().map(|r| r.template).collect();
    // STRINGID_STOPLEARNINGMOVE = 20, STRINGID_DIDNOTLEARNMOVE = 21.
    assert_eq!(templates, [TemplateId::Table(20), TemplateId::Table(21)]);
}

/// `Cmd_getexp` buffers `STRINGID_ABOOSTED` for a traded recipient's award and
/// `STRINGID_EMPTYSTRING4` otherwise (`src/battle_script_commands.c:3381`-`:3397`).
#[test]
fn a_boosted_award_reads_gained_a_boosted() {
    let mut narrator = narrator(BattleKind::Wild);
    let award = |boosted| BattleEvent::ExpGained {
        amount: 69,
        boosted,
    };

    assert_eq!(
        lines(&narrate(&mut narrator, &[award(false), award(true)])),
        [
            "TORCHIC gained\n69 EXP. Points!{CLEAR}",
            "TORCHIC gained a boosted\n69 EXP. Points!{CLEAR}",
        ]
    );
}
