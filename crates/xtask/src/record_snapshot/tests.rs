use std::path::PathBuf;

use assets::pack::AssetPack;

use super::{
    capture_loaded, default_paths, fnv1a64, git_sha, render_meta, run_with_paths,
    RecordSnapshotError, TITLE_FRAME_INDEX,
};
use crate::Scene;

const WINDOW_FRAME_GREEN_BGR555: u16 = 0x03E0;
const MAIN_MENU_BACKGROUND_BGR555: u16 = 0x4104;
const REPLACEMENT_BACKGROUND_BGR555: u16 = 0x001F;
/// [`MAIN_MENU_BACKGROUND_BGR555`] expanded by hand, independent of
/// `rendering`: `0x4104` holds 5-bit channels r=4, g=8, b=16, and
/// `to_rgb888`'s `(c << 3) | (c >> 2)` gives 33, 66, 132. Kept as literals
/// so a channel-order regression cannot be pasted over from observed output.
const MAIN_MENU_BACKGROUND_RGB888: [u8; 3] = [33, 66, 132];

#[derive(Clone, Copy)]
enum EntryKind {
    Image,
    Palette,
}

impl EntryKind {
    fn tag(self) -> u8 {
        match self {
            Self::Image => 0,
            Self::Palette => 1,
        }
    }
}

struct Entry {
    id: &'static str,
    kind: EntryKind,
    meta: Vec<u8>,
    payload: Vec<u8>,
}

fn write_synthetic_pack(mut entries: Vec<Entry>) -> Vec<u8> {
    entries.sort_by(|left, right| left.id.cmp(right.id));

    let header_size =
        assets::pack::MAGIC.len() + std::mem::size_of::<u32>() + std::mem::size_of::<u32>();
    let mut directory_size = 0usize;
    for entry in &entries {
        directory_size += std::mem::size_of::<u16>()
            + entry.id.len()
            + std::mem::size_of::<u8>()
            + std::mem::size_of::<u64>()
            + std::mem::size_of::<u64>()
            + entry.meta.len();
    }
    let mut offset = header_size + directory_size;
    let mut offsets = Vec::new();
    for entry in &entries {
        offsets.push(offset);
        offset += entry.payload.len();
    }

    let mut pack = Vec::new();
    pack.extend_from_slice(&assets::pack::MAGIC);
    pack.extend_from_slice(&assets::pack::FORMAT_VERSION.to_le_bytes());
    pack.extend_from_slice(&u32::try_from(entries.len()).unwrap().to_le_bytes());
    for (entry, &offset) in entries.iter().zip(&offsets) {
        pack.extend_from_slice(&u16::try_from(entry.id.len()).unwrap().to_le_bytes());
        pack.extend_from_slice(entry.id.as_bytes());
        pack.push(entry.kind.tag());
        pack.extend_from_slice(&u64::try_from(offset).unwrap().to_le_bytes());
        pack.extend_from_slice(&u64::try_from(entry.payload.len()).unwrap().to_le_bytes());
        pack.extend_from_slice(&entry.meta);
    }
    for entry in &entries {
        pack.extend_from_slice(&entry.payload);
    }
    pack
}

fn image_meta(width: u32, height: u32, bit_depth: u8) -> Vec<u8> {
    let mut meta = Vec::new();
    meta.extend_from_slice(&width.to_le_bytes());
    meta.extend_from_slice(&height.to_le_bytes());
    meta.push(bit_depth);
    meta
}

fn palette_meta(color_count: u16) -> Vec<u8> {
    color_count.to_le_bytes().to_vec()
}

fn synthetic_main_menu_pack_bytes() -> Vec<u8> {
    synthetic_main_menu_pack_bytes_with_background(MAIN_MENU_BACKGROUND_BGR555)
}

fn synthetic_main_menu_pack_bytes_with_background(background: u16) -> Vec<u8> {
    let frame_pixels = vec![1u8; 24 * 24];
    let mut frame_palette = vec![0u8; 32];
    frame_palette[2..4].copy_from_slice(&WINDOW_FRAME_GREEN_BGR555.to_le_bytes());

    let font_pixels =
        vec![0u8; (assets::fonts::SHEET_WIDTH * assets::fonts::SHEET_HEIGHT) as usize];

    let mut bg_palette = vec![0u8; 32];
    bg_palette[0..2].copy_from_slice(&background.to_le_bytes());

    write_synthetic_pack(vec![
        Entry {
            id: "text-window/image/1",
            kind: EntryKind::Image,
            meta: image_meta(24, 24, 4),
            payload: frame_pixels,
        },
        Entry {
            id: "text-window/palette/1",
            kind: EntryKind::Palette,
            meta: palette_meta(16),
            payload: frame_palette,
        },
        Entry {
            id: "font/normal/glyphs",
            kind: EntryKind::Image,
            meta: image_meta(assets::fonts::SHEET_WIDTH, assets::fonts::SHEET_HEIGHT, 2),
            payload: font_pixels,
        },
        Entry {
            id: "interface/palette/main_menu_bg",
            kind: EntryKind::Palette,
            meta: palette_meta(16),
            payload: bg_palette,
        },
    ])
}

fn scratch_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "pokeemerald-rs-xtask-record-snapshot-test-{}-{:?}-{name}",
        std::process::id(),
        std::thread::current().id()
    ))
}

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

fn visible_generation(output_dir: &std::path::Path, scene: Scene) -> Option<PathBuf> {
    let pointer = output_dir.join(format!("{}.generation", scene.name()));
    std::fs::read_to_string(pointer)
        .ok()
        .map(|generation| output_dir.join(generation.trim()))
}

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
    assert_eq!(fnv1a64(&[]), 0xcbf2_9ce4_8422_2325);
}

#[test]
fn fnv1a64_matches_the_published_non_empty_known_answer() {
    assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
}

#[test]
fn git_sha_of_this_checkout_is_full_length_with_an_optional_dirty_suffix() {
    let sha = git_sha(&crate::extract::repo_root()).expect("this checkout is a real git repo");
    let hex = sha.strip_suffix("-dirty").unwrap_or(&sha);
    assert_eq!(hex.len(), 40, "a git SHA-1 is 40 hex characters: {sha}");
    assert!(
        hex.chars().all(|c| c.is_ascii_hexdigit()),
        "a git SHA must be all hex digits: {sha}"
    );
}

#[test]
fn git_sha_marks_a_dirty_worktree_and_leaves_a_clean_one_bare() {
    let repo = scratch_path("dirty-marker-repo");
    let _guard = ScratchGuard(repo.clone());
    std::fs::create_dir_all(&repo).unwrap();

    let git = |args: &[&str]| {
        let ok = std::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(args)
            .output()
            .expect("git must be on PATH")
            .status
            .success();
        assert!(ok, "git {args:?} failed");
    };
    git(&["init", "--quiet"]);
    git(&["config", "user.email", "test@example.invalid"]);
    git(&["config", "user.name", "test"]);
    git(&["config", "commit.gpgsign", "false"]);
    std::fs::write(repo.join("tracked.txt"), b"one").unwrap();
    git(&["add", "tracked.txt"]);
    git(&["commit", "--quiet", "-m", "initial"]);

    let clean = git_sha(&repo).expect("a freshly committed repo has a HEAD");
    assert!(
        !clean.ends_with("-dirty"),
        "a clean worktree must report a bare SHA: {clean}"
    );

    std::fs::write(repo.join("tracked.txt"), b"two").unwrap();
    let dirty = git_sha(&repo).expect("HEAD is unchanged by an uncommitted edit");
    assert_eq!(
        dirty,
        format!("{clean}-dirty"),
        "an uncommitted change must mark the SHA dirty"
    );
}

#[test]
fn git_sha_is_none_for_a_path_with_no_git_repo() {
    let scratch = scratch_path("not-a-repo");
    std::fs::create_dir_all(&scratch).unwrap();
    let guard = ScratchGuard(scratch.clone());
    // A dangling .git file prevents discovery of an enclosing repository.
    std::fs::write(
        scratch.join(".git"),
        b"gitdir: this-path-deliberately-does-not-exist\n",
    )
    .unwrap();
    assert!(git_sha(&scratch).is_none());
    drop(guard);
}

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
    let (pack_path, _guard) = write_pack("title-against-main-menu-pack");
    let output_dir = scratch_path("title-scene-error-out");
    let err = run_with_paths(Scene::Title, &pack_path, &output_dir).unwrap_err();
    assert!(matches!(err, RecordSnapshotError::Scene(_)));
}

#[test]
fn the_title_capture_frame_has_press_start_visible() {
    assert!(
        pokeemerald_rs::title::press_start_visible(TITLE_FRAME_INDEX),
        "TITLE_FRAME_INDEX must witness the Press Start banner"
    );
}

#[test]
fn main_menu_new_game_writes_rgb_channels_in_order_and_correct_metadata() {
    let (pack_path, _pack_guard) = write_pack("main-menu-new-game-report");
    let output_dir = scratch_path("main-menu-new-game-out");
    let out_guard = ScratchGuard(output_dir.clone());

    let report = run_with_paths(Scene::MainMenuNewGame, &pack_path, &output_dir).unwrap();

    assert_eq!(report.payload_len, 240 * 160 * 3);
    let rgb_bytes = std::fs::read(&report.rgb_path).unwrap();
    assert_eq!(rgb_bytes.len(), report.payload_len);

    assert_eq!(
        &rgb_bytes[..3],
        &MAIN_MENU_BACKGROUND_RGB888,
        "the first pixel must preserve RGB channel order"
    );

    let meta = std::fs::read_to_string(&report.meta_path).unwrap();
    assert!(meta.contains("scene: main-menu-new-game\n"));
    assert!(meta.contains("width: 240\n"));
    assert!(meta.contains("height: 160\n"));
    assert!(meta.contains("pixel_format: rgb888\n"));
    assert!(meta.contains("inputs: none\n"));
    assert!(meta.contains(&format!("rgb_hash: fnv1a64:{}\n", report.rgb_hash)));
    assert!(meta.contains(&format!("pack_hash: fnv1a64:{}\n", report.pack_hash)));
    assert!(meta.contains(&format!("git_sha: {}\n", report.git_sha)));

    assert_eq!(report.rgb_hash, format!("{:016x}", fnv1a64(&rgb_bytes)));
    let pack_bytes = std::fs::read(&pack_path).unwrap();
    assert_eq!(report.pack_hash, format!("{:016x}", fnv1a64(&pack_bytes)));

    let visible = visible_generation(&output_dir, Scene::MainMenuNewGame).unwrap();
    assert_eq!(report.rgb_path.parent(), Some(visible.as_path()));
    assert_eq!(report.meta_path.parent(), Some(visible.as_path()));

    let leftovers: Vec<PathBuf> = std::fs::read_dir(&output_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with('.'))
        })
        .collect();
    assert!(
        leftovers.is_empty(),
        "staging temporaries must be renamed away, not left behind: {leftovers:?}"
    );

    drop(out_guard);
}

#[test]
fn replacing_the_pack_path_cannot_change_loaded_pack_provenance() {
    let (pack_path, _pack_guard) = write_pack("loaded-pack-provenance");
    let original_bytes = std::fs::read(&pack_path).unwrap();
    let pack = AssetPack::load(&pack_path).unwrap();

    let replacement = synthetic_main_menu_pack_bytes_with_background(REPLACEMENT_BACKGROUND_BGR555);
    assert_ne!(replacement, original_bytes);
    std::fs::write(&pack_path, &replacement).unwrap();

    let output_dir = scratch_path("loaded-pack-provenance-out");
    let out_guard = ScratchGuard(output_dir.clone());
    let report = capture_loaded(Scene::MainMenuNewGame, &pack, &output_dir, || Ok(())).unwrap();
    let rgb = std::fs::read(&report.rgb_path).unwrap();

    assert_eq!(
        report.pack_hash,
        format!("{:016x}", fnv1a64(&original_bytes))
    );
    assert_ne!(report.pack_hash, format!("{:016x}", fnv1a64(&replacement)));
    assert_eq!(
        &rgb[..3],
        &MAIN_MENU_BACKGROUND_RGB888,
        "composition and provenance must both use the retained original buffer"
    );

    drop(out_guard);
}

#[test]
fn failure_after_rgb_staging_preserves_the_visible_generation() {
    let (pack_path, _pack_guard) = write_pack("failed-generation-commit");
    let pack = AssetPack::load(&pack_path).unwrap();
    let output_dir = scratch_path("failed-generation-commit-out");
    let out_guard = ScratchGuard(output_dir.clone());

    let injected_failure = || {
        Err(RecordSnapshotError::Write(
            output_dir.join("injected-after-rgb"),
            "injected failure".to_owned(),
        ))
    };
    let err =
        capture_loaded(Scene::MainMenuNewGame, &pack, &output_dir, injected_failure).unwrap_err();
    assert!(matches!(err, RecordSnapshotError::Write(_, _)), "{err}");
    assert!(
        visible_generation(&output_dir, Scene::MainMenuNewGame).is_none(),
        "an initial failure before the commit point must publish no generation"
    );

    let previous = capture_loaded(Scene::MainMenuNewGame, &pack, &output_dir, || Ok(())).unwrap();
    let pointer_path = output_dir.join("main-menu-new-game.generation");
    let previous_pointer = std::fs::read(&pointer_path).unwrap();
    let previous_rgb = std::fs::read(&previous.rgb_path).unwrap();
    let previous_meta = std::fs::read(&previous.meta_path).unwrap();

    let err = capture_loaded(Scene::MainMenuNewGame, &pack, &output_dir, || {
        Err(RecordSnapshotError::Write(
            output_dir.join("injected-after-rgb-again"),
            "injected failure".to_owned(),
        ))
    })
    .unwrap_err();
    assert!(matches!(err, RecordSnapshotError::Write(_, _)), "{err}");
    assert_eq!(std::fs::read(&pointer_path).unwrap(), previous_pointer);
    assert_eq!(std::fs::read(&previous.rgb_path).unwrap(), previous_rgb);
    assert_eq!(std::fs::read(&previous.meta_path).unwrap(), previous_meta);
    assert!(
        std::fs::read_dir(&output_dir).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with('.')),
        "failed staging artifacts must be cleaned up"
    );

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
fn default_paths_are_derived_without_capturing_into_the_repository() {
    let repo_root = crate::extract::repo_root();
    let expected_pack = repo_root.join(crate::extract::OUTPUT_RELATIVE_PATH);
    let expected_dir = repo_root.join("snapshots");
    let (pack_path, output_dir) = default_paths(&repo_root);
    assert_eq!(pack_path, expected_pack);
    assert_eq!(output_dir, expected_dir);
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_scene_round_trips_the_capture_and_matches_a_second_run() {
    // Extraction rewrites the shared real pack non-atomically.
    let _pack = crate::extract::REAL_PACK_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
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
