//! `CANCELER_THAW` at the action boundary, driven through the private
//! `run_action` seam because no constructor admits a thaw-hit move yet.

use assets::{MoveId, SpeciesId};

use super::trainer::fixed_ivs;
use super::{Battle, BattleEvent};
use crate::damage::BattleRng;
use crate::dex::Dex;
use crate::pokemon::BattlePokemon;
use crate::status1::Status1;

/// Replays `script`, then draws zero.
struct ScriptedRng(Vec<u16>, usize);

impl BattleRng for ScriptedRng {
    fn next_u16(&mut self) -> u16 {
        let v = self.0.get(self.1).copied().unwrap_or(0);
        self.1 += 1;
        v
    }
}

fn battle(confusion: u8) -> Battle {
    let dex = Dex::new();
    let mut player = BattlePokemon::new(
        &dex,
        SpeciesId::RATTATA,
        20,
        fixed_ivs(255),
        0,
        vec![MoveId::TACKLE],
    )
    .unwrap();
    player.set_status1(Status1::Frozen);
    if confusion > 0 {
        player.volatiles_mut().set_confusion(confusion);
    }
    let enemy = BattlePokemon::new(
        &dex,
        SpeciesId::ZIGZAGOON,
        20,
        fixed_ivs(255),
        0,
        vec![MoveId::TACKLE],
    )
    .unwrap();
    Battle::new(dex, player, enemy, false, &mut ScriptedRng(vec![0], 0)).unwrap()
}

fn run(battle: &mut Battle, script: Vec<u16>) -> Vec<BattleEvent> {
    let mut events = Vec::new();
    let _ = battle.run_action(
        true,
        MoveId::FLAME_WHEEL,
        None,
        &mut ScriptedRng(script, 0),
        &mut events,
    );
    events
}

#[test]
fn a_thaw_hit_move_that_stays_frozen_thaws_before_it_is_used() {
    let mut battle = battle(0);
    let events = run(&mut battle, vec![1]);
    assert_eq!(
        events[..2],
        [
            BattleEvent::ThawedByMove {
                by_player: true,
                move_id: MoveId::FLAME_WHEEL
            },
            BattleEvent::UsedMove {
                by_player: true,
                move_id: MoveId::FLAME_WHEEL
            },
        ],
        "{events:?}"
    );
    assert_eq!(battle.player.status1(), Status1::Healthy);
}

#[test]
fn the_thaw_follows_the_confusion_decrement() {
    let mut battle = battle(1);
    let events = run(&mut battle, vec![1]);
    assert_eq!(
        events[..3],
        [
            BattleEvent::SnappedOutOfConfusion { by_player: true },
            BattleEvent::ThawedByMove {
                by_player: true,
                move_id: MoveId::FLAME_WHEEL
            },
            BattleEvent::UsedMove {
                by_player: true,
                move_id: MoveId::FLAME_WHEEL
            },
        ],
        "{events:?}"
    );
    assert_eq!(battle.player.status1(), Status1::Healthy);
}

#[test]
fn a_confusion_self_hit_cancels_the_thaw() {
    let mut battle = battle(3);
    let events = run(&mut battle, vec![1, 0, 0]);
    assert!(
        matches!(
            events[1],
            BattleEvent::ConfusionSelfHit {
                by_player: true,
                ..
            }
        ),
        "{events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, BattleEvent::ThawedByMove { .. })),
        "{events:?}"
    );
    assert_eq!(battle.player.status1(), Status1::Frozen);
}
