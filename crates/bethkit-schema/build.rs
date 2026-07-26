// SPDX-License-Identifier: Apache-2.0

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=BETHKIT_SCHEMA_BUNDLE");

    let manifest_dir: PathBuf = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is provided by Cargo"),
    );
    let repository_bundle: PathBuf = manifest_dir.join("../../schemas/embedded/bethkit.bkschemas");
    println!("cargo:rerun-if-changed={}", repository_bundle.display());

    let out_dir: PathBuf =
        PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is provided by Cargo"));
    let generated: PathBuf = out_dir.join("embedded_catalog.rs");

    let source: Option<PathBuf> = env::var_os("BETHKIT_SCHEMA_BUNDLE")
        .map(PathBuf::from)
        .or_else(|| repository_bundle.is_file().then_some(repository_bundle));
    match source {
        Some(path) => embed_bundle(&path, &out_dir, &generated),
        None => {
            fs::write(
                generated,
                "pub(crate) static EMBEDDED_BUNDLE: Option<&[u8]> = None;\n",
            )
            .expect("embedded catalog source can be written");
        }
    }
}

fn embed_bundle(source: &Path, out_dir: &Path, generated: &Path) {
    if !source.is_file() {
        panic!(
            "BETHKIT_SCHEMA_BUNDLE does not point to a file: {}",
            source.display()
        );
    }

    let destination: PathBuf = out_dir.join("schema-catalog.bkschemas");
    fs::copy(source, destination).expect("schema bundle can be copied into OUT_DIR");
    fs::write(
        generated,
        concat!(
            "pub(crate) static EMBEDDED_BUNDLE: Option<&[u8]> = ",
            "Some(include_bytes!(concat!(env!(\"OUT_DIR\"), ",
            "\"/schema-catalog.bkschemas\")));\n",
        ),
    )
    .expect("embedded catalog source can be written");
}
