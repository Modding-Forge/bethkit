// SPDX-License-Identifier: Apache-2.0

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=BETHKIT_SCHEMA_BUNDLE");
    println!("cargo:rerun-if-env-changed=BETHKIT_SKYRIM_SE_SCHEMA");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_SCHEMA_SKYRIM_SE");

    let out_dir: PathBuf =
        PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is provided by Cargo"));
    let generated: PathBuf = out_dir.join("embedded_catalog.rs");

    if let Some(bundle) = env::var_os("BETHKIT_SCHEMA_BUNDLE").map(PathBuf::from) {
        embed_bundle(&bundle, &out_dir, &generated);
        return;
    }

    let mut packages: Vec<PathBuf> = Vec::new();
    if env::var_os("CARGO_FEATURE_SCHEMA_SKYRIM_SE").is_some() {
        let source: PathBuf = env::var_os("BETHKIT_SKYRIM_SE_SCHEMA")
            .map(PathBuf::from)
            .expect("schema-skyrim-se requires BETHKIT_SKYRIM_SE_SCHEMA to point to a package");
        packages.push(source);
    }
    embed_packages(&packages, &out_dir, &generated);
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
            "pub(crate) static EMBEDDED_PACKAGES: &[&[u8]] = &[];\n",
        ),
    )
    .expect("embedded catalog source can be written");
}

fn embed_packages(sources: &[PathBuf], out_dir: &Path, generated: &Path) {
    let mut declarations = String::from(
        "pub(crate) static EMBEDDED_BUNDLE: Option<&[u8]> = None;\n\
         pub(crate) static EMBEDDED_PACKAGES: &[&[u8]] = &[\n",
    );
    for (index, source) in sources.iter().enumerate() {
        if !source.is_file() {
            panic!("schema package does not exist: {}", source.display());
        }
        println!("cargo:rerun-if-changed={}", source.display());
        let file_name: String = format!("schema-package-{index}.bkschema");
        fs::copy(source, out_dir.join(&file_name))
            .expect("schema package can be copied into OUT_DIR");
        declarations.push_str(&format!(
            "    include_bytes!(concat!(env!(\"OUT_DIR\"), \"/{file_name}\")),\n"
        ));
    }
    declarations.push_str("];\n");
    fs::write(generated, declarations).expect("embedded catalog source can be written");
}
