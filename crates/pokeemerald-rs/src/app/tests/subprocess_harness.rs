//! The subprocess stderr harness: re-executes this test binary to read back
//! what [`App::start_title_music`]'s own `eprintln!` wrote.

use super::super::App;

/// Set only on the child process re-executed below, so it can tell that run
/// apart from an ordinary one -- including CI's blanket `cargo test -p
/// pokeemerald-rs -- --ignored` real-pack sweep, where this stays unset.
const START_TITLE_MUSIC_BOUNDARY_CHILD: &str =
    "POKEEMERALD_RS_923_START_TITLE_MUSIC_BOUNDARY_CHILD";

/// The `attempt`th candidate scratch path: unique per test/thread, mirroring
/// `flow::tests::TempSave::new`, with a counter after the first to step past an occupied name.
fn scratch_pack_candidate(label: &str, attempt: u32) -> std::path::PathBuf {
    let suffix = if attempt == 0 {
        String::new()
    } else {
        format!("-{attempt}")
    };
    std::env::temp_dir().join(format!(
        "pokeemerald-rs-923-{label}-{}-{:?}{suffix}.pack",
        std::process::id(),
        std::thread::current().id()
    ))
}

/// A scratch, valid but entryless asset pack: an empty
/// [`pack_format::PackWriter`] already serializes exactly this, at the live
/// [`pack_format::FORMAT_VERSION`]. The file is created exclusively
/// (`O_EXCL`), so a name already occupied -- including by a symlink -- is
/// skipped rather than followed and truncated, and the bytes go through the
/// handle that creation returned.
fn write_empty_scratch_pack(label: &str) -> std::path::PathBuf {
    use std::io::Write as _;

    let bytes = pack_format::PackWriter::new()
        .finish()
        .expect("an entryless pack always serializes");
    for attempt in 0..64 {
        let path = scratch_pack_candidate(label, attempt);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                file.write_all(&bytes)
                    .expect("the scratch pack path must be writable");
                return path;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => panic!("the scratch pack path must be creatable: {error}"),
        }
    }
    panic!("every candidate scratch pack name was already occupied");
}

/// A symlink planted at candidate 0 (the name the helper tries first) must be left alone: its
/// target keeps its contents and the pack lands at a different name.
#[cfg(unix)]
#[test]
fn scratch_pack_creation_does_not_write_through_a_planted_symlink() {
    let label = "symlink-regression";
    let target = std::env::temp_dir().join(format!(
        "pokeemerald-rs-923-{label}-target-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    {
        use std::io::Write as _;

        let mut target_file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .expect("the symlink target must be creatable");
        target_file
            .write_all(b"precious")
            .expect("the symlink target must be writable");
    }
    // Candidate 0 is the helper's first choice; an occupied name fails the test.
    let planted = scratch_pack_candidate(label, 0);
    if let Err(error) = std::os::unix::fs::symlink(&target, &planted) {
        drop(std::fs::remove_file(&target));
        panic!("candidate 0 must be free to plant the symlink: {error}");
    }

    let written = write_empty_scratch_pack(label);

    let target_contents = std::fs::read_to_string(&target);
    let pack_exists = written.is_file();
    drop(std::fs::remove_file(&written));
    drop(std::fs::remove_file(&planted));
    drop(std::fs::remove_file(&target));
    assert_eq!(
        target_contents.as_deref().ok(),
        Some("precious"),
        "the symlink target must stay unchanged"
    );
    assert_ne!(written, planted, "the occupied name must be skipped");
    assert_eq!(
        written,
        scratch_pack_candidate(label, 1),
        "the pack must land at the next candidate"
    );
    assert!(pack_exists, "the pack must be written at the fresh name");
}

/// Re-executes this test binary as a child against a scratch entryless pack
/// (pack load succeeds, song lookup fails), because only a subprocess can
/// read back what [`App::start_title_music`]'s own `eprintln!` wrote.
#[test]
fn start_title_music_failure_emits_its_subsystem_prefix_once_at_the_eprintln_boundary() {
    if std::env::var_os(START_TITLE_MUSIC_BOUNDARY_CHILD).is_some() {
        let mut context = crate::music::MusicContext::new();
        // Never actually called: the song lookup fails first.
        let played = App::start_title_music(
            &crate::pack_source::PackSource::Runtime,
            &mut context,
            || {
                Ok(platform::AudioOutput::null(
                    crate::music::RING_CAPACITY_FRAMES,
                ))
            },
        );
        assert!(
            played.is_none(),
            "an entryless pack has no `mus_title` entry, so the song lookup must fail"
        );
        return;
    }

    let pack_path = write_empty_scratch_pack("start-title-music-boundary");
    let exe = std::env::current_exe().expect("the running test binary has a path");
    let output = std::process::Command::new(exe)
        .args([
            "--exact",
            "--nocapture",
            "app::tests::subprocess_harness::start_title_music_failure_emits_its_subsystem_prefix_once_at_the_eprintln_boundary",
        ])
        .env(START_TITLE_MUSIC_BOUNDARY_CHILD, "1")
        .env(pack_format::PACK_PATH_ENV, &pack_path)
        .output()
        .expect("re-running this test binary must succeed");
    drop(std::fs::remove_file(&pack_path));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("the title screen will play without music"),
        "the child never reached the boundary; stderr:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("music:").count(),
        1,
        "the eprintln! boundary must name its subsystem once across all of stderr, not once per prefix layer:\n{stderr}"
    );
    assert!(
        output.status.success(),
        "the child test must pass: status {:?}\nstderr:\n{stderr}",
        output.status
    );
}
