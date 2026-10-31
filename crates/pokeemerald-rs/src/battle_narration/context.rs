//! The state one narration request carries.
//!
//! `BtlController_EmitPrintString` copies the battle's text state into the
//! print command before the string is chosen
//! (`src/battle_controllers.c:1136`-`:1165`), and `BufferStringBattle`
//! restores it on the printing side (`src/battle_message.c:1959`-`:1982`). A
//! [`NarrationContext`] is that copy: it owns every name, ability, and buffer
//! the template can reach, so later battle changes never alter a queued
//! message.

use assets::items::ItemId;
use assets::{AbilityId, MoveId, SpeciesId};
use engine::text::Token;

/// A side of a single battle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// The player's side (`B_SIDE_PLAYER`).
    Player,
    /// The opposing side (`B_SIDE_OPPONENT`).
    Opponent,
}

impl Side {
    /// The other side.
    #[must_use]
    pub const fn other(self) -> Self {
        match self {
            Self::Player => Self::Opponent,
            Self::Opponent => Self::Player,
        }
    }

    /// The side a move's user is on, from the event's `by_player` flag.
    #[must_use]
    pub const fn user(by_player: bool) -> Self {
        if by_player {
            Self::Player
        } else {
            Self::Opponent
        }
    }
}

/// An active battler as the print command saw it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Battler {
    /// The nickname `GetMonData(.., MON_DATA_NICKNAME)` returned.
    pub name: String,
    /// `sBattlerAbilities[battler]`; `None` when the caller did not capture it.
    pub ability: Option<AbilityId>,
}

/// Whether the opponent is a trainer (`BATTLE_TYPE_TRAINER`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BattleKind {
    /// A wild battle; opposing names take the `Wild ` prefix.
    Wild,
    /// A trainer battle; opposing names take the `Foe ` prefix.
    Trainer,
}

/// The opposing trainer's text (`gTrainers[gTrainerBattleOpponent_A]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trainer {
    /// The trainer class name (`B_TRAINER1_CLASS`).
    pub class: String,
    /// The trainer name (`B_TRAINER1_NAME`).
    pub name: String,
    /// The defeat text (`GetTrainerALoseText`), terminator removed.
    pub lose_text: Vec<Token>,
}

/// A status name `TryGetStatusString` maps buffer 1's Japanese spelling to
/// (`gStatusConditionStringsTable`, `src/battle_main.c:577`-`:586`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusName {
    /// `gText_Poison`.
    Poison,
    /// `gText_Sleep`.
    Sleep,
    /// `gText_Paralysis`.
    Paralysis,
    /// `gText_Burn`.
    Burn,
    /// `gText_Ice`.
    Ice,
    /// `gText_Confusion`.
    Confusion,
    /// `gText_Love`.
    Love,
}

/// One entry of a buffer program, the `FD kind ... FF` byte string
/// `ExpandBattleTextBuffPlaceholders` interprets (`include/battle_message.h:65`-`:80`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BufferOp {
    /// `B_BUFF_STRING`: appends a `gBattleStringsTable` entry, unexpanded.
    String(u16),
    /// `B_BUFF_STRING` for a special template, by upstream symbol.
    Special(&'static str),
    /// `B_BUFF_NUMBER`: replaces the output with a left-aligned decimal.
    Number {
        /// The value to print.
        value: u32,
        /// Digit positions (`maxDigits`).
        width: u8,
    },
    /// `B_BUFF_MOVE`: appends a move name.
    Move(MoveId),
    /// `B_BUFF_TYPE`: appends a type name (`gTypeNames` index).
    Type(u8),
    /// `B_BUFF_MON_NICK_WITH_PREFIX`: appends the side's prefix and a nickname.
    MonNickWithPrefix {
        /// The battler's side.
        side: Side,
        /// The nickname.
        name: String,
    },
    /// `B_BUFF_STAT`: appends a battle stat name (`gStatNamesTable` index).
    Stat(u8),
    /// `B_BUFF_SPECIES`: replaces the output with a species name.
    Species(SpeciesId),
    /// `B_BUFF_MON_NICK`: replaces the output with a nickname.
    MonNick(String),
    /// `B_BUFF_NEGATIVE_FLAVOR`: appends a Pokéblock complaint (`FLAVOR_*` index).
    NegativeFlavor(u8),
    /// `B_BUFF_ABILITY`: appends an ability name.
    Ability(AbilityId),
    /// `B_BUFF_ITEM`: replaces the output with an item name.
    Item(ItemId),
}

/// The contents of `gBattleTextBuff1`, `2`, or `3`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum BattleBuffer {
    /// Starts with the terminator.
    #[default]
    Empty,
    /// Plain text copied verbatim.
    Text(String),
    /// A status buffer; only buffer 1 normalises it into a name.
    Status(StatusName),
    /// A program beginning with `B_BUFF_PLACEHOLDER_BEGIN`.
    Program(Vec<BufferOp>),
}

/// Everything a template may read.
///
/// The role fields (`attacker`, `defender`, `effect`, `active`, `scripting`)
/// name a [`Side`] in a single battle; a role left `None` is an error only if
/// the template reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NarrationContext {
    /// Wild or trainer opponent.
    pub kind: BattleKind,
    /// The player's active battler.
    pub player: Option<Battler>,
    /// The opponent's active battler.
    pub opponent: Option<Battler>,
    /// `gBattlerAttacker`.
    pub attacker: Option<Side>,
    /// `gBattlerTarget`.
    pub defender: Option<Side>,
    /// `gEffectBattler`.
    pub effect: Option<Side>,
    /// `gActiveBattler`.
    pub active: Option<Side>,
    /// `gBattleScripting.battler`.
    pub scripting: Option<Side>,
    /// `gBattleMsgDataPtr->currentMove`; `None` stands for a value at or past
    /// `MOVES_COUNT`, which prints `move_type`'s `sATypeMove_Table` entry.
    pub current_move: Option<MoveId>,
    /// `gBattleMsgDataPtr->originallyUsedMove`, with the same `None` meaning.
    pub original_move: Option<MoveId>,
    /// `gBattleStruct->stringMoveType`.
    pub move_type: u8,
    /// `gLastUsedItem`.
    pub last_item: Option<ItemId>,
    /// `gLastUsedAbility`.
    pub last_ability: Option<AbilityId>,
    /// The opposing trainer.
    pub trainer: Option<Trainer>,
    /// `gSaveBlock2Ptr->playerName`.
    pub player_name: String,
    /// `gBattleTextBuff1` through `3`.
    pub buffers: [BattleBuffer; 3],
    /// `gStringVar1` through `3` as they stand before the print.
    pub string_vars: [Vec<Token>; 3],
}

impl NarrationContext {
    /// A context with no roles, empty buffers, and empty string variables.
    #[must_use]
    pub fn new(kind: BattleKind, player_name: String) -> Self {
        Self {
            kind,
            player: None,
            opponent: None,
            attacker: None,
            defender: None,
            effect: None,
            active: None,
            scripting: None,
            current_move: None,
            original_move: None,
            move_type: 0,
            last_item: None,
            last_ability: None,
            trainer: None,
            player_name,
            buffers: Default::default(),
            string_vars: Default::default(),
        }
    }

    /// The battler on `side`.
    #[must_use]
    pub const fn battler(&self, side: Side) -> Option<&Battler> {
        match side {
            Side::Player => self.player.as_ref(),
            Side::Opponent => self.opponent.as_ref(),
        }
    }
}

impl NarrationContext {
    /// Sets the move user as attacker and the other side as defender and
    /// effect battler.
    #[must_use]
    pub(super) fn user_vs_target(mut self, user: Side) -> Self {
        self.attacker = Some(user);
        self.defender = Some(user.other());
        self.effect = Some(user.other());
        self
    }

    /// Sets the move both message readers of the current-move placeholders see.
    #[must_use]
    pub(super) fn with_move(mut self, move_id: MoveId) -> Self {
        self.current_move = Some(move_id);
        self.original_move = Some(move_id);
        self
    }

    /// Overrides what `side`'s battler holds as its ability.
    #[must_use]
    pub(super) fn with_ability(mut self, side: Side, ability: AbilityId) -> Self {
        let battler = match side {
            Side::Player => self.player.as_mut(),
            Side::Opponent => self.opponent.as_mut(),
        };
        if let Some(battler) = battler {
            battler.ability = Some(ability);
        }
        self
    }

    /// Sets buffer `index` (0 through 2).
    #[must_use]
    pub(super) fn with_buffer(mut self, index: usize, buffer: BattleBuffer) -> Self {
        self.buffers[index] = buffer;
        self
    }
}
