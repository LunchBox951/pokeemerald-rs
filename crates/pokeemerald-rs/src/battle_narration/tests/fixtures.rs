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

/// Encodes `{NAME}` as `FD selector`, `{PAUSE n}` as `FC 08 n`, `{SHIFT_DOWN n}` as `FC 0E n`, a newline as `FE`,
/// `{PAUSE_UNTIL_PRESS}` as `FC 09`, `\p` as the page clear `FB`, and `\l` as the scroll prompt `FA` (`charmap.txt:1087`-`:1088`).
pub(super) fn template(source: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => {
                let name: String = chars.by_ref().take_while(|&c| c != '}').collect();
                if name == "PAUSE_UNTIL_PRESS" {
                    out.extend([0xFC, 0x09]);
                    continue;
                }
                if let Some(pixels) = name.strip_prefix("SHIFT_DOWN ") {
                    out.extend([
                        0xFC,
                        0x0E,
                        pixels.parse().expect("SHIFT_DOWN takes a pixel count"),
                    ]);
                    continue;
                }
                if let Some(frames) = name.strip_prefix("PAUSE ") {
                    out.extend([
                        0xFC,
                        0x08,
                        frames.parse().expect("PAUSE takes a frame count"),
                    ]);
                    continue;
                }
                let selector = SELECTORS
                    .iter()
                    .find(|(known, _)| *known == name)
                    .unwrap_or_else(|| panic!("fixture placeholder {name} is unknown"));
                out.extend([0xFD, selector.1]);
            }
            '\\' => match chars.next() {
                Some('p') => out.push(0xFB),
                Some('l') => out.push(0xFA),
                other => panic!("only \\p and \\l are escapes, not {other:?}"),
            },
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

/// The tokens of ordinary text, terminator removed, as a trainer class or name
/// arrives from the class-name table.
pub(super) fn text_tokens(source: &str) -> Vec<Token> {
    let mut tokens = text::decode(&template(source)).unwrap();
    tokens.pop();
    tokens
}

/// The rival's class name as `gTrainerClassNames` stores it: the `PK` and `MN`
/// glyph bytes, then ` TRAINER` (`src/data/text/trainer_class_names.h:52`).
pub(super) const RIVAL_CLASS: [u8; 11] = [
    0x53, 0x54, 0x00, 0xCE, 0xCC, 0xBB, 0xC3, 0xC8, 0xBF, 0xCC, 0xFF,
];

/// The rival's class name decoded to tokens, terminator removed.
pub(super) fn rival_class() -> Vec<Token> {
    let mut tokens = text::decode(&RIVAL_CLASS).unwrap();
    tokens.pop();
    tokens
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
                class: text_tokens("YOUNGSTER"),
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

/// Upstream table templates (`src/battle_message.c`) keyed by `STRINGID_*`.
pub(super) const SLOTS: &[(u16, &str)] = &[
    (12, "{B_TRAINER1_LOSE_TEXT}"),
    (13, "{B_BUFF1} gained{B_BUFF2}\n{B_BUFF3} EXP. Points!\\p"),
    (15, "{B_BUFF1} learned\n{B_BUFF2}!\\p"),
    (16, "{B_BUFF1} is trying to\nlearn {B_BUFF2}.\\p"),
    (17, "But, {B_BUFF1} can't learn\nmore than four moves.\\p"),
    (18, "Delete a move to make\nroom for {B_BUFF2}?"),
    (19, "{B_BUFF1} forgot\n{B_BUFF2}.\\p"),
    (20, "{PAUSE 32}Stop learning\n{B_BUFF2}?"),
    (21, "{B_BUFF1} did not learn\n{B_BUFF2}.\\p"),
    (23, "{B_ATK_NAME_WITH_PREFIX}'s\nattack missed!"),
    (28, "{B_ATK_NAME_WITH_PREFIX}\nfainted!\\p"),
    (29, "{B_DEF_NAME_WITH_PREFIX}\nfainted!\\p"),
    (30, "{B_PLAYER_NAME} got ¥{B_BUFF1}\nfor winning!\\p"),
    (31, "{B_PLAYER_NAME} is out of\nusable POKéMON!\\p"),
    (32, "{B_PLAYER_NAME} whited out!"),
    (36, "{B_SCR_ACTIVE_NAME_WITH_PREFIX}'s {B_SCR_ACTIVE_ABILITY}\nmade {B_EFF_NAME_WITH_PREFIX} sleep!"),
    (41, "{B_SCR_ACTIVE_NAME_WITH_PREFIX}'s {B_SCR_ACTIVE_ABILITY}\npoisoned {B_EFF_NAME_WITH_PREFIX}!"),
    (42, "{B_ATK_NAME_WITH_PREFIX} is hurt\nby poison!"),
    (46, "{B_EFF_NAME_WITH_PREFIX} was burned!"),
    (48, "{B_ATK_NAME_WITH_PREFIX} is hurt\nby its burn!"),
    (69, "{B_DEF_NAME_WITH_PREFIX}\nfell in love!"),
    (70, "{B_ATK_NAME_WITH_PREFIX} is in love\nwith {B_SCR_ACTIVE_NAME_WITH_PREFIX}!"),
    (71, "{B_ATK_NAME_WITH_PREFIX} is\nimmobilized by love!"),
    (200, "{B_DEF_NAME_WITH_PREFIX}'s {B_DEF_ABILITY}\nprevents romance!"),
    (104, "{B_DEF_NAME_WITH_PREFIX} was seeded!"),
    (105, "{B_DEF_NAME_WITH_PREFIX} evaded\nthe attack!"),
    (106, "{B_ATK_NAME_WITH_PREFIX}'s health is\nsapped by LEECH SEED!"),
    (298, "{B_SCR_ACTIVE_NAME_WITH_PREFIX}'s {B_LAST_ITEM}\nrestored health!"),
    (107, "{B_ATK_NAME_WITH_PREFIX} is fast\nasleep."),
    (108, "{B_ATK_NAME_WITH_PREFIX} woke up!"),
    (51, "{B_ATK_NAME_WITH_PREFIX} is\nfrozen solid!"),
    (52, "{B_DEF_NAME_WITH_PREFIX} was\ndefrosted!"),
    (53, "{B_ATK_NAME_WITH_PREFIX} was\ndefrosted!"),
    (54, "{B_ATK_NAME_WITH_PREFIX} was\ndefrosted by {B_CURRENT_MOVE}!"),
    (278, "{B_ATK_NAME_WITH_PREFIX} is\nloafing around!"),
    (197, "{B_DEF_NAME_WITH_PREFIX} restored HP\nusing its {B_DEF_ABILITY}!"),
    (56, "{B_SCR_ACTIVE_NAME_WITH_PREFIX}'s {B_SCR_ACTIVE_ABILITY}\nparalyzed {B_EFF_NAME_WITH_PREFIX}!"),
    (209, "sharply "),
    (210, "rose!"),
    (211, "harshly "),
    (212, "fell!"),
    (216, "{B_DEF_NAME_WITH_PREFIX}'s {B_BUFF1}\n{B_BUFF2}"),
    (217, "A critical hit!"),
    (221, "It's not very effective…"),
    (222, "It's super effective!"),
    (219, "1, 2, and... Poof!\\p"),
    (220, "And...\\p"),
    (223, "Got away safely!\\p"),
    (224, "Wild {B_BUFF1} fled!"),
    (226, "Can't escape!\\p"),
    (310, "{B_DEF_NAME_WITH_PREFIX}'s {B_DEF_ABILITY}\ninfatuated {B_ATK_NAME_WITH_PREFIX}!"),
    (327, "{B_DEF_NAME_WITH_PREFIX}'s {B_DEF_ABILITY}\nmade {B_CURRENT_MOVE} useless!"),
    (329, ""),
    (330, " a boosted"),
    (339, "{B_ATK_NAME_WITH_PREFIX} fled\nusing {B_ATK_ABILITY}!\\p"),
    (306, "{B_DEF_NAME_WITH_PREFIX}'s {B_DEF_ABILITY}\nblocks {B_CURRENT_MOVE}!"),
    (321, "Player defeated\n{B_TRAINER1_CLASS} {B_TRAINER1_NAME}!\\p"),
    (355, "{B_ATK_NAME_WITH_PREFIX}'s {B_ATK_ABILITY}\nprevents {B_DEF_NAME_WITH_PREFIX}'s {B_DEF_ABILITY} from working!"),
];

pub(super) const SPECIALS: &[(&str, &str)] = &[
    (
        "sText_WildPkmnAppeared",
        "Wild {B_OPPONENT_MON1_NAME} appeared!\\p",
    ),
    (
        "sText_Trainer1WantsToBattle",
        "{B_TRAINER1_CLASS} {B_TRAINER1_NAME}\nwould like to battle!\\p",
    ),
    ("sText_GoPkmn", "Go! {B_PLAYER_MON1_NAME}!"),
    (
        "sText_Trainer1SentOutPkmn",
        "{B_TRAINER1_CLASS} {B_TRAINER1_NAME} sent\nout {B_OPPONENT_MON1_NAME}!",
    ),
    (
        "sText_Trainer1SentOutPkmn2",
        "{B_TRAINER1_CLASS} {B_TRAINER1_NAME} sent\nout {B_BUFF1}!",
    ),
    ("sText_GoPkmn2", "Go! {B_BUFF1}!"),
    ("sText_DoItPkmn", "Do it! {B_BUFF1}!"),
    ("sText_GoForItPkmn", "Go for it, {B_BUFF1}!"),
    (
        "sText_YourFoesWeakGetEmPkmn",
        "Your foe's weak!\nGet 'em, {B_BUFF1}!",
    ),
    (
        "sText_AttackerUsedX",
        "{B_ATK_NAME_WITH_PREFIX} used\n{B_BUFF2}",
    ),
    ("sText_ExclamationMark", "!"),
];

pub(super) fn upstream_fixture() -> Fixture {
    Fixture::new(SLOTS, SPECIALS)
}
