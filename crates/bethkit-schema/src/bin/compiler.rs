// SPDX-License-Identifier: Apache-2.0
//!
//! Command-line compiler for normalized JSON schema packages.

use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use bethkit_schema::{encode_bundle, SchemaGame, SchemaPackage, ValidationStatus};

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
        Some("verify-release") => verify_release(&arguments[1..]),
        _ => Err(usage().into()),
    }
}

fn verify_release(arguments: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if arguments.len() != SchemaGame::all().len() {
        return Err(format!(
            "release requires {} packages, received {}",
            SchemaGame::all().len(),
            arguments.len()
        )
        .into());
    }
    let mut games = BTreeSet::new();
    for argument in arguments {
        let package = SchemaPackage::open(Path::new(argument))?;
        let manifest = package.manifest();
        if !games.insert(manifest.game) {
            return Err(format!(
                "release contains duplicate package for {}",
                manifest.game.slug()
            )
            .into());
        }
        if manifest.validation_status != ValidationStatus::Approved {
            return Err(format!("{} package is not approved", manifest.game.slug()).into());
        }
        if manifest.byte_coverage != 1.0 {
            return Err(format!(
                "{} package does not have complete byte coverage",
                manifest.game.slug()
            )
            .into());
        }
        if manifest.validated_records == 0 || !is_sha256(&manifest.corpus_sha256) {
            return Err(format!(
                "{} package has no verified differential corpus",
                manifest.game.slug()
            )
            .into());
        }
    }
    let expected: BTreeSet<SchemaGame> = SchemaGame::all().into_iter().collect();
    if games != expected {
        return Err("release package set does not cover all supported games".into());
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        && value.bytes().any(|byte| byte != b'0')
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
        .map(|argument| SchemaPackage::open(Path::new(argument)))
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
        "  bethkit-schema-compiler bundle <input.bkschema>... <output.bkschemas>",
        "\n  bethkit-schema-compiler verify-release <input.bkschema>...",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rejects empty and malformed differential-corpus digests.
    #[test]
    fn release_gate_requires_nonzero_sha256() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        // given
        let valid = format!("{}1", "0".repeat(63));

        // when
        let valid_result = is_sha256(&valid);

        // then
        assert!(valid_result);
        assert!(!is_sha256(&"0".repeat(64)));
        assert!(!is_sha256("not-a-digest"));
        Ok(())
    }
}
