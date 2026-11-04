//! The flinch secondary and its action canceller, driven through complete
//! turns. `MOVE_EFFECT_FLINCH` is set silently after a landed hit's chance
//! draw (`SetMoveEffect`, `pokeemerald/src/battle_script_commands.c:2547-2567`;
//! `Cmd_seteffectwithchance`, `:2908-2939`); `CANCELER_FLINCH` consumes it and
//! ends the target's action ahead of confusion and paralysis
//! (`pokeemerald/src/battle_util.c:2110-2121`); `HandleEndTurn_ContinueBattle`
//! clears every battler's flag (`pokeemerald/src/battle_main.c:3941-3945`).

use crate::common::{max_iv_mon, SequenceRng};
use assets::MoveId;
use battle::{Battle, BattleEvent, BattleOutcome, BattlePokemon, Dex, PlayerAction, Status1};

const HEADBUTT: MoveId = MoveId::HEADBUTT;
const BITE: MoveId = MoveId::BITE;
const ASTONISH: MoveId = MoveId::ASTONISH;
const TACKLE: MoveId = MoveId::TACKLE;

/// `SPECIES_RATTATA`, outspeeding every level-5 enemy below.
const RATTATA: u16 = 19;
const ZIGZAGOON: u16 = 288;
/// Dark-type: takes Astonish's Ghost damage.
const POOCHYENA: u16 = 286;
/// Slow and bulky: survives two of the player's hits.
const MAKUHITA: u16 = 335;
/// Shield Dust in its only ability slot.
const WURMPLE: u16 = 290;
/// Inner Focus in its only ability slot.
const ZUBAT: u16 = 41;

/// Construction's turn-number draw, the turn-start draw, and the enemy's move
/// selection.
const PREFIX: [u16; 3] = [0, 0, 0];
/// A turn after the first: the turn-start draw and the enemy's selection.
const NEXT_TURN_PREFIX: [u16; 2] = [0, 0];
/// One landing Tackle: accuracy, non-critical, damage roll, discarded
/// secondary-effect draw.
const ENEMY_HIT: [u16; 4] = [0, 1, 0, 0];
/// Residues under and over the 30% moves' chance.
const FLINCHES: u16 = 29;
const NO_FLINCH: u16 = 30;

/// A landed attacker hit: accuracy, non-critical, damage roll, chance.
fn hit_with_chance(chance: u16) -> [u16; 4] {
    [0, 1, 0, chance]
}

fn script(groups: &[&[u16]]) -> Vec<u16> {
    groups.concat()
}

fn player_with(move_id: MoveId) -> BattlePokemon {
    max_iv_mon(&Dex::new(), RATTATA, 5, vec![move_id])
}

fn enemy_of(species: u16) -> BattlePokemon {
    max_iv_mon(&Dex::new(), species, 5, vec![TACKLE])
}

fn start(player: BattlePokemon, enemy: BattlePokemon, draws: Vec<u16>) -> (Battle, SequenceRng) {
    let mut rng = SequenceRng::new(draws);
    let battle = Battle::new(Dex::new(), player, enemy, false, &mut rng).unwrap();
    (battle, rng)
}

fn turn(battle: &mut Battle, rng: &mut SequenceRng) -> Vec<BattleEvent> {
    battle.take_turn(PlayerAction::UseMove(0), rng).unwrap()
}

fn flinched(events: &[BattleEvent], by_player: bool) -> bool {
    events.iter().any(|event| {
        matches!(event, BattleEvent::Flinched { by_player: side, .. } if *side == by_player)
    })
}

fn used_move(events: &[BattleEvent], by_player: bool) -> bool {
    events.iter().any(|event| {
        matches!(event, BattleEvent::UsedMove { by_player: side, .. } if *side == by_player)
    })
}

#[test]
fn each_flinch_move_costs_the_slower_target_its_action_in_exactly_four_attacker_draws() {
    for (move_id, species) in [
        (HEADBUTT, ZIGZAGOON),
        (BITE, ZIGZAGOON),
        (ASTONISH, POOCHYENA),
    ] {
        let enemy = enemy_of(species);
        let enemy_pp = enemy.moves()[0].pp;
        let (mut battle, mut rng) = start(
            player_with(move_id),
            enemy,
            script(&[&PREFIX, &hit_with_chance(FLINCHES)]),
        );

        let events = turn(&mut battle, &mut rng);

        // Three setup draws plus accuracy, critical, variance and chance; the
        // enemy's Tackle never draws.
        assert_eq!(rng.draws(), 3 + 4, "{move_id:?}");
        rng.assert_exhausted();
        assert!(
            matches!(
                events[0],
                BattleEvent::UsedMove {
                    by_player: true,
                    ..
                }
            ),
            "{events:?}"
        );
        assert!(
            matches!(
                events[1],
                BattleEvent::Hit {
                    by_player: true,
                    ..
                }
            ),
            "{events:?}"
        );
        assert!(
            events.contains(&BattleEvent::Flinched {
                by_player: false,
                move_id: TACKLE,
            }),
            "{move_id:?}: {events:?}"
        );
        assert!(!used_move(&events, false), "{move_id:?}: {events:?}");
        assert_eq!(battle.enemy().moves()[0].pp, enemy_pp, "{move_id:?}");
        assert_eq!(
            battle.player().current_hp(),
            battle.player().stats().max_hp,
            "{move_id:?}: the flinched enemy never hit back"
        );
        assert!(!battle.enemy().volatiles().flinched, "{move_id:?}");
    }
}

#[test]
fn infliction_itself_draws_nothing_and_emits_no_event_before_the_cancelled_action() {
    let (mut battle, mut rng) = start(
        player_with(HEADBUTT),
        enemy_of(ZIGZAGOON),
        script(&[&PREFIX, &hit_with_chance(FLINCHES)]),
    );

    let events = turn(&mut battle, &mut rng);

    // UsedMove, Hit, then straight to the flinched enemy's cancelled action.
    assert_eq!(events.len(), 3, "{events:?}");
    assert!(
        matches!(events[2], BattleEvent::Flinched { .. }),
        "{events:?}"
    );
}

#[test]
fn residue_thirty_leaves_the_target_free_to_act() {
    for chance in [NO_FLINCH, 99, 130] {
        let (mut battle, mut rng) = start(
            player_with(HEADBUTT),
            enemy_of(ZIGZAGOON),
            script(&[&PREFIX, &hit_with_chance(chance), &ENEMY_HIT]),
        );

        let events = turn(&mut battle, &mut rng);

        rng.assert_exhausted();
        assert!(!flinched(&events, false), "chance {chance}: {events:?}");
        assert!(used_move(&events, false), "chance {chance}: {events:?}");
    }
}

#[test]
fn wrapped_residues_flinch_like_their_base_residue() {
    let (mut battle, mut rng) = start(
        player_with(HEADBUTT),
        enemy_of(ZIGZAGOON),
        script(&[&PREFIX, &hit_with_chance(129)]),
    );

    let events = turn(&mut battle, &mut rng);

    rng.assert_exhausted();
    assert!(flinched(&events, false), "{events:?}");
}

#[test]
fn shield_dust_and_inner_focus_targets_never_flinch_but_the_chance_draw_is_spent() {
    for species in [WURMPLE, ZUBAT] {
        let (mut battle, mut rng) = start(
            player_with(HEADBUTT),
            enemy_of(species),
            script(&[&PREFIX, &hit_with_chance(0), &ENEMY_HIT]),
        );

        let events = turn(&mut battle, &mut rng);

        rng.assert_exhausted();
        assert!(!flinched(&events, false), "species {species}: {events:?}");
        assert!(used_move(&events, false), "species {species}: {events:?}");
    }
}

#[test]
fn a_lethal_hit_spends_the_chance_draw_and_the_fainted_target_never_flinches() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 50, vec![HEADBUTT]);
    let mut enemy = enemy_of(ZIGZAGOON);
    enemy.apply_damage(enemy.stats().max_hp - 1);
    let (mut battle, mut rng) = start(
        player,
        enemy,
        script(&[&PREFIX, &hit_with_chance(FLINCHES)]),
    );

    let events = turn(&mut battle, &mut rng);

    assert!(
        events.contains(&BattleEvent::Fainted { by_player: false }),
        "fixture sanity -- the hit must be lethal: {events:?}"
    );
    assert!(!flinched(&events, false), "{events:?}");
    rng.assert_exhausted();
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
}

#[test]
fn a_target_that_already_acted_is_not_flinched_and_acts_normally_next_turn() {
    // The enemy outspeeds the player, so it has already acted when the
    // player's Headbutt lands; the chance roll would otherwise flinch.
    let dex = Dex::new();
    let player = max_iv_mon(&dex, ZIGZAGOON, 20, vec![HEADBUTT]);
    let enemy = max_iv_mon(&dex, RATTATA, 20, vec![TACKLE]);
    assert!(player.stats().speed < enemy.stats().speed, "fixture sanity");
    let (mut battle, mut rng) = start(
        player,
        enemy,
        script(&[
            &PREFIX,
            &ENEMY_HIT,
            &hit_with_chance(0),
            &NEXT_TURN_PREFIX,
            &ENEMY_HIT,
            &hit_with_chance(NO_FLINCH),
        ]),
    );

    let first = turn(&mut battle, &mut rng);
    assert!(used_move(&first, false), "{first:?}");
    assert!(used_move(&first, true), "{first:?}");
    assert!(!flinched(&first, true), "{first:?}");
    assert!(!flinched(&first, false), "{first:?}");

    // Had the stale flag survived, the enemy would lose this action.
    let second = turn(&mut battle, &mut rng);
    rng.assert_exhausted();
    assert!(used_move(&second, false), "{second:?}");
    assert!(!flinched(&second, false), "{second:?}");
}

#[test]
fn a_flinch_cancels_only_its_own_turn() {
    let (mut battle, mut rng) = start(
        player_with(HEADBUTT),
        max_iv_mon(&Dex::new(), MAKUHITA, 8, vec![TACKLE]),
        script(&[
            &PREFIX,
            &hit_with_chance(FLINCHES),
            &NEXT_TURN_PREFIX,
            &hit_with_chance(NO_FLINCH),
            &ENEMY_HIT,
        ]),
    );

    let first = turn(&mut battle, &mut rng);
    assert!(flinched(&first, false), "{first:?}");
    assert!(!battle.enemy().volatiles().flinched);

    let second = turn(&mut battle, &mut rng);
    rng.assert_exhausted();
    assert!(!flinched(&second, false), "{second:?}");
    assert!(used_move(&second, false), "{second:?}");
}

#[test]
fn a_flinched_battler_loses_its_action_without_pp_or_confusion_or_paralysis_draws() {
    let mut player = max_iv_mon(&Dex::new(), RATTATA, 20, vec![HEADBUTT]);
    player.volatiles_mut().set_confusion(3);
    player.set_status1(Status1::Paralysed);
    player.volatiles_mut().set_flinched();
    let starting_pp = player.moves()[0].pp;
    // Paralysis quarters Speed, so the enemy acts first; the flinched player
    // then draws nothing at all.
    let (mut battle, mut rng) = start(player, enemy_of(ZIGZAGOON), script(&[&PREFIX, &ENEMY_HIT]));
    assert!(battle.player().stats().speed > 0);

    let events = turn(&mut battle, &mut rng);

    rng.assert_exhausted();
    let flinch_index = events
        .iter()
        .position(|event| {
            *event
                == BattleEvent::Flinched {
                    by_player: true,
                    move_id: HEADBUTT,
                }
        })
        .unwrap_or_else(|| panic!("{events:?}"));
    assert!(!used_move(&events, true), "{events:?}");
    assert!(
        !events.iter().any(|event| matches!(
            event,
            BattleEvent::Confused { .. }
                | BattleEvent::SnappedOutOfConfusion { .. }
                | BattleEvent::ConfusionSelfHit { .. }
                | BattleEvent::FullyParalyzed { .. }
        )),
        "{events:?}"
    );
    assert!(flinch_index > 0, "the faster enemy acted first: {events:?}");
    assert_eq!(battle.player().moves()[0].pp, starting_pp);
    assert_eq!(battle.player().volatiles().confusion_turns, 3);
    assert!(!battle.player().volatiles().flinched);
}

#[test]
fn a_flinched_lead_that_acts_first_emits_the_ordered_event_before_anything_else() {
    let mut player = player_with(HEADBUTT);
    player.volatiles_mut().set_flinched();
    let starting_pp = player.moves()[0].pp;
    let (mut battle, mut rng) = start(player, enemy_of(ZIGZAGOON), script(&[&PREFIX, &ENEMY_HIT]));

    let events = turn(&mut battle, &mut rng);

    rng.assert_exhausted();
    assert_eq!(
        events[0],
        BattleEvent::Flinched {
            by_player: true,
            move_id: HEADBUTT,
        }
    );
    assert!(
        matches!(
            events[1],
            BattleEvent::UsedMove {
                by_player: false,
                ..
            }
        ),
        "{events:?}"
    );
    assert_eq!(battle.player().moves()[0].pp, starting_pp);
}

#[test]
fn a_thaw_precedes_the_flinch_of_the_same_action() {
    let mut player = player_with(HEADBUTT);
    player.set_status1(Status1::Frozen);
    player.volatiles_mut().set_flinched();
    // The freeze draw of zero thaws; the flinch then cancels the move.
    let (mut battle, mut rng) = start(
        player,
        enemy_of(ZIGZAGOON),
        script(&[&PREFIX, &[0], &ENEMY_HIT]),
    );

    let events = turn(&mut battle, &mut rng);

    rng.assert_exhausted();
    assert_eq!(events[0], BattleEvent::Thawed { by_player: true });
    assert_eq!(
        events[1],
        BattleEvent::Flinched {
            by_player: true,
            move_id: HEADBUTT,
        }
    );
}

#[test]
fn an_earlier_canceller_wins_and_the_unconsumed_flag_clears_at_end_of_turn() {
    let mut player = player_with(HEADBUTT);
    player.set_status1(Status1::Frozen);
    player.volatiles_mut().set_flinched();
    let (mut battle, mut rng) = start(
        player,
        enemy_of(ZIGZAGOON),
        script(&[
            &PREFIX,
            // Frozen solid: a nonzero residue keeps the lead frozen.
            &[1],
            &ENEMY_HIT,
            // Next turn the lead thaws and its Headbutt proceeds unflinched.
            &NEXT_TURN_PREFIX,
            &[0],
            &hit_with_chance(NO_FLINCH),
            &ENEMY_HIT,
        ]),
    );

    let first = turn(&mut battle, &mut rng);
    assert!(
        first.contains(&BattleEvent::FrozenSolid {
            by_player: true,
            move_id: HEADBUTT,
        }),
        "{first:?}"
    );
    assert!(!flinched(&first, true), "{first:?}");
    assert!(
        !battle.player().volatiles().flinched,
        "the end-of-turn clear runs even though the canceller never did"
    );

    let second = turn(&mut battle, &mut rng);
    rng.assert_exhausted();
    assert!(!flinched(&second, true), "{second:?}");
    assert!(used_move(&second, true), "{second:?}");
}
