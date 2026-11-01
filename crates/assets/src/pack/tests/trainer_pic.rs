use super::super::{AssetPack, PackError};
use super::shared::{
    image_meta, pack_bytes, write_pack, Entry, IMAGE_KIND_TAG, INDEXED_8_BIT, PALETTE_KIND_TAG,
    RAW_KIND_TAG, TEXT_WINDOW_BIT_DEPTH,
};
use crate::TrainerPicId;

const PICS: [(TrainerPicId, &str); 2] = [
    (TrainerPicId::BRENDAN, "brendan"),
    (TrainerPicId::MAY, "may"),
];

fn image(id: String, side: (u32, u32), depth: u8, pixel: u8) -> Entry {
    Entry {
        id: Box::leak(id.into_boxed_str()),
        kind_tag: IMAGE_KIND_TAG,
        meta: image_meta(side.0, side.1, depth),
        payload: vec![pixel; (side.0 * side.1) as usize],
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

fn entries() -> Vec<Entry> {
    let mut entries = Vec::new();
    for (index, (_, name)) in PICS.iter().enumerate() {
        let tag = u8::try_from(index).unwrap() + 1;
        entries.push(image(
            format!("trainer-pic/{name}/front"),
            (64, 64),
            TEXT_WINDOW_BIT_DEPTH,
            tag,
        ));
        entries.push(palette(
            format!("trainer-pic/{name}/palette"),
            16,
            u16::from(tag) << 8,
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
fn each_identity_resolves_its_own_image_and_palette() {
    let (pack, path) = load("trainer-pic-all", entries());
    for (index, (id, _)) in PICS.iter().enumerate() {
        let tag = u8::try_from(index).unwrap() + 1;
        let handle = pack.trainer_pic(*id).unwrap();
        assert_eq!(handle.id, *id);
        assert_eq!((handle.art.image.width, handle.art.image.height), (64, 64));
        assert_eq!(handle.art.image.bit_depth, 4);
        assert_eq!(handle.art.image.pixels[0], tag);
        assert_eq!(handle.art.palette.color_count, 16);
        assert_eq!(handle.art.palette.color(0), Some(u16::from(tag) << 8));
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn identities_without_a_published_picture_are_unsupported() {
    let (pack, path) = load("trainer-pic-unsupported", entries());
    for id in [
        TrainerPicId::HIKER,
        TrainerPicId::RS_BRENDAN,
        TrainerPicId::RS_MAY,
        TrainerPicId(u8::MAX),
    ] {
        assert_eq!(
            pack.trainer_pic(id).unwrap_err(),
            PackError::UnsupportedTrainerPic(id)
        );
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn missing_and_wrong_kind_entries_are_named() {
    let without: Vec<Entry> = entries()
        .into_iter()
        .filter(|e| e.id != "trainer-pic/may/palette")
        .collect();
    let (pack, path) = load("trainer-pic-missing", without);
    assert!(pack.trainer_pic(TrainerPicId::BRENDAN).is_ok());
    assert!(matches!(
        pack.trainer_pic(TrainerPicId::MAY).unwrap_err(),
        PackError::UnknownAsset(id) if id == "trainer-pic/may/palette"
    ));
    let _ = std::fs::remove_file(path);

    let raw = Entry {
        id: "trainer-pic/may/front",
        kind_tag: RAW_KIND_TAG,
        meta: vec![],
        payload: vec![0; 4],
    };
    let (pack, path) = load(
        "trainer-pic-wrong-kind",
        replaced("trainer-pic/may/front", raw),
    );
    assert!(matches!(
        pack.trainer_pic(TrainerPicId::MAY).unwrap_err(),
        PackError::WrongKind { id, expected: "image", .. } if id == "trainer-pic/may/front"
    ));
    let _ = std::fs::remove_file(path);
}

#[test]
fn malformed_entries_are_rejected_on_read() {
    let cases = [
        (
            "trainer-pic/brendan/front",
            TrainerPicId::BRENDAN,
            image("trainer-pic/brendan/front".into(), (32, 32), 4, 1),
        ),
        (
            "trainer-pic/may/front",
            TrainerPicId::MAY,
            image("trainer-pic/may/front".into(), (64, 64), INDEXED_8_BIT, 1),
        ),
        (
            "trainer-pic/may/front",
            TrainerPicId::MAY,
            image("trainer-pic/may/front".into(), (64, 64), 4, 16),
        ),
        (
            "trainer-pic/brendan/palette",
            TrainerPicId::BRENDAN,
            palette("trainer-pic/brendan/palette".into(), 15, 0),
        ),
        (
            "trainer-pic/may/palette",
            TrainerPicId::MAY,
            palette("trainer-pic/may/palette".into(), 32, 0),
        ),
    ];
    for (index, (entry_id, pic, entry)) in cases.into_iter().enumerate() {
        let (pack, path) = load(
            &format!("trainer-pic-bad-{index}"),
            replaced(entry_id, entry),
        );
        assert!(
            matches!(
                pack.trainer_pic(pic).unwrap_err(),
                PackError::MalformedTrainerPicAsset { id, .. } if id == entry_id
            ),
            "case {index}"
        );
        let _ = std::fs::remove_file(path);
    }
}
