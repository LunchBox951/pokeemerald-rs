//! The subprocess stderr harness: re-executes this test binary to read back
//! what [`App::start_title_music`]'s own `eprintln!` wrote.

use super::super::App;

/// Set only on the child process re-executed below, so it can tell that run
/// apart from an ordinary one -- including CI's blanket `cargo test -p
/// pokeemerald-rs -- --ignored` real-pack sweep, where this stays unset.
const START_TITLE_MUSIC_BOUNDARY_CHILD: &str =
    "POKEEMERALD_RS_923_START_TITLE_MUSIC_BOUNDARY_CHILD";

/// A scratch, valid but entryless asset pack: an empty
/// [`pack_format::PackWriter`] already serializes exactly this, at the live
/// [`pack_format::FORMAT_VERSION`]. Path is unique per test/thread, mirroring
/// `flow::tests::TempSave::new`.
fn write_empty_scratch_pack(label: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-923-{label}-{}-{:?}.pack",
        std::process::id(),
        std::thread::current().id()
    ));
    let bytes = pack_format::PackWriter::new()
        .finish()
        .expect("an entryless pack always serializes");
    std::fs::write(&path, bytes).expect("the scratch pack path must be writable");
    path
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
            crate::pack_source::PackSource::Runtime,
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
