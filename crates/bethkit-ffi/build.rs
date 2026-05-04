// SPDX-License-Identifier: Apache-2.0
//! cbindgen build script — generates `bethkit.h` from the FFI crate.
//!
//! Header generation only runs when the `generate-header` feature is enabled:
//!
//! ```text
//! cargo build -p bethkit-ffi --features generate-header
//! ```
//!
//! Without the feature, the pre-committed `bethkit.h` is used as-is.

fn main() {
    #[cfg(feature = "generate-header")]
    generate_header();
}

#[cfg(feature = "generate-header")]
fn generate_header() {
    let crate_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");

    // cbindgen.toml lives at the workspace root, two directories above the crate
    // (crates/bethkit-ffi → crates/ → workspace root).
    let config_path = std::path::PathBuf::from(&crate_dir)
        .parent()
        .expect("crate dir has a parent (crates/)")
        .parent()
        .expect("crates/ dir has a parent (workspace root)")
        .join("cbindgen.toml");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed={}", config_path.display());

    let config = cbindgen::Config::from_file(&config_path)
        .expect("Unable to load cbindgen.toml from workspace root");

    // Write into target/ first to avoid Windows file-lock issues when
    // editors have the source-tree copy open.
    let tmp = std::path::PathBuf::from(&out_dir).join("bethkit.h");
    cbindgen::Builder::new()
        .with_crate(&crate_dir)
        .with_config(config)
        .generate()
        .expect("Unable to generate bethkit.h")
        .write_to_file(&tmp);

    // Copy into the crate directory only when content has changed.
    let dest = std::path::PathBuf::from(&crate_dir).join("bethkit.h");
    let new_content = std::fs::read(&tmp).expect("Failed to read generated header");
    let needs_update = dest
        .exists()
        .then(|| std::fs::read(&dest).ok())
        .flatten()
        .map(|old| old != new_content)
        .unwrap_or(true);

    if needs_update {
        std::fs::write(&dest, &new_content).expect("Failed to write bethkit.h");
    }
}
