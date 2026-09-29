use std::ffi::OsString;
use std::path::PathBuf;

use engine::save::file::{default_save_path_from, HostFamily};

#[test]
fn release_channel_owns_its_default_save_directory() {
    let root = PathBuf::from("/player/data/pokeemerald-rs");
    let expected = match option_env!("POKEEMERALD_RELEASE_CHANNEL") {
        Some(channel @ ("unstable" | "stable" | "main")) => root.join(channel),
        _ => root,
    };
    let actual = default_save_path_from(HostFamily::Xdg, |key| {
        (key == "XDG_DATA_HOME").then(|| OsString::from("/player/data"))
    })
    .unwrap();
    assert_eq!(actual, expected.join("pokeemerald.sav"));
}
