use super::*;
use assets::pack::AssetPack;
use assets::TrainerClass;

/// `a`..`m` stand for the letters in the fixtures below; `{PKMN}` is the
/// `PK` and `MN` glyph pair.
const CHARMAP: &str = "\
' ' = 00
'$' = FF
'a' = C0
'b' = C1
'c' = C2
'd' = C3
'e' = C4
'f' = C5
'g' = C6
'h' = C7
'i' = C8
'j' = C9
'k' = CA
'l' = CB
'm' = CC
'é' = 1B
PKMN = 53 54
";

fn symbol(index: usize) -> String {
    format!("TRAINER_CLASS_C{index:02}")
}

/// The body each class carries: a distinct letter, so a swapped row fails.
fn body(index: usize) -> String {
    match index {
        0 | 1 | 50 | 65 => "{PKMN} ab".to_owned(),
        2 => "a é".to_owned(),
        _ => char::from(b'a' + u8::try_from(index % 13).unwrap()).to_string(),
    }
}

fn constants(order: &[usize]) -> String {
    constants_with(order, |index| index)
}

/// The header with `value` giving each class's index.
fn constants_with(order: &[usize], value: impl Fn(usize) -> usize) -> String {
    let mut out = String::from("#define RS_FACILITY_CLASS_X 0x4D\n\n");
    for &index in order {
        // Mix hexadecimal, decimal, and trailing comments as upstream does.
        let number = value(index);
        let line = match index % 3 {
            0 => format!("#define {} {number} // Unused\n", symbol(index)),
            1 => format!("#define {}  0x{number:x}\n", symbol(index)),
            _ => format!("#define {} 0x{number:X}\n", symbol(index)),
        };
        out.push_str(&line);
    }
    out.push_str("\n#define TRAINER_ENCOUNTER_MUSIC_MALE 0\n");
    out
}

fn row(index: usize) -> String {
    format!("    [{}] = _(\"{}\"),\n", symbol(index), body(index))
}

fn names(order: &[usize]) -> String {
    let mut out = String::from("const u8 gTrainerClassNames[][13] = {\n");
    for &index in order {
        out.push_str(&row(index));
    }
    out.push_str("};\n");
    out
}

fn in_order() -> Vec<usize> {
    (0..66).collect()
}

fn expected(index: usize) -> Vec<u8> {
    match index {
        0 | 1 | 50 | 65 => vec![0x53, 0x54, 0x00, 0xC0, 0xC1, 0xFF],
        2 => vec![0xC0, 0x00, 0x1B, 0xFF],
        _ => vec![0xC0 + u8::try_from(index % 13).unwrap(), 0xFF],
    }
}

/// Hand-written so the tests do not rely on `encode` to check `encode`.
fn canonical() -> Vec<u8> {
    let mut out = b"TCLN\x01\x00\x42\x00".to_vec();
    for index in 0..66 {
        let stream = expected(index);
        out.extend_from_slice(&u16::try_from(index).unwrap().to_le_bytes());
        out.extend_from_slice(&u32::try_from(stream.len()).unwrap().to_le_bytes());
        out.extend_from_slice(&stream);
    }
    out
}

fn fault(result: Located<Vec<u8>>) -> (File, Fault) {
    result.expect_err("expected a source fault")
}

fn build_with(constants: &str, names: &str) -> Located<Vec<u8>> {
    build(CHARMAP, constants, names)
}

#[test]
fn builds_every_class_with_one_eos_and_the_glyph_tokens_kept() {
    assert_eq!(
        build_with(&constants(&in_order()), &names(&in_order())).unwrap(),
        canonical()
    );
}

#[test]
fn row_and_define_order_do_not_matter_and_comments_are_skipped() {
    let mut reversed = in_order();
    reversed.reverse();
    let mut text = names(&reversed).replace("= {\n", "= {\n    /* a comment */\n    // another\n");
    text = text.replace("_(\"a\")", "_(\"a\" \"\")");
    assert_eq!(
        build_with(&constants(&reversed), &text).unwrap(),
        canonical()
    );
}

#[test]
fn adjacent_literals_concatenate() {
    let text = names(&in_order()).replace("_(\"{PKMN} ab\")", "_(\"{PKMN}\" \" ab\")");
    assert_eq!(
        build_with(&constants(&in_order()), &text).unwrap(),
        canonical()
    );
}

#[test]
fn a_missing_row_is_refused_naming_the_index() {
    let order: Vec<usize> = (0..66).filter(|&i| i != 7).collect();
    let (file, fault_) = fault(build_with(&constants(&in_order()), &names(&order)));
    assert_eq!(file, File::Names);
    assert!(fault_.detail.contains("class index 7"), "{fault_:?}");
}

#[test]
fn a_duplicate_or_unknown_row_designator_is_refused() {
    let mut order = in_order();
    order.push(3);
    let (file, fault_) = fault(build_with(&constants(&in_order()), &names(&order)));
    assert_eq!(file, File::Names);
    assert!(fault_.detail.contains("appears twice"), "{fault_:?}");

    let text = names(&in_order()).replace("TRAINER_CLASS_C09]", "TRAINER_CLASS_NOPE]");
    let (_, fault_) = fault(build_with(&constants(&in_order()), &text));
    assert!(fault_.detail.contains("not a trainer class"), "{fault_:?}");
}

#[test]
fn a_changed_added_or_repeated_class_define_is_refused() {
    let base = constants(&in_order());
    // A 67th class.
    let added = format!("{base}#define TRAINER_CLASS_NEW 0x42\n");
    let (file, fault_) = fault(build_with(&added, &names(&in_order())));
    assert_eq!(file, File::Constants);
    assert!(fault_.detail.contains("outside"), "{fault_:?}");

    // Two names for one index.
    let repeated = format!("{base}#define TRAINER_CLASS_ALIAS 5\n");
    let (_, fault_) = fault(build_with(&repeated, &names(&in_order())));
    assert!(
        fault_.detail.contains("repeats class index 5"),
        "{fault_:?}"
    );

    // A class dropped from the header.
    let kept: Vec<usize> = (0..66).filter(|&i| i != 12).collect();
    let dropped = constants(&kept);
    let (_, fault_) = fault(build_with(&dropped, &names(&in_order())));
    assert!(fault_.detail.contains("65 `TRAINER_CLASS_*`"), "{fault_:?}");

    // An expression instead of a literal.
    let expr = base.replace(
        "#define TRAINER_CLASS_C04  0x4\n",
        "#define TRAINER_CLASS_C04 (0x3 + 1)\n",
    );
    let (_, fault_) = fault(build_with(&expr, &names(&in_order())));
    assert!(fault_.detail.contains("integer literal"), "{fault_:?}");
    assert!(fault_.line > 0);
}

#[test]
fn a_swapped_define_moves_the_name_with_its_index() {
    // Swapping the header's values for C05 and C06 must swap their streams.
    let swapped = constants_with(&in_order(), |index| match index {
        5 => 6,
        6 => 5,
        other => other,
    });
    let bytes = build_with(&swapped, &names(&in_order())).unwrap();
    let table = TrainerClassNameTable::decode(&bytes).unwrap();
    // The row spelled under C05 now sits at index 6, and the reverse.
    assert_eq!(table.name(TrainerClass(6)), Some(&expected(5)[..]));
    assert_eq!(table.name(TrainerClass(5)), Some(&expected(6)[..]));
    assert_eq!(table.name(TrainerClass(7)), Some(&expected(7)[..]));
}

#[test]
fn a_changed_row_width_or_second_initializer_is_refused() {
    let wider = names(&in_order()).replace("[][13]", "[][14]");
    let (file, fault_) = fault(build_with(&constants(&in_order()), &wider));
    assert_eq!(file, File::Names);
    assert!(fault_.detail.contains("14 bytes wide"), "{fault_:?}");

    let unsized_ = names(&in_order()).replace("[][13]", "[]");
    let (_, fault_) = fault(build_with(&constants(&in_order()), &unsized_));
    assert!(fault_.detail.contains("unexpected shape"), "{fault_:?}");

    let twice = format!(
        "{}{}",
        names(&in_order()),
        "const u8 gTrainerClassNames[][13] = {};\n"
    );
    let (_, fault_) = fault(build_with(&constants(&in_order()), &twice));
    assert!(fault_.detail.contains("initialized twice"), "{fault_:?}");

    let (_, fault_) = fault(build_with(&constants(&in_order()), "int x;\n"));
    assert!(fault_.detail.contains("no initializer"), "{fault_:?}");
}

#[test]
fn malformed_rows_and_trailing_text_are_refused() {
    for (from, to, needle) in [
        (
            "_(\"d\"),\n    [TRAINER_CLASS_C04]",
            "_(\"d\")\n    [TRAINER_CLASS_C04]",
            "comma",
        ),
        (
            "[TRAINER_CLASS_C03] = _(",
            "[TRAINER_CLASS_C03] = X(",
            "unexpected shape",
        ),
        (
            "[TRAINER_CLASS_C03] = _(",
            "[TRAINER_CLASS_C03] _(",
            "unexpected shape",
        ),
        ("};\n", "};\nint tail;\n", "after the class-name table"),
        ("};\n", "}\n", "after the class-name table"),
    ] {
        let text = names(&in_order()).replace(from, to);
        assert_ne!(text, names(&in_order()), "{from}");
        let (file, fault_) = fault(build_with(&constants(&in_order()), &text));
        assert_eq!(file, File::Names, "{needle}");
        assert!(fault_.detail.contains(needle), "{needle}: {fault_:?}");
    }
    let (_, fault_) = fault(build_with(
        &constants(&in_order()),
        &names(&in_order()).replace("\"c\"", "x"),
    ));
    assert!(fault_.detail.contains("inside `_()`"), "{fault_:?}");
}

#[test]
fn preprocessor_conditionals_are_refused() {
    let text = format!("#if 0\n{}#endif\n", names(&in_order()));
    let (file, fault_) = fault(build_with(&constants(&in_order()), &text));
    assert_eq!(file, File::Names);
    assert!(fault_.detail.contains("preprocessor"), "{fault_:?}");
    assert_eq!(fault_.line, 1);
}

#[test]
fn unmappable_text_names_its_file_and_line() {
    let text = names(&in_order()).replace("{PKMN} ab", "{NOPE} ab");
    let (file, fault_) = fault(build_with(&constants(&in_order()), &text));
    assert_eq!(file, File::Names);
    assert!(
        fault_.detail.contains("unknown charmap constant"),
        "{fault_:?}"
    );
    assert_eq!(fault_.line, 2);

    let text = names(&in_order()).replace("\"c\"", "\"~\"");
    let (_, fault_) = fault(build_with(&constants(&in_order()), &text));
    assert!(fault_.detail.contains("unmapped character"), "{fault_:?}");

    assert_eq!(
        fault(build(
            "'a' = 1\n",
            &constants(&in_order()),
            &names(&in_order())
        ))
        .0,
        File::Charmap
    );
}

#[test]
fn an_encoded_name_over_the_row_is_refused_but_a_full_row_is_not() {
    // Twelve letters and the appended `EOS` fill the 13-byte row exactly.
    let full = names(&in_order()).replace("\"c\"", "\"abcdefghijkl\"");
    build_with(&constants(&in_order()), &full).unwrap();
    let over = names(&in_order()).replace("\"c\"", "\"abcdefghijklm\"");
    let (file, fault_) = fault(build_with(&constants(&in_order()), &over));
    assert_eq!(file, File::Names);
    assert!(fault_.detail.contains("exceeds its 13 bytes"), "{fault_:?}");
    assert_eq!(fault_.line, 17);
}

#[test]
fn an_explicit_terminator_in_the_text_is_refused_by_the_schema() {
    // `$` maps to `EOS` in the charmap, so the appended terminator would follow it.
    let text = names(&in_order()).replace("\"c\"", "\"c$\"");
    let (file, fault_) = fault(build_with(&constants(&in_order()), &text));
    assert_eq!(file, File::Names);
    assert!(fault_.detail.contains("terminator"), "{fault_:?}");
}

#[test]
fn extractor_pushes_exactly_one_entry_and_nothing_on_failure() {
    let dir = std::env::temp_dir().join(format!("pokeemerald-rs-tcln-{}", std::process::id()));
    for (path, text) in [
        (CHARMAP_PATH, CHARMAP.to_owned()),
        (CONSTANTS_PATH, constants(&in_order())),
        (NAMES_PATH, names(&in_order())),
    ] {
        let target = dir.join(path);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(target, text).unwrap();
    }
    let mut writer = PackWriter::new();
    extract_trainer_class_names(&dir, &mut writer).unwrap();
    assert_eq!(writer.len(), 1);

    std::fs::write(dir.join(NAMES_PATH), "").unwrap();
    let mut writer = PackWriter::new();
    assert!(matches!(
        extract_trainer_class_names(&dir, &mut writer),
        Err(ExtractError::TrainerClassNameSource { line: 0, ref path, .. })
            if path.ends_with(NAMES_PATH)
    ));
    assert_eq!(writer.len(), 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
#[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
fn class_names_are_extracted_from_the_real_checkout() {
    assert!(super::super::upstream_present(), "run ./init.sh first");
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-extract-test-trainer-class-name-{}.pack",
        std::process::id()
    ));
    super::super::extract_to(&path).expect("extraction should succeed against a real checkout");
    let pack = AssetPack::load(&path).unwrap();
    let table = pack.trainer_class_names().unwrap();
    // Spelled by hand through `charmap.txt`: `{PKMN}` is `53 54`, `é` is `1B`.
    // "{PKMN} TRAINER$"
    let rival = [
        0x53, 0x54, 0x00, 0xCE, 0xCC, 0xBB, 0xC3, 0xC8, 0xBF, 0xCC, 0xFF,
    ];
    for class in [
        TrainerClass::PKMN_TRAINER_1,
        TrainerClass::PKMN_TRAINER_2,
        TrainerClass::RIVAL,
        TrainerClass::RS_PROTAG,
    ] {
        assert_eq!(table.name(class), Some(&rival[..]), "{class:?}");
    }
    // "HIKER$"
    assert_eq!(
        table.name(TrainerClass::HIKER),
        Some(&[0xC2, 0xC3, 0xC5, 0xBF, 0xCC, 0xFF][..])
    );
    // "COOLTRAINER$" twice, as two separate classes.
    let cooltrainer = [
        0xBD, 0xC9, 0xC9, 0xC6, 0xCE, 0xCC, 0xBB, 0xC3, 0xC8, 0xBF, 0xCC, 0xFF,
    ];
    assert_eq!(
        table.name(TrainerClass::COOLTRAINER),
        Some(&cooltrainer[..])
    );
    assert_eq!(
        table.name(TrainerClass::COOLTRAINER_2),
        Some(&cooltrainer[..])
    );
    // "SWIMMER♂$" and "SWIMMER♀$" end in `B5` and `B6`; "POKéFAN$" holds `1B`.
    assert_eq!(
        table.name(TrainerClass::SWIMMER_M).unwrap()[7..],
        [0xB5, 0xFF]
    );
    assert_eq!(
        table.name(TrainerClass::SWIMMER_F).unwrap()[7..],
        [0xB6, 0xFF]
    );
    assert_eq!(table.name(TrainerClass::POKEFAN).unwrap()[3], 0x1B);
    // "PARASOL LADY$" fills its 13-byte row with no padding.
    assert_eq!(table.name(TrainerClass::PARASOL_LADY).unwrap().len(), 13);
    let _ = std::fs::remove_file(path);
}
