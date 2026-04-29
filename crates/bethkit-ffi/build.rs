// SPDX-License-Identifier: Apache-2.0
//!
//! cbindgen build script — generates `bethkit.h` from the FFI crate.

fn main() {
    let crate_dir: String =
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    cbindgen::Builder::new()
        .with_crate(crate_dir)
        .with_language(cbindgen::Language::C)
        .with_include_guard("BETHKIT_H")
        .generate()
        .expect("Unable to generate bethkit.h")
        .write_to_file("bethkit.h");
}
