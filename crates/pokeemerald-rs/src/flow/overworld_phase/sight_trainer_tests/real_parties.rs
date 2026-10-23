//! The four Route 103 sight trainers whose real parties fight (Andrew,
//! Rhett, Marcos, Pete), each driven end to end through the production
//! path: the player steps into the trainer's real cone, the trigger builds
//! the real extracted party, the approach plays out, the headless battle runs
//! to its conclusion, and the defeated flag and prize money land.
//!
//! Nothing here borrows the stand-in party of [`super::support::seed_battle`].
//! The approach's message box takes its documented fallback (a pack-less
//! phase cannot open one, `sight_trainer_approach::advance_intro_message`),
//! which starts the battle the frame the speech would have opened; the
//! speech's own handshake has its own tests in `approach_handoff`.
//!
//! # Where the goldens come from
//!
//! Every personality and original-trainer id below is computed from
//! `CreateNPCTrainerParty` (`battle_main.c:1960-2073`), not read back from
//! the code under test: a male personality base `0x88`, plus the wrapping
//! Gen III byte sum of the trainer's name and then each species' name,
//! carried across the party, shifted left by eight. The shared stream is
//! seeded to `1`, so `Random()` yields `0x41C6, 0xAC21, 0xD2EE, 0x1FB7,
//! 0x79E3, 0xB6A8, 0x4AFA` (`random.c:11-15`); each member consumes two for
//! its `Random32()` original-trainer id (low half first, none of these
//! rolls shiny), and `Battle::new_trainer` then spends one for the
//! turn-order seed (`battle_main.c:3852-3861`). A Torchic lead is used
//! because its Blaze ability and neutral typing keep every trainer's switch
//! evaluation out of play, which the battle crate does not model.

use assets::trainers::TrainerId;
use assets::{AbilityId, MoveId, SpeciesId};
use engine::overworld::{Direction, PlayerState};
use platform::ButtonState;

use super::super::OverworldPhase;
use super::support::*;

const TORCHIC: u16 = 280;
const SLASH: u16 = 163;
const RNG_SEED: u32 = 1;

/// One mon of a trainer's party as `CreateNPCTrainerParty` builds it.
struct Member {
    species: u16,
    level: u8,
    /// `fixed_ivs` of the party entry's raw IV byte: every stat gets this.
    iv: u8,
    personality: u32,
    ot_id: u32,
    ability: AbilityId,
    moves: &'static [MoveId],
}

struct Fighter {
    name: &'static str,
    id: u16,
    /// Where the player stands: one tile in front of the trainer.
    tile: (i32, i32),
    facing: Direction,
    elevation: u8,
    party: &'static [Member],
    /// `4 * lastMonLevel * classValue` (`Cmd_getmoneyreward`).
    prize: u32,
    /// The shared stream after construction and the battle's turn-order seed.
    rng_after_construction: u32,
    /// The battle's own turn-order seed: the last of those draws.
    random_turn_number: u16,
    /// The stream after the Torchic lead's one-hit wins: each attacking turn
    /// spends the turn-order seed, `AI_CheckBadMove`'s four simulated-damage
    /// rolls and its final tie choice, then Slash's accuracy, critical,
    /// damage and secondary-chance draws (ten), and a knocked-out target
    /// answers nothing. Andrew's three members are 7 + 3 * 10 draws in all,
    /// a single-member party 3 + 10.
    rng_after_battle: u32,
}

const FIGHTERS: [Fighter; 4] = [
    Fighter {
        name: "Andrew",
        id: 336,
        tile: (50, 10),
        facing: Direction::North,
        elevation: 3,
        party: &[
            Member {
                species: 129,
                level: 5,
                iv: 0,
                personality: 0x000A_B988,
                ot_id: 0xAC21_41C6,
                ability: AbilityId::SWIFT_SWIM,
                moves: &[MoveId::SPLASH],
            },
            Member {
                species: 72,
                level: 10,
                iv: 0,
                personality: 0x0016_4988,
                ot_id: 0x1FB7_D2EE,
                ability: AbilityId::CLEAR_BODY,
                moves: &[MoveId::POISON_STING, MoveId::SUPERSONIC],
            },
            Member {
                species: 129,
                level: 15,
                iv: 0,
                personality: 0x0021_0288,
                ot_id: 0xB6A8_79E3,
                ability: AbilityId::SWIFT_SWIM,
                moves: &[MoveId::SPLASH, MoveId::TACKLE],
            },
        ],
        prize: 600,
        rng_after_construction: 0x4AFA_DDCE,
        random_turn_number: 0x4AFA,
        rng_after_battle: 0x8C29_FAE4,
    },
    Fighter {
        name: "Rhett",
        id: 703,
        tile: (67, 6),
        facing: Direction::North,
        elevation: 3,
        party: &[Member {
            species: 335,
            level: 15,
            iv: 12,
            personality: 0x000A_0D88,
            ot_id: 0xAC21_41C6,
            ability: AbilityId::THICK_FAT,
            moves: &[
                MoveId::FOCUS_ENERGY,
                MoveId::SAND_ATTACK,
                MoveId::ARM_THRUST,
                MoveId::VITAL_THROW,
            ],
        }],
        prize: 480,
        rng_after_construction: 0xD2EE_F54A,
        random_turn_number: 0xD2EE,
        rng_after_battle: 0x7937_654C,
    },
    Fighter {
        name: "Marcos",
        id: 702,
        tile: (67, 8),
        facing: Direction::South,
        elevation: 3,
        party: &[Member {
            species: 100,
            level: 15,
            iv: 12,
            personality: 0x000A_1F88,
            ot_id: 0xAC21_41C6,
            ability: AbilityId::SOUNDPROOF,
            moves: &[
                MoveId::CHARGE,
                MoveId::TACKLE,
                MoveId::SCREECH,
                MoveId::SONIC_BOOM,
            ],
        }],
        prize: 480,
        rng_after_construction: 0xD2EE_F54A,
        random_turn_number: 0xD2EE,
        rng_after_battle: 0x7937_654C,
    },
    Fighter {
        name: "Pete",
        id: 735,
        tile: (36, 12),
        facing: Direction::South,
        elevation: 1,
        party: &[Member {
            species: 72,
            level: 15,
            iv: 0,
            personality: 0x000A_0988,
            ot_id: 0xAC21_41C6,
            ability: AbilityId::CLEAR_BODY,
            moves: &[MoveId::POISON_STING, MoveId::SUPERSONIC, MoveId::CONSTRICT],
        }],
        prize: 120,
        rng_after_construction: 0xD2EE_F54A,
        random_turn_number: 0xD2EE,
        rng_after_battle: 0x7937_654C,
    },
];

/// Route 103 with `fighter`'s cone reaching the player. Pete stands at
/// elevation 1, which the fixture's uniform elevation-3 ground would hide
/// from his cone, so that one cell is raised to match.
fn phase_in_cone_of(fighter: &Fighter) -> OverworldPhase {
    let player = PlayerState::new(fighter.tile, fighter.elevation, fighter.facing);
    let (x, y) = fighter.tile;
    let scene = if fighter.elevation == 3 {
        crate::overworld::test_support::synthetic_scene(80, 16)
    } else {
        let at = (u16::try_from(x).unwrap(), u16::try_from(y).unwrap());
        crate::overworld::test_support::synthetic_scene_with_cell_elevation(
            80,
            16,
            at,
            fighter.elevation,
        )
    };
    let mut phase = OverworldPhase::for_test(scene, ROUTE_103, player, None);
    phase.rng = engine::rng::Rng::new(RNG_SEED);
    phase.set_fresh_lead_for_test(lead(TORCHIC, 50, SLASH));
    phase
}

fn assert_member_matches(name: &str, index: usize, member: &Member, mon: &battle::BattlePokemon) {
    let context = format!("{name} member {index}");
    assert_eq!(mon.species(), SpeciesId(member.species), "{context}");
    assert_eq!(mon.level(), member.level, "{context}");
    assert_eq!(mon.personality(), member.personality, "{context}");
    assert_eq!(mon.original_trainer_id(), member.ot_id, "{context}");
    assert_eq!(mon.ivs().as_array(), [member.iv; 6], "{context}");
    assert_eq!(mon.ability(), member.ability, "{context}");
    assert_eq!(mon.held_item(), assets::items::ItemId::NONE, "{context}");
    let moves: Vec<MoveId> = mon.moves().iter().map(|slot| slot.move_id).collect();
    assert_eq!(moves, member.moves, "{context}");
}

/// The trigger builds exactly the party and stream state upstream's
/// `CreateNPCTrainerParty` would, on the first frame, and the approach then
/// spends no more of the shared stream.
#[test]
fn each_cone_builds_the_real_party_with_upstream_construction_draws() {
    for fighter in &FIGHTERS {
        let mut phase = phase_in_cone_of(fighter);

        phase.step(ButtonState::new());

        let approach = phase
            .sight_approach
            .as_ref()
            .unwrap_or_else(|| panic!("{}: the cone must start the approach", fighter.name));
        let battle = approach.battle();
        let bench = battle.trainer().expect("a trainer battle").bench();
        assert_eq!(
            1 + bench.len(),
            fighter.party.len(),
            "{}: party size",
            fighter.name
        );
        for (index, (member, mon)) in fighter
            .party
            .iter()
            .zip(std::iter::once(battle.enemy()).chain(bench))
            .enumerate()
        {
            assert_member_matches(fighter.name, index, member, mon);
        }
        assert_eq!(
            battle.random_turn_number(),
            fighter.random_turn_number,
            "{}",
            fighter.name
        );
        assert_eq!(
            phase.rng.state(),
            fighter.rng_after_construction,
            "{}: draws spent by construction",
            fighter.name
        );
    }
}

/// Cone, approach walk, battle and conclusion for each trainer, with the
/// defeated flag, the prize money and the lead's return checked at the end.
#[test]
fn each_cone_runs_start_to_conclusion_and_rewards_the_win() {
    for fighter in &FIGHTERS {
        let mut phase = phase_in_cone_of(fighter);
        let flag = TRAINER_FLAGS_START + fighter.id;
        let money_before = phase.save1().money;

        phase.step(ButtonState::new());
        assert!(phase.sight_approach.is_some(), "{}", fighter.name);
        let after_construction = phase.rng.state();

        let mut handed_off = false;
        for _ in 0..600 {
            phase.step(ButtonState::new());
            if phase.is_sight_trainer_battle_active() {
                handed_off = true;
                break;
            }
            assert_eq!(
                phase.rng.state(),
                after_construction,
                "{}: the approach must not touch the shared stream",
                fighter.name
            );
        }
        assert!(handed_off, "{}: the approach must hand off", fighter.name);
        assert_eq!(
            active_sight_trainer_id(&phase),
            Some(TrainerId(fighter.id)),
            "{}",
            fighter.name
        );
        assert!(
            phase.lead_for_test().is_none(),
            "{}: lead on loan",
            fighter.name
        );

        let outcome = play_out_sight_battle(&mut phase, 64);
        assert_eq!(
            outcome,
            Some(battle::BattleOutcome::PlayerWon),
            "{}",
            fighter.name
        );
        assert_eq!(
            phase.rng.state(),
            fighter.rng_after_battle,
            "{}: draws spent from construction to the win",
            fighter.name
        );
        assert!(!phase.is_sight_trainer_battle_active(), "{}", fighter.name);
        assert!(
            phase.lead_for_test().is_some(),
            "{}: lead returned",
            fighter.name
        );
        assert_eq!(
            phase.save1().event_data.flag_get(flag),
            Ok(true),
            "{}: defeated flag",
            fighter.name
        );
        assert_eq!(
            phase.save1().money - money_before,
            fighter.prize,
            "{}: prize",
            fighter.name
        );

        for _ in 0..FRAMES_STANDING_STILL {
            phase.step(ButtonState::new());
            assert!(
                phase.sight_approach.is_none() && !phase.is_sight_trainer_battle_active(),
                "{}: a defeated trainer must not re-trigger",
                fighter.name
            );
        }
    }
}
