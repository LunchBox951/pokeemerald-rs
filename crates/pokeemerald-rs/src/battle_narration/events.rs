//! Which messages upstream prints for each [`BattleEvent`], and with which
//! battlers, abilities, and buffers.
//!
//! In every arm `user` is the side that used the move and `target` the other.
//! Roles follow the script that prints the string: a stat drop names the
//! target (`STRINGID_DEFENDERSSTATFELL`), a stat rise the user
//! (`STRINGID_ATTACKERSSTATROSE`), and an ability that acts at move end
//! (`SetMoveEffect`'s synchronize and contact cases) names its holder as the
//! scripting battler and the afflicted battler as the effect battler.

use assets::{AbilityId, Effectiveness};
use battle::{BattleEvent, ChangedStat, Status1};

use super::context::{BattleBuffer, BufferOp, NarrationContext, Side, StatusName};
use super::lifecycle;
use super::names::NarrationNames;
use super::narrator::Narrator;
use super::string_ids as id;
use super::{NarrationError, NarrationRequest, TemplateId};

fn table(template: u16, context: NarrationContext) -> NarrationRequest {
    NarrationRequest {
        template: TemplateId::Table(template),
        context,
    }
}

/// `gBattleStats` order in `gStatNamesTable`.
const fn stat_index(stat: ChangedStat) -> u8 {
    match stat {
        ChangedStat::Attack => 1,
        ChangedStat::Defense => 2,
        ChangedStat::Speed => 3,
        ChangedStat::SpAttack => 4,
        ChangedStat::SpDefense => 5,
        ChangedStat::Accuracy => 6,
        ChangedStat::Evasion => 7,
    }
}

fn stat_buffer(stat: ChangedStat) -> BattleBuffer {
    BattleBuffer::Program(vec![BufferOp::Stat(stat_index(stat))])
}

/// `ChangeStatBuffs`: a change of two stages prefixes `harshly` or `sharply`
/// (`src/battle_script_commands.c:6993`-`:7054`).
fn stat_change_buffer(rose: bool, magnitude: u8) -> BattleBuffer {
    let (strongly, plain) = if rose {
        (id::STATSHARPLY, id::STATROSE)
    } else {
        (id::STATHARSHLY, id::STATFELL)
    };
    let mut ops = Vec::new();
    if magnitude == 2 {
        ops.push(BufferOp::String(strongly));
    }
    ops.push(BufferOp::String(plain));
    BattleBuffer::Program(ops)
}

fn status_buffer(status: Status1) -> Result<BattleBuffer, NarrationError> {
    match status {
        Status1::Poisoned => Ok(BattleBuffer::Status(StatusName::Poison)),
        Status1::Paralysed => Ok(BattleBuffer::Status(StatusName::Paralysis)),
        Status1::Healthy => Err(NarrationError::MissingContext("cured status")),
    }
}

/// The requests for `event`, updating `narrator`'s active battlers first.
pub(super) fn requests(
    narrator: &mut Narrator,
    names: &impl NarrationNames,
    event: &BattleEvent,
) -> Result<Vec<NarrationRequest>, NarrationError> {
    if let Some(requests) = announcements(narrator, event) {
        return Ok(requests);
    }
    if let Some(requests) = ability_results(narrator, event)? {
        return Ok(requests);
    }
    if let Some(requests) = status_results(narrator, event) {
        return Ok(requests);
    }
    if let Some(requests) = stat_results(narrator, event) {
        return Ok(requests);
    }
    lifecycle::requests(narrator, names, event)
}

/// Announcements and failures printed in the user's own name.
fn announcements(narrator: &Narrator, event: &BattleEvent) -> Option<Vec<NarrationRequest>> {
    let base = narrator.context();
    let request = match *event {
        BattleEvent::UsedMove { by_player, move_id } => {
            let context = base
                .user_vs_target(Side::user(by_player))
                .with_move(move_id)
                .with_buffer(
                    1,
                    BattleBuffer::Program(vec![
                        BufferOp::Move(move_id),
                        BufferOp::Special("sText_ExclamationMark"),
                    ]),
                );
            NarrationRequest {
                template: TemplateId::Special("sText_AttackerUsedX"),
                context,
            }
        }
        BattleEvent::Missed { by_player, .. } => {
            table(id::ATTACKMISSED, base.user_vs_target(Side::user(by_player)))
        }
        BattleEvent::FailedNoPp { .. } => table(id::BUTNOPPLEFT, base),
        BattleEvent::SnappedOutOfConfusion { by_player } => table(
            id::PKMNHEALEDCONFUSION,
            base.user_vs_target(Side::user(by_player)),
        ),
        BattleEvent::Confused { by_player } => table(
            id::PKMNISCONFUSED,
            base.user_vs_target(Side::user(by_player)),
        ),
        BattleEvent::ConfusionSelfHit { .. } => table(id::ITHURTCONFUSION, base),
        BattleEvent::FullyParalyzed { by_player, .. } => table(
            id::PKMNISPARALYZED,
            base.user_vs_target(Side::user(by_player)),
        ),
        BattleEvent::NoEffect { by_player, .. } => table(
            id::ITDOESNTAFFECT,
            base.user_vs_target(Side::user(by_player)),
        ),
        BattleEvent::Recoil { by_player, .. } => table(
            id::PKMNHITWITHRECOIL,
            base.user_vs_target(Side::user(by_player)),
        ),
        BattleEvent::Fainted { by_player } => {
            let mut context = base;
            context.defender = Some(Side::user(by_player));
            table(id::TARGETFAINTED, context)
        }
        BattleEvent::Drained { by_player, .. } => table(
            id::PKMNENERGYDRAINED,
            base.user_vs_target(Side::user(by_player)),
        ),
        BattleEvent::LiquidOoze { .. } => table(id::ITSUCKEDLIQUIDOOZE, base),
        BattleEvent::NothingHappened { .. } => table(id::BUTNOTHINGHAPPENED, base),
        BattleEvent::ButItFailed { .. } => table(id::BUTITFAILED, base),
        BattleEvent::GettingPumped { by_player, .. } => table(
            id::PKMNGETTINGPUMPED,
            base.user_vs_target(Side::user(by_player)),
        ),
        BattleEvent::ChargingPower { by_player, .. } => table(
            id::PKMNCHARGINGPOWER,
            base.user_vs_target(Side::user(by_player)),
        ),
        BattleEvent::HurtByPoison { by_player, .. } => {
            let mut context = base;
            context.attacker = Some(Side::user(by_player));
            table(id::PKMNHURTBYPOISON, context)
        }
        BattleEvent::Hit {
            is_critical: true, ..
        } => table(id::CRITICALHIT, base),
        BattleEvent::Hit { .. } => return Some(Vec::new()),
        BattleEvent::TypeEffectiveness { effectiveness, .. } => match effectiveness {
            Effectiveness::SuperEffective => table(id::SUPEREFFECTIVE, base),
            Effectiveness::NotVeryEffective => table(id::NOTVERYEFFECTIVE, base),
            Effectiveness::NoEffect | Effectiveness::Normal => return Some(Vec::new()),
        },
        BattleEvent::AbsorbMadeMoveUseless { by_player, move_id } => table(
            id::PKMNSXMADEYUSELESS,
            base.user_vs_target(Side::user(by_player))
                .with_move(move_id),
        ),
        BattleEvent::MultiHit { hits, .. } => table(
            id::HITXTIMES,
            base.with_buffer(
                0,
                BattleBuffer::Program(vec![BufferOp::Number {
                    value: u32::from(hits),
                    width: 1,
                }]),
            ),
        ),
        _ => return None,
    };
    Some(vec![request])
}

/// Results that name an ability, and the stat changes it prevents.
fn ability_results(
    narrator: &Narrator,
    event: &BattleEvent,
) -> Result<Option<Vec<NarrationRequest>>, NarrationError> {
    let base = narrator.context();
    let request = match *event {
        BattleEvent::LevitateBlocked { by_player, move_id } => table(
            id::PKMNMAKESGROUNDMISS,
            base.user_vs_target(Side::user(by_player))
                .with_move(move_id)
                .with_ability(Side::user(by_player).other(), AbilityId::LEVITATE),
        ),
        BattleEvent::WonderGuardBlocked { by_player, move_id } => table(
            id::AVOIDEDDAMAGE,
            base.user_vs_target(Side::user(by_player))
                .with_move(move_id)
                .with_ability(Side::user(by_player).other(), AbilityId::WONDER_GUARD),
        ),
        BattleEvent::AbilityHealed {
            by_player, move_id, ..
        } => table(
            id::PKMNRESTOREDHPUSING,
            base.user_vs_target(Side::user(by_player))
                .with_move(move_id),
        ),
        BattleEvent::FlashFireAbsorbed {
            by_player,
            move_id,
            boosted,
        } => {
            let template = if boosted {
                id::PKMNRAISEDFIREPOWERWITH
            } else {
                id::PKMNSXMADEYINEFFECTIVE
            };
            table(
                template,
                base.user_vs_target(Side::user(by_player))
                    .with_move(move_id)
                    .with_ability(Side::user(by_player).other(), AbilityId::FLASH_FIRE),
            )
        }
        BattleEvent::SoundproofProtected { by_player, move_id } => table(
            id::PKMNSXBLOCKSY,
            base.user_vs_target(Side::user(by_player))
                .with_move(move_id)
                .with_ability(Side::user(by_player).other(), AbilityId::SOUNDPROOF),
        ),
        BattleEvent::LimberProtected { by_player, .. } => table(
            id::PKMNPREVENTSPARALYSISWITH,
            base.user_vs_target(Side::user(by_player))
                .with_ability(Side::user(by_player).other(), AbilityId::LIMBER),
        ),
        BattleEvent::OwnTempoProtected { by_player, .. } => table(
            id::PKMNPREVENTSCONFUSIONWITH,
            base.user_vs_target(Side::user(by_player))
                .with_ability(Side::user(by_player).other(), AbilityId::OWN_TEMPO),
        ),
        BattleEvent::StatLossPrevented {
            by_player,
            stat,
            ability,
            ..
        } => {
            let holder = Side::user(by_player).other();
            let template = if ability == AbilityId::HYPER_CUTTER || ability == AbilityId::KEEN_EYE {
                id::PKMNSXPREVENTSYLOSS
            } else {
                id::PKMNPREVENTSSTATLOSSWITH
            };
            let mut context = base
                .user_vs_target(Side::user(by_player))
                .with_ability(holder, ability)
                .with_buffer(0, stat_buffer(stat));
            context.scripting = Some(holder);
            table(template, context)
        }
        BattleEvent::ShedSkinCured { by_player, status } => {
            let side = Side::user(by_player);
            let mut context = base
                .with_ability(side, AbilityId::SHED_SKIN)
                .with_buffer(0, status_buffer(status)?);
            context.scripting = Some(side);
            table(id::PKMNSXCUREDYPROBLEM, context)
        }
        BattleEvent::RoughSkinDamage { by_player, .. } => table(
            id::PKMNHURTSWITH,
            base.user_vs_target(Side::user(by_player))
                .with_ability(Side::user(by_player).other(), AbilityId::ROUGH_SKIN),
        ),
        _ => return Ok(None),
    };
    Ok(Some(vec![request]))
}

/// Status moves and the abilities that pass a status on.
fn status_results(narrator: &Narrator, event: &BattleEvent) -> Option<Vec<NarrationRequest>> {
    let base = narrator.context();
    let request = match *event {
        BattleEvent::Paralyzed { by_player, .. } => table(
            id::PKMNWASPARALYZED,
            base.user_vs_target(Side::user(by_player)),
        ),
        BattleEvent::AlreadyParalyzed { by_player, .. } => table(
            id::PKMNISALREADYPARALYZED,
            base.user_vs_target(Side::user(by_player)),
        ),
        BattleEvent::Poisoned { by_player, .. } => table(
            id::PKMNWASPOISONED,
            base.user_vs_target(Side::user(by_player)),
        ),
        BattleEvent::ConfusionInflicted { by_player, .. } => table(
            id::PKMNWASCONFUSED,
            base.user_vs_target(Side::user(by_player)),
        ),
        BattleEvent::AlreadyConfused { by_player, .. } => table(
            id::PKMNALREADYCONFUSED,
            base.user_vs_target(Side::user(by_player)),
        ),
        BattleEvent::ParalyzedBySynchronize { by_player, .. } => reflected(
            id::PKMNWASPARALYZEDBY,
            base,
            Side::user(by_player),
            AbilityId::SYNCHRONIZE,
            Holder::Target,
        ),
        BattleEvent::PoisonedBySynchronize { by_player, .. } => reflected(
            id::PKMNPOISONEDBY,
            base,
            Side::user(by_player),
            AbilityId::SYNCHRONIZE,
            Holder::Target,
        ),
        BattleEvent::ParalyzedByStatic { by_player, .. } => reflected(
            id::PKMNWASPARALYZEDBY,
            base,
            Side::user(by_player),
            AbilityId::STATIC,
            Holder::Target,
        ),
        BattleEvent::PoisonedByPoisonPoint { by_player, .. } => reflected(
            id::PKMNPOISONEDBY,
            base,
            Side::user(by_player),
            AbilityId::POISON_POINT,
            Holder::Target,
        ),
        BattleEvent::ParalyzedByAttackerSynchronize { by_player, .. } => reflected(
            id::PKMNWASPARALYZEDBY,
            base,
            Side::user(by_player),
            AbilityId::SYNCHRONIZE,
            Holder::User,
        ),
        BattleEvent::PoisonedByAttackerSynchronize { by_player, .. } => reflected(
            id::PKMNPOISONEDBY,
            base,
            Side::user(by_player),
            AbilityId::SYNCHRONIZE,
            Holder::User,
        ),
        BattleEvent::SynchronizePoisonOrSteelTypeProtected { by_player, .. } => reflected(
            id::PKMNSXHADNOEFFECTONY,
            base,
            Side::user(by_player),
            AbilityId::SYNCHRONIZE,
            Holder::Target,
        ),
        BattleEvent::AttackerSynchronizePoisonOrSteelTypeProtected { by_player, .. } => reflected(
            id::PKMNSXHADNOEFFECTONY,
            base,
            Side::user(by_player),
            AbilityId::SYNCHRONIZE,
            Holder::User,
        ),
        BattleEvent::SynchronizeLimberProtected { by_player, .. } => {
            blocked_reflection(base, Side::user(by_player), AbilityId::LIMBER)
        }
        BattleEvent::SynchronizeImmunityProtected { by_player, .. } => {
            blocked_reflection(base, Side::user(by_player), AbilityId::IMMUNITY)
        }
        _ => return None,
    };
    Some(vec![request])
}

/// Which battler of a move holds the ability that acts at move end.
#[derive(Clone, Copy)]
enum Holder {
    /// The move's user.
    User,
    /// The move's target.
    Target,
}

/// An ability acting on the other battler: the holder is the scripting
/// battler and the afflicted battler the effect battler.
fn reflected(
    template: u16,
    base: NarrationContext,
    user: Side,
    ability: AbilityId,
    holder: Holder,
) -> NarrationRequest {
    let (holder, afflicted) = match holder {
        Holder::User => (user, user.other()),
        Holder::Target => (user.other(), user),
    };
    let mut context = base.user_vs_target(user).with_ability(holder, ability);
    context.scripting = Some(holder);
    context.effect = Some(afflicted);
    table(template, context)
}

/// `STRINGID_PKMNSXPREVENTSYSZ`: the original attacker's own `ability` stops
/// the target's Synchronize from working (`src/battle_script_commands.c:2395`-`:2410`);
/// attacker and target keep their roles from the move.
fn blocked_reflection(base: NarrationContext, user: Side, ability: AbilityId) -> NarrationRequest {
    let context = base
        .user_vs_target(user)
        .with_ability(user, ability)
        .with_ability(user.other(), AbilityId::SYNCHRONIZE);
    table(id::PKMNSXPREVENTSYSZ, context)
}

/// Stat changes and the abilities that stop them.
fn stat_results(narrator: &Narrator, event: &BattleEvent) -> Option<Vec<NarrationRequest>> {
    let base = narrator.context();
    let request = match *event {
        BattleEvent::StatFell {
            by_player,
            stat,
            magnitude,
            ..
        } => table(
            id::DEFENDERSSTATFELL,
            base.user_vs_target(Side::user(by_player))
                .with_buffer(0, stat_buffer(stat))
                .with_buffer(1, stat_change_buffer(false, magnitude)),
        ),
        BattleEvent::StatRose {
            by_player,
            stat,
            magnitude,
            ..
        } => table(
            id::ATTACKERSSTATROSE,
            base.user_vs_target(Side::user(by_player))
                .with_buffer(0, stat_buffer(stat))
                .with_buffer(1, stat_change_buffer(true, magnitude)),
        ),
        BattleEvent::StatWontGoLower {
            by_player, stat, ..
        } => table(
            id::STATSWONTDECREASE,
            base.user_vs_target(Side::user(by_player))
                .with_buffer(0, stat_buffer(stat)),
        ),
        BattleEvent::StatWontGoHigher {
            by_player, stat, ..
        } => table(
            id::STATSWONTINCREASE,
            base.user_vs_target(Side::user(by_player))
                .with_buffer(0, stat_buffer(stat)),
        ),
        _ => return None,
    };
    Some(vec![request])
}
