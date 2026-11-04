//! Trainer switch resolution and per-battler history, driven through the
//! private action seam because no AI decision selects a switch yet.

use assets::trainers::TrainerId;
use assets::{MoveId, SpeciesId};

use super::trainer::fixed_ivs;
use super::{
    Battle, BattleEvent, BattlerSide, EnemyAction, LastLandedMove, PerishSongTurns, PlayerAction,
};
use crate::damage::BattleRng;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::{BattlePokemon, MOVE_NONE};
use crate::stat_change::{set_stage, ChangedStat};
use crate::stat_stage::StatStage;
use crate::status1::Status1;

const MAY_ROUTE_103_MUDKIP: TrainerId = TrainerId(529);
const TREECKO: SpeciesId = SpeciesId(277);
const TORCHIC: SpeciesId = SpeciesId(280);
const MUDKIP: SpeciesId = SpeciesId(283);
const PICHU: SpeciesId = SpeciesId(172);
const SANDSHREW: SpeciesId = SpeciesId(27);
const GASTLY: SpeciesId = SpeciesId(92);
const MEOWTH: SpeciesId = SpeciesId(52);

/// Replays `script` and fails closed: a draw past its end panics with the draw
/// index, so a test scripts exactly the draws it expects and a stray extra
/// draw cannot pass unnoticed. [`ScriptedRng::assert_exhausted`] catches the
/// opposite drift, a draw that no longer happens.
struct ScriptedRng {
    script: Vec<u16>,
    draws: usize,
}

impl ScriptedRng {
    fn new(script: &[u16]) -> Self {
        Self {
            script: script.to_vec(),
            draws: 0,
        }
    }

    /// A script of `count` zero draws.
    fn zeros(count: usize) -> Self {
        Self::new(&vec![0; count])
    }

    /// Asserts the code under test consumed the whole script, no more, no less.
    #[track_caller]
    fn assert_exhausted(&self) {
        assert_eq!(
            self.draws,
            self.script.len(),
            "the script held {} draws but {} were taken",
            self.script.len(),
            self.draws,
        );
    }
}

impl BattleRng for ScriptedRng {
    fn next_u16(&mut self) -> u16 {
        let index = self.draws;
        let Some(value) = self.script.get(index).copied() else {
            panic!(
                "ScriptedRng exhausted: draw index {index} but the script holds {}",
                self.script.len()
            );
        };
        self.draws += 1;
        value
    }
}

#[test]
#[should_panic(expected = "ScriptedRng exhausted: draw index 2 but the script holds 2")]
fn a_scripted_rng_panics_with_the_draw_index_when_the_script_runs_out() {
    let mut rng = ScriptedRng::new(&[7, 9]);
    assert_eq!((rng.next_u16(), rng.next_u16()), (7, 9));
    rng.next_u16();
}

#[test]
#[should_panic(expected = "the script held 2 draws but 1 were taken")]
fn a_scripted_rng_with_an_unspent_draw_is_not_exhausted() {
    let mut rng = ScriptedRng::zeros(2);
    rng.next_u16();
    rng.assert_exhausted();
}

fn mon(dex: &Dex, species: SpeciesId, level: u8, moves: &[MoveId]) -> BattlePokemon {
    BattlePokemon::new(dex, species, level, fixed_ivs(255), 0, moves.to_vec())
        .expect("dex-resident")
}

fn fainted(mut mon: BattlePokemon) -> BattlePokemon {
    mon.apply_damage(mon.stats().max_hp);
    mon
}

/// A level-5 player against a level-50 party `[Treecko, Torchic, Mudkip,
/// Pichu]`, so the player's Tackle never knocks anything out.
fn four_mon_battle(rng: &mut ScriptedRng) -> Battle {
    let dex = Dex::new();
    let player = mon(&dex, MEOWTH, 5, &[MoveId::TACKLE, MoveId::FOCUS_ENERGY]);
    let party = vec![
        mon(&dex, TREECKO, 50, &[MoveId::TACKLE, MoveId::GROWL]),
        mon(&dex, TORCHIC, 50, &[MoveId::TACKLE, MoveId::GROWL]),
        mon(&dex, MUDKIP, 50, &[MoveId::TACKLE, MoveId::GROWL]),
        mon(&dex, PICHU, 50, &[MoveId::TACKLE, MoveId::GROWL]),
    ];
    Battle::new_trainer(dex, player, MAY_ROUTE_103_MUDKIP, party, rng).expect("a valid battle")
}

fn battle_with_party(party: Vec<BattlePokemon>, player: BattlePokemon) -> Battle {
    // Construction takes exactly the one seed draw.
    battle_with_party_drawing(party, player, 1)
}

/// As [`battle_with_party`], pinning construction to `draws` draws.
fn battle_with_party_drawing(
    party: Vec<BattlePokemon>,
    player: BattlePokemon,
    draws: usize,
) -> Battle {
    let dex = Dex::new();
    let mut rng = ScriptedRng::zeros(draws);
    let battle = Battle::new_trainer(dex, player, MAY_ROUTE_103_MUDKIP, party, &mut rng)
        .expect("a valid battle");
    rng.assert_exhausted();
    battle
}

/// Runs one turn with the enemy's action fixed by the test.
fn turn_with(
    battle: &mut Battle,
    player_move: usize,
    enemy_action: EnemyAction,
    rng: &mut ScriptedRng,
) -> Result<Vec<BattleEvent>, BattleError> {
    let player_action = battle.validate_player_action(PlayerAction::UseMove(player_move))?;
    battle.start_turn(rng);
    let mut events = Vec::new();
    battle.resolve_selected_actions(player_action, enemy_action, rng, &mut events)?;
    Ok(events)
}

fn candidate_slots(battle: &Battle) -> Vec<(usize, SpeciesId)> {
    battle
        .trainer()
        .expect("a trainer battle")
        .candidates()
        .map(|(slot, mon)| (slot, mon.species()))
        .collect()
}

#[test]
fn switches_keep_every_party_member_in_its_original_slot() {
    let mut rng = ScriptedRng::zeros(5);
    let mut battle = four_mon_battle(&mut rng);
    let species = [TREECKO, TORCHIC, MUDKIP, PICHU];

    for target in [2, 0, 3, 1] {
        turn_with(&mut battle, 1, EnemyAction::Switch(target), &mut rng)
            .expect("a healthy non-active slot");

        let context = battle.trainer().expect("a trainer battle");
        assert_eq!(context.active_slot(), target);
        assert_eq!(battle.enemy().species(), species[target]);
        let expected: Vec<_> = (0..4)
            .filter(|slot| *slot != target)
            .map(|slot| (slot, species[slot]))
            .collect();
        assert_eq!(candidate_slots(&battle), expected);
        assert_eq!(context.bench().len(), 3);
        assert_eq!(context.bench_len(), 3);
    }
    rng.assert_exhausted();
}

#[test]
fn a_switch_preserves_the_outgoing_members_hp_pp_and_status() {
    let mut rng = ScriptedRng::zeros(3);
    let mut battle = four_mon_battle(&mut rng);
    battle.enemy.apply_damage(7);
    battle.enemy.set_status1(Status1::Paralysed);
    battle.enemy.deduct_pp_by(0, 3).expect("a spendable slot");
    let hp = battle.enemy().current_hp();
    let pp = battle.enemy().moves()[0].pp;

    turn_with(&mut battle, 1, EnemyAction::Switch(1), &mut rng).expect("a healthy slot");
    turn_with(&mut battle, 1, EnemyAction::Switch(0), &mut rng).expect("back to the lead");

    assert_eq!(battle.enemy().current_hp(), hp);
    assert_eq!(battle.enemy().moves()[0].pp, pp);
    assert_eq!(battle.enemy().status1(), Status1::Paralysed);
    rng.assert_exhausted();
}

#[test]
fn a_switch_naming_anything_but_a_healthy_non_active_slot_changes_nothing() {
    let dex = Dex::new();
    let player = mon(&dex, MEOWTH, 5, &[MoveId::TACKLE]);
    let party = vec![
        mon(&dex, TREECKO, 50, &[MoveId::TACKLE]),
        fainted(mon(&dex, TORCHIC, 50, &[MoveId::TACKLE])),
        mon(&dex, MUDKIP, 50, &[MoveId::TACKLE]),
    ];
    let mut battle = battle_with_party(party, player);

    for slot in [0, 1, 3, 99] {
        let mut rng = ScriptedRng::zeros(1);
        let before_hp = battle.player().current_hp();
        let error = turn_with(&mut battle, 0, EnemyAction::Switch(slot), &mut rng)
            .expect_err("not a healthy non-active slot");

        assert_eq!(error, BattleError::InvalidTrainerSwitch(slot));
        assert_eq!(rng.draws, 1, "only the turn-start seed was drawn");
        rng.assert_exhausted();
        assert_eq!(battle.player().current_hp(), before_hp);
        assert_eq!(battle.trainer().unwrap().active_slot(), 0);
        assert_eq!(battle.enemy().species(), TREECKO);
    }
}

#[test]
fn a_wild_battle_refuses_a_switch() {
    let dex = Dex::new();
    let player = mon(&dex, MEOWTH, 5, &[MoveId::TACKLE]);
    let wild = mon(&dex, TREECKO, 5, &[MoveId::TACKLE]);
    let mut rng = ScriptedRng::zeros(2);
    let mut battle = Battle::new(dex, player, wild, false, &mut rng).expect("a wild battle");

    let error = turn_with(&mut battle, 0, EnemyAction::Switch(1), &mut rng)
        .expect_err("a wild battle has no party");

    assert_eq!(error, BattleError::InvalidTrainerSwitch(1));
    rng.assert_exhausted();
}

#[test]
fn bench_counts_only_healthy_members_but_the_party_keeps_its_fainted_ones() {
    let dex = Dex::new();
    let player = mon(&dex, MEOWTH, 5, &[MoveId::TACKLE]);
    let party = vec![
        mon(&dex, TREECKO, 50, &[MoveId::TACKLE]),
        fainted(mon(&dex, TORCHIC, 50, &[MoveId::TACKLE])),
        mon(&dex, MUDKIP, 50, &[MoveId::TACKLE]),
    ];
    let mut battle = battle_with_party(party, player);
    let context = battle.trainer().unwrap();
    assert_eq!(context.bench().len(), 2);
    assert_eq!(context.bench_len(), 1);

    battle.enemy.apply_damage(battle.enemy.stats().max_hp);
    let mut events = Vec::new();
    battle
        .settle_fainted_enemy(&mut events)
        .expect("a replacement exists");

    assert_eq!(
        events,
        [BattleEvent::TrainerSentOut {
            species: MUDKIP,
            bench_remaining: 0,
        }]
    );
    let context = battle.trainer().unwrap();
    assert_eq!(context.active_slot(), 2);
    assert_eq!(
        candidate_slots(&battle),
        [(0, TREECKO), (1, TORCHIC)],
        "the fallen lead and the fainted member keep their slots"
    );
    assert!(context.bench().iter().all(BattlePokemon::is_fainted));
}

#[test]
fn a_forced_replacement_after_a_voluntary_switch_can_name_the_original_lead() {
    let dex = Dex::new();
    let player = mon(&dex, MEOWTH, 5, &[MoveId::TACKLE]);
    let party = vec![
        mon(&dex, TREECKO, 50, &[MoveId::TACKLE]),
        fainted(mon(&dex, TORCHIC, 50, &[MoveId::TACKLE])),
        mon(&dex, MUDKIP, 50, &[MoveId::TACKLE]),
    ];
    let mut battle = battle_with_party(party, player);
    let mut rng = ScriptedRng::zeros(5);
    turn_with(&mut battle, 0, EnemyAction::Switch(2), &mut rng).expect("Mudkip is healthy");
    assert_eq!(battle.trainer().unwrap().bench_len(), 1);

    battle.enemy.apply_damage(battle.enemy.stats().max_hp);
    let mut events = Vec::new();
    battle
        .settle_fainted_enemy(&mut events)
        .expect("a replacement exists");

    assert_eq!(battle.trainer().unwrap().active_slot(), 0);
    assert_eq!(battle.enemy().species(), TREECKO);
    assert_eq!(
        candidate_slots(&battle),
        [(1, TORCHIC), (2, MUDKIP)],
        "the active slot is excluded and the fainted Mudkip keeps its own"
    );
    rng.assert_exhausted();
}

#[test]
fn a_switch_acts_before_a_faster_players_move() {
    let dex = Dex::new();
    let player = mon(&dex, MEOWTH, 5, &[MoveId::QUICK_ATTACK]);
    let party = vec![
        mon(&dex, TREECKO, 50, &[MoveId::TACKLE]),
        mon(&dex, TORCHIC, 50, &[MoveId::TACKLE]),
    ];
    let mut battle = battle_with_party(party, player);
    let lead_hp = battle.enemy().current_hp();
    let mut rng = ScriptedRng::zeros(5);

    let events = turn_with(&mut battle, 0, EnemyAction::Switch(1), &mut rng)
        .expect("a healthy non-active slot");

    assert!(matches!(
        events.as_slice(),
        [
            BattleEvent::TrainerSentOut {
                species: TORCHIC,
                ..
            },
            BattleEvent::UsedMove {
                by_player: true,
                ..
            },
            BattleEvent::Hit {
                by_player: true,
                ..
            },
            ..
        ]
    ));
    assert!(
        events.iter().all(|event| !matches!(
            event,
            BattleEvent::Hit {
                by_player: false,
                ..
            }
        )),
        "a switching enemy does not attack"
    );
    let context = battle.trainer().unwrap();
    assert_eq!(context.bench()[0].current_hp(), lead_hp);
    assert_eq!(context.bench()[0].moves()[0].pp, 35, "the lead spent no PP");
    rng.assert_exhausted();
}

#[test]
fn an_equal_speed_switch_turn_draws_no_action_order_tie() {
    let dex = Dex::new();
    let build = || {
        let player = mon(&dex, MEOWTH, 50, &[MoveId::TACKLE]);
        let party = vec![
            mon(&dex, MEOWTH, 50, &[MoveId::TACKLE]),
            mon(&dex, MEOWTH, 50, &[MoveId::TACKLE]),
        ];
        // Equal-speed battlers take a second draw at construction.
        battle_with_party_drawing(party, player, 2)
    };

    let mut with_switch = build();
    let mut switch_rng = ScriptedRng::zeros(6);
    turn_with(&mut with_switch, 0, EnemyAction::Switch(1), &mut switch_rng).expect("switches");

    let mut baseline = build();
    let mut baseline_rng = ScriptedRng::zeros(4);
    let mut events = Vec::new();
    baseline
        .act(
            true,
            MoveId::TACKLE,
            Some(0),
            &mut baseline_rng,
            &mut events,
        )
        .expect("the move resolves");

    assert_eq!(
        switch_rng.draws,
        2 + baseline_rng.draws,
        "the turn seed, the player's move, and only the end-of-turn tie draw"
    );
    switch_rng.assert_exhausted();
    baseline_rng.assert_exhausted();
}

#[test]
fn a_switch_resets_the_outgoing_battlers_scratch_and_history_only() {
    let mut rng = ScriptedRng::zeros(1);
    let mut battle = four_mon_battle(&mut rng);
    set_stage(&mut battle.enemy, ChangedStat::Attack, StatStage::MAX);
    battle.enemy.volatiles_mut().set_focus_energy();
    battle.enemy.volatiles_mut().set_charge();
    battle.enemy.volatiles_mut().set_defense_curl();
    battle.enemy.volatiles_mut().set_confusion(3);
    battle.enemy.volatiles_mut().activate_flash_fire();
    battle.last_move_used = MoveId::TACKLE;
    let enemy = BattlerSide::Enemy.index();
    let player = BattlerSide::Player.index();
    battle.battler_states[enemy].record_move_end(
        BattlerSide::Player,
        true,
        LastLandedMove::Move(MoveId::TACKLE),
    );
    battle.battler_states[enemy].set_perish_song(Some(PerishSongTurns(0)));
    battle.battler_states[player].record_move_end(
        BattlerSide::Enemy,
        true,
        LastLandedMove::Unavailable,
    );

    let mut events = Vec::new();
    battle
        .switch_enemy_to(1, &mut events)
        .expect("a healthy non-active slot");

    assert_eq!(
        battle.enemy().stages(),
        crate::pokemon::StatStages::default()
    );
    assert_eq!(
        battle.enemy().volatiles(),
        crate::volatile::Volatiles::default()
    );
    assert_eq!(battle.battler_states[enemy], super::BattlerState::default());
    assert_eq!(battle.last_move_used, MOVE_NONE);
    assert_eq!(
        battle.battler_state(BattlerSide::Player).last_landed_move(),
        LastLandedMove::Unavailable,
        "the other battler keeps its history"
    );
    let lead = &battle.trainer().unwrap().bench()[0];
    assert_eq!(lead.stages(), crate::pokemon::StatStages::default());
    assert_eq!(lead.volatiles(), crate::volatile::Volatiles::default());
    rng.assert_exhausted();
}

#[test]
fn a_forced_replacement_enters_with_clean_history() {
    let mut rng = ScriptedRng::zeros(1);
    let mut battle = four_mon_battle(&mut rng);
    let enemy = BattlerSide::Enemy.index();
    battle.battler_states[enemy].record_move_end(
        BattlerSide::Player,
        true,
        LastLandedMove::Move(MoveId::TACKLE),
    );
    battle.battler_states[enemy].set_perish_song(Some(PerishSongTurns(2)));
    battle.enemy.apply_damage(battle.enemy.stats().max_hp);
    battle.clear_fainted_battler_state(false);
    assert_eq!(
        battle.battler_states[enemy],
        super::BattlerState::default(),
        "fainting clears the history"
    );

    battle.battler_states[enemy].record_move_end(
        BattlerSide::Player,
        true,
        LastLandedMove::Move(MoveId::TACKLE),
    );
    battle.battler_states[enemy].set_perish_song(Some(PerishSongTurns(2)));
    let mut events = Vec::new();
    battle.settle_fainted_enemy(&mut events).expect("replaced");

    assert_eq!(battle.battler_states[enemy], super::BattlerState::default());
    rng.assert_exhausted();
}

#[test]
fn a_player_reserve_enters_with_clean_history() {
    let dex = Dex::new();
    let player = mon(&dex, MEOWTH, 5, &[MoveId::TACKLE]);
    let reserve = mon(&dex, PICHU, 5, &[MoveId::TACKLE]);
    let party = vec![mon(&dex, TREECKO, 50, &[MoveId::TACKLE])];
    let mut rng = ScriptedRng::zeros(1);
    let mut battle = Battle::new_trainer_with_player_reserves(
        dex,
        player,
        vec![reserve],
        MAY_ROUTE_103_MUDKIP,
        party,
        &mut rng,
    )
    .expect("a valid battle");
    let player_side = BattlerSide::Player.index();
    battle.battler_states[player_side].set_perish_song(Some(PerishSongTurns(1)));
    battle.player.apply_damage(battle.player.stats().max_hp);

    battle.last_move_used = MoveId::TACKLE;
    assert!(battle.send_out_next_player_reserve(&mut Vec::new()));

    assert_eq!(
        battle.battler_states[player_side],
        super::BattlerState::default()
    );
    assert_eq!(
        battle.last_move_used, MOVE_NONE,
        "a double faint's trainer replacement scores against MOVE_NONE"
    );
    rng.assert_exhausted();
}

/// Gives a level-5 player and a level-50 Sandshrew target, so the move is
/// the only variable.
fn history_battle(player_moves: &[MoveId]) -> Battle {
    history_battle_against(SANDSHREW, player_moves)
}

fn history_battle_against(target: SpeciesId, player_moves: &[MoveId]) -> Battle {
    let dex = Dex::new();
    let player = mon(&dex, MEOWTH, 5, player_moves);
    let party = vec![mon(&dex, target, 50, &[MoveId::TACKLE])];
    battle_with_party(party, player)
}

fn enemy_state(battle: &Battle) -> (LastLandedMove, Option<BattlerSide>) {
    let state = battle.battler_state(BattlerSide::Enemy);
    (state.last_landed_move(), state.last_hitter())
}

#[test]
fn history_starts_empty() {
    let battle = history_battle(&[MoveId::TACKLE]);

    assert_eq!(enemy_state(&battle), (LastLandedMove::None, None));
    assert_eq!(battle.battler_state(BattlerSide::Enemy).perish_song(), None);
}

#[test]
fn the_landed_move_differs_from_the_attempted_move_when_a_move_misses() {
    let mut battle = history_battle(&[MoveId::TACKLE]);
    let mut events = Vec::new();

    let mut landing = ScriptedRng::zeros(4);
    battle
        .run_action(true, MoveId::TACKLE, Some(0), &mut landing, &mut events)
        .expect("resolves");
    landing.assert_exhausted();
    assert_eq!(
        enemy_state(&battle),
        (
            LastLandedMove::Move(MoveId::TACKLE),
            Some(BattlerSide::Player)
        )
    );

    // Cut has 95 accuracy; a draw of 99 misses.
    let mut missing = ScriptedRng::new(&[99]);
    battle
        .run_action(true, MoveId::CUT, None, &mut missing, &mut events)
        .expect("resolves");
    missing.assert_exhausted();

    assert!(events.contains(&BattleEvent::Missed {
        by_player: true,
        move_id: MoveId::CUT,
    }));
    assert_eq!(battle.last_move_used, MoveId::CUT, "the attempted move");
    assert_eq!(
        enemy_state(&battle),
        (LastLandedMove::Unavailable, Some(BattlerSide::Player)),
        "the miss overwrites the landed move and the hitter survives"
    );
}

#[test]
fn a_type_immune_target_records_no_landed_move() {
    let mut battle = history_battle_against(GASTLY, &[MoveId::TACKLE]);
    let mut rng = ScriptedRng::zeros(4);

    battle
        .run_action(true, MoveId::TACKLE, Some(0), &mut rng, &mut Vec::new())
        .expect("resolves");
    rng.assert_exhausted();

    assert_eq!(
        enemy_state(&battle),
        (LastLandedMove::Unavailable, Some(BattlerSide::Player))
    );
}

#[test]
fn a_status_move_that_takes_effect_is_the_landed_move() {
    let mut battle = history_battle(&[MoveId::GROWL]);
    let mut rng = ScriptedRng::zeros(1);

    battle
        .run_action(true, MoveId::GROWL, Some(0), &mut rng, &mut Vec::new())
        .expect("resolves");
    rng.assert_exhausted();

    assert_eq!(
        enemy_state(&battle),
        (
            LastLandedMove::Move(MoveId::GROWL),
            Some(BattlerSide::Player)
        )
    );
}

#[test]
fn a_cancelled_action_records_an_unavailable_landed_move_but_the_attempted_move() {
    let mut battle = history_battle(&[MoveId::TACKLE]);
    battle.player.set_status1(Status1::Paralysed);
    let mut events = Vec::new();

    // A zero draw is full paralysis, and the cancelled action draws nothing
    // after it.
    let mut rng = ScriptedRng::new(&[0]);
    battle
        .run_action(true, MoveId::TACKLE, Some(0), &mut rng, &mut events)
        .expect("resolves");
    rng.assert_exhausted();

    assert!(events.contains(&BattleEvent::FullyParalyzed {
        by_player: true,
        move_id: MoveId::TACKLE,
    }));
    assert_eq!(battle.last_move_used, MoveId::TACKLE);
    assert_eq!(
        enemy_state(&battle),
        (LastLandedMove::Unavailable, Some(BattlerSide::Player))
    );
}

#[test]
fn a_self_targeting_move_records_on_its_user() {
    let mut battle = history_battle(&[MoveId::FOCUS_ENERGY]);
    let mut events = Vec::new();
    // Focus Energy draws nothing, whether it lands or fails.
    let mut rng = ScriptedRng::new(&[]);

    battle
        .run_action(true, MoveId::FOCUS_ENERGY, Some(0), &mut rng, &mut events)
        .expect("resolves");
    let player = battle.battler_state(BattlerSide::Player);
    assert_eq!(
        (player.last_landed_move(), player.last_hitter()),
        (
            LastLandedMove::Move(MoveId::FOCUS_ENERGY),
            Some(BattlerSide::Player)
        )
    );
    assert_eq!(enemy_state(&battle), (LastLandedMove::None, None));

    battle
        .run_action(true, MoveId::FOCUS_ENERGY, Some(0), &mut rng, &mut events)
        .expect("resolves");
    assert_eq!(
        battle.battler_state(BattlerSide::Player).last_landed_move(),
        LastLandedMove::Unavailable,
        "a repeated Focus Energy fails"
    );
    rng.assert_exhausted();
}

#[test]
fn a_knocked_out_target_keeps_no_hitter_but_records_the_landed_move() {
    let mut battle = history_battle(&[MoveId::TACKLE]);
    battle.enemy.apply_damage(battle.enemy.stats().max_hp - 1);
    let mut rng = ScriptedRng::zeros(4);

    battle
        .run_action(true, MoveId::TACKLE, Some(0), &mut rng, &mut Vec::new())
        .expect("resolves");
    rng.assert_exhausted();

    assert!(battle.enemy().is_fainted());
    assert_eq!(
        enemy_state(&battle),
        (LastLandedMove::Move(MoveId::TACKLE), None)
    );
}

#[test]
fn perish_song_state_is_an_input_that_none_zero_and_a_count_all_distinguish() {
    let mut battle = history_battle(&[MoveId::TACKLE]);
    let enemy = BattlerSide::Enemy.index();

    for turns in [None, Some(PerishSongTurns(0)), Some(PerishSongTurns(3))] {
        battle.battler_states[enemy].set_perish_song(turns);
        assert_eq!(
            battle.battler_state(BattlerSide::Enemy).perish_song(),
            turns
        );
    }
}

#[test]
fn a_finished_action_clears_its_own_landed_move_but_not_its_hitter() {
    let dex = Dex::new();
    let player = mon(&dex, MEOWTH, 50, &[MoveId::TACKLE]);
    let party = vec![mon(&dex, TREECKO, 50, &[MoveId::TACKLE, MoveId::GROWL])];
    let mut battle = battle_with_party(party, player);
    let mut rng = ScriptedRng::zeros(6);

    // The faster player lands Tackle, then the enemy lands Growl.
    turn_with(&mut battle, 0, EnemyAction::Move(1), &mut rng).expect("the turn resolves");

    assert_eq!(
        enemy_state(&battle),
        (LastLandedMove::None, Some(BattlerSide::Player)),
        "the enemy's own action closed after Tackle landed on it"
    );
    assert_eq!(
        battle.battler_state(BattlerSide::Player).last_landed_move(),
        LastLandedMove::Move(MoveId::GROWL),
        "Growl landed on the player after the player's action closed"
    );
    rng.assert_exhausted();
}

#[test]
fn natural_cure_clears_the_outgoing_members_status_on_switch_out() {
    let dex = Dex::new();
    let player = mon(&dex, MEOWTH, 5, &[MoveId::TACKLE, MoveId::FOCUS_ENERGY]);
    let party = vec![
        mon(&dex, SpeciesId(363), 50, &[MoveId::TACKLE, MoveId::GROWL]),
        mon(&dex, TORCHIC, 50, &[MoveId::TACKLE, MoveId::GROWL]),
    ];
    let mut battle = battle_with_party(party, player);
    assert_eq!(battle.enemy().ability(), assets::AbilityId::NATURAL_CURE);
    battle.enemy.set_status1(Status1::Paralysed);
    let mut rng = ScriptedRng::zeros(2);

    turn_with(&mut battle, 1, EnemyAction::Switch(1), &mut rng).expect("a healthy slot");
    turn_with(&mut battle, 1, EnemyAction::Switch(0), &mut rng).expect("back to the lead");

    assert_eq!(battle.enemy().species(), SpeciesId(363));
    assert_eq!(battle.enemy().status1(), Status1::Healthy);
    rng.assert_exhausted();
}

#[test]
fn a_member_without_natural_cure_keeps_its_status_through_a_switch() {
    let mut rng = ScriptedRng::zeros(3);
    let mut battle = four_mon_battle(&mut rng);
    assert_ne!(battle.enemy().ability(), assets::AbilityId::NATURAL_CURE);
    battle.enemy.set_status1(Status1::Paralysed);

    turn_with(&mut battle, 1, EnemyAction::Switch(1), &mut rng).expect("a healthy slot");
    turn_with(&mut battle, 1, EnemyAction::Switch(0), &mut rng).expect("back to the lead");

    assert_eq!(battle.enemy().status1(), Status1::Paralysed);
    rng.assert_exhausted();
}

#[test]
fn an_invalid_switch_does_not_cure_a_natural_cure_member() {
    let dex = Dex::new();
    let player = mon(&dex, MEOWTH, 5, &[MoveId::TACKLE]);
    let party = vec![mon(&dex, SpeciesId(363), 50, &[MoveId::TACKLE])];
    let mut battle = battle_with_party(party, player);
    battle.enemy.set_status1(Status1::Paralysed);

    let mut rng = ScriptedRng::zeros(1);
    turn_with(&mut battle, 0, EnemyAction::Switch(0), &mut rng).expect_err("the active slot");

    assert_eq!(battle.enemy().status1(), Status1::Paralysed);
    rng.assert_exhausted();
}
