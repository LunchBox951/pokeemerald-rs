//! Bounded retry for the Windows Cloud Files sync-root registration the
//! `windows_cloud_files` fixture needs. Platform-neutral so its logic is
//! tested on every host, not only Windows.

use std::time::Duration;

/// `HRESULT_FROM_WIN32(ERROR_NOT_FOUND)`: the one `CfRegisterSyncRoot` failure
/// this fixture treats as environmental.
///
/// `ERROR_NOT_FOUND` (1168, "Element not found") is in the Win32 System Error
/// Codes list
/// (<https://learn.microsoft.com/en-us/windows/win32/debug/system-error-codes--1000-1299->).
/// The `CfRegisterSyncRoot` reference
/// (<https://learn.microsoft.com/en-us/windows/win32/api/cfapi/nf-cfapi-cfregistersyncroot>)
/// documents only "an `HRESULT` error code" on failure and does not enumerate
/// which codes it returns, so the classification is empirical: this code means
/// the provider registration is not yet visible to the Cloud Files filter, a
/// runner-state race rather than a defect in the read path under test.
///
/// It is retried, never skipped: every other HRESULT fails at once, and so
/// does this one if it persists through every attempt. On a save-data-risk
/// change a leg that cannot build the fixture must fail, not read green while
/// the read-path tests never ran.
pub(super) const HRESULT_ERROR_NOT_FOUND: i32 = i32::from_ne_bytes(0x8007_0490_u32.to_ne_bytes());

/// Registration attempts before an environmental failure is reported as the
/// test's failure.
pub(super) const REGISTER_ATTEMPTS: u32 = 5;

/// Pause after the first environmental failure; it grows linearly per attempt
/// (200, 400, 600, 800 ms), at most two seconds across all retries.
pub(super) const REGISTER_BACKOFF: Duration = Duration::from_millis(200);

/// Whether `result` is a failure to register that retrying can clear and that
/// says nothing about the code under test.
const fn is_environmental_registration_failure(result: i32) -> bool {
    result == HRESULT_ERROR_NOT_FOUND
}

/// Runs `try_once` up to `attempts` times, sleeping `backoff * attempt` between
/// environmental failures.
///
/// Returns the first success. Returns `Err(hresult)` at the first failure that
/// is not environmental, which is never retried, and with the last
/// environmental HRESULT once every attempt has failed: persistent
/// unavailability still fails the test rather than skipping it, so a Windows
/// leg that never exercised the read path cannot read green.
pub(super) fn register_with_retry<T>(
    attempts: u32,
    backoff: Duration,
    mut try_once: impl FnMut() -> Result<T, i32>,
    mut sleep: impl FnMut(Duration),
) -> Result<T, i32> {
    let mut attempt = 1;
    loop {
        match try_once() {
            Err(result) if is_environmental_registration_failure(result) && attempt < attempts => {
                sleep(backoff * attempt);
                attempt += 1;
            }
            outcome => return outcome,
        }
    }
}

#[test]
fn a_transient_not_found_registration_is_retried_until_it_succeeds() {
    let mut calls = 0;
    let mut sleeps = Vec::new();
    let outcome = register_with_retry(
        REGISTER_ATTEMPTS,
        REGISTER_BACKOFF,
        || {
            calls += 1;
            if calls < 3 {
                Err(HRESULT_ERROR_NOT_FOUND)
            } else {
                Ok("registered")
            }
        },
        |pause| sleeps.push(pause),
    );

    assert_eq!(outcome, Ok("registered"));
    assert_eq!(calls, 3);
    assert_eq!(sleeps, [REGISTER_BACKOFF, REGISTER_BACKOFF * 2]);
}

#[test]
fn a_persistent_not_found_registration_is_bounded_and_still_fails() {
    let mut calls = 0;
    let mut slept = Duration::ZERO;
    let outcome: Result<(), i32> = register_with_retry(
        REGISTER_ATTEMPTS,
        REGISTER_BACKOFF,
        || {
            calls += 1;
            Err(HRESULT_ERROR_NOT_FOUND)
        },
        |pause| slept += pause,
    );

    assert_eq!(outcome, Err(HRESULT_ERROR_NOT_FOUND));
    assert_eq!(calls, REGISTER_ATTEMPTS);
    assert!(
        slept <= Duration::from_secs(2),
        "the back-off must stay short, slept {slept:?}"
    );
}

#[test]
fn a_registration_failure_that_is_not_environmental_fails_at_once() {
    // E_ACCESSDENIED: a bad fixture or a real fault, never a reason to skip.
    let access_denied = i32::from_ne_bytes(0x8007_0005_u32.to_ne_bytes());
    let mut calls = 0;
    let mut slept = false;
    let outcome: Result<(), i32> = register_with_retry(
        REGISTER_ATTEMPTS,
        REGISTER_BACKOFF,
        || {
            calls += 1;
            Err(access_denied)
        },
        |_| slept = true,
    );

    assert_eq!(outcome, Err(access_denied));
    assert_eq!(calls, 1);
    assert!(!slept, "a non-environmental failure must not be retried");
}

#[test]
fn the_not_found_hresult_is_the_win32_error_in_hresult_form() {
    assert_eq!(HRESULT_ERROR_NOT_FOUND, -2_147_023_728);
    assert!(is_environmental_registration_failure(
        HRESULT_ERROR_NOT_FOUND
    ));
}
