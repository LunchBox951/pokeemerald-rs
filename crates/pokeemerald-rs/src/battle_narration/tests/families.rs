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
    (15, "{B_BUFF1} learned\n{B_BUFF2}!\\p"),
    (16, "{B_BUFF1} is trying to\nlearn {B_BUFF2}.\\p"),
    (17, "But, {B_BUFF1} can't learn\nmore than four moves.\\p"),
    (18, "Delete a move to make\nroom for {B_BUFF2}?"),
    (19, "{B_BUFF1} forgot\n{B_BUFF2}.\\p"),
    (21, "{B_BUFF1} did not learn\n{B_BUFF2}.\\p"),
    (23, "{B_ATK_NAME_WITH_PREFIX}'s\nattack missed!"),
    (28, "{B_ATK_NAME_WITH_PREFIX}\nfainted!\\p"),
    (29, "{B_DEF_NAME_WITH_PREFIX}\nfainted!\\p"),
    (30, "{B_PLAYER_NAME} got ¥{B_BUFF1}\nfor winning!\\p"),
    (31, "{B_PLAYER_NAME} is out of\nusable POKéMON!\\p"),
    (32, "{B_PLAYER_NAME} whited out!"),
    (56, "{B_SCR_ACTIVE_NAME_WITH_PREFIX}'s {B_SCR_ACTIVE_ABILITY}\nparalyzed {B_EFF_NAME_WITH_PREFIX}!"),
    (209, "sharply "),
    (210, "rose!"),
    (211, "harshly "),
    (212, "fell!"),
    (216, "{B_DEF_NAME_WITH_PREFIX}'s {B_BUFF1}\n{B_BUFF2}"),
    (217, "A critical hit!"),
    (221, "It's not very effective…"),
    (222, "It's super effective!"),
    (219, "1, 2, and... Poof!\\p"),
    (220, "And...\\p"),
    (223, "Got away safely!\\p"),
    (224, "Wild {B_BUFF1} fled!"),
    (226, "Can't escape!\\p"),
    (304, ""),
    (327, "{B_DEF_NAME_WITH_PREFIX}'s {B_DEF_ABILITY}\nmade {B_CURRENT_MOVE} useless!"),
    (339, "{B_ATK_NAME_WITH_PREFIX} fled\nusing {B_ATK_ABILITY}!\\p"),
    (306, "{B_DEF_NAME_WITH_PREFIX}'s {B_DEF_ABILITY}\nblocks {B_CURRENT_MOVE}!"),
    (321, "Player defeated\n{B_TRAINER1_CLASS} {B_TRAINER1_NAME}!\\p"),
    (355, "{B_ATK_NAME_WITH_PREFIX}'s {B_ATK_ABILITY}\nprevents {B_DEF_NAME_WITH_PREFIX}'s {B_DEF_ABILITY} from working!"),
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
        ["Wild RATTATA appeared!{SCROLL}"]
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
            "YOUNGSTER BEN\nwould like to battle!{SCROLL}",
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
            "Foe RATTATA\nfainted!{SCROLL}",
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
        BattleEvent::ExpGained(1234),
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
            "TORCHIC gained\n1234 EXP. Points!{SCROLL}",
            "TORCHIC is trying to\nlearn EMBER.{SCROLL}",
            "But, TORCHIC can't learn\nmore than four moves.{SCROLL}",
            "Delete a move to make\nroom for EMBER?",
            "1, 2, and... Poof!{SCROLL}",
            "TORCHIC forgot\nSCRATCH.{SCROLL}",
            "And...{SCROLL}",
            "TORCHIC learned\nEMBER!{SCROLL}",
            "TORCHIC did not learn\nEMBER.{SCROLL}",
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
            "Player defeated\nYOUNGSTER BEN!{SCROLL}",
            "Good battle!{CTRL:0x09}",
            "BRENDAN got ¥120\nfor winning!{SCROLL}"
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
            "BRENDAN is out of\nusable POKéMON!{SCROLL}",
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
            "Can't escape!{SCROLL}",
            "Got away safely!{SCROLL}",
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
        ["TORCHIC fled\nusing RUN AWAY!{SCROLL}"]
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
    };

    assert_eq!(
        lines(&narrate(&mut narrator, &[useless])),
        ["Wild VAPOREON's WATER ABSORB\nmade WATER GUN useless!"]
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
