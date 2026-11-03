use pack_format::{user_data_dir, user_pack_path, APP_DATA_SUBDIRECTORY, RELEASE_CHANNEL};

#[test]
fn release_channel_import_uses_its_own_data_directory() {
    let expected = match option_env!("POKEEMERALD_RELEASE_CHANNEL") {
        Some("unstable") => "pokeemerald-rs/unstable",
        Some("stable") => "pokeemerald-rs/stable",
        Some("main") => "pokeemerald-rs/main",
        _ => "pokeemerald-rs",
    };
    assert_eq!(APP_DATA_SUBDIRECTORY, expected);
    assert_eq!(
        user_pack_path(),
        user_data_dir().map(|root| root.join(expected).join("pokeemerald.pack"))
    );
    assert_eq!(
        RELEASE_CHANNEL,
        option_env!("POKEEMERALD_RELEASE_CHANNEL").unwrap_or("dev")
    );
}
