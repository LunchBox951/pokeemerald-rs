//! `BATTLE_TYPE_TRAINER` (issue #237): the scripted Route 103 rival battle's
//! five deltas from a wild encounter — running refused, a party opponent, the
//! forced post-faint send-out, `x1.5` experience, and prize money — pinned
//! end to end through the public `battle` API, the same way every other
//! `turn_engine/` module pins its family of behaviour. Per-script AI draw
//! accounting is pinned next to the AI itself
//! (`crates/battle/src/battle/trainer_ai.rs`); construction is pinned in
//! `crates/pokeemerald-rs/src/flow/route103_rival/tests.rs`.
//!
//! The trainers used here are the real ones. `TRAINER_MAY_ROUTE_103_MUDKIP`
//! (`include/constants/opponents.h:533`) is the rival a player who chose
//! Mudkip fights: `sParty_MayRoute103Mudkip`
//! (`src/data/trainer_parties.h:6916`-`:6921`) is a single `.iv = 0`,
//! level-5 **Treecko** — the type-advantaged answer to the player's Water
//! starter — and her `aiFlags` are `AI_SCRIPT_CHECK_BAD_MOVE |
//! AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY`
//! (`src/data/trainers.h:6352`-`:6362`). A level-5 Treecko's
//! `GiveBoxMonInitialMoveset` moveset is Pound + Leer (both level-1 entries
//! of `sTreeckoLevelUpLearnset`, `level_up_learnsets.h:3572`-`:3574`;
//! Absorb is level 6).
//!
//! Two-mon parties do not exist on Route 103, so the send-out-in-party-order
//! rule is exercised against a hand-built party instead — the rule is
//! upstream's, and pinning it only against a one-mon party would pin
//! nothing.

use crate::common::{max_iv_mon, SequenceRng};
use assets::trainers::TrainerId;
use assets::{MoveId, SpeciesId};
use battle::{
    Battle, BattleError, BattleEvent, BattleOutcome, BattlePokemon, Dex, HitOutcome, PlayerAction,
};

/// `TRAINER_MAY_ROUTE_103_MUDKIP` — the rival fought after choosing Mudkip.
const MAY_ROUTE_103_MUDKIP: TrainerId = TrainerId(529);
/// `TRAINER_BRENDAN_ROUTE_103_TREECKO` — the one Route 103 entry whose third
/// `aiFlags` bit is `AI_SCRIPT_SETUP_FIRST_TURN` rather than
/// `AI_SCRIPT_CHECK_VIABILITY` (`src/data/trainers.h:6280`-`:6290`).
const BRENDAN_ROUTE_103_TREECKO: TrainerId = TrainerId(523);

const TREECKO: u16 = 277;
const TORCHIC: u16 = 280;
const MUDKIP: u16 = 283;
const ZIGZAGOON: u16 = 288;
const PICHU: u16 = 172;

const POUND: MoveId = MoveId(1);
const SCRATCH: MoveId = MoveId(10);
const TACKLE: MoveId = MoveId(33);
const LEER: MoveId = MoveId(43);
const GROWL: MoveId = MoveId(45);
const SLASH: MoveId = MoveId(163);

/// The rival's real party: one level-5 Treecko knowing Pound and Leer.
fn rival_treecko(dex: &Dex) -> Vec<BattlePokemon> {
    vec![max_iv_mon(dex, TREECKO, 5, vec![POUND, LEER])]
}

#[test]
fn running_from_a_trainer_is_refused_before_any_draw_and_leaves_the_battle_usable() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, MUDKIP, 5, vec![TACKLE]);

    let mut rng = SequenceRng::new([0]); // Battle::new_trainer's turn-number draw
    let mut battle = Battle::new_trainer(
        dex,
        player,
        MAY_ROUTE_103_MUDKIP,
        rival_treecko(&Dex::new()),
        &mut rng,
    )
    .unwrap();
    assert_eq!(rng.draws(), 1, "no speed tie for these two");

    let failure = battle.take_turn(PlayerAction::Run, &mut rng).unwrap_err();
    assert_eq!(
        failure.error(),
        BattleError::NoRunningFromTrainer,
        "a trainer battle's refusal is its own error, not first_battle's"
    );
    assert!(
        failure.events().is_empty(),
        "a pre-draw rejection reports no events"
    );
    assert_eq!(
        rng.draws(),
        1,
        "the refusal is checked ahead of the turn-number draw -- the shared \
         stream must not move at all"
    );
    assert!(battle.outcome().is_none(), "the battle is still usable");
    assert_eq!(
        battle.run_tries(),
        0,
        "runTries is only bumped by a real TryRunFromBattle attempt"
    );
}

#[test]
fn the_route_103_rival_battle_exposes_its_trainer_context() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, MUDKIP, 5, vec![TACKLE]);
    let mut rng = SequenceRng::new([0]);
    let battle = Battle::new_trainer(
        dex,
        player,
        MAY_ROUTE_103_MUDKIP,
        rival_treecko(&Dex::new()),
        &mut rng,
    )
    .unwrap();

    let context = battle.trainer().expect("a trainer battle has a context");
    assert_eq!(context.id(), MAY_ROUTE_103_MUDKIP);
    assert_eq!(context.bench_len(), 0, "the Route 103 party is one mon");
    // TRAINER_CLASS_RIVAL's gTrainerMoneyTable value is 15, the party's last
    // mon is level 5, moneyMultiplier is 1: 4 * 5 * 1 * 15.
    assert_eq!(context.money(), 300);
    assert_eq!(battle.enemy().species(), SpeciesId(TREECKO));
    assert_eq!(battle.enemy().level(), 5);
    let known: Vec<MoveId> = battle.enemy().moves().iter().map(|m| m.move_id).collect();
    assert_eq!(known, vec![POUND, LEER]);
}

#[test]
fn a_wild_battle_has_no_trainer_context() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, MUDKIP, 5, vec![TACKLE]);
    let enemy = max_iv_mon(&dex, ZIGZAGOON, 2, vec![TACKLE, GROWL]);
    let mut rng = SequenceRng::new([0]);
    let battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    assert!(battle.trainer().is_none());
}

/// Beating the rival's only mon must end the battle in victory, with the
/// `x1.5` trainer experience bonus and the prize money, in
/// `Cmd_getexp`-then-`Cmd_getmoneyreward` order.
#[test]
fn beating_the_last_party_mon_pays_boosted_exp_then_money_then_ends_the_battle() {
    let dex = Dex::new();
    // A level-50 Rattata one-shots a level-5 Treecko with Slash and easily
    // outspeeds it, so the rival's chosen action never executes.
    let player = max_iv_mon(&dex, 19, 50, vec![SLASH]);

    // Battle::new_trainer: 1 turn number.
    // take_turn: 1 turn number, 2-5 simulatedRNG, 6 AI_CV_DefenseDown is
    // skipped (healthy user, default Defense stage) so the next draw is the
    // tie-break, 7 accuracy, 8 crit, 9 damage roll, 10 effect chance.
    let mut rng = SequenceRng::new([0; 16]);
    let mut battle = Battle::new_trainer(
        dex,
        player,
        MAY_ROUTE_103_MUDKIP,
        rival_treecko(&Dex::new()),
        &mut rng,
    )
    .unwrap();

    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    let tail: Vec<&BattleEvent> = events
        .iter()
        .filter(|e| {
            matches!(
                e,
                BattleEvent::Fainted { .. }
                    | BattleEvent::ExpGained(_)
                    | BattleEvent::MoneyGained(_)
                    | BattleEvent::Ended(_)
            )
        })
        .collect();
    // Treecko's expYield is 65: 65*5/7 = 46, then the trainer bonus
    // 46*150/100 = 69.
    assert_eq!(
        tail,
        vec![
            &BattleEvent::Fainted { by_player: false },
            &BattleEvent::ExpGained(69),
            &BattleEvent::MoneyGained(300),
            &BattleEvent::Ended(BattleOutcome::PlayerWon),
        ]
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
}

/// The same KO against a *wild* Treecko pays the unboosted award — the pin
/// that proves the `x1.5` really came from `BATTLE_TYPE_TRAINER` and not
/// from the species/level.
#[test]
fn the_same_knockout_in_a_wild_battle_pays_the_unboosted_award() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, 19, 50, vec![SLASH]);
    let enemy = max_iv_mon(&dex, TREECKO, 5, vec![POUND, LEER]);

    let mut rng = SequenceRng::new([0; 16]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(
        events.contains(&BattleEvent::ExpGained(46)),
        "a wild KO pays expYield * level / 7 with no bonus: {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, BattleEvent::MoneyGained(_))),
        "a wild battle pays no prize money"
    );
}

/// The forced post-faint send-out: party order, at the *end* of the turn,
/// and the battle carries on.
#[test]
fn a_fainted_trainer_mon_is_replaced_by_the_next_one_in_party_order() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, 19, 50, vec![SLASH]);
    // A three-mon party. Route 103's is one mon, so this is a synthetic
    // party against a real trainer id -- the send-out *rule* is upstream's
    // regardless of who is fielding it.
    let party = vec![
        max_iv_mon(&dex, TREECKO, 5, vec![POUND, LEER]),
        max_iv_mon(&dex, TORCHIC, 5, vec![SCRATCH, GROWL]),
        max_iv_mon(&dex, MUDKIP, 5, vec![TACKLE, GROWL]),
    ];

    let mut rng = SequenceRng::new([0; 64]);
    let mut battle =
        Battle::new_trainer(dex, player, MAY_ROUTE_103_MUDKIP, party, &mut rng).unwrap();
    assert_eq!(battle.trainer().unwrap().bench_len(), 2);

    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(
        events.contains(&BattleEvent::TrainerSentOut {
            species: SpeciesId(TORCHIC),
            bench_remaining: 1,
        }),
        "the second party member comes out, not the third: {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, BattleEvent::Ended(_) | BattleEvent::MoneyGained(_))),
        "the battle is not over while the trainer still has mons"
    );
    assert_eq!(battle.outcome(), None);
    assert_eq!(battle.enemy().species(), SpeciesId(TORCHIC));
    assert!(
        !battle.enemy().is_fainted()
            && battle.enemy().current_hp() == battle.enemy().stats().max_hp,
        "the replacement comes out at full HP"
    );

    // Second KO: the third mon comes out.
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(events.contains(&BattleEvent::TrainerSentOut {
        species: SpeciesId(MUDKIP),
        bench_remaining: 0,
    }));

    // Third KO: bench empty, so this one ends the battle and pays out.
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(!events
        .iter()
        .any(|e| matches!(e, BattleEvent::TrainerSentOut { .. })));
    assert!(events.contains(&BattleEvent::MoneyGained(300)));
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
}

/// EXP is applied to the owned player before trainer continuation, so a
/// replacement fights the levelled-up mon rather than its stale snapshot.
#[test]
fn a_level_crossed_before_replacement_updates_the_next_turns_combat() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, TREECKO, 5, vec![SLASH]);
    let old_player = player.clone();
    let mut lead = max_iv_mon(&dex, MUDKIP, 5, vec![TACKLE]);
    lead.apply_damage(lead.current_hp() - 1);
    let party = vec![lead, max_iv_mon(&dex, PICHU, 5, vec![TACKLE])];

    let mut rng = SequenceRng::new([u16::MAX; 128]);
    let mut battle =
        Battle::new_trainer(dex, player, MAY_ROUTE_103_MUDKIP, party, &mut rng).unwrap();
    let old_stats = battle.player().stats();

    let first = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    let exp_index = first
        .iter()
        .position(|event| matches!(event, BattleEvent::ExpGained(_)))
        .expect("the faint awards EXP");
    let send_out_index = first
        .iter()
        .position(|event| matches!(event, BattleEvent::TrainerSentOut { .. }))
        .expect("the trainer sends out the replacement");
    assert!(
        exp_index < send_out_index,
        "EXP precedes send-out: {first:?}"
    );
    assert_eq!(battle.player().level(), 6);
    assert_ne!(battle.player().stats(), old_stats);
    assert_eq!(battle.enemy().species(), SpeciesId(PICHU));

    let expected_damage = |attacker: &BattlePokemon| {
        let mut hit_rng = SequenceRng::new([u16::MAX; 4]);
        match battle::hit::resolve_hit(
            &Dex::new(),
            SLASH,
            attacker,
            battle.enemy(),
            false,
            &mut hit_rng,
        )
        .unwrap()
        {
            HitOutcome::Hit { damage, .. } => damage,
            other => panic!("the deterministic Slash should hit: {other:?}"),
        }
    };
    let updated_damage = expected_damage(battle.player());
    let stale_damage = expected_damage(&old_player);
    assert_ne!(
        updated_damage, stale_damage,
        "the level-up must affect damage"
    );

    let second = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(
        second.contains(&BattleEvent::Hit {
            by_player: true,
            move_id: SLASH,
            damage: updated_damage,
            is_critical: false,
        }),
        "the next turn uses the updated level and stats: {second:?}"
    );
}

/// Every knocked-out party member pays its own boosted award, so the exp a
/// multi-mon trainer hands over is the sum of them — pinned alongside the
/// send-out because the two share the same faint path.
#[test]
fn each_knocked_out_party_member_pays_its_own_boosted_award() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, 19, 50, vec![SLASH]);
    let party = vec![
        max_iv_mon(&dex, TREECKO, 5, vec![POUND, LEER]),
        max_iv_mon(&dex, TORCHIC, 5, vec![SCRATCH, GROWL]),
    ];
    let mut rng = SequenceRng::new([0; 64]);
    let mut battle =
        Battle::new_trainer(dex, player, MAY_ROUTE_103_MUDKIP, party, &mut rng).unwrap();

    let first = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    // Treecko expYield 65 -> 46 -> 69.
    assert!(first.contains(&BattleEvent::ExpGained(69)), "{first:?}");
    let second = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    // Torchic expYield 65 as well, so the same award -- but it must be paid
    // a *second* time rather than folded into the first.
    assert!(second.contains(&BattleEvent::ExpGained(69)), "{second:?}");
}

/// A replacement must not act on the turn it came out: upstream settles the
/// send-out in `HandleFaintedMonActions`, *after* both battlers' actions.
#[test]
fn a_replacement_does_not_act_on_the_turn_it_is_sent_out() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, 19, 50, vec![SLASH]);
    let party = vec![
        max_iv_mon(&dex, TREECKO, 5, vec![POUND, LEER]),
        max_iv_mon(&dex, TORCHIC, 5, vec![SCRATCH, GROWL]),
    ];
    let mut rng = SequenceRng::new([0; 64]);
    let mut battle =
        Battle::new_trainer(dex, player, MAY_ROUTE_103_MUDKIP, party, &mut rng).unwrap();
    let player_hp_before = battle.player().current_hp();

    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(
        !events.iter().any(|e| matches!(
            e,
            BattleEvent::Hit {
                by_player: false,
                ..
            }
        )),
        "neither the fainted lead nor its replacement may land a hit: {events:?}"
    );
    assert_eq!(
        battle.player().current_hp(),
        player_hp_before,
        "the player must take no damage on the send-out turn"
    );
}

/// `AI_SetupFirstTurn` is the third flag `TRAINER_BRENDAN_ROUTE_103_TREECKO`
/// carries instead of `AI_SCRIPT_CHECK_VIABILITY`. Both trainers must be
/// constructible — the pin that this port reproduces the upstream table's
/// inconsistency rather than normalising it.
#[test]
fn both_route_103_ai_flag_shapes_construct_and_play() {
    for trainer in [MAY_ROUTE_103_MUDKIP, BRENDAN_ROUTE_103_TREECKO] {
        let dex = Dex::new();
        let player = max_iv_mon(&dex, 19, 50, vec![SLASH]);
        let party = vec![max_iv_mon(&dex, TORCHIC, 5, vec![SCRATCH, GROWL])];
        let mut rng = SequenceRng::new([0; 32]);
        let mut battle = Battle::new_trainer(dex, player, trainer, party, &mut rng)
            .unwrap_or_else(|e| panic!("trainer {} must construct: {e}", trainer.0));
        let events = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();
        assert!(events.contains(&BattleEvent::Ended(BattleOutcome::PlayerWon)));
    }
}

/// Losing a trainer battle is the ordinary defeat outcome: no money, no
/// send-out, and the same deferred white-out the wild path documents.
#[test]
fn losing_to_a_trainer_ends_in_the_ordinary_defeat_outcome_with_no_payout() {
    let dex = Dex::new();
    // A level-1 Magikarp (species 129) knowing only Tackle, against a
    // level-100 Treecko: the rival's Pound ends it in one hit.
    let player = max_iv_mon(&dex, 129, 1, vec![TACKLE]);
    let party = vec![max_iv_mon(&dex, TREECKO, 100, vec![POUND, LEER])];

    let mut rng = SequenceRng::new([0; 32]);
    let mut battle =
        Battle::new_trainer(dex, player, MAY_ROUTE_103_MUDKIP, party, &mut rng).unwrap();
    let mut events = Vec::new();
    for _ in 0..8 {
        if battle.outcome().is_some() {
            break;
        }
        events.extend(
            battle
                .take_turn(PlayerAction::UseMove(0), &mut rng)
                .unwrap(),
        );
    }
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerLost));
    assert!(events.contains(&BattleEvent::Fainted { by_player: true }));
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, BattleEvent::MoneyGained(_))),
        "a loss pays nothing"
    );
    assert_eq!(
        events.last(),
        Some(&BattleEvent::Ended(BattleOutcome::PlayerLost))
    );
}

/// The three construction screens `Battle::new_trainer` runs ahead of its
/// first draw, and the fact that a rejection leaves the shared stream
/// untouched.
///
/// Slash is the pin that the AI screen is genuinely *narrower* than the
/// execution screen rather than a duplicate of it: `EFFECT_HIGH_CRITICAL` is
/// an ordinary hit script the turn engine runs happily
/// ([`battle::is_ordinary_hit_effect`]), but `AI_CheckViability` routes it
/// to `AI_CV_HighCrit` (`data/battle_ai_scripts.s:1449`), a branch this slice
/// does not model — and one that draws, so admitting it would desynchronise
/// the shared stream rather than merely mis-score.
#[test]
fn an_unscoreable_party_moveset_is_rejected_before_any_draw() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, MUDKIP, 5, vec![TACKLE]);
    assert!(
        battle::is_ordinary_hit_effect(dex.move_data(SLASH).unwrap().effect),
        "Slash must be executable, or this test proves nothing"
    );
    let party = vec![max_iv_mon(&dex, TORCHIC, 34, vec![SCRATCH, SLASH])];

    let mut rng = SequenceRng::new([]);
    let error =
        Battle::new_trainer(dex, player, MAY_ROUTE_103_MUDKIP, party, &mut rng).unwrap_err();
    assert_eq!(error, BattleError::UnscoreableMoveEffect(SLASH));
}

/// A trainer whose `aiFlags` set a script this slice does not run is refused
/// outright rather than silently playing a *different* AI.
/// `TRAINER_WINONA_1` (`include/constants/opponents.h:274`) carries
/// `AI_SCRIPT_RISKY` on top of the three Route 103 scripts
/// (`src/data/trainers.h:3252`).
#[test]
fn a_trainer_with_an_unmodelled_ai_script_is_rejected_before_any_draw() {
    const WINONA_1: TrainerId = TrainerId(270);
    let dex = Dex::new();
    let player = max_iv_mon(&dex, MUDKIP, 5, vec![TACKLE]);
    let mut rng = SequenceRng::new([]);
    let error = Battle::new_trainer(dex, player, WINONA_1, rival_treecko(&Dex::new()), &mut rng)
        .unwrap_err();
    assert_eq!(
        error,
        BattleError::UnsupportedAiFlags(assets::trainers::AiFlags::RISKY),
        "the error names only the unmodelled bit"
    );
}

#[test]
fn an_empty_party_or_unknown_trainer_is_rejected_before_any_draw() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, MUDKIP, 5, vec![TACKLE]);
    let mut rng = SequenceRng::new([]);
    assert_eq!(
        Battle::new_trainer(
            dex.clone(),
            player.clone(),
            MAY_ROUTE_103_MUDKIP,
            Vec::new(),
            &mut rng
        )
        .unwrap_err(),
        BattleError::EmptyTrainerParty(MAY_ROUTE_103_MUDKIP)
    );
    assert_eq!(
        Battle::new_trainer(
            dex,
            player,
            TrainerId(60_000),
            rival_treecko(&Dex::new()),
            &mut rng
        )
        .unwrap_err(),
        BattleError::UnknownTrainer(TrainerId(60_000))
    );
}
