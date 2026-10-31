use super::super::{AssetPack, PackError, SinglesHealthboxSide};
use super::shared::{
    image_meta, pack_bytes, write_pack, Entry, IMAGE_KIND_TAG, INDEXED_8_BIT, PALETTE_KIND_TAG,
    RAW_KIND_TAG, TEXT_WINDOW_BIT_DEPTH,
};

const PLAYER_ID: &str = "battle-interface/image/healthbox_singles_player";
const OPPONENT_ID: &str = "battle-interface/image/healthbox_singles_opponent";
const SHADOW_ID: &str = "battle-interface/image/enemy_mon_shadow";
const STATUS_BAR_ID: &str = "battle-interface/palette/ball_status_bar";
const BALL_DISPLAY_ID: &str = "battle-interface/palette/ball_display";
const FRAME_IDS: [&str; 4] = [PLAYER_ID, OPPONENT_ID, STATUS_BAR_ID, BALL_DISPLAY_ID];

/// Each 8x8 tile of the source raster holds its own index: `column + 3 * row`
/// modulo 16, so a reordered tile changes the byte.
fn raster(width: u32, height: u32) -> Vec<u8> {
    (0..height)
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .map(|(x, y)| u8::try_from((x / 8 + 3 * (y / 8)) % 16).unwrap())
        .collect()
}

fn image(id: &'static str, width: u32, height: u32, depth: u8) -> Entry {
    Entry {
        id,
        kind_tag: IMAGE_KIND_TAG,
        meta: image_meta(width, height, depth),
        payload: raster(width, height),
    }
}

/// Colour `n` of a palette is `n * step`.
fn palette(id: &'static str, colors: u16, step: u16) -> Entry {
    Entry {
        id,
        kind_tag: PALETTE_KIND_TAG,
        meta: colors.to_le_bytes().to_vec(),
        payload: (0..colors).flat_map(|n| (n * step).to_le_bytes()).collect(),
    }
}

fn raw(id: &'static str) -> Entry {
    Entry {
        id,
        kind_tag: RAW_KIND_TAG,
        meta: vec![],
        payload: vec![0; 8],
    }
}

fn frame_entries() -> Vec<Entry> {
    vec![
        image(PLAYER_ID, 128, 64, TEXT_WINDOW_BIT_DEPTH),
        image(OPPONENT_ID, 128, 32, TEXT_WINDOW_BIT_DEPTH),
        palette(STATUS_BAR_ID, 16, 1),
        palette(BALL_DISPLAY_ID, 16, 32),
    ]
}

fn shadow_entry() -> Entry {
    image(SHADOW_ID, 32, 8, TEXT_WINDOW_BIT_DEPTH)
}

fn load(name: &str, entries: Vec<Entry>) -> (AssetPack, std::path::PathBuf) {
    let path = write_pack(name, &pack_bytes(entries));
    (AssetPack::load(&path).unwrap(), path)
}

fn replaced(mut entries: Vec<Entry>, entry: Entry) -> Vec<Entry> {
    entries.retain(|e| e.id != entry.id);
    entries.push(entry);
    entries
}

#[test]
fn the_side_geometry_is_the_upstream_metatile_size() {
    assert_eq!(SinglesHealthboxSide::Player.metatile_tiles(), (8, 8));
    assert_eq!(SinglesHealthboxSide::Opponent.metatile_tiles(), (8, 4));
    assert_eq!(SinglesHealthboxSide::Player.image_size(), (128, 64));
    assert_eq!(SinglesHealthboxSide::Opponent.image_size(), (128, 32));
}

#[test]
fn frames_are_side_specific_and_keep_the_source_raster() {
    let (pack, path) = load("healthbox-frames", frame_entries());
    let frames = pack.singles_healthbox_frames().unwrap();

    assert_eq!(frames.player.side(), SinglesHealthboxSide::Player);
    assert_eq!(frames.player.metatile_tiles(), (8, 8));
    let player = frames.player.image();
    assert_eq!(
        (player.width, player.height, player.bit_depth),
        (128, 64, 4)
    );
    assert_eq!(player.pixels, raster(128, 64).as_slice());
    // Tile (1, 0) holds 1, (0, 1) holds 3, (15, 7) holds (15 + 21) % 16 = 4.
    assert_eq!(
        [
            player.pixels[8],
            player.pixels[8 * 128],
            player.pixels[64 * 128 - 1]
        ],
        [1, 3, 4]
    );

    assert_eq!(frames.opponent.side(), SinglesHealthboxSide::Opponent);
    assert_eq!(frames.opponent.metatile_tiles(), (8, 4));
    let opponent = frames.opponent.image();
    assert_eq!(
        (opponent.width, opponent.height, opponent.bit_depth),
        (128, 32, 4)
    );
    assert_eq!(opponent.pixels, raster(128, 32).as_slice());

    assert_eq!(frames.ball_status_bar_palette.color_count, 16);
    assert_eq!(frames.ball_display_palette.color_count, 16);
    for index in 0..16 {
        let level = u16::try_from(index).unwrap();
        assert_eq!(frames.ball_status_bar_palette.color(index), Some(level));
        assert_eq!(frames.ball_display_palette.color(index), Some(level * 32));
    }
    assert_eq!(frames.ball_status_bar_palette.color(16), None);

    let _ = std::fs::remove_file(path);
}

#[test]
fn the_shadow_is_standalone() {
    let (pack, path) = load("enemy-shadow", vec![shadow_entry()]);
    let shadow = pack.enemy_mon_shadow().unwrap();
    assert_eq!((shadow.width, shadow.height, shadow.bit_depth), (32, 8, 4));
    assert_eq!(shadow.pixels, raster(32, 8).as_slice());
    assert!(matches!(
        pack.singles_healthbox_frames().unwrap_err(),
        PackError::UnknownAsset(id) if id == PLAYER_ID
    ));
    let _ = std::fs::remove_file(path);

    let (pack, path) = load("frames-without-shadow", frame_entries());
    assert!(pack.singles_healthbox_frames().is_ok());
    assert!(matches!(
        pack.enemy_mon_shadow().unwrap_err(),
        PackError::UnknownAsset(id) if id == SHADOW_ID
    ));
    let _ = std::fs::remove_file(path);
}

#[test]
fn the_two_frames_are_not_interchangeable() {
    let swapped = [
        image(PLAYER_ID, 128, 32, TEXT_WINDOW_BIT_DEPTH),
        image(OPPONENT_ID, 128, 64, TEXT_WINDOW_BIT_DEPTH),
    ];
    for entry in swapped {
        let id = entry.id;
        let (pack, path) = load("frames-swapped", replaced(frame_entries(), entry));
        assert!(
            matches!(
                pack.singles_healthbox_frames().unwrap_err(),
                PackError::MalformedBattleInterfaceAsset { id: bad, .. } if bad == id
            ),
            "{id}"
        );
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn images_off_the_upstream_shape_are_rejected() {
    let cases: [(&str, u32, u32, u8); 9] = [
        (PLAYER_ID, 64, 128, 4),
        (PLAYER_ID, 128, 64, INDEXED_8_BIT),
        (PLAYER_ID, 64, 64, 4),
        (OPPONENT_ID, 64, 64, 4),
        (OPPONENT_ID, 128, 32, INDEXED_8_BIT),
        (OPPONENT_ID, 64, 32, 4),
        (SHADOW_ID, 8, 32, 4),
        (SHADOW_ID, 32, 8, INDEXED_8_BIT),
        (SHADOW_ID, 64, 8, 4),
    ];
    for (id, width, height, depth) in cases {
        let mut entries = frame_entries();
        entries.push(shadow_entry());
        let (pack, path) = load(
            "frames-bad-shape",
            replaced(entries, image(id, width, height, depth)),
        );
        let error = if id == SHADOW_ID {
            pack.enemy_mon_shadow().unwrap_err()
        } else {
            pack.singles_healthbox_frames().unwrap_err()
        };
        assert!(
            matches!(
                error,
                PackError::MalformedBattleInterfaceAsset { id: bad, .. } if bad == id
            ),
            "{id} {width}x{height} at {depth}-bit"
        );
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn an_index_past_the_4bpp_bank_is_rejected() {
    for (id, bad_index) in [(PLAYER_ID, 16), (OPPONENT_ID, 255), (SHADOW_ID, 16)] {
        let mut entries = frame_entries();
        entries.push(shadow_entry());
        let (width, height) = match id {
            PLAYER_ID => (128, 64),
            OPPONENT_ID => (128, 32),
            _ => (32, 8),
        };
        let mut entry = image(id, width, height, 4);
        entry.payload[5] = bad_index;
        let (pack, path) = load("frames-bad-index", replaced(entries, entry));
        let error = if id == SHADOW_ID {
            pack.enemy_mon_shadow().unwrap_err()
        } else {
            pack.singles_healthbox_frames().unwrap_err()
        };
        assert!(
            matches!(
                error,
                PackError::MalformedBattleInterfaceAsset { id: bad, .. } if bad == id
            ),
            "{id}"
        );
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn palettes_of_the_wrong_size_are_rejected() {
    for id in [STATUS_BAR_ID, BALL_DISPLAY_ID] {
        for colors in [0, 15, 17, 32] {
            let (pack, path) = load(
                "frames-bad-palette",
                replaced(frame_entries(), palette(id, colors, 1)),
            );
            assert!(
                matches!(
                    pack.singles_healthbox_frames().unwrap_err(),
                    PackError::MalformedBattleInterfaceAsset { id: bad, .. } if bad == id
                ),
                "{id} with {colors} colours"
            );
            let _ = std::fs::remove_file(path);
        }
    }
}

#[test]
fn payloads_that_contradict_their_metadata_fail_to_load() {
    let mut cases = Vec::new();
    for len in [8191, 8193] {
        let mut entry = image(PLAYER_ID, 128, 64, 4);
        entry.payload.resize(len, 0);
        cases.push(replaced(frame_entries(), entry));
    }
    for len in [31, 33] {
        let mut entry = palette(STATUS_BAR_ID, 16, 1);
        entry.payload.resize(len, 0);
        cases.push(replaced(frame_entries(), entry));
    }
    for entries in cases {
        let path = write_pack("frames-length", &pack_bytes(entries));
        assert!(matches!(AssetPack::load(&path), Err(PackError::Truncated)));
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn missing_or_wrong_kind_entries_fail_without_substitution() {
    for id in FRAME_IDS {
        let mut entries = frame_entries();
        entries.retain(|e| e.id != id);
        let (pack, path) = load("frames-missing", entries);
        assert!(matches!(
            pack.singles_healthbox_frames().unwrap_err(),
            PackError::UnknownAsset(missing) if missing == id
        ));
        let _ = std::fs::remove_file(path);

        let expected = if id.contains("/palette/") {
            "palette"
        } else {
            "image"
        };
        let (pack, path) = load("frames-wrong-kind", replaced(frame_entries(), raw(id)));
        assert!(matches!(
            pack.singles_healthbox_frames().unwrap_err(),
            PackError::WrongKind { id: bad, expected: want, actual: "raw blob" }
                if bad == id && want == expected
        ));
        let _ = std::fs::remove_file(path);
    }

    let (pack, path) = load("shadow-wrong-kind", vec![raw(SHADOW_ID)]);
    assert!(matches!(
        pack.enemy_mon_shadow().unwrap_err(),
        PackError::WrongKind { id, expected: "image", actual: "raw blob" } if id == SHADOW_ID
    ));
    let _ = std::fs::remove_file(path);
}
