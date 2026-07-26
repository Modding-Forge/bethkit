// SPDX-License-Identifier: Apache-2.0
//!
//! Command-line compiler for normalized JSON schema packages.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use bethkit_schema::{encode_bundle, SchemaPackage};

fn main() {
    if let Err(error) = run() {
        eprintln!("bethkit-schema-compiler: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<String> = env::args().skip(1).collect();
    match arguments.first().map(String::as_str) {
        Some("compile") => compile_package(&arguments[1..]),
        Some("bundle") => compile_bundle(&arguments[1..]),
        _ => Err(usage().into()),
    }
}

fn compile_package(arguments: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if arguments.len() != 2 {
        return Err(usage().into());
    }
    let input: PathBuf = PathBuf::from(&arguments[0]);
    let output: PathBuf = PathBuf::from(&arguments[1]);
    let package: SchemaPackage = parse_json_package(&input)?;
    fs::write(output, package.to_bytes()?)?;
    Ok(())
}

fn compile_bundle(arguments: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if arguments.len() < 2 {
        return Err(usage().into());
    }
    let output: PathBuf = PathBuf::from(arguments.last().expect("argument count was checked"));
    let packages: Vec<SchemaPackage> = arguments[..arguments.len() - 1]
        .iter()
        .map(|argument| parse_json_package(Path::new(argument)))
        .collect::<Result<Vec<_>, _>>()?;
    fs::write(output, encode_bundle(&packages)?)?;
    Ok(())
}

fn parse_json_package(path: &Path) -> Result<SchemaPackage, Box<dyn std::error::Error>> {
    #[derive(serde::Deserialize)]
    struct JsonPackage {
        manifest: bethkit_schema::SchemaManifest,
        records: Vec<bethkit_schema::SchemaRecord>,
        #[serde(default)]
        callback_bindings: Vec<bethkit_schema::CallbackBinding>,
    }

    let bytes: Vec<u8> = fs::read(path)?;
    let package: JsonPackage = serde_json::from_slice(&bytes)?;
    Ok(SchemaPackage::new_with_callbacks(
        package.manifest,
        package.records,
        package.callback_bindings,
    )?)
}

fn usage() -> &'static str {
    concat!(
        "usage:\n",
        "  bethkit-schema-compiler compile <input.json> <output.bkschema>\n",
        "  bethkit-schema-compiler bundle <input.json>... <output.bkschemas>",
    )
}
