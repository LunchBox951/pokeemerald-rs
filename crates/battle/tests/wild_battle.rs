#[expect(
    dead_code,
    reason = "the escape and move-selection fixtures in this shared module are used only by turn_engine's test binary"
)]
mod common;

use assets::{MoveEffect, MoveId, SpeciesId};
use battle::{
    build_wild_pokemon, Battle, BattleError, BattleEvent, BattleOutcome, BattlePokemon, Dex, Ivs,
    Nature, PlayerAction, MOVE_NONE,
};
use common::{max_iv_mon as fixed_mon, SequenceRng as ScriptedRng, MAX_IVS};

#[test]
fn scripted_wild_battle_runs_move_vs_move_to_a_faint_and_reports_victory() {
    let dex = Dex::new();

    let player = fixed_mon(&dex, 4, 50, vec![MoveId(33)]);

    let minimum_damage_variance_roll: u16 = 15;
    let mut rng = ScriptedRng::new([
        // build_wild_pokemon (5 draws):
        0, // wild nature
        0, // personality, first attempt
        0, // personality, second draw
        0, // wild IV draw 1
        0, // wild IV draw 2
        // Battle::new (1 draw):
        0, // battle-start turn number
        // the turn (6 draws):
        0, // turn number
        0, // opponent's move selection
        // no turn-order draw: the player is far faster, so no speed tie
        0, // accuracy roll
        1, // critical-hit roll
        minimum_damage_variance_roll,
        0, // discarded effect-chance roll
    ]);

    let enemy = build_wild_pokemon(&dex, SpeciesId(19), 5, vec![MoveId(33)], &mut rng)
        .expect("wild Rattata construction");
    assert_eq!(enemy.nature(), Nature::Hardy);
    assert_eq!(enemy.ivs().hp, 0);

    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).expect("Battle::new");

    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .expect("take_turn");

    assert!(
        events.iter().any(|e| matches!(
            e,
            BattleEvent::Hit {
                by_player: true,
                move_id: MoveId(33),
                ..
            }
        )),
        "expected the player's Tackle to land, named in the event: {events:?}"
    );
    assert!(
        events.contains(&BattleEvent::Fainted { by_player: false }),
        "expected the wild Rattata to faint: {events:?}"
    );
    assert!(
        events.contains(&BattleEvent::ExpGained(40)),
        "expected exactly Cmd_getexp's award -- Rattata expYield 57 \
         (species_info.h) at level 5: 57*5/7 = 40 -- pinning that the engine \
         feeds wild_faint_exp the *enemy's* base_exp and level: {events:?}"
    );
    assert_eq!(
        battle.player().evs().speed,
        1,
        "MonGainEVs runs on this KO too, before the exp award -- \
         Rattata's own Speed EV yield (species_info.h) lands on the player \
         regardless of the wild mon's own level or HP"
    );
    assert_eq!(
        events.last(),
        Some(&BattleEvent::Ended(BattleOutcome::PlayerWon))
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
    assert_eq!(battle.enemy().current_hp(), 0);
    assert_eq!(
        battle.player().current_hp(),
        battle.player().stats().max_hp,
        "the far-lower-level wild mon should never have gotten to act"
    );
    assert_eq!(
        rng.draws(),
        12,
        "5 (wild construction) + 1 (battle start) + 6 (the turn)"
    );

    let rejected = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap_err();
    assert_eq!(rejected.error(), BattleError::BattleAlreadyOver);
    assert!(
        rejected.events().is_empty(),
        "a turn rejected before it began reports no events"
    );
    assert_eq!(rng.draws(), 12);
}

#[test]
fn a_faster_player_always_escapes_a_wild_battle_successfully() {
    let dex = Dex::new();
    let player = fixed_mon(&dex, 4, 50, vec![MoveId(33)]);

    // Draw order: five wild-construction draws, battle-start turn number,
    // turn number, then opponent move selection; guaranteed escape rolls nothing.
    let mut rng = ScriptedRng::new([0, 0, 0, 0, 0, 0, 0, 0]);
    let enemy = build_wild_pokemon(&dex, SpeciesId(19), 5, vec![MoveId(33)], &mut rng)
        .expect("wild Rattata construction");

    let player_hp_before = player.current_hp();
    let enemy_hp_before = enemy.current_hp();
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).expect("Battle::new");

    let events = battle
        .take_turn(PlayerAction::Run, &mut rng)
        .expect("take_turn(Run)");

    assert_eq!(
        events,
        vec![
            BattleEvent::RunAttempt {
                by_player: true,
                success: true,
            },
            BattleEvent::Ended(BattleOutcome::PlayerRan),
        ]
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerRan));
    assert_eq!(
        battle.run_tries(),
        1,
        "TryRunFromBattle increments runTries after the unconditional \
         same-speed-or-faster success too (battle_util.c:475)"
    );
    assert_eq!(battle.player().current_hp(), player_hp_before);
    assert_eq!(battle.enemy().current_hp(), enemy_hp_before);
    assert_eq!(
        rng.draws(),
        8,
        "5 (wild construction) + 1 (battle start) + 2 (turn number, move pick)"
    );
}

#[test]
fn a_battle_with_a_move_outside_this_slice_is_refused_before_it_starts() {
    let dex = Dex::new();
    // Horn Drill has nonzero power but an unsupported OHKO effect, so a
    // power-only check would admit it to the wrong damage/RNG path.
    let horn_drill_ohko_effect = MoveEffect(38);
    let horn_drill_token_power = 1;
    let horn_drill = dex.move_data(MoveId(32)).unwrap();
    assert_eq!(horn_drill.effect, horn_drill_ohko_effect);
    assert_eq!(horn_drill.power, horn_drill_token_power);

    let player = fixed_mon(&dex, 4, 50, vec![MoveId(33)]);
    let enemy = fixed_mon(&dex, 19, 5, vec![MoveId(32)]);

    let mut rng = ScriptedRng::new([]);
    assert_eq!(
        Battle::new(dex, player, enemy, false, &mut rng).err(),
        Some(BattleError::UnsupportedMoveEffect(MoveId(32)))
    );
    assert_eq!(rng.draws(), 0);
}

#[test]
fn an_exhausted_player_party_loses_only_after_every_reserve_has_had_its_turn() {
    let dex = Dex::new();
    let player = fixed_mon(&dex, 19, 5, vec![MoveId(33)]);
    let player_max_hp = player.stats().max_hp;
    let reserve = fixed_mon(&dex, 7, 5, vec![MoveId(33)]);
    let reserve_max_hp = reserve.stats().max_hp;
    let enemy = fixed_mon(&dex, 4, 50, vec![MoveId(33)]);
    let tackle_max_pp = dex.move_data(MoveId(33)).unwrap().pp;

    // RNG order: battle-start number; then, per turn, turn number,
    // opponent move selection, accuracy, crit, damage, and effect-chance rolls.
    let mut rng = ScriptedRng::new([0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 0, 0]);
    let mut battle =
        Battle::new_with_player_reserves(dex, player, vec![reserve], enemy, false, &mut rng)
            .expect("Battle::new_with_player_reserves");

    let turn1 = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .expect("turn 1");
    assert_eq!(
        turn1,
        vec![
            BattleEvent::Hit {
                by_player: false,
                move_id: MoveId(33),
                damage: player_max_hp,
                is_critical: false,
            },
            BattleEvent::Fainted { by_player: true },
            BattleEvent::PlayerSentOut {
                species: SpeciesId(7),
                reserves_remaining: 0,
            },
        ],
        "the healthy reserve takes over instead of ending the battle: {turn1:?}"
    );
    assert_eq!(battle.outcome(), None);
    assert_eq!(battle.player().species(), SpeciesId(7));

    let turn2 = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .expect("turn 2");
    assert_eq!(
        turn2,
        vec![
            BattleEvent::Hit {
                by_player: false,
                move_id: MoveId(33),
                damage: reserve_max_hp,
                is_critical: false,
            },
            BattleEvent::Fainted { by_player: true },
            BattleEvent::Ended(BattleOutcome::PlayerLost),
        ],
        "the second faint has no reserve left, so the battle ends: {turn2:?}"
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerLost));
    assert_eq!(rng.draws(), 13);

    assert_eq!(battle.player().species(), SpeciesId(7));
    let members: Vec<_> = battle.player_members().collect();
    assert_eq!(members.len(), 2);
    assert_eq!(members[0].species(), SpeciesId(19));
    assert_eq!(members[0].current_hp(), 0);
    assert_eq!(members[0].moves()[0].pp, tackle_max_pp);
    assert_eq!(members[1].species(), SpeciesId(7));
    assert_eq!(members[1].current_hp(), 0);
    assert_eq!(members[1].moves()[0].pp, tackle_max_pp);
}

#[test]
fn an_impossible_battler_cannot_be_built_at_all() {
    let dex = Dex::new();
    assert_eq!(
        BattlePokemon::new(&dex, SpeciesId(1), 101, MAX_IVS, 0, vec![MoveId(33)]),
        Err(BattleError::InvalidLevel(101))
    );
    assert!(matches!(
        BattlePokemon::new(
            &dex,
            SpeciesId(1),
            5,
            Ivs { hp: 99, ..MAX_IVS },
            0,
            vec![MoveId(33)]
        ),
        Err(BattleError::InvalidIv(99))
    ));
    // Reject an empty moveset before wild move selection can spin forever.
    assert_eq!(
        BattlePokemon::new(&dex, SpeciesId(1), 5, MAX_IVS, 0, vec![]),
        Err(BattleError::InvalidMoveCount(0))
    );
    assert_eq!(
        BattlePokemon::new(&dex, SpeciesId(1), 5, MAX_IVS, 0, vec![MOVE_NONE]),
        Err(BattleError::PlaceholderMove(0))
    );
}
