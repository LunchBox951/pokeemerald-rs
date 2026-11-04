//! Title music through output loss: startup without an output, transport
//! reports, and the fade and drain continuing while no stream is attached.

use super::super::{
    report_transport, title_music_open_failure_message, title_music_transport_message, App,
};
use platform::audio::{TransportFailure, TransportState};
use platform::PlatformError;

/// A synthetic pack holding a `mus_title` whose only voice needs no sample,
/// so [`App::start_title_music`] loads it through the production path.
fn mus_title_pack(label: &str) -> &'static std::path::Path {
    use assets::{Envelope, MusicPlayerId, Square1Voice, VoiceEntry, VoiceGroup, VoiceGroupId};
    use pack_format::{EntryKind, PackEntry, PackWriter};

    let group_id = "audio/voicegroup/transporttest";
    let song = assets::Song::new(
        VoiceGroupId(group_id.to_owned()),
        MusicPlayerId(0),
        0,
        None,
        vec![vec![]],
    )
    .expect("a one-empty-track song is well-formed");
    let group = VoiceGroup::new(vec![VoiceEntry::Square1(Square1Voice {
        base_key: 60,
        length: 0,
        sweep: 0,
        duty: 2,
        envelope: Envelope {
            attack: 0,
            decay: 0,
            sustain: 15,
            release: 0,
        },
        fixed_rate: false,
    })])
    .expect("one slot is well under VOICE_SLOT_COUNT");
    let mut writer = PackWriter::new();
    for (id, payload) in [
        ("audio/song/mus_title", song.encode()),
        (group_id, group.encode()),
    ] {
        writer.push(PackEntry {
            id: id.to_owned(),
            kind: EntryKind::Raw,
            payload,
        });
    }
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-1024-{label}-{}-{:?}.pack",
        std::process::id(),
        std::thread::current().id()
    ));
    crate::pack_test_support::write_scratch_exclusive(
        &path,
        writer.finish().expect("the synthetic pack serializes"),
    );
    Box::leak(path.into_boxed_path())
}

/// An output that cannot even be opened (the transport worker failed to
/// start) must not take the title's sequencing with it: the player runs on
/// the null output, and the opener is asked once.
#[test]
fn an_unopenable_output_leaves_the_title_music_sequencing_silently() {
    let path = mus_title_pack("open-failure");
    let mut opens = 0;
    let mut context = crate::music::MusicContext::new();
    let started = App::start_title_music(
        &crate::pack_source::PackSource::Test(path),
        &mut context,
        || {
            opens += 1;
            Err(PlatformError::NoAudioDevice)
        },
    );
    drop(std::fs::remove_file(path));

    assert_eq!(opens, 1);
    let mut player = started.expect("a failed open must not drop the music");
    assert!(player.is_running());
    player.fade_out(crate::music::TITLE_FADE_OUT_SPEED);
    for _ in 0..64 {
        player.advance_frame();
    }
    assert!(player.fade_finished());
}

#[test]
fn the_open_failure_message_names_its_subsystem_once() {
    let message = title_music_open_failure_message(&PlatformError::NoAudioDevice);
    assert_eq!(message.matches("music:").count(), 1, "{message}");
    assert!(
        message.contains("no default audio output device"),
        "{message}"
    );
}

/// Loss, recovery and first connection read differently; a repeat of the
/// state the player is already in says nothing.
#[test]
fn each_transition_kind_has_its_own_report() {
    use TransportFailure::{NoOutput, StreamLost};
    use TransportState::{Connecting, Null, Ready, Unavailable};

    let lines = [
        title_music_transport_message(Ready, Unavailable(StreamLost)),
        title_music_transport_message(Connecting, Unavailable(NoOutput)),
        title_music_transport_message(Unavailable(NoOutput), Unavailable(StreamLost)),
        title_music_transport_message(Unavailable(StreamLost), Ready),
        title_music_transport_message(Connecting, Ready),
    ];
    let lines: Vec<String> = lines.into_iter().map(Option::unwrap).collect();
    assert!(lines[0].contains("lost (StreamLost)"), "{}", lines[0]);
    assert!(lines[1].contains("unavailable (NoOutput)"), "{}", lines[1]);
    assert!(
        lines[2].contains("still unavailable (StreamLost)"),
        "{}",
        lines[2]
    );
    assert!(lines[3].contains("recovered"), "{}", lines[3]);
    assert!(lines[4].contains("connected"), "{}", lines[4]);
    assert_eq!(title_music_transport_message(Ready, Ready), None);
    assert_eq!(title_music_transport_message(Null, Connecting), None);
}

/// Through the real player: an outage, its recovery, and a second outage
/// each produce exactly one report, in order, and polling again produces
/// none.
#[test]
fn a_lost_and_recovered_output_is_reported_once_per_transition() {
    let mut player = crate::music::MusicPlayer::start(
        super::music_lifecycle::looping_song_for_test(),
        platform::AudioOutput::null(crate::music::RING_CAPACITY_FRAMES),
    )
    .expect("null backend never errors");
    for state in [
        TransportState::Connecting,
        TransportState::Unavailable(TransportFailure::NoOutput),
        TransportState::Ready,
        TransportState::Unavailable(TransportFailure::StreamLost),
        TransportState::Ready,
    ] {
        player.script_transport_for_test(state, None, 48_000);
    }

    let mut lines = Vec::new();
    report_transport(
        || player.poll_transport(),
        |line| lines.push(line.to_owned()),
    );
    report_transport(
        || player.poll_transport(),
        |line| lines.push(line.to_owned()),
    );

    let kinds: Vec<&str> = lines
        .iter()
        .map(|line| {
            ["unavailable", "connected", "lost", "recovered"]
                .into_iter()
                .find(|kind| line.contains(kind))
                .unwrap_or("?")
        })
        .collect();
    assert_eq!(kinds, ["unavailable", "recovered", "lost", "recovered"]);
}

/// `App::step` consumes the transport: a `Ready` scripted behind the player
/// reaches its tail bounds on the next frame, and the player stays.
#[test]
fn stepping_consumes_the_transport_and_refreshes_the_tail() {
    let mut app = App::new_headless();
    app.attach_music_for_test(
        crate::music::MusicPlayer::start(
            super::music_lifecycle::looping_song_for_test(),
            platform::AudioOutput::null(crate::music::RING_CAPACITY_FRAMES),
        )
        .expect("null backend never errors"),
    );
    app.script_music_transport_for_test(TransportState::Ready, Some(7_680), 48_000);
    app.step().unwrap();
    assert_eq!(app.music_tail_bound_for_test(), Some(23));
}

/// The fade and the drain bound are counted in game frames, so a title
/// whose output is lost mid-fade reaches silence on the usual 64th frame and
/// is still released afterward though nothing ever drains its ring.
#[test]
fn a_fade_completes_and_releases_the_player_while_the_output_is_lost() {
    const FADE_FRAMES: usize = 64;
    const RELEASE_BUDGET: usize = 400;

    let mut app = App::new_headless();
    app.attach_music_for_test(
        crate::music::MusicPlayer::start(
            super::music_lifecycle::looping_song_for_test(),
            platform::AudioOutput::null(crate::music::RING_CAPACITY_FRAMES),
        )
        .expect("null backend never errors"),
    );
    app.script_music_transport_for_test(
        TransportState::Unavailable(TransportFailure::StreamLost),
        None,
        0,
    );

    for frame in 1..FADE_FRAMES {
        app.step().unwrap();
        assert_eq!(
            app.music_fade_finished_for_test(),
            Some(false),
            "frame {frame}"
        );
    }
    app.step().unwrap();
    assert_eq!(app.music_fade_finished_for_test(), Some(true));

    let released = (0..RELEASE_BUDGET).any(|_| {
        app.step().unwrap();
        !app.has_music_for_test()
    });
    assert!(released, "a stalled ring must not hold the player forever");
}
