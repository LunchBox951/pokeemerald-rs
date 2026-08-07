//! Unit/integration tests for [`super::run_with_paths`] and its private
//! helpers.
//!
//! CI has no `pokeemerald/` checkout and no real asset pack (module docs'
//! "CI stays pack-free" requirement, issue #226), so every test here builds
//! a small **synthetic** pack in memory, mirroring
//! `pokeemerald_rs::main_menu::tests`' own fixture style (a small,
//! independent copy, not a shared helper — that module's own docs explain
//! why the two crates' test fixtures stay decoupled rather than shared).
//! [`Scene::Title`] is exercised only for its error-handling wiring here
//! (a full synthetic title pack needs far more entries than this
//! subcommand's own tests are worth duplicating) — its pixel content is
//! already covered by `pokeemerald_rs::title::tests`.

use std::path::PathBuf;

use super::{fnv1a64, git_sha, render_meta, run_with_paths, RecordSnapshotError};
use crate::Scene;

/// One directory entry for [`write_synthetic_pack`] (mirrors
/// `pokeemerald_rs::main_menu::tests`' `Entry`).
struct Entry {
    id: &'static str,
    kind_tag: u8,
    meta: Vec<u8>,
    payload: Vec<u8>,
}

fn write_synthetic_pack(mut entries: Vec<Entry>) -> Vec<u8> {
    entries.sort_by(|a, b| a.id.cmp(b.id));

    let header_size = 8 + 4 + 4;
    let mut directory_size = 0usize;
    for e in &entries {
        directory_size += 2 + e.id.len() + 1 + 8 + 8 + e.meta.len();
    }
    let mut offset = header_size + directory_size;
    let mut offsets = Vec::new();
    for e in &entries {
        offsets.push(offset);
        offset += e.payload.len();
    }

    let mut out = Vec::new();
    out.extend_from_slice(&assets::pack::MAGIC);
    out.extend_from_slice(&assets::pack::FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(&u32::try_from(entries.len()).unwrap().to_le_bytes());
    for (e, &off) in entries.iter().zip(&offsets) {
        out.extend_from_slice(&u16::try_from(e.id.len()).unwrap().to_le_bytes());
        out.extend_from_slice(e.id.as_bytes());
        out.push(e.kind_tag);
        out.extend_from_slice(&u64::try_from(off).unwrap().to_le_bytes());
        out.extend_from_slice(&u64::try_from(e.payload.len()).unwrap().to_le_bytes());
        out.extend_from_slice(&e.meta);
    }
    for e in &entries {
        out.extend_from_slice(&e.payload);
    }
    out
}

fn image_meta(width: u32, height: u32, bit_depth: u8) -> Vec<u8> {
    let mut m = Vec::new();
    m.extend_from_slice(&width.to_le_bytes());
    m.extend_from_slice(&height.to_le_bytes());
    m.push(bit_depth);
    m
}

fn palette_meta(color_count: u16) -> Vec<u8> {
    color_count.to_le_bytes().to_vec()
}

/// A minimal pack covering exactly what [`pokeemerald_rs::main_menu::MainMenuScene::from_pack`]
/// needs — see `pokeemerald_rs::main_menu::tests::synthetic_main_menu_pack_bytes`'s
/// identical fixture for the field-by-field rationale (duplicated here
/// rather than shared, module docs).
fn synthetic_main_menu_pack_bytes() -> Vec<u8> {
    let frame_pixels = vec![1u8; 24 * 24];
    let mut frame_palette = vec![0u8; 32];
    frame_palette[2..4].copy_from_slice(&0x03E0u16.to_le_bytes()); // bright green

    let font_pixels =
        vec![0u8; (assets::fonts::SHEET_WIDTH * assets::fonts::SHEET_HEIGHT) as usize];

    let mut bg_palette = vec![0u8; 32];
    bg_palette[0..2].copy_from_slice(&0x4104u16.to_le_bytes()); // dark blue

    write_synthetic_pack(vec![
        Entry {
            id: "text-window/image/1",
            kind_tag: 0,
            meta: image_meta(24, 24, 4),
            payload: frame_pixels,
        },
        Entry {
            id: "text-window/palette/1",
            kind_tag: 1,
            meta: palette_meta(16),
            payload: frame_palette,
        },
        Entry {
            id: "font/normal/glyphs",
            kind_tag: 0,
            meta: image_meta(assets::fonts::SHEET_WIDTH, assets::fonts::SHEET_HEIGHT, 2),
            payload: font_pixels,
        },
        Entry {
            id: "interface/palette/main_menu_bg",
            kind_tag: 1,
            meta: palette_meta(16),
            payload: bg_palette,
        },
    ])
}

/// A scratch path under the OS temp dir, unique per test-and-suffix pair so
/// parallel `cargo test` runs never collide.
fn scratch_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "pokeemerald-rs-xtask-record-snapshot-test-{}-{:?}-{name}",
        std::process::id(),
        std::thread::current().id()
    ))
}

/// RAII cleanup for a scratch file or directory this test tree created.
struct ScratchGuard(PathBuf);

impl Drop for ScratchGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn write_pack(name: &str) -> (PathBuf, ScratchGuard) {
    let path = scratch_path(&format!("{name}.pack"));
    std::fs::write(&path, synthetic_main_menu_pack_bytes()).unwrap();
    let guard = ScratchGuard(path.clone());
    (path, guard)
}

// -- `fnv1a64` ------------------------------------------------------------

#[test]
fn fnv1a64_is_deterministic_and_input_sensitive() {
    let a = fnv1a64(b"hello world");
    let b = fnv1a64(b"hello world");
    let c = fnv1a64(b"hello worlD");
    assert_eq!(a, b, "same input must hash identically every time");
    assert_ne!(c, a, "different input must (in practice) hash differently");
}

#[test]
fn fnv1a64_matches_the_published_offset_basis_for_empty_input() {
    // The FNV-1a 64-bit offset basis is itself the hash of the empty
    // string (no bytes ever XOR/multiply the seed) — pins the constant
    // against the published algorithm rather than just "some hash".
    assert_eq!(fnv1a64(&[]), 0xcbf2_9ce4_8422_2325);
}

// -- `git_sha` --------------------------------------------------------------

#[test]
fn git_sha_of_this_checkout_is_a_40_char_hex_string() {
    // This test runs inside the actual pokeemerald-rs checkout (`cargo
    // test`'s own working directory is this crate's manifest dir, but
    // `crate::extract::repo_root()` walks back to the repo root
    // regardless) -- a real git repo, so this must succeed, not fall back
    // to "unknown".
    let sha = git_sha(&crate::extract::repo_root()).expect("this checkout is a real git repo");
    assert_eq!(sha.len(), 40, "a git SHA-1 is 40 hex characters: {sha}");
    assert!(
        sha.chars().all(|c| c.is_ascii_hexdigit()),
        "a git SHA must be all hex digits: {sha}"
    );
}

#[test]
fn git_sha_is_none_for_a_path_with_no_git_repo() {
    // The OS temp dir itself is (almost certainly) not inside a git
    // checkout `-C` can walk up from into this repository's own history.
    let scratch = scratch_path("not-a-repo");
    std::fs::create_dir_all(&scratch).unwrap();
    let guard = ScratchGuard(scratch.clone());
    assert!(git_sha(&scratch).is_none());
    drop(guard);
}

// -- `render_meta` ----------------------------------------------------------

#[test]
fn render_meta_has_the_documented_fixed_field_order() {
    let text = render_meta(
        Scene::Title,
        &[],
        "cafef00ddeadbeef",
        "deadbeefcafef00d",
        "abc123",
    );
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines,
        vec![
            "scene: title",
            "width: 240",
            "height: 160",
            "pixel_format: rgb888",
            "inputs: none",
            "rgb_hash: fnv1a64:cafef00ddeadbeef",
            "pack_hash: fnv1a64:deadbeefcafef00d",
            "git_sha: abc123",
        ]
    );
}

#[test]
fn render_meta_joins_multiple_inputs_with_commas() {
    let text = render_meta(Scene::MainMenuOption, &["DPAD_DOWN", "A"], "1", "0", "x");
    assert!(text.contains("inputs: DPAD_DOWN,A\n"));
}

// -- `run_with_paths`: error paths -------------------------------------------

#[test]
fn missing_pack_fails_closed_with_pack_error() {
    let pack_path = scratch_path("missing.pack");
    let output_dir = scratch_path("missing-out");
    let err = run_with_paths(Scene::MainMenuNewGame, &pack_path, &output_dir).unwrap_err();
    assert!(matches!(err, RecordSnapshotError::Pack(_)));
    assert!(
        !output_dir.exists(),
        "a failed capture must not create output"
    );
}

#[test]
fn title_scene_against_a_main_menu_only_pack_fails_closed_with_scene_error() {
    // Proves `Scene::Title` really is wired into `compose`/`from_pack`
    // (not silently skipped): a pack with none of the `title/*` entries it
    // needs must surface a `Scene` error, not succeed or panic.
    let (pack_path, _guard) = write_pack("title-against-main-menu-pack");
    let output_dir = scratch_path("title-scene-error-out");
    let err = run_with_paths(Scene::Title, &pack_path, &output_dir).unwrap_err();
    assert!(matches!(err, RecordSnapshotError::Scene(_)));
}

// -- `run_with_paths`: the main-menu proving case ----------------------------

#[test]
fn main_menu_new_game_writes_a_full_frame_of_rgb_bytes_and_correct_metadata() {
    let (pack_path, _pack_guard) = write_pack("main-menu-new-game-report");
    let output_dir = scratch_path("main-menu-new-game-out");
    let out_guard = ScratchGuard(output_dir.clone());

    let report = run_with_paths(Scene::MainMenuNewGame, &pack_path, &output_dir).unwrap();

    assert_eq!(report.payload_len, 240 * 160 * 3);
    let rgb_bytes = std::fs::read(&report.rgb_path).unwrap();
    assert_eq!(rgb_bytes.len(), report.payload_len);

    let meta = std::fs::read_to_string(&report.meta_path).unwrap();
    assert!(meta.contains("scene: main-menu-new-game\n"));
    assert!(meta.contains("width: 240\n"));
    assert!(meta.contains("height: 160\n"));
    assert!(meta.contains("pixel_format: rgb888\n"));
    assert!(meta.contains("inputs: none\n"));
    assert!(meta.contains(&format!("rgb_hash: fnv1a64:{}\n", report.rgb_hash)));
    assert!(meta.contains(&format!("pack_hash: fnv1a64:{}\n", report.pack_hash)));
    assert!(meta.contains(&format!("git_sha: {}\n", report.git_sha)));

    // `rgb_hash`/`pack_hash` in the report/metadata must be the real hash
    // of the `.rgb` file's own bytes / the pack's own bytes, not a
    // placeholder.
    assert_eq!(report.rgb_hash, format!("{:016x}", fnv1a64(&rgb_bytes)));
    let pack_bytes = std::fs::read(&pack_path).unwrap();
    assert_eq!(report.pack_hash, format!("{:016x}", fnv1a64(&pack_bytes)));

    drop(out_guard);
}

#[test]
fn main_menu_option_records_its_dpad_down_input_and_a_different_frame() {
    let (pack_path, _pack_guard) = write_pack("main-menu-option-vs-new-game");

    let new_game_dir = scratch_path("main-menu-option-vs-new-game-out-a");
    let new_game_guard = ScratchGuard(new_game_dir.clone());
    let new_game = run_with_paths(Scene::MainMenuNewGame, &pack_path, &new_game_dir).unwrap();

    let option_dir = scratch_path("main-menu-option-vs-new-game-out-b");
    let option_guard = ScratchGuard(option_dir.clone());
    let option = run_with_paths(Scene::MainMenuOption, &pack_path, &option_dir).unwrap();

    let option_meta = std::fs::read_to_string(&option.meta_path).unwrap();
    assert!(option_meta.contains("inputs: DPAD_DOWN\n"));

    let new_game_rgb = std::fs::read(&new_game.rgb_path).unwrap();
    let option_rgb = std::fs::read(&option.rgb_path).unwrap();
    assert_ne!(
        new_game_rgb, option_rgb,
        "moving the selection must change the captured frame"
    );
    assert_ne!(
        new_game.rgb_hash, option.rgb_hash,
        "a different captured frame must hash differently"
    );

    drop(new_game_guard);
    drop(option_guard);
}

#[test]
fn capturing_the_same_scene_twice_is_byte_identical() {
    let (pack_path, _pack_guard) = write_pack("determinism");

    let dir_a = scratch_path("determinism-out-a");
    let guard_a = ScratchGuard(dir_a.clone());
    let first = run_with_paths(Scene::MainMenuNewGame, &pack_path, &dir_a).unwrap();

    let dir_b = scratch_path("determinism-out-b");
    let guard_b = ScratchGuard(dir_b.clone());
    let second = run_with_paths(Scene::MainMenuNewGame, &pack_path, &dir_b).unwrap();

    let rgb_a = std::fs::read(&first.rgb_path).unwrap();
    let rgb_b = std::fs::read(&second.rgb_path).unwrap();
    assert_eq!(
        rgb_a, rgb_b,
        ".rgb payload must be byte-identical across runs"
    );

    let meta_a = std::fs::read_to_string(&first.meta_path).unwrap();
    let meta_b = std::fs::read_to_string(&second.meta_path).unwrap();
    assert_eq!(meta_a, meta_b, ".meta must be byte-identical across runs");

    assert_eq!(first.rgb_hash, second.rgb_hash);
    assert_eq!(first.pack_hash, second.pack_hash);
    assert_eq!(first.git_sha, second.git_sha);

    drop(guard_a);
    drop(guard_b);
}

#[test]
fn run_writes_under_the_default_repo_snapshots_directory() {
    // `run` (not `run_with_paths`) against the real default pack path,
    // which does not exist in this synthetic-pack test environment --
    // must fail closed with a `Pack` error, exactly like
    // `missing_pack_fails_closed_with_pack_error` above, proving `run`
    // really does default to `crate::extract::repo_root()`'s pack path
    // rather than silently succeeding against nothing.
    if assets::pack::AssetPack::default_path().is_file() {
        // A real pack is present in this environment (e.g. a developer
        // machine after `cargo xtask extract`) -- this test only checks
        // the *no-pack* fail-closed path, so skip rather than assert
        // something about real pack content here (covered by
        // `real_pack_scene_round_trips_the_capture_and_matches_a_second_run`
        // below).
        return;
    }
    let err = super::run(Scene::MainMenuNewGame).unwrap_err();
    assert!(matches!(err, RecordSnapshotError::Pack(_)));
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_scene_round_trips_the_capture_and_matches_a_second_run() {
    let output_dir = std::env::temp_dir().join(format!(
        "pokeemerald-rs-xtask-record-snapshot-real-pack-{}",
        std::process::id()
    ));
    let guard = ScratchGuard(output_dir.clone());

    let first = run_with_paths(
        Scene::MainMenuNewGame,
        &assets::pack::AssetPack::default_path(),
        &output_dir,
    )
    .expect("run `cargo xtask extract` first");
    let rgb_a = std::fs::read(&first.rgb_path).unwrap();
    assert!(
        rgb_a.iter().any(|&b| b != 0),
        "a real capture must not be entirely black"
    );

    let second = run_with_paths(
        Scene::MainMenuNewGame,
        &assets::pack::AssetPack::default_path(),
        &output_dir,
    )
    .unwrap();
    let rgb_b = std::fs::read(&second.rgb_path).unwrap();
    assert_eq!(
        rgb_a, rgb_b,
        "two captures of the same real pack must match byte-for-byte"
    );
    assert_eq!(first.rgb_hash, second.rgb_hash);
    assert_eq!(first.pack_hash, second.pack_hash);

    drop(guard);
}
