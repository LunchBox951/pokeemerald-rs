use super::*;
use assets::pack::AssetPack;

const CHARMAP: &str = "\
' ' = 00
'a' = D5
'd' = D8
'e' = D9
'f' = DA
'i' = DD
'n' = E2
't' = E8
'.' = AD
'…' = B0
'!' = AB
'\\l' = FA
'\\p' = FB
'\\n' = FE
STR_VAR_1 = FD 02
PLAYER = FD 01
";

const FAINTED: [u8; 14] = [
    0xFD, 0x02, 0x00, 0xDA, 0xD5, 0xDD, 0xE2, 0xE8, 0xD9, 0xD8, 0xB0, 0xFB, 0xFE, 0xFF,
];

const STRINGS: &str = "\
// don't \"confuse\" the lexer
#include \"global.h\"
extern const u8 gText_PkmnFainted_FldPsn[];
const u8 gText_Other[] = _(\"unmapped zzz\");
/* \" ' */
const u8 gText_PkmnFainted_FldPsn[] = _(\"{STR_VAR_1} fainted…\\p\\n\");
const u8 gText_After[] = _(\"a!\");
";

fn canonical(stream: &[u8]) -> Vec<u8> {
    let mut out = b"FMSG\x01\x00\x01\x00\x00\x00".to_vec();
    out.extend_from_slice(&u32::try_from(stream.len()).unwrap().to_le_bytes());
    out.extend_from_slice(stream);
    out
}

fn detail(result: Located<Vec<u8>>) -> String {
    result.expect_err("expected a source fault").1.detail
}

#[test]
fn builds_exactly_the_upstream_stream_preserving_tokens() {
    assert_eq!(build(CHARMAP, STRINGS).unwrap(), canonical(&FAINTED));
}

#[test]
fn adjacent_literals_build_the_same_payload() {
    let split = STRINGS.replace(
        "_(\"{STR_VAR_1} fainted…\\p\\n\")",
        "_(\"{STR_VAR_1} \" \"fainted…\"\n    \"\\p\\n\")",
    );
    assert_eq!(build(CHARMAP, &split).unwrap(), canonical(&FAINTED));
}

#[test]
fn ellipsis_is_one_glyph_not_three_periods() {
    let periods = STRINGS.replace('…', "...");
    assert_ne!(build(CHARMAP, &periods).unwrap(), canonical(&FAINTED));
}

#[test]
fn refuses_missing_duplicate_and_unmappable_definitions() {
    let missing = STRINGS.replace("gText_PkmnFainted_FldPsn[] = _", "gText_Renamed[] = _");
    assert!(detail(build(CHARMAP, &missing)).contains("has no definition"));

    let twice = format!("{STRINGS}const u8 gText_PkmnFainted_FldPsn[] = _(\"a\");\n");
    assert!(detail(build(CHARMAP, &twice)).contains("defined twice"));

    let unmapped = STRINGS.replace("fainted", "fa~nted");
    assert!(detail(build(CHARMAP, &unmapped)).contains("unmapped character"));

    let unknown = STRINGS.replace("{STR_VAR_1}", "{NOPE}");
    assert!(detail(build(CHARMAP, &unknown)).contains("unknown charmap constant"));

    let escape = STRINGS.replace("\\p\\n", "\\q");
    assert!(detail(build(CHARMAP, &escape)).contains("unknown escape"));

    assert!(build("'a' = 1\n", STRINGS).is_err());
    assert!(build(CHARMAP, "const u8 x[] = _(\"open").is_err());
}

#[test]
fn extractor_pushes_exactly_one_entry_and_nothing_on_failure() {
    let dir = std::env::temp_dir().join(format!("pokeemerald-rs-fmsg-{}", std::process::id()));
    for (path, text) in [(CHARMAP_PATH, CHARMAP), (STRINGS_PATH, STRINGS)] {
        let target = dir.join(path);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(target, text).unwrap();
    }
    let mut writer = PackWriter::new();
    extract_field_message(&dir, &mut writer).unwrap();
    assert_eq!(writer.len(), 1);

    std::fs::write(dir.join(STRINGS_PATH), "").unwrap();
    let mut writer = PackWriter::new();
    assert!(matches!(
        extract_field_message(&dir, &mut writer),
        Err(ExtractError::FieldMessageSource { line: 0, .. })
    ));
    assert_eq!(writer.len(), 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
#[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
fn field_message_is_extracted_from_the_real_checkout() {
    assert!(super::super::upstream_present(), "run ./init.sh first");
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-extract-test-field-message-{}.pack",
        std::process::id()
    ));
    super::super::extract_to(&path).expect("extraction should succeed against a real checkout");
    let pack = AssetPack::load(&path).unwrap();
    // `{STR_VAR_1}` ` ` `fainted` `…` `\p` `\n` EOS, spelled by `charmap.txt`.
    assert_eq!(
        pack.field_messages()
            .unwrap()
            .string(FieldMessage::PkmnFaintedFldPsn),
        [0xFD, 0x02, 0x00, 0xDA, 0xD5, 0xDD, 0xE2, 0xE8, 0xD9, 0xD8, 0xB0, 0xFB, 0xFE, 0xFF]
    );
    let _ = std::fs::remove_file(path);
}
