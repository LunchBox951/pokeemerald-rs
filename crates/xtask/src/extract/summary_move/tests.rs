use std::fmt::Write as _;

use assets::pack::{
    ContestCategory, ContestEffectId, ContestEffectType, SummaryMoveTable, MOVE_DESCRIPTION_COUNT,
};
use assets::MoveId;

use super::*;

const CHARMAP: &str = "\
@ comment
' ' = 00
'a' = D5
'b' = D6
'.' = AD
'$' = FF
'\\\\' = AE
'\\l' = FA
'\\p' = FB
'\\n' = FE
";

/// The slot upstream leaves as an empty `_(\"\")`.
const EMPTY_SLOT: usize = 6;

struct Fixture {
    charmap: String,
    moves: String,
    contest: String,
    global: String,
    descriptions: String,
    contest_moves: String,
    contest_text: String,
    contest_strings: String,
}

impl Fixture {
    fn sources(&self) -> Sources<'_> {
        Sources {
            charmap: &self.charmap,
            moves: &self.moves,
            contest: &self.contest,
            global: &self.global,
            descriptions: &self.descriptions,
            contest_moves: &self.contest_moves,
            contest_text: &self.contest_text,
            contest_strings: &self.contest_strings,
        }
    }

    fn build(&self) -> Located<Vec<u8>> {
        build(&self.sources())
    }

    fn fault(&self) -> (File, Fault) {
        self.build()
            .expect_err("the fixture is meant to be refused")
    }
}

/// `CONTEST_EFFECT_TYPE_*` names by raw value; 7 is unassigned upstream.
const TYPE_NAMES: [(usize, &str); 8] = [
    (0, "APPEAL"),
    (1, "AVOID"),
    (2, "MON"),
    (3, "MONS"),
    (4, "WORSEN"),
    (5, "SPECIAL"),
    (6, "ORDER"),
    (8, "UNKNOWN"),
];

fn effect_type(effect: usize) -> (usize, &'static str) {
    TYPE_NAMES[effect % TYPE_NAMES.len()]
}

fn effect_name(n: usize) -> String {
    if n == 47 {
        "CONTEST_EFFECT_DONT_EXCITE_AUDIENCE".to_owned()
    } else {
        format!("CONTEST_EFFECT_E{n}")
    }
}

/// Upstream's shapes: adjacent literals, the pointer and effect tables out of
/// order, a `{0}` row, nested combo groups, unreferenced and unrelated definitions.
fn fixture() -> Fixture {
    let mut moves = String::from("#define MOVE_NONE 0\n");
    for n in 1..=354 {
        writeln!(moves, "#define MOVE_M{n} {n}").unwrap();
    }
    moves.push_str("\n#define MOVES_COUNT 355\n");

    let mut contest = String::new();
    for n in 0..48 {
        writeln!(contest, "#define {}   {n}", effect_name(n)).unwrap();
    }
    for (value, name) in TYPE_NAMES {
        writeln!(contest, "#define CONTEST_EFFECT_TYPE_{name} {value}").unwrap();
    }

    let global = "#define CONTEST_CATEGORY_COOL     0\n\
                  #define CONTEST_CATEGORY_BEAUTY   1\n\
                  #define CONTEST_CATEGORY_CUTE     2\n\
                  #define CONTEST_CATEGORY_SMART    3\n\
                  #define CONTEST_CATEGORY_TOUGH    4\n\
                  #define CONTEST_CATEGORIES_COUNT  5\n"
        .to_owned();

    let (descriptions, contest_moves, contest_text, contest_strings) = tables();
    Fixture {
        charmap: CHARMAP.to_owned(),
        moves,
        contest,
        global,
        descriptions,
        contest_moves,
        contest_text,
        contest_strings,
    }
}

/// The four table sources: descriptions, contest moves and effects, effect
/// pointers, and the assembly strings.
fn tables() -> (String, String, String, String) {
    let mut descriptions = String::from(
        "// header\nstatic const u8 sNullDescription[] = _(\n    \"\");\n\nstatic const u8 sUnreferenced[] = _(\"zz\");\n",
    );
    for n in 1..=354 {
        let body = if n - 1 == EMPTY_SLOT {
            "\"\"".to_owned()
        } else if n % 2 == 0 {
            "\"a\\n\"\n    \"b.\"".to_owned()
        } else {
            "\"ab\"".to_owned()
        };
        writeln!(
            descriptions,
            "\nstatic const u8 sM{n}Description[] = _(\n    {body});"
        )
        .unwrap();
    }
    descriptions.push_str(
        "\n// MOVE_NONE is ignored.\nconst u8 *const gMoveDescriptionPointers[MOVES_COUNT - 1] =\n{\n",
    );
    for n in (1..=354).rev() {
        let comma = if n == 1 { "" } else { "," };
        let target = if n - 1 == EMPTY_SLOT {
            "sNullDescription".to_owned()
        } else {
            format!("sM{n}Description")
        };
        writeln!(descriptions, "    [MOVE_M{n} - 1] = {target}{comma}").unwrap();
    }
    descriptions.push_str("};\n");

    let mut contest_moves = String::from(
        "const struct ContestMove gContestMoves[MOVES_COUNT] =\n{\n    [MOVE_NONE] = {0},\n",
    );
    for n in 1..=354 {
        writeln!(
            contest_moves,
            "\n    [MOVE_M{n}] =\n    {{\n        .effect = {},\n        .contestCategory = CONTEST_CATEGORY_{},\n        .comboStarterId = {},\n        .comboMoves = {{COMBO_A, COMBO_B}},\n    }},",
            effect_name(n % 47),
            ["COOL", "BEAUTY", "CUTE", "SMART", "TOUGH"][n % 5],
            n % 3,
        )
        .unwrap();
    }
    contest_moves.push_str("};\n\nconst struct ContestEffect gContestEffects[] =\n{\n");
    for n in (0..48).rev() {
        writeln!(
            contest_moves,
            "    [{}] =\n    {{\n        .effectType = CONTEST_EFFECT_TYPE_{},\n        .appeal = {},\n        .jam = {},\n    }},",
            effect_name(n),
            effect_type(n).1,
            n * 5,
            n
        )
        .unwrap();
    }
    contest_moves.push_str("};\n\nconst bool8 gComboStarterLookupTable[] = { FALSE, TRUE };\n");

    let mut contest_text = String::from("extern const u8 gText_Other[];\n");
    for n in 0..48 {
        writeln!(contest_text, "extern const u8 gText_E{n}[];").unwrap();
    }
    contest_text.push_str("\nconst u8 *const gContestEffectDescriptionPointers[] =\n{\n");
    for n in 0..48 {
        let comma = if n == 47 { "" } else { "," };
        writeln!(
            contest_text,
            "    [{}]   = gText_E{n}{comma}",
            effect_name(n)
        )
        .unwrap();
    }
    contest_text.push_str("};\n");

    let mut contest_strings = String::from("@ Contest move effect descriptions\n\n");
    for n in 0..48 {
        if n == 0 {
            contest_strings.push_str("gText_E0::\n\t.string \"a\\n\"\n\t.string \"b.$\"\n\n");
        } else if n % 2 == 0 {
            writeln!(
                contest_strings,
                "gText_E{n}::\n\t.string \"a\\n\"\n\t.string \"ba.$\"\n"
            )
            .unwrap();
        } else {
            writeln!(contest_strings, "gText_E{n}::\n\t.string \"ab$\"\n").unwrap();
        }
    }
    contest_strings.push_str("@ Unused\ngText_Other::\n\t.string \"unmapped?!$\"\n");

    (descriptions, contest_moves, contest_text, contest_strings)
}

fn table_of(bytes: &[u8]) -> SummaryMoveTable<'_> {
    SummaryMoveTable::decode(bytes).expect("the extractor writes the assets schema")
}

#[test]
fn the_fixture_extracts_to_the_expected_records() {
    let bytes = fixture().build().unwrap();
    let table = table_of(&bytes);

    // Odd moves: "ab"; even moves: "a\nb." through adjacent literals.
    assert_eq!(
        table.move_description(MoveId(1)),
        Some([0xD5, 0xD6, 0xFF].as_slice())
    );
    assert_eq!(
        table.move_description(MoveId(2)),
        Some([0xD5, 0xFE, 0xD6, 0xAD, 0xFF].as_slice())
    );
    assert_eq!(
        table.move_description(MoveId(354)),
        Some([0xD5, 0xFE, 0xD6, 0xAD, 0xFF].as_slice())
    );
    assert_eq!(table.move_description(MoveId(0)), None);
    // The empty `_("")` entry keeps its slot and the entries after it do not shift.
    let empty_move = MoveId(u16::try_from(EMPTY_SLOT + 1).unwrap());
    assert_eq!(table.move_description(empty_move), Some([0xFF].as_slice()));
    assert_eq!(
        table.move_description(MoveId(u16::try_from(EMPTY_SLOT + 2).unwrap())),
        Some([0xD5, 0xFE, 0xD6, 0xAD, 0xFF].as_slice())
    );

    let none = table.contest_move(MoveId(0)).unwrap();
    assert_eq!(
        (none.category, none.effect.index()),
        (ContestCategory::Cool, 0)
    );
    let row = table.contest_move(MoveId(3)).unwrap();
    assert_eq!(
        (row.category, row.effect.index()),
        (ContestCategory::Smart, 3)
    );
    let row = table.contest_move(MoveId(354)).unwrap();
    assert_eq!(
        (row.category, row.effect.index()),
        (ContestCategory::Tough, 25)
    );

    let effect = table.contest_effect(ContestEffectId::new(47).unwrap());
    assert_eq!(effect.effect_type, ContestEffectType::Unknown);
    // Two- and three-digit literals are read whole, not by their first digit.
    assert_eq!((effect.appeal, effect.jam), (235, 47));
    let effect = table.contest_effect(ContestEffectId::new(9).unwrap());
    assert_eq!((effect.appeal, effect.jam), (45, 9));
    // `.string` pieces concatenate and the `$` is the terminator.
    assert_eq!(
        table
            .contest_effect(ContestEffectId::new(0).unwrap())
            .description,
        [0xD5, 0xFE, 0xD6, 0xAD, 0xFF]
    );
    assert_eq!(
        table
            .contest_effect(ContestEffectId::new(2).unwrap())
            .description,
        [0xD5, 0xFE, 0xD6, 0xD5, 0xAD, 0xFF]
    );
    assert_eq!(
        table
            .contest_effect(ContestEffectId::new(1).unwrap())
            .description,
        [0xD5, 0xD6, 0xFF]
    );
    assert_eq!(table.descriptions().count(), MOVE_DESCRIPTION_COUNT);
}

#[test]
fn a_trailing_comma_and_row_order_change_nothing() {
    let expected = fixture().build().unwrap();
    let mut f = fixture();
    f.contest_text = f.contest_text.replace("gText_E47\n", "gText_E47,\n");
    f.descriptions = f
        .descriptions
        .replace("= sM1Description\n", "= sM1Description,\n");
    assert_eq!(f.build().unwrap(), expected);
}

/// Refuse the fixture, naming `file` and a message containing `needle`.
fn refuse(f: &Fixture, file: &str, needle: &str) -> Fault {
    let (found, fault) = f.fault();
    assert_eq!(found.path(), file, "{}", fault.detail);
    assert!(fault.detail.contains(needle), "{}", fault.detail);
    fault
}

#[test]
fn a_description_table_that_is_not_the_full_set_is_refused() {
    let mut f = fixture();
    f.descriptions = f
        .descriptions
        .replace("    [MOVE_M200 - 1] = sM200Description,\n", "");
    refuse(&f, DESCRIPTIONS_PATH, "move description id 199 is missing");

    let mut f = fixture();
    f.descriptions = f.descriptions.replace(
        "    [MOVE_M200 - 1] = sM200Description,\n",
        "    [MOVE_M200 - 1] = sM200Description,\n    [MOVE_M200 - 1] = sM199Description,\n",
    );
    refuse(
        &f,
        DESCRIPTIONS_PATH,
        "move description id 199 is duplicated",
    );
}

#[test]
fn the_description_table_has_no_move_none_slot() {
    let mut f = fixture();
    f.descriptions = f.descriptions.replace(
        "    [MOVE_M1 - 1] = sM1Description\n",
        "    [MOVE_M1 - 1] = sM1Description,\n    [MOVE_NONE - 1] = sM1Description\n",
    );
    refuse(&f, DESCRIPTIONS_PATH, "`MOVE_NONE - 1` is negative");
}

#[test]
fn unresolved_symbols_and_text_are_refused() {
    let mut f = fixture();
    f.descriptions = f
        .descriptions
        .replace("= sM9Description,", "= sMissingDescription,");
    refuse(
        &f,
        DESCRIPTIONS_PATH,
        "`sMissingDescription` has no definition",
    );

    let mut f = fixture();
    f.descriptions = f
        .descriptions
        .replace("[MOVE_M9 - 1]", "[MOVE_UNKNOWN - 1]");
    refuse(&f, DESCRIPTIONS_PATH, "`MOVE_UNKNOWN` is not defined");

    let mut f = fixture();
    f.descriptions = f
        .descriptions
        .replace("_(\n    \"ab\");", "_(\n    \"a?\");");
    refuse(&f, DESCRIPTIONS_PATH, "unmapped character");

    let mut f = fixture();
    f.contest_text = f.contest_text.replace("= gText_E5,", "= gText_Nowhere,");
    refuse(&f, CONTEST_TEXT_PATH, "`gText_Nowhere` has no definition");

    let mut f = fixture();
    f.contest_strings = f
        .contest_strings
        .replace("gText_E5::\n", "gText_E5:\n\t.long 1\n");
    refuse(&f, CONTEST_STRINGS_PATH, "unsupported line in `gText_E5`");
}

#[test]
fn contest_move_rows_are_read_in_full() {
    let mut f = fixture();
    f.contest_moves = f
        .contest_moves
        .replace("        .contestCategory = CONTEST_CATEGORY_SMART,\n", "");
    refuse(&f, CONTEST_MOVES_PATH, "record has no `.contestCategory`");

    let mut f = fixture();
    f.contest_moves = f.contest_moves.replacen(
        "        .comboStarterId = 2,\n",
        "        .comboStarterId = 2,\n        .effect = CONTEST_EFFECT_E1,\n",
        1,
    );
    refuse(&f, CONTEST_MOVES_PATH, "`.effect` is set twice");

    let mut f = fixture();
    f.contest_moves =
        f.contest_moves
            .replacen("        .comboStarterId", "        .comboStarter", 1);
    refuse(&f, CONTEST_MOVES_PATH, "unexpected field `.comboStarter`");

    let mut f = fixture();
    f.contest_moves = f
        .contest_moves
        .replacen("CONTEST_CATEGORY_SMART", "CONTEST_CATEGORY_NEW", 1);
    refuse(
        &f,
        CONTEST_MOVES_PATH,
        "`CONTEST_CATEGORY_NEW` is not defined",
    );

    let mut f = fixture();
    f.global = f.global.replace("TOUGH    4", "TOUGH    5");
    let bytes = f.build().expect_err("category 5 is past the schema");
    assert_eq!(bytes.0.path(), CONTEST_MOVES_PATH);
    assert!(
        bytes.1.detail.contains("contest category 5"),
        "{}",
        bytes.1.detail
    );

    let mut f = fixture();
    f.contest_moves = f.contest_moves.replacen(
        "    [MOVE_M3] =",
        "    [MOVE_M2] =\n    {\n        .effect = CONTEST_EFFECT_E1,\n        .contestCategory = CONTEST_CATEGORY_COOL,\n    },\n    [MOVE_M3] =",
        1,
    );
    refuse(&f, CONTEST_MOVES_PATH, "contest move id 2 is duplicated");
}

#[test]
fn contest_effect_rows_are_read_in_full() {
    let mut f = fixture();
    f.contest_moves = f.contest_moves.replace("        .jam = 3,\n", "");
    refuse(&f, CONTEST_MOVES_PATH, "record has no `.jam`");

    // 256 does not fit a byte and must not wrap or truncate to 25.
    let mut f = fixture();
    f.contest_moves = f
        .contest_moves
        .replacen(".appeal = 15,", ".appeal = 256,", 1);
    refuse(&f, CONTEST_MOVES_PATH, "appeal 256 does not fit");

    // A hex literal is refused rather than read as its leading `0`.
    let mut f = fixture();
    f.contest_moves = f
        .contest_moves
        .replacen(".appeal = 15,", ".appeal = 0x28,", 1);
    refuse(&f, CONTEST_MOVES_PATH, "unsupported value");

    let mut f = fixture();
    f.contest_moves = f.contest_moves.replacen(
        "    [CONTEST_EFFECT_E3] =",
        "    [CONTEST_EFFECT_E3] = {0},\n    [CONTEST_EFFECT_E3] =",
        1,
    );
    refuse(&f, CONTEST_MOVES_PATH, "cannot be `{0}`");

    // The effect type 7 is unassigned upstream.
    let mut f = fixture();
    f.contest.push_str("#define CONTEST_EFFECT_TYPE_BOGUS 7\n");
    f.contest_moves =
        f.contest_moves
            .replacen("CONTEST_EFFECT_TYPE_MONS,", "CONTEST_EFFECT_TYPE_BOGUS,", 1);
    refuse(&f, CONTEST_MOVES_PATH, "has type 7");
}

#[test]
fn upstreams_positional_combo_group_is_skipped_as_the_next_member() {
    // `[MOVE_STOMP]` writes `.comboStarterId = 0,{0, COMBO_STARTER_LEER}`.
    let expected = fixture().build().unwrap();
    let mut f = fixture();
    f.contest_moves = f.contest_moves.replacen(
        "        .comboMoves = {COMBO_A, COMBO_B},",
        "{0, COMBO_B},",
        1,
    );
    assert_eq!(f.build().unwrap(), expected);

    // After any other member the group has no meaning, so it is refused.
    let mut f = fixture();
    f.contest_moves = f.contest_moves.replacen(
        "        .comboStarterId = 1,\n        .comboMoves = {COMBO_A, COMBO_B},",
        "        .comboStarterId = 1,\n        .comboMoves = {COMBO_A, COMBO_B},\n{9},",
        1,
    );
    let (file, _) = f.fault();
    assert_eq!(file.path(), CONTEST_MOVES_PATH);
}

#[test]
fn a_description_slot_with_no_effect_row_is_refused() {
    let mut f = fixture();
    f.contest_text = f.contest_text.replace(
        "    [CONTEST_EFFECT_E1]   = gText_E1,\n",
        "    [CONTEST_EFFECT_E1]   = gText_E1,\n    [CONTEST_EFFECT_E48]   = gText_E1,\n",
    );
    f.contest.push_str("#define CONTEST_EFFECT_E48 48\n");
    refuse(
        &f,
        CONTEST_TEXT_PATH,
        "contest effect 48 has a description but no row",
    );
}

#[test]
fn pinned_counts_are_checked_against_the_headers() {
    let mut f = fixture();
    f.moves = f.moves.replace("MOVES_COUNT 355", "MOVES_COUNT 356");
    refuse(
        &f,
        MOVES_PATH,
        "`MOVES_COUNT` is 356, expected the pinned 355",
    );

    let mut f = fixture();
    f.global = f
        .global
        .replace("CATEGORIES_COUNT  5", "CATEGORIES_COUNT  6");
    refuse(&f, GLOBAL_PATH, "`CONTEST_CATEGORIES_COUNT` is 6");

    let mut f = fixture();
    f.contest = f
        .contest
        .replace("DONT_EXCITE_AUDIENCE   47", "DONT_EXCITE_AUDIENCE   48");
    refuse(
        &f,
        CONTEST_PATH,
        "`CONTEST_EFFECT_DONT_EXCITE_AUDIENCE` is 48",
    );
}

#[test]
fn a_failed_build_pushes_no_entry() {
    let dir = crate::extract::owned_fixture::OwnedDir::create("summary-move-empty")
        .expect("a scratch directory");
    let mut writer = PackWriter::new();
    let err = extract_summary_move(dir.path(), &mut writer).expect_err("no sources");
    assert!(matches!(err, ExtractError::ReadFailed(..)), "{err}");
    assert_eq!(writer.len(), 0);

    let mut f = fixture();
    f.contest_text = f.contest_text.replace("= gText_E5,", "= gText_Nowhere,");
    let path = dir.path();
    for (file, text) in [
        (File::Charmap, &f.charmap),
        (File::Moves, &f.moves),
        (File::Contest, &f.contest),
        (File::Global, &f.global),
        (File::Descriptions, &f.descriptions),
        (File::ContestMoves, &f.contest_moves),
        (File::ContestText, &f.contest_text),
        (File::ContestStrings, &f.contest_strings),
    ] {
        let target = path.join(file.path());
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(target, text).unwrap();
    }
    let err = extract_summary_move(path, &mut writer).expect_err("a missing string");
    assert!(
        matches!(&err, ExtractError::SummaryMoveSource { path, .. } if path.ends_with(CONTEST_TEXT_PATH)),
        "{err}"
    );
    assert_eq!(writer.len(), 0);
}

#[test]
#[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
fn summary_move_table_is_extracted_from_the_real_checkout() {
    assert!(super::super::upstream_present(), "run ./init.sh first");
    let upstream = super::super::repo_root().join("pokeemerald");
    let mut writer = PackWriter::new();
    extract_summary_move(&upstream, &mut writer).expect("extraction should succeed");
    assert_eq!(writer.len(), 1);

    let bytes = extract_payload(&upstream).unwrap();
    let table = table_of(&bytes);
    assert_eq!(table.descriptions().count(), 354);
    assert_eq!(table.contest_moves().count(), 355);
    assert_eq!(table.contest_effects().count(), 48);
    // Move 1 (`MOVE_POUND`, slot 0) and move 354 (`MOVE_PSYCHO_BOOST`, slot 353).
    assert_eq!(table.move_description(MoveId(0)), None);
    for (move_id, effect, category) in [(1, 0, 4), (3, 17, 4), (354, 1, 3)] {
        let row = table.contest_move(MoveId(move_id)).unwrap();
        assert_eq!(
            (row.effect.index(), row.category.raw()),
            (effect, category),
            "move {move_id}"
        );
    }
    let none = table.contest_move(MoveId(0)).unwrap();
    assert_eq!((none.effect.index(), none.category.raw()), (0, 0));
    for (id, kind, appeal, jam) in [
        (0, 0, 40, 0),
        (1, 0, 60, 0),
        (17, 3, 20, 10),
        (47, 4, 30, 0),
    ] {
        let effect = table.contest_effect(ContestEffectId::new(id).unwrap());
        assert_eq!(
            (effect.effect_type.raw(), effect.appeal, effect.jam),
            (kind, appeal, jam),
            "effect {id}"
        );
    }
    // Every referenced stream is a terminated string; nothing is empty bar the stored `EOS`.
    assert!(table.descriptions().all(|s| s.last() == Some(&0xFF)));
    assert!(table
        .contest_effects()
        .all(|e| e.description.len() > 1 && e.description.last() == Some(&0xFF)));
}
