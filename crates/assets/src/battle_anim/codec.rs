//! The `battle-anim/catalog` wire format: all integers little-endian, strings
//! a `u16` byte length then UTF-8.
//!
//! ```text
//! version: u16
//! program_count: u16
//! program_count x:
//!   symbol: string, command_count: u16, command_count x command
//! for each of Moves, StatusConditions, General, Special:
//!   AnimationTable::len x program_id: u16
//! ```
//!
//! A command is its `sScriptCmdTable` opcode byte then its operands in the
//! order of `asm/macros/battle_anim_script.inc`. Program ids stand where
//! upstream has a script pointer; an argument list is a `u8` count then `i16`s.
//! No byte follows the last table.

use super::{
    AnimCommand, AnimSymbol, AnimationSelector, AnimationTable, BattleAnimationCatalog,
    BattleAnimationError, BattleAnimationProgram, EffectSpriteId, PanSweep, ProgramId,
};

const SCHEMA_VERSION: u16 = 1;

pub(super) fn encode(catalog: &BattleAnimationCatalog) -> Vec<u8> {
    let mut out = Vec::new();
    put_u16(&mut out, SCHEMA_VERSION);
    put_len(&mut out, catalog.programs.len());
    for program in &catalog.programs {
        put_symbol(&mut out, &program.symbol);
        put_len(&mut out, program.commands.len());
        for command in &program.commands {
            put_command(&mut out, command);
        }
    }
    for table in &catalog.tables {
        for program in table {
            put_u16(&mut out, program.get());
        }
    }
    out
}

fn put_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_i16(out: &mut Vec<u8>, value: i16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_i8(out: &mut Vec<u8>, value: i8) {
    out.extend_from_slice(&value.to_le_bytes());
}

/// Counts are bounded by construction: programs and commands by their `u16`
/// checks, symbols by [`AnimSymbol::new`].
fn put_len(out: &mut Vec<u8>, count: usize) {
    put_u16(
        out,
        u16::try_from(count).expect("count bounded by construction"),
    );
}

fn put_symbol(out: &mut Vec<u8>, symbol: &AnimSymbol) {
    put_len(out, symbol.as_str().len());
    out.extend_from_slice(symbol.as_str().as_bytes());
}

/// The count is bounded by `ANIM_ARGS_COUNT` at construction.
fn put_args(out: &mut Vec<u8>, args: &[i16]) {
    out.push(u8::try_from(args.len()).expect("count bounded by construction"));
    for arg in args {
        put_i16(out, *arg);
    }
}

fn put_sweep(out: &mut Vec<u8>, opcode: u8, sweep: PanSweep) {
    out.push(opcode);
    put_u16(out, sweep.se);
    put_i8(out, sweep.current_pan);
    put_i8(out, sweep.target_pan);
    put_i8(out, sweep.increment_pan);
    out.push(sweep.delay);
}

#[expect(
    clippy::too_many_lines,
    reason = "one exhaustive match keeps every opcode's operand layout together"
)]
fn put_command(out: &mut Vec<u8>, command: &AnimCommand) {
    match command {
        AnimCommand::LoadSpriteGfx(sprite) => {
            out.push(0x00);
            put_u16(out, sprite.get());
        }
        AnimCommand::UnloadSpriteGfx(sprite) => {
            out.push(0x01);
            put_u16(out, sprite.get());
        }
        AnimCommand::CreateSprite {
            template,
            relative_to_target,
            subpriority,
            args,
        } => {
            out.push(0x02);
            put_symbol(out, template);
            out.push(u8::from(*relative_to_target));
            out.push(*subpriority);
            put_args(out, args);
        }
        AnimCommand::CreateVisualTask {
            task,
            priority,
            args,
        } => {
            out.push(0x03);
            put_symbol(out, task);
            out.push(*priority);
            put_args(out, args);
        }
        AnimCommand::Delay(frames) => out.extend_from_slice(&[0x04, *frames]),
        AnimCommand::WaitForVisualFinish => out.push(0x05),
        AnimCommand::Nop => out.push(0x06),
        AnimCommand::Nop2 => out.push(0x07),
        AnimCommand::End => out.push(0x08),
        AnimCommand::PlaySe(se) => {
            out.push(0x09);
            put_u16(out, *se);
        }
        AnimCommand::MonBg(battler) => out.extend_from_slice(&[0x0A, *battler]),
        AnimCommand::ClearMonBg(battler) => out.extend_from_slice(&[0x0B, *battler]),
        AnimCommand::SetAlpha { eva, evb } => out.extend_from_slice(&[0x0C, *eva, *evb]),
        AnimCommand::BlendOff => out.push(0x0D),
        AnimCommand::Call(target) => {
            out.push(0x0E);
            put_u16(out, target.get());
        }
        AnimCommand::Return => out.push(0x0F),
        AnimCommand::SetArg { arg, value } => {
            out.extend_from_slice(&[0x10, *arg]);
            put_i16(out, *value);
        }
        AnimCommand::ChooseTwoTurnAnim { even, odd } => {
            out.push(0x11);
            put_u16(out, even.get());
            put_u16(out, odd.get());
        }
        AnimCommand::JumpIfMoveTurn { turn, target } => {
            out.extend_from_slice(&[0x12, *turn]);
            put_u16(out, target.get());
        }
        AnimCommand::Goto(target) => {
            out.push(0x13);
            put_u16(out, target.get());
        }
        AnimCommand::FadeToBg(bg) => out.extend_from_slice(&[0x14, *bg]),
        AnimCommand::RestoreBg => out.push(0x15),
        AnimCommand::WaitBgFadeOut => out.push(0x16),
        AnimCommand::WaitBgFadeIn => out.push(0x17),
        AnimCommand::ChangeBg(bg) => out.extend_from_slice(&[0x18, *bg]),
        AnimCommand::PlaySeWithPan { se, pan } => {
            out.push(0x19);
            put_u16(out, *se);
            put_i8(out, *pan);
        }
        AnimCommand::SetPan(pan) => {
            out.push(0x1A);
            put_i8(out, *pan);
        }
        AnimCommand::PanSe(sweep) => put_sweep(out, 0x1B, *sweep),
        AnimCommand::LoopSeWithPan {
            se,
            pan,
            wait,
            times,
        } => {
            out.push(0x1C);
            put_u16(out, *se);
            put_i8(out, *pan);
            out.extend_from_slice(&[*wait, *times]);
        }
        AnimCommand::WaitPlaySeWithPan { se, pan, wait } => {
            out.push(0x1D);
            put_u16(out, *se);
            put_i8(out, *pan);
            out.push(*wait);
        }
        AnimCommand::SetBldCnt(bldcnt) => {
            out.push(0x1E);
            put_u16(out, *bldcnt);
        }
        AnimCommand::CreateSoundTask { task, args } => {
            out.push(0x1F);
            put_symbol(out, task);
            put_args(out, args);
        }
        AnimCommand::WaitSound => out.push(0x20),
        AnimCommand::JumpArgEq { arg, value, target } => {
            out.extend_from_slice(&[0x21, *arg]);
            put_i16(out, *value);
            put_u16(out, target.get());
        }
        AnimCommand::MonBgStatic(battler) => out.extend_from_slice(&[0x22, *battler]),
        AnimCommand::ClearMonBgStatic(battler) => out.extend_from_slice(&[0x23, *battler]),
        AnimCommand::JumpIfContest(target) => {
            out.push(0x24);
            put_u16(out, target.get());
        }
        AnimCommand::FadeToBgFromSet {
            opponent,
            player,
            contest,
        } => out.extend_from_slice(&[0x25, *opponent, *player, *contest]),
        AnimCommand::PanSeAdjustNone(sweep) => put_sweep(out, 0x26, *sweep),
        AnimCommand::PanSeAdjustAll(sweep) => put_sweep(out, 0x27, *sweep),
        AnimCommand::SplitBgPrio(battler) => out.extend_from_slice(&[0x28, *battler]),
        AnimCommand::SplitBgPrioAll => out.push(0x29),
        AnimCommand::SplitBgPrioFoes(battler) => out.extend_from_slice(&[0x2A, *battler]),
        AnimCommand::Invisible(battler) => out.extend_from_slice(&[0x2B, *battler]),
        AnimCommand::Visible(battler) => out.extend_from_slice(&[0x2C, *battler]),
        AnimCommand::TeamAttackMoveBack(battler) => out.extend_from_slice(&[0x2D, *battler]),
        AnimCommand::TeamAttackMoveFwd(battler) => out.extend_from_slice(&[0x2E, *battler]),
        AnimCommand::StopSound => out.push(0x2F),
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], BattleAnimationError> {
        let (head, tail) = self
            .bytes
            .split_at_checked(len)
            .ok_or(BattleAnimationError::Truncated)?;
        self.bytes = tail;
        Ok(head)
    }

    fn u8(&mut self) -> Result<u8, BattleAnimationError> {
        Ok(self.take(1)?[0])
    }

    fn i8(&mut self) -> Result<i8, BattleAnimationError> {
        Ok(i8::from_le_bytes([self.u8()?]))
    }

    fn u16(&mut self) -> Result<u16, BattleAnimationError> {
        let bytes = self.take(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn i16(&mut self) -> Result<i16, BattleAnimationError> {
        let bytes = self.take(2)?;
        Ok(i16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn symbol(&mut self) -> Result<AnimSymbol, BattleAnimationError> {
        let len = usize::from(self.u16()?);
        let text = std::str::from_utf8(self.take(len)?)
            .map_err(|_| BattleAnimationError::InvalidSymbol)?;
        AnimSymbol::new(text)
    }

    fn sprite(&mut self) -> Result<EffectSpriteId, BattleAnimationError> {
        EffectSpriteId::new(self.u16()?)
    }

    fn program(&mut self) -> Result<ProgramId, BattleAnimationError> {
        Ok(ProgramId::new(self.u16()?))
    }

    fn args(&mut self) -> Result<Vec<i16>, BattleAnimationError> {
        let count = usize::from(self.u8()?);
        (0..count).map(|_| self.i16()).collect()
    }

    fn sweep(&mut self) -> Result<PanSweep, BattleAnimationError> {
        Ok(PanSweep {
            se: self.u16()?,
            current_pan: self.i8()?,
            target_pan: self.i8()?,
            increment_pan: self.i8()?,
            delay: self.u8()?,
        })
    }

    fn command(&mut self) -> Result<AnimCommand, BattleAnimationError> {
        let opcode = self.u8()?;
        Ok(match opcode {
            0x00 => AnimCommand::LoadSpriteGfx(self.sprite()?),
            0x01 => AnimCommand::UnloadSpriteGfx(self.sprite()?),
            0x02 => {
                let template = self.symbol()?;
                let relative_to_target = self.flag()?;
                AnimCommand::CreateSprite {
                    template,
                    relative_to_target,
                    subpriority: self.u8()?,
                    args: self.args()?,
                }
            }
            0x03 => AnimCommand::CreateVisualTask {
                task: self.symbol()?,
                priority: self.u8()?,
                args: self.args()?,
            },
            0x04 => AnimCommand::Delay(self.u8()?),
            0x05 => AnimCommand::WaitForVisualFinish,
            0x06 => AnimCommand::Nop,
            0x07 => AnimCommand::Nop2,
            0x08 => AnimCommand::End,
            0x09 => AnimCommand::PlaySe(self.u16()?),
            0x0A => AnimCommand::MonBg(self.u8()?),
            0x0B => AnimCommand::ClearMonBg(self.u8()?),
            0x0C => AnimCommand::SetAlpha {
                eva: self.u8()?,
                evb: self.u8()?,
            },
            0x0D => AnimCommand::BlendOff,
            0x0E => AnimCommand::Call(self.program()?),
            0x0F => AnimCommand::Return,
            0x10 => AnimCommand::SetArg {
                arg: self.u8()?,
                value: self.i16()?,
            },
            0x11 => AnimCommand::ChooseTwoTurnAnim {
                even: self.program()?,
                odd: self.program()?,
            },
            0x12 => AnimCommand::JumpIfMoveTurn {
                turn: self.u8()?,
                target: self.program()?,
            },
            0x13 => AnimCommand::Goto(self.program()?),
            0x14 => AnimCommand::FadeToBg(self.u8()?),
            0x15 => AnimCommand::RestoreBg,
            0x16 => AnimCommand::WaitBgFadeOut,
            0x17 => AnimCommand::WaitBgFadeIn,
            0x18 => AnimCommand::ChangeBg(self.u8()?),
            0x19 => AnimCommand::PlaySeWithPan {
                se: self.u16()?,
                pan: self.i8()?,
            },
            0x1A => AnimCommand::SetPan(self.i8()?),
            0x1B => AnimCommand::PanSe(self.sweep()?),
            0x1C => AnimCommand::LoopSeWithPan {
                se: self.u16()?,
                pan: self.i8()?,
                wait: self.u8()?,
                times: self.u8()?,
            },
            0x1D => AnimCommand::WaitPlaySeWithPan {
                se: self.u16()?,
                pan: self.i8()?,
                wait: self.u8()?,
            },
            0x1E => AnimCommand::SetBldCnt(self.u16()?),
            0x1F => AnimCommand::CreateSoundTask {
                task: self.symbol()?,
                args: self.args()?,
            },
            0x20 => AnimCommand::WaitSound,
            0x21 => AnimCommand::JumpArgEq {
                arg: self.u8()?,
                value: self.i16()?,
                target: self.program()?,
            },
            0x22 => AnimCommand::MonBgStatic(self.u8()?),
            0x23 => AnimCommand::ClearMonBgStatic(self.u8()?),
            0x24 => AnimCommand::JumpIfContest(self.program()?),
            0x25 => AnimCommand::FadeToBgFromSet {
                opponent: self.u8()?,
                player: self.u8()?,
                contest: self.u8()?,
            },
            0x26 => AnimCommand::PanSeAdjustNone(self.sweep()?),
            0x27 => AnimCommand::PanSeAdjustAll(self.sweep()?),
            0x28 => AnimCommand::SplitBgPrio(self.u8()?),
            0x29 => AnimCommand::SplitBgPrioAll,
            0x2A => AnimCommand::SplitBgPrioFoes(self.u8()?),
            0x2B => AnimCommand::Invisible(self.u8()?),
            0x2C => AnimCommand::Visible(self.u8()?),
            0x2D => AnimCommand::TeamAttackMoveBack(self.u8()?),
            0x2E => AnimCommand::TeamAttackMoveFwd(self.u8()?),
            0x2F => AnimCommand::StopSound,
            other => return Err(BattleAnimationError::UnknownOpcode(other)),
        })
    }

    /// The `relative_to_target` byte is a strict 0 or 1: the canonical encoding
    /// has no other spelling of it.
    fn flag(&mut self) -> Result<bool, BattleAnimationError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            other => Err(BattleAnimationError::InvalidFlag(other)),
        }
    }
}

pub(super) fn decode(bytes: &[u8]) -> Result<BattleAnimationCatalog, BattleAnimationError> {
    let mut reader = Reader { bytes };
    let version = reader.u16()?;
    if version != SCHEMA_VERSION {
        return Err(BattleAnimationError::UnsupportedVersion(version));
    }
    let program_count = usize::from(reader.u16()?);
    let mut programs = Vec::with_capacity(program_count.min(reader.bytes.len()));
    for _ in 0..program_count {
        let symbol = reader.symbol()?;
        let command_count = usize::from(reader.u16()?);
        let mut commands = Vec::with_capacity(command_count.min(reader.bytes.len()));
        for _ in 0..command_count {
            commands.push(reader.command()?);
        }
        programs.push(BattleAnimationProgram::new(symbol, commands)?);
    }
    let mut selectors = Vec::new();
    for table in AnimationTable::ALL {
        for index in 0..table.len() {
            selectors.push((AnimationSelector::new(table, index)?, reader.program()?));
        }
    }
    if !reader.bytes.is_empty() {
        return Err(BattleAnimationError::TrailingBytes(reader.bytes.len()));
    }
    BattleAnimationCatalog::new(programs, selectors)
}
