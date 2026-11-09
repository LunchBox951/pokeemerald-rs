//! Every battle event the turn engine can report has a narration mapping.

use assets::items::ItemId;
use assets::{AbilityId, Effectiveness, MoveId};
use battle::{BattleEvent, BattleOutcome, ChangedStat, StatStage, Status1};

use super::fixtures::{narrator, RATTATA};
use crate::battle_narration::string_ids::PKMNLEARNEDMOVE;
use crate::battle_narration::{AssetNames, BattleKind, NarrationError, TemplateId};

const BY_PLAYER: bool = true;
const MOVE: MoveId = MoveId::TACKLE;

fn every_event() -> Vec<BattleEvent> {
    let mut events = move_events();
    events.extend(ability_and_status_events());
    events.extend(party_events());
    events
}

fn move_events() -> Vec<BattleEvent> {
    let (by_player, move_id) = (BY_PLAYER, MOVE);
    let stage = StatStage::new(-1).unwrap();
    let stat = ChangedStat::Attack;
    vec![
        BattleEvent::Missed { by_player, move_id },
        BattleEvent::FailedNoPp { by_player, move_id },
        BattleEvent::SnappedOutOfConfusion { by_player },
        BattleEvent::Confused { by_player },
        BattleEvent::ConfusionSelfHit {
            by_player,
            damage: 3,
        },
        BattleEvent::FullyParalyzed { by_player, move_id },
        BattleEvent::FastAsleep { by_player, move_id },
        BattleEvent::WokeUp { by_player },
        BattleEvent::LoafingAround { by_player, move_id },
        BattleEvent::FrozenSolid { by_player, move_id },
        BattleEvent::Thawed { by_player },
        BattleEvent::ThawedByMove { by_player, move_id },
        BattleEvent::Burned { by_player, move_id },
        BattleEvent::TargetDefrosted { by_player, move_id },
        BattleEvent::NoEffect { by_player, move_id },
        BattleEvent::LevitateBlocked { by_player, move_id },
        BattleEvent::WonderGuardBlocked { by_player, move_id },
        BattleEvent::AbilityHealed {
            by_player,
            move_id,
            healed: 4,
            ability: AbilityId::VOLT_ABSORB,
        },
        BattleEvent::FlashFireAbsorbed {
            by_player,
            move_id,
            boosted: true,
        },
        BattleEvent::Hit {
            by_player,
            move_id,
            damage: 5,
            is_critical: false,
        },
        BattleEvent::Recoil {
            by_player,
            move_id,
            damage: 2,
        },
        BattleEvent::Fainted { by_player },
        BattleEvent::Drained {
            by_player,
            move_id,
            healed: 2,
        },
        BattleEvent::LiquidOoze {
            by_player,
            move_id,
            damage: 2,
        },
        BattleEvent::MultiHit {
            by_player,
            move_id,
            hits: 3,
        },
        BattleEvent::NothingHappened { by_player, move_id },
        BattleEvent::GettingPumped { by_player, move_id },
        BattleEvent::ButItFailed { by_player, move_id },
        BattleEvent::ChargingPower { by_player, move_id },
        BattleEvent::WildFled,
        BattleEvent::StatFell {
            by_player,
            move_id,
            stat,
            new_stage: stage,
            magnitude: 1,
        },
        BattleEvent::StatWontGoLower {
            by_player,
            move_id,
            stat,
        },
        BattleEvent::StatLossPrevented {
            by_player,
            move_id,
            stat,
            ability: AbilityId::HYPER_CUTTER,
        },
        BattleEvent::StatRose {
            by_player,
            move_id,
            stat,
            new_stage: stage,
            magnitude: 1,
        },
        BattleEvent::StatWontGoHigher {
            by_player,
            move_id,
            stat,
        },
    ]
}

fn ability_and_status_events() -> Vec<BattleEvent> {
    let (by_player, move_id) = (BY_PLAYER, MOVE);
    vec![
        BattleEvent::SoundproofProtected { by_player, move_id },
        BattleEvent::Paralyzed { by_player, move_id },
        BattleEvent::AlreadyParalyzed { by_player, move_id },
        BattleEvent::LimberProtected { by_player, move_id },
        BattleEvent::SynchronizeLimberProtected { by_player, move_id },
        BattleEvent::ParalyzedBySynchronize { by_player, move_id },
        BattleEvent::Poisoned { by_player, move_id },
        BattleEvent::PoisonedBySynchronize { by_player, move_id },
        BattleEvent::ParalyzedByStatic { by_player, move_id },
        BattleEvent::RoughSkinDamage {
            by_player,
            move_id,
            damage: 1,
        },
        BattleEvent::ParalyzedByAttackerSynchronize { by_player, move_id },
        BattleEvent::AsleepByEffectSpore { by_player, move_id },
        BattleEvent::PoisonedByEffectSpore { by_player, move_id },
        BattleEvent::ParalyzedByEffectSpore { by_player, move_id },
        BattleEvent::PoisonedByPoisonPoint { by_player, move_id },
        BattleEvent::PoisonedByAttackerSynchronize { by_player, move_id },
        BattleEvent::AttackerSynchronizePoisonOrSteelTypeProtected { by_player, move_id },
        BattleEvent::SynchronizeImmunityProtected { by_player, move_id },
        BattleEvent::SynchronizePoisonOrSteelTypeProtected { by_player, move_id },
        BattleEvent::ConfusionInflicted { by_player, move_id },
        BattleEvent::AlreadyConfused { by_player, move_id },
        BattleEvent::OwnTempoProtected { by_player, move_id },
        BattleEvent::HurtByPoison {
            by_player,
            damage: 1,
        },
        BattleEvent::HurtByBurn {
            by_player,
            damage: 1,
        },
        BattleEvent::Seeded { by_player, move_id },
        BattleEvent::LeechSeedEvaded { by_player, move_id },
        BattleEvent::SappedByLeechSeed {
            by_player,
            damage: 1,
        },
        BattleEvent::LeechSeedHealed {
            by_player,
            healed: 1,
        },
        BattleEvent::ItemRestoredHealth {
            by_player,
            item: ItemId::ORAN_BERRY,
            healed: 10,
        },
        BattleEvent::ShedSkinCured {
            by_player,
            status: Status1::Poisoned,
        },
    ]
}

fn party_events() -> Vec<BattleEvent> {
    let (by_player, move_id) = (BY_PLAYER, MOVE);
    vec![
        BattleEvent::RunAttempt {
            by_player,
            success: true,
        },
        BattleEvent::TrainerSentOut {
            species: RATTATA,
            bench_remaining: 1,
        },
        BattleEvent::PlayerSentOut {
            species: RATTATA,
            reserves_remaining: 1,
        },
        BattleEvent::ExpGained {
            amount: 10,
            boosted: false,
        },
        BattleEvent::MoveLearned { move_id },
        BattleEvent::MoveLearnPrompt { move_id },
        BattleEvent::MoveReplaced {
            learned: move_id,
            forgotten: MoveId::SCRATCH,
            slot: 0,
        },
        BattleEvent::MoveLearnDeclined { move_id },
        BattleEvent::MoneyGained(10),
        BattleEvent::Ended(BattleOutcome::PlayerLost),
        BattleEvent::InLove { by_player },
        BattleEvent::ImmobilizedByLove { by_player, move_id },
        BattleEvent::Infatuated { by_player, move_id },
        BattleEvent::ObliviousPreventsAttraction { by_player, move_id },
        BattleEvent::InfatuatedByCuteCharm { by_player, move_id },
        BattleEvent::UsedMove { by_player, move_id },
        BattleEvent::TypeEffectiveness {
            by_player,
            effectiveness: Effectiveness::SuperEffective,
        },
        BattleEvent::AbsorbMadeMoveUseless {
            by_player,
            move_id,
            ability: AbilityId::WATER_ABSORB,
        },
    ]
}

/// How many variants `BattleEvent` has.
const VARIANTS: usize = 83;

/// A variant's position in `BattleEvent`. The match has no wildcard, so a new
/// variant stops this file compiling until it is listed here and in the
/// fixture above.
const fn variant(event: &BattleEvent) -> usize {
    match event {
        BattleEvent::RunAttempt { .. } => 0,
        BattleEvent::Missed { .. } => 1,
        BattleEvent::FailedNoPp { .. } => 2,
        BattleEvent::FastAsleep { .. } => 3,
        BattleEvent::WokeUp { .. } => 4,
        BattleEvent::SnappedOutOfConfusion { .. } => 5,
        BattleEvent::Confused { .. } => 6,
        BattleEvent::ConfusionSelfHit { .. } => 7,
        BattleEvent::FullyParalyzed { .. } => 8,
        BattleEvent::NoEffect { .. } => 9,
        BattleEvent::LevitateBlocked { .. } => 10,
        BattleEvent::WonderGuardBlocked { .. } => 11,
        BattleEvent::AbilityHealed { .. } => 12,
        BattleEvent::FlashFireAbsorbed { .. } => 13,
        BattleEvent::Hit { .. } => 14,
        BattleEvent::Recoil { .. } => 15,
        BattleEvent::Fainted { .. } => 16,
        BattleEvent::Drained { .. } => 17,
        BattleEvent::LiquidOoze { .. } => 18,
        BattleEvent::MultiHit { .. } => 19,
        BattleEvent::NothingHappened { .. } => 20,
        BattleEvent::GettingPumped { .. } => 21,
        BattleEvent::ButItFailed { .. } => 22,
        BattleEvent::ChargingPower { .. } => 23,
        BattleEvent::WildFled => 24,
        BattleEvent::StatFell { .. } => 25,
        BattleEvent::StatWontGoLower { .. } => 26,
        BattleEvent::StatLossPrevented { .. } => 27,
        BattleEvent::SoundproofProtected { .. } => 28,
        BattleEvent::StatRose { .. } => 29,
        BattleEvent::StatWontGoHigher { .. } => 30,
        BattleEvent::Paralyzed { .. } => 31,
        BattleEvent::AlreadyParalyzed { .. } => 32,
        BattleEvent::LimberProtected { .. } => 33,
        BattleEvent::SynchronizeLimberProtected { .. } => 34,
        BattleEvent::ParalyzedBySynchronize { .. } => 35,
        BattleEvent::Poisoned { .. } => 36,
        BattleEvent::PoisonedBySynchronize { .. } => 37,
        BattleEvent::ParalyzedByStatic { .. } => 38,
        BattleEvent::RoughSkinDamage { .. } => 39,
        BattleEvent::ParalyzedByAttackerSynchronize { .. } => 40,
        BattleEvent::AsleepByEffectSpore { .. } => 41,
        BattleEvent::PoisonedByEffectSpore { .. } => 42,
        BattleEvent::ParalyzedByEffectSpore { .. } => 43,
        BattleEvent::PoisonedByPoisonPoint { .. } => 44,
        BattleEvent::PoisonedByAttackerSynchronize { .. } => 45,
        BattleEvent::AttackerSynchronizePoisonOrSteelTypeProtected { .. } => 46,
        BattleEvent::SynchronizeImmunityProtected { .. } => 47,
        BattleEvent::SynchronizePoisonOrSteelTypeProtected { .. } => 48,
        BattleEvent::ConfusionInflicted { .. } => 49,
        BattleEvent::AlreadyConfused { .. } => 50,
        BattleEvent::OwnTempoProtected { .. } => 51,
        BattleEvent::HurtByPoison { .. } => 52,
        BattleEvent::HurtByBurn { .. } => 53,
        BattleEvent::ShedSkinCured { .. } => 54,
        BattleEvent::TrainerSentOut { .. } => 55,
        BattleEvent::PlayerSentOut { .. } => 56,
        BattleEvent::ExpGained { .. } => 57,
        BattleEvent::MoveLearned { .. } => 58,
        BattleEvent::MoveLearnPrompt { .. } => 59,
        BattleEvent::MoveReplaced { .. } => 60,
        BattleEvent::MoveLearnDeclined { .. } => 61,
        BattleEvent::MoneyGained(..) => 62,
        BattleEvent::Ended(..) => 63,
        BattleEvent::UsedMove { .. } => 64,
        BattleEvent::TypeEffectiveness { .. } => 65,
        BattleEvent::AbsorbMadeMoveUseless { .. } => 66,
        BattleEvent::LoafingAround { .. } => 67,
        BattleEvent::FrozenSolid { .. } => 68,
        BattleEvent::Thawed { .. } => 69,
        BattleEvent::ThawedByMove { .. } => 70,
        BattleEvent::Burned { .. } => 71,
        BattleEvent::TargetDefrosted { .. } => 72,
        BattleEvent::Seeded { .. } => 73,
        BattleEvent::SappedByLeechSeed { .. } => 74,
        BattleEvent::LeechSeedHealed { .. } => 75,
        BattleEvent::ItemRestoredHealth { .. } => 76,
        BattleEvent::LeechSeedEvaded { .. } => 77,
        BattleEvent::InLove { .. } => 78,
        BattleEvent::ImmobilizedByLove { .. } => 79,
        BattleEvent::Infatuated { .. } => 80,
        BattleEvent::ObliviousPreventsAttraction { .. } => 81,
        BattleEvent::InfatuatedByCuteCharm { .. } => 82,
    }
}

#[test]
fn every_event_variant_maps_to_at_least_the_requests_upstream_prints() {
    let mut narrator = narrator(BattleKind::Trainer);

    // A landed `Hit` and the seeder's `LeechSeedHealed` print nothing upstream.
    for event in every_event() {
        let requests = narrator.requests(&AssetNames, &event);
        assert!(
            requests.is_ok_and(|requests| !requests.is_empty()
                || matches!(
                    event,
                    BattleEvent::Hit { .. } | BattleEvent::LeechSeedHealed { .. }
                )),
            "{event:?} has no narration"
        );
    }
}

#[test]
fn the_fixture_list_names_each_variant_once() {
    let events = every_event();
    let kinds: std::collections::HashSet<_> = events.iter().map(std::mem::discriminant).collect();

    assert_eq!(kinds.len(), events.len());
    let listed: std::collections::BTreeSet<_> = events.iter().map(variant).collect();
    assert_eq!(listed, (0..VARIANTS).collect(), "every variant is listed");
    assert_eq!(
        narrator(BattleKind::Wild)
            .requests(&AssetNames, &events[0])
            .map(|r| r.len()),
        Ok::<_, NarrationError>(1)
    );
}

/// A free slot takes the move straight into `BattleScript_LearnedNewMove`
/// (`data/battle_scripts_1.s:3182`-`:3188`), so the event prints the same
/// PKMNLEARNEDMOVE line, with the same nickname and move buffers, that
/// closes a replacement.
#[test]
fn a_free_slot_learn_prints_the_learned_line_a_replacement_ends_with() {
    let mut narrator = narrator(BattleKind::Wild);
    let move_id = MoveId::EMBER;

    let learned = narrator
        .requests(&AssetNames, &BattleEvent::MoveLearned { move_id })
        .unwrap();
    let replaced = narrator
        .requests(
            &AssetNames,
            &BattleEvent::MoveReplaced {
                learned: move_id,
                forgotten: MoveId::SCRATCH,
                slot: 0,
            },
        )
        .unwrap();

    assert_eq!(learned.len(), 1);
    assert_eq!(learned[0].template, TemplateId::Table(PKMNLEARNEDMOVE));
    assert_eq!(Some(&learned[0]), replaced.last());
}
