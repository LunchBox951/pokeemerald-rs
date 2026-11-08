//! Operand types and the command set of one animation script.

use super::BattleAnimationError;

/// The length of `gBattleAnimArgs` (`include/battle_anim.h:36`), which
/// `createsprite`, `createvisualtask`, and `createsoundtask` fill from their
/// argument lists.
pub const ANIM_ARGS_COUNT: usize = 8;

/// How many effect-sprite resources upstream names: `ANIM_TAG_BONE` through
/// `ANIM_TAG_BLUE_RING_2` (`include/constants/battle_anim.h:10-298`).
pub const EFFECT_SPRITE_COUNT: u16 = 289;

/// The largest `createsprite` subpriority byte: the macro stores
/// `subpriority_offset & 0x7F` and keeps `ANIMSPRITE_IS_TARGET` (bit 7) apart
/// (`asm/macros/battle_anim_script.inc:13-25`).
pub const SUBPRIORITY_MASK: u8 = 0x7F;

/// One effect sprite sheet and palette: the `ANIM_TAG_*` index into
/// `gBattleAnimPicTable` and `gBattleAnimPaletteTable`
/// (`GET_TRUE_SPRITE_INDEX`, `include/constants/battle_anim.h:5`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EffectSpriteId(u16);

impl EffectSpriteId {
    /// Wraps the `ANIM_TAG_*` index, i.e. the tag minus `ANIM_SPRITES_START` (10000).
    ///
    /// # Errors
    ///
    /// [`BattleAnimationError::SpriteIdOutOfRange`] at or beyond [`EFFECT_SPRITE_COUNT`].
    pub const fn new(index: u16) -> Result<Self, BattleAnimationError> {
        if index >= EFFECT_SPRITE_COUNT {
            return Err(BattleAnimationError::SpriteIdOutOfRange(index));
        }
        Ok(Self(index))
    }

    /// The index into the effect-sprite tables.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// A program's catalog position, the target of `call`, `goto`, and the
/// conditional jumps. Resolve it with
/// [`BattleAnimationCatalog::program_by_id`](super::BattleAnimationCatalog::program_by_id).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProgramId(u16);

impl ProgramId {
    /// Wraps a catalog position; the catalog checks it against its length.
    #[must_use]
    pub const fn new(index: u16) -> Self {
        Self(index)
    }

    /// The catalog position.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// An upstream C identifier: a script label (`Move_POUND`), a sprite template
/// (`gSimplePaletteBlendSpriteTemplate`), or a task function
/// (`AnimTask_HorizontalShake`). Starts with an ASCII letter or `_`, continues
/// with letters, digits, and `_`, and fits a `u16` length prefix.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AnimSymbol(String);

impl AnimSymbol {
    /// Wraps `name`.
    ///
    /// # Errors
    ///
    /// [`BattleAnimationError::InvalidSymbol`] for an empty, overlong, or
    /// non-identifier name.
    pub fn new(name: &str) -> Result<Self, BattleAnimationError> {
        let mut bytes = name.bytes();
        let starts_well = bytes
            .next()
            .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_');
        let identifier = bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
        if !starts_well || u16::try_from(name.len()).is_err() || !identifier {
            return Err(BattleAnimationError::InvalidSymbol);
        }
        Ok(Self(name.to_owned()))
    }

    /// The identifier text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The operands `panse`, `panse_adjustnone`, and `panse_adjustall` share
/// (`asm/macros/battle_anim_script.inc:147,218,227`; `Cmd_panse`,
/// `src/battle_anim.c:1377-1406`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanSweep {
    /// The sound effect (`SE_*`).
    pub se: u16,
    /// The starting pan, a `SOUND_PAN_*` (-64 attacker side, 63 target side).
    pub current_pan: i8,
    /// The ending pan.
    pub target_pan: i8,
    /// The per-step pan change.
    pub increment_pan: i8,
    /// Frames between steps.
    pub delay: u8,
}

/// One script command. The hex number on each variant is its
/// `sScriptCmdTable` slot (`src/battle_anim.c:118-171`) and wire opcode.
/// Operands keep the widths and signedness of
/// `asm/macros/battle_anim_script.inc`.
///
/// Battler operands are raw `ANIM_ATTACKER`..`ANIM_DEF_PARTNER` selectors
/// (`include/constants/battle_anim.h:301-304`) and background operands raw
/// `BG_*` ids; neither is interpreted here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnimCommand {
    /// 0x00 `loadspritegfx`: load a sprite sheet and palette.
    LoadSpriteGfx(EffectSpriteId),
    /// 0x01 `unloadspritegfx`: free a sprite sheet and palette.
    UnloadSpriteGfx(EffectSpriteId),
    /// 0x02 `createsprite`: spawn `template` with `args` in `gBattleAnimArgs`.
    CreateSprite {
        /// The `SpriteTemplate` symbol.
        template: AnimSymbol,
        /// `ANIMSPRITE_IS_TARGET`: the subpriority is relative to the target
        /// battler, not the attacker (`src/battle_anim.c:383-399`).
        relative_to_target: bool,
        /// The biased subpriority offset, at most [`SUBPRIORITY_MASK`]: 64 and
        /// above add `value - 64`, below 64 subtract `value`.
        subpriority: u8,
        /// The template arguments, at most [`ANIM_ARGS_COUNT`].
        args: Vec<i16>,
    },
    /// 0x03 `createvisualtask`: run a task function with `args`.
    CreateVisualTask {
        /// The task function symbol.
        task: AnimSymbol,
        /// The task priority.
        priority: u8,
        /// The task arguments, at most [`ANIM_ARGS_COUNT`].
        args: Vec<i16>,
    },
    /// 0x04 `delay`: wait this many frames (0 waits one frame, `Cmd_delay`).
    Delay(u8),
    /// 0x05 `waitforvisualfinish`.
    WaitForVisualFinish,
    /// 0x06 `nop`.
    Nop,
    /// 0x07 `nop2`.
    Nop2,
    /// 0x08 `end`.
    End,
    /// 0x09 `playse`.
    PlaySe(u16),
    /// 0x0A `monbg`.
    MonBg(u8),
    /// 0x0B `clearmonbg`.
    ClearMonBg(u8),
    /// 0x0C `setalpha`: the `BLDALPHA` coefficients.
    SetAlpha {
        /// `eva`, the first-target weight.
        eva: u8,
        /// `evb`, the second-target weight.
        evb: u8,
    },
    /// 0x0D `blendoff`.
    BlendOff,
    /// 0x0E `call`: run another program, then resume after this command.
    Call(ProgramId),
    /// 0x0F `return`.
    Return,
    /// 0x10 `setarg`: write `gBattleAnimArgs[arg]`.
    SetArg {
        /// The raw argument index, kept even past [`ANIM_ARGS_COUNT`]
        /// (upstream does not bounds-check it).
        arg: u8,
        /// The value written.
        value: i16,
    },
    /// 0x11 `choosetwoturnanim`: jump to `even` on even `gAnimMoveTurn`, else `odd`
    /// (`Cmd_choosetwoturnanim`, `src/battle_anim.c:1073-1079`).
    ChooseTwoTurnAnim {
        /// The program for an even turn.
        even: ProgramId,
        /// The program for an odd turn.
        odd: ProgramId,
    },
    /// 0x12 `jumpifmoveturn`.
    JumpIfMoveTurn {
        /// The turn compared with `gAnimMoveTurn`.
        turn: u8,
        /// The program jumped to on a match.
        target: ProgramId,
    },
    /// 0x13 `goto`.
    Goto(ProgramId),
    /// 0x14 `fadetobg`.
    FadeToBg(u8),
    /// 0x15 `restorebg`.
    RestoreBg,
    /// 0x16 `waitbgfadeout`.
    WaitBgFadeOut,
    /// 0x17 `waitbgfadein`.
    WaitBgFadeIn,
    /// 0x18 `changebg`.
    ChangeBg(u8),
    /// 0x19 `playsewithpan`.
    PlaySeWithPan {
        /// The sound effect.
        se: u16,
        /// The pan, a `SOUND_PAN_*`.
        pan: i8,
    },
    /// 0x1A `setpan`.
    SetPan(i8),
    /// 0x1B `panse`.
    PanSe(PanSweep),
    /// 0x1C `loopsewithpan`.
    LoopSeWithPan {
        /// The sound effect.
        se: u16,
        /// The pan.
        pan: i8,
        /// Frames between plays.
        wait: u8,
        /// How many plays.
        times: u8,
    },
    /// 0x1D `waitplaysewithpan`.
    WaitPlaySeWithPan {
        /// The sound effect.
        se: u16,
        /// The pan.
        pan: i8,
        /// Frames to wait before playing.
        wait: u8,
    },
    /// 0x1E `setbldcnt`: the raw `BLDCNT` value.
    SetBldCnt(u16),
    /// 0x1F `createsoundtask`: run a sound task function with `args`.
    CreateSoundTask {
        /// The task function symbol.
        task: AnimSymbol,
        /// The task arguments, at most [`ANIM_ARGS_COUNT`].
        args: Vec<i16>,
    },
    /// 0x20 `waitsound`.
    WaitSound,
    /// 0x21 `jumpargeq`: jump when `gBattleAnimArgs[arg] == value`.
    JumpArgEq {
        /// The raw argument index, kept even past [`ANIM_ARGS_COUNT`]; stock
        /// `Move_MAGNITUDE` jumps on index 15
        /// (`data/battle_anim_scripts.s:3324-3326`).
        arg: u8,
        /// The compared value.
        value: i16,
        /// The program jumped to on a match.
        target: ProgramId,
    },
    /// 0x22 `monbg_static`.
    MonBgStatic(u8),
    /// 0x23 `clearmonbg_static`.
    ClearMonBgStatic(u8),
    /// 0x24 `jumpifcontest`.
    JumpIfContest(ProgramId),
    /// 0x25 `fadetobgfromset`: one background per scene kind.
    FadeToBgFromSet {
        /// The background for an opponent-targeted move.
        opponent: u8,
        /// The background for a player-targeted move.
        player: u8,
        /// The background in a contest.
        contest: u8,
    },
    /// 0x26 `panse_adjustnone`.
    PanSeAdjustNone(PanSweep),
    /// 0x27 `panse_adjustall`.
    PanSeAdjustAll(PanSweep),
    /// 0x28 `splitbgprio`.
    SplitBgPrio(u8),
    /// 0x29 `splitbgprio_all`.
    SplitBgPrioAll,
    /// 0x2A `splitbgprio_foes`.
    SplitBgPrioFoes(u8),
    /// 0x2B `invisible`.
    Invisible(u8),
    /// 0x2C `visible`.
    Visible(u8),
    /// 0x2D `teamattack_moveback` (unused upstream).
    TeamAttackMoveBack(u8),
    /// 0x2E `teamattack_movefwd` (unused upstream).
    TeamAttackMoveFwd(u8),
    /// 0x2F `stopsound`.
    StopSound,
}

impl AnimCommand {
    /// Whether execution never continues to the next command.
    pub(super) const fn is_terminator(&self) -> bool {
        matches!(
            self,
            Self::End | Self::Return | Self::Goto(_) | Self::ChooseTwoTurnAnim { .. }
        )
    }

    /// The programs this command can transfer control to.
    pub(super) fn targets(&self) -> impl Iterator<Item = ProgramId> {
        let (first, second) = match self {
            Self::Call(target)
            | Self::Goto(target)
            | Self::JumpIfContest(target)
            | Self::JumpIfMoveTurn { target, .. }
            | Self::JumpArgEq { target, .. } => (Some(*target), None),
            Self::ChooseTwoTurnAnim { even, odd } => (Some(*even), Some(*odd)),
            _ => (None, None),
        };
        first.into_iter().chain(second)
    }

    /// Rejects a `createsprite` subpriority above [`SUBPRIORITY_MASK`] or an
    /// argument list longer than [`ANIM_ARGS_COUNT`].
    pub(super) fn check_operands(&self) -> Result<(), BattleAnimationError> {
        match self {
            Self::CreateSprite {
                subpriority, args, ..
            } => {
                if *subpriority > SUBPRIORITY_MASK {
                    return Err(BattleAnimationError::SubpriorityOutOfRange(*subpriority));
                }
                check_args(args)
            }
            Self::CreateVisualTask { args, .. } | Self::CreateSoundTask { args, .. } => {
                check_args(args)
            }
            _ => Ok(()),
        }
    }
}

fn check_args(args: &[i16]) -> Result<(), BattleAnimationError> {
    if args.len() > ANIM_ARGS_COUNT {
        return Err(BattleAnimationError::ArgCountTooLarge(args.len()));
    }
    Ok(())
}
