use super::super::{AssetPack, BattlerDirection, PackError};
use super::shared::{
    image_meta, pack_bytes, write_pack, Entry, IMAGE_KIND_TAG, INDEXED_8_BIT, PALETTE_KIND_TAG,
    RAW_KIND_TAG, TEXT_WINDOW_BIT_DEPTH,
};
use crate::TrainerPicId;

/// Each trainer with its `TRAINER_BACK_PIC_*` index and pack-id name.
const TRAINERS: [(TrainerPicId, u16, &str); 2] = [
    (TrainerPicId::BRENDAN, 0, "brendan"),
    (TrainerPicId::MAY, 1, "may"),
];
const DIRECTIONS: [BattlerDirection; 2] = [BattlerDirection::Front, BattlerDirection::Back];
const FRAME_BYTES: usize = 64 * 64;

/// A pixel value no other image of the fixture shares: trainer, direction and
/// (for a back sheet) frame together.
fn signature(trainer: usize, direction: BattlerDirection, frame: usize) -> u8 {
    let side = match direction {
        BattlerDirection::Front => 1,
        BattlerDirection::Back => 2 + u8::try_from(frame).unwrap(),
    };
    u8::try_from(trainer).unwrap() * 5 + side
}

fn image(id: String, (width, height): (u32, u32), depth: u8, pixel: u8) -> Entry {
    Entry {
        id: Box::leak(id.into_boxed_str()),
        kind_tag: IMAGE_KIND_TAG,
        meta: image_meta(width, height, depth),
        payload: vec![pixel; (width * height) as usize],
    }
}

/// A 64x256 sheet whose four frames each hold their own signature.
fn back_sheet(id: String, trainer: usize) -> Entry {
    let payload = (0..4)
        .flat_map(|frame| vec![signature(trainer, BattlerDirection::Back, frame); FRAME_BYTES])
        .collect();
    Entry {
        id: Box::leak(id.into_boxed_str()),
        kind_tag: IMAGE_KIND_TAG,
        meta: image_meta(64, 256, TEXT_WINDOW_BIT_DEPTH),
        payload,
    }
}

fn palette(id: String, colors: u16, first: u16) -> Entry {
    let mut payload = vec![0u8; usize::from(colors) * 2];
    payload[..2].copy_from_slice(&first.to_le_bytes());
    Entry {
        id: Box::leak(id.into_boxed_str()),
        kind_tag: PALETTE_KIND_TAG,
        meta: colors.to_le_bytes().to_vec(),
        payload,
    }
}

fn first_color(trainer: usize) -> u16 {
    0x0100 * (u16::try_from(trainer).unwrap() + 1)
}

fn entries() -> Vec<Entry> {
    let mut entries = Vec::new();
    for (trainer, (_, _, name)) in TRAINERS.iter().enumerate() {
        entries.push(image(
            format!("trainer/{name}/front"),
            (64, 64),
            TEXT_WINDOW_BIT_DEPTH,
            signature(trainer, BattlerDirection::Front, 0),
        ));
        entries.push(back_sheet(format!("trainer/{name}/back"), trainer));
        entries.push(palette(
            format!("trainer/{name}/palette"),
            16,
            first_color(trainer),
        ));
    }
    entries
}

fn replaced(id: &str, entry: Entry) -> Vec<Entry> {
    let mut entries: Vec<Entry> = entries().into_iter().filter(|e| e.id != id).collect();
    entries.push(entry);
    entries
}

fn load(name: &str, entries: Vec<Entry>) -> (AssetPack, std::path::PathBuf) {
    let path = write_pack(name, &pack_bytes(entries));
    (AssetPack::load(&path).unwrap(), path)
}

#[test]
fn each_trainer_resolves_both_directions_with_its_own_art() {
    let (pack, path) = load("trainer-battler-all", entries());
    for (trainer, (id, _, _)) in TRAINERS.iter().enumerate() {
        for direction in DIRECTIONS {
            let handle = pack.trainer_battler(*id, direction).unwrap();
            assert_eq!((handle.id, handle.direction), (*id, direction));
            let image = handle.art.image;
            let height = match direction {
                BattlerDirection::Front => 64,
                BattlerDirection::Back => 256,
            };
            assert_eq!((image.width, image.height), (64, height));
            assert_eq!(image.bit_depth, 4);
            assert_eq!(handle.art.palette.color_count, 16);
            assert_eq!(handle.art.palette.color(0), Some(first_color(trainer)));
            for (frame, rows) in image.pixels.chunks(FRAME_BYTES).enumerate() {
                assert!(
                    rows.iter()
                        .all(|&p| p == signature(trainer, direction, frame)),
                    "{id:?} {direction:?} frame {frame}"
                );
            }
        }
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn the_back_sheet_keeps_every_frame() {
    let (pack, path) = load("trainer-battler-frames", entries());
    let sheet = pack
        .trainer_battler(TrainerPicId::MAY, BattlerDirection::Back)
        .unwrap();
    let frames: Vec<u8> = sheet
        .art
        .image
        .pixels
        .chunks(FRAME_BYTES)
        .map(|frame| frame[0])
        .collect();
    let expected: Vec<u8> = (0..4)
        .map(|frame| signature(1, BattlerDirection::Back, frame))
        .collect();
    assert_eq!(frames, expected);
    let _ = std::fs::remove_file(path);
}

#[test]
fn both_directions_of_a_trainer_share_one_palette() {
    let (pack, path) = load("trainer-battler-palette", entries());
    for (id, _, _) in TRAINERS {
        let front = pack.trainer_battler(id, BattlerDirection::Front).unwrap();
        let back = pack.trainer_battler(id, BattlerDirection::Back).unwrap();
        assert_eq!(front.art.palette.raw, back.art.palette.raw);
    }
    let brendan = pack
        .trainer_battler(TrainerPicId::BRENDAN, BattlerDirection::Front)
        .unwrap();
    let may = pack
        .trainer_battler(TrainerPicId::MAY, BattlerDirection::Front)
        .unwrap();
    assert_ne!(brendan.art.palette.raw, may.art.palette.raw);
    let _ = std::fs::remove_file(path);
}

#[test]
fn the_back_index_is_not_the_front_identity() {
    assert_eq!(TrainerPicId::BRENDAN.index(), 71);
    assert_eq!(TrainerPicId::MAY.index(), 72);
    for (id, back_index, _) in TRAINERS {
        assert_eq!(id.battler_back_index(), Some(back_index));
    }
    // Hiker's front identity is numerically Brendan's back index.
    for id in [
        TrainerPicId::HIKER,
        TrainerPicId::RS_BRENDAN,
        TrainerPicId::RS_MAY,
        TrainerPicId::RED,
        TrainerPicId::LEAF,
        TrainerPicId::WALLY,
        TrainerPicId::STEVEN,
        TrainerPicId(u8::MAX),
    ] {
        assert_eq!(id.battler_back_index(), None, "{id:?}");
    }
}

#[test]
fn trainers_without_battler_art_are_unsupported_in_both_directions() {
    let (pack, path) = load("trainer-battler-unsupported", entries());
    for id in [
        TrainerPicId::HIKER,
        TrainerPicId::RS_BRENDAN,
        TrainerPicId::RS_MAY,
        TrainerPicId::RED,
        TrainerPicId(u8::MAX),
    ] {
        for direction in DIRECTIONS {
            assert_eq!(
                pack.trainer_battler(id, direction).unwrap_err(),
                PackError::UnsupportedTrainerBattler(id)
            );
        }
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn missing_and_wrong_kind_entries_are_named() {
    let without: Vec<Entry> = entries()
        .into_iter()
        .filter(|e| e.id != "trainer/may/back")
        .collect();
    let (pack, path) = load("trainer-battler-missing", without);
    assert!(pack
        .trainer_battler(TrainerPicId::MAY, BattlerDirection::Front)
        .is_ok());
    assert!(matches!(
        pack.trainer_battler(TrainerPicId::MAY, BattlerDirection::Back).unwrap_err(),
        PackError::UnknownAsset(id) if id == "trainer/may/back"
    ));
    let _ = std::fs::remove_file(path);

    let raw = Entry {
        id: "trainer/brendan/palette",
        kind_tag: RAW_KIND_TAG,
        meta: vec![],
        payload: vec![0; 4],
    };
    let (pack, path) = load(
        "trainer-battler-wrong-kind",
        replaced("trainer/brendan/palette", raw),
    );
    for direction in DIRECTIONS {
        assert!(matches!(
            pack.trainer_battler(TrainerPicId::BRENDAN, direction).unwrap_err(),
            PackError::WrongKind { id, expected: "palette", .. } if id == "trainer/brendan/palette"
        ));
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn malformed_entries_are_rejected_on_read() {
    let cases = [
        (
            "front as a back-sized sheet",
            "trainer/brendan/front",
            TrainerPicId::BRENDAN,
            BattlerDirection::Front,
            image("trainer/brendan/front".into(), (64, 256), 4, 1),
        ),
        (
            "back as a single frame",
            "trainer/may/back",
            TrainerPicId::MAY,
            BattlerDirection::Back,
            image("trainer/may/back".into(), (64, 64), 4, 1),
        ),
        (
            "back truncated to two frames",
            "trainer/may/back",
            TrainerPicId::MAY,
            BattlerDirection::Back,
            image("trainer/may/back".into(), (64, 128), 4, 1),
        ),
        (
            "back at 8 bits",
            "trainer/brendan/back",
            TrainerPicId::BRENDAN,
            BattlerDirection::Back,
            image("trainer/brendan/back".into(), (64, 256), INDEXED_8_BIT, 1),
        ),
        (
            "front with a pixel past the palette",
            "trainer/may/front",
            TrainerPicId::MAY,
            BattlerDirection::Front,
            image("trainer/may/front".into(), (64, 64), 4, 16),
        ),
        (
            "palette of fifteen colours",
            "trainer/brendan/palette",
            TrainerPicId::BRENDAN,
            BattlerDirection::Back,
            palette("trainer/brendan/palette".into(), 15, 0),
        ),
        (
            "palette of thirty-two colours",
            "trainer/may/palette",
            TrainerPicId::MAY,
            BattlerDirection::Front,
            palette("trainer/may/palette".into(), 32, 0),
        ),
    ];
    for (index, (what, entry_id, trainer, direction, entry)) in cases.into_iter().enumerate() {
        let (pack, path) = load(
            &format!("trainer-battler-bad-{index}"),
            replaced(entry_id, entry),
        );
        assert!(
            matches!(
                pack.trainer_battler(trainer, direction).unwrap_err(),
                PackError::MalformedTrainerBattlerAsset { id, .. } if id == entry_id
            ),
            "{what}"
        );
        let _ = std::fs::remove_file(path);
    }
}
