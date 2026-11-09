//! Stream error classification and the callbacks' boundary: callbacks do no
//! connection work, and a replaced stream's late error stays its own.

use super::*;

#[test]
fn a_fatal_stream_error_rebuilds_on_the_current_default() {
    let w = world(|w| w.default = Some("a".into()));
    let mut rig = rig(&w, 4);
    rig.transport.step(Duration::ZERO);
    assert_eq!(rig.events(), [TransportState::Ready]);

    raise_error(&w, 0, cpal::ErrorKind::DeviceNotAvailable);
    // Nothing happens on the audio thread beyond the flag.
    assert_eq!(with(&w).builds, 1);
    let next = rig.transport.step(MS(20));
    assert_eq!(next, MS(120));
    assert_eq!(
        rig.events(),
        [TransportState::Unavailable(TransportFailure::StreamLost)]
    );

    rig.transport.step(MS(120));
    assert_eq!(rig.events(), [TransportState::Ready]);
    assert_eq!(with(&w).log, ["build", "drop", "build"]);
}

#[test]
fn recoverable_stream_errors_are_counted_but_do_not_rebuild() {
    let w = world(|w| w.default = Some("a".into()));
    let mut rig = rig(&w, 4);
    rig.transport.step(Duration::ZERO);
    rig.events();
    for kind in [
        cpal::ErrorKind::Xrun,
        cpal::ErrorKind::RealtimeDenied,
        cpal::ErrorKind::DeviceChanged,
    ] {
        raise_error(&w, 0, kind);
    }
    rig.transport.step(MS(20));
    assert_eq!(rig.events(), []);
    assert_eq!(with(&w).builds, 1);
    assert_eq!(rig.shared.stream_errors.load(Ordering::Relaxed), 3);
}

#[test]
fn a_late_error_from_a_replaced_stream_does_not_poison_its_successor() {
    let w = world(|w| w.default = Some("a".into()));
    let mut rig = rig(&w, 4);
    rig.transport.step(Duration::ZERO);
    raise_error(&w, 0, cpal::ErrorKind::DeviceNotAvailable);
    rig.transport.step(MS(20));
    rig.transport.step(MS(120));
    rig.events();

    raise_error(&w, 0, cpal::ErrorKind::DeviceNotAvailable);
    rig.transport.step(MS(140));
    assert_eq!(rig.events(), []);
    assert_eq!(with(&w).builds, 2);
}

fn assert_callbacks_do_no_connection_work<T: cpal::SizedSample + Default + 'static>() {
    let w = world(|w| {
        w.default = Some("a".into());
        if std::any::TypeId::of::<T>() == std::any::TypeId::of::<i16>() {
            w.format = Some(cpal::SampleFormat::I16);
        }
    });
    let mut rig = rig(&w, 64);
    rig.transport.step(Duration::ZERO);
    assert_eq!(with(&w).builds, 1);

    let ops = with(&w).ops;
    for _ in 0..3 {
        run_data_callback::<T>(&w, 0, 128);
    }
    raise_error(&w, 0, cpal::ErrorKind::Xrun);
    raise_error(&w, 0, cpal::ErrorKind::DeviceNotAvailable);
    assert_eq!(
        with(&w).ops,
        ops,
        "callbacks must not discover, negotiate, build, play, pause or drop"
    );
    // The reconnect happens only when the worker steps.
    rig.transport.step(MS(20));
    rig.transport.step(MS(120));
    assert!(with(&w).ops > ops);
}

#[test]
fn callbacks_do_no_connection_work_in_either_sample_format() {
    assert_callbacks_do_no_connection_work::<f32>();
    assert_callbacks_do_no_connection_work::<i16>();
}

#[test]
fn a_busy_device_error_rebuilds_because_the_stream_may_have_ended() {
    let w = world(|w| w.default = Some("a".into()));
    let mut rig = rig(&w, 4);
    rig.transport.step(Duration::ZERO);
    rig.events();
    raise_error(&w, 0, cpal::ErrorKind::DeviceBusy);
    rig.transport.step(MS(20));
    assert_eq!(
        rig.events(),
        [TransportState::Unavailable(TransportFailure::StreamLost)]
    );
}
