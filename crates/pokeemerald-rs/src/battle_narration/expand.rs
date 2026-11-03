//! Template expansion, `BattleStringExpandPlaceholders` and
//! `ExpandBattleTextBuffPlaceholders` (`src/battle_message.c:2312`-`:2873`).

use assets::pack::BattleMessageTable;
use engine::text::format::{convert_uint_to_decimal_string_n, StringConvertMode};
use engine::text::{self, Token, EXT_CTRL_CODE_PAUSE_UNTIL_PRESS};

use super::context::{BattleBuffer, BufferOp, NarrationContext, Side, StatusName};
use super::names::{flavor_name, stat_name, type_name, NarrationNames};
use super::{NarrationError, NarrationRequest, TemplateId};

/// `B_TXT_*` selectors this layer resolves (`include/battle_message.h:11`-`:64`).
mod selector {
    pub(super) const BUFF1: u8 = 0x00;
    pub(super) const BUFF2: u8 = 0x01;
    pub(super) const COPY_VAR_1: u8 = 0x02;
    pub(super) const COPY_VAR_3: u8 = 0x04;
    pub(super) const PLAYER_MON1_NAME: u8 = 0x05;
    pub(super) const OPPONENT_MON1_NAME: u8 = 0x06;
    pub(super) const ATK_NAME_WITH_PREFIX_MON1: u8 = 0x0D;
    pub(super) const ATK_NAME_WITH_PREFIX: u8 = 0x0F;
    pub(super) const DEF_NAME_WITH_PREFIX: u8 = 0x10;
    pub(super) const EFF_NAME_WITH_PREFIX: u8 = 0x11;
    pub(super) const ACTIVE_NAME_WITH_PREFIX: u8 = 0x12;
    pub(super) const SCR_ACTIVE_NAME_WITH_PREFIX: u8 = 0x13;
    pub(super) const CURRENT_MOVE: u8 = 0x14;
    pub(super) const LAST_MOVE: u8 = 0x15;
    pub(super) const LAST_ITEM: u8 = 0x16;
    pub(super) const LAST_ABILITY: u8 = 0x17;
    pub(super) const ATK_ABILITY: u8 = 0x18;
    pub(super) const DEF_ABILITY: u8 = 0x19;
    pub(super) const SCR_ACTIVE_ABILITY: u8 = 0x1A;
    pub(super) const EFF_ABILITY: u8 = 0x1B;
    pub(super) const TRAINER1_CLASS: u8 = 0x1C;
    pub(super) const TRAINER1_NAME: u8 = 0x1D;
    pub(super) const PLAYER_NAME: u8 = 0x23;
    pub(super) const TRAINER1_LOSE_TEXT: u8 = 0x24;
    pub(super) const ATK_PREFIX1: u8 = 0x28;
    pub(super) const DEF_PREFIX1: u8 = 0x29;
    pub(super) const ATK_PREFIX2: u8 = 0x2A;
    pub(super) const DEF_PREFIX2: u8 = 0x2B;
    pub(super) const ATK_PREFIX3: u8 = 0x2C;
    pub(super) const DEF_PREFIX3: u8 = 0x2D;
    pub(super) const BUFF3: u8 = 0x34;
}

/// `sATypeMove_Table` symbols in `gTypeNames` order.
const TYPE_MOVE_SYMBOLS: [&str; 18] = [
    "sATypeMove_Table[TYPE_NORMAL]",
    "sATypeMove_Table[TYPE_FIGHTING]",
    "sATypeMove_Table[TYPE_FLYING]",
    "sATypeMove_Table[TYPE_POISON]",
    "sATypeMove_Table[TYPE_GROUND]",
    "sATypeMove_Table[TYPE_ROCK]",
    "sATypeMove_Table[TYPE_BUG]",
    "sATypeMove_Table[TYPE_GHOST]",
    "sATypeMove_Table[TYPE_STEEL]",
    "sATypeMove_Table[TYPE_MYSTERY]",
    "sATypeMove_Table[TYPE_FIRE]",
    "sATypeMove_Table[TYPE_WATER]",
    "sATypeMove_Table[TYPE_GRASS]",
    "sATypeMove_Table[TYPE_ELECTRIC]",
    "sATypeMove_Table[TYPE_PSYCHIC]",
    "sATypeMove_Table[TYPE_ICE]",
    "sATypeMove_Table[TYPE_DRAGON]",
    "sATypeMove_Table[TYPE_DARK]",
];

/// Expands `request` into the tokens upstream would print, terminator included.
///
/// # Errors
///
/// Returns [`NarrationError`] when the template, a name, or a context field the
/// template reads is unavailable, or when it reads a placeholder this layer
/// does not model.
pub fn expand(
    table: &BattleMessageTable<'_>,
    names: &impl NarrationNames,
    request: &NarrationRequest,
) -> Result<Vec<Token>, NarrationError> {
    let template = match request.template {
        TemplateId::Table(id) => table.string(id)?,
        TemplateId::Special(symbol) => table
            .special_by_symbol(symbol)
            .ok_or_else(|| NarrationError::UnknownSpecial(symbol.to_owned()))?,
    };
    let mut expander = Expander {
        table,
        names,
        context: &request.context,
        string_vars: request.context.string_vars.clone(),
    };
    expander.template(&text::decode(template)?)
}

struct Expander<'a, N> {
    table: &'a BattleMessageTable<'a>,
    names: &'a N,
    context: &'a NarrationContext,
    /// `gStringVar1` through `3`, which expanding a buffer program overwrites.
    string_vars: [Vec<Token>; 3],
}

impl<N: NarrationNames> Expander<'_, N> {
    fn template(&mut self, tokens: &[Token]) -> Result<Vec<Token>, NarrationError> {
        let mut out = Vec::with_capacity(tokens.len());
        for token in tokens {
            match token {
                Token::Placeholder(selector) => {
                    out.extend(self.placeholder(*selector)?);
                    if *selector == selector::TRAINER1_LOSE_TEXT {
                        out.push(Token::ExtCtrl {
                            sub: EXT_CTRL_CODE_PAUSE_UNTIL_PRESS,
                            args: Vec::new(),
                        });
                    }
                }
                other => out.push(other.clone()),
            }
        }
        Ok(out)
    }

    fn placeholder(&mut self, id: u8) -> Result<Vec<Token>, NarrationError> {
        let context = self.context;
        match id {
            selector::BUFF1 => self.buffer(0),
            selector::BUFF2 => self.buffer(1),
            selector::BUFF3 => self.buffer(2),
            selector::COPY_VAR_1..=selector::COPY_VAR_3 => {
                Ok(self.string_vars[usize::from(id - selector::COPY_VAR_1)].clone())
            }
            selector::PLAYER_MON1_NAME => self.battler_name(Side::Player),
            selector::OPPONENT_MON1_NAME => self.battler_name(Side::Opponent),
            selector::ATK_NAME_WITH_PREFIX_MON1 | selector::ATK_NAME_WITH_PREFIX => {
                self.name_with_prefix(context.attacker, "attacker")
            }
            selector::DEF_NAME_WITH_PREFIX => self.name_with_prefix(context.defender, "defender"),
            selector::EFF_NAME_WITH_PREFIX => {
                self.name_with_prefix(context.effect, "effect battler")
            }
            selector::ACTIVE_NAME_WITH_PREFIX => {
                self.name_with_prefix(context.active, "active battler")
            }
            selector::SCR_ACTIVE_NAME_WITH_PREFIX => {
                self.name_with_prefix(context.scripting, "scripting battler")
            }
            selector::CURRENT_MOVE => self.move_text(context.current_move),
            selector::LAST_MOVE => self.move_text(context.original_move),
            selector::LAST_ITEM => {
                let item = context
                    .last_item
                    .ok_or(NarrationError::MissingContext("last used item"))?;
                self.item(item)
            }
            selector::LAST_ABILITY => {
                let ability = context
                    .last_ability
                    .ok_or(NarrationError::MissingContext("last used ability"))?;
                self.ability(ability)
            }
            selector::ATK_ABILITY => self.battler_ability(context.attacker, "attacker"),
            selector::DEF_ABILITY => self.battler_ability(context.defender, "defender"),
            selector::SCR_ACTIVE_ABILITY => {
                self.battler_ability(context.scripting, "scripting battler")
            }
            selector::EFF_ABILITY => self.battler_ability(context.effect, "effect battler"),
            selector::TRAINER1_CLASS => text_tokens(&self.trainer()?.class),
            selector::TRAINER1_NAME => text_tokens(&self.trainer()?.name),
            selector::PLAYER_NAME => text_tokens(&context.player_name),
            selector::TRAINER1_LOSE_TEXT => Ok(self.trainer()?.lose_text.clone()),
            selector::ATK_PREFIX1 | selector::ATK_PREFIX2 | selector::ATK_PREFIX3 => {
                ally_or_foe(context.attacker, "attacker")
            }
            selector::DEF_PREFIX1 | selector::DEF_PREFIX2 | selector::DEF_PREFIX3 => {
                ally_or_foe(context.defender, "defender")
            }
            other => Err(NarrationError::UnsupportedPlaceholder(other)),
        }
    }

    /// One of the three battle buffers (`:2333`-`:2375`).
    fn buffer(&mut self, index: usize) -> Result<Vec<Token>, NarrationError> {
        let context = self.context;
        match &context.buffers[index] {
            BattleBuffer::Empty => Ok(Vec::new()),
            BattleBuffer::Text(text) => text_tokens(text),
            BattleBuffer::Status(status) if index == 0 => text_tokens(status_name(*status)),
            BattleBuffer::Status(_) => Err(NarrationError::StatusOutsideBuffer1),
            BattleBuffer::Program(ops) => {
                let expanded = self.program(ops)?;
                self.string_vars[index].clone_from(&expanded);
                Ok(expanded)
            }
        }
    }

    /// `ExpandBattleTextBuffPlaceholders`: number, species, and nickname
    /// entries replace the output so far; every other entry appends.
    fn program(&self, ops: &[BufferOp]) -> Result<Vec<Token>, NarrationError> {
        let mut out = Vec::new();
        for op in ops {
            match op {
                BufferOp::String(id) => out.extend(table_text(self.table.string(*id)?)?),
                BufferOp::Special(symbol) => {
                    let bytes = self
                        .table
                        .special_by_symbol(symbol)
                        .ok_or_else(|| NarrationError::UnknownSpecial((*symbol).to_owned()))?;
                    out.extend(table_text(bytes)?);
                }
                BufferOp::Number { value, width } => {
                    out = convert_uint_to_decimal_string_n(
                        *value,
                        StringConvertMode::LeftAlign,
                        *width,
                    )?;
                }
                BufferOp::Move(move_id) => out.extend(text_tokens(
                    &self
                        .names
                        .move_name(*move_id)
                        .ok_or(NarrationError::UnknownName("move"))?,
                )?),
                BufferOp::Type(type_id) => out.extend(text_tokens(
                    type_name(*type_id).ok_or(NarrationError::UnknownName("type"))?,
                )?),
                BufferOp::MonNickWithPrefix { side, name } => {
                    out.extend(self.prefix(*side)?);
                    out.extend(text_tokens(name)?);
                }
                BufferOp::Stat(stat) => out.extend(text_tokens(
                    stat_name(*stat).ok_or(NarrationError::UnknownName("stat"))?,
                )?),
                BufferOp::Species(species) => {
                    out = text_tokens(
                        &self
                            .names
                            .species_name(*species)
                            .ok_or(NarrationError::UnknownName("species"))?,
                    )?;
                }
                BufferOp::MonNick(name) => out = text_tokens(name)?,
                BufferOp::NegativeFlavor(flavor) => out.extend(text_tokens(
                    flavor_name(*flavor).ok_or(NarrationError::UnknownName("flavor"))?,
                )?),
                BufferOp::Ability(ability) => out.extend(self.ability(*ability)?),
                // `CopyItemName` copies over the output so far.
                BufferOp::Item(item) => out = self.item(*item)?,
            }
        }
        Ok(out)
    }

    fn trainer(&self) -> Result<&super::context::Trainer, NarrationError> {
        self.context
            .trainer
            .as_ref()
            .ok_or(NarrationError::MissingContext("trainer"))
    }

    fn battler(&self, side: Side) -> Result<&super::context::Battler, NarrationError> {
        self.context
            .battler(side)
            .ok_or(NarrationError::MissingContext("battler"))
    }

    fn battler_name(&self, side: Side) -> Result<Vec<Token>, NarrationError> {
        text_tokens(&self.battler(side)?.name)
    }

    /// `HANDLE_NICKNAME_STRING_CASE`: opposing battlers take `Foe ` or `Wild `.
    fn name_with_prefix(
        &self,
        role: Option<Side>,
        what: &'static str,
    ) -> Result<Vec<Token>, NarrationError> {
        let side = role.ok_or(NarrationError::MissingContext(what))?;
        let mut out = self.prefix(side)?;
        out.extend(self.battler_name(side)?);
        Ok(out)
    }

    fn prefix(&self, side: Side) -> Result<Vec<Token>, NarrationError> {
        match (side, self.context.kind) {
            (Side::Player, _) => Ok(Vec::new()),
            (Side::Opponent, super::BattleKind::Trainer) => text_tokens("Foe "),
            (Side::Opponent, super::BattleKind::Wild) => text_tokens("Wild "),
        }
    }

    fn battler_ability(
        &self,
        role: Option<Side>,
        what: &'static str,
    ) -> Result<Vec<Token>, NarrationError> {
        let side = role.ok_or(NarrationError::MissingContext(what))?;
        let ability = self
            .battler(side)?
            .ability
            .ok_or(NarrationError::MissingContext("battler ability"))?;
        self.ability(ability)
    }

    fn ability(&self, ability: assets::AbilityId) -> Result<Vec<Token>, NarrationError> {
        text_tokens(
            &self
                .names
                .ability_name(ability)
                .ok_or(NarrationError::UnknownName("ability"))?,
        )
    }

    fn item(&self, item: assets::items::ItemId) -> Result<Vec<Token>, NarrationError> {
        text_tokens(
            &self
                .names
                .item_name(item)
                .ok_or(NarrationError::UnknownName("item"))?,
        )
    }

    /// A move name, or the type-move string when the move is out of range.
    fn move_text(&self, move_id: Option<assets::MoveId>) -> Result<Vec<Token>, NarrationError> {
        if let Some(move_id) = move_id {
            return text_tokens(
                &self
                    .names
                    .move_name(move_id)
                    .ok_or(NarrationError::UnknownName("move"))?,
            );
        }
        let symbol = TYPE_MOVE_SYMBOLS
            .get(usize::from(self.context.move_type))
            .ok_or(NarrationError::UnknownName("type"))?;
        let bytes = self
            .table
            .special_by_symbol(symbol)
            .ok_or_else(|| NarrationError::UnknownSpecial((*symbol).to_owned()))?;
        table_text(bytes)
    }
}

/// A table entry's tokens, terminator dropped; its own placeholders stay.
fn table_text(bytes: &[u8]) -> Result<Vec<Token>, NarrationError> {
    let mut tokens = text::decode(bytes)?;
    if tokens.last() == Some(&Token::End) {
        tokens.pop();
    }
    Ok(tokens)
}

/// `sText_AllyPkmnPrefix*` and `sText_FoePkmnPrefix*`, which spell `Ally`
/// and `Foe` in all three variants (`src/battle_message.c:413`-`:418`).
fn ally_or_foe(role: Option<Side>, what: &'static str) -> Result<Vec<Token>, NarrationError> {
    match role.ok_or(NarrationError::MissingContext(what))? {
        Side::Player => text_tokens("Ally"),
        Side::Opponent => text_tokens("Foe"),
    }
}

/// `gText_Poison` and the other English status names.
const fn status_name(status: StatusName) -> &'static str {
    match status {
        StatusName::Poison => "poison",
        StatusName::Sleep => "sleep",
        StatusName::Paralysis => "paralysis",
        StatusName::Burn => "burn",
        StatusName::Ice => "ice",
        StatusName::Confusion => "confusion",
        StatusName::Love => "love",
    }
}

/// The glyph tokens of `text`, with no terminator.
fn text_tokens(text: &str) -> Result<Vec<Token>, NarrationError> {
    let mut tokens = text::decode(&text::encode_str(text)?)?;
    tokens.pop();
    Ok(tokens)
}
