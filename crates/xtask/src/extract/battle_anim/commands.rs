//! The 48 primitive script commands: operand text to a typed [`AnimCommand`].
//!
//! Operand widths follow the assembler macros in
//! `asm/macros/battle_anim_script.inc`: `.byte` and `.2byte` accept both the
//! signed and the unsigned spelling of their width and keep the bits.

use assets::{AnimCommand, AnimSymbol, EffectSpriteId, PanSweep, ProgramId};

use super::expr::{Constants, Style};

/// The primitive command names, one per [`AnimCommand`] variant.
pub(super) const PRIMITIVES: [&str; 48] = [
    "loadspritegfx",
    "unloadspritegfx",
    "createsprite",
    "createvisualtask",
    "delay",
    "waitforvisualfinish",
    "nop",
    "nop2",
    "end",
    "playse",
    "monbg",
    "clearmonbg",
    "setalpha",
    "blendoff",
    "call",
    "return",
    "setarg",
    "choosetwoturnanim",
    "jumpifmoveturn",
    "goto",
    "fadetobg",
    "restorebg",
    "waitbgfadeout",
    "waitbgfadein",
    "changebg",
    "playsewithpan",
    "setpan",
    "panse",
    "loopsewithpan",
    "waitplaysewithpan",
    "setbldcnt",
    "createsoundtask",
    "waitsound",
    "jumpargeq",
    "monbg_static",
    "clearmonbg_static",
    "jumpifcontest",
    "fadetobgfromset",
    "panse_adjustnone",
    "panse_adjustall",
    "splitbgprio",
    "splitbgprio_all",
    "splitbgprio_foes",
    "invisible",
    "visible",
    "teamattack_moveback",
    "teamattack_movefwd",
    "stopsound",
];

/// A command whose control-flow operands still name labels.
#[derive(Debug, Clone)]
pub(super) struct Pending {
    command: AnimCommand,
    /// The labels the command transfers to, in operand order.
    pub(super) targets: Vec<String>,
}

impl Pending {
    fn plain(command: AnimCommand) -> Self {
        Self {
            command,
            targets: Vec::new(),
        }
    }

    fn jump(command: AnimCommand, targets: &[&str]) -> Self {
        Self {
            command,
            targets: targets.iter().map(|t| (*t).to_owned()).collect(),
        }
    }

    /// Whether execution never continues to the next command.
    pub(super) const fn terminates(&self) -> bool {
        matches!(
            self.command,
            AnimCommand::End
                | AnimCommand::Return
                | AnimCommand::Goto(_)
                | AnimCommand::ChooseTwoTurnAnim { .. }
        )
    }

    /// The command with its targets replaced by `ids`, in operand order.
    pub(super) fn link(&self, ids: &[ProgramId]) -> AnimCommand {
        match (&self.command, ids) {
            (AnimCommand::Call(_), [id]) => AnimCommand::Call(*id),
            (AnimCommand::Goto(_), [id]) => AnimCommand::Goto(*id),
            (AnimCommand::JumpIfContest(_), [id]) => AnimCommand::JumpIfContest(*id),
            (AnimCommand::ChooseTwoTurnAnim { .. }, [even, odd]) => {
                AnimCommand::ChooseTwoTurnAnim {
                    even: *even,
                    odd: *odd,
                }
            }
            (AnimCommand::JumpIfMoveTurn { turn, .. }, [id]) => AnimCommand::JumpIfMoveTurn {
                turn: *turn,
                target: *id,
            },
            (AnimCommand::JumpArgEq { arg, value, .. }, [id]) => AnimCommand::JumpArgEq {
                arg: *arg,
                value: *value,
                target: *id,
            },
            (command, _) => command.clone(),
        }
    }
}

/// Converts the primitive `name` with operand text `args`, or `None` when
/// `name` is not a primitive.
pub(super) fn convert(
    name: &str,
    args: &[String],
    constants: &Constants,
) -> Option<Result<Pending, String>> {
    PRIMITIVES.contains(&name).then(|| {
        Operands {
            name,
            args,
            constants,
        }
        .build()
    })
}

struct Operands<'a> {
    name: &'a str,
    args: &'a [String],
    constants: &'a Constants,
}

impl Operands<'_> {
    fn build(&self) -> Result<Pending, String> {
        use AnimCommand as C;
        let plain = |command| Ok(Pending::plain(command));
        match self.name {
            "loadspritegfx" => plain(C::LoadSpriteGfx(self.sprite()?)),
            "unloadspritegfx" => plain(C::UnloadSpriteGfx(self.sprite()?)),
            "createsprite" => self.create_sprite(),
            "createvisualtask" => {
                self.at_least(2)?;
                plain(C::CreateVisualTask {
                    task: self.symbol(0)?,
                    priority: self.byte(1)?,
                    args: self.halves(2)?,
                })
            }
            "createsoundtask" => {
                self.at_least(1)?;
                plain(C::CreateSoundTask {
                    task: self.symbol(0)?,
                    args: self.halves(1)?,
                })
            }
            "delay" => plain(C::Delay(self.one_byte()?)),
            "waitforvisualfinish" => self.bare(C::WaitForVisualFinish),
            "nop" => self.bare(C::Nop),
            "nop2" => self.bare(C::Nop2),
            "end" => self.bare(C::End),
            "blendoff" => self.bare(C::BlendOff),
            "return" => self.bare(C::Return),
            "restorebg" => self.bare(C::RestoreBg),
            "waitbgfadeout" => self.bare(C::WaitBgFadeOut),
            "waitbgfadein" => self.bare(C::WaitBgFadeIn),
            "splitbgprio_all" => self.bare(C::SplitBgPrioAll),
            "waitsound" => self.bare(C::WaitSound),
            "stopsound" => self.bare(C::StopSound),
            _ => self.build_audio(),
        }
    }

    fn build_audio(&self) -> Result<Pending, String> {
        use AnimCommand as C;
        let plain = |command| Ok(Pending::plain(command));
        match self.name {
            "playse" => {
                self.exactly(1)?;
                plain(C::PlaySe(self.unsigned_half(0)?))
            }
            "monbg" => plain(C::MonBg(self.one_byte()?)),
            "clearmonbg" => plain(C::ClearMonBg(self.one_byte()?)),
            "monbg_static" => plain(C::MonBgStatic(self.one_byte()?)),
            "clearmonbg_static" => plain(C::ClearMonBgStatic(self.one_byte()?)),
            "fadetobg" => plain(C::FadeToBg(self.one_byte()?)),
            "changebg" => plain(C::ChangeBg(self.one_byte()?)),
            "splitbgprio" => plain(C::SplitBgPrio(self.one_byte()?)),
            "splitbgprio_foes" => plain(C::SplitBgPrioFoes(self.one_byte()?)),
            "invisible" => plain(C::Invisible(self.one_byte()?)),
            "visible" => plain(C::Visible(self.one_byte()?)),
            "teamattack_moveback" => plain(C::TeamAttackMoveBack(self.one_byte()?)),
            "teamattack_movefwd" => plain(C::TeamAttackMoveFwd(self.one_byte()?)),
            "setalpha" => {
                self.exactly(2)?;
                plain(C::SetAlpha {
                    eva: self.exact_byte(0)?,
                    evb: self.exact_byte(1)?,
                })
            }
            "setarg" => {
                self.exactly(2)?;
                plain(C::SetArg {
                    arg: self.byte(0)?,
                    value: self.half(1)?,
                })
            }
            "playsewithpan" => {
                self.exactly(2)?;
                plain(C::PlaySeWithPan {
                    se: self.unsigned_half(0)?,
                    pan: self.pan(1)?,
                })
            }
            "setpan" => {
                self.exactly(1)?;
                plain(C::SetPan(self.pan(0)?))
            }
            "panse" => plain(C::PanSe(self.sweep()?)),
            "panse_adjustnone" => plain(C::PanSeAdjustNone(self.sweep()?)),
            "panse_adjustall" => plain(C::PanSeAdjustAll(self.sweep()?)),
            "loopsewithpan" => {
                self.exactly(4)?;
                plain(C::LoopSeWithPan {
                    se: self.unsigned_half(0)?,
                    pan: self.pan(1)?,
                    wait: self.byte(2)?,
                    times: self.byte(3)?,
                })
            }
            "waitplaysewithpan" => {
                self.exactly(3)?;
                plain(C::WaitPlaySeWithPan {
                    se: self.unsigned_half(0)?,
                    pan: self.pan(1)?,
                    wait: self.byte(2)?,
                })
            }
            "setbldcnt" => {
                self.exactly(1)?;
                plain(C::SetBldCnt(self.unsigned_half(0)?))
            }
            "fadetobgfromset" => {
                self.exactly(3)?;
                plain(C::FadeToBgFromSet {
                    opponent: self.byte(0)?,
                    player: self.byte(1)?,
                    contest: self.byte(2)?,
                })
            }
            _ => self.build_flow(),
        }
    }

    fn build_flow(&self) -> Result<Pending, String> {
        use AnimCommand as C;
        match self.name {
            "call" => self.target(C::Call),
            "goto" => self.target(C::Goto),
            "jumpifcontest" => self.target(C::JumpIfContest),
            "choosetwoturnanim" => {
                self.exactly(2)?;
                let (even, odd) = (self.label(0)?, self.label(1)?);
                Ok(Pending::jump(
                    C::ChooseTwoTurnAnim {
                        even: ProgramId::new(0),
                        odd: ProgramId::new(0),
                    },
                    &[&even, &odd],
                ))
            }
            "jumpifmoveturn" => {
                self.exactly(2)?;
                let turn = self.byte(0)?;
                let label = self.label(1)?;
                Ok(Pending::jump(
                    C::JumpIfMoveTurn {
                        turn,
                        target: ProgramId::new(0),
                    },
                    &[&label],
                ))
            }
            "jumpargeq" => {
                self.exactly(3)?;
                let (arg, value) = (self.byte(0)?, self.half(1)?);
                let label = self.label(2)?;
                Ok(Pending::jump(
                    C::JumpArgEq {
                        arg,
                        value,
                        target: ProgramId::new(0),
                    },
                    &[&label],
                ))
            }
            other => Err(format!("`{other}` is not a primitive command")),
        }
    }

    fn create_sprite(&self) -> Result<Pending, String> {
        self.at_least(3)?;
        let target = self.constants.eval("ANIM_TARGET", Style::Gas)?;
        let battler = self.value(1)?;
        let subpriority = self.value(2)? & 0x7F;
        Ok(Pending::plain(AnimCommand::CreateSprite {
            template: self.symbol(0)?,
            relative_to_target: battler == target,
            subpriority: u8::try_from(subpriority).map_err(|_| "subpriority overflow")?,
            args: self.halves(3)?,
        }))
    }

    fn sweep(&self) -> Result<PanSweep, String> {
        self.exactly(5)?;
        Ok(PanSweep {
            se: self.unsigned_half(0)?,
            current_pan: self.pan(1)?,
            target_pan: self.pan(2)?,
            increment_pan: self.pan(3)?,
            delay: self.byte(4)?,
        })
    }

    fn sprite(&self) -> Result<EffectSpriteId, String> {
        self.exactly(1)?;
        let start = self.constants.eval("ANIM_SPRITES_START", Style::Gas)?;
        let index = self
            .value(0)?
            .checked_sub(start)
            .ok_or("sprite tag overflows")?;
        let index = u16::try_from(index).map_err(|_| format!("sprite tag index {index}"))?;
        EffectSpriteId::new(index).map_err(|e| e.to_string())
    }

    fn target(&self, build: fn(ProgramId) -> AnimCommand) -> Result<Pending, String> {
        self.exactly(1)?;
        let label = self.label(0)?;
        Ok(Pending::jump(build(ProgramId::new(0)), &[&label]))
    }

    fn bare(&self, command: AnimCommand) -> Result<Pending, String> {
        self.exactly(0)?;
        Ok(Pending::plain(command))
    }

    fn one_byte(&self) -> Result<u8, String> {
        self.exactly(1)?;
        self.byte(0)
    }

    fn exactly(&self, count: usize) -> Result<(), String> {
        if self.args.len() == count {
            return Ok(());
        }
        Err(format!(
            "`{}` takes {count} operands, found {}",
            self.name,
            self.args.len()
        ))
    }

    fn at_least(&self, count: usize) -> Result<(), String> {
        if self.args.len() >= count {
            return Ok(());
        }
        Err(format!(
            "`{}` takes at least {count} operands, found {}",
            self.name,
            self.args.len()
        ))
    }

    fn value(&self, index: usize) -> Result<i64, String> {
        self.constants.eval(&self.args[index], Style::Gas)
    }

    /// A `.byte` operand.
    fn byte(&self, index: usize) -> Result<u8, String> {
        let value = self.value(index)?;
        i8::try_from(value)
            .map(i8::cast_unsigned)
            .or_else(|_| u8::try_from(value))
            .map_err(|_| format!("{value} does not fit a byte"))
    }

    /// A byte the macro packs by arithmetic, so only the unsigned range is faithful.
    fn exact_byte(&self, index: usize) -> Result<u8, String> {
        let value = self.value(index)?;
        u8::try_from(value).map_err(|_| format!("{value} does not fit an unsigned byte"))
    }

    fn pan(&self, index: usize) -> Result<i8, String> {
        Ok(self.byte(index)?.cast_signed())
    }

    /// A `.2byte` operand read as signed.
    fn half(&self, index: usize) -> Result<i16, String> {
        Ok(self.unsigned_half(index)?.cast_signed())
    }

    /// A `.2byte` operand read as unsigned.
    fn unsigned_half(&self, index: usize) -> Result<u16, String> {
        let value = self.value(index)?;
        i16::try_from(value)
            .map(i16::cast_unsigned)
            .or_else(|_| u16::try_from(value))
            .map_err(|_| format!("{value} does not fit a halfword"))
    }

    fn halves(&self, from: usize) -> Result<Vec<i16>, String> {
        (from..self.args.len()).map(|i| self.half(i)).collect()
    }

    fn symbol(&self, index: usize) -> Result<AnimSymbol, String> {
        AnimSymbol::new(&self.args[index])
            .map_err(|_| format!("`{}` is not a symbol", self.args[index]))
    }

    fn label(&self, index: usize) -> Result<String, String> {
        self.symbol(index).map(|s| s.as_str().to_owned())
    }
}
