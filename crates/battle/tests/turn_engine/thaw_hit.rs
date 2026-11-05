//! Flame Wheel and Sacred Fire (`EFFECT_THAW_HIT`) through a real `Battle`:
//! the burn secondary (`SetMoveEffect`'s `MOVE_EFFECT_BURN`,
//! `pokeemerald/src/battle_script_commands.c:2340-2379`), `CANCELER_THAW`
//! (`battle_util.c:2250-2262`) and `MOVEEND_DEFROST`
//! (`battle_script_commands.c:4257-4272`). Every script is exact:
//! `SequenceRng::assert_exhausted` fails on a missing or an extra draw.

use crate::common::{max_iv_mon, SequenceRng};
use assets::trainers::TrainerId;
use assets::{AbilityId, MoveId, SpeciesId};
use battle::{Battle, BattleError, BattleEvent, BattlePokemon, Dex, PlayerAction, Status1};

const TACKLE: MoveId = MoveId::TACKLE;
const FLAME_WHEEL: MoveId = MoveId::FLAME_WHEEL;
const SACRED_FIRE: MoveId = MoveId::SACRED_FIRE;

/// `SPECIES_RATTATA`: Normal-type and faster than [`ZIGZAGOON`] at equal levels.
const RATTATA: u16 = 19;
const ZIGZAGOON: u16 = 288;
const LEVEL: u8 = 20;

/// Construction's turn-number draw, the turn-start draw, and the enemy's
/// move selection.
const PREFIX: [u16; 3] = [0, 0, 0];
/// One landing Tackle: accuracy, non-critical, damage roll, discarded
/// secondary-effect draw.
const TACKLE_HIT: [u16; 4] = [0, 1, 0, 0];
/// A thaw-hit move that lands: accuracy, non-critical, damage roll, then the
/// secondary-effect draw given by the caller.
const fn fire_hit(effect_draw: u16) -> [u16; 4] {
    [0, 1, 0, effect_draw]
}

fn player(move_id: MoveId, status: Status1) -> BattlePokemon {
    let mut mon = max_iv_mon(&Dex::new(), RATTATA, LEVEL, vec![move_id]);
    mon.set_status1(status);
    mon
}

fn enemy(status: Status1) -> BattlePokemon {
    let mut mon = max_iv_mon(&Dex::new(), ZIGZAGOON, LEVEL, vec![TACKLE]);
    mon.set_status1(status);
    mon
}

fn run(
    player: BattlePokemon,
    enemy: BattlePokemon,
    groups: &[&[u16]],
) -> (Battle, Vec<BattleEvent>, SequenceRng) {
    let mut rng = SequenceRng::new(groups.concat());
    let mut battle = Battle::new(Dex::new(), player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    (battle, events, rng)
}

fn position(events: &[BattleEvent], wanted: &BattleEvent) -> usize {
    events
        .iter()
        .position(|event| event == wanted)
        .unwrap_or_else(|| panic!("{wanted:?} missing from {events:?}"))
}

fn burned(move_id: MoveId) -> BattleEvent {
    BattleEvent::Burned {
        by_player: true,
        move_id,
    }
}

fn enemy_hurt_by_burn(events: &[BattleEvent]) -> bool {
    events.iter().any(|event| {
        matches!(
            event,
            BattleEvent::HurtByBurn {
                by_player: false,
                ..
            }
        )
    })
}

#[test]
fn a_frozen_user_surviving_the_draw_thaws_with_its_move_and_burns_the_target() {
    for (move_id, landing_draw) in [(FLAME_WHEEL, 9), (SACRED_FIRE, 49)] {
        let user = player(move_id, Status1::Frozen);
        let starting_pp = user.moves()[0].pp;

        // Prefix, one freeze draw (residue 1: still frozen, but the move
        // survives it), the move's four draws, then the enemy's Tackle.
        let (battle, events, rng) = run(
            user,
            enemy(Status1::Healthy),
            &[&PREFIX, &[1], &fire_hit(landing_draw), &TACKLE_HIT],
        );

        assert_eq!(rng.draws(), 3 + 1 + 4 + 4, "{move_id:?}");
        rng.assert_exhausted();
        let thawed = position(
            &events,
            &BattleEvent::ThawedByMove {
                by_player: true,
                move_id,
            },
        );
        let announced = position(
            &events,
            &BattleEvent::UsedMove {
                by_player: true,
                move_id,
            },
        );
        assert!(thawed < announced, "the thaw announces ahead of the move");
        assert!(
            position(&events, &burned(move_id)) > announced,
            "the burn follows the hit"
        );
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, BattleEvent::Thawed { .. })),
            "no second draw thaws the user: {events:?}"
        );
        assert_eq!(battle.player().status1(), Status1::Healthy);
        assert_eq!(battle.player().moves()[0].pp, starting_pp - 1);
        assert_eq!(battle.enemy().status1(), Status1::Burned, "{move_id:?}");
        assert!(
            enemy_hurt_by_burn(&events),
            "the fresh burn ticks at end of turn: {events:?}"
        );
    }
}

#[test]
fn the_burn_chance_is_strictly_below_the_moves_secondary_chance() {
    for (move_id, draw, lands) in [
        (FLAME_WHEEL, 9, true),
        (FLAME_WHEEL, 10, false),
        (FLAME_WHEEL, 109, true),
        (FLAME_WHEEL, 110, false),
        (SACRED_FIRE, 49, true),
        (SACRED_FIRE, 50, false),
        (SACRED_FIRE, 149, true),
        (SACRED_FIRE, 150, false),
    ] {
        let (battle, events, rng) = run(
            player(move_id, Status1::Healthy),
            enemy(Status1::Healthy),
            &[&PREFIX, &fire_hit(draw), &TACKLE_HIT],
        );

        // The chance draw is spent either way and nothing else draws.
        assert_eq!(rng.draws(), 3 + 4 + 4, "{move_id:?} {draw}");
        rng.assert_exhausted();
        assert_eq!(
            battle.enemy().status1() == Status1::Burned,
            lands,
            "{move_id:?} {draw}"
        );
        assert_eq!(
            events.contains(&burned(move_id)),
            lands,
            "{move_id:?} {draw}"
        );
    }
}

#[test]
fn a_burned_target_hits_for_less_and_the_user_is_never_burned() {
    let (healthy, _, _) = run(
        player(FLAME_WHEEL, Status1::Healthy),
        enemy(Status1::Healthy),
        &[&PREFIX, &fire_hit(10), &TACKLE_HIT],
    );
    let (scorched, _, _) = run(
        player(FLAME_WHEEL, Status1::Healthy),
        enemy(Status1::Healthy),
        &[&PREFIX, &fire_hit(9), &TACKLE_HIT],
    );

    // The player is faster, so the burn lands before the enemy's Tackle.
    assert!(
        scorched.player().current_hp() > healthy.player().current_hp(),
        "a freshly burned enemy's Tackle is halved"
    );
    assert_eq!(scorched.player().status1(), Status1::Healthy);
}

#[test]
fn a_confusion_self_hit_ahead_of_the_thaw_stage_leaves_the_user_frozen() {
    let mut user = player(FLAME_WHEEL, Status1::Frozen);
    user.volatiles_mut().set_confusion(3);
    let starting_pp = user.moves()[0].pp;

    // Prefix, the freeze draw (residue 1), the confusion coin (even:
    // self-hit) and its damage roll, then the enemy's Tackle. No accuracy,
    // critical or effect draw follows: the move never starts.
    let (battle, events, rng) = run(
        user,
        enemy(Status1::Healthy),
        &[&PREFIX, &[1], &[0, 0], &TACKLE_HIT],
    );

    assert_eq!(rng.draws(), 3 + 1 + 2 + 4);
    rng.assert_exhausted();
    assert!(events.contains(&BattleEvent::Confused { by_player: true }));
    assert!(
        events.iter().any(|event| matches!(
            event,
            BattleEvent::ConfusionSelfHit {
                by_player: true,
                ..
            }
        )),
        "{events:?}"
    );
    assert!(!events.iter().any(|event| matches!(
        event,
        BattleEvent::ThawedByMove { .. } | BattleEvent::Thawed { .. }
    )));
    assert_eq!(battle.player().status1(), Status1::Frozen);
    assert_eq!(battle.player().moves()[0].pp, starting_pp);
    assert_eq!(battle.enemy().status1(), Status1::Healthy);
}

#[test]
fn a_fire_hit_thaws_a_frozen_target_and_blocks_the_burn_on_that_hit() {
    // The hit lands with a burn-passing draw, yet the target's freeze blocks
    // the burn; move end then thaws it, so it acts without a freeze draw.
    let (battle, events, rng) = run(
        player(FLAME_WHEEL, Status1::Healthy),
        enemy(Status1::Frozen),
        &[&PREFIX, &fire_hit(9), &TACKLE_HIT],
    );

    assert_eq!(rng.draws(), 3 + 4 + 4);
    rng.assert_exhausted();
    let defrosted = position(
        &events,
        &BattleEvent::TargetDefrosted {
            by_player: true,
            move_id: FLAME_WHEEL,
        },
    );
    let enemy_move = position(
        &events,
        &BattleEvent::UsedMove {
            by_player: false,
            move_id: TACKLE,
        },
    );
    assert!(defrosted < enemy_move);
    assert!(!events.contains(&burned(FLAME_WHEEL)));
    assert_eq!(battle.enemy().status1(), Status1::Healthy);
}

#[test]
fn a_non_fire_hit_leaves_a_frozen_target_frozen() {
    let (battle, events, rng) = run(
        player(TACKLE, Status1::Healthy),
        enemy(Status1::Frozen),
        // The enemy's freeze draw (residue 1) cancels its move.
        &[&PREFIX, &TACKLE_HIT, &[1]],
    );

    assert_eq!(rng.draws(), 3 + 4 + 1);
    rng.assert_exhausted();
    assert!(!events
        .iter()
        .any(|event| matches!(event, BattleEvent::TargetDefrosted { .. })));
    assert_eq!(battle.enemy().status1(), Status1::Frozen);
}

#[test]
fn a_missed_fire_move_does_not_thaw_its_target() {
    let (battle, events, rng) = run(
        player(SACRED_FIRE, Status1::Healthy),
        enemy(Status1::Frozen),
        // Sacred Fire's 95 accuracy rolls `95 % 100 + 1 = 96` and misses;
        // the enemy's freeze draw (residue 1) then cancels its move.
        &[&PREFIX, &[95], &[1]],
    );

    assert_eq!(rng.draws(), 3 + 1 + 1);
    rng.assert_exhausted();
    assert!(!events
        .iter()
        .any(|event| matches!(event, BattleEvent::TargetDefrosted { .. })));
    assert_eq!(battle.enemy().status1(), Status1::Frozen);
}

#[test]
fn a_fire_hit_that_faints_its_frozen_target_reports_no_thaw() {
    let dex = Dex::new();
    let strong = max_iv_mon(&dex, RATTATA, 50, vec![SACRED_FIRE]);
    let mut weak = max_iv_mon(&dex, ZIGZAGOON, 3, vec![TACKLE]);
    weak.set_status1(Status1::Frozen);
    let mut rng = SequenceRng::new(
        [&PREFIX[..], &fire_hit(0)[..]]
            .concat()
            .into_iter()
            .chain([0; 8]),
    );
    let mut battle = Battle::new(dex, strong, weak, false, &mut rng).unwrap();

    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(battle.enemy().is_fainted());
    assert!(!events.iter().any(|event| matches!(
        event,
        BattleEvent::TargetDefrosted { .. } | BattleEvent::Burned { .. }
    )));
}

#[test]
fn flame_wheel_and_sacred_fire_are_admitted_on_every_participant_path() {
    let dex = Dex::new();
    let both = || {
        max_iv_mon(
            &Dex::new(),
            ZIGZAGOON,
            LEVEL,
            vec![FLAME_WHEEL, SACRED_FIRE],
        )
    };
    let player = || max_iv_mon(&Dex::new(), RATTATA, LEVEL, vec![FLAME_WHEEL, SACRED_FIRE]);
    let generous = || SequenceRng::new([1; 256]);

    // A wild enemy, ordinary and first battle, and the player's own picks.
    for first_battle in [false, true] {
        let mut rng = generous();
        let mut battle = Battle::new(Dex::new(), player(), both(), first_battle, &mut rng)
            .unwrap_or_else(|error| panic!("wild enemy refused: {error:?}"));
        for slot in 0..2 {
            battle
                .take_turn(PlayerAction::UseMove(slot), &mut rng)
                .unwrap_or_else(|error| panic!("player slot {slot} refused: {error:?}"));
        }
    }

    // A trainer's party, active member and bench, under its AI scripts.
    let mut rng = generous();
    // The trainer path refuses a player species with an ambiguous ability.
    let mudkip = max_iv_mon(&dex, SpeciesId::MUDKIP.0, LEVEL, vec![FLAME_WHEEL]);
    let mut battle =
        Battle::new_trainer(dex, mudkip, TrainerId(529), vec![both(), both()], &mut rng)
            .unwrap_or_else(|error| panic!("trainer party refused: {error:?}"));
    battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap_or_else(|error| panic!("trainer turn refused: {error:?}"));
}

#[test]
fn the_enemys_burn_lands_on_the_player_and_ticks_at_end_of_turn() {
    let dex = Dex::new();
    let mut rng = SequenceRng::new([
        // Construction, turn start, and the wild enemy's selection of slot 0.
        0, 0, 0, // The player is faster: Tackle, then the enemy's Flame Wheel
        // with a burn-passing effect draw.
        0, 1, 0, 0, 0, 1, 0, 9,
    ]);
    let mut battle = Battle::new(
        Dex::new(),
        max_iv_mon(&dex, RATTATA, LEVEL, vec![TACKLE]),
        max_iv_mon(&dex, ZIGZAGOON, LEVEL, vec![FLAME_WHEEL]),
        false,
        &mut rng,
    )
    .unwrap();

    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    rng.assert_exhausted();
    assert!(events.contains(&BattleEvent::Burned {
        by_player: false,
        move_id: FLAME_WHEEL,
    }));
    assert_eq!(battle.player().status1(), Status1::Burned);
    assert!(events.iter().any(|event| matches!(
        event,
        BattleEvent::HurtByBurn {
            by_player: true,
            ..
        }
    )));
}

/// `SPECIES_VULPIX`: Flash Fire in its primary ability slot.
const VULPIX: u16 = 37;

#[test]
fn a_flash_fire_holder_absorbs_flame_wheel_with_only_an_accuracy_draw() {
    let dex = Dex::new();
    let vulpix = max_iv_mon(&dex, VULPIX, LEVEL, vec![TACKLE]);
    assert_eq!(vulpix.ability(), AbilityId::FLASH_FIRE);

    // Accuracy only for the absorbed move; then the enemy's Tackle.
    let (battle, events, rng) = run(
        player(FLAME_WHEEL, Status1::Healthy),
        vulpix,
        &[&PREFIX, &[0], &TACKLE_HIT],
    );

    assert_eq!(rng.draws(), 3 + 1 + 4);
    rng.assert_exhausted();
    assert!(events.contains(&BattleEvent::FlashFireAbsorbed {
        by_player: true,
        move_id: FLAME_WHEEL,
        boosted: true,
    }));
    assert!(battle.enemy().volatiles().flash_fire);
    assert_eq!(battle.enemy().status1(), Status1::Healthy);
}

#[test]
fn a_frozen_flash_fire_holder_is_hit_and_thawed_instead_of_absorbing() {
    let dex = Dex::new();
    let mut vulpix = max_iv_mon(&dex, VULPIX, LEVEL, vec![TACKLE]);
    vulpix.set_status1(Status1::Frozen);

    let (battle, events, rng) = run(
        player(FLAME_WHEEL, Status1::Healthy),
        vulpix,
        &[&PREFIX, &fire_hit(9), &TACKLE_HIT],
    );

    assert_eq!(rng.draws(), 3 + 4 + 4);
    rng.assert_exhausted();
    assert!(!events
        .iter()
        .any(|event| matches!(event, BattleEvent::FlashFireAbsorbed { .. })));
    assert!(events.contains(&BattleEvent::TargetDefrosted {
        by_player: true,
        move_id: FLAME_WHEEL,
    }));
    assert!(!battle.enemy().volatiles().flash_fire);
    assert_eq!(battle.enemy().status1(), Status1::Healthy);
}

#[test]
fn burn_hits_keep_refusing_the_unported_ability_interactions() {
    let dex = Dex::new();
    let ralts = max_iv_mon(&dex, SpeciesId::RALTS.0, LEVEL, vec![TACKLE]);
    assert_eq!(ralts.ability(), AbilityId::SYNCHRONIZE);
    let dunsparce = max_iv_mon(&dex, SpeciesId::DUNSPARCE.0, LEVEL, vec![FLAME_WHEEL]);
    assert_eq!(dunsparce.ability(), AbilityId::SERENE_GRACE);

    // A burn that could land on a Synchronize holder would reflect onto the
    // attacker, which the engine does not model.
    let mut rng = SequenceRng::new([]);
    let refusal = Battle::new(
        Dex::new(),
        ralts,
        max_iv_mon(&dex, ZIGZAGOON, LEVEL, vec![FLAME_WHEEL]),
        false,
        &mut rng,
    )
    .err();
    assert_eq!(
        refusal,
        Some(BattleError::UnportedAbilityInteraction(
            AbilityId::SYNCHRONIZE
        ))
    );
    rng.assert_exhausted();

    let mut rng = SequenceRng::new([]);
    let refusal = Battle::new(
        Dex::new(),
        max_iv_mon(&dex, ZIGZAGOON, LEVEL, vec![TACKLE]),
        dunsparce,
        false,
        &mut rng,
    )
    .err();
    assert_eq!(
        refusal,
        Some(BattleError::UnportedAbilityInteraction(
            AbilityId::SERENE_GRACE
        ))
    );
    rng.assert_exhausted();
}

#[test]
fn blaze_kick_stays_refused_despite_sharing_the_burn_byte() {
    let dex = Dex::new();
    let mut rng = SequenceRng::new([]);
    let refusal = Battle::new(
        Dex::new(),
        max_iv_mon(&dex, RATTATA, LEVEL, vec![TACKLE]),
        max_iv_mon(&dex, ZIGZAGOON, LEVEL, vec![MoveId::BLAZE_KICK]),
        false,
        &mut rng,
    )
    .err();
    assert_eq!(
        refusal,
        Some(BattleError::UnsupportedMoveEffect(MoveId::BLAZE_KICK))
    );
}
