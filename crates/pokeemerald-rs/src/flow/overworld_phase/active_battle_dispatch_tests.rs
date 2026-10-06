//! Active-battle ownership, dispatch, completion, and starter wiring.

use super::test_support::*;
use super::{ActiveBattle, OverworldPhase};
use crate::new_game;
use engine::overworld::{Direction, PlayerState};
use engine::rng::Rng;
use platform::{ButtonState, Buttons};

/// Issue #207 review, round 2: the production starter *wiring*, not just the
/// starter constructor -- [`OverworldPhase::load_default`] must hand back a
/// phase whose party lead is the provisional starter, or a real playthrough
/// rolls I-4 encounters it can never fight. Mutation-pinned: deleting
/// `load_default`'s `party_lead` assignment fails only here, because every
/// pack-free test builds its phase through `for_test`, which deliberately
/// leaves the lead `None`.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn load_default_hands_back_a_fightable_provisional_starter() {
    let phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
    let lead = phase
        .party_lead
        .as_ref()
        .expect("a fresh game starts with the provisional starter (issue #207 review)");
    assert_eq!(lead.species(), new_game::PROVISIONAL_STARTER_SPECIES);
    assert_eq!(lead.level(), new_game::PROVISIONAL_STARTER_LEVEL);
    assert!(!lead.is_fainted(), "the lead must be able to fight");
}

/// Each [`OverworldPhase::active_battle`] variant is reported by exactly
/// one `is_*_battle_active` accessor, and `in_battle()` by all of them.
#[test]
fn active_battle_reports_exactly_one_frame_owner_for_each_variant() {
    use crate::flow::npc_trainer_battle;

    const PLAYER_TRAINER_ID: u32 = 0x1234_5678;
    // `TRAINER_BRENDAN_ROUTE_103_TREECKO`; which trainer it is does not
    // matter here.
    let stand_in_trainer = assets::trainers::TrainerId(532);

    let mut rng = Rng::new(1);
    let wild_battle = crate::flow::wild_encounter::start_wild_battle(
        new_game::provisional_starter(),
        engine::overworld::wild_encounter::WildEncounter {
            species: assets::SpeciesId(290), // SPECIES_WURMPLE
            level: 2,
            slot: 0,
        },
        PLAYER_TRAINER_ID,
        &mut rng,
    )
    .expect("a Wurmple encounter is fightable");
    let first_battle = crate::flow::first_battle::start_first_battle(
        new_game::provisional_starter(),
        PLAYER_TRAINER_ID,
        &mut rng,
    )
    .expect("the provisional starter must construct the scripted first battle");
    let rival_battle = npc_trainer_battle::start_npc_trainer_battle(
        new_game::provisional_starter(),
        stand_in_trainer,
        &mut rng,
    )
    .expect("the stand-in trainer must always construct");
    let sight_battle = npc_trainer_battle::start_npc_trainer_battle(
        new_game::provisional_starter(),
        stand_in_trainer,
        &mut rng,
    )
    .expect("the stand-in trainer must always construct");

    let variants = [
        ActiveBattle::Wild(wild_battle),
        ActiveBattle::First(first_battle),
        ActiveBattle::Rival {
            battle: rival_battle,
            trainer_id: stand_in_trainer,
        },
        ActiveBattle::SightTrainer {
            battle: sight_battle,
            trainer_id: stand_in_trainer,
        },
    ];

    for variant in variants {
        let expect_wild = matches!(variant, ActiveBattle::Wild(_));
        let expect_first = matches!(variant, ActiveBattle::First(_));
        let expect_rival = matches!(variant, ActiveBattle::Rival { .. });
        let expect_sight = matches!(variant, ActiveBattle::SightTrainer { .. });

        let mut phase = synthetic_phase(PlayerState::new((4, 6), 3, Direction::West), None);
        phase.active_battle = Some(variant);

        assert!(
            phase.in_battle(),
            "an installed ActiveBattle variant must own the frame"
        );
        assert_eq!(
            phase.is_wild_battle_active(),
            expect_wild,
            "is_wild_battle_active must agree with the installed variant"
        );
        assert_eq!(
            phase.is_first_battle_active(),
            expect_first,
            "is_first_battle_active must agree with the installed variant"
        );
        assert_eq!(
            phase.is_rival_battle_active(),
            expect_rival,
            "is_rival_battle_active must agree with the installed variant"
        );
        assert_eq!(
            phase.is_sight_trainer_battle_active(),
            expect_sight,
            "is_sight_trainer_battle_active must agree with the installed variant"
        );

        // Clearing the slot drops the whole variant at once, `trainer_id`
        // included.
        phase.active_battle = None;
        assert!(!phase.in_battle());
        assert!(!phase.is_wild_battle_active());
        assert!(!phase.is_first_battle_active());
        assert!(!phase.is_rival_battle_active());
        assert!(!phase.is_sight_trainer_battle_active());
    }
}

/// `step` dispatches one frame to the installed variant's own driver: the
/// overworld stays frozen and the variant survives as the same arm (or has
/// ended through its own driver).
#[test]
fn step_dispatches_each_active_battle_variant_to_its_own_driver() {
    use crate::flow::npc_trainer_battle;
    const PLAYER_TRAINER_ID: u32 = 0x1234_5678;
    let stand_in_trainer = assets::trainers::TrainerId(532);
    let mut rng = Rng::new(1);
    let mk_npc = |rng: &mut Rng| {
        npc_trainer_battle::start_npc_trainer_battle(
            new_game::provisional_starter(),
            stand_in_trainer,
            rng,
        )
        .expect("stand-in trainer constructs")
    };
    let wild = crate::flow::wild_encounter::start_wild_battle(
        new_game::provisional_starter(),
        engine::overworld::wild_encounter::WildEncounter {
            species: assets::SpeciesId(290),
            level: 2,
            slot: 0,
        },
        PLAYER_TRAINER_ID,
        &mut rng,
    )
    .expect("wild");
    let first = crate::flow::first_battle::start_first_battle(
        new_game::provisional_starter(),
        PLAYER_TRAINER_ID,
        &mut rng,
    )
    .expect("first");
    let rival = mk_npc(&mut rng);
    let sight = mk_npc(&mut rng);
    let variants = [
        ActiveBattle::Wild(wild),
        ActiveBattle::First(first),
        ActiveBattle::Rival {
            battle: rival,
            trainer_id: stand_in_trainer,
        },
        ActiveBattle::SightTrainer {
            battle: sight,
            trainer_id: stand_in_trainer,
        },
    ];
    for variant in variants {
        let kind = std::mem::discriminant(&variant);
        let mut phase = synthetic_phase(PlayerState::new((4, 6), 3, Direction::West), None);
        phase.active_battle = Some(variant);
        phase.step(held(Buttons::LEFT));
        assert_eq!(
            phase.player.position(),
            (4, 6),
            "battle must freeze the overworld"
        );
        let after = phase.active_battle.as_ref().map(std::mem::discriminant);
        assert!(
            after.is_none() || after == Some(kind),
            "the variant must be driven by its own arm, never re-slotted as a sibling"
        );
    }
}

/// Each installed variant is driven to its end: a dispatcher that reinstalls
/// the taken variant without calling a driver leaves the slot occupied.
#[test]
fn step_drives_each_active_battle_variant_to_completion() {
    use crate::flow::npc_trainer_battle;
    const PLAYER_TRAINER_ID: u32 = 0x1234_5678;
    let stand_in_trainer = assets::trainers::TrainerId(532);
    let mut rng = Rng::new(1);
    let mk_npc = |rng: &mut Rng| {
        npc_trainer_battle::start_npc_trainer_battle(
            new_game::provisional_starter(),
            stand_in_trainer,
            rng,
        )
        .expect("stand-in trainer constructs")
    };
    let wild = crate::flow::wild_encounter::start_wild_battle(
        new_game::provisional_starter(),
        engine::overworld::wild_encounter::WildEncounter {
            species: assets::SpeciesId(290),
            level: 2,
            slot: 0,
        },
        PLAYER_TRAINER_ID,
        &mut rng,
    )
    .expect("wild");
    let first = crate::flow::first_battle::start_first_battle(
        new_game::provisional_starter(),
        PLAYER_TRAINER_ID,
        &mut rng,
    )
    .expect("first");
    let rival = mk_npc(&mut rng);
    let sight = mk_npc(&mut rng);
    let variants = [
        ActiveBattle::Wild(wild),
        ActiveBattle::First(first),
        ActiveBattle::Rival {
            battle: rival,
            trainer_id: stand_in_trainer,
        },
        ActiveBattle::SightTrainer {
            battle: sight,
            trainer_id: stand_in_trainer,
        },
    ];
    for variant in variants {
        let kind = std::mem::discriminant(&variant);
        let mut phase = synthetic_phase(PlayerState::new((4, 6), 3, Direction::West), None);
        phase.active_battle = Some(variant);
        let mut frames = 0;
        while phase.active_battle.is_some() && frames < 1000 {
            phase.step(ButtonState::new());
            frames += 1;
        }
        assert!(
            phase.active_battle.is_none(),
            "variant {kind:?} was never driven to an end in {frames} frames"
        );
    }
}
