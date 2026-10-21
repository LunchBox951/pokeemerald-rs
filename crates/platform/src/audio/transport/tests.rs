//! Drives the transport state machine against a fake host in fake time.

use std::any::Any;
use std::sync::Mutex;
use std::thread::ThreadId;

use super::*;
use crate::audio::AudioOutput;
use crate::ring::ring_buffer;

mod reroute;
mod setup_and_backlog;
mod stopped_queue_and_cooldown;

type DataFn<T> = Box<dyn FnMut(&mut [T], &cpal::OutputCallbackInfo) + Send>;

struct Callbacks {
    data: Box<dyn Any + Send>,
    error: Box<dyn FnMut(cpal::Error) + Send>,
}

// A fake host world: independent failure switches, not a state machine.
#[allow(clippy::struct_excessive_bools)]
#[derive(Default)]
struct World {
    default: Option<String>,
    unsupported: bool,
    format: Option<cpal::SampleFormat>,
    fail_build: bool,
    /// The kind `build` fails with while `fail_build` is set.
    build_kind: Option<cpal::ErrorKind>,
    fail_play: bool,
    no_hw_pause: bool,
    /// Fake monotonic time the transport reads after blocking calls.
    clock: Duration,
    /// How far each `build` advances `clock`, as a slow OS call would.
    build_latency: Duration,
    /// Every call that reaches the audio stack, host or device or stream.
    ops: usize,
    discoveries: usize,
    builds: usize,
    plays: usize,
    pauses: usize,
    log: Vec<&'static str>,
    streams: Vec<Callbacks>,
    discovery_threads: Vec<ThreadId>,
}

type SharedWorld = Arc<Mutex<World>>;

fn world(f: impl FnOnce(&mut World)) -> SharedWorld {
    let mut w = World::default();
    f(&mut w);
    Arc::new(Mutex::new(w))
}

fn with(world: &SharedWorld) -> std::sync::MutexGuard<'_, World> {
    world.lock().unwrap()
}

struct FakeHost(SharedWorld);

struct FakeDevice(SharedWorld);

struct FakeStream(SharedWorld, usize);

impl OutputHost for FakeHost {
    type Device = FakeDevice;

    fn default_output(&mut self) -> Result<Option<(String, FakeDevice)>, cpal::Error> {
        let mut w = with(&self.0);
        w.ops += 1;
        w.discoveries += 1;
        w.discovery_threads.push(std::thread::current().id());
        Ok(w.default
            .clone()
            .map(|id| (id, FakeDevice(Arc::clone(&self.0)))))
    }
}

impl OutputDevice for FakeDevice {
    type Stream = FakeStream;

    fn supported_output_configs(
        &self,
    ) -> Result<Vec<cpal::SupportedStreamConfigRange>, cpal::Error> {
        let mut w = with(&self.0);
        w.ops += 1;
        if w.unsupported {
            return Ok(Vec::new());
        }
        let format = w.format.unwrap_or(cpal::SampleFormat::F32);
        Ok(vec![cpal::SupportedStreamConfigRange::new(
            AudioOutput::CHANNELS,
            8_000,
            48_000,
            cpal::SupportedBufferSize::Unknown,
            format,
        )])
    }

    fn build_output_stream<T, D, E>(
        &self,
        _config: cpal::StreamConfig,
        data_callback: D,
        error_callback: E,
        _timeout: Option<Duration>,
    ) -> Result<FakeStream, cpal::Error>
    where
        T: cpal::SizedSample + 'static,
        D: FnMut(&mut [T], &cpal::OutputCallbackInfo) + Send + 'static,
        E: FnMut(cpal::Error) + Send + 'static,
    {
        let mut w = with(&self.0);
        w.ops += 1;
        let latency = w.build_latency;
        w.clock += latency;
        if w.fail_build {
            return Err(cpal::Error::new(
                w.build_kind.unwrap_or(cpal::ErrorKind::BackendError),
            ));
        }
        w.builds += 1;
        w.log.push("build");
        let index = w.streams.len();
        let data: DataFn<T> = Box::new(data_callback);
        w.streams.push(Callbacks {
            data: Box::new(data),
            error: Box::new(error_callback),
        });
        Ok(FakeStream(Arc::clone(&self.0), index))
    }
}

impl OutputStream for FakeStream {
    fn play(&self) -> Result<(), cpal::Error> {
        let mut w = with(&self.0);
        w.ops += 1;
        if w.fail_play {
            return Err(cpal::Error::new(cpal::ErrorKind::DeviceNotAvailable));
        }
        w.plays += 1;
        Ok(())
    }

    fn pause(&self) -> Result<(), cpal::Error> {
        let mut w = with(&self.0);
        w.ops += 1;
        if w.no_hw_pause {
            return Err(cpal::Error::new(cpal::ErrorKind::UnsupportedOperation));
        }
        w.pauses += 1;
        Ok(())
    }
}

impl Drop for FakeStream {
    fn drop(&mut self) {
        let mut w = with(&self.0);
        w.ops += 1;
        w.log.push("drop");
        // A real stream's data callback, and the consumer it owns, die with
        // the stream; its error callback may still be invoked late.
        let index = self.1;
        w.streams[index].data = Box::new(());
    }
}

/// Invoke stream `index`'s data callback once, as the audio thread would.
fn run_data_callback<T: cpal::SizedSample + Default + 'static>(
    world: &SharedWorld,
    index: usize,
    len: usize,
) {
    let mut w = with(world);
    let callback = w.streams[index]
        .data
        .downcast_mut::<DataFn<T>>()
        .expect("callback sample format");
    let mut buffer = vec![T::default(); len];
    let stamp = cpal::OutputStreamTimestamp {
        callback: cpal::StreamInstant::ZERO,
        playback: cpal::StreamInstant::from_millis(10),
    };
    callback(&mut buffer, &cpal::OutputCallbackInfo::new(stamp));
}

fn raise_error(world: &SharedWorld, index: usize, kind: cpal::ErrorKind) {
    let mut w = with(world);
    (w.streams[index].error)(cpal::Error::new(kind));
}

struct Rig {
    transport: Transport<FakeHost>,
    producer: Producer,
    shared: Arc<Shared>,
}

fn rig(world: &SharedWorld, frames: usize) -> Rig {
    let (producer, consumer) = ring_buffer(frames * 2);
    drop(consumer);
    let shared = Arc::new(Shared::new(TransportState::Connecting, 13_379, 0));
    let clock = Arc::clone(world);
    let transport = Transport::new(
        FakeHost(Arc::clone(world)),
        producer.clone(),
        Arc::clone(&shared),
    )
    .with_clock(move || with(&clock).clock);
    Rig {
        transport,
        producer,
        shared,
    }
}

impl Rig {
    fn events(&self) -> Vec<TransportState> {
        std::iter::from_fn(|| self.shared.pop_event()).collect()
    }
}

const MS: fn(u64) -> Duration = Duration::from_millis;

#[test]
fn start_with_no_output_is_a_state_not_an_error_and_the_producer_still_works() {
    let w = world(|_| {});
    let mut rig = rig(&w, 4);
    rig.transport.step(Duration::ZERO);

    assert_eq!(
        rig.events(),
        [TransportState::Unavailable(TransportFailure::NoOutput)]
    );
    // The ring behaves as a full or empty ring would with a stalled consumer.
    assert_eq!(rig.producer.push(&[0.5; 8]), 8);
    assert_eq!(rig.producer.push(&[0.5; 4]), 0);
    assert_eq!(rig.producer.underruns(), 0);

    with(&w).default = Some("a".into());
    let due = rig.transport.step(Duration::ZERO);
    assert_eq!(due, MS(100), "not retried before its deadline");
    rig.transport.step(MS(100));
    assert_eq!(rig.events(), [TransportState::Ready]);
    assert_eq!(
        rig.shared.device_sample_rate.load(Ordering::Acquire),
        13_379
    );
}

#[test]
fn retries_are_bounded_by_a_burst_then_a_cooldown() {
    let w = world(|_| {});
    let mut rig = rig(&w, 4);
    let mut attempts = Vec::new();
    let mut now = Duration::ZERO;
    // An attempt reschedules the retry; a discovery probe in a cooldown
    // leaves it alone while the identity stays the same.
    while attempts.len() < 6 {
        let before = rig.transport.next_attempt;
        let next = rig.transport.step(now);
        if rig.transport.next_attempt != before {
            attempts.push(now);
        }
        // Waking early does nothing.
        let early = rig.transport.step(now + MS(1));
        assert_eq!(early, next);
        now = next;
    }
    assert_eq!(
        attempts,
        [MS(0), MS(100), MS(300), MS(700), MS(5_700), MS(5_800)]
    );
    // Six attempts plus one probe a second through the cooldown.
    assert_eq!(with(&w).discoveries, 6 + 4);
    assert_eq!(
        rig.events(),
        [TransportState::Unavailable(TransportFailure::NoOutput)],
        "a repeated failure is reported once"
    );
}

#[test]
fn unsupported_configuration_is_reported_and_retried_within_bounds() {
    let w = world(|w| {
        w.default = Some("a".into());
        w.unsupported = true;
    });
    let mut rig = rig(&w, 4);
    let next = rig.transport.step(Duration::ZERO);
    assert_eq!(next, MS(100));
    assert_eq!(
        rig.events(),
        [TransportState::Unavailable(TransportFailure::Unsupported)]
    );
    assert_eq!(with(&w).builds, 0);
}

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

#[test]
fn a_replaced_default_output_is_followed_with_the_old_stream_released_first() {
    let w = world(|w| w.default = Some("a".into()));
    let mut rig = rig(&w, 4);
    rig.transport.step(Duration::ZERO);
    rig.events();

    with(&w).default = Some("b".into());
    // The old stream is still healthy; only rediscovery notices.
    rig.transport.step(MS(500));
    assert_eq!(rig.events(), []);
    rig.transport.step(MS(1_000));
    assert_eq!(
        rig.events(),
        [
            TransportState::Unavailable(TransportFailure::DefaultChanged),
            TransportState::Ready
        ]
    );
    assert_eq!(with(&w).log, ["build", "drop", "build"]);
}

#[test]
fn a_vanished_default_output_recovers_when_one_returns() {
    let w = world(|w| w.default = Some("a".into()));
    let mut rig = rig(&w, 4);
    rig.transport.step(Duration::ZERO);
    rig.events();

    with(&w).default = None;
    rig.transport.step(MS(1_000));
    assert_eq!(
        rig.events(),
        [TransportState::Unavailable(TransportFailure::NoOutput)]
    );
    with(&w).default = Some("a".into());
    let mut now = MS(1_000);
    while rig.events().is_empty() && now < MS(7_000) {
        now = rig.transport.step(now);
    }
    assert_eq!(with(&w).builds, 2);
}

#[test]
fn a_stream_that_keeps_failing_cannot_reset_the_retry_budget() {
    let w = world(|w| w.default = Some("a".into()));
    let mut rig = rig(&w, 4);
    let mut builds = Vec::new();
    let mut now = Duration::ZERO;
    while builds.len() < 5 {
        let before = with(&w).builds;
        rig.transport.step(now);
        if with(&w).builds > before {
            builds.push(now);
            let index = before;
            raise_error(&w, index, cpal::ErrorKind::StreamInvalidated);
            now += HEALTH_POLL;
            rig.transport.step(now);
        }
        now = rig.transport.step(now);
    }
    assert_eq!(builds, [MS(0), MS(120), MS(340), MS(760), MS(5_780)]);
}

#[test]
fn queued_history_is_discarded_when_a_stream_reattaches() {
    let w = world(|w| w.default = Some("a".into()));
    let mut rig = rig(&w, 4);
    rig.shared.running.store(true, Ordering::Release);
    rig.transport.step(Duration::ZERO);
    assert_eq!(rig.events().last(), Some(&TransportState::Ready));

    // The stream is lost; the producer keeps filling the ring meanwhile.
    assert_eq!(rig.producer.push(&[0.5; 4]), 4);
    raise_error(&w, 0, cpal::ErrorKind::DeviceNotAvailable);
    rig.transport.step(MS(20));
    assert_eq!(rig.producer.push(&[0.5; 4]), 4);
    rig.transport.step(MS(120));

    assert_eq!(rig.events().last(), Some(&TransportState::Ready));
    assert_eq!(rig.producer.available_space(), rig.producer.capacity());
    assert_eq!(rig.producer.underruns(), 0, "disposal is not an underrun");
}

#[test]
fn a_queue_prefilled_before_the_first_start_is_kept() {
    let w = world(|w| w.default = Some("a".into()));
    let mut rig = rig(&w, 4);
    assert_eq!(rig.producer.push(&[0.25; 6]), 6);
    rig.transport.step(Duration::ZERO);
    assert_eq!(rig.producer.available_space(), 2);
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
fn playback_intent_is_applied_by_the_worker_and_failures_are_reported() {
    let w = world(|w| w.default = Some("a".into()));
    let mut rig = rig(&w, 4);
    rig.transport.step(Duration::ZERO);
    assert_eq!(
        with(&w).plays,
        0,
        "a stopped transport leaves the stream paused"
    );

    rig.shared.running.store(true, Ordering::Release);
    rig.transport.step(MS(20));
    rig.transport.step(MS(40));
    assert_eq!(with(&w).plays, 1, "start is applied once");

    rig.shared.running.store(false, Ordering::Release);
    rig.transport.step(MS(60));
    assert_eq!(with(&w).pauses, 1);

    rig.events();
    with(&w).fail_play = true;
    rig.shared.running.store(true, Ordering::Release);
    rig.transport.step(MS(80));
    assert_eq!(
        rig.events(),
        [TransportState::Unavailable(TransportFailure::StreamLost)]
    );
}

#[test]
fn a_build_failure_is_reported_and_retried() {
    let w = world(|w| {
        w.default = Some("a".into());
        w.fail_build = true;
    });
    let mut rig = rig(&w, 4);
    rig.transport.step(Duration::ZERO);
    assert_eq!(
        rig.events(),
        [TransportState::Unavailable(TransportFailure::ConnectFailed)]
    );
    with(&w).fail_build = false;
    rig.transport.step(MS(100));
    assert_eq!(rig.events(), [TransportState::Ready]);
}

#[test]
fn the_worker_connects_off_the_caller_thread_and_start_never_waits() {
    let w = world(|_| {});
    let mut output =
        AudioOutput::open_with_host(FakeHost(Arc::clone(&w)), 64).expect("worker spawns");
    output.start().expect("start records intent");
    assert!(output.is_running());

    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut seen = Vec::new();
    while seen.is_empty() {
        assert!(std::time::Instant::now() < deadline, "no first transition");
        seen.extend(output.poll_transport_state());
        std::thread::sleep(MS(5));
    }
    with(&w).default = Some("a".into());
    while !seen.contains(&TransportState::Ready) {
        assert!(
            std::time::Instant::now() < deadline,
            "never connected: {seen:?}"
        );
        seen.extend(output.poll_transport_state());
        std::thread::sleep(MS(5));
    }
    assert_eq!(
        seen[0],
        TransportState::Unavailable(TransportFailure::NoOutput)
    );
    assert_eq!(output.transport_state(), TransportState::Ready);
    {
        let w = with(&w);
        assert!(!w.discovery_threads.is_empty());
        assert!(w
            .discovery_threads
            .iter()
            .all(|id| *id != std::thread::current().id()));
    }

    drop(output);
    let w = with(&w);
    assert_eq!(w.log.last(), Some(&"drop"), "shutdown releases the stream");
}

#[test]
fn the_null_backend_has_no_transport() {
    let mut output = AudioOutput::null(16);
    assert_eq!(output.transport_state(), TransportState::Null);
    assert_eq!(output.poll_transport_state(), None);
    output.start().unwrap();
    output.start().unwrap();
    assert!(output.is_running());
    output.stop().unwrap();
    assert!(!output.is_running());
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

#[test]
fn attaching_before_start_reaches_ready_without_hardware_pause() {
    let w = world(|w| {
        w.default = Some("a".into());
        w.no_hw_pause = true;
    });
    let mut rig = rig(&w, 4);
    rig.transport.step(Duration::ZERO);
    assert_eq!(rig.events(), [TransportState::Ready]);
}

#[test]
fn startup_prefill_survives_start_before_the_first_attach() {
    let w = world(|w| w.default = Some("a".into()));
    let mut rig = rig(&w, 4);
    assert_eq!(rig.producer.push(&[0.25; 6]), 6);
    rig.shared.running.store(true, Ordering::Release);
    rig.transport.step(Duration::ZERO);
    assert_eq!(rig.events().last(), Some(&TransportState::Ready));
    assert_eq!(rig.producer.available_space(), 2);
}
