use super::{next_attempt, SERIES_ATTEMPTS};

/// Plays one whole series, returning each attempt's power and remaining
/// attempts.
fn series(base_power: u8, defense_curl: bool) -> Vec<(u32, u8)> {
    let mut lock = None;
    let mut attempts = Vec::new();
    for _ in 0..SERIES_ATTEMPTS {
        let attempt = next_attempt(base_power, lock, defense_curl);
        attempts.push((attempt.power, attempt.attempts_left));
        lock = Some(attempt.attempts_left);
    }
    attempts
}

#[test]
fn powers_double_per_landed_attempt_and_release_on_the_fifth() {
    assert_eq!(
        series(30, false),
        [(30, 4), (60, 3), (120, 2), (240, 1), (480, 0)]
    );
}

#[test]
fn defense_curl_doubles_every_attempt_once_more() {
    assert_eq!(
        series(30, true),
        [(60, 4), (120, 3), (240, 2), (480, 1), (960, 0)]
    );
}
