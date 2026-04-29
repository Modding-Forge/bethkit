// SPDX-License-Identifier: Apache-2.0
//!
//! Integration tests for `bethkit-bsa` using real Skyrim SE archive files.
//!
//! Tests are gated behind the existence of the test BSA files in
//! `tests/testdata/`. The helper macro `skip_if_missing!` skips a test
//! gracefully when the file is absent so CI without test assets still passes.

use std::path::Path;

/// Path to the shared testdata directory.
fn testdata(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/")
        .parent()
        .expect("workspace root")
        .join("tests/testdata")
        .join(name)
}

/// Skip the test (with a notice) if the file does not exist.
macro_rules! require_file {
    ($path:expr) => {{
        let p: std::path::PathBuf = $path;
        if !p.exists() {
            eprintln!("SKIP: test file not found: {}", p.display());
            return Ok(());
        }
        p
    }};
}

/// Opens the Skyrim - Misc BSA and verifies that at least one file is present.
#[test]
fn sse_bsa_open_lists_files() -> Result<(), Box<dyn std::error::Error>> {
    // given
    let path = require_file!(testdata("Skyrim - Misc.bsa"));

    // when
    let archive = bethkit_bsa::open(&path)?;

    // then
    assert!(archive.file_count() > 0, "expected at least one file");
    assert_eq!(archive.format_name(), "BSA TES4/FO3/SSE");
    Ok(())
}

/// Opens the smallest CC BSA and verifies the file count and format name.
#[test]
fn sse_cc_bsa_firewood() -> Result<(), Box<dyn std::error::Error>> {
    // given
    let path = require_file!(testdata("ccqdrsse002-firewood.bsa"));

    // when
    let archive = bethkit_bsa::open(&path)?;

    // then
    assert!(archive.file_count() > 0);
    eprintln!(
        "ccqdrsse002-firewood.bsa: {} files ({})",
        archive.file_count(),
        archive.format_name()
    );
    Ok(())
}

/// Opens MarketplaceTextures BSA and verifies that entry paths look sane.
#[test]
fn sse_bsa_paths_are_normalised() -> Result<(), Box<dyn std::error::Error>> {
    // given
    let path = require_file!(testdata("MarketplaceTextures.bsa"));

    // when
    let archive = bethkit_bsa::open(&path)?;

    // then
    for entry in archive.entries() {
        // All paths must be lowercase.
        assert_eq!(
            entry.path,
            entry.path.to_ascii_lowercase(),
            "path is not lowercase: {}",
            entry.path
        );
        // No backslashes in paths.
        assert!(
            !entry.path.contains('\\'),
            "path contains backslash: {}",
            entry.path
        );
    }
    Ok(())
}

/// Extracts the first file from each available BSA and checks minimum size.
#[test]
fn sse_bsa_extract_first_file() -> Result<(), Box<dyn std::error::Error>> {
    // given
    let candidates = [
        "Skyrim - Misc.bsa",
        "ccqdrsse002-firewood.bsa",
        "MarketplaceTextures.bsa",
    ];

    for name in &candidates {
        let path = testdata(name);
        if !path.exists() {
            eprintln!("SKIP {name}: not found");
            continue;
        }

        // when
        let archive = bethkit_bsa::open(&path)?;
        if archive.file_count() == 0 {
            continue;
        }
        let first = &archive.entries()[0].path.clone();
        let data = archive
            .extract(first)
            .ok_or("first entry must be findable")??
            .to_vec();

        // then
        assert!(
            !data.is_empty(),
            "extracted file must not be empty: {first}"
        );
        eprintln!("{name}: extracted `{first}` → {} bytes", data.len());
    }
    Ok(())
}
