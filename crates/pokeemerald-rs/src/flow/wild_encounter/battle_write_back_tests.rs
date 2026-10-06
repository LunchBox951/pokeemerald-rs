//! Battle write-back: what the overworld party lead carries out of a finished wild battle
//! (neutral stat stages, cleared volatiles) and the empty-slot guard on `advance_wild_battle`.

use super::test_support::{player_mon, ENCOUNTER_SEED, TEST_PLAYER_TRAINER_ID, WURMPLE};
use super::{advance_wild_battle, start_wild_battle};
use assets::MoveId;
use battle::{Battle, BattlePokemon, StatStage};
use engine::overworld::wild_encounter::WildEncounter;
use engine::rng::Rng;

/// Issue #207 review, finding 2: an in-battle stat-stage change (String
/// Shot, Growl, Tail Whip) must not leak back into the overworld party lead
/// when the battle ends -- upstream keeps stages in `gBattleMons` only,
/// set to `DEFAULT_STAT_STAGE` when each battler's entry is built at battle
/// entry (`battle_main.c:3423-3424`), and the party
/// struct has no stage field to persist them in. A leaked stage would give
/// the *next* encounter wrong turn order, damage, and accuracy.
///
/// The non-neutral stage is planted on the lead before the handoff (the
/// write-back path cannot tell when a stage moved, only that it is
/// non-neutral at the end), and the battle ends via the production driver's
/// own successful run attempt.
#[test]
fn ending_a_battle_writes_the_lead_back_with_neutral_stat_stages() {
    let mut rng = Rng::new(ENCOUNTER_SEED);
    // Skip the four overworld roll draws, as the full-battle scenario above
    // does: the encounter they produce is the slot-0 level-2 Wurmple.
    for _ in 0..4 {
        rng.next_u16();
    }
    let encounter = WildEncounter {
        species: WURMPLE,
        level: 2,
        slot: 0,
    };

    // A level-50 Treecko whose Speed stage is already at -2: even halved it
    // outruns a level-2 Wurmple, so the driver's run succeeds immediately
    // and the battle ends with the stage still non-neutral.
    let mut lead = player_mon(277, 50, vec![MoveId::POUND]);
    lead.stages_mut().speed = StatStage::new(-2).unwrap();

    let battle = start_wild_battle(lead, encounter, TEST_PLAYER_TRAINER_ID, &mut rng)
        .expect("a Route 101 Wurmple must be fightable");
    assert_eq!(
        battle.player().stages().speed,
        StatStage::new(-2).unwrap(),
        "Battle::new must carry the planted stage in, or this pins nothing"
    );

    let mut slot = Some(battle);
    let mut written_back: Option<BattlePokemon> = None;
    let mut frames = 0;
    while slot.is_some() {
        advance_wild_battle(&mut slot, &mut written_back, &mut rng);
        frames += 1;
        assert!(frames < 200, "the headless driver must terminate");
    }
    let lead = written_back.expect("the battle writes the lead mon back when it ends");
    assert_eq!(
        lead.stages(),
        battle::StatStages::default(),
        "in-battle stat stages are battle-local and must be reset on the write-back"
    );
}

/// Reviewer finding 2 (`clear_battle_scratch`): Focus Energy and Charge are
/// `gBattleMons[].status2` / `gStatuses3[]` bits, not `struct Pokemon`
/// fields -- `BattleStartClearSetData` zeroes both at the start of every
/// battle (`src/battle_main.c:3034`). A write-back that reset only stat
/// stages would let a Focus-Energy'd lead walk into the *next* encounter
/// still critting at `+2` stages, exactly the leak
/// `ending_a_battle_writes_the_lead_back_with_neutral_stat_stages` pins for
/// stages.
///
/// Mirrors that test's structure: the volatiles are planted directly on the
/// lead before the handoff (no move in this crate sets Focus Energy or
/// Charge yet, so the only way to reach a non-default value is to plant it),
/// carried into `Battle::new` is checked as a sanity bound, and the same
/// successful-run driver loop ends the battle.
#[test]
fn ending_a_battle_writes_the_lead_back_with_cleared_volatiles() {
    let mut rng = Rng::new(ENCOUNTER_SEED);
    for _ in 0..4 {
        rng.next_u16();
    }
    let encounter = WildEncounter {
        species: WURMPLE,
        level: 2,
        slot: 0,
    };

    let mut lead = player_mon(277, 50, vec![MoveId::POUND]);
    lead.volatiles_mut().set_focus_energy();
    lead.volatiles_mut().set_charge();

    let battle = start_wild_battle(lead, encounter, TEST_PLAYER_TRAINER_ID, &mut rng)
        .expect("a Route 101 Wurmple must be fightable");
    assert!(
        battle.player().volatiles().focus_energy,
        "Battle::new must carry the planted volatiles in, or this pins nothing"
    );
    assert!(battle.player().volatiles().charged_up());

    let mut slot = Some(battle);
    let mut written_back: Option<BattlePokemon> = None;
    let mut frames = 0;
    while slot.is_some() {
        advance_wild_battle(&mut slot, &mut written_back, &mut rng);
        frames += 1;
        assert!(frames < 200, "the headless driver must terminate");
    }
    let lead = written_back.expect("the battle writes the lead mon back when it ends");
    assert_eq!(
        lead.volatiles(),
        battle::Volatiles::default(),
        "in-battle volatiles are battle-local and must be reset on the write-back"
    );
}

/// `advance_wild_battle` is a no-op on an empty slot -- the guard the
/// per-frame caller relies on.
#[test]
fn advancing_an_absent_battle_does_nothing() {
    let mut slot: Option<Battle> = None;
    let mut lead = None;
    let mut rng = Rng::new(1);
    assert!(advance_wild_battle(&mut slot, &mut lead, &mut rng).is_none());
    assert!(lead.is_none());
    assert_eq!(rng.state(), Rng::new(1).state(), "no battle, no draw");
}
