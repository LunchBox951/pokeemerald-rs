//! Backend-neutral battle-animation program catalog (S-4, issue #1856): the
//! script tables upstream launches through `LaunchBattleAnimation`
//! (`src/battle_anim.c:208-262`), in the one raw pack entry both extraction
//! backends fill.
//!
//! The four selector tables are `gBattleAnims_Moves`, `gBattleAnims_StatusConditions`,
//! `gBattleAnims_General`, and `gBattleAnims_Special`
//! (`data/battle_anim_scripts.s:18,377,389,415`). Each selector resolves to one
//! [`BattleAnimationProgram`], a script of [`AnimCommand`]s whose opcodes and
//! operand shapes are `sScriptCmdTable` (`src/battle_anim.c:118-171`) and the
//! macros of `asm/macros/battle_anim_script.inc`.
//!
//! [`BattleAnimationCatalog::encode`] and [`BattleAnimationCatalog::decode`]
//! are inverses over the wire format owned by [`codec`]; construction and
//! decoding enforce the same rules, so a value that exists always encodes.
//! The catalog is complete by construction: every selector of every table has
//! a program, and there is no fallback program for a selector the payload omits.

mod codec;
mod command;
mod error;

use std::collections::HashSet;
use std::fmt;

pub use command::{AnimCommand, AnimSymbol, EffectSpriteId, PanSweep, ProgramId};
pub use command::{ANIM_ARGS_COUNT, EFFECT_SPRITE_COUNT, SUBPRIORITY_MASK};
pub use error::BattleAnimationError;

/// The four upstream script tables a selector indexes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AnimationTable {
    /// `gBattleAnims_Moves` (`data/battle_anim_scripts.s:18`), indexed by move id
    /// (`DoMoveAnim`, `src/battle_anim.c:201-206`).
    Moves,
    /// `gBattleAnims_StatusConditions` (`data/battle_anim_scripts.s:377`), indexed by
    /// `B_ANIM_STATUS_*` (`include/constants/battle_anim.h:390-398`).
    StatusConditions,
    /// `gBattleAnims_General` (`data/battle_anim_scripts.s:389`), indexed by
    /// `B_ANIM_*` (`include/constants/battle_anim.h:356-379`).
    General,
    /// `gBattleAnims_Special` (`data/battle_anim_scripts.s:415`), indexed by
    /// `B_ANIM_*` (`include/constants/battle_anim.h:381-388`).
    Special,
}

impl AnimationTable {
    /// Every table, in wire order.
    pub const ALL: [Self; 4] = [
        Self::Moves,
        Self::StatusConditions,
        Self::General,
        Self::Special,
    ];

    /// How many selectors the table holds.
    ///
    /// The move table lists `MOVES_COUNT` (355, `include/constants/moves.h:360`)
    /// moves; upstream appends one more `Move_COUNT` entry
    /// (`data/battle_anim_scripts.s:374`) that its own comment says "cannot be
    /// reached", so it is not a selector. The status table lists the nine
    /// conditions before the nonexistent `B_ANIM_STATUS_WRAPPED`.
    #[must_use]
    pub const fn len(self) -> u16 {
        match self {
            Self::Moves => 355,
            Self::StatusConditions => 9,
            Self::General => 23,
            Self::Special => 7,
        }
    }

    /// Never true: every table has selectors.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        false
    }

    const fn slot(self) -> usize {
        match self {
            Self::Moves => 0,
            Self::StatusConditions => 1,
            Self::General => 2,
            Self::Special => 3,
        }
    }
}

impl fmt::Display for AnimationTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Moves => "moves",
            Self::StatusConditions => "status conditions",
            Self::General => "general",
            Self::Special => "special",
        })
    }
}

/// One slot of one animation table: the `tableId` of `LaunchBattleAnimation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AnimationSelector {
    table: AnimationTable,
    index: u16,
}

impl AnimationSelector {
    /// Names slot `index` of `table`.
    ///
    /// # Errors
    ///
    /// [`BattleAnimationError::SelectorOutOfRange`] when `index` is not below
    /// [`AnimationTable::len`].
    pub const fn new(table: AnimationTable, index: u16) -> Result<Self, BattleAnimationError> {
        if index >= table.len() {
            return Err(BattleAnimationError::SelectorOutOfRange { table, index });
        }
        Ok(Self { table, index })
    }

    /// The table the selector indexes.
    #[must_use]
    pub const fn table(self) -> AnimationTable {
        self.table
    }

    /// The slot within [`table`](Self::table).
    #[must_use]
    pub const fn index(self) -> u16 {
        self.index
    }
}

/// One upstream animation script: the commands from a label to the command that
/// leaves it.
///
/// The last command must be one that never falls through (`end`, `return`,
/// `goto`, or `choosetwoturnanim`), so a program cannot run off its own end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BattleAnimationProgram {
    symbol: AnimSymbol,
    commands: Vec<AnimCommand>,
}

impl BattleAnimationProgram {
    /// Bundles `commands` under the upstream label `symbol` (e.g. `Move_POUND`).
    ///
    /// # Errors
    ///
    /// [`BattleAnimationError::EmptyProgram`], [`BattleAnimationError::TooManyCommands`],
    /// or [`BattleAnimationError::MissingTerminator`] for the program's shape;
    /// [`BattleAnimationError::ArgCountTooLarge`],
    /// [`BattleAnimationError::ArgIndexOutOfRange`], or
    /// [`BattleAnimationError::SubpriorityOutOfRange`] for an operand that
    /// the interpreter would use out of range.
    pub fn new(
        symbol: AnimSymbol,
        commands: Vec<AnimCommand>,
    ) -> Result<Self, BattleAnimationError> {
        let Some(last) = commands.last() else {
            return Err(BattleAnimationError::EmptyProgram);
        };
        if u16::try_from(commands.len()).is_err() {
            return Err(BattleAnimationError::TooManyCommands(commands.len()));
        }
        if !last.is_terminator() {
            return Err(BattleAnimationError::MissingTerminator);
        }
        for command in &commands {
            command.check_operands()?;
        }
        Ok(Self { symbol, commands })
    }

    /// The upstream label the script starts at.
    #[must_use]
    pub fn symbol(&self) -> &AnimSymbol {
        &self.symbol
    }

    /// The commands in execution order.
    #[must_use]
    pub fn commands(&self) -> &[AnimCommand] {
        &self.commands
    }
}

/// Every animation program and selector table the pack's `battle-anim/catalog`
/// payload carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BattleAnimationCatalog {
    programs: Vec<BattleAnimationProgram>,
    tables: [Vec<ProgramId>; 4],
}

impl BattleAnimationCatalog {
    /// Bundles `programs` (their position is their [`ProgramId`]) with the
    /// program each selector runs.
    ///
    /// # Errors
    ///
    /// [`BattleAnimationError::TooManyPrograms`] beyond the `u16` id space;
    /// [`BattleAnimationError::DuplicateSymbol`] for a repeated label;
    /// [`BattleAnimationError::ProgramOutOfRange`] for a `call`/`goto`/jump
    /// target or selector that names no program;
    /// [`BattleAnimationError::DuplicateSelector`] or
    /// [`BattleAnimationError::MissingSelector`] when the selectors are not
    /// exactly every slot of every table once.
    pub fn new(
        programs: Vec<BattleAnimationProgram>,
        selectors: impl IntoIterator<Item = (AnimationSelector, ProgramId)>,
    ) -> Result<Self, BattleAnimationError> {
        if u16::try_from(programs.len()).is_err() {
            return Err(BattleAnimationError::TooManyPrograms(programs.len()));
        }
        let mut symbols = HashSet::new();
        for program in &programs {
            if !symbols.insert(program.symbol.as_str()) {
                return Err(BattleAnimationError::DuplicateSymbol(
                    program.symbol.as_str().to_owned(),
                ));
            }
            for command in &program.commands {
                for target in command.targets() {
                    check_program(target, programs.len())?;
                }
            }
        }
        let mut slots: [Vec<Option<ProgramId>>; 4] =
            AnimationTable::ALL.map(|table| vec![None; usize::from(table.len())]);
        for (selector, program) in selectors {
            check_program(program, programs.len())?;
            let slot = &mut slots[selector.table.slot()][usize::from(selector.index)];
            if slot.replace(program).is_some() {
                return Err(BattleAnimationError::DuplicateSelector(selector));
            }
        }
        let mut tables: [Vec<ProgramId>; 4] = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
        for table in AnimationTable::ALL {
            for (index, program) in (0..).zip(&slots[table.slot()]) {
                let selector = AnimationSelector { table, index };
                tables[table.slot()]
                    .push(program.ok_or(BattleAnimationError::MissingSelector(selector))?);
            }
        }
        Ok(Self { programs, tables })
    }

    /// The program `selector` launches. Total: construction guarantees every
    /// selector has one.
    #[must_use]
    pub fn program(&self, selector: AnimationSelector) -> &BattleAnimationProgram {
        let id = self.tables[selector.table.slot()][usize::from(selector.index)];
        &self.programs[usize::from(id.get())]
    }

    /// The program a `call`/`goto`/jump operand names, or `None` for an id
    /// from another catalog.
    #[must_use]
    pub fn program_by_id(&self, id: ProgramId) -> Option<&BattleAnimationProgram> {
        self.programs.get(usize::from(id.get()))
    }

    /// Every program, indexed by [`ProgramId`].
    #[must_use]
    pub fn programs(&self) -> &[BattleAnimationProgram] {
        &self.programs
    }

    /// Serializes to the `battle-anim/catalog` wire format.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        codec::encode(self)
    }

    /// Parses the `battle-anim/catalog` wire format.
    ///
    /// # Errors
    ///
    /// [`BattleAnimationError::Truncated`], [`BattleAnimationError::TrailingBytes`],
    /// [`BattleAnimationError::UnsupportedVersion`],
    /// [`BattleAnimationError::UnknownOpcode`], or
    /// [`BattleAnimationError::InvalidSymbol`] for a malformed payload; plus
    /// every error of [`AnimSymbol::new`], [`EffectSpriteId::new`],
    /// [`BattleAnimationProgram::new`], and [`BattleAnimationCatalog::new`].
    pub fn decode(bytes: &[u8]) -> Result<Self, BattleAnimationError> {
        codec::decode(bytes)
    }
}

fn check_program(id: ProgramId, count: usize) -> Result<(), BattleAnimationError> {
    if usize::from(id.get()) >= count {
        return Err(BattleAnimationError::ProgramOutOfRange {
            id: id.get(),
            count,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests;
