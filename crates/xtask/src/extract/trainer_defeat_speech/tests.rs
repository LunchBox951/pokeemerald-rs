use super::*;
use crate::extract::occupied_fixture_names::assert_claim_steps_around_occupied_names;
use crate::extract::owned_fixture::OwnedDir;
use assets::pack::AssetPack;

/// `a`..`m` stand for the texts in the fixtures below.
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
'…' = B0
'\\n' = FE
'\\l' = FA
PLAYER = FD 01
KUN = FD 05
";

const TRAINERS: &str = "\
Route103_Text_DaisyIntro:
	.string \"unmapped zzz$\"

Route103_Text_DaisyDefeated:
	.string \"a$\"

@ a comment between labels
Route103_Text_AmyDefeated:
	.string \"b\\n\"
	.string \"…$\"

Route103_Text_LivDefeated:
	.string \"c$\"
Route103_Text_AmyRematchDefeated::
	.string \"d$\"
Route103_Text_LivRematchDefeated:
	.string \"e$\"
Route103_Text_AndrewDefeated:
	.string \"f$\"
Route103_Text_MiguelDefeated:
	.string \"g$\"
Route103_Text_MiguelRematchDefeated:
	.string \"h$\"
Route103_Text_PeteDefeated:
	.string \"i$\"
Route103_Text_IsabelleDefeated:
	.string \"j$\"
Route103_Text_RhettDefeated:
	.string \"k$\"
Route103_Text_MarcosDefeated:
	.string \"l$\"
Route103_Text_MarcosPostBattle:
	.string \"unmapped zzz$\"
";

const ROUTE_103: &str = "\
Route103_EventScript_Rival::
	end

Route103_Text_MayDefeated:
	.string \"a{PLAYER}{KUN}\\l\"
	.string \"m$\"

Route103_Text_BrendanDefeated:
	.string \"{PLAYER}$\"
";

/// The streams `CHARMAP` spells for the fixtures, `EOS` included, in ID order.
fn expected() -> Vec<Vec<u8>> {
    vec![
        vec![0xC0, 0xFF],
        vec![0xC1, 0xFE, 0xB0, 0xFF],
        vec![0xC2, 0xFF],
        vec![0xC3, 0xFF],
        vec![0xC4, 0xFF],
        vec![0xC5, 0xFF],
        vec![0xC6, 0xFF],
        vec![0xC7, 0xFF],
        vec![0xC8, 0xFF],
        vec![0xC9, 0xFF],
        vec![0xCA, 0xFF],
        vec![0xCB, 0xFF],
        vec![0xC0, 0xFD, 0x01, 0xFD, 0x05, 0xFA, 0xCC, 0xFF],
        vec![0xFD, 0x01, 0xFF],
    ]
}

/// Hand-written so the tests do not rely on `encode` to check `encode`.
fn canonical(streams: &[Vec<u8>]) -> Vec<u8> {
    let mut out = b"TDFS\x01\x00".to_vec();
    out.extend_from_slice(&u16::try_from(streams.len()).unwrap().to_le_bytes());
    for (id, stream) in streams.iter().enumerate() {
        out.extend_from_slice(&u16::try_from(id).unwrap().to_le_bytes());
        out.extend_from_slice(&u32::try_from(stream.len()).unwrap().to_le_bytes());
        out.extend_from_slice(stream);
    }
    out
}

fn fault(result: Located<Vec<u8>>) -> (File, Fault) {
    result.expect_err("expected a source fault")
}

#[test]
fn builds_every_context_with_one_eos_and_the_tokens_kept() {
    // The fixtures also hold unmappable text under unselected labels
    // (`DaisyIntro`, `MarcosPostBattle`), which must not be read.
    assert_eq!(
        build(CHARMAP, TRAINERS, ROUTE_103).unwrap(),
        canonical(&expected())
    );
}

#[test]
fn a_missing_label_is_refused_naming_its_file() {
    let (file, fault_) = fault(build(
        CHARMAP,
        &TRAINERS.replace("Route103_Text_PeteDefeated", "Route103_Text_PeteRenamed"),
        ROUTE_103,
    ));
    assert_eq!(file, File::Trainers);
    assert_eq!(fault_.line, 0);
    assert!(
        fault_.detail.contains("Route103_Text_PeteDefeated"),
        "{fault_:?}"
    );

    let (file, fault_) = fault(build(
        CHARMAP,
        TRAINERS,
        &ROUTE_103.replace("Route103_Text_BrendanDefeated", "Route103_Text_Other"),
    ));
    assert_eq!(file, File::Route103);
    assert!(
        fault_.detail.contains("Route103_Text_BrendanDefeated"),
        "{fault_:?}"
    );
}

#[test]
fn a_label_defined_twice_is_refused_with_its_line() {
    let twice = format!("{TRAINERS}Route103_Text_AmyDefeated:\n\t.string \"a$\"\n");
    let (file, fault_) = fault(build(CHARMAP, &twice, ROUTE_103));
    assert_eq!(file, File::Trainers);
    assert!(fault_.detail.contains("defined twice"), "{fault_:?}");
    assert_eq!(fault_.line, twice.lines().count() - 1);
}

#[test]
fn a_selected_label_with_another_directive_is_refused() {
    let (file, fault_) = fault(build(
        CHARMAP,
        &TRAINERS.replace("\t.string \"c$\"", "\t.byte 0xFF"),
        ROUTE_103,
    ));
    assert_eq!(file, File::Trainers);
    assert!(fault_.detail.contains("unsupported line"), "{fault_:?}");
}

#[test]
fn unmappable_text_in_a_selected_label_names_its_file_and_line() {
    let (file, fault_) = fault(build(
        CHARMAP,
        TRAINERS,
        &ROUTE_103.replace("{PLAYER}$", "{NOPE}$"),
    ));
    assert_eq!(file, File::Route103);
    assert!(
        fault_.detail.contains("unknown charmap constant"),
        "{fault_:?}"
    );
    assert_eq!(fault_.line, 9);

    let (file, fault_) = fault(build(
        CHARMAP,
        &TRAINERS.replace("\"d$\"", "\"~$\""),
        ROUTE_103,
    ));
    assert_eq!(file, File::Trainers);
    assert!(fault_.detail.contains("unmapped character"), "{fault_:?}");

    assert_eq!(
        fault(build("'a' = 1\n", TRAINERS, ROUTE_103)).0,
        File::Charmap
    );
}

#[test]
fn a_text_with_no_terminator_is_refused_by_the_schema() {
    let (_, fault_) = fault(build(
        CHARMAP,
        &TRAINERS.replace("\"a$\"", "\"a\""),
        ROUTE_103,
    ));
    assert!(fault_.detail.contains("id 0"), "{fault_:?}");
}

#[test]
fn a_rival_text_with_no_terminator_names_the_script_file() {
    let (file, fault_) = fault(build(
        CHARMAP,
        TRAINERS,
        &ROUTE_103.replace("\"{PLAYER}$\"", "\"{PLAYER}\""),
    ));
    assert_eq!(file, File::Route103);
    assert!(fault_.detail.contains("id 13"), "{fault_:?}");
}

/// A fresh directory holding the three source files the extractor reads.
fn source_fixture(prefix: &str) -> OwnedDir {
    let owned = OwnedDir::create(prefix).unwrap();
    let dir = owned.path();
    for (path, text) in [
        (CHARMAP_PATH, CHARMAP),
        (TRAINERS_PATH, TRAINERS),
        (ROUTE_103_PATH, ROUTE_103),
    ] {
        let target = dir.join(path);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(target, text).unwrap();
    }
    owned
}

#[test]
fn the_source_fixture_steps_around_names_it_did_not_create() {
    let legacy = std::env::temp_dir().join(format!("pokeemerald-rs-tdfs-{}", std::process::id()));
    assert_claim_steps_around_occupied_names("pokeemerald-rs-tdfs-occupied", &legacy, || {
        source_fixture("pokeemerald-rs-tdfs-occupied")
    });
}

#[test]
fn extractor_pushes_exactly_one_entry_and_nothing_on_failure() {
    let owned = source_fixture("pokeemerald-rs-tdfs");
    let dir = owned.path();
    let mut writer = PackWriter::new();
    extract_trainer_defeat_speech(dir, &mut writer).unwrap();
    assert_eq!(writer.len(), 1);

    std::fs::write(dir.join(ROUTE_103_PATH), "").unwrap();
    let mut writer = PackWriter::new();
    assert!(matches!(
        extract_trainer_defeat_speech(dir, &mut writer),
        Err(ExtractError::TrainerDefeatSpeechSource { line: 0, ref path, .. })
            if path.ends_with(ROUTE_103_PATH)
    ));
    assert_eq!(writer.len(), 0);
}

#[test]
#[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
fn defeat_speeches_are_extracted_from_the_real_checkout() {
    assert!(super::super::upstream_present(), "run ./init.sh first");
    let owned = OwnedDir::create("pokeemerald-rs-extract-test-trainer-defeat-speech").unwrap();
    let path = owned.path().join("extract.pack");
    super::super::extract_to(&path).expect("extraction should succeed against a real checkout");
    let pack = AssetPack::load(&path).unwrap();
    let table = pack.trainer_defeat_speeches().unwrap();
    // Checked by shape, not spelled out, so no game text is committed:
    // every speech is one `EOS`-terminated stream, and the rivals keep
    // `{PLAYER}` (`FD 01`), May also `{KUN}` (`FD 05`), as placeholder tokens.
    let has = |speech: &[u8], token: [u8; 2]| speech.windows(2).any(|pair| pair == token);
    for context in TrainerDefeatSpeechContext::ALL {
        let speech = table.speech(context);
        // One `EOS`, at the end.
        assert_eq!(
            speech.iter().position(|&b| b == 0xFF),
            Some(speech.len() - 1),
            "{context:?}"
        );
    }
    let may = table.speech(TrainerDefeatSpeechContext::Route103May);
    assert!(has(may, [0xFD, 0x01]) && has(may, [0xFD, 0x05]));
    assert!(has(
        table.speech(TrainerDefeatSpeechContext::Route103Brendan),
        [0xFD, 0x01]
    ));
    // The two Amy/Liv defeat speeches and their rematches differ.
    let shared = [
        TrainerDefeatSpeechContext::Route103Amy,
        TrainerDefeatSpeechContext::Route103Liv,
        TrainerDefeatSpeechContext::Route103AmyRematch,
        TrainerDefeatSpeechContext::Route103LivRematch,
    ];
    for (i, a) in shared.iter().enumerate() {
        for b in &shared[i + 1..] {
            assert_ne!(table.speech(*a), table.speech(*b));
        }
    }
}
