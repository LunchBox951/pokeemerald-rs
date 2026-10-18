//! Messages for a battle's run attempts, send-outs, experience, move
//! learning, and outcome.

use battle::{BattleEvent, BattleOutcome};

use super::context::{BattleBuffer, Battler, BufferOp, Side};
use super::names::NarrationNames;
use super::narrator::{side_index, Narrator};
use super::string_ids as id;
use super::{NarrationError, NarrationRequest, TemplateId};

fn table(template: u16, context: super::NarrationContext) -> NarrationRequest {
    NarrationRequest {
        template: TemplateId::Table(template),
        context,
    }
}

fn mon_nick(name: &str) -> BattleBuffer {
    BattleBuffer::Program(vec![BufferOp::MonNick(name.to_owned())])
}

/// The player's send-out wording for the opponent's remaining health
/// (`STRINGID_SWITCHINMON`, `src/battle_message.c:2095`-`:2107`).
const fn player_switch_in(hp_scale: u8) -> &'static str {
    match hp_scale {
        0 => "sText_GoPkmn2",
        1 => "sText_DoItPkmn",
        2 => "sText_GoForItPkmn",
        _ => "sText_YourFoesWeakGetEmPkmn",
    }
}

/// Makes the staged replacement for `side` (or, absent one, a battler named
/// after `species`) the active battler and returns its name.
fn send_out(
    narrator: &mut Narrator,
    names: &impl NarrationNames,
    side: Side,
    species: assets::SpeciesId,
) -> Result<String, NarrationError> {
    let incoming = match narrator.staged[side_index(side)].take() {
        Some(battler) => battler,
        None => Battler {
            name: names
                .species_name(species)
                .ok_or(NarrationError::UnknownName("species"))?,
            ability: None,
        },
    };
    let name = incoming.name.clone();
    match side {
        Side::Player => narrator.scene.player = incoming,
        Side::Opponent => narrator.scene.opponent = incoming,
    }
    Ok(name)
}

/// The requests for `event` that are not a move's own result.
pub(super) fn requests(
    narrator: &mut Narrator,
    names: &impl NarrationNames,
    event: &BattleEvent,
) -> Result<Vec<NarrationRequest>, NarrationError> {
    if let Some(requests) = battle_flow(narrator, event) {
        return Ok(requests);
    }
    party(narrator, names, event)
}

/// Run attempts, a fleeing wild Pokémon, and how the battle ends.
fn battle_flow(narrator: &Narrator, event: &BattleEvent) -> Option<Vec<NarrationRequest>> {
    let requests = match *event {
        BattleEvent::RunAttempt { success, .. } => {
            let template = if success {
                id::GOTAWAYSAFELY
            } else {
                id::CANTESCAPE
            };
            vec![table(template, narrator.context())]
        }
        BattleEvent::WildFled => {
            let name = narrator.scene().opponent.name.clone();
            let context = narrator.context().with_buffer(0, BattleBuffer::Text(name));
            vec![table(id::WILDPKMNFLED, context)]
        }
        BattleEvent::MoneyGained(amount) => {
            let context = narrator.context().with_buffer(
                0,
                BattleBuffer::Program(vec![BufferOp::Number {
                    value: amount,
                    width: 5,
                }]),
            );
            vec![
                table(id::PLAYERDEFEATEDTRAINER1, narrator.context()),
                table(id::TRAINER1LOSETEXT, narrator.context()),
                table(id::PLAYERGOTMONEY, context),
            ]
        }
        BattleEvent::Ended(BattleOutcome::PlayerLost) => vec![
            table(id::PLAYERWHITEOUT, narrator.context()),
            table(id::PLAYERWHITEOUT2, narrator.context()),
        ],
        BattleEvent::Ended(
            BattleOutcome::PlayerWon | BattleOutcome::PlayerRan | BattleOutcome::WildFled,
        ) => Vec::new(),
        _ => return None,
    };
    Some(requests)
}

/// Send-outs, experience, and move learning.
fn party(
    narrator: &mut Narrator,
    names: &impl NarrationNames,
    event: &BattleEvent,
) -> Result<Vec<NarrationRequest>, NarrationError> {
    let player_nick = narrator.scene().player.name.clone();
    match *event {
        BattleEvent::TrainerSentOut { species, .. } => {
            let name = send_out(narrator, names, Side::Opponent, species)?;
            let mut context = narrator.context().with_buffer(0, mon_nick(&name));
            context.scripting = Some(Side::Opponent);
            Ok(vec![NarrationRequest {
                template: TemplateId::Special("sText_Trainer1SentOutPkmn2"),
                context,
            }])
        }
        BattleEvent::PlayerSentOut { species, .. } => {
            let name = send_out(narrator, names, Side::Player, species)?;
            let mut context = narrator.context().with_buffer(0, mon_nick(&name));
            context.scripting = Some(Side::Player);
            Ok(vec![NarrationRequest {
                template: TemplateId::Special(player_switch_in(narrator.scene().hp_scale)),
                context,
            }])
        }
        BattleEvent::ExpGained(amount) => {
            let context = narrator
                .context()
                .with_buffer(0, mon_nick(&player_nick))
                .with_buffer(
                    1,
                    BattleBuffer::Program(vec![BufferOp::String(id::EMPTYSTRING3)]),
                )
                .with_buffer(
                    2,
                    BattleBuffer::Program(vec![BufferOp::Number {
                        value: amount,
                        width: 5,
                    }]),
                );
            Ok(vec![table(id::PKMNGAINEDEXP, context)])
        }
        BattleEvent::MoveLearnPrompt { move_id } => {
            let context = narrator
                .context()
                .with_buffer(0, mon_nick(&player_nick))
                .with_buffer(1, BattleBuffer::Program(vec![BufferOp::Move(move_id)]));
            Ok([
                id::TRYTOLEARNMOVE1,
                id::TRYTOLEARNMOVE2,
                id::TRYTOLEARNMOVE3,
            ]
            .map(|template| table(template, context.clone()))
            .to_vec())
        }
        BattleEvent::MoveReplaced {
            learned, forgotten, ..
        } => {
            let with_move = |move_id| {
                narrator
                    .context()
                    .with_buffer(0, mon_nick(&player_nick))
                    .with_buffer(1, BattleBuffer::Program(vec![BufferOp::Move(move_id)]))
            };
            Ok(vec![
                table(id::POOF123, narrator.context()),
                table(id::PKMNFORGOTMOVE, with_move(forgotten)),
                table(id::ANDELLIPSIS, narrator.context()),
                table(id::PKMNLEARNEDMOVE, with_move(learned)),
            ])
        }
        BattleEvent::MoveLearnDeclined { move_id } => {
            let context = narrator
                .context()
                .with_buffer(0, mon_nick(&player_nick))
                .with_buffer(1, BattleBuffer::Program(vec![BufferOp::Move(move_id)]));
            Ok(vec![table(id::DIDNOTLEARNMOVE, context)])
        }
        _ => Err(NarrationError::UnmappedEvent),
    }
}
