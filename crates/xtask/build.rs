//! Records the Cargo profile this binary was built with, so `e2e --release`
//! can tell a release build from a debug one (F-3, V-1; issue #1878). Debug assertions
//! and target-directory names cannot: both are configurable per profile.

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-env-changed=PROFILE");
    let profile = std::env::var("PROFILE").unwrap_or_default();
    println!("cargo::rustc-env=XTASK_BUILD_PROFILE={profile}");
}
