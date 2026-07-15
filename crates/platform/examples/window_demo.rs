//! Manual smoke test for the `platform` pipeline: opens a window, pumps
//! keyboard input, and presents a static test pattern at the GBA's native
//! refresh cadence, integer-scaled and letterboxed to the window.
//!
//! Not run in CI — CI is headless. Run locally with:
//!
//! ```sh
//! cargo run -p platform --example window_demo
//! ```
//!
//! Close the window, or hold Start (Enter), to exit.

use platform::{Buttons, Platform};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut platform = Platform::new("pokeemerald-rs — platform smoke test")?;
    let frame = platform::present::test_pattern();

    while platform.pump()? {
        if platform.buttons().is_held(Buttons::START) {
            break;
        }
        platform.present(&frame)?;
        platform.wait_for_next_frame();
    }

    Ok(())
}
