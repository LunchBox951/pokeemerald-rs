//! A frozen loaded lead: `CANCELER_FROZEN` runs after the sleep canceller and
//! before confusion and paralysis with exactly one `Random() % 5` draw
//! (`pokeemerald/src/battle_util.c:2059`-`:2083`). Zero thaws and proceeds;
//! any other value cancels the move before PP is spent. `Status1::Frozen` is
//! `STATUS1_FREEZE` (`pokeemerald/include/constants/battle.h:119`).
//!
//! The thaw-hit exception (`EFFECT_THAW_HIT`, `CANCELER_THAW`,
//! `battle_util.c:2066`-`:2072`, `:2250`-`:2262`) is pinned at the canceller
//! itself in `status1.rs`'s unit tests: Flame Wheel and Sacred Fire carry an
//! unported burn secondary, so no constructor admits them yet.

use crate::common::{max_iv_mon, SequenceRng};
use assets::{AbilityId, MoveId, SpeciesId};
use battle::{Battle, BattleError, BattleEvent, BattlePokemon, Dex, PlayerAction, Status1};

const TACKLE: MoveId = MoveId::TACKLE;

/// `SPECIES_RATTATA`: Normal-type and faster than [`ZIGZAGOON`] at equal levels.
const RATTATA: u16 = 19;
const ZIGZAGOON: u16 = 288;
const SEVIPER: u16 = 379;
const LEVEL: u8 = 20;

/// Construction's turn-number draw, the turn-start draw, and the enemy's
/// move selection.
const PREFIX: [u16; 3] = [0, 0, 0];
/// One landing Tackle: accuracy, non-critical, damage roll, discarded
/// secondary-effect draw.
const HIT: [u16; 4] = [0, 1, 0, 0];

fn frozen_player() -> BattlePokemon {
    let mut mon = max_iv_mon(&Dex::new(), RATTATA, LEVEL, vec![TACKLE]);
    mon.set_status1(Status1::Frozen);
    mon
}

fn enemy() -> BattlePokemon {
    max_iv_mon(&Dex::new(), ZIGZAGOON, LEVEL, vec![TACKLE])
}

fn script(freeze_draw: u16, rest: &[&[u16]]) -> Vec<u16> {
    let mut draws = PREFIX.to_vec();
    draws.push(freeze_draw);
    for group in rest {
        draws.extend_from_slice(group);
    }
    draws
}

fn run(
    player: BattlePokemon,
    enemy: BattlePokemon,
    draws: Vec<u16>,
) -> (Battle, Vec<BattleEvent>, SequenceRng) {
    let mut rng = SequenceRng::new(draws);
    let mut battle = Battle::new(Dex::new(), player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    (battle, events, rng)
}

fn player_hit(event: &BattleEvent) -> bool {
    matches!(
        event,
        BattleEvent::Hit {
            by_player: true,
            move_id: TACKLE,
            ..
        }
    )
}

fn is_frozen_solid(event: &BattleEvent) -> bool {
    matches!(
        event,
        BattleEvent::FrozenSolid {
            by_player: true,
            move_id: TACKLE
        }
    )
}

#[test]
fn a_zero_draw_thaws_the_lead_and_its_move_proceeds_with_one_freeze_draw() {
    let player = frozen_player();
    let starting_pp = player.moves()[0].pp;

    let (battle, events, rng) = run(player, enemy(), script(0, &[&HIT, &HIT]));

    // Prefix, one freeze draw, then both battlers' Tackle.
    assert_eq!(rng.draws(), 3 + 1 + 4 + 4);
    rng.assert_exhausted();
    assert_eq!(events[0], BattleEvent::Thawed { by_player: true });
    assert!(player_hit(&events[1]), "{events:?}");
    assert_eq!(battle.player().status1(), Status1::Healthy);
    assert_eq!(battle.player().moves()[0].pp, starting_pp - 1);
}

#[test]
fn every_multiple_of_five_thaws() {
    for value in [0, 5, 10, 65_530] {
        let (battle, events, rng) = run(frozen_player(), enemy(), script(value, &[&HIT, &HIT]));
        rng.assert_exhausted();
        assert_eq!(
            events[0],
            BattleEvent::Thawed { by_player: true },
            "draw {value}"
        );
        assert_eq!(battle.player().status1(), Status1::Healthy, "draw {value}");
    }
}

#[test]
fn a_nonzero_residue_keeps_the_lead_frozen_and_cancels_the_move_without_pp() {
    for value in [1, 2, 3, 4, 6, 65_534] {
        let player = frozen_player();
        let starting_pp = player.moves()[0].pp;

        // Only the enemy's Tackle follows the one freeze draw.
        let (battle, events, rng) = run(player, enemy(), script(value, &[&HIT]));

        assert_eq!(rng.draws(), 3 + 1 + 4, "draw {value}");
        rng.assert_exhausted();
        assert!(is_frozen_solid(&events[0]), "draw {value}: {events:?}");
        assert!(!events.iter().any(player_hit), "draw {value}: {events:?}");
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, BattleEvent::Thawed { .. })),
            "draw {value}: {events:?}"
        );
        assert_eq!(battle.player().status1(), Status1::Frozen, "draw {value}");
        assert_eq!(battle.player().moves()[0].pp, starting_pp, "draw {value}");
        assert_eq!(
            battle.player().current_hp(),
            battle.player().stats().max_hp - enemy_hit_damage(&events),
            "freeze has no residual: only the enemy's hit removes HP"
        );
    }
}

fn enemy_hit_damage(events: &[BattleEvent]) -> u32 {
    events
        .iter()
        .find_map(|event| match event {
            BattleEvent::Hit {
                by_player: false,
                damage,
                ..
            } => Some(*damage),
            _ => None,
        })
        .expect("the enemy's Tackle lands")
}

#[test]
fn a_frozen_solid_cancellation_leaves_confusion_untouched_and_draws_no_coin() {
    let mut player = frozen_player();
    player.volatiles_mut().set_confusion(3);

    let (battle, events, rng) = run(player, enemy(), script(1, &[&HIT]));

    rng.assert_exhausted();
    assert!(is_frozen_solid(&events[0]), "{events:?}");
    assert_eq!(battle.player().volatiles().confusion_turns, 3);
    assert!(!events
        .iter()
        .any(|event| matches!(event, BattleEvent::Confused { .. })));
}

#[test]
fn a_thaw_precedes_the_confusion_decrement_of_the_same_action() {
    let mut player = frozen_player();
    player.volatiles_mut().set_confusion(1);

    // The duration-one decrement snaps out without a draw.
    let (battle, events, rng) = run(player, enemy(), script(0, &[&HIT, &HIT]));

    rng.assert_exhausted();
    assert_eq!(events[0], BattleEvent::Thawed { by_player: true });
    assert_eq!(
        events[1],
        BattleEvent::SnappedOutOfConfusion { by_player: true }
    );
    assert!(player_hit(&events[2]), "{events:?}");
    assert_eq!(battle.player().status1(), Status1::Healthy);
}

#[test]
fn a_thawed_lead_can_still_hit_itself_in_confusion_without_spending_pp() {
    let mut player = frozen_player();
    player.volatiles_mut().set_confusion(3);
    let starting_pp = player.moves()[0].pp;

    // Freeze draw 0, the confusion coin (even: self-hit), its damage roll,
    // then the enemy's Tackle.
    let (battle, events, rng) = run(player, enemy(), script(0, &[&[0, 0], &HIT]));

    rng.assert_exhausted();
    assert_eq!(events[0], BattleEvent::Thawed { by_player: true });
    assert_eq!(events[1], BattleEvent::Confused { by_player: true });
    assert!(
        matches!(
            events[2],
            BattleEvent::ConfusionSelfHit {
                by_player: true,
                ..
            }
        ),
        "{events:?}"
    );
    assert_eq!(battle.player().status1(), Status1::Healthy);
    assert_eq!(battle.player().moves()[0].pp, starting_pp);
}

#[test]
fn shed_skin_cures_a_frozen_holder_at_end_of_turn_with_no_residual() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, SEVIPER, 10, vec![TACKLE]);
    assert_eq!(player.ability(), AbilityId::SHED_SKIN);
    player.set_status1(Status1::Frozen);
    let enemy = max_iv_mon(&dex, ZIGZAGOON, 10, vec![TACKLE]);

    // Freeze draw 1 (stays frozen), the enemy's Tackle, then Shed Skin's
    // `Random() % 3 == 0` cure draw.
    let (battle, events, rng) = run(player, enemy, script(1, &[&HIT, &[0]]));

    assert_eq!(rng.draws(), 3 + 1 + 4 + 1);
    rng.assert_exhausted();
    assert!(is_frozen_solid(&events[0]), "{events:?}");
    assert!(events.contains(&BattleEvent::ShedSkinCured {
        by_player: true,
        status: Status1::Frozen,
    }));
    assert_eq!(battle.player().status1(), Status1::Healthy);
}

#[test]
fn a_failed_shed_skin_draw_leaves_the_holder_frozen() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, SEVIPER, 10, vec![TACKLE]);
    player.set_status1(Status1::Frozen);
    let enemy = max_iv_mon(&dex, ZIGZAGOON, 10, vec![TACKLE]);

    let (battle, events, rng) = run(player, enemy, script(1, &[&HIT, &[1]]));

    rng.assert_exhausted();
    assert!(!events
        .iter()
        .any(|event| matches!(event, BattleEvent::ShedSkinCured { .. })));
    assert_eq!(battle.player().status1(), Status1::Frozen);
}

#[test]
fn a_frozen_defender_does_not_unlock_the_serene_grace_poison_screen() {
    // A frozen defender can thaw on its own earlier canceller draw, so its
    // starting status cannot justify admitting Serene Grace's unported
    // chance-doubling (`battle_script_commands.c:2334-2335,2912-2913`).
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, SpeciesId::ZIGZAGOON.0, 10, vec![TACKLE]);
    player.set_status1(Status1::Frozen);
    let foe = max_iv_mon(&dex, SpeciesId::DUNSPARCE.0, 10, vec![MoveId::POISON_STING]);
    assert_eq!(foe.ability(), AbilityId::SERENE_GRACE);

    let mut rng = SequenceRng::new([]);
    let refusal = Battle::new(dex, player, foe, false, &mut rng).err();

    assert_eq!(
        refusal,
        Some(BattleError::UnportedAbilityInteraction(
            AbilityId::SERENE_GRACE
        ))
    );
    rng.assert_exhausted();
}
