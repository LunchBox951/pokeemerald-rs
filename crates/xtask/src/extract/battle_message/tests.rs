use std::fmt::Write as _;

use super::*;
use assets::pack::{AssetPack, SpecialBattleMessage};

const CHARMAP: &str = "\
@ comment
' ' = 00
'a' = D5
'b' = D6
'!' = AB
'/' = BA
'$' = FF
'“' = B1
'”' = B2
'\\'' = B4
'\\l' = FA
'\\p' = FB
'\\n' = FE
PLAYER = FD 01
B_BUFF1 = FD 00
B_BUFF2 = FD 01
STR_VAR_1 = FD 02
PAUSE = FC 08
PLAY_SE = FC 10
SE_FLEE = 11 00
PLAY_BGM = FC 0B
MUS_X = 60 01
";

fn slot_symbol(id: u16) -> String {
    if id >= 377 {
        EXTERNAL_LABELS[usize::from(id - 377)].to_owned()
    } else {
        format!("sText_Slot{id}")
    }
}

fn slot_body(id: u16) -> &'static str {
    if id.is_multiple_of(2) {
        "a {PLAYER}\\p"
    } else {
        "a!{PAUSE 32}"
    }
}

fn slot_bytes(id: u16) -> Vec<u8> {
    if id >= 377 {
        return external_bytes();
    }
    if id.is_multiple_of(2) {
        vec![0xD5, 0x00, 0xFD, 0x01, 0xFB, 0xFF]
    } else {
        vec![0xD5, 0xAB, 0xFC, 0x08, 0x20, 0xFF]
    }
}

fn external_bytes() -> Vec<u8> {
    vec![0xB1, 0xFD, 0x02, 0xB2, 0xFE, 0xD5, 0xFF]
}

struct Fixture {
    ids: String,
    message: String,
    pc: String,
}

fn fixture() -> Fixture {
    let mut ids = String::from("#define STRINGID_INTROMSG 0\n");
    for id in 12..381 {
        writeln!(ids, "#define STRINGID_SLOT{id} {id} // note").unwrap();
    }
    ids.push_str("#define BATTLESTRINGS_COUNT 381\n/* x */\n");
    ids.push_str("#define BATTLESTRINGS_TABLE_START STRINGID_SLOT12\n");

    let mut message = String::from(
        "// don't \"confuse\" the lexer\n#include \"global.h\"\n/* \" ' */\nstatic const u8 sText_Fwd[];\n",
    );
    for id in 12..377 {
        writeln!(
            message,
            "static const u8 sText_Slot{id}[] = _(\"{}\");",
            slot_body(id)
        )
        .unwrap();
    }
    message.push_str("static const u8 sText_Fwd[] = _(\"a\" \"b\");\n");
    for (index, template) in BATTLE_MESSAGE_SPECIAL_TEMPLATES.iter().enumerate() {
        if !template.symbol.starts_with("sATypeMove_Table") {
            writeln!(
                message,
                "static const u8 {}[] = _(\"{}\");",
                template.symbol,
                if index == 0 { "a/b" } else { "b" }
            )
            .unwrap();
        }
    }
    message.push_str("static const u8 sATypeMove_Table[NUMBER_OF_MON_TYPES][17] =\n{\n");
    for template in &BATTLE_MESSAGE_SPECIAL_TEMPLATES {
        if let Some(kind) = template
            .symbol
            .strip_prefix("sATypeMove_Table[")
            .and_then(|s| s.strip_suffix(']'))
        {
            writeln!(message, "    [{kind}] = _(\"ab\"),").unwrap();
        }
    }
    message.push_str("};\nconst u8 *const gBattleStringsTable[BATTLESTRINGS_COUNT - BATTLESTRINGS_TABLE_START] =\n{\n");
    for id in (12..381).rev() {
        writeln!(
            message,
            "    [STRINGID_SLOT{id} - BATTLESTRINGS_TABLE_START] = {},",
            slot_symbol(id)
        )
        .unwrap();
    }
    message.push_str("};\n");

    let mut pc = String::new();
    for label in EXTERNAL_LABELS {
        write!(
            pc,
            "{label}::\n\t.string \"“{{STR_VAR_1}}”\\n\"\n\t.string \"a$\"\n\n"
        )
        .unwrap();
    }
    pc.push_str("gText_Other::\n\t.string \"unmapped zzz$\"\n");
    Fixture { ids, message, pc }
}

fn run(f: &Fixture) -> Located<Vec<u8>> {
    build(&Sources {
        charmap: CHARMAP,
        ids: &f.ids,
        message: &f.message,
        pc_transfer: &f.pc,
    })
}

fn detail(result: Located<Vec<u8>>) -> String {
    match result {
        Err((_, fault)) => fault.detail,
        Ok(_) => panic!("should be refused"),
    }
}

#[test]
fn builds_the_full_inventory_with_representative_tokens() {
    let bytes = run(&fixture()).unwrap();
    let table = BattleMessageTable::decode(&bytes).unwrap();
    for id in 12..381 {
        assert_eq!(table.string(id).unwrap(), slot_bytes(id), "{id}");
    }
    // `//` inside a literal, adjacent literals via a forward-declared symbol.
    assert_eq!(
        table
            .special_by_symbol("sText_TwoTrainersWantToBattle")
            .unwrap(),
        [0xD5, 0xBA, 0xD6, 0xFF]
    );
    assert_eq!(
        table
            .special_by_symbol("sATypeMove_Table[TYPE_DARK]")
            .unwrap(),
        [0xD5, 0xD6, 0xFF]
    );
    assert_eq!(
        table
            .special_templates(SpecialBattleMessage::UsedMove)
            .count(),
        26
    );
}

#[test]
fn rebuilding_is_deterministic_regardless_of_row_order() {
    assert_eq!(run(&fixture()).unwrap(), run(&fixture()).unwrap());
}

#[test]
fn extractor_pushes_exactly_one_entry_and_nothing_on_failure() {
    let dir = std::env::temp_dir().join(format!("pokeemerald-rs-bmsg-{}", std::process::id()));
    let f = fixture();
    for (path, text) in [
        (CHARMAP_PATH, CHARMAP),
        (IDS_PATH, f.ids.as_str()),
        (MESSAGE_PATH, f.message.as_str()),
        (PC_TRANSFER_PATH, f.pc.as_str()),
    ] {
        let target = dir.join(path);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(target, text).unwrap();
    }
    let mut writer = PackWriter::new();
    extract_battle_message(&dir, &mut writer).unwrap();
    assert_eq!(writer.len(), 1);

    std::fs::write(dir.join(MESSAGE_PATH), "").unwrap();
    let mut writer = PackWriter::new();
    assert!(matches!(
        extract_battle_message(&dir, &mut writer),
        Err(ExtractError::BattleMessageSource { .. })
    ));
    assert_eq!(writer.len(), 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn refuses_unmapped_unknown_and_malformed_strings() {
    let mut f = fixture();
    f.message = f.message.replace("\"a/b\"", "\"a~b\"");
    assert!(detail(run(&f)).contains("unmapped character"));

    let mut f = fixture();
    f.message = f.message.replace("a {PLAYER}", "a {NOPE}");
    assert!(detail(run(&f)).contains("unknown charmap constant"));

    let mut f = fixture();
    f.message = f.message.replace(
        "sText_Slot13[] = _(\"a!{PAUSE 32}\")",
        "sText_Slot13[] = _(\"a!{PAUSE\")",
    );
    assert!(detail(run(&f)).contains("unterminated"));

    // A control with no operand ends the stream before its terminator.
    let mut f = fixture();
    f.message = f.message.replace(
        "sText_Slot13[] = _(\"a!{PAUSE 32}\")",
        "sText_Slot13[] = _(\"a!{PAUSE}\")",
    );
    assert!(detail(run(&f)).contains("missing terminator"));
}

#[test]
fn refuses_table_shape_faults() {
    let row = "    [STRINGID_SLOT20 - BATTLESTRINGS_TABLE_START] = sText_Slot20,\n";

    let mut f = fixture();
    f.message = f.message.replace(row, "");
    assert!(detail(run(&f)).contains("368 slots"));

    let mut f = fixture();
    f.message = f.message.replace(row, &format!("{row}{row}"));
    assert!(detail(run(&f)).contains("repeats the slot"));

    let mut f = fixture();
    f.message = f.message.replace(
        row,
        "    [STRINGID_SLOT20 - BATTLESTRINGS_TABLE_START] = sText_Missing,\n",
    );
    assert!(detail(run(&f)).contains("`sText_Missing` has no definition"));

    let mut f = fixture();
    f.ids = f.ids.replace("STRINGID_SLOT20 20", "STRINGID_SLOT20 400");
    assert!(detail(run(&f)).contains("outside the table range"));

    let mut f = fixture();
    f.ids = f
        .ids
        .replace("BATTLESTRINGS_COUNT 381", "BATTLESTRINGS_COUNT 382");
    assert!(detail(run(&f)).contains("expected the pinned 381"));

    let mut f = fixture();
    f.message = f.message.replace("= sText_Slot20,", "sText_Slot20,");
    assert!(detail(run(&f)).contains("unexpected shape"));
}

#[test]
fn refuses_duplicate_definitions_and_missing_specials() {
    let mut f = fixture();
    f.message
        .push_str("static const u8 sText_Slot20[] = _(\"a\");\n");
    assert!(detail(run(&f)).contains("defined twice"));

    let mut f = fixture();
    f.message = f
        .message
        .replace("static const u8 sText_GoPkmn[] = _(\"b\");\n", "");
    assert!(detail(run(&f)).contains("`sText_GoPkmn` has no definition"));

    let mut f = fixture();
    f.message = f.message.replace("    [TYPE_ICE] = _(\"ab\"),\n", "");
    assert!(detail(run(&f)).contains("sATypeMove_Table[TYPE_ICE]"));

    let mut f = fixture();
    f.message = f.message.replace(
        "[TYPE_ICE] = _(\"ab\")",
        "[TYPE_ICE] = _(\"ababababababababab\")",
    );
    assert!(detail(run(&f)).contains("exceeds"));
}

#[test]
fn refuses_external_label_faults() {
    let mut f = fixture();
    f.pc =
        f.pc.replace("gText_PkmnTransferredLanettesPC::", "gText_Renamed::");
    assert!(detail(run(&f)).contains("`gText_PkmnTransferredLanettesPC` is not defined"));

    let mut f = fixture();
    f.pc.push_str("gText_PkmnTransferredLanettesPC::\n\t.string \"a$\"\n");
    assert!(detail(run(&f)).contains("defined twice"));

    let mut f = fixture();
    f.pc = f.pc.replace(".string \"a$\"", ".byte 1");
    assert!(detail(run(&f)).contains("unsupported line"));
}

#[test]
fn charmap_rejects_duplicates_and_bad_values() {
    assert!(Charmap::parse("'a' = 01\n'a' = 02\n").is_err());
    assert!(Charmap::parse("FOO = 01\nFOO = 02\n").is_err());
    assert!(Charmap::parse("'a' = 1\n").is_err());
    assert!(Charmap::parse("FOO\n").is_err());
}

#[test]
fn integer_operands_follow_preproc_sizing() {
    let map = Charmap::parse(CHARMAP).unwrap();
    let bytes = |s: &str| map.convert(s, 1).unwrap();
    assert_eq!(bytes("{PAUSE 32}"), [0xFC, 0x08, 0x20]);
    assert_eq!(bytes("{PAUSE 0x0102}"), [0xFC, 0x08, 0x02, 0x01]);
    assert_eq!(bytes("{PAUSE 300}"), [0xFC, 0x08, 0x2C, 0x01]);
    assert_eq!(bytes("{PAUSE 1W}"), [0xFC, 0x08, 1, 0, 0, 0]);
    assert_eq!(bytes("{PLAY_SE SE_FLEE}"), [0xFC, 0x10, 0x11, 0x00]);
    assert_eq!(bytes("{PLAY_BGM MUS_X}"), [0xFC, 0x0B, 0x60, 0x01]);
    assert_eq!(bytes("{B_BUFF1}{B_BUFF2}"), [0xFD, 0x00, 0xFD, 0x01]);
    assert!(map.convert("{PAUSE 0x1}", 1).is_err());
    assert!(map.convert("{}", 1).is_err());
    assert!(map.convert("\\q", 1).is_err());
}

#[test]
fn lexer_skips_comments_preprocessor_and_char_literals() {
    let tokens =
        tokenize("#define X \\\n 1\n// \"x\nint a = '\"'; /* \" */ _(\"s\\\"t\")").unwrap();
    let strings: Vec<_> = tokens
        .iter()
        .filter_map(|t| match &t.tok {
            Tok::Str(s) => Some(s.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(strings, ["s\\\"t"]);
    assert!(tokenize("\"open").is_err());
    assert!(tokenize("/* open").is_err());
}

#[test]
fn lexer_refuses_character_literals_cut_off_at_end_of_input() {
    assert!(tokenize("'").is_err());
    assert!(tokenize("int a = 'x").is_err());
    assert!(tokenize("int a = '\\").is_err());
    assert!(tokenize("int a = '\\n'").is_ok());
}

#[test]
#[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
fn battle_message_table_is_extracted_from_the_real_checkout() {
    assert!(super::super::upstream_present(), "run ./init.sh first");
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-extract-test-battle-message-{}.pack",
        std::process::id()
    ));
    super::super::extract_to(&path).expect("extraction should succeed against a real checkout");
    let pack = AssetPack::load(&path).unwrap();
    let table = pack.battle_messages().unwrap();

    assert_eq!(table.strings().count(), 369);
    for id in 12..381 {
        let stream = table.string(id).unwrap();
        assert_eq!(stream.last(), Some(&0xFF), "{id}");
    }
    for (message, count) in [
        (SpecialBattleMessage::IntroMsg, 10),
        (SpecialBattleMessage::IntroSendOut, 10),
        (SpecialBattleMessage::ReturnMon, 7),
        (SpecialBattleMessage::SwitchInMon, 8),
        (SpecialBattleMessage::UsedMove, 26),
        (SpecialBattleMessage::BattleEnd, 13),
    ] {
        assert_eq!(
            table.special_templates(message).count(),
            count,
            "{message:?}"
        );
    }

    // "{PLAY_SE SE_FLEE}Got away safely!\p": control bytes, charmap letters, \p, EOS.
    let got_away = [
        0xFC, 0x10, 0x11, 0x00, 0xC1, 0xE3, 0xE8, 0x00, 0xD5, 0xEB, 0xD5, 0xED, 0x00, 0xE7, 0xD5,
        0xDA, 0xD9, 0xE0, 0xED, 0xAB, 0xFB, 0xFF,
    ];
    assert_eq!(
        table.special_by_symbol("sText_GotAwaySafely").unwrap(),
        got_away
    );
    // The four PC-transfer slots come from the assembly file: {STR_VAR_2} first, `$` last.
    let upstream = super::super::repo_root().join("pokeemerald");
    let defines = parse_defines(&std::fs::read_to_string(upstream.join(IDS_PATH)).unwrap());
    let id = |name: &str| u16::try_from(resolve(&defines, name).unwrap()).unwrap();
    assert_eq!(id("STRINGID_PKMNTRANSFERREDSOMEONESPC"), 375);
    assert_eq!(id("STRINGID_PKMNBOXLANETTESPCFULL"), 378);
    let someone = table.string(375).unwrap();
    assert_eq!(&someone[..2], [0xFD, 0x03]);
    let box_full = table.string(378).unwrap();
    assert_eq!(&box_full[..7], [0xBC, 0xC9, 0xD2, 0x00, 0xB1, 0xFD, 0x04]);
    assert_eq!(box_full.last(), Some(&0xFF));
    // Type rows end at EOS, not at the 17-byte C row padding.
    let normal = table
        .special_by_symbol("sATypeMove_Table[TYPE_NORMAL]")
        .unwrap();
    assert_eq!(normal.len(), "a NORMAL move".len() + 1);
    // USEDMOVE's template keeps its `{B_BUFF2}` tail (FD 01) before EOS.
    let used = table.special_by_symbol("sText_AttackerUsedX").unwrap();
    assert_eq!(&used[used.len() - 3..], [0xFD, 0x01, 0xFF]);
    let _ = std::fs::remove_file(path);
}
