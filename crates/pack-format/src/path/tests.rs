//! Unit tests for pack-path resolution.
//!
//! Every test drives [`super::resolve`] / [`super::data_dir`] with a fake
//! environment, a fake executable directory, and a fake existence
//! predicate, so all four rungs and all three OS conventions are checked on
//! whichever host runs the suite. Nothing here reads or writes the real
//! environment.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::{
    data_dir, default_pack_path, repo_pack_path, resolve, user_data_dir, DataDirRule, PACK_PATH_ENV,
};
use crate::layout::OUTPUT_RELATIVE_PATH;

/// An environment built from `(key, value)` pairs; every other key is unset.
fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
    let owned: Vec<(String, OsString)> = pairs
        .iter()
        .map(|&(k, v)| (k.to_owned(), OsString::from(v)))
        .collect();
    move |key| owned.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
}

/// An existence predicate that answers `true` for exactly `present`.
fn exists_of(present: &[&str]) -> impl Fn(&Path) -> bool {
    let owned: Vec<PathBuf> = present.iter().map(PathBuf::from).collect();
    move |path| owned.iter().any(|p| p == path)
}

#[test]
fn the_env_override_wins_over_every_other_rung() {
    let path = resolve(
        &env_of(&[
            (PACK_PATH_ENV, "/override/custom.pack"),
            ("HOME", "/home/dev"),
        ]),
        Some(Path::new("/opt/game")),
        &exists_of(&[
            "/home/dev/.local/share/pokeemerald-rs/pokeemerald.pack",
            "/opt/game/assets-pack/pokeemerald.pack",
        ]),
        DataDirRule::Xdg,
    );
    assert_eq!(path, PathBuf::from("/override/custom.pack"));
}

#[test]
fn the_env_override_is_honoured_even_when_the_file_is_absent() {
    // A typo must report a missing pack at the requested path rather than
    // silently loading a different one.
    let path = resolve(
        &env_of(&[(PACK_PATH_ENV, "/typo.pack"), ("HOME", "/home/dev")]),
        None,
        &exists_of(&["/home/dev/.local/share/pokeemerald-rs/pokeemerald.pack"]),
        DataDirRule::Xdg,
    );
    assert_eq!(path, PathBuf::from("/typo.pack"));
}

#[test]
fn an_empty_env_override_is_ignored() {
    let path = resolve(
        &env_of(&[(PACK_PATH_ENV, ""), ("HOME", "/home/dev")]),
        None,
        &exists_of(&["/home/dev/.local/share/pokeemerald-rs/pokeemerald.pack"]),
        DataDirRule::Xdg,
    );
    assert_eq!(
        path,
        PathBuf::from("/home/dev/.local/share/pokeemerald-rs/pokeemerald.pack")
    );
}

#[test]
fn the_user_data_pack_wins_over_the_executable_directory() {
    let path = resolve(
        &env_of(&[("HOME", "/home/dev")]),
        Some(Path::new("/opt/game")),
        &exists_of(&[
            "/home/dev/.local/share/pokeemerald-rs/pokeemerald.pack",
            "/opt/game/assets-pack/pokeemerald.pack",
        ]),
        DataDirRule::Xdg,
    );
    assert_eq!(
        path,
        PathBuf::from("/home/dev/.local/share/pokeemerald-rs/pokeemerald.pack")
    );
}

#[test]
fn xdg_data_home_beats_the_home_fallback() {
    let path = resolve(
        &env_of(&[("XDG_DATA_HOME", "/xdg"), ("HOME", "/home/dev")]),
        None,
        &exists_of(&[
            "/xdg/pokeemerald-rs/pokeemerald.pack",
            "/home/dev/.local/share/pokeemerald-rs/pokeemerald.pack",
        ]),
        DataDirRule::Xdg,
    );
    assert_eq!(path, PathBuf::from("/xdg/pokeemerald-rs/pokeemerald.pack"));
}

#[test]
fn a_relative_xdg_data_home_is_ignored_in_favour_of_the_home_fallback() {
    // The Base Directory Specification requires a relative `$XDG_DATA_HOME`
    // to be ignored, not resolved: honouring `data` would let the process's
    // current directory choose the pack.
    assert_eq!(
        data_dir(
            &env_of(&[("XDG_DATA_HOME", "data"), ("HOME", "/home/dev")]),
            DataDirRule::Xdg
        ),
        Some(PathBuf::from("/home/dev/.local/share"))
    );
    let path = resolve(
        &env_of(&[("XDG_DATA_HOME", "data"), ("HOME", "/home/dev")]),
        None,
        &exists_of(&[
            "data/pokeemerald-rs/pokeemerald.pack",
            "/home/dev/.local/share/pokeemerald-rs/pokeemerald.pack",
        ]),
        DataDirRule::Xdg,
    );
    assert_eq!(
        path,
        PathBuf::from("/home/dev/.local/share/pokeemerald-rs/pokeemerald.pack")
    );
}

#[test]
fn a_relative_xdg_data_home_with_no_home_yields_no_data_directory() {
    // Ignored means ignored: with nothing to fall back to there is no
    // user-data directory at all, rather than a cwd-relative one.
    assert_eq!(
        data_dir(&env_of(&[("XDG_DATA_HOME", "data")]), DataDirRule::Xdg),
        None
    );
}

#[test]
fn macos_looks_under_library_application_support() {
    let path = resolve(
        &env_of(&[("HOME", "/Users/dev"), ("XDG_DATA_HOME", "/xdg")]),
        None,
        &exists_of(&["/Users/dev/Library/Application Support/pokeemerald-rs/pokeemerald.pack"]),
        DataDirRule::MacOs,
    );
    assert_eq!(
        path,
        PathBuf::from("/Users/dev/Library/Application Support/pokeemerald-rs/pokeemerald.pack")
    );
}

#[test]
fn windows_looks_under_appdata() {
    let path = resolve(
        &env_of(&[("APPDATA", "C:/Users/dev/AppData/Roaming"), ("HOME", "/h")]),
        None,
        &exists_of(&["C:/Users/dev/AppData/Roaming/pokeemerald-rs/pokeemerald.pack"]),
        DataDirRule::Windows,
    );
    assert_eq!(
        path,
        PathBuf::from("C:/Users/dev/AppData/Roaming/pokeemerald-rs/pokeemerald.pack")
    );
}

#[test]
fn windows_falls_back_to_userprofile_when_appdata_is_unset() {
    // A Windows service or a stripped shell can hand a process
    // `%USERPROFILE%` without `%APPDATA%`; the conventional roaming
    // directory is still derivable, and the save resolver already derives
    // it, so the importer's default output must not vanish here.
    assert_eq!(
        data_dir(
            &env_of(&[("USERPROFILE", r"C:\Users\dev")]),
            DataDirRule::Windows
        ),
        Some(
            PathBuf::from(r"C:\Users\dev")
                .join("AppData")
                .join("Roaming")
        )
    );
    let path = resolve(
        &env_of(&[("USERPROFILE", "C:/Users/dev")]),
        None,
        &exists_of(&["C:/Users/dev/AppData/Roaming/pokeemerald-rs/pokeemerald.pack"]),
        DataDirRule::Windows,
    );
    assert_eq!(
        path,
        PathBuf::from("C:/Users/dev/AppData/Roaming/pokeemerald-rs/pokeemerald.pack")
    );
}

#[test]
fn windows_appdata_beats_the_userprofile_fallback() {
    assert_eq!(
        data_dir(
            &env_of(&[("APPDATA", "D:/roaming"), ("USERPROFILE", "C:/Users/dev"),]),
            DataDirRule::Windows
        ),
        Some(PathBuf::from("D:/roaming"))
    );
}

#[test]
fn the_executable_directory_is_used_when_no_user_data_pack_exists() {
    let path = resolve(
        &env_of(&[("HOME", "/home/dev")]),
        Some(Path::new("/opt/game")),
        &exists_of(&["/opt/game/assets-pack/pokeemerald.pack"]),
        DataDirRule::Xdg,
    );
    assert_eq!(
        path,
        PathBuf::from("/opt/game/assets-pack/pokeemerald.pack")
    );
}

#[test]
fn nothing_present_falls_back_to_the_compile_time_repo_path() {
    let path = resolve(
        &env_of(&[("HOME", "/home/dev")]),
        Some(Path::new("/opt/game")),
        &exists_of(&[]),
        DataDirRule::Xdg,
    );
    assert_eq!(path, repo_pack_path());
    assert!(path.ends_with(OUTPUT_RELATIVE_PATH), "{}", path.display());
}

#[test]
fn a_scrubbed_environment_still_resolves_to_the_repo_path() {
    let path = resolve(&env_of(&[]), None, &exists_of(&[]), DataDirRule::Xdg);
    assert_eq!(path, repo_pack_path());
}

#[test]
fn data_dir_is_none_when_its_variables_are_unset() {
    assert_eq!(data_dir(&env_of(&[]), DataDirRule::Xdg), None);
    assert_eq!(data_dir(&env_of(&[]), DataDirRule::MacOs), None);
    assert_eq!(data_dir(&env_of(&[]), DataDirRule::Windows), None);
}

#[test]
fn empty_data_dir_variables_are_treated_as_unset() {
    assert_eq!(
        data_dir(
            &env_of(&[("XDG_DATA_HOME", ""), ("HOME", "/home/dev")]),
            DataDirRule::Xdg
        ),
        Some(PathBuf::from("/home/dev/.local/share"))
    );
    assert_eq!(data_dir(&env_of(&[("HOME", "")]), DataDirRule::Xdg), None);
}

#[test]
fn the_repo_path_ends_at_the_published_relative_location() {
    let path = repo_pack_path();
    assert!(path.ends_with(OUTPUT_RELATIVE_PATH), "{}", path.display());
    assert!(path.is_absolute(), "{}", path.display());
}

#[test]
fn the_host_helpers_agree_with_the_rule_they_are_built_from() {
    // Reads the real environment, so it asserts only the relationship
    // between the two public helpers, never a concrete directory.
    match (user_data_dir(), super::user_pack_path()) {
        (Some(dir), Some(pack)) => {
            assert_eq!(pack, dir.join("pokeemerald-rs").join("pokeemerald.pack"));
        }
        (None, None) => {}
        other => panic!("user_data_dir and user_pack_path disagree: {other:?}"),
    }
}

#[test]
fn the_default_path_is_the_repo_path_in_a_plain_developer_checkout() {
    // CI and a developer machine both run with no `POKEEMERALD_PACK`, no
    // user-data pack, and test binaries under `target/`, so rung 4 wins and
    // every existing pack test keeps finding the extracted pack.
    if std::env::var_os(PACK_PATH_ENV).is_some() {
        return;
    }
    if super::user_pack_path().is_some_and(|p| p.is_file()) {
        return;
    }
    assert_eq!(default_pack_path(), repo_pack_path());
}
