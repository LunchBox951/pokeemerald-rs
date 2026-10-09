//! `BATTLE_TYPE_TRAINER`: the scripted Route 103 rival battle's five deltas
//! from a wild encounter — running refused, a party opponent, the forced
//! post-faint send-out, `x1.5` experience, and prize money — pinned end to
//! end through the public `battle` API. Per-script AI draw accounting is
//! pinned next to the AI itself (`crates/battle/src/battle/trainer_ai.rs`);
//! construction is pinned in
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
//! Two-mon parties do not exist on Route 103, so the forced-replacement
//! fixtures below use hand-built parties instead — pinning behaviour only
//! against a one-mon party would pin nothing. A forced post-faint send-out
//! runs `GetMostSuitableMonToSwitchInto`'s type/damage selector before
//! falling back to party order; see `TrainerContext::send_out_next`'s docs.

use crate::common::{max_iv_mon, script, SequenceRng};
use assets::trainers::TrainerId;
use assets::{MoveId, SpeciesId};
use battle::status1::poison_residual_damage;

// Exact draw groups, each scripted 0 and labelled with the upstream call site
// that consumes it. A fixture's script is the concatenation of its groups, so
// an extra or missing draw changes `rng.draws()` and fails the final
// `assert_exhausted`.
const CONSTRUCTION_SEED: &[u16] = &[0]; // battle_main.c:3140
const TURN_SEED: &[u16] = &[0]; // battle_main.c:3923 / :4013
const AI_SIMULATED_DAMAGE: &[u16] = &[0; 4]; // battle_ai_script_commands.c:341, one per slot
const DAMAGING_MOVE_ABILITY_GUESSES: &[u16] = &[0; 2]; // battle_ai_scripts.s:59, :93
const LEER_ABILITY_GUESSES: &[u16] = &[0; 2]; // battle_ai_scripts.s:93, :309
const GROWL_ABILITY_GUESSES: &[u16] = &[0; 3]; // battle_ai_scripts.s:93, :279, :309
const GROWL_VIABILITY_ROLL: &[u16] = &[0]; // battle_ai_scripts.s:1107
const SETUP_FIRST_TURN_ROLL: &[u16] = &[0]; // battle_ai_scripts.s:2644
const TRAINER_MOVE_PICK: &[u16] = &[0]; // battle_ai_script_commands.c:445
const WILD_MOVE_SLOT_PICK: &[u16] = &[0]; // battle_controller_opponent.c:1599
const ACCURACY_ONLY: &[u16] = &[0]; // battle_script_commands.c:1176
                                    // battle_script_commands.c accuracy :1176, crit :1282, variance :1641,
                                    // secondary chance :2923 (drawn even for moves with no secondary effect).
const ACC_CRIT_VARIANCE_SECONDARY: &[u16] = &[0; 4];
// Struggle skips the secondary-chance draw.
const ACC_CRIT_VARIANCE: &[u16] = &[0; 3];
use battle::{
    Battle, BattleError, BattleEvent, BattleOutcome, BattlePokemon, Dex, HitOutcome,
    MoveLearnDecision, PlayerAction, PpBonuses, Status1,
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
/// `SPECIES_RATTATA`: paired with Slash and level 50 in these fixtures, it
/// one-shots and outspeeds any level-5 party member, so the rival's own
/// action never executes.
const RATTATA: u16 = 19;
/// `SPECIES_CHANSEY`: pure Normal, and slower than [`KANGASKHAN`].
const CHANSEY: u16 = 113;
/// `SPECIES_KANGASKHAN`: pure Normal, fast enough to act ahead of
/// [`CHANSEY`].
const KANGASKHAN: u16 = 115;

const POUND: MoveId = MoveId::POUND;
const SCRATCH: MoveId = MoveId::SCRATCH;
const TACKLE: MoveId = MoveId::TACKLE;
const LEER: MoveId = MoveId::LEER;
const GROWL: MoveId = MoveId::GROWL;
const ABSORB: MoveId = MoveId::ABSORB;
/// `MOVE_PURSUIT`, Treecko's level-16 learnset move: `EFFECT_PURSUIT` has
/// no resolver, so `battle::hit`'s allow-list refuses it (see its module
/// docs).
const PURSUIT: MoveId = MoveId::PURSUIT;
/// `MOVE_PECK` (`include/constants/moves.h:68`) — Torchic's level-16
/// learnset entry.
const PECK: MoveId = MoveId::PECK;
const SAND_ATTACK: MoveId = MoveId::SAND_ATTACK;
const FIRE_SPIN: MoveId = MoveId::FIRE_SPIN;
const QUICK_ATTACK: MoveId = MoveId::QUICK_ATTACK;
const SLASH: MoveId = MoveId::SLASH;
/// `MOVE_MEGA_KICK` (`include/constants/moves.h:25`): a plain-hit Normal
/// move with far more power than Tackle's.
const MEGA_KICK: MoveId = MoveId::MEGA_KICK;
/// `MOVE_WATER_GUN` (`include/constants/moves.h:59`).
const WATER_GUN: MoveId = MoveId::WATER_GUN;

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

/// Upstream runs `Cmd_getexp` before `Cmd_getmoneyreward`.
#[test]
fn beating_the_last_party_mon_pays_boosted_exp_then_money_then_ends_the_battle() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 50, vec![SLASH]);
    let mut rng = SequenceRng::new(script(&[
        CONSTRUCTION_SEED,             // battle_main.c:3140 construction turn seed
        TURN_SEED,                     // battle_main.c:3923/:4013 turn-start seed
        AI_SIMULATED_DAMAGE,           // battle_ai_script_commands.c:341, slots 0..3
        DAMAGING_MOVE_ABILITY_GUESSES, // battle_ai_scripts.s:59/:93 CheckBadMove guesses (battle_ai_script_commands.c:1383)
        LEER_ABILITY_GUESSES, // battle_ai_scripts.s:93/:309 Soundproof, Clear Body/White Smoke guesses
        TRAINER_MOVE_PICK,    // battle_ai_script_commands.c:445 highest-score pick
        ACC_CRIT_VARIANCE_SECONDARY,
    ]));
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
    assert_eq!(rng.draws(), 15, "exact scripted draw stream is consumed");
    rng.assert_exhausted();
}

/// The same KO against a *wild* Treecko pays the unboosted award — the pin
/// that proves the `x1.5` really came from `BATTLE_TYPE_TRAINER` and not
/// from the species/level.
#[test]
fn the_same_knockout_in_a_wild_battle_pays_the_unboosted_award() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 50, vec![SLASH]);
    let enemy = max_iv_mon(&dex, TREECKO, 5, vec![POUND, LEER]);

    let mut rng = SequenceRng::new(script(&[
        CONSTRUCTION_SEED,   // battle_main.c:3140 construction turn seed
        TURN_SEED,           // battle_main.c:3923/:4013 turn-start seed
        WILD_MOVE_SLOT_PICK, // battle_controller_opponent.c:1599 move-slot pick
        ACC_CRIT_VARIANCE_SECONDARY,
    ]));
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
    assert_eq!(rng.draws(), 7, "exact scripted draw stream is consumed");
    rng.assert_exhausted();
}

/// Coincidence, not a party-order rule: nothing is ever super effective
/// against a pure Normal-type player, so this fixture always reaches the
/// damage fallback. Scratch and Tackle are both Normal, so they score
/// identically there too, and the tie-break keeps the earlier bench member,
/// Torchic.
#[test]
fn a_fainted_trainer_mon_is_replaced_by_the_next_one_in_party_order() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 50, vec![SLASH]);
    let party = vec![
        max_iv_mon(&dex, TREECKO, 5, vec![POUND, LEER]),
        max_iv_mon(&dex, TORCHIC, 5, vec![SCRATCH, GROWL]),
        max_iv_mon(&dex, MUDKIP, 5, vec![TACKLE, GROWL]),
    ];

    let mut rng = SequenceRng::new(script(&[
        CONSTRUCTION_SEED,             // battle_main.c:3140 construction turn seed
        TURN_SEED,                     // battle_main.c:3923/:4013 turn-start seed
        AI_SIMULATED_DAMAGE,           // battle_ai_script_commands.c:341, slots 0..3
        DAMAGING_MOVE_ABILITY_GUESSES, // battle_ai_scripts.s:59/:93 CheckBadMove guesses (battle_ai_script_commands.c:1383)
        LEER_ABILITY_GUESSES, // battle_ai_scripts.s:93/:309 Soundproof, Clear Body/White Smoke guesses
        TRAINER_MOVE_PICK,    // battle_ai_script_commands.c:445 highest-score pick
        ACC_CRIT_VARIANCE_SECONDARY,
        TURN_SEED,                     // battle_main.c:3923/:4013 turn-start seed
        AI_SIMULATED_DAMAGE,           // battle_ai_script_commands.c:341, slots 0..3
        DAMAGING_MOVE_ABILITY_GUESSES, // battle_ai_scripts.s:59/:93 CheckBadMove guesses (battle_ai_script_commands.c:1383)
        GROWL_ABILITY_GUESSES, // battle_ai_scripts.s:93/:279/:309 Soundproof, Hyper Cutter, Clear Body/White Smoke guesses
        TRAINER_MOVE_PICK,     // battle_ai_script_commands.c:445 highest-score pick
        ACC_CRIT_VARIANCE_SECONDARY,
        TURN_SEED,                     // battle_main.c:3923/:4013 turn-start seed
        AI_SIMULATED_DAMAGE,           // battle_ai_script_commands.c:341, slots 0..3
        DAMAGING_MOVE_ABILITY_GUESSES, // battle_ai_scripts.s:59/:93 CheckBadMove guesses (battle_ai_script_commands.c:1383)
        GROWL_ABILITY_GUESSES, // battle_ai_scripts.s:93/:279/:309 Soundproof, Hyper Cutter, Clear Body/White Smoke guesses
        TRAINER_MOVE_PICK,     // battle_ai_script_commands.c:445 highest-score pick
        ACC_CRIT_VARIANCE_SECONDARY,
    ]));
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

    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(
        events.contains(&BattleEvent::TrainerSentOut {
            species: SpeciesId(MUDKIP),
            bench_remaining: 0,
        }),
        "the third party member comes out: {events:?}"
    );

    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, BattleEvent::TrainerSentOut { .. })),
        "the bench is empty, so no replacement is sent out: {events:?}"
    );
    assert!(
        events.contains(&BattleEvent::MoneyGained(300)),
        "the bench-empty knockout pays out and ends the battle"
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
    assert_eq!(rng.draws(), 45, "exact scripted draw stream is consumed");
    rng.assert_exhausted();
}

/// Fire-type player: the type/super-effective pass picks the Grass member
/// for worst typing, rejects it for lacking a super-effective move, then
/// picks the Water member with Water Gun instead of the party-order Grass
/// member (`pokeemerald/src/battle_ai_switch_items.c:690`-`:738`).
#[test]
fn a_fainted_trainer_mon_is_replaced_by_the_most_suitable_bench_member() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, TORCHIC, 50, vec![SLASH]);
    let party = vec![
        max_iv_mon(&dex, ZIGZAGOON, 5, vec![TACKLE, GROWL]),
        max_iv_mon(&dex, TREECKO, 5, vec![POUND, LEER]),
        max_iv_mon(&dex, MUDKIP, 5, vec![WATER_GUN, TACKLE]),
    ];

    let mut rng = SequenceRng::new(script(&[
        CONSTRUCTION_SEED,    // battle_main.c:3140 construction turn seed
        TURN_SEED,            // battle_main.c:3923/:4013 turn-start seed
        AI_SIMULATED_DAMAGE,  // battle_ai_script_commands.c:341, slots 0..3
        GROWL_VIABILITY_ROLL, // battle_ai_scripts.s:1107 non-physical target discourage roll
        TRAINER_MOVE_PICK,    // battle_ai_script_commands.c:445 highest-score pick
        ACC_CRIT_VARIANCE_SECONDARY,
    ]));
    let mut battle =
        Battle::new_trainer(dex, player, MAY_ROUTE_103_MUDKIP, party, &mut rng).unwrap();

    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(
        events.contains(&BattleEvent::TrainerSentOut {
            species: SpeciesId(MUDKIP),
            bench_remaining: 1,
        }),
        "the super-effective Mudkip comes out, not the party-order Treecko: {events:?}"
    );
    assert_eq!(battle.enemy().species(), SpeciesId(MUDKIP));
    assert_eq!(rng.draws(), 12, "exact scripted draw stream is consumed");
    rng.assert_exhausted();
}

/// Proves the most-damage fallback independently of party order: a pure
/// Normal-type player is never hit super effectively, so the type pass
/// declines both bench members. Upstream's most-damage pass then scores
/// every candidate off the *same* base damage (the fainted Zigzagoon's
/// stats against a stale move, `pokeemerald/src/battle_ai_switch_items.c:772`-`:779`),
/// so only each candidate's own move's STAB and type effectiveness can
/// still differ the outcome: Water Gun earns no STAB from a Normal-type
/// Zigzagoon, but Tackle does, so the party-order-second Pichu is sent out
/// over the party-order-first Mudkip.
#[test]
fn a_fainted_trainer_mon_is_replaced_by_the_stab_boosted_bench_member_out_of_party_order() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 50, vec![SLASH]);
    let party = vec![
        max_iv_mon(&dex, ZIGZAGOON, 5, vec![TACKLE]),
        max_iv_mon(&dex, MUDKIP, 5, vec![WATER_GUN]),
        max_iv_mon(&dex, PICHU, 5, vec![TACKLE]),
    ];

    let mut rng = SequenceRng::new(script(&[
        CONSTRUCTION_SEED,             // battle_main.c:3140 construction turn seed
        TURN_SEED,                     // battle_main.c:3923/:4013 turn-start seed
        AI_SIMULATED_DAMAGE,           // battle_ai_script_commands.c:341, slots 0..3
        DAMAGING_MOVE_ABILITY_GUESSES, // battle_ai_scripts.s:59/:93 CheckBadMove guesses (battle_ai_script_commands.c:1383)
        TRAINER_MOVE_PICK,             // battle_ai_script_commands.c:445 highest-score pick
        ACC_CRIT_VARIANCE_SECONDARY,
    ]));
    let mut battle =
        Battle::new_trainer(dex, player, MAY_ROUTE_103_MUDKIP, party, &mut rng).unwrap();

    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(
        events.contains(&BattleEvent::TrainerSentOut {
            species: SpeciesId(PICHU),
            bench_remaining: 1,
        }),
        "Tackle's STAB from the Normal-type Zigzagoon sends out Pichu, not the party-order Mudkip: {events:?}"
    );
    assert_eq!(battle.enemy().species(), SpeciesId(PICHU));
    assert_eq!(rng.draws(), 13, "exact scripted draw stream is consumed");
    rng.assert_exhausted();
}

/// Two Normal moves against the same defender score identically once base
/// damage comes from the one stale move rather than each candidate's own
/// (`pokeemerald/src/battle_ai_switch_items.c:772`-`:779`,
/// `battle_script_commands.c:1306`-`:1311`,`:1536`-`:1552`), so the strict
/// `bestDmg < gBattleMoveDamage` comparison keeps the earlier party member
/// regardless of Mega Kick's vastly higher power.
#[test]
fn tied_move_types_send_out_the_earlier_bench_member_regardless_of_base_power() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 50, vec![SLASH]);
    let party = vec![
        max_iv_mon(&dex, ZIGZAGOON, 5, vec![TACKLE]),
        max_iv_mon(&dex, PICHU, 5, vec![TACKLE]),
        max_iv_mon(&dex, MUDKIP, 5, vec![MEGA_KICK]),
    ];

    let mut rng = SequenceRng::new(script(&[
        CONSTRUCTION_SEED,             // battle_main.c:3140 construction turn seed
        TURN_SEED,                     // battle_main.c:3923/:4013 turn-start seed
        AI_SIMULATED_DAMAGE,           // battle_ai_script_commands.c:341, slots 0..3
        DAMAGING_MOVE_ABILITY_GUESSES, // battle_ai_scripts.s:59/:93 CheckBadMove guesses (battle_ai_script_commands.c:1383)
        TRAINER_MOVE_PICK,             // battle_ai_script_commands.c:445 highest-score pick
        ACC_CRIT_VARIANCE_SECONDARY,
    ]));
    let mut battle =
        Battle::new_trainer(dex, player, MAY_ROUTE_103_MUDKIP, party, &mut rng).unwrap();

    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(
        events.contains(&BattleEvent::TrainerSentOut {
            species: SpeciesId(PICHU),
            bench_remaining: 1,
        }),
        "Mega Kick's power cannot outscore Tackle when both are Normal: {events:?}"
    );
    assert_eq!(battle.enemy().species(), SpeciesId(PICHU));
    assert_eq!(rng.draws(), 13, "exact scripted draw stream is consumed");
    rng.assert_exhausted();
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
        .outcome
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

/// Upstream `GiveMoveToMon` teaches every crossed level's move with no
/// effect-coverage screen, including moves this crate's turn engine cannot
/// yet execute: `EFFECT_PURSUIT` retargets and repowers itself outside the
/// plain-hit script, so it is deliberately absent from [`battle::hit`]'s
/// allow-list despite resolving to `BattleScript_EffectHit`.
#[test]
fn a_crossed_level_learns_an_unexecutable_move_that_selection_then_refuses() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, TREECKO, 5, vec![SLASH]);
    let level_16 =
        assets::experience_for_level(dex.species(SpeciesId(TREECKO)).unwrap().growth_rate, 16)
            .unwrap();

    assert!(
        player
            .apply_experience(&dex, level_16 - player.experience())
            .unwrap()
            .is_none(),
        "an empty slot never asks the player anything"
    );

    assert_eq!(player.level(), 16, "the thresholds were crossed");
    assert_eq!(player.experience(), level_16);
    assert!(
        battle::initial_moveset(SpeciesId(TREECKO), 16).contains(&PURSUIT),
        "fixture sanity: level 16 is the learnset entry that holds Pursuit"
    );
    assert_eq!(
        player
            .moves()
            .iter()
            .map(|slot| slot.move_id)
            .collect::<Vec<_>>(),
        vec![SLASH, ABSORB, QUICK_ATTACK, PURSUIT],
        "each crossed level's move is taught into the next empty slot with \
         no effect-coverage screen, exactly as upstream's GiveMoveToMon \
         hands them out"
    );
    assert_eq!(
        player.moves()[3].pp,
        dex.move_data(PURSUIT).unwrap().pp,
        "a freshly learned move's PP starts at the move's own base PP"
    );

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
    assert!(
        battle.take_turn(PlayerAction::UseMove(0), &mut rng).is_ok(),
        "and another action can still be chosen this turn"
    );
}

#[test]
fn a_single_crossed_level_learns_its_learnset_move() {
    let dex = Dex::new();
    let mut mon = max_iv_mon(&dex, TORCHIC, 15, vec![SCRATCH, GROWL]);
    let level_16 =
        assets::experience_for_level(dex.species(SpeciesId(TORCHIC)).unwrap().growth_rate, 16)
            .unwrap();

    assert!(
        mon.apply_experience(&dex, level_16 - mon.experience())
            .unwrap()
            .is_none(),
        "an empty slot never asks the player anything"
    );

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

#[test]
fn a_crossed_levels_already_known_move_is_skipped_at_no_slot_cost() {
    let dex = Dex::new();
    let mut mon = max_iv_mon(&dex, TORCHIC, 15, vec![SCRATCH, PECK]);
    let level_16 =
        assets::experience_for_level(dex.species(SpeciesId(TORCHIC)).unwrap().growth_rate, 16)
            .unwrap();

    assert!(
        mon.apply_experience(&dex, level_16 - mon.experience())
            .unwrap()
            .is_none(),
        "an already-known move is skipped without asking"
    );

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

/// Ascending order and the per-level cap mirror upstream's one-level-at-a-
/// time `Cmd_getexp` loop, each write capped by `Task_GiveExpToMon`
/// (`battle_controller_player.c:1154`-`:1181`).
#[test]
fn a_multi_level_jump_learns_each_crossed_levels_moves_in_order() {
    let dex = Dex::new();
    let mut mon = max_iv_mon(&dex, TORCHIC, 13, vec![SCRATCH]);
    let growth_rate = dex.species(SpeciesId(TORCHIC)).unwrap().growth_rate;
    let level_28 = assets::experience_for_level(growth_rate, 28).unwrap();
    let level_29 = assets::experience_for_level(growth_rate, 29).unwrap();

    let pending = mon
        .apply_experience(&dex, level_29 - mon.experience())
        .unwrap()
        .expect("the fourth entry has no slot left, so the walk asks");

    assert_eq!(
        mon.level(),
        28,
        "the award pauses at the prompted level; level 29 waits on the answer"
    );
    assert_eq!(
        mon.experience(),
        level_28,
        "consumed exactly up to level 28's threshold (Task_GiveExpToMon's cap)"
    );
    assert_eq!(
        mon.stats().max_hp,
        max_iv_mon(&dex, TORCHIC, 28, vec![SCRATCH]).stats().max_hp,
        "stats are level 28's while the question is open, not level 29's"
    );
    let learned: Vec<MoveId> = mon.moves().iter().map(|slot| slot.move_id).collect();
    assert_eq!(
        learned,
        vec![SCRATCH, PECK, SAND_ATTACK, FIRE_SPIN],
        "every crossed level's move lands, in ascending level order, until \
         the slots run out -- nothing is skipped for want of a modelled effect"
    );
    assert_eq!(
        pending.move_id(),
        QUICK_ATTACK,
        "level 28's Quick Attack is the first entry with no slot left, so \
         it is the one the player is asked about (MON_HAS_MAX_MOVES)"
    );
    assert_eq!(pending.level(), 28);
    assert!(
        !learned.contains(&QUICK_ATTACK),
        "and nothing is bumped until that question is answered"
    );
    assert_eq!(
        mon.resolve_move_learn(&dex, MoveLearnDecision::Decline)
            .unwrap()
            .next,
        None,
        "declining resumes the level-up, which finds no further entry to offer"
    );
    assert_eq!(
        mon.level(),
        29,
        "the answer releases the award's remainder (Cmd_getexp case 5)"
    );
    assert_eq!(mon.experience(), level_29);
}

/// A full moveset routes through upstream's yes/no box rather than silently
/// discarding the move (`BattleScript_AskToLearnMove`,
/// `battle_script_commands.c:5368`-`:5370`).
#[test]
fn a_full_moveset_asks_before_learning_and_honours_either_answer() {
    let original_moves = vec![SCRATCH, GROWL, TACKLE, LEER];
    let dex = Dex::new();
    let mut mon = max_iv_mon(&dex, TORCHIC, 15, original_moves.clone());
    let level_16 =
        assets::experience_for_level(dex.species(SpeciesId(TORCHIC)).unwrap().growth_rate, 16)
            .unwrap();

    let pending = mon
        .apply_experience(&dex, level_16 - mon.experience())
        .unwrap()
        .expect("four filled slots must raise the replacement question");
    assert_eq!(pending.move_id(), PECK);

    assert_eq!(
        mon.level(),
        16,
        "the level still rises while the question is open"
    );
    assert_eq!(
        mon.moves()
            .iter()
            .map(|slot| slot.move_id)
            .collect::<Vec<_>>(),
        original_moves,
        "and nothing about the moveset moves until it is answered"
    );

    let mut declined = mon.clone();
    assert!(declined
        .resolve_move_learn(&dex, MoveLearnDecision::Decline)
        .unwrap()
        .learned
        .is_none());
    assert_eq!(
        declined
            .moves()
            .iter()
            .map(|slot| slot.move_id)
            .collect::<Vec<_>>(),
        original_moves
    );

    mon.resolve_move_learn(&dex, MoveLearnDecision::Replace(2))
        .unwrap();
    assert_eq!(
        mon.moves()
            .iter()
            .map(|slot| slot.move_id)
            .collect::<Vec<_>>(),
        vec![SCRATCH, GROWL, PECK, LEER],
        "TACKLE was the slot named, so TACKLE is the move forgotten"
    );
    assert_eq!(mon.moves()[2].pp, dex.move_data(PECK).unwrap().pp);
}

#[test]
#[allow(clippy::too_many_lines)]
fn a_trainer_battles_exp_award_surfaces_the_replacement_prompt() {
    let dex = Dex::new();
    let growth_rate = dex.species(SpeciesId(TORCHIC)).unwrap().growth_rate;
    let level_16 = assets::experience_for_level(growth_rate, 16).unwrap();
    let mut player = max_iv_mon(&dex, TORCHIC, 15, vec![SCRATCH, GROWL, TACKLE, LEER]);
    assert!(player
        .apply_experience(&dex, level_16 - 1 - player.experience())
        .unwrap()
        .is_none());
    let three_pp_ups_on_the_slot_about_to_be_given_up = PpBonuses::from_bits(0b0000_1100);
    let player = player
        .with_pp_bonuses(&dex, three_pp_ups_on_the_slot_about_to_be_given_up)
        .unwrap();
    let party = vec![max_iv_mon(&dex, TREECKO, 5, vec![POUND, LEER])];

    let mut rng = SequenceRng::new([u16::MAX; 128]);
    let mut battle =
        Battle::new_trainer(dex, player, MAY_ROUTE_103_MUDKIP, party, &mut rng).unwrap();

    let mut events = Vec::new();
    for _ in 0..8 {
        events = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();
        if battle.pending_move_learn().is_some() || battle.outcome().is_some() {
            break;
        }
    }
    let exp_index = events
        .iter()
        .position(|event| matches!(event, BattleEvent::ExpGained(_)))
        .unwrap_or_else(|| panic!("the faint awards EXP: {events:?}"));
    let prompt_index = events
        .iter()
        .position(|event| event == &BattleEvent::MoveLearnPrompt { move_id: PECK })
        .expect("crossing level 16 with four moves must ask about Peck");
    assert!(
        exp_index < prompt_index,
        "the award is applied before the question is asked: {events:?}"
    );
    // Upstream finishes the level-up script before running
    // `BattleScript_HandleFaintedMon` (`battle_util.c:1894`-`:1951`).
    assert_eq!(
        battle.outcome(),
        None,
        "the battle's end waits on the answer"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, BattleEvent::MoneyGained(_) | BattleEvent::Ended(_))),
        "nothing after the faint runs while the question is open: {events:?}"
    );
    assert_eq!(
        battle.pending_move_learn().map(|pending| pending.move_id()),
        Some(PECK)
    );

    assert_eq!(
        battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap_err()
            .error(),
        BattleError::MoveLearnPending(PECK),
        "an unanswered prompt refuses the next turn"
    );

    let dex = Dex::new();
    let money = battle.trainer().expect("a trainer battle").money();
    let answered = battle
        .resolve_move_learn(MoveLearnDecision::Replace(1), &mut rng)
        .unwrap();
    assert_eq!(
        answered,
        vec![
            BattleEvent::MoveReplaced {
                learned: PECK,
                forgotten: GROWL,
                slot: 1,
            },
            // Upstream's `Cmd_getmoneyreward` follows `Cmd_getexp`.
            BattleEvent::MoneyGained(money),
            BattleEvent::Ended(BattleOutcome::PlayerWon),
        ]
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
    assert!(battle.pending_move_learn().is_none());
    assert_eq!(
        battle
            .player()
            .moves()
            .iter()
            .map(|slot| slot.move_id)
            .collect::<Vec<_>>(),
        vec![SCRATCH, PECK, TACKLE, LEER]
    );
    assert_eq!(
        battle.player().pp_bonuses().get(1),
        0,
        "the forgotten move took its PP Ups with it (RemoveMonPPBonus)"
    );
    assert_eq!(
        battle.player().max_pp(&dex, 1).unwrap(),
        dex.move_data(PECK).unwrap().pp
    );
    assert_eq!(
        battle
            .resolve_move_learn(MoveLearnDecision::Decline, &mut rng)
            .unwrap_err(),
        BattleError::NoMoveLearnPending,
        "answering twice is a caller bug, not a second decision"
    );
}

#[test]
fn a_prompts_deferred_send_out_arrives_with_the_answer_and_the_battle_plays_on() {
    let dex = Dex::new();
    let growth_rate = dex.species(SpeciesId(TORCHIC)).unwrap().growth_rate;
    let level_16 = assets::experience_for_level(growth_rate, 16).unwrap();
    let mut player = max_iv_mon(&dex, TORCHIC, 15, vec![SCRATCH, GROWL, TACKLE, LEER]);
    assert!(player
        .apply_experience(&dex, level_16 - 1 - player.experience())
        .unwrap()
        .is_none());
    let party = vec![
        max_iv_mon(&dex, TREECKO, 5, vec![POUND, LEER]),
        max_iv_mon(&dex, TREECKO, 5, vec![POUND, LEER]),
    ];

    let mut rng = SequenceRng::new([u16::MAX; 128]);
    let mut battle =
        Battle::new_trainer(dex, player, MAY_ROUTE_103_MUDKIP, party, &mut rng).unwrap();

    let mut events = Vec::new();
    for _ in 0..8 {
        events = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();
        if battle.pending_move_learn().is_some() {
            break;
        }
    }
    assert!(
        events.contains(&BattleEvent::MoveLearnPrompt { move_id: PECK }),
        "the first knockout's award must ask about Peck: {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, BattleEvent::TrainerSentOut { .. })),
        "the replacement waits on the answer: {events:?}"
    );
    assert!(
        battle.enemy().is_fainted(),
        "the fainted mon is still on the field while the question is open"
    );

    let answered = battle
        .resolve_move_learn(MoveLearnDecision::Decline, &mut rng)
        .unwrap();
    assert_eq!(
        answered,
        vec![
            BattleEvent::MoveLearnDeclined { move_id: PECK },
            BattleEvent::TrainerSentOut {
                species: SpeciesId(TREECKO),
                bench_remaining: 0,
            },
        ]
    );
    assert_eq!(battle.outcome(), None, "the battle is still going");
    assert!(
        !battle.enemy().is_fainted(),
        "the replacement is on the field now"
    );
    assert!(
        battle.take_turn(PlayerAction::UseMove(0), &mut rng).is_ok(),
        "and the next turn is takeable again"
    );
}

/// Upstream's `BattleScript_TryLearnMoveLoop` exhausts every same-level
/// learnset entry before running the deferred faint aftermath.
#[test]
fn a_multi_prompt_chain_resolves_fully_before_the_deferred_transition() {
    const WYNAUT: u16 = 360;
    const COUNTER: MoveId = MoveId::COUNTER;
    const MIRROR_COAT: MoveId = MoveId::MIRROR_COAT;
    const SAFEGUARD: MoveId = MoveId::SAFEGUARD;
    const DESTINY_BOND: MoveId = MoveId::DESTINY_BOND;
    const LEVEL_15_BLOCK: [MoveId; 4] = [COUNTER, MIRROR_COAT, SAFEGUARD, DESTINY_BOND];

    let dex = Dex::new();
    let growth_rate = dex.species(SpeciesId(WYNAUT)).unwrap().growth_rate;
    let level_15 = assets::experience_for_level(growth_rate, 15).unwrap();
    let mut player = max_iv_mon(&dex, WYNAUT, 14, vec![SCRATCH, GROWL, TACKLE, LEER]);
    assert!(player
        .apply_experience(&dex, level_15 - 1 - player.experience())
        .unwrap()
        .is_none());
    let party = vec![max_iv_mon(&dex, TREECKO, 5, vec![POUND, LEER])];

    let mut rng = SequenceRng::new([u16::MAX; 128]);
    let mut battle =
        Battle::new_trainer(dex, player, MAY_ROUTE_103_MUDKIP, party, &mut rng).unwrap();

    for _ in 0..8 {
        let _ = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();
        if battle.pending_move_learn().is_some() {
            break;
        }
    }
    assert_eq!(
        battle.pending_move_learn().map(|pending| pending.move_id()),
        Some(LEVEL_15_BLOCK[0]),
        "the knockout's award must reach level 15's first entry"
    );
    let money = battle.trainer().expect("a trainer battle").money();

    for pair in LEVEL_15_BLOCK.windows(2) {
        let answered = battle
            .resolve_move_learn(MoveLearnDecision::Decline, &mut rng)
            .unwrap();
        assert_eq!(
            answered,
            vec![
                BattleEvent::MoveLearnDeclined { move_id: pair[0] },
                BattleEvent::MoveLearnPrompt { move_id: pair[1] },
            ]
        );
        assert_eq!(battle.outcome(), None);
    }

    let answered = battle
        .resolve_move_learn(MoveLearnDecision::Decline, &mut rng)
        .unwrap();
    assert_eq!(
        answered,
        vec![
            BattleEvent::MoveLearnDeclined {
                move_id: LEVEL_15_BLOCK[3]
            },
            BattleEvent::MoneyGained(money),
            BattleEvent::Ended(BattleOutcome::PlayerWon),
        ]
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
    assert!(battle.pending_move_learn().is_none());
}

#[test]
fn each_knocked_out_party_member_pays_its_own_boosted_award() {
    const BOOSTED_AWARD_PER_LEVEL_5_STARTER: u32 = 69;

    let dex = Dex::new();
    let player = max_iv_mon(&dex, 19, 50, vec![SLASH]);
    let party = vec![
        max_iv_mon(&dex, TREECKO, 5, vec![POUND, LEER]),
        max_iv_mon(&dex, TORCHIC, 5, vec![SCRATCH, GROWL]),
    ];
    let mut rng = SequenceRng::new(script(&[
        CONSTRUCTION_SEED,             // battle_main.c:3140 construction turn seed
        TURN_SEED,                     // battle_main.c:3923/:4013 turn-start seed
        AI_SIMULATED_DAMAGE,           // battle_ai_script_commands.c:341, slots 0..3
        DAMAGING_MOVE_ABILITY_GUESSES, // battle_ai_scripts.s:59/:93 CheckBadMove guesses (battle_ai_script_commands.c:1383)
        LEER_ABILITY_GUESSES, // battle_ai_scripts.s:93/:309 Soundproof, Clear Body/White Smoke guesses
        TRAINER_MOVE_PICK,    // battle_ai_script_commands.c:445 highest-score pick
        ACC_CRIT_VARIANCE_SECONDARY,
        TURN_SEED,                     // battle_main.c:3923/:4013 turn-start seed
        AI_SIMULATED_DAMAGE,           // battle_ai_script_commands.c:341, slots 0..3
        DAMAGING_MOVE_ABILITY_GUESSES, // battle_ai_scripts.s:59/:93 CheckBadMove guesses (battle_ai_script_commands.c:1383)
        GROWL_ABILITY_GUESSES, // battle_ai_scripts.s:93/:279/:309 Soundproof, Hyper Cutter, Clear Body/White Smoke guesses
        TRAINER_MOVE_PICK,     // battle_ai_script_commands.c:445 highest-score pick
        ACC_CRIT_VARIANCE_SECONDARY,
    ]));
    let mut battle =
        Battle::new_trainer(dex, player, MAY_ROUTE_103_MUDKIP, party, &mut rng).unwrap();

    let first = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(
        first.contains(&BattleEvent::ExpGained(BOOSTED_AWARD_PER_LEVEL_5_STARTER)),
        "{first:?}"
    );
    let second = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(
        second.contains(&BattleEvent::ExpGained(BOOSTED_AWARD_PER_LEVEL_5_STARTER)),
        "paid again rather than folded into the first knockout's award: {second:?}"
    );
    assert_eq!(rng.draws(), 30, "exact scripted draw stream is consumed");
    rng.assert_exhausted();
}

/// Upstream settles the send-out in `HandleFaintedMonActions`, after both
/// battlers' actions.
#[test]
fn a_replacement_does_not_act_on_the_turn_it_is_sent_out() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, 19, 50, vec![SLASH]);
    let party = vec![
        max_iv_mon(&dex, TREECKO, 5, vec![POUND, LEER]),
        max_iv_mon(&dex, TORCHIC, 5, vec![SCRATCH, GROWL]),
    ];
    let mut rng = SequenceRng::new(script(&[
        CONSTRUCTION_SEED,             // battle_main.c:3140 construction turn seed
        TURN_SEED,                     // battle_main.c:3923/:4013 turn-start seed
        AI_SIMULATED_DAMAGE,           // battle_ai_script_commands.c:341, slots 0..3
        DAMAGING_MOVE_ABILITY_GUESSES, // battle_ai_scripts.s:59/:93 CheckBadMove guesses (battle_ai_script_commands.c:1383)
        LEER_ABILITY_GUESSES, // battle_ai_scripts.s:93/:309 Soundproof, Clear Body/White Smoke guesses
        TRAINER_MOVE_PICK,    // battle_ai_script_commands.c:445 highest-score pick
        ACC_CRIT_VARIANCE_SECONDARY,
    ]));
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
    assert_eq!(rng.draws(), 15, "exact scripted draw stream is consumed");
    rng.assert_exhausted();
}

/// `AI_SetupFirstTurn` is the third flag `TRAINER_BRENDAN_ROUTE_103_TREECKO`
/// carries instead of `AI_SCRIPT_CHECK_VIABILITY` — the pin that this port
/// reproduces the upstream table's inconsistency rather than normalising it.
#[test]
fn both_route_103_ai_flag_shapes_construct_and_play() {
    for (trainer, script_groups) in [
        (
            MAY_ROUTE_103_MUDKIP,
            script(&[
                CONSTRUCTION_SEED,
                TURN_SEED,
                AI_SIMULATED_DAMAGE,
                DAMAGING_MOVE_ABILITY_GUESSES,
                GROWL_ABILITY_GUESSES,
                TRAINER_MOVE_PICK,
                ACC_CRIT_VARIANCE_SECONDARY,
            ]),
        ),
        (
            // Brendan's third flag is SetupFirstTurn: one extra roll before the pick.
            BRENDAN_ROUTE_103_TREECKO,
            script(&[
                CONSTRUCTION_SEED,
                TURN_SEED,
                AI_SIMULATED_DAMAGE,
                DAMAGING_MOVE_ABILITY_GUESSES,
                GROWL_ABILITY_GUESSES,
                SETUP_FIRST_TURN_ROLL,
                TRAINER_MOVE_PICK,
                ACC_CRIT_VARIANCE_SECONDARY,
            ]),
        ),
    ] {
        let dex = Dex::new();
        let player = max_iv_mon(&dex, 19, 50, vec![SLASH]);
        let party = vec![max_iv_mon(&dex, TORCHIC, 5, vec![SCRATCH, GROWL])];
        let mut rng = SequenceRng::new(script_groups.iter().copied());
        let mut battle = Battle::new_trainer(dex, player, trainer, party, &mut rng)
            .unwrap_or_else(|e| panic!("trainer {} must construct: {e}", trainer.0));
        let events = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();
        assert!(events.contains(&BattleEvent::Ended(BattleOutcome::PlayerWon)));
        assert_eq!(
            rng.draws(),
            script_groups.len(),
            "exact scripted draw stream is consumed"
        );
        rng.assert_exhausted();
    }
}

#[test]
fn losing_to_a_trainer_ends_in_the_ordinary_defeat_outcome_with_no_payout() {
    const MAGIKARP: u16 = 129;

    let dex = Dex::new();
    let player = max_iv_mon(&dex, MAGIKARP, 1, vec![TACKLE]);
    let party = vec![max_iv_mon(&dex, TREECKO, 100, vec![POUND, LEER])];

    let mut rng = SequenceRng::new(script(&[
        CONSTRUCTION_SEED,   // battle_main.c:3140 construction turn seed
        TURN_SEED,           // battle_main.c:3923/:4013 turn-start seed
        AI_SIMULATED_DAMAGE, // battle_ai_script_commands.c:341, slots 0..3
        TRAINER_MOVE_PICK,   // battle_ai_script_commands.c:445 highest-score pick
        ACC_CRIT_VARIANCE_SECONDARY,
    ]));
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
    assert_eq!(rng.draws(), 11, "exact scripted draw stream is consumed");
    rng.assert_exhausted();
}

/// `Cmd_checkteamslost`'s player-side HP total
/// (`src/battle_script_commands.c:3534`-`:3564`) reads zero only once the
/// active member and every reserve has fainted -- the trainer-battle
/// counterpart of `wild_battle.rs`'s
/// `an_exhausted_player_party_loses_only_after_every_reserve_has_had_its_turn`.
#[test]
fn a_healthy_player_reserve_survives_a_trainer_battle_lead_faint_and_is_itself_exhausted_next() {
    let dex = Dex::new();
    let lead = max_iv_mon(&dex, MUDKIP, 5, vec![TACKLE]);
    let lead_pp = lead.moves()[0].pp;
    let reserve = max_iv_mon(&dex, TORCHIC, 5, vec![SCRATCH]);
    let reserve_pp = reserve.moves()[0].pp;
    // A level-100 opponent one-shots either level-5 party member regardless
    // of speed order.
    let party = vec![max_iv_mon(&dex, TREECKO, 100, vec![POUND])];

    let mut rng = SequenceRng::new(script(&[
        CONSTRUCTION_SEED,   // battle_main.c:3140 construction turn seed
        TURN_SEED,           // battle_main.c:3923/:4013 turn-start seed
        AI_SIMULATED_DAMAGE, // battle_ai_script_commands.c:341, slots 0..3
        TRAINER_MOVE_PICK,   // battle_ai_script_commands.c:445 highest-score pick
        ACC_CRIT_VARIANCE_SECONDARY,
        TURN_SEED,           // battle_main.c:3923/:4013 turn-start seed
        AI_SIMULATED_DAMAGE, // battle_ai_script_commands.c:341, slots 0..3
        TRAINER_MOVE_PICK,   // battle_ai_script_commands.c:445 highest-score pick
        ACC_CRIT_VARIANCE_SECONDARY,
    ]));
    let mut battle = Battle::new_trainer_with_player_reserves(
        dex,
        lead,
        vec![reserve],
        MAY_ROUTE_103_MUDKIP,
        party,
        &mut rng,
    )
    .expect("a trainer battle admits an ordered player reserve list");
    assert_eq!(
        battle.player_members().count(),
        2,
        "the reserve must be admitted alongside the lead, not dropped"
    );

    let turn1 = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(
        turn1.contains(&BattleEvent::Fainted { by_player: true }),
        "the level-100 Treecko one-shots the level-5 lead: {turn1:?}"
    );
    assert!(
        turn1.contains(&BattleEvent::PlayerSentOut {
            species: SpeciesId(TORCHIC),
            reserves_remaining: 0,
        }),
        "the healthy reserve takes over instead of ending the battle: {turn1:?}"
    );
    assert_eq!(battle.outcome(), None);
    assert_eq!(battle.player().species(), SpeciesId(TORCHIC));

    let turn2 = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(
        turn2.contains(&BattleEvent::Fainted { by_player: true }),
        "the reserve falls to the same one-shot: {turn2:?}"
    );
    assert_eq!(
        turn2.last(),
        Some(&BattleEvent::Ended(BattleOutcome::PlayerLost)),
        "no reserve remains, so the whole-party exhaustion ends the battle: {turn2:?}"
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerLost));

    let members: Vec<_> = battle.player_members().collect();
    assert_eq!(members.len(), 2);
    assert_eq!(members[0].species(), SpeciesId(MUDKIP));
    assert_eq!(members[0].current_hp(), 0);
    assert_eq!(members[0].moves()[0].pp, lead_pp);
    assert_eq!(members[1].species(), SpeciesId(TORCHIC));
    assert_eq!(members[1].current_hp(), 0);
    assert_eq!(members[1].moves()[0].pp, reserve_pp);
    assert_eq!(rng.draws(), 21, "exact scripted draw stream is consumed");
    rng.assert_exhausted();
}

/// `AI_CheckViability` routes `EFFECT_HIGH_CRITICAL` — an otherwise ordinary
/// hit script ([`battle::is_ordinary_hit_effect`]) — to `AI_CV_HighCrit`
/// (`data/battle_ai_scripts.s:1449`), an unmodelled branch that draws RNG;
/// admitting it would desynchronise the shared stream rather than merely
/// mis-score it.
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

/// `BattleAI_DoAIProcessing` skips a zero-PP slot before its script runs, so
/// an exhausted move the AI cannot score never reaches scoring; admission must
/// not reject the party over it.
#[test]
fn an_exhausted_unscoreable_move_does_not_block_a_trainer_battle() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, MUDKIP, 100, vec![GROWL]);
    let mut enemy = max_iv_mon(&dex, TORCHIC, 34, vec![SLASH, SCRATCH]);
    while enemy.moves()[0].pp > 0 {
        enemy.deduct_pp(0).unwrap();
    }
    let scratch_pp = enemy.moves()[1].pp;
    let mut rng = SequenceRng::new([u16::MAX; 64]);
    let mut battle =
        Battle::new_trainer(dex, player, MAY_ROUTE_103_MUDKIP, vec![enemy], &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(events.iter().any(|event| matches!(
        event,
        BattleEvent::Hit {
            by_player: false,
            move_id: SCRATCH,
            ..
        }
    )));
    assert_eq!(battle.enemy().moves()[0].pp, 0);
    assert_eq!(battle.enemy().moves()[1].pp, scratch_pp - 1);
}

/// `sIgnoredPowerfulMoveEffects` (`src/battle_ai_script_commands.c:266-280`)
/// keeps Overheat out of the most-powerful-move comparison; an exhausted one is
/// admitted yet still sits in the moveset, so it must not make Scratch look
/// weaker and cost it its `AI_TryToFaint` bonus over Growl.
#[test]
fn an_exhausted_ignored_effect_move_does_not_skew_the_power_comparison() {
    let dex = Dex::new();
    let mut enemy = max_iv_mon(&dex, TORCHIC, 34, vec![MoveId::OVERHEAT, SCRATCH, GROWL]);
    while enemy.moves()[0].pp > 0 {
        enemy.deduct_pp(0).unwrap();
    }
    let player = max_iv_mon(&dex, RATTATA, 100, vec![GROWL]);
    let mut rng = SequenceRng::new(script(&[
        CONSTRUCTION_SEED,             // battle_main.c:3140 construction turn seed
        TURN_SEED,                     // battle_main.c:3923/:4013 turn-start seed
        AI_SIMULATED_DAMAGE,           // battle_ai_script_commands.c:341, slots 0..3
        DAMAGING_MOVE_ABILITY_GUESSES, // battle_ai_scripts.s:59/:93 CheckBadMove guesses (battle_ai_script_commands.c:1383)
        GROWL_ABILITY_GUESSES, // battle_ai_scripts.s:93/:279/:309 Soundproof, Hyper Cutter, Clear Body/White Smoke guesses
        TRAINER_MOVE_PICK,     // battle_ai_script_commands.c:445 highest-score pick
        ACCURACY_ONLY,         // battle_script_commands.c:1176 Growl accuracy
        ACC_CRIT_VARIANCE_SECONDARY,
    ]));
    let mut battle =
        Battle::new_trainer(dex, player, MAY_ROUTE_103_MUDKIP, vec![enemy], &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(events.iter().any(|event| matches!(
        event,
        BattleEvent::Hit {
            by_player: false,
            move_id: SCRATCH,
            ..
        }
    )));
    assert_eq!(rng.draws(), 17, "exact scripted draw stream is consumed");
    rng.assert_exhausted();
}

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

/// `HandleAction_ActionFinished` clears `gCurrentMove` (`pokeemerald/src/battle_util.c:658-670`)
/// before `BattleTurnPassed` scores replacements (`pokeemerald/src/battle_main.c:3956-3969`).
#[test]
fn a_residual_poison_knockout_scores_replacements_with_no_move_resolving() {
    const NO_LEVEL_UP_LEVEL: u8 = 100;
    const BENCH_LEVEL: u8 = 5;
    const NONCRITICAL_MIN_DAMAGE_DRAW: u16 = u16::MAX;
    const RNG_DRAW_BUDGET: usize = 128;
    const MEGA_KICK_SLOT: usize = 0;
    const BENCH_REMAINING_AFTER_SEND_OUT: usize = 1;

    let dex = Dex::new();
    // No bench move is super effective on Chansey, so the damage pass decides;
    // Chansey acts last, so a missing reset leaves Mega Kick as the stale move.
    let player = max_iv_mon(&dex, CHANSEY, NO_LEVEL_UP_LEVEL, vec![MEGA_KICK]);
    let mut lead = max_iv_mon(&dex, KANGASKHAN, NO_LEVEL_UP_LEVEL, vec![GROWL]);
    lead.set_status1(Status1::Poisoned);
    let residual = poison_residual_damage(lead.stats().max_hp);
    lead.apply_damage(lead.stats().max_hp - residual);
    let party = vec![
        lead,
        max_iv_mon(&dex, PICHU, BENCH_LEVEL, vec![TACKLE]),
        max_iv_mon(&dex, MUDKIP, BENCH_LEVEL, vec![WATER_GUN]),
    ];

    let mut rng = SequenceRng::new([NONCRITICAL_MIN_DAMAGE_DRAW; RNG_DRAW_BUDGET]);
    let mut battle =
        Battle::new_trainer(dex, player, MAY_ROUTE_103_MUDKIP, party, &mut rng).unwrap();

    let events = battle
        .take_turn(PlayerAction::UseMove(MEGA_KICK_SLOT), &mut rng)
        .unwrap();

    let tick_index = events
        .iter()
        .position(|event| {
            matches!(
                event,
                BattleEvent::HurtByPoison {
                    by_player: false,
                    ..
                }
            )
        })
        .unwrap_or_else(|| panic!("the lead must fall to the residual, not the hit: {events:?}"));
    let sent_out_index = events
        .iter()
        .position(|event| matches!(event, BattleEvent::TrainerSentOut { .. }))
        .unwrap_or_else(|| panic!("the bench replaces the fallen lead: {events:?}"));
    assert!(
        tick_index < sent_out_index,
        "the send-out is the residual pass's, not the action phase's: {events:?}"
    );
    assert_eq!(
        events[sent_out_index],
        BattleEvent::TrainerSentOut {
            species: SpeciesId(PICHU),
            bench_remaining: BENCH_REMAINING_AFTER_SEND_OUT,
        },
        "a cleared gCurrentMove scores every candidate off the floor base, \
         so Tackle's STAB keeps party-order-first Pichu: {events:?}"
    );
    assert_eq!(battle.enemy().species(), SpeciesId(PICHU));
}

/// `HandleFaintedMonActions` case 4 walks battlers from 0
/// (`src/battle_util.c:1924`-`:1935`), so a double faint sends the player's
/// replacement out (`data/battle_scripts_1.s:2883`-`:2893`) before the
/// trainer chooses its own.
#[test]
fn a_double_faint_sends_the_players_replacement_out_before_the_trainers() {
    let dex = Dex::new();
    let mut lead = max_iv_mon(&dex, KANGASKHAN, 50, vec![battle::STRUGGLE]);
    lead.apply_damage(lead.current_hp() - 1);
    let reserve = max_iv_mon(&dex, MUDKIP, 5, vec![TACKLE]);
    let mut enemy_lead = max_iv_mon(&dex, CHANSEY, 5, vec![POUND]);
    enemy_lead.apply_damage(enemy_lead.current_hp() - 1);
    let party = vec![
        enemy_lead,
        max_iv_mon(&dex, TREECKO, 5, vec![POUND]),
        max_iv_mon(&dex, TORCHIC, 5, vec![SCRATCH]),
    ];
    let mut rng = SequenceRng::new(script(&[
        CONSTRUCTION_SEED,   // battle_main.c:3140 construction turn seed
        TURN_SEED,           // battle_main.c:3923/:4013 turn-start seed
        AI_SIMULATED_DAMAGE, // battle_ai_script_commands.c:341, slots 0..3
        TRAINER_MOVE_PICK,   // battle_ai_script_commands.c:445 highest-score pick
        ACC_CRIT_VARIANCE, // battle_script_commands.c :1176/:1282/:1641, Struggle skips the :2923 secondary draw
    ]));
    let mut battle = Battle::new_trainer_with_player_reserves(
        dex,
        lead,
        vec![reserve],
        MAY_ROUTE_103_MUDKIP,
        party,
        &mut rng,
    )
    .unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        events.contains(&BattleEvent::Fainted { by_player: true }),
        "{events:?}"
    );
    assert!(
        events.contains(&BattleEvent::Fainted { by_player: false }),
        "{events:?}"
    );
    let player_sent = events
        .iter()
        .position(|event| matches!(event, BattleEvent::PlayerSentOut { .. }))
        .expect("the player's replacement is sent out");
    let trainer_sent = events
        .iter()
        .position(|event| matches!(event, BattleEvent::TrainerSentOut { .. }))
        .expect("the trainer's replacement is sent out");
    assert!(
        player_sent < trainer_sent,
        "the player's replacement goes out first: {events:?}"
    );
    assert_eq!(rng.draws(), 10, "exact scripted draw stream is consumed");
    rng.assert_exhausted();
}
