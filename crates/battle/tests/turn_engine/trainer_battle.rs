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
/// `SPECIES_MACHOP` (`include/constants/species.h:66`) -- the player lead
/// every "overwhelm the rival" test below uses. Single-ability
/// (`ABILITY_GUTS`, `abilities[1]` is `ABILITY_NONE`), which is what keeps
/// it a target `Cmd_get_ability` answers without the `Random() & 1` guess
/// this crate refuses to model (`BattleError::AmbiguousTargetAbility`).
const MACHOP: u16 = 66;
const PICHU: u16 = 172;

const POUND: MoveId = MoveId(1);
const SCRATCH: MoveId = MoveId(10);
const TACKLE: MoveId = MoveId(33);
const LEER: MoveId = MoveId(43);
const GROWL: MoveId = MoveId(45);
const ABSORB: MoveId = MoveId(71);
/// `MOVE_PECK` (`include/constants/moves.h:68`) — Torchic's level-16
/// learnset entry.
const PECK: MoveId = MoveId(64);
const SAND_ATTACK: MoveId = MoveId(28);
const FIRE_SPIN: MoveId = MoveId(83);
const QUICK_ATTACK: MoveId = MoveId(98);
const SLASH: MoveId = MoveId(163);
const PURSUIT: MoveId = MoveId(228);

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
    // A level-50 Machop one-shots a level-5 Treecko with Slash and easily
    // outspeeds it, so the rival's chosen action never executes.
    let player = max_iv_mon(&dex, MACHOP, 50, vec![SLASH]);

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
    let player = max_iv_mon(&dex, MACHOP, 50, vec![SLASH]);
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
    let player = max_iv_mon(&dex, MACHOP, 50, vec![SLASH]);
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

/// Level-up move learning is **unscreened**, exactly as upstream's
/// `GiveMoveToMon` teaches (issue #252): a level-6 Treecko learns Absorb
/// even though `EFFECT_ABSORB` has no resolver in this crate
/// ([`battle::hit`]'s module docs). This is the successor to the old
/// `a_crossed_level_does_not_learn_the_learnset_move_yet` deferral pin,
/// flipped to the upstream behaviour it deferred.
///
/// The fail-closed half that survives is the *other* one this crate always
/// had, and this test pins both halves together so neither can drift: the
/// unexecutable move sits in the player's moveset — which
/// [`Battle::new`]/[`Battle::new_trainer`] deliberately do not screen, only
/// the opposing side's — and is refused when it is **selected**, by
/// `validate_player_move`, ahead of the turn's first RNG draw, with a
/// recoverable error that leaves the battle usable.
#[test]
fn a_crossed_level_learns_an_unexecutable_move_that_selection_then_refuses() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, TREECKO, 5, vec![SLASH]);
    // Level 16, crossing three learnset entries at once: Absorb (6), Quick
    // Attack (11) and Pursuit (16). The specimen moved here from Absorb
    // alone when issue #293 made `EFFECT_ABSORB` executable -- the *screen*
    // is what this test pins, so the ratchet is to re-aim it at a move the
    // engine still cannot run rather than to relax the assertion.
    let level_16 =
        assets::experience_for_level(dex.species(SpeciesId(TREECKO)).unwrap().growth_rate, 16)
            .unwrap();

    player.apply_experience(&dex, level_16 - player.experience());

    assert_eq!(player.level(), 16, "three thresholds were crossed");
    assert_eq!(player.experience(), level_16);
    assert_eq!(
        player
            .moves()
            .iter()
            .map(|slot| slot.move_id)
            .collect::<Vec<_>>(),
        vec![SLASH, ABSORB, QUICK_ATTACK, PURSUIT],
        "all three are taught into the empty slots with no effect-coverage \
         screen, exactly as upstream's GiveMoveToMon hands them out"
    );
    assert_eq!(
        player.moves()[3].pp,
        dex.move_data(PURSUIT).unwrap().pp,
        "a freshly learned move's PP starts at the move's own base PP"
    );
    // `EFFECT_PURSUIT` points at `BattleScript_EffectHit` in the table but
    // the engine re-targets and re-powers it outside the script
    // (`battle_script_commands.c:8745`/`:9854`), which is exactly why
    // `crate::hit`'s allow-list leaves it out.
    assert_eq!(
        battle::ensure_resolvable(&dex, ABSORB),
        Err(BattleError::UnsupportedMoveEffect(ABSORB)),
        "Absorb is executable now, but through `crate::drain` -- the plain \
         hit pipeline still refuses it, one draw short as it is"
    );

    // The fail-closed half: unexecutable *in the player's moveset* is fine;
    // unexecutable *as this turn's pick* is refused, before any draw.
    let mut rng = SequenceRng::new([u16::MAX; 128]);
    let mut battle = Battle::new_trainer(
        dex,
        player,
        MAY_ROUTE_103_MUDKIP,
        rival_treecko(&Dex::new()),
        &mut rng,
    )
    .expect("construction never screens the player's moveset");
    let draws_before = rng.draws();

    let failure = battle
        .take_turn(PlayerAction::UseMove(3), &mut rng)
        .unwrap_err();
    assert_eq!(
        failure.error(),
        BattleError::UnsupportedMoveEffect(PURSUIT),
        "EFFECT_PURSUIT has no resolver, so selecting it is refused -- \
         pinning *why*, so this breaks loudly the day EFFECT_PURSUIT lands"
    );
    assert!(
        failure.events().is_empty(),
        "a pre-draw rejection reports no events"
    );
    assert_eq!(
        rng.draws(),
        draws_before,
        "validate_player_move runs ahead of the turn-number draw -- the \
         shared stream must not move at all"
    );
    assert!(battle.outcome().is_none(), "the refusal is recoverable");
    // Another action can still be chosen this turn -- and the one chosen is
    // Absorb, taught by the very same level-up run, so this also pins that
    // the refusal above is about Pursuit specifically rather than about
    // learned moves in general (Absorb executes through `crate::drain`).
    assert!(
        battle.take_turn(PlayerAction::UseMove(1), &mut rng).is_ok(),
        "the freshly learned Absorb is selectable and executes"
    );
}

/// A single crossed level learns that level's move into the first empty
/// slot — the ordinary case `MonTryLearningNewMove`/`GiveMoveToMon` models
/// (`pokeemerald/src/pokemon.c:3014`-`:3044`, `:2934`-`:2955`).
#[test]
fn a_single_crossed_level_learns_its_learnset_move() {
    let dex = Dex::new();
    let mut mon = max_iv_mon(&dex, TORCHIC, 15, vec![SCRATCH, GROWL]);
    let level_16 =
        assets::experience_for_level(dex.species(SpeciesId(TORCHIC)).unwrap().growth_rate, 16)
            .unwrap();

    mon.apply_experience(&dex, level_16 - mon.experience());

    assert_eq!(mon.level(), 16, "exactly one level crossed");
    assert_eq!(
        mon.moves()
            .iter()
            .map(|slot| slot.move_id)
            .collect::<Vec<_>>(),
        vec![SCRATCH, GROWL, PECK],
        "Peck (Torchic's level-16 entry) lands in the first empty slot"
    );
    assert_eq!(
        mon.moves()[2].pp,
        dex.move_data(PECK).unwrap().pp,
        "a freshly learned move's PP starts at the move's own base PP"
    );
}

/// `GiveMoveToBoxMon`'s `MON_ALREADY_KNOWS_MOVE` branch
/// (`pokemon.c:2951`-`:2952`): a mon that already knows the crossed
/// level's learnset move neither duplicates it nor spends a slot on it.
#[test]
fn a_crossed_levels_already_known_move_is_skipped_at_no_slot_cost() {
    let dex = Dex::new();
    let mut mon = max_iv_mon(&dex, TORCHIC, 15, vec![SCRATCH, PECK]);
    let level_16 =
        assets::experience_for_level(dex.species(SpeciesId(TORCHIC)).unwrap().growth_rate, 16)
            .unwrap();

    mon.apply_experience(&dex, level_16 - mon.experience());

    assert_eq!(mon.level(), 16, "exactly one level crossed");
    assert_eq!(
        mon.moves()
            .iter()
            .map(|slot| slot.move_id)
            .collect::<Vec<_>>(),
        vec![SCRATCH, PECK],
        "level 16's Peck is already known -- no duplicate, no slot spent"
    );
}

/// A multi-level jump processes every crossed level in ascending order,
/// exactly as upstream's own one-level-at-a-time `Cmd_getexp` loop does
/// (`battle_script_commands.c` case 3 → case 4 → case 5, looping back to
/// case 3 until the whole award is spent). Torchic crosses four learnset
/// levels here — 16 Peck, 19 Sand Attack, 25 Fire Spin, 28 Quick Attack —
/// with one slot already taken, so the first three land *in learnset order*
/// (no skips: Sand Attack and Fire Spin are not executable by this crate's
/// turn engine and are taught anyway) and the fourth runs out of slots and
/// is declined.
#[test]
fn a_multi_level_jump_learns_each_crossed_levels_moves_in_order() {
    let dex = Dex::new();
    let mut mon = max_iv_mon(&dex, TORCHIC, 13, vec![SCRATCH]);
    let growth_rate = dex.species(SpeciesId(TORCHIC)).unwrap().growth_rate;
    let level_29 = assets::experience_for_level(growth_rate, 29).unwrap();

    mon.apply_experience(&dex, level_29 - mon.experience());

    assert_eq!(mon.level(), 29, "many levels crossed in a single call");
    let learned: Vec<MoveId> = mon.moves().iter().map(|slot| slot.move_id).collect();
    assert_eq!(
        learned,
        vec![SCRATCH, PECK, SAND_ATTACK, FIRE_SPIN],
        "every crossed level's move lands, in ascending level order, until \
         the slots run out -- nothing is skipped for want of a modelled effect"
    );
    assert!(
        !learned.contains(&QUICK_ATTACK),
        "level 28's Quick Attack is the first entry with no slot left, so \
         it is declined (MON_HAS_MAX_MOVES) rather than bumping a move"
    );
}

/// A full moveset declines the crossed level's move instead of prompting a
/// replacement — the four-known-moves yes/no box
/// (`BattleScript_AskToLearnMove`, `battle_script_commands.c:5368`-`:5370`)
/// is a UI slice out of scope, modelled as the answer a player who chooses
/// "Stop learning?" would give: not learned, moveset unchanged. This is the
/// one recorded divergence on the learn path.
#[test]
fn a_full_moveset_declines_a_learnable_move() {
    let original_moves = vec![SCRATCH, GROWL, TACKLE, LEER];
    let dex = Dex::new();
    let mut mon = max_iv_mon(&dex, TORCHIC, 15, original_moves.clone());
    let level_16 =
        assets::experience_for_level(dex.species(SpeciesId(TORCHIC)).unwrap().growth_rate, 16)
            .unwrap();

    mon.apply_experience(&dex, level_16 - mon.experience());

    assert_eq!(
        mon.level(),
        16,
        "the level still rises even though nothing is learned"
    );
    assert_eq!(
        mon.moves()
            .iter()
            .map(|slot| slot.move_id)
            .collect::<Vec<_>>(),
        original_moves,
        "every slot is already full, so Peck is declined rather than \
         bumping an existing move"
    );
}

/// Every knocked-out party member pays its own boosted award, so the exp a
/// multi-mon trainer hands over is the sum of them — pinned alongside the
/// send-out because the two share the same faint path.
#[test]
fn each_knocked_out_party_member_pays_its_own_boosted_award() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, MACHOP, 50, vec![SLASH]);
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
    let player = max_iv_mon(&dex, MACHOP, 50, vec![SLASH]);
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
        let player = max_iv_mon(&dex, MACHOP, 50, vec![SLASH]);
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

/// The screen on the *player's* side of a trainer battle: the AI's target.
///
/// `Cmd_get_ability` (`src/battle_ai_script_commands.c:1350`-`:1405`) answers
/// from `gSpeciesInfo[].abilities` when nothing is recorded in
/// `BATTLE_HISTORY`, and when slot 1 is not `ABILITY_NONE` it picks between
/// the two with `Random() & 1` (`:1383`). `AI_CheckBadMove` walks
/// `get_ability AI_TARGET` on its mainline (`data/battle_ai_scripts.s:93`),
/// so against a two-ability lead that draw would be spent once per scored
/// slot, every turn -- and this crate models no abilities at all, so it can
/// never be the *recorded* branch.
///
/// `SPECIES_RATTATA` is the fixture because it is genuinely two-ability
/// upstream (`ABILITY_RUN_AWAY` / `ABILITY_GUTS`), and `SPECIES_MACHOP`
/// (`ABILITY_GUTS` alone) is the control that proves the refusal is the
/// second ability and not the screen refusing everything.
#[test]
fn a_two_ability_player_lead_is_refused_before_any_draw() {
    const RATTATA: u16 = 19;
    let dex = Dex::new();
    let mut rng = SequenceRng::new([]);
    let error = Battle::new_trainer(
        dex,
        max_iv_mon(&Dex::new(), RATTATA, 50, vec![SLASH]),
        MAY_ROUTE_103_MUDKIP,
        rival_treecko(&Dex::new()),
        &mut rng,
    )
    .unwrap_err();
    assert_eq!(
        error,
        BattleError::AmbiguousTargetAbility(SpeciesId(RATTATA)),
        "the error names the species, which is what a future ability slice \
         has to make answerable"
    );
    assert_eq!(rng.draws(), 0, "an empty script: the refusal drew nothing");

    // The control: the same battle with a single-ability lead constructs.
    let mut rng = SequenceRng::new([0; 4]);
    Battle::new_trainer(
        Dex::new(),
        max_iv_mon(&Dex::new(), MACHOP, 50, vec![SLASH]),
        MAY_ROUTE_103_MUDKIP,
        rival_treecko(&Dex::new()),
        &mut rng,
    )
    .expect("a single-ability lead is a target `Cmd_get_ability` never guesses about");
}

/// ...and the same refusal comes out of the no-draw pre-flight, which is
/// where a per-frame caller actually meets it: `ensure_trainer_party_startable`
/// takes the lead as an argument precisely so this cannot be a screen a
/// caller forgets to run (issue #293 review).
#[test]
fn the_pre_flight_refuses_the_same_two_ability_lead() {
    const RATTATA: u16 = 19;
    let dex = Dex::new();
    let moves = [POUND];
    let party = [battle::TrainerPartyMon {
        species: SpeciesId(TREECKO),
        level: 5,
        moves: &moves,
        held_item: assets::items::ItemId::NONE,
    }];
    assert_eq!(
        battle::ensure_trainer_party_startable(
            &dex,
            MAY_ROUTE_103_MUDKIP,
            &max_iv_mon(&Dex::new(), RATTATA, 50, vec![SLASH]),
            &party,
        ),
        Err(BattleError::AmbiguousTargetAbility(SpeciesId(RATTATA)))
    );
    assert_eq!(
        battle::ensure_trainer_party_startable(
            &dex,
            MAY_ROUTE_103_MUDKIP,
            &max_iv_mon(&Dex::new(), MACHOP, 50, vec![SLASH]),
            &party,
        ),
        Ok(())
    );
}
