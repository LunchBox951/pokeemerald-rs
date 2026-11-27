use super::*;
use assets::pack::AssetPack;

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
'…' = B0
'\\n' = FE
'\\l' = FA
PLAYER = FD 01
KUN = FD 05
";

/// The scripts use the real keys and invented intro labels, so a label derived
/// from a script's spelling reads the wrong text. Amy and Liv share a trainer.
/// Rematch and post-battle labels hold unmappable text and must stay unread.
const ROUTE_103: &str = "\
Route103_EventScript_Rival::
	end

Route103_EventScript_Daisy::
	trainerbattle_single TRAINER_DAISY, Synthetic_Third, Synthetic_Unused
	msgbox Synthetic_Junk, MSGBOX_AUTOCLOSE
	end

@ a comment between scripts
Route103_EventScript_Amy::
	trainerbattle_double TRAINER_AMY_AND_LIV_1, Synthetic_First, Synthetic_Unused, Synthetic_Unused, Synthetic_Callback
	goto_if_eq VAR_RESULT, TRUE, Route102_EventScript_AmyRematch
	end

Route102_EventScript_AmyRematch::
	trainerbattle_rematch_double TRAINER_AMY_AND_LIV_1, Synthetic_Junk, Synthetic_Junk, Synthetic_Junk
	end

Route103_EventScript_Liv::
	trainerbattle_double TRAINER_AMY_AND_LIV_1, Synthetic_Fifth, Synthetic_Unused, Synthetic_Unused, Synthetic_Callback
	end

Route103_EventScript_Andrew::
	trainerbattle_single TRAINER_ANDREW, Synthetic_Second, Synthetic_Unused
	end

Route103_EventScript_Miguel::
	trainerbattle_single TRAINER_MIGUEL_1, Synthetic_Seventh, Synthetic_Unused, Synthetic_Callback
	end

Route103_EventScript_Marcos::
	trainerbattle_single TRAINER_MARCOS, Synthetic_Sixth, Synthetic_Unused
	end

Route103_EventScript_Rhett::
	trainerbattle_single TRAINER_RHETT, Synthetic_Ninth, Synthetic_Unused
	end

Route103_EventScript_Pete::
	trainerbattle_single TRAINER_PETE, Synthetic_Eighth, Synthetic_Unused
	end

Route103_EventScript_Isabelle::
	trainerbattle_single TRAINER_ISABELLE, Synthetic_Fourth, Synthetic_Unused
	end
";

const TRAINERS: &str = "\
Route103_Text_DaisyIntro:
	.string \"unmapped zzz$\"

Synthetic_First:
	.string \"a{PLAYER}\\n\"
	.string \"…$\"

Synthetic_Second:
	.string \"b{KUN}\\l\"
	.string \"b$\"

@ a comment between labels
Synthetic_Third:
	.string \"c$\"
Synthetic_Fourth::
	.string \"d$\"
Synthetic_Fifth:
	.string \"e$\"
Synthetic_Sixth:
	.string \"f$\"
Synthetic_Seventh:
	.string \"g$\"
Synthetic_Eighth:
	.string \"h$\"
Synthetic_Ninth:
	.string \"i$\"
Synthetic_Junk:
	.string \"unmapped zzz$\"
";

/// The streams `CHARMAP` spells for the fixtures, `EOS` included, in
/// inventory order: Amy, Andrew, Daisy, Isabelle, Liv, Marcos, Miguel, Pete, Rhett.
fn expected() -> Vec<Vec<u8>> {
    vec![
        vec![0xC0, 0xFD, 0x01, 0xFE, 0xB0, 0xFF],
        vec![0xC1, 0xFD, 0x05, 0xFA, 0xC1, 0xFF],
        vec![0xC2, 0xFF],
        vec![0xC3, 0xFF],
        vec![0xC4, 0xFF],
        vec![0xC5, 0xFF],
        vec![0xC6, 0xFF],
        vec![0xC7, 0xFF],
        vec![0xC8, 0xFF],
    ]
}

/// Hand-written so the tests do not rely on `encode` to check `encode`.
fn canonical(streams: &[Vec<u8>]) -> Vec<u8> {
    let keys = [
        "Route103_EventScript_Amy",
        "Route103_EventScript_Andrew",
        "Route103_EventScript_Daisy",
        "Route103_EventScript_Isabelle",
        "Route103_EventScript_Liv",
        "Route103_EventScript_Marcos",
        "Route103_EventScript_Miguel",
        "Route103_EventScript_Pete",
        "Route103_EventScript_Rhett",
    ];
    let mut out = b"STIS\x01\x00".to_vec();
    out.extend_from_slice(&u16::try_from(streams.len()).unwrap().to_le_bytes());
    for (key, stream) in keys.iter().zip(streams) {
        out.extend_from_slice(&u16::try_from(key.len()).unwrap().to_le_bytes());
        out.extend_from_slice(key.as_bytes());
        out.extend_from_slice(&u32::try_from(stream.len()).unwrap().to_le_bytes());
        out.extend_from_slice(stream);
    }
    out
}

fn fault(result: Located<Vec<u8>>) -> (File, Fault) {
    result.expect_err("expected a source fault")
}

fn route_without(script: &str) -> String {
    ROUTE_103.replace(&format!("{script}::"), &format!("{script}_Renamed::"))
}

#[test]
fn builds_every_script_from_the_label_its_command_names() {
    assert_eq!(
        build(CHARMAP, TRAINERS, ROUTE_103).unwrap(),
        canonical(&expected())
    );
}

#[test]
fn two_scripts_sharing_a_trainer_keep_their_own_text() {
    let bytes = build(CHARMAP, TRAINERS, ROUTE_103).unwrap();
    let table = SightTrainerIntroSpeechTable::decode(&bytes).unwrap();
    assert_ne!(
        table.speech("Route103_EventScript_Amy"),
        table.speech("Route103_EventScript_Liv")
    );
}

#[test]
fn the_output_does_not_depend_on_definition_order() {
    let (head, tail) = ROUTE_103.split_at(ROUTE_103.find("Route103_EventScript_Andrew").unwrap());
    let (andrew, rest) = tail.split_at(tail.find("Route103_EventScript_Miguel").unwrap());
    let reordered = format!("{head}{rest}{andrew}");
    assert_ne!(reordered, ROUTE_103);
    assert_eq!(
        build(CHARMAP, TRAINERS, &reordered).unwrap(),
        canonical(&expected())
    );
    let (first, second) = TRAINERS.split_at(TRAINERS.find("Synthetic_Fifth").unwrap());
    let flipped = format!("{second}{first}");
    assert_eq!(
        build(CHARMAP, &flipped, ROUTE_103).unwrap(),
        canonical(&expected())
    );
}

#[test]
fn changing_one_scripts_operand_changes_only_its_record() {
    let changed = ROUTE_103.replace(
        "TRAINER_PETE, Synthetic_Eighth",
        "TRAINER_PETE, Synthetic_Third",
    );
    let mut streams = expected();
    streams[7] = streams[2].clone();
    assert_eq!(
        build(CHARMAP, TRAINERS, &changed).unwrap(),
        canonical(&streams)
    );
}

#[test]
fn a_script_with_no_definition_is_refused_naming_the_script() {
    for script in SIGHT_TRAINER_INTRO_SCRIPTS {
        let (file, fault_) = fault(build(CHARMAP, TRAINERS, &route_without(script)));
        assert_eq!(file, File::Route103);
        assert!(fault_.detail.contains(script), "{fault_:?}");
        assert!(fault_.detail.contains("not defined"), "{fault_:?}");
    }
}

#[test]
fn a_script_defined_twice_is_refused_with_its_line() {
    let twice = format!("{ROUTE_103}Route103_EventScript_Rhett::\n\tend\n");
    let (file, fault_) = fault(build(CHARMAP, TRAINERS, &twice));
    assert_eq!(file, File::Route103);
    assert!(fault_.detail.contains("defined twice"), "{fault_:?}");
    assert_eq!(fault_.line, twice.lines().count() - 1);
}

#[test]
fn an_unsupported_or_malformed_first_command_is_refused_with_its_line() {
    let cases = [
        (
            "trainerbattle_single TRAINER_PETE, Synthetic_Eighth, Synthetic_Unused",
            "trainerbattle_rematch TRAINER_PETE, Synthetic_Eighth, Synthetic_Unused",
        ),
        (
            "trainerbattle_single TRAINER_PETE, Synthetic_Eighth, Synthetic_Unused",
            "msgbox Synthetic_Eighth, MSGBOX_AUTOCLOSE",
        ),
        (
            "trainerbattle_single TRAINER_PETE, Synthetic_Eighth, Synthetic_Unused",
            "trainerbattle_single TRAINER_PETE, Synthetic_Eighth",
        ),
        (
            "trainerbattle_single TRAINER_PETE, Synthetic_Eighth, Synthetic_Unused",
            "trainerbattle_single TRAINER_PETE, , Synthetic_Unused",
        ),
        (
            "trainerbattle_single TRAINER_PETE, Synthetic_Eighth, Synthetic_Unused",
            "trainerbattle_single TRAINER_PETE, Synthetic Eighth, Synthetic_Unused",
        ),
        (
            "trainerbattle_double TRAINER_AMY_AND_LIV_1, Synthetic_First, Synthetic_Unused, Synthetic_Unused, Synthetic_Callback",
            "trainerbattle_double TRAINER_AMY_AND_LIV_1, Synthetic_First, Synthetic_Unused",
        ),
        (
            "trainerbattle_single TRAINER_PETE, Synthetic_Eighth, Synthetic_Unused",
            "trainerbattle_single A, B, C, D, E, F, G",
        ),
    ];
    for (from, to) in cases {
        let changed = ROUTE_103.replace(from, to);
        assert_ne!(changed, ROUTE_103, "{from}");
        let (file, fault_) = fault(build(CHARMAP, TRAINERS, &changed));
        assert_eq!(file, File::Route103, "{to}");
        assert!(fault_.line > 0, "{to}: {fault_:?}");
        assert!(
            fault_.detail.contains("Route103_EventScript_"),
            "{to}: {fault_:?}"
        );
    }
}

#[test]
fn a_script_with_no_command_is_refused() {
    let empty = ROUTE_103.replace(
        "Route103_EventScript_Rhett::\n\ttrainerbattle_single TRAINER_RHETT, Synthetic_Ninth, Synthetic_Unused\n\tend\n",
        "Route103_EventScript_Rhett::\n",
    );
    assert_ne!(empty, ROUTE_103);
    let (file, fault_) = fault(build(CHARMAP, TRAINERS, &empty));
    assert_eq!(file, File::Route103);
    assert!(
        fault_.detail.contains("Route103_EventScript_Rhett"),
        "{fault_:?}"
    );
    assert!(fault_.detail.contains("no command"), "{fault_:?}");
}

#[test]
fn a_missing_or_duplicate_text_label_is_refused_naming_its_file() {
    let (file, fault_) = fault(build(
        CHARMAP,
        &TRAINERS.replace("Synthetic_Eighth", "Synthetic_Renamed"),
        ROUTE_103,
    ));
    assert_eq!(file, File::Trainers);
    assert_eq!(fault_.line, 0);
    assert!(fault_.detail.contains("Synthetic_Eighth"), "{fault_:?}");

    let twice = format!("{TRAINERS}Synthetic_Third:\n\t.string \"a$\"\n");
    let (file, fault_) = fault(build(CHARMAP, &twice, ROUTE_103));
    assert_eq!(file, File::Trainers);
    assert!(fault_.detail.contains("defined twice"), "{fault_:?}");
    assert_eq!(fault_.line, twice.lines().count() - 1);
}

#[test]
fn text_faults_name_their_file_and_line() {
    let (file, fault_) = fault(build(
        CHARMAP,
        &TRAINERS.replace("\"c$\"", "\"~$\""),
        ROUTE_103,
    ));
    assert_eq!(file, File::Trainers);
    assert!(fault_.detail.contains("unmapped character"), "{fault_:?}");
    assert!(fault_.line > 0);

    let (file, fault_) = fault(build(
        CHARMAP,
        &TRAINERS.replace("\t.string \"c$\"", "\t.byte 0xFF"),
        ROUTE_103,
    ));
    assert_eq!(file, File::Trainers);
    assert!(fault_.detail.contains("unsupported line"), "{fault_:?}");

    assert_eq!(
        fault(build("'a' = 1\n", TRAINERS, ROUTE_103)).0,
        File::Charmap
    );
}

#[test]
fn a_text_with_no_terminator_is_refused_by_the_schema() {
    let (_, fault_) = fault(build(
        CHARMAP,
        &TRAINERS.replace("\"c$\"", "\"c\""),
        ROUTE_103,
    ));
    assert!(
        fault_.detail.contains("Route103_EventScript_Daisy"),
        "{fault_:?}"
    );
}

#[test]
fn extractor_pushes_exactly_one_entry_and_nothing_on_failure() {
    let dir = std::env::temp_dir().join(format!("pokeemerald-rs-stis-{}", std::process::id()));
    for (path, text) in [
        (CHARMAP_PATH, CHARMAP),
        (TRAINERS_PATH, TRAINERS),
        (ROUTE_103_PATH, ROUTE_103),
    ] {
        let target = dir.join(path);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(target, text).unwrap();
    }
    let mut writer = PackWriter::new();
    extract_sight_trainer_intro_speech(&dir, &mut writer).unwrap();
    assert_eq!(writer.len(), 1);

    std::fs::write(dir.join(ROUTE_103_PATH), "").unwrap();
    let mut writer = PackWriter::new();
    assert!(matches!(
        extract_sight_trainer_intro_speech(&dir, &mut writer),
        Err(ExtractError::SightTrainerIntroSpeechSource { line: 0, ref path, .. })
            if path.ends_with(ROUTE_103_PATH)
    ));
    assert_eq!(writer.len(), 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
#[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
fn intro_speeches_are_extracted_from_the_real_checkout() {
    assert!(super::super::upstream_present(), "run ./init.sh first");
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-extract-test-sight-trainer-intro-speech-{}.pack",
        std::process::id()
    ));
    super::super::extract_to(&path).expect("extraction should succeed against a real checkout");
    let pack = AssetPack::load(&path).unwrap();
    let table = pack.sight_trainer_intro_speeches().unwrap();
    // Checked by shape, so no game text is committed: one `EOS`, at the end.
    for script in SIGHT_TRAINER_INTRO_SCRIPTS {
        let speech = table.speech(script).unwrap();
        assert_eq!(
            speech.iter().position(|&b| b == 0xFF),
            Some(speech.len() - 1),
            "{script}"
        );
    }
    assert_ne!(
        table.speech("Route103_EventScript_Amy"),
        table.speech("Route103_EventScript_Liv")
    );
    let _ = std::fs::remove_file(path);
}
