use std::env;

fn main() {
    println!("cargo::rerun-if-env-changed=POKEEMERALD_RELEASE_CHANNEL");
    let channel = env::var("POKEEMERALD_RELEASE_CHANNEL").unwrap_or_else(|_| "dev".into());
    let directory = match channel.as_str() {
        "dev" => "pokeemerald-rs",
        "unstable" => "pokeemerald-rs/unstable",
        "stable" => "pokeemerald-rs/stable",
        "main" => "pokeemerald-rs/main",
        _ => panic!("POKEEMERALD_RELEASE_CHANNEL must be dev, unstable, stable, or main"),
    };
    println!("cargo::rustc-env=POKEEMERALD_BUILD_CHANNEL={channel}");
    println!("cargo::rustc-env=POKEEMERALD_APP_DATA_SUBDIRECTORY={directory}");
}
