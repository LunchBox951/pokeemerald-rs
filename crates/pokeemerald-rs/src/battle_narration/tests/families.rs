//! One test per message family, driving [`Narrator`] with events and expanding
//! the requests against upstream template text.

use assets::{AbilityId, Effectiveness, MoveId};
use battle::{BattleEvent, BattleOutcome, ChangedStat, StatStage, Status1};

use super::fixtures::{battler, narrator, render, request, scene, Fixture, RATTATA, ZIGZAGOON};
use crate::battle_narration::{
    expand, AssetNames, BattleKind, NarrationError, NarrationRequest, Narrator, Side, TemplateId,
};

/// Upstream table templates (`src/battle_message.c`) keyed by `STRINGID_*`.
const SLOTS: &[(u16, &str)] = &[
    (12, "{B_TRAINER1_LOSE_TEXT}"),
    (13, "{B_BUFF1} gained{B_BUFF2}\n{B_BUFF3} EXP. Points!\\p"),
    (15, "{B_BUFF1} learned\n{B_BUFF2}!{WAIT_SE}\\p"),
    (16, "{B_BUFF1} is trying to\nlearn {B_BUFF2}.\\p"),
    (17, "But, {B_BUFF1} can't learn\nmore than four moves.\\p"),
    (18, "Delete a move to make\nroom for {B_BUFF2}?"),
    (19, "{B_BUFF1} forgot\n{B_BUFF2}.\\p"),
    (20, "{PAUSE 32}Stop learning\n{B_BUFF2}?"),
    (21, "{B_BUFF1} did not learn\n{B_BUFF2}.\\p"),
    (23, "{B_ATK_NAME_WITH_PREFIX}'s\nattack missed!"),
    (28, "{B_ATK_NAME_WITH_PREFIX}\nfainted!\\p"),
    (29, "{B_DEF_NAME_WITH_PREFIX}\nfainted!\\p"),
    (30, "{B_PLAYER_NAME} got ¥{B_BUFF1}\nfor winning!\\p"),
    (31, "{B_PLAYER_NAME} is out of\nusable POKéMON!\\p"),
    (32, "{B_PLAYER_NAME} whited out!{PAUSE_UNTIL_PRESS}"),
    (36, "{B_SCR_ACTIVE_NAME_WITH_PREFIX}'s {B_SCR_ACTIVE_ABILITY}\nmade {B_EFF_NAME_WITH_PREFIX} sleep!"),
    (41, "{B_SCR_ACTIVE_NAME_WITH_PREFIX}'s {B_SCR_ACTIVE_ABILITY}\npoisoned {B_EFF_NAME_WITH_PREFIX}!"),
    (42, "{B_ATK_NAME_WITH_PREFIX} is hurt\nby poison!"),
    (48, "{B_ATK_NAME_WITH_PREFIX} is hurt\nby its burn!"),
    (107, "{B_ATK_NAME_WITH_PREFIX} is fast\nasleep."),
    (108, "{B_ATK_NAME_WITH_PREFIX} woke up!"),
    (51, "{B_ATK_NAME_WITH_PREFIX} is\nfrozen solid!"),
    (53, "{B_ATK_NAME_WITH_PREFIX} was\ndefrosted!"),
    (54, "{B_ATK_NAME_WITH_PREFIX} was\ndefrosted by {B_CURRENT_MOVE}!"),
    (278, "{B_ATK_NAME_WITH_PREFIX} is\nloafing around!"),
    (197, "{B_DEF_NAME_WITH_PREFIX} restored HP\nusing its {B_DEF_ABILITY}!"),
    (56, "{B_SCR_ACTIVE_NAME_WITH_PREFIX}'s {B_SCR_ACTIVE_ABILITY}\nparalyzed {B_EFF_NAME_WITH_PREFIX}!\\lIt may be unable to move!"),
    (209, "sharply "),
    (210, "rose!"),
    (211, "harshly "),
    (212, "fell!"),
    (216, "{B_DEF_NAME_WITH_PREFIX}'s {B_BUFF1}\n{B_BUFF2}"),
    (217, "A critical hit!"),
    (221, "It's not very effective…"),
    (222, "It's super effective!"),
    (
        219,
        "{PAUSE 32}1, {PAUSE 15}2, and{PAUSE 15}… {PAUSE 15}… {PAUSE 15}… {PAUSE 15}{PLAY_SE SE_BALL_BOUNCE_1}Poof!\\p",
    ),
    (220, "And…\\p"),
    (223, "{PLAY_SE SE_FLEE}Got away safely!\\p"),
    (224, "{PLAY_SE SE_FLEE}Wild {B_BUFF1} fled!"),
    (226, "Can't escape!\\p"),
    (327, "{B_DEF_NAME_WITH_PREFIX}'s {B_DEF_ABILITY}\nmade {B_CURRENT_MOVE} useless!"),
    (329, ""),
    (330, " a boosted"),
    (339, "{PLAY_SE SE_FLEE}{B_ATK_NAME_WITH_PREFIX} fled\nusing {B_ATK_ABILITY}!\\p"),
    (306, "{B_DEF_NAME_WITH_PREFIX}'s {B_DEF_ABILITY}\nblocks {B_CURRENT_MOVE}!"),
    (321, "Player defeated\n{B_TRAINER1_CLASS} {B_TRAINER1_NAME}!\\p"),
    (355, "{B_ATK_NAME_WITH_PREFIX}'s {B_ATK_ABILITY}\nprevents {B_DEF_NAME_WITH_PREFIX}'s\\l{B_DEF_ABILITY} from working!"),
];

const SPECIALS: &[(&str, &str)] = &[
    (
        "sText_WildPkmnAppeared",
        "Wild {B_OPPONENT_MON1_NAME} appeared!\\p",
    ),
    (
        "sText_Trainer1WantsToBattle",
        "{B_TRAINER1_CLASS} {B_TRAINER1_NAME}\nwould like to battle!\\p",
    ),
    ("sText_GoPkmn", "Go! {B_PLAYER_MON1_NAME}!"),
    (
        "sText_Trainer1SentOutPkmn",
        "{B_TRAINER1_CLASS} {B_TRAINER1_NAME} sent\nout {B_OPPONENT_MON1_NAME}!",
    ),
    (
        "sText_Trainer1SentOutPkmn2",
        "{B_TRAINER1_CLASS} {B_TRAINER1_NAME} sent\nout {B_BUFF1}!",
    ),
    ("sText_GoPkmn2", "Go! {B_BUFF1}!"),
    ("sText_DoItPkmn", "Do it! {B_BUFF1}!"),
    ("sText_GoForItPkmn", "Go for it, {B_BUFF1}!"),
    (
        "sText_YourFoesWeakGetEmPkmn",
        "Your foe's weak!\nGet 'em, {B_BUFF1}!",
    ),
    (
        "sText_AttackerUsedX",
        "{B_ATK_NAME_WITH_PREFIX} used\n{B_BUFF2}",
    ),
    ("sText_ExclamationMark", "!"),
];

fn fixture() -> Fixture {
    Fixture::new(SLOTS, SPECIALS)
}

fn lines(requests: &[NarrationRequest]) -> Vec<String> {
    let fixture = fixture();
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
            "Wild RATTATA's STATIC\nparalyzed TORCHIC!{SCROLL}It may be unable to move!",
            "TORCHIC's SYNCHRONIZE\nparalyzed Wild RATTATA!{SCROLL}It may be unable to move!",
            "TORCHIC's LIMBER\nprevents Wild RATTATA's{SCROLL}SYNCHRONIZE from working!"
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
            "Wild PARAS's EFFECT SPORE\nparalyzed TORCHIC!{SCROLL}It may be unable to move!",
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
            "{CTRL:0x08}1, {CTRL:0x08}2, and{CTRL:0x08}… {CTRL:0x08}… {CTRL:0x08}… \
             {CTRL:0x08}{CTRL:0x10}Poof!{CLEAR}",
            "TORCHIC forgot\nSCRATCH.{CLEAR}",
            "And…{CLEAR}",
            "TORCHIC learned\nEMBER!{CTRL:0x0a}{CLEAR}",
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
            "BRENDAN whited out!{CTRL:0x09}"
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
            "{CTRL:0x10}Got away safely!{CLEAR}",
            "{CTRL:0x10}Wild RATTATA fled!"
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
        ["{CTRL:0x10}TORCHIC fled\nusing RUN AWAY!{CLEAR}"]
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

/// Hand-encoded upstream bytes (`charmap.txt:420`-`:428`, `:1087`-`:1088`) for
/// `sText_PlayerWhiteout2`, `sText_123Poof`, `sText_AndEllipsis`,
/// `sText_PkmnWasParalyzedBy`, `sText_GotAwaySafely` and `sText_WildPkmnFled`
/// (`src/battle_message.c:78`, `:105`, `:313`, `:314`, `:318`, `:321`). They
/// bypass the fixture encoder, so dropping a wait, a sound command or a
/// continuation from `expand` changes what comes out.
mod upstream_bytes {
    use std::collections::BTreeMap;

    use assets::pack::{
        BattleMessageTable, BATTLE_MESSAGE_SPECIAL_TEMPLATES as SPECIAL_TEMPLATES,
        BATTLE_MESSAGE_TABLE_END as TABLE_END, BATTLE_MESSAGE_TABLE_START as TABLE_START,
    };
    use engine::text::{self, Token};

    use super::{
        expand, narrate, narrator, render, AssetNames, BattleEvent, BattleKind, BattleOutcome,
        MoveId,
    };

    const END: u8 = 0xFF;
    const SCROLL: u8 = 0xFA;
    const CLEAR: u8 = 0xFB;
    const NEWLINE: u8 = 0xFE;
    const PAUSE_UNTIL_PRESS: [u8; 2] = [0xFC, 0x09];
    /// `FC 10 lo hi` for `SE_FLEE` (17) and `SE_BALL_BOUNCE_1` (56).
    const PLAY_SE_FLEE: [u8; 4] = [0xFC, 0x10, 17, 0];
    const PLAY_SE_BALL_BOUNCE_1: [u8; 4] = [0xFC, 0x10, 56, 0];

    /// `FC 08 n`.
    fn pause(frames: u8) -> [u8; 3] {
        [0xFC, 0x08, frames]
    }

    fn glyphs(source: &str) -> Vec<u8> {
        source
            .chars()
            .map(|c| text::char_to_byte(c).expect("glyph encodes"))
            .collect()
    }

    fn chars(source: &str) -> Vec<Token> {
        source.chars().map(Token::Char).collect()
    }

    fn control(sub: u8, args: &[u8]) -> Token {
        Token::ExtCtrl {
            sub,
            args: args.to_vec(),
        }
    }

    fn poof_slot() -> Vec<u8> {
        [
            &pause(32)[..],
            &glyphs("1, "),
            &pause(15),
            &glyphs("2, and"),
            &pause(15),
            &glyphs("… "),
            &pause(15),
            &glyphs("… "),
            &pause(15),
            &glyphs("… "),
            &pause(15),
            &PLAY_SE_BALL_BOUNCE_1,
            &glyphs("Poof!"),
            &[CLEAR, END],
        ]
        .concat()
    }

    fn paralyzed_slot() -> Vec<u8> {
        [
            &[0xFD, 0x13][..],
            &glyphs("'s "),
            &[0xFD, 0x1A, NEWLINE],
            &glyphs("paralyzed "),
            &[0xFD, 0x11],
            &glyphs("!"),
            &[SCROLL],
            &glyphs("It may be unable to move!"),
            &[END],
        ]
        .concat()
    }

    fn slots() -> BTreeMap<u16, Vec<u8>> {
        BTreeMap::from([
            (
                32,
                [
                    &[0xFD, 0x23][..],
                    &glyphs(" whited out!"),
                    &PAUSE_UNTIL_PRESS,
                    &[END],
                ]
                .concat(),
            ),
            (56, paralyzed_slot()),
            (219, poof_slot()),
            (220, [&glyphs("And…")[..], &[CLEAR, END]].concat()),
            (
                223,
                [
                    &PLAY_SE_FLEE[..],
                    &glyphs("Got away safely!"),
                    &[CLEAR, END],
                ]
                .concat(),
            ),
            (
                224,
                [
                    &PLAY_SE_FLEE[..],
                    &glyphs("Wild "),
                    &[0xFD, 0x00],
                    &glyphs(" fled!"),
                    &[END],
                ]
                .concat(),
            ),
        ])
    }

    /// What `events` print against the upstream bytes above.
    fn expanded(events: &[BattleEvent]) -> Vec<Vec<Token>> {
        let slots = slots();
        let empty: &[u8] = &[END];
        let table = BattleMessageTable::from_entries(
            (TABLE_START..TABLE_END).map(|id| (id, slots.get(&id).map_or(empty, Vec::as_slice))),
            (0..SPECIAL_TEMPLATES.len()).map(|index| (u16::try_from(index).unwrap(), empty)),
        )
        .unwrap();
        let mut narrator = narrator(BattleKind::Wild);
        narrate(&mut narrator, events)
            .iter()
            .map(|request| expand(&table, &AssetNames, request).unwrap())
            .collect()
    }

    #[test]
    fn the_second_whiteout_line_waits_for_a_press() {
        let lines = expanded(&[BattleEvent::Ended(BattleOutcome::PlayerLost)]);

        assert_eq!(
            lines[1],
            [
                chars("BRENDAN whited out!"),
                vec![control(0x09, &[]), Token::End]
            ]
            .concat()
        );
    }

    #[test]
    fn learning_over_a_move_keeps_every_poof_wait_and_the_bounce_sound() {
        let lines = expanded(&[BattleEvent::MoveReplaced {
            learned: MoveId::EMBER,
            forgotten: MoveId::SCRATCH,
            slot: 0,
        }]);

        assert_eq!(
            lines[0],
            [
                vec![control(0x08, &[32])],
                chars("1, "),
                vec![control(0x08, &[15])],
                chars("2, and"),
                vec![control(0x08, &[15])],
                chars("… "),
                vec![control(0x08, &[15])],
                chars("… "),
                vec![control(0x08, &[15])],
                chars("… "),
                vec![control(0x08, &[15]), control(0x10, &[56, 0])],
                chars("Poof!"),
                vec![Token::PromptClear, Token::End],
            ]
            .concat()
        );
        assert_eq!(
            lines[2],
            [chars("And…"), vec![Token::PromptClear, Token::End]].concat()
        );
    }

    #[test]
    fn escapes_start_with_the_flee_sound() {
        let lines = expanded(&[
            BattleEvent::RunAttempt {
                by_player: true,
                success: true,
            },
            BattleEvent::WildFled,
        ]);

        assert_eq!(
            lines[0],
            [
                vec![control(0x10, &[17, 0])],
                chars("Got away safely!"),
                vec![Token::PromptClear, Token::End],
            ]
            .concat()
        );
        assert_eq!(
            lines[1],
            [
                vec![control(0x10, &[17, 0])],
                chars("Wild RATTATA fled!"),
                vec![Token::End],
            ]
            .concat()
        );
    }

    #[test]
    fn an_ability_paralysis_scrolls_on_to_its_second_sentence() {
        let lines = expanded(&[BattleEvent::ParalyzedByStatic {
            by_player: true,
            move_id: MoveId::TACKLE,
        }]);

        assert_eq!(
            render(&lines[0]).replace('’', "'"),
            "Wild RATTATA's STATIC\nparalyzed TORCHIC!{SCROLL}It may be unable to move!"
        );
        assert!(lines[0].contains(&Token::PromptScroll));
    }
}
