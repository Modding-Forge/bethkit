// SPDX-License-Identifier: Apache-2.0
//! Live round-trip suite against real compiled scripts.
//!
//! # Precondition
//!
//! | Variable              | Default                                                    |
//! |-----------------------|------------------------------------------------------------|
//! | `SKYRIM_DATA_DIR`     | `E:\SteamLibrary\steamapps\common\Skyrim Special Edition\Data` |
//! | `FO4_DATA_DIR`        | `E:\SteamLibrary\steamapps\common\Fallout 4\Data`          |
//! | `BETHKIT_PEX_CORPUS`  | unset; `;`-separated folders searched recursively           |
//!
//! Scripts are read from loose `.pex` files and from `.bsa`/`.ba2` archives.
//! Game data folders must round-trip without a single failure. Extra corpus
//! folders (for example a mod list) may contain broken scripts: those are
//! reported, but every script that parses must still round-trip exactly.
//! Missing folders skip their part, so CI passes without game data.
//!
//! # Run
//!
//! ```text
//! cargo test -p bethkit-pex --release --test corpus_live -- --nocapture
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use bethkit_pex::{PexFile, PexView};
use rayon::prelude::*;

const DEFAULT_SKYRIM: &str = r"E:\SteamLibrary\steamapps\common\Skyrim Special Edition\Data";
const DEFAULT_FO4: &str = r"E:\SteamLibrary\steamapps\common\Fallout 4\Data";

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// A script and where it came from.
struct Script {
    origin: String,
    bytes: Vec<u8>,
}

fn has_extension(path: &Path, extensions: &[&str]) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| extensions.iter().any(|e| ext.eq_ignore_ascii_case(e)))
}

/// Collects the scripts of one archive; unreadable archives are reported.
fn collect_archive(path: &Path, scripts: &mut Vec<Script>, problems: &mut Vec<String>) {
    let archive = match bethkit_bsa::open(path) {
        Ok(archive) => archive,
        Err(error) => {
            problems.push(format!("{}: {error}", path.display()));
            return;
        }
    };
    for entry in archive.entries() {
        if !entry.path.ends_with(".pex") {
            continue;
        }
        match archive.extract(&entry.path) {
            Some(Ok(bytes)) => scripts.push(Script {
                origin: format!("{}/{}", path.display(), entry.path),
                bytes: bytes.into_owned(),
            }),
            Some(Err(error)) => {
                problems.push(format!("{}/{}: {error}", path.display(), entry.path))
            }
            None => problems.push(format!(
                "{}/{}: listed but missing",
                path.display(),
                entry.path
            )),
        }
    }
}

/// Collects loose scripts and scripts in archives below `dir`.
fn collect(dir: &Path, recursive: bool, scripts: &mut Vec<Script>, problems: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path: PathBuf = entry.path();
        if path.is_dir() {
            if recursive
                || path
                    .file_name()
                    .is_some_and(|n| n.eq_ignore_ascii_case("scripts"))
            {
                collect(&path, recursive, scripts, problems);
            }
        } else if has_extension(&path, &["pex"]) {
            match std::fs::read(&path) {
                Ok(bytes) => scripts.push(Script {
                    origin: path.display().to_string(),
                    bytes,
                }),
                Err(error) => problems.push(format!("{}: {error}", path.display())),
            }
        } else if has_extension(&path, &["bsa", "ba2"]) {
            collect_archive(&path, scripts, problems);
        }
    }
}

/// Parses, rewrites and re-views every script; returns failures by message.
fn check(scripts: &[Script]) -> (BTreeMap<String, Vec<String>>, usize) {
    let results: Vec<Result<(), String>> = scripts
        .par_iter()
        .map(|script| {
            let parsed = PexFile::parse(&script.bytes).map_err(|e| format!("parse: {e}"))?;
            let written = parsed.write().map_err(|e| format!("write: {e}"))?;
            if written != script.bytes {
                return Err("round trip differs".to_owned());
            }
            let viewed = PexView::parse(&script.bytes)
                .and_then(PexView::into_file)
                .map_err(|e| format!("view: {e}"))?;
            if viewed != parsed {
                return Err("view differs".to_owned());
            }
            Ok(())
        })
        .collect();
    let mut failures: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut not_round_tripped = 0;
    for (script, result) in scripts.iter().zip(results) {
        if let Err(message) = result {
            if !message.starts_with("parse:") {
                not_round_tripped += 1;
            }
            // Keep the error kind without offsets so equal problems group together.
            let kind: String = message
                .split(" at offset")
                .next()
                .unwrap_or(&message)
                .to_owned();
            failures
                .entry(kind)
                .or_default()
                .push(script.origin.clone());
        }
    }
    (failures, not_round_tripped)
}

fn report(name: &str, scripts: &[Script], problems: &[String]) -> (usize, usize) {
    let bytes: usize = scripts.iter().map(|s| s.bytes.len()).sum();
    let started = Instant::now();
    let (failures, not_round_tripped) = check(scripts);
    let elapsed = started.elapsed();
    let failed: usize = failures.values().map(Vec::len).sum();
    println!(
        "{name}: {} scripts, {:.1} MB, {failed} failed, checked in {elapsed:?}",
        scripts.len(),
        bytes as f64 / 1e6
    );
    for (kind, origins) in &failures {
        println!("  {} x {kind}", origins.len());
        for origin in origins.iter().take(3) {
            println!("      {origin}");
        }
    }
    for problem in problems.iter().take(5) {
        println!("  archive problem: {problem}");
    }
    (failed, not_round_tripped)
}

/// Requires every script of the installed games to round-trip exactly.
#[test]
fn game_scripts_round_trip() -> TestResult {
    for (name, variable, default) in [
        ("Skyrim SE", "SKYRIM_DATA_DIR", DEFAULT_SKYRIM),
        ("Fallout 4", "FO4_DATA_DIR", DEFAULT_FO4),
    ] {
        let dir: PathBuf = std::env::var_os(variable).map_or_else(|| default.into(), PathBuf::from);
        if !dir.is_dir() {
            println!("SKIP {name}: {} not found", dir.display());
            continue;
        }

        // given
        let mut scripts: Vec<Script> = Vec::new();
        let mut problems: Vec<String> = Vec::new();
        collect(&dir, false, &mut scripts, &mut problems);

        // when
        let (failed, _) = report(name, &scripts, &problems);

        // then
        assert!(!scripts.is_empty(), "{name}: no scripts found");
        assert_eq!(failed, 0, "{name}: scripts failed");
    }
    Ok(())
}

/// Requires every parsable script of extra corpus folders to round-trip exactly.
#[test]
fn corpus_scripts_round_trip() -> TestResult {
    let Some(dirs) = std::env::var_os("BETHKIT_PEX_CORPUS") else {
        println!("SKIP: BETHKIT_PEX_CORPUS is not set");
        return Ok(());
    };

    // given
    let mut scripts: Vec<Script> = Vec::new();
    let mut problems: Vec<String> = Vec::new();
    for dir in std::env::split_paths(&dirs) {
        collect(&dir, true, &mut scripts, &mut problems);
    }

    // when
    let (_, not_round_tripped) = report("corpus", &scripts, &problems);

    // then
    assert_eq!(
        not_round_tripped, 0,
        "parsed scripts that did not round-trip"
    );
    Ok(())
}
