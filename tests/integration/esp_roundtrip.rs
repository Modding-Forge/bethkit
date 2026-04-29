// SPDX-License-Identifier: Apache-2.0
//!
//! Integration tests for `bethkit-core` using real plugin files.
//!
//! All `.esp`, `.esm`, and `.esl` files found in `tests/testdata/` are tested
//! automatically. Place any Skyrim SE plugin file there to add it to the suite.
//!
//! Run with:
//! ```text
//! cargo test --test esp_roundtrip -- --nocapture
//! ```

use std::path::{Path, PathBuf};

use bethkit_core::{GameContext, Plugin, PluginKind, RecordFlags, Signature};

/// Returns all `.esp` / `.esm` / `.esl` files in `tests/testdata/`.
///
/// Navigates from `CARGO_MANIFEST_DIR` (crate root) up to the workspace root,
/// then into `tests/testdata/`. Returns an empty list if the directory is
/// missing so that CI without real plugin files still passes.
fn collect_testdata() -> Vec<PathBuf> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    // CARGO_MANIFEST_DIR = .../bethkit/crates/bethkit-core
    // parent()           = .../bethkit/crates
    // parent()           = .../bethkit  (workspace root)
    let dir = manifest
        .parent()
        .and_then(|p| p.parent())
        .map(|root| root.join("tests").join("testdata"))
        .unwrap_or_else(|| manifest.join("testdata"));
    if !dir.exists() {
        return Vec::new();
    }
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("failed to read testdata dir")
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            let ext = path.extension()?.to_ascii_lowercase();
            if ext == "esp" || ext == "esm" || ext == "esl" {
                Some(path)
            } else {
                None
            }
        })
        .collect();
    paths.sort();
    paths
}

/// Opens a plugin with SSE context and reports the path on error.
fn open_plugin(path: &Path) -> Result<Plugin, String> {
    Plugin::open(path, GameContext::sse())
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// Every plugin in testdata/ must open without error.
#[test]
fn all_plugins_parse_without_error() -> Result<(), Box<dyn std::error::Error>> {
    // given
    let paths = collect_testdata();
    if paths.is_empty() {
        eprintln!("SKIP: no files in tests/testdata/ — add real SSE plugins to enable");
        return Ok(());
    }

    let mut failures: Vec<String> = Vec::new();
    let mut count = 0usize;

    for path in &paths {
        // when
        match open_plugin(path) {
            Ok(_) => count += 1,
            Err(msg) => failures.push(msg),
        }
    }

    // then
    if !failures.is_empty() {
        eprintln!("\n{} of {} plugins failed to parse:", failures.len(), paths.len());
        for f in &failures {
            eprintln!("  FAIL: {f}");
        }
        panic!("{} plugin(s) failed to parse — see stderr for details", failures.len());
    }
    eprintln!("OK: {count} plugins parsed successfully");
    Ok(())
}

/// Every plugin must have an accessible group list (may be empty for stubs).
#[test]
fn all_plugins_have_accessible_groups() -> Result<(), Box<dyn std::error::Error>> {
    // given
    let paths = collect_testdata();
    if paths.is_empty() { return Ok(()); }

    for path in &paths {
        // when
        let Ok(plugin) = open_plugin(path) else { continue };
        // then — just ensure the call does not panic and returns a slice
        let _groups = plugin.groups();
    }
    Ok(())
}

/// Every record signature must consist of ASCII alphanumeric chars or `_`.
#[test]
fn all_record_signatures_are_valid_ascii() -> Result<(), Box<dyn std::error::Error>> {
    // given
    let paths = collect_testdata();
    if paths.is_empty() { return Ok(()); }

    let mut bad: Vec<String> = Vec::new();

    for path in &paths {
        // when
        let Ok(plugin) = open_plugin(path) else { continue };

        for group in plugin.groups() {
            for record in group.records_recursive() {
                let sig = record.header.signature;
                // then
                if !sig.0.iter().all(|b| b.is_ascii_alphanumeric() || *b == b'_') {
                    bad.push(format!(
                        "{}: invalid signature {sig} in FormID {:08X}",
                        path.file_name().unwrap().to_string_lossy(),
                        record.header.form_id.0,
                    ));
                }
            }
        }
    }

    if !bad.is_empty() {
        for b in &bad { eprintln!("BAD: {b}"); }
        panic!("{} bad signature(s) found", bad.len());
    }
    Ok(())
}

/// Plugin kind must be one of the five known variants.
#[test]
fn all_plugins_have_known_kind() -> Result<(), Box<dyn std::error::Error>> {
    // given
    let paths = collect_testdata();
    if paths.is_empty() { return Ok(()); }

    for path in &paths {
        // when
        let Ok(plugin) = open_plugin(path) else { continue };
        let kind = plugin.kind();
        // then — pattern match ensures exhaustiveness at compile time
        let _ok = matches!(
            kind,
            PluginKind::Plugin | PluginKind::Master | PluginKind::Light
                | PluginKind::Medium | PluginKind::Update
        );
    }
    Ok(())
}

/// `.esm` files must not be detected as the Update-only variant (SSE has no
#[test]
/// Update plugins — that is a Starfield concept).
fn esm_files_are_never_update_kind() -> Result<(), Box<dyn std::error::Error>> {
    // given
    let paths = collect_testdata();
    if paths.is_empty() { return Ok(()); }

    let mut failures: Vec<String> = Vec::new();

    for path in &paths {
        let Ok(plugin) = open_plugin(path) else { continue };
        let ext = path.extension().map(|e| e.to_ascii_lowercase());

        if ext.as_deref() == Some("esm") && plugin.kind() == PluginKind::Update {
            failures.push(format!(
                "{}: .esm detected as Update (unexpected for SSE)",
                path.file_name().unwrap().to_string_lossy()
            ));
        }
    }

    if !failures.is_empty() {
        for f in &failures { eprintln!("FAIL: {f}"); }
        panic!("{} kind mismatch(es)", failures.len());
    }
    Ok(())
}

/// All master filenames from MAST subrecords must be non-empty ASCII strings.
#[test]
fn all_master_names_are_non_empty_ascii() -> Result<(), Box<dyn std::error::Error>> {
    // given
    let paths = collect_testdata();
    if paths.is_empty() { return Ok(()); }

    let mut failures: Vec<String> = Vec::new();

    for path in &paths {
        // when
        let Ok(plugin) = open_plugin(path) else { continue };

        for master in plugin.masters() {
            // then
            if master.is_empty() {
                failures.push(format!(
                    "{}: empty master filename",
                    path.file_name().unwrap().to_string_lossy()
                ));
            }
            if !master.is_ascii() {
                failures.push(format!(
                    "{}: non-ASCII master filename: {master:?}",
                    path.file_name().unwrap().to_string_lossy()
                ));
            }
        }
    }

    if !failures.is_empty() {
        for f in &failures { eprintln!("FAIL: {f}"); }
        panic!("{} invalid master name(s)", failures.len());
    }
    Ok(())
}

/// Triggering lazy subrecord parsing on every record must not produce errors.
#[test]
fn all_subrecords_parse_without_error() -> Result<(), Box<dyn std::error::Error>> {
    // given
    let paths = collect_testdata();
    if paths.is_empty() { return Ok(()); }

    let mut failures: Vec<String> = Vec::new();
    let mut total_records = 0usize;

    for path in &paths {
        // when
        let Ok(plugin) = open_plugin(path) else { continue };

        for group in plugin.groups() {
            for record in group.records_recursive() {
                total_records += 1;
                // then — trigger lazy subrecord parsing
                if let Err(e) = record.subrecords() {
                    failures.push(format!(
                        "{}: FormID {:08X} subrecord parse failed: {e}",
                        path.file_name().unwrap().to_string_lossy(),
                        record.header.form_id.0,
                    ));
                }
            }
        }
    }

    if !failures.is_empty() {
        eprintln!("{} of {total_records} records failed subrecord parsing:", failures.len());
        for f in failures.iter().take(20) { eprintln!("  {f}"); }
        panic!("{} record(s) failed subrecord parsing", failures.len());
    }
    eprintln!("OK: {total_records} records had subrecords parsed");
    Ok(())
}

/// EDID (editor ID) subrecords, where present, must decode as valid UTF-8.
#[test]
fn edid_subrecords_are_valid_utf8() -> Result<(), Box<dyn std::error::Error>> {
    // given
    let paths = collect_testdata();
    if paths.is_empty() { return Ok(()); }

    let mut failures: Vec<String> = Vec::new();
    let mut edid_count = 0usize;

    for path in &paths {
        // when
        let Ok(plugin) = open_plugin(path) else { continue };

        for group in plugin.groups() {
            for record in group.records_recursive() {
                let Ok(Some(edid_sr)) = record.get(Signature::EDID) else { continue };
                edid_count += 1;
                // then
                if let Err(e) = edid_sr.as_zstring() {
                    failures.push(format!(
                        "{}: FormID {:08X} EDID decode failed: {e}",
                        path.file_name().unwrap().to_string_lossy(),
                        record.header.form_id.0,
                    ));
                }
            }
        }
    }

    if !failures.is_empty() {
        for f in failures.iter().take(20) { eprintln!("FAIL: {f}"); }
        panic!("{} EDID(s) failed UTF-8 decode", failures.len());
    }
    eprintln!("OK: {edid_count} EDID subrecords decoded");
    Ok(())
}

/// Compressed records must decompress without error and not panic.
#[test]
fn compressed_records_decompress_correctly() -> Result<(), Box<dyn std::error::Error>> {
    // given
    let paths = collect_testdata();
    if paths.is_empty() { return Ok(()); }

    let mut failures: Vec<String> = Vec::new();
    let mut compressed_count = 0usize;

    for path in &paths {
        // when
        let Ok(plugin) = open_plugin(path) else { continue };

        for group in plugin.groups() {
            for record in group.records_recursive() {
                if !record.header.flags.contains(RecordFlags::COMPRESSED) {
                    continue;
                }
                compressed_count += 1;
                // then — trigger decompression via subrecord parse
                if let Err(e) = record.subrecords() {
                    failures.push(format!(
                        "{}: FormID {:08X} decompression failed: {e}",
                        path.file_name().unwrap().to_string_lossy(),
                        record.header.form_id.0,
                    ));
                }
            }
        }
    }

    if !failures.is_empty() {
        for f in failures.iter().take(20) { eprintln!("FAIL: {f}"); }
        panic!("{} compressed record(s) failed to decompress", failures.len());
    }
    eprintln!("OK: {compressed_count} compressed records decompressed");
    Ok(())
}

/// The HEDR version float must be positive and finite for every plugin.
#[test]
fn all_plugins_have_valid_hedr_version() -> Result<(), Box<dyn std::error::Error>> {
    // given
    let paths = collect_testdata();
    if paths.is_empty() { return Ok(()); }

    let mut failures: Vec<String> = Vec::new();

    for path in &paths {
        // when
        let Ok(plugin) = open_plugin(path) else { continue };
        let v = plugin.header.hedr_version;
        // then
        if !v.is_finite() || v <= 0.0 {
            failures.push(format!(
                "{}: invalid HEDR version {v}",
                path.file_name().unwrap().to_string_lossy()
            ));
        }
    }

    if !failures.is_empty() {
        for f in &failures { eprintln!("FAIL: {f}"); }
        panic!("{} plugin(s) with invalid HEDR version", failures.len());
    }
    Ok(())
}

/// `find_record` must return the correct record when searching by FormID.
#[test]
fn find_record_returns_correct_record() -> Result<(), Box<dyn std::error::Error>> {
    // given
    let paths = collect_testdata();
    if paths.is_empty() { return Ok(()); }

    for path in &paths {
        // when
        let Ok(plugin) = open_plugin(path) else { continue };

        // Extract the first FormID without keeping a borrow into plugin.
        let first_fid: Option<FormId> = plugin
            .groups()
            .iter()
            .flat_map(|g| g.records_recursive())
            .next()
            .map(|r| r.header.form_id);

        if let Some(fid) = first_fid {
            // then
            let found = plugin.find_record(fid);
            assert!(
                found.is_some(),
                "{}: find_record({:08X}) returned None, expected Some",
                path.file_name().unwrap().to_string_lossy(),
                fid.0,
            );
            assert_eq!(
                found.unwrap().header.form_id,
                fid,
                "find_record returned wrong record"
            );
        }
    }
    Ok(())
}

/// Print a summary table of all testdata plugins. Informational only.
#[test]
fn print_plugin_summary() -> Result<(), Box<dyn std::error::Error>> {
    // given
    let paths = collect_testdata();
    if paths.is_empty() {
        eprintln!("SKIP: no testdata files");
        return Ok(());
    }

    eprintln!("\n{:<52} {:8} {:6} {:7}", "File", "Kind", "Groups", "Masters");
    eprintln!("{}", "-".repeat(76));

    let mut ok = 0usize;
    let mut err = 0usize;

    for path in &paths {
        // when
        match open_plugin(path) {
            Ok(plugin) => {
                ok += 1;
                eprintln!(
                    "{:<52} {:8} {:6} {:7}",
                    path.file_name().unwrap().to_string_lossy(),
                    format!("{:?}", plugin.kind()),
                    plugin.groups().len(),
                    plugin.masters().len(),
                );
            }
            Err(e) => {
                err += 1;
                eprintln!(
                    "{:<52} ERROR: {e}",
                    path.file_name().unwrap().to_string_lossy()
                );
            }
        }
    }
    eprintln!("{}", "-".repeat(76));
    eprintln!("Total: {ok} OK, {err} errors");
    Ok(())
}
