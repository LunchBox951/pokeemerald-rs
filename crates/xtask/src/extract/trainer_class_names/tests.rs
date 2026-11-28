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

/// The real upstream symbol of each class, so the fixture header agrees with
/// the stable [`TrainerClass`] identities.
fn symbol(index: usize) -> String {
    CLASS_SYMBOLS[index].0.to_owned()
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

    let text = names(&in_order()).replace(&format!("{}]", symbol(9)), "TRAINER_CLASS_NOPE]");
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
        "#define TRAINER_CLASS_PKMN_BREEDER  0x4\n",
        "#define TRAINER_CLASS_PKMN_BREEDER (0x3 + 1)\n",
    );
    let (_, fault_) = fault(build_with(&expr, &names(&in_order())));
    assert!(fault_.detail.contains("integer literal"), "{fault_:?}");
    assert!(fault_.line > 0);
}

#[test]
fn a_swapped_or_renamed_define_is_refused_against_the_rust_identity() {
    // Swapping LADY and BEAUTY keeps 0..66 covered once each, but `LADY`
    // would then name index 21 while `TrainerClass::LADY` stays 20.
    assert_eq!(TrainerClass::LADY.index(), 20);
    assert_eq!(TrainerClass::BEAUTY.index(), 21);
    let swapped = constants_with(&in_order(), |index| match index {
        20 => 21,
        21 => 20,
        other => other,
    });
    let (file, fault_) = fault(build_with(&swapped, &names(&in_order())));
    assert_eq!(file, File::Constants);
    assert!(
        fault_.detail.contains("`TRAINER_CLASS_LADY` is 21")
            && fault_.detail.contains("identity is 20"),
        "{fault_:?}"
    );
    assert!(fault_.line > 0);

    // A define renamed to a symbol no `TrainerClass` constant carries.
    let renamed = constants(&in_order()).replace(
        "#define TRAINER_CLASS_LASS ",
        "#define TRAINER_CLASS_LASSIE ",
    );
    let (file, fault_) = fault(build_with(&renamed, &names(&in_order())));
    assert_eq!(file, File::Constants);
    assert!(
        fault_.detail.contains("`TRAINER_CLASS_LASSIE` names no"),
        "{fault_:?}"
    );
}

#[test]
fn every_class_symbol_spells_its_rust_identity_in_index_order() {
    for (index, (symbol, class)) in CLASS_SYMBOLS.iter().enumerate() {
        assert_eq!(usize::from(class.index()), index, "{symbol}");
        assert!(symbol.starts_with(CLASS_PREFIX), "{symbol}");
    }
    assert_eq!(CLASS_SYMBOLS[20].0, "TRAINER_CLASS_LADY");
    assert_eq!(CLASS_SYMBOLS[65].0, "TRAINER_CLASS_RS_PROTAG");
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
            "_(\"d\"),\n    [TRAINER_CLASS_PKMN_BREEDER]",
            "_(\"d\")\n    [TRAINER_CLASS_PKMN_BREEDER]",
            "comma",
        ),
        (
            "[TRAINER_CLASS_TEAM_AQUA] = _(",
            "[TRAINER_CLASS_TEAM_AQUA] = X(",
            "unexpected shape",
        ),
        (
            "[TRAINER_CLASS_TEAM_AQUA] = _(",
            "[TRAINER_CLASS_TEAM_AQUA] _(",
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
    // Checked by shape, not spelled out, so no game text is committed: the
    // classes sharing a name upstream share one stream, every other class has
    // its own, the `{PKMN}` classes begin with the `PK` and `MN` glyphs
    // (`53 54`), and every stream ends in `EOS` within its 13-byte row.
    let names: Vec<&[u8]> = TrainerClass::all()
        .map(|class| table.name(class).unwrap())
        .collect();
    for group in [
        &[
            TrainerClass::PKMN_TRAINER_1,
            TrainerClass::PKMN_TRAINER_2,
            TrainerClass::RIVAL,
            TrainerClass::RS_PROTAG,
        ][..],
        &[TrainerClass::COOLTRAINER, TrainerClass::COOLTRAINER_2],
        &[TrainerClass::TUBER_F, TrainerClass::TUBER_M],
    ] {
        for class in group {
            assert_eq!(table.name(*class), table.name(group[0]), "{class:?}");
        }
    }
    let distinct: std::collections::HashSet<&[u8]> = names.iter().copied().collect();
    assert_eq!(distinct.len(), 66 - 3 - 1 - 1);
    assert_eq!(table.name(TrainerClass::RIVAL).unwrap()[..2], [0x53, 0x54]);
    for (class, name) in TrainerClass::all().zip(&names) {
        assert_eq!(name.last(), Some(&0xFF), "{class:?}");
        assert!(name.len() <= 13, "{class:?}");
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn an_octal_literal_is_refused_instead_of_read_as_decimal() {
    // C reads `010` as 8; reading it as 10 would pass the identity check on the wrong index.
    assert_eq!(super::number("010"), None);
    assert_eq!(super::number("00"), None);
    assert_eq!(super::number("0"), Some(0));
    assert_eq!(super::number("10"), Some(10));
    assert_eq!(super::number("0x0A"), Some(10));

    let octal = constants(&in_order()).replace(
        "#define TRAINER_CLASS_BLACK_BELT 12 ",
        "#define TRAINER_CLASS_BLACK_BELT 012 ",
    );
    assert_ne!(octal, constants(&in_order()));
    let (file, _) = fault(build_with(&octal, &names(&in_order())));
    assert_eq!(file, File::Constants);
}

#[test]
fn comment_prefixed_preprocessor_conditionals_are_refused() {
    for (open, close) in [
        ("/* guard */ #if 0", "/* guard */ #endif"),
        ("/* a\n b */ #if 0", "/**/#endif"),
        ("/* guard */ #if 0", "#endif"),
        ("#if 0", "/* guard */ #endif"),
    ] {
        let text = format!("{open}\n{}{close}\n", names(&in_order()));
        let (file, fault_) = fault(build_with(&constants(&in_order()), &text));
        assert_eq!(file, File::Names, "{open:?}");
        assert!(fault_.detail.contains("preprocessor"), "{fault_:?}");
        assert_eq!(fault_.line, 1 + open.matches('\n').count(), "{open:?}");
    }
}

#[test]
fn a_hash_inside_a_line_comment_is_not_a_directive() {
    let text = format!("// #if 0\n{}", names(&in_order()));
    assert!(build_with(&constants(&in_order()), &text).is_ok());
}

#[test]
fn a_directive_after_a_comment_closed_across_a_line_splice_is_refused() {
    // The second input is new behaviour: its `#` shares a physical line with
    // the `/` that closes the comment. The first the base already refused.
    for (prefix, line) in [
        ("/* comment *\\\n/\n#define _(s) \"b\"\n/* end */\n", 3),
        ("/* comment *\\\n/ #define _(s) \"b\"\n/* end */\n", 2),
    ] {
        let text = format!("{prefix}{}", names(&in_order()));
        let (file, fault_) = fault(build_with(&constants(&in_order()), &text));
        assert_eq!(file, File::Names, "{prefix:?}");
        assert!(fault_.detail.contains("preprocessor"), "{fault_:?}");
        assert_eq!(fault_.line, line, "{prefix:?}");
    }
}

#[test]
fn spliced_lines_keep_physical_line_numbers() {
    let (tokens, directive) = tokenize_with_directive("a \\\nb\n#if 0\nc").unwrap();
    let lines: Vec<_> = tokens.iter().map(|t| t.line).collect();
    assert_eq!(lines, [1, 2, 4]);
    assert_eq!(directive, Some(3));
}
