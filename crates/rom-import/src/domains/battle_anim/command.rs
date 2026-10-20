//! One program's commands, read from `sScriptCmdTable`'s byte layout.
//!
//! Operand widths follow the macros of `asm/macros/battle_anim_script.inc`:
//! each command is its opcode byte then its operands in macro order, little
//! endian, with pointers four bytes.

use assets::{AnimCommand, AnimSymbol, EffectSpriteId, PanSweep, ProgramId};

use super::PACK_ID;
use crate::error::ImportError;
use crate::reader::{GbaPtr, RomReader};

/// `ANIM_SPRITES_START` (`include/constants/battle_anim.h`): the first
/// `ANIM_TAG_*` value, which `loadspritegfx` stores whole.
const ANIM_SPRITES_START: u16 = 10_000;
/// `ANIMSPRITE_IS_TARGET`: `createsprite`'s subpriority byte keeps the
/// target flag in bit 7 (`asm/macros/battle_anim_script.inc`).
const IS_TARGET: u8 = 0x80;

/// How a command operand's address becomes the identity the catalog holds.
pub trait AnimPointers {
    /// The sprite template a `createsprite` at ROM offset `at` names.
    ///
    /// # Errors
    ///
    /// [`ImportError::AnimationPointer`] if the address is not recorded.
    fn template(&mut self, at: usize, ptr: GbaPtr) -> Result<AnimSymbol, ImportError>;

    /// The task function a `createvisualtask` or `createsoundtask` names.
    ///
    /// # Errors
    ///
    /// [`ImportError::AnimationPointer`] if the address is not recorded.
    fn task(&mut self, at: usize, ptr: GbaPtr) -> Result<AnimSymbol, ImportError>;

    /// The program a `call`, `goto`, or conditional jump names.
    ///
    /// # Errors
    ///
    /// [`ImportError::AnimationPointer`] if the address is not recorded.
    fn program(&mut self, at: usize, ptr: GbaPtr) -> Result<ProgramId, ImportError>;
}

/// The commands of one program and the bytes they took.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedProgram {
    /// The commands, through the first terminator.
    pub commands: Vec<AnimCommand>,
    /// The bytes from the first command through the terminator.
    pub len: usize,
}

/// Decode the program that starts at `start`, through its first terminator.
///
/// `limit` bounds the program's bytes, so a stream with no terminator ends
/// here instead of at the end of the image. Every read is held to the bound
/// before it happens: a command at or straddling the limit belongs to the
/// next label, so none of its bytes are read and none of its operands are
/// resolved.
///
/// # Errors
///
/// [`ImportError::StructMismatch`] if the program outgrows `limit`;
/// [`ImportError::Truncated`] if the image ends mid-command within it;
/// [`ImportError::AnimationOpcode`] for a byte that is no command;
/// [`ImportError::PointerOutOfRange`] for an operand outside the cartridge
/// window; whatever `pointers` refuses.
pub fn decode_program(
    reader: &RomReader<'_>,
    start: GbaPtr,
    limit: usize,
    pointers: &mut impl AnimPointers,
) -> Result<DecodedProgram, ImportError> {
    let mut cursor = Cursor {
        reader,
        at: start.offset(),
        end: start.offset().saturating_add(limit),
    };
    let mut commands = Vec::new();
    loop {
        let command = cursor.command(pointers)?;
        let used = cursor.at.saturating_sub(start.offset());
        let last = terminates(&command);
        commands.push(command);
        if last {
            return Ok(DecodedProgram {
                commands,
                len: used,
            });
        }
    }
}

/// Whether `command` never continues to the next one.
fn terminates(command: &AnimCommand) -> bool {
    matches!(
        command,
        AnimCommand::End
            | AnimCommand::Return
            | AnimCommand::Goto(_)
            | AnimCommand::ChooseTwoTurnAnim { .. }
    )
}

struct Cursor<'r, 'a> {
    reader: &'r RomReader<'a>,
    at: usize,
    /// The offset one past the program's last byte.
    end: usize,
}

impl Cursor<'_, '_> {
    /// Refuse a read of `len` bytes that would pass the program's end.
    fn within(&self, len: usize) -> Result<(), ImportError> {
        if self.end.saturating_sub(self.at) < len {
            return Err(ImportError::StructMismatch {
                root: PACK_ID,
                field: "program length",
            });
        }
        Ok(())
    }

    fn bytes<const N: usize>(&mut self) -> Result<[u8; N], ImportError> {
        self.within(N)?;
        let found = self.reader.slice_at(self.at, N)?;
        self.at += N;
        Ok(found.try_into().expect("slice_at returns N bytes"))
    }

    fn u8(&mut self) -> Result<u8, ImportError> {
        self.bytes::<1>().map(|[byte]| byte)
    }

    fn i8(&mut self) -> Result<i8, ImportError> {
        self.u8().map(u8::cast_signed)
    }

    fn u16(&mut self) -> Result<u16, ImportError> {
        self.bytes().map(u16::from_le_bytes)
    }

    fn i16(&mut self) -> Result<i16, ImportError> {
        self.u16().map(u16::cast_signed)
    }

    /// A pointer operand, with the offset it was read at.
    fn ptr(&mut self) -> Result<(usize, GbaPtr), ImportError> {
        self.within(4)?;
        let at = self.at;
        let ptr = self.reader.ptr(at)?;
        self.at += 4;
        Ok((at, ptr))
    }

    fn program(&mut self, pointers: &mut impl AnimPointers) -> Result<ProgramId, ImportError> {
        let (at, ptr) = self.ptr()?;
        pointers.program(at, ptr)
    }

    /// A counted list of `.2byte` arguments.
    fn args(&mut self) -> Result<Vec<i16>, ImportError> {
        let count = self.u8()?;
        (0..count).map(|_| self.i16()).collect()
    }

    fn sweep(&mut self) -> Result<PanSweep, ImportError> {
        Ok(PanSweep {
            se: self.u16()?,
            current_pan: self.i8()?,
            target_pan: self.i8()?,
            increment_pan: self.i8()?,
            delay: self.u8()?,
        })
    }

    fn sprite(&mut self) -> Result<EffectSpriteId, ImportError> {
        let tag = self.u16()?;
        tag.checked_sub(ANIM_SPRITES_START)
            .map_or(
                Err(assets::BattleAnimationError::SpriteIdOutOfRange(tag)),
                EffectSpriteId::new,
            )
            .map_err(|source| ImportError::BattleAnimation {
                id: PACK_ID,
                source,
            })
    }

    fn command(&mut self, pointers: &mut impl AnimPointers) -> Result<AnimCommand, ImportError> {
        use AnimCommand as C;
        let opcode_at = self.at;
        let opcode = self.u8()?;
        Ok(match opcode {
            0x00 => C::LoadSpriteGfx(self.sprite()?),
            0x01 => C::UnloadSpriteGfx(self.sprite()?),
            0x02 => {
                let (at, ptr) = self.ptr()?;
                let template = pointers.template(at, ptr)?;
                let flags = self.u8()?;
                C::CreateSprite {
                    template,
                    relative_to_target: flags & IS_TARGET != 0,
                    subpriority: flags & !IS_TARGET,
                    args: self.args()?,
                }
            }
            0x03 => {
                let (at, ptr) = self.ptr()?;
                let task = pointers.task(at, ptr)?;
                C::CreateVisualTask {
                    task,
                    priority: self.u8()?,
                    args: self.args()?,
                }
            }
            0x04 => C::Delay(self.u8()?),
            0x05 => C::WaitForVisualFinish,
            0x06 => C::Nop,
            0x07 => C::Nop2,
            0x08 => C::End,
            0x09 => C::PlaySe(self.u16()?),
            0x0A => C::MonBg(self.u8()?),
            0x0B => C::ClearMonBg(self.u8()?),
            0x0C => C::SetAlpha {
                eva: self.u8()?,
                evb: self.u8()?,
            },
            0x0D => C::BlendOff,
            0x0E => C::Call(self.program(pointers)?),
            0x0F => C::Return,
            0x10 => C::SetArg {
                arg: self.u8()?,
                value: self.i16()?,
            },
            0x11 => C::ChooseTwoTurnAnim {
                even: self.program(pointers)?,
                odd: self.program(pointers)?,
            },
            0x12 => C::JumpIfMoveTurn {
                turn: self.u8()?,
                target: self.program(pointers)?,
            },
            0x13 => C::Goto(self.program(pointers)?),
            0x14 => C::FadeToBg(self.u8()?),
            0x15 => C::RestoreBg,
            0x16 => C::WaitBgFadeOut,
            0x17 => C::WaitBgFadeIn,
            0x18 => C::ChangeBg(self.u8()?),
            0x19 => C::PlaySeWithPan {
                se: self.u16()?,
                pan: self.i8()?,
            },
            0x1A => C::SetPan(self.i8()?),
            0x1B => C::PanSe(self.sweep()?),
            0x1C => C::LoopSeWithPan {
                se: self.u16()?,
                pan: self.i8()?,
                wait: self.u8()?,
                times: self.u8()?,
            },
            0x1D => C::WaitPlaySeWithPan {
                se: self.u16()?,
                pan: self.i8()?,
                wait: self.u8()?,
            },
            0x1E => C::SetBldCnt(self.u16()?),
            0x1F => {
                let (at, ptr) = self.ptr()?;
                let task = pointers.task(at, ptr)?;
                C::CreateSoundTask {
                    task,
                    args: self.args()?,
                }
            }
            _ => return self.late_command(opcode_at, opcode, pointers),
        })
    }

    /// The commands from `waitsound` on.
    fn late_command(
        &mut self,
        opcode_at: usize,
        opcode: u8,
        pointers: &mut impl AnimPointers,
    ) -> Result<AnimCommand, ImportError> {
        use AnimCommand as C;
        Ok(match opcode {
            0x20 => C::WaitSound,
            0x21 => C::JumpArgEq {
                arg: self.u8()?,
                value: self.i16()?,
                target: self.program(pointers)?,
            },
            0x22 => C::MonBgStatic(self.u8()?),
            0x23 => C::ClearMonBgStatic(self.u8()?),
            0x24 => C::JumpIfContest(self.program(pointers)?),
            0x25 => C::FadeToBgFromSet {
                opponent: self.u8()?,
                player: self.u8()?,
                contest: self.u8()?,
            },
            0x26 => C::PanSeAdjustNone(self.sweep()?),
            0x27 => C::PanSeAdjustAll(self.sweep()?),
            0x28 => C::SplitBgPrio(self.u8()?),
            0x29 => C::SplitBgPrioAll,
            0x2A => C::SplitBgPrioFoes(self.u8()?),
            0x2B => C::Invisible(self.u8()?),
            0x2C => C::Visible(self.u8()?),
            0x2D => C::TeamAttackMoveBack(self.u8()?),
            0x2E => C::TeamAttackMoveFwd(self.u8()?),
            0x2F => C::StopSound,
            opcode => {
                return Err(ImportError::AnimationOpcode {
                    at: opcode_at,
                    opcode,
                })
            }
        })
    }
}
