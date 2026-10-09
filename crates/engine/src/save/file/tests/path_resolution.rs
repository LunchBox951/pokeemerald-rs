use super::super::{
    data_dir_for, default_save_path_from, HostFamily, SAVE_DIR_NAME, SAVE_FILE_NAME, SAVE_PATH_ENV,
};
use super::*;
use std::ffi::OsString;

fn env_of<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
    move |name| {
        pairs
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| OsString::from(*value))
    }
}

#[test]
fn windows_data_dir_prefers_appdata_then_falls_back_to_the_user_profile() {
    let with_appdata = env_of(&[("APPDATA", "C:/Users/May/AppData/Roaming")]);
    assert_eq!(
        data_dir_for(HostFamily::Windows, with_appdata),
        Some(PathBuf::from("C:/Users/May/AppData/Roaming"))
    );

    let profile_only = env_of(&[("USERPROFILE", "C:/Users/May")]);
    assert_eq!(
        data_dir_for(HostFamily::Windows, profile_only),
        Some(
            PathBuf::from("C:/Users/May")
                .join("AppData")
                .join("Roaming")
        )
    );

    assert_eq!(data_dir_for(HostFamily::Windows, env_of(&[])), None);
}

#[test]
fn macos_data_dir_is_the_application_support_directory() {
    assert_eq!(
        data_dir_for(HostFamily::MacOs, env_of(&[("HOME", "/Users/may")])),
        Some(
            PathBuf::from("/Users/may")
                .join("Library")
                .join("Application Support")
        )
    );
    assert_eq!(data_dir_for(HostFamily::MacOs, env_of(&[])), None);
}

#[test]
fn xdg_data_dir_prefers_an_absolute_xdg_data_home() {
    let absolute = env_of(&[("XDG_DATA_HOME", "/srv/data"), ("HOME", "/home/may")]);
    assert_eq!(
        data_dir_for(HostFamily::Xdg, absolute),
        Some(PathBuf::from("/srv/data"))
    );
}

#[test]
fn xdg_data_dir_ignores_a_relative_xdg_data_home_and_uses_home() {
    let relative = env_of(&[("XDG_DATA_HOME", "data"), ("HOME", "/home/may")]);
    assert_eq!(
        data_dir_for(HostFamily::Xdg, relative),
        Some(PathBuf::from("/home/may").join(".local").join("share"))
    );
    assert_eq!(data_dir_for(HostFamily::Xdg, env_of(&[])), None);
}

#[test]
fn an_empty_environment_variable_counts_as_unset() {
    let empty = env_of(&[("HOME", ""), ("XDG_DATA_HOME", "")]);
    assert_eq!(data_dir_for(HostFamily::Xdg, empty), None);
}

#[test]
fn the_save_path_override_wins_over_every_data_directory() {
    let env = env_of(&[(SAVE_PATH_ENV, "/tmp/scratch.sav"), ("HOME", "/home/may")]);
    assert_eq!(
        default_save_path_from(HostFamily::Xdg, env).unwrap(),
        PathBuf::from("/tmp/scratch.sav")
    );
}

#[test]
fn without_an_override_the_save_path_is_the_named_file_under_the_data_directory() {
    let env = env_of(&[("HOME", "/home/may")]);
    assert_eq!(
        default_save_path_from(HostFamily::Xdg, env).unwrap(),
        PathBuf::from("/home/may")
            .join(".local")
            .join("share")
            .join(SAVE_DIR_NAME)
            .join(SAVE_FILE_NAME)
    );
}

#[test]
fn an_empty_override_falls_through_to_the_data_directory() {
    let env = env_of(&[(SAVE_PATH_ENV, ""), ("HOME", "/home/may")]);
    assert!(default_save_path_from(HostFamily::Xdg, env)
        .unwrap()
        .starts_with("/home/may"));
}

#[test]
fn no_data_directory_is_a_named_error_not_a_guessed_path() {
    let err = default_save_path_from(HostFamily::Xdg, env_of(&[])).unwrap_err();
    assert!(matches!(err, SaveFileError::NoDataDirectory));
    assert!(
        err.to_string().contains(SAVE_PATH_ENV),
        "the diagnostic must name the override that fixes it: {err}"
    );
}

#[test]
fn relative_user_roots_yield_no_data_directory() {
    for root in ["home", "./home", "C:home", r"\home"] {
        for family in [HostFamily::Xdg, HostFamily::MacOs] {
            assert_eq!(data_dir_for(family, env_of(&[("HOME", root)])), None);
        }
    }
    for root in ["home", "./home", "C:home", r"\home", "/home"] {
        let pairs = [("APPDATA", root), ("USERPROFILE", root)];
        assert_eq!(data_dir_for(HostFamily::Windows, env_of(&pairs)), None);
    }
}

#[test]
fn windows_accepts_drive_and_unc_roots_only() {
    for root in [
        "C:/roaming",
        r"C:\roaming",
        r"\\server\share",
        "//server/share",
        r"\\?\UNC\server\share",
        r"\\?\unc\server\share",
        r"\\?\C:\roaming",
        r"\\.\C:\roaming",
        r"\\?\Volume{26a21bda-a627-11d7-9931-806e6f6e6963}\Users\May\AppData\Roaming",
        r"\\?\Volume{26a21bda-a627-11d7-9931-806e6f6e6963}\",
        r"\\?\GLOBALROOT\Device\HarddiskVolume1\Users\May",
        r"\\.\BootPartition\Users\May",
    ] {
        assert_eq!(
            data_dir_for(HostFamily::Windows, env_of(&[("APPDATA", root)])),
            Some(PathBuf::from(root))
        );
    }
}

#[test]
fn an_invalid_windows_appdata_falls_back_to_a_valid_userprofile() {
    let env = env_of(&[("APPDATA", "roaming"), ("USERPROFILE", "C:/Users/May")]);
    assert_eq!(
        data_dir_for(HostFamily::Windows, env),
        Some(
            PathBuf::from("C:/Users/May")
                .join("AppData")
                .join("Roaming")
        )
    );
}

#[test]
fn a_relative_home_leaves_the_implicit_save_path_unresolvable_but_not_the_override() {
    let env = env_of(&[("HOME", "home")]);
    assert!(matches!(
        default_save_path_from(HostFamily::Xdg, env),
        Err(SaveFileError::NoDataDirectory)
    ));
    let env = env_of(&[(SAVE_PATH_ENV, "mine.sav"), ("HOME", "home")]);
    assert_eq!(
        default_save_path_from(HostFamily::Xdg, env).unwrap(),
        PathBuf::from("mine.sav")
    );
}

#[test]
fn incomplete_unc_roots_are_rejected_in_favour_of_the_userprofile() {
    for root in [
        "//",
        r"\\",
        "//server",
        r"\\server",
        r"\\server\",
        "///share",
        r"\\?\",
        r"\\?\UNC",
        r"\\?\UNC\server",
        r"\\?\UNC\server\",
        r"\\?\C:",
        r"\\?\Volume{26a21bda-a627-11d7-9931-806e6f6e6963}",
        r"\\.\BootPartition",
        r"\\?\\Users",
        r"\\?\unc\server",
        r"\\.\Unc\server",
    ] {
        let pairs = [("APPDATA", root), ("USERPROFILE", "C:/Users/dev")];
        assert_eq!(
            data_dir_for(HostFamily::Windows, env_of(&pairs)),
            Some(
                PathBuf::from("C:/Users/dev")
                    .join("AppData")
                    .join("Roaming")
            )
        );
    }
}

#[test]
fn a_volume_guid_appdata_is_an_absolute_windows_root() {
    let root = r"\\?\Volume{26a21bda-a627-11d7-9931-806e6f6e6963}\Users\May\AppData\Roaming";
    assert_eq!(
        data_dir_for(HostFamily::Windows, env_of(&[("APPDATA", root)])),
        Some(PathBuf::from(root))
    );
    assert_eq!(
        data_dir_for(
            HostFamily::Windows,
            env_of(&[("APPDATA", root), ("USERPROFILE", r"C:\Users\May")])
        ),
        Some(PathBuf::from(root))
    );
}

#[test]
fn engine_save_paths_delegate_to_the_shared_pack_format_resolver() {
    let cases: [(HostFamily, &[(&str, &str)]); 9] = [
        (HostFamily::Xdg, &[("XDG_DATA_HOME", "/x"), ("HOME", "/h")]),
        (HostFamily::Xdg, &[("XDG_DATA_HOME", "x"), ("HOME", "/h")]),
        (HostFamily::Xdg, &[("XDG_DATA_HOME", "x"), ("HOME", "h")]),
        (HostFamily::MacOs, &[("HOME", "/Users/may")]),
        (HostFamily::MacOs, &[("HOME", "Users/may")]),
        (HostFamily::MacOs, &[]),
        (
            HostFamily::Windows,
            &[("APPDATA", "C:/A"), ("USERPROFILE", "C:/U")],
        ),
        (
            HostFamily::Windows,
            &[("APPDATA", "A"), ("USERPROFILE", "C:/U")],
        ),
        (
            HostFamily::Windows,
            &[("APPDATA", "A"), ("USERPROFILE", "U")],
        ),
    ];
    for (family, pairs) in cases {
        let shared = pack_format::data_dir_for(family, env_of(pairs));
        assert_eq!(
            data_dir_for(family, env_of(pairs)),
            shared,
            "{family:?} {pairs:?}"
        );
        let save = default_save_path_from(family, env_of(pairs));
        match shared {
            Some(dir) => assert_eq!(
                save.expect("shared directory resolves"),
                dir.join(SAVE_DIR_NAME).join(SAVE_FILE_NAME)
            ),
            None => assert!(matches!(save, Err(SaveFileError::NoDataDirectory))),
        }
    }
}
