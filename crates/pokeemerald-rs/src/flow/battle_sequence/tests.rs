use assets::MoveId;
use battle::BattleEvent;

use super::{script_hold, Hold, WAIT_TIME_LONG};

fn released_on(mut hold: Hold) -> usize {
    (1..=1_000)
        .find(|_| hold.released())
        .expect("a hold releases")
}

#[test]
fn a_hold_of_none_releases_on_the_frame_its_print_completes() {
    assert_eq!(released_on(Hold::new(0)), 1);
}

/// `Cmd_waitmessage` counts `gPauseCounterBattle` up to its operand, one per
/// frame, once the controller is free (`battle_script_commands.c:2159-2177`);
/// the frame the print completes on only frees the controller.
#[test]
fn a_hold_counts_its_frames_from_the_frame_after_the_print_completes() {
    let mut hold = Hold::new(WAIT_TIME_LONG);
    assert!(!hold.released(), "the completing frame is not counted");
    for frame in 1..WAIT_TIME_LONG {
        assert!(!hold.released(), "frame {frame} of {WAIT_TIME_LONG}");
    }
    assert!(hold.released());
}

#[test]
fn the_move_announcement_faint_and_experience_prints_carry_no_hold() {
    let move_id = MoveId::POUND;
    for event in [
        BattleEvent::UsedMove {
            by_player: true,
            move_id,
        },
        BattleEvent::Fainted { by_player: false },
        BattleEvent::ExpGained {
            amount: 17,
            boosted: false,
        },
        BattleEvent::MoveLearnDeclined { move_id },
    ] {
        assert_eq!(script_hold(&event, 0, 1), 0, "{event:?}");
    }
}

#[test]
fn a_result_message_holds_for_the_long_wait() {
    let crit = BattleEvent::Hit {
        by_player: true,
        move_id: MoveId::POUND,
        damage: 5,
        is_critical: true,
    };
    assert_eq!(script_hold(&crit, 0, 1), WAIT_TIME_LONG);
}

/// The forgetting lines of a learned move print with no hold, and only the
/// learned line waits (`battle_scripts_1.s:3178-3187`).
#[test]
fn only_the_last_print_of_a_replaced_move_holds() {
    let replaced = BattleEvent::MoveReplaced {
        learned: MoveId::POUND,
        forgotten: MoveId::TACKLE,
        slot: 0,
    };
    let holds: Vec<u16> = (0..4)
        .map(|position| script_hold(&replaced, position, 4))
        .collect();
    assert_eq!(holds, [0, 0, 0, WAIT_TIME_LONG]);
}

/// A wild opponent poisoned at 75% faints the lead while the player tries to
/// run; the reserve's send-out reads the opponent's health at that moment
/// (scale 0, "Go!"), not after the end-of-turn poison drops it below 70%.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn the_replacement_wording_follows_the_opponents_health_at_the_send_out() {
    use super::{BattleSequence, TurnPolicy};
    use crate::battle_narration::{BattleKind, Battler, Scene};
    use assets::SpeciesId;
    use battle::{Battle, BattlePokemon, Dex, Ivs, Status1, MAX_IV};
    use engine::rng::Rng;

    let ivs = Ivs {
        hp: MAX_IV,
        attack: MAX_IV,
        defense: MAX_IV,
        speed: MAX_IV,
        sp_attack: MAX_IV,
        sp_defense: MAX_IV,
    };
    let mon = |species: u16, level: u8| {
        BattlePokemon::new(
            &Dex::new(),
            SpeciesId(species),
            level,
            ivs,
            0,
            vec![MoveId::POUND],
        )
        .unwrap()
    };
    let battler = |name: &str| Battler {
        name: name.to_owned(),
        ability: None,
    };
    for seed in 1..200u32 {
        let mut lead = mon(213, 2);
        lead.apply_damage(lead.stats().max_hp - 1);
        let reserve = mon(290, 5);
        let mut enemy = mon(277, 50);
        let max = enemy.stats().max_hp;
        let at_send_out = max * 3 / 4;
        enemy.apply_damage(max - at_send_out);
        enemy.set_status1(Status1::Poisoned);
        let mut rng = Rng::new(seed);
        let mut battle = Battle::new_with_player_reserves(
            Dex::new(),
            lead,
            vec![reserve],
            enemy,
            false,
            &mut crate::flow::wild_encounter::SharedRng::new(&mut rng),
        )
        .unwrap();
        let pack = assets::pack::AssetPack::load(&assets::pack::AssetPack::repo_pack_path())
            .expect("needs a local pack: run `cargo xtask extract` first");
        let scene = Scene {
            kind: BattleKind::Wild,
            player_name: "MAY".to_owned(),
            player: battler("SHUCKLE"),
            opponent: battler("TREECKO"),
            trainer: None,
            hp_scale: 0,
        };
        let mut sequence = BattleSequence::begin(
            pack,
            0,
            scene,
            TurnPolicy::Run,
            vec![battler("WURMPLE")],
            false,
        )
        .unwrap();
        let events = sequence.take_turn(&mut battle, &mut rng);
        if !events
            .iter()
            .any(|event| matches!(event, BattleEvent::PlayerSentOut { .. }))
        {
            continue;
        }
        let end_hp = battle.enemy().current_hp();
        assert!(
            end_hp * 100 / max < 70,
            "poison dropped the opponent below 70%"
        );
        assert!(at_send_out * 100 / max >= 70);
        sequence.print_steps(&battle, &events).unwrap();
        assert_eq!(
            sequence.scene().hp_scale,
            0,
            "the send-out happened at {at_send_out}/{max} HP (at least 70%: \"Go!\")"
        );
        return;
    }
    panic!("no seed produced a failed run and a lead faint");
}

#[test]
fn the_opponents_health_at_a_send_out_undoes_the_effects_after_it() {
    use super::opponent_health_at;
    use battle::{BattlePokemon, Dex, Ivs};

    let mut enemy = BattlePokemon::new(
        &Dex::new(),
        assets::SpeciesId(277),
        50,
        Ivs::default(),
        0,
        vec![MoveId::POUND],
    )
    .unwrap();
    enemy.apply_damage(enemy.stats().max_hp - 50);
    let later = [
        BattleEvent::HurtByPoison {
            by_player: false,
            damage: 12,
        },
        BattleEvent::LeechSeedHealed {
            by_player: false,
            healed: 3,
        },
        // The player's own battler changes nothing about the opponent.
        BattleEvent::HurtByBurn {
            by_player: true,
            damage: 9,
        },
    ];
    assert_eq!(opponent_health_at(&enemy, &later), 50 + 12 - 3);
    assert_eq!(opponent_health_at(&enemy, &[]), 50);
}
