use std::fmt::Write as _;
use std::path::PathBuf;

use assets::audio::{cry_id, DirectSoundSample, Sample, SampleId, CRY_TABLE_LEN};
use assets::{AssetPack, SpeciesId};
use pack_format::PackWriter;

use super::sources::parse_cry_sources;
use super::tables::parse_cry_tables;
use super::{extract_cries, resolve_cries, CRY_SOURCES_PATH, CRY_TABLES_PATH};
use crate::extract::{upstream_present, wav, ExtractError};

fn tables_text(normal: &[&str], reverse: &[&str]) -> String {
    let mut text = String::from("\t.align 2\ngCryTable::\n");
    for symbol in normal {
        writeln!(text, "\tcry {symbol}").unwrap();
    }
    text.push_str("\n\t.align 2\ngCryTable_Reverse::\n");
    for symbol in reverse {
        writeln!(text, "\tcry_reverse {symbol}").unwrap();
    }
    text
}

fn definition(symbol: &str, path: &str) -> String {
    format!("\t.align 2\n{symbol}::\n\t.incbin \"{path}\"\n\n")
}

fn cry_definition(symbol: &str, stem: &str) -> String {
    definition(
        symbol,
        &format!("sound/direct_sound_samples/cries/{stem}.bin"),
    )
}

fn table_error(text: &str) -> String {
    parse_cry_tables(text).unwrap_err().detail
}

fn source_error(text: &str) -> String {
    parse_cry_sources(text).unwrap_err().detail
}

#[test]
fn tables_keep_record_order() {
    let tables = parse_cry_tables(&tables_text(
        &["Cry_B", "Cry_A", "Cry_C"],
        &["Cry_B", "Cry_A", "Cry_C"],
    ))
    .unwrap();
    assert_eq!(tables.normal, ["Cry_B", "Cry_A", "Cry_C"]);
    assert_eq!(tables.reverse, tables.normal);
}

#[test]
fn tables_accept_comments_and_blank_lines() {
    let text = "\n\t.align 2\ngCryTable:: @ normal\n\n\tcry Cry_A @ first\ngCryTable_Reverse::\n\tcry_reverse Cry_A\n";
    assert_eq!(parse_cry_tables(text).unwrap().normal, ["Cry_A"]);
}

#[test]
fn tables_refuse_every_malformed_record() {
    let ok = tables_text(&["Cry_A"], &["Cry_A"]);
    let cases: [(String, &str); 14] = [
        (
            ok.replace("\tcry Cry_A", "\tcry_reverse Cry_A"),
            "expected a `cry` record",
        ),
        (
            ok.replace("\tcry_reverse Cry_A", "\tcry Cry_A"),
            "expected a `cry_reverse` record",
        ),
        (
            ok.replace("\tcry Cry_A", "\tcry Cry_A Cry_B"),
            "exactly one sample symbol",
        ),
        (
            ok.replace("\tcry Cry_A", "\tcry"),
            "exactly one sample symbol",
        ),
        (
            ok.replace("\tcry Cry_A", "\tcry Pokemon_A"),
            "not a `Cry_*` symbol",
        ),
        (
            ok.replace("\tcry Cry_A", "\tcry Cry_"),
            "not a `Cry_*` symbol",
        ),
        (
            ok.replace("\tcry Cry_A", "\tcry Cry_A\n\t.byte 1"),
            "expected a `cry` record",
        ),
        (format!("\tcry Cry_A\n{ok}"), "outside a cry table"),
        (
            ok.replace("gCryTable_Reverse::", "gCryTable_Other::"),
            "unexpected label",
        ),
        (format!("{ok}gCryTable::\n"), "duplicate table"),
        (
            ok.replace(
                "\tcry_reverse Cry_A",
                "\tcry_reverse Cry_A\n\tcry_reverse Cry_A",
            ),
            "duplicate symbol",
        ),
        (
            tables_text(&["Cry_A", "Cry_B"], &["Cry_A"]),
            "2 records but reverse table has 1",
        ),
        (
            tables_text(&["Cry_A", "Cry_B"], &["Cry_B", "Cry_A"]),
            "position 0",
        ),
        (
            "\t.align 2\ngCryTable_Reverse::\n\tcry_reverse Cry_A\ngCryTable::\n\tcry Cry_A\n"
                .to_owned(),
            "precedes the normal table",
        ),
    ];
    for (text, expected) in cases {
        let detail = table_error(&text);
        assert!(
            detail.contains(expected),
            "`{detail}` lacks `{expected}`:\n{text}"
        );
    }
}

#[test]
fn tables_refuse_a_missing_table() {
    let no_reverse = "gCryTable::\n\tcry Cry_A\n";
    assert!(table_error(no_reverse).contains("missing table `gCryTable_Reverse`"));
    let no_normal = "gCryTable_Reverse::\n\tcry_reverse Cry_A\n";
    assert!(table_error(no_normal).contains("precedes the normal table"));
    assert!(table_error("").contains("missing table `gCryTable`"));
}

#[test]
fn sources_bind_a_symbol_to_its_declared_path_only() {
    let text = format!(
        "{}{}{}",
        cry_definition("Cry_HoOh", "ho_oh"),
        cry_definition("Cry_Unused265", "unused_265"),
        definition("DirectSoundWaveData_x", "sound/direct_sound_samples/x.bin"),
    );
    let sources = parse_cry_sources(&text).unwrap();
    assert_eq!(sources.len(), 2);
    assert_eq!(
        sources["Cry_HoOh"].wav_path,
        "sound/direct_sound_samples/cries/ho_oh.wav"
    );
    assert_eq!(
        sources["Cry_HoOh"].sample_id,
        "audio/sample/direct-sound/cry/ho_oh"
    );
    assert_eq!(
        sources["Cry_Unused265"].sample_id,
        "audio/sample/direct-sound/cry/unused_265"
    );
}

#[test]
fn sources_refuse_duplicate_and_incomplete_definitions() {
    let one = cry_definition("Cry_A", "a");
    let unrelated = definition("Other", "sound/direct_sound_samples/o.bin");
    let cases = [
        (format!("{one}{unrelated}{one}"), "duplicate label"),
        (format!("Cry_A::\n{unrelated}"), "`Cry_A` has no `.incbin`"),
        ("Cry_A::\n".to_owned(), "`Cry_A` has no `.incbin`"),
        (
            "\t.incbin \"sound/direct_sound_samples/cries/a.bin\"\n".to_owned(),
            "without a label",
        ),
        (format!("{one}\t.word 1\n"), "unexpected statement"),
    ];
    for (text, expected) in cases {
        let detail = source_error(&text);
        assert!(detail.contains(expected), "`{detail}` lacks `{expected}`");
    }
}

#[test]
fn sources_refuse_invalid_sample_paths() {
    let bad = [
        "sound/direct_sound_samples/cries/../x.bin",
        "/sound/direct_sound_samples/cries/a.bin",
        "sound/direct_sound_samples/cries/a\\b.bin",
        "sound/direct_sound_samples/cries/sub/a.bin",
        "sound/direct_sound_samples/cries/a.wav",
        "sound/direct_sound_samples/cries/.bin",
        "sound/direct_sound_samples/cries/A.bin",
        "sound/direct_sound_samples/a.bin",
        "sound/direct_sound_samples/cries/a.bin.bin",
    ];
    for path in bad {
        let text = definition("Cry_A", path);
        assert!(parse_cry_sources(&text).is_err(), "`{path}` was accepted");
    }
    for operand in [
        "",
        " ",
        " \"a.bin\" extra",
        " 'x'",
        " \"sound/direct_sound_samples/cries/a.bin",
    ] {
        let text = format!("Cry_A::\n\t.incbin{operand}\n");
        assert!(
            parse_cry_sources(&text).is_err(),
            "operand `{operand}` was accepted"
        );
    }
}

#[test]
fn sources_ignore_other_blocks_and_cry_less_noise() {
    // A non-cry block's operand is never read as a cry path.
    let text = definition("Other", "anything at all");
    assert!(parse_cry_sources(&text).unwrap().is_empty());
}

#[test]
fn resolution_follows_table_order_and_refuses_unresolved_and_shared_samples() {
    let tables = tables_text(&["Cry_B", "Cry_A"], &["Cry_B", "Cry_A"]);
    let sources = format!(
        "{}{}",
        cry_definition("Cry_A", "a"),
        cry_definition("Cry_B", "b")
    );
    let resolved = resolve_cries(&tables, &sources).unwrap();
    let ids: Vec<_> = resolved.iter().map(|cry| cry.sample_id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "audio/sample/direct-sound/cry/b",
            "audio/sample/direct-sound/cry/a"
        ]
    );

    let missing = cry_definition("Cry_A", "a");
    let error = resolve_cries(&tables, &missing).unwrap_err().to_string();
    assert!(error.contains("`Cry_B` has no definition"), "{error}");
    assert!(error.contains(CRY_SOURCES_PATH), "{error}");

    let shared = format!(
        "{}{}",
        cry_definition("Cry_A", "a"),
        cry_definition("Cry_B", "a")
    );
    assert!(resolve_cries(&tables, &shared)
        .unwrap_err()
        .to_string()
        .contains("shares sample"));

    let error = resolve_cries("", &sources).unwrap_err().to_string();
    assert!(error.contains(CRY_TABLES_PATH), "{error}");
}

fn wav_bytes(extra: &[(&[u8; 4], Vec<u8>)], data: &[u8]) -> Vec<u8> {
    let mut fmt = Vec::new();
    for field in [1u16, 1] {
        fmt.extend_from_slice(&field.to_le_bytes());
    }
    fmt.extend_from_slice(&8000u32.to_le_bytes());
    fmt.extend_from_slice(&8000u32.to_le_bytes());
    fmt.extend_from_slice(&1u16.to_le_bytes());
    fmt.extend_from_slice(&8u16.to_le_bytes());
    let mut chunks: Vec<([u8; 4], Vec<u8>)> = vec![(*b"fmt ", fmt)];
    chunks.extend(extra.iter().map(|(id, body)| (**id, body.clone())));
    chunks.push((*b"data", data.to_vec()));
    let mut body = b"WAVE".to_vec();
    for (id, chunk) in chunks {
        body.extend_from_slice(&id);
        body.extend_from_slice(&u32::try_from(chunk.len()).unwrap().to_le_bytes());
        body.extend_from_slice(&chunk);
        if chunk.len() % 2 == 1 {
            body.push(0);
        }
    }
    let mut bytes = b"RIFF".to_vec();
    bytes.extend_from_slice(&u32::try_from(body.len()).unwrap().to_le_bytes());
    bytes.extend(body);
    bytes
}

fn looped_wav() -> Vec<u8> {
    let mut smpl = vec![0u8; 36 + 24];
    smpl[12..16].copy_from_slice(&60u32.to_le_bytes());
    smpl[28..32].copy_from_slice(&1u32.to_le_bytes());
    smpl[44..48].copy_from_slice(&1u32.to_le_bytes());
    smpl[48..52].copy_from_slice(&3u32.to_le_bytes());
    wav_bytes(
        &[(b"smpl", smpl), (b"agbl", 2u32.to_le_bytes().to_vec())],
        &[128, 129, 130, 131, 132, 133],
    )
}

fn pitched_wav() -> Vec<u8> {
    wav_bytes(
        &[(b"agbp", 1_234_567u32.to_le_bytes().to_vec())],
        &[118, 128, 138, 128, 108],
    )
}

/// Identity of a claimed directory: what cleanup compares before it deletes.
/// Unix: device and inode. Windows: the file ID `win-fs` reads through a
/// handle, which survives the creation-time reuse NTFS name tunnelling can
/// cause. Elsewhere none is available, so cleanup leaks the directory rather
/// than risk deleting a replacement.
#[cfg(unix)]
type Identity = (u64, u64);
#[cfg(windows)]
type Identity = win_fs::WindowsFileIdentity;
#[cfg(not(any(unix, windows)))]
type Identity = ();

/// The identity of the real directory (not a link) at `root`, if it has one.
fn identity_at(root: &std::path::Path) -> Option<Identity> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        let metadata = std::fs::symlink_metadata(root).ok()?;
        metadata.is_dir().then(|| (metadata.dev(), metadata.ino()))
    }
    #[cfg(windows)]
    {
        let handle = win_fs::open_for_verified_delete(root).ok()?;
        win_fs::WindowsFileIdentity::of(&handle).ok()
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = root;
        None
    }
}

/// A fixture tree this process created itself. The root is claimed with an
/// exclusive `create_dir` under an unpredictable name and is never
/// pre-deleted; cleanup deletes it only while it is still the directory that
/// was claimed.
struct Fixture {
    root: PathBuf,
    claimed: Option<Identity>,
}

/// Eight bytes a neighbour cannot predict: a `SipHash` with keys std seeds from
/// the operating system's random source.
fn unpredictable() -> u64 {
    use std::hash::{BuildHasher as _, Hasher as _};
    std::hash::RandomState::new().build_hasher().finish()
}

impl Fixture {
    /// Claims a fresh private directory under the system temp directory.
    fn claim(name: &str) -> Self {
        Self::claim_in(&std::env::temp_dir(), || {
            format!(
                "cry-fixture-{name}-{}-{:016x}",
                std::process::id(),
                unpredictable()
            )
        })
    }

    /// Claims the first candidate directory under `base` that `create_dir`
    /// creates. An occupied candidate (left by another process, or by an
    /// earlier run) is skipped and never touched.
    fn claim_in(base: &std::path::Path, mut candidate: impl FnMut() -> String) -> Self {
        loop {
            let root = base.join(candidate());
            match Self::create_private_dir(&root) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("claim {}: {error}", root.display()),
            }
            // Cleanup is armed only by this proof that the entry at `root` is
            // an empty real directory, not a symlink or a replacement.
            assert!(
                std::fs::symlink_metadata(&root).unwrap().is_dir(),
                "{} is not a directory",
                root.display()
            );
            assert!(
                std::fs::read_dir(&root).unwrap().next().is_none(),
                "{} was not empty when claimed",
                root.display()
            );
            return Self {
                claimed: identity_at(&root),
                root,
            };
        }
    }

    fn create_private_dir(path: &std::path::Path) -> std::io::Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt as _;
            std::fs::DirBuilder::new().mode(0o700).create(path)
        }
        #[cfg(not(unix))]
        std::fs::create_dir(path)
    }

    /// Deletes the claimed tree, but only while `root` is still the very
    /// directory that was claimed; a replacement is left alone.
    fn remove_owned_root(&self) {
        let Some(claimed) = self.claimed else {
            return;
        };
        #[cfg(unix)]
        if identity_at(&self.root) == Some(claimed) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::FileTypeExt as _;
            // `win-fs` owns the Windows identity check and handle-bound
            // delete. The handle verifies the root, the contents go (a link
            // is removed, never followed), and the empty root is then deleted
            // through that same handle, which re-verifies its identity.
            let Ok(handle) = win_fs::open_for_verified_delete(&self.root) else {
                return;
            };
            if win_fs::WindowsFileIdentity::of(&handle).ok() != Some(claimed) {
                return;
            }
            let Ok(entries) = std::fs::read_dir(&self.root) else {
                return;
            };
            for entry in entries.flatten() {
                let Ok(kind) = entry.file_type() else {
                    continue;
                };
                let _ = if kind.is_symlink_dir() {
                    std::fs::remove_dir(entry.path())
                } else if kind.is_dir() {
                    std::fs::remove_dir_all(entry.path())
                } else {
                    std::fs::remove_file(entry.path())
                };
            }
            let _ = win_fs::remove_through_open_handle(&handle, claimed, |_| {});
        }
        #[cfg(not(any(unix, windows)))]
        let _ = claimed;
    }

    /// 388 cries whose definitions appear in reverse table order; position 5 is a `Cry_HoOh`
    /// style symbol whose file name differs from the lowercased symbol.
    fn new(name: &str) -> Self {
        let fixture = Self::claim(name);
        Self::populate(&fixture.root);
        fixture
    }

    fn populate(root: &std::path::Path) {
        let cries = root.join("sound/direct_sound_samples/cries");
        std::fs::create_dir_all(&cries).unwrap();
        let stems: Vec<String> = (0..CRY_TABLE_LEN).map(Self::stem).collect();
        let symbols: Vec<String> = (0..CRY_TABLE_LEN).map(Self::symbol).collect();
        let normal: Vec<&str> = symbols.iter().map(String::as_str).collect();
        std::fs::write(root.join(CRY_TABLES_PATH), tables_text(&normal, &normal)).unwrap();
        let mut definitions = definition("Noise", "sound/direct_sound_samples/noise.bin");
        for index in (0..CRY_TABLE_LEN).rev() {
            definitions.push_str(&cry_definition(&symbols[index], &stems[index]));
            let bytes = match index {
                0 => looped_wav(),
                1 => pitched_wav(),
                _ => wav_bytes(&[], &[128, u8::try_from(index % 200).unwrap()]),
            };
            std::fs::write(cries.join(format!("{}.wav", stems[index])), bytes).unwrap();
        }
        std::fs::write(root.join(CRY_SOURCES_PATH), definitions).unwrap();
    }

    fn symbol(index: usize) -> String {
        if index == 5 {
            "Cry_HoOh".to_owned()
        } else {
            format!("Cry_S{index}")
        }
    }

    fn stem(index: usize) -> String {
        if index == 5 {
            "ho_oh".to_owned()
        } else {
            format!("s_{index}")
        }
    }

    fn extract(&self) -> Result<Vec<u8>, ExtractError> {
        let mut writer = PackWriter::new();
        extract_cries(&self.root, &mut writer)?;
        Ok(writer.finish().unwrap())
    }

    fn pack(&self) -> AssetPack {
        let path = self.root.join("out.pack");
        std::fs::write(&path, self.extract().unwrap()).unwrap();
        AssetPack::load(&path).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.remove_owned_root();
    }
}

#[test]
fn an_occupied_candidate_is_skipped_and_survives() {
    let base = Fixture::claim("occupied-base");
    let occupant = base.root.join("taken");
    std::fs::create_dir(&occupant).unwrap();
    std::fs::write(occupant.join("sentinel"), b"keep").unwrap();
    let mut names = ["taken", "free"].into_iter();
    let root = {
        let fixture = Fixture::claim_in(&base.root, || names.next().unwrap().to_owned());
        assert_eq!(fixture.root, base.root.join("free"));
        fixture.root.clone()
    };
    assert!(!root.exists(), "the claimed root is cleaned up");
    assert_eq!(std::fs::read(occupant.join("sentinel")).unwrap(), b"keep");
}

#[test]
fn a_root_replaced_before_drop_is_not_deleted() {
    let base = Fixture::claim("replaced-base");
    let fixture = Fixture::claim_in(&base.root, || "root".to_owned());
    let root = fixture.root.clone();
    std::fs::rename(&root, base.root.join("moved")).unwrap();
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("sentinel"), b"keep").unwrap();
    drop(fixture);
    assert_eq!(std::fs::read(root.join("sentinel")).unwrap(), b"keep");
}

#[cfg(unix)]
#[test]
fn a_root_replaced_by_a_symlink_before_drop_is_not_followed() {
    let base = Fixture::claim("symlinked-base");
    let fixture = Fixture::claim_in(&base.root, || "root".to_owned());
    let root = fixture.root.clone();
    let target = base.root.join("target");
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("sentinel"), b"keep").unwrap();
    std::fs::remove_dir(&root).unwrap();
    std::os::unix::fs::symlink(&target, &root).unwrap();
    drop(fixture);
    assert_eq!(std::fs::read(target.join("sentinel")).unwrap(), b"keep");
}

fn sample_id(stem: &str) -> SampleId {
    SampleId(format!("audio/sample/direct-sound/cry/{stem}"))
}

fn expected_payload(source: &[u8]) -> Vec<u8> {
    let decoded = wav::decode(source).unwrap();
    Sample::DirectSound(
        DirectSoundSample::new(
            decoded.base_frequency,
            decoded.loop_start,
            decoded.sample_count,
            decoded.data,
        )
        .unwrap(),
    )
    .encode()
}

#[test]
fn extraction_emits_the_table_and_every_sample_through_the_established_codec() {
    let fixture = Fixture::new("valid");
    let pack = fixture.pack();

    let table = pack.cry_table().unwrap();
    assert_eq!(table.samples().len(), CRY_TABLE_LEN);
    assert_eq!(table.samples()[5], sample_id("ho_oh"));
    assert_eq!(table.samples()[6], sample_id("s_6"));
    assert_eq!(pack.entries().count(), CRY_TABLE_LEN + 1);

    let cries = fixture.root.join("sound/direct_sound_samples/cries");
    for (position, id) in table.samples().iter().enumerate() {
        let source = std::fs::read(cries.join(format!("{}.wav", Fixture::stem(position)))).unwrap();
        assert_eq!(
            pack.raw(&id.0).unwrap(),
            expected_payload(&source),
            "{id:?}"
        );
    }

    let looped = pack.sample(&sample_id("s_0")).unwrap();
    let Sample::DirectSound(looped) = looped else {
        panic!("not DirectSound")
    };
    assert_eq!(
        (looped.sample_count(), looped.loop_start(), looped.data()),
        (2, Some(1), &[0, 1, 2][..])
    );
    let Sample::DirectSound(pitched) = pack.sample(&sample_id("s_1")).unwrap() else {
        panic!("not DirectSound")
    };
    assert_eq!(
        (pitched.base_frequency, pitched.loop_start()),
        (1_234_567, None)
    );
}

#[test]
fn extraction_fails_closed_without_emitting_a_partial_pack() {
    let fixture = Fixture::new("short");
    let tables = fixture.root.join(CRY_TABLES_PATH);
    let text = std::fs::read_to_string(&tables).unwrap();
    // Drop the last record of each table: 387 is not the schema's table length.
    let text = text
        .replace("\tcry Cry_S387\n", "")
        .replace("\tcry_reverse Cry_S387\n", "");
    std::fs::write(&tables, text).unwrap();
    let error = fixture.extract().unwrap_err().to_string();
    assert!(error.contains("388"), "{error}");

    let fixture = Fixture::new("no-wav");
    std::fs::remove_file(
        fixture
            .root
            .join("sound/direct_sound_samples/cries/s_9.wav"),
    )
    .unwrap();
    assert!(matches!(
        fixture.extract(),
        Err(ExtractError::ReadFailed(..))
    ));
}

fn checkout_root() -> PathBuf {
    crate::extract::repo_root().join("pokeemerald")
}

#[test]
#[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
fn checkout_cries_cover_the_388_entry_closure() {
    assert!(upstream_present(), "run ./init.sh first");
    let upstream = checkout_root();
    let read = |path: &str| std::fs::read_to_string(upstream.join(path)).unwrap();
    let tables = parse_cry_tables(&read(CRY_TABLES_PATH)).unwrap();
    assert_eq!(tables.normal.len(), 388);
    assert_eq!(tables.reverse.len(), 388);
    assert_eq!(tables.normal, tables.reverse);

    let mut writer = PackWriter::new();
    extract_cries(&upstream, &mut writer).unwrap();
    let path = std::env::temp_dir().join(format!("cry-checkout-{}.pack", std::process::id()));
    std::fs::write(&path, writer.finish().unwrap()).unwrap();
    let pack = AssetPack::load(&path).unwrap();
    let _ = std::fs::remove_file(&path);

    let table = pack.cry_table().unwrap();
    let sample_entries = pack
        .entries()
        .filter(|entry| entry.id.starts_with("audio/sample/direct-sound/cry/"))
        .count();
    assert_eq!(sample_entries, 388);
    for id in table.samples() {
        pack.sample(id).unwrap();
    }
    let distinct: std::collections::BTreeSet<_> = table.samples().iter().collect();
    assert_eq!(distinct.len(), 388);

    let sources = parse_cry_sources(&read(CRY_SOURCES_PATH)).unwrap();
    for (species, symbol, wav_stem) in [
        (SpeciesId::GROUDON, "Cry_Groudon", "groudon"),
        (SpeciesId::KYOGRE, "Cry_Kyogre", "kyogre"),
        (SpeciesId::TREECKO, "Cry_Treecko", "treecko"),
        (SpeciesId::TORCHIC, "Cry_Torchic", "torchic"),
        (SpeciesId::MUDKIP, "Cry_Mudkip", "mudkip"),
        (SpeciesId::POOCHYENA, "Cry_Poochyena", "poochyena"),
        (SpeciesId::ZIGZAGOON, "Cry_Zigzagoon", "zigzagoon"),
    ] {
        let position = usize::from(cry_id(species).unwrap().index());
        assert_eq!(tables.normal[position], symbol);
        assert_eq!(table.samples()[position], sample_id(wav_stem));
        let source = std::fs::read(upstream.join(&sources[symbol].wav_path)).unwrap();
        assert_eq!(
            pack.raw(&table.samples()[position].0).unwrap(),
            expected_payload(&source)
        );
    }
}
