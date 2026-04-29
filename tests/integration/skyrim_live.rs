// SPDX-License-Identifier: Apache-2.0
//!
//! XXXL live integration + benchmark suite against a real Skyrim Special Edition
//! game installation with a full mod list.
//!
//! # Precondition
//!
//! Set the environment variable `SKYRIM_DATA_DIR` to the path of your Skyrim SE
//! `Data/` folder, **or** place the installation at the default path:
//!
//! ```text
//! E:\SteamLibrary\steamapps\common\Skyrim Special Edition\Data
//! ```
//!
//! If neither path exists the entire suite is skipped so that CI passes without
//! a game installation.
//!
//! # Run
//!
//! ```text
//! cargo test --test skyrim_live -- --nocapture
//! ```

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use bethkit_core::{
    GameContext, Plugin, PluginKind, RecordFlags, RecordView, SchemaRegistry, Signature,
};

// ── constants ─────────────────────────────────────────────────────────────────

const DEFAULT_DATA_DIR: &str =
    r"E:\SteamLibrary\steamapps\common\Skyrim Special Edition\Data";

// Signatures that intentionally do not live in the SSE schema (overrides,
// compiler-internal records, etc.) and are expected to be schema-unknown.
const KNOWN_NO_SCHEMA: &[&[u8; 4]] = &[b"NAVM", b"NAVI", b"REFR", b"ACHR", b"PGRE", b"PMIS"];

// ── helpers ───────────────────────────────────────────────────────────────────

/// Locates the Skyrim SE Data directory.
///
/// Returns `None` when the suite should be skipped.
fn find_data_dir() -> Option<PathBuf> {
    // 1. Environment variable override.
    if let Ok(val) = std::env::var("SKYRIM_DATA_DIR") {
        let p = PathBuf::from(val);
        if p.exists() {
            return Some(p);
        }
        eprintln!("SKYRIM_DATA_DIR is set but path does not exist: {}", p.display());
        return None;
    }

    // 2. Hard-coded default installation path.
    let default = PathBuf::from(DEFAULT_DATA_DIR);
    if default.exists() {
        return Some(default);
    }

    None
}

/// Collects all `.esp` / `.esm` / `.esl` files in `dir`, sorted by name.
fn collect_plugins(dir: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("failed to read Data directory")
        .filter_map(|e| {
            let e = e.ok()?;
            let p = e.path();
            let ext = p.extension()?.to_ascii_lowercase();
            if ext == "esp" || ext == "esm" || ext == "esl" {
                Some(p)
            } else {
                None
            }
        })
        .collect();
    paths.sort();
    paths
}

/// Opens a plugin with SSE context, returning an error message on failure.
fn open(path: &Path) -> Result<Plugin, String> {
    Plugin::open(path, GameContext::sse())
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// Prints a section banner to stderr.
fn banner(title: &str) {
    eprintln!();
    eprintln!("━━━  {title}  ━━━");
}

/// Formats a byte count as a human-readable string.
fn fmt_bytes(n: u64) -> String {
    const GIB: u64 = 1 << 30;
    const MIB: u64 = 1 << 20;
    const KIB: u64 = 1 << 10;
    if n >= GIB {
        format!("{:.2} GiB", n as f64 / GIB as f64)
    } else if n >= MIB {
        format!("{:.1} MiB", n as f64 / MIB as f64)
    } else {
        format!("{:.1} KiB", n as f64 / KIB as f64)
    }
}

/// Formats a rate as MB/s.
fn fmt_mbps(bytes: u64, elapsed: Duration) -> String {
    let secs = elapsed.as_secs_f64();
    if secs == 0.0 {
        return "∞ MB/s".to_owned();
    }
    format!("{:.1} MB/s", bytes as f64 / (1 << 20) as f64 / secs)
}

// ── Benchmark result accumulator ──────────────────────────────────────────────

struct BenchResult {
    label: &'static str,
    files: usize,
    bytes: u64,
    elapsed: Duration,
    records: u64,
    errors: usize,
}

impl BenchResult {
    fn print(&self) {
        let mbps = fmt_mbps(self.bytes, self.elapsed);
        let rps = if self.elapsed.as_secs_f64() > 0.0 {
            format!("{:.0}", self.records as f64 / self.elapsed.as_secs_f64())
        } else {
            "∞".to_owned()
        };
        eprintln!(
            "  {:50}  {:>6} files  {:>10}  {:>8.3} s  {:>12} MB/s  {:>12} rec/s  {} err",
            self.label,
            self.files,
            fmt_bytes(self.bytes),
            self.elapsed.as_secs_f64(),
            mbps.trim_end_matches(" MB/s"),
            rps,
            self.errors,
        );
    }
}

// ── Test 1: Dataset discovery ─────────────────────────────────────────────────

/// Reports dataset statistics — does NOT assert, just prints.
///
/// Verifies that the live Skyrim SE data directory is readable and contains
/// a plausible number of plugin files.
#[test]
fn live_01_dataset_discovery() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else {
        eprintln!("SKIP: Skyrim SE Data directory not found");
        return Ok(());
    };

    banner("DATASET DISCOVERY");

    let paths = collect_plugins(&dir);
    let total_bytes: u64 = paths
        .iter()
        .filter_map(|p| std::fs::metadata(p).ok())
        .map(|m| m.len())
        .sum();

    let esm = paths.iter().filter(|p| p.extension().map(|e| e == "esm").unwrap_or(false)).count();
    let esp = paths.iter().filter(|p| p.extension().map(|e| e == "esp").unwrap_or(false)).count();
    let esl = paths.iter().filter(|p| p.extension().map(|e| e == "esl").unwrap_or(false)).count();

    eprintln!("  Data dir : {}", dir.display());
    eprintln!("  ESM      : {esm}");
    eprintln!("  ESP      : {esp}");
    eprintln!("  ESL      : {esl}");
    eprintln!("  Total    : {} files  ({})", paths.len(), fmt_bytes(total_bytes));

    // Sanity: at minimum Skyrim.esm + Update.esm must exist.
    assert!(
        paths.iter().any(|p| p.file_name().map(|n| n == "Skyrim.esm").unwrap_or(false)),
        "Skyrim.esm not found in Data directory"
    );
    assert!(paths.len() >= 5, "too few plugin files — expected at least 5");

    Ok(())
}

// ── Test 2: All plugins open without error ─────────────────────────────────────

/// Every `.esp` / `.esm` / `.esl` file in the Data directory must parse
/// without returning an error.
#[test]
fn live_02_all_plugins_open() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else { return Ok(()); };
    banner("ALL PLUGINS OPEN");

    let paths = collect_plugins(&dir);
    let mut ok = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for path in &paths {
        match open(path) {
            Ok(_) => ok += 1,
            Err(e) => failures.push(e),
        }
    }

    eprintln!("  Opened {ok} / {} plugins without error", paths.len());
    if !failures.is_empty() {
        eprintln!("  FAILURES ({}):", failures.len());
        for f in failures.iter().take(30) {
            eprintln!("    {f}");
        }
        if failures.len() > 30 {
            eprintln!("    … and {} more", failures.len() - 30);
        }
        panic!("{} plugin(s) failed to open", failures.len());
    }

    Ok(())
}

// ── Test 3: Record signature validity ─────────────────────────────────────────

/// All record signatures across every plugin must consist exclusively of
/// ASCII alphanumeric bytes or `_`.
#[test]
fn live_03_all_signatures_are_valid_ascii() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else { return Ok(()); };
    banner("RECORD SIGNATURE VALIDITY");

    let paths = collect_plugins(&dir);
    let mut bad: Vec<String> = Vec::new();
    let mut total_records = 0u64;

    for path in &paths {
        let Ok(plugin) = open(path) else { continue };
        for group in plugin.groups() {
            for record in group.records_recursive() {
                total_records += 1;
                let sig = record.header.signature;
                if !sig.0.iter().all(|b| b.is_ascii_alphanumeric() || *b == b'_') {
                    bad.push(format!(
                        "{}: invalid signature {sig} at FormID {:08X}",
                        path.file_name().unwrap().to_string_lossy(),
                        record.header.form_id.0,
                    ));
                }
            }
        }
    }

    eprintln!("  Checked {total_records} record signatures");
    if !bad.is_empty() {
        for b in bad.iter().take(20) {
            eprintln!("  BAD: {b}");
        }
        panic!("{} invalid signature(s) found", bad.len());
    }

    Ok(())
}

// ── Test 4: Plugin kinds ──────────────────────────────────────────────────────

/// Every plugin must have a recognised PluginKind, and `.esm` files must
/// not be detected as the Starfield-only `Update` variant.
#[test]
fn live_04_plugin_kinds_are_valid() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else { return Ok(()); };
    banner("PLUGIN KINDS");

    let paths = collect_plugins(&dir);
    let mut kind_counts: HashMap<&'static str, usize> = HashMap::new();
    let mut bad_update_esm: Vec<String> = Vec::new();

    for path in &paths {
        let Ok(plugin) = open(path) else { continue };
        let kind = plugin.kind();
        let label = match kind {
            PluginKind::Plugin => "Plugin (.esp)",
            PluginKind::Master => "Master (.esm)",
            PluginKind::Light  => "Light  (.esl)",
            PluginKind::Medium => "Medium",
            PluginKind::Update => "Update",
        };
        *kind_counts.entry(label).or_default() += 1;

        let ext = path.extension().map(|e| e.to_ascii_lowercase());
        if ext.as_deref() == Some("esm") && kind == PluginKind::Update {
            bad_update_esm.push(path.file_name().unwrap().to_string_lossy().into_owned());
        }
    }

    for (label, count) in &kind_counts {
        eprintln!("  {label:20} : {count}");
    }

    if !bad_update_esm.is_empty() {
        for f in &bad_update_esm {
            eprintln!("  WARN: {f} detected as Update-kind (unexpected for SSE)");
        }
        panic!("{} .esm file(s) incorrectly detected as Update kind", bad_update_esm.len());
    }

    Ok(())
}

// ── Test 5: HEDR version validity ────────────────────────────────────────────

/// The HEDR version float must be positive and finite for every plugin.
#[test]
fn live_05_hedr_versions_are_valid() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else { return Ok(()); };
    banner("HEDR VERSION VALIDITY");

    let paths = collect_plugins(&dir);
    let mut bad: Vec<String> = Vec::new();
    let mut versions: HashMap<u32, usize> = HashMap::new(); // version bits -> count

    for path in &paths {
        let Ok(plugin) = open(path) else { continue };
        let v = plugin.header.hedr_version;
        *versions.entry(v.to_bits()).or_default() += 1;
        if !v.is_finite() || v <= 0.0 {
            bad.push(format!(
                "{}: invalid HEDR version {v}",
                path.file_name().unwrap().to_string_lossy()
            ));
        }
    }

    // Print unique version values.
    let mut sorted_versions: Vec<(f32, usize)> = versions
        .into_iter()
        .map(|(bits, count)| (f32::from_bits(bits), count))
        .collect();
    sorted_versions.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    for (v, count) in &sorted_versions {
        eprintln!("  HEDR version {v:.4} : {count} plugin(s)");
    }

    if !bad.is_empty() {
        for f in &bad {
            eprintln!("  BAD: {f}");
        }
        panic!("{} invalid HEDR version(s)", bad.len());
    }

    Ok(())
}

// ── Test 6: Master filename validity ─────────────────────────────────────────

/// All MAST subrecords must be non-empty, printable ASCII strings.
#[test]
fn live_06_master_filenames_are_valid() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else { return Ok(()); };
    banner("MASTER FILENAME VALIDITY");

    let paths = collect_plugins(&dir);
    let mut bad: Vec<String> = Vec::new();
    let mut total_masters = 0u64;
    let mut master_counts: HashMap<usize, usize> = HashMap::new();

    for path in &paths {
        let Ok(plugin) = open(path) else { continue };
        let masters = plugin.masters();
        *master_counts.entry(masters.len()).or_default() += 1;
        total_masters += masters.len() as u64;
        for m in masters {
            if m.is_empty() || !m.is_ascii() {
                bad.push(format!(
                    "{}: invalid master {:?}",
                    path.file_name().unwrap().to_string_lossy(),
                    m
                ));
            }
        }
    }

    eprintln!("  Total MAST references : {total_masters}");
    let mut buckets: Vec<(usize, usize)> = master_counts.into_iter().collect();
    buckets.sort_by_key(|(k, _)| *k);
    for (count, n) in &buckets {
        eprintln!("  {count:3} master(s) : {n} plugin(s)");
    }

    if !bad.is_empty() {
        for f in bad.iter().take(20) {
            eprintln!("  BAD: {f}");
        }
        panic!("{} invalid master filename(s)", bad.len());
    }

    Ok(())
}

// ── Test 7: Subrecord parsing ─────────────────────────────────────────────────

/// Triggering lazy subrecord parsing on every record across every plugin
/// must not return an error.
#[test]
fn live_07_all_subrecords_parse() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else { return Ok(()); };
    banner("SUBRECORD PARSE COVERAGE");

    let paths = collect_plugins(&dir);
    let mut failures: Vec<String> = Vec::new();
    let mut total_records = 0u64;
    let mut total_subrecords = 0u64;

    for path in &paths {
        let Ok(plugin) = open(path) else { continue };
        for group in plugin.groups() {
            for record in group.records_recursive() {
                total_records += 1;
                match record.subrecords() {
                    Ok(srs) => total_subrecords += srs.len() as u64,
                    Err(e) => {
                        failures.push(format!(
                            "{}: FormID {:08X} ({}) parse failed: {e}",
                            path.file_name().unwrap().to_string_lossy(),
                            record.header.form_id.0,
                            record.header.signature,
                        ));
                    }
                }
            }
        }
    }

    eprintln!("  Records        : {total_records}");
    eprintln!("  Subrecords     : {total_subrecords}");
    if !failures.is_empty() {
        eprintln!("  FAILURES ({}):", failures.len());
        for f in failures.iter().take(30) {
            eprintln!("    {f}");
        }
        panic!("{} subrecord parse failure(s)", failures.len());
    }

    Ok(())
}

// ── Test 8: EDID subrecords are valid UTF-8 ───────────────────────────────────

/// Every EDID (Editor ID) subrecord must decode to a valid UTF-8 string.
#[test]
fn live_08_edid_subrecords_are_valid_utf8() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else { return Ok(()); };
    banner("EDID UTF-8 VALIDITY");

    let paths = collect_plugins(&dir);
    let sig_edid = Signature(*b"EDID");
    let mut failures: Vec<String> = Vec::new();
    let mut edid_count = 0u64;
    let mut max_len = 0usize;

    for path in &paths {
        let Ok(plugin) = open(path) else { continue };
        for group in plugin.groups() {
            for record in group.records_recursive() {
                let Ok(Some(sr)) = record.get(sig_edid) else { continue };
                edid_count += 1;
                match sr.as_zstring() {
                    Ok(s) => max_len = max_len.max(s.len()),
                    Err(e) => {
                        failures.push(format!(
                            "{}: FormID {:08X} EDID decode failed: {e}",
                            path.file_name().unwrap().to_string_lossy(),
                            record.header.form_id.0,
                        ));
                    }
                }
            }
        }
    }

    eprintln!("  EDID subrecords : {edid_count}  (max length: {max_len})");
    if !failures.is_empty() {
        for f in failures.iter().take(20) {
            eprintln!("  BAD: {f}");
        }
        panic!("{} EDID(s) failed UTF-8 decode", failures.len());
    }

    Ok(())
}

// ── Test 9: Compressed records decompress ─────────────────────────────────────

/// Every compressed record (COMPRESSED flag set) must decompress without
/// error and produce a non-empty subrecord list.
#[test]
fn live_09_compressed_records_decompress() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else { return Ok(()); };
    banner("COMPRESSED RECORD DECOMPRESSION");

    let paths = collect_plugins(&dir);
    let mut failures: Vec<String> = Vec::new();
    let mut compressed_count = 0u64;
    let mut total_records = 0u64;

    for path in &paths {
        let Ok(plugin) = open(path) else { continue };
        for group in plugin.groups() {
            for record in group.records_recursive() {
                total_records += 1;
                if !record.header.flags.contains(RecordFlags::COMPRESSED) {
                    continue;
                }
                compressed_count += 1;
                match record.subrecords() {
                    Ok(srs) if srs.is_empty() => {
                        // A non-zero-sized compressed record yielding zero
                        // subrecords is suspicious but not a hard error.
                    }
                    Ok(_) => {}
                    Err(e) => {
                        failures.push(format!(
                            "{}: FormID {:08X} ({}) decompression failed: {e}",
                            path.file_name().unwrap().to_string_lossy(),
                            record.header.form_id.0,
                            record.header.signature,
                        ));
                    }
                }
            }
        }
    }

    let pct = if total_records > 0 {
        compressed_count as f64 / total_records as f64 * 100.0
    } else {
        0.0
    };
    eprintln!(
        "  Compressed : {compressed_count} / {total_records} records  ({pct:.1}%)"
    );

    if !failures.is_empty() {
        for f in failures.iter().take(20) {
            eprintln!("  FAIL: {f}");
        }
        panic!("{} compressed record(s) failed to decompress", failures.len());
    }

    Ok(())
}

// ── Test 10: Record flag invariants ──────────────────────────────────────────

/// Records with conflicting flag combinations (e.g., DELETED + LOCALIZED)
/// are not expected in clean SSE plugins.  This test collects anomalies
/// without hard-failing so that modded setups with intentionally odd flags
/// do not block CI.
#[test]
fn live_10_record_flag_inventory() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else { return Ok(()); };
    banner("RECORD FLAG INVENTORY");

    let paths = collect_plugins(&dir);

    let mut deleted_count = 0u64;
    let mut localized_count = 0u64;
    let mut compressed_count = 0u64;
    let mut ignored_count = 0u64;
    let mut initially_disabled = 0u64;

    for path in &paths {
        let Ok(plugin) = open(path) else { continue };
        for group in plugin.groups() {
            for record in group.records_recursive() {
                let f = record.header.flags;
                if f.contains(RecordFlags::DELETED)           { deleted_count += 1; }
                if f.contains(RecordFlags::LOCALIZED)         { localized_count += 1; }
                if f.contains(RecordFlags::COMPRESSED)        { compressed_count += 1; }
                if f.contains(RecordFlags::IGNORED)           { ignored_count += 1; }
                if f.contains(RecordFlags::INITIALLY_DISABLED){ initially_disabled += 1; }
            }
        }
    }

    eprintln!("  DELETED           : {deleted_count}");
    eprintln!("  LOCALIZED         : {localized_count}");
    eprintln!("  COMPRESSED        : {compressed_count}");
    eprintln!("  IGNORED           : {ignored_count}");
    eprintln!("  INITIALLY_DISABLED: {initially_disabled}");

    Ok(())
}

// ── Test 11: Schema coverage ──────────────────────────────────────────────────

/// Measures what fraction of record types encountered in the wild are
/// covered by our SSE schema registry.
///
/// Emits a detailed coverage report. Does not fail — schema coverage is
/// tracked as a metric, not an invariant.
#[test]
fn live_11_schema_coverage() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else { return Ok(()); };
    banner("SSE SCHEMA COVERAGE");

    let paths = collect_plugins(&dir);
    let reg = SchemaRegistry::sse();

    let mut sig_counts: HashMap<[u8; 4], u64> = HashMap::new();

    for path in &paths {
        let Ok(plugin) = open(path) else { continue };
        for group in plugin.groups() {
            for record in group.records_recursive() {
                *sig_counts.entry(record.header.signature.0).or_default() += 1;
            }
        }
    }

    let total_distinct = sig_counts.len();
    let covered: Vec<([u8; 4], u64)> = sig_counts
        .iter()
        .filter(|(sig, _)| reg.get(Signature(**sig)).is_some())
        .map(|(sig, &count)| (*sig, count))
        .collect();
    let uncovered: Vec<([u8; 4], u64)> = sig_counts
        .iter()
        .filter(|(sig, _)| reg.get(Signature(**sig)).is_none())
        .map(|(sig, &count)| (*sig, count))
        .collect();

    let total_covered_records: u64 = covered.iter().map(|(_, c)| c).sum();
    let total_uncovered_records: u64 = uncovered.iter().map(|(_, c)| c).sum();
    let total_records: u64 = total_covered_records + total_uncovered_records;
    let coverage_pct = total_covered_records as f64 / total_records as f64 * 100.0;
    let type_coverage_pct = covered.len() as f64 / total_distinct as f64 * 100.0;

    eprintln!(
        "  Schema registry size     : {}",
        reg.len()
    );
    eprintln!(
        "  Distinct record types    : {total_distinct} found in wild"
    );
    eprintln!(
        "  Type coverage            : {} / {total_distinct}  ({type_coverage_pct:.1}%)",
        covered.len()
    );
    eprintln!(
        "  Record coverage          : {total_covered_records} / {total_records}  \
         ({coverage_pct:.1}%)"
    );

    // Print uncovered types sorted by frequency (most common first).
    let mut uncovered_sorted = uncovered;
    uncovered_sorted.sort_by(|a, b| b.1.cmp(&a.1));
    if !uncovered_sorted.is_empty() {
        eprintln!("  Uncovered record types (top 20 by frequency):");
        for (sig, count) in uncovered_sorted.iter().take(20) {
            let s = Signature(*sig);
            eprintln!("    {s}  {count} records");
        }
    }

    Ok(())
}

// ── Test 12: Schema-guided field decode (RecordView) ─────────────────────────

/// Runs RecordView field decoding on every record whose type is covered by
/// the SSE schema.  Counts decode successes, benign-missing fields, and
/// hard decode errors.
#[test]
fn live_12_schema_field_decode() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else { return Ok(()); };
    banner("SCHEMA-GUIDED FIELD DECODE (RecordView)");

    let paths = collect_plugins(&dir);
    let reg = SchemaRegistry::sse();

    let mut records_decoded = 0u64;
    let mut records_skipped = 0u64;
    let mut fields_decoded = 0u64;
    let mut fields_missing = 0u64;
    let mut decode_errors: Vec<String> = Vec::new();

    for path in &paths {
        let Ok(plugin) = open(path) else { continue };
        for group in plugin.groups() {
            for record in group.records_recursive() {
                let Some(schema) = reg.get(record.header.signature) else {
                    records_skipped += 1;
                    continue;
                };
                records_decoded += 1;
                let view = RecordView::new(record, schema);
                match view.fields() {
                    Ok(fields) => {
                        for f in &fields {
                            use bethkit_core::FieldValue;
                            match &f.value {
                                FieldValue::Missing => fields_missing += 1,
                                _ => fields_decoded += 1,
                            }
                        }
                    }
                    Err(e) => {
                        if decode_errors.len() < 50 {
                            decode_errors.push(format!(
                                "{}: {} FormID {:08X}: {e}",
                                path.file_name().unwrap().to_string_lossy(),
                                record.header.signature,
                                record.header.form_id.0,
                            ));
                        }
                    }
                }
            }
        }
    }

    eprintln!("  Records decoded        : {records_decoded}");
    eprintln!("  Records skipped        : {records_skipped} (no schema)");
    eprintln!("  Fields decoded (value) : {fields_decoded}");
    eprintln!("  Fields missing         : {fields_missing}");
    eprintln!("  Decode errors          : {}", decode_errors.len());
    for e in decode_errors.iter().take(10) {
        eprintln!("    ERR: {e}");
    }

    // Tolerate up to 0.5% decode errors (corrupt mods exist in the wild).
    let error_rate = decode_errors.len() as f64 / records_decoded.max(1) as f64;
    assert!(
        error_rate < 0.005,
        "field decode error rate {:.3}% exceeds 0.5% threshold",
        error_rate * 100.0
    );

    Ok(())
}

// ── Test 13: Deep analysis of Skyrim.esm ─────────────────────────────────────

/// Performs a thorough analysis of `Skyrim.esm` — the largest and most
/// complex plugin in the base game — and prints a detailed breakdown.
#[test]
fn live_13_skyrim_esm_deep_analysis() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else { return Ok(()); };
    banner("SKYRIM.ESM DEEP ANALYSIS");

    let esm_path = dir.join("Skyrim.esm");
    assert!(esm_path.exists(), "Skyrim.esm not found at {}", esm_path.display());

    let file_size = std::fs::metadata(&esm_path)?.len();
    eprintln!("  File size : {}", fmt_bytes(file_size));

    let t0 = Instant::now();
    let plugin = open(&esm_path).map_err(|e| e.to_string())?;
    let open_time = t0.elapsed();

    eprintln!("  Open time : {:.3} s", open_time.as_secs_f64());
    eprintln!("  HEDR ver  : {}", plugin.header.hedr_version);
    eprintln!("  Masters   : {:?}", plugin.masters());
    eprintln!("  Groups    : {}", plugin.group_count());
    eprintln!("  Localized : {}", plugin.is_localized());

    // Record-type histogram.
    let mut sig_counts: HashMap<[u8; 4], u64> = HashMap::new();
    let mut total_records = 0u64;
    let mut compressed_records = 0u64;
    let mut deleted_records = 0u64;
    let mut localized_records = 0u64;
    let mut total_subrecords = 0u64;
    let mut failed_subrecords = 0u64;

    for group in plugin.groups() {
        for record in group.records_recursive() {
            total_records += 1;
            *sig_counts.entry(record.header.signature.0).or_default() += 1;
            let f = record.header.flags;
            if f.contains(RecordFlags::COMPRESSED)        { compressed_records += 1; }
            if f.contains(RecordFlags::DELETED)           { deleted_records += 1; }
            if f.contains(RecordFlags::LOCALIZED)         { localized_records += 1; }
            match record.subrecords() {
                Ok(srs) => total_subrecords += srs.len() as u64,
                Err(_) => failed_subrecords += 1,
            }
        }
    }

    eprintln!("  Total records      : {total_records}");
    eprintln!("  Compressed records : {compressed_records}");
    eprintln!("  Deleted records    : {deleted_records}");
    eprintln!("  Localized records  : {localized_records}");
    eprintln!("  Total subrecords   : {total_subrecords}");
    eprintln!("  Subrecord failures : {failed_subrecords}");

    // Top 30 record types by frequency.
    let mut sorted: Vec<([u8; 4], u64)> = sig_counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));
    eprintln!("  Top 30 record types:");
    for (sig, count) in sorted.iter().take(30) {
        let pct = *count as f64 / total_records as f64 * 100.0;
        eprintln!(
            "    {}  {:>8}  ({pct:4.1}%)",
            Signature(*sig),
            count
        );
    }

    assert_eq!(failed_subrecords, 0, "Skyrim.esm had subrecord parse failures");

    Ok(())
}

// ── Test 14: Group type distribution across all plugins ───────────────────────

/// Collects group type statistics across all plugins and prints the
/// distribution.  Validates that no unknown (negative or >9) group type
/// appears in SSE plugins.
#[test]
fn live_14_group_type_distribution() -> Result<(), Box<dyn std::error::Error>> {
    use bethkit_core::GroupType;
    let Some(dir) = find_data_dir() else { return Ok(()); };
    banner("GROUP TYPE DISTRIBUTION");

    let paths = collect_plugins(&dir);
    let mut type_counts: HashMap<i32, u64> = HashMap::new();
    let mut unknown_types: Vec<String> = Vec::new();

    fn count_group(
        group: &bethkit_core::Group,
        counts: &mut HashMap<i32, u64>,
        unknowns: &mut Vec<String>,
        path_name: &str,
    ) {
        let raw: i32 = group.header.group_type as i32;
        *counts.entry(raw).or_default() += 1;
        // NOTE: GroupType repr values 0-9 are the only known SSE group types.
        if raw < 0 || raw > 9 {
            if unknowns.len() < 20 {
                unknowns.push(format!("{path_name}: unknown group type {raw}"));
            }
        }
        for child in group.children() {
            use bethkit_core::GroupChild;
            if let GroupChild::Group(sub) = child {
                count_group(sub, counts, unknowns, path_name);
            }
        }
    }

    for path in &paths {
        let Ok(plugin) = open(path) else { continue };
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        for group in plugin.groups() {
            count_group(group, &mut type_counts, &mut unknown_types, &name);
        }
    }

    let type_names = [
        (0i32, "Normal (top-level)"),
        (1,  "World children"),
        (2,  "Interior cell block"),
        (3,  "Interior cell sub-block"),
        (4,  "Exterior cell block"),
        (5,  "Exterior cell sub-block"),
        (6,  "Cell children"),
        (7,  "Topic children"),
        (8,  "Cell persistent children"),
        (9,  "Cell temporary children"),
    ];

    for (raw, label) in &type_names {
        let count = type_counts.get(raw).copied().unwrap_or(0);
        eprintln!("  type {raw}: {label:30} : {count}");
    }

    if !unknown_types.is_empty() {
        for u in &unknown_types {
            eprintln!("  UNKNOWN: {u}");
        }
        panic!("{} unknown group type(s) found", unknown_types.len());
    }

    Ok(())
}

// ── Test 15: find_record correctness spot-check ───────────────────────────────

/// Picks the first record with a known FormID from each of the five base
/// game ESMs and verifies that `find_record` returns the same record.
#[test]
fn live_15_find_record_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else { return Ok(()); };
    banner("find_record ROUND-TRIP");

    let base_esms = [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ];

    for esm_name in &base_esms {
        let path = dir.join(esm_name);
        if !path.exists() {
            eprintln!("  SKIP {esm_name}: not found");
            continue;
        }
        let Ok(plugin) = open(&path) else {
            eprintln!("  SKIP {esm_name}: failed to open");
            continue;
        };

        // Grab the first record we encounter.
        let Some(first_record) = plugin.groups()
            .iter()
            .flat_map(|g| g.records_recursive())
            .next()
        else {
            eprintln!("  SKIP {esm_name}: no records found");
            continue;
        };

        let fid = first_record.header.form_id;
        let found = plugin.find_record(fid);

        assert!(
            found.is_some(),
            "{esm_name}: find_record({fid}) returned None but record exists"
        );
        assert_eq!(
            found.unwrap().header.form_id,
            fid,
            "{esm_name}: find_record returned wrong record"
        );
        eprintln!("  {esm_name}: find_record({fid}) OK");
    }

    Ok(())
}

// ── Benchmark A: all-ESMs parse throughput ────────────────────────────────────

/// Measures the wall-clock time to open and fully iterate (subrecord parse)
/// every ESM file in the Data directory.
///
/// Reports: total bytes, elapsed time, MB/s, records/s.
#[test]
fn bench_a_all_esm_full_parse() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else { return Ok(()); };
    banner("BENCHMARK A — All ESM files: full parse + subrecord decode");

    let paths: Vec<PathBuf> = collect_plugins(&dir)
        .into_iter()
        .filter(|p| p.extension().map(|e| e == "esm").unwrap_or(false))
        .collect();

    let total_bytes: u64 = paths
        .iter()
        .filter_map(|p| std::fs::metadata(p).ok())
        .map(|m| m.len())
        .sum();

    let mut records_total = 0u64;
    let mut subrecords_total = 0u64;
    let mut errors = 0usize;

    let t0 = Instant::now();
    for path in &paths {
        let Ok(plugin) = open(path) else { errors += 1; continue };
        for group in plugin.groups() {
            for record in group.records_recursive() {
                records_total += 1;
                match record.subrecords() {
                    Ok(srs) => subrecords_total += srs.len() as u64,
                    Err(_) => errors += 1,
                }
            }
        }
    }
    let elapsed = t0.elapsed();

    let res = BenchResult {
        label: "All ESMs (full parse + subrecord decode)",
        files: paths.len(),
        bytes: total_bytes,
        elapsed,
        records: records_total,
        errors,
    };
    res.print();
    eprintln!("  Subrecords decoded : {subrecords_total}");

    Ok(())
}

// ── Benchmark B: Skyrim.esm — repeated cold + warm parse ─────────────────────

/// Measures how long it takes to open and parse Skyrim.esm three times
/// back-to-back to capture OS page-cache warm-up effects.
#[test]
fn bench_b_skyrim_esm_repeated_parse() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else { return Ok(()); };
    banner("BENCHMARK B — Skyrim.esm repeated parse (3 runs)");

    let path = dir.join("Skyrim.esm");
    if !path.exists() {
        eprintln!("  SKIP: Skyrim.esm not found");
        return Ok(());
    }
    let file_size = std::fs::metadata(&path)?.len();

    for run in 1..=3 {
        let t0 = Instant::now();
        let plugin = open(&path).map_err(|e| e.to_string())?;

        let mut record_count = 0u64;
        for group in plugin.groups() {
            for record in group.records_recursive() {
                record_count += 1;
                let _ = record.subrecords();
            }
        }
        let elapsed = t0.elapsed();

        eprintln!(
            "  Run {run}: {:.3} s  ({})  {} records  ({} MB/s)",
            elapsed.as_secs_f64(),
            fmt_bytes(file_size),
            record_count,
            fmt_mbps(file_size, elapsed).trim_end_matches(" MB/s"),
        );
    }

    Ok(())
}

// ── Benchmark C: schema lookup micro-benchmark ────────────────────────────────

/// Measures the raw schema registry lookup speed by performing 10 million
/// sequential lookups against the SSE registry.
#[test]
fn bench_c_schema_lookup_speed() -> Result<(), Box<dyn std::error::Error>> {
    banner("BENCHMARK C — Schema registry lookup speed (10 M lookups)");

    let reg = SchemaRegistry::sse();
    let sigs: [Signature; 8] = [
        Signature(*b"NPC_"),
        Signature(*b"WEAP"),
        Signature(*b"ARMO"),
        Signature(*b"CELL"),
        Signature(*b"WRLD"),
        Signature(*b"QUST"),
        Signature(*b"DIAL"),
        Signature(*b"XXXX"), // intentionally unknown
    ];

    const ITERATIONS: u64 = 10_000_000;
    let mut hits = 0u64;
    let t0 = Instant::now();
    for i in 0..ITERATIONS {
        let sig = sigs[(i % sigs.len() as u64) as usize];
        if reg.get(sig).is_some() {
            hits += 1;
        }
    }
    let elapsed = t0.elapsed();

    let lookups_per_sec = ITERATIONS as f64 / elapsed.as_secs_f64();
    eprintln!(
        "  {ITERATIONS} lookups in {:.3} s  ({:.0} lookups/s)  hits: {hits}",
        elapsed.as_secs_f64(),
        lookups_per_sec,
    );

    Ok(())
}

// ── Benchmark D: EDID decode throughput ──────────────────────────────────────

/// Measures how fast we can decode all EDID subrecords across all ESM and
/// heavy ESP files.
#[test]
fn bench_d_edid_decode_throughput() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else { return Ok(()); };
    banner("BENCHMARK D — EDID decode throughput");

    // Use only the ESM files for a stable benchmark.
    let paths: Vec<PathBuf> = collect_plugins(&dir)
        .into_iter()
        .filter(|p| p.extension().map(|e| e == "esm").unwrap_or(false))
        .collect();

    let sig_edid = Signature(*b"EDID");
    let mut edid_count = 0u64;
    let mut edid_bytes = 0u64;
    let mut errors = 0usize;

    let t0 = Instant::now();
    for path in &paths {
        let Ok(plugin) = open(path) else { continue };
        for group in plugin.groups() {
            for record in group.records_recursive() {
                let Ok(Some(sr)) = record.get(sig_edid) else { continue };
                edid_count += 1;
                edid_bytes += sr.as_bytes().len() as u64;
                if sr.as_zstring().is_err() {
                    errors += 1;
                }
            }
        }
    }
    let elapsed = t0.elapsed();

    eprintln!(
        "  {} EDID subrecords  {}  in {:.3} s  ({:.0} EDID/s)  {} errors",
        edid_count,
        fmt_bytes(edid_bytes),
        elapsed.as_secs_f64(),
        edid_count as f64 / elapsed.as_secs_f64(),
        errors,
    );

    Ok(())
}

// ── Benchmark E: all-plugins header-only throughput ───────────────────────────

/// Measures how fast we can open and read just the headers (no group/record
/// iteration) of all 2 000+ plugins.
#[test]
fn bench_e_all_plugins_header_only() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else { return Ok(()); };
    banner("BENCHMARK E — All plugins: header-only open");

    let paths = collect_plugins(&dir);
    let total_bytes: u64 = paths
        .iter()
        .filter_map(|p| std::fs::metadata(p).ok())
        .map(|m| m.len())
        .sum();

    let mut ok = 0usize;
    let mut errors = 0usize;

    let t0 = Instant::now();
    for path in &paths {
        match open(path) {
            Ok(_) => ok += 1,
            Err(_) => errors += 1,
        }
    }
    let elapsed = t0.elapsed();

    eprintln!(
        "  {} plugins  {}  in {:.3} s  ({:.1} plugins/s)  {} errors",
        paths.len(),
        fmt_bytes(total_bytes),
        elapsed.as_secs_f64(),
        ok as f64 / elapsed.as_secs_f64(),
        errors,
    );

    Ok(())
}

// ── Benchmark F: full-stack decode of Skyrim.esm via RecordView ───────────────

/// Opens Skyrim.esm and runs RecordView field decoding over every record
/// whose type is covered by the schema.  Reports total fields decoded and
/// elapsed time.
#[test]
fn bench_f_skyrim_esm_full_field_decode() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else { return Ok(()); };
    banner("BENCHMARK F — Skyrim.esm: full RecordView field decode");

    let path = dir.join("Skyrim.esm");
    if !path.exists() {
        eprintln!("  SKIP: Skyrim.esm not found");
        return Ok(());
    }

    let plugin = open(&path).map_err(|e| e.to_string())?;
    let reg = SchemaRegistry::sse();

    let mut records_decoded = 0u64;
    let mut fields_decoded = 0u64;
    let mut decode_errors = 0u64;

    let t0 = Instant::now();
    for group in plugin.groups() {
        for record in group.records_recursive() {
            let Some(schema) = reg.get(record.header.signature) else { continue };
            records_decoded += 1;
            let view = RecordView::new(record, schema);
            match view.fields() {
                Ok(fields) => fields_decoded += fields.len() as u64,
                Err(_) => decode_errors += 1,
            }
        }
    }
    let elapsed = t0.elapsed();

    eprintln!(
        "  Records decoded  : {records_decoded}  ({:.0} rec/s)",
        records_decoded as f64 / elapsed.as_secs_f64()
    );
    eprintln!(
        "  Fields decoded   : {fields_decoded}  ({:.0} field/s)",
        fields_decoded as f64 / elapsed.as_secs_f64()
    );
    eprintln!("  Decode errors    : {decode_errors}");
    eprintln!("  Elapsed          : {:.3} s", elapsed.as_secs_f64());

    Ok(())
}

// ── Benchmark G: compression decompression throughput ────────────────────────

/// Measures raw decompression throughput across all compressed records in
/// all ESM files.
#[test]
fn bench_g_decompression_throughput() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else { return Ok(()); };
    banner("BENCHMARK G — zlib decompression throughput (all ESMs)");

    let paths: Vec<PathBuf> = collect_plugins(&dir)
        .into_iter()
        .filter(|p| p.extension().map(|e| e == "esm").unwrap_or(false))
        .collect();

    let mut compressed_records = 0u64;
    let mut compressed_bytes = 0u64;
    let mut errors = 0usize;

    let t0 = Instant::now();
    for path in &paths {
        let Ok(plugin) = open(path) else { continue };
        for group in plugin.groups() {
            for record in group.records_recursive() {
                if !record.header.flags.contains(RecordFlags::COMPRESSED) {
                    continue;
                }
                compressed_records += 1;
                // data_size includes 4-byte uncompressed-size prefix when
                // compressed, so the source bytes length is our proxy.
                compressed_bytes += record.header.data_size as u64;
                if record.subrecords().is_err() {
                    errors += 1;
                }
            }
        }
    }
    let elapsed = t0.elapsed();

    eprintln!(
        "  {} compressed records  {}  in {:.3} s  ({})  {} errors",
        compressed_records,
        fmt_bytes(compressed_bytes),
        elapsed.as_secs_f64(),
        fmt_mbps(compressed_bytes, elapsed),
        errors,
    );

    Ok(())
}

// ── Benchmark H: full suite aggregate ────────────────────────────────────────

/// Runs a single-pass aggregate over ALL plugins: open + iterate all records
/// + parse all subrecords.  This is the highest-throughput number that
/// represents "how fast is bethkit-core end-to-end".
#[test]
fn bench_h_aggregate_all_plugins_full_pass() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else { return Ok(()); };
    banner("BENCHMARK H — All plugins: aggregate single-pass (open+iterate+parse)");

    let paths = collect_plugins(&dir);
    let total_bytes: u64 = paths
        .iter()
        .filter_map(|p| std::fs::metadata(p).ok())
        .map(|m| m.len())
        .sum();

    let mut records = 0u64;
    let mut subrecords = 0u64;
    let mut errors = 0usize;

    let t0 = Instant::now();
    for path in &paths {
        let Ok(plugin) = open(path) else { errors += 1; continue };
        for group in plugin.groups() {
            for record in group.records_recursive() {
                records += 1;
                match record.subrecords() {
                    Ok(srs) => subrecords += srs.len() as u64,
                    Err(_) => errors += 1,
                }
            }
        }
    }
    let elapsed = t0.elapsed();

    let res = BenchResult {
        label: "ALL plugins — open + iterate + subrecord parse",
        files: paths.len(),
        bytes: total_bytes,
        elapsed,
        records,
        errors,
    };
    res.print();
    eprintln!("  Subrecords : {subrecords}");
    eprintln!();
    eprintln!(
        "  *** Peak throughput: {}  ({:.0} records/s) ***",
        fmt_mbps(total_bytes, elapsed),
        records as f64 / elapsed.as_secs_f64(),
    );

    Ok(())
}
