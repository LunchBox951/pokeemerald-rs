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
    data_dir, default_pack_path, repo_pack_path, resolve, user_data_dir, DataDirRule, Probe,
    PACK_PATH_ENV,
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

/// A probe that finds exactly `present` and reports everything else
/// missing.
fn exists_of(present: &[&str]) -> impl Fn(&Path) -> Probe {
    let owned: Vec<PathBuf> = present.iter().map(PathBuf::from).collect();
    move |path| {
        if owned.iter().any(|p| p == path) {
            Probe::Found
        } else {
            Probe::Missing
        }
    }
}

/// A probe that cannot examine `unreadable`, finds `present`, and reports
/// everything else missing.
fn probe_of(present: &[&str], unreadable: &[&str]) -> impl Fn(&Path) -> Probe {
    let found: Vec<PathBuf> = present.iter().map(PathBuf::from).collect();
    let blocked: Vec<PathBuf> = unreadable.iter().map(PathBuf::from).collect();
    move |path| {
        if blocked.iter().any(|p| p == path) {
            Probe::Unreadable
        } else if found.iter().any(|p| p == path) {
            Probe::Found
        } else {
            Probe::Missing
        }
    }
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

/// Whether a real user pack at `candidate` could stop resolution short of the
/// repo rung, as the plain-developer-checkout test must ask it of whatever
/// machine runs the suite.
///
/// [`Path::is_file`] would answer this wrong: it folds an unreadable
/// candidate into "absent", while [`super::resolve`] stops on one and hands
/// it back (`crates/pack-format/src/path.rs:187-191`).
fn a_user_pack_may_stop_resolution(candidate: &Path) -> bool {
    super::probe(candidate) != Probe::Missing
}

#[test]
fn the_default_path_is_the_repo_path_in_a_plain_developer_checkout() {
    // CI and a developer machine both run with no `POKEEMERALD_PACK`, no
    // user-data pack, and test binaries under `target/`, so rung 4 wins and
    // every existing pack test keeps finding the extracted pack.
    if std::env::var_os(PACK_PATH_ENV).is_some() {
        return;
    }
    if super::user_pack_path().is_some_and(|p| a_user_pack_may_stop_resolution(&p)) {
        return;
    }
    // Rung 3 must agree too: a portable-install candidate beside the test
    // binary that is present or unreadable also stops resolution short of
    // the repo path, mirroring `default_pack_path`'s own exe-dir rung
    // (`crates/pack-format/src/path.rs:104-107`, `:193-198`).
    let exe_candidate = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(OUTPUT_RELATIVE_PATH)));
    if exe_candidate.is_some_and(|candidate| super::probe(&candidate) != Probe::Missing) {
        return;
    }
    assert_eq!(default_pack_path(), repo_pack_path());
}

/// The guard above must answer the same question resolution does. `is_file`
/// does not: it folds a candidate that cannot be examined into "absent",
/// while [`super::resolve`] hands such a candidate back
/// (`crates/pack-format/src/path.rs:187-191`), so a guard built on `is_file`
/// lets the assertion run on a machine where rung 2 wins instead.
#[cfg(unix)]
#[test]
fn the_developer_checkout_guard_agrees_with_resolution_about_an_unreadable_user_pack() {
    use super::probe;

    let home = std::env::temp_dir().join(format!("pack-format-guard-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let app = home.join(".local").join("share").join("pokeemerald-rs");
    std::fs::create_dir_all(&app).expect("scratch directories");
    // A self-referential symlink where the pack file goes: `metadata` fails
    // with `ELOOP`, which is neither `NotFound` nor `NotADirectory`, so the
    // real probe reports `Unreadable`. Chosen over a mode-0 parent because a
    // privileged user (this sandbox runs as uid 0) walks straight through
    // that one, and this test must mean the same thing under any uid.
    let candidate = app.join("pokeemerald.pack");
    std::os::unix::fs::symlink("pokeemerald.pack", &candidate).expect("the loop links");

    let guard_skips = a_user_pack_may_stop_resolution(&candidate);
    // `default_pack_path`'s own core, driven with the same `HOME` and the
    // same real probe.
    let resolved = resolve(
        &env_of(&[("HOME", home.to_str().expect("a UTF-8 scratch path"))]),
        Some(Path::new("/opt/game")),
        &probe,
        DataDirRule::Xdg,
    );

    let _ = std::fs::remove_file(&candidate);
    let _ = std::fs::remove_dir_all(&home);

    if guard_skips {
        return;
    }
    assert_eq!(
        resolved,
        repo_pack_path(),
        "the guard did not skip, so resolution must agree there is no user pack"
    );
}

#[test]
fn an_unreadable_user_pack_stops_resolution_instead_of_falling_through() {
    // The player installed a pack and something made its directory
    // unsearchable. Walking on would either load the portable install
    // silently or report the checkout path as missing; neither tells them
    // what actually happened.
    let path = resolve(
        &env_of(&[("HOME", "/home/dev")]),
        Some(Path::new("/opt/game")),
        &probe_of(
            &["/opt/game/assets-pack/pokeemerald.pack"],
            &["/home/dev/.local/share/pokeemerald-rs/pokeemerald.pack"],
        ),
        DataDirRule::Xdg,
    );
    assert_eq!(
        path,
        PathBuf::from("/home/dev/.local/share/pokeemerald-rs/pokeemerald.pack"),
        "an unreadable candidate is handed back for the loader to diagnose"
    );
}

#[test]
fn an_unreadable_executable_directory_pack_also_stops_resolution() {
    let path = resolve(
        &env_of(&[("HOME", "/home/dev")]),
        Some(Path::new("/opt/game")),
        &probe_of(&[], &["/opt/game/assets-pack/pokeemerald.pack"]),
        DataDirRule::Xdg,
    );
    assert_eq!(
        path,
        PathBuf::from("/opt/game/assets-pack/pokeemerald.pack")
    );
}

#[test]
fn a_missing_candidate_still_advances_to_the_next_rung() {
    // The other half of the distinction: only `Missing` walks on, and it
    // must keep doing so or every packless developer checkout breaks.
    let path = resolve(
        &env_of(&[("HOME", "/home/dev")]),
        Some(Path::new("/opt/game")),
        &probe_of(&["/opt/game/assets-pack/pokeemerald.pack"], &[]),
        DataDirRule::Xdg,
    );
    assert_eq!(
        path,
        PathBuf::from("/opt/game/assets-pack/pokeemerald.pack")
    );
}

#[test]
fn the_real_probe_separates_absent_from_unreadable_and_finds_a_file() {
    use super::{probe, Probe};

    let dir = std::env::temp_dir().join(format!("pack-format-probe-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch directory");

    let file = dir.join("pokeemerald.pack");
    std::fs::write(&file, b"not really a pack").expect("the candidate writes");
    assert_eq!(probe(&file), Probe::Found);
    assert_eq!(probe(&dir.join("absent.pack")), Probe::Missing);
    // A directory wearing the pack's name is not a pack, and the next rung
    // is a better answer than a read error on it.
    assert_eq!(probe(&dir), Probe::Missing);

    let _ = std::fs::remove_file(&file);
    let _ = std::fs::remove_dir(&dir);
}

#[cfg(unix)]
#[test]
fn the_real_probe_reports_a_pack_behind_an_unsearchable_directory_as_unreadable() {
    use std::os::unix::fs::PermissionsExt as _;

    use super::{probe, Probe};

    let root = std::env::temp_dir().join(format!("pack-format-noaccess-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let inner = root.join("pokeemerald-rs");
    std::fs::create_dir_all(&inner).expect("scratch directories");
    let pack = inner.join("pokeemerald.pack");
    std::fs::write(&pack, b"the player's pack").expect("the pack writes");

    std::fs::set_permissions(&inner, std::fs::Permissions::from_mode(0o000))
        .expect("the directory closes");
    let answer = probe(&pack);
    std::fs::set_permissions(&inner, std::fs::Permissions::from_mode(0o755))
        .expect("the directory reopens");

    // A privileged user ignores directory permissions, so the close-off
    // does not block them and there is nothing to assert. Asked of the
    // outcome rather than of the uid, so it needs no libc.
    if answer != Probe::Found {
        assert_eq!(
            answer,
            Probe::Unreadable,
            "a pack behind an unsearchable directory is not the same as no pack"
        );
    }

    let _ = std::fs::remove_file(&pack);
    let _ = std::fs::remove_dir(&inner);
    let _ = std::fs::remove_dir(&root);
}

/// A regular file where a candidate's *parent directory* should be proves
/// the candidate cannot exist, so resolution must walk on to the next rung
/// rather than stop on a path it has just been told is unreachable.
#[test]
fn the_real_probe_reports_a_candidate_under_a_regular_file_as_missing() {
    use super::{probe, Probe};

    let root = std::env::temp_dir().join(format!("pack-format-notdir-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("scratch directory");

    // `$XDG_DATA_HOME/pokeemerald-rs` is a regular file, not this
    // project's data directory.
    let intermediate = root.join("pokeemerald-rs");
    std::fs::write(&intermediate, b"stray file").expect("the stray file writes");

    let observed = probe(&intermediate.join("pokeemerald.pack"));

    let _ = std::fs::remove_file(&intermediate);
    let _ = std::fs::remove_dir(&root);

    assert_eq!(
        observed,
        Probe::Missing,
        "no pack can live under a regular file, so the next rung is the right place to look"
    );
}

/// A regular file at rung 2's user-data directory proves no pack can be
/// there, so resolution must still find rung 3's valid pack.
#[test]
fn a_regular_file_at_the_user_data_rung_advances_resolution_to_a_valid_later_rung() {
    use super::{probe, HOST_RULE};

    let root =
        std::env::temp_dir().join(format!("pack-format-resolve-notdir-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("scratch directory");

    // Stands in for the user-data directory: a regular file, not a
    // directory, so no candidate can exist beneath it under any convention.
    let data_home = root.join("data-home-file");
    std::fs::write(&data_home, b"not a directory").expect("the obstruction writes");

    let exe_dir = root.join("game");
    let pack = exe_dir.join(OUTPUT_RELATIVE_PATH);
    std::fs::create_dir_all(pack.parent().expect("the pack has a parent"))
        .expect("executable-directory pack directory");
    std::fs::write(&pack, b"the portable pack").expect("the portable pack writes");

    // The variable `data_dir` reads for this host's own rule.
    let key = match HOST_RULE {
        DataDirRule::Xdg => "XDG_DATA_HOME",
        DataDirRule::MacOs => "HOME",
        DataDirRule::Windows => "APPDATA",
    };
    let data_home_value = data_home.clone().into_os_string();
    let env = move |k: &str| {
        if k == key {
            Some(data_home_value.clone())
        } else {
            None
        }
    };

    let path = resolve(&env, Some(exe_dir.as_path()), &probe, HOST_RULE);

    let _ = std::fs::remove_file(&data_home);
    let _ = std::fs::remove_dir_all(&exe_dir);
    let _ = std::fs::remove_dir(&root);

    assert_eq!(
        path, pack,
        "a regular file blocking the user-data rung must not stop resolution before a valid later rung"
    );
}
