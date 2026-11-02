use std::path::PathBuf;

use assets::audio::{DirectSoundMode, DirectSoundVoice, Envelope};
use assets::audio::{DirectSoundSample, ProgrammableWave};
use assets::pack::{
    BattleMessageTable, PackProblem, BATTLE_MESSAGE_SPECIAL_COUNT, BATTLE_MESSAGE_TABLE_END,
    BATTLE_MESSAGE_TABLE_START,
};
use assets::{Sample, SampleId, Song, VoiceEntry, VoiceGroup, VoiceGroupId};
use pack_format::{EntryKind, PackEntry, PackWriter};

use super::{load_ready_pack, Requirements};

struct Media {
    directory: PathBuf,
}

impl Media {
    fn new(bytes: &[u8]) -> Self {
        let directory = std::env::temp_dir().join(format!(
            "app-pack-readiness-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir(&directory).unwrap();
        std::fs::write(directory.join("game.pack"), bytes).unwrap();
        Self { directory }
    }

    fn pack(&self) -> PathBuf {
        self.directory.join("game.pack")
    }
}

impl Drop for Media {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

fn synthetic_pack(omit: Option<&str>) -> Vec<u8> {
    synthetic_pack_replacing(omit, &[])
}

fn synthetic_pack_replacing(omit: Option<&str>, replace: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut writer = PackWriter::new();
    for asset in Requirements::for_game().0 {
        if Some(asset.id.as_str()) == omit {
            continue;
        }
        let replacement = replace.iter().find(|(id, _)| *id == asset.id);
        let payload = match asset.kind {
            _ if replacement.is_some() => replacement.unwrap().1.clone(),
            EntryKind::Image { width, height, .. } => vec![0; (width * height) as usize],
            EntryKind::Palette { color_count } => vec![0; usize::from(color_count) * 2],
            EntryKind::Raw => raw_payload(&asset.id, asset.byte_len),
        };
        writer.push(PackEntry {
            id: asset.id,
            kind: asset.kind,
            payload,
        });
    }
    writer.finish().unwrap()
}

fn raw_payload(id: &str, byte_len: Option<u64>) -> Vec<u8> {
    if id.starts_with("audio/sample/direct-sound/") {
        Sample::DirectSound(DirectSoundSample::new(1, None, 1, vec![0, 0]).unwrap()).encode()
    } else if id.starts_with("audio/sample/programmable-wave/") {
        Sample::ProgrammableWave(ProgrammableWave { table: [0; 16] }).encode()
    } else if id.starts_with("audio/voicegroup/") {
        VoiceGroup::new(vec![]).unwrap().encode()
    } else if id.starts_with("audio/song/") {
        Song::new(
            VoiceGroupId("audio/voicegroup/title".into()),
            assets::MusicPlayerId(0),
            0,
            None,
            vec![],
        )
        .unwrap()
        .encode()
    } else if id == "battle-message/table" {
        let eos: &[u8] = &[0xff];
        BattleMessageTable::from_entries(
            (BATTLE_MESSAGE_TABLE_START..BATTLE_MESSAGE_TABLE_END).map(|slot| (slot, eos)),
            (0..u16::try_from(BATTLE_MESSAGE_SPECIAL_COUNT).unwrap()).map(|slot| (slot, eos)),
        )
        .unwrap()
        .encode()
    } else {
        vec![0; usize::try_from(byte_len.unwrap()).unwrap()]
    }
}

#[test]
fn current_import_inventory_is_compatible_without_unfinished_extraction_domains() {
    let bytes = synthetic_pack(None);
    let media = Media::new(&bytes);
    let pack = load_ready_pack(&media.pack()).unwrap();
    assert_eq!(pack.bytes(), bytes);
    assert!(pack.cry_table().is_err());
    assert!(pack.pokedex_data().is_err());
    assert!(!media.directory.join("save.sav").exists());
}

#[test]
fn missing_gameplay_assets_block_boot_before_a_save_is_opened() {
    for id in [
        "layout/route101/map",
        "tileset/general/anim/flower/0",
        "sprite/mom",
        "bag/item-icon/image/poke_ball",
        "audio/song/mus_title",
    ] {
        let bytes = synthetic_pack(Some(id));
        let media = Media::new(&bytes);
        let save = media.directory.join("save.sav");
        let error = crate::App::new_headless_real_at(&media.pack(), &save)
            .err()
            .expect("incomplete pack must not boot");
        let crate::app::AppError::Title(crate::title::TitleSceneError::Readiness(error)) = error
        else {
            panic!("expected readiness failure");
        };
        assert_eq!(error.problem(), &PackProblem::MissingAsset(id.into()));
        assert!(!save.exists());
        assert_eq!(std::fs::read(media.pack()).unwrap(), bytes);
    }
}

#[test]
fn malformed_metatile_attributes_block_boot_before_a_save_is_opened() {
    let id = "tileset/general/metatile-attributes";
    let mut bytes = synthetic_pack(None);
    let directory = pack_format::parse_directory(&bytes).unwrap();
    let offset = directory
        .iter()
        .find(|entry| entry.id == id)
        .unwrap()
        .offset;
    bytes[offset..offset + 2].copy_from_slice(&0xf000u16.to_le_bytes());
    let media = Media::new(&bytes);
    let Err(error) = load_ready_pack(&media.pack()) else {
        panic!("malformed attributes must fail readiness");
    };
    assert!(
        matches!(error.problem(), PackProblem::InvalidAsset { id: bad_id, .. } if bad_id == id)
    );
    let save = media.directory.join("save.sav");
    let error = crate::App::new_headless_real_at(&media.pack(), &save)
        .err()
        .expect("malformed pack must not boot");
    assert!(matches!(
        error,
        crate::app::AppError::Title(crate::title::TitleSceneError::Readiness(_))
    ));
    assert!(!save.exists());
    assert_eq!(std::fs::read(media.pack()).unwrap(), bytes);
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn checkout_extracted_pack_passes_the_player_readiness_boundary() {
    let path = assets::AssetPack::repo_pack_path();
    let before = std::fs::read(&path).unwrap();
    let pack = load_ready_pack(&path).unwrap();
    assert_eq!(pack.bytes(), before);
    assert_eq!(std::fs::read(path).unwrap(), before);
}

fn assert_blocked_before_save(bytes: &[u8], id: &str, reason: &str) {
    let media = Media::new(bytes);
    let Err(error) = load_ready_pack(&media.pack()) else {
        panic!("{id}: must fail readiness");
    };
    assert!(
        matches!(error.problem(), PackProblem::InvalidAsset { id: bad, reason: why }
            if bad == id && why.contains(reason)),
        "{error}"
    );
    let text = error.to_string();
    assert!(text.contains("--import-rom <rom>") && text.contains("cargo xtask extract"));
    let save = media.directory.join("save.sav");
    let error = crate::App::new_headless_real_at(&media.pack(), &save)
        .err()
        .expect("unready pack must not boot");
    assert!(matches!(
        error,
        crate::app::AppError::Title(crate::title::TitleSceneError::Readiness(_))
    ));
    assert!(!save.exists());
    assert_eq!(std::fs::read(media.pack()).unwrap(), bytes);
}

fn with_first_pixel(id: &str, pixel: u8) -> Vec<u8> {
    let mut bytes = synthetic_pack(None);
    let offset = pack_format::parse_directory(&bytes)
        .unwrap()
        .iter()
        .find(|entry| entry.id == id)
        .unwrap()
        .offset;
    bytes[offset] = pixel;
    bytes
}

#[test]
fn press_start_indices_the_title_renderer_cannot_repack_are_blocked_at_readiness() {
    let id = crate::title::PRESS_START_IMAGE_ID;
    for pixel in [16, 17, 100, 255] {
        assert_blocked_before_save(&with_first_pixel(id, pixel), id, "4-bit");
    }
    // The last index the renderer repacks still boots to a title scene.
    let bytes = with_first_pixel(id, 15);
    {
        let media = Media::new(&bytes);
        let save = media.directory.join("save.sav");
        assert!(crate::App::new_headless_real_at(&media.pack(), &save).is_ok());
    }
    // Other 8bpp sheets keep their declared depth.
    let version = with_first_pixel("title/image/emerald_version", 255);
    let media = Media::new(&version);
    assert!(load_ready_pack(&media.pack()).is_ok());
}

fn title_voicegroup_with(mode: DirectSoundMode) -> Vec<(&'static str, Vec<u8>)> {
    let voice = VoiceEntry::DirectSound(DirectSoundVoice {
        base_key: 60,
        pan: None,
        sample: SampleId("audio/sample/direct-sound/classical_choir_voice_ahhs".into()),
        envelope: Envelope {
            attack: 255,
            decay: 0,
            sustain: 255,
            release: 0,
        },
        mode,
    });
    vec![(
        "audio/voicegroup/title",
        VoiceGroup::new(vec![voice]).unwrap().encode(),
    )]
}

#[test]
fn a_reverse_title_voice_is_blocked_before_the_silent_music_fallback() {
    for mode in [DirectSoundMode::Resampled, DirectSoundMode::Fixed] {
        let bytes = synthetic_pack_replacing(None, &title_voicegroup_with(mode));
        let media = Media::new(&bytes);
        assert!(load_ready_pack(&media.pack()).is_ok());
    }
    let bytes = synthetic_pack_replacing(None, &title_voicegroup_with(DirectSoundMode::Reverse));
    assert_blocked_before_save(&bytes, "audio/voicegroup/title", "reverse");
}

#[test]
fn later_loads_stay_on_the_pack_that_crossed_the_readiness_boundary() {
    use crate::pack_source::PackSource;
    let valid = synthetic_pack(None);
    let incomplete = synthetic_pack(Some("audio/song/mus_title"));
    let media = Media::new(&valid);
    let other = media.directory.join("later.pack");
    std::fs::write(&other, &incomplete).unwrap();
    let first = media.pack();
    // A resolver whose answer changes after the first call, like a user-data
    // pack appearing while the title screen is open.
    let calls = std::cell::Cell::new(0);
    let app = crate::App::boot_resolving_pack_once(
        || {
            calls.set(calls.get() + 1);
            if calls.get() == 1 {
                first.clone()
            } else {
                other.clone()
            }
        },
        || Ok(platform::Platform::new_headless()),
        || Ok(crate::game_save::SaveSlot::none()),
    )
    .unwrap();
    assert_eq!(calls.get(), 1);
    let PackSource::Explicit(path) = app.pack_source() else {
        panic!("expected a pinned source");
    };
    assert_eq!(&**path, media.pack().as_path());
    assert!(app.pack_source().load().is_ok());
    // An unready first resolution still blocks before any save opens.
    assert!(crate::App::boot_resolving_pack_once(
        || other.clone(),
        || Ok(platform::Platform::new_headless()),
        || panic!("the save medium must not open"),
    )
    .is_err());
}
