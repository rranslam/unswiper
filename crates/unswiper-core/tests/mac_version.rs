// SPDX-License-Identifier: MIT
//! The Mac app's version for `swift run` (AppInfo.fallbackVersion) must match the workspace
//! version; the .app gets the same value from build-dmg.sh, which reads Cargo.toml.

#[test]
fn mac_fallback_version_matches_cargo() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../apps/macos/Sources/Unswiper/App/AppInfo.swift"
    );
    let swift = std::fs::read_to_string(path).expect("AppInfo.swift");
    let want = format!(
        "static let fallbackVersion = \"{}\"",
        env!("CARGO_PKG_VERSION")
    );
    assert!(
        swift.contains(&want),
        "update AppInfo.fallbackVersion in {path} to {}",
        env!("CARGO_PKG_VERSION")
    );
}
