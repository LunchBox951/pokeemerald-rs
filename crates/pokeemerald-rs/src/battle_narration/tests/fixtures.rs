//! A complete message table built from upstream template text, so tests
//! exercise expansion without the asset pack.

use std::collections::BTreeMap;

use assets::pack::BattleMessageTable;
use assets::pack::{
    BATTLE_MESSAGE_SPECIAL_TEMPLATES as SPECIAL_TEMPLATES, BATTLE_MESSAGE_TABLE_END as TABLE_END,
    BATTLE_MESSAGE_TABLE_START as TABLE_START,
};
use assets::{AbilityId, SpeciesId};
use engine::text::{self, Token};

use crate::battle_narration::{
    BattleKind, Battler, NarrationContext, NarrationRequest, Narrator, Scene, TemplateId, Trainer,
};

/// `B_TXT_*` names to selectors, from `include/battle_message.h:11`-`:64`.
const SELECTORS: &[(&str, u8)] = &[
    ("B_BUFF1", 0x00),
    ("B_BUFF2", 0x01),
    ("STR_VAR_1", 0x02),
    ("STR_VAR_2", 0x03),
    ("STR_VAR_3", 0x04),
    ("B_PLAYER_MON1_NAME", 0x05),
    ("B_OPPONENT_MON1_NAME", 0x06),
    ("B_ATK_NAME_WITH_PREFIX", 0x0F),
    ("B_DEF_NAME_WITH_PREFIX", 0x10),
    ("B_EFF_NAME_WITH_PREFIX", 0x11),
    ("B_ACTIVE_NAME_WITH_PREFIX", 0x12),
    ("B_SCR_ACTIVE_NAME_WITH_PREFIX", 0x13),
    ("B_CURRENT_MOVE", 0x14),
    ("B_LAST_MOVE", 0x15),
    ("B_LAST_ITEM", 0x16),
    ("B_LAST_ABILITY", 0x17),
    ("B_ATK_ABILITY", 0x18),
    ("B_DEF_ABILITY", 0x19),
    ("B_SCR_ACTIVE_ABILITY", 0x1A),
    ("B_EFF_ABILITY", 0x1B),
    ("B_TRAINER1_CLASS", 0x1C),
    ("B_TRAINER1_NAME", 0x1D),
    ("B_PLAYER_NAME", 0x23),
    ("B_TRAINER1_LOSE_TEXT", 0x24),
    ("B_ATK_PREFIX1", 0x28),
    ("B_DEF_PREFIX1", 0x29),
    ("B_BUFF3", 0x34),
    ("B_LINK_PLAYER_MON1_NAME", 0x09),
    ("B_ATK_PARTNER_NAME", 0x0E),
    ("B_PC_CREATOR_NAME", 0x27),
    ("B_TRAINER1_WIN_TEXT", 0x25),
    ("B_ATK_PREFIX2", 0x2A),
    ("B_DEF_PREFIX3", 0x2D),
    ("B_ATK_NAME_WITH_PREFIX_MON1", 0x0D),
];

/// Encodes `{NAME}` as `FD selector`, a newline as `FE`, and `\p` as `FA`.
pub(super) fn template(source: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => {
                let name: String = chars.by_ref().take_while(|&c| c != '}').collect();
                let selector = SELECTORS
                    .iter()
                    .find(|(known, _)| *known == name)
                    .unwrap_or_else(|| panic!("fixture placeholder {name} is unknown"));
                out.extend([0xFD, selector.1]);
            }
            '\\' => {
                assert_eq!(chars.next(), Some('p'), "only \\p is an escape");
                out.push(0xFA);
            }
            '\n' => out.push(0xFE),
            other => out.push(text::char_to_byte(other).expect("fixture glyph encodes")),
        }
    }
    out.push(0xFF);
    out
}

/// Owns the encoded strings a [`BattleMessageTable`] borrows.
pub(super) struct Fixture {
    slots: BTreeMap<u16, Vec<u8>>,
    specials: BTreeMap<&'static str, Vec<u8>>,
}

impl Fixture {
    /// Every slot and special empty except the given upstream templates.
    pub(super) fn new(slots: &[(u16, &str)], specials: &[(&'static str, &str)]) -> Self {
        Self {
            slots: slots
                .iter()
                .map(|(id, text)| (*id, template(text)))
                .collect(),
            specials: specials
                .iter()
                .map(|(symbol, text)| (*symbol, template(text)))
                .collect(),
        }
    }

    pub(super) fn table(&self) -> BattleMessageTable<'_> {
        let empty: &'static [u8] = &[0xFF];
        let slots = (TABLE_START..TABLE_END)
            .map(|id| (id, self.slots.get(&id).map_or(empty, Vec::as_slice)));
        let special = SPECIAL_TEMPLATES.iter().enumerate().map(|(index, entry)| {
            (
                u16::try_from(index).unwrap(),
                self.specials.get(entry.symbol).map_or(empty, Vec::as_slice),
            )
        });
        BattleMessageTable::from_entries(slots, special).unwrap()
    }
}

/// The text `tokens` print, with controls spelled as `decode_to_string` does.
pub(super) fn render(tokens: &[Token]) -> String {
    text::decode_to_string(&text::encode(tokens).unwrap()).unwrap()
}

pub(super) const RATTATA: SpeciesId = SpeciesId(19);
pub(super) const ZIGZAGOON: SpeciesId = SpeciesId(288);

pub(super) fn battler(name: &str, ability: Option<AbilityId>) -> Battler {
    Battler {
        name: name.to_owned(),
        ability,
    }
}

pub(super) fn scene(kind: BattleKind) -> Scene {
    Scene {
        kind,
        player_name: "BRENDAN".to_owned(),
        player: battler("TORCHIC", Some(AbilityId::BLAZE)),
        opponent: battler("RATTATA", Some(AbilityId::RUN_AWAY)),
        trainer: (kind == BattleKind::Trainer).then(|| {
            let mut lose_text = text::decode(&template("Good battle!")).unwrap();
            lose_text.pop();
            Trainer {
                class: "YOUNGSTER".to_owned(),
                name: "BEN".to_owned(),
                lose_text,
            }
        }),
        hp_scale: 0,
    }
}

pub(super) fn narrator(kind: BattleKind) -> Narrator {
    Narrator::new(scene(kind))
}

pub(super) fn bare_context(kind: BattleKind) -> NarrationContext {
    NarrationContext::new(kind, "BRENDAN".to_owned())
}

pub(super) fn request(template: TemplateId, context: NarrationContext) -> NarrationRequest {
    NarrationRequest { template, context }
}
